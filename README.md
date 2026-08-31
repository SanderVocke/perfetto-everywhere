# perfetto-everywhere

Rust-first Perfetto tracing for native applications and browser applications
spanning Window, Dedicated Worker, and AudioWorklet realms.

```text
one Rust instrumentation API
  ├─ native: Perfetto SDK in-process session → application-owned .pftrace
  ├─ Window/Workers: bounded 48-byte record batches ┐
  └─ AudioWorklet: bounded transferable chunk pool      ├→ collector Worker
                                                     └→ standard .pftrace
optional tracing-subscriber Layer → the same API/backends
```

The repository is currently a `0.1.0` release candidate and is not yet published
to crates.io. Use the Git dependency while evaluating it:

```toml
[dependencies]
perfetto-everywhere = { git = "https://github.com/SanderVocke/perfetto-everywhere" }
```

## Instrumentation API

Instrumentation callsites are backend-independent:

```rust
use perfetto_everywhere::{
    Category, Field, FieldName, FieldValue, FlowAttachment, StaticName,
    TraceBackend, Tracer, TrackId,
};

const AUDIO: Category = Category::new("audio");
const COMPILE: StaticName = StaticName::new("compile graph");
const READY: StaticName = StaticName::new("graph ready");
const NODES: FieldName = FieldName::new("nodes");
const LOAD: StaticName = StaticName::new("cpu load");

fn instrument<B: TraceBackend>(tracer: &Tracer<B>) {
    let flow = tracer.new_flow();
    let fields = [Field::new(NODES, FieldValue::U64(12))];
    {
        let _span = tracer.span_on(
            AUDIO, COMPILE, TrackId::CURRENT, &fields,
            FlowAttachment::Step(flow),
        );
        let _ = tracer.counter_f64(LOAD, TrackId(1), 0.75);
    }
    let _ = tracer.event_on(
        AUDIO, READY, TrackId::CURRENT, &[],
        FlowAttachment::Terminate(flow),
    );
}
```

The API supports:

- lexical RAII spans and explicit tracks;
- instant events and severity/target/message logs;
- bool, i64, u64, f64, static-string, and ordinary dynamic-string fields;
- first-class i64 and f64 counter tracks;
- trace-global asynchronous flow steps/termination;
- category/backend filtering and explicit recorded/disabled/dropped/unsupported status;
- a compile-time `disabled` backend.

Static names/categories/fields receive deterministic metadata IDs. Dynamic
strings are an allocation-capable native/ordinary-browser path and are rejected
by AudioWorklet recording.

## Native quick start

No Perfetto daemon is required:

```rust,ignore
use perfetto_everywhere::{
    CaptureConfig, CaptureSession, PlatformBackend, Tracer,
};

fn capture() -> Result<(), Box<dyn std::error::Error>> {
    let tracer = Tracer::new(PlatformBackend::initialize()?);
    let session = CaptureSession::start(CaptureConfig::default())?;

    // Call shared instrumentation code with &tracer.

    let report = session.finish()?;
    report.write_file("capture.pftrace")?;
    Ok(())
}
```

`CaptureConfig` controls bounded buffer size, flush timeout, category filters,
explicit tracks, and counter descriptors. Startup may remain capture-off; only
one session is active at a time, and sequential sessions are supported. Finish
flushes/stops, consumes every SDK output chunk, and returns application-owned
bytes and lifecycle timings. See [`docs/native.md`](docs/native.md) and
[`examples/native-capture`](examples/native-capture).

Run the complete native example and SQL validation:

```bash
nix develop --command scripts/run-native-example.sh artifacts
nix develop --command scripts/validate-native.sh
```

## Browser quick start

Browser instrumentation compiles for `wasm32-unknown-unknown` and records local
source-clock ticks into bounded batches. The page transfers those batches to the
collector Worker; producers never construct protobuf.

The portable helper [`web/perfetto-browser-runtime.js`](web/perfetto-browser-runtime.js)
provides `BrowserCaptureController` for realm/metadata/calibration registration,
ordinary batch submission, AudioWorklet chunk recycling, finalization, Blob creation,
and download. The complete integration is executable documentation:

```bash
nix develop --command scripts/run-browser-multirealm.sh artifacts
nix develop --command scripts/validate-browser-multirealm.sh artifacts
```

See [`web/browser-multirealm-example.html`](web/browser-multirealm-example.html)
and [`docs/browser-runtime.md`](docs/browser-runtime.md). The example emits one
trace containing Window, two Dedicated Workers, and AudioWorklet tracks plus a
Window → Worker → AudioWorklet flow.

### Hosting

Transferable trace chunks use ordinary `ArrayBuffer` ownership and do not require
COOP/COEP or cross-origin isolation. The included server defaults remain useful
for testing optional cross-origin embedding behavior.

## AudioWorklet quick start

Configure a bounded recyclable chunk pool before constructing the worklet:

