//! Message model: wire type constants and the [`Wsysmsg`] enum (SPEC.md §4).
//!
//! One variant per drawfcall type — all 33. Field order inside each variant
//! is wire order. The RPC `tag` lives in the frame envelope, not here.
//!
//! Note (SPEC.md §8): the protocol has *no version handshake*. Connection
//! setup is `Tctxt`/`Rctxt` (server-mode attach, first frame) followed by
//! `Tinit`/`Rinit`. `Rrdmouse`/`Rrdkbd4` replies are asynchronous and may
//! arrive out of order; the client multiplexes by tag.

/// `Rerror` — error reply to any request (S→C).
pub const RERROR: u8 = 1;
/// `Trdmouse` — request next mouse event (C→S).
pub const TRDMOUSE: u8 = 2;
/// `Rrdmouse` — mouse event + resized flag (S→C).
pub const RRDMOUSE: u8 = 3;
/// `Tmoveto` — warp cursor (C→S).
pub const TMOVETO: u8 = 4;
/// `Rmoveto` — reply (S→C).
pub const RMOVETO: u8 = 5;
/// `Tcursor` — set 16×16 cursor (C→S).
pub const TCURSOR: u8 = 6;
/// `Rcursor` — reply (S→C).
pub const RCURSOR: u8 = 7;
/// `Tbouncemouse` — inject synthetic mouse event (C→S).
pub const TBOUNCEMOUSE: u8 = 8;
/// `Rbouncemouse` — reply (S→C).
pub const RBOUNCEMOUSE: u8 = 9;
/// `Trdkbd` — request next key, legacy 16-bit rune (C→S).
pub const TRDKBD: u8 = 10;
/// `Rrdkbd` — key event, 16-bit rune (S→C).
pub const RRDKBD: u8 = 11;
/// `Tlabel` — set window label (C→S).
pub const TLABEL: u8 = 12;
/// `Rlabel` — reply (S→C).
pub const RLABEL: u8 = 13;
/// `Tinit` — attach: winsize then label (C→S).
pub const TINIT: u8 = 14;
/// `Rinit` — reply (S→C).
pub const RINIT: u8 = 15;
/// `Trdsnarf` — read clipboard (C→S).
pub const TRDSNARF: u8 = 16;
/// `Rrdsnarf` — clipboard contents (S→C).
pub const RRDSNARF: u8 = 17;
/// `Twrsnarf` — write clipboard (C→S).
pub const TWRSNARF: u8 = 18;
/// `Rwrsnarf` — reply (S→C).
pub const RWWSNARF: u8 = 19;
/// `Trddraw` — read ≤count bytes of the inner draw stream (C→S).
pub const TRDDRAW: u8 = 20;
/// `Rrddraw` — data segment `count[4] data[count]` (S→C).
pub const RRDDRAW: u8 = 21;
/// `Twrdraw` — data segment of draw commands (C→S).
pub const TWDRAW: u8 = 22;
/// `Rwrdraw` — write ack, echoed count (S→C).
pub const RWWDRAW: u8 = 23;
/// `Ttop` — raise window (C→S).
pub const TTOP: u8 = 24;
/// `Rtop` — reply (S→C).
pub const RTOP: u8 = 25;
/// `Tresize` — request window size as a Rectangle (C→S).
pub const TRESIZE: u8 = 26;
/// `Rresize` — reply (S→C).
pub const RRESIZE: u8 = 27;
/// `Tcursor2` — set cursor with hi-res image, 343-byte frame (C→S).
pub const TCURSOR2: u8 = 28;
/// `Rcursor2` — reply (S→C).
pub const RCURSOR2: u8 = 29;
/// `Tctxt` — attach id, first frame in server mode (C→S).
pub const TCTXT: u8 = 30;
/// `Rctxt` — attach accepted (S→C).
pub const RCTXT: u8 = 31;
/// `Trdkbd4` — request next key, 32-bit rune (C→S).
pub const TRDKBD4: u8 = 32;
/// `Rrdkbd4` — key event, 32-bit rune (S→C).
pub const RRDKBD4: u8 = 33;

/// Highest valid wire type.
pub const MAX_TYPE: u8 = RRDKBD4;

/// Wire payload of a 16×16 `Tcursor` (frame = 6 + 73 = 79 bytes).
pub const CURSOR_PAYLOAD: usize = 73;
/// Wire payload of a `Tcursor2` (frame = 6 + 337 = 343 bytes — the SPEC.md
/// §4 quirk; research.md's 345 was wrong).
pub const CURSOR2_PAYLOAD: usize = 337;
/// Full frame size of `Tcursor2`: [`MIN_FRAME`](crate::MIN_FRAME) + [`CURSOR2_PAYLOAD`].
pub const CURSOR2_FRAME: usize = 343;

/// Point on the wire: two u32 BE halves. The C `Point` is signed; negative
/// values wrap on the wire and are preserved bit-exact as u32.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Point {
    pub x: u32,
    pub y: u32,
}

/// Rectangle on the wire: `min.x min.y max.x max.y`, 4×u32 BE (16 bytes).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Rect {
    pub min: Point,
    pub max: Point,
}

/// 16×16 cursor (`Cursor` from cursor.h): hot-spot offset plus two 16×16
/// bitmasks (2×32 bytes). `arrow` is the wire `arrow[1]` byte: any nonzero
/// value selects the system arrow cursor.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Cursor {
    pub offset: Point,
    pub clr: [u8; 32],
    pub set: [u8; 32],
    pub arrow: bool,
}

