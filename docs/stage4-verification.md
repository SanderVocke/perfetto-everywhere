# Stage 4 verification

```bash
nix develop --command cargo test -p perfetto-everywhere-collector
nix develop --command cargo check -p perfetto-everywhere-collector \
  --target wasm32-unknown-unknown
nix develop --command scripts/validate-collector.sh artifacts
```

Unit tests cover protobuf decoding, periodic snapshots, missing/non-monotonic/
noisy clocks, malformed and partial records, unknown protocol versions, and
capture limits. `scripts/validate-collector.sh` creates a deterministic two-realm
trace and uses Trace Processor SQL to validate descriptors, typed events,
counters, flow, periodic custom-clock snapshots, an injected-drift mapping
boundary, nonnegative durations, and zero clock-sync errors.

The selected sequence-scoped clock model and BOOTTIME normalization are tested
against the repository's reproducibly downloaded Trace Processor rather than
accepted from protobuf parseability alone.
