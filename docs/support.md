# Support and troubleshooting

## Compatibility matrix

| Environment | Automated status |
|---|---|
| Linux x86_64 native, Rust 1.85/current | Build, unit, capture, SQL |
| `wasm32-unknown-unknown` | Build on Rust 1.85/current |
| Chromium Window + Dedicated Workers | Headless runtime, transfer, clocks |
| Chromium AudioWorklet | Short every-push and full scheduled/manual acceptance |
| Firefox | Not yet claimed |
| Safari | Not yet claimed |
| Windows/macOS native | Not yet claimed by CI |

Unsupported environments may compile, but this project only claims the rows with
automated evidence. Compatibility additions require trace import and runtime—not
just compilation—before updating this table.

## Browser capture will not start

Check `crossOriginIsolated`, `SharedArrayBuffer`, module-worker support, and the
console error. Every subresource must satisfy COEP. Use `scripts/serve.py` to
separate header problems from application integration.

## AudioWorklet constructor fails

Verify that the SAB header magic, capacity, sample rate, quantum size, WASM
module, and wasm-bindgen JS/WASM versions match. The generated glue and CLI are
pinned to 0.2.127. `TextDecoder` is absent in tested AudioWorkletGlobalScope; the
provided initialization-only shim is required.

## Events are missing

Inspect `trace producer health` events for dropped records, high-water occupancy,
and repaired span boundaries. Check category filters and whether capture was
active. Ordinary producers must flush/submit their final batch before finish.

## Events are shifted in time

Query `clock_snapshot`, then inspect `clock calibration` events for uncertainty
and fit residuals. A source clock without an initial mapping is rejected. Device,
resume, sample-rate, and detected discontinuity boundaries require fresh
calibration/a new continuous segment.

## Trace Processor rejects a trace

Run the checked-in validation script and inspect non-informational `stats` rows.
Do not suppress parser, clock-sync, hierarchy, or data-loss diagnostics. Record
protocol/schema upgrades require golden and current Trace Processor tests.
