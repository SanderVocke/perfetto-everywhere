# Stage 5 verification

Short development run:

```bash
nix develop --command scripts/run-audio-browser.sh artifacts
nix develop --command scripts/validate-audio.sh artifacts
```

Full acceptance run (two 60-second contexts):

```bash
AUDIO_DURATION_MS=60000 nix develop --command scripts/run-audio-browser.sh artifacts
AUDIO_REQUIRE_FULL=1 nix develop --command scripts/validate-audio.sh artifacts
```

The accepted local full run observed 22,496 baseline callbacks with one
scheduler discontinuity and 22,536 traced callbacks with zero discontinuities.
The traced run emitted 90,145 records (four per quantum plus one flow event),
with zero drops, 145-record maximum occupancy, 0.005 ms producer p99 versus the
0.267 ms threshold, 122 clock calibrations, and a 6.88 MB trace. Forced capacity
overflow dropped and counted all four records without publishing a partial
group.

The run retains JSON metrics, the `.pftrace`, and SQL output under `artifacts/`.
It covers baseline versus traced callback continuity, exact records-per-quantum,
normal zero-drop operation, producer p99 against 10% of the quantum budget,
forced complete-group overflow, malformed headers, ring wrap/draining,
periodic `getOutputTimestamp` calibration, fitted/raw clock diagnostics, integer
and floating counters, logical quantum spans, and producer health.

Native unit tests additionally cover wrapping sequence arithmetic, impossible
occupancy, fixed header/record alignment, malformed protocol, complete groups,
clock fitting, incomplete-span repair, and missing/noisy/non-monotonic clocks.