/// 32×32 cursor (`Cursor2`): the 16×16 pair plus a hi-res 32×32 pair.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Cursor2 {
    pub offset: Point,
    pub clr: [u8; 32],
    pub set: [u8; 32],
    pub offset2: Point,
    pub clr2: [u8; 128],
    pub set2: [u8; 128],
    pub arrow: bool,
}

/// One drawfcall message — all 33 wire types (SPEC.md §4).
///
/// `Rrddraw`/`Twrdraw` carry the inner little-endian draw stream as opaque
/// bytes; parsing it is out of scope for this crate (ARCHITECTURE.md,
/// future `draw.rs`). `Rrdmouse.resized` is kept as the raw u8 flag byte.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Wsysmsg {
    /// Type 1. Error reply to any request.
    Rerror {
        error: String,
    },
    /// Type 2. Empty request.
    Trdmouse,
    /// Type 3, 23-byte frame: `x y buttons msec` (u32 BE) + `resized` flag.
    Rrdmouse {
        x: u32,
        y: u32,
        buttons: u32,
        msec: u32,
        resized: u8,
    },
    /// Type 4, 14-byte frame.
    Tmoveto {
        x: u32,
        y: u32,
    },
    /// Type 5. Empty reply.
    Rmoveto,
    /// Type 6, 79-byte frame.
    Tcursor {
        cursor: Cursor,
    },
    /// Type 7. Empty reply.
    Rcursor,
    /// Type 8, 18-byte frame.
    Tbouncemouse {
        x: u32,
        y: u32,
        buttons: u32,
    },
    /// Type 9. Empty reply.
    Rbouncemouse,
    /// Type 10. Empty request (legacy).
    Trdkbd,
    /// Type 11, 8-byte frame: u16 BE rune.
    Rrdkbd {
        rune: u16,
    },
    /// Type 12, `label[s]`.
    Tlabel {
        label: String,
    },
    /// Type 13. Empty reply.
    Rlabel,
    /// Type 14. Wire order is winsize FIRST, then label (SPEC.md §2.4).
    Tinit {
        winsize: String,
        label: String,
    },
    /// Type 15. Empty reply.
    Rinit,
    /// Type 16. Empty request.
    Trdsnarf,
    /// Type 17, `snarf[s]`.
    Rrdsnarf {
        snarf: String,
    },
    /// Type 18, `snarf[s]`.
    Twrsnarf {
        snarf: String,
    },
    /// Type 19. Empty reply.
    Rwrsnarf,
    /// Type 20, 10-byte frame: read request.
    Trddraw {
        count: u32,
    },
    /// Type 21, `count[4] data[count]`; `count` == `data.len()` on encode.
    Rrddraw {
        data: Vec<u8>,
    },
    /// Type 22, `count[4] data[count]`; `count` == `data.len()` on encode.
    Twrdraw {
        data: Vec<u8>,
    },
    /// Type 23, 10-byte frame: echoed write count.
    Rwrdraw {
        count: u32,
    },
    /// Type 24. Empty request.
    Ttop,
    /// Type 25. Empty reply.
    Rtop,
    /// Type 26, 22-byte frame.
    Tresize {
        rect: Rect,
    },
    /// Type 27. Empty reply.
    Rresize,
    /// Type 28, exactly 343 bytes on the wire.
    Tcursor2 {
        cursor: Cursor2,
    },
    /// Type 29. Empty reply.
    Rcursor2,
    /// Type 30, `id[s]` (server-mode attach handshake).
    Tctxt {
        id: String,
    },
    /// Type 31. Empty reply.
    Rctxt,
    /// Type 32. Empty request.
    Trdkbd4,
    /// Type 33, 10-byte frame: u32 BE rune.
    Rrdkbd4 {
        rune: u32,
    },
}

impl Wsysmsg {
    /// Wire type byte of this message.
    pub fn msg_type(&self) -> u8 {
        match self {
            Self::Rerror { .. } => RERROR,
            Self::Trdmouse => TRDMOUSE,
            Self::Rrdmouse { .. } => RRDMOUSE,
            Self::Tmoveto { .. } => TMOVETO,
            Self::Rmoveto => RMOVETO,
            Self::Tcursor { .. } => TCURSOR,
            Self::Rcursor => RCURSOR,
            Self::Tbouncemouse { .. } => TBOUNCEMOUSE,
            Self::Rbouncemouse => RBOUNCEMOUSE,
            Self::Trdkbd => TRDKBD,
            Self::Rrdkbd { .. } => RRDKBD,
            Self::Tlabel { .. } => TLABEL,
            Self::Rlabel => RLABEL,
            Self::Tinit { .. } => TINIT,
            Self::Rinit => RINIT,
            Self::Trdsnarf => TRDSNARF,
            Self::Rrdsnarf { .. } => RRDSNARF,
            Self::Twrsnarf { .. } => TWRSNARF,
            Self::Rwrsnarf => RWWSNARF,
            Self::Trddraw { .. } => TRDDRAW,
            Self::Rrddraw { .. } => RRDDRAW,
            Self::Twrdraw { .. } => TWDRAW,
            Self::Rwrdraw { .. } => RWWDRAW,
            Self::Ttop => TTOP,
            Self::Rtop => RTOP,
            Self::Tresize { .. } => TRESIZE,
            Self::Rresize => RRESIZE,
            Self::Tcursor2 { .. } => TCURSOR2,
            Self::Rcursor2 => RCURSOR2,
            Self::Tctxt { .. } => TCTXT,
            Self::Rctxt => RCTXT,
            Self::Trdkbd4 => TRDKBD4,
            Self::Rrdkbd4 { .. } => RRDKBD4,
        }
    }
}
