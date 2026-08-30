#!/usr/bin/env bash
set -euo pipefail
output=${1:-artifacts/browser-ui.png}
free_port() { python3 - <<'PY'
import socket
with socket.socket() as sock:
    sock.bind(("127.0.0.1", 0)); print(sock.getsockname()[1])
PY
}
port=$(free_port); debug_port=$(free_port); profile=$(mktemp -d)
server_log=$(mktemp); browser_log=$(mktemp)
python3 scripts/serve.py --port "$port" --directory . --no-isolation >"$server_log" 2>&1 & server_pid=$!
chrome=${CHROME_BIN:-$(command -v google-chrome || command -v chromium || command -v chromium-browser)}
HOME="$profile" "$chrome" --headless --no-sandbox --disable-gpu --disable-crash-reporter \
 --user-data-dir="$profile/chromium" --window-size=1600,1000 \
 --remote-debugging-address=127.0.0.1 --remote-debugging-port="$debug_port" \
 "http://127.0.0.1:$port/web/open-multirealm-ui.html" >"$browser_log" 2>&1 & browser_pid=$!
cleanup(){
  status=$?; kill "$browser_pid" "$server_pid" 2>/dev/null || true
  wait "$browser_pid" "$server_pid" 2>/dev/null || true
  if [[ $status -ne 0 ]]; then cat "$browser_log" "$server_log" >&2; fi
  rm -rf "$profile" "$server_log" "$browser_log" 2>/dev/null || true
  exit "$status"
}
trap cleanup EXIT
for _ in $(seq 1 100); do
  curl -fsS "http://127.0.0.1:$debug_port/json/version" >/dev/null 2>&1 && break
  sleep 0.1
done
node scripts/cdp-screenshot.mjs "$debug_port" "$output" 20 282 222
[[ -s "$output" ]]
