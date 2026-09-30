# p9draw — архитектура (кирпич 1: paint)

## Скоуп

Замена `devdraw` из plan9port: сервер, который говорит по wire-протоколу
drawfcall (`Wsysmsg`) + несёт внутренний draw-поток — см. `SPEC.md`.
Цель кирпича 1 — **протокольная корректность**: собранный plan9port-клиент
(`libdraw`) работает с p9draw без изменений и патчей.

**GPU/рендеринг — вне скоупа.** Точка подключения — трейт `Backend`
(аналог `ClientImpl` + `gfx_*` из plan9port); пока существует только
headless-реализация-заглушка.

Не в скоупе также: 9p-маршрутизация нескольких клиентов (`9pserve`,
`post9pservice` — в дереве plan9port их всё равно нет), macOS/X11-бэкенды.

## Крейты

```
p9draw/
├── crates/
│   ├── p9draw-protocol/   # модель + encode/decode, БЕЗ IO
│   └── p9draw-server/     # сокет, accept, dispatch, Backend trait
└── SPEC.md
```

### p9draw-protocol (без IO)

Чистая библиотека: детерминированная, тестируемая golden-байтами.

| Модуль | Содержимое |
|---|---|
| `msg.rs` | `enum Wsysmsg` — все 33 типа из drawfcall.h; константы типов (`TRDmouse=2`, …); `tag: u8` в конверте, не в сообщении |
| `enc.rs` | `size(msg) -> u32` (аналог `sizeW2M`), `encode(msg, buf)` — u32/u16 BE, строки `n[4]+bytes`, rect 4×u32 BE |
| `dec.rs` | `decode(&[u8]) -> Result<(tag, Wsysmsg), DecodeError>`; строгая проверка длины кадра |
| `draw.rs` | внутренний draw-поток (LE): `parse_stream(&[u8]) -> Vec<Result<DrawCmd, DrawError>>`; `drawcoord`; `DrawCmd` — 29 команд из `devdraw.c draw_datawrite` |
| `chan.rs` | дескриптор канала `Chan(u32)`, именованные константы (RGB24=0x081828, XRGB32=0x68081828, …), `chantostr`/`strtochan`/`depth` |
| `winsize.rs` | `parsewinsize`: `WxH`, `WxH@X,Y`, `x,y,w,h` (strtol-семантика, base 0) |
| `errors.rs` | канонические строки ошибок Rerror ("short draw message", …) — константы для совместимости |

Правила: никаких `std::io`/сокетов/потоков; `encode`/`decode` — чистые
функции над байтами. Всё, что подтверждено `SPEC.md`, фиксируется тестами
здесь, а не в серверном крейте.

### p9draw-server (IO, event loop)

| Модуль | Содержимое |
|---|---|
| `main.rs` | CLI: `--legacy` (stdin/stdout), `-s NAME` (unix-сокет), `--trace` |
| `socket.rs` | путь namespace: `$NAMESPACE` \| `/tmp/ns.<uid>.<display>` (display: `:0.0`→`:0`, `/`→`_`); listen, accept |
| `session.rs` | состояние клиента: кольца мыши/клавы/тегов (256), `readdata`, `mouserect`, dpi/forcedpi, `wsysid`, stall-флаги — прямой аналог `Client` из devdraw.h |
| `dispatch.rs` | `runmsg` — таблица диспетчеризации 1:1 с srv.c; `reply` (чётный T → T+1, Rerror=1); контроль «too many queued … reads» |
| `drawdata.rs` | `draw_dataread`/`draw_datawrite`: буфер `readdata` (ответы `'I'`,`'q'`,`'r'`), ≤64 КиБ на `Trddraw`, разбор потока через `p9draw-protocol::draw` |
| `events.rs` | `gfx_mousetrack`/`gfx_keystroke`-семантика: повтор последнего mouse при resize (`resized=1`), clamp в mouserect, stall, latin1-композиция, Kcmd+'r' (dpi 100↔225) |
| `backend.rs` | `trait Backend`: attach/setcursor/setlabel/setmouse/topwin/bouncemouse/getsnarf/putsnarf/keystroke/mousetrack/resize/shutdown; `HeadlessBackend` — заглушка |

