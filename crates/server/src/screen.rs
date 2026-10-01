//! serve mode — the final glue: p9draw-server replaces devdraw on the
//! legacy pipe (SPEC.md §2.1) and renders acme into a real local window.
//!
//! drawfcall frames come in on stdin (`size[4 BE] tag[1] type[1] payload`,
//! SPEC.md §2.2), replies go out on stdout — exactly the fd pair the
//! plan9port client dup2()s its pipe onto. This module owns the client's
//! image store (render::Image keyed by the `'b'`-chosen ids) together with
//! the ScreenHost window:
//!
//! - image 0 is the screen image (XRGB32, host-sized). Its pixel layout
//!   (`b g r x` rows) is byte-identical to `ScreenHost::surface()`, so
//!   presenting is a straight copy;
//! - `'b'` with screen_id != 0 makes a window image; after every visual
//!   Twrdraw the windows are composed over image 0 in creation order and
//!   blitted to the host surface (no separate refresh machinery in v0);
//! - Trdmouse/Trdkbd4 tags park in queues and are answered from the
//!   poll_events cycle: HostEvent::Mouse → Rrdmouse (msec = /proc/uptime
//!   ms, `resized` rides byte 19 of the msec group — the drawfcall.c
//!   `p[19]` quirk, SPEC.md §4), Key → Rrdkbd4, Resize → recreate image 0
//!   and raise the resized flag (flushed to a parked Trdmouse right away,
//!   or queued as a synthetic mouse event for the next read), Close →
//!   exit. Trdmouse is answered only when the mouse state changed since
//!   the last delivered event (devdraw blocks the read; an instant
//!   unchanged reply makes the client spin the RPC at full speed);
//! - unknown drawfcall types get Rerror, so the client tells us what is
//!   still missing (SPEC.md §2.3: Rerror answers any request).
//!
//! v0 gaps (accepted, logged in the module docs): fonts ('i'/'l'/'s'/'x')
//! and compressed writes ('Y') are accepted but not rasterized, so acme
//! runs but its text is invisible; 'e'/'E'/'p'/'P' are no-ops; the cursor
//! is not themed; 'd' composes opaquely (mask/alpha ignored).

use std::collections::{HashMap, VecDeque};
use std::io::{self, Read, Write};
use std::sync::Arc;
use std::sync::mpsc::{self, RecvTimeoutError};
use std::thread;
use std::time::Duration;

use p9draw_host::{HostEvent, ScreenHost};
use p9draw_protocol::{DrawCmd, Point, Rect, Wsysmsg, decode, encode, parse_drawcmds};
use p9draw_render::{Chan, Image, compose_over, draw_tile, fill};

use crate::frameread::FrameAssembler;
use crate::pump::Logger;

/// Last resort when neither the Tinit hint nor `$WINSIZE` yields a
/// size (SPEC.md §2.4 winsize is a client hint; 900x700 is our fallback
/// window). plan9port has no env knob at all — an empty libdraw
/// `winsize` global makes x11 devdraw consult X resources — so reading
/// WINSIZE in the server is a p9draw container extension: set e.g.
/// `WINSIZE=1939x1293` when launching acme for a bigger canvas.
pub const DEFAULT_WINSIZE: (u32, u32) = (900, 700);
/// Screen image channel: byte-identical to `ScreenHost::surface()`
/// (B, G, R, X per pixel — see p9draw-host docs and SPEC.md §7).
pub const SCREEN_CHAN: Chan = Chan::XRGB32;
/// dpi reported for the `'q' "d"` query; acme scales fonts from it (192 is
/// what the live capture saw on this host).
pub const SCREEN_DPI: u32 = 192;
/// Largest Rrddraw payload per reply (devdraw's 64 KiB server buffer).
const MAX_READ_CHUNK: usize = 65536;
/// Pending-tag / rune queue cap, mirroring devdraw's 256-entry rings.
const QUEUE_CAP: usize = 256;

// --- pure helpers (unit-tested below) -------------------------------------

/// parsewinsize (SPEC.md §2.4): `WxH`, `WxH@X,Y` (position ignored in v0),
/// or the 4-number rect forms `x,y,w,h` / `x y w h` (only w×h is used —
/// window placement belongs to the host). strtol base 0 also allows hex;
/// clients send decimal, which is all we accept.
pub fn parse_winsize(s: &str) -> Result<(u32, u32), String> {
    let t = s.trim();
    if t.is_empty() {
        return Ok(DEFAULT_WINSIZE);
    }
    let base = t.split('@').next().unwrap_or(t);
    let nums: Vec<&str> = base
        .split(|c: char| c == ',' || c == ' ' || c == '\t')
        .filter(|p| !p.is_empty())
        .collect();
    if nums.len() == 4 {
        return Ok((parse_u32(nums[2])?, parse_u32(nums[3])?));
    }
    if let Some((w, h)) = base.split_once('x').or_else(|| base.split_once('X')) {
        return Ok((parse_u32(w)?, parse_u32(h)?));
    }
    Err(format!(
        "unsupported winsize {s:?} (want WxH, WxH@X,Y or x,y,w,h)"
    ))
}

fn parse_u32(s: &str) -> Result<u32, String> {
    s.trim()
        .parse::<u32>()
        .map_err(|e| format!("bad number {s:?}: {e}"))
}

/// Client winsize resolution order (SPEC.md §2.4): the Tinit hint (what a
/// plan9port client puts into libdraw's `winsize` global; acme sends ""),
/// then the `WINSIZE` environment variable, then [`DEFAULT_WINSIZE`].
/// `Err` means the chosen string did not parse — the caller logs it and
/// falls back to the default, exactly like a bad explicit hint.
fn resolve_winsize(hint: &str, env: Option<&str>) -> Result<(u32, u32), String> {
    if !hint.trim().is_empty() {
        return parse_winsize(hint);
    }
    match env.map(str::trim).filter(|s| !s.is_empty()) {
        Some(s) => parse_winsize(s),
        None => Ok(DEFAULT_WINSIZE),
    }
}

