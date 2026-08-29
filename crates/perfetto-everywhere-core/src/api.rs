use crate::{Category, FieldName, StaticName};
use core::{
    marker::PhantomData,
    sync::atomic::{AtomicU64, Ordering},
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum Severity {
    Trace = 1,
    Debug = 2,
    Info = 3,
    Warn = 4,
    Error = 5,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(transparent)]
pub struct FlowId(u64);

impl FlowId {
    pub const fn new(value: u64) -> Option<Self> {
        if value == 0 { None } else { Some(Self(value)) }
    }

    pub const fn get(self) -> u64 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FlowAttachment {
    None,
    Step(FlowId),
    Terminate(FlowId),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(transparent)]
pub struct TrackId(pub u64);

impl TrackId {
    pub const CURRENT: Self = Self(0);
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum FieldValue<'a> {
    Bool(bool),
    I64(i64),
    U64(u64),
    F64(f64),
    /// A metadata-backed static string, safe for real-time producers.
    StaticStr(StaticName),
    /// An ordinary-realm/native dynamic string. Real-time backends reject it.
    Str(&'a str),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Field<'a> {
    pub name: FieldName,
    pub value: FieldValue<'a>,
}

impl<'a> Field<'a> {
    pub const fn new(name: FieldName, value: FieldValue<'a>) -> Self {
        Self { name, value }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EmitStatus {
    Recorded,
    Disabled,
    Dropped,
    Unsupported,
}

impl EmitStatus {
    pub const fn was_recorded(self) -> bool {
        matches!(self, Self::Recorded)
    }
}

/// Backend contract shared by native, ordinary browser, and AudioWorklet producers.
///
/// Implementations must consume fields synchronously; borrowed values do not outlive
/// a call. A backend may reject `FieldValue::Str` with `EmitStatus::Unsupported`.
pub trait TraceBackend {
    fn is_enabled(&self, category: Category) -> bool;

    fn span_begin(
        &self,
        category: Category,
        name: StaticName,
        track: TrackId,
        fields: &[Field<'_>],
        flow: FlowAttachment,
    ) -> EmitStatus;

    fn span_end(&self, track: TrackId) -> EmitStatus;

    fn event(
        &self,
        category: Category,
        name: StaticName,
        track: TrackId,
        fields: &[Field<'_>],
        flow: FlowAttachment,
    ) -> EmitStatus;

    fn log(
        &self,
        severity: Severity,
        target: StaticName,
        message: StaticName,
        fields: &[Field<'_>],
    ) -> EmitStatus;

    fn counter_i64(&self, name: StaticName, track: TrackId, value: i64) -> EmitStatus;

    fn counter_f64(&self, name: StaticName, track: TrackId, value: f64) -> EmitStatus;
}

/// Target-independent instrumentation handle.
pub struct Tracer<B> {
    backend: B,
    next_flow: AtomicU64,
}

impl<B: TraceBackend> Tracer<B> {
    pub const fn new(backend: B) -> Self {
        Self {
            backend,
            next_flow: AtomicU64::new(1),
        }
    }

    pub fn span(
        &self,
        category: Category,
        name: StaticName,
        fields: &[Field<'_>],
    ) -> SpanGuard<'_, B> {
        self.span_on(
            category,
            name,
            TrackId::CURRENT,
            fields,
            FlowAttachment::None,
        )
    }

    pub fn span_on(
        &self,
        category: Category,
        name: StaticName,
        track: TrackId,
        fields: &[Field<'_>],
        flow: FlowAttachment,
    ) -> SpanGuard<'_, B> {
        let status = if self.backend.is_enabled(category) {
            self.backend.span_begin(category, name, track, fields, flow)
        } else {
            EmitStatus::Disabled
        };
        SpanGuard {
            backend: &self.backend,
            track,
            active: status.was_recorded(),
            begin_status: status,
            not_send: PhantomData,
        }
    }

    pub fn event(&self, category: Category, name: StaticName, fields: &[Field<'_>]) -> EmitStatus {
        self.event_on(
            category,
            name,
            TrackId::CURRENT,
            fields,
            FlowAttachment::None,
        )
    }

    pub fn event_on(
        &self,
        category: Category,
        name: StaticName,
        track: TrackId,
        fields: &[Field<'_>],
        flow: FlowAttachment,
    ) -> EmitStatus {
        if self.backend.is_enabled(category) {
            self.backend.event(category, name, track, fields, flow)
        } else {
            EmitStatus::Disabled
        }
    }

    pub fn log(
        &self,
        severity: Severity,
        target: StaticName,
        message: StaticName,
        fields: &[Field<'_>],
    ) -> EmitStatus {
        self.backend.log(severity, target, message, fields)
    }

    pub fn counter_i64(&self, name: StaticName, track: TrackId, value: i64) -> EmitStatus {
        self.backend.counter_i64(name, track, value)
    }

    pub fn counter_f64(&self, name: StaticName, track: TrackId, value: f64) -> EmitStatus {
        self.backend.counter_f64(name, track, value)
    }

    pub fn new_flow(&self) -> FlowId {
        loop {
            let id = self.next_flow.fetch_add(1, Ordering::Relaxed);
            if id != 0 {
                return FlowId(id);
            }
        }
    }

    pub const fn backend(&self) -> &B {
        &self.backend
    }
}

/// A lexical span guard. It is intentionally not `Send`: begin/end stay on the
/// same execution track unless an explicit asynchronous API models otherwise.
///
/// ```compile_fail
/// use perfetto_everywhere_core::{NoopBackend, SpanGuard};
/// fn assert_send<T: Send>() {}
/// assert_send::<SpanGuard<'static, NoopBackend>>();
/// ```
#[must_use = "dropping the guard emits the span end"]
pub struct SpanGuard<'a, B: TraceBackend> {
    backend: &'a B,
    track: TrackId,
    active: bool,
    begin_status: EmitStatus,
    not_send: PhantomData<*mut ()>,
}

impl<B: TraceBackend> SpanGuard<'_, B> {
    pub const fn status(&self) -> EmitStatus {
        self.begin_status
    }
}

impl<B: TraceBackend> Drop for SpanGuard<'_, B> {
    fn drop(&mut self) {
        if self.active {
            let _ = self.backend.span_end(self.track);
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct NoopBackend;

impl TraceBackend for NoopBackend {
    fn is_enabled(&self, _: Category) -> bool {
        false
    }

    fn span_begin(
        &self,
        _: Category,
        _: StaticName,
        _: TrackId,
        _: &[Field<'_>],
        _: FlowAttachment,
    ) -> EmitStatus {
        EmitStatus::Disabled
    }

    fn span_end(&self, _: TrackId) -> EmitStatus {
        EmitStatus::Disabled
    }

    fn event(
        &self,
        _: Category,
        _: StaticName,
        _: TrackId,
        _: &[Field<'_>],
        _: FlowAttachment,
    ) -> EmitStatus {
        EmitStatus::Disabled
    }

    fn log(&self, _: Severity, _: StaticName, _: StaticName, _: &[Field<'_>]) -> EmitStatus {
        EmitStatus::Disabled
    }

    fn counter_i64(&self, _: StaticName, _: TrackId, _: i64) -> EmitStatus {
        EmitStatus::Disabled
    }

    fn counter_f64(&self, _: StaticName, _: TrackId, _: f64) -> EmitStatus {
        EmitStatus::Disabled
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::cell::Cell;

    const CATEGORY: Category = Category::new("test");
    const SPAN: StaticName = StaticName::new("span");

    struct CountingBackend(Cell<u32>);

    impl TraceBackend for CountingBackend {
        fn is_enabled(&self, _: Category) -> bool {
            true
        }
        fn span_begin(
            &self,
            _: Category,
            _: StaticName,
            _: TrackId,
            _: &[Field<'_>],
            _: FlowAttachment,
        ) -> EmitStatus {
            self.0.set(self.0.get() + 1);
            EmitStatus::Recorded
        }
        fn span_end(&self, _: TrackId) -> EmitStatus {
            self.0.set(self.0.get() + 1);
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
        fn log(&self, _: Severity, _: StaticName, _: StaticName, _: &[Field<'_>]) -> EmitStatus {
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
    fn span_guard_closes_only_recorded_spans() {
        let tracer = Tracer::new(CountingBackend(Cell::new(0)));
        {
            let guard = tracer.span(CATEGORY, SPAN, &[]);
            assert_eq!(guard.status(), EmitStatus::Recorded);
        }
        assert_eq!(tracer.backend().0.get(), 2);

        let noop = Tracer::new(NoopBackend);
        assert_eq!(
            noop.span(CATEGORY, SPAN, &[]).status(),
            EmitStatus::Disabled
        );
    }

    #[test]
    fn flow_ids_are_nonzero_and_unique() {
        let tracer = Tracer::new(NoopBackend);
        let first = tracer.new_flow();
        let second = tracer.new_flow();
        assert_ne!(first, second);
        assert_ne!(first.0, 0);

        tracer.next_flow.store(u64::MAX, Ordering::Relaxed);
        assert_eq!(tracer.new_flow().get(), u64::MAX);
        assert_eq!(tracer.new_flow().get(), 1);
    }
}
