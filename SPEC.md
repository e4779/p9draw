# p9draw — спецификация wire-протокола (drawfcall / Wsysmsg)

Версия: 1.0 (2026-10-01). Основа: plan9port (9fans), выжимки
`docs/research.md` + прямая сверка с исходниками.

**Источники (проверено чтением):**
`include/drawfcall.h` (словарь типов, PUT/GET), `src/libdraw/drawfcall.c`
(sizeW2M/convW2M/convM2W/readwsysmsg), `src/cmd/devdraw/srv.c`
(dispatch, очереди, reply), `src/cmd/devdraw/devdraw.c`
(внутренний draw-поток), `src/cmd/devdraw/devdraw.h` (кольца 256),
`src/libdraw/drawclient.c` (транспорт, mux 1..255),
`src/cmd/devdraw/winsize.c`, `src/lib9/getns.c`, `include/draw.h`
(BPLONG/каналы), `include/cursor.h`, `src/libdraw/chan.c`.

---

## 1. Обзор: два уровня протокола

| Уровень | Что это | Где определён |
|---|---|---|
| **drawfcall (Wsysmsg)** | RPC клиент↔сервер: кадры `size[4] tag[1] type[1] payload`. События мыши/клавы, resize, snarf, окно | `drawfcall.h`, `drawfcall.c`, `srv.c` |
| **внутренний draw-поток** | однобуквенные команды `/dev/draw` ('b','A','S',…): аллокация image, рисование, загрузка пикселей. Едет **внутри** `Twrdraw`/`Trddraw` | `devdraw.c draw_datawrite/read` |

Разное endianness: drawfcall — **big-endian**; внутренний draw-поток —
**little-endian** (`BPSHORT/BPLONG`: `p[0]=v, p[1]=v>>8`, draw.h:527–530).

---

## 2. Транспорт и handshake

### 2.1 Два режима (выбор на клиенте — `_displayconnect`, libdraw/drawclient.c)

| Режим | Условие | Транспорт |
|---|---|---|
| Legacy (по умолчанию) | нет `$wsysid` | `pipe()` + `fork`; потомок `dup2(pipe,0/1)`, `NOLIBTHREADDAEMONIZE=1`, `execl($DEVDRAW \|\| "devdraw", argv0, argv0, "(devdraw)")`; родитель общается через pipe |
| Сервер | `$wsysid="name/id"` | `dial("unix!$NAMESPACE/name")`; **первый кадр обязан быть `Tctxt{id}`** (id — часть после `/`); сервер `Rctxt` = успех, `Rerror` = разрыв. Далее fd = канал протокола |

Сервер (`devdraw -s srvname`, srv.c `gfx_started`): путь сокета =
`unix!$NAMESPACE/srvname`. Namespace (`getns`, lib9/getns.c):
`$NAMESPACE`, иначе `/tmp/ns.<user>.<display>`; display
канонизируется `xxx:0.0` → `xxx:0`, `/` → `_`.

В дереве plan9port **никто не выставляет `$wsysid`** — это делает внешний
wrapper (см. OPEN-1). `Tctxt` сервер принимает в любом режиме.

### 2.2 Фрейминг (оба направления)

```
size[4] BE = ПОЛНЫЙ размер кадра, включая эти 4 байта
tag[1]     = offset 4 (u8)
type[1]    = offset 5 (u8)
payload[]  = offset 6…
```

Чтение: `readn(4)` → `readn(size−4)` (`readwsysmsg`, `serveproc`).
Минимальный кадр — 6 байт. `MAXWMSG = 4 MiB` объявлен в drawfcall.h
(лимит на клиентской стороне; сервер в serveproc явного лимита не ставит —
буфер растёт под кадр; см. OPEN-4).

### 2.3 Правила RPC

- **Чётный type = запрос (T, клиент→сервер); ответ = T+1**
  (`replymsg`: `if(type%2==0) type++`). Единственное исключение —
  `Rerror=1`: ответ-ошибка на любой запрос (payload `error[s]`).
