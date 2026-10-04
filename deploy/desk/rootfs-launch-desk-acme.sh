#!/bin/sh
# desk acme launcher — adapted from scripts/launch-wayland-serve.sh for labwc.
pkill -x p9draw-server 2>/dev/null; pkill -x 9pserve 2>/dev/null; pkill -x fontsrv 2>/dev/null
sleep 1
export PLAN9=/config/plan9port
export NAMESPACE=/tmp/ns.acme.desk
mkdir -p "$NAMESPACE" /tmp/serve
XDG_RUNTIME_DIR=/config/.XDG setsid "$PLAN9/bin/9" fontsrv >/dev/null 2>&1 &
sleep 3
unset DISPLAY
P9DRAW_SERVE=1 P9DRAW_STATS=1 WINSIZE=1600x1000 \
  XDG_RUNTIME_DIR=/config/.XDG WAYLAND_DISPLAY=wayland-1 \
  NAMESPACE="$NAMESPACE" \
  setsid "$PLAN9/bin/9" acme -f /mnt/font/DejaVuSansMono/16a/font \
  >>/tmp/serve/acme-desk.err 2>&1 </dev/null &
