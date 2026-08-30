#![doc = "Validated compact-record to Perfetto protobuf collector."]
#![forbid(unsafe_code)]

use perfetto_everywhere_core::{
    FLAG_FLOW_STEP, FLAG_FLOW_TERMINATE, ProtocolError, RECORD_SIZE, Record, RecordKind,
    validate_record_groups,
};
use perfetto_everywhere_web::{ClockCalibration, MetadataEntry, ProducerHealth};
use prost::Message;
use std::collections::{BTreeMap, BTreeSet};
use tracing_perfetto_sdk_schema::{
    BuiltinClock, ClockSnapshot, CounterDescriptor, DebugAnnotation, Trace, TracePacket,
    TrackDescriptor, TrackEvent, clock_snapshot, debug_annotation, trace_packet, track_descriptor,
    track_event,
};

const CUSTOM_CLOCK_ID: u32 = 64;
const REFERENCE_CLOCK_ID: u32 = BuiltinClock::Boottime as u32;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RealmDescriptor {
    pub id: u32,
    pub label: String,
}

#[derive(Clone, Debug)]
struct EventGroup {
    header: Record,
    fields: Vec<Record>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CollectorError {
    PartialRecordBytes(usize),
    Protocol(String),
    UnknownRealm(u32),
    RealmCollision(u32),
    MetadataCollision(u32),
    UnknownMetadata(u32),
    MissingCalibration(u32),
    NonMonotonicCalibration(u32),
    InvalidClockSample(u32),
    CaptureLimitExceeded,
}

impl core::fmt::Display for CollectorError {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::PartialRecordBytes(size) => {
                write!(formatter, "record input has {size} trailing bytes")
            }
            Self::Protocol(message) => write!(formatter, "record protocol error: {message}"),
            Self::UnknownRealm(id) => write!(formatter, "unknown realm {id}"),
            Self::RealmCollision(id) => write!(formatter, "conflicting realm definition {id}"),
            Self::MetadataCollision(id) => write!(formatter, "conflicting metadata ID {id:#x}"),
            Self::UnknownMetadata(id) => write!(formatter, "unknown metadata ID {id:#x}"),
            Self::MissingCalibration(id) => {
                write!(formatter, "realm {id} has no clock calibration")
            }
            Self::NonMonotonicCalibration(id) => {
                write!(formatter, "realm {id} has non-monotonic clock calibration")
            }
            Self::InvalidClockSample(id) => {
                write!(formatter, "realm {id} has an invalid clock sample")
            }
            Self::CaptureLimitExceeded => write!(formatter, "collector record limit exceeded"),
        }
    }
}

impl std::error::Error for CollectorError {}

impl From<ProtocolError> for CollectorError {
    fn from(value: ProtocolError) -> Self {
        Self::Protocol(value.to_string())
    }
}

#[derive(Clone, Debug)]
pub struct CollectorConfig {
    pub max_records: usize,
    pub max_clock_uncertainty_ns: u64,
}

impl Default for CollectorConfig {
    fn default() -> Self {
        Self {
            max_records: 2_000_000,
            max_clock_uncertainty_ns: 5_000_000,
        }
    }
}

/// Collector state. Producers may arrive in any order; finalization validates
/// metadata and clocks before emitting deterministic protobuf packets.
pub struct Collector {
    config: CollectorConfig,
    realms: BTreeMap<u32, RealmDescriptor>,
    metadata: BTreeMap<u32, MetadataEntry>,
    calibrations: BTreeMap<u32, Vec<ClockCalibration>>,
    groups: Vec<EventGroup>,
    health: BTreeMap<u32, ProducerHealth>,
    ingested_records: usize,
}

impl Collector {
    pub fn new(config: CollectorConfig) -> Self {
        Self {
            config,
            realms: BTreeMap::new(),
            metadata: BTreeMap::new(),
            calibrations: BTreeMap::new(),
            groups: Vec::new(),
            health: BTreeMap::new(),
            ingested_records: 0,
        }
    }

