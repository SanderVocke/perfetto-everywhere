use perfetto_everywhere_collector::{Collector, CollectorConfig, RealmDescriptor};
use perfetto_everywhere_core::{
    Category, Field, FieldName, FieldValue, FlowAttachment, FlowId, StaticName, Tracer, TrackId,
};
use perfetto_everywhere_web::{ClockCalibration, OrdinaryBackend, SourceClock};
use std::{cell::Cell, error::Error, fs, path::PathBuf, rc::Rc};

#[derive(Clone)]
struct TestClock(Rc<Cell<u64>>);
impl SourceClock for TestClock {
    fn now_ticks(&self) -> Option<u64> {
        Some(self.0.get())
    }
}

const APP: Category = Category::new("collector-smoke");
const TASK: StaticName = StaticName::new("clocked task");
const EVENT: StaticName = StaticName::new("clocked event");
const COUNTER: StaticName = StaticName::new("clocked counter");
const REALM: FieldName = FieldName::new("realm");

fn realm_batch(
    realm: u32,
    timestamp: u64,
    flow: FlowAttachment,
) -> Result<(Vec<u8>, Vec<perfetto_everywhere_web::MetadataEntry>), Box<dyn Error>> {
    let clock = TestClock(Rc::new(Cell::new(timestamp)));
    let backend = OrdinaryBackend::new(realm, realm + 100, clock.clone(), 32, &[APP])?;
    let tracer = Tracer::new(backend);
    let fields = [Field::new(REALM, FieldValue::U64(u64::from(realm)))];
    {
        let _span = tracer.span_on(APP, TASK, TrackId::CURRENT, &fields, flow);
        clock.0.set(timestamp + 100);
        let _ = tracer.event(APP, EVENT, &fields);
        let _ = tracer.counter_f64(COUNTER, TrackId(1), f64::from(realm) / 10.0);
        // End after the second clock snapshot to exercise a mapping-segment boundary.
        clock.0.set(timestamp + 2_000);
    }
    Ok((
        tracer.backend().flush_and_take_batch().unwrap(),
        tracer.backend().take_metadata(),
    ))
}

fn main() -> Result<(), Box<dyn Error>> {
    let output = std::env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("artifacts/collector-clock.pftrace"));
    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent)?;
    }
    let flow = FlowId::new(42).unwrap();
    let (page, page_metadata) = realm_batch(1, 1_500, FlowAttachment::Step(flow))?;
    let (worker, worker_metadata) = realm_batch(2, 2_500, FlowAttachment::Terminate(flow))?;

    let mut collector = Collector::new(CollectorConfig::default());
    collector.register_realm(RealmDescriptor {
        id: 1,
        label: "page".to_owned(),
        ticks_per_second: 1_000_000_000,
    })?;
    collector.register_realm(RealmDescriptor {
        id: 2,
        label: "worker".to_owned(),
        ticks_per_second: 1_000_000_000,
    })?;
    collector.register_metadata_all(page_metadata)?;
    collector.register_metadata_all(worker_metadata)?;
    for sample in [
        ClockCalibration {
            realm_id: 1,
            clock_id: 101,
            source_ticks: 1_000,
            reference_time_ns: 1_700_000_000_000_000_000,
            uncertainty_ns: 20,
        },
        ClockCalibration {
            realm_id: 1,
            clock_id: 101,
            source_ticks: 3_000,
            reference_time_ns: 1_700_000_000_000_002_001,
            uncertainty_ns: 30,
        },
        ClockCalibration {
            realm_id: 2,
            clock_id: 102,
            source_ticks: 2_000,
            reference_time_ns: 1_700_000_000_000_000_500,
            uncertainty_ns: 25,
        },
        ClockCalibration {
            realm_id: 2,
            clock_id: 102,
            source_ticks: 4_000,
            reference_time_ns: 1_700_000_000_000_002_500,
            uncertainty_ns: 35,
        },
    ] {
        collector.add_calibration(sample)?;
    }
    collector.ingest_batch(&worker)?;
    collector.ingest_batch(&page)?;
    let bytes = collector.finish()?;
    fs::write(&output, &bytes)?;
    println!("wrote {} ({} bytes)", output.display(), bytes.len());
    Ok(())
}
