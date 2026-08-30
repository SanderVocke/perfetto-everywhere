# Versioning and release policy

The crates follow Semantic Versioning. Before 1.0, public Rust API changes may
occur in minor releases and are documented. The compact record protocol is
versioned independently from crate semver:

- unknown record versions, kinds, flags, and malformed groups are rejected;
- a producer and collector must agree on an explicitly supported protocol;
- adding optional record kinds requires collector compatibility tests;
- changing field offsets/meaning requires a new protocol version;
- metadata namespaces and clock identity rules are part of the protocol;
- old golden fixtures remain until the documented support window expires.

`Cargo.lock`, `flake.lock`, wasm-bindgen/CLI, `perfetto-sdk`, prost, and the
Perfetto schema snapshot are pinned. Dependency updates require native capture,
WASM/browser runtime, AudioWorklet, protobuf golden, and current Trace Processor
validation. GitHub Actions are pinned to full commit hashes and reviewed during
updates.

A release candidate requires clean package verification of all publishable
workspace crates, MSRV/current native+WASM CI, short browser CI, a successful
full AudioWorklet workflow, documentation, license/security checks, and the
criterion-by-criterion acceptance audit. Crates.io publication is intentionally
outside the current implementation plan.
