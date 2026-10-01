//! Inner draw-command stream (SPEC.md §6): the packed, little-endian
//! command sequence carried inside Twrdraw/Trddraw data segments.
//!
//! Every command starts with one op letter and its fields follow back to
//! back; all multi-byte numbers are little-endian, unlike the big-endian
//! frame envelope. Commands are neither aligned nor length-prefixed: most
//! ops have a fixed size, the exceptions being
//!
//! * 'q'/'n'/'N'/'t' — an explicit element count up front,
//! * 's'/'x' — an explicit rune-index count,
//! * 'p'/'P' — variable-length drawcoord vertices,
//! * 'y' — exactly bytesperline(R, depth)·Dy(R) pixel bytes: devdraw.c:1425
//!   advances by what memload consumed (libmemdraw/load.c:14-17), so the
//!   next command starts right after. The depth comes from the 'b'
//!   allocimage ops of the same stream, with a tail-of-payload fallback for
//!   images allocated in earlier Twrdraws (v0, SPEC.md §6).
//! * 'Y' — pixel data runs to the end of the payload (v0: the compressed
//!   wire length is only knowable by decompressing; acme sends 'Y' last).
//!
//! The decoder validates layout only. Cross-field policies devdraw enforces
//! (window repl/chan consistency in 'b', rectinrect for 'r', non-empty
//! names for 'n'/'N', the 4096-char font limit for 'i') are server-layer
//! semantics, not wire layout, and are deliberately not checked here.
//!
//! Polygon quirk (verified against plan9port devdraw.c, drawcmd): the
//! vertex list of 'p'/'P' starts at offset 31 — right after sp — and holds
//! n+1 drawcoord vertices accumulated from (0, 0). The p0 the C code reads
//! at a+31 is overwritten by the first vertex before use, so no absolute
//! p0 exists on the wire (SPEC.md §6 documents that dead read).

use std::collections::HashMap;

use crate::{Point, ProtocolError, Rect};

/// One command of the inner draw stream (SPEC.md §6). Field order inside
/// each variant is wire order; ids are the client-chosen image numbers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DrawCmd {
    /// 'b' — allocimage (51 bytes). screen_id != 0 makes a window; the C
    /// server then requires repl == 0 and chan == the screen channel.
    Allocate {
        id: u32,
        /// Wire bytes 5..7 as a LE u16. devdraw reads BGSHORT(a+5), so the
        /// bytes at 7..9 are ignored (observed garbage-tolerant on the wire).
        screen_id: u16,
        /// 0 = Refbackup, 1 = Refnone, 2 = Refmesg.
        refresh: u8,
        /// Channel descriptor, LE u32: one byte per channel as
        /// (code<<4)|bits, first string channel in the HIGH byte
        /// (GREY1 = 0x31, x8r8g8b8 = 0x68081828 — SPEC.md §7).
        chan: u32,
        repl: u8,
        r: Rect,
        clip_r: Rect,
        /// Fill color as a raw LE u32 pixel (channel packing: SPEC.md §8,
        /// OPEN-3 — memory order still under research).
        value: u32,
    },
    /// 'A' — allocscreen (14 bytes).
    AllocScreen {
        id: u32,
        image_id: u32,
        fill_id: u32,
        public: u8,
    },
    /// 'S' — use public screen (9 bytes); chan must match the screen.
    PublicScreen { id: u32, chan: u32 },
    /// 'c' — set repl and clipr of an existing image (22 bytes).
    ReplClip { dst_id: u32, repl: u8, clip_r: Rect },
    /// 'd' — composite draw (45 bytes).
    Draw {
        dst_id: u32,
        src_id: u32,
        mask_id: u32,
        r: Rect,
        src_pt: Point,
        mask_pt: Point,
    },
    /// 'D' — toggle debug (2 bytes).
    Debug { val: u8 },
    /// 'e' — ellipse outline / 'E' — filled ellipse (45 bytes each).
    /// ox bit31 marks an arc (angles in ox/oy; bit30 keeps the flag after
    /// devdraw normalizes) — bits are preserved raw for the render layer.
    Ellipse {
        filled: bool,
        dst_id: u32,
        src_id: u32,
        center: Point,
        /// Horizontal semi-axis.
        a: u32,
        /// Vertical semi-axis.
        b: u32,
        thick: u32,
        sp: Point,
        ox: u32,
        oy: u32,
    },
    /// 'f' — free image (5 bytes).
    Free { id: u32 },
    /// 'F' — free screen (5 bytes).
    FreeScreen { id: u32 },
    /// 'i' — initialize a font (10 bytes); nchars ≤ 4096 server-side.
    InitFont {
        font_id: u32,
        nchars: u32,
        ascent: u8,
    },
    /// 'J' — image 0 := screen image (1 byte).
    Image0Screen,
    /// 'I' — read image info (1 byte); the 12×"%11d " (144-byte ASCII)
    /// reply is buffered server-side for the next Trddraw.
    ReadInfo,
    /// 'q' — query (2+n bytes); only 'd' (dpi) is supported by devdraw,
    /// each spec yields one "%11d " chunk into readdata.
    Query { specs: Vec<u8> },
    /// 'l' — load a character into a font (37 bytes).
    LoadFont {
        font_id: u32,
        src_id: u32,
        index: u16,
        r: Rect,
        sp: Point,
        left: u8,
        width: u8,
    },
    /// 'L' — line (45 bytes).
    Line {
        dst_id: u32,
        p0: Point,
        p1: Point,
        end0: u32,
        end1: u32,
        radius: u32,
        src_id: u32,
        sp: Point,
    },
    /// 'n' — attach a named image (6+j bytes). devdraw rejects empty names;
    /// this codec accepts them (layout only).
    AttachNamed { dst_id: u32, name: String },
    /// 'N' — register (set=true) or drop (set=false) an image name (7+j).
    NameImage {
        dst_id: u32,
        set: bool,
        name: String,
    },
    /// 'o' — position a window (21 bytes): new r.min and screenr.min.
    Position {
        id: u32,
        r_min: Point,
        screen_r_min: Point,
    },
    /// 'O' — compositing op for the next draw operation (2 bytes).
    SetOp { op: u8 },
    /// 'p' — polygon outline (31 bytes + n+1 drawcoord vertices).
    Polygon {
        dst_id: u32,
        /// Count of drawcoord vertices minus one (devdraw reads n+1).
        n: u16,
        end0: u32,
        end1: u32,
        radius: u32,
        src_id: u32,
        sp: Point,
        pts: Vec<Point>,
    },
    /// 'P' — filled polygon (31 bytes + n+1 drawcoord vertices). The wind /
    /// ignore slots replace end0/end1/radius of 'p'; ignore is 8 bytes the
    /// server never looks at, kept raw.
    FillPolygon {
        dst_id: u32,
        n: u16,
        wind: u32,
        ignore: [u32; 2],
        src_id: u32,
        sp: Point,
        pts: Vec<Point>,
    },
    /// 'r' — read pixels (21 bytes); reply bytes land in readdata.
    ReadPixels { id: u32, r: Rect },
    /// 's' — draw a string (47 + 2·ni bytes); indices are font rune
    /// numbers, LE u16 each.
    String {
        dst_id: u32,
        src_id: u32,
        font_id: u32,
        p: Point,
        clip_r: Rect,
        sp: Point,
        indices: Vec<u16>,
    },
    /// 'x' — draw a string with background (59 + 2·ni bytes).
    StringBg {
        dst_id: u32,
        src_id: u32,
        font_id: u32,
        p: Point,
        clip_r: Rect,
        sp: Point,
        bg_id: u32,
        bg_pt: Point,
        indices: Vec<u16>,
    },
    /// 't' — push windows to top/bottom (4 + 4·nw bytes).
    Top { top: u8, ids: Vec<u32> },
    /// 'v' — flush pending output (1 byte).
    Flush,
    /// 'y' — write pixels (21 bytes + data); data is the rest of the
    /// payload, memload-decoded by the server.
    WritePixels { id: u32, r: Rect, data: Vec<u8> },
    /// 'Y' — write compressed pixels (21 bytes + data), same tail rule.
    WriteCompressed { id: u32, r: Rect, data: Vec<u8> },
    /// Op byte outside SPEC.md §6. Never produced by [parse_drawcmds] —
    /// a packed stream cannot skip a command of unknown length — but kept
    /// so the server layer can synthesize the devdraw Rerror. Construct
    /// directly if needed.
    Unknown { op: u8 },
}

