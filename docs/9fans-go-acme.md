---
type: tool
title: 9fans/go cmd/acme — Go-транслитерация оригинального acme
description: "Порт acme на Go внутри 9fans/go: дословная транслитерация C-исходников (dat.h→dat.h.go, xfid.c→xfid.go, wind, exec, edit, regx, disk, dump), bigLock event loop, draw-пакет как libdraw. Замер в декабре 2021; родословная ProjectSerenity → 9fans/go → Edwood."
tags:
  - go
  - acme
  - port
  - lineage
generated:
  by: human:e4779
  at: "2026-10-03T18:14:16Z"
---

# Overview

cmd/acme в репо 9fans/go — «акме в переводе»: не реворк, а структурная
транслитерация оригинальных C-файлов plan9port. Имена говорят сами за
себя: dat.h.go, xfid.go, fsys1.go, look1.go.

# Details

- internal/-пакеты 1:1 с C-файлами: adraw (draw), bufs (buf), disk, dump,
  edit (sam-команды), exec (исполнение тегов), fileload, regx, runes
  (Rune-тип), wind (Row/Column/Window), ui, util.
- bigLock() в main — однопоточный event loop оригинала сохранён.
- Рендер: 9fans.net/go/draw — Go-биндинг libdraw (растровая модель).
- Замер: последние коммиты декабрь 2021.

## Родословная (из README Edwood)
plan9 acme (C) → plan9port acme (C, devdraw)
→ ProjectSerenity/acme (транслитерация C→Go)
→ 9fans/go cmd/acme → rjkroege/edwood (форк, дивергирует).

## Важный факт
Edwood до сих пор требует инфраструктуру plan9port: devdraw, 9pserve,
fontsrv (растровая draw-модель — несущая стена порта). Чисто-Go режим
(duitdraw/mux9p) — экспериментальные теги. На build для Plan 9 —
9fans.net/go PR#28.

# See also

- [editor-rendering-stacks](editor-rendering-stacks.md)
- [ad-editor](ad-editor.md)
