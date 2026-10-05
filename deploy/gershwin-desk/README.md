# deploy/gershwin-desk — Gershwin (GNUstep) desktop + plan9port acme

Единственный рабочий стол на hlab (решение 2026-10-05: снесены webtop,
acme-web, desk, ad-hoc webtop-gershwin — четыре стека, ноль консистентности).
Стек: `gershwin-session → Xvfb :1 → Selkies → HTTPS` на базе
`lscr.io/linuxserver/webtop:debian-xfce`, порт 3009 (https 3010).

## Почему так

- Gershwin сам приносит WM (uROSWM), Workspace, Terminal, глобальное меню —
  не колхозим окружение вручную (урок labwc-пути desk).
- Gershwin и plan9port строятся В ОБРАЗ: gershwin-developer строго по
  upstream (clone → bootstrap.sh → checkout.sh → make install), plan9port —
  `./INSTALL`. Стол переживает рестарт без сети и без пересборок.
  Урок webtop-gershwin: рецепт, живущий только в образе, умирает вместе с ним.
- X-путь, а не PIXELFLUX_WAYLAND (как было в desk): gershwin-session/uROSWM —
  X11; Selkies снимает X-дисплей. Проверено живьём в webtop-gershwin 2026-09.
- Оверрайды сессии — runtime-маунты (паттерн desk): правка сессии без
  пересборки образа.

## Файлы

- `Containerfile` — образ `localhost/gershwin-desk:v1`: Gershwin + plan9port.
- `svc-de-run` — подменяет s6-сервис `svc-de`: gershwin-session
  (Workspace Menu WindowManager) на X-дисплее.
- `launch-gershwin-acme.sh` — fontsrv+acme одним запуском (без respawn,
  Exit = выход — решение пользователя).

## Масштаб и обои (HiDPI)

- Selkies в режиме HiDPI отдаёт физические пиксели клиента (X-экран = экрану
  браузера 1:1). UI Gershwin масштабируется через `GSScaleFactor` в defaults
  abc — пишется при старте сессии (svc-de-run), 2.0 = двойной масштаб.
  Fallback без него — dpi/144, при 96 dpi даёт 0.667 (вот почему «всё мелко»).
- Обои: GWDesktopView рисует их в логических пикселях (компенсирует scale
  factor), поэтому картинка должна быть нативного разрешения и со стилем
  ScaleStyle: `desktopinfo` → usebackimage=1, imagepath=/config/Wallpaper.jpg,
  imagestyle=3 (в defaults abc, /config volume).

## Разрешение запинено (важно!)

Селкиесу серверно запрещено менять размер экрана:
`SELKIES_MANUAL_WIDTH/HEIGHT=2560x1600` (quadlet). Причина: GWDesktopView не
умеет жить при живом RANDR-ресайзе — каждая переподключенная вкладка с другим
вьюпортом оставляла на руте мусор и ломала отрисовку (три раунда лечения
вотчером 2026-10-05). Статичный экран = класс багов исчез. Вотчер размера
остался как страховка. Захочешь другой размер — env в quadlet + restart.
Плата: на клиентах с другим аспектом — letterbox, апскейл ~1.15x.

## Сборка и развёртывание

```sh
doas podman build -t localhost/gershwin-desk:v1 deploy/gershwin-desk/  # долго: GNUstep
cd ~/projects/homelab && bash scripts/deploy.sh                         # quadlet → systemd
systemctl start gershwin-desk
```

Хост-артефакты: `/srv/homelab/gershwin-desk/` (config/, svc-de-run,
launch-gershwin-acme.sh, gershwin-desk.secret.env — пароль унаследован от
desk). Профили Chrome (`/srv/homelab/webtop/config/chrome-*-profile`, вкл.
AmneziaWG-проксируемый) подключим маунтом позже — браузеры добавляем после
того, как база устоится.