/// Parse the packed inner draw-command stream of a Twrdraw (or Trddraw)
/// payload into commands (SPEC.md §6, little-endian).
///
/// The whole payload must be a whole number of commands; any unknown op
/// byte or truncated command aborts with [ProtocolError] — exactly the
/// devdraw behavior of failing the entire write (SPEC.md §6: error in any
/// command → Rerror for the whole Twrdraw, preceding commands stay applied).
pub fn parse_drawcmds(payload: &[u8]) -> Result<Vec<DrawCmd>, ProtocolError> {
    let mut cmds = Vec::new();
    let mut off = 0;
    // 'y' needs the target image's depth to size its pixel rows exactly
    // (devdraw keeps the whole image table server-side); the parser is
    // stateless, so it tracks the 'b' declarations of the same stream.
    let mut chans: HashMap<u32, u32> = HashMap::new();
    while off < payload.len() {
        let (cmd, next) = parse_one(payload, off, &mut chans)?;
        cmds.push(cmd);
        off = next;
    }
    Ok(cmds)
}

fn le16(buf: &[u8], at: usize) -> u16 {
    u16::from_le_bytes([buf[at], buf[at + 1]])
}

fn le32(buf: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([buf[at], buf[at + 1], buf[at + 2], buf[at + 3]])
}

fn le_point(buf: &[u8], at: usize) -> Point {
    Point {
        x: le32(buf, at),
        y: le32(buf, at + 4),
    }
}

fn le_rect(buf: &[u8], at: usize) -> Rect {
    Rect {
        min: le_point(buf, at),
        max: le_point(buf, at + 8),
    }
}

/// Channel bit depth from the wire descriptor — mirrors `render::Chan::depth`
/// (`chantodepth`): descriptor bytes high→low, zero bytes skipped, the low
/// nibble of each is the channel width in bits.
fn chan_depth(desc: u32) -> u32 {
    (0..4)
        .map(|i| (desc >> (24 - 8 * i)) as u8)
        .filter(|&b| b != 0)
        .map(|b| (b & 0x0f) as u32)
        .sum()
}

/// bytesperline(R, depth)·Dy(R) — packed rect-relative rows, the layout
/// render's write_bytes consumes as one 'y' payload (memload, load.c:14).
/// A malformed rect yields a huge usize and the `need` check rejects it.
fn y_row_bytes(r: &Rect, depth: u32) -> i32 {
    let dx = (r.max.x as i32).wrapping_sub(r.min.x as i32);
    let dy = (r.max.y as i32).wrapping_sub(r.min.y as i32);
    (dx * depth as i32 + 7) / 8 * dy
}

/// Layout guard: size bytes must remain at off (counting the op byte).
fn need(buf: &[u8], off: usize, size: usize) -> Result<(), ProtocolError> {
    let got = buf.len() - off;
    if got < size {
        Err(ProtocolError::DrawCmdTruncated {
            op: buf[off],
            offset: off,
            needed: size,
            got,
        })
    } else {
        Ok(())
    }
}

/// One coordinate in drawcoord form (devdraw.c): a 1-byte signed delta from
/// old (bit7 clear, bit6 = sign of the 7-bit delta), or a 3-byte absolute
/// (bit7 set) whose bit 22 signs a 23-bit value. Coordinates accumulate
/// bit-exactly like the C int arithmetic, so negative values wrap as u32.
/// pos advances past the consumed bytes; a shortfall is reported relative
/// to the enclosing command (cmd_off), matching need().
fn drawcoord(
    buf: &[u8],
    pos: &mut usize,
    old: u32,
    op: u8,
    cmd_off: usize,
) -> Result<u32, ProtocolError> {
    let trunc = |at: usize, extra: usize| ProtocolError::DrawCmdTruncated {
        op,
        offset: cmd_off,
        needed: at - cmd_off + extra,
        got: buf.len() - cmd_off,
    };
    let first = match buf.get(*pos) {
        Some(&v) => v,
        None => return Err(trunc(*pos, 1)),
    };
    *pos += 1;
    let mut x = (first & 0x7F) as u32;
    if first & 0x80 != 0 {
        let b1 = match buf.get(*pos) {
            Some(&v) => v,
            None => return Err(trunc(*pos, 1)),
        };
        *pos += 1;
        let b2 = match buf.get(*pos) {
            Some(&v) => v,
            None => return Err(trunc(*pos, 1)),
        };
        *pos += 1;
        x |= (b1 as u32) << 7;
        x |= (b2 as u32) << 15;
        if x & (1 << 22) != 0 {
            x |= 0xFF80_0000; // devdraw.c: x |= ~0U<<23
        }
    } else {
        if first & 0x40 != 0 {
            x |= 0xFFFF_FF80; // devdraw.c: x |= ~0U<<7
        }
        x = x.wrapping_add(old);
    }
    Ok(x)
}

