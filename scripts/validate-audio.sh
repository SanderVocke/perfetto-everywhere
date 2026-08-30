#!/usr/bin/env bash
set -euo pipefail
output=${1:-artifacts}
mkdir -p "$output" target/perfetto-tool-home
HOME="$PWD/target/perfetto-tool-home" scripts/trace_processor \
  query -f tests/sql/audio.sql "$output/audio-transport.pftrace" \
  | tee "$output/audio-query.txt"
python3 - "$output/audio-transport.json" "$output/audio-query.txt" <<'PY'
import json, os, sys
raw=json.load(open(sys.argv[1]))
query=open(sys.argv[2]).read()
minimum_duration = 60000 if os.environ.get('AUDIO_REQUIRE_FULL') == '1' else 1000
assert raw['requestedDurationMs'] >= minimum_duration, raw
assert raw['baselineCallbacks'] > 0 and raw['callbacks'] > 0, raw
assert raw['records'] == raw['expectedRecords'] == raw['callbacks'] * 4 + 1, raw
assert raw['dropped'] == 0, raw
assert raw['discontinuities'] <= raw['baselineDiscontinuities'], raw
assert raw['benchmarkP99Ms'] < raw['quantumBudgetMs'] * 0.10, raw
assert raw['forcedDropped'] >= 4 and raw['malformedRejected'], raw
assert raw['calibrations'] >= 3, raw
assert f'6,{raw["calibrations"]}' in query, query
assert f'64,{raw["calibrations"]}' in query, query
assert f'"audio process quantum",{raw["callbacks"]}' in query, query
assert f'"clock calibration",{raw["calibrations"]}' in query, query
assert f'"AudioWorklet/audio cpu load",{raw["callbacks"]},0.200000,0.290000' in query, query
assert f'"AudioWorklet/audio queue depth",{raw["callbacks"]},0.000000,15.000000' in query, query
assert '"debug.dropped_records","0"' in query, query
assert '"debug.repaired_span_boundaries","0"' in query, query
assert query.count('\n0\n') >= 2, query
PY
echo "AudioWorklet transport and clock validation passed"
