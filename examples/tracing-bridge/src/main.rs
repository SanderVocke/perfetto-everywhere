#[cfg(not(feature = "disabled"))]
mod enabled {
    use perfetto_everywhere::{CaptureConfig, CaptureSession, PerfettoLayer, PlatformBackend};
    use std::{error::Error, fs, path::PathBuf, thread, time::Duration};
    use tracing_subscriber::prelude::*;

    pub fn run() -> Result<(), Box<dyn Error>> {
        let output = std::env::args_os()
            .nth(1)
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("artifacts/tracing-bridge.pftrace"));
        if let Some(parent) = output.parent() {
            fs::create_dir_all(parent)?;
        }

        let backend = PlatformBackend::initialize()?;
        let layer = PerfettoLayer::new(backend);
        let subscriber = tracing_subscriber::registry().with(layer);
        let session = CaptureSession::start(CaptureConfig::default())?;
        tracing::subscriber::with_default(subscriber, || {
            let span = tracing::info_span!(
                target: "graph",
                "compile graph through tracing",
                nodes = 123_u64,
                load = 0.625_f64,
                success = true,
                phase = tracing::field::Empty,
            );
            span.record("phase", "prepare");
            {
                let _guard = span.enter();
                tracing::info!(target: "graph", revision = -7_i64, message = "starting compile");
                thread::sleep(Duration::from_millis(1));
                tracing::warn!(target: "audio", queue_depth = 9_u64, recoverable = true, "queue pressure");
            }
            // A tracing span may be entered repeatedly; the layer emits one slice per entry.
            let _second_entry = span.enter();
        });
        let report = session.finish()?;
        report.write_file(&output)?;
        println!("wrote {} ({} bytes)", output.display(), report.bytes.len());
        Ok(())
    }
}

#[cfg(not(feature = "disabled"))]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    enabled::run()
}

#[cfg(feature = "disabled")]
fn main() {}
