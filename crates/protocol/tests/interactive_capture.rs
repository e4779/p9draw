//! Interactive capture (fixtures/live-acme-interactive, 2026-10-01): a
//! HUMAN drove acme with mouse and keyboard through the p9draw MITM for
//! about a minute — unlike fixtures/live-acme (init-only, 45 ms) this
//! stream exercises the event path: 456 Rrdmouse, typing "Hello GLM!" via
//! Trdkbd4/Rrdkbd4, and 631 Twrdraw frames (64.5 KB of real draw
//! commands). Wire findings: docs/fixtures-analysis.md §6.
//!
//! The session was ALIVE at capture time, so the last .bin frame may be
//! truncated: `walk` stops at the first frame whose declared size
//! overruns the buffer, and capture.log counts are compared with a ±1
//! slack. Rrdmouse layout quirk (SPEC.md §4): `resized` rides byte 1 of
//! the msec group (frame offset 19); frame byte 22 is stale garbage no
//! receiver reads.

use p9draw_protocol::{decode, encode, parse_drawcmds, Wsysmsg};

fn fixture(name: &str) -> Vec<u8> {
    std::fs::read(format!(
        "{}/../../fixtures/live-acme-interactive/{name}",
        env!("CARGO_MANIFEST_DIR")
    ))
    .unwrap()
}

fn log_count(dir: &str, name: &str) -> u32 {
    let log = std::fs::read_to_string(format!(
        "{}/../../fixtures/live-acme-interactive/capture.log",
        env!("CARGO_MANIFEST_DIR")
    ))
    .unwrap();
    log.lines()
        .filter(|l| {
            let mut it = l.split_whitespace();
            let _ts = it.next();
            let d = it.next();
            let _tag = it.next();
            let n = it.next();
            d == Some(dir) && n == Some(name)
        })
        .count() as u32
}

