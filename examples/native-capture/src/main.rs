#[cfg(not(feature = "disabled"))]
mod enabled {
    use perfetto_everywhere::{
        CaptureConfig, CaptureSession, Category, EmitStatus, Field, FieldName, FieldValue,
        FlowAttachment, PlatformBackend, Severity, StaticName, TraceBackend, Tracer, TrackId,
    };
    use std::{error::Error, fs, path::PathBuf, thread, time::Duration};

    const APP: Category = Category::new("application");
    const FILTERED: Category = Category::new("filtered-out");
    const COMPILE: StaticName = StaticName::new("compile graph");
    const CACHE_MISS: StaticName = StaticName::new("cache miss");
    const GRAPH_READY: StaticName = StaticName::new("graph ready");
    const TARGET: StaticName = StaticName::new("native_example");
    const MESSAGE: StaticName = StaticName::new("queue pressure");
    const STATIC_VALUE: StaticName = StaticName::new("statically interned");
    const QUEUE: StaticName = StaticName::new("queue_depth");
    const LOAD: StaticName = StaticName::new("cpu_load");
    const ENABLED: FieldName = FieldName::new("enabled");
    const REVISION: FieldName = FieldName::new("revision");
    const NODES: FieldName = FieldName::new("nodes");
    const RATIO: FieldName = FieldName::new("ratio");
    const PHASE: FieldName = FieldName::new("phase");
    const DETAILS: FieldName = FieldName::new("details");

    fn emit_feature_set<B: TraceBackend>(tracer: &Tracer<B>) {
        let flow = tracer.new_flow();
        let fields = [
            Field::new(ENABLED, FieldValue::Bool(true)),
            Field::new(REVISION, FieldValue::I64(-7)),
            Field::new(NODES, FieldValue::U64(123)),
            Field::new(RATIO, FieldValue::F64(0.625)),
            Field::new(PHASE, FieldValue::StaticStr(STATIC_VALUE)),
            Field::new(DETAILS, FieldValue::Str("dynamic native field")),
        ];
        {
            let _outer = tracer.span_on(
                APP,
                COMPILE,
                TrackId::CURRENT,
                &fields,
                FlowAttachment::Step(flow),
            );
            {
                let _inner = tracer.span(APP, StaticName::new("nested work"), &[]);
                thread::sleep(Duration::from_millis(1));
            }
            let _ = tracer.event(APP, CACHE_MISS, &fields[..2]);
            let _ = tracer.log(Severity::Warn, TARGET, MESSAGE, &fields[2..]);
            let _ = tracer.counter_i64(QUEUE, TrackId(1), 2);
            let _ = tracer.counter_f64(LOAD, TrackId(2), 0.25);
            let _ = tracer.counter_i64(QUEUE, TrackId(1), 9);
            let _ = tracer.counter_f64(LOAD, TrackId(2), 0.91);
        }
        thread::spawn(|| {
            let worker = Tracer::new(PlatformBackend::initialize().expect("worker backend"));
            let _span = worker.span_on(
                APP,
                StaticName::new("worker task"),
                TrackId(3),
                &[],
                FlowAttachment::None,
            );
        })
        .join()
        .expect("worker instrumentation");
        let _ = tracer.event_on(
            APP,
            GRAPH_READY,
            TrackId::CURRENT,
            &[],
            FlowAttachment::Terminate(flow),
        );
    }

    fn run_capture(path: PathBuf, buffer_size_kb: u32) -> Result<(), Box<dyn Error>> {
        let tracer = Tracer::new(PlatformBackend::initialize()?);
        let session = CaptureSession::start(CaptureConfig {
            buffer_size_kb,
            enabled_categories: vec![APP],
            tracks: vec![TrackId(1), TrackId(2), TrackId(3)],
            counter_tracks: vec![(QUEUE, TrackId(1)), (LOAD, TrackId(2))],
            ..CaptureConfig::default()
        })?;
        assert_eq!(
            tracer.event(FILTERED, StaticName::new("must not appear"), &[]),
            EmitStatus::Disabled
        );
        emit_feature_set(&tracer);
        let report = session.finish()?;
        report.write_file(&path)?;
        println!(
            "wrote {} ({} bytes, buffer={} KiB, flush={:?}, stop={:?}, read={:?})",
            path.display(),
            report.bytes.len(),
            report.configured_buffer_kb,
            report.flush_elapsed,
            report.stop_elapsed,
            report.read_elapsed
        );
        Ok(())
    }

    fn run_overflow(path: PathBuf) -> Result<(), Box<dyn Error>> {
        let tracer = Tracer::new(PlatformBackend::initialize()?);
        let session = CaptureSession::start(CaptureConfig {
            buffer_size_kb: 8,
            enabled_categories: vec![APP],
            ..CaptureConfig::default()
        })?;
        for index in 0..50_000_u64 {
            let field = [Field::new(NODES, FieldValue::U64(index))];
            let _ = tracer.event(APP, CACHE_MISS, &field);
        }
        let report = session.finish()?;
        report.write_file(&path)?;
        println!(
            "wrote overflow trace {} ({} bytes)",
            path.display(),
            report.bytes.len()
        );
        Ok(())
    }

    pub fn run() -> Result<(), Box<dyn Error>> {
        let output = std::env::args_os()
            .nth(1)
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("artifacts"));
        fs::create_dir_all(&output)?;

        let tracer = Tracer::new(PlatformBackend::initialize()?);
        assert_eq!(
            tracer.event(APP, StaticName::new("before capture"), &[]),
            EmitStatus::Disabled
        );

        run_capture(output.join("native-first.pftrace"), 1024)?;
        run_capture(output.join("native-second.pftrace"), 256)?;
        run_overflow(output.join("native-overflow.pftrace"))?;
        Ok(())
    }
}

#[cfg(not(feature = "disabled"))]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    enabled::run()
}

#[cfg(feature = "disabled")]
fn main() {
    let tracer = perfetto_everywhere::Tracer::new(perfetto_everywhere::NoopBackend);
    let _ = tracer.event(
        perfetto_everywhere::Category::new("disabled"),
        perfetto_everywhere::StaticName::new("compiled out"),
        &[],
    );
}
