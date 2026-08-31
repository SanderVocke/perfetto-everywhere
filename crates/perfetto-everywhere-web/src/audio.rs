#[cfg(target_arch = "wasm32")]
mod wasm {
    use js_sys::Uint8Array;
    use perfetto_everywhere_core::{
        Category, FlowAttachment, FlowId, StaticName, TraceBackend, TrackId,
    };
    use perfetto_everywhere_raw::RawRingBackend;
    use std::cell::Cell;
    use wasm_bindgen::prelude::*;

    const PROCESS_QUANTUM: StaticName = StaticName::new("audio process quantum");
    const QUEUE_DEPTH: StaticName = StaticName::new("audio queue depth");
    const CPU_LOAD: StaticName = StaticName::new("audio cpu load");
    const GRAPH_INSTALLED: StaticName = StaticName::new("audio graph installed");
    const AUDIO: Category = Category::new("audio");

    #[wasm_bindgen]
    pub struct AudioChunkProducer {
        backend: RawRingBackend,
        expected_frame: Cell<Option<u64>>,
        callbacks: u64,
        discontinuities: u64,
        transfer: Vec<u8>,
    }

    #[wasm_bindgen]
    impl AudioChunkProducer {
        #[wasm_bindgen(constructor)]
        pub fn new(realm_id: u32, clock_id: u32, capacity_records: usize) -> Result<Self, JsValue> {
            let backend = RawRingBackend::new(realm_id, clock_id, capacity_records, &[AUDIO])
                .map_err(JsValue::from_str)?;
            Ok(Self {
                backend,
                expected_frame: Cell::new(None),
                callbacks: 0,
                discontinuities: 0,
                transfer: vec![0; capacity_records * perfetto_everywhere_core::RECORD_SIZE],
            })
        }

        pub fn callback_only(&mut self, frame: f64, quantum_frames: u32) {
            self.begin_callback(frame.max(0.0) as u64, quantum_frames);
        }

        pub fn process_quantum(
            &mut self,
            frame: f64,
            quantum_frames: u32,
            queue_depth: i32,
            cpu_load: f64,
        ) -> u8 {
            let frame = frame.max(0.0) as u64;
            self.begin_callback(frame, quantum_frames);
            self.backend.set_timestamp(frame);
            let status = self.backend.span_begin(
                AUDIO,
                PROCESS_QUANTUM,
                TrackId::CURRENT,
                &[],
                FlowAttachment::None,
            );
            if status == perfetto_everywhere_core::EmitStatus::Recorded {
                let _ =
                    self.backend
                        .counter_i64(QUEUE_DEPTH, TrackId::CURRENT, i64::from(queue_depth));
                let _ = self
                    .backend
                    .counter_f64(CPU_LOAD, TrackId::CURRENT, cpu_load);
                self.backend
                    .set_timestamp(frame.saturating_add(u64::from(quantum_frames)));
                let _ = self.backend.span_end(TrackId::CURRENT);
            }
            status as u8
        }

        pub fn install_flow(&self, frame: f64, flow: u64) -> u8 {
            self.backend.set_timestamp(frame.max(0.0) as u64);
            let attachment = FlowId::new(flow)
                .map(FlowAttachment::Terminate)
                .unwrap_or(FlowAttachment::None);
            self.backend
                .event(AUDIO, GRAPH_INSTALLED, TrackId::CURRENT, &[], attachment) as u8
        }

        pub fn available_records(&self) -> usize {
            self.backend.available_records()
        }

        pub fn drain(&mut self, maximum_bytes: usize) -> Uint8Array {
            let limit = maximum_bytes.min(self.transfer.len());
            let length = self.backend.drain_into(&mut self.transfer[..limit]);
            Uint8Array::from(&self.transfer[..length])
        }

        pub fn dropped_records(&self) -> u64 {
            self.backend.health().dropped_records
        }

        pub fn emitted_records(&self) -> u64 {
            self.backend.health().emitted_records
        }

        pub fn high_water_records(&self) -> usize {
            self.backend.health().high_water_records
        }

        pub fn callbacks(&self) -> u64 {
            self.callbacks
        }

        pub fn discontinuities(&self) -> u64 {
            self.discontinuities
        }

        pub fn finish(&self) {
            self.backend.set_enabled(false);
        }

        fn begin_callback(&mut self, frame: u64, quantum_frames: u32) {
            if self
                .expected_frame
                .get()
                .is_some_and(|expected| frame != expected)
            {
                self.discontinuities = self.discontinuities.saturating_add(1);
            }
            self.expected_frame
                .set(Some(frame.saturating_add(u64::from(quantum_frames))));
            self.callbacks = self.callbacks.saturating_add(1);
            self.backend.set_timestamp(frame);
        }
    }
}

#[cfg(target_arch = "wasm32")]
pub use wasm::AudioChunkProducer;
