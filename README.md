# perfetto-everywhere

A Rust-first tracing library for native applications and browser applications
spanning Window, Dedicated Worker, and AudioWorklet realms. Native captures use
Perfetto's in-process backend. Browser captures use compact records and a WASM
collector that exports standard Perfetto trace files.

This repository is under active implementation. The public API is not yet ready
for use.

## Development

```bash
nix develop
cargo test --workspace
cargo check --workspace --target wasm32-unknown-unknown
```

Licensed under either Apache-2.0 or MIT, at your option.
