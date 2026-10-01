//! p9draw-protocol — pure codec for the plan9port devdraw wire protocol
//! (drawfcall / `Wsysmsg`), as specified in `SPEC.md`.
//!
//! Frame layout (both directions):
//!
//! ```text
//! size[4 BE] tag[1] type[1] payload...
//! ```
//!
//! * `size` counts the whole frame including its own 4 bytes (minimum 6).
//! * Even `type` = client request; the reply is `type + 1`. `Rerror` (1) is
//!   the error reply to any request (SPEC.md §2.3).
//! * Primitives are big-endian; strings are `n[4 BE] + n bytes` without NUL.
//!   The *inner* draw stream carried inside `Twrdraw`/`Trddraw` is
//!   little-endian; [`DrawCmd`] / [`parse_drawcmds`] decode it (SPEC.md §6).
//!
//! No IO and no dependencies: everything is a pure function over byte
//! slices, so the whole protocol surface is pinned by golden-byte tests.

mod decode;
mod drawcmd;
mod encode;
mod messages;

pub use decode::decode;
pub use drawcmd::{parse_drawcmds, DrawCmd};
pub use encode::{encode, encode_into, encoded_size};
pub use messages::*;

/// Minimum frame: `size[4] tag[1] type[1]` with empty payload.
pub const MIN_FRAME: usize = 6;

/// Client-side frame limit from drawfcall.h (`MAXWMSG`, 4 MiB). The
/// plan9port server does not enforce it (SPEC.md §8, OPEN-4); this codec
/// rejects larger declared sizes on decode.
pub const MAXWMSG: u32 = 4 << 20;

/// Everything that can go wrong while decoding a frame.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProtocolError {
    /// Buffer is shorter than [`MIN_FRAME`].
    FrameTooShort(usize),
    /// The `size[4]` prefix disagrees with the actual buffer length.
    SizeMismatch {
        declared: u32,
        actual: usize,
    },
    /// The `size[4]` prefix exceeds [`MAXWMSG`].
    FrameTooLarge(u32),
    /// Type byte outside the known `1..=33` range.
    UnknownType(u8),
    /// Frame ended in the middle of a fixed-size payload.
    PayloadTooShort {
        ty: u8,
        needed: usize,
        got: usize,
    },
    /// Leftover payload bytes after a fixed-size message.
    TrailingBytes {
        ty: u8,
        extra: usize,
    },
    /// The `count[4]` of a data segment disagrees with the remaining payload.
    CountMismatch {
        ty: u8,
        count: u32,
        remaining: usize,
    },
    /// A string field is not valid UTF-8 (Plan 9 strings are UTF-8).
    InvalidUtf8 {
        ty: u8,
    },
    /// Inner draw stream (SPEC.md §6): the op byte at `offset` has no entry
    /// in the command table. A packed stream cannot skip a command whose
    /// length is unknown, so parsing stops there (devdraw Rerrors the
    /// whole Twrdraw).
    UnknownDrawCmd {
        op: u8,
        offset: usize,
    },
    /// Inner draw command `op` starting at `offset` needs `needed` bytes,
    /// only `got` are available (fixed-size shortfall, or a cut variable
    /// part: counts, names, rune indices, drawcoord vertices).
    DrawCmdTruncated {
        op: u8,
        offset: usize,
        needed: usize,
        got: usize,
    },
    /// Inner draw command's `name` field is not valid UTF-8 (Plan 9 names
    /// are UTF-8, mirroring the outer-protocol string check).
    DrawCmdBadUtf8 {
        op: u8,
        offset: usize,
    },
}

impl std::fmt::Display for ProtocolError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::FrameTooShort(len) => {
                write!(f, "frame of {len} bytes is shorter than the {MIN_FRAME}-byte header")
            }
            Self::SizeMismatch { declared, actual } => {
                write!(f, "declared frame size {declared} does not match buffer length {actual}")
            }
            Self::FrameTooLarge(size) => {
                write!(f, "declared frame size {size} exceeds MAXWMSG ({MAXWMSG})")
            }
            Self::UnknownType(ty) => write!(f, "unknown message type {ty}"),
            Self::PayloadTooShort { ty, needed, got } => {
                write!(f, "type {ty}: need {needed} more payload bytes, got {got}")
            }
            Self::TrailingBytes { ty, extra } => {
                write!(f, "type {ty}: {extra} trailing payload bytes")
            }
            Self::CountMismatch { ty, count, remaining } => {
                write!(
                    f,
                    "type {ty}: data count {count} does not match {remaining} remaining payload bytes"
                )
            }
            Self::InvalidUtf8 { ty } => write!(f, "type {ty}: string field is not valid UTF-8"),
            Self::UnknownDrawCmd { op, offset } => {
                write!(f, "inner draw stream: unknown command {op:#04x} at offset {offset}")
            }
            Self::DrawCmdTruncated { op, offset, needed, got } => {
                write!(
                    f,
                    "inner draw command {op:#04x} at offset {offset}: need {needed} bytes, got {got}"
                )
            }
            Self::DrawCmdBadUtf8 { op, offset } => {
                write!(f, "inner draw command {op:#04x} at offset {offset}: name is not valid UTF-8")
            }
        }
    }
}

