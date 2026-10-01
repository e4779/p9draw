//! Frame encoding (SPEC.md §2.2, §3): `size[4 BE] tag[1] type[1] payload`.
//!
//! Mirrors `sizeW2M`/`convW2M` from plan9port `drawfcall.c`: integers are
//! big-endian, strings are `n[4 BE] + n bytes` without NUL, rectangles are
//! `min.x min.y max.x max.y` and points `x y`, all u32 BE.

use crate::messages::*;
use crate::{MAXWMSG, MIN_FRAME};

/// Full frame size that `encode(msg, tag)` produces — the analogue of
/// `sizeW2M` in drawfcall.c.
pub fn encoded_size(msg: &Wsysmsg) -> u32 {
    let payload: usize = match msg {
        Wsysmsg::Rerror { error } => 4 + error.len(),
        Wsysmsg::Rrdmouse { .. } => 17, // 4×u32 + 1 flag byte
        Wsysmsg::Tmoveto { .. } => 8,
        Wsysmsg::Tcursor { .. } => CURSOR_PAYLOAD,
        Wsysmsg::Tbouncemouse { .. } => 12,
        Wsysmsg::Rrdkbd { .. } => 2,
        Wsysmsg::Tlabel { label } => 4 + label.len(),
        Wsysmsg::Tinit { winsize, label } => 8 + winsize.len() + label.len(),
        Wsysmsg::Rrdsnarf { snarf } | Wsysmsg::Twrsnarf { snarf } => 4 + snarf.len(),
        Wsysmsg::Trddraw { .. } | Wsysmsg::Rwrdraw { .. } => 4,
        Wsysmsg::Rrddraw { data } | Wsysmsg::Twrdraw { data } => 4 + data.len(),
        Wsysmsg::Tresize { .. } => 16,
        Wsysmsg::Tcursor2 { .. } => CURSOR2_PAYLOAD,
        Wsysmsg::Tctxt { id } => 4 + id.len(),
        Wsysmsg::Rrdkbd4 { .. } => 4,
        Wsysmsg::Trdmouse
        | Wsysmsg::Rmoveto
        | Wsysmsg::Rcursor
        | Wsysmsg::Rbouncemouse
        | Wsysmsg::Trdkbd
        | Wsysmsg::Rlabel
        | Wsysmsg::Rinit
        | Wsysmsg::Trdsnarf
        | Wsysmsg::Rwrsnarf
        | Wsysmsg::Ttop
        | Wsysmsg::Rtop
        | Wsysmsg::Rresize
        | Wsysmsg::Rcursor2
        | Wsysmsg::Rctxt
        | Wsysmsg::Trdkbd4 => 0,
    };
    let total = (MIN_FRAME + payload) as u32;
    debug_assert!(total <= MAXWMSG, "encoded frame exceeds MAXWMSG");
    total
}

/// Encode into a freshly allocated vector.
pub fn encode(msg: &Wsysmsg, tag: u8) -> Vec<u8> {
    let mut buf = Vec::with_capacity(encoded_size(msg) as usize);
    encode_into(msg, tag, &mut buf);
    buf
}

