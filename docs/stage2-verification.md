# Stage 2 verification

Commands:

```bash
nix develop --command scripts/run-native-example.sh artifacts
nix develop --command scripts/validate-native.sh
nix develop --command scripts/benchmark-native.sh artifacts
```

The example proves capture-off startup, category filtering, two sequential
sessions (1024 and 256 KiB), application-owned file output, nested spans,
instants, structured logs, bool/i64/u64/f64/static/dynamic-string fields,
integer/double counters on registered tracks, and a flow. A third 8 KiB capture
forces bounded overwrite and retains Trace Processor health evidence.

The synthetic benchmark executes three release processes per mode with 100,000
iterations. On the implementation host, representative medians were 0.054 ms
disabled, 0.177 ms inactive, and 30.267 ms active (about 3.30 million recorded
instants/s, 3.70 MB trace). Process CPU and peak RSS are retained by the script.
These values are only a catastrophic-regression sanity check and are not a
hardware-independent or AudioWorklet performance claim.

The CI version uses fewer iterations/repetitions while checking all three modes;
the reproducible full command retains raw and summarized output under
`artifacts/`.
