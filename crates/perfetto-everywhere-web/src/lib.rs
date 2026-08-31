#![doc = "Bounded browser producers for `perfetto-everywhere`."]
#![forbid(unsafe_code)]

mod audio;
mod chunks;

pub use chunks::{
    CHUNK_PROTOCOL_VERSION, ChunkCollectorState, ChunkDescriptor, ChunkPoolConfig,
    ChunkProtocolError, ChunkTransportHealth, MemoryChunkSink, StoppedDescriptor,
};

#[cfg(target_arch = "wasm32")]
pub use audio::AudioChunkProducer;

use perfetto_everywhere_core::{
    Category, EmitStatus, FLAG_FLOW_STEP, FLAG_FLOW_TERMINATE, FLAG_GROUP_END, FLAG_GROUP_START,
    Field, FieldValue, FlowAttachment, MetadataId, RECORD_SIZE, Record, RecordKind, Severity,
    StaticName, TraceBackend, TrackId,
};
use std::{
    cell::{Cell, RefCell},
    collections::{BTreeMap, BTreeSet},
};

pub trait SourceClock {
    fn now_ticks(&self) -> Option<u64>;
}

#[derive(Clone, Copy, Debug)]
pub struct FixedClock(pub u64);

impl SourceClock for FixedClock {
    fn now_ticks(&self) -> Option<u64> {
        Some(self.0)
    }
}

#[cfg(target_arch = "wasm32")]
#[derive(Clone, Copy, Debug, Default)]
pub struct PerformanceClock;

