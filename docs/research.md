---
type: research
title: "p9draw: исследование draw-протокола (сырые выжимки)"
generated:
  by: human:e4779
  at: "2026-10-03T18:14:16Z"
---

# p9draw: исследование draw-протокола plan9port (ночь 2026-09-30/10-01)

Сырые выжимки трёх исследователей. Основа для SPEC.md.

## transport+handshake

## Транспорт и handshake plan9port devdraw (по коду)

**Главное исправление ожиданий:** `post9pservice`/`9pserve` в этом дереве **отсутствуют**. Два транспорта, выбор в `_displayconnect` (libdraw/drawclient.c):

| Режим | Условие | Транспорт |
|---|---|---|
| Legacy (по умолчанию) | нет `$wsysid` | `pipe()` + `fork`; потомок dup2(pipe,0/1), `NOLIBTHREADDAEMONIZE=1`, `execl($DEVDRAW||"devdraw", argv0, argv0, "(devdraw)")`; у родителя `d->srvfd = p[0]` |
| Сервер | `$wsysid="name/id"` | `dial("unix!$NAMESPACE/name")`; первый пакет **Tctxt{id}** → сервер `c->wsysid=strdup(id)`, отвечает **Rctxt** (Rerror — abort). fd → `d->srvfd` |

**Фрейминг:** все сообщения = `size[4] BE + body`; тело = `type[1] tag[1] payload` (convW2M/convM2W, drawfcall.c). Mux (`_displaymux`): теги 1..255, `drawgettag/drawsettag` работают с `msg[4]`. Асинхронные ответы (мышь/kbd) идут с теми же тегами из очередей сервера.

**Последовательность (клиент, init.c):** `initdraw` → `_displaymux` → `_displayconnect` → `_displayinit`: **Tinit{label, winsize}** → Rinit. Номера: Tinit=14, Rerror=1, Tctxt=30, Rctxt=31, Trdkbd4=32; `replymsg` делает `type%2==0 → type++`, т.е. **R = T+1**.

**Сервер (cmd/devdraw/srv.c):** `threadmain`: флаг `-s srvname` = сервер, иначе legacy (`client0`, `rfd=3,wfd=4` — dup 0→3, 1→4, /dev/null на 0/1). `gfx_started()` (из gfx_main после подъёма GUI): legacy → `proccreate(serveproc, client0)`; сервер → `smprint("unix!%s/%s", getns(), srvname)` → `announce` → `listenproc`: `listen(adir,dir)` → новый `Client{displaydpi=100, rfd=wfd=fd}` → `serveproc`. `serveproc`: `read(rfd,4)` → readn body → `convM2W` → `runmsg`. EOF у client0 → `rpc_shutdown()` + `threadexitsall`. Tinit → `rpc_attach(c,label,winsize)` (создаёт окно, возвращает Memimage) → `draw_initdisplaymemimage(c,i)` → Rinit.

**Draw-протокол внутри:** идёт как поток байтов в **Twrdraw{count,data}/Trddraw** (22/20) → `draw_datawrite`/`draw_dataread` (devdraw.c). Опкоды: `'b'` allocimage, `'A'` allocscreen (`'A' id[4] imageid[4] fillid[4] public[1]`, libdraw/window.c), **`'S'` = public screen: `id[4] chan[4]`** (генерит `publicscreen`), `'r'` readinfo и т.д.

**OPEN:** «версия 'S'» в вопросе — не версия: handshake версией здесь **нет вообще**; `'S'` — опкод public-screen в draw-потоке. Кто выставляет `$wsysid` в src — не нашёл (вероятно, внешний wrapper `devdraw -s`); помечено OPEN.

---

## messages

Исследование завершено. Все слои подтверждены кодом: `drawfcall.h` (типы/схема), `drawfcall.c` (convW2M/convM2W), `srv.c` (runmsg — dispatch сервера; `devdraw.c` — только внутренний draw-слой), `drawclient.c` (обёртки `_*`).