- Клиент заполняет `tag=0`; мультиплексор (`muxrpc`, mintag=1, maxtag=255)
  подменяет байт 4 на тег 1..255 ⇒ **≤255 одновременных RPC**.
  Клиент ждёт ответ с `type == tx.type+1` на своём теге.
- `Rrdmouse`/`Rrdkbd(4)` приходят **асинхронно**: сервер ставит тег в
  очередь (`mousetags`/`kbdtags`) и отвечает, когда событие произошло;
  порядок ответов не гарантирован.

### 2.4 Последовательность подключения

```
[сервер-режим]  dial → Tctxt{id} → Rctxt
любой режим:    Tinit{winsize[s], label[s]} → Rinit
                (сервер: rpc_attach → создаёт окно/screenimage →
                 draw_initdisplaymemimage)
далее:          Trdkbd4, Trdmouse (циклы событий), Twrdraw('J','I','b','A',…)
```

**Порядок полей Tinit: сначала `winsize`, потом `label`**
(convW2M: `PUTSTRING(winsize); PUTSTRING(label)`). Комментарий в
drawfcall.h «Tinit winsize[s] label[s] font[s]» устарел: **поля `font`
на проводе нет**.

Формат `winsize` (parsewinsize, strtol base 0): `WxH`, `WxH@X,Y`,
`x,y,w,h` или `x y w h` (4 числа — прямоугольник с заданным min).

### 2.5 Клиентские обёртки (что реально ходит по сети)

libdraw шлёт только: `Trdkbd4` (не Trdkbd), `Tcursor2` (не Tcursor;
Cursor2 получается `scalecursor`), `Tinit`, `Tmoveto`, `Tbouncemouse`,
`Tlabel`, `Trdsnarf`, `Twrsnarf`, `Trddraw`, `Twrdraw`, `Ttop`, `Tresize`.
`Tcursor`/`Trdkbd` поддерживаются сервером для совместимости.

---

## 3. Кодирование примитивов (drawfcall-слой)

| Тип | Кодирование |
|---|---|
| u32 | 4 байта **BE** (`PUT/GET`: `p[0]=v>>24 … p[3]=v`) |
| u16 | 2 байта **BE** (`PUT2/GET2`) |
| строка `s` | `n[4 BE] + n байт`, **без NUL**; nil → `n=0` (пустая) |
| Rectangle | `min.x, min.y, max.x, max.y` — 4×u32 BE (16 байт) |
| Point | `x, y` — 2×u32 BE (8 байт) |
| uchar-массив | сырые байты |

Размер кадра фиксирован для большинства типов (см. таблицу) —
кодирование 1:1 повторяет `sizeW2M`/`convW2M` (drawfcall.c).

---

## 4. Таблица сообщений drawfcall (все 33 типа)

Размер — полный, байт (кадр целиком, включая 4-байтовый префикс).
Payload начинается с offset 6. Порядок полей — порядок на проводе.

