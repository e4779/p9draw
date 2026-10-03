---
type: research
title: "p9draw: текст и шрифты — дизайн-документ"
generated:
  by: human:e4779
  at: "2026-10-03T18:14:16Z"
---

# p9draw: текст и шрифты — дизайн-документ

Статус: исследование готово, реализация не начата. Дата подготовки: окт 2026.
Источники: plan9port @ `~/.cache/checkouts/github.com/9fans/plan9port`
(`cmd/devdraw/devdraw.c`, `cmd/fontsrv/`, `libdraw/{font,string,stringbg,openfont,getsubfont,readsubfont,buildfont}.c`),
`SPEC.md` §4/§6, живые захваты `fixtures/live-acme{,-interactive}` + `docs/fixtures-analysis.md`,
текущее состояние контейнера acme-web (read-only инспекция при подготовке).

## 1. TL;DR

- На уровне wsysmsg (33 типа, SPEC §4) **шрифтовых типов НЕТ**: `Tinit` — это winsize+label
  окна, `Tcursor` — курсор. Гипотеза «Tloadfont/Tloadsubfont» не подтвердилась — таких
  сообщений в протоколе не существует. ВЕСЬ шрифт живёт **внутри draw-потока** `Twrdraw`
  как команды: `'i'` (инициализация шрифта на cache-image), `'l'` (загрузка глифа в кэш),
  `'s'`/`'x'` (рендер строки, stringbg).
- Реальный devdraw **не открывает шрифтовых файлов**: кэш глифов — обычная картинка,
  созданная клиентом (`'b'`), заполненная клиентом (`'y'`/`'Y'` биты сабфонтов + `'l'`
  копии глифов с метриками). Строку `'s'`/`'x'` сервер рендерит **из этого кэша**.
- acme считает layout **полностью локально** из шрифтовых файлов; сервер о метриках
  не спрашивается (единственный серверный round-trip — `'q'` dpi для hidpi-свопа).
  Следствие: p9draw может не иметь ни одного шрифтового файла и рисовать текст
  байт-в-байт как настоящий devdraw.
- **Рекомендация** (§7): протокольно-точная реализация `'i'`/`'l'`/`'s'`/`'x'` поверх уже
  существующего композита (`compose_over_masked`, grey-маски блендятся с коммита
  «drawcmd encoder…»): ~220–320 LoC, ноль новых зависимостей, fontsrv не нужен.

## 2. Wire-семантика шрифтовых команд (devdraw.c)

Форматы и смещения — таблица в SPEC §6; здесь семантика и канонические ошибки.
Источники: `devdraw.c`:572 (drawchar), 885 ('i'), 991 ('l'), 1273 ('s'/'x'), 1413 ('y');
`devdraw.h`:134 (FChar).

