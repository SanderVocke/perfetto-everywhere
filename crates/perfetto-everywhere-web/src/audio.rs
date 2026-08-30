#[cfg(any(target_arch = "wasm32", test))]
use perfetto_everywhere_core::RECORD_SIZE;

pub const AUDIO_HEADER_WORDS: usize = 16;
pub const AUDIO_HEADER_BYTES: usize = AUDIO_HEADER_WORDS * 4;
pub const AUDIO_RING_MAGIC: i32 = 0x5045_4631; // "PEF1"

#[cfg(target_arch = "wasm32")]
const MAGIC_INDEX: u32 = 0;
#[cfg(target_arch = "wasm32")]
const CAPACITY_INDEX: u32 = 1;
#[cfg(target_arch = "wasm32")]
const WRITE_INDEX: u32 = 2;
#[cfg(target_arch = "wasm32")]
const READ_INDEX: u32 = 3;
#[cfg(target_arch = "wasm32")]
const DROPPED_INDEX: u32 = 4;
#[cfg(target_arch = "wasm32")]
const CALLBACKS_INDEX: u32 = 5;
#[cfg(target_arch = "wasm32")]
const DONE_INDEX: u32 = 6;
#[cfg(target_arch = "wasm32")]
const DISCONTINUITIES_INDEX: u32 = 7;
#[cfg(target_arch = "wasm32")]
const HIGH_WATER_INDEX: u32 = 8;
#[cfg(target_arch = "wasm32")]
const SAMPLE_RATE_INDEX: u32 = 9;
#[cfg(target_arch = "wasm32")]
const QUANTUM_FRAMES_INDEX: u32 = 10;

pub const fn ring_can_reserve(write: u32, read: u32, capacity: u32, count: u32) -> bool {
    let occupancy = write.wrapping_sub(read);
    occupancy <= capacity && count <= capacity - occupancy
}

#[cfg(target_arch = "wasm32")]
mod wasm {
    use super::*;
    use js_sys::{Atomics, DataView, Int32Array, SharedArrayBuffer};
    use perfetto_everywhere_core::{
        Category, EmitStatus, FLAG_FLOW_STEP, FLAG_FLOW_TERMINATE, FLAG_GROUP_END,
        FLAG_GROUP_START, Field, FieldValue, FlowAttachment, FlowId, Record, RecordKind, Severity,
        StaticName, TraceBackend, TrackId,
    };
    use std::cell::Cell;
    use wasm_bindgen::prelude::*;

    pub struct AudioRingBackend {
        header: Int32Array,
        data: DataView,
        capacity: u32,
        realm_id: u32,
        clock_id: u32,
        current_frame: Cell<u64>,
        expected_frame: Cell<Option<u64>>,
    }

    impl AudioRingBackend {
        pub fn new(
            buffer: SharedArrayBuffer,
            realm_id: u32,
            clock_id: u32,
        ) -> Result<Self, JsValue> {
            if realm_id == 0 || clock_id == 0 {
                return Err(JsValue::from_str("realm and clock IDs must be nonzero"));
            }
            if buffer.byte_length() < AUDIO_HEADER_BYTES as u32 + RECORD_SIZE as u32 {
                return Err(JsValue::from_str("audio ring buffer is too small"));
            }
            let header = Int32Array::new_with_byte_offset_and_length(
                buffer.as_ref(),
                0,
                AUDIO_HEADER_WORDS as u32,
            );
            if Atomics::load(&header, MAGIC_INDEX)? != AUDIO_RING_MAGIC {
                return Err(JsValue::from_str("audio ring magic/version mismatch"));
            }
            let capacity = Atomics::load(&header, CAPACITY_INDEX)? as u32;
            let sample_rate = Atomics::load(&header, SAMPLE_RATE_INDEX)?;
            let quantum_frames = Atomics::load(&header, QUANTUM_FRAMES_INDEX)?;
            if sample_rate <= 0 || quantum_frames <= 0 {
                return Err(JsValue::from_str(
                    "sample rate and quantum size must be positive",
                ));
            }
            let expected_bytes = AUDIO_HEADER_BYTES as u32 + capacity * RECORD_SIZE as u32;
            if capacity == 0 || buffer.byte_length() < expected_bytes {
                return Err(JsValue::from_str("audio ring capacity/length mismatch"));
            }
            let data = DataView::new_with_shared_array_buffer(
                &buffer,
                AUDIO_HEADER_BYTES,
                (capacity as usize) * RECORD_SIZE,
            );
            Ok(Self {
                header,
                data,
                capacity,
                realm_id,
                clock_id,
                current_frame: Cell::new(0),
                expected_frame: Cell::new(None),
            })
        }