    pub fn register_realm(&mut self, descriptor: RealmDescriptor) -> Result<(), CollectorError> {
        if descriptor.id == 0 {
            return Err(CollectorError::UnknownRealm(0));
        }
        if let Some(existing) = self.realms.get(&descriptor.id) {
            if existing != &descriptor {
                return Err(CollectorError::RealmCollision(descriptor.id));
            }
            return Ok(());
        }
        self.realms.insert(descriptor.id, descriptor);
        Ok(())
    }

    pub fn register_metadata(&mut self, entry: MetadataEntry) -> Result<(), CollectorError> {
        if let Some(existing) = self.metadata.get(&entry.id.0) {
            if existing != &entry {
                return Err(CollectorError::MetadataCollision(entry.id.0));
            }
            return Ok(());
        }
        self.metadata.insert(entry.id.0, entry);
        Ok(())
    }

    pub fn register_metadata_all(
        &mut self,
        entries: impl IntoIterator<Item = MetadataEntry>,
    ) -> Result<(), CollectorError> {
        for entry in entries {
            self.register_metadata(entry)?;
        }
        Ok(())
    }

    pub fn add_calibration(&mut self, sample: ClockCalibration) -> Result<(), CollectorError> {
        if sample.realm_id == 0
            || sample.clock_id == 0
            || sample.source_ticks == 0
            || sample.reference_time_ns == 0
            || sample.uncertainty_ns > self.config.max_clock_uncertainty_ns
        {
            return Err(CollectorError::InvalidClockSample(sample.realm_id));
        }
        let samples = self.calibrations.entry(sample.realm_id).or_default();
        if let Some(previous) = samples.last() {
            if sample.clock_id != previous.clock_id
                || sample.source_ticks <= previous.source_ticks
                || sample.reference_time_ns <= previous.reference_time_ns
            {
                return Err(CollectorError::NonMonotonicCalibration(sample.realm_id));
            }
        }
        samples.push(sample);
        Ok(())
    }

    pub fn set_health(&mut self, realm_id: u32, health: ProducerHealth) {
        self.health.insert(realm_id, health);
    }

    pub fn ingest_batch(&mut self, bytes: &[u8]) -> Result<(), CollectorError> {
        if bytes.len() % RECORD_SIZE != 0 {
            return Err(CollectorError::PartialRecordBytes(
                bytes.len() % RECORD_SIZE,
            ));
        }
        let records: Vec<Record> = bytes
            .chunks_exact(RECORD_SIZE)
            .map(Record::decode)
            .collect::<Result<_, _>>()?;
        if self.ingested_records + records.len() > self.config.max_records {
            return Err(CollectorError::CaptureLimitExceeded);
        }
        validate_record_groups(&records)?;
        self.ingested_records += records.len();

        let mut index = 0;
        while index < records.len() {
            let header = records[index];
            index += 1;
            let mut fields = Vec::new();
            if header.flags & perfetto_everywhere_core::FLAG_GROUP_END == 0 {
                loop {
                    let field = records[index];
                    index += 1;
                    fields.push(field);
                    if field.flags & perfetto_everywhere_core::FLAG_GROUP_END != 0 {
                        break;
                    }
                }
            }
            self.groups.push(EventGroup { header, fields });
        }
        Ok(())
    }

    pub fn finish(mut self) -> Result<Vec<u8>, CollectorError> {
        self.validate_references()?;
        self.groups.sort_by_key(|group| {
            (
                group.header.realm_id,
                group.header.timestamp,
                group.header.kind as u8,
            )
        });
        self.repair_span_boundaries();
        self.groups.sort_by_key(|group| {
            (
                group.header.realm_id,
                group.header.timestamp,
                group.header.kind as u8,
            )
        });

        let mut packets = Vec::new();
        self.emit_clock_snapshots(&mut packets)?;
        self.emit_descriptors(&mut packets)?;
        self.emit_events(&mut packets)?;
        self.emit_health(&mut packets)?;
        Ok(Trace { packet: packets }.encode_to_vec())
    }

