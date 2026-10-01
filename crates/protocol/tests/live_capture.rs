//! Golden regression: the REAL plan9port-acme session captured through the
//! p9draw MITM on 2026-10-01 (fixtures/live-acme, analysis in
//! docs/fixtures-analysis.md).
//!
//! The whole c2s/s2c byte streams must decode frame-by-frame with the crate
//! decoder; message counts and per-frame (type, size) sequences must match
//! capture.log, which the capture itself wrote during the session; and our
//! encoder must reproduce every real frame byte-for-byte.

use p9draw_protocol::{decode, encode, encoded_size, Wsysmsg};

const C2S: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../fixtures/live-acme/c2s.bin"
));
const S2C: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../fixtures/live-acme/s2c.bin"
));
const LOG: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../fixtures/live-acme/capture.log"
));

/// Wire name of each variant (spelling matches capture.log / SPEC.md §4).
fn type_name(msg: &Wsysmsg) -> &'static str {
    use Wsysmsg::*;
    match msg {
        Rerror { .. } => "Rerror",
        Trdmouse => "Trdmouse",
        Rrdmouse { .. } => "Rrdmouse",
        Tmoveto { .. } => "Tmoveto",
        Rmoveto => "Rmoveto",
        Tcursor { .. } => "Tcursor",
        Rcursor => "Rcursor",
        Tbouncemouse { .. } => "Tbouncemouse",
        Rbouncemouse => "Rbouncemouse",
        Trdkbd => "Trdkbd",
        Rrdkbd { .. } => "Rrdkbd",
        Tlabel { .. } => "Tlabel",
        Rlabel => "Rlabel",
        Tinit { .. } => "Tinit",
        Rinit => "Rinit",
        Trdsnarf => "Trdsnarf",
        Rrdsnarf { .. } => "Rrdsnarf",
        Twrsnarf { .. } => "Twrsnarf",
        Rwrsnarf => "Rwrsnarf",
        Trddraw { .. } => "Trddraw",
        Rrddraw { .. } => "Rrddraw",
        Twrdraw { .. } => "Twrdraw",
        Rwrdraw { .. } => "Rwrdraw",
        Ttop => "Ttop",
        Rtop => "Rtop",
        Tresize { .. } => "Tresize",
        Rresize => "Rresize",
        Tcursor2 { .. } => "Tcursor2",
        Rcursor2 => "Rcursor2",
        Tctxt { .. } => "Tctxt",
        Rctxt => "Rctxt",
        Trdkbd4 => "Trdkbd4",
        Rrdkbd4 { .. } => "Rrdkbd4",
    }
}

/// Split a capture into `size[4 BE] tag[1] type[1] payload` frames and decode
/// each with the crate decoder. Golden round-trip: `encode` must reproduce
/// the real bytes exactly (SPEC §3/§4 promise 1:1 with convW2M/convM2W).
fn decode_stream(bytes: &[u8]) -> Vec<(u8, Wsysmsg)> {
    let mut off = 0;
    let mut out = Vec::new();
    while off < bytes.len() {
        assert!(off + 6 <= bytes.len(), "truncated header at offset {off}");
        let size =
            u32::from_be_bytes([bytes[off], bytes[off + 1], bytes[off + 2], bytes[off + 3]])
                as usize;
        assert!(size >= 6 && off + size <= bytes.len(), "bad size {size} at offset {off}");
        let frame = &bytes[off..off + size];
        let (tag, msg) =
            decode(frame).unwrap_or_else(|e| panic!("frame at offset {off} ({size}B): {e}"));
        assert_eq!(msg.msg_type(), frame[5], "type byte vs decoded variant at {off}");
        assert_eq!(encode(&msg, tag), frame, "round-trip mismatch at offset {off}");
        off += size;
        out.push((tag, msg));
    }
    assert_eq!(off, bytes.len(), "stream not fully consumed");
    out
}

/// capture.log line: `<ms> <c2s|s2c> tag=<n> <Type> ... (<N>B)` — return
/// (type, frame-size) pairs for one direction, preserving file order.
fn log_entries(dir: &str) -> Vec<(String, u32)> {
    LOG.lines()
        .filter_map(|line| {
            let f: Vec<&str> = line.split_whitespace().collect();
            if f.len() < 5 || f[1] != dir {
                return None;
            }
            let ty = f[3].to_string();
            let sz = f.last().unwrap();
            let size: u32 = sz[1..sz.len() - 2].parse().unwrap();
            Some((ty, size))
        })
        .collect()
}

/// (type, encoded-size) digest of a decoded stream — comparable to log entries.
fn digest(stream: &[(u8, Wsysmsg)]) -> Vec<(String, u32)> {
    stream
        .iter()
        .map(|(_, m)| (type_name(m).to_string(), encoded_size(m)))
        .collect()
}

#[test]
fn c2s_matches_capture_log() {
    let stream = decode_stream(C2S);
    assert_eq!(stream.len(), 35);

    let log = log_entries("c2s");
    assert_eq!(log.len(), 35);
    assert_eq!(digest(&stream), log, "c2s (type,size) sequence vs capture.log");

    // Legacy pipe mode: first frame is Tinit (no Tctxt — nobody sets $wsysid).
    assert!(matches!(
        &stream[0],
        (1, Wsysmsg::Tinit { winsize, label }) if winsize.is_empty() && label == "acme"
    ));
    assert!(matches!(
        &stream[1],
        (1, Wsysmsg::Twrdraw { data }) if data.as_slice() == b"JI"
    ));
    // All RPCs share tag 1 (freed and reused after each reply); the two
    // concurrent async event reads get tags 1 and 2 at the tail.
    for (i, (tag, _)) in stream.iter().enumerate() {
        let expected = if i + 1 == stream.len() { 2 } else { 1 };
        assert_eq!(*tag, expected, "tag of c2s frame #{i}");
    }
    assert!(matches!(stream[33], (1, Wsysmsg::Trdkbd4)));
    assert!(matches!(stream[34], (2, Wsysmsg::Trdmouse)));
    assert!(!stream.iter().any(|(_, m)| matches!(m, Wsysmsg::Rerror { .. })));
}

