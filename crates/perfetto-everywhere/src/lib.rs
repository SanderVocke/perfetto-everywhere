#![doc = "Unified Perfetto tracing for native and browser Rust applications."]
#![forbid(unsafe_code)]

pub use perfetto_everywhere_core::*;

#[cfg(all(not(feature = "disabled"), not(target_arch = "wasm32")))]
pub use perfetto_everywhere_native::{
    CaptureConfig, CaptureReport, CaptureSession, NativeBackend as PlatformBackend, NativeError,
};
#[cfg(all(not(feature = "disabled"), target_arch = "wasm32"))]
pub use perfetto_everywhere_web::{
    ClockCalibration, MetadataEntry, OrdinaryBackend, PerformanceClock, ProducerHealth,
};
#[cfg(all(not(feature = "disabled"), target_arch = "wasm32"))]
pub type PlatformBackend = OrdinaryBackend<PerformanceClock>;

/// Backend selected when instrumentation is compiled out.
#[cfg(feature = "disabled")]
pub type PlatformBackend = NoopBackend;
