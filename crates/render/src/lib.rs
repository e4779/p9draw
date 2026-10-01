//! p9draw-render — software raster for plan9port images (SPEC.md §6-7).
//!
//! v0 scope:
//! - byte-aligned channels at full speed; sub-byte GREY1/2/4 via packed
//!   rows ([Image::with_packed], `rgba_at`/`set_grey` bit access);
//! - `fill` honors the image clip rectangle;
//! - `compose_over` is an opaque copy — alpha is ignored (TODO);
//! - `draw_tile`/`compose_over_masked` implement the 'd'-command
//!   repeat-tile and GREY8-mask semantics for `repl` sources (SPEC.md §6:
//!   `R` dst rect + `P` src point + `maskpt`);
//!
//! `write_bytes`/`write_bytes_compressed` apply the 'y'/'Y' pixel
//! writes (memload / `_cloadmemimage`), packed rows included.
//!
//! Pixel bytes are stored little-endian per pixel word, where the first
//! channel of the chan string is the most significant byte (so `x8r8g8b8`
//! rows look like `b g r x`). This matches plan9port memimage on
//! little-endian hosts; memdraw's in-memory order is SPEC.md OPEN-3 —
//! TODO(p9draw): verify against memdraw/alloc.c before 1.0.

pub mod chan;
pub mod image;

pub use chan::{Chan, ChanError};
pub use image::{
    compose_over, compose_over_masked, copy_rect, draw_tile, draw_tile_masked, fill, grey_at,
    rgba_at, set_grey, write_bytes, write_bytes_compressed, Image, RenderError,
};

/// Wire geometry shared with the codec: raster and protocol code use the
/// same `Point`/`Rect` types (u32 halves, bit-exact two's complement of the
/// signed C values — see `p9draw_protocol::Point`).
pub use p9draw_protocol::{Point, Rect};
