# Security and privacy

The library performs no network upload. Trace files may contain messages,
arguments, paths, IDs, and application state; applications must treat them as
potentially sensitive, use category/duration limits, and choose storage/sharing
policy explicitly. Browser Blob/download and native file APIs remain under
application control.

CI and `scripts/check.sh` run RustSec `cargo audit` and a resolved-license
metadata policy. At plan completion there are no known vulnerability advisories.
RustSec reports `RUSTSEC-2024-0436` as an allowed *unmaintained* warning for
`paste 1.0.15`, transitively required by pinned `perfetto-sdk 1.1.1`; it is not a
vulnerability. Removing it requires an upstream SDK update and must retain native
trace compatibility tests.

Malformed browser records, unknown versions/metadata/clocks, collisions, limits,
and non-monotonic/noisy calibration are rejected. The AudioWorklet producer never parses untrusted dynamic strings or protobuf. Transferable chunk ownership is validated with capture IDs, sequences, and pool tokens; the library performs no network transfer.

Please report suspected vulnerabilities privately to the repository owner before
opening a public issue containing exploit or sensitive trace data.
