---
type: tool
title: plan9port — Plan 9 userspace для Unix, база акме-семейства
description: "Портирование план9-пользовательского пространства на Unix (Russ Cox): acme, sam, rc, plumber, 9p-клиент, fontsrv. Собирается in-place через ./INSTALL; источник — 9fans/plan9port."
resource: ../_sources/anvil-editor-research-2026-09.md
tags:
  - plan9port
  - acme
  - 9p
  - base
generated:
  by: human:e4779
  at: "2026-10-03T18:14:16Z"
---

# Overview

plan9port (Russ Cox, 9fans) — перенос план9-userland'а на Unix: acme, sam,
rc, plumber, fontsrv, 9p-клиент. Это **база акме-семейства**: тот acme,
которым пользуются на Linux/BSD/macOS, и тот контекст, от которого
отталкиваются все наследники (Edward, Anvil и др.).

# Details

- Собирается in-place: checkout = установочная директория; ./INSTALL
  (неинтерактивен), PLAN9 env, бинари в $PLAN9/bin, обёртка `9`.
- fontsrv транскодирует системные шрифты в plan9-subfonts — мост
  векторного мира в растровый acme.
- 9p-клиент: plan9port умеет ходить на 9p-серверы (фундамент для
  wikifs-rs и грида).

## Статус на hlab (2026-09-30)
- Источник: librarian-кэш ~/.cache/checkouts/github.com/9fans/plan9port.
- Сборка осознанно отложена (сузили масштаб до исходников). Когда
  понадобится графический acme: собирать в webtop-gershwin
  (/usr/local/plan9, deps: build-essential + libx11/libxext/libxft/
  libfontconfig-dev уже стоят), затем DISPLAY=:1 acme & — и он виден
  в браузере на порту 3002.

# See also

- [editor-rendering-stacks](editor-rendering-stacks.md)
- [acme-remote-editing](acme-remote-editing.md)
