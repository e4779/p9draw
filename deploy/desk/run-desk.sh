#!/bin/sh
# run-desk.sh — start the desk container (rootful; supervises its own session).
mkdir -p /srv/homelab/desk/config /srv/homelab/desk/bin
podman run -d --name desk \
  --network systemd-homelab \
  -p 3007:3000 -p 3008:3001 \
  -e CUSTOM_USER=e4779 -e PASSWORD="$(cat /srv/homelab/desk/desk.secret)" \
  -e PUID=1000 -e PGID=1000 \
  -e TITLE=desk -e SELKIES_ENCODER=x264enc,jpeg \
  -e PIXELFLUX_WAYLAND=true -e DISPLAY= \
  -v /srv/homelab/desk/config:/config \
  -v /srv/homelab/acme-web/plan9port:/config/plan9port \
  -v /srv/homelab/desk/bin/p9draw-server:/usr/local/bin/p9draw-server:ro \
  -v /srv/homelab/desk/custom-cont-init.d:/custom-cont-init.d:ro \
  -v /srv/homelab/desk/custom-services.d:/custom-services.d:ro \
  -v /home/e4779:/home/e4779 \
  localhost/desk:v1
