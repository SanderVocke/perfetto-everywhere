# Stage 6 verification

```bash
nix develop --command scripts/run-browser-multirealm.sh artifacts
nix develop --command scripts/validate-browser-multirealm.sh artifacts
nix develop --command scripts/capture-browser-ui.sh artifacts/browser-ui.png
```

The accepted local trace contains 24 ordinary records plus one flow event and
four records for each of 784 AudioWorklet callbacks. It has zero drops, twelve
clock calibrations, no clock/parser errors, no negative slices, and distinct
Window, Worker A, Worker B, and AudioWorklet tracks. SQL proves integer/double
counters, typed realm/category/source fields, health events, and both flow links.
The run also terminates both Workers and starts/aborts a second controller.

A reproducible Perfetto UI screenshot shows the four realm tracks on one shared
timeline, dense logical audio quanta, clock/health instants, and flow markers.
Manual inspection confirms tracks expand, event arguments are searchable,
counter tracks plot, and selecting flow endpoints exposes the asynchronous
links. SQL remains the deterministic flow and clock validation surface.