        /// Marks callback entry and detects a gap in the logical quantum stream.
        pub fn begin_callback(&self, frame: u64, quantum_frames: u32) -> Result<(), JsValue> {
            if let Some(expected) = self.expected_frame.get() {
                if frame != expected {
                    Atomics::add(&self.header, DISCONTINUITIES_INDEX, 1)?;
                }
            }
            self.current_frame.set(frame);
            self.expected_frame
                .set(Some(frame.saturating_add(u64::from(quantum_frames))));
            Atomics::add(&self.header, CALLBACKS_INDEX, 1)?;
            Atomics::store(&self.header, QUANTUM_FRAMES_INDEX, quantum_frames as i32)?;
            Ok(())
        }

        /// Records one complete logical quantum span and two counter samples with
        /// a single capacity decision and publication store.
        pub fn record_quantum(
            &self,
            span: StaticName,
            queue_counter: StaticName,
            load_counter: StaticName,
            frame: u64,
            quantum_frames: u32,
            queue_depth: i64,
            cpu_load: f64,
        ) -> EmitStatus {
            let Some(write) = self.reserve(4) else {
                return EmitStatus::Dropped;
            };
            let complete = FLAG_GROUP_START | FLAG_GROUP_END;
            self.write_record(
                write,
                Record::new(
                    RecordKind::SpanBegin,
                    complete,
                    self.realm_id,
                    span.id.0,
                    self.clock_id,
                    frame,
                    u64::from(Category::new("audio").id.0),
                    0,
                    0,
                ),
            );
            self.write_record(
                write.wrapping_add(1),
                Record::new(
                    RecordKind::CounterI64,
                    complete,
                    self.realm_id,
                    queue_counter.id.0,
                    self.clock_id,
                    frame,
                    queue_depth as u64,
                    0,
                    0,
                ),
            );
            self.write_record(
                write.wrapping_add(2),
                Record::new(
                    RecordKind::CounterF64,
                    complete,
                    self.realm_id,
                    load_counter.id.0,
                    self.clock_id,
                    frame,
                    cpu_load.to_bits(),
                    0,
                    0,
                ),
            );
            self.write_record(
                write.wrapping_add(3),
                Record::new(
                    RecordKind::SpanEnd,
                    complete,
                    self.realm_id,
                    0,
                    self.clock_id,
                    frame.saturating_add(u64::from(quantum_frames)),
                    0,
                    0,
                    0,
                ),
            );
            self.publish(write, 4);
            EmitStatus::Recorded
        }

        pub fn finish(&self) -> Result<(), JsValue> {
            Atomics::store(&self.header, DONE_INDEX, 1)?;
            Ok(())
        }

        fn reserve(&self, count: u32) -> Option<u32> {
            let write = Atomics::load(&self.header, WRITE_INDEX).ok()? as u32;
            let read = Atomics::load(&self.header, READ_INDEX).ok()? as u32;
            if !ring_can_reserve(write, read, self.capacity, count) {
                let _ = Atomics::add(&self.header, DROPPED_INDEX, count as i32);
                return None;
            }
            Some(write)
        }

        fn publish(&self, write: u32, count: u32) {
            let next = write.wrapping_add(count);
            let read = Atomics::load(&self.header, READ_INDEX).unwrap_or(0) as u32;
            let occupancy = next.wrapping_sub(read) as i32;
            let old_high = Atomics::load(&self.header, HIGH_WATER_INDEX).unwrap_or(0);
            if occupancy > old_high {
                let _ = Atomics::store(&self.header, HIGH_WATER_INDEX, occupancy);
            }
            let _ = Atomics::store(&self.header, WRITE_INDEX, next as i32);
        }

