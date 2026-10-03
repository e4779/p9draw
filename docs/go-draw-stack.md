---
type: concept
title: "Go-стек для плана9-графики: Edwood-мозг, Gio-краска, пустота в draw-шимах"
description: Инвентаризация Go-ассетов по слоям — 9fans/go + acme-lsp (живой LSP для acme, 232*), Edwood (445*, единственный полный транслит мозга acme), go-text/typesetting (чистый Go шейпинг), Gio (pure-Go Wayland+wasm); draw-протокол шимов в Go НЕТ — все ездят на C devdraw.
tags:
  - go
  - edwood
  - gio
  - go-text
  - acme-lsp
  - 9fans-go
  - draw-protocol
generated:
  by: human:e4779
  at: "2026-10-03T18:14:16Z"
---

# Overview

Go-мир план9 сильнее Rust-мира в мозгах и протоколах, и слабее
в краске. Контрольный слой — лидер: 9fans/go (rsc) + acme-lsp
(232*, живой, org 9fans) — Go-ответ acre, не мёртв. Мозг acme —
Edwood (445*, rjkroege) — единственный полный транслит. Краска —
пусто: Edwood ездит на C devdraw через 9fans/go/draw; единственный
draw-шим в мире — wl9 (C).

# Details

## Инвентаризация по слоям (2026-09-30)
- 9p/контроль: 9fans/go (dial, acme, plumb), acme-lsp (Go, 232*,
  9fans org) — живой LSP-мост (аналог мёртвого acre).
- Мозг acme: rjkroege/edwood, Go, 445* — полный транслит, MIT.
- Раскладка: internal/frame внутри edwood (транслит libframe,
  строко-квантованная); go-text/typesetting + go-text/render —
  чисто-Go OpenType шейпинг из экосистемы Gio.
- Краска: НИЧЕГО нативного. Gio (git.sr.ht/~eliasnaur/gio) —
  pure-Go immediate-mode GUI: нативный Wayland, без cgo, собирается
  в js/wasm (браузер) из коробки. giocanvas (153*) — канвас на Gio.

## Значение для лестницы (devdraw → frame → acme)
- В Go готово всё, кроме кирпича 1: мозг (Edwood) + протоколы
  (9fans/go) + шейпинг (go-text) + краска-платформа (Gio).
- «acme в браузере» через Gio = deployment target, а не самодельный
  WebCodecs-пламбинг.
- Go-вариант культурно апстримибелен: blabs-тусовка = Go (rsc, pike),
  Go-devdraw стал бы первым нативным и легибельным сообществу.
- Трейд-офф: Rust = современный рендер-стек из коробки (wgpu,
  cosmic-text — Zed-прув), но ноль мозга acme (отсюда труп
  Edward-POC). Go = мозг + протоколы + тусовка, слабее шейпинг/GPU.
- Протоколы нейтральны к языку: кирпичи можно смешивать (Go-шим по
  образцу wl9 обслужит и C-acme, и Edwood).

# See also

- [draw-wayland-shims](draw-wayland-shims.md) — wl9/wio, 388 форков
- [9fans-go-acme](9fans-go-acme.md)
- [edward](edward.md) — тот же мозг, старая попытка
- [acre](acre.md) — Rust-аналог acme-lsp, мёртв
