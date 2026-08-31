# AudioWorklet producer

`AudioChunkProducer` records exact `currentFrame` values into the bounded raw
Wasm ring. After a render quantum, JavaScript drains complete 48-byte groups
into one of a preallocated pool of ordinary `ArrayBuffer` chunks. A completed or
partially used chunk transfers to the collector, which consumes its used prefix
and transfers the empty buffer back. No trace byte storage is shared between
realms and browser tracing does not require cross-origin isolation.

The pool contains at least one active and two spare/in-flight buffers. Chunk
capacity is validated to hold the largest group accepted by the raw ring.
Rotation occurs between complete groups, including when the next group does not
fit the active chunk's remaining tail. A missing spare never blocks audio;
records remain in the raw ring until space returns and overflow drops complete
groups.

Stop disables new records and drains accepted records. If every buffer is in
flight, completion remains pending until a recycle arrives. The final partial
chunk precedes `trace-stopped`, whose `chunkCount` declares the half-open
sequence range; zero represents an empty capture. The collector rejects gaps,
duplicates, stale capture IDs, invalid tokens, malformed lengths, and incomplete
groups.

The page samples `AudioContext.getOutputTimestamp()` outside the callback and
supplies clock calibrations to the collector. Logical quantum intervals are not
direct callback CPU-duration measurements.

`scripts/run-audio-browser.sh` compares baseline and traced contexts, exercises
forced raw overflow and multi-chunk transfer without isolation headers, and
produces data for `scripts/validate-audio.sh`. Set
`AUDIO_DURATION_MS=60000 AUDIO_REQUIRE_FULL=1` for the full acceptance run.
