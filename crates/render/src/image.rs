//! Software image and the v0 raster operations: `fill`, `compose_over`,
//! `draw_tile` (SPEC.md §6 'd' semantics, §7 pixel format).

use std::fmt;

use p9draw_protocol::{Point, Rect};

use crate::chan::Chan;

/// Software image: the plan9 `Image` subset carried by the draw stream
/// (SPEC.md 'b' allocimage: id, rect, clipr, chan, repl).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Image {
    /// Draw-stream image id ('b' `id[4]`).
    pub id: u32,
    /// Geometry: pixel (0,0) of the buffer is `rect.min`.
    pub rect: Rect,
    /// Clip rectangle; drawing is confined to `rect ∩ clipr`.
    pub clipr: Rect,
    /// Pixel channel descriptor (SPEC.md §7).
    pub chan: Chan,
    /// `repl` from 'b': when true, the image repeats (tiles) infinitely
    /// with period `Dx(rect) × Dy(rect)` (see `draw_tile`).
    pub repl: bool,
    /// Raw pixel rows, contiguous, no row padding:
    /// `len == Dx·Dy·depth/8`, bytes little-endian per pixel word
    /// (first chan-string channel = most significant byte; crate doc note
    /// on memdraw OPEN-3 applies).
    pub pixels: Vec<u8>,
}

/// Image construction / raster precondition failures.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RenderError {
    /// Image rect has non-positive extent.
    EmptyRect,
    /// Channel depth is not a whole number of bytes (sub-byte packing such
    /// as GREY1/2/4 is out of scope for v0).
    UnsupportedDepth(u32),
    /// Pixel buffer size disagrees with rect × chan.
    PixelBufferSize { expected: usize, got: usize },
    /// A 'Y' compressed stream ended before the rect was fully decoded.
    TruncatedCompressed { got: usize },
    /// A sub-byte write needs a byte-aligned x offset within the row.
    UnalignedSubByteWrite,
}

impl fmt::Display for RenderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RenderError::EmptyRect => write!(f, "image rect is empty"),
            RenderError::UnsupportedDepth(d) => {
                write!(f, "channel depth {} is not byte-aligned", d)
            }
            RenderError::PixelBufferSize { expected, got } => write!(
                f,
                "pixel buffer size mismatch: expected {} bytes, got {}",
                expected, got
            ),
            RenderError::TruncatedCompressed { got } => {
                write!(f, "compressed pixel data ends early ({} bytes)", got)
            }
            RenderError::UnalignedSubByteWrite => {
                write!(f, "sub-byte write needs a byte-aligned x offset")
            }
        }
    }
}

impl std::error::Error for RenderError {}

// --- private geometry over the u32 wire types (render math is signed) ---

/// Wire points are bit-exact two's complement of the signed C values
/// (negative coords wrap on the wire; see `p9draw_protocol::Point`).
fn sx(v: u32) -> i32 {
    v as i32
}

fn dx(r: Rect) -> i32 {
    sx(r.max.x).wrapping_sub(sx(r.min.x))
}

fn dy(r: Rect) -> i32 {
    sx(r.max.y).wrapping_sub(sx(r.min.y))
}

fn is_empty(r: Rect) -> bool {
    dx(r) <= 0 || dy(r) <= 0
}

fn isect(a: Rect, b: Rect) -> Rect {
    let x0 = sx(a.min.x).max(sx(b.min.x));
    let y0 = sx(a.min.y).max(sx(b.min.y));
    let x1 = sx(a.max.x).min(sx(b.max.x));
    let y1 = sx(a.max.y).min(sx(b.max.y));
    Rect {
        min: Point {
            x: x0 as u32,
            y: y0 as u32,
        },
        max: Point {
            x: x1 as u32,
            y: y1 as u32,
        },
    }
}

fn contains(r: Rect, x: i32, y: i32) -> bool {
    x >= sx(r.min.x) && x < sx(r.max.x) && y >= sx(r.min.y) && y < sx(r.max.y)
}

impl Image {
    /// Bytes per pixel (validated byte-aligned at construction).
    fn bpp(&self) -> usize {
        (self.chan.depth() / 8) as usize
    }

    /// Row stride in bytes; rows are contiguous, no padding (the 'r'
    /// read-pixels reply uses the same `bytesperline·Dy` layout).
    fn bpl(&self) -> usize {
        dx(self.rect) as usize * self.bpp()
    }

    /// `bytesperline(rect, depth)` for an arbitrary width: rows are
    /// packed, so sub-byte channels share whole bytes across pixels
    /// (GREY1 1627 px wide → 204 bytes per row). Identical to [bpl](Self::bpl)
    /// for byte-aligned depths.
    fn row_bytes(&self, w: i32) -> usize {
        (w as usize * self.chan.depth() as usize + 7) / 8
    }

    /// Allocate a zeroed image with `clipr = rect` and `repl = false`.
    pub fn new(id: u32, rect: Rect, chan: Chan) -> Result<Image, RenderError> {
        let depth = chan.depth();
        if is_empty(rect) {
            return Err(RenderError::EmptyRect);
        }
        if depth == 0 || depth % 8 != 0 {
            return Err(RenderError::UnsupportedDepth(depth));
        }
        let len = dx(rect) as usize * dy(rect) as usize * (depth / 8) as usize;
        Ok(Image {
            id,
            rect,
            clipr: rect,
            chan,
            repl: false,
            pixels: vec![0u8; len],
        })
    }

    /// Assemble an image from an existing pixel buffer (e.g. data decoded
    /// from a 'y' write-pixels command). `clipr` is intersected with
    /// `rect`; the buffer length must equal `Dx·Dy·depth/8`.
    pub fn with_pixels(
        id: u32,
        rect: Rect,
        clipr: Rect,
        chan: Chan,
        repl: bool,
        pixels: Vec<u8>,
    ) -> Result<Image, RenderError> {
        let depth = chan.depth();
        if is_empty(rect) {
            return Err(RenderError::EmptyRect);
        }
        if depth == 0 || depth % 8 != 0 {
            return Err(RenderError::UnsupportedDepth(depth));
        }
        let expected = dx(rect) as usize * dy(rect) as usize * (depth / 8) as usize;
        if pixels.len() != expected {
            return Err(RenderError::PixelBufferSize {
                expected,
                got: pixels.len(),
            });
        }
        Ok(Image {
            id,
            rect,
            clipr: isect(clipr, rect),
            chan,
            repl,
            pixels,
        })
    }

    /// Assemble an image from an existing buffer of PACKED rows —
    /// `len == Dy·bytesperline(rect, depth)` — the layout 'y'/'Y' pixel
    /// writes and 'r' reads use for every channel width, including the
    /// sub-byte GREY1/2/4 acme loads its font glyphs into. Zero-depth
    /// descriptors stay out (see [Image::new]).
    pub fn with_packed(
        id: u32,
        rect: Rect,
        clipr: Rect,
        chan: Chan,
        repl: bool,
        pixels: Vec<u8>,
    ) -> Result<Image, RenderError> {
        let depth = chan.depth();
        if is_empty(rect) {
            return Err(RenderError::EmptyRect);
        }
        if depth == 0 {
            return Err(RenderError::UnsupportedDepth(0));
        }
        let expected = dy(rect) as usize * ((dx(rect) as usize * depth as usize + 7) / 8);
        if pixels.len() != expected {
            return Err(RenderError::PixelBufferSize {
                expected,
                got: pixels.len(),
            });
        }
        Ok(Image {
            id,
            rect,
            clipr: isect(clipr, rect),
            chan,
            repl,
            pixels,
        })
    }
}

/// Fill `rect` on `img` with the raw pixel `value` (packed per `img.chan`),
/// clipped to `img.rect ∩ img.clipr`. No-op when the intersection is empty.
pub fn fill(img: &mut Image, rect: Rect, value: u32) {
    let r = isect(isect(img.rect, img.clipr), rect);
    if is_empty(r) {
        return;
    }
    let bpp = img.bpp();
    let bpl = img.bpl();
    let x0 = sx(r.min.x) - sx(img.rect.min.x);
    let y0 = sx(r.min.y) - sx(img.rect.min.y);
    for row in 0..dy(r) {
        let row_start = (y0 + row) as usize * bpl + x0 as usize * bpp;
        for col in 0..dx(r) {
            let off = row_start + col as usize * bpp;
            for i in 0..bpp {
                img.pixels[off + i] = (value >> (8 * i)) as u8;
            }
        }
    }
}