#[test]
fn s2c_matches_capture_log() {
    let stream = decode_stream(S2C);
    assert_eq!(stream.len(), 33);
    let log = log_entries("s2c");
    assert_eq!(log.len(), 33);
    assert_eq!(digest(&stream), log, "s2c (type,size) sequence vs capture.log");

    assert!(matches!(stream[0], (1, Wsysmsg::Rinit)));
    assert!(matches!(stream[32], (1, Wsysmsg::Rwrdraw { .. })));
    assert!(!stream.iter().any(|(_, m)| matches!(m, Wsysmsg::Rerror { .. })));
}

#[test]
fn every_request_was_answered_before_eof() {
    let c2s = decode_stream(C2S);
    let s2c = decode_stream(S2C);

    // Twrdraw <-> Rwrdraw, FIFO per tag: reply count echoes the write count.
    // Includes the FINAL Twrdraw — its ack is the last s2c frame, so the wire
    // shows no unanswered draw RPC when the session ended.
    let writes: Vec<u32> = c2s
        .iter()
        .filter_map(|(_, m)| match m {
            Wsysmsg::Twrdraw { data } => Some(data.len() as u32),
            _ => None,
        })
        .collect();
    let acks: Vec<u32> = s2c
        .iter()
        .filter_map(|(_, m)| match m {
            Wsysmsg::Rwrdraw { count } => Some(*count),
            _ => None,
        })
        .collect();
    assert_eq!(writes.len(), 30);
    assert_eq!(acks.len(), 30);
    assert_eq!(writes, acks);

    // Trddraw <-> Rrddraw: the server may return less than requested (§4).
    let reads: Vec<u32> = c2s
        .iter()
        .filter_map(|(_, m)| match m {
            Wsysmsg::Trddraw { count } => Some(*count),
            _ => None,
        })
        .collect();
    assert_eq!(reads, vec![145, 12]);
    let datas: Vec<&[u8]> = s2c
        .iter()
        .filter_map(|(_, m)| match m {
            Wsysmsg::Rrddraw { data } => Some(data.as_slice()),
            _ => None,
        })
        .collect();
    assert_eq!(datas.len(), 2);
    // 'I' image info: 12 fields of "%11d " = 144 bytes ASCII.
    assert_eq!(datas[0].len(), 144);
    assert!(String::from_utf8_lossy(datas[0]).contains("x8r8g8b8"));
    // 'q' query dpi reply.
    assert_eq!(datas[1].len(), 12);
    assert_eq!(String::from_utf8_lossy(datas[1]).trim(), "192");
}

#[test]
fn wire_encoding_matches_spec() {
    let c2s = decode_stream(C2S);

    // allocscreen 'A' (SPEC §6): little-endian ids, fillid = image 1.
    assert!(c2s.iter().any(|(_, m)| matches!(
        m,
        Wsysmsg::Twrdraw { data }
            if data.len() == 14
                && data[0] == b'A'
                && data[1..5] == [1, 0, 0, 0]  // id, LE
                && data[5..9] == [0, 0, 0, 0]  // imageid = screen image
                && data[9..13] == [1, 0, 0, 0] // fillid = image 1
                && data[13] == 0               // public = 0
    )));

    // Window allocimage 'b': chan is a LE u32 descriptor, one channel byte =
    // (char_code << 4) | nbits. 0x68081828 == "x8r8g8b8" — exactly the chan
    // string the 'I' info reply reports for the screen. GREY1 == 0x31.
    let window = c2s
        .iter()
        .filter_map(|(_, m)| match m {
            Wsysmsg::Twrdraw { data }
                if data.len() == 51 && data[0] == b'b' && data[5] == 1 =>
            {
                Some(data) // screenid u16 @5 == 1 → window image
            }
            _ => None,
        })
        .next()
        .expect("window allocimage with screenid=1");
    assert_eq!(&window[10..14], &[0x28, 0x18, 0x08, 0x68]); // LE 0x68081828
    assert_eq!(window[14], 0); // windows: repl = 0 (SPEC §6)
    let mut rect = Vec::new();
    for v in [0u32, 0, 1939, 1293] {
        rect.extend_from_slice(&v.to_le_bytes());
    }
    assert_eq!(&window[15..31], &rect[..]); // R == screen 1939x1293, LE
    assert_eq!(&window[31..47], &rect[..]); // clipR == R

    // GREY1 tile (first alloc): chan 0x00000031 on the wire.
    let grey1 = c2s
        .iter()
        .filter_map(|(_, m)| match m {
            Wsysmsg::Twrdraw { data } if data.len() == 51 && data[0] == b'b' => Some(data),
            _ => None,
        })
        .next()
        .unwrap();
    assert_eq!(&grey1[10..14], &[0x31, 0x00, 0x00, 0x00]);
    assert_eq!(grey1[14], 1); // repl = 1 (1x1 tile)
}