| Тип | Имя | Направление | Размер | Поля |
|---:|---|---|---:|---|
| 1 | Rerror | S→C | 6+len | `error[s]` (строка ≤255 байт + NUL в источнике) |
| 2 | Trdmouse | C→S | 6 | — (запрос следующего события мыши) |
| 3 | Rrdmouse | S→C | 23 | `x[4] y[4] buttons[4] msec[4]` (u32 BE); `resized[1]` пишется поверх байта 1 группы msec (offset 19), байт 22 кадра — пад |
| 4 | Tmoveto | C→S | 14 | `x[4] y[4]` (warp курсора) |
| 5 | Rmoveto | S→C | 6 | — |
| 6 | Tcursor | C→S | 79 | `off.x[4] off.y[4] clr[32] set[32] arrow[1]` |
| 7 | Rcursor | S→C | 6 | — |
| 8 | Tbouncemouse | C→S | 18 | `x[4] y[4] buttons[4]` (синтетическое событие) |
| 9 | Rbouncemouse | S→C | 6 | — |
| 10 | Trdkbd | C→S | 6 | — (legacy, rune u16) |
| 11 | Rrdkbd | S→C | 8 | `rune[2]` (u16 BE) |
| 12 | Tlabel | C→S | 10+ | `label[s]` |
| 13 | Rlabel | S→C | 6 | — |
| 14 | Tinit | C→S | 14+ | `winsize[s] label[s]` — **именно в этом порядке** |
| 15 | Rinit | S→C | 6 | — |
| 16 | Trdsnarf | C→S | 6 | — |
| 17 | Rrdsnarf | S→C | 10+ | `snarf[s]` |
| 18 | Twrsnarf | C→S | 10+ | `snarf[s]` |
| 19 | Rwrsnarf | S→C | 6 | — |
| 20 | Trddraw | C→S | 10 | `count[4]` (читать ≤count байт draw-потока) |
| 21 | Rrddraw | S→C | 10+n | `count[4] data[count]` (≤65536 за раз — серверный буфер) |
| 22 | Twrdraw | C→S | 10+n | `count[4] data[count]` (команды draw-потока) |
| 23 | Rwrdraw | S→C | 10 | `count[4]` (подтверждение; ошибка → Rerror) |
| 24 | Ttop | C→S | 6 | — (поднять окно) |
| 25 | Rtop | S→C | 6 | — |
| 26 | Tresize | C→S | 22 | `rect` = `min.x[4] min.y[4] max.x[4] max.y[4]` (просьба о размере окна) |
| 27 | Rresize | S→C | 6 | — |
| 28 | Tcursor2 | C→S | 343 | `off[8] clr[32] set[32] off2[8] clr2[128] set2[128] arrow[1]` |
| 29 | Rcursor2 | S→C | 6 | — |
| 30 | Tctxt | C→S | 10+ | `id[s]` (attach в серверном режиме, первый кадр) |
| 31 | Rctxt | S→C | 6 | — |
| 32 | Trdkbd4 | C→S | 6 | — (запрос следующей клавиши, rune u32) |
| 33 | Rrdkbd4 | S→C | 10 | `rune[4]` (u32 BE) |

Примечания:
- `clr`/`set` — битмаски курсора 16×16 (`Cursor`: offset Point + 2×16
  байт), `clr2`/`set2` — 32×32 (2×32 байта) (`cursor.h`).
- `arrow=1` → системный курсор-стрелка (сервер вызывает setcursor(nil)).
- `Tcursor2` ровно 343 байта: 6+8+32+32+8+128+128+1
  (в research.md стояло 345 — ошибка, исправлено по sizeW2M).
- `resized` в Rrdmouse — u8 0/1, но на проводе он **не** отдельный байт
  payload: convW2M делает `PUT(p+18, msec); p[19] = resized` — флаг
  пишется поверх байта 1 группы msec, биты 16..23 msec по проводу не
  передаются (клиент видит их нулями). Байт 22 кадра sizeW2M считает,
  но convW2M туда не пишет: на проводе там мусор переиспользуемого
  буфера (в capture 2026-10-01 — стабильный `0x20`). Приёмник (convM2W)
  читает msec все 4 байта и resized из p[19]; байт 22 не читает никто.

### Канонические строки ошибок (Rerror payload)

Сервер копирует `rerrstr` (буфер 256). Известные строки devdraw
(пригодны для golden-тестов): `bad draw command`, `short draw message`,
`unknown id for draw image`, `unknown id for draw screen`,
`image id in use`, `screen id in use`, `image memory allocation failed`,
`readimage outside image`, `writeimage outside image`, `image not a font`,
`character index out of range`, `no image with that name`,
`named image no longer valid`, `image already has name`,
`wrong name for image`, `out of memory`, `bad argument in draw message`,
`too many queued mouse reads`, `too many queued keyboard reads`.

---

## 5. События: мышь, клавиатура, resize

Все очереди на сервере — кольца на **256** элементов (devdraw.h).

### Мышь
- `Trdmouse` → тег в `mousetags`; при переполнении очереди тегов →
  Rerror `too many queued mouse reads`.
- События копятся в `Mousebuf` (256). Пока читатель не забирает
  (`stall`), новые ставятся в очередь **только при смене buttons**;
  координаты clamp в `mouserect`.