/// Parse the single command starting at off; returns it and the offset just
/// past its last byte.
fn parse_one(
    buf: &[u8],
    off: usize,
    chans: &mut HashMap<u32, u32>,
) -> Result<(DrawCmd, usize), ProtocolError> {
    let op = buf[off];
    let id = |at: usize| le32(buf, off + at);
    let pt = |at: usize| le_point(buf, off + at);
    let rect = |at: usize| le_rect(buf, off + at);
    Ok(match op {
        b'b' => {
            need(buf, off, 51)?;
            let wid = id(1);
            let chan = id(10);
            // remember the depth for a later 'y' in the same stream
            chans.insert(wid, chan);
            (
                DrawCmd::Allocate {
                    id: wid,
                    screen_id: le16(buf, off + 5),
                    refresh: buf[off + 9],
                    chan,
                    repl: buf[off + 14],
                    r: rect(15),
                    clip_r: rect(31),
                    value: id(47),
                },
                off + 51,
            )
        }
        b'A' => {
            need(buf, off, 14)?;
            (
                DrawCmd::AllocScreen {
                    id: id(1),
                    image_id: id(5),
                    fill_id: id(9),
                    public: buf[off + 13],
                },
                off + 14,
            )
        }
        b'S' => {
            need(buf, off, 9)?;
            (DrawCmd::PublicScreen { id: id(1), chan: id(5) }, off + 9)
        }
        b'c' => {
            need(buf, off, 22)?;
            (
                DrawCmd::ReplClip {
                    dst_id: id(1),
                    repl: buf[off + 5],
                    clip_r: rect(6),
                },
                off + 22,
            )
        }
        b'd' => {
            need(buf, off, 45)?;
            (
                DrawCmd::Draw {
                    dst_id: id(1),
                    src_id: id(5),
                    mask_id: id(9),
                    r: rect(13),
                    src_pt: pt(29),
                    mask_pt: pt(37),
                },
                off + 45,
            )
        }
        b'D' => {
            need(buf, off, 2)?;
            (DrawCmd::Debug { val: buf[off + 1] }, off + 2)
        }
        b'e' | b'E' => {
            need(buf, off, 45)?;
            (
                DrawCmd::Ellipse {
                    filled: op == b'E',
                    dst_id: id(1),
                    src_id: id(5),
                    center: pt(9),
                    a: id(17),
                    b: id(21),
                    thick: id(25),
                    sp: pt(29),
                    ox: id(37),
                    oy: id(41),
                },
                off + 45,
            )
        }
        b'f' => {
            need(buf, off, 5)?;
            (DrawCmd::Free { id: id(1) }, off + 5)
        }
        b'F' => {
            need(buf, off, 5)?;
            (DrawCmd::FreeScreen { id: id(1) }, off + 5)
        }
        b'i' => {
            need(buf, off, 10)?;
            (
                DrawCmd::InitFont {
                    font_id: id(1),
                    nchars: id(5),
                    ascent: buf[off + 9],
                },
                off + 10,
            )
        }
        b'J' => {
            need(buf, off, 1)?;
            (DrawCmd::Image0Screen, off + 1)
        }
        b'I' => {
            need(buf, off, 1)?;
            (DrawCmd::ReadInfo, off + 1)
        }
        b'v' => {
            need(buf, off, 1)?;
            (DrawCmd::Flush, off + 1)
        }
        b'q' => {
            need(buf, off, 2)?;
            let n = buf[off + 1] as usize;
            need(buf, off, 2 + n)?;
            (
                DrawCmd::Query {
                    specs: buf[off + 2..off + 2 + n].to_vec(),
                },
                off + 2 + n,
            )
        }
        b'l' => {
            need(buf, off, 37)?;
            (
                DrawCmd::LoadFont {
                    font_id: id(1),
                    src_id: id(5),
                    index: le16(buf, off + 9),
                    r: rect(11),
                    sp: pt(27),
                    left: buf[off + 35],
                    width: buf[off + 36],
                },
                off + 37,
            )
        }
        b'L' => {
            need(buf, off, 45)?;
            (
                DrawCmd::Line {
                    dst_id: id(1),
                    p0: pt(5),
                    p1: pt(13),
                    end0: id(21),
                    end1: id(25),
                    radius: id(29),
                    src_id: id(33),
                    sp: pt(37),
                },
                off + 45,
            )
        }
        b'n' | b'N' => {
            let (fixed, j_at, name_at) = if op == b'n' { (6, 5, 6) } else { (7, 6, 7) };
            need(buf, off, fixed)?;
            let j = buf[off + j_at] as usize;
            need(buf, off, fixed + j)?;
            let name = match std::str::from_utf8(&buf[off + name_at..off + name_at + j]) {
                Ok(s) => s.to_string(),
                Err(_) => return Err(ProtocolError::DrawCmdBadUtf8 { op, offset: off }),
            };
            let cmd = if op == b'n' {
                DrawCmd::AttachNamed { dst_id: id(1), name }
            } else {
                DrawCmd::NameImage {
                    dst_id: id(1),
                    set: buf[off + 5] != 0,
                    name,
                }
            };
            (cmd, off + fixed + j)
        }
        b'o' => {
            need(buf, off, 21)?;
            (
                DrawCmd::Position {
                    id: id(1),
                    r_min: pt(5),
                    screen_r_min: pt(13),
                },
                off + 21,
            )
        }
        b'O' => {
            need(buf, off, 2)?;
            (DrawCmd::SetOp { op: buf[off + 1] }, off + 2)
        }
        b'p' | b'P' => {
            need(buf, off, 31)?;
            let n = le16(buf, off + 5);
            // drawcoord vertices start right after sp (offset 31), n+1 of
            // them, accumulated from (0, 0). No absolute p0 on the wire —
            // see the module quirk note.
            let mut pos = off + 31;
            let (mut ox, mut oy) = (0u32, 0u32);
            let mut pts = Vec::with_capacity(n as usize + 1);
            for _ in 0..=n {
                let x = drawcoord(buf, &mut pos, ox, op, off)?;
                let y = drawcoord(buf, &mut pos, oy, op, off)?;
                ox = x;
                oy = y;
                pts.push(Point { x, y });
            }
            let cmd = if op == b'p' {
                DrawCmd::Polygon {
                    dst_id: id(1),
                    n,
                    end0: id(7),
                    end1: id(11),
                    radius: id(15),
                    src_id: id(19),
                    sp: pt(23),
                    pts,
                }
            } else {
                DrawCmd::FillPolygon {
                    dst_id: id(1),
                    n,
                    wind: id(7),
                    ignore: [id(11), id(15)],
                    src_id: id(19),
                    sp: pt(23),
                    pts,
                }
            };
            (cmd, pos)
        }
        b'r' => {
            need(buf, off, 21)?;
            (DrawCmd::ReadPixels { id: id(1), r: rect(5) }, off + 21)
        }
        b's' | b'x' => {
            let fixed = if op == b's' { 47 } else { 59 };
            need(buf, off, fixed)?;
            let ni = le16(buf, off + 45) as usize;
            need(buf, off, fixed + 2 * ni)?;
            let mut indices = Vec::with_capacity(ni);
            for i in 0..ni {
                indices.push(le16(buf, off + fixed + 2 * i));
            }
            let cmd = if op == b's' {
                DrawCmd::String {
                    dst_id: id(1),
                    src_id: id(5),
                    font_id: id(9),
                    p: pt(13),
                    clip_r: rect(21),
                    sp: pt(37),
                    indices,
                }
            } else {
                DrawCmd::StringBg {
                    dst_id: id(1),
                    src_id: id(5),
                    font_id: id(9),
                    p: pt(13),
                    clip_r: rect(21),
                    sp: pt(37),
                    bg_id: id(47),
                    bg_pt: pt(51),
                    indices,
                }
            };
            (cmd, off + fixed + 2 * ni)
        }
        b't' => {
            need(buf, off, 4)?;
            let nw = le16(buf, off + 2) as usize;
            need(buf, off, 4 + 4 * nw)?;
            let mut ids = Vec::with_capacity(nw);
            for i in 0..nw {
                ids.push(le32(buf, off + 4 + 4 * i));
            }
            (
                DrawCmd::Top {
                    top: buf[off + 1],
                    ids,
                },
                off + 4 + 4 * nw,
            )
        }
        b'y' | b'Y' => {
            need(buf, off, 21)?;
            let wid = id(1);
            let r = rect(5);
            if op == b'Y' {
                // v0: the compressed stream's wire length is only knowable
                // by decompressing it, so it keeps devdraw's tail semantics
                // (SPEC.md §6) — clients send 'Y' as the last op, like acme.
                return Ok((
                    DrawCmd::WriteCompressed {
                        id: wid,
                        r,
                        data: buf[off + 21..].to_vec(),
                    },
                    buf.len(),
                ));
            }
            // 'y': memload consumes exactly bytesperline(R, depth)·Dy(R)
            // (libmemdraw/load.c:14-17, devdraw.c:1425 `m += y`) and the
            // stream continues with the next command.
            let len = match chans.get(&wid) {
                Some(&desc) => y_row_bytes(&r, chan_depth(desc)) as usize,
                // v0 fallback: image allocated in an earlier Twrdraw — the
                // parser has no depth for it, consume the tail (SPEC.md §6).
                None => buf.len() - (off + 21),
            };
            need(buf, off, 21 + len)?;
            (
                DrawCmd::WritePixels {
                    id: wid,
                    r,
                    data: buf[off + 21..off + 21 + len].to_vec(),
                },
                off + 21 + len,
            )
        }
        _ => return Err(ProtocolError::UnknownDrawCmd { op, offset: off }),
    })
}

// --- encoding (SPEC.md §6, little-endian) ----------------------------------
//
// The inverse of [parse_drawcmds]: pack commands back into the wire
// stream. Field offsets above are the single source of truth; the encoder
// mirrors them field by field. Round-trip identity encode∘parse == id is
// regression-tested against every Twrdraw payload of the live acme
// captures (tests/drawcmd_roundtrip.rs).

/// Encode commands into the packed inner draw stream (SPEC.md §6,
/// little-endian). [DrawCmd::Unknown] has no wire form (a packed stream
/// cannot skip an unknown command) and is skipped.
pub fn encode_drawcmds(cmds: &[DrawCmd]) -> Vec<u8> {
    let mut out = Vec::new();
    for cmd in cmds {
        encode_one(cmd, &mut out);
    }
    out
}

