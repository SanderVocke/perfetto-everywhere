#!/usr/bin/env bash
set -euo pipefail
output=${1:-artifacts}
mkdir -p "$output" target/perfetto-tool-home
cargo run --release -p perfetto-everywhere-collector --example clock-snapshot \
  -- "$output/collector-clock.pftrace"
HOME="$PWD/target/perfetto-tool-home" scripts/trace_processor \
  query -f tests/sql/collector.sql "$output/collector-clock.pftrace" \
  | tee "$output/collector-query.txt"
grep -Fq '6,4,1000000000,1000002499' "$output/collector-query.txt"
grep -Fq '64,4,1000,4000' "$output/collector-query.txt"
grep -Fq '"clocked task",2,1000000500,1000000999,2000,2000' "$output/collector-query.txt"
grep -Fq '"page/clocked counter",1,0.100000,0.100000' "$output/collector-query.txt"
grep -Fq '"worker/clocked counter",1,0.200000,0.200000' "$output/collector-query.txt"
test "$(grep -c '^1$' "$output/collector-query.txt")" -ge 1
test "$(grep -c '^0$' "$output/collector-query.txt")" -ge 2
echo "collector and clock-snapshot validation passed"
