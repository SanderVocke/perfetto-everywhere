# Stage 1 verification

The public semantic and protocol contracts are in:

- `perfetto-everywhere-core/src/api.rs`;
- `perfetto-everywhere-core/src/metadata.rs`;
- `perfetto-everywhere-core/src/protocol.rs`;
- `docs/api-and-protocol.md`.

Verified properties include typed values, RAII close behavior, non-`Send` lexical
guards, nonzero rollover-safe flows, namespaced metadata collision rejection,
48-byte protocol layout, version/kind/flag rejection, and complete field groups.
The fixed encoder returns a stack array and metadata validation uses bounded
slice iteration; neither requires allocation. The backend contract consumes
borrowed fields synchronously and permits a real-time backend to reject dynamic
strings.

Commands:

```bash
nix develop --command cargo test --workspace --all-features
nix develop --command cargo check -p perfetto-everywhere --features disabled --examples
nix develop --command cargo check -p perfetto-everywhere \
  --target wasm32-unknown-unknown --features disabled --examples
nix develop --command cargo clippy --workspace --all-targets --all-features -- -D warnings
```

`examples/api-contract.rs` is the same instrumentation source for native and
WASM and contains no platform conditional. Backend lifecycle remains explicitly
platform-owned, as required by the API contract.