fn encode_one(cmd: &DrawCmd, out: &mut Vec<u8>) {
    fn u16(v: u16, out: &mut Vec<u8>) {
        out.extend_from_slice(&v.to_le_bytes());
    }
    fn u32(v: u32, out: &mut Vec<u8>) {
        out.extend_from_slice(&v.to_le_bytes());
    }
    fn point(p: &Point, out: &mut Vec<u8>) {
        u32(p.x, out);
        u32(p.y, out);
    }
    fn rect(r: &Rect, out: &mut Vec<u8>) {
        point(&r.min, out);
        point(&r.max, out);
    }
    match cmd {
        DrawCmd::Allocate { id, screen_id, refresh, chan, repl, r, clip_r, value } => {
            out.push(b'b');
            u32(*id, out);
            // devdraw reads BGSHORT(a+5): the u16 lands in the low half and
            // bytes 7..9 are ignored — emit the zeroed high half.
            u16(*screen_id, out);
            out.extend_from_slice(&[0, 0]);
            out.push(*refresh);
            u32(*chan, out);
            out.push(*repl);
            rect(r, out);
            rect(clip_r, out);
            u32(*value, out);
        }
        DrawCmd::AllocScreen { id, image_id, fill_id, public } => {
            out.push(b'A');
            u32(*id, out);
            u32(*image_id, out);
            u32(*fill_id, out);
            out.push(*public);
        }
        DrawCmd::PublicScreen { id, chan } => {
            out.push(b'S');
            u32(*id, out);
            u32(*chan, out);
        }
        DrawCmd::ReplClip { dst_id, repl, clip_r } => {
            out.push(b'c');
            u32(*dst_id, out);
            out.push(*repl);
            rect(clip_r, out);
        }
        DrawCmd::Draw { dst_id, src_id, mask_id, r, src_pt, mask_pt } => {
            out.push(b'd');
            u32(*dst_id, out);
            u32(*src_id, out);
            u32(*mask_id, out);
            rect(r, out);
            point(src_pt, out);
            point(mask_pt, out);
        }
        DrawCmd::Debug { val } => {
            out.push(b'D');
            out.push(*val);
        }
        DrawCmd::Ellipse { filled, dst_id, src_id, center, a, b, thick, sp, ox, oy } => {
            out.push(if *filled { b'E' } else { b'e' });
            u32(*dst_id, out);
            u32(*src_id, out);
            point(center, out);
            u32(*a, out);
            u32(*b, out);
            u32(*thick, out);
            point(sp, out);
            u32(*ox, out);
            u32(*oy, out);
        }
        DrawCmd::Free { id } | DrawCmd::FreeScreen { id } => {
            out.push(if matches!(cmd, DrawCmd::Free { .. }) { b'f' } else { b'F' });
            u32(*id, out);
        }
        DrawCmd::InitFont { font_id, nchars, ascent } => {
            out.push(b'i');
            u32(*font_id, out);
            u32(*nchars, out);
            out.push(*ascent);
        }
        DrawCmd::Image0Screen => out.push(b'J'),
        DrawCmd::ReadInfo => out.push(b'I'),
        DrawCmd::Query { specs } => {
            out.push(b'q');
            out.push(specs.len() as u8);
            out.extend_from_slice(specs);
        }
        DrawCmd::LoadFont { font_id, src_id, index, r, sp, left, width } => {
            out.push(b'l');
            u32(*font_id, out);
            u32(*src_id, out);
            u16(*index, out);
            rect(r, out);
            point(sp, out);
            out.push(*left);
            out.push(*width);
        }
        DrawCmd::Line { dst_id, p0, p1, end0, end1, radius, src_id, sp } => {
            out.push(b'L');
            u32(*dst_id, out);
            point(p0, out);
            point(p1, out);
            u32(*end0, out);
            u32(*end1, out);
            u32(*radius, out);
            u32(*src_id, out);
            point(sp, out);
        }
        DrawCmd::AttachNamed { dst_id, name } => {
            out.push(b'n');
            u32(*dst_id, out);
            out.push(name.len() as u8);
            out.extend_from_slice(name.as_bytes());
        }
        DrawCmd::NameImage { dst_id, set, name } => {
            out.push(b'N');
            u32(*dst_id, out);
            out.push(u8::from(*set));
            out.push(name.len() as u8);
            out.extend_from_slice(name.as_bytes());
        }
        DrawCmd::Position { id, r_min, screen_r_min } => {
            out.push(b'o');
            u32(*id, out);
            point(r_min, out);
            point(screen_r_min, out);
        }
        DrawCmd::SetOp { op } => {
            out.push(b'O');
            out.push(*op);
        }
        DrawCmd::Polygon { dst_id, n, end0, end1, radius, src_id, sp, pts }
        | DrawCmd::FillPolygon { dst_id, n, wind: end0, ignore: [end1, radius], src_id, sp, pts } => {
            out.push(if matches!(cmd, DrawCmd::Polygon { .. }) { b'p' } else { b'P' });
            u32(*dst_id, out);
            u16(*n, out);
            u32(*end0, out);
            u32(*end1, out);
            u32(*radius, out);
            u32(*src_id, out);
            point(sp, out);
            // n+1 drawcoord vertices accumulated from (0, 0) — the module
            // quirk note on parse_one 'p'/'P' applies verbatim in reverse.
            let (mut ox, mut oy) = (0u32, 0u32);
            for p in pts {
                drawcoord_encode(out, ox, p.x);
                ox = p.x;
                drawcoord_encode(out, oy, p.y);
                oy = p.y;
            }
        }
        DrawCmd::ReadPixels { id, r } => {
            out.push(b'r');
            u32(*id, out);
            rect(r, out);
        }
        DrawCmd::String { dst_id, src_id, font_id, p, clip_r, sp, indices } => {
            out.push(b's');
            string_head(dst_id, src_id, font_id, p, clip_r, sp, indices.len(), out);
            push_indices(indices, out);
        }
        DrawCmd::StringBg { dst_id, src_id, font_id, p, clip_r, sp, bg_id, bg_pt, indices } => {
            out.push(b'x');
            string_head(dst_id, src_id, font_id, p, clip_r, sp, indices.len(), out);
            u32(*bg_id, out);
            point(bg_pt, out);
            push_indices(indices, out);
        }
        DrawCmd::Top { top, ids } => {
            out.push(b't');
            out.push(*top);
            u16(ids.len() as u16, out);
            for id in ids {
                u32(*id, out);
            }
        }
        DrawCmd::Flush => out.push(b'v'),
        DrawCmd::WritePixels { id, r, data } => {
            out.push(b'y');
            u32(*id, out);
            rect(r, out);
            out.extend_from_slice(data);
        }
        DrawCmd::WriteCompressed { id, r, data } => {
            out.push(b'Y');
            u32(*id, out);
            rect(r, out);
            out.extend_from_slice(data);
        }
        DrawCmd::Unknown { .. } => {}
    }
}

/// Shared prefix of 's'/'x': dst/src/font ids, p, clipr, sp, ni — the
/// fields 'x' interleaves bgid/bgpt after. LE, like everything here.
fn string_head(
    dst_id: &u32,
    src_id: &u32,
    font_id: &u32,
    p: &Point,
    clip_r: &Rect,
    sp: &Point,
    ni: usize,
    out: &mut Vec<u8>,
) {
    out.extend_from_slice(&dst_id.to_le_bytes());
    out.extend_from_slice(&src_id.to_le_bytes());
    out.extend_from_slice(&font_id.to_le_bytes());
    out.extend_from_slice(&p.x.to_le_bytes());
    out.extend_from_slice(&p.y.to_le_bytes());
    out.extend_from_slice(&clip_r.min.x.to_le_bytes());
    out.extend_from_slice(&clip_r.min.y.to_le_bytes());
    out.extend_from_slice(&clip_r.max.x.to_le_bytes());
    out.extend_from_slice(&clip_r.max.y.to_le_bytes());
    out.extend_from_slice(&sp.x.to_le_bytes());
    out.extend_from_slice(&sp.y.to_le_bytes());
    out.extend_from_slice(&(ni as u16).to_le_bytes());
}

