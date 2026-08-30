#![doc = "`tracing-subscriber` compatibility layer for `perfetto-everywhere`."]
#![forbid(unsafe_code)]

use perfetto_everywhere_core::{
    Category, EmitStatus, Field, FieldName, FieldValue, FlowAttachment, Severity, StaticName,
    TraceBackend, Tracer, TrackId,
};
use std::sync::{Arc, Mutex};
use tracing::{
    Event, Metadata, Subscriber,
    field::{Field as TracingField, Visit},
    span::{Attributes, Id, Record as SpanRecord},
};
use tracing_subscriber::{Layer, layer::Context, registry::LookupSpan};

/// Cloneable synchronized backend handle. The mutex enables ordinary single-thread
/// WASM backends to satisfy `Layer`'s `Send + Sync` contract; this adapter is not
/// permitted on the AudioWorklet callback path.
pub struct SharedBackend<B> {
    inner: Arc<Mutex<B>>,
}

impl<B> Clone for SharedBackend<B> {
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
        }
    }
}

impl<B> SharedBackend<B> {
    pub fn new(backend: B) -> Self {
        Self {
            inner: Arc::new(Mutex::new(backend)),
        }
    }

    pub fn with<R>(&self, operation: impl FnOnce(&B) -> R) -> Option<R> {
        self.inner.lock().ok().map(|backend| operation(&backend))
    }
}

impl<B: TraceBackend> TraceBackend for SharedBackend<B> {
    fn is_enabled(&self, category: Category) -> bool {
        self.with(|backend| backend.is_enabled(category))
            .unwrap_or(false)
    }