- `buttons`: бит0=левая, бит1=средняя, бит2=правая (platform-swap через
  `mouseswap`); `msec` — время события в мс от платформенного ввода.
- Сервер отвечает из `matchmouse`: берёт тег из очереди + событие из
  кольца; после отправки `resized` сбрасывается.

### Resize
- **Push-сообщения нет.** Изменение размера доставляется флагом
  `resized=1` внутри ближайшего `Rrdmouse` (сервер повторяет последнее
  событие мыши — `gfx_mouseresized` → `gfx_mousetrack(-1,-1,-1,-1)`).
- Получив `resized`, клиент вызывает `getwindow()`: `Twrdraw('J')`
  (image 0 := screenimage), `Twrdraw('I')` + `Trddraw` (ASCII-инфо),
  `Twrdraw('q','d')` + `Trddraw` (dpi), затем `'A'`/`'b'` — screen/окно.
- Клиент может попросить размер сам: `Tresize{rect}` → `rpc_resizewindow`.

### Клавиатура
- `Trdkbd4` → тег в `kbdtags` как `(tag<<1) | (is4)`; переполнение →
  Rerror `too many queued keyboard reads`. Ответ — `Rrdkbd4` (или
  `Rrdkbd` для legacy-запросов) через `matchkbd`.
- Кольцо `Kbdbuf` (256 rune); при заполнении — stall.
- **Alt-композиция — на сервере**: `Kalt` переключает alting-режим,
  набранная последовательность прогоняется через `latin1()`; отмена по
  клику мыши (`gfx_abortcompose`).
- `Kcmd+'r'`: тумблер DPI 100↔225 (`forcedpi`; при displaydpi≥200 → 100)
  + пересоздание screenimage.

---

## 6. Внутренний draw-протокол (payload Twrdraw/Trddraw)

Поток команд, склеенных вплотную; каждая начинается байтом-буквой.
**Все числа — little-endian** (BGLONG/BPSHORT). Смещения — от начала
команды (offset 0 = сама буква). «n» ниже — длина команды.

Ошибка любой команды (`draw_datawrite` < 0) → **Rerror на весь Twrdraw**;
часть потока до ошибки может быть уже применена (сервер идёт по потоку
последовательно). Результаты чтений (`'I'`, `'q'`, `'r'`) буферизуются в
`client->readdata` и отдаются следующим `Trddraw` (≤65536 байт за раз).