## Вопрос 2 — таблица wire-сообщений devdraw (Wsysmsg)

**Кадр (оба направления):** `len[4] BE` — общий размер пакета *включая* эти 4 байта, затем `tag[1] u8` (офс. 4), `type[1] u8` (офс. 5), поля (с офс. 6). Чтение: `readwsysmsg`/`serveproc` = `readn(4) → readn(n−4)`. Лимит `MAXWMSG`=4MB.

**Правило T/R:** чётный тип = T (клиент→сервер), нечётный = ответ (`replymsg`: `if(m->type%2==0) m->type++`); `Rerror=1` — ответ-ошибка на любой T. Tag: клиент задаёт 0, мультиплексор `muxrpc` подменяет на 1..255 (`drawgettag/drawsettag` — байт 4), т.е. ≤255 одновременных RPC.

Кодирование: `u32`/`u16` — big-endian (`PUT/GET/PUT2`); `s` = `n[4 BE]+байты` (без NUL в кадре; nil→пустая); `rect` = 4×u32 BE (min.x,min.y,max.x,max.y). Пустые пакеты = 6 байт.

| Типы | Имя | C→S поля | S→C поля |
|---|---|---|---|
| 2/3 | Trdmouse/Rrdmouse | — | `x,y,buttons,msec` u32×4, `resized` u8 (=23Б) |
| 4/5 | Tmoveto/Rmoveto | `x,y` u32 | — |
| 6/7 | Tcursor/Rcursor | `off.x,off.y` u32, `clr[32]`, `set[32]`, `arrow` u8 (=79Б) | — |
| 8/9 | Tbouncemouse/Rbouncemouse | `x,y,buttons` u32×3 | — |
| 10/11 | Trdkbd/Rrdkbd | — | `rune` **u16** BE |
| 12/13 | Tlabel/Rlabel | `label` s | — |
| 14/15 | Tinit/Rinit | `winsize` s, `label` s | — |
| 16/17 | Trdsnarf/Rrdsnarf | — | `snarf` s |
| 18/19 | Twrsnarf/Rwrsnarf | `snarf` s | — |
| 20/21 | Trddraw/Rrddraw | `count` u32 | `count` u32 + `data[count]` |
| 22/23 | Twrdraw/Rwrdraw | `count` u32 + `data[count]` | `count` u32 |
| 24/25 | Ttop/Rtop | — | — |
| 26/27 | Tresize/Rresize | `rect` 4×u32 | — |
| 28/29 | Tcursor2/Rcursor2 | `off1`(u32×2),`clr1[32]`,`set1[32]`,`off2`,`clr2[64]`,`set2[64]`,`arrow` u8 (=345Б) | — |
| 30/31 | Tctxt/Rctxt | `id` s (attach, только srv-режим `$wsysid`) | — |
| 32/33 | Trdkbd4/Rrdkbd4 | — | `rune` **u32** BE |

**Семантика:** `Trdkbd/Trdmouse` — сервер копит теги в очередях (`kbdtags`/`mousetags`, srv.c) и отвечает асинхронно из `matchkbd/matchmouse`; resize клиенту доставляется **флагом `resized` в Rrdmouse** (повтор последнего события мыши), а не Rresize. `Twrdraw/Trddraw` — транспорт внутреннего однобуквенного /dev/draw-протокола (парсинг `draw_datawrite`, devdraw.c; буквы 'A','c','d','e','f','l','p','s'…, тоже BE) — детали отдельно.

**Клиент (drawclient.c):** всегда `Trdkbd4` и `Tcursor2` (Cursor масштабируется `scalecursor`); `Tcursor/Trdkbd` — legacy, по сети не ходят.

**OPEN:** комментарий в `drawfcall.h` обещает `Tinit ... font[s]` — в `convW2M` третьей строки нет, на проводе только winsize+label (устаревший комментарий).

---

## events+images

Готово. Компактная выжимка (вопрос 3):

