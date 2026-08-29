#![doc = "Unified Perfetto tracing for native and browser Rust applications."]
#![forbid(unsafe_code)]

pub use perfetto_everywhere_core as core;

#[cfg(not(target_arch = "wasm32"))]
pub use perfetto_everywhere_native as platform;
#[cfg(target_arch = "wasm32")]
pub use perfetto_everywhere_web as platform;