impl std::error::Error for ProtocolError {}

#[cfg(test)]
mod tests {
    use crate::*;

    fn hex(b: &[u8]) -> String {
        b.iter().map(|x| format!("{x:02x}")).collect()
    }

    fn unhex(s: &str) -> Vec<u8> {
        assert_eq!(s.len() % 2, 0, "odd hex length: {s}");
        (0..s.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap())
            .collect()
    }

    /// Bidirectional golden check: encode must produce exactly `golden`
    /// bytes, and `golden` must decode back into `msg` with the same tag.
    fn assert_golden(msg: &Wsysmsg, tag: u8, golden: &str) {
        let bytes = encode(msg, tag);
        assert_eq!(hex(&bytes), golden, "encode side of {msg:?}");
        assert_eq!(bytes.len() as u32, encoded_size(msg), "size prefix of {msg:?}");
        let (got_tag, got) = decode(&unhex(golden)).expect("decode side of golden bytes");
        assert_eq!(got_tag, tag);
        assert_eq!(got, *msg, "decode side of {golden}");
    }

    fn check_roundtrip(tag: u8, msg: &Wsysmsg) {
        let bytes = encode(msg, tag);
        assert_eq!(bytes.len() as u32, encoded_size(msg), "size prefix of {msg:?}");
        let (got_tag, got) = decode(&bytes).unwrap_or_else(|e| panic!("decode {msg:?}: {e}"));
        assert_eq!(got_tag, tag);
        assert_eq!(&got, msg);
    }

    fn sample_cursor() -> Cursor {
        let mut clr = [0u8; 32];
        clr[0] = 0x80;
        let mut set = [0u8; 32];
        set[31] = 0x01;
        Cursor {
            // u32::MAX wraps to -1 like the signed C Point on the wire.
            offset: Point { x: u32::MAX, y: 7 },
            clr,
            set,
            arrow: false,
        }
    }

    fn sample_cursor2() -> Cursor2 {
        let mut clr2 = [0u8; 128];
        clr2[0] = 0xaa;
        let mut set2 = [0u8; 128];
        set2[127] = 0x55;
        Cursor2 {
            offset: Point { x: 1, y: 2 },
            clr: [7; 32],
            set: [8; 32],
            offset2: Point { x: u32::MAX, y: 0 },
            clr2,
            set2,
            arrow: true,
        }
    }

    /// One sample per wire type: 33 entries, distinct type bytes.
    fn samples() -> Vec<(u8, Wsysmsg)> {
        vec![
            (1, Wsysmsg::Rerror { error: "bad draw command".into() }),
            (2, Wsysmsg::Trdmouse),
            (3, Wsysmsg::Rrdmouse { x: 100, y: 200, buttons: 4, msec: 12_345, resized: 1 }),
            (4, Wsysmsg::Tmoveto { x: 5, y: 9 }),
            (5, Wsysmsg::Rmoveto),
            (6, Wsysmsg::Tcursor { cursor: sample_cursor() }),
            (7, Wsysmsg::Rcursor),
            (8, Wsysmsg::Tbouncemouse { x: 1, y: 2, buttons: 2 }),
            (9, Wsysmsg::Rbouncemouse),
            (10, Wsysmsg::Trdkbd),
            (11, Wsysmsg::Rrdkbd { rune: 0x61 }),
            (12, Wsysmsg::Tlabel { label: "acme".into() }),
            (13, Wsysmsg::Rlabel),
            (14, Wsysmsg::Tinit { winsize: "640x480".into(), label: "col".into() }),
            (15, Wsysmsg::Rinit),
            (16, Wsysmsg::Trdsnarf),
            (17, Wsysmsg::Rrdsnarf { snarf: "clipboard".into() }),
            (18, Wsysmsg::Twrsnarf { snarf: String::new() }),
            (19, Wsysmsg::Rwrsnarf),
            (20, Wsysmsg::Trddraw { count: 65536 }),
            (21, Wsysmsg::Rrddraw { data: vec![0xab, 0xcd, 0x00] }),
            (22, Wsysmsg::Twrdraw { data: b"Jq d".to_vec() }),
            (23, Wsysmsg::Rwrdraw { count: 4 }),
            (24, Wsysmsg::Ttop),
            (25, Wsysmsg::Rtop),
            (26, Wsysmsg::Tresize {
                rect: Rect { min: Point { x: 0, y: 0 }, max: Point { x: 640, y: 480 } },
            }),
            (27, Wsysmsg::Rresize),
            (28, Wsysmsg::Tcursor2 { cursor: sample_cursor2() }),
            (29, Wsysmsg::Rcursor2),
            (30, Wsysmsg::Tctxt { id: "wsys.42".into() }),
            (31, Wsysmsg::Rctxt),
            (32, Wsysmsg::Trdkbd4),
            (33, Wsysmsg::Rrdkbd4 { rune: 0x10_FFFE }),
        ]
    }

