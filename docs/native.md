# Native capture

The native backend pins `perfetto-sdk` 1.1.1 and uses its in-process backend;
no `traced` daemon is required. Initialization is process-global and idempotent.
Only one application-owned `CaptureSession` is accepted at a time, while any
number of sessions may run sequentially.

```rust
use perfetto_everywhere::{CaptureConfig, CaptureSession, PlatformBackend, Tracer};

let tracer = Tracer::new(PlatformBackend::initialize()?);
let session = CaptureSession::start(CaptureConfig::default())?;
// Instrumented application work using `tracer`.
let report = session.finish()?;
report.write_file("capture.pftrace")?;
# Ok::<(), Box<dyn std::error::Error>>(())
```

`CaptureConfig` controls the bounded Perfetto buffer, category allow-list,
explicit tracks, counter tracks, and flush timeout. Track/counter descriptors
are registered before capture so Trace Processor can resolve their hierarchy.
Events before/after a session return `EmitStatus::Disabled`. Concurrent capture,
I/O failures, poisoned state, and SDK/session failures are reported as
`NativeError`.

`finish` flushes, stops, consumes every SDK read callback chunk, and returns
application-owned bytes plus flush/stop/read timings. Dropping an unfinished
session stops it and clears global capture/filter state. A deliberately tiny
8 KiB example confirms overwrite behavior through Perfetto's
`traced_buf_bytes_overwritten`, chunk-overwrite, and wrap-count statistics; the
application thread does not wait for buffer space.

See `examples/native-capture`, `tests/sql/native.sql`, and
`scripts/validate-native.sh`.
