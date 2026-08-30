#!/usr/bin/env bash
set -euo pipefail
output=${1:-artifacts}
mkdir -p "$output"
scripts/build-browser-tests.sh
free_port() {
  python3 - <<'PY'
import socket
with socket.socket() as sock:
    sock.bind(("127.0.0.1", 0))
    print(sock.getsockname()[1])
PY
}
port=${PORT:-$(free_port)}
debug_port=$(free_port)
profile=$(mktemp -d)
server_log=$(mktemp)
browser_log=$(mktemp)
python3 scripts/serve.py --port "$port" >"$server_log" 2>&1 &
server_pid=$!
chrome=${CHROME_BIN:-}
if [[ -z "$chrome" ]]; then
  chrome=$(command -v google-chrome || command -v chromium || command -v chromium-browser)
fi
HOME="$profile" "$chrome" \
  --headless --no-sandbox --disable-gpu --disable-dev-shm-usage --disable-crash-reporter \
  --user-data-dir="$profile/chromium" --remote-debugging-address=127.0.0.1 \
  --remote-debugging-port="$debug_port" \
  "http://127.0.0.1:$port/ordinary-browser-test.html" >"$browser_log" 2>&1 &
browser_pid=$!
cleanup() {
  status=$?
  kill "$browser_pid" "$server_pid" 2>/dev/null || true
  wait "$browser_pid" "$server_pid" 2>/dev/null || true
  if [[ $status -ne 0 ]]; then
    echo "--- server log ---" >&2
    cat "$server_log" >&2
    echo "--- browser log ---" >&2
    cat "$browser_log" >&2
  fi
  rm -rf "$profile" "$server_log" "$browser_log" 2>/dev/null || true
  exit "$status"
}
trap cleanup EXIT
node tests/browser/collect-ordinary.mjs "$debug_port" "$output/ordinary-browser.json"
