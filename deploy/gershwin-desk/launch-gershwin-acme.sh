#!/usr/bin/with-contenv bash
# fontsrv + acme pair, once (no respawn: Exit means exit, user decision 2026-10)
export HOME=/home/e4779
export PLAN9=/opt/plan9port
export PATH="$PLAN9/bin:$PATH"
export DISPLAY="${DISPLAY:-:1}"
pkill -x acme 2>/dev/null; pkill -x fontsrv 2>/dev/null
"$PLAN9/bin/fontsrv" &
/bin/sleep 0.5
exec "$PLAN9/bin/acme"