    fn repair_span_boundaries(&mut self) {
        let mut depths: BTreeMap<(u32, u64), usize> = BTreeMap::new();
        let mut last_clock_and_timestamp: BTreeMap<u32, (u32, u64)> = BTreeMap::new();
        let mut retained = Vec::with_capacity(self.groups.len());
        for group in self.groups.drain(..) {
            let realm = group.header.realm_id;
            last_clock_and_timestamp
                .entry(realm)
                .and_modify(|entry| {
                    if group.header.timestamp > entry.1 {
                        *entry = (group.header.clock_id, group.header.timestamp);
                    }
                })
                .or_insert((group.header.clock_id, group.header.timestamp));
            let key = (realm, group.header.arg);
            match group.header.kind {
                RecordKind::SpanBegin => {
                    *depths.entry(key).or_default() += 1;
                    retained.push(group);
                }
                RecordKind::SpanEnd => {
                    let depth = depths.entry(key).or_default();
                    if *depth == 0 {
                        self.health
                            .entry(realm)
                            .or_default()
                            .repaired_span_boundaries += 1;
                    } else {
                        *depth -= 1;
                        retained.push(group);
                    }
                }
                _ => retained.push(group),
            }
        }
        for ((realm, track), depth) in depths {
            let Some((clock_id, timestamp)) = last_clock_and_timestamp.get(&realm).copied() else {
                continue;
            };
            for offset in 0..depth {
                retained.push(EventGroup {
                    header: Record::new(
                        RecordKind::SpanEnd,
                        perfetto_everywhere_core::FLAG_GROUP_START
                            | perfetto_everywhere_core::FLAG_GROUP_END,
                        realm,
                        0,
                        clock_id,
                        timestamp.saturating_add(offset as u64 + 1),
                        0,
                        0,
                        track,
                    ),
                    fields: Vec::new(),
                });
                self.health
                    .entry(realm)
                    .or_default()
                    .repaired_span_boundaries += 1;
            }
        }
        self.groups = retained;
    }

    fn validate_references(&self) -> Result<(), CollectorError> {
        for group in &self.groups {
            let header = group.header;
            if !self.realms.contains_key(&header.realm_id) {
                return Err(CollectorError::UnknownRealm(header.realm_id));
            }
            let samples = self
                .calibrations
                .get(&header.realm_id)
                .ok_or(CollectorError::MissingCalibration(header.realm_id))?;
            if samples
                .first()
                .is_none_or(|sample| sample.clock_id != header.clock_id)
            {
                return Err(CollectorError::MissingCalibration(header.realm_id));
            }
            if !matches!(header.kind, RecordKind::SpanEnd) {
                self.metadata_label(header.name_id)?;
            }
            if matches!(header.kind, RecordKind::SpanBegin | RecordKind::Instant) {
                self.metadata_label(header.value as u32)?;
            }
            if header.kind == RecordKind::Log {
                self.metadata_label(header.arg as u32)?;
            }
            for field in &group.fields {
                self.metadata_label(field.name_id)?;
                if field.kind == RecordKind::FieldStaticStr {
                    self.metadata_label(field.value as u32)?;
                }
            }
        }
        Ok(())
    }

    fn metadata_label(&self, id: u32) -> Result<&str, CollectorError> {
        self.metadata
            .get(&id)
            .map(|entry| entry.label.as_str())
            .ok_or(CollectorError::UnknownMetadata(id))
    }

