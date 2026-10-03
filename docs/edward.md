---
type: tool
title: Edward — форк Edwood без оконного менеджера (каждое окно = окно ОС)
description: "Экспериментальный форк Edwood (Go, сама транслитерационная линейка acme): убирает WM из редактора, каждое acme-окно становится отдельным окном ОС; тайтлбары отдаёт тайлинговому WM. POC, нестабилен."
resource: ../_sources/anvil-editor-research-2026-09.md
tags:
  - edward
  - edwood
  - acme
  - go
  - wayland
generated:
  by: human:e4779
  at: "2026-10-03T18:14:16Z"
---

# Overview

Edward (github.com/fhs/edward) — экспериментальный форк Edwood: «which
removes window management. Each window in Edward uses a separate window
in the OS windowing system, leaving window management entirely to the
OS-native window manager». На Xorg/Wayland с тайлинговым WM (i3, sway,
dwm, wmii) даёт опыт, близкий к традиционному acme. В репо лежит
нетронутый README-edwood.md родителя.

# Details

## Родословная (документально)
plan9 acme (C) → plan9port → ProjectSerenity/acme (транслитерация C→Go)
→ 9fans/go cmd/acme → Edwood → **Edward**. Файлы — та же
транслитерационная анатомия: acme.go, col.go, row.go, wind.go, text.go,
dat.go, disk.go, exec.go, edit.go, regx.go, xfid.go, fsys.go; варианты
acme_p9p/plan9/unix/windows.go под билд-теги.

## Стек (go.mod)
go 1.11; 9fans.net/go (снапшот 2018!); fhs/mux9p (9p-мультиплексор
автора); ktye/duitdraw (чисто-Go порт plan9 draw — рендер без
plan9port). Для перевода на современный рендер — заменить draw-слой.

## Идея в контексте
Edward = per-window drawterm на уровне редактора: каждое окно acme —
отдельная сущность ОС, раскладку делает WM (i3/sway), мульти-воркспейсы
бесплатно. Та же мысль, что per-surface streaming (webland), но на X11
и без кодеков. POC: «more unstable than Edwood».

# See also

- [9fans-go-acme](9fans-go-acme.md) — родительская транслитерация
- [acme-remote-editing](acme-remote-editing.md)
- [fleet-split-pattern](fleet-split-pattern.md)