/// Normalized grey (0..255) at absolute image pixel (x, y) for
/// single-channel images of any depth — packed rows for the sub-byte
/// GREY1/2/4 (leftmost pixel = most significant bits, like memimage).
/// None outside the rect or for a zero-depth stub.
pub fn grey_at(img: &Image, x: i32, y: i32) -> Option<u8> {
    if img.pixels.is_empty() || !contains(img.rect, x, y) {
        return None;
    }
    let depth = img.chan.depth() as usize;
    let lx = (x - sx(img.rect.min.x)) as usize;
    let ly = (y - sx(img.rect.min.y)) as usize;
    let stride = (dx(img.rect) as usize * depth + 7) / 8;
    let bit = (ly * stride + lx * depth / 8) * 8 + lx * depth % 8;
    let maxv = ((1u32 << depth) - 1) as u32;
    let raw = (u32::from(img.pixels[bit / 8]) >> (8 - depth - bit % 8)) & maxv;
    Some((raw * 255 / maxv) as u8)
}

/// Write a normalized grey (0..255) at absolute image pixel (x, y),
/// scaling to the channel depth with rounding. No-op outside the rect
/// or for a zero-depth stub.
pub fn set_grey(img: &mut Image, x: i32, y: i32, v: u8) {
    if img.pixels.is_empty() || !contains(img.rect, x, y) {
        return;
    }
    let depth = img.chan.depth() as usize;
    let maxv = ((1u32 << depth) - 1) as u16;
    let raw = ((u16::from(v) * maxv + 127) / 255) as u8;
    let lx = (x - sx(img.rect.min.x)) as usize;
    let ly = (y - sx(img.rect.min.y)) as usize;
    let stride = (dx(img.rect) as usize * depth + 7) / 8;
    let bit = (ly * stride + lx * depth / 8) * 8 + lx * depth % 8;
    let shift = 8 - depth - bit % 8;
    let byte = &mut img.pixels[bit / 8];
    *byte = (*byte & !((maxv as u8) << shift)) | (raw << shift);
}

/// Read one pixel of `img` at absolute (x, y) as a canonical plan9 RGBA
/// word (`r<<24|g<<16|b<<8|a`) — memdraw `_imgtorgba`
/// (libmemdraw/draw.c:2014): alpha defaults to 0xFF and is only narrowed
/// by an explicit CAlpha channel; CGREY sets r=g=b (not alpha), which is
/// why acme's GREY1 black color tile paints opaque ink. `None` where the
/// pixel has no rendering: a zero-depth stub image, a point outside the
/// rect, or a colormap channel (CMap8 — v0 has no colormaps). Sub-byte
/// channels share the packed MSB-first row layout [grey_at](fn.grey_at)
/// reads; multi-channel descs are expected uniform-depth (everything the
/// traffic carries: GREY1/8, RGB24, the 32-bit families).
pub fn rgba_at(img: &Image, x: i32, y: i32) -> Option<u32> {
    if img.pixels.is_empty() || !contains(img.rect, x, y) {
        return None;
    }
    let depth = img.chan.depth() as usize;
    let lx = (x - sx(img.rect.min.x)) as usize;
    let ly = (y - sx(img.rect.min.y)) as usize;
    let word = if depth >= 8 {
        let bytes = depth / 8;
        let off = ly * img.bpl() + lx * bytes;
        let mut w = 0u32;
        for i in 0..bytes {
            w |= u32::from(img.pixels[off + i]) << (8 * i);
        }
        w
    } else {
        let stride = (dx(img.rect) as usize * depth + 7) / 8;
        let bit = (ly * stride + lx * depth / 8) * 8 + lx * depth % 8;
        let maxv = ((1u32 << depth) - 1) as u32;
        (u32::from(img.pixels[bit / 8]) >> (8 - depth - bit % 8)) & maxv
    };
    let mut r = 0u32;
    let mut g = 0u32;
    let mut b = 0u32;
    let mut a = 0xFFu32;
    let mut shift = 0u32;
    let mut cc = img.chan.0;
    while cc != 0 {
        let nb = cc & 0x0F;
        let code = (cc >> 4) & 0x0F;
        cc >>= 8;
        if nb == 0 {
            continue;
        }
        let maxv = (1u32 << nb) - 1;
        let v8 = ((word >> shift) & maxv) * 255 / maxv;
        shift += nb;
        match code {
            0 => r = v8,                      // CRed
            1 => g = v8,                      // CGreen
            2 => b = v8,                      // CBlue
            3 => { r = v8; g = v8; b = v8; }  // CGrey
            4 => a = v8,                      // CAlpha
            5 => return None,                 // CMap: no colormaps in v0
            _ => {}                           // CIgnore (x): dropped
        }
    }
    Some((r << 24) | (g << 16) | (b << 8) | a)
}

/// 'd' mask channel (SPEC.md §6 maskid/maskpt): per-pixel alpha from a
/// single-channel grey image — acme's allocimagemix qmask is GREY8 and
/// its font cache images are GREY1..GREY8. Absent masks and any other
/// channel shape draw opaque (v0: memdraw would read the alpha channel
/// of richer mask chans; the traffic only ever uses grey masks).
enum Mask<'a> {
    /// No usable mask: every src pixel lands opaque.
    Opaque,
    /// Grey alpha; the point is the wire `maskpt` (dst_rect.min ↔ pt).
    Grey { img: &'a Image, pt: (i32, i32) },
}

impl Mask<'_> {
    /// Alpha at the mask pixel that maps onto absolute dst pixel (px, py)
    /// given the dst anchor. Some(255) = opaque, Some(0..254) = blend,
    /// None = transparent (non-repl mask outside its window — memdraw
    /// effectively clips the draw rect to the mask window).
    fn alpha_at(&self, px: i32, py: i32, anchor: (i32, i32)) -> Option<u8> {
        let (img, (mpt_x, mpt_y)) = match self {
            Mask::Opaque => return Some(255),
            Mask::Grey { img, pt } => (img, (pt.0 + (px - anchor.0), pt.1 + (py - anchor.1))),
        };
        let (mut qx, mut qy) = (mpt_x, mpt_y);
        let (mminx, mminy) = (sx(img.rect.min.x), sx(img.rect.min.y));
        let (period_x, period_y) = (dx(img.rect), dy(img.rect));
        if img.repl {
            qx = mminx + (qx - mminx).rem_euclid(period_x);
            qy = mminy + (qy - mminy).rem_euclid(period_y);
        } else if !contains(img.rect, qx, qy) {
            return None;
        }
        grey_at(img, qx, qy)
    }
}

/// v0 composite (SPEC.md 'd'): opaque copy of `src_rect` to `dst_rect`
/// (top-left corners correspond), clipped to both images' rect ∩ clipr.
/// A `repl` source is copied as-is here (no tiling). Alpha and `maskid`
/// are ignored — TODO(p9draw): mask/alpha composition per memdraw.
pub fn compose_over(dst: &mut Image, dst_rect: Rect, src: &Image, src_rect: Rect) {
    compose_over_masked(dst, dst_rect, src, src_rect, None);
}

/// [compose_over] with the 'd' mask channel: `mask.1` (the wire maskpt)
/// maps onto `dst_rect.min`, grey bytes blend
/// `out = (src·m + dst·(255−m) + 127) / 255` per byte (m = 255 reproduces
/// src byte-exactly — the "fill and tile agree" invariant).
pub fn compose_over_masked(
    dst: &mut Image,
    dst_rect: Rect,
    src: &Image,
    src_rect: Rect,
    mask: Option<(&Image, Point)>,
) {
    // Map src_rect.min onto dst_rect.min; take only the src_rect window.
    // `blit` treats `src_pt` as the source pixel that lands on
    // `dst_rect.min` (SPEC.md §6 'd': P is aligned with R.min) — that
    // pixel is `src_rect.min` itself, not the dst−src offset.
    let src_pt = (sx(src_rect.min.x), sx(src_rect.min.y));
    blit(
        dst,
        dst_rect,
        src,
        src_pt,
        isect(src.rect, src_rect),
        false,
        mask,
    );
}

