use core::fmt;

pub const RECORD_VERSION: u8 = 1;
pub const RECORD_SIZE: usize = 48;

pub const FLAG_GROUP_START: u16 = 1 << 0;
pub const FLAG_GROUP_END: u16 = 1 << 1;
pub const FLAG_FLOW_STEP: u16 = 1 << 2;
pub const FLAG_FLOW_TERMINATE: u16 = 1 << 3;
const KNOWN_FLAGS: u16 = FLAG_GROUP_START | FLAG_GROUP_END | FLAG_FLOW_STEP | FLAG_FLOW_TERMINATE;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum RecordKind {
    SpanBegin = 1,
    SpanEnd = 2,
    Instant = 3,
    CounterI64 = 4,
    CounterF64 = 5,
    Log = 6,
    FieldBool = 7,
    FieldI64 = 8,
    FieldU64 = 9,
    FieldF64 = 10,
    FieldStaticStr = 11,
    Health = 12,
}

impl RecordKind {
    pub const fn is_field(self) -> bool {
        matches!(
            self,
            Self::FieldBool
                | Self::FieldI64
                | Self::FieldU64
                | Self::FieldF64
                | Self::FieldStaticStr
        )
    }
}

impl TryFrom<u8> for RecordKind {
    type Error = ProtocolError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(Self::SpanBegin),
            2 => Ok(Self::SpanEnd),
            3 => Ok(Self::Instant),
            4 => Ok(Self::CounterI64),
            5 => Ok(Self::CounterF64),
            6 => Ok(Self::Log),
            7 => Ok(Self::FieldBool),
            8 => Ok(Self::FieldI64),
            9 => Ok(Self::FieldU64),
            10 => Ok(Self::FieldF64),
            11 => Ok(Self::FieldStaticStr),
            12 => Ok(Self::Health),
            other => Err(ProtocolError::UnknownKind(other)),
        }
    }
}

/// Fixed-layout browser producer record.
///
/// Multi-record events are contiguous groups. The event header has
/// `FLAG_GROUP_START`; the final header/field has `FLAG_GROUP_END`. Producers
/// reserve the entire group before writing so collectors never observe partial
/// fields for a published event.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Record {
    pub version: u8,
    pub kind: RecordKind,
    pub flags: u16,
    pub realm_id: u32,
    pub name_id: u32,
    pub clock_id: u32,
    pub timestamp: u64,
    pub value: u64,
    pub flow_id: u64,
    pub arg: u64,
}

impl Record {
    #[allow(clippy::too_many_arguments)]
    pub const fn new(
        kind: RecordKind,
        flags: u16,
        realm_id: u32,
        name_id: u32,
        clock_id: u32,
        timestamp: u64,
        value: u64,
        flow_id: u64,
        arg: u64,
    ) -> Self {
        Self {
            version: RECORD_VERSION,
            kind,
            flags,
            realm_id,
            name_id,
            clock_id,
            timestamp,
            value,
            flow_id,
            arg,
        }
    }

    pub fn encode(self) -> [u8; RECORD_SIZE] {
        let mut output = [0_u8; RECORD_SIZE];
        output[0] = self.version;
        output[1] = self.kind as u8;
        output[2..4].copy_from_slice(&self.flags.to_le_bytes());
        output[4..8].copy_from_slice(&self.realm_id.to_le_bytes());
        output[8..12].copy_from_slice(&self.name_id.to_le_bytes());
        output[12..16].copy_from_slice(&self.clock_id.to_le_bytes());
        output[16..24].copy_from_slice(&self.timestamp.to_le_bytes());
        output[24..32].copy_from_slice(&self.value.to_le_bytes());
        output[32..40].copy_from_slice(&self.flow_id.to_le_bytes());
        output[40..48].copy_from_slice(&self.arg.to_le_bytes());
        output
    }

