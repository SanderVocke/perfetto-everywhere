# AudioWorklet producer

## Integration and timeline semantics

`AudioRingBackend` implements the common static-metadata facade over a
single-producer/single-consumer SharedArrayBuffer ring. Rust AudioWorklet code
may use it directly. `AudioRingProducer` is a small wasm-bindgen helper for the
provided JavaScript bootstrap.

The producer records exact `currentFrame` values. `record_quantum` atomically
reserves four records and emits a logical interval from `frame` to
`frame + quantum_frames` plus integer queue-depth and floating CPU-load samples.
That interval identifies the quantum on the audio timeline; it is not a direct
measurement of callback CPU duration.

The page periodically calls `AudioContext.getOutputTimestamp()` outside the
callback and sends `(context frame, performance epoch, uncertainty)` samples to
the collector. The collector emits sequence-scoped custom Perfetto clock
snapshots. Within a continuous segment it applies a robust median offset so a
noisy observation cannot introduce timestamp steps into audio spans; raw
observations and fit residuals remain visible as `clock calibration` events.

## Ring header

The first 64 bytes are sixteen atomic `i32` words:

| Index | Meaning |
|---:|---|
| 0 | `PEF1` magic/protocol version |
| 1 | record capacity |
| 2/3 | monotonically wrapping write/read sequences |
| 4 | dropped records |
| 5 | callbacks observed |
| 6 | producer done flag |
| 7 | frame discontinuities |
| 8 | high-water occupancy |
| 9/10 | sample rate and quantum frames |

The remaining bytes are `capacity × 48` fixed record slots. The producer reads
indices, checks room for the complete group, writes primitive fields, and then
publishes one atomic write index. Insufficient capacity atomically adds the
whole group size to `dropped` and returns. The collector copies complete wrapped
ranges and advances the read index.

## Steady-state audit

| Operation | Callback behavior |
|---|---|
| Timestamp | numeric `currentFrame`; no `performance`/wall-clock call |
| Admission | two atomic loads and bounded arithmetic |
| Record write | fixed stack `[u8; 48]` and twelve DataView `u32` stores per record |
| Publication | high-water update and one atomic write-index store |
| Metadata | static numeric IDs only; strings registered by the page |
| Full ring | atomic dropped increment and immediate return |
| Export | collector Worker only |

There is no callback wait, lock, file operation, protobuf encoding, dynamic
formatting/string copy, per-event message, retry loop, growing container, or
explicit heap allocation. JavaScript passes frame/queue values as numbers rather
than allocating `BigInt` values per callback. Setup constructs the WASM instance,
DataView, ring, and metadata before processing starts.

Generic dynamic-string fields return `Unsupported`. Separate begin/end facade
calls can be lost independently under overflow; the preferred quantum helper
reserves the complete pair, and collector boundary repair remains an observable
fallback.

## Validation

`scripts/run-audio-browser.sh` runs equal-duration baseline and traced contexts,
a 10,000-iteration producer benchmark, normal bounded collection, malformed
header rejection, and forced overflow. `scripts/validate-audio.sh` checks raw
callback/record/drop/discontinuity/p99 data and Trace Processor SQL for logical
spans, counters, clock snapshots, health, nonnegative slices, and zero clock-sync
errors. `AUDIO_DURATION_MS=60000 AUDIO_REQUIRE_FULL=1` selects the full acceptance
case.
