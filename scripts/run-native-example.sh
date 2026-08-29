#!/usr/bin/env bash
set -euo pipefail
output=${1:-artifacts}
mkdir -p "$output"
cargo run --release -p native-capture-example -- "$output" \
  2>&1 | tee "$output/native-run.txt"
