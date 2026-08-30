#![doc = "Import-free bounded producer for raw WebAssembly modules."]
#![forbid(unsafe_code)]

use perfetto_everywhere_core::{
    Category, EmitStatus, FLAG_FLOW_STEP, FLAG_FLOW_TERMINATE, FLAG_GROUP_END, FLAG_GROUP_START,
    Field, FieldValue, FlowAttachment, RECORD_SIZE, Record, RecordKind, Severity, StaticName,
    TraceBackend, TrackId,
};
use std::cell::{Cell, RefCell};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct RawProducerHealth {
    pub emitted_records: u64,
    pub dropped_records: u64,
    pub completed_drains: u64,
    pub high_water_records: usize,
}

struct RawRing {
    records: Vec<[u8; RECORD_SIZE]>,
    read: u64,
    write: u64,
    health: RawProducerHealth,
}

impl RawRing {
    fn occupancy(&self) -> usize {
        self.write.wrapping_sub(self.read) as usize
    }

    fn reserve(&mut self, count: usize) -> Option<u64> {
        if count > self.records.len() || self.occupancy() + count > self.records.len() {
            self.health.dropped_records = self.health.dropped_records.saturating_add(count as u64);
            return None;
        }
        Some(self.write)
    }

    fn push(&mut self, sequence: u64, record: Record) {
        let slot = sequence as usize % self.records.len();
        self.records[slot] = record.encode();
    }

    fn publish(&mut self, count: usize) {
        self.write = self.write.wrapping_add(count as u64);
        self.health.emitted_records = self.health.emitted_records.saturating_add(count as u64);
        self.health.high_water_records = self.health.high_water_records.max(self.occupancy());
    }

    fn group_records(&self) -> Option<usize> {
        let available = self.occupancy();
        if available == 0 {
            return None;
        }
        for offset in 0..available {
            let slot = self.read.wrapping_add(offset as u64) as usize % self.records.len();
            let flags = u16::from_le_bytes([self.records[slot][2], self.records[slot][3]]);
            if flags & FLAG_GROUP_END != 0 {
                return Some(offset + 1);
            }
        }
        None
    }

    fn drain_into(&mut self, destination: &mut [u8]) -> usize {
        let mut records_written = 0;
        while let Some(group_records) = self.group_records() {
            if (records_written + group_records) * RECORD_SIZE > destination.len() {
                break;
            }
            for _ in 0..group_records {
                let slot = self.read as usize % self.records.len();
                let offset = records_written * RECORD_SIZE;
                destination[offset..offset + RECORD_SIZE].copy_from_slice(&self.records[slot]);
                self.read = self.read.wrapping_add(1);
                records_written += 1;
            }
        }
        if records_written > 0 {
            self.health.completed_drains = self.health.completed_drains.saturating_add(1);
        }
        records_written * RECORD_SIZE
    }
}

/// Import-free bounded producer for raw Wasm modules.
///
/// The owner supplies timestamps before instrumented work and drains encoded complete
/// record groups into preallocated storage afterwards. Construction may allocate; event
/// emission, timestamp updates, health reads, and draining do not grow storage.
pub struct RawRingBackend {
    realm_id: u32,
    clock_id: u32,
    enabled: Cell<bool>,
    timestamp: Cell<u64>,
    categories: Vec<u32>,
    ring: RefCell<RawRing>,
}

