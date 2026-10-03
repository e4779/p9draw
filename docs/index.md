---
okf_version: "0.2"
---

# Subdirectories

* [_sources](_sources/index.md) - Gioui, нативный ssh-remote per-window, Range Statements, REST-компромисс, исходники только архивами; sshfs в текущих доках отсутствует.

# concept

* [Go-стек для плана9-графики: Edwood-мозг, Gio-краска, пустота в draw-шимах](go-draw-stack.md) - Инвентаризация Go-ассетов по слоям — 9fans/go + acme-lsp (живой LSP для acme, 232*), Edwood (445*, единственный полный транслит мозга acme), go-text/typesetting (чистый Go шейпинг), Gio (pure-Go Wayland+wasm); draw-протокол шимов в Go НЕТ — все ездят на C devdraw.
* [wl9 и wio — Wayland-шимы для draw-мира, и что показали 388 форков plan9port](draw-wayland-shims.md) - Обзор красильного слоя план9-экосистемы на Wayland — wl9 (michaelforney, rio-wayland shim), wio (rio-подобный композитор); 388 форков plan9port = патч-стоянки, Rust-devdraw не существует.
* [Роадмап красильного слоя — три кирпича, база C plan9port, язык открыт](roadmap-devdraw.md) - Замороженный план — (1) свой devdraw на draw-протоколе, валидация на нетронутом C-acme; (2) пиксельный libframe; (3) мозг из Edwood. Референсы wl9, 9webdraw, jsdrawterm, issue

# methodology

* [Acme-remote — пер-оконный удалённый контекст](acme-remote-editing.md) - Три школы удалённости через призму редактора (протокол/видео/per-window ssh) и вопрос «файлы видны — а как посылать команды?»; одна учётка и одно пространство имён как цель грида.
* [Fleet-split — тонкое стекло, удалённый мозг, один протокол](fleet-split-pattern.md) - Архитектурный паттерн Fleet (Skiko-фронтенд локально, JVM-бэкенд удалённо, protobuf между ними) как «drawterm для IDE»; применимость к связке агент+редактор+композитор.
* [Sam / Range Statements — выделение как вход и выход языка](sam-range-statements.md) - Структурные регулярные выражения sam как подмножество более широкого языка range statements (Anvil): выделения окна — вход, результат исполнения — новые выделения; тактильный цикл мышь↔выражение.

# research

* [p9draw: исследование draw-протокола (сырые выжимки)](research.md)
* [p9draw: текст и шрифты — дизайн-документ](font-design.md)
* [Разбор живого захвата acme (fixtures/live-acme)](fixtures-analysis.md)

# research-synthesis

* [Acme \& editor lineage — карта семейства и три школы удалённости](overview.md) - Карта наследников acme (wily, edward, anvil, ad, uroswm), рендер-стеков современных редакторов (Fleet/VSCode/Zed/Gioui) и трёх школ удалённой работы; принципы acme, которые переносятся целиком.
* [Карта родословной acme (mermaid)](lineage-map.md) - Диаграмма-карта семейства acme, трёх школ удалённости и plan9-grid в виде mermaid-блока; дубль editor-lineage.drawio в читаемом виде.
* [Рендер-стеки современных редакторов](editor-rendering-stacks.md) - Кто на чём рисует — JetBrains (Swing vs Fleet/Skiko), VSCode (Electron DOM + WebGL2), Zed (GPUI Blade→wgpu), Anvil (Gioui), acme (libdraw/subfonts); таблица «что менять в acme на что» по шести слоям.

# tool

* [9fans/go cmd/acme — Go-транслитерация оригинального acme](9fans-go-acme.md) - Порт acme на Go внутри 9fans/go: дословная транслитерация C-исходников (dat.h→dat.h.go, xfid.c→xfid.go, wind, exec, edit, regx, disk, dump), bigLock event loop, draw-пакет как libdraw. Замер в декабре 2021; родословная ProjectSerenity → 9fans/go → Edwood.
* [acre — LSP-клиент для живого acme на Rust (Madelynn Jibson)](acre.md) - Rust-мост между живым plan9port-acme и LSP-серверами: окно в acme со списком файлов/диагностик, правый клик исполняет; p9port НЕ транслитерирован — acre использует его namespace-сокеты как есть. Beta с 2020, dorman с окт 2022.
* [ad — модальный терминальный редактор в духе acme (sminez)](ad-editor.md) - Rust-редактор от автора крейта ninep: модальность vi/kakoune + acme-подход к расширяемости, «text is something you can execute»; работает в терминале одним окном. Плейграунд, не daily driver.
* [Anvil — пост-acme на Go/Gioui с нативным ssh](anvil.md) - Тайлящий multi-pane редактор на Go+Gioui; нативный ssh-remote (per-window контекст), Range Statements, лозенги, REST API вместо fs; исходники — только архивы по релизам.
* [Edward — форк Edwood без оконного менеджера (каждое окно = окно ОС)](edward.md) - Экспериментальный форк Edwood (Go, сама транслитерационная линейка acme): убирает WM из редактора, каждое acme-окно становится отдельным окном ОС; тайтлбары отдаёт тайлинговому WM. POC, нестабилен.
* [JetBrains Fleet — закрытый IDE-перезапуск; открытые компоненты вокруг](fleet.md) - Fleet проприетарен (репозитория нет), но архитектура документирована (Skiko-фронтенд + JVM-бэкенд + protobuf), а весь окружающий стек открыт: skiko, Compose Multiplatform, intellij-community, JBR.
* [plan9port — Plan 9 userspace для Unix, база акме-семейства](plan9port.md) - Портирование план9-пользовательского пространства на Unix (Russ Cox): acme, sam, rc, plumber, 9p-клиент, fontsrv. Собирается in-place через ./INSTALL; источник — 9fans/plan9port.
* [uROSWM + XCBKit — Objective-C WM из стека Gershwin](uroswm-xcbkit.md) - Оконный менеджер uROSWM, написанный на Objective-C поверх XCBKit (свой ObjC-фреймворк для X11 от Alessandro Sangiuliano); усыновлён Gershwin'ом — процесс WindowManager в сессии Workspace+Menu+WindowManager.
* [Wily — клон acme для X11 (1990-е, историческое)](wily.md) - Ранний клон acme под X11 (Gary Capell) — историческое подтверждение того, что жанр «пересобрать acme» стар столько же, сколько сам acme.
* [Крейт plan9 (2020, заброшен) — Rust-клиент живого acme и plumber](plan9-nine-crate.md) - Мёртвый крейт plan9 v0.1.1 (5 скачиваний) — на деле мини-«9fans/go для Rust»: dial через plan9-namespace, fsys/fid, клиент живого acme (index/log/окна) и plumbing-сообщения. Построен на крейте nine.