/// Draw with repeat-tile handling, SPEC.md 'd': `dst_rect` is `R[16]` and
/// `src_pt` is `P[8]` — the source pixel that maps to `dst_rect.min`. A
/// `repl` source repeats with period `Dx(src.rect) × Dy(src.rect)` over the
/// whole plane (wrapped with Euclidean modulo, so negative anchors work);
/// a non-`repl` source degrades to an aligned, clipped copy.
pub fn draw_tile(dst: &mut Image, dst_rect: Rect, src: &Image, src_pt: Point) {
    draw_tile_masked(dst, dst_rect, src, src_pt, None);
}

/// [draw_tile] with the 'd' mask channel (see [compose_over_masked]).
pub fn draw_tile_masked(
    dst: &mut Image,
    dst_rect: Rect,
    src: &Image,
    src_pt: Point,
    mask: Option<(&Image, Point)>,
) {
    blit(
        dst,
        dst_rect,
        src,
        (sx(src_pt.x), sx(src_pt.y)),
        src.rect,
        src.repl,
        mask,
    );
}

/// 'l' loadchar copy (memdraw dst=R ← src at P, no mask): an opaque
/// byte blit for byte-aligned equal-depth images (the [blit] path), a
/// per-pixel normalized rescale for grey images of different depths
/// (acme's GREY1 glyph bits into a deeper cache image), a no-op for
/// other channel mismatches. Clips to dst rect ∩ clipr; a repl source
/// tiles like 'd'.
pub fn copy_rect(dst: &mut Image, dst_rect: Rect, src: &Image, src_pt: Point) {
    if dst.bpp() > 0 && dst.bpp() == src.bpp() {
        blit(
            dst,
            dst_rect,
            src,
            (sx(src_pt.x), sx(src_pt.y)),
            src.rect,
            src.repl,
            None,
        );
        return;
    }
    if !dst.chan.is_grey() || !src.chan.is_grey() {
        return;
    }
    let clip = isect(isect(dst.rect, dst.clipr), dst_rect);
    if is_empty(clip) {
        return;
    }
    let sminx = sx(src.rect.min.x);
    let sminy = sx(src.rect.min.y);
    let (period_x, period_y) = (dx(src.rect), dy(src.rect));
    let anchor_x = sx(dst_rect.min.x);
    let anchor_y = sx(dst_rect.min.y);
    for row in 0..dy(clip) {
        for col in 0..dx(clip) {
            let px = sx(clip.min.x) + col;
            let py = sx(clip.min.y) + row;
            // P maps onto dst_rect.min (before clipping), like blit.
            let (mut qx, mut qy) = (
                sx(src_pt.x) + (px - anchor_x),
                sx(src_pt.y) + (py - anchor_y),
            );
            if src.repl {
                qx = sminx + (qx - sminx).rem_euclid(period_x);
                qy = sminy + (qy - sminy).rem_euclid(period_y);
            }
            if let Some(v) = grey_at(src, qx, qy) {
                set_grey(dst, px, py, v);
            }
        }
    }
}

/// Core v0 blit: walk clipped dst pixels, map each to source coordinates
/// via `src_pt` (source pixel at `dst_rect.min`), optionally wrap-tile,
/// drop out-of-window pixels, copy per pixel (or grey-mask blend). When
/// src and dst depths differ, each src pixel converts through
/// [rgba_at] → `dst.chan.rgbatoimg` (memdraw `_imgtorgba`/`_rgbatoimg`):
/// acme's GREY1 color tiles land on x8r8g8b8 windows instead of the draw
/// silently vanishing — the 's'/'x' glyph path (2026-10 live capture).
fn blit(
    dst: &mut Image,
    dst_rect: Rect,
    src: &Image,
    src_pt: (i32, i32),
    src_win: Rect,
    tile: bool,
    mask: Option<(&Image, Point)>,
) {
    let mut clip = isect(isect(dst.rect, dst.clipr), dst_rect);
    if is_empty(clip) {
        return;
    }
    let convert = dst.bpp() != src.bpp();
    // Only single-channel grey masks blend (any depth — GREY1 font cells
    // as well as GREY8); everything else draws opaque.
    let mask = match mask {
        Some((m, pt)) if m.chan.is_grey() => Mask::Grey {
            img: m,
            pt: (sx(pt.x), sx(pt.y)),
        },
        _ => Mask::Opaque,
    };
    let anchor_x = sx(dst_rect.min.x);
    let anchor_y = sx(dst_rect.min.y);
    // A non-repl mask bounds the draw: mask pixel `pt` sits on the dst
    // anchor (dst_rect.min), so the dst-space window is the mask rect
    // shifted onto the anchor (outside = transparent).
    if let Mask::Grey { img, .. } = &mask {
        if !img.repl {
            let w = Rect {
                min: dst_rect.min,
                max: Point {
                    x: (anchor_x + dx(img.rect)) as u32,
                    y: (anchor_y + dy(img.rect)) as u32,
                },
            };
            clip = isect(clip, w);
            if is_empty(clip) {
                return;
            }
        }
    }
    let bpp = dst.bpp();
    let dst_bpl = dst.bpl();
    let src_bpl = src.bpl();
    let sminx = sx(src.rect.min.x);
    let sminy = sx(src.rect.min.y);
    let period_x = dx(src.rect);
    let period_y = dy(src.rect);
    let dst_min_x = sx(dst.rect.min.x);
    let dst_min_y = sx(dst.rect.min.y);
    for row in 0..dy(clip) {
        let py = sx(clip.min.y) + row;
        let qy = src_pt.1 + (py - anchor_y);
        for col in 0..dx(clip) {
            let px = sx(clip.min.x) + col;
            let mut qx = src_pt.0 + (px - anchor_x);
            let mut qy = qy;
            if tile {
                qx = sminx + (qx - sminx).rem_euclid(period_x);
                qy = sminy + (qy - sminy).rem_euclid(period_y);
            }
            if !contains(src_win, qx, qy) {
                continue;
            }
            let m = mask.alpha_at(px, py, (anchor_x, anchor_y));
            let Some(m) = m else { continue }; // transparent mask pixel
            // Different depths: convert the src pixel into dst's chan
            // first (see fn doc); a colormap pixel has no rendering and
            // is skipped.
            let conv: [u8; 4] = if convert {
                let Some(rgba) = rgba_at(src, qx, qy) else { continue };
                dst.chan.rgbatoimg(rgba).to_le_bytes()
            } else {
                [0; 4]
            };
            let so = (qy - sminy) as usize * src_bpl + (qx - sminx) as usize * bpp;
            let doff = (py - dst_min_y) as usize * dst_bpl + (px - dst_min_x) as usize * bpp;
            if m == 255 {
                for i in 0..bpp {
                    dst.pixels[doff + i] = if convert { conv[i] } else { src.pixels[so + i] };
                }
            } else {
                for i in 0..bpp {
                    let s = u32::from(if convert { conv[i] } else { src.pixels[so + i] });
                    let d = u32::from(dst.pixels[doff + i]);
                    dst.pixels[doff + i] = ((s * u32::from(m) + d * u32::from(255 - m) + 127) / 255)
                        as u8;
                }
            }
        }
    }
}

