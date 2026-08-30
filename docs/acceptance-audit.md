# Acceptance audit

## Objective restatement

The implementation is complete only when `SanderVocke/perfetto-everywhere` is
an independently clonable, documented Rust workspace/submodule that provides the
common API, native files, multirealm browser files, bounded AudioWorklet support,
collector-owned clocks/snapshots, a `tracing` adapter, reproducible package/tests,
and green required/full GitHub Actions, with every `IMPLEMENT.md` criterion tied
to direct evidence.

## Prompt-to-artifact checklist

| Immutable criterion | Direct implementation and evidence | Status |
|---|---|---|
| 1. Independent upstream/submodule | Public `SanderVocke/perfetto-everywhere`, SSH `origin`, `main`; `docs/repository.md`; parent `.gitmodules`/gitlink; standalone and recursive clone commands | PASS |
| 2. Unified public API | `perfetto-everywhere-core/{api,metadata,protocol}.rs`, facade target aliases/features, `examples/api-contract.rs`; native/WASM/disabled/tracing feature CI | PASS |
| 3. Native capture | `perfetto-everywhere-native`, `examples/native-capture`, `scripts/{run-native-example,validate-native}.sh`, native/overflow SQL, two files and overflow health | PASS |
| 4. Browser multirealm | `BrowserCaptureController`, page/two Workers/AudioWorklet example, collector Worker, `browser-multirealm.pftrace` artifact and SQL validating four realms/all semantics/two-hop flow | PASS |
| 5. AudioWorklet safety | `web/src/audio.rs`, 64-byte SAB header/48-byte slots, static audit in `docs/audio-worklet.md`, forced overflow, baseline/traced metrics, short/full workflows | PASS |
| 6. Clock correctness | Raw realm ticks, `ClockCalibration`, robust continuous-segment fit, sequence-local ID 64, periodic `ClockSnapshot`, raw residual events, synthetic drift/error tests and clock SQL | PASS |
| 7. Trace usability/resilience | Trace Processor validators for native/bridge/collector/audio/multirealm; malformed/version/limit/collision tests; health and span repair; UI screenshot/checklist | PASS |
| 8. `tracing` compatibility | `perfetto-everywhere-tracing`, repeated enter/late-field unit test, native trace/SQL, browser WASM 9-record test, documented counter/flow/AudioWorklet limitations | PASS |
| 9. User experience | Comprehensive `README.md`, rustdoc, native/browser/audio/tracing quick starts, examples, deployment headers, support/clock/safety docs and matrices | PASS |
| 10. Reproducibility/quality | Nix lock, Cargo lock, Rust 1.85/current, fmt/clippy/tests/docs, native/WASM/browser, RustSec/license policy, verified workspace packaging, `scripts/check.sh` | PASS |
| 11. Green GitHub Actions | Required CI run `33286475631` at audited source commit `0762896`; full run `33286745109` and retained `audio-worklet-60s-0762896...` artifact | PASS; final documentation/workflow-only run recorded below |
| 12. Evidence-backed release | This audit, stage verification docs, clean-clone log/hash, CI URLs, clean/pushed upstream and matching parent gitlink | PASS after final parent pointer commit |

## Explicit architecture and functionality mapping

| Requested deliverable | Evidence |
|---|---|
| Rust crate/workspace | Six publishable crates in root Cargo workspace; verified together by `scripts/package-workspace.sh` |
| Native in-process file tracing | `CaptureSession`, `CaptureReport::{write_to,write_file}`, native example and SQL |
| Web page/multiple Worker/worklet trace files | `web/browser-multirealm-example.html`, runtime/collector workers, browser CI artifact |
| AudioWorklet support | `AudioRingBackend`/`AudioRingProducer`, SAB scripts, 60-second workflow |
| Unified Rust API where possible | `Tracer<B>`/`TraceBackend`, target-selected `PlatformBackend`; setup differences isolated/documented |
| `tracing` facade | `PerfettoLayer`/`SharedBackend`, native/browser tests and migration docs |
| User-friendly docs | README plus `docs/{native,browser-producers,browser-runtime,audio-worklet,tracing-compatibility,support}.md` |
| Create `SanderVocke/perfetto-everywhere` with `gh` | `docs/repository.md`, live GitHub metadata and origin |
| GitHub Actions CI green | `.github/workflows/{ci,full-browser-acceptance}.yml` plus `gh run` evidence |
| Periodic collector clock snapshots | collector fitting/snapshot/diagnostic code and `clock_snapshot` SQL with zero sync errors |

