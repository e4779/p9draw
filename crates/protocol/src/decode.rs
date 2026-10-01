//! Frame decoding with strict length checks (SPEC.md §2.2, §3, §4).
//!
//! [`decode`] takes one complete frame buffer (`size[4] tag[1] type[1]
//! payload`) and returns the tag plus the parsed [`Wsysmsg`]. The declared
//! size must match the buffer length exactly and every message must consume
//! its whole payload — no prefixes, no suffixes.

use crate::messages::*;
use crate::{ProtocolError, MAXWMSG, MIN_FRAME};

/// Decode one complete frame. Returns `(tag, message)`.
pub fn decode(buf: &[u8]) -> Result<(u8, Wsysmsg), ProtocolError> {
    if buf.len() < MIN_FRAME {
        return Err(ProtocolError::FrameTooShort(buf.len()));
    }
    let size = u32::from_be_bytes([buf[0], buf[1], buf[2], buf[3]]);
    if size > MAXWMSG {
        return Err(ProtocolError::FrameTooLarge(size));
    }
    if buf.len() != size as usize {
        return Err(ProtocolError::SizeMismatch {
            declared: size,
            actual: buf.len(),
        });
    }
    let tag = buf[4];
    let ty = buf[5];
    let mut r = Reader { buf, pos: MIN_FRAME };
    let msg = parse_payload(ty, &mut r)?;
    // Data segments consumed their payload by construction (count-checked);
    // everything else must end exactly here.
    r.finish(ty)?;
    Ok((tag, msg))
}

fn parse_payload(ty: u8, r: &mut Reader<'_>) -> Result<Wsysmsg, ProtocolError> {
    let msg = match ty {
        RERROR => Wsysmsg::Rerror { error: r.string(ty)? },
        TRDMOUSE => Wsysmsg::Trdmouse,
        RRDMOUSE => {
            let x = r.u32(ty)?;
            let y = r.u32(ty)?;
            let buttons = r.u32(ty)?;
            let msec = r.u32(ty)?;
            // Frame byte 22 exists (sizeW2M counts it) but devdraw never
            // writes it — stale buffer garbage no receiver reads; resized
            // rides msec byte 1 (drawfcall.c p[19], SPEC.md §4).
            let _pad = r.u8(ty)?;
            Wsysmsg::Rrdmouse {
                x,
                y,
                buttons,
                msec,
                resized: (msec >> 16) as u8,
            }
        }
        TMOVETO => Wsysmsg::Tmoveto {
            x: r.u32(ty)?,
            y: r.u32(ty)?,
        },
        RMOVETO => Wsysmsg::Rmoveto,
        TCURSOR => Wsysmsg::Tcursor { cursor: r.cursor(ty)? },
        RCURSOR => Wsysmsg::Rcursor,
        TBOUNCEMOUSE => Wsysmsg::Tbouncemouse {
            x: r.u32(ty)?,
            y: r.u32(ty)?,
            buttons: r.u32(ty)?,
        },
        RBOUNCEMOUSE => Wsysmsg::Rbouncemouse,
        TRDKBD => Wsysmsg::Trdkbd,
        RRDKBD => Wsysmsg::Rrdkbd { rune: r.u16(ty)? },
        TLABEL => Wsysmsg::Tlabel { label: r.string(ty)? },
        RLABEL => Wsysmsg::Rlabel,
        TINIT => {
            // Wire order: winsize first, then label (SPEC.md §2.4).
            let winsize = r.string(ty)?;
            let label = r.string(ty)?;
            Wsysmsg::Tinit { winsize, label }
        }
        RINIT => Wsysmsg::Rinit,
        TRDSNARF => Wsysmsg::Trdsnarf,
        RRDSNARF => Wsysmsg::Rrdsnarf { snarf: r.string(ty)? },
        TWRSNARF => Wsysmsg::Twrsnarf { snarf: r.string(ty)? },
        RWWSNARF => Wsysmsg::Rwrsnarf,
        TRDDRAW => Wsysmsg::Trddraw { count: r.u32(ty)? },
        RRDDRAW => Wsysmsg::Rrddraw { data: r.data_segment(ty)? },
        TWDRAW => Wsysmsg::Twrdraw { data: r.data_segment(ty)? },
        RWWDRAW => Wsysmsg::Rwrdraw { count: r.u32(ty)? },
        TTOP => Wsysmsg::Ttop,
        RTOP => Wsysmsg::Rtop,
        TRESIZE => Wsysmsg::Tresize { rect: r.rect(ty)? },
        RRESIZE => Wsysmsg::Rresize,
        TCURSOR2 => Wsysmsg::Tcursor2 { cursor: r.cursor2(ty)? },
        RCURSOR2 => Wsysmsg::Rcursor2,
        TCTXT => Wsysmsg::Tctxt { id: r.string(ty)? },
        RCTXT => Wsysmsg::Rctxt,
        TRDKBD4 => Wsysmsg::Trdkbd4,
        RRDKBD4 => Wsysmsg::Rrdkbd4 { rune: r.u32(ty)? },
        _ => return Err(ProtocolError::UnknownType(ty)),
    };
    Ok(msg)
}

