#!/usr/bin/env bash
set -euo pipefail
cargo build --release --target wasm32-unknown-unknown -p ordinary-browser-producer
mkdir -p web/pkg/ordinary
wasm-bindgen \
  --target web \
  --out-dir web/pkg/ordinary \
  --out-name ordinary_browser_producer \
  target/wasm32-unknown-unknown/release/ordinary_browser_producer.wasm
