---
type: tool
title: acre — LSP-клиент для живого acme на Rust (Madelynn Jibson)
description: "Rust-мост между живым plan9port-acme и LSP-серверами: окно в acme со списком файлов/диагностик, правый клик исполняет; p9port НЕ транслитерирован — acre использует его namespace-сокеты как есть. Beta с 2020, dorman с окт 2022."
resource: ../_sources/anvil-editor-research-2026-09.md
tags:
  - acre
  - acme
  - lsp
  - rust
  - plan9port
generated:
  by: human:e4779
  at: "2026-10-03T18:14:16Z"
---

# Overview

acre (madelynnblue/acre, ранее mjibson/acre; автор Madelynn Jibson,
ранее Matt) — «langserver client for acme in Rust». Создаёт окно в
живом acme со списком открытых файлов и команд; диагностик и
goto-def через LSP-серверы (gopls и ко), запускаемые сабпроцессами.
Beta: «purposefully crashes on most errors».

# Details

## Архитектура: p9port НЕ транслитерирован — он используется
- plan9port acme уже экспортирует свою 9p-ФС в $NAMESPACE
  (/tmp/ns.$user.$display/acme). acre подключается к этому сокету
  (dial через namespace-каталог, crate nine) и управляет окнами:
  читает index, log, открывает/пишет окна.
- LSP-серверы — обычные сабпроцессы; acre переводит LSP ↔ акме-окна
  (диагностики, references, goto-def).
- Итог: ник: живой acme рендерит через devdraw как всегда, acre — мост,
  ~несколько тысяч строк. Транслитерация (ProjectSerenity → 9fans/go →
  Edwood) — ДРУГАЯ ветка: там портировали сам acme и застряли на devdraw.

## Состав репо
- plan9/ — тот самый «мёртвый крейт plan9» (dial/fsys/fid/acme/plumb
  на crate nine) — библиотечная половина, публикуется на crates.io.
- src/ — acre: LSP-клиент, acre.toml (servers: executable, files regex,
  root_uri, format_on_put, actions_on_put).

## Статус
- Коммиты Matt Jibson до 2022-10-15 («bump version»); аккаунт автора
  сменился на madelynnblue (GitHub редиректит). Сайт madelynn.blue/acre.
- Форков нет, открытых ишуй нет (проверено поиском GitHub 2026-09-30).

## Место в таксономии «как дать acme современные мозги»
1. Транслитерация acme (ProjectSerenity → 9fans/go → Edwood) — портируй
   сам редактор; стена devdraw.
2. Клиент живого acme (9fans/go/acme для Go; acre для Rust) — живой
   acme + внешний мозг.
3. Пер-поверхностный стриминг (webland) — свой композитор, браузер =
   дисплей.
acre — ветка 2 на Rust. Июньский вопрос «файлы видны — а как посылать
команды?» acre отвечает: команды через LSP+окна.

# See also

- [plan9-crate-dead](plan9-nine-crate.md) — библиотечная половина
- [9fans-go-acme](9fans-go-acme.md) — транслитерационная ветка
- [acme-remote-editing](acme-remote-editing.md)