        fn write_record(&self, sequence: u32, record: Record) {
            let slot = sequence % self.capacity;
            let offset = slot as usize * RECORD_SIZE;
            let bytes = record.encode();
            for index in 0..(RECORD_SIZE / 4) {
                let start = index * 4;
                let word = u32::from_le_bytes([
                    bytes[start],
                    bytes[start + 1],
                    bytes[start + 2],
                    bytes[start + 3],
                ]);
                self.data.set_uint32_endian(offset + start, word, true);
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
            if fields
                .iter()
                .any(|field| matches!(field.value, FieldValue::Str(_)))
            {
                return EmitStatus::Unsupported;
            }
            let count = 1 + fields.len() as u32;
            let Some(write) = self.reserve(count) else {
                return EmitStatus::Dropped;
            };
            let (flow_flag, flow_id) = match flow {
                FlowAttachment::None => (0, 0),
                FlowAttachment::Step(id) => (FLAG_FLOW_STEP, id.get()),
                FlowAttachment::Terminate(id) => (FLAG_FLOW_TERMINATE, id.get()),
            };
            let timestamp = self.current_frame.get();
            self.write_record(
                write,
                Record::new(
                    kind,
                    FLAG_GROUP_START
                        | if fields.is_empty() { FLAG_GROUP_END } else { 0 }
                        | flow_flag,
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
                    FieldValue::StaticStr(value) => {
                        (RecordKind::FieldStaticStr, u64::from(value.id.0))
                    }
                    FieldValue::Str(_) => return EmitStatus::Unsupported,
                };
                self.write_record(
                    write.wrapping_add(index as u32 + 1),
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
            self.publish(write, count);
            EmitStatus::Recorded
        }
    }

    impl TraceBackend for AudioRingBackend {
        fn is_enabled(&self, _: Category) -> bool {
            Atomics::load(&self.header, DONE_INDEX).unwrap_or(1) == 0
        }

        fn span_begin(
            &self,
            category: Category,
            name: StaticName,
            track: TrackId,
            fields: &[Field<'_>],
            flow: FlowAttachment,
        ) -> EmitStatus {
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

    const PROCESS_QUANTUM: StaticName = StaticName::new("audio process quantum");
    const QUEUE_DEPTH: StaticName = StaticName::new("audio queue depth");
    const CPU_LOAD: StaticName = StaticName::new("audio cpu load");
    const GRAPH_INSTALLED: StaticName = StaticName::new("audio graph installed");
    const AUDIO: Category = Category::new("audio");

    #[wasm_bindgen]
    pub struct AudioRingProducer {
        backend: AudioRingBackend,
    }

    #[wasm_bindgen]
    impl AudioRingProducer {
        #[wasm_bindgen(constructor)]
        pub fn new(
            buffer: SharedArrayBuffer,
            realm_id: u32,
            clock_id: u32,
        ) -> Result<Self, JsValue> {
            Ok(Self {
                backend: AudioRingBackend::new(buffer, realm_id, clock_id)?,
            })
        }

        pub fn callback_only(&self, frame: f64, quantum_frames: u32) -> Result<(), JsValue> {
            self.backend
                .begin_callback(frame.max(0.0) as u64, quantum_frames)
        }

        pub fn process_quantum(
            &self,
            frame: f64,
            quantum_frames: u32,
            queue_depth: i32,
            cpu_load: f64,
        ) -> Result<u8, JsValue> {
            let frame = frame.max(0.0) as u64;
            self.backend.begin_callback(frame, quantum_frames)?;
            Ok(self.backend.record_quantum(
                PROCESS_QUANTUM,
                QUEUE_DEPTH,
                CPU_LOAD,
                frame,
                quantum_frames,
                i64::from(queue_depth),
                cpu_load,
            ) as u8)
        }

        pub fn install_flow(&self, frame: f64, flow: u64) -> Result<u8, JsValue> {
            self.backend.current_frame.set(frame.max(0.0) as u64);
            let attachment = FlowId::new(flow)
                .map(FlowAttachment::Terminate)
                .unwrap_or(FlowAttachment::None);
            Ok(self
                .backend
                .event(AUDIO, GRAPH_INSTALLED, TrackId::CURRENT, &[], attachment)
                as u8)
        }

        pub fn finish(&self) -> Result<(), JsValue> {
            self.backend.finish()
        }
    }
}

#[cfg(target_arch = "wasm32")]
pub use wasm::{AudioRingBackend, AudioRingProducer};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reservation_is_bounded_and_wrap_safe() {
        assert!(ring_can_reserve(4, 0, 8, 4));
        assert!(!ring_can_reserve(4, 0, 8, 5));
        assert!(ring_can_reserve(u32::MAX - 1, u32::MAX - 5, 8, 4));
        assert!(ring_can_reserve(2, u32::MAX - 1, 8, 3));
        assert!(!ring_can_reserve(20, 0, 8, 1));
    }

    #[test]
    fn header_and_records_are_aligned() {
        assert_eq!(AUDIO_HEADER_BYTES, 64);
        assert_eq!(AUDIO_HEADER_BYTES % 16, 0);
        assert_eq!(RECORD_SIZE, 48);
    }
}