/// The 'ni' rune indices closing 's'/'x'.
fn push_indices(indices: &[u16], out: &mut Vec<u8>) {
    for ix in indices {
        out.extend_from_slice(&ix.to_le_bytes());
    }
}

/// Inverse of the drawcoord decoder (SPEC.md §6): a delta inside the
/// 7-bit signed range rides the 1-byte form, anything else the 3-byte
/// absolute form (bit7 set, low 23 bits, bit22 sign-extends on read).
fn drawcoord_encode(out: &mut Vec<u8>, old: u32, x: u32) {
    let delta = (x as i32).wrapping_sub(old as i32);
    if (-64..=63).contains(&delta) {
        out.push((delta as u8) & 0x7F);
    } else {
        out.push(0x80 | (x & 0x7F) as u8);
        out.push(((x >> 7) & 0xFF) as u8);
        out.push(((x >> 15) & 0xFF) as u8);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn u16le(v: u16) -> Vec<u8> {
        v.to_le_bytes().to_vec()
    }

    fn u32le(v: u32) -> Vec<u8> {
        v.to_le_bytes().to_vec()
    }

    /// Wire point/rect bytes from (possibly negative) C ints — the u32
    /// fields hold the wrapped wire bits, like the outer codec.
    fn pt(x: i32, y: i32) -> Vec<u8> {
        let mut v = u32le(x as u32);
        v.extend(u32le(y as u32));
        v
    }

    fn rect(x0: i32, y0: i32, x1: i32, y1: i32) -> Vec<u8> {
        let mut v = pt(x0, y0);
        v.extend(pt(x1, y1));
        v
    }

    fn wire_point(x: i32, y: i32) -> Point {
        Point {
            x: x as u32,
            y: y as u32,
        }
    }

    fn wire_rect(x0: i32, y0: i32, x1: i32, y1: i32) -> Rect {
        Rect {
            min: wire_point(x0, y0),
            max: wire_point(x1, y1),
        }
    }

    fn parse1(bytes: &[u8]) -> DrawCmd {
        let cmds = parse_drawcmds(bytes).expect("single command parses");
        assert_eq!(cmds.len(), 1, "exactly one command in {bytes:?}");
        cmds.into_iter().next().unwrap()
    }

    #[test]
    fn empty_payload_yields_no_commands() {
        assert_eq!(parse_drawcmds(&[]), Ok(vec![]));
    }

    #[test]
    fn allocate_matches_capture_bytes() {
        // The first 'b' of the live capture (fixtures-analysis §2): 1×1 GREY1
        // repl tile, chan 0x31, repl 1, value ff ff ff ff, screen rect.
        let mut b = vec![b'b'];
        b.extend(u32le(1)); // id
        b.extend([0, 0, 0, 0]); // screenid (u16 0 + 2 ignored)
        b.push(0); // refresh = Refbackup
        b.extend(u32le(0x31)); // GREY1
        b.push(1); // repl
        b.extend(rect(0, 0, 1939, 1293));
        b.extend(rect(0, 0, 1939, 1293));
        b.extend(u32le(0xffff_ffff));
        assert_eq!(b.len(), 51);
        assert_eq!(
            parse1(&b),
            DrawCmd::Allocate {
                id: 1,
                screen_id: 0,
                refresh: 0,
                chan: 0x31,
                repl: 1,
                r: wire_rect(0, 0, 1939, 1293),
                clip_r: wire_rect(0, 0, 1939, 1293),
                value: 0xffff_ffff,
            }
        );
    }

    #[test]
    fn allocate_screenid_reads_u16_and_ignores_high_bytes() {
        // devdraw: scrnid = BGSHORT(a+5) — bytes 7..9 are never looked at.
        let mut b = vec![b'b'];
        b.extend(u32le(3));
        b.extend([0x01, 0x00, 0xAB, 0xCD]); // u16 = 1, garbage high half
        b.push(1); // refresh = Refnone (observed on the capture window)
        b.extend(u32le(0x6808_1828)); // x8r8g8b8
        b.push(0); // windows: repl = 0
        b.extend(rect(0, 0, 100, 50));
        b.extend(rect(0, 0, 100, 50));
        b.extend(u32le(0));
        match parse1(&b) {
            DrawCmd::Allocate { screen_id, chan, repl, refresh, .. } => {
                assert_eq!(screen_id, 1);
                assert_eq!(chan, 0x6808_1828);
                assert_eq!(repl, 0);
                assert_eq!(refresh, 1);
            }
            other => panic!("expected Allocate, got {other:?}"),
        }
    }

    #[test]
    fn y_takes_exact_rows_when_depth_is_in_stream() {
        // devdraw.c:1425 advances by what memload consumed — exactly
        // bytesperline(R, depth)·Dy(R) (libmemdraw/load.c:14-17). A 'y'
        // whose image was allocated earlier in the SAME stream must not
        // swallow the commands that follow it (broke the e2e text step:
        // 'l'/'x'/'v' after 'y' never ran).
        let mut stream = vec![b'b'];
        stream.extend(u32le(5));
        stream.extend([0, 0, 0, 0]);
        stream.push(0); // refresh
        stream.extend(u32le(0x6808_1828)); // x8r8g8b8
        stream.push(0); // repl
        stream.extend(rect(0, 0, 4, 2));
        stream.extend(rect(0, 0, 4, 2));
        stream.extend(u32le(0));
        let pixels: Vec<u8> = (0..32).collect(); // 4·2·4 bytes
        stream.push(b'y');
        stream.extend(u32le(5));
        stream.extend(rect(0, 0, 4, 2));
        stream.extend(pixels.iter().copied());
        stream.push(b'v'); // must survive, not be eaten by 'y'
        let cmds = parse_drawcmds(&stream).expect("parses");
        assert_eq!(cmds.len(), 3, "b, y, v");
        match &cmds[1] {
            DrawCmd::WritePixels { id: 5, r, data } => {
                assert_eq!(*r, wire_rect(0, 0, 4, 2));
                assert_eq!(data, &pixels, "exactly rows, no tail");
            }
            other => panic!("expected WritePixels, got {other:?}"),
        }
        assert_eq!(cmds[2], DrawCmd::Flush);
        assert_eq!(encode_drawcmds(&cmds), stream, "identity");
    }

    #[test]
    fn y_without_in_stream_depth_falls_back_to_tail() {
        // v0: an image allocated in an earlier Twrdraw has no depth in this
        // stateless parser — the payload tail is consumed (SPEC.md §6).
        let mut stream = vec![b'y'];
        stream.extend(u32le(9));
        stream.extend(rect(0, 0, 2, 1));
        stream.extend([1, 2, 3, 4, 5, 6, 7, 8]);
        stream.push(b'v');
        let cmds = parse_drawcmds(&stream).expect("parses");
        assert_eq!(cmds.len(), 1, "tail fallback keeps old behavior");
    }

    #[test]
    fn fixed_size_ops_roundtrip() {
        let cases: Vec<(Vec<u8>, DrawCmd)> = vec![
            (
                [b'A'].iter().copied()
                    .chain(u32le(1))
                    .chain(u32le(0))
                    .chain(u32le(1))
                    .chain([0])
                    .collect(),
                DrawCmd::AllocScreen { id: 1, image_id: 0, fill_id: 1, public: 0 },
            ),
            (
                [b'S'].iter().copied().chain(u32le(1)).chain(u32le(0x31)).collect(),
                DrawCmd::PublicScreen { id: 1, chan: 0x31 },
            ),
            (
                [b'c'].iter().copied().chain(u32le(4)).chain([1]).chain(rect(0, 0, 9, 9)).collect(),
                DrawCmd::ReplClip { dst_id: 4, repl: 1, clip_r: wire_rect(0, 0, 9, 9) },
            ),
            (
                [b'd'].iter().copied()
                    .chain(u32le(3)).chain(u32le(1)).chain(u32le(1))
                    .chain(rect(10, 20, 30, 40))
                    .chain(pt(1, 2))
                    .chain(pt(3, 4))
                    .collect(),
                DrawCmd::Draw {
                    dst_id: 3,
                    src_id: 1,
                    mask_id: 1,
                    r: wire_rect(10, 20, 30, 40),
                    src_pt: wire_point(1, 2),
                    mask_pt: wire_point(3, 4),
                },
            ),
            (vec![b'D', 1], DrawCmd::Debug { val: 1 }),
            (
                // 'e' with the arc bits (bit31 + bit30) preserved raw.
                [b'e'].iter().copied()
                    .chain(u32le(2)).chain(u32le(1))
                    .chain(pt(50, 60))
                    .chain(u32le(10)).chain(u32le(20)).chain(u32le(0))
                    .chain(pt(0, 0))
                    .chain(u32le(0xC000_0042)).chain(u32le(0x2B))
                    .collect(),
                DrawCmd::Ellipse {
                    filled: false,
                    dst_id: 2,
                    src_id: 1,
                    center: wire_point(50, 60),
                    a: 10,
                    b: 20,
                    thick: 0,
                    sp: wire_point(0, 0),
                    ox: 0xC000_0042,
                    oy: 0x2B,
                },
            ),
            (
                [b'E'].iter().copied()
                    .chain(u32le(2)).chain(u32le(1))
                    .chain(pt(50, 60))
                    .chain(u32le(10)).chain(u32le(20)).chain(u32le(3))
                    .chain(pt(0, 0))
                    .chain(u32le(0)).chain(u32le(0))
                    .collect(),
                DrawCmd::Ellipse {
                    filled: true,
                    dst_id: 2,
                    src_id: 1,
                    center: wire_point(50, 60),
                    a: 10,
                    b: 20,
                    thick: 3,
                    sp: wire_point(0, 0),
                    ox: 0,
                    oy: 0,
                },
            ),
            ([b'f'].iter().copied().chain(u32le(5)).collect(), DrawCmd::Free { id: 5 }),
            ([b'F'].iter().copied().chain(u32le(6)).collect(), DrawCmd::FreeScreen { id: 6 }),
            (
                [b'i'].iter().copied().chain(u32le(9)).chain(u32le(256)).chain([7]).collect(),
                DrawCmd::InitFont { font_id: 9, nchars: 256, ascent: 7 },
            ),
            (vec![b'J'], DrawCmd::Image0Screen),
            (vec![b'I'], DrawCmd::ReadInfo),
            (vec![b'v'], DrawCmd::Flush),
            (
                [b'l'].iter().copied()
                    .chain(u32le(9)).chain(u32le(2))
                    .chain(u16le(65))
                    .chain(rect(0, 0, 8, 12))
                    .chain(pt(1, 1))
                    .chain([2, 8])
                    .collect(),
                DrawCmd::LoadFont {
                    font_id: 9,
                    src_id: 2,
                    index: 65,
                    r: wire_rect(0, 0, 8, 12),
                    sp: wire_point(1, 1),
                    left: 2,
                    width: 8,
                },
            ),
            (
                [b'L'].iter().copied()
                    .chain(u32le(3))
                    .chain(pt(0, 0)).chain(pt(80, 60))
                    .chain(u32le(1)).chain(u32le(2)).chain(u32le(4))
                    .chain(u32le(1))
                    .chain(pt(5, 5))
                    .collect(),
                DrawCmd::Line {
                    dst_id: 3,
                    p0: wire_point(0, 0),
                    p1: wire_point(80, 60),
                    end0: 1,
                    end1: 2,
                    radius: 4,
                    src_id: 1,
                    sp: wire_point(5, 5),
                },
            ),
            (
                [b'o'].iter().copied().chain(u32le(3)).chain(pt(11, 22)).chain(pt(33, 44)).collect(),
                DrawCmd::Position {
                    id: 3,
                    r_min: wire_point(11, 22),
                    screen_r_min: wire_point(33, 44),
                },
            ),
            (vec![b'O', 7], DrawCmd::SetOp { op: 7 }),
            (
                [b'r'].iter().copied().chain(u32le(3)).chain(rect(0, 0, 16, 16)).collect(),
                DrawCmd::ReadPixels { id: 3, r: wire_rect(0, 0, 16, 16) },
            ),
        ];
        for (bytes, want) in &cases {
            assert_eq!(&parse1(bytes), want, "case {want:?}");
        }
    }

    #[test]
    fn string_and_stringbg_read_indices() {
        // 's' dst src font p clipr sp ni=2 indices[2]
        let mut s = vec![b's'];
        s.extend(u32le(3));
        s.extend(u32le(1));
        s.extend(u32le(9));
        s.extend(pt(10, 20));
        s.extend(rect(0, 0, 200, 20));
        s.extend(pt(0, 0));
        s.extend(u16le(2));
        s.extend(u16le(65));
        s.extend(u16le(66));
        assert_eq!(
            parse1(&s),
            DrawCmd::String {
                dst_id: 3,
                src_id: 1,
                font_id: 9,
                p: wire_point(10, 20),
                clip_r: wire_rect(0, 0, 200, 20),
                sp: wire_point(0, 0),
                indices: vec![65, 66],
            }
        );

        // 'x' — same head, bgid@47 bgpt@51 before the indices.
        let mut x = vec![b'x'];
        x.extend(u32le(3));
        x.extend(u32le(1));
        x.extend(u32le(9));
        x.extend(pt(10, 20));
        x.extend(rect(0, 0, 200, 20));
        x.extend(pt(0, 0));
        x.extend(u16le(1));
        x.extend(u32le(2));
        x.extend(pt(4, 5));
        x.extend(u16le(65));
        assert_eq!(
            parse1(&x),
            DrawCmd::StringBg {
                dst_id: 3,
                src_id: 1,
                font_id: 9,
                p: wire_point(10, 20),
                clip_r: wire_rect(0, 0, 200, 20),
                sp: wire_point(0, 0),
                bg_id: 2,
                bg_pt: wire_point(4, 5),
                indices: vec![65],
            }
        );
    }

    #[test]
    fn top_reads_window_id_list() {
        let mut b = vec![b't', 1];
        b.extend(u16le(2));
        b.extend(u32le(7));
        b.extend(u32le(8));
        assert_eq!(parse1(&b), DrawCmd::Top { top: 1, ids: vec![7, 8] });
        // nw = 0 is legal (devdraw continues immediately).
        assert_eq!(
            parse_drawcmds(&[b't', 0, 0, 0]),
            Ok(vec![DrawCmd::Top { top: 0, ids: vec![] }])
        );
    }

    #[test]
    fn name_commands_carry_utf8_names() {
        let mut n = vec![b'n'];
        n.extend(u32le(4));
        n.push(4);
        n.extend(b"font");
        assert_eq!(
            parse1(&n),
            DrawCmd::AttachNamed { dst_id: 4, name: "font".into() }
        );

        let mut m = vec![b'N'];
        m.extend(u32le(4));
        m.push(1); // set
        m.push(3);
        m.extend(b"win");
        assert_eq!(
            parse1(&m),
            DrawCmd::NameImage { dst_id: 4, set: true, name: "win".into() }
        );
    }

    #[test]
    fn query_carries_spec_bytes() {
        // Exact bytes of the live capture's second Twrdraw.
        assert_eq!(
            parse_drawcmds(b"q\x01d"),
            Ok(vec![DrawCmd::Query { specs: vec![b'd'] }])
        );
        let mut many = vec![b'q', 3, b'd', b'd', b'd'];
        many.push(b'v'); // next command right after the query
        assert_eq!(
            parse_drawcmds(&many),
            Ok(vec![
                DrawCmd::Query { specs: vec![b'd', b'd', b'd'] },
                DrawCmd::Flush,
            ])
        );
    }

    #[test]
    fn polygon_decodes_absolute_and_delta_coords() {
        // n=1 → 2 vertices from offset 31, accumulated from (0, 0).
        // v0 = (0x1A2B3C, -1): absolute 3-byte forms BC 56 34 / FF FF FF.
        // v1 = (+5, -3) deltas: 05 / 7D → (0x1A2B3C+5, -4).
        let mut p = vec![b'p'];
        p.extend(u32le(3));
        p.extend(u16le(1));
        p.extend(u32le(1)); // end0
        p.extend(u32le(2)); // end1
        p.extend(u32le(0)); // radius
        p.extend(u32le(1)); // src
        p.extend(pt(0, 0)); // sp
        p.extend([0x80 | 0x3C, 0x56, 0x34]); // x0 absolute = 0x1A2B3C
        p.extend([0xFF, 0xFF, 0xFF]); // y0 absolute = -1
        p.extend([0x05]); // x1 = x0 + 5
        p.extend([0x40 | 0x7D]); // y1 = y0 - 3 (7-bit negative delta)
        assert_eq!(
            parse1(&p),
            DrawCmd::Polygon {
                dst_id: 3,
                n: 1,
                end0: 1,
                end1: 2,
                radius: 0,
                src_id: 1,
                sp: wire_point(0, 0),
                pts: vec![wire_point(0x1A_2B3C, -1), wire_point(0x1A_2B41, -4)],
            }
        );
    }

    #[test]
    fn fill_polygon_maps_wind_ignore_slots() {
        let mut b = vec![b'P'];
        b.extend(u32le(5));
        b.extend(u16le(0)); // n = 0 → one vertex
        b.extend(u32le(1)); // wind
        b.extend(u32le(11)); // ignore[0]
        b.extend(u32le(22)); // ignore[1]
        b.extend(u32le(1)); // src
        b.extend(pt(0, 0));
        b.extend([0x0A]); // delta x = 10 from 0
        b.extend([0x14]); // delta y = 20 from 0
        assert_eq!(
            parse1(&b),
            DrawCmd::FillPolygon {
                dst_id: 5,
                n: 0,
                wind: 1,
                ignore: [11, 22],
                src_id: 1,
                sp: wire_point(0, 0),
                pts: vec![wire_point(10, 20)],
            }
        );
    }

    #[test]
    fn polygon_truncation_reports_command_relative_need() {
        // 31 fixed bytes + 2 vertices of one-byte deltas = 35 bytes total.
        // Cutting the last byte leaves the final y-coordinate short by one.
        let mut p = vec![b'p'];
        p.extend(u32le(3));
        p.extend(u16le(1));
        p.extend(u32le(0)); // end0
        p.extend(u32le(0)); // end1
        p.extend(u32le(0)); // radius
        p.extend(u32le(1)); // src
        p.extend(pt(0, 0)); // sp
        p.extend([0x0A, 0x14]); // v0 = (10, 20)
        p.extend([0x0A, 0x14]); // v1 = (20, 40)
        assert_eq!(p.len(), 35);
        let e = parse_drawcmds(&p[..34]).unwrap_err();
        assert_eq!(
            e,
            ProtocolError::DrawCmdTruncated { op: b'p', offset: 0, needed: 35, got: 34 }
        );
        // Sanity: the full buffer parses.
        assert!(parse_drawcmds(&p).is_ok());
    }

    #[test]
    fn write_pixels_take_the_rest_of_the_payload() {
        // devdraw memload receives n-m bytes: everything after the header.
        let mut b = vec![b'y'];
        b.extend(u32le(3));
        b.extend(rect(0, 0, 4, 4));
        b.extend([0xDE, 0xAD, 0xBE]);
        assert_eq!(
            parse1(&b),
            DrawCmd::WritePixels { id: 3, r: wire_rect(0, 0, 4, 4), data: vec![0xDE, 0xAD, 0xBE] }
        );
        // Even bytes that would otherwise look like a command belong to data
        // (a full 21-byte header plus one tail byte).
        let mut t = vec![b'y'];
        t.extend(u32le(0));
        t.extend(rect(0, 0, 0, 0));
        t.push(b'J');
        assert_eq!(
            parse_drawcmds(&t).unwrap(),
            vec![DrawCmd::WritePixels { id: 0, r: wire_rect(0, 0, 0, 0), data: vec![b'J'] }]
        );
        // Empty tail is a layout-valid header-only write.
        let mut h = vec![b'Y'];
        h.extend(u32le(3));
        h.extend(rect(0, 0, 1, 1));
        assert_eq!(
            parse1(&h),
            DrawCmd::WriteCompressed { id: 3, r: wire_rect(0, 0, 1, 1), data: vec![] }
        );
    }

    #[test]
    fn packed_sequences_parse_back_to_back() {
        // Capture frame 3 is exactly "JI"; add more to prove no padding.
        assert_eq!(
            parse_drawcmds(b"JI"),
            Ok(vec![DrawCmd::Image0Screen, DrawCmd::ReadInfo])
        );
        assert_eq!(
            parse_drawcmds(b"JvI"),
            Ok(vec![DrawCmd::Image0Screen, DrawCmd::Flush, DrawCmd::ReadInfo])
        );
        // A 'd' followed by 'v' in one payload (capture frame with count=46).
        let mut b = vec![b'd'];
        b.extend(u32le(3));
        b.extend(u32le(1));
        b.extend(u32le(1));
        b.extend(rect(0, 0, 1939, 1293));
        b.extend(pt(0, 0));
        b.extend(pt(0, 0));
        b.push(b'v');
        assert_eq!(
            parse_drawcmds(&b).unwrap(),
            vec![
                DrawCmd::Draw {
                    dst_id: 3,
                    src_id: 1,
                    mask_id: 1,
                    r: wire_rect(0, 0, 1939, 1293),
                    src_pt: wire_point(0, 0),
                    mask_pt: wire_point(0, 0),
                },
                DrawCmd::Flush,
            ]
        );
    }

    #[test]
    fn unknown_op_aborts_with_offset() {
        assert_eq!(
            parse_drawcmds(b"z"),
            Err(ProtocolError::UnknownDrawCmd { op: b'z', offset: 0 })
        );
        // A valid command first, then garbage: error points at the garbage.
        assert_eq!(
            parse_drawcmds(b"Jz"),
            Err(ProtocolError::UnknownDrawCmd { op: b'z', offset: 1 })
        );
        // The variant stays constructible for server-side error synthesis.
        let u = DrawCmd::Unknown { op: 0xFF };
        assert_eq!(u, DrawCmd::Unknown { op: 255 });
    }

    #[test]
    fn truncated_fixed_ops_report_needed_and_got() {
        // 'd' minus its last byte.
        let mut d = vec![b'd'];
        d.extend(u32le(1));
        d.extend(u32le(2));
        d.extend(u32le(3));
        d.extend(rect(0, 0, 9, 9));
        d.extend(pt(0, 0));
        d.extend(pt(0, 0)); // mask_pt @37 (SPEC §6: 'd' is 45 bytes)
        assert_eq!(d.len(), 45);
        assert_eq!(
            parse_drawcmds(&d[..44]),
            Err(ProtocolError::DrawCmdTruncated { op: b'd', offset: 0, needed: 45, got: 44 })
        );
        // Second command truncated: offset points at its op byte.
        let mut two = vec![b'J'];
        two.extend_from_slice(&d[..44]);
        assert_eq!(
            parse_drawcmds(&two),
            Err(ProtocolError::DrawCmdTruncated { op: b'd', offset: 1, needed: 45, got: 44 })
        );
    }

    #[test]
    fn count_fields_must_fit_the_payload() {
        // 'q' promises 2 specs, delivers 1.
        assert_eq!(
            parse_drawcmds(b"q\x02d"),
            Err(ProtocolError::DrawCmdTruncated { op: b'q', offset: 0, needed: 4, got: 3 })
        );
        // 't' promises 2 ids, delivers none.
        assert_eq!(
            parse_drawcmds(&[b't', 1, 2, 0]),
            Err(ProtocolError::DrawCmdTruncated { op: b't', offset: 0, needed: 12, got: 4 })
        );
        // 's' promises ni=2 indices, delivers 1.
        let mut s = vec![b's'];
        s.extend(u32le(1));
        s.extend(u32le(1));
        s.extend(u32le(1));
        s.extend(pt(0, 0));
        s.extend(rect(0, 0, 9, 9));
        s.extend(pt(0, 0));
        s.extend(u16le(2));
        s.extend(u16le(65));
        assert_eq!(
            parse_drawcmds(&s),
            Err(ProtocolError::DrawCmdTruncated { op: b's', offset: 0, needed: 51, got: 49 })
        );
        // 'n' promises a 4-byte name, delivers 3.
        let mut n = vec![b'n'];
        n.extend(u32le(4));
        n.push(4);
        n.extend(b"fon");
        assert_eq!(
            parse_drawcmds(&n),
            Err(ProtocolError::DrawCmdTruncated { op: b'n', offset: 0, needed: 10, got: 9 })
        );
    }

    #[test]
    fn name_must_be_utf8_like_outer_strings() {
        let mut n = vec![b'N'];
        n.extend(u32le(4));
        n.push(1);
        n.push(2);
        n.extend([0xFF, 0xFE]); // invalid UTF-8 name bytes
        assert_eq!(
            parse_drawcmds(&n),
            Err(ProtocolError::DrawCmdBadUtf8 { op: b'N', offset: 0 })
        );
    }

    #[test]
    fn encode_roundtrips_wire_bytes() {
        // parse → encode must reproduce the wire bytes exactly (including
        // the ignored screenid high half and the counted tails), and
        // encode → parse must yield the same command.
        let cases: Vec<(Vec<u8>, DrawCmd)> = vec![
            (
                [b'b'].iter().copied()
                    .chain(u32le(3))
                    .chain([0x01, 0x00, 0, 0]) // u16 screenid, canonical zero half
                    .chain([1])
                    .chain(u32le(0x6808_1828))
                    .chain([0])
                    .chain(rect(0, 0, 100, 50))
                    .chain(rect(0, 0, 100, 50))
                    .chain(u32le(0))
                    .collect(),
                DrawCmd::Allocate {
                    id: 3,
                    screen_id: 1,
                    refresh: 1,
                    chan: 0x6808_1828,
                    repl: 0,
                    r: wire_rect(0, 0, 100, 50),
                    clip_r: wire_rect(0, 0, 100, 50),
                    value: 0,
                },
            ),
            (
                [b'd'].iter().copied()
                    .chain(u32le(3)).chain(u32le(1)).chain(u32le(0))
                    .chain(rect(0, 0, 120, 90))
                    .chain(pt(0, 0))
                    .chain(pt(0, 0))
                    .collect(),
                DrawCmd::Draw {
                    dst_id: 3,
                    src_id: 1,
                    mask_id: 0,
                    r: wire_rect(0, 0, 120, 90),
                    src_pt: wire_point(0, 0),
                    mask_pt: wire_point(0, 0),
                },
            ),
            (
                [b'y'].iter().copied()
                    .chain(u32le(7))
                    .chain(rect(0, 0, 2, 1))
                    .chain([1, 2, 3, 4, 5, 6, 7, 8])
                    .collect(),
                DrawCmd::WritePixels {
                    id: 7,
                    r: wire_rect(0, 0, 2, 1),
                    data: vec![1, 2, 3, 4, 5, 6, 7, 8],
                },
            ),
            (
                [b'Y'].iter().copied()
                    .chain(u32le(7))
                    .chain(rect(0, 0, 2, 1))
                    .chain([0xAA, 0xBB])
                    .collect(),
                DrawCmd::WriteCompressed {
                    id: 7,
                    r: wire_rect(0, 0, 2, 1),
                    data: vec![0xAA, 0xBB],
                },
            ),
            (
                [b'q', 2, b'd', b'd'].to_vec(),
                DrawCmd::Query { specs: vec![b'd', b'd'] },
            ),
            (
                [b't', 1].iter().copied()
                    .chain(u16le(2))
                    .chain(u32le(1)).chain(u32le(2))
                    .collect(),
                DrawCmd::Top { top: 1, ids: vec![1, 2] },
            ),
            (
                [b'n'].iter().copied()
                    .chain(u32le(4))
                    .chain([3])
                    .chain(b"foo".iter().copied())
                    .collect(),
                DrawCmd::AttachNamed { dst_id: 4, name: "foo".to_string() },
            ),
            (
                [b's'].iter().copied()
                    .chain(u32le(1)).chain(u32le(1)).chain(u32le(1))
                    .chain(pt(0, 0))
                    .chain(rect(0, 0, 9, 9))
                    .chain(pt(0, 0))
                    .chain(u16le(2))
                    .chain(u16le(65)).chain(u16le(66))
                    .collect(),
                DrawCmd::String {
                    dst_id: 1,
                    src_id: 1,
                    font_id: 1,
                    p: wire_point(0, 0),
                    clip_r: wire_rect(0, 0, 9, 9),
                    sp: wire_point(0, 0),
                    indices: vec![65, 66],
                },
            ),
            (
                [b'x'].iter().copied()
                    .chain(u32le(1)).chain(u32le(1)).chain(u32le(1))
                    .chain(pt(0, 0))
                    .chain(rect(0, 0, 9, 9))
                    .chain(pt(0, 0))
                    .chain(u16le(1))
                    .chain(u32le(2))
                    .chain(pt(1, 1))
                    .chain(u16le(65))
                    .collect(),
                DrawCmd::StringBg {
                    dst_id: 1,
                    src_id: 1,
                    font_id: 1,
                    p: wire_point(0, 0),
                    clip_r: wire_rect(0, 0, 9, 9),
                    sp: wire_point(0, 0),
                    bg_id: 2,
                    bg_pt: wire_point(1, 1),
                    indices: vec![65],
                },
            ),
        ];
        for (wire, cmd) in &cases {
            assert_eq!(encode_drawcmds(&[cmd.clone()]), *wire, "encode {cmd:?}");
            assert_eq!(parse_drawcmds(&wire), Ok(vec![cmd.clone()]), "reparse {cmd:?}");
        }
        // The screenid high half is ignored on read (BGSHORT), so the
        // encoder's contract there is one-sided: garbage is accepted and
        // normalized to zeros, never reproduced.
        let mut garbage = cases[0].0.clone();
        garbage[7] = 0xAB;
        garbage[8] = 0xCD;
        assert_eq!(parse_drawcmds(&garbage), Ok(vec![cases[0].1.clone()]));
        assert_eq!(encode_drawcmds(&[cases[0].1.clone()]), cases[0].0);
    }

    #[test]
    fn encode_polygon_roundtrips_semantically() {
        // Byte identity for 'p' depends on the client's delta-vs-absolute
        // drawcoord choices, so the encoder contract for polygons is
        // semantic: parse(encode(cmd)) == cmd, and the wire length matches
        // the layout (31 header + n+1 vertices of 1 or 3 bytes each).
        let cmd = DrawCmd::Polygon {
            dst_id: 5,
            n: 2,
            end0: 1,
            end1: 2,
            radius: 3,
            src_id: 4,
            sp: wire_point(0, 0),
            pts: vec![wire_point(10, 12), wire_point(200, -30), wire_point(100_000, 7)],
        };
        let wire = encode_drawcmds(&[cmd.clone()]);
        assert_eq!(parse_drawcmds(&wire), Ok(vec![cmd]));
    }

    #[test]
    fn all_letters_are_known_or_erroring() {
        // Every SPEC §6 letter is either parseable or reports a truncation
        // on a bare op byte; every other byte (including 'm', commented out
        // in devdraw) must be rejected as unknown.
        for op in b'@'..=b'z' {
            let single = [op];
            let res = parse_drawcmds(&single);
            let known = b"bAScdDeEfFiJIqvOlLnNopPrstxyY".contains(&op);
            let shape_ok =
                res.is_ok() || matches!(res, Err(ProtocolError::DrawCmdTruncated { .. }));
            assert_eq!(
                shape_ok, known,
                "letter {} must be {} (got {res:?})",
                op as char,
                if known { "known" } else { "rejected" }
            );
        }
    }
}