/// ms since boot from `/proc/uptime` content — the field acme's `msec`
/// deltas live in (OPEN-5, confirmed by the interactive capture). The wire
/// word is u32: values beyond ~49.7 days wrap, exactly like devdraw's
/// (OPEN-5 observed the same 64K-boundary artifact from the msec/resized
/// overlap; for the client only deltas matter).
pub fn uptime_ms_from_str(proc_uptime: &str) -> Option<u32> {
    let first = proc_uptime.split_whitespace().next()?;
    let secs: f64 = first.parse().ok()?;
    Some((secs * 1000.0).round() as u64 as u32)
}

/// Live `/proc/uptime` read; 0 on failure (event deltas degrade, protocol
/// does not break).
pub fn read_uptime_ms() -> u32 {
    std::fs::read_to_string("/proc/uptime")
        .ok()
        .and_then(|t| uptime_ms_from_str(&t))
        .unwrap_or(0)
}

/// Build the Rrdmouse payload from a host event. Coordinates are signed
/// window-local pixels; the wire halves are bit-exact two's complement.
pub fn mouse_reply(x: i32, y: i32, buttons: u8, msec: u32, resized: u8) -> Wsysmsg {
    Wsysmsg::Rrdmouse {
        x: x as u32,
        y: y as u32,
        buttons: u32::from(buttons),
        msec,
        resized,
    }
}

// --- geometry over the u32 wire types (draw math is signed) ---------------

fn sx(v: u32) -> i32 {
    v as i32
}

fn rect_dx(r: Rect) -> i32 {
    sx(r.max.x).wrapping_sub(sx(r.min.x))
}

fn rect_dy(r: Rect) -> i32 {
    sx(r.max.y).wrapping_sub(sx(r.min.y))
}

fn point_add(p: Point, dx: i32, dy: i32) -> Point {
    Point {
        x: sx(p.x).wrapping_add(dx) as u32,
        y: sx(p.y).wrapping_add(dy) as u32,
    }
}

fn isect(a: Rect, b: Rect) -> Rect {
    let x0 = sx(a.min.x).max(sx(b.min.x));
    let y0 = sx(a.min.y).max(sx(b.min.y));
    let x1 = sx(a.max.x).min(sx(b.max.x));
    let y1 = sx(a.max.y).min(sx(b.max.y));
    Rect {
        min: Point { x: x0 as u32, y: y0 as u32 },
        max: Point { x: x1 as u32, y: y1 as u32 },
    }
}

fn rect_contains(outer: Rect, inner: Rect) -> bool {
    sx(outer.min.x) <= sx(inner.min.x)
        && sx(outer.min.y) <= sx(inner.min.y)
        && sx(inner.max.x) <= sx(outer.max.x)
        && sx(inner.max.y) <= sx(outer.max.y)
}

fn rect_of(w: u32, h: u32) -> Rect {
    Rect {
        min: Point { x: 0, y: 0 },
        max: Point { x: w, y: h },
    }
}

// --- draw-stream pieces the store needs -----------------------------------

/// `'b'` allocimage: build the store image, pre-filled with `value` (the
/// allocimage fill), clipped to `clip_r`. Sub-byte channels (GREY1/2/4 —
/// acme's masks) and zero-depth descriptors become geometry-only stubs:
/// the v0 raster is byte-aligned only (render::Image::new rejects them),
/// and mask pixels are ignored by 'd' anyway, so a stub keeps acme's init
/// alive instead of Rerror-ing the whole Twrdraw.
fn make_image(
    id: u32,
    r: Rect,
    clip_r: Rect,
    chan: Chan,
    repl: bool,
    value: u32,
) -> Result<Image, String> {
    if rect_dx(r) <= 0 || rect_dy(r) <= 0 {
        return Err("bad draw command".to_string());
    }
    let depth = chan.depth();
    if depth == 0 || depth % 8 != 0 {
        return Ok(Image {
            id,
            rect: r,
            clipr: isect(clip_r, r),
            chan,
            repl,
            pixels: Vec::new(),
        });
    }
    let len = rect_dx(r) as usize * rect_dy(r) as usize * (depth / 8) as usize;
    let mut img = Image {
        id,
        rect: r,
        clipr: r,
        chan,
        repl,
        pixels: vec![0u8; len],
    };
    // The wire value is the canonical D-color RGBA (`r<<24|g<<16|b<<8|a`,
    // draw.h: DPaleyellow = 0xFFFFAAFF) — memfillcolor runs it through
    // _rgbatoimg into the image's channel format before painting. Filling
    // with the raw word misreads it as b<<0|g<<8|r<<16 and turns acme
    // paleyellow pink.
    fill(&mut img, r, chan.rgbatoimg(value));
    img.clipr = isect(clip_r, r);
    Ok(img)
}

/// Source pixel for 'L' lines: sample `img` at `p` (LE pixel word), 0 when
/// outside or sub-byte.
fn sample(img: &Image, p: Point) -> u32 {
    let bpp = (img.chan.depth() / 8) as usize;
    if bpp == 0 {
        return 0;
    }
    let x = sx(p.x) - sx(img.rect.min.x);
    let y = sx(p.y) - sx(img.rect.min.y);
    if x < 0 || y < 0 || x >= rect_dx(img.rect) || y >= rect_dy(img.rect) {
        return 0;
    }
    let bpl = rect_dx(img.rect) as usize * bpp;
    let off = y as usize * bpl + x as usize * bpp;
    if off + bpp > img.pixels.len() {
        return 0;
    }
    let mut v = 0u32;
    for i in 0..bpp {
        v |= u32::from(img.pixels[off + i]) << (8 * i);
    }
    v
}

