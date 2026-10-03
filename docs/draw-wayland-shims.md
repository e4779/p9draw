---
type: concept
title: wl9 и wio — Wayland-шимы для draw-мира, и что показали 388 форков plan9port
description: Обзор красильного слоя план9-экосистемы на Wayland — wl9 (michaelforney, rio-wayland shim), wio (rio-подобный композитор); 388 форков plan9port = патч-стоянки, Rust-devdraw не существует.
tags:
  - wl9
  - wio
  - wayland
  - devdraw
  - draw-protocol
  - plan9port
generated:
  by: human:e4779
  at: "2026-10-03T18:14:16Z"
---

# Overview

Идея «заменить красильщик, не трогая клиента» уже доказана в C:
wl9 (michaelforney, «rio-wayland shim server», 22*) — шим-сервер,
позволяющий rio работать на Wayland, реализуя draw-протокол и
/dev/draw. wio (Rubo3, 35*) — rio-подобный Wayland-композитор
(философия, не протокол). Для acme и в Rust — не сделал никто.

# Details

## Форки plan9port (9fans/plan9port, 1961*, 388 форков)
- Топ по звёздам — персональные патч-ветки известных план9-людей:
  mariusae (9*), eaburns (6*, Go draw-биндинги), jxy (9front),
  rjkroege (автор Edwood), lufia. Обновляются годами.
- Ни один не строит новый paint-стек. Fork-культура = патч-стоянка.

## Поиски (2026-09-30)
- GitHub code/repo search: «devdraw rust», «libframe rust»,
  «draw protocol rust» — пусто (код-поиск даёт только ложные
  срабатывания по подстрокам).
- «plan9 wayland»: wl9, wio, mcpcpc/wlcatclock (кошачьи часы),
  biocini/pane (окружение на 9p-идеях).

## Значение для проекта Rust devdraw
- wl9 — готовый референс протокольной стороны (C): как принять
  draw-протокол и обслужить клиент под Wayland.
- Ниша «Rust devdraw для acme» полностью свободна; наша лестница
  (devdraw → libframe → acme) никем не занята, и у первого кирпича
  появился C-прецедент.

# See also

- [plan9port](plan9port.md) — исходники в librarian-кэше
- [editor-rendering-stacks](editor-rendering-stacks.md)
- [acre](acre.md) — протокольный подход на 9p-стороне
