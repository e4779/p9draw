---
type: tool
title: JetBrains Fleet — закрытый IDE-перезапуск; открытые компоненты вокруг
description: "Fleet проприетарен (репозитория нет), но архитектура документирована (Skiko-фронтенд + JVM-бэкенд + protobuf), а весь окружающий стек открыт: skiko, Compose Multiplatform, intellij-community, JBR."
tags:
  - fleet
  - jetbrains
  - skiko
  - closed-source
generated:
  by: human:e4779
  at: "2026-10-03T18:14:16Z"
---

# Overview

Fleet — проприетарный «IDE-перезапуск» JetBrains. Публичного репозитория нет
(github.com/JetBrains/fleet — 404, проверено 2026-09-30). Но архитектура
описана в блоге, и почти весь используемый стек — открыт.

# Details

## Что открыто и склонировано для вдохновения
- **skiko** (github.com/JetBrains/skiko) — Skia-биндинг для Kotlin/JVM:
  как JetBrains оборачивают Skia; паттерны текстового рендера. [в кэше]
- **Compose Multiplatform** — UI-фреймворк на skiko.
- **intellij-community** — классическая платформа целиком: editor internals,
  VFS, PSI (кодовая модель), инкрементальный разбор.
- **JBR** (JetBrains Runtime) — форк JDK с рендер-патчами.

## Архитектурный паттерн (воспроизводим без кода Fleet)
См. [fleet-split-pattern](fleet-split-pattern.md): Skiko-стекло локально,
интеллект-бэкенд где угодно, один protobuf-протокол. Урок: код конкурента
не нужен — нужен чертёж слоёв и открытые компоненты, из которых собирается
эквивалент.

# See also

- [fleet-split-pattern](fleet-split-pattern.md)
- [editor-rendering-stacks](editor-rendering-stacks.md)