| Буква | Имя | n (байт) | Поля (LE, если не указано) |
|---|---|---:|---|
| `'b'` | allocimage | 51 | `id[4]` @1, `screenid[4]` @5 (читается как **u16** — старшие 2 байта игнорируются), `refresh[1]` @9 (0=Refbackup, 1=Refnone, 2=Refmesg), `chan[4]` @10, `repl[1]` @14, `R[16]` @15, `clipR[16]` @31, `value[4]` @47 (цвет по каналу). screenid≠0 → окно: chan обязан = каналу экрана, repl=0 |
| `'A'` | allocscreen | 14 | `id[4]` @1, `imageid[4]` @5 (image экрана), `fillid[4]` @9, `public[1]` @13 |
| `'S'` | use public screen | 9 | `id[4]` @1, `chan[4]` @5 (должен совпасть с каналом экрана). Это НЕ handshake-версия — версий в протоколе нет |
| `'c'` | set repl/clip | 22 | `dstid[4]`, `repl[1]` @5, `clipR[16]` @6 |
| `'d'` | draw (composite) | 45 | `dstid[4]`, `srcid[4]` @5, `maskid[4]` @9, `R[16]` @13, `P[8]` @29 (src pt), `P[8]` @37 (mask pt) |
| `'D'` | debug | 2 | `val[1]` |
| `'e'` | ellipse | 45 | `dstid[4]`, `srcid[4]` @5, `center[8]` @9, `a[4]` @17, `b[4]` @21, `thick[4]` @25, `sp[8]` @29, `ox[4]` @37, `oy[4]` @41. `'E'` = filled. bit31 у `ox` → дуга (углы в ox/oy) |
| `'f'` | free image | 5 | `id[4]` |
| `'F'` | free screen | 5 | `id[4]` |
| `'i'` | init font | 10 | `fontid[4]`, `nchars[4]` @5 (≤4096), `ascent[1]` @9 |
| `'J'` | image0 := screen | 1 | — |
| `'I'` | read image info | 1 | → readdata: 12 полей по `%11d ` (144 байта ASCII): clientid, infoid, **chan-строка** (`%11s`), repl, r.min.x/y, r.max.x/y, clipr ×4 |
| `'q'` | query | 2+n | `n[1]` @1, `n×queryspec`; поддержан только `'d'` (dpi) → `%11d ` на каждый |
| `'l'` | load char | 37 | `fontid[4]`, `srcid[4]` @5, `index[2]` @9, `R[16]` @11, `P[8]` @27, `left[1]` @35, `width[1]` @36 |
| `'L'` | line | 45 | `dstid[4]`, `p0[8]` @5, `p1[8]` @13, `end0[4]` @21, `end1[4]` @25, `radius[4]` @29, `srcid[4]` @33, `sp[8]` @37 |
| `'n'` | attach named | 6+n | `dstid[4]`, `j[1]` @5 (n>0), `name[j]` |
| `'N'` | name image | 7+n | `dstid[4]`, `in[1]` @5 (1=поставить имя, 0=снять), `j[1]` @6, `name[j]` |
| `'o'` | position window | 21 | `id[4]`, `r.min[8]` @5, `screenr.min[8]` @13 |
| `'O'` | set op | 2 | `op[1]` (для следующей draw-операции) |
| `'p'` | polygon | 31+pts | `dstid[4]`, `n[2]` @5, `end0[4]` @7, `end1[4]` @11, `radius[4]` @15, `srcid[4]` @19, `sp[8]` @23, `p0[8]` @31, затем **n+1** точек в `drawcoord`-кодировании |
| `'P'` | fill polygon | 31+pts | как `'p'`, но @7=`wind[4]`, @11=`ignore[4]` (radius нет) |
| `'r'` | read pixels | 21 | `id[4]`, `R[16]` @5; R обязан лежать внутри image → readdata = `bytesperline(R,depth)·Dy(R)` байт |
| `'s'` | string | 47+2·ni | `dstid[4]`, `srcid[4]` @5, `fontid[4]` @9, `P[8]` @13, `clipR[16]` @21, `sp[8]` @37, `ni[2]` @45, затем `ni×index[2]` |
| `'x'` | stringbg | 59+2·ni | как `'s'` + `bgid[4]` @47, `bgpt[8]` @51 |
| `'t'` | top/bottom | 4+4·nw | `top[1]` @1, `nw[2]` @2, `nw×id[4]` |
| `'v'` | flush | 1 | — |
| `'y'` | write pixels | 21+data | `id[4]`, `R[16]` @5, `data[…]` — ровно `bytesperline(R,depth)·Dy(R)` байт (memload: libmemdraw/load.c:14-17, devdraw.c:1425 `m += y`); depth — канал image из `'b'` этого же потока, иначе v0-fallback: хвост payload |
| `'Y'` | write compressed | 21+data | как `'y'`, данные в сжатом формате image; v0: длина выводится только декодированием → data = хвост payload, acme шлёт `'Y'` последней командой |

`'m'` (create image mask) в коде **закомментирован** — не реализован.

Статус p9draw (`screen.rs` `apply_one`): применяются `'b'`/`'A'`/`'S'`/`'c'`/`'d'`/`'f'`/`'F'`/`'J'`/`'I'`/`'q'`/`'r'`/`'y'`/`'Y'`/`'v'`/`'t'`/`'o'` и все шрифтовые `'i'`/`'l'`/`'s'`/`'x'` — шрифт заводится `'i'` на картинку клиента, `'l'` копирует глиф и метрики в кэш, `'s'`/`'x'` рисуют строку из этого кэша (маска — клетка font-image, серые маски GREY1..GRE8 блендятся; базлайн `P` = `pt.y+ascent` клиента, `clipR` заменяет `clipr` на время команды). Остаточный v0 gap: `'e'`/`'E'`/`'p'`/`'P'` принимаются без растеризации, именованные картинки и `'O'` SetOp не смоделированы. `'v'` = drawflush (devdraw.c:1406) — 1 байт, полей id/rect нет, в трассе `P9DRAW_TRACE` они печатаются как `-`. Rerror называет команду и причину: ошибки парсинга — op-байт и offset (`ProtocolError` Display), ошибки применения — префикс `draw op '<буква>':`.

