---
type: research-synthesis
title: Рендер-стеки современных редакторов
description: Кто на чём рисует — JetBrains (Swing vs Fleet/Skiko), VSCode (Electron DOM + WebGL2), Zed (GPUI Blade→wgpu), Anvil (Gioui), acme (libdraw/subfonts); таблица «что менять в acme на что» по шести слоям.
tags:
  - rendering
  - skia
  - gpui
  - webgpu
  - editors
generated:
  by: human:e4779
  at: "2026-09-30T07:09:16Z"
---

# Overview

Редактор — шесть слоёв. Acme 1989 определён на всех шести; современная
замена — по слоям, скелет переносится целиком.

# Details

## Кто на чём рисует (проверено 2026-09-30)

- JetBrains: классические IDEA/PyCharm — Swing/Java2D. Fleet — **Skiko
  (Skia for Kotlin)**: тонкий фронтенд рисует Skia'ой, интеллект — в
  отдельном JVM-бэкенде, protobuf-протокол, бэкенд может быть удалённым.
- VSCode: Electron/Chromium; Monaco рендерит строки в DOM (спаны),
  с 2024–2025 экспериментальный GPU-путь через WebGL2 (атласы глифов).
- Zed: свой GPUI — Metal на macOS, Blade (мигрирует на wgpu) на Linux;
  текст растеризуется в GPU-атласы. Форк gpui-ce выделен из кодовой базы.
- Anvil: Gioui (immediate-mode, Go).
- Acme: libdraw/bitblt, subfonts через fontsrv-транскодер.

## Таблица замены по слоям

| слой | acme 1989 | современная замена |
|---|---|---|
| рендер | libdraw bitblt | GPU: Skia / GPUI / WebGPU |
| шрифты | subfonts + fontsrv | HarfBuzz / cosmic-text / parley; системные фолбэки |
| окна | acme сам себе WM | делегировать композитору (Edward-теза) |
| дистанция | 9p-экспорт ФС | anvil-ssh per-window / Fleet-split / per-surface WebCodecs |
| API расширений | fs (/mnt/acme, 9p) | НЕ менять; транспорт — 9p+FUSE или REST |
| ввод | трёхкнопочные аккорды | клавиатурные биндинги + мышь-первоклассник + IME |

Константы (не меняются ни в одной клетке): selection-as-command,
per-window контекст, делегирование шеллу, fs-API как принцип.

# See also

- [fleet-split-pattern](fleet-split-pattern.md)
- [anvil](anvil.md)
