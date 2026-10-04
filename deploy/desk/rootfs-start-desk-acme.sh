#!/bin/sh
# start-desk-acme: start fontsrv + acme + p9draw pair (no kill semantics).
export PLAN9=/config/plan9port
export NAMESPACE=/tmp/ns.acme.desk
mkdir -p "$NAMESPACE" /tmp/serve
XDG_RUNTIME_DIR=/config/.XDG setsid "$PLAN9/bin/9" fontsrv >/dev/null 2>&1 &
sleep 3
unset DISPLAY
P9DRAW_SERVE=1 P9DRAW_STATS=1 \
  XDG_RUNTIME_DIR=/config/.XDG WAYLAND_DISPLAY=wayland-1 \
  NAMESPACE="$NAMESPACE" \
  setsid "$PLAN9/bin/9" acme -f /mnt/font/DejaVuSansMono/16a/font \
  >>/tmp/serve/acme-desk.err 2>&1 </dev/null &