/// 'L' line, v0: 1-px Bresenham dots in the sampled source color. end0/
/// end1/radius (caps, thickness) are not drawn.
fn draw_line(dst: &mut Image, p0: Point, p1: Point, value: u32) {
    let (mut x0, mut y0) = (sx(p0.x) as i64, sx(p0.y) as i64);
    let (x1, y1) = (sx(p1.x) as i64, sx(p1.y) as i64);
    let ddx = (x1 - x0).abs();
    let ddy = -(y1 - y0).abs();
    let mut err = ddx + ddy;
    let step_x = if x0 < x1 { 1 } else { -1 };
    let step_y = if y0 < y1 { 1 } else { -1 };
    loop {
        let dot = Rect {
            min: Point { x: x0 as u32, y: y0 as u32 },
            max: Point {
                x: (x0 + 1) as u32,
                y: (y0 + 1) as u32,
            },
        };
        fill(dst, dot, value);
        if x0 == x1 && y0 == y1 {
            break;
        }
        let e2 = 2 * err;
        if e2 >= ddy {
            err += ddy;
            x0 += step_x;
        }
        if e2 <= ddx {
            err += ddx;
            y0 += step_y;
        }
    }
}

/// 'y' memload: raw pixel rows (`bytesperline(R)·Dy(R)` bytes) into the
/// image at `r`. devdraw passes the whole rest of the write to memload;
/// we require the exact row span and reject partial writes.
fn write_pixels(img: &mut Image, r: Rect, data: &[u8]) -> Result<(), String> {
    if !rect_contains(img.rect, r) {
        return Err("writeimage outside image".to_string());
    }
    let bpp = (img.chan.depth() / 8) as usize;
    if bpp == 0 {
        return Ok(()); // stub image: nothing to fill
    }
    let w = rect_dx(r) as usize;
    let rows = rect_dy(r) as usize;
    let need = w * bpp * rows;
    if data.len() < need {
        return Err("bad draw command".to_string());
    }
    let bpl = rect_dx(img.rect) as usize * bpp;
    let x0 = (sx(r.min.x) - sx(img.rect.min.x)) as usize;
    let y0 = (sx(r.min.y) - sx(img.rect.min.y)) as usize;
    for row in 0..rows {
        let dst = (y0 + row) * bpl + x0 * bpp;
        let src = row * w * bpp;
        img.pixels[dst..dst + w * bpp].copy_from_slice(&data[src..src + w * bpp]);
    }
    Ok(())
}

/// 'r' readpixels: rows of `img` at `r` into the readdata buffer (same
/// bytesperline·Dy layout as 'y'; callers check rect containment).
fn read_pixels_into(img: &Image, r: Rect, out: &mut Vec<u8>) {
    let bpp = (img.chan.depth() / 8) as usize;
    let w = rect_dx(r) as usize;
    let rows = rect_dy(r) as usize;
    if bpp == 0 || w == 0 || rows == 0 {
        return;
    }
    let bpl = rect_dx(img.rect) as usize * bpp;
    let x0 = (sx(r.min.x) - sx(img.rect.min.x)) as usize;
    let y0 = (sx(r.min.y) - sx(img.rect.min.y)) as usize;
    for row in 0..rows {
        let off = (y0 + row) * bpl + x0 * bpp;
        out.extend_from_slice(&img.pixels[off..off + w * bpp]);
    }
}

/// One `%11d ` / `%11s ` field of the 'I' info reply — 12 bytes each,
/// 12 fields = the 144-byte ASCII block the client expects (fixture:
/// `Trddraw 145 → Rrddraw 144`).
fn info_field(v: u32) -> String {
    format!("{:>11} ", v)
}

fn info_line(clientid: u32, infoid: u32, chan: &str, repl: u32, r: Rect, clipr: Rect) -> String {
    let mut s = String::with_capacity(144);
    s.push_str(&info_field(clientid));
    s.push_str(&info_field(infoid));
    s.push_str(&format!("{:>11} ", chan));
    s.push_str(&info_field(repl));
    for v in [
        r.min.x,
        r.min.y,
        r.max.x,
        r.max.y,
        clipr.min.x,
        clipr.min.y,
        clipr.max.x,
        clipr.max.y,
    ] {
        s.push_str(&info_field(v));
    }
    s
}

// --- the screen: image store + window host --------------------------------

/// Client-visible screen. Single-threaded, driven by [`serve_stdio`].
pub struct Screen {
    host: ScreenHost,
    /// draw-stream images by client id; id 0 is the screen image.
    images: HashMap<u32, Image>,
    /// 'A'/'S' screens: id → (image id, fill image id) — bookkeeping only;
    /// v0 composites windows straight onto image 0.
    screens: HashMap<u32, (u32, u32)>,
    /// window image ids ('b' with screen_id != 0), creation order.
    windows: Vec<u32>,
    /// readback bytes ('I'/'q'/'r') drained by Trddraw.
    readdata: Vec<u8>,
    /// runes waiting for a Trdkbd4/Trdkbd.
    kbdq: VecDeque<u32>,
    /// tags of Trdmouse/Trdkbd4/Trdkbd that arrived with no event yet.
    mousetags: VecDeque<u8>,
    kbd4tags: VecDeque<u8>,
    kbdlegtags: VecDeque<u8>,
    /// last mouse state, plan9 button mask (1/2/4, 8/16 = wheel).
    mouse: Option<(i32, i32, u8)>,
    /// `mouse` changed since the last delivered Rrdmouse. A Trdmouse is
    /// answered only when this is set — devdraw blocks the read otherwise,
    /// and an instant unchanged reply makes the client spin the RPC.
    mouse_fresh: bool,
    /// set by Resize, carried by the next Rrdmouse (SPEC.md §5), then reset.
    resized: bool,
    dpi: u32,
    /// constant client id reported in 'I' info (devdraw assigns 1).
    clientid: u32,
    /// in-memory clipboard (v0: not the system snarf).
    snarf: String,
    /// current window size in physical pixels (image 0 geometry).
    win: (u32, u32),
}

