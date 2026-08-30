#!/usr/bin/env bash
# Complete release-candidate regeneration. Expected runtime is several minutes.
set -euo pipefail
scripts/check.sh
AUDIO_DURATION_MS=60000 scripts/run-audio-browser.sh artifacts
AUDIO_REQUIRE_FULL=1 scripts/validate-audio.sh artifacts
echo "complete perfetto-everywhere release validation passed"