### `'i'` — initialize font (10 B)
`fontid[4] nchars[4] ascent[1]`. Превращает существующую картинку в шрифт: выделяет
`fchar[nchars]` (нулями), `ascent` = u8. Валидации: id≠0 («can't use display as font»),
картинка не окно/layer («can't use window as font»), 1≤nchars≤4096 («bad font size
(4096 chars max)»), иначе «unknown id for draw image». Повторный `'i'` на том же id
легален — fchar перевыделяется, старые глифы забываются (клиент так делает fontresize
при расширении кэша). FChar = {minx,maxx:int; miny,maxy:u8; left:i8; width:u8}.

### `'l'` — load char (37 B)
`fontid[4] srcid[4] index[2] R[16] P[8] left[1] width[1]`. Копирует прямоугольник R из
картинки srcid в картинку шрифта (маска memopaque — простое копирование) и запоминает
`fc[index] = {minx=R.min.x, maxx=R.max.x, miny=R.min.y, maxy=R.max.y, left, width}`.
Ошибки: «image not a font» (nfchar==0), «character index out of range» (index≥nfchar).
Клиент шлёт `'l'` на **каждый** cache-miss (font.c loadchar, ровно 37-байтный буфер).

### `'s'` — string (47+2·ni B), `'x'` — stringbg (59+2·ni B)
`dstid srcid fontid P clipR sp ni [bgid bgpt] ni×index[2]` (LE, смещения — SPEC §6).

- **P — базлайн строки**: клиент шлёт `pt.y + font->ascent` (string.c:111-112).
- **clipR на время команды заменяет dst->clipr**; после команды (и при ошибке) восстанавливается.
- Валидация: `nfchar==0` → «image not a font»; **все** индексы проверяются до рисования
  (в `'x'` — в первом проходе подсчёта ширины фона): плохой индекс → «character index
  out of range», при этом фон ещё не нарисован.
- **Фон `'x'`**: rect = (p.x, p.y−ascent)…(p.x+Σwidth, p.y−ascent+Dy(font.image.r)),
  где Σwidth = сумма fchar[ci].width по индексам; рисуется **до** символов:
  `memdraw(dst, r, bgid, bgpt, memopaque, ZP, op)` — заливка фоном-паттерном.
- **Символ** (drawchar, devdraw.c:572-589):
  - маска = клетка font-image `[fc.minx..maxx, fc.miny..maxy]`;
  - экран rect = (p.x+fc.left, p.y−(ascent−fc.miny))…(+maxx−minx, +maxy−miny);
  - src-паттерн от sp+(fc.left, fc.miny); blit текущим op (дефолт SoverD);
  - p.x += fc.width; sp.x += fc.width.
- **dstflush**: rect (p.x, p.y−ascent)…(q.x, p.y−ascent+Dy(font.image.r)), q.x — конечный p.x.

## 3. Клиентский путь: что именно шлёт acme/libdraw

### 3.1 Открытие шрифта (openfont.c, buildfont.c, getsubfont.c, readsubfont.c)
1. `openfont(d, name)` → parsefontscale («12*name»), ветки:
   `*default*` → встроенный defontdata; `/lib/font/bit/…` → unsharp → `$PLAN9/font/…`
   (статические файлы plan9port); `/mnt/font/…` → `_fontpipe` (см. §4); иначе прямой open.
2. `.font`-файл — текст: первая строка `height ascent` (buildfont.c:46-51), далее строки
   `min max offset subfontname` → buildfont → Font с Cachefont[] (диапазоны рун).
3. Сабфонт лениво: `_getsubfont` → readsubfonti: bits = readimage (по проводу `'b'` +
   `'y'`/`'Y'`), затем заголовок 3×12 ASCII (n, ascent, height) + (n+1) записей Fontchar
   по 6 байт LE (x:u16, top:u8, bottom:u8, left:i8, width:u8) → allocsubfont.

### 3.2 Кэш глифов (font.c)
- `Font.cache[]` (Cacheinfo: x, width, left, value, age): хеш-веер NFLOOK=5, LRU-age,
  MAXFCACHE=1029. При нехватке/расширении — fontresize: `allocimage` GREY
  (depth = max depth сабфонтов) + `'i'` на сервере. **4 `'i'` в живом потоке = 4
  (ре)аллокации cache-images** шрифтов acme.
- `loadchar`: Cachefont по диапазону рун → подгрузка сабфонта (lookupsubfont/_getsubfont)
  → `'l'` копия глифа в кэш → Cacheinfo. Сабфонты вытесняются по age (SUBFAGE=10000, MAXSUBF=50).

### 3.3 Строка (_string, string.c:104-160)
Чанки по **≤100 символов**; на чанк — **одна** `'s'`/`'x'`: ids, P=(pt.x, pt.y+ascent),
clipr=dst.clipr, sp, ni, [bgid, bgpt], индексы кэш-ячеек (ushort) из cachechars;
bgp.x тоже сдвигается на ширину чанка. После чанка — agefont. При неудаче сабфонта —
фолбэк на default font.

### 3.4 КРИТИЧНОЕ: layout не зависит от сервера
Высоты/ширины acme берёт **из файла локально** (openfont синхронно читает файл до любого
draw-сообщения; stringwidth/charwidth — локальные функции над Fontchar). По проводу
метрики едут только внутри `'i'` (ascent) и `'l'` (границы клетки, left, width).
Сервер не участвует в layout и не обязан знать файлы шрифтов.

## 4. fontsrv: механика и статус в контейнере acme-web
- Режимы (fontsrv/main.c): `fontsrv [-m mtpt] [-s srvname]` — 9P-демон: дерево
  fontdir/sizedir/fontfile/subfontfile, размеры 4…28, флаг antialias, чанки рун;
  растеризация системных TTF/OTF через FreeType (mksubfont, x11.c). И `fontsrv -p path`
  (pflag; libdraw зовёт `fontsrv -pp <path>` — pflag=2 → **dump файла в stdout**):
  `_fontpipe` (openfont.c:267-307) запускает его **подпроцессом на каждое открытие**
  `/mnt/font/*`-имени и читает файл из pipe (маркер успеха '\001'). Никакого
  монтирования и $NAMESPACE не требуется.
- Контейнер acme-web (podman quadlet `acme-web.container`, webtop ubuntu-kde, 3005/3006):
  plan9port собран в `/config/plan9port` (custom-cont-init.d/10-plan9port.sh, идемпотентно);
  `bin/fontsrv` **собран, но не запущен** (read-only ps внутри контейнера; 9pserve держит
  только /mnt/acme). acme использует дефолты `fontnames[2] =
  /lib/font/bit/lucsans/euro.8.font` и `/lib/font/bit/lucm/unicode.9.font` (acme.c:40-44)
  → unsharp → статические файлы `/config/plan9port/font/**`. $FONT не выставлен —
  fontsrv для этого сценария не нужен вообще.
- Статус платформы: в контейнере уже два acme — старый на `devdraw.real`, новый (18:31)
  на `/config/plan9port/bin/p9draw-server serve`: serve-режим p9draw уже обслуживает
  живой acme, отсутствие текста — единственный гэп (что и требовалось закрыть данным исследованием).

## 5. Полная wire-последовательность текста (по живому capture)
1. `Tinit`/`Rinit` (legacy; SPEC §2.4): winsize=""+label="acme".
2. draw-init: `'J'` (image0:=screen), `'I'` (→144 B ASCII: chan x8r8g8b8, 1939×1293),
   `'q' 'd'` (→192 dpi).
3. `'b'` тайлы (GREY1 бел/чёрн, repl=1), `'A'` allocscreen, `'b'` окно (screenid≠0).
4. На каждый шрифт, лениво по мере строк: `'b'` cache-image (GREY1..GREY8 по depth
   сабфонтов) → `'i'` (nchars, ascent) → `'b'`+`'y'`/`'Y'` bits-image сабфонта →
   `'l'`×N на cache-miss.
5. Текст: `'x'` чанками ≤100 поверх фона + `'v'` flush. В interactive-захвате:
   631 Twrdraw, «первые опы кадров»: v 461, d 48, b 46, **l 62, i 4**, f 6, A/J/q/Y по 1;
   `'s'`/`'x'` появляются внутри многооповых кадров (парсятся без ошибок).

## 6. Что реализовать в p9draw-server (подсистемы)
Место: `crates/server/src/screen.rs`, apply_one (сейчас ветки 659-662 — заглушки:
`InitFont | LoadFont | String | StringBg`; имена вариантов protocol-крейта: 'l' = LoadFont).
Хранилище: registry `HashMap<u32, FontData>` (или Option<FontData> внутри Image):

- **Ш1. FontData + `'i'` InitFont** — ascent, `fchar: Vec<FChar>`; валидации §2 с
  каноническими строками ошибок. ~40 LoC.
- **Ш2. `'l'` LoadFont** — копия rect src→font-image (blit-путь уже есть у 'y'/'d'),
  запись fc. ~50 LoC.
- **Ш3. `'s'` String** — цикл drawchar на `compose_over_masked` (маска = клетка
  font-image; grey-blend уже реализован для 'd'); семантика clipr-swap; advance p/sp;
  dirty. ~70 LoC.
- **Ш4. `'x'` StringBg** — bg-rect (ascent, Σwidth, Dy(font r)) + заливка фоном
  (repl-паттерн → tile-путь; без repl → compose, как в 'd'), затем цикл Ш3. ~40 LoC.
- **Ш5. trace/docs** — буквы i/l/s/x в `trace.rs::op_letter` (проверить покрытие),
  README + SPEC §6 статус. ~20 LoC.

Итого ~220–320 LoC кода+тестов, **0 новых зависимостей**. `'O'` SetOp не трогаем
(acme не шлёт; SoverD — дефолт и в devdraw).

## 7. Откуда глифы: варианты и рекомендация

**A. Протокольно-точно: клиент присылает глифы сам (реализация §6). — РЕКОМЕНДУЕТСЯ.**
Плюсы: это ровно то, что делает настоящий devdraw — байт-в-байт совместимость с acme;
ноль шрифтовых файлов и зависимостей на сервере; layout никогда не расходится (метрики
одни и те же — клиентские); маленький дифф; тестопригодность (битмапы уже в живых
фикстурах). Минусы: сервер сам не может нарисовать текст вне клиентского потока (не нужно
по ТЗ); antialias-глифы требуют точного grey-blend (уже есть, см. OPEN-1).

**B. fontsrv по 9P из контейнера / серверные subfont-файлы plan9port. — Отклонить.**
Плюсы: теоретически сервер мог бы рендерить текст без клиента. Минусы: **не нужно** —
acme шлёт глифы сам; дублирование метрик гарантированно рассинхронизирует layout
(acme мерит строку файлом, сервер рисовал бы другими); пути/$PLAN9 внутри контейнера;
лишняя движущаяся часть. fontsrv остаётся чисто клиентской опцией для юзера
(FONT=/mnt/font/… — acme сам дёрнет `fontsrv -pp` сабпроцессом).

**C. Своя растеризация TTF (ab_glyph и т.п.) на сервере. — Отклонить.**
Плюсы: независимость от шрифтов клиента. Минусы: +зависимости; надо воспроизводить
plan9-метрики (Fontchar x/top/bottom/left/width, ascent) или жить с расхождением layout:
acme измеряет строку СВОИМИ метриками, а рисовалось бы ДРУГИМИ глифами → текст съезжает;
сотни KB кода ради гэпа, которого в протоколе нет.

## 8. Тест-план (по ступеням §6)
- **Ш1**: unit — init валидный; ошибки id=0 / окно / nchars=4097 / unknown — точные строки.
- **Ш2**: unit — `'l'` копирует биты (GREY1 клетка 3×5), поля fc == присланным; ошибки
  not-a-font / index.
- **Ш3/Ш4**: unit golden — строка "Hi" GREY1-маской на белом поле → попиксельный буфер;
  `'x'`: фон закрашен ровно Σwidth×Dy(font r) при базлайне P; clipr ограничивает;
  плохой индекс в середине → Rerror «character index out of range», фон не нарисован,
  восстановлен clipr (семантика SPEC §6: часть потока до ошибки применена).
- **Replay**: все 631 Twrdraw interactive-фикстуры через apply() → 0 ошибок + golden-hash
  фреймбуфера (каркас по образцу `crates/server/tests/pipe_capture.rs`;
  парсинг уже зелёный в `crates/protocol/tests/interactive_capture.rs`).
- **e2e** (после деплоя главным агентом): живой acme в контейнере — виден текст тегов/тел,
  скрин в fixtures/.
- **Регресс**: `cargo test` воркспейса остаётся зелёным (сейчас 122).

## 9. OPEN-вопросы
1. **Grey-blend pixel-exact**: GREY8 (antialias) маска — точное совпадение формулы
   compose_over_masked с memdraw SoverD по каналам. Для GREY1-шрифтов acme это точная
   копия; риск только с fontsrv-antialias. Проверить визуально на e2e-скрине.
2. **bg без repl в `'x'`**: acme шлёт repl-паттерны; поведение memdraw при выходе за r
   (clamp) — зафиксировать выбранное поведение в тесте.
3. **dstflush**: v0 репрезентит весь экран при dirty — оставить; точные dirty-rects не
   критичны для webtop.
4. **`'O'` SetOp для строк** — не делать до появления в живом трафике.
