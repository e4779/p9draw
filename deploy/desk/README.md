# deploy/desk — контейнерный стол (labwc + acme + terminal + browser)

Стек: `acme → p9draw → labwc → Selkies → HTTPS` внутри одного контейнера
на базе `lscr.io/linuxserver/webtop:debian-xfce` (Selkies/s6/pulse plumbing
образа, десктоп заменён на labwc).

## Файлы

- `Containerfile` — образ `localhost/desk:v1` (labwc, foot, chromium; см. `podman build`).
- `rootfs-svc-de-run` — ПЕРЕКРЫВАЕТ s6-сервис `svc-de` образа: сессия = labwc как
  ВЛОЖЕННЫЙ композитор-клиент на сцене захвата Selkies. Больше ничего: acme
  стартует один раз через autostart labwc.
- `rootfs-launch-desk-acme.sh` — запуск пары fontsrv+acme+p9draw (меню → Acme;
  kill-and-start, без супервизии — Exit = выход, по решению пользователя).
- `rootfs-cont-init-40-desk-config.sh` — пишет rc.xml/menu.xml/autostart в /config (wayland-0).
- `rootfs-cont-init-41-agent-links.sh` — шимы `pi`/`bun` из смонтированного `/home/e4779`.

## Сборка и развёртывание

```sh
podman build -t localhost/desk:v1 deploy/desk/          # образ
cd ~/projects/homelab && bash scripts/deploy.sh          # quadlet → /etc/containers/systemd + daemon-reload
systemctl restart desk                                   # юнит: homelab/quadlet/desk.container
```

Секреты: `/srv/homelab/desk/desk.secret.env` (PASSWORD), в git не попадают.

## Инварианты (проверено приёмкой 2026-10-04)

- Один супервизор сессии: `svc-de` = labwc. Никаких custom-services и respawn-циклов.
- Архитектура дисплеев: Selkies (pixelflux) владеет **wayland-1** — это сцена
  захвата; labwc стартует вложенным клиентом (`WAYLAND_DISPLAY=wayland-1`) и
  публикует свой сокет **wayland-0** для приложений; ввод Selkies роутит на
  wayland-0. Запускать labwc headless на wayland-0 нельзя — capture снимает
  пустую сцену (чёрный кадр, стрим замирает).
- В этом build labwc `-C <файл>` отключает autostart-поиск — конфиг держать в
  дефолтном пути.
- `PUID=1000` обязателен (иначе uid 911 образа ломает маунты ~/.bun).
- Сеть `systemd-homelab` — дефолтная podman-сеть теряет LAN-маршрут.
- Секреты не в git; история — PLAN.md и docs/remote-desktop-three-ways.md.