    fn span_begin(
        &self,
        category: Category,
        name: StaticName,
        track: TrackId,
        fields: &[Field<'_>],
        flow: FlowAttachment,
    ) -> EmitStatus {
        self.with(|backend| backend.span_begin(category, name, track, fields, flow))
            .unwrap_or(EmitStatus::Unsupported)
    }

    fn span_end(&self, track: TrackId) -> EmitStatus {
        self.with(|backend| backend.span_end(track))
            .unwrap_or(EmitStatus::Unsupported)
    }

    fn event(
        &self,
        category: Category,
        name: StaticName,
        track: TrackId,
        fields: &[Field<'_>],
        flow: FlowAttachment,
    ) -> EmitStatus {
        self.with(|backend| backend.event(category, name, track, fields, flow))
            .unwrap_or(EmitStatus::Unsupported)
    }

    fn log(
        &self,
        severity: Severity,
        target: StaticName,
        message: StaticName,
        fields: &[Field<'_>],
    ) -> EmitStatus {
        self.with(|backend| backend.log(severity, target, message, fields))
            .unwrap_or(EmitStatus::Unsupported)
    }

    fn counter_i64(&self, name: StaticName, track: TrackId, value: i64) -> EmitStatus {
        self.with(|backend| backend.counter_i64(name, track, value))
            .unwrap_or(EmitStatus::Unsupported)
    }

    fn counter_f64(&self, name: StaticName, track: TrackId, value: f64) -> EmitStatus {
        self.with(|backend| backend.counter_f64(name, track, value))
            .unwrap_or(EmitStatus::Unsupported)
    }
}

#[derive(Clone, Debug)]
enum OwnedValue {
    Bool(bool),
    I64(i64),
    U64(u64),
    F64(f64),
    StaticStr(&'static str),
    String(String),
}

#[derive(Clone, Debug, Default)]
struct OwnedFields {
    values: Vec<(&'static str, OwnedValue)>,
}

impl OwnedFields {
    fn set(&mut self, name: &'static str, value: OwnedValue) {
        if let Some(existing) = self.values.iter_mut().find(|entry| entry.0 == name) {
            existing.1 = value;
        } else {
            self.values.push((name, value));
        }
    }

    fn borrowed(&self) -> Vec<Field<'_>> {
        self.values
            .iter()
            .map(|(name, value)| {
                let value = match value {
                    OwnedValue::Bool(value) => FieldValue::Bool(*value),
                    OwnedValue::I64(value) => FieldValue::I64(*value),
                    OwnedValue::U64(value) => FieldValue::U64(*value),
                    OwnedValue::F64(value) => FieldValue::F64(*value),
                    OwnedValue::StaticStr(value) => FieldValue::StaticStr(StaticName::new(value)),
                    OwnedValue::String(value) => FieldValue::Str(value),
                };
                Field::new(FieldName::new(name), value)
            })
            .collect()
    }
}

impl Visit for OwnedFields {
    fn record_bool(&mut self, field: &TracingField, value: bool) {
        self.set(field.name(), OwnedValue::Bool(value));
    }

    fn record_i64(&mut self, field: &TracingField, value: i64) {
        self.set(field.name(), OwnedValue::I64(value));
    }

    fn record_u64(&mut self, field: &TracingField, value: u64) {
        self.set(field.name(), OwnedValue::U64(value));
    }

    fn record_f64(&mut self, field: &TracingField, value: f64) {
        self.set(field.name(), OwnedValue::F64(value));
    }

    fn record_str(&mut self, field: &TracingField, value: &str) {
        self.set(field.name(), OwnedValue::String(value.to_owned()));
    }

    fn record_error(&mut self, field: &TracingField, value: &(dyn std::error::Error + 'static)) {
        self.set(field.name(), OwnedValue::String(value.to_string()));
    }

    fn record_debug(&mut self, field: &TracingField, value: &dyn core::fmt::Debug) {
        self.set(field.name(), OwnedValue::String(format!("{value:?}")));
    }
}

#[derive(Clone, Debug)]
struct SpanState {
    category: Category,
    name: StaticName,
    fields: OwnedFields,
    active_enters: usize,
}

/// A compatibility layer implemented entirely on the common facade.
pub struct PerfettoLayer<B> {
    tracer: Arc<Tracer<SharedBackend<B>>>,
}

impl<B> Clone for PerfettoLayer<B> {
    fn clone(&self) -> Self {
        Self {
            tracer: Arc::clone(&self.tracer),
        }
    }
}

impl<B: TraceBackend> PerfettoLayer<B> {
    pub fn new(backend: B) -> Self {
        Self {
            tracer: Arc::new(Tracer::new(SharedBackend::new(backend))),
        }
    }

    pub fn backend_handle(&self) -> SharedBackend<B> {
        self.tracer.backend().clone()
    }

    pub fn tracer(&self) -> &Tracer<SharedBackend<B>> {
        &self.tracer
    }
}

impl<S, B> Layer<S> for PerfettoLayer<B>
where
    S: Subscriber + for<'lookup> LookupSpan<'lookup>,
    B: TraceBackend + Send + 'static,
{
    fn on_new_span(&self, attributes: &Attributes<'_>, id: &Id, context: Context<'_, S>) {
        let metadata = attributes.metadata();
        let mut fields = OwnedFields::default();
        attributes.record(&mut fields);
        fields.set("tracing.target", OwnedValue::StaticStr(metadata.target()));
        fields.set(
            "tracing.level",
            OwnedValue::StaticStr(metadata.level().as_str()),
        );
        if let Some(span) = context.span(id) {
            span.extensions_mut().insert(SpanState {
                category: Category::new(metadata.target()),
                name: StaticName::new(metadata.name()),
                fields,
                active_enters: 0,
            });
        }
    }

    fn on_record(&self, id: &Id, values: &SpanRecord<'_>, context: Context<'_, S>) {
        if let Some(span) = context.span(id) {
            if let Some(state) = span.extensions_mut().get_mut::<SpanState>() {
                values.record(&mut state.fields);
            }
        }
    }

    fn on_enter(&self, id: &Id, context: Context<'_, S>) {
        if let Some(span) = context.span(id) {
            if let Some(state) = span.extensions_mut().get_mut::<SpanState>() {
                let fields = state.fields.borrowed();
                let status = self.tracer.backend().span_begin(
                    state.category,
                    state.name,
                    TrackId::CURRENT,
                    &fields,
                    FlowAttachment::None,
                );
                if status.was_recorded() {
                    state.active_enters += 1;
                }
            }
        }
    }

    fn on_exit(&self, id: &Id, context: Context<'_, S>) {
        if let Some(span) = context.span(id) {
            if let Some(state) = span.extensions_mut().get_mut::<SpanState>() {
                if state.active_enters > 0 {
                    state.active_enters -= 1;
                    let _ = self.tracer.backend().span_end(TrackId::CURRENT);
                }
            }
        }
    }

    fn on_event(&self, event: &Event<'_>, _context: Context<'_, S>) {
        let metadata = event.metadata();
        let mut fields = OwnedFields::default();
        event.record(&mut fields);
        fields.set(
            "tracing.level",
            OwnedValue::StaticStr(metadata.level().as_str()),
        );
        let borrowed = fields.borrowed();
        let _ = self.tracer.log(
            severity(metadata),
            StaticName::new(metadata.target()),
            StaticName::new(metadata.name()),
            &borrowed,
        );
    }
}

fn severity(metadata: &Metadata<'_>) -> Severity {
    match *metadata.level() {
        tracing::Level::TRACE => Severity::Trace,
        tracing::Level::DEBUG => Severity::Debug,
        tracing::Level::INFO => Severity::Info,
        tracing::Level::WARN => Severity::Warn,
        tracing::Level::ERROR => Severity::Error,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tracing_subscriber::prelude::*;

    #[derive(Default)]
    struct CountingBackend {
        begins: AtomicUsize,
        ends: AtomicUsize,
        logs: AtomicUsize,
    }

    impl TraceBackend for CountingBackend {
        fn is_enabled(&self, _: Category) -> bool {
            true
        }
        fn span_begin(
            &self,
            _: Category,
            _: StaticName,
            _: TrackId,
            fields: &[Field<'_>],
            _: FlowAttachment,
        ) -> EmitStatus {
            assert!(
                fields
                    .iter()
                    .any(|field| field.name.label == "tracing.target")
            );
            self.begins.fetch_add(1, Ordering::Relaxed);
            EmitStatus::Recorded
        }
        fn span_end(&self, _: TrackId) -> EmitStatus {
            self.ends.fetch_add(1, Ordering::Relaxed);
            EmitStatus::Recorded
        }
        fn event(
            &self,
            _: Category,
            _: StaticName,
            _: TrackId,
            _: &[Field<'_>],
            _: FlowAttachment,
        ) -> EmitStatus {
            EmitStatus::Recorded
        }
        fn log(
            &self,
            _: Severity,
            _: StaticName,
            _: StaticName,
            fields: &[Field<'_>],
        ) -> EmitStatus {
            assert!(fields.iter().any(|field| field.name.label == "message"));
            self.logs.fetch_add(1, Ordering::Relaxed);
            EmitStatus::Recorded
        }
        fn counter_i64(&self, _: StaticName, _: TrackId, _: i64) -> EmitStatus {
            EmitStatus::Recorded
        }
        fn counter_f64(&self, _: StaticName, _: TrackId, _: f64) -> EmitStatus {
            EmitStatus::Recorded
        }
    }

    #[test]
    fn maps_repeated_span_entries_and_typed_events() {
        let layer = PerfettoLayer::new(CountingBackend::default());
        let handle = layer.backend_handle();
        let subscriber = tracing_subscriber::registry().with(layer);
        tracing::subscriber::with_default(subscriber, || {
            let span = tracing::info_span!("compile", nodes = 12_u64, ready = true);
            {
                let _guard = span.enter();
                tracing::warn!(ratio = 0.5_f64, "queue pressure");
            }
            let _guard = span.enter();
        });
        handle.with(|backend| {
            assert_eq!(backend.begins.load(Ordering::Relaxed), 2);
            assert_eq!(backend.ends.load(Ordering::Relaxed), 2);
            assert_eq!(backend.logs.load(Ordering::Relaxed), 1);
        });
    }
}
