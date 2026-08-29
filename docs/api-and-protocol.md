# API and compact protocol contract

## Ownership and lifecycle

Instrumentation is expressed through `Tracer<B>` and `TraceBackend`. The same
instrumentation function can be generic over a backend on every target. Backend
construction and capture/realm setup remain platform-owned operations.

A lexical `SpanGuard` closes only a successfully recorded begin and is not
`Send`, keeping begin/end on one execution track. Future asynchronous spans must
use an explicit ID API rather than moving a lexical guard. Backends consume
borrowed fields synchronously. Shutdown rejects new records, drains complete
published groups, emits health diagnostics, and then finalizes capture output.

`EmitStatus` distinguishes recorded, disabled, dropped, and unsupported data.
Dynamic strings are allowed only when a backend explicitly supports them;
AudioWorklet producers return `Unsupported`. `NoopBackend` is the contract for
compile-time-disabled instrumentation.

## Static metadata

Names, categories, and field names use namespaced deterministic FNV-1a IDs.
Definitions are registered before capture and equal IDs with different
namespace/label pairs are rejected. Runtime-created ordinary/native strings use
a separate metadata control channel and are never embedded per event on the
AudioWorklet path.

Flow ID zero is reserved. Trace-global flows are allocated by an initiating
realm and transported with application messages. Track ID zero means the
backend's current execution track.

## Record protocol version 1

Each event/field record is exactly 48 little-endian bytes:

| Offset | Type | Meaning |
|---:|---|---|
| 0 | `u8` | protocol version |
| 1 | `u8` | record kind |
| 2 | `u16` | group/flow flags |
| 4 | `u32` | realm ID |
| 8 | `u32` | static name/field ID |
| 12 | `u32` | source clock-domain ID |
| 16 | `u64` | source-domain timestamp |
| 24 | `u64` | typed value bits/severity/health value |
| 32 | `u64` | nonzero flow ID or zero |
| 40 | `u64` | kind-specific primitive argument |

Event headers and their typed field records are contiguous groups. A producer
reserves a complete group, marks its first and final records, writes all bytes,
and only then publishes it. Ordinary batches flush only between groups. A full
AudioWorklet ring drops the complete group. Collectors reject partial groups,
unknown versions/kinds/flags, conflicting flow flags, and fields crossing realm,
clock, or timestamp boundaries.

Metadata definitions and clock calibration samples are bounded control messages,
not event records. This keeps fixed records small while allowing static strings,
dynamic ordinary-realm metadata, and periodic Perfetto clock snapshots.

## Audio timeline semantics

An AudioWorklet event timestamp is an exact `currentFrame` reading in its source
clock. A logical quantum span covers a declared frame interval; it is not a
measurement of callback CPU duration. Exact frame and sample-rate metadata are
preserved even when the collector constructs nanosecond audio-clock timestamps.
