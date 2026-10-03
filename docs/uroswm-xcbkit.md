---
type: tool
title: uROSWM + XCBKit — Objective-C WM из стека Gershwin
description: Оконный менеджер uROSWM, написанный на Objective-C поверх XCBKit (свой ObjC-фреймворк для X11 от Alessandro Sangiuliano); усыновлён Gershwin'ом — процесс WindowManager в сессии Workspace+Menu+WindowManager.
tags:
  - uroswm
  - xcbkit
  - gershwin
  - objective-c
  - wayland
generated:
  by: human:e4779
  at: "2026-09-30T07:09:16Z"
---

# Overview

uROSWM — соло-проект (2020, Alessandro Sangiuliano): WM на Objective-C, потому
что twm/echinus «слишком бедны на ICCCM/EWMH». Gershwin-проект усыновил его:
в org лежат gershwin-uroswm и gershwin-xcbkit, совмещённые в один репо.

# Details

- Запускается как WindowManager в сессии gershwin-session Workspace Menu WindowManager.
- Проверено живьём на hlab (контейнер webtop-gershwin, сентябрь 2026): стекинг
  работает, Menubar-strut применяется.

# See also

- [editor-rendering-stacks](editor-rendering-stacks.md)