#[cfg(target_arch = "wasm32")]
impl SourceClock for PerformanceClock {
    fn now_ticks(&self) -> Option<u64> {
        use js_sys::{Function, Reflect, global};
        use wasm_bindgen::{JsCast, JsValue};
        let performance = Reflect::get(&global(), &JsValue::from_str("performance")).ok()?;
        let now: Function = Reflect::get(&performance, &JsValue::from_str("now"))
            .ok()?
            .dyn_into()
            .ok()?;
        let milliseconds = now.call0(&performance).ok()?.as_f64()?;
        Some((milliseconds * 1_000_000.0).round() as u64)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MetadataEntry {
    pub id: MetadataId,
    pub namespace: u8,
    pub label: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ClockCalibration {
    pub realm_id: u32,
    pub clock_id: u32,
    pub source_ticks: u64,
    pub reference_time_ns: u64,
    pub uncertainty_ns: u64,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ProducerHealth {
    pub emitted_records: u64,
    pub dropped_records: u64,
    pub raw_dropped_records: u64,
    pub pool_starvation_records: u64,
    pub completed_batches: u64,
    pub high_water_records: usize,
    pub max_in_flight_chunks: usize,
    pub returned_buffers: u64,
    pub rejected_chunks: u64,
    pub storage_failures: u64,
    /// Collector-side repairs of unmatched span boundaries attributed to this realm.
    pub repaired_span_boundaries: u64,
}

#[derive(Debug)]
struct BatchPool {
    capacity: usize,
    active: Vec<Record>,
    ready: Option<Vec<Record>>,
    spare: Option<Vec<Record>>,
    health: ProducerHealth,
}

impl BatchPool {
    fn new(capacity: usize) -> Self {
        let capacity = capacity.max(1);
        Self {
            capacity,
            active: Vec::with_capacity(capacity),
            ready: None,
            spare: Some(Vec::with_capacity(capacity)),
            health: ProducerHealth::default(),
        }
    }

    fn reserve_group(&mut self, count: usize) -> bool {
        if count > self.capacity {
            self.health.dropped_records += count as u64;
            return false;
        }
        if self.active.len() + count <= self.capacity {
            return true;
        }
        if self.ready.is_some() {
            self.health.dropped_records += count as u64;
            return false;
        }
        self.rotate_active();
        true
    }

    fn rotate_active(&mut self) {
        debug_assert!(self.ready.is_none());
        let replacement = self.spare.take().expect("batch pool spare invariant");
        let completed = core::mem::replace(&mut self.active, replacement);
        if completed.is_empty() {
            self.spare = Some(completed);
        } else {
            self.health.completed_batches += 1;
            self.ready = Some(completed);
        }
    }

    fn push(&mut self, record: Record) {
        self.active.push(record);
        self.health.emitted_records += 1;
        self.health.high_water_records = self.health.high_water_records.max(self.active.len());
    }

    fn flush(&mut self) -> bool {
        if self.active.is_empty() || self.ready.is_some() {
            return false;
        }
        self.rotate_active();
        true
    }

    fn drain_ready_bytes(&mut self) -> Option<Vec<u8>> {
        let records = self.ready.take()?;
        let mut output = Vec::with_capacity(records.len() * RECORD_SIZE);
        for record in &records {
            output.extend_from_slice(&record.encode());
        }
        let mut records = records;
        records.clear();
        self.spare = Some(records);
        Some(output)
    }
}

/// Ordinary Window/Dedicated Worker backend. Event storage is bounded to two
/// reusable record batches; exported transfer bytes are allocated outside the
/// event call and may be transferred to a collector Worker.
pub struct OrdinaryBackend<C> {
    realm_id: u32,
    clock_id: u32,
    clock: C,
    enabled: Cell<bool>,
    categories: BTreeSet<u32>,
    batches: RefCell<BatchPool>,
    metadata: RefCell<BTreeMap<u32, MetadataEntry>>,
}

impl<C: SourceClock> OrdinaryBackend<C> {
    pub fn new(
        realm_id: u32,
        clock_id: u32,
        clock: C,
        capacity_records: usize,
        categories: &[Category],
    ) -> Result<Self, &'static str> {
        if realm_id == 0 {
            return Err("realm ID zero is reserved");
        }
        if clock_id == 0 {
            return Err("clock ID zero is reserved");
        }
        Ok(Self {
            realm_id,
            clock_id,
            clock,
            enabled: Cell::new(true),
            categories: categories.iter().map(|category| category.id.0).collect(),
            batches: RefCell::new(BatchPool::new(capacity_records)),
            metadata: RefCell::new(BTreeMap::new()),
        })
    }

    pub fn set_enabled(&self, enabled: bool) {
        self.enabled.set(enabled);
    }

    pub fn flush(&self) -> bool {
        self.batches.borrow_mut().flush()
    }

    pub fn take_batch(&self) -> Option<Vec<u8>> {
        self.batches.borrow_mut().drain_ready_bytes()
    }

    pub fn flush_and_take_batch(&self) -> Option<Vec<u8>> {
        self.flush();
        self.take_batch()
    }

    pub fn health(&self) -> ProducerHealth {
        self.batches.borrow().health
    }

    pub fn take_metadata(&self) -> Vec<MetadataEntry> {
        core::mem::take(&mut *self.metadata.borrow_mut())
            .into_values()
            .collect()
    }

    fn category_enabled(&self, category: Category) -> bool {
        self.enabled.get()
            && (self.categories.is_empty() || self.categories.contains(&category.id.0))
    }

    fn register_metadata(&self, id: MetadataId, namespace: u8, label: &str) -> bool {
        let mut metadata = self.metadata.borrow_mut();
        if let Some(existing) = metadata.get(&id.0) {
            return existing.namespace == namespace && existing.label == label;
        }
        metadata.insert(
            id.0,
            MetadataEntry {
                id,
                namespace,
                label: label.to_owned(),
            },
        );
        true
    }

    fn prepare_metadata(
        &self,
        category: Option<Category>,
        name: StaticName,
        fields: &[Field<'_>],
    ) -> bool {
        if !self.register_metadata(name.id, 1, name.label) {
            return false;
        }
        if let Some(category) = category {
            if !self.register_metadata(category.id, 2, category.label) {
                return false;
            }
        }
        for field in fields {
            if !self.register_metadata(field.name.id, 3, field.name.label) {
                return false;
            }
            match field.value {
                FieldValue::StaticStr(value)
                    if !self.register_metadata(value.id, 1, value.label) =>
                {
                    return false;
                }
                FieldValue::Str(value) => {
                    let id = MetadataId::for_label(4, value);
                    if !self.register_metadata(id, 4, value) {
                        return false;
                    }
                }
                _ => {}
            }
        }
        true
    }

    fn flow_parts(flow: FlowAttachment) -> (u16, u64) {
        match flow {
            FlowAttachment::None => (0, 0),
            FlowAttachment::Step(id) => (FLAG_FLOW_STEP, id.get()),
            FlowAttachment::Terminate(id) => (FLAG_FLOW_TERMINATE, id.get()),
        }
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
        let Some(timestamp) = self.clock.now_ticks() else {
            return EmitStatus::Unsupported;
        };
        if !self.prepare_metadata(category, name, fields) {
            return EmitStatus::Unsupported;
        }
        let count = 1 + fields.len();
        let mut batches = self.batches.borrow_mut();
        if !batches.reserve_group(count) {
            return EmitStatus::Dropped;
        }
        let (flow_flag, flow_id) = Self::flow_parts(flow);
        let header_end = if fields.is_empty() { FLAG_GROUP_END } else { 0 };
        batches.push(Record::new(
            kind,
            FLAG_GROUP_START | header_end | flow_flag,
            self.realm_id,
            name.id.0,
            self.clock_id,
            timestamp,
            value,
            flow_id,
            arg_override.unwrap_or(track.0),
        ));
        for (index, field) in fields.iter().enumerate() {
            let is_last = index + 1 == fields.len();
            let (kind, value) = match field.value {
                FieldValue::Bool(value) => (RecordKind::FieldBool, u64::from(value)),
                FieldValue::I64(value) => (RecordKind::FieldI64, value as u64),
                FieldValue::U64(value) => (RecordKind::FieldU64, value),
                FieldValue::F64(value) => (RecordKind::FieldF64, value.to_bits()),
                FieldValue::StaticStr(value) => (RecordKind::FieldStaticStr, u64::from(value.id.0)),
                FieldValue::Str(value) => (
                    RecordKind::FieldStaticStr,
                    u64::from(MetadataId::for_label(4, value).0),
                ),
            };
            batches.push(Record::new(
                kind,
                if is_last { FLAG_GROUP_END } else { 0 },
                self.realm_id,
                field.name.id.0,
                self.clock_id,
                timestamp,
                value,
                0,
                0,
            ));
        }
        EmitStatus::Recorded
    }
}

impl<C: SourceClock> TraceBackend for OrdinaryBackend<C> {
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
            u64::from(category.id.0),
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
            u64::from(category.id.0),
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
        if !self.enabled.get() || !self.register_metadata(target.id, 1, target.label) {
            return EmitStatus::Disabled;
        }
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
    use perfetto_everywhere_core::{FieldName, validate_record_groups};

    const APP: Category = Category::new("application");
    const EVENT: StaticName = StaticName::new("event");

    fn decode(bytes: &[u8]) -> Vec<Record> {
        bytes
            .chunks_exact(RECORD_SIZE)
            .map(|chunk| Record::decode(chunk).unwrap())
            .collect()
    }

    #[test]
    fn emits_typed_complete_groups_and_metadata() {
        let backend = OrdinaryBackend::new(2, 3, FixedClock(99), 8, &[APP]).unwrap();
        let fields = [
            Field::new(FieldName::new("ok"), FieldValue::Bool(true)),
            Field::new(FieldName::new("count"), FieldValue::I64(-2)),
            Field::new(FieldName::new("ratio"), FieldValue::F64(0.5)),
            Field::new(FieldName::new("detail"), FieldValue::Str("dynamic")),
        ];
        assert_eq!(
            backend.event(APP, EVENT, TrackId(7), &fields, FlowAttachment::None),
            EmitStatus::Recorded
        );
        let bytes = backend.flush_and_take_batch().unwrap();
        let records = decode(&bytes);
        assert_eq!(records.len(), 5);
        assert!(validate_record_groups(&records).is_ok());
        assert_eq!(records[0].timestamp, 99);
        assert!(
            backend
                .take_metadata()
                .iter()
                .any(|entry| entry.label == "dynamic")
        );
    }

    #[test]
    fn bounded_double_batches_drop_without_growing() {
        let backend = OrdinaryBackend::new(2, 3, FixedClock(1), 2, &[]).unwrap();
        for _ in 0..4 {
            assert_eq!(
                backend.event(APP, EVENT, TrackId::CURRENT, &[], FlowAttachment::None),
                EmitStatus::Recorded
            );
        }
        assert_eq!(
            backend.event(APP, EVENT, TrackId::CURRENT, &[], FlowAttachment::None),
            EmitStatus::Dropped
        );
        let health = backend.health();
        assert_eq!(health.emitted_records, 4);
        assert_eq!(health.dropped_records, 1);
        assert!(health.high_water_records <= 2);
        assert_eq!(backend.take_batch().unwrap().len(), 2 * RECORD_SIZE);
        assert!(backend.flush());
        assert_eq!(backend.take_batch().unwrap().len(), 2 * RECORD_SIZE);
    }

    #[test]
    fn filtering_and_shutdown_switch_are_explicit() {
        let backend = OrdinaryBackend::new(2, 3, FixedClock(1), 2, &[APP]).unwrap();
        assert!(!backend.is_enabled(Category::new("other")));
        backend.set_enabled(false);
        assert_eq!(
            backend.event(APP, EVENT, TrackId::CURRENT, &[], FlowAttachment::None),
            EmitStatus::Disabled
        );
    }
}