impl Screen {
    fn init(winsize: &str, label: &str, logger: &Logger) -> Result<Screen, String> {
        let env_ws = std::env::var("WINSIZE").ok();
        let (w, h) = match resolve_winsize(winsize, env_ws.as_deref()) {
            Ok(wh) => wh,
            Err(e) => {
                logger.log(&format!(
                    "serve: {e}; using {}x{}",
                    DEFAULT_WINSIZE.0, DEFAULT_WINSIZE.1
                ));
                DEFAULT_WINSIZE
            }
        };
        let title = if label.is_empty() { "p9draw" } else { label };
        let host = ScreenHost::open(title, w, h).map_err(|e| e.to_string())?;
        let mut sc = Screen {
            host,
            images: HashMap::new(),
            screens: HashMap::new(),
            windows: Vec::new(),
            readdata: Vec::new(),
            kbdq: VecDeque::new(),
            mousetags: VecDeque::new(),
            kbd4tags: VecDeque::new(),
            kbdlegtags: VecDeque::new(),
            mouse: None,
            mouse_fresh: false,
            resized: false,
            dpi: SCREEN_DPI,
            clientid: 1,
            snarf: String::new(),
            win: (w, h),
        };
        let img = Image::new(0, rect_of(w, h), SCREEN_CHAN)
            .map_err(|e| format!("screen image: {e}"))?;
        sc.images.insert(0, img);
        Ok(sc)
    }

    fn lookup_mut(&mut self, id: u32) -> Result<&mut Image, String> {
        self.images
            .get_mut(&id)
            .ok_or_else(|| "unknown id for draw image".to_string())
    }

    fn remove_image(&mut self, id: u32) -> Result<Image, String> {
        self.images
            .remove(&id)
            .ok_or_else(|| "unknown id for draw image".to_string())
    }

    /// Apply one Twrdraw payload. Ok(dirty) — dirty means pixels changed
    /// (or a flush arrived) and the screen should be re-presented.
    fn apply(&mut self, data: &[u8]) -> Result<bool, String> {
        let cmds = parse_drawcmds(data).map_err(|_| "bad draw command".to_string())?;
        let mut dirty = false;
        for cmd in cmds {
            match cmd {
                DrawCmd::Allocate { id, screen_id, refresh: _, chan, repl, r, clip_r, value } => {
                    if id == 0 {
                        return Err("image id in use".to_string());
                    }
                    let img = make_image(id, r, clip_r, Chan(chan), repl != 0, value)?;
                    self.images.insert(id, img);
                    if screen_id != 0 {
                        self.windows.retain(|w| *w != id);
                        self.windows.push(id);
                        dirty = true;
                    }
                }
                DrawCmd::AllocScreen { id, image_id, fill_id, public: _ } => {
                    self.screens.insert(id, (image_id, fill_id));
                }
                DrawCmd::PublicScreen { id, chan: _ } => {
                    // public screen attaches to image 0 in v0
                    self.screens.insert(id, (0, 0));
                }
                DrawCmd::ReplClip { dst_id, repl, clip_r } => {
                    let img = self.lookup_mut(dst_id)?;
                    img.repl = repl != 0;
                    img.clipr = isect(clip_r, img.rect);
                }
                DrawCmd::Draw { dst_id, src_id, mask_id: _, r, src_pt, mask_pt: _ } => {
                    if src_id == dst_id {
                        // self-copy needs the image twice: clone the source.
                        let src = self
                            .images
                            .get(&src_id)
                            .ok_or_else(|| "unknown id for draw image".to_string())?
                            .clone();
                        let dst = self
                            .images
                            .get_mut(&dst_id)
                            .ok_or_else(|| "unknown id for draw image".to_string())?;
                        if src.repl {
                            draw_tile(dst, r, &src, src_pt);
                        } else {
                            let src_rect = Rect {
                                min: src_pt,
                                max: point_add(src_pt, rect_dx(r), rect_dy(r)),
                            };
                            compose_over(dst, r, &src, src_rect);
                        }
                    } else {
                        let src = self.remove_image(src_id)?;
                        if !self.images.contains_key(&dst_id) {
                            self.images.insert(src_id, src);
                            return Err("unknown id for draw image".to_string());
                        }
                        {
                            let dst = self.images.get_mut(&dst_id).expect("checked above");
                            if src.repl {
                                draw_tile(dst, r, &src, src_pt);
                            } else {
                                let src_rect = Rect {
                                    min: src_pt,
                                    max: point_add(src_pt, rect_dx(r), rect_dy(r)),
                                };
                                compose_over(dst, r, &src, src_rect);
                            }
                        }
                        self.images.insert(src_id, src);
                    }
                    dirty = true;
                }
                DrawCmd::Line { dst_id, p0, p1, end0: _, end1: _, radius: _, src_id, sp } => {
                    let value = if src_id == dst_id {
                        let img = self
                            .images
                            .get(&src_id)
                            .ok_or_else(|| "unknown id for draw image".to_string())?;
                        sample(img, sp)
                    } else {
                        let src = self.remove_image(src_id)?;
                        if !self.images.contains_key(&dst_id) {
                            self.images.insert(src_id, src);
                            return Err("unknown id for draw image".to_string());
                        }
                        let value = sample(&src, sp);
                        self.images.insert(src_id, src);
                        value
                    };
                    let dst = self.images.get_mut(&dst_id).expect("checked above");
                    draw_line(dst, p0, p1, value);
                    dirty = true;
                }
                DrawCmd::Free { id } => {
                    if id == 0 {
                        return Err("unknown id for draw image".to_string());
                    }
                    self.images.remove(&id);
                    self.windows.retain(|w| *w != id);
                }
                DrawCmd::FreeScreen { id } => {
                    self.screens.remove(&id);
                }
                DrawCmd::Image0Screen => {
                    if !self.images.contains_key(&0) {
                        let img = Image::new(0, rect_of(self.win.0, self.win.1), SCREEN_CHAN)
                            .map_err(|_| "image memory allocation failed".to_string())?;
                        self.images.insert(0, img);
                    }
                }
                DrawCmd::ReadInfo => {
                    let img = self
                        .images
                        .get(&0)
                        .ok_or_else(|| "unknown id for draw image".to_string())?;
                    let line = info_line(
                        self.clientid,
                        0,
                        &img.chan.to_string(),
                        u32::from(img.repl),
                        img.rect,
                        img.clipr,
                    );
                    self.readdata.extend_from_slice(line.as_bytes());
                }
                DrawCmd::Query { specs } => {
                    for &s in &specs {
                        if s == b'd' {
                            self.readdata
                                .extend_from_slice(format!("{:>11} ", self.dpi).as_bytes());
                        } else {
                            return Err("unknown draw query".to_string());
                        }
                    }
                }
                DrawCmd::ReadPixels { id, r } => {
                    let img = self
                        .images
                        .get(&id)
                        .ok_or_else(|| "unknown id for draw image".to_string())?;
                    if !rect_contains(img.rect, r) {
                        return Err("readimage outside image".to_string());
                    }
                    read_pixels_into(img, r, &mut self.readdata);
                }
                DrawCmd::WritePixels { id, r, data } => {
                    let img = self.lookup_mut(id)?;
                    write_pixels(img, r, &data)?;
                    dirty = true;
                }
                DrawCmd::WriteCompressed { .. } => {
                    // plan9-compressed pixel stream: not decoded in v0; the
                    // batch survives, the image content is not updated.
                }
                DrawCmd::Flush => {
                    dirty = true;
                }
                DrawCmd::InitFont { .. }
                | DrawCmd::LoadFont { .. }
                | DrawCmd::String { .. }
                | DrawCmd::StringBg { .. } => {
                    // fonts: accepted, nothing rasterized (v0 gap — acme
                    // text stays invisible until a font raster lands)
                }
                DrawCmd::Ellipse { .. } | DrawCmd::Polygon { .. } | DrawCmd::FillPolygon { .. } => {
                    // arc/polygon raster: v0 gap
                }
                DrawCmd::AttachNamed { .. } | DrawCmd::NameImage { .. } => {
                    // named images: not modeled (single client)
                }
                DrawCmd::Position { .. }
                | DrawCmd::SetOp { .. }
                | DrawCmd::Debug { .. }
                | DrawCmd::Top { .. } => {
                    // window-manager chrome without v0 visual effect
                }
                DrawCmd::Unknown { op: _ } => {
                    return Err("bad draw command".to_string());
                }
            }
        }
        Ok(dirty)
    }