    fn emit_clock_snapshots(&self, packets: &mut Vec<TracePacket>) -> Result<(), CollectorError> {
        let reference_origin = self
            .calibrations
            .values()
            .flat_map(|samples| samples.iter().map(|sample| sample.reference_time_ns))
            .min()
            .ok_or(CollectorError::InvalidClockSample(0))?;
        for (realm, samples) in &self.calibrations {
            if !self.realms.contains_key(realm) {
                return Err(CollectorError::UnknownRealm(*realm));
            }
            for sample in samples {
                let snapshot = ClockSnapshot {
                    clocks: vec![
                        clock_snapshot::Clock {
                            clock_id: Some(REFERENCE_CLOCK_ID),
                            timestamp: Some(
                                sample.reference_time_ns.saturating_sub(reference_origin)
                                    + 1_000_000_000,
                            ),
                            is_incremental: Some(false),
                            unit_multiplier_ns: None,
                        },
                        clock_snapshot::Clock {
                            clock_id: Some(CUSTOM_CLOCK_ID),
                            timestamp: Some(sample.source_ticks),
                            is_incremental: Some(false),
                            unit_multiplier_ns: Some(1),
                        },
                    ],
                    primary_trace_clock: Some(BuiltinClock::Boottime as i32),
                };
                packets.push(sequence_packet(
                    *realm,
                    trace_packet::Data::ClockSnapshot(snapshot),
                ));
            }
        }
        Ok(())
    }

    fn emit_descriptors(&self, packets: &mut Vec<TracePacket>) -> Result<(), CollectorError> {
        let mut event_tracks: BTreeSet<(u32, u64)> =
            self.health.keys().map(|realm| (*realm, 0)).collect();
        let mut counter_tracks = BTreeSet::new();
        for group in &self.groups {
            let track = if group.header.kind == RecordKind::Log {
                0
            } else {
                group.header.arg
            };
            if matches!(
                group.header.kind,
                RecordKind::CounterI64 | RecordKind::CounterF64
            ) {
                counter_tracks.insert((group.header.realm_id, track, group.header.name_id));
                event_tracks.insert((group.header.realm_id, track));
            } else {
                event_tracks.insert((group.header.realm_id, track));
            }
        }
        for (realm, track) in event_tracks {
            let realm_label = &self.realms[&realm].label;
            let name = if track == 0 {
                realm_label.clone()
            } else {
                format!("{realm_label}/track {track}")
            };
            packets.push(packet(trace_packet::Data::TrackDescriptor(
                TrackDescriptor {
                    uuid: Some(event_track_uuid(realm, track)),
                    static_or_dynamic_name: Some(track_descriptor::StaticOrDynamicName::Name(name)),
                    disallow_merging_with_system_tracks: Some(true),
                    ..Default::default()
                },
            )));
        }
        for (realm, track, name_id) in counter_tracks {
            let name = self.metadata_label(name_id)?;
            packets.push(packet(trace_packet::Data::TrackDescriptor(
                TrackDescriptor {
                    uuid: Some(counter_track_uuid(realm, track, name_id)),
                    parent_uuid: Some(event_track_uuid(realm, track)),
                    counter: Some(CounterDescriptor {
                        categories: vec!["browser".to_owned()],
                        is_incremental: Some(false),
                        ..Default::default()
                    }),
                    static_or_dynamic_name: Some(track_descriptor::StaticOrDynamicName::Name(
                        format!("{}/{}", self.realms[&realm].label, name),
                    )),
                    ..Default::default()
                },
            )));
        }
        Ok(())
    }

