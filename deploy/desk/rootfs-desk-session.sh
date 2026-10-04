#!/bin/sh
# desk-session (s6 longrun): ONE labwc on wayland-1 (what Selkies captures)
# + acme pair supervisor.

# orphans cleanup (survive s6 restarts via dbus-run-session)
pkill -x labwc 2>/dev/null
pkill -x acme 2>/dev/null
pkill -x p9draw-server 2>/dev/null
pkill -x 9pserve 2>/dev/null
pkill -x fontsrv 2>/dev/null
sleep 1
mkdir -p /config/.XDG
chown -R abc:abc /config/.XDG

# hold wayland-0 so labwc binds wayland-1 (the Selkies capture display).
# The holder is OUR child: it lives as long as this service does.
touch /config/.XDG/wayland-0.lock
flock -n /config/.XDG/wayland-0.lock -c 'sleep infinity' &
HOLDER=$!

# compositor: skip wayland-0 (locked) -> binds wayland-1
(
  export XDG_RUNTIME_DIR=/config/.XDG
  unset DISPLAY WAYLAND_DISPLAY
  exec s6-setuidgid abc labwc -C /config/.config/labwc/rc.xml
) &
LABWC=$!

# wait for the wayland-1 socket
for i in $(seq 1 40); do
  [ -S /config/.XDG/wayland-1 ] && break
  sleep 0.5
done

# acme supervisor: start when dead; stop if the compositor dies
while kill -0 $LABWC 2>/dev/null; do
  s6-setuidgid abc /usr/local/bin/start-desk-acme-alive || \
    s6-setuidgid abc /usr/local/bin/start-desk-acme.sh
  sleep 8
done
kill $HOLDER 2>/dev/null