### Кодирование точек полигона (`drawcoord`)

```
b = u8;
x = b & 0x7F;
если b & 0x80:  // 3-байтовая форма, АБСОЛЮТНОЕ значение
    x |= u8<<7; x |= u8<<15; если bit22 → знакорасширение до 32 бит
иначе:          // 1-байтовая форма, ДЕЛЬТА от предыдущей координаты
    если b & 0x40 → знакорасширение 7 бит
    x += oldx
```

---

## 7. Пиксельный формат

Формат пикселей **не фиксирован** — определяется дескриптором канала
(`chan`, u32), который клиент передаёт в `'b'`/`'S'` и получает строкой
в ответе `'I'`.

- Каждый байт дескриптора: `(тип<<4) | глубина` (`__DC`);
  типы (`channames = "rgbkamx"`): r=0, g=1, b=2, k=3 (grey), a=4 (alpha),
  m=5 (палитра), x=6 (ignore). **Первый канал строки = старший байт**
  дескриптора (`CHAN1<<24 | …`).
- `chantostr` перечисляет каналы от **старшего** байта дескриптора
  к младшему (первая пара строки = старший байт): RGB24 (0x081828) →
  `"r8g8b8"`, XRGB32 → `"x8r8g8b8"`, GREY8 → `"k8"`, CMAP8 → `"m8"`.
- Глубина = сумма глубин каналов (chantodepth).

Именованные значения (draw.h, посчитано по `__DC`):

| Константа | hex | Строка |
|---|---|---|
| GREY1 | 0x31 | k1 |
| GREY2 | 0x32 | k2 |
| GREY4 | 0x34 | k4 |
| GREY8 | 0x38 | k8 |
| CMAP8 | 0x58 | m8 |
| RGB15 | 0x61051525 | x1r5g5b5 |
| RGB16 | 0x051625 | r5g6b5 |
| RGB24 | 0x081828 | r8g8b8 |
| BGR24 | 0x281808 | b8g8r8 |
| RGBA32 | 0x08182848 | r8g8b8a8 |
| ARGB32 | 0x48081828 | a8r8g8b8 |
| ABGR32 | 0x48281808 | a8b8g8r8 |
| XRGB32 | 0x68081828 | x8r8g8b8 |
| XBGR32 | 0x68281808 | x8b8g8r8 |

X11 выбирает канал по глубине visual (research, x11-screen.c):
GREY1/2/4, CMAP8, RGB15, RGB16, RGB24, XRGB32; при byteswap-дисплее
XBGR32/BGR24. Поле `value` в `'b'` — канонический RGBA u32
(`r<<24|g<<16|b<<8|a`, draw.h D-цвета: `DPaleyellow` = `0xFFFFAAFF`),
memdraw прогоняет его через `_rgbatoimg` в формат канала (OPEN-3 закрыт).

---

## 8. OPEN-вопросы

1. **`$wsysid` снаружи.** Формат `"name/id"` подтверждён
   (libdraw/drawclient.c), но в дереве plan9port никто эту переменную не
   выставляет — внешний wrapper (напр. менеджер сессий). Правила генерации
   `name`/`id` вне исследования.
   **Статус: STILL-OPEN** — capture 2026-10-01 шёл в legacy-режиме
   (первый кадр = `Tinit`, `Tctxt` в потоке нет; docs/fixtures-analysis.md):
   acme сам `$wsysid` не ставит; генерация name/id не наблюдаема — нужен
   захват серверного режима.