impl RawRingBackend {
    pub fn new(
        realm_id: u32,
        clock_id: u32,
        capacity_records: usize,
        categories: &[Category],
    ) -> Result<Self, &'static str> {
        if realm_id == 0 {
            return Err("realm ID zero is reserved");
        }
        if clock_id == 0 {
            return Err("clock ID zero is reserved");
        }
        if capacity_records == 0 {
            return Err("raw ring capacity must be positive");
        }
        Ok(Self {
            realm_id,
            clock_id,
            enabled: Cell::new(true),
            timestamp: Cell::new(0),
            categories: categories.iter().map(|category| category.id.0).collect(),
            ring: RefCell::new(RawRing {
                records: vec![[0; RECORD_SIZE]; capacity_records],
                read: 0,
                write: 0,
                health: RawProducerHealth::default(),
            }),
        })
    }

    pub fn set_enabled(&self, enabled: bool) {
        self.enabled.set(enabled);
    }

    pub fn set_timestamp(&self, timestamp: u64) {
        self.timestamp.set(timestamp);
    }

    pub fn available_records(&self) -> usize {
        self.ring.borrow().occupancy()
    }

    /// Drain as many complete groups as fit. Returns the initialized byte count.
    pub fn drain_into(&self, destination: &mut [u8]) -> usize {
        self.ring.borrow_mut().drain_into(destination)
    }

    pub fn health(&self) -> RawProducerHealth {
        self.ring.borrow().health
    }

    fn category_enabled(&self, category: Category) -> bool {
        self.enabled.get()
            && (self.categories.is_empty() || self.categories.contains(&category.id.0))
    }

    #[allow(clippy::too_many_arguments)]
    fn emit(
        &self,
        kind: RecordKind,
        category: Option<Category>,
        name: StaticName,
        track: TrackId,
        value: u64,
        fields: &[Field<'_>],
        flow: FlowAttachment,
        arg_override: Option<u64>,
    ) -> EmitStatus {
        if !self.enabled.get() {
            return EmitStatus::Disabled;
        }
        if fields
            .iter()
            .any(|field| matches!(field.value, FieldValue::Str(_)))
        {
            return EmitStatus::Unsupported;
        }
        let count = 1 + fields.len();
        let mut ring = self.ring.borrow_mut();
        let Some(write) = ring.reserve(count) else {
            return EmitStatus::Dropped;
        };
        let (flow_flag, flow_id) = match flow {
            FlowAttachment::None => (0, 0),
            FlowAttachment::Step(id) => (FLAG_FLOW_STEP, id.get()),
            FlowAttachment::Terminate(id) => (FLAG_FLOW_TERMINATE, id.get()),
        };
        let timestamp = self.timestamp.get();
        ring.push(
            write,
            Record::new(
                kind,
                FLAG_GROUP_START | if fields.is_empty() { FLAG_GROUP_END } else { 0 } | flow_flag,
                self.realm_id,
                name.id.0,
                self.clock_id,
                timestamp,
                category.map_or(value, |category| u64::from(category.id.0)),
                flow_id,
                arg_override.unwrap_or(track.0),
            ),
        );
        for (index, field) in fields.iter().enumerate() {
            let (kind, value) = match field.value {
                FieldValue::Bool(value) => (RecordKind::FieldBool, u64::from(value)),
                FieldValue::I64(value) => (RecordKind::FieldI64, value as u64),
                FieldValue::U64(value) => (RecordKind::FieldU64, value),
                FieldValue::F64(value) => (RecordKind::FieldF64, value.to_bits()),
                FieldValue::StaticStr(value) => (RecordKind::FieldStaticStr, u64::from(value.id.0)),
                FieldValue::Str(_) => return EmitStatus::Unsupported,
            };
            ring.push(
                write.wrapping_add(index as u64 + 1),
                Record::new(
                    kind,
                    if index + 1 == fields.len() {
                        FLAG_GROUP_END
                    } else {
                        0
                    },
                    self.realm_id,
                    field.name.id.0,
                    self.clock_id,
                    timestamp,
                    value,
                    0,
                    0,
                ),
            );
        }
        ring.publish(count);
        EmitStatus::Recorded
    }
}

impl TraceBackend for RawRingBackend {
    fn is_enabled(&self, category: Category) -> bool {
        self.category_enabled(category)
    }

