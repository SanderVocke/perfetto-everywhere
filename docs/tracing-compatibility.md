# `tracing` compatibility

Enable the facade feature and attach `PerfettoLayer` to a subscriber:

```toml
perfetto-everywhere = { version = "0.1", features = ["tracing"] }
tracing = "0.1"
tracing-subscriber = "0.3"
```

```rust
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

The layer is implemented on `TraceBackend`; it does not bypass the common API.
It maps every span enter/exit to a Perfetto slice, handles repeated entries and
late `Span::record`, and maps events to structured logs. Bool, i64, u64, f64,
string/debug/error values, event messages, levels, and targets are retained.
Disabled/filtered span begins do not emit unmatched ends.

`SharedBackend` wraps the backend in an `Arc<Mutex<_>>` to satisfy
`tracing-subscriber`'s cross-thread layer contract and exposes `with` so browser
code can flush/take ordinary batches. This allocation/formatting/locking adapter
is supported for native and ordinary browser realms only. It is expressly
forbidden on the AudioWorklet callback path.

The adapter does not invent conventions for numeric counter plots, explicit
tracks, custom timestamps, or cross-realm flows. Use the direct facade for those
features alongside `tracing` migration code.

See `examples/tracing-bridge`, `scripts/validate-tracing.sh`, and the ordinary
browser test's `produce_tracing` export.