2. **chan на macOS** (`mac-screen.c`) не проверен — какой канал даёт
   платформа, снесено из research без сверки.
   **Статус: STILL-OPEN** — capture 2026-10-01 сделан на Linux/X11
   (экран x8r8g8b8, 192 dpi); mac-screen.c по-прежнему не сверен.
3. **Порядок байтов пикселя в памяти** относительно chan-строки и упаковка
   `value` в `'b'` — сверено с libmemdraw (`memsetchan`, `_rgbatoimg`,
   `_memfillcolor`).
   **Статус: CONFIRMED (провод и память; libmemdraw/draw.c, alloc.c).**
   Провод: `chan` — LE u32, байт канала = `(код<<4)|nbits`: `x8r8g8b8` =
   `0x68081828`, GREY1 = `0x31`; `value` — LE u32 в каноническом RGBA
   (D-цвета: `r<<24|g<<16|b<<8|a`). Память: `memsetchan` даёт сдвиги от
   последнего байта строки (b8→0, g8→8, r8→16, x8→24), т.е. на LE-хосте
   байты пикселя `[B,G,R,X]`; `value` конвертируется `_rgbatoimg`
   (серые — fixed-point `RGB2K >>19`). Реализация: `Chan::rgbatoimg`
   (crates/render) + `make_image` (crates/server).
4. **Сервер не ограничивает размер кадра** (`serveproc` растит буфер под
   `size[4]` без MAXWMSG-проверки; 4 MiB — клиентский лимит). Для p9draw
   нужно выбрать и задокументировать политику.
   **Статус: STILL-OPEN** — в capture 2026-10-01 максимальный кадр 235 B,
   лимит не нагружался.
5. **`msec` мыши** — платформенное время события; единицы/эпоха на
   конкретных бэкендах не сверялись.
   **Статус: CONFIRMED-by-capture-2026-10-01-interactive (Linux/X11).**
   456 `Rrdmouse`: msec = ms с загрузки ОС (uptime) — первое значение
   1 342 223 032 мс ≈ 15.53 сут совпадает с /proc/uptime хоста на момент
   захвата; дельты соседних событий 1–20 мс (медиана 18). Единственный
   аномальный скачок −65522 — 64K-граница из-за перекрытия бит 16..23
   msec с `resized` (§4). Для клиента значимы только дельты. macOS-бэкенд
   по-прежнему не сверялся.
6. **bit30 в `ox` команды `'e'`/`'E'`** — условия упаковки углов дуги
   (`memarc`) не вскрывались; описано только поведение bit31.
   **Статус: STILL-OPEN** — команд `'e'`/`'E'` в capture 2026-10-01 нет.
7. **Мультиклиентная маршрутизация** (кто раздаёт `wsysid`, поведение
   нескольких соединений на один screen) — в дереве нет `9pserve`/
   `post9pservice`; семантика вне одиночного сервера неизвестна.
   **Статус: STILL-OPEN** — capture 2026-10-01: один клиент в legacy-режиме,
   серверный режим не захватывался.

Снято (больше не OPEN): «версия 'S'» — handshake версией протокол не
имеет вовсе, `'S'` — опкод public screen; комментарий `font[s]` в Tinit —
устарел, поля на проводе нет (проверено convW2M); LE внутреннего потока —
подтверждено (draw.h:527–530); Tcursor2 = 343 Б (в research было 345).

Сверка с живым трафиком (fixtures/live-acme, MITM p9draw, 2026-10-01,
разбор: docs/fixtures-analysis.md; тест: crates/protocol/tests/
live_capture.rs): фрейминг §2.2 (BE, size включает себя) — 68/68 кадров;
строки §3 и порядок winsize→label §2.4 — подтверждены; переиспользование
tag=1 после ответа §2.3 — подтверждено; LE внутреннего потока §6 (id,
rect, chan-дескриптор 0x68081828 = x8r8g8b8, GREY1 = 0x31) — подтверждён;
каждый draw-RPC получил ответ до EOF (30 Twrdraw↔30 Rwrdraw,
2 Trddraw↔2 Rrddraw, ноль Rerror).
