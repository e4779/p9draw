---
type: tool
title: Anvil — пост-acme на Go/Gioui с нативным ssh
description: Тайлящий multi-pane редактор на Go+Gioui; нативный ssh-remote (per-window контекст), Range Statements, лозенги, REST API вместо fs; исходники — только архивы по релизам.
resource: ../_sources/anvil-editor-research-2026-09.md
tags:
  - anvil
  - acme
  - editor
  - gioui
  - go
  - ssh
generated:
  by: human:e4779
  at: "2026-09-30T07:09:16Z"
---

# Overview

Anvil — «графический, multi-pane тайлящий редактор, смело использующий мышь
и тесно интегрированный с шеллом». Самый законченный из живых наследников
acme: решает ровно то, чего acme-архитектуре не хватало — современный рендер
и эргономику, не трогая философию.

# Details

- Gioui-рендер (immediate-mode, GPU), тайлинг multi-pane.
- Нативный ssh: `[user@]host:/path` — per-window удалённый контекст;
  удалённой стороне нужны только sh+cat.
- Range Statements: надмножество sam structural regex; выделения — вход/выход.
- Лозенги ◊...◊ для команд/поиска с пробелами.
- REST API как компромисс вместо fs-API (см. why-rest в источнике).
- Исходники: только архивы по релизам (живого git нет).

# See also

- [acme-remote-editing](acme-remote-editing.md) — сравнение подходов удалённости.

# Верификация по клону (2026-09-30, jeffwilliams/anvil)

- Автор: **Jeff Williams** (kanobe@gmail.com / info@anvil-editor.net) —
  хозяин anvil-editor.net. Репозиторий импортирован на GitHub снапшотом
  в мае 2025 («Import code to github»), без git-истории разработки;
  теги v0.6.1 → v0.7 → v0.7.1; последний коммит — дек 2025.
- Gioui подтверждён: gioui.org v0.8.0 + собственный форк автора
  (github.com/jeffwilliams/gio).
- «sshfs» в проекте — это **не FUSE-sshfs**: в editor/cmd/anvil/fs.go
  есть тип sshFs — собственная SSH-реализация удалённой ФС Anvil
  (NewSshFs; удалённые команды через shell, дефолт sh — то самое
  «удалённой стороне нужны только sh+cat»). GetFs() возвращает localFs
  или sshFs по виду пути ([user@]host:/path).
- Директория docs/ (mkdocs-источник сайта) в репо отсутствует —
  сайт собирается отдельно.

# Что взято из acme (археология по коду, 2026-09-30)

## Взято концептуально 1:1 (другой транспорт, та же модель)
- REST API = acme fs API переизданная: /wins/{id}/body, /tag,
  /selections (≈ acme /addr), /execute, /notifs (≈ acme /event), /cmds,
  /ws (live-канал). Полное соответствие акме-файлам — но REST/WebSocket
  вместо 9p.
- Windows-модель: tiled columns (col.go), окна с тегом и телом
  (endpoints /tag, /body — прямо акме-терминология).
- **plumbing** — 6 файлов с plumbing в cmd/anvil.
- Обвязка экосистемы (extras/cmd): **awin** (≈ acme win — шелл-окна),
  adiff, ado, aedit, autodump (≈ acme dumpfile), wrap, mdtoc,
  **anvsshd** (ssh-серверная сторона remote editing), aclangd, asm,
  awatch, Rt.

## Построено с нуля
- Рендер: Gioui GPU (не libdraw/frame); лицензия MIT 2019 Jeff Williams —
  ни строчки кода acme.
- Буфер: piece table (internal/pctbl) вместо акме-буферов.
- Своя regex-машина, typeset, events; свой SSH-стек (sshFs + anvsshd).
- С нуля: мультикирсоры (/selections), лозенги, Range statements
  (internal/expr), fuzzy, intvl.

## Чего из acme НЕТ
- snarf — 0 файлов (обычный clipboard через atotto/clipboard).
- Edit/sam-команд под этим именем нет — Range statements другой язык.
- 9p/fs-протокол — заменён REST/WS.

## Вердикт
Anvil — не транслитерация (как 9fans/go→Edwood), а **чистая
ре-имплементация концептуальной модели acme на Gioui + REST/ssh**,
плюс современный плюс-набор. Кодовой родословной нет — есть
концептуальная.