/// Walk complete frames; returns `(frames, tail_bytes)`. A nonzero tail is
/// the head of an unterminated final frame — normal for a live capture.
fn walk(buf: &[u8]) -> (Vec<(u8, Wsysmsg)>, usize) {
    let mut out = Vec::new();
    let mut off = 0;
    while off + 4 <= buf.len() {
        let size = u32::from_be_bytes([buf[off], buf[off + 1], buf[off + 2], buf[off + 3]]) as usize;
        if size < 6 || off + size > buf.len() {
            break;
        }
        let (tag, msg) = decode(&buf[off..off + size]).expect("complete frame must decode");
        out.push((tag, msg));
        off += size;
    }
    (out, buf.len() - off)
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

#[test]
fn interactive_frame_counts_match_capture_log() {
    let (c2s, tail_c) = walk(&fixture("c2s.bin"));
    let (s2c, tail_s) = walk(&fixture("s2c.bin"));
    // A live tail, if any, is one unterminated frame head, not garbage.
    assert!(tail_c < 4096 && tail_s < 4096, "tail {tail_c}/{tail_s}");
    let raw = |pat: fn(&Wsysmsg) -> bool, v: &[(u8, Wsysmsg)]| {
        v.iter().filter(|(_, m)| pat(m)).count() as i64
    };
    let check = |dir: &str, name: &str, n: i64| {
        let log = log_count(dir, name) as i64;
        assert!((n - log).abs() <= 1, "{dir} {name}: raw {n} vs log {log}");
    };
    check("c2s", "Trdmouse", raw(|m| matches!(m, Wsysmsg::Trdmouse), &c2s));
    check("c2s", "Twrdraw", raw(|m| matches!(m, Wsysmsg::Twrdraw { .. }), &c2s));
    check("c2s", "Trdkbd4", raw(|m| matches!(m, Wsysmsg::Trdkbd4), &c2s));
    check("s2c", "Rrdmouse", raw(|m| matches!(m, Wsysmsg::Rrdmouse { .. }), &s2c));
    check("s2c", "Rrdkbd4", raw(|m| matches!(m, Wsysmsg::Rrdkbd4 { .. }), &s2c));
    check("s2c", "Rwrdraw", raw(|m| matches!(m, Wsysmsg::Rwrdraw { .. }), &s2c));
    // Exact raw counts of the 2026-10-01 capture (both streams end clean).
    assert_eq!(c2s.len(), 1128);
    assert_eq!(s2c.len(), 1126);
}

/// The first live Rrdmouse, byte-exact: 23 B, tag 2, x=310 y=22 buttons=0
/// msec=0x5000b2b8. Byte 22 (0x20) is stale devdraw buffer garbage; the
/// real resized flag is msec byte 1 (0x00).
#[test]
fn rrdmouse_wire_layout() {
    let raw = fixture("s2c.bin");
    let pos = raw
        .windows(23)
        .position(|w| w[5] == 3 && u32::from_be_bytes([w[0], w[1], w[2], w[3]]) == 23)
        .unwrap();
    assert_eq!(
        hex(&raw[pos..pos + 23]),
        "0000001702030000013600000016000000005000b2b820"
    );
    let (tag, msg) = decode(&raw[pos..pos + 23]).unwrap();
    assert_eq!(tag, 2);
    assert_eq!(
        msg,
        Wsysmsg::Rrdmouse { x: 310, y: 22, buttons: 0, msec: 1_342_223_032, resized: 0 }
    );
    // Re-encode is byte-identical except the void byte 22 (we pad 0).
    let mut re = encode(&msg, tag);
    assert_eq!(re.len(), 23);
    assert_eq!(&re[..22], &raw[pos..pos + 22]);
    re[22] = 0x20;
    assert_eq!(re, raw[pos..pos + 23]);
}

/// All 456 live mouse events: resized == 0 everywhere (no resize happened;
/// the old decoder surfaced the stale 0x20 byte as resized=32). msec =
/// uptime ms with exactly one 64K-boundary dip — the msec/resized overlap.
#[test]
fn rrdmouse_msec_is_uptime_ms() {
    let buf = fixture("s2c.bin");
    let (s2c, _) = walk(&buf);
    let ms: Vec<u32> = s2c
        .iter()
        .filter_map(|(_, m)| match m {
            Wsysmsg::Rrdmouse { msec, .. } => Some(*msec),
            _ => None,
        })
        .collect();
    let resized_all_zero = s2c.iter().all(|(_, m)| match m {
        Wsysmsg::Rrdmouse { resized, .. } => *resized == 0,
        _ => true,
    });
    assert!(resized_all_zero);
    assert_eq!(ms.len(), 456);
    assert_eq!(ms[0], 1_342_223_032, "≈ 15.53 days = host uptime at capture");
    let deltas: Vec<i64> = ms.windows(2).map(|w| w[1] as i64 - w[0] as i64).collect();
    let neg: Vec<i64> = deltas.iter().copied().filter(|d| *d < 0).collect();
    assert_eq!(neg, vec![-65522], "single 64K boundary: 0x5000fffe → 0x5000000c");
    let pos: Vec<i64> = deltas.iter().copied().filter(|d| *d > 0).collect();
    assert_eq!(pos.iter().min(), Some(&1));
    assert_eq!(pos.iter().max(), Some(&7708), "longest human pause between events");
    // Frame offset 19 carries the resized flag and is 0 in every frame.
    let mut off = 0;
    while off + 4 <= buf.len() {
        let size = u32::from_be_bytes([buf[off], buf[off + 1], buf[off + 2], buf[off + 3]]) as usize;
        if size < 6 || off + size > buf.len() {
            break;
        }
        if buf[off + 5] == 3 {
            assert_eq!(buf[off + 19], 0, "frame[19] (resized) at {off}");
        }
        off += size;
    }
}

/// Typed input: one PUA special rune (plan9port KF range, 0xF014) then
/// "Hello GLM!". 12 Trdkbd4 requests → 11 replies (one still parked in
/// the live session's queue). No legacy Trdkbd/Rrdkbd frames at all.
#[test]
fn rrdkbd4_hello_glm() {
    let (s2c, _) = walk(&fixture("s2c.bin"));
    let (c2s, _) = walk(&fixture("c2s.bin"));
    let runes: Vec<u32> = s2c
        .iter()
        .filter_map(|(_, m)| match m {
            Wsysmsg::Rrdkbd4 { rune } => Some(*rune),
            _ => None,
        })
        .collect();
    let want: Vec<u32> = [0xF014_u32]
        .into_iter()
        .chain("Hello GLM!".chars().map(|c| c as u32))
        .collect();
    assert_eq!(runes, want);
    let reqs = c2s
        .iter()
        .filter(|(_, m)| matches!(m, Wsysmsg::Trdkbd4))
        .count();
    assert_eq!(reqs, 12);
    let replies = (runes.len() as i64 - log_count("s2c", "Rrdkbd4") as i64).abs();
    assert!(replies <= 1, "kbd replies vs log: {replies}");
    assert!(!c2s.iter().any(|(_, m)| matches!(m, Wsysmsg::Trdkbd)));
}

/// MAIN VALUE: hundreds of real draw commands. Every Twrdraw payload of
/// the interactive capture must parse with zero errors — no unknown ops
/// in 64 452 bytes of acme output (first-ops per frame: v 461, d 48,
/// b 46, l 62, f 6, i 4, A/J/q/Y 1 each; 'I' appears inside frames).
#[test]
fn parse_drawcmds_over_all_interactive_twrdraw() {
    let (c2s, _) = walk(&fixture("c2s.bin"));
    let mut frames = 0usize;
    let mut total = 0usize;
    let mut errs: Vec<String> = Vec::new();
    for (_, m) in &c2s {
        if let Wsysmsg::Twrdraw { data } = m {
            frames += 1;
            match parse_drawcmds(data) {
                Ok(cmds) => total += cmds.len(),
                Err(e) => errs.push(format!("frame {frames}: {e}")),
            }
        }
    }
    assert!(errs.is_empty(), "{} failures:\n{}", errs.len(), errs.join("\n"));
    let log = log_count("c2s", "Twrdraw") as i64;
    assert!((frames as i64 - log).abs() <= 1, "Twrdraw raw {frames} vs log {log}");
    assert!(total >= frames, "every Twrdraw carries >= 1 command (got {total})");
}
