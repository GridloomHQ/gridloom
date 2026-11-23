#!/usr/bin/env bash
# Window-open smoke only. This does not automate or certify native interactions.
set -euo pipefail
cd "$(dirname "$0")/.."
mkdir -p target/native-smoke
log=target/native-smoke/window.log
set +e
if [[ -z "${DISPLAY:-}${WAYLAND_DISPLAY:-}" ]]; then
  command -v xvfb-run >/dev/null || { echo 'Native smoke needs a display or xvfb-run.' >&2; exit 2; }
  command -v dbus-run-session >/dev/null || { echo 'Headless native smoke needs dbus-run-session.' >&2; exit 2; }
  # Xvfb has no GPU/session bus. Keep this software-rendering choice confined to
  # the headless test, not the normal application. See tauri-apps/tauri#15936.
  dbus-run-session -- xvfb-run -a env GDK_BACKEND=x11 LIBGL_ALWAYS_SOFTWARE=1 WEBKIT_DISABLE_COMPOSITING_MODE=1 timeout --kill-after=5s --signal=TERM 45s target/debug/gridloom-desktop > "$log" 2>&1
else
  timeout --kill-after=5s --signal=TERM 45s target/debug/gridloom-desktop > "$log" 2>&1
fi
status=$?
set -e
cat "$log"
[[ "$status" = 124 ]] || { echo "Native process exited unexpectedly ($status)." >&2; exit 1; }
grep -q 'Tayriva desktop window opened.' "$log" || { echo 'No native window-open event observed.' >&2; exit 1; }
echo 'PASS: native process served frontend and emitted a window-open event; interactive native UI not exercised.'
