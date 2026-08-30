# Browser capture runtime

`web/perfetto-browser-runtime.js` contains the minimal JavaScript required by
the Web platform around the Rust/WASM API. `BrowserCaptureController` owns:

- cross-origin-isolation feature checks;
- collector Worker startup and shutdown;
- realm/metadata/calibration registration;
- transferable ordinary-realm batches;
- AudioWorklet SAB allocation and drain scheduling;
- final collector request, errors, and application-owned trace bytes.

Platform setup remains explicit while all instrumentation callsites stay in the
shared Rust facade. `collectorWorkerUrl` and `collectorWorkerOptions` constructor
options select application-packaged collector assets instead of the example
filename. A controller is single-use; construct another after finish
or abort to restart capture. Duplicate realms/metadata, invalid lifecycle calls,
missing browser features, and collector errors are rejected rather than ignored.

The helper also exports deterministic metadata IDs, ordinary/audio calibration
constructors, ring construction, `waitMessage`, and a Blob/download helper.
Applications may replace this small module while retaining the Rust protocol.

## Complete example

```bash
nix develop --command scripts/run-browser-multirealm.sh artifacts
nix develop --command scripts/validate-browser-multirealm.sh artifacts
```

`web/browser-multirealm-example.html` records one Window, two Dedicated Workers,
and one AudioWorklet. It registers static metadata, calibrates all four source
clocks, transfers ordinary batches, drains the audio ring, finalizes one Blob,
adds a download link, and verifies controller restart. Flow ID 42 forms:

```text
Window request graph rebuild
  -> Worker A compile graph
  -> AudioWorklet audio graph installed
```

The example reports producer disconnection, drops, callback discontinuities,
record counts, and calibration count. Collector limits, malformed/unknown data,
and span repair are covered by Rust tests.

## Deployment

The page and all same-origin scripts/WASM must be served with at least:

```text
Cross-Origin-Opener-Policy: same-origin
Cross-Origin-Embedder-Policy: require-corp
```

The included development server provides these headers. Production deployments
must ensure every loaded subresource satisfies the selected COEP policy.
