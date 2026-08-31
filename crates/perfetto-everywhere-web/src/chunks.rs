use perfetto_everywhere_core::{RECORD_SIZE, Record, validate_record_groups};
use std::collections::BTreeMap;

pub const CHUNK_PROTOCOL_VERSION: u32 = 2;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ChunkPoolConfig {
    pub capture_id: u64,
    pub chunk_bytes: usize,
    pub pool_size: usize,
    pub max_group_records: usize,
}

impl ChunkPoolConfig {
    pub fn validate(self) -> Result<Self, ChunkProtocolError> {
        if self.capture_id == 0 {
            return Err(ChunkProtocolError::InvalidCaptureId);
        }
        if self.pool_size < 2 {
            return Err(ChunkProtocolError::PoolTooSmall);
        }
        let minimum = self
            .max_group_records
            .checked_mul(RECORD_SIZE)
            .ok_or(ChunkProtocolError::InvalidCapacity)?;
        if self.max_group_records == 0
            || self.chunk_bytes < minimum
            || self.chunk_bytes % RECORD_SIZE != 0
        {
            return Err(ChunkProtocolError::InvalidCapacity);
        }
        Ok(self)
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ChunkTransportHealth {
    pub completed_chunks: u64,
    pub returned_buffers: u64,
    pub pool_starvation_records: u64,
    pub rejected_chunks: u64,
    pub max_in_flight: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ChunkDescriptor {
    pub capture_id: u64,
    pub sequence: u64,
    pub pool_token: u32,
    pub used_bytes: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StoppedDescriptor {
    pub capture_id: u64,
    pub chunk_count: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ChunkProtocolError {
    InvalidCaptureId,
    PoolTooSmall,
    InvalidCapacity,
    StaleCapture,
    UnknownPoolToken(u32),
    TokenAlreadyOwned(u32),
    InvalidSequence { expected: u64, actual: u64 },
    InvalidUsedBytes(usize),
    InvalidRecordGroup(String),
    AlreadyStopped,
    IncompleteCapture { expected: u64, received: u64 },
}

impl core::fmt::Display for ChunkProtocolError {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::InvalidCaptureId => formatter.write_str("capture ID zero is reserved"),
            Self::PoolTooSmall => formatter.write_str("chunk pool needs at least two buffers"),
            Self::InvalidCapacity => formatter.write_str("invalid chunk capacity"),
            Self::StaleCapture => formatter.write_str("chunk belongs to a stale capture"),
            Self::UnknownPoolToken(token) => write!(formatter, "unknown pool token {token}"),
            Self::TokenAlreadyOwned(token) => {
                write!(formatter, "pool token {token} is already collector-owned")
            }
            Self::InvalidSequence { expected, actual } => {
                write!(
                    formatter,
                    "expected chunk sequence {expected}, got {actual}"
                )
            }
            Self::InvalidUsedBytes(bytes) => write!(formatter, "invalid used byte count {bytes}"),
            Self::InvalidRecordGroup(error) => write!(formatter, "invalid record group: {error}"),
            Self::AlreadyStopped => formatter.write_str("capture is already stopped"),
            Self::IncompleteCapture { expected, received } => write!(
                formatter,
                "capture declared {expected} chunks but collector received {received}",
            ),
        }
    }
}

impl std::error::Error for ChunkProtocolError {}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum TokenOwner {
    Producer,
    Collector,
}

/// Validates chunk ordering and exclusive pool-token ownership independently
/// from the browser message API.
pub struct ChunkCollectorState {
    config: ChunkPoolConfig,
    owners: Vec<TokenOwner>,
    next_sequence: u64,
    stopped: Option<u64>,
    health: ChunkTransportHealth,
}

impl ChunkCollectorState {
    pub fn new(config: ChunkPoolConfig) -> Result<Self, ChunkProtocolError> {
        let config = config.validate()?;
        Ok(Self {
            config,
            owners: vec![TokenOwner::Producer; config.pool_size],
            next_sequence: 0,
            stopped: None,
            health: ChunkTransportHealth::default(),
        })
    }

    pub fn ingest(
        &mut self,
        descriptor: ChunkDescriptor,
        bytes: &[u8],
    ) -> Result<(), ChunkProtocolError> {
        if self.stopped.is_some() {
            return Err(ChunkProtocolError::AlreadyStopped);
        }
        if descriptor.capture_id != self.config.capture_id {
            return Err(ChunkProtocolError::StaleCapture);
        }
        if descriptor.sequence != self.next_sequence {
            return Err(ChunkProtocolError::InvalidSequence {
                expected: self.next_sequence,
                actual: descriptor.sequence,
            });
        }
        let owner = self
            .owners
            .get_mut(descriptor.pool_token as usize)
            .ok_or(ChunkProtocolError::UnknownPoolToken(descriptor.pool_token))?;
        if *owner == TokenOwner::Collector {
            return Err(ChunkProtocolError::TokenAlreadyOwned(descriptor.pool_token));
        }
        if descriptor.used_bytes == 0
            || descriptor.used_bytes > self.config.chunk_bytes
            || descriptor.used_bytes != bytes.len()
            || descriptor.used_bytes % RECORD_SIZE != 0
        {
            return Err(ChunkProtocolError::InvalidUsedBytes(descriptor.used_bytes));
        }
        let records = bytes
            .chunks_exact(RECORD_SIZE)
            .map(Record::decode)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| ChunkProtocolError::InvalidRecordGroup(error.to_string()))?;
        validate_record_groups(&records)
            .map_err(|error| ChunkProtocolError::InvalidRecordGroup(error.to_string()))?;
        *owner = TokenOwner::Collector;
        self.next_sequence = self.next_sequence.saturating_add(1);
        self.health.completed_chunks = self.health.completed_chunks.saturating_add(1);
        self.health.max_in_flight = self.health.max_in_flight.max(
            self.owners
                .iter()
                .filter(|owner| **owner == TokenOwner::Collector)
                .count(),
        );
        Ok(())
    }

    pub fn recycle(&mut self, capture_id: u64, token: u32) -> Result<(), ChunkProtocolError> {
        if capture_id != self.config.capture_id {
            return Err(ChunkProtocolError::StaleCapture);
        }
        let owner = self
            .owners
            .get_mut(token as usize)
            .ok_or(ChunkProtocolError::UnknownPoolToken(token))?;
        if *owner != TokenOwner::Collector {
            return Err(ChunkProtocolError::UnknownPoolToken(token));
        }
        *owner = TokenOwner::Producer;
        self.health.returned_buffers = self.health.returned_buffers.saturating_add(1);
        Ok(())
    }

    pub fn stop(&mut self, stopped: StoppedDescriptor) -> Result<(), ChunkProtocolError> {
        if stopped.capture_id != self.config.capture_id {
            return Err(ChunkProtocolError::StaleCapture);
        }
        if self.stopped.is_some() {
            return Err(ChunkProtocolError::AlreadyStopped);
        }
        if stopped.chunk_count != self.next_sequence {
            return Err(ChunkProtocolError::IncompleteCapture {
                expected: stopped.chunk_count,
                received: self.next_sequence,
            });
        }
        self.stopped = Some(stopped.chunk_count);
        Ok(())
    }

    pub fn is_complete(&self) -> bool {
        self.stopped == Some(self.next_sequence)
    }

    pub fn health(&self) -> ChunkTransportHealth {
        self.health
    }
}

/// Collects validated chunks by sequence for hosts that need to defer capture
/// assembly. Browser integrations can replace this with an incremental sink.
#[derive(Default)]
pub struct MemoryChunkSink {
    chunks: BTreeMap<u64, Vec<u8>>,
}

impl MemoryChunkSink {
    pub fn push(&mut self, sequence: u64, bytes: &[u8]) {
        self.chunks.insert(sequence, bytes.to_vec());
    }

    pub fn into_bytes(self) -> Vec<u8> {
        let size = self.chunks.values().map(Vec::len).sum();
        let mut bytes = Vec::with_capacity(size);
        for chunk in self.chunks.into_values() {
            bytes.extend_from_slice(&chunk);
        }
        bytes
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use perfetto_everywhere_core::{FLAG_GROUP_END, FLAG_GROUP_START, RecordKind};

    fn config() -> ChunkPoolConfig {
        ChunkPoolConfig {
            capture_id: 7,
            chunk_bytes: RECORD_SIZE * 4,
            pool_size: 3,
            max_group_records: 4,
        }
    }

    fn group() -> Vec<u8> {
        Record::new(
            RecordKind::Instant,
            FLAG_GROUP_START | FLAG_GROUP_END,
            1,
            2,
            3,
            4,
            5,
            0,
            0,
        )
        .encode()
        .to_vec()
    }

    #[test]
    fn validates_configuration_before_capture() {
        assert_eq!(
            ChunkPoolConfig {
                chunk_bytes: RECORD_SIZE * 3,
                ..config()
            }
            .validate(),
            Err(ChunkProtocolError::InvalidCapacity)
        );
        assert_eq!(
            ChunkPoolConfig {
                capture_id: 0,
                ..config()
            }
            .validate(),
            Err(ChunkProtocolError::InvalidCaptureId)
        );
    }

    #[test]
    fn accepts_empty_capture_without_a_sentinel_chunk() {
        let mut state = ChunkCollectorState::new(config()).unwrap();
        state
            .stop(StoppedDescriptor {
                capture_id: 7,
                chunk_count: 0,
            })
            .unwrap();
        assert!(state.is_complete());
    }

    #[test]
    fn validates_sequences_and_token_recycling() {
        let mut state = ChunkCollectorState::new(config()).unwrap();
        let bytes = group();
        state
            .ingest(
                ChunkDescriptor {
                    capture_id: 7,
                    sequence: 0,
                    pool_token: 0,
                    used_bytes: bytes.len(),
                },
                &bytes,
            )
            .unwrap();
        assert!(matches!(
            state.ingest(
                ChunkDescriptor {
                    capture_id: 7,
                    sequence: 1,
                    pool_token: 0,
                    used_bytes: bytes.len(),
                },
                &bytes,
            ),
            Err(ChunkProtocolError::TokenAlreadyOwned(0))
        ));
        state.recycle(7, 0).unwrap();
        state
            .ingest(
                ChunkDescriptor {
                    capture_id: 7,
                    sequence: 1,
                    pool_token: 0,
                    used_bytes: bytes.len(),
                },
                &bytes,
            )
            .unwrap();
        state
            .stop(StoppedDescriptor {
                capture_id: 7,
                chunk_count: 2,
            })
            .unwrap();
        assert!(state.is_complete());
        assert_eq!(state.health().completed_chunks, 2);
    }

    #[test]
    fn rejects_partial_and_incomplete_capture() {
        let mut state = ChunkCollectorState::new(config()).unwrap();
        assert!(matches!(
            state.ingest(
                ChunkDescriptor {
                    capture_id: 7,
                    sequence: 0,
                    pool_token: 0,
                    used_bytes: 1,
                },
                &[0],
            ),
            Err(ChunkProtocolError::InvalidUsedBytes(1))
        ));
        assert!(matches!(
            state.stop(StoppedDescriptor {
                capture_id: 7,
                chunk_count: 1,
            }),
            Err(ChunkProtocolError::IncompleteCapture { .. })
        ));
    }
}