    fn emit_events(&self, packets: &mut Vec<TracePacket>) -> Result<(), CollectorError> {
        for group in &self.groups {
            let header = group.header;
            let (event_type, name, counter) = match header.kind {
                RecordKind::SpanBegin => (
                    track_event::Type::SliceBegin,
                    Some(self.metadata_label(header.name_id)?.to_owned()),
                    None,
                ),
                RecordKind::SpanEnd => (track_event::Type::SliceEnd, None, None),
                RecordKind::Instant | RecordKind::Log | RecordKind::Health => (
                    track_event::Type::Instant,
                    Some(self.metadata_label(header.name_id)?.to_owned()),
                    None,
                ),
                RecordKind::CounterI64 => (
                    track_event::Type::Counter,
                    None,
                    Some(track_event::CounterValueField::CounterValue(
                        header.value as i64,
                    )),
                ),
                RecordKind::CounterF64 => (
                    track_event::Type::Counter,
                    None,
                    Some(track_event::CounterValueField::DoubleCounterValue(
                        f64::from_bits(header.value),
                    )),
                ),
                kind if kind.is_field() => {
                    return Err(CollectorError::Protocol(
                        "field used as group header".to_owned(),
                    ));
                }
                _ => {
                    return Err(CollectorError::Protocol(
                        "unsupported record kind".to_owned(),
                    ));
                }
            };
            let is_counter = counter.is_some();
            let track = if header.kind == RecordKind::Log {
                0
            } else {
                header.arg
            };
            let mut annotations = self.field_annotations(&group.fields)?;
            annotations.push(annotation(
                "source_frame_or_tick",
                debug_annotation::Value::UintValue(header.timestamp),
            ));
            if matches!(header.kind, RecordKind::SpanBegin | RecordKind::Instant) {
                annotations.push(annotation(
                    "category",
                    debug_annotation::Value::StringValue(
                        self.metadata_label(header.value as u32)?.to_owned(),
                    ),
                ));
            }
            if header.kind == RecordKind::Log {
                annotations.extend([
                    annotation("severity", debug_annotation::Value::UintValue(header.value)),
                    annotation(
                        "target",
                        debug_annotation::Value::StringValue(
                            self.metadata_label(header.arg as u32)?.to_owned(),
                        ),
                    ),
                ]);
            }
            let event = TrackEvent {
                categories: if is_counter {
                    Vec::new()
                } else {
                    vec!["browser".to_owned()]
                },
                r#type: Some(event_type as i32),
                track_uuid: Some(if is_counter {
                    counter_track_uuid(header.realm_id, track, header.name_id)
                } else {
                    event_track_uuid(header.realm_id, track)
                }),
                name_field: name.map(track_event::NameField::Name),
                counter_value_field: counter,
                flow_ids: if header.flags & FLAG_FLOW_STEP != 0 && header.flow_id != 0 {
                    vec![header.flow_id]
                } else {
                    Vec::new()
                },
                terminating_flow_ids: if header.flags & FLAG_FLOW_TERMINATE != 0
                    && header.flow_id != 0
                {
                    vec![header.flow_id]
                } else {
                    Vec::new()
                },
                debug_annotations: annotations,
                ..Default::default()
            };
            packets.push(TracePacket {
                timestamp: Some(header.timestamp),
                timestamp_clock_id: Some(CUSTOM_CLOCK_ID),
                data: Some(trace_packet::Data::TrackEvent(event)),
                optional_trusted_packet_sequence_id: Some(
                    trace_packet::OptionalTrustedPacketSequenceId::TrustedPacketSequenceId(
                        header.realm_id,
                    ),
                ),
                ..Default::default()
            });
        }
        Ok(())
    }

    fn field_annotations(&self, fields: &[Record]) -> Result<Vec<DebugAnnotation>, CollectorError> {
        fields
            .iter()
            .map(|field| {
                let value = match field.kind {
                    RecordKind::FieldBool => debug_annotation::Value::BoolValue(field.value != 0),
                    RecordKind::FieldI64 => debug_annotation::Value::IntValue(field.value as i64),
                    RecordKind::FieldU64 => debug_annotation::Value::UintValue(field.value),
                    RecordKind::FieldF64 => {
                        debug_annotation::Value::DoubleValue(f64::from_bits(field.value))
                    }
                    RecordKind::FieldStaticStr => debug_annotation::Value::StringValue(
                        self.metadata_label(field.value as u32)?.to_owned(),
                    ),
                    _ => {
                        return Err(CollectorError::Protocol(
                            "non-field in field group".to_owned(),
                        ));
                    }
                };
                Ok(annotation(self.metadata_label(field.name_id)?, value))
            })
            .collect()
    }