Параллелизм: accept-loop → сессия на соединение. Блокировки srv.c
(`eventlk`, `wfdlk`, `drawlk`) отображаются на `Mutex`/каналы; требование
tо же — не держать lock draw-данных, ожидая графику.

### p9draw-gpu (вне скоупа, будущий кирпич)

Реализация `Backend` поверх GPU-стека. Контракт — `backend.rs` + описание
в `SPEC.md` (rpc_attach → Memimage-эквивалент, resize-цикл через
`resized`-флаг Rrdmouse).

## План тестов (golden bytes)

1. **Golden-фикстуры кадров** для каждого из 33 типов drawfcall: пары
   `(Wsysmsg, hex-байты)`. Источник эталона — plan9port: `convW2M`
   (drawfcall.c) прогнать через C-харнесс или снять `DEVDRAWTRACE=1`.
   Обязательные кейсы: пустой кадр 6 байт; строка n=0; `Tcursor2`=343 Б;
   `Rrdmouse`=23 Б; `Tinit` = winsize,затем label; rune u16 (Rrdkbd) vs u32 (Rrdkbd4).
2. **Round-trip property** (`proptest`): `decode(encode(m)) == m` на
   генераторе всех вариантов; границы `MAXWMSG`.
3. **Draw-поток**: golden на каждую команду ('b' 51 Б, 'A' 14, 'c' 22,
   'd' 45, 'D' 2, 'e'/'E' 45, 'f'/'F' 5, 'i' 10, 'J'/'I' 1, 'q' 2+n,
   'l' 37, 'L' 45, 'n' 6+n, 'N' 7+n, 'o' 21, 'O' 2, 'p'/'P' 31+coords,
   'r' 21, 's' 47+2n, 'x' 59+2n, 'S' 9, 't' 4+4w, 'v' 1, 'y'/'Y' 21+data);
   таблица кейсов `drawcoord` (1/3 байта, знак, delta); `'b'`: screenid
   читается как u16 LE из 4-байтового поля (квирк — тест фиксирует).
4. **chan.rs**: все именованные каналы — hex-значение ↔ строка
   ("r8g8b8", "x8r8g8b8", …) ↔ depth; round-trip `chantostr`/`strtochan`.
5. **Session-тесты** (headless backend): очередь тегов мыши/клавы и
   переполнение → Rerror; доставка resize флагом в Rrdmouse; kbd-тег
   `(tag<<1)|is4`; latin1-композиция; Trddraw >64 КиБ → кламп.
6. **Interop с настоящим plan9port**: собранный `o.devdraw` (legacy,
   pipe) + тестовый клиент `cmd/devdraw/drawclient.c` (команды
   init/mouse/kbd по stdin). Затем тот же клиент против p9draw-сервера
   в legacy-режиме — поведение бит-в-бит. В конце — `initdraw`-приложение
   (например, `test.c` из libdraw) против p9draw.
7. **Fuzz**: `cargo-fuzz` на `decode` и `parse_stream` — усечённые кадры,
   неизвестный тип, `count` больше тела; никаких паник, только `Err`.

## Этапы

1. `p9draw-protocol` + golden-фикстуры (быстрая победа, нулевой риск).
2. Сервер в legacy-режиме (stdin/stdout) с `HeadlessBackend` — уже можно
   гонять interop-тест 6.
3. Unix-сокет + `Tctxt`-handshake (серверный режим `-s`).
4. Полный draw-поток (`drawdata.rs`) + `readdata`.
5. Стабилизация `Backend` → передача эстафеты gpu-кирпичу.

## Риски

- **Два endianness в одном протоколе** (drawfcall BE, draw-поток LE) —
  только golden-тесты спасают; не полагаться на «очевидно».
- **Асинхронные ответы**: Rrdmouse/Rrdkbd приходят вне пары запрос-ответ;
  сервер волен отвечать в любом порядке, клиент мультиплексирует по тегу.
- **Тонкая семантика очередей** (stall, повторы при resize) — копировать
  srv.c дословно, не «улучшать» до совместимости.
- Квирки ('b' screenid u16, bit31 в 'e'/'E' ox, `'q'` только 'd')
  зафиксированы в SPEC.md — не вычищать «по ошибке».
