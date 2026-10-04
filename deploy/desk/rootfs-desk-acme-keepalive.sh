#!/bin/sh
# desk-acme-keepalive v2 — respawn when either process is gone;
# defunct zombies do not count (PID1 never reaps them).
alive() {
  ps -eo stat,comm | grep -E "^[^Z]" | grep -qw "$1"
}
while :; do
  if ! alive acme || ! alive p9draw-server; then
    /usr/local/bin/launch-desk-acme.sh
  fi
  sleep 8
done