    fn emit_health(&self, packets: &mut Vec<TracePacket>) -> Result<(), CollectorError> {
        for (realm, health) in &self.health {
            if !self.realms.contains_key(realm) || !self.calibrations.contains_key(realm) {
                return Err(CollectorError::UnknownRealm(*realm));
            }
            let timestamp = self.calibrations[realm]
                .last()
                .expect("calibration validated")
                .source_ticks;
            let event = TrackEvent {
                categories: vec!["browser.health".to_owned()],
                r#type: Some(track_event::Type::Instant as i32),
                track_uuid: Some(event_track_uuid(*realm, 0)),
                name_field: Some(track_event::NameField::Name(
                    "trace producer health".to_owned(),
                )),
                debug_annotations: vec![
                    annotation(
                        "emitted_records",
                        debug_annotation::Value::UintValue(health.emitted_records),
                    ),
                    annotation(
                        "dropped_records",
                        debug_annotation::Value::UintValue(health.dropped_records),
                    ),
                    annotation(
                        "completed_batches",
                        debug_annotation::Value::UintValue(health.completed_batches),
                    ),
                    annotation(
                        "high_water_records",
                        debug_annotation::Value::UintValue(health.high_water_records as u64),
                    ),
                    annotation(
                        "repaired_span_boundaries",
                        debug_annotation::Value::UintValue(health.repaired_span_boundaries),
                    ),
                ],
                ..Default::default()
            };
            packets.push(TracePacket {
                timestamp: Some(timestamp),
                timestamp_clock_id: Some(CUSTOM_CLOCK_ID),
                data: Some(trace_packet::Data::TrackEvent(event)),
                optional_trusted_packet_sequence_id: Some(
                    trace_packet::OptionalTrustedPacketSequenceId::TrustedPacketSequenceId(*realm),
                ),
                ..Default::default()
            });
        }
        Ok(())
    }
}

fn packet(data: trace_packet::Data) -> TracePacket {
    TracePacket {
        data: Some(data),
        ..Default::default()
    }
}

fn sequence_packet(sequence: u32, data: trace_packet::Data) -> TracePacket {
    TracePacket {
        data: Some(data),
        optional_trusted_packet_sequence_id: Some(
            trace_packet::OptionalTrustedPacketSequenceId::TrustedPacketSequenceId(sequence),
        ),
        ..Default::default()
    }
}

fn annotation(name: &str, value: debug_annotation::Value) -> DebugAnnotation {
    DebugAnnotation {
        name_field: Some(debug_annotation::NameField::Name(name.to_owned())),
        value: Some(value),
        ..Default::default()
    }
}

fn mix(mut value: u64) -> u64 {
    value ^= value >> 30;
    value = value.wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value ^= value >> 27;
    value = value.wrapping_mul(0x94d0_49bb_1331_11eb);
    value ^ (value >> 31)
}

fn event_track_uuid(realm: u32, track: u64) -> u64 {
    0x1000_0000_0000_0000 | (mix((u64::from(realm) << 32) ^ track) & 0x0fff_ffff_ffff_ffff)
}

fn counter_track_uuid(realm: u32, track: u64, name: u32) -> u64 {
    0x2000_0000_0000_0000
        | (mix((u64::from(realm) << 32) ^ track ^ u64::from(name)) & 0x0fff_ffff_ffff_ffff)
}

#[cfg(target_arch = "wasm32")]
mod wasm {
    use super::*;
    use wasm_bindgen::prelude::*;

    #[wasm_bindgen]
    pub struct WasmCollector {
        inner: Option<Collector>,
    }

    #[wasm_bindgen]
    impl WasmCollector {
        #[wasm_bindgen(constructor)]
        pub fn new(max_records: usize) -> Self {
            Self {
                inner: Some(Collector::new(CollectorConfig {
                    max_records,
                    ..CollectorConfig::default()
                })),
            }
        }

        pub fn register_realm(&mut self, id: u32, label: String) -> Result<(), JsValue> {
            self.inner_mut()?
                .register_realm(RealmDescriptor { id, label })
                .map_err(js_error)
        }

        pub fn register_metadata(
            &mut self,
            id: u32,
            namespace: u8,
            label: String,
        ) -> Result<(), JsValue> {
            self.inner_mut()?
                .register_metadata(MetadataEntry {
                    id: perfetto_everywhere_core::MetadataId(id),
                    namespace,
                    label,
                })
                .map_err(js_error)
        }

