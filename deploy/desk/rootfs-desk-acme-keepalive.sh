#!/bin/sh
# desk-acme-keepalive v2 — respawn when either process is gone;
# defunct zombies do not count (PID1 never reaps them).
alive() {
  ps -eo stat,comm | grep -E "^[^Z]" | grep -qw "$1"
}
# single instance guard
if [ -f /tmp/ka.pid ] && kill -0 "$(cat /tmp/ka.pid)" 2>/dev/null; then
  exit 0
fi
echo $$ > /tmp/ka.pid

while :; do
  if ! alive acme || ! alive p9draw-server; then
    /usr/local/bin/launch-desk-acme.sh
  fi
  sleep 8
done
