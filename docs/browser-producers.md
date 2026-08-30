# Ordinary browser producers

`OrdinaryBackend<C>` implements the common facade for Window and Dedicated
Worker realms. Each producer records source-clock ticks, realm ID, and clock ID;
it does not pre-convert timestamps or construct Perfetto protobuf.

The production `PerformanceClock` uses realm-local `performance.now()` and emits
nanoseconds since that realm's `performance.timeOrigin`. Page/Worker bootstrap
exchanges periodic calibration samples containing local ticks, reference epoch,
and measured uncertainty. The collector will preserve these as clock mappings.

Event storage consists of two preallocated fixed-capacity `Vec<Record>` batches.
When the active batch fills, it rotates to ready and the spare becomes active.
If the ready batch has not been drained when another rotation is needed, the
complete event/field group is dropped and counted. Batches only rotate between
groups, cannot grow, and are serialized to transfer bytes outside event calls.
`flush_and_take_batch` supports explicit size/time/lifecycle flushing.

Static names/categories/fields are registered into an out-of-band metadata map.
Dynamic ordinary-realm strings are interned into that map and transported as
metadata IDs; this allocation-capable path is not shared with AudioWorklet
recording. Metadata ID/label conflicts return `Unsupported` rather than emitting
ambiguous records.

`ProducerHealth` reports emitted records, dropped records, completed batches,
and active high-water occupancy. `set_enabled(false)` provides explicit realm
shutdown/capture-off behavior.

The browser test under `tests/browser` loads the same WASM producer in a page and
two Dedicated Workers under COOP/COEP, collects repeated Worker clock samples,
transfers three batches, and verifies protocol version plus unique realm/clock
IDs. Unit tests exercise complete typed groups, dynamic metadata, filters,
repeated flush/reuse, and bounded overflow.
