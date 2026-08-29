#!/usr/bin/env bash
set -euo pipefail
trace=${1:-artifacts/native-first.pftrace}
output=${2:-artifacts/native-query.txt}
mkdir -p "$(dirname "$output")" target/perfetto-tool-home
HOME="$PWD/target/perfetto-tool-home" scripts/trace_processor \
  query -f tests/sql/native.sql "$trace" | tee "$output"
grep -Fq '"compile graph",1' "$output"
grep -Fq '"nested work",1' "$output"
grep -Fq '"queue pressure",1' "$output"
grep -Fq '"worker task",1' "$output"
grep -Fq '"cpu_load [track 2]",2,0.250000,0.910000,0.580000' "$output"
grep -Fq '"queue_depth [track 1]",2,2.000000,9.000000,5.500000' "$output"
grep -Fq '"debug.enabled","true"' "$output"
grep -Fq '"debug.revision","-7"' "$output"
grep -Fq '"debug.nodes","123"' "$output"
grep -Fq '"debug.ratio","0.625"' "$output"
grep -Fq '"debug.phase","statically interned"' "$output"
grep -Fq '"debug.details","dynamic native field"' "$output"
test "$(grep -c '^1$' "$output")" -ge 1
test "$(grep -c '^0$' "$output")" -ge 2

overflow_output="$(dirname "$output")/native-overflow-query.txt"
overflow_trace="$(dirname "$trace")/native-overflow.pftrace"
HOME="$PWD/target/perfetto-tool-home" scripts/trace_processor \
  query -f tests/sql/native-overflow.sql "$overflow_trace" | tee "$overflow_output"
python3 - "$overflow_output" <<'PY'
import csv, sys
rows = list(csv.DictReader(open(sys.argv[1])))
values = {row['name']: int(row['value']) for row in rows}
assert values['traced_buf_bytes_overwritten'] > 0, values
assert values['traced_buf_chunks_overwritten'] > 0, values
assert values['traced_buf_write_wrap_count'] > 0, values
PY
second_trace="$(dirname "$trace")/native-second.pftrace"
second_output="$(dirname "$output")/native-second-query.txt"
test -s "$second_trace"
HOME="$PWD/target/perfetto-tool-home" scripts/trace_processor \
  query -f tests/sql/native.sql "$second_trace" > "$second_output"
grep -Fq '"compile graph",1' "$second_output"
grep -Fq '"serious_import_errors"' "$second_output"
test "$(tail -1 "$second_output")" = 0

echo "native acceptance validation passed"
