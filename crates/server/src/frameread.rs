//! Incremental drawfcall frame reassembly from a raw byte stream
//! (SPEC.md §2.2): `size[4 BE] tag[1] type[1] payload`.
//!
//! `decode` (p9draw-protocol) needs one complete frame; this module splits
//! a byte stream into frames without blocking, so the capture pumps can
//! decode every message while forwarding raw bytes.

use p9draw_protocol::{MAXWMSG, MIN_FRAME};

/// Framing went wrong: the declared size is impossible, so byte offsets in
/// this direction are no longer frame-aligned. Callers should keep
/// forwarding raw bytes but stop trying to decode (capture semantics).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BadFrameSize {
    pub declared: u32,
}

impl std::fmt::Display for BadFrameSize {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "declared frame size {} is not a valid drawfcall size",
            self.declared
        )
    }
}

/// Assembles complete frames out of arbitrary byte chunks.
#[derive(Debug, Default)]
pub struct FrameAssembler {
    buf: Vec<u8>,
}

impl FrameAssembler {
    pub fn new() -> Self {
        Self { buf: Vec::new() }
    }

    /// Feed a raw chunk; every complete frame is appended to `out`.
    /// Frames are extracted by the `size[4]` prefix alone — payload
    /// validation is `p9draw_protocol::decode`'s job.
    pub fn feed(&mut self, chunk: &[u8], out: &mut Vec<Vec<u8>>) -> Result<(), BadFrameSize> {
        self.buf.extend_from_slice(chunk);
        loop {
            if self.buf.len() < 4 {
                return Ok(());
            }
            let declared = u32::from_be_bytes([self.buf[0], self.buf[1], self.buf[2], self.buf[3]]);
            if declared < MIN_FRAME as u32 || declared > MAXWMSG {
                return Err(BadFrameSize { declared });
            }
            if self.buf.len() < declared as usize {
                return Ok(()); // wait for the rest of the frame
            }
            out.push(self.buf.drain(..declared as usize).collect());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use p9draw_protocol::{decode, encode, Wsysmsg};

    fn feed_all(a: &mut FrameAssembler, chunks: &[&[u8]]) -> Vec<Vec<u8>> {
        let mut out = Vec::new();
        for c in chunks {
            a.feed(c, &mut out).unwrap();
        }
        out
    }

    #[test]
    fn extracts_two_frames_from_one_chunk() {
        let f1 = encode(&Wsysmsg::Tctxt { id: "0".into() }, 1);
        let f2 = encode(&Wsysmsg::Rctxt, 1);
        let mut all = f1.clone();
        all.extend_from_slice(&f2);
        let mut a = FrameAssembler::new();
        let frames = feed_all(&mut a, &[&all]);
        assert_eq!(frames, vec![f1, f2]);
    }

    #[test]
    fn reassembles_frame_split_across_chunks() {
        let f1 = encode(
            &Wsysmsg::Tinit { winsize: "640x480".into(), label: "acme".into() },
            7,
        );
        let mut a = FrameAssembler::new();
        let mut out = Vec::new();
        for (i, b) in f1.iter().enumerate() {
            a.feed(std::slice::from_ref(b), &mut out).unwrap();
            let expected = if i + 1 == f1.len() { 1 } else { 0 };
            assert_eq!(out.len(), expected, "after byte {i}");
        }
        assert_eq!(out[0], f1);
    }

    #[test]
    fn rejects_impossible_declared_size() {
        let mut a = FrameAssembler::new();
        let mut out = Vec::new();
        // Below MIN_FRAME.
        assert_eq!(
            a.feed(&[0, 0, 0, 5, 1, 2, 3], &mut out),
            Err(BadFrameSize { declared: 5 })
        );
        // Above MAXWMSG.
        let mut a2 = FrameAssembler::new();
        let big = (MAXWMSG + 1).to_be_bytes();
        assert_eq!(
            a2.feed(&big, &mut out),
            Err(BadFrameSize { declared: MAXWMSG + 1 })
        );
        assert!(out.is_empty());
    }

    #[test]
    fn assembler_output_decodes_via_protocol() {
        let f = encode(
            &Wsysmsg::Rrdmouse { x: 1, y: 2, buttons: 4, msec: 5, resized: 0 },
            9,
        );
        let mut a = FrameAssembler::new();
        let frames = feed_all(&mut a, &[&f]);
        assert_eq!(frames.len(), 1);
        let (tag, msg) = decode(&frames[0]).unwrap();
        assert_eq!(tag, 9);
        assert_eq!(
            msg,
            Wsysmsg::Rrdmouse { x: 1, y: 2, buttons: 4, msec: 5, resized: 0 }
        );
    }
}