struct Reader<'a> {
    buf: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    fn remaining(&self) -> usize {
        self.buf.len() - self.pos
    }

    /// Borrow `n` bytes and advance. The returned slice borrows the buffer
    /// for `'a`, not `&mut self`, so callers can copy out of it freely.
    fn take(&mut self, n: usize, ty: u8) -> Result<&'a [u8], ProtocolError> {
        if self.remaining() < n {
            return Err(ProtocolError::PayloadTooShort {
                ty,
                needed: n,
                got: self.remaining(),
            });
        }
        let out = &self.buf[self.pos..self.pos + n];
        self.pos += n;
        Ok(out)
    }

    fn u8(&mut self, ty: u8) -> Result<u8, ProtocolError> {
        Ok(self.take(1, ty)?[0])
    }

    fn u16(&mut self, ty: u8) -> Result<u16, ProtocolError> {
        let b = self.take(2, ty)?;
        Ok(u16::from_be_bytes([b[0], b[1]]))
    }

    fn u32(&mut self, ty: u8) -> Result<u32, ProtocolError> {
        let b = self.take(4, ty)?;
        Ok(u32::from_be_bytes([b[0], b[1], b[2], b[3]]))
    }

    /// `n[4 BE] + n bytes`, no NUL (SPEC.md §3).
    fn string(&mut self, ty: u8) -> Result<String, ProtocolError> {
        let n = self.u32(ty)? as usize;
        let bytes = self.take(n, ty)?;
        match std::str::from_utf8(bytes) {
            Ok(s) => Ok(s.to_owned()),
            Err(_) => Err(ProtocolError::InvalidUtf8 { ty }),
        }
    }

    /// `count[4] data[count]` — the count must match the rest of the frame
    /// exactly (SPEC.md §4: Rrddraw/Twrdraw are `10 + n` bytes total).
    fn data_segment(&mut self, ty: u8) -> Result<Vec<u8>, ProtocolError> {
        let count = self.u32(ty)? as usize;
        if count != self.remaining() {
            return Err(ProtocolError::CountMismatch {
                ty,
                count: count as u32,
                remaining: self.remaining(),
            });
        }
        Ok(self.take(count, ty)?.to_vec())
    }

    fn point(&mut self, ty: u8) -> Result<Point, ProtocolError> {
        let x = self.u32(ty)?;
        let y = self.u32(ty)?;
        Ok(Point { x, y })
    }

    fn rect(&mut self, ty: u8) -> Result<Rect, ProtocolError> {
        let min = self.point(ty)?;
        let max = self.point(ty)?;
        Ok(Rect { min, max })
    }

    fn cursor(&mut self, ty: u8) -> Result<Cursor, ProtocolError> {
        let offset = self.point(ty)?;
        let mut clr = [0u8; 32];
        clr.copy_from_slice(self.take(32, ty)?);
        let mut set = [0u8; 32];
        set.copy_from_slice(self.take(32, ty)?);
        let arrow = self.u8(ty)? != 0;
        Ok(Cursor {
            offset,
            clr,
            set,
            arrow,
        })
    }

    fn cursor2(&mut self, ty: u8) -> Result<Cursor2, ProtocolError> {
        let offset = self.point(ty)?;
        let mut clr = [0u8; 32];
        clr.copy_from_slice(self.take(32, ty)?);
        let mut set = [0u8; 32];
        set.copy_from_slice(self.take(32, ty)?);
        let offset2 = self.point(ty)?;
        let mut clr2 = [0u8; 128];
        clr2.copy_from_slice(self.take(128, ty)?);
        let mut set2 = [0u8; 128];
        set2.copy_from_slice(self.take(128, ty)?);
        let arrow = self.u8(ty)? != 0;
        Ok(Cursor2 {
            offset,
            clr,
            set,
            offset2,
            clr2,
            set2,
            arrow,
        })
    }

    fn finish(&self, ty: u8) -> Result<(), ProtocolError> {
        if self.remaining() != 0 {
            return Err(ProtocolError::TrailingBytes {
                ty,
                extra: self.remaining(),
            });
        }
        Ok(())
    }
}
