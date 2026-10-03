---
type: methodology
title: Fleet-split — тонкое стекло, удалённый мозг, один протокол
description: Архитектурный паттерн Fleet (Skiko-фронтенд локально, JVM-бэкенд удалённо, protobuf между ними) как «drawterm для IDE»; применимость к связке агент+редактор+композитор.
tags:
  - fleet
  - architecture
  - remote
  - pattern
generated:
  by: human:e4779
  at: "2026-09-30T07:09:16Z"
---

# Overview

Fleet спроектирован для локальных, удалённых и распределённых конфигураций:
фронтенд (Skiko/Skia) рисует, бэкенд (JVM) держит интеллект, между ними —
protobuf. Это drawterm-школа, переизданная для IDE.

# Details

- Тот же паттерн, что pi-web (агент-бэкенд на сервере, браузер-стекло),
  VSCode Remote (головной сервер + локальный UI) и drawterm.
- Паттерн «стекло/мозг» естественно описывает и связку агент+редактор:
  редактор-делегатор на хосте, композитор вместо tmux, агент как стадия пайпа.
- Противоположность монолиту-поглотителю (Emacs): интеллект не втягивается
  в процесс стекла.

# See also

- [editor-rendering-stacks](editor-rendering-stacks.md)
- [acme-remote-editing](acme-remote-editing.md)
