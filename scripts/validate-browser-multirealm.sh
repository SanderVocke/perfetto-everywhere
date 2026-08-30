#!/usr/bin/env bash
set -euo pipefail
output=${1:-artifacts}
mkdir -p "$output" target/perfetto-tool-home
HOME="$PWD/target/perfetto-tool-home" scripts/trace_processor \
  query -f tests/sql/browser-multirealm.sql "$output/browser-multirealm.pftrace" \
  | tee "$output/browser-multirealm-query.txt"
python3 - "$output/browser-multirealm.json" "$output/browser-multirealm-query.txt" <<'PY'
import json, sys
raw=json.load(open(sys.argv[1]))
query=open(sys.argv[2]).read()
assert raw['ordinaryRecords'] == 24, raw
assert raw['audioRecords'] == raw['expectedAudioRecords'] == raw['audioCallbacks'] * 4 + 1, raw
assert raw['dropped'] == 0 and raw['discontinuities'] <= 1 and raw['restartVerified'], raw
for track in ('Window','Worker A','Worker B','AudioWorklet'):
    assert f'"{track}",' in query, track
for name in ('request graph rebuild','compile graph','ordinary task','audio graph installed'):
    assert f'"{name}",' in query, name
assert '"request graph rebuild","compile graph"' in query, query
assert '"compile graph","audio graph installed"' in query, query
assert f'6,{raw["calibrations"]}' in query, query
assert f'64,{raw["calibrations"]}' in query, query
assert f'"AudioWorklet/audio cpu load",{raw["audioCallbacks"]},0.200000,0.290000' in query
assert f'"AudioWorklet/audio queue depth",{raw["audioCallbacks"]},0.000000,15.000000' in query
assert '"Window/worker counter",1,1.000000,1.000000' in query
assert '"Worker A/worker counter",1,2.000000,2.000000' in query
assert '"Worker B/worker counter",1,3.000000,3.000000' in query
assert '"debug.realm","1"' in query
assert '"debug.target","ordinary producer"' in query
assert '"debug.severity","3"' in query
assert query.count('\n0\n') >= 3, query
PY
echo "complete browser multirealm validation passed"
