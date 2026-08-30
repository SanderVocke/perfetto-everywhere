# Collector, schema, and clocks

## Schema policy

The browser collector pins `tracing-perfetto-sdk-schema` 0.13.1 and prost 0.14.1.
That crate generates Rust from an Apache-licensed snapshot of Perfetto's public
protobuf schema. `perfetto-everywhere-collector` deliberately maps only
`Trace`, `TracePacket`, `ClockSnapshot`, `TrackDescriptor`, `TrackEvent`,
`CounterDescriptor`, and `DebugAnnotation`. The schema package is not exposed
in the public producer API. An upgrade must pass protobuf golden tests and both
the pinned/current supported Trace Processor validation before merge.

## Validation and output

`Collector` registers realms and metadata, ingests only complete versioned
record batches, validates all realm/metadata/clock references, enforces a
configurable record limit, and rejects malformed versions, fields, groups,
collisions, and clock samples. Unmatched span ends are discarded and unmatched
begins receive a synthetic end at the realm's last source timestamp; both are
counted as `repaired_span_boundaries` in producer-health events. It emits
deterministic descriptors, events, typed fields, counters, flows, health events,
and final protobuf bytes.
`WasmCollector` exposes the same operations to a collector Worker.

Long captures are bounded by `max_records`; production applications must stop or
chunk before the limit. The current final protobuf is application-owned bytes,
which can be wrapped in a Blob or streamed by a higher-level runtime stage.

## Clock model

Every producer retains local source ticks and has its own trusted packet
sequence. Custom clock ID 64 is intentionally reused because IDs 64–127 are
sequence-scoped; reserved global IDs are not used. Each clock snapshot and all
dependent events use the realm's same `trusted_packet_sequence_id`.

The collector normalizes the earliest browser reference calibration to trace
BOOTTIME 1 second, then emits paired observations:

```text
BOOTTIME trace reference = normalized browser reference ns
custom clock 64          = producer source ns
```

All snapshots are emitted before dependent packets in the offline file. Events
carry raw source timestamps and `timestamp_clock_id = 64`; Trace Processor
converts them during import. Within one continuous clock segment, the collector
uses a robust median source/reference offset for all periodic snapshots. This
prevents noisy observations from introducing timestamp steps into spans while
retaining every raw observation, uncertainty, and fit residual as `clock
calibration` events. A lifecycle/discontinuity starts a new segment in the
higher-level runtime. The configured uncertainty ceiling rejects excessively
noisy samples; snapshots do not manufacture accuracy beyond supplied data.

The deterministic smoke trace has two realms, two snapshots each, injected
1 ns observation drift, and a slice crossing a snapshot boundary. SQL verifies
four snapshot pairs, stable fitted timestamps/durations, calibration residuals,
counters, flow, nonnegative slices, and zero non-informational `clock_sync*`
statistics.