## Command, test, and gate coverage

| Surface | Verification |
|---|---|
| Gate 1 repository/bootstrap | `gh repo view`; standalone clone; parent recursive clone |
| Gate 2 API/protocol | core unit/golden/compile-fail tests and native/WASM API-contract builds |
| Gate 3 native | native release example, two traces, feature/counter/flow SQL, forced overflow, benchmark |
| Gate 4 writer/clocks | collector unit tests and `scripts/validate-collector.sh` with real Trace Processor |
| Gate 5 audio | static audit, short CI, full baseline/traced workflow, raw JSON and audio SQL |
| Gate 6 usability | complete example command, controller restart, UI screenshot, two-hop flow SQL |
| Formatting/lints/tests/docs | `cargo fmt`, clippy `-D warnings`, workspace/all-feature tests, rustdoc `-D warnings` |
| Build/feature matrix | native current/MSRV; WASM current/MSRV; default/disabled/tracing/combined |
| Packaging | full Cargo workspace package creation and independent tarball verification |
| Supply chain | `cargo audit`; resolved-license metadata policy; pinned Cargo/Nix/Actions/dependencies |
| Full local release | `nix develop --command scripts/validate-release.sh` |
| Full remote audio | `gh workflow run full-browser-acceptance.yml`; `gh run watch --exit-status` |

## Named evidence index

- Native: `examples/native-capture`, `tests/sql/native*.sql`,
  `scripts/validate-native.sh`, `docs/{native,stage2-verification}.md`.
- Browser producers: `crates/perfetto-everywhere-web`,
  `web/ordinary-browser-test.html`, `docs/browser-producers.md`.
- Collector/clocks: `crates/perfetto-everywhere-collector`,
  `tests/sql/collector.sql`, `scripts/validate-collector.sh`,
  `docs/collector-and-clocks.md`.
- Audio: `web/{audio-browser-test,perfetto-audio-worklet,audio-collector-worker}.js/html`,
  `tests/sql/audio.sql`, `scripts/{run-audio-browser,validate-audio}.sh`.
- Complete web: `web/{perfetto-browser-runtime,browser-multirealm-example,multirealm-collector-worker}.js/html`,
  `tests/sql/browser-multirealm.sql`, multirealm run/validation/UI scripts.
- Adapter: `crates/perfetto-everywhere-tracing`, `examples/tracing-bridge`,
  `tests/sql/tracing.sql`, `scripts/validate-tracing.sh`.
- Reproducibility: `flake.{nix,lock}`, `Cargo.lock`, `scripts/check.sh`,
  `scripts/validate-release.sh`, both GitHub workflows.

## Final clean-checkout execution

A fresh standalone GitHub clone was detached at
`0762896eeb0cc4872778a6d5e02fb371bc7242de` and ran:

```bash
nix develop --command scripts/validate-release.sh
```

The command rebuilt/tested/documented/packaged all crates, ran supply-chain
checks, regenerated and queried every native/collector/browser trace, captured
the Perfetto UI, and ran equal 60-second baseline/traced audio cases. It ended
with `complete perfetto-everywhere release validation passed`. The 862-line raw
log is `docs/evidence/clean-validation.log` (SHA-256
`cf03348a5be581d901b62a6c40425c980e667dbb88b69c92156cbbb337216ca5`).
The clone remained clean because reproducible outputs are ignored artifacts.

The clean full audio result (`docs/evidence/clean-audio.json`) was 22,500
baseline callbacks/zero discontinuities and 22,536 traced callbacks/90,145
records/zero drops/zero discontinuities, 145-record high water, 122 clock
calibrations, 0.010 ms p99 versus the 0.267 ms threshold, and a 6.88 MB trace.

GitHub CI run <https://github.com/SanderVocke/perfetto-everywhere/actions/runs/33286475631>
passed every required job at the same audited source commit. Full workflow run
<https://github.com/SanderVocke/perfetto-everywhere/actions/runs/33286745109>
also passed: 22,506 baseline callbacks and 22,547 traced callbacks/90,189 records,
zero drops/discontinuities, 0.010 ms p99, and retained artifact
`audio-worklet-60s-0762896eeb0cc4872778a6d5e02fb371bc7242de` (artifact ID
`9724714329`). Commit `e065333` only upgrades the pinned artifact-upload action
to its Node 24 release; final CI/full-workflow links for the final audited commit
are appended before release completion.
