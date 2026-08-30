#!/usr/bin/env bash
set -euo pipefail
output=${1:-artifacts}
mkdir -p "$output" target/perfetto-tool-home
cargo run --release -p tracing-bridge-example -- "$output/tracing-bridge.pftrace"
HOME="$PWD/target/perfetto-tool-home" scripts/trace_processor \
  query -f tests/sql/tracing.sql "$output/tracing-bridge.pftrace" \
  | tee "$output/tracing-query.txt"
grep -Fq '"compile graph through tracing",2' "$output/tracing-query.txt"
grep -Fq '"debug.nodes","123"' "$output/tracing-query.txt"
grep -Fq '"debug.load","0.625"' "$output/tracing-query.txt"
grep -Fq '"debug.success","true"' "$output/tracing-query.txt"
grep -Fq '"debug.phase","prepare"' "$output/tracing-query.txt"
grep -Fq '"debug.message","starting compile"' "$output/tracing-query.txt"
grep -Fq '"debug.message","queue pressure"' "$output/tracing-query.txt"
grep -Fq '"debug.target","audio"' "$output/tracing-query.txt"
grep -Fq '"debug.tracing_level","WARN"' "$output/tracing-query.txt"
test "$(tail -1 "$output/tracing-query.txt")" = 0
echo "tracing compatibility validation passed"
