#![doc = "Daemon-free native Perfetto capture backend for `perfetto-everywhere`."]
#![forbid(unsafe_code)]

use perfetto_everywhere_core::{
    Category, EmitStatus, Field, FieldValue, FlowAttachment, Severity, StaticName, TraceBackend,
    TrackId,
};
use perfetto_sdk::{
    heap_buffer::HeapBuffer,
    pb_msg::{PbMsg, PbMsgWriter},
    producer::{Backends, Producer, ProducerInitArgsBuilder},
    protos::config::{
        data_source_config::DataSourceConfig,
        trace_config::{TraceConfig, TraceConfigBufferConfig, TraceConfigDataSource},
        track_event::track_event_config::TrackEventConfig,
    },
    tracing_session::TracingSession,
    track_event,
    track_event::{
        EventContext, TrackEvent, TrackEventCounter, TrackEventDebugArg, TrackEventFlow,
        TrackEventTrack, TrackEventType,
    },
    track_event_categories, track_event_counter, track_event_end,
};
use std::{
    cell::RefCell,
    collections::{BTreeMap, BTreeSet},
    error::Error,
    ffi::CString,
    fmt, fs,
    io::Write,
    path::Path,
    sync::{
        Arc, Mutex, OnceLock, RwLock,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

track_event_categories! {
    pub mod native_categories {
        ("perfetto_everywhere", "perfetto-everywhere application events", []),
    }
}
use native_categories as perfetto_te_ns;

static INITIALIZED: OnceLock<Result<(), String>> = OnceLock::new();
static CAPTURE_ACTIVE: AtomicBool = AtomicBool::new(false);
static CATEGORY_FILTER: OnceLock<RwLock<Option<BTreeSet<u32>>>> = OnceLock::new();
static TRACKS: OnceLock<Mutex<TrackRegistry>> = OnceLock::new();

thread_local! {
    static EVENT_NAMES: RefCell<BTreeMap<(u32, &'static str), CString>> =
        const { RefCell::new(BTreeMap::new()) };
}

#[derive(Default)]
struct TrackRegistry {
    named: BTreeMap<u64, Arc<TrackEventTrack>>,
    counters: BTreeMap<(u64, u32), Arc<TrackEventTrack>>,
}

fn tracks() -> &'static Mutex<TrackRegistry> {
    TRACKS.get_or_init(|| Mutex::new(TrackRegistry::default()))
}

fn explicit_track(id: TrackId) -> Option<Arc<TrackEventTrack>> {
    let mut registry = tracks().lock().ok()?;
    if let Some(track) = registry.named.get(&id.0) {
        return Some(Arc::clone(track));
    }
    let track = Arc::new(
        TrackEventTrack::register_named_track(
            "explicit track",
            id.0,
            TrackEventTrack::process_track_uuid(),
        )
        .ok()?,
    );
    registry.named.insert(id.0, Arc::clone(&track));
    Some(track)
}

fn counter_track(name: StaticName, parent: TrackId) -> Option<Arc<TrackEventTrack>> {
    let key = (parent.0, name.id.0);
    let mut registry = tracks().lock().ok()?;
    if let Some(track) = registry.counters.get(&key) {
        return Some(Arc::clone(track));
    }
    let parent_uuid = TrackEventTrack::process_track_uuid();
    let track = if parent == TrackId::CURRENT {
        TrackEventTrack::register_counter_track(name.label, parent_uuid).ok()?
    } else {
        let display_name = format!("{} [track {}]", name.label, parent.0);
        TrackEventTrack::register_counter_track_with_dynamic_name(&display_name, parent_uuid)
            .ok()?
    };
    let track = Arc::new(track);
    registry.counters.insert(key, Arc::clone(&track));
    Some(track)
}

fn category_filter() -> &'static RwLock<Option<BTreeSet<u32>>> {
    CATEGORY_FILTER.get_or_init(|| RwLock::new(None))
}

#[derive(Debug)]
pub enum NativeError {
    Initialization(String),
    Session(String),
    CaptureAlreadyActive,
    Io(std::io::Error),
    Poisoned(&'static str),
}

impl fmt::Display for NativeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Initialization(message) => {
                write!(formatter, "Perfetto initialization failed: {message}")
            }
            Self::Session(message) => write!(formatter, "Perfetto session failed: {message}"),
            Self::CaptureAlreadyActive => write!(formatter, "a native capture is already active"),
            Self::Io(error) => write!(formatter, "trace output failed: {error}"),
            Self::Poisoned(state) => write!(formatter, "native tracing {state} lock is poisoned"),
        }
    }
}

