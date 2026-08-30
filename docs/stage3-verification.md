# Stage 3 verification

```bash
nix develop --command cargo test -p perfetto-everywhere-web
nix develop --command cargo check -p perfetto-everywhere \
  --target wasm32-unknown-unknown
nix develop --command scripts/run-ordinary-browser.sh artifacts
```

The automated Chromium run requires cross-origin isolation and proves one page
plus two module Workers load the generated WASM, perform ten clock-calibration
ping/pongs, and transfer eight complete records per realm with distinct realm
and source-clock IDs. Raw realm/calibration output is retained at
`artifacts/ordinary-browser.json` and in CI.

Native unit tests prove producer batch bounds, complete field groups, typed
values, metadata registration, filtering, shutdown, drop accounting, and reuse.
No producer dependency contains Perfetto protobuf code or network collection.
