#![doc = "Unified Perfetto tracing for native and browser Rust applications."]
#![forbid(unsafe_code)]

pub use perfetto_everywhere_core::*;

#[cfg(all(not(feature = "disabled"), not(target_arch = "wasm32")))]
pub use perfetto_everywhere_native as platform;
#[cfg(all(not(feature = "disabled"), target_arch = "wasm32"))]
pub use perfetto_everywhere_web as platform;

/// Backend selected when instrumentation is compiled out.
#[cfg(feature = "disabled")]
pub type PlatformBackend = NoopBackend;
