#![cfg(target_arch = "wasm32")]

use perfetto_everywhere_core::{
    Category, Field, FieldName, FieldValue, FlowAttachment, FlowId, Severity, StaticName, Tracer,
    TrackId,
};
use perfetto_everywhere_tracing::PerfettoLayer;
use perfetto_everywhere_web::{OrdinaryBackend, PerformanceClock};
use tracing_subscriber::prelude::*;
use wasm_bindgen::prelude::*;

const APP: Category = Category::new("browser-test");
const PAGE_TASK: StaticName = StaticName::new("request graph rebuild");
const WORKER_TASK: StaticName = StaticName::new("compile graph");
const OTHER_TASK: StaticName = StaticName::new("ordinary task");
const HEARTBEAT: StaticName = StaticName::new("heartbeat");
const COUNTER: StaticName = StaticName::new("worker counter");
const TARGET: StaticName = StaticName::new("ordinary producer");
const MESSAGE: StaticName = StaticName::new("worker ready");
const REALM_FIELD: FieldName = FieldName::new("realm");

#[wasm_bindgen]
pub fn produce_tracing(realm: u32) -> Result<Vec<u8>, JsValue> {
    let backend = OrdinaryBackend::new(realm, realm + 100, PerformanceClock, 64, &[])
        .map_err(JsValue::from_str)?;
    let layer = PerfettoLayer::new(backend);
    let handle = layer.backend_handle();
    let subscriber = tracing_subscriber::registry().with(layer);
    tracing::subscriber::with_default(subscriber, || {
        let span = tracing::info_span!(target: "browser_adapter", "adapter span", nodes = 7_u64);
        let _guard = span.enter();
        tracing::warn!(target: "browser_adapter", ready = true, message = "adapter event");
    });
    handle
        .with(|backend| backend.flush_and_take_batch())
        .flatten()
        .ok_or_else(|| JsValue::from_str("tracing adapter returned no batch"))
}

#[wasm_bindgen]
pub fn produce(realm: u32, flow: u64) -> Result<Vec<u8>, JsValue> {
    let backend = OrdinaryBackend::new(realm, realm + 100, PerformanceClock, 64, &[APP])
        .map_err(JsValue::from_str)?;
    let tracer = Tracer::new(backend);
    let fields = [Field::new(REALM_FIELD, FieldValue::U64(u64::from(realm)))];
    let flow = FlowId::new(flow).map_or(FlowAttachment::None, FlowAttachment::Step);
    let task = match realm {
        1 => PAGE_TASK,
        2 => WORKER_TASK,
        _ => OTHER_TASK,
    };
    {
        let _span = tracer.span_on(APP, task, TrackId::CURRENT, &fields, flow);
        let _ = tracer.event(APP, HEARTBEAT, &fields);
        let _ = tracer.counter_i64(COUNTER, TrackId(1), i64::from(realm));
        let _ = tracer.log(Severity::Info, TARGET, MESSAGE, &fields);
    }
    tracer
        .backend()
        .flush_and_take_batch()
        .ok_or_else(|| JsValue::from_str("producer returned no batch"))
}