```js
const config = {
  captureId: 1,
  capacityRecords: 8192,
  chunkBytes: 8192 * 48,
  poolSize: 3,
};
const node = new AudioWorkletNode(context, "perfetto-audio", {
  processorOptions: { ...config, wasmModule, realmId: 4, clockId: 104, record: true },
});
controller.attachAudioPort(node.port);
```

The callback records into bounded realm-local storage and drains complete groups into preallocated transferable chunks. It never waits, grows producer storage, encodes protobuf, formats dynamic strings, writes files, or posts per event. Pool starvation drops complete groups and remains observable in producer health.

Audio timestamps are exact `currentFrame` values. Logical quantum spans describe
the audio sample timeline, not unavailable callback CPU-entry time. The page
periodically records `AudioContext.getOutputTimestamp()` calibrations; the
collector emits custom Perfetto clock snapshots and raw fit diagnostics. Perfetto
UI and SQL receive one converted timeline automatically.

An import-free raw Wasm module can instead use `perfetto-everywhere-raw` to
record into a preallocated linear-memory ring, set source timestamps explicitly,
and drain complete groups into caller-owned transfer storage. See
[`docs/raw-wasm.md`](docs/raw-wasm.md).

See [`docs/audio-worklet.md`](docs/audio-worklet.md). Run the short test with:

```bash
AUDIO_DURATION_MS=3000 nix develop --command scripts/run-audio-browser.sh artifacts
nix develop --command scripts/validate-audio.sh artifacts
```

The full acceptance workflow uses equal 60-second baseline/traced runs.

## Migrating existing `tracing` instrumentation

Enable the optional feature:

```toml
perfetto-everywhere = {
  git = "https://github.com/SanderVocke/perfetto-everywhere",
  features = ["tracing"]
}
```

```rust,ignore
use perfetto_everywhere::{PerfettoLayer, PlatformBackend};
use tracing_subscriber::prelude::*;

let layer = PerfettoLayer::new(PlatformBackend::initialize()?);
let subscriber = tracing_subscriber::registry().with(layer);
tracing::subscriber::with_default(subscriber, || {
    let span = tracing::info_span!("compile", nodes = 12_u64);
    let _guard = span.enter();
    tracing::warn!(ready = true, "queue pressure");
});
# Ok::<(), Box<dyn std::error::Error>>(())
```

The layer preserves repeated span enter/exit, late fields, messages, level,
target, and typed values on native and ordinary browser WASM. It allocates and
uses a mutex, so it is forbidden on the AudioWorklet callback path. Counters,
tracks, and flows remain direct facade APIs. See
[`docs/tracing-compatibility.md`](docs/tracing-compatibility.md).

## Target and feature matrix

| Target/configuration | Status | Backend |
|---|---|---|
| Linux native | Automated | `perfetto-sdk` 1.1.1 in-process |
| `wasm32-unknown-unknown` Window/Worker | Automated in Chromium | bounded ordinary producer |
| `wasm32-unknown-unknown` AudioWorklet | Automated in Chromium | bounded transferable chunks |
| Import-free raw Wasm | Unit/Wasm checks | bounded linear-memory ring |
| `disabled` | Automated native/WASM | no-op backend |
| `tracing` | Automated native/ordinary WASM | compatibility layer |
| Firefox/Safari | Not claimed | requires compatibility validation |
| Native system/ftrace | Optional, not required | future backend configuration |

The MSRV is Rust 1.85.0. Browser WASM uses wasm-bindgen 0.2.127. The current
supported automated browser is Chromium; code feature-detects required Web APIs
and fails with actionable errors rather than claiming untested support.

## Clock and loss model

Every browser realm retains its source timestamp definition. The collector owns
calibration, descriptor/ID validation, deterministic ordering, clock snapshots,
protobuf encoding, limits, span-boundary repair, health events, and export.
Trace Processor converts valid custom clocks during import, so normal `slice.ts`,
`counter.ts`, flow, and UI analysis share one timeline. Raw calibration
uncertainty/residuals and producer loss remain inspectable.

Capture completeness is subordinate to application continuity. Ordinary batches
and the AudioWorklet ring are bounded; drops, high-water occupancy, malformed
records, clock failures, and repaired span boundaries are explicit diagnostics.

## Development and validation

```bash
nix develop
scripts/check.sh
```

Important focused commands:

```bash
cargo test --workspace --all-features
cargo check -p perfetto-everywhere --target wasm32-unknown-unknown
cargo check -p perfetto-everywhere --features disabled
cargo check -p perfetto-everywhere --features tracing
scripts/validate-collector.sh artifacts
```

See [`docs/development.md`](docs/development.md), the stage verification documents,
and GitHub Actions. Large generated traces are CI/local artifacts, not permanent
source files.

## Security and privacy

Trace fields may contain user data, file paths, messages, and application state.
Treat `.pftrace` files as potentially sensitive. Use category filters, static
metadata, bounded capture durations, and application-controlled storage. The
library performs no network upload.

## License

Licensed under either Apache License 2.0 or MIT, at your option.
