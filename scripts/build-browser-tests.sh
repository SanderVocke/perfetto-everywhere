#!/usr/bin/env bash
set -euo pipefail
cargo build --release --target wasm32-unknown-unknown \
  -p ordinary-browser-producer -p perfetto-everywhere-web -p perfetto-everywhere-collector
mkdir -p web/pkg/ordinary
wasm-bindgen \
  --target web \
  --out-dir web/pkg/ordinary \
  --out-name ordinary_browser_producer \
  target/wasm32-unknown-unknown/release/ordinary_browser_producer.wasm
mkdir -p web/pkg/audio web/pkg/collector
wasm-bindgen \
  --target web \
  --out-dir web/pkg/audio \
  --out-name perfetto_everywhere_web \
  target/wasm32-unknown-unknown/release/perfetto_everywhere_web.wasm
wasm-bindgen \
  --target web \
  --out-dir web/pkg/collector \
  --out-name perfetto_everywhere_collector \
  target/wasm32-unknown-unknown/release/perfetto_everywhere_collector.wasm