    fn span_begin(
        &self,
        category: Category,
        name: StaticName,
        track: TrackId,
        fields: &[Field<'_>],
        flow: FlowAttachment,
    ) -> EmitStatus {
        if !self.category_enabled(category) {
            return EmitStatus::Disabled;
        }
        self.emit(
            RecordKind::SpanBegin,
            Some(category),
            name,
            track,
            0,
            fields,
            flow,
            None,
        )
    }

    fn span_end(&self, track: TrackId) -> EmitStatus {
        self.emit(
            RecordKind::SpanEnd,
            None,
            StaticName::new("span end"),
            track,
            0,
            &[],
            FlowAttachment::None,
            None,
        )
    }

    fn event(
        &self,
        category: Category,
        name: StaticName,
        track: TrackId,
        fields: &[Field<'_>],
        flow: FlowAttachment,
    ) -> EmitStatus {
        if !self.category_enabled(category) {
            return EmitStatus::Disabled;
        }
        self.emit(
            RecordKind::Instant,
            Some(category),
            name,
            track,
            0,
            fields,
            flow,
            None,
        )
    }

    fn log(
        &self,
        severity: Severity,
        target: StaticName,
        message: StaticName,
        fields: &[Field<'_>],
    ) -> EmitStatus {
        self.emit(
            RecordKind::Log,
            None,
            message,
            TrackId::CURRENT,
            severity as u64,
            fields,
            FlowAttachment::None,
            Some(u64::from(target.id.0)),
        )
    }

    fn counter_i64(&self, name: StaticName, track: TrackId, value: i64) -> EmitStatus {
        self.emit(
            RecordKind::CounterI64,
            None,
            name,
            track,
            value as u64,
            &[],
            FlowAttachment::None,
            None,
        )
    }

    fn counter_f64(&self, name: StaticName, track: TrackId, value: f64) -> EmitStatus {
        self.emit(
            RecordKind::CounterF64,
            None,
            name,
            track,
            value.to_bits(),
            &[],
            FlowAttachment::None,
            None,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use perfetto_everywhere_core::{
        FieldName, FieldValue, FlowAttachment, Record, Tracer, validate_record_groups,
    };

    const AUDIO: Category = Category::new("audio");
    const CALLBACK: StaticName = StaticName::new("callback");

    fn decode(bytes: &[u8]) -> Vec<Record> {
        bytes
            .chunks_exact(RECORD_SIZE)
            .map(|chunk| Record::decode(chunk).unwrap())
            .collect()
    }

    #[test]
    fn drains_only_complete_groups_without_growing() {
        let backend = RawRingBackend::new(4, 104, 4, &[AUDIO]).unwrap();
        backend.set_timestamp(128);
        let fields = [Field::new(FieldName::new("frames"), FieldValue::U64(128))];
        assert_eq!(
            backend.event(
                AUDIO,
                CALLBACK,
                TrackId::CURRENT,
                &fields,
                FlowAttachment::None
            ),
            EmitStatus::Recorded
        );
        assert_eq!(backend.available_records(), 2);

        let mut too_small = [0_u8; RECORD_SIZE];
        assert_eq!(backend.drain_into(&mut too_small), 0);
        assert_eq!(backend.available_records(), 2);

        let mut output = [0_u8; RECORD_SIZE * 2];
        assert_eq!(backend.drain_into(&mut output), output.len());
        let records = decode(&output);
        assert!(validate_record_groups(&records).is_ok());
        assert_eq!(records[0].timestamp, 128);
        assert_eq!(backend.available_records(), 0);
    }

    #[test]
    fn overflow_drops_whole_group_and_recovers_after_drain() {
        let backend = RawRingBackend::new(4, 104, 2, &[]).unwrap();
        let tracer = Tracer::new(backend);
        let fields = [Field::new(FieldName::new("frames"), FieldValue::U64(128))];
        assert_eq!(tracer.event(AUDIO, CALLBACK, &fields), EmitStatus::Recorded);
        assert_eq!(tracer.event(AUDIO, CALLBACK, &fields), EmitStatus::Dropped);
        assert_eq!(tracer.backend().health().dropped_records, 2);
        let mut output = [0_u8; RECORD_SIZE * 2];
        assert_eq!(tracer.backend().drain_into(&mut output), output.len());
        assert_eq!(tracer.event(AUDIO, CALLBACK, &fields), EmitStatus::Recorded);
    }

    #[test]
    fn dynamic_strings_are_rejected_and_disabled_is_inert() {
        let backend = RawRingBackend::new(4, 104, 2, &[]).unwrap();
        let fields = [Field::new(
            FieldName::new("message"),
            FieldValue::Str("dynamic"),
        )];
        assert_eq!(
            backend.event(
                AUDIO,
                CALLBACK,
                TrackId::CURRENT,
                &fields,
                FlowAttachment::None
            ),
            EmitStatus::Unsupported
        );
        backend.set_enabled(false);
        assert_eq!(
            backend.event(AUDIO, CALLBACK, TrackId::CURRENT, &[], FlowAttachment::None),
            EmitStatus::Disabled
        );
        assert_eq!(backend.available_records(), 0);
    }
}