## Два уровня протокола
Клиент (`drawclient.c`) говорит с devdraw через RPC **drawfcall** (`Wsysmsg`): пакет = `size[4] tag[1] type[1] body`, ответ `type|1` (R=T+1), tag 1..255 (mux). Draw-команды (байтовый поток, старый формат) едут внутри `Twrdraw`/`Rrddraw` и диспетчеризуются в `devdraw.c draw_datawrite`. Отдельного ответа на draw-команду нет: ошибка → `werrstr` + `Rerror` на весь `Twrdraw` (srv.c). Результаты чтений (`r`,`I`,`q`) копятся в `client->readdata` и отдаются последующим `Trddraw`.

## События — НЕ 'm'/'k'/'r'
Однобайтовые 'm'/'r'/'k' — это draw-команды ('r' = readimage), события же — RPC:

| RPC | payload (LE32, body с offset 6) |
|---|---|
| `Trdmouse`→`Rrdmouse` | `x[4] y[4] buttons[4] msec[4] resized[1]` |
| `Trdkbd`→`Rrdkbd` | `rune[2]`; `Trdkbd4`→`Rrdkbd4`: `rune[4]` |
| `Tmoveto`, `Tcursor/Tcursor2`, `Tbouncemouse`, `Tresize rect[4×4]`, `Tlabel`, `Ttop`, `Tctxt id`, `Tinit winsize+label`(Rinit пуст) | см. `sizeW2M` |

Готового «push» от сервера нет: `runmsg` кладёт tag в `mousetags`/`kbdtags` (kbd: `tag<<1 | is4`), при приходе события `matchmouse`/`matchkbd` отвечают. Буферы `Mousebuf/Kbdbuf` в devdraw.h. Клиент всегда шлёт `Trdkbd4` (`_displayrdkbd`). Alt-композиция (latin1) — серверная (`gfx_keystroke`,`kputc`).

## Resize-цикл
Сервер пересоздаёт `screenimage` платформенно (`rpc_resizeimg`/`gfx_replacescreenimage`; `Tinit`→`rpc_attach(label,winsize)`+`draw_initdisplaymemimage`). Клиенту resize доставляется флагом `resized` **внутри Rrdmouse** (после `gfx_mouseresized`→повтор последнего mouse): `mouse.c` `_ioproc` → `resizec` → приложение зовёт `getwindow()`: `bufimage 'J','I'` — `'J'` = image 0 := screenimage; `'I'` → ASCII 12×`%11d `: clientid, id, chan(строка), repl, r, clipr → парсинг; затем `'q'(1,'d')` → dpi; `allocscreen`+окно через `'A'/'b'`. Клиент может просить изменение окна: `Tresize`→`rpc_resizewindow`.

## Аллокация image
`'b'` (51 байт): `id[4] screenid[4] refresh[1] chan[4] repl[1] R[16] clipR[16] rrggbbaa[4]`. Если `screenid≠0` — окно: `chan` обязан равняться каналу экрана, `repl=0`, `memlalloc` + refresh (`Refnone/Refbackup/Refmesg`); иначе `allocmemimage(r,chan)` + `Frepl` + `memfillcolor(value)`. Ответа нет. Смежные: `'A' id[4] imageid[4] fillid[4] public[1]` (экран); `'f'/'F'` free image/screen; `'n'` attach by name; `'i' fontid[4] nchars[4] ascent[1]`; `'r' id[4] R` → пиксели (`memunload`); `'y'/'Y'` → `memload`.

## Пиксельный формат
Не фиксирован: `chan` — plan9-дескриптор (u32 `strtochan`). X11 выбирает по глубине visual (`x11-screen.c`): GREY1/2/4, CMAP8, RGB15, RGB16, RGB24, **XRGB32**; при byteswap XBGR32/BGR24. Т.е. современный X11 32bpp = XRGB32, а не гарантированный RGBA.

**OPEN:** chan в `mac-screen.c` не смотрел; эндьянность `PUT/GET/BGLONG` не сверял с макроопределениями (draw-протокол исторически little-endian — для Rust-сервера проверить по `memdraw`-макросам).
