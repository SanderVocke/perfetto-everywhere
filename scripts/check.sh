#!/usr/bin/env bash
set -euo pipefail
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
RUSTDOCFLAGS=-Dwarnings cargo doc --workspace --all-features --no-deps
cargo build --release --workspace
cargo check -p perfetto-everywhere --features disabled --examples
cargo check -p perfetto-everywhere --target wasm32-unknown-unknown --features disabled --examples
cargo build --release --target wasm32-unknown-unknown \
  -p perfetto-everywhere \
  -p perfetto-everywhere-core \
  -p perfetto-everywhere-web \
  -p perfetto-everywhere-collector \
  -p perfetto-everywhere-tracing
for manifest in crates/*/Cargo.toml; do
  cargo package --manifest-path "$manifest" --list --allow-dirty >/dev/null
done