    pub fn decode(input: &[u8]) -> Result<Self, ProtocolError> {
        if input.len() != RECORD_SIZE {
            return Err(ProtocolError::WrongSize(input.len()));
        }
        if input[0] != RECORD_VERSION {
            return Err(ProtocolError::UnsupportedVersion(input[0]));
        }
        let flags = u16::from_le_bytes([input[2], input[3]]);
        if flags & !KNOWN_FLAGS != 0 {
            return Err(ProtocolError::UnknownFlags(flags & !KNOWN_FLAGS));
        }
        if flags & FLAG_FLOW_STEP != 0 && flags & FLAG_FLOW_TERMINATE != 0 {
            return Err(ProtocolError::ConflictingFlowFlags);
        }
        let record = Self {
            version: input[0],
            kind: RecordKind::try_from(input[1])?,
            flags,
            realm_id: u32::from_le_bytes(input[4..8].try_into().unwrap()),
            name_id: u32::from_le_bytes(input[8..12].try_into().unwrap()),
            clock_id: u32::from_le_bytes(input[12..16].try_into().unwrap()),
            timestamp: u64::from_le_bytes(input[16..24].try_into().unwrap()),
            value: u64::from_le_bytes(input[24..32].try_into().unwrap()),
            flow_id: u64::from_le_bytes(input[32..40].try_into().unwrap()),
            arg: u64::from_le_bytes(input[40..48].try_into().unwrap()),
        };
        if record.realm_id == 0 {
            return Err(ProtocolError::ReservedRealm);
        }
        if record.clock_id == 0 {
            return Err(ProtocolError::ReservedClock);
        }
        Ok(record)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProtocolError {
    WrongSize(usize),
    UnsupportedVersion(u8),
    UnknownKind(u8),
    UnknownFlags(u16),
    ConflictingFlowFlags,
    ReservedRealm,
    ReservedClock,
    FieldOutsideGroup(usize),
    HeaderInsideGroup(usize),
    GroupNotStarted(usize),
    UnterminatedGroup,
    MismatchedGroupContext(usize),
}

impl fmt::Display for ProtocolError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::WrongSize(size) => write!(formatter, "record has {size} bytes, expected 48"),
            Self::UnsupportedVersion(version) => {
                write!(formatter, "unsupported record version {version}")
            }
            Self::UnknownKind(kind) => write!(formatter, "unknown record kind {kind}"),
            Self::UnknownFlags(flags) => write!(formatter, "unknown record flags {flags:#x}"),
            Self::ConflictingFlowFlags => write!(formatter, "flow step and terminate are both set"),
            Self::ReservedRealm => write!(formatter, "realm ID zero is reserved"),
            Self::ReservedClock => write!(formatter, "clock ID zero is reserved"),
            Self::FieldOutsideGroup(index) => write!(formatter, "field {index} is outside a group"),
            Self::HeaderInsideGroup(index) => {
                write!(formatter, "header {index} interrupts a group")
            }
            Self::GroupNotStarted(index) => {
                write!(formatter, "record {index} has no group-start flag")
            }
            Self::UnterminatedGroup => write!(formatter, "record group is unterminated"),
            Self::MismatchedGroupContext(index) => {
                write!(formatter, "field {index} does not match its group context")
            }
        }
    }
}

/// Validates complete groups and prevents fields from being attached across realm,
/// clock, or timestamp boundaries.
pub fn validate_record_groups(records: &[Record]) -> Result<(), ProtocolError> {
    let mut context: Option<(u32, u32, u64)> = None;
    for (index, record) in records.iter().enumerate() {
        if record.kind.is_field() {
            let Some(expected) = context else {
                return Err(ProtocolError::FieldOutsideGroup(index));
            };
            if expected != (record.realm_id, record.clock_id, record.timestamp) {
                return Err(ProtocolError::MismatchedGroupContext(index));
            }
            if record.flags & FLAG_GROUP_START != 0 {
                return Err(ProtocolError::HeaderInsideGroup(index));
            }
            if record.flags & FLAG_GROUP_END != 0 {
                context = None;
            }
            continue;
        }

        if context.is_some() {
            return Err(ProtocolError::HeaderInsideGroup(index));
        }
        if record.flags & FLAG_GROUP_START == 0 {
            return Err(ProtocolError::GroupNotStarted(index));
        }
        if record.flags & FLAG_GROUP_END == 0 {
            context = Some((record.realm_id, record.clock_id, record.timestamp));
        }
    }
    if context.is_some() {
        Err(ProtocolError::UnterminatedGroup)
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn header(flags: u16) -> Record {
        Record::new(RecordKind::Instant, flags, 2, 9, 3, 123, 7, 42, 5)
    }

    #[test]
    fn golden_layout_is_48_little_endian_bytes() {
        let bytes = header(FLAG_GROUP_START | FLAG_GROUP_END).encode();
        assert_eq!(bytes.len(), 48);
        assert_eq!(&bytes[0..4], &[1, 3, 3, 0]);
        assert_eq!(&bytes[4..8], &2_u32.to_le_bytes());
        assert_eq!(&bytes[16..24], &123_u64.to_le_bytes());
        assert_eq!(Record::decode(&bytes), Ok(header(3)));
    }

    #[test]
    fn rejects_unknown_versions_kinds_flags_and_partial_bytes() {
        let mut bytes = header(3).encode();
        bytes[0] = 2;
        assert_eq!(
            Record::decode(&bytes),
            Err(ProtocolError::UnsupportedVersion(2))
        );
        bytes[0] = RECORD_VERSION;
        bytes[1] = 99;
        assert_eq!(Record::decode(&bytes), Err(ProtocolError::UnknownKind(99)));
        bytes[1] = RecordKind::Instant as u8;
        bytes[2..4].copy_from_slice(&0x8000_u16.to_le_bytes());
        assert_eq!(
            Record::decode(&bytes),
            Err(ProtocolError::UnknownFlags(0x8000))
        );
        assert!(matches!(
            Record::decode(&bytes[..47]),
            Err(ProtocolError::WrongSize(47))
        ));
    }

    #[test]
    fn validates_complete_field_groups() {
        let field = Record::new(
            RecordKind::FieldI64,
            FLAG_GROUP_END,
            2,
            10,
            3,
            123,
            44,
            0,
            0,
        );
        assert!(validate_record_groups(&[header(FLAG_GROUP_START), field]).is_ok());
        assert_eq!(
            validate_record_groups(&[header(FLAG_GROUP_START)]),
            Err(ProtocolError::UnterminatedGroup)
        );
        assert_eq!(
            validate_record_groups(&[field]),
            Err(ProtocolError::FieldOutsideGroup(0))
        );
    }
}
