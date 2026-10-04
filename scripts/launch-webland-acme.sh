#!/bin/sh
# launch-webland-acme — canonical launcher for the acme+p9draw pair inside the
# webland-test container (native Wayland client of the webland compositor).
#
# Install once (persists across container restarts):
#   doas podman cp scripts/launch-webland-acme.sh webland-test:/usr/local/bin/
#
# Traps documented in docs/roadmap-devdraw.md:
#   - DISPLAY must be UNSET (leaked DISPLAY silently selects X11);
#   - XDG_RUNTIME_DIR=/run/webland-runtime, WAYLAND_DISPLAY=wayland-1;
#   - NOLIBTHREADDAEMONIZE must NOT be set (kills 9pserve via libthread sysfatal);
#   - fontsrv before acme; -f /mnt/font/DejaVuSansMono/16a/font.
#
# Keepalive (respawns the pair if either acme or p9draw-server is not alive):
#   doas podman exec -d webland-test /usr/local/bin/webland-acme-keepalive.sh

pkill -x p9draw-server 2>/dev/null; pkill -x 9pserve 2>/dev/null; pkill -x fontsrv 2>/dev/null
sleep 1
export PLAN9=/opt/plan9port
export NAMESPACE=/tmp/ns.acme.webland
mkdir -p "$NAMESPACE" /tmp/serve
XDG_RUNTIME_DIR=/run/webland-runtime setsid "$PLAN9/bin/9" fontsrv >/dev/null 2>&1 &
sleep 3
unset DISPLAY
P9DRAW_SERVE=1 P9DRAW_STATS=1 WINSIZE=1800x1400 \
  XDG_RUNTIME_DIR=/run/webland-runtime WAYLAND_DISPLAY=wayland-1 \
  NAMESPACE="$NAMESPACE" \
  setsid "$PLAN9/bin/9" acme -f /mnt/font/DejaVuSansMono/16a/font \
  >>/tmp/serve/acme-webland.err 2>&1 </dev/null &