/// Write raw pixel bytes over `rect` (SPEC.md §6 'y', memload): rows of
/// `bytesperline(rect, depth)` bytes, top-to-bottom — the submitted rows
/// belong to `rect`. The write is clipped to `img.rect ∩ img.clipr`
/// (v0 note: memload itself ignores clipr; with clipr == rect — what the
/// captures allocate — the two agree byte for byte, and keeping row
/// alignment here preserves the submitted layout under clipping).
/// Sub-byte channels write packed rows and need the clipped x offset
/// byte-aligned within the image row (acme's glyph loads start at x=0).
/// `data` may carry more than the rect needs (devdraw hands the whole
/// write tail to memload).
pub fn write_bytes(img: &mut Image, rect: Rect, data: &[u8]) -> Result<(), RenderError> {
    if img.pixels.is_empty() {
        return Ok(()); // zero-depth stub image: nothing to fill
    }
    let r = isect(isect(img.rect, img.clipr), rect);
    if is_empty(r) {
        return Ok(());
    }
    let depth = img.chan.depth();
    let x0 = sx(r.min.x) - sx(img.rect.min.x);
    if (x0 as usize * depth as usize) % 8 != 0 {
        return Err(RenderError::UnalignedSubByteWrite);
    }
    let xoff = x0 as usize * depth as usize / 8;
    let stride = img.row_bytes(dx(img.rect));
    let wrow = img.row_bytes(dx(r));
    let src_stride = img.row_bytes(dx(rect));
    let src_x0 = (sx(r.min.x) - sx(rect.min.x)) as usize * depth as usize / 8;
    let src_y0 = (sx(r.min.y) - sx(rect.min.y)) as usize;
    let rows = dy(r) as usize;
    let need = (src_y0 + rows) * src_stride;
    if data.len() < need {
        return Err(RenderError::PixelBufferSize {
            expected: need,
            got: data.len(),
        });
    }
    let y0 = (sx(r.min.y) - sx(img.rect.min.y)) as usize;
    for row in 0..rows {
        let dst = (y0 + row) * stride + xoff;
        let src = (src_y0 + row) * src_stride + src_x0;
        img.pixels[dst..dst + wrow].copy_from_slice(&data[src..src + wrow]);
    }
    Ok(())
}