impl Error for NativeError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            _ => None,
        }
    }
}

impl From<std::io::Error> for NativeError {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value)
    }
}

#[derive(Clone, Debug)]
pub struct CaptureConfig {
    pub buffer_size_kb: u32,
    pub flush_timeout: Duration,
    /// Empty means all facade categories.
    pub enabled_categories: Vec<Category>,
    /// Explicit tracks registered before capture starts.
    pub tracks: Vec<TrackId>,
    /// Counter tracks registered before capture starts.
    pub counter_tracks: Vec<(StaticName, TrackId)>,
}

impl Default for CaptureConfig {
    fn default() -> Self {
        Self {
            buffer_size_kb: 4096,
            flush_timeout: Duration::from_secs(5),
            enabled_categories: Vec::new(),
            tracks: Vec::new(),
            counter_tracks: Vec::new(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct CaptureReport {
    pub bytes: Vec<u8>,
    pub configured_buffer_kb: u32,
    pub flush_elapsed: Duration,
    pub stop_elapsed: Duration,
    pub read_elapsed: Duration,
}

impl CaptureReport {
    pub fn write_to(&self, mut writer: impl Write) -> Result<(), NativeError> {
        writer.write_all(&self.bytes)?;
        writer.flush()?;
        Ok(())
    }

    pub fn write_file(&self, path: impl AsRef<Path>) -> Result<(), NativeError> {
        fs::write(path, &self.bytes)?;
        Ok(())
    }
}

/// Application-owned in-process capture. Concurrent sessions are rejected;
/// sequential sessions are supported.
pub struct CaptureSession {
    session: Option<TracingSession>,
    config: CaptureConfig,
    finished: bool,
}

impl CaptureSession {
    pub fn start(config: CaptureConfig) -> Result<Self, NativeError> {
        NativeBackend::initialize()?;
        if CAPTURE_ACTIVE
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return Err(NativeError::CaptureAlreadyActive);
        }

        let start_result = (|| {
            let selected = if config.enabled_categories.is_empty() {
                None
            } else {
                Some(
                    config
                        .enabled_categories
                        .iter()
                        .map(|category| category.id.0)
                        .collect(),
                )
            };
            *category_filter()
                .write()
                .map_err(|_| NativeError::Poisoned("category filter"))? = selected;

            for track in &config.tracks {
                explicit_track(*track).ok_or_else(|| {
                    NativeError::Initialization(format!("failed to register track {}", track.0))
                })?;
            }
            for (name, track) in &config.counter_tracks {
                counter_track(*name, *track).ok_or_else(|| {
                    NativeError::Initialization(format!(
                        "failed to register counter track {}",
                        name.label
                    ))
                })?;
            }

            let mut session = TracingSession::in_process()
                .map_err(|error| NativeError::Session(error.to_string()))?;
            session.setup(&trace_config(config.buffer_size_kb));
            session.start_blocking();
            Ok(Self {
                session: Some(session),
                config,
                finished: false,
            })
        })();

        if start_result.is_err() {
            CAPTURE_ACTIVE.store(false, Ordering::Release);
        }
        start_result
    }

    pub fn flush(&mut self) -> Result<Duration, NativeError> {
        let before = Instant::now();
        self.session
            .as_mut()
            .ok_or_else(|| NativeError::Session("capture has already finished".to_owned()))?
            .flush_blocking(self.config.flush_timeout);
        Ok(before.elapsed())
    }

    pub fn finish(mut self) -> Result<CaptureReport, NativeError> {
        let flush_elapsed = self.flush()?;
        let mut session = self
            .session
            .take()
            .ok_or_else(|| NativeError::Session("capture has already finished".to_owned()))?;
        let before_stop = Instant::now();
        session.stop_blocking();
        let stop_elapsed = before_stop.elapsed();

        let chunks = Arc::new(Mutex::new(Vec::new()));
        let callback_chunks = Arc::clone(&chunks);
        let before_read = Instant::now();
        session.read_trace_blocking(move |data, _has_more| {
            callback_chunks
                .lock()
                .expect("trace callback byte lock")
                .extend_from_slice(data);
        });
        let read_elapsed = before_read.elapsed();
        let bytes = Arc::try_unwrap(chunks)
            .map_err(|_| NativeError::Session("trace callback retained byte buffer".to_owned()))?
            .into_inner()
            .map_err(|_| NativeError::Poisoned("trace bytes"))?;
        self.finished = true;
        CAPTURE_ACTIVE.store(false, Ordering::Release);
        *category_filter()
            .write()
            .map_err(|_| NativeError::Poisoned("category filter"))? = None;
        Ok(CaptureReport {
            bytes,
            configured_buffer_kb: self.config.buffer_size_kb,
            flush_elapsed,
            stop_elapsed,
            read_elapsed,
        })
    }
}

impl Drop for CaptureSession {
    fn drop(&mut self) {
        if !self.finished {
            if let Some(session) = self.session.as_mut() {
                session.stop_blocking();
            }
            CAPTURE_ACTIVE.store(false, Ordering::Release);
            if let Ok(mut filter) = category_filter().write() {
                *filter = None;
            }
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct NativeBackend;

impl NativeBackend {
    pub fn initialize() -> Result<Self, NativeError> {
        let result = INITIALIZED.get_or_init(|| {
            Producer::init(
                ProducerInitArgsBuilder::new()
                    .backends(Backends::IN_PROCESS)
                    .build(),
            );
            TrackEvent::init();
            native_categories::register().map_err(|error| error.to_string())
        });
        result
            .as_ref()
            .map(|_| Self)
            .map_err(|message| NativeError::Initialization(message.clone()))
    }

    fn category_selected(category: Category) -> bool {
        if !CAPTURE_ACTIVE.load(Ordering::Acquire) {
            return false;
        }
        category_filter()
            .read()
            .map(|filter| {
                filter
                    .as_ref()
                    .is_none_or(|selected| selected.contains(&category.id.0))
            })
            .unwrap_or(false)
    }
}

fn trace_config(buffer_size_kb: u32) -> Vec<u8> {
    let writer = PbMsgWriter::new();
    let heap = HeapBuffer::new(writer.stream_writer());
    let mut message = PbMsg::new(&writer).expect("trace config message");
    {
        let mut config = TraceConfig { msg: &mut message };
        config.set_buffers(|buffer: &mut TraceConfigBufferConfig| {
            buffer.set_size_kb(buffer_size_kb.max(4));
        });
        config.set_data_sources(|source: &mut TraceConfigDataSource| {
            source.set_config(|data: &mut DataSourceConfig| {
                data.set_name("track_event");
                data.set_track_event_config(|track: &mut TrackEventConfig| {
                    track.set_enabled_categories("perfetto_everywhere");
                });
            });
        });
    }
    message.finalize();
    let size = writer.stream_writer().get_written_size();
    let mut bytes = vec![0; size];
    heap.copy_into(&mut bytes);
    bytes
}

fn add_fields(context: &mut EventContext, fields: &[Field<'_>]) {
    for field in fields {
        let argument = match field.value {
            FieldValue::Bool(value) => TrackEventDebugArg::Bool(value),
            FieldValue::I64(value) => TrackEventDebugArg::Int64(value),
            FieldValue::U64(value) => TrackEventDebugArg::Uint64(value),
            FieldValue::F64(value) => TrackEventDebugArg::Double(value),
            FieldValue::StaticStr(value) => TrackEventDebugArg::String(value.label),
            FieldValue::Str(value) => TrackEventDebugArg::String(value),
        };
        context.add_debug_arg(field.name.label, argument);
    }
}

fn add_common(
    context: &mut EventContext,
    category: Category,
    track: TrackId,
    fields: &[Field<'_>],
    flow: FlowAttachment,
) {
    context.add_debug_arg("category", TrackEventDebugArg::String(category.label));
    add_fields(context, fields);
    let registered_track = if track == TrackId::CURRENT {
        None
    } else {
        explicit_track(track)
    };
    if let Some(registered_track) = registered_track.as_deref() {
        context.set_track(registered_track);
    }
    match flow {
        FlowAttachment::None => {}
        FlowAttachment::Step(id) => {
            context.set_flow(&TrackEventFlow::process_scoped_flow(id.get()));
        }
        FlowAttachment::Terminate(id) => {
            context.set_terminating_flow(&TrackEventFlow::process_scoped_flow(id.get()));
        }
    }
}

fn with_event_name<R>(name: StaticName, operation: impl FnOnce(&CString) -> R) -> R {
    EVENT_NAMES.with(|names| {
        let mut names = names.borrow_mut();
        let name = names.entry((name.id.0, name.label)).or_insert_with(|| {
            CString::new(name.label).unwrap_or_else(|_| CString::new("invalid name").unwrap())
        });
        operation(name)
    })
}

fn emit_named(
    event_type: fn(*const std::os::raw::c_char) -> TrackEventType,
    category: Category,
    name: StaticName,
    track: TrackId,
    fields: &[Field<'_>],
    flow: FlowAttachment,
) {
    with_event_name(name, |c_name| {
        track_event!(
            "perfetto_everywhere",
            event_type(c_name.as_ptr()),
            |context: &mut EventContext| add_common(context, category, track, fields, flow)
        );
    });
}

impl TraceBackend for NativeBackend {
    fn is_enabled(&self, category: Category) -> bool {
        Self::category_selected(category)
    }

    fn span_begin(
        &self,
        category: Category,
        name: StaticName,
        track: TrackId,
        fields: &[Field<'_>],
        flow: FlowAttachment,
    ) -> EmitStatus {
        if !Self::category_selected(category) {
            return EmitStatus::Disabled;
        }
        emit_named(
            TrackEventType::SliceBegin,
            category,
            name,
            track,
            fields,
            flow,
        );
        EmitStatus::Recorded
    }

    fn span_end(&self, track: TrackId) -> EmitStatus {
        if !CAPTURE_ACTIVE.load(Ordering::Acquire) {
            return EmitStatus::Disabled;
        }
        track_event_end!("perfetto_everywhere", |context: &mut EventContext| {
            let registered_track = if track == TrackId::CURRENT {
                None
            } else {
                explicit_track(track)
            };
            if let Some(registered_track) = registered_track.as_deref() {
                context.set_track(registered_track);
            }
        });
        EmitStatus::Recorded
    }

    fn event(
        &self,
        category: Category,
        name: StaticName,
        track: TrackId,
        fields: &[Field<'_>],
        flow: FlowAttachment,
    ) -> EmitStatus {
        if !Self::category_selected(category) {
            return EmitStatus::Disabled;
        }
        emit_named(TrackEventType::Instant, category, name, track, fields, flow);
        EmitStatus::Recorded
    }

    fn log(
        &self,
        severity: Severity,
        target: StaticName,
        message: StaticName,
        fields: &[Field<'_>],
    ) -> EmitStatus {
        if !CAPTURE_ACTIVE.load(Ordering::Acquire) {
            return EmitStatus::Disabled;
        }
        with_event_name(message, |name| {
            track_event!(
                "perfetto_everywhere",
                TrackEventType::Instant(name.as_ptr()),
                |context: &mut EventContext| {
                    context
                        .add_debug_arg("severity", TrackEventDebugArg::Uint64(severity as u64))
                        .add_debug_arg("target", TrackEventDebugArg::String(target.label))
                        .add_debug_arg("message", TrackEventDebugArg::String(message.label));
                    add_fields(context, fields);
                }
            );
        });
        EmitStatus::Recorded
    }

    fn counter_i64(&self, name: StaticName, track: TrackId, value: i64) -> EmitStatus {
        if !CAPTURE_ACTIVE.load(Ordering::Acquire) {
            return EmitStatus::Disabled;
        }
        let Some(counter_track) = counter_track(name, track) else {
            return EmitStatus::Unsupported;
        };
        track_event_counter!("perfetto_everywhere", |context: &mut EventContext| {
            context
                .set_track(&counter_track)
                .set_counter(TrackEventCounter::Int64(value));
        });
        EmitStatus::Recorded
    }

    fn counter_f64(&self, name: StaticName, track: TrackId, value: f64) -> EmitStatus {
        if !CAPTURE_ACTIVE.load(Ordering::Acquire) {
            return EmitStatus::Disabled;
        }
        let Some(counter_track) = counter_track(name, track) else {
            return EmitStatus::Unsupported;
        };
        track_event_counter!("perfetto_everywhere", |context: &mut EventContext| {
            context
                .set_track(&counter_track)
                .set_counter(TrackEventCounter::Double(value));
        });
        EmitStatus::Recorded
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_has_bytes_and_clamps_tiny_buffers() {
        assert!(!trace_config(1).is_empty());
    }

    #[test]
    fn report_propagates_writer_failures() {
        struct BrokenWriter;
        impl Write for BrokenWriter {
            fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
                Err(std::io::Error::other("expected failure"))
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }
        let report = CaptureReport {
            bytes: vec![1, 2, 3],
            configured_buffer_kb: 4,
            flush_elapsed: Duration::ZERO,
            stop_elapsed: Duration::ZERO,
            read_elapsed: Duration::ZERO,
        };
        assert!(matches!(
            report.write_to(BrokenWriter),
            Err(NativeError::Io(_))
        ));
    }

    #[test]
    fn capture_rejects_concurrent_sessions() {
        let first = CaptureSession::start(CaptureConfig::default()).unwrap();
        assert!(matches!(
            CaptureSession::start(CaptureConfig::default()),
            Err(NativeError::CaptureAlreadyActive)
        ));
        drop(first);
    }
}
