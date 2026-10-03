#!/bin/sh
# launch-wayland-serve - start the serve-acme set with a NATIVE Wayland window.
#
# Deploy (the canonical copy lives here; inside acme-web it is /tmp/launch-wayland.sh):
#   doas podman cp scripts/launch-wayland-serve.sh acme-web:/tmp/launch-wayland.sh
#   doas podman exec acme-web /tmp/launch-wayland.sh
#
# Why: winit 0.30 prefers Wayland whenever WAYLAND_DISPLAY resolves. The only
# non-obvious bits are (a) DISPLAY must be *unset*, not empty -- a stray
# DISPLAY leaks from the podman-exec client env through su and silently
# restores the X11 (XWayland) backend; (b) the session runs at
# XDG_RUNTIME_DIR=/config/.XDG (capital .XDG, not the conventional .xdg).
#
# Verification recipe (see README, Wayland-native serve window):
#   - xwininfo -root -tree must NOT list a p9draw-server window;
#   - the serve process must hold a connected socket to /config/.XDG/wayland-1;
#   - WAYLAND_DEBUG=1 on the acme env exposes the xdg_toplevel wire trace.
su abc -s /bin/sh /tmp/killserve-abc.sh
sleep 1
NAMESPACE=/tmp/ns.abc.serve setsid /config/plan9port/bin/9 fontsrv >/dev/null 2>&1 &
sleep 2
su abc -s /bin/sh -c "unset DISPLAY; P9DRAW_SERVE=1 P9DRAW_STATS=1 WINSIZE=1800x1400 NAMESPACE=/tmp/ns.abc.serve XDG_RUNTIME_DIR=/config/.XDG WAYLAND_DISPLAY=wayland-1 PIXELFLUX_WAYLAND=true setsid /config/plan9port/bin/9 acme -f /mnt/font/DejaVuSansMono/16a/font >/tmp/serve/acme-wayland.stderr 2>&1 </dev/null &"
sleep 18
exit 0
