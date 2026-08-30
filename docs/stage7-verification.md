# Stage 7 verification

```bash
nix develop --command cargo test -p perfetto-everywhere-tracing
nix develop --command cargo check -p perfetto-everywhere --features tracing
nix develop --command cargo check -p perfetto-everywhere \
  --features tracing --target wasm32-unknown-unknown
nix develop --command scripts/validate-tracing.sh artifacts
nix develop --command scripts/run-ordinary-browser.sh artifacts
```

The native trace contains two entries of one reusable span, typed initial and
late-recorded fields, INFO/WARN events, dynamic messages, levels, and graph/audio
targets with zero import errors. Unit tests check begin/end balance and typed
message mapping. The headless ordinary-browser test emits nine complete compact
records through the same layer. Documentation and crate boundaries exclude the
mutex/allocating adapter from AudioWorklet real-time use.