    /// Composite windows over image 0 and publish to the host surface.
    fn present(&mut self) {
        let mut scr = match self.images.remove(&0) {
            Some(s) => s,
            None => return,
        };
        for id in &self.windows {
            if let Some(w) = self.images.get(id) {
                compose_over(&mut scr, w.rect, w, w.rect);
            }
        }
        {
            let surface = self.host.surface();
            let n = surface.len().min(scr.pixels.len());
            surface[..n].copy_from_slice(&scr.pixels[..n]);
        }
        self.images.insert(0, scr);
        self.host.present();
    }

    fn on_mouse(&mut self, x: i32, y: i32, buttons: u8, out: &mut impl Write) -> io::Result<()> {
        self.mouse = Some((x, y, buttons));
        if let Some(tag) = self.mousetags.pop_front() {
            self.mouse_fresh = false;
            let reply = mouse_reply(x, y, buttons, read_uptime_ms(), u8::from(self.resized));
            self.resized = false;
            send(&reply, tag, out)?;
        } else {
            // No reader waiting: keep the state parked as the answer to the
            // next Trdmouse (plan9 allows replying from the latest mouse).
            self.mouse_fresh = true;
        }
        Ok(())
    }

    fn on_key(&mut self, c: char, out: &mut impl Write) -> io::Result<()> {
        let rune = c as u32;
        if let Some(tag) = self.kbd4tags.pop_front() {
            send(&Wsysmsg::Rrdkbd4 { rune }, tag, out)?;
        } else if let Some(tag) = self.kbdlegtags.pop_front() {
            send(&Wsysmsg::Rrdkbd { rune: rune as u16 }, tag, out)?;
        } else if self.kbdq.len() < QUEUE_CAP {
            self.kbdq.push_back(rune);
        }
        Ok(())
    }

    fn on_resize(&mut self, w: u32, h: u32, out: &mut impl Write) -> io::Result<()> {
        if w == 0 || h == 0 {
            return Ok(());
        }
        self.win = (w, h);
        if let Ok(img) = Image::new(0, rect_of(w, h), SCREEN_CHAN) {
            self.images.insert(0, img);
            self.resized = true;
            self.present();
            // devdraw reports a resize as a mouse event promptly (the
            // client re-inits on resized=1): answer a parked read now
            // instead of waiting for the next motion, or queue the
            // synthetic event for the next Trdmouse.
            if let Some(tag) = self.mousetags.pop_front() {
                let (x, y, buttons) = self.mouse.unwrap_or((0, 0, 0));
                let reply = mouse_reply(x, y, buttons, read_uptime_ms(), u8::from(self.resized));
                self.resized = false;
                self.mouse_fresh = false;
                send(&reply, tag, out)?;
            } else {
                self.mouse_fresh = true;
            }
        }
        Ok(())
    }

