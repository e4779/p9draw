---
type: research-synthesis
title: Карта родословной acme (mermaid)
description: Диаграмма-карта семейства acme, трёх школ удалённости и plan9-grid в виде mermaid-блока; дубль editor-lineage.drawio в читаемом виде.
tags:
  - acme
  - lineage
  - mermaid
  - map
generated:
  by: human:e4779
  at: "2026-10-03T18:14:16Z"
---

# Overview

Карта семейства acme и школ удалённости. Визуальная версия лежит рядом:
`editor-lineage.drawio` (открывается в draw.io). Этот mermaid рендерится
на GitHub и в любом mermaid-совместимом вьювере.

# Карта

```mermaid
flowchart TD
  subgraph lineage["Родословная кода"]
    acme["Plan 9 acme (Pike, 1994)"] --> p9p["plan9port acme (C, devdraw, fontsrv)"]
    p9p -->|"транслитерация C→Go"| serenity["ProjectSerenity/acme"]
    serenity --> gofans["9fans/go cmd/acme (замер: дек 2021)"]
    gofans -->|"форк"| edwood["Edwood (живой форк)"]
    edwood -->|"форк: минус WM"| edward["Edward: окна = окна ОС, WM у композитора"]
    acme -.->|"клон, 90-е"| wily["Wily (X11, мёртв)"]
  end

  subgraph spirit["По духу"]
    anvil["Anvil: Go+Gioui, ssh per-window, Range Statements, лозенги"]
    ad["ad sminez: Rust, ninep+fsys, modal kakoune, LSP, tree-sitter"]
    fleet["Fleet закрыт: Skiko-стекло + JVM-мозг, protobuf; IDEA = Swing"]
  end

  gofans -.->|"по духу, Go"| anvil
  p9p -.->|"философия"| ad
  serenity -.->|"дух, закрыт"| fleet

  subgraph schools["Три школы удалённого стола"]
    proto["Протокол: waypipe — wayland по ssh, per-app окна, RTT-чувствителен"]
    video["Видео и per-surface: webland x2, Greenfield — окно = H.264-поток, композитит браузер"]
    perwin["Per-window ssh: Anvil — окно несёт удалённый контекст, нужны sh+cat"]
  end

  webcodecs["WebCodecs 2023 — точка перелома жанра; Greenfield 2017 был рано"]
  webcodecs -.->|"включила жанр"| video

  subgraph grid["plan9-grid: одна учётка, одно пространство имён"]
    gridnode["NAS LAN: 9front CPU server; VPS: Linux + plan9port; ноутбук: терминал или drawterm. Агент + данные + вики"]
    ninepg["~mag/9pg: 9front virtual playground QEMU — ALL-IN-ONE и DISTRIBUTED"]
    wikifs["wikifs-rs: 9p-сервер вики на Rust, ninep, mount -t 9p"]
  end

  ninepg --> gridnode
  wikifs --> gridnode
```

# See also

- [editor-rendering-stacks](editor-rendering-stacks.md) — таблица слоёв
- [acme-remote-editing](acme-remote-editing.md) — три школы подробнее
- [fleet-split-pattern](fleet-split-pattern.md)
