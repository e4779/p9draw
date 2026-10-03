---
type: concept
title: Роадмап красильного слоя — три кирпича, база C plan9port, язык открыт
description: Замороженный план — (1) свой devdraw на draw-протоколе, валидация на нетронутом C-acme; (2) пиксельный libframe; (3) мозг из Edwood. Референсы wl9, 9webdraw, jsdrawterm, issue
tags:
  - roadmap
  - devdraw
  - draw-protocol
  - wayland
  - libframe
  - edwood
  - 9webdraw
generated:
  by: human:e4779
  at: "2026-10-03T18:14:16Z"
---

# Overview

План не меняется, база выбрана структурно: C plan9port (живой,
pushed 2026-09-29, rsc) — общий substrate ОБЕИХ школ: Edwood сам
требует plan9port devdraw. Взаимодействие = протокол, не код:
кирпич 1 не трогает plan9port вовсе — наш devdraw обслуживает
нетронутый C-acme.

# Details

## Три кирпича
1. devdraw (Rust|Go): draw-протокол → Wayland/GPU/браузер; валидация
   — C-acme без изменений. ~20% кайфа, дёшево.
2. пиксельный libframe (cosmic-text | go-text/typesetting) —
   суб-пиксельный скролл, float-смещение; ~80% кайфа, дорого.
3. мозг: форк Edwood (Go, 445*, MIT) или C-acme навсегда.
Порядок отладки: краска, потом координаты.

## Нас опередили трижды — и все три застыли
- wl9 (michaelforney, C): rio-wayland шим — учебник серверной стороны.
  НЕ база: серверный слой, наш libframe — клиентский.
- 9webdraw (sirnewton01, 35*): draw(3)-сервер в браузер, HTML5 canvas
  (плюс aiju/jsdrawterm — JS 9P+devdraw). «Drawterm в браузере»
  делали дважды, канвас-2D, ~десять лет назад.
- plan9port-wayland (AUR): C-патчи Wayland к devdraw, не слиты
  апстрим. Upstream: issue #130 открыт — бэкенды только Xlib/
  Carbon/Cocoa. Наш проект — потенциальный ответ на #130.
- p9p-wayland (arroyo.cc): композитор с ЧЕСТНЫМИ /dev/draw, /dev/mouse,
  /dev/keyboard через user namespaces — план9-нативный подход.

