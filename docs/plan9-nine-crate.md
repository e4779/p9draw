---
type: tool
title: Крейт plan9 (2020, заброшен) — Rust-клиент живого acme и plumber
description: "Мёртвый крейт plan9 v0.1.1 (5 скачиваний) — на деле мини-«9fans/go для Rust»: dial через plan9-namespace, fsys/fid, клиент живого acme (index/log/окна) и plumbing-сообщения. Построен на крейте nine."
tags:
  - plan9
  - rust
  - acme
  - plumb
  - client
  - dead
generated:
  by: human:e4779
  at: "2026-10-03T18:14:16Z"
---

# Overview

Крейт `plan9` (2020, v0.1.1, «Plan9 interaction, based on
github.com/9fans/go») — не «взаимодействие вообще», а **клиентская
библиотека живого plan9-окружения на Rust**: 6 модулей — dial, conn,
fid, fsys, acme, plumb.

# Details

- dial.rs: план9-стиль dial + namespace() — план9-namespace-каталог
  (как $NAMESPACE plan9port: /tmp/ns.$user.$display); mount_service
  («acme» / «plumb») цепляется к живым сервисам через их сокеты.
- acme.rs: клиент живого acme — WinInfo::windows() (список окон через
  index), LogReader (поток событий /log: id/op/name), открытие окон.
  WinInfo/LogReader — калька с Go-пакета 9fans/go/acme.
- plumb.rs: plumbing-сообщения (Message { dst, typ, data } → send)
  живому plumber'у.
- Построен на крейте **nine** (nine::p2000::OpenMode) — не путать
  с ninep sminez'а (тем, что в wikifs-rs).

## Статус и смысл
- Мёртв: 2020, v0.1.1, ~5 свежих скачиваний.
- Но это единственный известный **Rust-клиент живого acme**: Rust-код,
  читающий окна/события acme и шлющий plumbing — то, чем является
  9fans/go/acme для Go.
- Требует plan9port-окружения (namespace-сокеты живого acme/plumber).

## Возможное применение
- Оживить (форк, перевести nine→ninep или актуализировать) и получить
  Rust-демон-мост к живому acme в webtop: события окон → любая
  Rust-логика → запись обратно. В связке с ad (fsys на ninep, серверная
  сторона) закрывает обе половины «управляемого редактора».

# See also

- [ad-editor](ad-editor.md) — fsys-интерфейс на ninep (серверная половина)
- [9fans-go-acme](9fans-go-acme.md) — Go-оригинал этого дизайна
