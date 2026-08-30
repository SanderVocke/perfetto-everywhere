# Stage 8 verification

Documentation and packaging surfaces:

- `README.md`: architecture and native/browser/AudioWorklet/`tracing` quick starts;
- rustdoc: README-backed facade docs plus crate/type docs;
- `docs/support.md`: tested matrix and troubleshooting;
- `docs/versioning.md`: semver, protocol, dependency, and release policy;
- `docs/security.md`: privacy, malformed input, RustSec, and license policy;
- focused architecture/integration documents and executable examples.

Local checks:

```bash
nix develop --command scripts/check.sh
```

The Nix shell uses Rust/Cargo 1.90 for stabilized interdependent-workspace
packaging; CI separately builds/tests Rust 1.85.0 MSRV and current stable.
`scripts/package-workspace.sh` creates and independently verifies all six
publishable crates together without requiring them to exist on crates.io.

GitHub Actions `CI` has required jobs for quality/docs/package verification,
MSRV native+WASM, current native capture/SQL, collector/clocks, current WASM
feature powerset, short/complete Chromium integration, `tracing`, RustSec, and
license metadata. Browser traces, raw metrics, SQL, and UI screenshots are
uploaded as artifacts. `Full browser acceptance` runs the equal 60-second audio
cases manually/weekly and retains its trace/metrics/SQL artifact.

The only RustSec finding is the documented non-vulnerability unmaintained
warning for transitive `paste 1.0.15` through pinned `perfetto-sdk`.
