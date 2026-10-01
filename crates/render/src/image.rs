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

/// v0 composite (SPEC.md 'd'): opaque copy of `src_rect` to `dst_rect`
/// (top-left corners correspond), clipped to both images' rect ∩ clipr.
/// A `repl` source is copied as-is here (no tiling). Alpha and `maskid`
/// are ignored — TODO(p9draw): mask/alpha composition per memdraw.
pub fn compose_over(dst: &mut Image, dst_rect: Rect, src: &Image, src_rect: Rect) {
    // Map src_rect.min onto dst_rect.min; take only the src_rect window.
    // `blit` treats `src_pt` as the source pixel that lands on
    // `dst_rect.min` (SPEC.md §6 'd': P is aligned with R.min) — that
    // pixel is `src_rect.min` itself, not the dst−src offset.
    let src_pt = (sx(src_rect.min.x), sx(src_rect.min.y));
    blit(dst, dst_rect, src, src_pt, isect(src.rect, src_rect), false);
}

/// Draw with repeat-tile handling, SPEC.md 'd': `dst_rect` is `R[16]` and
/// `src_pt` is `P[8]` — the source pixel that maps to `dst_rect.min`. A
/// `repl` source repeats with period `Dx(src.rect) × Dy(src.rect)` over the
/// whole plane (wrapped with Euclidean modulo, so negative anchors work);
/// a non-`repl` source degrades to an aligned, clipped copy.
pub fn draw_tile(dst: &mut Image, dst_rect: Rect, src: &Image, src_pt: Point) {
    blit(
        dst,
        dst_rect,
        src,
        (sx(src_pt.x), sx(src_pt.y)),
        src.rect,
        src.repl,
    );
}

/// Core v0 blit: walk clipped dst pixels, map each to source coordinates
/// via `src_pt` (source pixel at `dst_rect.min`), optionally wrap-tile,
/// drop out-of-window pixels, copy per pixel. No-op when channel depths
/// differ (TODO(p9draw): channel conversion).
fn blit(
    dst: &mut Image,
    dst_rect: Rect,
    src: &Image,
    src_pt: (i32, i32),
    src_win: Rect,
    tile: bool,
) {
    let clip = isect(isect(dst.rect, dst.clipr), dst_rect);
    if is_empty(clip) || dst.bpp() != src.bpp() {
        return;
    }
    let bpp = dst.bpp();
    let dst_bpl = dst.bpl();
    let src_bpl = src.bpl();
    let sminx = sx(src.rect.min.x);
    let sminy = sx(src.rect.min.y);
    let period_x = dx(src.rect);
    let period_y = dy(src.rect);
    let anchor_x = sx(dst_rect.min.x);
    let anchor_y = sx(dst_rect.min.y);
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
            let so = (qy - sminy) as usize * src_bpl + (qx - sminx) as usize * bpp;
            let doff = (py - dst_min_y) as usize * dst_bpl + (px - dst_min_x) as usize * bpp;
            for i in 0..bpp {
                dst.pixels[doff + i] = src.pixels[so + i];
            }
        }
    }
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
}
