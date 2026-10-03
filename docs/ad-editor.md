---
type: tool
title: ad — модальный терминальный редактор в духе acme (sminez)
description: "Rust-редактор от автора крейта ninep: модальность vi/kakoune + acme-подход к расширяемости, «text is something you can execute»; работает в терминале одним окном. Плейграунд, не daily driver."
tags:
  - ad
  - sminez
  - ninep
  - rust
  - editor
  - terminal
generated:
  by: human:e4779
  at: "2026-09-30T07:09:16Z"
---

# Overview

ad (github.com/sminez/ad) — «an adaptable text editor»: модальность vi/kakoune
с acme-расширяемостью. Автор — создатель ninep (Rust 9p-библиотеки), на которой
стоит wikifs-rs.

# Details

## Архитектура (по клону 2026-09-30)
Cargo workspace: crates/ = ad_client, ad_event, ad_repl, ad_watch;
src/ = buffer, dot (терминология acme: текущее выделение), exec,
fsys (акме-стиль файловый интерфейс управления редактором — аналог
/mnt/acme, на ninep того же автора), lsp (встроенный LSP), mode
(модальное редактирование), tree-sitter (синтаксис через
tree-sitter-plumbing-rules), fuzz, benches, reference-tests.
Это не порт, а переосмысление: философия acme (текст исполняем,
управление через файловый интерфейс) + современный стек (LSP,
tree-sitter) + модальность kakoune (selection-first).

- Терминальный, одно окно — без собственного WM (родственник Edward-тезы).
- Автор честно предупреждает: playground, не daily driver.
- Иллюстрация тезиса «acme-семейство дублируется в каждом языке».

# See also

- [wily](wily.md)