    // ---- golden bytes (SPEC.md §4) -------------------------------------

    /// Rerror with a canonical devdraw error string (SPEC.md §4).
    #[test]
    fn golden_error_frame() {
        assert_golden(
            &Wsysmsg::Rerror { error: "bad draw command".into() },
            7,
            "0000001a070100000010626164206472617720636f6d6d616e64",
        );
    }

    /// SPEC.md §8: the protocol has no version handshake; connection setup
    /// is the attach pair `Tctxt`/`Rctxt` (server mode, first frame).
    #[test]
    fn golden_handshake_frames() {
        assert_golden(&Wsysmsg::Tctxt { id: "0".into() }, 1, "0000000b011e0000000130");
        assert_golden(&Wsysmsg::Rctxt, 1, "00000006011f");
    }

    /// Tinit sends winsize FIRST, then label (SPEC.md §2.4); the stale
    /// `font` field from the drawfcall.h comment does not exist on the wire.
    #[test]
    fn golden_tinit_field_order() {
        assert_golden(
            &Wsysmsg::Tinit { winsize: "640x480".into(), label: "acme".into() },
            1,
            "00000019010e00000007363430783438300000000461636d65",
        );
    }

    /// Data-segment frames: read request, write ack, and both
    /// `count[4] data[count]` segments (the little-endian draw commands
    /// inside `data` are decoded by [`parse_drawcmds`]).
    #[test]
    fn golden_draw_data_frames() {
        assert_golden(&Wsysmsg::Trddraw { count: 65536 }, 2, "0000000a021400010000");
        assert_golden(&Wsysmsg::Rwrdraw { count: 1 }, 3, "0000000a031700000001");
        assert_golden(&Wsysmsg::Twrdraw { data: b"J".to_vec() }, 3, "0000000b0316000000014a");
        assert_golden(&Wsysmsg::Rrddraw { data: vec![0xab, 0xcd] }, 2, "0000000c021500000002abcd");
    }

    /// Mouse event: four u32 BE fields + the resized flag byte.
    #[test]
    fn golden_mouse_frames() {
        assert_golden(&Wsysmsg::Trdmouse, 4, "000000060402");
        assert_golden(
            &Wsysmsg::Rrdmouse { x: 100, y: 200, buttons: 1, msec: 1234, resized: 0 },
            4,
            "00000017040300000064000000c800000001000004d200",
        );
        // Resize has no push message: it rides resized=1 in the next
        // Rrdmouse (SPEC.md §5), repeating the last event.
        let golden = format!("000000170403{}01", "0".repeat(32));
        assert_golden(
            &Wsysmsg::Rrdmouse { x: 0, y: 0, buttons: 0, msec: 0, resized: 1 },
            4,
            &golden,
        );
    }

    /// Keyboard: legacy 16-bit rune vs 32-bit rune (Rrdkbd vs Rrdkbd4).
    #[test]
    fn golden_keyboard_frames() {
        assert_golden(&Wsysmsg::Trdkbd, 6, "00000006060a");
        assert_golden(&Wsysmsg::Rrdkbd { rune: 0x61 }, 6, "00000008060b0061");
        assert_golden(&Wsysmsg::Trdkbd4, 7, "000000060720");
        // Rune > 0xFFFF pins the 4-byte form.
        assert_golden(&Wsysmsg::Rrdkbd4 { rune: 0x1F600 }, 6, "0000000a06210001f600");
    }

    /// Client-requested window size as a plain Rectangle (4×u32 BE).
    #[test]
    fn golden_tresize_frame() {
        assert_golden(
            &Wsysmsg::Tresize {
                rect: Rect { min: Point { x: 0, y: 0 }, max: Point { x: 640, y: 480 } },
            },
            5,
            "00000016051a000000000000000000000280000001e0",
        );
    }

    /// Minimal 6-byte frames and the empty-string encoding (n = 0).
    #[test]
    fn golden_minimal_frames() {
        assert_golden(&Wsysmsg::Rtop, 9, "000000060919");
        assert_golden(&Wsysmsg::Trdsnarf, 1, "000000060110");
        assert_golden(&Wsysmsg::Tlabel { label: String::new() }, 10, "0000000a0a0c00000000");
    }