/// Write plan9-compressed pixel bytes over `rect` (SPEC.md §6 'Y',
/// memdraw `_cloadmemimage`): an LZ77 variant over a 1024-byte ring with
/// 3-byte minimum matches — control ≥ 128 emits that many literal bytes,
/// control < 128 emits `(control>>2)+3` bytes from `ring[memp −
/// ((control&3)<<8 | next) − 1]` (ring-wrapped). The decoded stream is
/// exactly `bytesperline(rect, depth)·Dy(rect)` rows written like
/// [write_bytes]; returns the compressed bytes consumed (devdraw: m += y).
pub fn write_bytes_compressed(
    img: &mut Image,
    rect: Rect,
    data: &[u8],
) -> Result<usize, RenderError> {
    if img.pixels.is_empty() {
        return Ok(0);
    }
    let r = isect(isect(img.rect, img.clipr), rect);
    if is_empty(r) {
        return Ok(0);
    }
    let depth = img.chan.depth();
    let x0 = sx(r.min.x) - sx(img.rect.min.x);
    if (x0 as usize * depth as usize) % 8 != 0 {
        return Err(RenderError::UnalignedSubByteWrite);
    }
    let xoff = x0 as usize * depth as usize / 8;
    let stride = img.row_bytes(dx(img.rect));
    let wrow = img.row_bytes(dx(r));
    let total_rows = dy(r) as usize;

    const NMEM: usize = 1024; // cload ring size
    const NMATCH: usize = 3; // minimum back-reference length
    let mut ring = [0u8; NMEM];
    let mut memp = 0usize;
    let mut u = 0usize;
    let y0 = (sx(r.min.y) - sx(img.rect.min.y)) as usize;
    let end_row = y0 + total_rows;
    let mut row = y0;
    let mut col = 0usize; // byte within the current row's written span
    // Rows fill left to right; the ring and the image advance in lockstep
    // (cload writes every decoded byte to both line and ring). A run that
    // crosses the rect boundary is a phase error like cload's linep check.
    macro_rules! put {
        ($b:expr) => {{
            if row >= end_row {
                return Err(RenderError::TruncatedCompressed { got: u });
            }
            let off = row * stride + xoff + col;
            img.pixels[off] = $b;
            col += 1;
            if col == wrow {
                row += 1;
                col = 0;
            }
        }};
    }
    loop {
        if row == end_row {
            break;
        }
        let Some(&c) = data.get(u) else {
            return Err(RenderError::TruncatedCompressed { got: u });
        };
        u += 1;
        if c >= 128 {
            for _ in 0..=(c - 128) {
                let Some(&b) = data.get(u) else {
                    return Err(RenderError::TruncatedCompressed { got: u });
                };
                u += 1;
                put!(b);
                ring[memp] = b;
                memp = (memp + 1) % NMEM;
            }
        } else {
            let Some(& offs_b) = data.get(u) else {
                return Err(RenderError::TruncatedCompressed { got: u });
            };
            u += 1;
            let offs = offs_b as usize + ((c as usize & 3) << 8) + 1;
            let mut omemp = if memp < offs { memp + NMEM - offs } else { memp - offs };
            for _ in 0..(c as usize >> 2) + NMATCH {
                let b = ring[omemp];
                put!(b);
                ring[memp] = b;
                memp = (memp + 1) % NMEM;
                omemp = (omemp + 1) % NMEM;
            }
        }
    }
    Ok(u)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(x0: i32, y0: i32, x1: i32, y1: i32) -> Rect {
        Rect {
            min: Point {
                x: x0 as u32,
                y: y0 as u32,
            },
            max: Point {
                x: x1 as u32,
                y: y1 as u32,
            },
        }
    }

    /// x8r8g8b8 pixel word: x=0, channels r g b from bit 23 down.
    fn px(r: u8, g: u8, b: u8) -> u32 {
        ((r as u32) << 16) | ((g as u32) << 8) | b as u32
    }

    fn pixel_at(img: &Image, x: i32, y: i32) -> u32 {
        let bpp = (img.chan.depth() / 8) as usize;
        let bpl = dx(img.rect) as usize * bpp;
        let o = y as usize * bpl + x as usize * bpp;
        u32::from_le_bytes(img.pixels[o..o + bpp].try_into().unwrap())
    }

    #[test]
    fn fill_writes_little_endian_pixel_bytes() {
        let mut img = Image::new(1, rect(0, 0, 2, 2), Chan::XRGB32).unwrap();
        fill(&mut img, rect(0, 0, 2, 2), px(0x10, 0x20, 0x30));
        // word = x<<24|r<<16|g<<8|b → LE bytes: b g r x
        assert_eq!(img.pixels, vec![0x30, 0x20, 0x10, 0x00].repeat(4));
    }

    #[test]
    fn fill_respects_clipr() {
        let mut img = Image::new(1, rect(0, 0, 4, 4), Chan::XRGB32).unwrap();
        img.clipr = rect(1, 1, 3, 3);
        fill(&mut img, rect(0, 0, 4, 4), px(0xFF, 0x00, 0x00));
        for y in 0..4i32 {
            for x in 0..4i32 {
                let inside = x >= 1 && x < 3 && y >= 1 && y < 3;
                let expect = if inside { px(0xFF, 0, 0) } else { 0 };
                assert_eq!(pixel_at(&img, x, y), expect, "at ({},{})", x, y);
            }
        }
    }

    #[test]
    fn fill_clips_to_image_rect() {
        let mut img = Image::new(1, rect(0, 0, 2, 2), Chan::XRGB32).unwrap();
        // rect reaching far outside; only row y=0 intersects the image.
        fill(&mut img, rect(-4, -4, 8, 1), px(7, 7, 7));
        assert_eq!(
            &img.pixels[0..8],
            &[0x07, 0x07, 0x07, 0x00, 0x07, 0x07, 0x07, 0x00]
        );
        assert!(img.pixels[8..].iter().all(|&b| b == 0));
    }

    #[test]
    fn compose_over_copies_pixels() {
        let mut src = Image::new(2, rect(0, 0, 2, 2), Chan::XRGB32).unwrap();
        fill(&mut src, rect(0, 0, 2, 2), px(1, 2, 3));
        let mut dst = Image::new(1, rect(0, 0, 4, 4), Chan::XRGB32).unwrap();
        compose_over(&mut dst, rect(1, 2, 3, 4), &src, rect(0, 0, 2, 2));
        assert_eq!(pixel_at(&dst, 1, 2), px(1, 2, 3));
        assert_eq!(pixel_at(&dst, 2, 3), px(1, 2, 3));
        assert_eq!(pixel_at(&dst, 0, 0), 0);
        assert_eq!(pixel_at(&dst, 3, 3), 0);
    }

    #[test]
    fn compose_over_respects_dst_clipr() {
        let mut src = Image::new(2, rect(0, 0, 3, 3), Chan::XRGB32).unwrap();
        fill(&mut src, rect(0, 0, 3, 3), px(9, 9, 9));
        let mut dst = Image::new(1, rect(0, 0, 4, 4), Chan::XRGB32).unwrap();
        dst.clipr = rect(1, 1, 3, 3);
        compose_over(&mut dst, rect(0, 0, 3, 3), &src, rect(0, 0, 3, 3));
        for y in 0..4i32 {
            for x in 0..4i32 {
                let inside = x >= 1 && x < 3 && y >= 1 && y < 3;
                let expect = if inside { px(9, 9, 9) } else { 0 };
                assert_eq!(pixel_at(&dst, x, y), expect, "at ({},{})", x, y);
            }
        }
    }

    #[test]
    fn fill_and_tile_of_same_color_give_identical_bytes() {
        // The 'd'-path write must be byte-faithful: tiling a filled 1×1
        // repl tile over a rect lands exactly the bytes fill() writes.
        let color = 0x00FF_FFAA; // DPaleyellow in x8r8g8b8 (b g r x rows)
        let mut tile = Image::new(2, rect(0, 0, 1, 1), Chan::XRGB32).unwrap();
        fill(&mut tile, rect(0, 0, 1, 1), color);
        tile.repl = true;
        let mut tiled = Image::new(1, rect(0, 0, 7, 5), Chan::XRGB32).unwrap();
        draw_tile(&mut tiled, rect(0, 0, 7, 5), &tile, rect(0, 0, 1, 1).min);
        let mut filled = Image::new(3, rect(0, 0, 7, 5), Chan::XRGB32).unwrap();
        fill(&mut filled, rect(0, 0, 7, 5), color);
        assert_eq!(tiled.pixels, filled.pixels);
        // Same bytes through the non-tiled compose path with a full-size
        // source (compose copies the src_rect window verbatim).
        let mut big = Image::new(4, rect(0, 0, 7, 5), Chan::XRGB32).unwrap();
        fill(&mut big, rect(0, 0, 7, 5), color);
        let mut composed = Image::new(5, rect(0, 0, 7, 5), Chan::XRGB32).unwrap();
        compose_over(&mut composed, rect(0, 0, 7, 5), &big, rect(0, 0, 7, 5));
        assert_eq!(composed.pixels, filled.pixels);
    }

    #[test]
    fn grey8_mask_blends_exactly_like_fill() {
        // acme allocimagemix (acme.c:1044): draw(white, src, qmask) with
        // qmask = GREY8 0x3F — the tag color. The blend must land on the
        // same bytes as filling with the precomputed mixed color:
        // (170·63 + 255·192 + 127) / 255 = 234 for the red byte.
        let mut src = Image::new(5, rect(0, 0, 1, 1), Chan::XRGB32).unwrap();
        fill(&mut src, rect(0, 0, 1, 1), 0x00AA_FFFF); // DPalebluegreen
        src.repl = true;
        let mut qmask = Image::new(4, rect(0, 0, 1, 1), Chan::GREY8).unwrap();
        fill(&mut qmask, rect(0, 0, 1, 1), 0x3F);
        qmask.repl = true;
        let mut dst = Image::new(6, rect(0, 0, 4, 4), Chan::XRGB32).unwrap();
        fill(&mut dst, rect(0, 0, 4, 4), 0x00FF_FFFF); // DWhite
        draw_tile_masked(
            &mut dst,
            rect(0, 0, 4, 4),
            &src,
            Point { x: 0, y: 0 },
            Some((&qmask, Point { x: 0, y: 0 })),
        );
        let mut want = Image::new(7, rect(0, 0, 4, 4), Chan::XRGB32).unwrap();
        fill(&mut want, rect(0, 0, 4, 4), 0x00EA_FFFF); // (234, 255, 255)
        assert_eq!(dst.pixels, want.pixels);
    }

    #[test]
    fn opaque_mask_copies_and_small_mask_clips() {
        let mut src = Image::new(1, rect(0, 0, 2, 2), Chan::XRGB32).unwrap();
        fill(&mut src, rect(0, 0, 2, 2), 0x0012_3456);
        let mut full = Image::new(2, rect(0, 0, 1, 1), Chan::GREY8).unwrap();
        fill(&mut full, rect(0, 0, 1, 1), 0xFF);
        full.repl = true;
        // Alpha 255 = verbatim src bytes, same as the unmasked draw.
        let mut a = Image::new(3, rect(0, 0, 2, 2), Chan::XRGB32).unwrap();
        draw_tile_masked(&mut a, rect(0, 0, 2, 2), &src, Point { x: 0, y: 0 }, Some((&full, Point { x: 0, y: 0 })));
        let mut b = Image::new(4, rect(0, 0, 2, 2), Chan::XRGB32).unwrap();
        draw_tile(&mut b, rect(0, 0, 2, 2), &src, Point { x: 0, y: 0 });
        assert_eq!(a.pixels, b.pixels);
        // Non-repl 1×1 mask at (0,0): only that pixel is drawn, the rest
        // of dst stays untouched (transparent outside the mask window).
        let mut tight = Image::new(5, rect(0, 0, 1, 1), Chan::GREY8).unwrap();
        fill(&mut tight, rect(0, 0, 1, 1), 0xFF);
        let mut dst = Image::new(6, rect(0, 0, 2, 2), Chan::XRGB32).unwrap();
        fill(&mut dst, rect(0, 0, 2, 2), 0x00AA_BBCC);
        draw_tile_masked(&mut dst, rect(0, 0, 2, 2), &src, Point { x: 0, y: 0 }, Some((&tight, Point { x: 0, y: 0 })));
        assert_eq!(u32::from_le_bytes(dst.pixels[0..4].try_into().unwrap()), 0x0012_3456);
        assert_eq!(u32::from_le_bytes(dst.pixels[4..8].try_into().unwrap()), 0x00AA_BBCC);
    }

    #[test]
    fn write_bytes_clips_and_packs_rows() {
        let mut img = Image::new(1, rect(0, 0, 4, 4), Chan::XRGB32).unwrap();
        img.clipr = rect(1, 1, 3, 3);
        // Full-image rows: only the clip window (2×2 pixels = 32 bytes)
        // may land. 16 bytes per submitted row.
        let rows: Vec<u8> = (0..4)
            .flat_map(|y| (0..4).flat_map(move |x| (x as u32 * 16 + y).to_le_bytes()))
            .collect();
        write_bytes(&mut img, rect(0, 0, 4, 4), &rows).unwrap();
        // Clipped rows stay aligned with the submitted rect: clipped
        // pixel (1,1) takes submitted word (x=1,y=1) → 17, (2,1) → 33,
        // row 2 → 18 and 34; everything outside the clip stays zero.
        assert_eq!(u32::from_le_bytes(img.pixels[0..4].try_into().unwrap()), 0);
        assert_eq!(u32::from_le_bytes(img.pixels[20..24].try_into().unwrap()), 17);
        assert_eq!(u32::from_le_bytes(img.pixels[24..28].try_into().unwrap()), 33);
        assert_eq!(u32::from_le_bytes(img.pixels[36..40].try_into().unwrap()), 18);
        assert_eq!(u32::from_le_bytes(img.pixels[40..44].try_into().unwrap()), 34);
        assert_eq!(u32::from_le_bytes(img.pixels[52..56].try_into().unwrap()), 0);
        // Sub-byte: GREY1 8×2 = 2 bytes per row, row-aligned writes.
        let mut g = Image::with_packed(9, rect(0, 0, 8, 2), rect(0, 0, 8, 2), Chan::GREY1, false, vec![0; 2]).unwrap();
        write_bytes(&mut g, rect(0, 0, 8, 2), &[0b1010_0101, 0b0101_1010]).unwrap();
        assert_eq!(g.pixels, vec![0b1010_0101, 0b0101_1010]);
        // Short data is an error; empty stubs are a silent no-op.
        assert!(write_bytes(&mut g, rect(0, 0, 8, 2), &[1]).is_err());
    }

    #[test]
    fn compressed_write_decodes_literals_and_backrefs() {
        // Row (8 px GREY8 = 8 bytes): 4 literals (0x83) then a back-ref
        // 4 bytes from distance 3: control = (4-3)<<2 = 0x04, offs-1 = 2
        // → the emitted bytes are BB CC DD CC.
        let mut img = Image::new(1, rect(0, 0, 8, 1), Chan::GREY8).unwrap();
        let stream = [0x83, 0xAA, 0xBB, 0xCC, 0xDD, 0x04, 0x02];
        let used = write_bytes_compressed(&mut img, rect(0, 0, 8, 1), &stream).unwrap();
        assert_eq!(used, stream.len());
        assert_eq!(&img.pixels[..8], &[0xAA, 0xBB, 0xCC, 0xDD, 0xBB, 0xCC, 0xDD, 0xBB]);
        // A literal run crossing the row boundary, then a back-ref closing
        // the second row: ring state carries across rows like cload's.
        let mut img2 = Image::new(2, rect(0, 0, 8, 2), Chan::GREY8).unwrap();
        let mut stream = vec![0x8A]; // 11 literals
        stream.extend(1u8..=11u8);
        stream.extend([0x08, 0x00]); // 5 bytes from distance 1 → five 11s
        let used = write_bytes_compressed(&mut img2, rect(0, 0, 8, 2), &stream).unwrap();
        assert_eq!(used, stream.len());
        assert_eq!(&img2.pixels[..8], &[1, 2, 3, 4, 5, 6, 7, 8]);
        assert_eq!(&img2.pixels[8..], &[9, 10, 11, 11, 11, 11, 11, 11]);
        // Truncated stream → error naming the consumed count.
        assert_eq!(
            write_bytes_compressed(&mut img2, rect(0, 0, 8, 1), &[0x87, 1, 2]),
            Err(RenderError::TruncatedCompressed { got: 3 })
        );
    }

    /// The acme font-glyph load of the live capture: decode the Twrdraw
    /// carrying the 'Y' command out of fixtures/live-acme-interactive and
    /// decompress it into a GREY1 1627×15 image. The compressor's stream
    /// must consume exactly and fill every row (census: 2029 bytes).
    #[test]
    fn live_capture_glyph_stream_decompresses_exactly() {
        use p9draw_protocol::{decode, Wsysmsg};
        const C2S: &[u8] = include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../fixtures/live-acme-interactive/c2s.bin"
        ));
        // Walk drawfcall frames (SPEC.md §2.2: size[4 BE] tag type payload;
        // Twrdraw payload = count[4 LE] + commands) and pull the 'Y'.
        let mut off = 0;
        let mut glyph = None;
        while off < C2S.len() {
            let size = u32::from_be_bytes([C2S[off], C2S[off + 1], C2S[off + 2], C2S[off + 3]])
                as usize;
            let (_, msg) = decode(&C2S[off..off + size]).expect("frame decodes");
            if let Wsysmsg::Twrdraw { data } = msg {
                // Inner op walk (SPEC.md §6 fixed sizes; the capture has
                // no 'p'/'P' — verified by the drawcmd_roundtrip test).
                let mut i = 0;
                while i < data.len() {
                    let step = match data[i] {
                        b'b' => 51,
                        b'A' => 14,
                        b'S' => 9,
                        b'c' => 22,
                        b'd' => 45,
                        b'D' | b'O' => 2,
                        b'e' | b'E' | b'L' => 45,
                        b'f' | b'F' => 5,
                        b'i' => 10,
                        b'J' | b'I' | b'v' => 1,
                        b'q' => 2 + data[i + 1] as usize,
                        b'l' => 37,
                        b'n' => 6 + data[i + 5] as usize,
                        b'N' => 7 + data[i + 6] as usize,
                        b'o' => 21,
                        b'r' => 21,
                        b's' => 47 + 2 * u16::from_le_bytes([data[i + 45], data[i + 46]]) as usize,
                        b'x' => 59 + 2 * u16::from_le_bytes([data[i + 45], data[i + 46]]) as usize,
                        b't' => 4 + 4 * u16::from_le_bytes([data[i + 2], data[i + 3]]) as usize,
                        b'y' | b'Y' => {
                            let id = u32::from_le_bytes(data[i + 1..i + 5].try_into().unwrap());
                            let r = Rect {
                                min: Point {
                                    x: u32::from_le_bytes(data[i + 5..i + 9].try_into().unwrap()),
                                    y: u32::from_le_bytes(data[i + 9..i + 13].try_into().unwrap()),
                                },
                                max: Point {
                                    x: u32::from_le_bytes(data[i + 13..i + 17].try_into().unwrap()),
                                    y: u32::from_le_bytes(data[i + 17..i + 21].try_into().unwrap()),
                                },
                            };
                            if data[i] == b'Y' {
                                glyph = Some((id, r, data[i + 21..].to_vec()));
                            }
                            data.len() - i
                        }
                        op => panic!("unwalked op {op:#x} at {i}"),
                    };
                    i += step;
                }
            }
            off += size;
        }
        let (id, r, data) = glyph.expect("capture carries a 'Y' glyph load");
        assert_eq!(id, 19);
        assert_eq!(
            r,
            Rect { min: Point { x: 0, y: 0 }, max: Point { x: 1627, y: 15 } }
        );
        // GREY1 1627×15 = 15 rows × ceil(1627/8) = 204 bytes packed.
        let mut img = Image::with_packed(
            id,
            r,
            r,
            Chan::GREY1,
            false,
            vec![0u8; 15 * 204],
        )
        .unwrap();
        let used = write_bytes_compressed(&mut img, r, &data).unwrap();
        assert_eq!(used, data.len(), "stream consumed to the last byte");
        // Rows were fully rewritten: no all-zero row survives.
        assert!(img.pixels.chunks(204).any(|row| row.iter().any(|&b| b != 0)));
    }

    #[test]
    fn compose_over_uses_only_src_rect() {
        let mut src = Image::new(2, rect(0, 0, 3, 3), Chan::XRGB32).unwrap();
        fill(&mut src, rect(0, 0, 1, 3), px(1, 1, 1));
        fill(&mut src, rect(1, 0, 3, 3), px(2, 2, 2));
        let mut dst = Image::new(1, rect(0, 0, 3, 3), Chan::XRGB32).unwrap();
        // src_rect is the single left column; only it may be copied.
        compose_over(&mut dst, rect(0, 0, 3, 3), &src, rect(0, 0, 1, 3));
        for y in 0..3i32 {
            assert_eq!(pixel_at(&dst, 0, y), px(1, 1, 1));
            assert_eq!(pixel_at(&dst, 1, y), 0);
            assert_eq!(pixel_at(&dst, 2, y), 0);
        }
    }

    #[test]
    fn draw_tile_repeats_repl_source() {
        let mut src = Image::new(2, rect(0, 0, 2, 1), Chan::XRGB32).unwrap();
        fill(&mut src, rect(0, 0, 1, 1), px(10, 0, 0));
        fill(&mut src, rect(1, 0, 2, 1), px(0, 20, 0));
        src.repl = true;
        let mut dst = Image::new(1, rect(0, 0, 5, 2), Chan::XRGB32).unwrap();
        draw_tile(&mut dst, rect(0, 0, 5, 2), &src, Point { x: 0, y: 0 });
        for y in 0..2i32 {
            assert_eq!(pixel_at(&dst, 0, y), px(10, 0, 0));
            assert_eq!(pixel_at(&dst, 1, y), px(0, 20, 0));
            assert_eq!(pixel_at(&dst, 2, y), px(10, 0, 0));
            assert_eq!(pixel_at(&dst, 3, y), px(0, 20, 0));
            assert_eq!(pixel_at(&dst, 4, y), px(10, 0, 0));
        }
    }

    #[test]
    fn draw_tile_wraps_negative_anchor_with_period() {
        // 2x2 source at rect.min (1,1), four distinct colors, repl.
        let mut src = Image::new(7, rect(1, 1, 3, 3), Chan::XRGB32).unwrap();
        fill(&mut src, rect(1, 1, 2, 2), px(1, 1, 1));
        fill(&mut src, rect(2, 1, 3, 2), px(2, 2, 2));
        fill(&mut src, rect(1, 2, 2, 3), px(3, 3, 3));
        fill(&mut src, rect(2, 2, 3, 3), px(4, 4, 4));
        src.repl = true;
        let mut dst = Image::new(1, rect(0, 0, 4, 4), Chan::XRGB32).unwrap();
        // Negative wire anchor (-2,-2): q = (x-2, y-2), wrapped mod 2.
        draw_tile(
            &mut dst,
            rect(0, 0, 4, 4),
            &src,
            Point {
                x: (-2i32) as u32,
                y: (-2i32) as u32,
            },
        );
        // Rows/cols cycle: y=0→src row 2, y=1→row 1; x=0→src col 2, x=1→col 1.
        assert_eq!(pixel_at(&dst, 0, 0), px(4, 4, 4));
        // (1,0): q=(-1,-2) → col 1, row 2 → px3; (1,1): q=(-1,-1) → (1,1) → px1.
        // The old px1/px3 here were transposed: they contradict this test's
        // own cycle comment and are jointly unsatisfiable by any translation-
        // invariant tiling (SPEC.md §6 'd': repl period = Dx(rect)×Dy(rect)).
        assert_eq!(pixel_at(&dst, 1, 0), px(3, 3, 3));
        assert_eq!(pixel_at(&dst, 0, 1), px(2, 2, 2));
        assert_eq!(pixel_at(&dst, 1, 1), px(1, 1, 1));
        assert_eq!(pixel_at(&dst, 3, 3), px(1, 1, 1));
    }

    #[test]
    fn draw_tile_without_repl_behaves_as_clipped_copy() {
        let mut src = Image::new(2, rect(0, 0, 2, 2), Chan::XRGB32).unwrap();
        fill(&mut src, rect(0, 0, 2, 2), px(5, 6, 7));
        let mut dst = Image::new(1, rect(0, 0, 4, 4), Chan::XRGB32).unwrap();
        draw_tile(&mut dst, rect(1, 1, 5, 5), &src, Point { x: 0, y: 0 });
        for y in 0..4i32 {
            for x in 0..4i32 {
                let inside = x >= 1 && x < 3 && y >= 1 && y < 3;
                let expect = if inside { px(5, 6, 7) } else { 0 };
                assert_eq!(pixel_at(&dst, x, y), expect, "at ({},{})", x, y);
            }
        }
    }

    #[test]
    fn new_rejects_bad_geometry_and_depth() {
        assert_eq!(
            Image::new(1, rect(0, 0, 0, 4), Chan::GREY8),
            Err(RenderError::EmptyRect)
        );
        assert_eq!(
            Image::new(1, rect(0, 0, 2, 2), Chan::GREY1),
            Err(RenderError::UnsupportedDepth(1))
        );
        assert_eq!(
            Image::new(1, rect(0, 0, 2, 2), Chan(0)),
            Err(RenderError::UnsupportedDepth(0))
        );
    }

    #[test]
    fn with_pixels_validates_buffer_size() {
        let r = rect(0, 0, 2, 2);
        assert_eq!(
            Image::with_pixels(1, r, r, Chan::XRGB32, false, vec![0; 15]),
            Err(RenderError::PixelBufferSize {
                expected: 16,
                got: 15
            })
        );
        let img = Image::with_pixels(1, r, r, Chan::XRGB32, false, vec![0; 16]).unwrap();
        assert_eq!(img.pixels.len(), 16);
        assert_eq!(img.clipr, r);
    }

    // --- packed grey access + 'l' copy_rect (font glyph loads) -------------

    /// A GREY1 image whose row bytes are given verbatim (width 8 ⇒ 1 B/row).
    fn grey1(rows: &[u8]) -> Image {
        Image::with_packed(
            1,
            rect(0, 0, 8, rows.len() as i32),
            rect(0, 0, 8, rows.len() as i32),
            Chan::GREY1,
            false,
            rows.to_vec(),
        )
        .unwrap()
    }

    #[test]
    fn chan_is_grey_matches_the_grey_family_only() {
        assert!(Chan::GREY1.is_grey());
        assert!(Chan::GREY2.is_grey());
        assert!(Chan::GREY4.is_grey());
        assert!(Chan::GREY8.is_grey());
        assert!(!Chan::XRGB32.is_grey());
        assert!(!Chan::CMAP8.is_grey());
        assert!(!Chan(0).is_grey());
    }

    #[test]
    fn grey_at_reads_packed_rows_msb_first() {
        // memimage packs the first (leftmost) pixel into the HIGH bits:
        // 0xF0 → x0..3 inked, 0x0F → x4..7 inked.
        let img = grey1(&[0xF0, 0x0F]);
        assert_eq!(grey_at(&img, 0, 0), Some(255));
        assert_eq!(grey_at(&img, 3, 0), Some(255));
        assert_eq!(grey_at(&img, 4, 0), Some(0));
        assert_eq!(grey_at(&img, 7, 0), Some(0));
        assert_eq!(grey_at(&img, 4, 1), Some(255));
        assert_eq!(grey_at(&img, 0, 1), Some(0));
        assert_eq!(grey_at(&img, 8, 0), None);
        assert_eq!(grey_at(&img, -1, 0), None);
    }

    #[test]
    fn set_grey_scales_and_packs_roundtrip() {
        let mut img = grey1(&[0x00]);
        set_grey(&mut img, 0, 0, 255);
        set_grey(&mut img, 7, 0, 255);
        set_grey(&mut img, 3, 0, 128); // rounds to 1 at depth 1; x3 = bit 4
        assert_eq!(img.pixels, vec![0b1001_0001]);
        assert_eq!(grey_at(&img, 0, 0), Some(255));
        assert_eq!(grey_at(&img, 3, 0), Some(255));
        // GREY8 passes values through unchanged.
        let mut g8 = Image::with_packed(
            1,
            rect(0, 0, 2, 1),
            rect(0, 0, 2, 1),
            Chan::GREY8,
            false,
            vec![0, 0],
        )
        .unwrap();
        set_grey(&mut g8, 1, 0, 200);
        assert_eq!(g8.pixels, vec![0, 200]);
        assert_eq!(grey_at(&g8, 1, 0), Some(200));
    }

    #[test]
    fn copy_rect_moves_packed_glyph_bits_between_grey_depths() {
        // 3×5 cell of ink at (0,4) in an 8-wide bits image …
        let mut bits = grey1(&[0; 16]);
        for y in 4..9 {
            for x in 0..3 {
                set_grey(&mut bits, x, y, 255);
            }
        }
        // … copied into a deeper GREY8 cache image (acme's depth-max cache).
        let mut cache = Image::with_packed(
            2,
            rect(0, 0, 16, 16),
            rect(0, 0, 16, 16),
            Chan::GREY8,
            false,
            vec![0; 256],
        )
        .unwrap();
        copy_rect(&mut cache, rect(8, 2, 11, 7), &bits, Point { x: 0, y: 4 });
        for y in 0..16i32 {
            for x in 0..16i32 {
                let ink = (8..11).contains(&x) && (2..7).contains(&y);
                assert_eq!(
                    grey_at(&cache, x, y),
                    Some(if ink { 255 } else { 0 }),
                    "({x},{y})"
                );
            }
        }
        // GREY1→GREY1 keeps the same packed bits bit for bit.
        let mut cache1 = grey1(&[0; 16]);
        copy_rect(&mut cache1, rect(4, 4, 7, 9), &bits, Point { x: 0, y: 4 });
        for y in 0..16i32 {
            for x in 0..8i32 {
                let ink = (4..7).contains(&x) && (4..9).contains(&y);
                assert_eq!(
                    grey_at(&cache1, x, y),
                    Some(if ink { 255 } else { 0 }),
                    "({x},{y})"
                );
            }
        }
    }

    #[test]
    fn copy_rect_clips_to_dst_and_tiles_repl() {
        // P maps onto dst_rect.min BEFORE clipping: rows 1,2 of a draw
        // anchored at y=0 take bits rows 1,2 even when row 0 is clipped.
        let bits = grey1(&[0xFF, 0x00, 0xFF]);
        let mut cache = grey1(&[0x00; 4]);
        cache.clipr = rect(0, 1, 8, 3);
        copy_rect(&mut cache, rect(-4, 0, 4, 3), &bits, Point { x: 0, y: 0 });
        // Only cols 0..4 are inside the draw rect; bits cols 4..8 are ink.
        assert_eq!(cache.pixels, vec![0x00, 0x00, 0xF0, 0x00]);
        // A repl grey source tiles with the image period.
        let mut tile = grey1(&[0xF0]);
        tile.repl = true;
        let mut dst = grey1(&[0x00, 0x00]);
        copy_rect(&mut dst, rect(0, 0, 8, 2), &tile, Point { x: 4, y: 0 });
        assert_eq!(dst.pixels, vec![0x0F, 0x0F]);
    }

    /// GREY1 test image via the packed-row constructor (Image::new
    /// rejects sub-byte depths, exactly like the server's alloc path).
    fn with_packed(id: u32, r: Rect, chan: Chan, repl: bool) -> Image {
        let len = dy(r) as usize * ((dx(r) as usize * chan.depth() as usize + 7) / 8);
        Image::with_packed(id, r, r, chan, repl, vec![0u8; len]).unwrap()
    }

    #[test]
    fn rgba_at_reads_grey_rgb_and_defaults_alpha() {
        // GREY1 packed MSB-first: value 1 → white, 0 → black; grey sets
        // rgb but leaves alpha at the default (memdraw _imgtorgba).
        let mut g1 = with_packed(1, rect(0, 0, 2, 1), Chan::GREY1, false);
        set_grey(&mut g1, 0, 0, 255); // white (normalized 0..255)
        assert_eq!(rgba_at(&g1, 0, 0), Some(0xFFFF_FFFF));
        assert_eq!(rgba_at(&g1, 1, 0), Some(0x0000_00FF));
        // GREY8 mid value expands by scaling, alpha opaque.
        let mut g8 = Image::new(2, rect(0, 0, 1, 1), Chan::GREY8).unwrap();
        set_grey(&mut g8, 0, 0, 0x80);
        assert_eq!(rgba_at(&g8, 0, 0), Some(0x8080_80FF));
        // x8r8g8b8: the ignored x byte never masquerades as alpha.
        let mut x32 = Image::new(3, rect(0, 0, 1, 1), Chan::XRGB32).unwrap();
        let xr = x32.rect;
        // paleyellow pixel word (chan.rs: rgbatoimg(0xFFFF_AAFF))
        fill(&mut x32, xr, 0x00FF_FFAA); // b g r x = AA FF FF 00
        assert_eq!(rgba_at(&x32, 0, 0), Some(0xFFFF_AAFF));
        // r8g8b8 rounds out to opaque too.
        let mut r24 = Image::new(4, rect(0, 0, 1, 1), Chan::RGB24).unwrap();
        let rr = r24.rect;
        fill(&mut r24, rr, 0xFFFF_AA); // b g r = AA FF FF (24-bit word)
        assert_eq!(rgba_at(&r24, 0, 0), Some(0xFFFF_AAFF));
        // Zero-depth stubs and off-rect points have no rendering.
        let stub = Image {
            id: 9,
            rect: rect(0, 0, 2, 2),
            clipr: rect(0, 0, 2, 2),
            chan: Chan(0),
            repl: false,
            pixels: Vec::new(),
        };
        assert_eq!(rgba_at(&stub, 0, 0), None);
        assert_eq!(rgba_at(&g1, 5, 5), None);
    }

    #[test]
    fn blit_converts_grey_color_tiles_onto_xrgb32() {
        // acme's live text path: a GREY1 1×1 repl ink tile drawn onto an
        // x8r8g8b8 window — depths 1 vs 32. The old blit dropped these
        // draws on sight (the missing-text symptom); now each pixel goes
        // through rgba_at → rgbatoimg like memdraw.
        let mut dst = Image::new(0, rect(0, 0, 4, 2), Chan::XRGB32).unwrap();
        let dr = dst.rect;
        fill(&mut dst, dr, 0x00FF_FFFF); // white, bytes FF FF FF 00
        let mut repl_ink = with_packed(1, rect(0, 0, 1, 1), Chan::GREY1, true); // pixel 0 = black
        let ink = repl_ink.clone();
        draw_tile_masked(&mut dst, rect(1, 0, 4, 1), &repl_ink, Point { x: 0, y: 0 }, None);
        assert_eq!(pixel_at(&dst, 0, 0), 0x00FF_FFFF, "outside the draw stays white");
        assert_eq!(pixel_at(&dst, 1, 0), 0x0000_0000, "grey ink converts to black xrgb32");
        assert_eq!(pixel_at(&dst, 2, 0), 0x0000_0000);
        assert_eq!(pixel_at(&dst, 3, 0), 0x0000_0000);
        // Masked run: a GREY1 mask limits the converted ink to its set bits.
        let mut dst2 = Image::new(2, rect(0, 0, 4, 1), Chan::XRGB32).unwrap();
        let d2 = dst2.rect;
        fill(&mut dst2, d2, 0x00FF_FFFF);
        let mut mask = with_packed(3, rect(0, 0, 4, 1), Chan::GREY1, false);
        set_grey(&mut mask, 0, 0, 255);
        set_grey(&mut mask, 2, 0, 255);
        draw_tile_masked(
            &mut dst2,
            rect(0, 0, 4, 1),
            &repl_ink,
            Point { x: 0, y: 0 },
            Some((&mask, Point { x: 0, y: 0 })),
        );
        assert_eq!(pixel_at(&dst2, 0, 0), 0x0000_0000, "masked-in pixel black");
        assert_eq!(pixel_at(&dst2, 1, 0), 0x00FF_FFFF, "masked-out pixel white");
        assert_eq!(pixel_at(&dst2, 2, 0), 0x0000_0000);
        assert_eq!(pixel_at(&dst2, 3, 0), 0x00FF_FFFF);
    }

    #[test]
    fn blit_blends_sub_byte_grey_masks_like_memdraw() {
        // GREY1 cell 2 rows of 0b1000_0000: pixel x0 inked, x1 off.
        let cell = grey1(&[0b1000_0000, 0b1000_0000, 0, 0]);
        let mut dst = Image::new(1, rect(0, 0, 2, 2), Chan::XRGB32).unwrap();
        fill(&mut dst, rect(0, 0, 2, 2), px(0x10, 0x20, 0x30));
        let src = Image::new(2, rect(0, 0, 1, 1), Chan::XRGB32).unwrap();
        draw_tile_masked(
            &mut dst,
            rect(0, 0, 2, 2),
            &src,
            Point { x: 0, y: 0 },
            Some((&cell, Point { x: 0, y: 0 })),
        );
        assert_eq!(pixel_at(&dst, 0, 0), 0); // ink: opaque src (black)
        assert_eq!(pixel_at(&dst, 1, 0), px(0x10, 0x20, 0x30)); // mask 0: dst kept
        // The mask window is the cell mapped onto the dst anchor: a draw
        // whose rect reaches past the cell leaves the rest untouched.
        let mut wide = Image::new(3, rect(0, 0, 4, 1), Chan::XRGB32).unwrap();
        fill(&mut wide, rect(0, 0, 4, 1), px(9, 9, 9));
        draw_tile_masked(
            &mut wide,
            rect(0, 0, 4, 1),
            &src,
            Point { x: 0, y: 0 },
            Some((&cell, Point { x: 0, y: 0 })),
        );
        assert_eq!(pixel_at(&wide, 0, 0), 0);
        assert_eq!(pixel_at(&wide, 1, 0), px(9, 9, 9));
        assert_eq!(pixel_at(&wide, 2, 0), px(9, 9, 9));
    }
}