/// Append the encoded frame to `buf`.
pub fn encode_into(msg: &Wsysmsg, tag: u8, buf: &mut Vec<u8>) {
    buf.extend_from_slice(&encoded_size(msg).to_be_bytes());
    buf.push(tag);
    buf.push(msg.msg_type());
    match msg {
        Wsysmsg::Rerror { error } => put_string(buf, error),
        Wsysmsg::Trdmouse => {}
        Wsysmsg::Rrdmouse { x, y, buttons, msec, resized } => {
            put_u32(buf, *x);
            put_u32(buf, *y);
            put_u32(buf, *buttons);
            // drawfcall.c convW2M: `PUT(p+18, msec); p[19] = resized` — the
            // flag overwrites byte 1 of the msec group, so msec bits
            // 16..24 are not representable on the wire. Byte 22 is counted
            // by sizeW2M but written by nobody (devdraw leaves stale
            // buffer garbage no receiver reads); we pad 0.
            let mut m = msec.to_be_bytes();
            m[1] = *resized;
            buf.extend_from_slice(&m);
            buf.push(0);
        }
        Wsysmsg::Tmoveto { x, y } => {
            put_u32(buf, *x);
            put_u32(buf, *y);
        }
        Wsysmsg::Rmoveto => {}
        Wsysmsg::Tcursor { cursor } => put_cursor(buf, cursor),
        Wsysmsg::Rcursor => {}
        Wsysmsg::Tbouncemouse { x, y, buttons } => {
            put_u32(buf, *x);
            put_u32(buf, *y);
            put_u32(buf, *buttons);
        }
        Wsysmsg::Rbouncemouse => {}
        Wsysmsg::Trdkbd => {}
        Wsysmsg::Rrdkbd { rune } => put_u16(buf, *rune),
        Wsysmsg::Tlabel { label } => put_string(buf, label),
        Wsysmsg::Rlabel => {}
        Wsysmsg::Tinit { winsize, label } => {
            // Wire order: winsize first, then label (SPEC.md §2.4).
            put_string(buf, winsize);
            put_string(buf, label);
        }
        Wsysmsg::Rinit => {}
        Wsysmsg::Trdsnarf => {}
        Wsysmsg::Rrdsnarf { snarf } => put_string(buf, snarf),
        Wsysmsg::Twrsnarf { snarf } => put_string(buf, snarf),
        Wsysmsg::Rwrsnarf => {}
        Wsysmsg::Trddraw { count } => put_u32(buf, *count),
        Wsysmsg::Rrddraw { data } => put_data(buf, data),
        Wsysmsg::Twrdraw { data } => put_data(buf, data),
        Wsysmsg::Rwrdraw { count } => put_u32(buf, *count),
        Wsysmsg::Ttop => {}
        Wsysmsg::Rtop => {}
        Wsysmsg::Tresize { rect } => put_rect(buf, rect),
        Wsysmsg::Rresize => {}
        Wsysmsg::Tcursor2 { cursor } => put_cursor2(buf, cursor),
        Wsysmsg::Rcursor2 => {}
        Wsysmsg::Tctxt { id } => put_string(buf, id),
        Wsysmsg::Rctxt => {}
        Wsysmsg::Trdkbd4 => {}
        Wsysmsg::Rrdkbd4 { rune } => put_u32(buf, *rune),
    }
}

fn put_u8(buf: &mut Vec<u8>, v: u8) {
    buf.push(v);
}

fn put_u16(buf: &mut Vec<u8>, v: u16) {
    buf.extend_from_slice(&v.to_be_bytes());
}

fn put_u32(buf: &mut Vec<u8>, v: u32) {
    buf.extend_from_slice(&v.to_be_bytes());
}

/// `n[4 BE] + n bytes`, no NUL (SPEC.md §3); empty string encodes n = 0.
fn put_string(buf: &mut Vec<u8>, s: &str) {
    put_u32(buf, s.len() as u32);
    buf.extend_from_slice(s.as_bytes());
}

fn put_data(buf: &mut Vec<u8>, data: &[u8]) {
    put_u32(buf, data.len() as u32);
    buf.extend_from_slice(data);
}

fn put_point(buf: &mut Vec<u8>, p: &Point) {
    put_u32(buf, p.x);
    put_u32(buf, p.y);
}

fn put_rect(buf: &mut Vec<u8>, r: &Rect) {
    put_point(buf, &r.min);
    put_point(buf, &r.max);
}

fn put_cursor(buf: &mut Vec<u8>, c: &Cursor) {
    put_point(buf, &c.offset);
    buf.extend_from_slice(&c.clr);
    buf.extend_from_slice(&c.set);
    put_u8(buf, u8::from(c.arrow));
}

fn put_cursor2(buf: &mut Vec<u8>, c: &Cursor2) {
    put_point(buf, &c.offset);
    buf.extend_from_slice(&c.clr);
    buf.extend_from_slice(&c.set);
    put_point(buf, &c.offset2);
    buf.extend_from_slice(&c.clr2);
    buf.extend_from_slice(&c.set2);
    put_u8(buf, u8::from(c.arrow));
}