    /// Pump one host event. Ok(true) → window closed, hang up.
    fn handle_event(&mut self, ev: HostEvent, out: &mut impl Write) -> io::Result<bool> {
        match ev {
            HostEvent::Mouse { x, y, buttons } => self.on_mouse(x, y, buttons, out)?,
            HostEvent::Key(c) => self.on_key(c, out)?,
            HostEvent::Resize { w, h } => self.on_resize(w, h, out)?,
            HostEvent::Close => return Ok(true),
        }
        Ok(false)
    }

    /// Answer one decoded client frame (request side of the RPC rules,
    /// SPEC.md §2.3: reply type = request type + 1 on the same tag).
    fn handle(&mut self, tag: u8, msg: Wsysmsg, out: &mut impl Write) -> io::Result<()> {
        match msg {
            Wsysmsg::Trdmouse => {
                // Answer only when the state changed since the last
                // delivered event — devdraw blocks the read; replying
                // instantly with unchanged state makes the client spin the
                // RPC at full speed (the idle 87% CPU burn).
                if self.mouse_fresh {
                    let (x, y, buttons) = self.mouse.expect("fresh state implies a known mouse");
                    let reply =
                        mouse_reply(x, y, buttons, read_uptime_ms(), u8::from(self.resized));
                    self.resized = false;
                    self.mouse_fresh = false;
                    send(&reply, tag, out)?;
                } else if self.mousetags.len() >= QUEUE_CAP {
                    send(
                        &Wsysmsg::Rerror {
                            error: "too many queued mouse reads".to_string(),
                        },
                        tag,
                        out,
                    )?;
                } else {
                    self.mousetags.push_back(tag);
                }
            }
            Wsysmsg::Trdkbd4 => {
                if let Some(rune) = self.kbdq.pop_front() {
                    send(&Wsysmsg::Rrdkbd4 { rune }, tag, out)?;
                } else if self.kbd4tags.len() >= QUEUE_CAP {
                    send(
                        &Wsysmsg::Rerror {
                            error: "too many queued keyboard reads".to_string(),
                        },
                        tag,
                        out,
                    )?;
                } else {
                    self.kbd4tags.push_back(tag);
                }
            }
            Wsysmsg::Trdkbd => {
                if let Some(rune) = self.kbdq.pop_front() {
                    send(&Wsysmsg::Rrdkbd { rune: rune as u16 }, tag, out)?;
                } else if self.kbdlegtags.len() >= QUEUE_CAP {
                    send(
                        &Wsysmsg::Rerror {
                            error: "too many queued keyboard reads".to_string(),
                        },
                        tag,
                        out,
                    )?;
                } else {
                    self.kbdlegtags.push_back(tag);
                }
            }
            Wsysmsg::Twrdraw { data } => {
                match self.apply(&data) {
                    Ok(dirty) => {
                        if dirty {
                            self.present();
                        }
                        send(&Wsysmsg::Rwrdraw { count: data.len() as u32 }, tag, out)?;
                    }
                    Err(e) => send(&Wsysmsg::Rerror { error: e }, tag, out)?,
                }
            }
            Wsysmsg::Trddraw { count } => {
                let n = (count as usize)
                    .min(self.readdata.len())
                    .min(MAX_READ_CHUNK);
                let data = self.readdata.drain(..n).collect();
                send(&Wsysmsg::Rrddraw { data }, tag, out)?;
            }
            Wsysmsg::Tlabel { label: _ } => {
                send(&Wsysmsg::Rlabel, tag, out)?;
            }
            Wsysmsg::Tmoveto { x: _, y: _ } => {
                // cursor warp: ScreenHost has no warp in v0
                send(&Wsysmsg::Rmoveto, tag, out)?;
            }
            Wsysmsg::Tbouncemouse { x, y, buttons } => {
                self.on_mouse(x as i32, y as i32, buttons as u8, out)?;
                send(&Wsysmsg::Rbouncemouse, tag, out)?;
            }
            Wsysmsg::Tcursor { cursor: _ } => {
                send(&Wsysmsg::Rcursor, tag, out)?;
            }
            Wsysmsg::Tcursor2 { cursor: _ } => {
                send(&Wsysmsg::Rcursor2, tag, out)?;
            }
            Wsysmsg::Trdsnarf => {
                send(
                    &Wsysmsg::Rrdsnarf {
                        snarf: self.snarf.clone(),
                    },
                    tag,
                    out,
                )?;
            }
            Wsysmsg::Twrsnarf { snarf } => {
                self.snarf = snarf;
                send(&Wsysmsg::Rwrsnarf, tag, out)?;
            }
            Wsysmsg::Ttop => {
                send(&Wsysmsg::Rtop, tag, out)?;
            }
            Wsysmsg::Tresize { rect: _ } => {
                // window size requests are not applied in v0 (the user owns
                // the window manager); the client re-inits on resized=1
                send(&Wsysmsg::Rresize, tag, out)?;
            }
            Wsysmsg::Tctxt { id: _ } => {
                // devdraw accepts Tctxt in any mode (SPEC.md §2.1)
                send(&Wsysmsg::Rctxt, tag, out)?;
            }
            Wsysmsg::Tinit { .. } => {
                send(
                    &Wsysmsg::Rerror {
                        error: "already initialized".to_string(),
                    },
                    tag,
                    out,
                )?;
            }
            _ => {
                // reply-typed or unknown frames from a client
                send(
                    &Wsysmsg::Rerror {
                        error: "unexpected message type for a client".to_string(),
                    },
                    tag,
                    out,
                )?;
            }
        }
        Ok(())
    }
}

fn send(msg: &Wsysmsg, tag: u8, out: &mut impl Write) -> io::Result<()> {
    out.write_all(&encode(msg, tag))?;
    out.flush()
}

// --- the serve loop --------------------------------------------------------