        #[allow(clippy::too_many_arguments)]
        pub fn add_calibration(
            &mut self,
            realm_id: u32,
            clock_id: u32,
            source_ticks: u64,
            reference_time_ns: u64,
            uncertainty_ns: u64,
        ) -> Result<(), JsValue> {
            self.inner_mut()?
                .add_calibration(ClockCalibration {
                    realm_id,
                    clock_id,
                    source_ticks,
                    reference_time_ns,
                    uncertainty_ns,
                })
                .map_err(js_error)
        }

        pub fn ingest_batch(&mut self, bytes: &[u8]) -> Result<(), JsValue> {
            self.inner_mut()?.ingest_batch(bytes).map_err(js_error)
        }

        pub fn finish(&mut self) -> Result<Vec<u8>, JsValue> {
            self.inner
                .take()
                .ok_or_else(|| JsValue::from_str("collector already finished"))?
                .finish()
                .map_err(js_error)
        }

        fn inner_mut(&mut self) -> Result<&mut Collector, JsValue> {
            self.inner
                .as_mut()
                .ok_or_else(|| JsValue::from_str("collector already finished"))
        }
    }

    fn js_error(error: CollectorError) -> JsValue {
        JsValue::from_str(&error.to_string())
    }
}

#[cfg(target_arch = "wasm32")]
pub use wasm::WasmCollector;

#[cfg(test)]
mod tests {
    use super::*;
    use perfetto_everywhere_core::{FLAG_GROUP_END, FLAG_GROUP_START, MetadataId};

    fn metadata(id: u32, label: &str) -> MetadataEntry {
        MetadataEntry {
            id: MetadataId(id),
            namespace: 1,
            label: label.to_owned(),
        }
    }

    fn record(realm: u32, timestamp: u64) -> Record {
        Record::new(
            RecordKind::Instant,
            FLAG_GROUP_START | FLAG_GROUP_END,
            realm,
            10,
            realm + 100,
            timestamp,
            20,
            0,
            0,
        )
    }

    fn configured() -> Collector {
        let mut collector = Collector::new(CollectorConfig::default());
        collector
            .register_realm(RealmDescriptor {
                id: 1,
                label: "page".to_owned(),
            })
            .unwrap();
        collector.register_metadata(metadata(10, "event")).unwrap();
        collector
            .register_metadata(metadata(20, "category"))
            .unwrap();
        collector
            .add_calibration(ClockCalibration {
                realm_id: 1,
                clock_id: 101,
                source_ticks: 1_000,
                reference_time_ns: 10_000,
                uncertainty_ns: 10,
            })
            .unwrap();
        collector
    }

    #[test]
    fn emits_parseable_trace_with_custom_clock_snapshot() {
        let mut collector = configured();
        collector.ingest_batch(&record(1, 1_100).encode()).unwrap();
        let bytes = collector.finish().unwrap();
        let trace = Trace::decode(bytes.as_slice()).unwrap();
        assert!(
            trace
                .packet
                .iter()
                .any(|packet| matches!(packet.data, Some(trace_packet::Data::ClockSnapshot(_))))
        );
        assert!(trace.packet.iter().any(|packet| {
            packet.timestamp == Some(1_100) && packet.timestamp_clock_id == Some(CUSTOM_CLOCK_ID)
        }));
    }