    /// The Tcursor2 quirk: exactly 343 bytes (SPEC.md §4; research.md said
    /// 345 — wrong). 6 header + 8+32+32+8+128+128 + 1 arrow byte.
    #[test]
    fn golden_tcursor2_frame_is_343_bytes() {
        let cursor = Cursor2 {
            offset: Point { x: 0, y: 0 },
            clr: [0; 32],
            set: [0; 32],
            offset2: Point { x: 0, y: 0 },
            clr2: [0; 128],
            set2: [0; 128],
            arrow: true,
        };
        let golden = format!("00000157081c{}01", "00".repeat(336));
        assert_eq!(golden.len(), CURSOR2_FRAME * 2);
        assert_golden(&Wsysmsg::Tcursor2 { cursor }, 8, &golden);
    }

    // ---- roundtrips -----------------------------------------------------

    /// Every one of the 33 wire types encodes→decodes losslessly.
    #[test]
    fn roundtrip_all_33_types() {
        let samples = samples();
        assert_eq!(samples.len(), 33);
        let mut types: Vec<u8> = samples.iter().map(|(_, m)| m.msg_type()).collect();
        types.sort_unstable();
        types.dedup();
        assert_eq!(types.len(), 33, "type bytes must be distinct");
        assert_eq!(types[0], 1);
        assert_eq!(types[32], 33);
        for (i, (_, msg)) in samples.iter().enumerate() {
            check_roundtrip(i as u8 + 1, msg);
        }
    }

    /// Max server draw-read chunk (SPEC.md §4: ≤65536 bytes per Trddraw).
    #[test]
    fn roundtrip_64k_draw_segment() {
        let data: Vec<u8> = (0..=u16::MAX).map(|i| ((i >> 8) as u8) ^ (i as u8)).collect();
        assert_eq!(data.len(), 65536);
        let msg = Wsysmsg::Twrdraw { data };
        let bytes = encode(&msg, 255); // 255 = mux max tag
        assert_eq!(bytes.len(), 65546);
        assert_eq!(decode(&bytes), Ok((255, msg)));
    }

    // ---- strict decode ---------------------------------------------------

    #[test]
    fn rejects_short_and_size_mismatched_frames() {
        assert_eq!(decode(&[]), Err(ProtocolError::FrameTooShort(0)));
        assert_eq!(decode(&[0, 0, 0, 5, 1]), Err(ProtocolError::FrameTooShort(5)));
        assert_eq!(
            decode(&[0, 0, 0, 7, 1, RTOP]),
            Err(ProtocolError::SizeMismatch { declared: 7, actual: 6 })
        );
    }

    #[test]
    fn rejects_oversized_frames() {
        // MAXWMSG = 4 MiB; checked before the length match (SPEC.md §8, OPEN-4).
        assert_eq!(
            decode(&[0x00, 0x40, 0x00, 0x01, 1, TRDMOUSE]),
            Err(ProtocolError::FrameTooLarge(0x0040_0001))
        );
    }

    #[test]
    fn rejects_unknown_type() {
        assert_eq!(decode(&[0, 0, 0, 6, 1, 0]), Err(ProtocolError::UnknownType(0)));
        assert_eq!(decode(&[0, 0, 0, 6, 1, 34]), Err(ProtocolError::UnknownType(34)));
    }

    #[test]
    fn rejects_truncated_payload() {
        // Self-consistent 10-byte frame, but Rrdmouse needs 17 payload bytes.
        let buf = [0, 0, 0, 10, 1, RRDMOUSE, 0, 0, 0, 0];
        assert_eq!(
            decode(&buf),
            Err(ProtocolError::PayloadTooShort { ty: RRDMOUSE, needed: 4, got: 0 })
        );
    }

    #[test]
    fn rejects_trailing_payload_bytes() {
        // 6 + 8 (Tmoveto payload) + 1 stray byte, size prefix kept consistent.
        let buf = [0, 0, 0, 15, 9, TMOVETO, 0, 0, 0, 1, 0, 0, 0, 2, 0xff];
        assert_eq!(decode(&buf), Err(ProtocolError::TrailingBytes { ty: TMOVETO, extra: 1 }));
    }

    #[test]
    fn rejects_inconsistent_data_count() {
        // count = 2 but 3 data bytes follow.
        let buf = [0, 0, 0, 13, 1, TWDRAW, 0, 0, 0, 2, 1, 2, 3];
        assert_eq!(
            decode(&buf),
            Err(ProtocolError::CountMismatch { ty: TWDRAW, count: 2, remaining: 3 })
        );
    }

    #[test]
    fn rejects_non_utf8_string() {
        let buf = [0, 0, 0, 11, 1, TCTXT, 0, 0, 0, 1, 0xff];
        assert_eq!(decode(&buf), Err(ProtocolError::InvalidUtf8 { ty: TCTXT }));
    }
}
