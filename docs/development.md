# Development and dependency policy

## Toolchain

The workspace MSRV is Rust 1.85.0. `flake.nix` provides that compiler, the
`wasm32-unknown-unknown` standard library, Chromium, Node, protobuf tooling, and
native C/C++ build tools. CI additionally tests the current GitHub-hosted stable
compiler until an explicit matrix is added.

```bash
nix develop --command scripts/check.sh
```

## Repository

Standalone clone:

```bash
git clone git@github.com:SanderVocke/perfetto-everywhere.git
```

Prototype repository clone after it registers this repository as a submodule:

```bash
git clone --recurse-submodules <prototype-repository-url>
```

## Dependency updates

Rust dependencies are pinned by `Cargo.lock`. Nix inputs are pinned by
`flake.lock`. Updates must pass native, WASM, browser, Trace Processor, and
protocol compatibility tests before merge. GitHub Actions are pinned to full
commit hashes and updated only after reviewing upstream release notes.

The native Perfetto SDK and browser protobuf schema are compatibility-sensitive;
their eventual versions and upgrade procedure will be documented before the API
is considered ready.
