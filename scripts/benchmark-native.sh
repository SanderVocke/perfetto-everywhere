#!/usr/bin/env bash
set -euo pipefail
output=${1:-artifacts}
iterations=${ITERATIONS:-100000}
repetitions=${REPETITIONS:-3}
mkdir -p "$output"
cargo build --release -p native-capture-example --bin native-overhead
raw="$output/native-overhead-raw.txt"
time_bin=$(type -P time)
: > "$raw"
for mode in disabled inactive active; do
  for repetition in $(seq 1 "$repetitions"); do
    echo "run mode=$mode repetition=$repetition" >&2
    resource=$(mktemp)
    measurement=$("$time_bin" -f 'user_s=%U system_s=%S max_rss_kib=%M' -o "$resource" \
      target/release/native-overhead "$mode" "$iterations")
    printf '%s repetition=%s ' "$measurement" "$repetition" >> "$raw"
    cat "$resource" >> "$raw"
    rm -f "$resource"
  done
done
python3 - "$raw" "$output/native-overhead.md" <<'PY'
import statistics, sys
raw, output = sys.argv[1:]
rows=[]
for line in open(raw):
    values=dict(item.split('=',1) for item in line.split())
    if 'elapsed_ns' not in values: continue
    rows.append(values)
with open(output,'w') as f:
    f.write('# Native overhead sanity check\n\n')
    f.write('Three release-mode processes per mode; each executes the same arithmetic loop. '
            '`inactive` calls the facade without a capture and `active` records one instant per iteration. '
            'Initialization and trace readout are outside `elapsed_ns`; process CPU/RSS include them. '
            'This is a regression sanity check, not an AudioWorklet or hardware-independent claim.\n\n')
    f.write('| Mode | Median workload ms | Iterations/s | Trace bytes | User s | System s | Peak RSS KiB |\n')
    f.write('|---|---:|---:|---:|---:|---:|---:|\n')
    for mode in ('disabled','inactive','active'):
        selected=[r for r in rows if r['mode']==mode]
        elapsed=statistics.median(int(r['elapsed_ns']) for r in selected)
        iterations=int(selected[0]['iterations'])
        f.write(f"| {mode} | {elapsed/1e6:.3f} | {iterations/(elapsed/1e9):.0f} | "
                f"{statistics.median(int(r['trace_bytes']) for r in selected):.0f} | "
                f"{statistics.median(float(r['user_s']) for r in selected):.3f} | "
                f"{statistics.median(float(r['system_s']) for r in selected):.3f} | "
                f"{statistics.median(int(r['max_rss_kib']) for r in selected):.0f} |\n")
print(open(output).read())
PY
