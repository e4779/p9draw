# Remote desktop — три подхода (фиксация 2026-10-04)

Контекст: цель — «графический терминал» на homelab: acme + терминал + браузер +
вьюверы + shell до системы (вызов агента). Доступ из дома/с работы, позже с телефона.

## 1. waypipe — окна у тебя локально (работает, dev-режим)

Контейнер `waypipe-test` (debian:trixie): sway + foot + plan9port.

```sh
waypipe ssh -o PubkeyAuthentication=no -p 2222 demo@192.168.1.3 env WLR_BACKENDS=wayland sway
```

- Плюсы: per-window (стримятся окна, не экран), ноль энкодера на сервере, простой ssh-транспорт, вкладка браузера не нужна. Проверено 29–30.09 и живо сейчас.
- Минусы: нужен Wayland-сеанс и waypipe НА КЛИЕНТЕ (WSL — ок, рабочий комп — зависит, телефон — нет). Не «десктоп в браузере».
- Вердикт: **держим как dev-режим**, инвестиций ноль. Документировать команды подключения.

## 2. webland / браузерный композитор — R&D (заморожено с багом)

Собственный деплой (husseinhareb/webland, контейнер `webland-test`, :3030): sway/acme
как Wayland-клиенты, композитор кодирует per-window H.264 (VA-API) → WebCodecs.

- Плюсы: пер-окно стриминг в браузере — идеологически самый «правильный»
  (стримим команды/окна, не экран); наш p9draw — ванильный клиент.
- Минусы/открытое: декод для клиента, присоединившегося mid-stream, не работает
  (RequestKeyframe consumed, idle surfaces skipped — патч
  `scripts/webland-keyframe-on-join.patch` применён; остаются idle-loop и
  decode-path гипотезы). Сырость апстрима.
- Вердикт: **заморожено как R&D**; вернуться фоновым debugging-раундом или
  апстрим-issue с собранными уликами.

## 3. Selkies webtop — основной трек (работает сегодня)

Контейнер `acme-web` (lscr.io/linuxserver/webtop:ubuntu-kde, :3005): KWin + Selkies
(websockets), acme через p9draw нативным Wayland-клиентом; рядом эталонный
devdraw-acme на XWayland. Стек: `acme → drawfcall → p9draw → Wayland → Selkies → браузер`.

- Плюсы: работает сейчас, через Keenetic-https доступен снаружи; вся обвязка
  (s6, pulse, Selkies) доказана.
- Минусы: KDE тяжёлый; полный стол вместо per-window; ubuntu-base.
- Вердикт: **основной трек** — пересобрать лёгкий вариант: base
  `webtop:debian-xfce` (уже на диске), сессия → **sway**, внутрь alacritty +
  chrome + plan9port/p9draw + ssh→hlab. Новый порт (3007), quadlet, keepalive.
  После стабилизации :3005 уходит (пользователь: «мне он не нужен»).

## Связанное (отложено)

- **draw-canvas / drawterm-в-браузере** на p9draw (jsdrawterm/9webdraw школа) —
  p9draw-render wasm-ready; дверь открыта после стабилизации основного.
- **prio (Arcan, rio-стиль)** + wio (Wayland, rio-стиль) — эстетические
  эксперименты в духе Plan 9; не кандидаты в основной стол.