    #[test]
    fn rejects_missing_or_nonmonotonic_clock_samples() {
        let mut collector = configured();
        assert_eq!(
            collector.add_calibration(ClockCalibration {
                realm_id: 1,
                clock_id: 101,
                source_ticks: 900,
                reference_time_ns: 11_000,
                uncertainty_ns: 0,
            }),
            Err(CollectorError::NonMonotonicCalibration(1))
        );
        assert_eq!(
            collector.add_calibration(ClockCalibration {
                realm_id: 2,
                clock_id: 102,
                source_ticks: 1,
                reference_time_ns: 1,
                uncertainty_ns: 5_000_001,
            }),
            Err(CollectorError::InvalidClockSample(2))
        );
        let mut missing = Collector::new(CollectorConfig::default());
        missing
            .register_realm(RealmDescriptor {
                id: 1,
                label: "page".to_owned(),
            })
            .unwrap();
        missing.register_metadata(metadata(10, "event")).unwrap();
        missing.register_metadata(metadata(20, "category")).unwrap();
        missing.ingest_batch(&record(1, 1_100).encode()).unwrap();
        assert_eq!(missing.finish(), Err(CollectorError::MissingCalibration(1)));
    }

    #[test]
    fn rejects_partial_malformed_unknown_and_over_limit_input() {
        let mut collector = configured();
        assert!(matches!(
            collector.ingest_batch(&[0; RECORD_SIZE - 1]),
            Err(CollectorError::PartialRecordBytes(_))
        ));
        let mut bad = record(1, 1_100).encode();
        bad[0] = 99;
        assert!(matches!(
            collector.ingest_batch(&bad),
            Err(CollectorError::Protocol(_))
        ));
        let mut limited = Collector::new(CollectorConfig {
            max_records: 0,
            ..CollectorConfig::default()
        });
        assert_eq!(
            limited.ingest_batch(&record(1, 1_100).encode()),
            Err(CollectorError::CaptureLimitExceeded)
        );
    }

    #[test]
    fn rejects_realm_metadata_collisions_and_unknown_metadata() {
        let mut collector = configured();
        assert_eq!(
            collector.register_realm(RealmDescriptor {
                id: 1,
                label: "different".to_owned(),
            }),
            Err(CollectorError::RealmCollision(1))
        );
        assert_eq!(
            collector.register_metadata(metadata(10, "different")),
            Err(CollectorError::MetadataCollision(10))
        );
        let mut unknown = record(1, 1_100);
        unknown.name_id = 999;
        collector.ingest_batch(&unknown.encode()).unwrap();
        assert_eq!(
            collector.finish(),
            Err(CollectorError::UnknownMetadata(999))
        );
    }

    #[test]
    fn repairs_and_reports_incomplete_span_boundaries() {
        let mut collector = configured();
        let mut open = record(1, 1_100);
        open.kind = RecordKind::SpanBegin;
        collector.ingest_batch(&open.encode()).unwrap();
        let bytes = collector.finish().unwrap();
        let trace = Trace::decode(bytes.as_slice()).unwrap();
        let events: Vec<&TrackEvent> = trace
            .packet
            .iter()
            .filter_map(|packet| match &packet.data {
                Some(trace_packet::Data::TrackEvent(event)) => Some(event),
                _ => None,
            })
            .collect();
        assert!(
            events
                .iter()
                .any(|event| event.r#type == Some(track_event::Type::SliceEnd as i32))
        );
        assert!(events.iter().any(|event| {
            event.debug_annotations.iter().any(|annotation| {
                annotation.name_field
                    == Some(debug_annotation::NameField::Name(
                        "repaired_span_boundaries".to_owned(),
                    ))
                    && annotation.value == Some(debug_annotation::Value::UintValue(1))
            })
        }));
    }

    #[test]
    fn periodic_samples_remain_in_source_order() {
        let mut collector = configured();
        collector
            .add_calibration(ClockCalibration {
                realm_id: 1,
                clock_id: 101,
                source_ticks: 2_000,
                reference_time_ns: 11_001,
                uncertainty_ns: 20,
            })
            .unwrap();
        collector.ingest_batch(&record(1, 1_500).encode()).unwrap();
        let bytes = collector.finish().unwrap();
        let trace = Trace::decode(bytes.as_slice()).unwrap();
        assert_eq!(
            trace
                .packet
                .iter()
                .filter(|packet| matches!(packet.data, Some(trace_packet::Data::ClockSnapshot(_))))
                .count(),
            2
        );
    }
}
