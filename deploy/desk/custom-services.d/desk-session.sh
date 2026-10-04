#!/bin/sh
# desk-session: ONE labwc (wayland-1, captured by Selkies) + the acme pair.
# If the compositor dies, the whole stack restarts. No other supervision.

while :; do
  # cleanup leftovers from the previous iteration
  pkill -x labwc 2>/dev/null
  pkill -x acme 2>/dev/null
  pkill -x p9draw-server 2>/dev/null
  pkill -x 9pserve 2>/dev/null
  pkill -x fontsrv 2>/dev/null
  rm -f /config/.XDG/wayland-0.lock
  touch /config/.XDG/wayland-0.lock

  # hold wayland-0 so labwc binds wayland-1 (the Selkies capture display)
  flock -n /config/.XDG/wayland-0.lock -c 'sleep infinity' &
  HOLDER=$!

  mkdir -p /config/.XDG /tmp/serve
  (
    export XDG_RUNTIME_DIR=/config/.XDG
    unset DISPLAY WAYLAND_DISPLAY
    exec s6-setuidgid abc labwc -C /config/.config/labwc/rc.xml
  ) &
  LABWC=$!

  # wait for the compositor socket
  for i in $(seq 1 40); do
    [ -S /config/.XDG/wayland-1 ] && break
    sleep 0.5
  done

  # fontsrv
  (
    export XDG_RUNTIME_DIR=/config/.XDG
    export PLAN9=/config/plan9port NAMESPACE=/tmp/ns.acme.desk
    mkdir -p "$NAMESPACE"
    exec s6-setuidgid abc "$PLAN9/bin/9" fontsrv
  ) >/dev/null 2>&1 &
  sleep 3

  # acme -> p9draw
  (
    export XDG_RUNTIME_DIR=/config/.XDG WAYLAND_DISPLAY=wayland-1 HOME=/home/e4779
    unset DISPLAY
    export PLAN9=/config/plan9port NAMESPACE=/tmp/ns.acme.desk
    P9DRAW_SERVE=1 P9DRAW_STATS=1 s6-setuidgid abc "$PLAN9/bin/9" acme -f /mnt/font/DejaVuSansMono/16a/font >>/tmp/serve/acme-desk.err 2>&1 </dev/null
  ) &

  # while the compositor lives, hold the wayland-0 lock and wait
  while kill -0 $LABWC 2>/dev/null; do sleep 5; done
  kill $HOLDER 2>/dev/null
done