/// `p9draw-server serve`: drawfcall on stdin → replies on stdout, acme in
/// a real window. A reader thread assembles frames (FrameAssembler) into a
/// channel; the main loop pumps host events and answers frames. stdin EOF
/// or HostEvent::Close ends the process — the pipe lifetime IS the session
/// lifetime, like real devdraw. The frame channel doubles as the sleep:
/// `recv_timeout(16 ms)` bounds the wake cadence with the window open, so
/// an idle session costs ~0% CPU; `present` runs only on real pixel changes.
pub fn serve_stdio(logger: Arc<Logger>) -> Result<(), String> {
    let (tx, rx) = mpsc::channel::<Vec<u8>>();
    let log_reader = Arc::clone(&logger);
    thread::spawn(move || {
        let stdin = io::stdin();
        let mut input = stdin.lock();
        let mut asm = FrameAssembler::new();
        let mut chunk = [0u8; 8192];
        let mut frames: Vec<Vec<u8>> = Vec::new();
        loop {
            match input.read(&mut chunk) {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    frames.clear();
                    if asm.feed(&chunk[..n], &mut frames).is_err() {
                        log_reader.log("serve: client lost frame alignment; hanging up");
                        break;
                    }
                    let mut peer_gone = false;
                    for f in frames.drain(..) {
                        if tx.send(f).is_err() {
                            peer_gone = true;
                            break;
                        }
                    }
                    if peer_gone {
                        break;
                    }
                }
            }
        }
    });

    let mut screen: Option<Screen> = None;
    let mut out = io::stdout().lock();
    loop {
        if let Some(sc) = screen.as_mut() {
            for ev in sc.host.poll_events() {
                let exit = sc
                    .handle_event(ev, &mut out)
                    .map_err(|e| format!("stdout: {e}"))?;
                if exit {
                    logger.log("serve: window closed; exiting");
                    return Ok(());
                }
            }
        }
        let idle = Duration::from_millis(if screen.is_some() { 16 } else { 200 });
        match rx.recv_timeout(idle) {
            Ok(frame) => {
                let (tag, msg) = match decode(&frame) {
                    Ok(v) => v,
                    Err(e) => {
                        logger.log(&format!("serve: undecodable frame: {e}"));
                        let tag = frame.get(4).copied().unwrap_or(0);
                        send(
                            &Wsysmsg::Rerror {
                                error: format!("bad message: {e}"),
                            },
                            tag,
                            &mut out,
                        )
                        .map_err(|e| format!("stdout: {e}"))?;
                        continue;
                    }
                };
                if screen.is_none() {
                    match msg {
                        Wsysmsg::Tinit { winsize, label } => {
                            match Screen::init(&winsize, &label, &logger) {
                                Ok(mut sc) => {
                                    sc.mouse = Some((0, 0, 0));
                                    sc.present();
                                    send(&Wsysmsg::Rinit, tag, &mut out)
                                        .map_err(|e| format!("stdout: {e}"))?;
                                    logger
                                        .log(&format!("serve: Tinit winsize={winsize:?} label={label:?}"));
                                    screen = Some(sc);
                                }
                                Err(e) => {
                                    send(&Wsysmsg::Rerror { error: e }, tag, &mut out)
                                        .map_err(|e| format!("stdout: {e}"))?;
                                }
                            }
                        }
                        _ => {
                            send(
                                &Wsysmsg::Rerror {
                                    error: "Tinit expected first".to_string(),
                                },
                                tag,
                                &mut out,
                            )
                            .map_err(|e| format!("stdout: {e}"))?;
                        }
                    }
                } else {
                    let sc = screen.as_mut().expect("checked above");
                    sc.handle(tag, msg, &mut out)
                        .map_err(|e| format!("stdout: {e}"))?;
                }
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => {
                logger.log("serve: client closed stdin; exiting");
                return Ok(());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use p9draw_protocol::{Point, Rect, Wsysmsg, decode, encode};

    // --- winsize (SPEC.md §2.4 parsewinsize) ------------------------------

    #[test]
    fn winsize_empty_falls_back_to_default() {
        assert_eq!(parse_winsize(""), Ok(DEFAULT_WINSIZE));
        assert_eq!(parse_winsize("   "), Ok(DEFAULT_WINSIZE));
    }

    #[test]
    fn winsize_parses_wxh() {
        assert_eq!(parse_winsize("900x700"), Ok((900, 700)));
        assert_eq!(parse_winsize("1024X768"), Ok((1024, 768)));
    }

    #[test]
    fn winsize_position_suffix_is_ignored() {
        assert_eq!(parse_winsize("1024x768@10,20"), Ok((1024, 768)));
    }

    #[test]
    fn winsize_rect_forms_yield_wh() {
        assert_eq!(parse_winsize("100,200,800,600"), Ok((800, 600)));
        assert_eq!(parse_winsize("100 200 800 600"), Ok((800, 600)));
    }

    #[test]
    fn winsize_rejects_garbage() {
        assert!(parse_winsize("hello").is_err());
        assert!(parse_winsize("100x").is_err());
        assert!(parse_winsize("x700").is_err());
    }

    #[test]
    fn make_image_converts_d_color_values_per_memdraw() {
        // draw.h D-colors ride the wire as canonical RGBA (alpha in the
        // low byte): DPaleyellow = 0xFFFFAAFF. memfillcolor converts via
        // _rgbatoimg to the chan format — x8r8g8b8 memory bytes are
        // [b g r x] = AA FF FF 00 (pale yellow), not the raw-word bytes
        // FF AA FF FF (pale pink).
        let img = make_image(
            1,
            rect_of(2, 1),
            rect_of(2, 1),
            Chan::XRGB32,
            false,
            0xFFFF_AAFF,
        )
        .unwrap();
        assert_eq!(
            img.pixels,
            vec![0xAA, 0xFF, 0xFF, 0x00, 0xAA, 0xFF, 0xFF, 0x00]
        );
        // DDarkyellow = 0xEEEE9EFF — acme's selection-highlight tile.
        let img = make_image(
            2,
            rect_of(1, 1),
            rect_of(1, 1),
            Chan::XRGB32,
            false,
            0xEEEE_9EFF,
        )
        .unwrap();
        assert_eq!(img.pixels, vec![0x9E, 0xEE, 0xEE, 0x00]);
        // Alpha is dropped when the chan has no alpha channel; DRed is red.
        let img = make_image(
            3,
            rect_of(1, 1),
            rect_of(1, 1),
            Chan::XRGB32,
            false,
            0xFF00_00FF,
        )
        .unwrap();
        assert_eq!(img.pixels, vec![0x00, 0x00, 0xFF, 0x00]);
    }

    #[test]
    fn winsize_hint_then_env_then_default() {
        assert_eq!(resolve_winsize("800x600", Some("1939x1293")), Ok((800, 600)));
        assert_eq!(resolve_winsize("", Some("1939x1293")), Ok((1939, 1293)));
        assert_eq!(resolve_winsize("   ", None), Ok(DEFAULT_WINSIZE));
        assert_eq!(resolve_winsize("", Some("  ")), Ok(DEFAULT_WINSIZE));
        // A bad explicit hint stays an error (caller logs + defaults).
        assert!(resolve_winsize("bogus", None).is_err());
        // So does a bad env value when the hint is empty.
        assert!(resolve_winsize("", Some("bogus")).is_err());
    }

    // --- uptime ms (OPEN-5: msec base) -------------------------------------

    #[test]
    fn uptime_reads_first_proc_field() {
        assert_eq!(uptime_ms_from_str("13422.30 53777.04"), Some(13_422_300));
        assert_eq!(uptime_ms_from_str("15.53"), Some(15_530));
    }

    #[test]
    fn uptime_rejects_junk() {
        assert_eq!(uptime_ms_from_str(""), None);
        assert_eq!(uptime_ms_from_str("abc def"), None);
    }

    #[test]
    fn uptime_wraps_like_the_u32_wire_word() {
        // 5e6 s = 5e9 ms > u32::MAX: devdraw's msec wraps; client deltas stay sane.
        assert_eq!(uptime_ms_from_str("5000000.00"), Some(705_032_704));
    }

    #[test]
    fn uptime_matches_the_live_interactive_fixture() {
        // fixtures-analysis.md OPEN-5: first live Rrdmouse msec =
        // 1 342 223 032 ms ≈ 15.53 days of hlab uptime.
        assert_eq!(
            uptime_ms_from_str("1342223.032 53777.04"),
            Some(1_342_223_032)
        );
    }

    // --- 'I' info reply (fixture: Trddraw 145 → Rrddraw 144) ----------------

    #[test]
    fn info_line_is_the_144_byte_ascii_block() {
        let r = Rect {
            min: Point { x: 0, y: 0 },
            max: Point { x: 1939, y: 1293 },
        };
        let line = info_line(1, 0, "x8r8g8b8", 0, r, r);
        assert_eq!(line.len(), 144);
        assert_eq!(&line[0..12], "          1 ");
        assert_eq!(&line[12..24], "          0 ");
        assert_eq!(&line[24..36], "   x8r8g8b8 ");
        assert!(line.contains("       1939 "));
    }

    // --- Rrdmouse encoding: the p[19] quirk (SPEC.md §4) --------------------

    #[test]
    fn rrdmouse_p19_quirk_exact_fixture_bytes() {
        // msec with bits 16..23 set (0x55 in byte 1) + resized=1: the flag
        // OVERWRITES byte 1 of the msec group (drawfcall.c p[19]); byte 22
        // is the never-written pad, encoded as 0.
        let f = encode(&mouse_reply(1, 2, 4, 0x0055_0041, 1), 7);
        assert_eq!(f.len(), 23);
        let expected: [u8; 23] = [
            0x00, 0x00, 0x00, 0x17, // size = 23, BE
            0x07, // tag
            0x03, // Rrdmouse
            0x00, 0x00, 0x00, 0x01, // x = 1
            0x00, 0x00, 0x00, 0x02, // y = 2
            0x00, 0x00, 0x00, 0x04, // buttons = 4
            0x00, 0x01, 0x00, 0x41, // msec group; byte 1 = resized, NOT 0x55
            0x00, // pad (convW2M never writes it; we emit 0)
        ];
        assert_eq!(f, expected);
        let (tag, back) = decode(&f).unwrap();
        assert_eq!(tag, 7);
        match back {
            Wsysmsg::Rrdmouse { x, y, buttons, msec, resized } => {
                assert_eq!((x, y, buttons), (1, 2, 4));
                // convM2W reads all four msec bytes: bits 16..23 now carry the flag.
                assert_eq!(msec, 0x0001_0041);
                assert_eq!(resized, 1);
            }
            other => panic!("wrong message: {other:?}"),
        }
    }

    #[test]
    fn rrdmouse_zero_resized_matches_fixture_pattern() {
        // All 456 live Rrdmouse frames: frame[19] == 0 when no resize,
        // frame[22] unwritten (we pad 0).
        let f = encode(&mouse_reply(10, 20, 0, 1234, 0), 2);
        assert_eq!(f[19], 0);
        assert_eq!(f[22], 0);
        assert_eq!(&f[18..22], &[0x00, 0x00, 0x04, 0xD2]);
    }

    #[test]
    fn mouse_reply_wraps_negatives_and_passes_buttons() {
        let f = encode(&mouse_reply(-1, -2, 5, 7, 0), 1);
        assert_eq!(&f[6..10], &[0xFF, 0xFF, 0xFF, 0xFF]);
        assert_eq!(&f[10..14], &[0xFF, 0xFF, 0xFF, 0xFE]);
        let (_, back) = decode(&f).unwrap();
        match back {
            Wsysmsg::Rrdmouse { buttons, .. } => assert_eq!(buttons, 5),
            other => panic!("wrong message: {other:?}"),
        }
    }
}
