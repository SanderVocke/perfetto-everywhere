#!/usr/bin/env bash
set -euo pipefail
cargo package --workspace \
  --exclude native-capture-example \
  --exclude tracing-bridge-example \
  --exclude ordinary-browser-producer
