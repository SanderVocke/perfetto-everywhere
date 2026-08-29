#[cfg(not(feature = "disabled"))]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use perfetto_everywhere::{
        CaptureConfig, CaptureSession, Category, PlatformBackend, StaticName, Tracer,
    };
    use std::{hint::black_box, time::Instant};

    const CATEGORY: Category = Category::new("benchmark");
    const EVENT: StaticName = StaticName::new("benchmark event");
    let mode = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "disabled".to_owned());
    let iterations: u64 = std::env::args()
        .nth(2)
        .unwrap_or_else(|| "100000".to_owned())
        .parse()?;
    let tracer = Tracer::new(PlatformBackend::initialize()?);
    let session = if mode == "active" {
        Some(CaptureSession::start(CaptureConfig {
            buffer_size_kb: 16 * 1024,
            enabled_categories: vec![CATEGORY],
            ..CaptureConfig::default()
        })?)
    } else {
        None
    };

    let before = Instant::now();
    let mut checksum = 0_u64;
    for index in 0..iterations {
        checksum = checksum.wrapping_add(index.rotate_left(7));
        if mode != "disabled" {
            black_box(tracer.event(CATEGORY, EVENT, &[]));
        }
    }
    let elapsed = before.elapsed();
    let trace_bytes = if let Some(session) = session {
        session.finish()?.bytes.len()
    } else {
        0
    };
    println!(
        "mode={mode} iterations={iterations} elapsed_ns={} trace_bytes={trace_bytes} checksum={checksum}",
        elapsed.as_nanos()
    );
    Ok(())
}

#[cfg(feature = "disabled")]
fn main() {}
