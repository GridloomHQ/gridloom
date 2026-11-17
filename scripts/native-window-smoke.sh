#!/usr/bin/env bash
# Window-open smoke only. This does not automate or certify native interactions.
set -euo pipefail
cd "$(dirname "$0")/.."
mkdir -p target/native-smoke
log=target/native-smoke/window.log
set +e
if [[ -z "${DISPLAY:-}${WAYLAND_DISPLAY:-}" ]]; then
  command -v xvfb-run >/dev/null || { echo 'Native smoke needs a display or xvfb-run.' >&2; exit 2; }
  xvfb-run -a timeout --signal=TERM 12s target/debug/gridloom-desktop --no-samples > "$log" 2>&1
else
  timeout --signal=TERM 12s target/debug/gridloom-desktop --no-samples > "$log" 2>&1
fi
status=$?
set -e
cat "$log"
[[ "$status" = 124 ]] || { echo "Native process exited unexpectedly ($status)." >&2; exit 1; }
grep -q 'Gridloom desktop window opened.' "$log" || { echo 'No native window-open event observed.' >&2; exit 1; }
echo 'PASS: native process served frontend and emitted a window-open event; interactive native UI not exercised.'
