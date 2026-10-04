#!/bin/sh
# run-desk.sh — start the desk container (rootful, like acme-web).
# Build first:  podman build -t localhost/desk:v1 -f deploy/desk/Containerfile deploy/desk
mkdir -p /srv/homelab/desk/config /srv/homelab/desk/bin
podman run -d --name desk \
  -p 3007:3000 -p 3008:3001 \
  -e CUSTOM_USER=e4779 -e PASSWORD=09674390ce80340b9e52 \
  -e TITLE=desk -e SELKIES_ENCODER=x264enc,jpeg \
  -e PIXELFLUX_WAYLAND=true -e DISPLAY= \
  -v /srv/homelab/desk/config:/config \
  -v /srv/homelab/acme-web/plan9port:/config/plan9port \
  -v /srv/homelab/desk/bin/p9draw-server:/usr/local/bin/p9draw-server:ro \
  -v /srv/homelab/desk/rootfs-svc-de-run:/etc/s6-overlay/s6-rc.d/svc-de/run:ro \
  -v /srv/homelab/desk/rootfs-labwc-rc.xml:/config/.config/labwc/rc.xml:ro \
  -v /srv/homelab/desk/rootfs-labwc-autostart:/config/.config/labwc/autostart:ro \
  -v /srv/homelab/desk/rootfs-launch-desk-acme.sh:/usr/local/bin/launch-desk-acme.sh:ro \
  -v /srv/homelab/desk/rootfs-desk-acme-keepalive.sh:/usr/local/bin/desk-acme-keepalive.sh:ro \
  -v /srv/homelab/desk/custom-cont-init.d:/custom-cont-init.d \
  -v /home/e4779/.bun:/home/abc/.bun \
  -v /home/e4779/.pi:/home/abc/.pi \
  localhost/desk:v1
