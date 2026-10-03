---
type: research-synthesis
title: Anvil — ресёрч редактора (2026-09-29)
description: Gioui, нативный ssh-remote per-window, Range Statements, REST-компромисс, исходники только архивами; sshfs в текущих доках отсутствует.
tags:
  - anvil
  - editor
  - gioui
  - ssh
generated:
  by: human:e4779
  at: "2026-10-03T18:14:16Z"
---

# Anvil — ресёрч редактора (2026-09-29)

Сайт: https://anvil-editor.net/ (mkdocs-material; sitemap полный, release-notes v0.2→v0.7.1).
Стек: Go + Gioui (immediate-mode GUI). Тайлящий multi-pane редактор, «bold use of the mouse».

## Remote editing (нативный ssh, не sshfs)
- Открытие `[username@]host:/path/to/file` → Anvil держит SSH-соединение,
  редактирование файла И исполнение команд «in the context of the window».
- От удалённой машины требуется только Unix-like окружение: `sh` и `cat`.
- Аутентификация: команда Hostpass (пароль), Keypass (ключ в anvil-конфиг-каталоге),
  ssh-agent (Linux, при запуске с правильным env). Пароли не хранятся даже в dumpfile.

## Differences from Acme (статья сайта)
1. Синтаксическая подсветка (с цитатой Пайка про Cuisenaire rods).
2. Нативный ssh-remote (см. выше) — per-window удалённый контекст.
3. Мультикирсоры и selections; Range Statements (надмножество sam structural
   regex): выделения — вход языка, результаты — новые выделения.
4. Клавиатура: стрелки, shift-выделение, Ctrl-T — исполнить слово,
   Ctrl-Enter — строку.
5. Файлы с пробелами в именах.
6. Лозенги: ◊команда с пробелами◊ — клик в любом месте ромба исполняет.

## Архитектура API: why REST
Статья /articles/why-rest/: acme экспонирует состояние как файловую систему
(изоляция расширений процессами, любой язык, правка на лету), но «предоставить
filesystem API нетривиально» вне plan9 → компромисс: REST API с той же
изоляционной идеей. (Ирония: вне plan9 нетривиально ровно потому, что нет 9p.)

## Исходники
- Живого git-репозитория нет: 404 на github.com/andrewchamberss/anvil и
  github.com/anvil-editor/anvil (git ls-remote, 2026-09-30).
- /how-to/build/ ссылается на «source code archive» со страницы download —
  архивы исходников по релизам, без git-истории.
- В текущих доках ноль упоминаний sshfs (проверено grep по 10 страницам,
  включая все release-notes) — память о sshfs либо из ранних ревизий, либо
  обобщение жанра «примонтировать удалённые файлы».
- GitHub-поиск находит только темы (KikyTokamuro/anvil-monokai, anvil-xcode).