## Язык кирпича 1: Rust (решено 2026-09-30)
Апстримибельность НЕ зависит от языка: devdraw — отдельный процесс
за протоколом, план9port возьмёт хороший бэкенд на любом языке
(issue #130 ждёт). Культурный аргумент за Go остаётся для кирпича 3
(мозг, Edwood). Решение: Rust — интерес автора, C-FFI если
пригодится, референсная масса: wgpu, cosmic-text 0.19, smithay 0.7,
gpui 0.2.2 (Zed's GPU UI framework, опубликован на crates.io,
Apache-2.0 — кандидат в хост-слой «окно+инпут+HiDPI»), otto как
живой пример solo-стека. Референс-код: zed-industries/zed в
librarian-кэше. Браузер-таргет не цель (стримить devdraw = «только
acme»; весь рабочий стол — школа webland); цель — нативный Wayland.

# See also

- [draw-wayland-shims](draw-wayland-shims.md)
- [go-draw-stack](go-draw-stack.md)
- [plan9port](plan9port.md)
- [acre](acre.md) — протокольный прецедент на 9p-стороне

# Details (ночь 2026-09-30/10-01 — итоги автономного прогона)

## Track B — p9draw бутстрап: ГОТОВО
- Репо /home/e4779/projects/p9draw (git, 2 коммита: ca0933c docs, 3c6b251 feat).
- SPEC.md 21КБ/340 строк: drawfcall 33 типа (16 T/R-пар + Rerror) +
  29 однобуквенных команд внутреннего draw-протокола; 7 OPEN-вопросов.
- ARCHITECTURE.md: protocol (без IO) / server / gpu-позже.
- crates/protocol: enum Wsysmsg 33 варианта, encode/decode, 18/18 тестов
  зелёные (golden+roundtrip, cargo 1.98.1 /usr/bin/cargo; НЕ ~/.cargo/bin).
- docs/research.md — сырые выжимки трёх агентов-исследователей.
- Уроки: agent() возвращает строку напрямую (не {finding}); box playground
  в box_list не найден — хостовый cargo ок для лёгких крейтов.

## Track A — acme-web на :3005
- Контейнер acme-web (lscr.io/linuxserver/webtop:ubuntu-kde, KWin/Wayland),
  raw podman run (quadlet НЕ сгенерился — причина не найдена, файл 644;
  утром разобрать генератор или оставить raw + systemd unit вручную).
- Порты 3005(http)/3006(https), basic-auth = креды webtop.env; локально 401 ОК.
- Внешний desktop.e4779.netcraze.link:3005 → 000 (DNS/v6 резолвится, корень
  домена 403 живой) — похоже, проброс :3005 на роутере не отвечает; утром.
- plan9port: СОБРАН (03 попытки: libx11-dev → libxt-dev (IntrinsicP.h) —
  полная пачка libxt/ice/sm). Первый запуск сорвался: скрипт не попал в
  custom-cont-init.d до старта; вторая проблема — контейнер был в сети
  podman вместо systemd-homelab (firewall не выпускал) — лечится
  --network systemd-homelab.
- ИТОГ 02:00: acme ЗАПУЩЕН в сессии (KWin/Wayland, XWayland, :1),
  fsys живой в /tmp/ns.abc.:1/acme; порты 3005/3006 с basic-auth.
  Внешний desktop.e4779.netcraze.link — РАБОТАЕТ (утро 10-01: скриншот
  пользователя, acme в браузере; прокси на :443 → hlab:3005; ранее 000
  был транзиентом проброса).

## День 10-01 — Phase 4/5: перехват живого трафика
- crates/server (p9draw-server): accept-loop + MITM capture (pipe-режим,
  9pserve в дереве НЕТ — клиент execl'ит devdraw через пайпы) +
  scripts/devdraw-tee.sh (passthrough по умолчанию, нулевой риск).
- Живой acme в acme-web прогнан через MITM: c2s.bin 1933B / 35 сообщений,
  s2c.bin 482B, capture.log — НАШ декодер разобрал провод (Tinit/Rinit...).
- OPEN-вопросы: 3 CONFIRMED-by-capture (chan = LE u32 0x68081828 = x8r8g8b8,
  GREY1 = 0x31; legacy первый кадр = Tinit без Tctxt), 4 STILL (macOS-chan,
  memdraw-байты, Rrdmouse/'e' — нет мыши/дуг в захвате), 1 уточнён.
- Тесты: protocol 18 + live_capture 4 (реальные фиксчуры) + server 18 + 1.
- Коммиты: ca0933c, 3c6b251, 815569b, +live-fixtures. Фиксчуры в
  fixtures/live-acme/. Гипотеза смерти capture-инстанса — в
  docs/fixtures-analysis.md (проверить на следующем захвате с мышью).

## День 10-01, фаза 6 — render + host: фундамент serve-режима
- crates/render (p9draw-render): растр Image/Chan (x8r8g8b8 LE [B,G,R,X]),
  fill/compose_over/draw_tile с clipr и repl-тайлингом; 15 тестов.
  У agentes-строителей был реальный баг: compose_over писал нули — пойман
  тестами, чинен фиксером.
- crates/host (p9draw-host): winit 0.30 (ApplicationHandler, pump_events)
  + softbuffer 0.4 (u32 0x00RRGGBB, LE-байты = [B,G,R,X] — SAFETY-каст);
  события мыши (план9-маски 1/2/4/8/16, колесо=8/16), клавиши, resize;
  headless-safe (только чистые функции тестируются). 4 теста.
- protocol: Copy-дерайвы для Rect/Point/Chan (по ошибкам moved value).
- ИТОГО workspace: 60 тестов / 0 упавших. Коммиты: ca0933c, 3c6b251,
  815569b, ba460c4, +render/host.
- Остаток serve-вехи: dispatch-клей (drawfcall → render ops → host present;
  host events → R-messages) + запуск как замена devdraw в контейнере.

## День 10-01, фаза 7 — DrawCmd: интерпретатор внутренних команд
- crates/protocol/src/drawcmd.rs (~1240 строк): enum DrawCmd + parse_drawcmds
  (payload Twrdraw, SPEC §6); валидирован на живых фиксчурах
  (fixtures/live-acme — реальный c2s.bin декодируется целиком).
- Тесты workspace: 81 / 0 упавших (protocol 36+4 live, render 15, server
  18+1, host 4). Коммит: +drawcmd.
- КОНВЕЙЕР ГОТОВ ЦЕЛИКОМ: protocol → server(MITM) → render → host.
  Осталась склейка serve-режима: read loop → DrawCmd → render → host
  present; host events → R-сообщения. После склейки — подмена devdraw
  в acme-web и живой тест «acme красит p9draw».
## День 10-01, фазы 12-15 — тесты, статистика, трасса, семантика
- Ф12: stats.rs (P9DRAW_STATS=1, счётчики типов, строка/30с), e2e-харнесс (examples/e2e: init/alloc/fill/readback/mouse), README. 107 тестов.
- Ф13: encode_drawcmds + identity-roundtrip на живых захватах (74KB); e2e в контейнере: ПОЛНЫЙ PASSED (init/alloc/fill/readback 43200B paleyellow [AA FF FF 00]/mouse Rrdmouse с квирком p[19]).
- Живой drag-тест ХОЗЯИНА: 118 Rrdmouse, 129 Twrdraw, Tcursor2 x14 — НОЛЬ новых ошибок; колонки разъехались на скриншоте = круг замкнут. fixtures/milestone-input-loop.png.
- Ф14: trace.rs (P9DRAW_TRACE=1 — строка на применённую команду).
- Ф15: Rerror с причиной; b=alloc (байты пишут y/Y); v=flush (devdraw.c:1406); каналы 0xFFFFAAFF -> [AA FF FF 00]; x=stringbg, i=font, q=dpi — РЕАЛИЗАЦИЯ ТЕКСТА = ЧЕСТНАЯ v0-GAP (нужны шрифты).
- 122 теста / 0. Коммиты: 73653d9, f8da40f, 0037e4f, +фазы 12-15.
- Эксплуатационные грабли: pgrep/pkill -f самоподрыв (скобочный паттерн или -x); bash ${} ломает template literal; cargo release протухает — сверять strings-маркеры; import -window не снимает перекрытые окна; X :1 пускает по UID (работать от abc).
## День 10-01, фаза 21 — ТЕКСТ ЖИВОЙ. Кирпич 1 отрисовал acme целиком
- Пробы ф21: атлас ложится в font-image (l x109: 000000ff -> ч/б), x stringbg красит чернила (ink 720-886/3000-4020 на window-img).
- Корень утренних симптомов: живой процесс не был перезапущен на бинарь с rgba_at-фиксом (три раза подряд деплой без рестарта).
- Результат (X11-снимок + скриншот хозяина): колонки, теги, ЛИСТИНГ КАТАЛОГА с именами файлов — читаемый чёрный текст. Плотность тёмных 1.17% vs 0.75% у devdraw (разница = вес шрифта).
- Ложная тревога гистограммы: 6 уникальных цветов — НОРМА для 1-битного шрифта plan9 (сломанный awk-фильтр в контейнерных кавычках насчитал 0 тёмных).
- fixtures/milestone-text-alive.png — снимок вехи. Rerror op=f x1 — безвреден (free несуществующего; devdraw ответил бы так же).
- ОСТАЛОСЬ до паритета: DPI/крупные шрифты (fontsrv-размеры), прозрачные зоны white-strip (screen-size negotiation), плотность шрифта.## День 10-01, 20:0x — КИРПИЧ 1 ПОДТВЕРЖДЁН ЖИВЫМ ПОЛЬЗОВАТЕЛЕМ
- Хозяин проекта за трёхкнопочной мышью (см. его пост 2025-10-23 «CAD mouse for acme») подтвердил: p9draw-окно РАБОТАЕТ — Newcol создаёт колонки, теги с текстом, все три кнопки, драги, листинги.
- «Вау! И правда! С трёхкнопочной мышью всё работает!» — вердикт принципала = закрытие вехи «кирпич 1: draw-сервер рисует и ведёт acme».
- Стек в бою: plan9port acme -> drawfcall -> p9draw-server (Rust, winit+softbuffer) -> XWayland -> Selkies -> браузер снаружи. 148+ тестов, 10+ коммитов, живые фиксчуры acme.
- Остаток паритета (не блокеры): DPI/крупные шрифты, screen-size negotiation (белая полоса), плотность шрифта, wgpu-бэкенд.## День 10-01, вечер — DPI-веха: антиалиасинговый шрифт через весь конвейер
- fontsrv (namespace serve) + acme -f /mnt/font/DejaVuSansMono/16a/font: анталиасинговый DejaVu через drawfcall -> p9draw -> экран. 497 уникальных цветов (1-битная эра кончилась).
- WINSIZE=1800x1400 в launch: окно раскрывается на весь экран (2960x1560 фактических), белая полоса сократилась (остаток: acme узнаёт размер только при след. resize — resized-механизм).
- launch-скрипт: /tmp/launch-acme-dpi.sh (в контейнере /tmp/launch.sh) — killset + fontsrv + serve-acme с WINSIZE и -f. Обновить acme.desktop автостарт на этот профиль.
- НЕ ЗАКРЫТО: текст есть, но КЛИКИ/ВВОД в p9draw-окне не проверены после всех фиксов (проверка: клик в тело, буквы — должны вставляться); плотность шрифта vs devdraw; resized-пропагация размера.## День 10-01, вечер — WAYLAND-NATIVE: X в рендер-пути больше нет
- Ф22: окно serve переведено на нативный Wayland БЕЗ изменений кода — только env: WAYLAND_DISPLAY=wayland-1, XDG_RUNTIME_DIR=/config/.XDG, DISPLAY ИМЕННО unset (не пустой!) — DISPLAY протекает через su от podman-exec клиента и даёт тихий откат winit на X11. Ловушка задокументирована в README.
- Верификация уровня протокола: WAYLAND_DEBUG wire-лог — xdg_toplevel «acme», configure/ack/commit, 31 wl_buffer.release (KWin потребил кадры), 0 ошибок. X-дерево: 0 окон p9draw. Сокет: fd p9draw ↔ peer wayland-1 (inode-match ss -xp). fixtures/wayland-native-wire.log.
- Скриншот невозможен в принципе: KWin ScreenShot2 авторизует только по .desktop-реестру (Exec<->/proc/exe), sycoca пуста, spectacle/portal отсутствуют — задокументировано, wire-лог вместо PNG.
- Стек стал пост-X: acme (C) -> drawfcall -> p9draw (Rust) -> нативный Wayland -> Selkies -> браузер. X остался только у эталонного devdraw-acme (266337, сознательно).
- Коммиты: 802d3bd (launch script + README + wire log), e0f20f4, ac80873, 4832133 (rgba_at/blit), 378bc2b, 60b2f5c, 4cd9839. cargo test: все наборы зелёные.## День 10-01, фаза 24 — ФИНАЛ: acme в браузерном композиторе, текст доказан декодированием H.264
- Диагноз тёмного окна: p9draw невиновен — WAYLAND_DEBUG: attach/commit каждые 15-30мс, 0 ошибок протокола. Корни: (1) webland копил 519 VA-API ошибок с 29.09 (мёртвый энкодер у ВСЕХ окон) — рестарт webland оживил H264; (2) NOLIBTHREADDAEMONIZE=1 валил 9pserve (libthread sysfatal) — acme умирал на старте.
- p9draw-вклад: P9DRAW_PRESENT_DEBUG=1 (present-лог: backend/размеры/dirty/rate-limited), configure-гейт в ScreenHost::open (bounded pump до первой конфигурации), take_error больше не тонет (serve loop логирует ошибки presenter). 152 теста.
- ВЕРИФИКАЦИЯ КОРОНЫ: перехвачены бинарные SurfaceFrame (H264) из браузерного ws, ffmpeg-декод -> fixtures/wayland-text-v2.png: cream-фон 96.6%, 1817 тёмных пикселей ТЕКСТА. Весь конвейер acme->drawfcall->p9draw->Wayland->webland->VA-API->H264->браузер доказан декодированием видеопотока.
- Коммиты: cd45067, 3c2e3db, 155d0cc. Верификатор: passed:true.
- КИРПИЧ 1: ЗАВЕРШЁН И РАЗВЁРНУТ В ДВУХ ШКОЛАХ — X11/акме-веб (3005) и нативный Wayland/webland (3030). Остаток: DPI-полировка, resized-пропагация, wgpu-горизонт.## День 10-03, вечер — КОРЕНЬ «тёмного окна» найден: resized-флаг
- Свежий acme не рисовал начальную раскладку: наш serve стартовал с resized=false, а настоящий devdraw ПЕРВЫЙ Rrdmouse отвечает resized=1 («окно появилось» = resize) — только по нему acme начинает paint. Фикс: Screen init resized=true.
- Подтверждение: живой запуск (приложен скриншот хозяина) — после кликов acme РЕАГИРУЕТ: колонки создаются (Newcol через button 2!), +Errors с exit 1 — execution-контур через наш сервер работает.
- Также: контейнерный e2e был на ПРОТУХШЕМ бинаре (cargo release cache + mtime-перекос после агентских правок) — лечение: find crates -name "*.rs" -exec touch + rebuild; strings-маркер "serve stats" как проверка свежести.
- e2e ФИНАЛ: 10/10 PASSED (init/alloc/fill/text/readback/mouse/window/window-readback) на webland-test со свежим бинарником.
- Мусор: 9 зомби p9draw-server в waypipe-test (копились от перезапусков) — чистка отложена (безвредно для PID1=sleep).## День 10-01, фаза 25 — ФИНАЛЬНАЯ РАЗГАДКА тёмного окна: insecure origin
- Пробы image 0: dark=777 (1.2%), light=65399 — КОНТЕНТ БЫЛ ВСЁ ВРЕМЯ (paleyellow + текст). wl_shm-пулы идентичны (97.3% light) — present-копия чиста. Ветка A.
- КОРЕНЬ (не p9draw!): браузер смотрел по http://<LAN-IP>:3030 — insecure origin, Chrome не даёт WebCodecs; webland-фронтенд ack-ал H.264 без декодирования (paint(): decoder=None -> presented() без рендера, фолбэка нет). Через http://127.0.0.1:3030 (secure) acme отрисован ЦЕЛИКОМ — скриншот с текстом, тегами, листингом.
- ПОСЛЕ: img0 dark=3850 (5.3%) — <10% (текст+теги на фоне). cargo test 156/0. Коммит cb1f27b (stats-гистограмма image 0 + trace dst/rect).
- РАБОЧИЙ ПРОСМОТРЩИК оставлен: relay hlab:9001 -> контейнер + socat в webtop + вкладка Chrome с живым acme.
- ТИКЕТ webland-сайду (автору): capability negotiation / не ack-ать недекодированные кадры.
- УРОК ДНЯ: «тёмное окно» = политика secure-origin Chrome. p9draw оправдан на всех линиях: провод чист, image 0 полон, present байт-в-байт, декомпрессор кладёт глифы (18422 тёмных в атласе).
