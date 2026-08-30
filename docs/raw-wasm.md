# Import-free raw WebAssembly producer

`perfetto-everywhere-raw` is the producer backend for WebAssembly modules that
must not import browser APIs. It depends only on `perfetto-everywhere-core` and
uses a fixed-capacity ring allocated during construction.

```rust
use perfetto_everywhere_core::{Category, StaticName, Tracer};
use perfetto_everywhere_raw::RawRingBackend;

const AUDIO: Category = Category::new("audio");
const CALLBACK: StaticName = StaticName::new("callback");

let backend = RawRingBackend::new(4, 104, 1024, &[AUDIO])?;
backend.set_timestamp(128);
let tracer = Tracer::new(backend);
let _ = tracer.event(AUDIO, CALLBACK, &[]);

let mut transfer = [0_u8; 48 * 8];
let initialized = tracer.backend().drain_into(&mut transfer);
let complete_groups = &transfer[..initialized];
# Ok::<(), &'static str>(())
```

The owner sets source-clock ticks before instrumented work, then drains complete
record groups into preallocated transfer storage. A group that cannot fit in the
ring is dropped atomically and counted. A destination too small for the next
complete group drains nothing from that group. Dynamic strings are unsupported;
register static metadata on the collector/control path.

Construction may allocate the ring and category list. `set_timestamp`, event
recording, health reads, and `drain_into` do not grow storage, format strings,
call JavaScript, wait, lock, or encode protobuf. The embedding module remains
responsible for exposing its own raw ABI and for proving that its final linked
Wasm artifact has no imports.
