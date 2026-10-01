//! Live-capture regression for the inner draw-command stream (SPEC.md §6):
//! every Twrdraw payload of the REAL plan9port-acme session captured on
//! 2026-10-01 (fixtures/live-acme/c2s.bin) must parse into DrawCmds without
//! errors, and the opening commands must match the bytes capture.log
//! recorded during the session (analysis: docs/fixtures-analysis.md §2–3).

use p9draw_protocol::{decode, parse_drawcmds, DrawCmd, Point, Rect, Wsysmsg};

const C2S: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../fixtures/live-acme/c2s.bin"
));

fn pt(x: i32, y: i32) -> Point {
    Point { x: x as u32, y: y as u32 }
}

/// The screen of the capture: x8r8g8b8, 1939×1293 (fixtures-analysis §2/§3).
fn screen_rect() -> Rect {
    Rect { min: pt(0, 0), max: pt(1939, 1293) }
}

/// All Twrdraw data segments of the c2s stream, in wire order.
fn twrdraw_payloads() -> Vec<Vec<u8>> {
    let mut off = 0;
    let mut out = Vec::new();
    while off < C2S.len() {
        assert!(off + 6 <= C2S.len(), "truncated header at {off}");
        let size =
            u32::from_be_bytes([C2S[off], C2S[off + 1], C2S[off + 2], C2S[off + 3]]) as usize;
        let frame = &C2S[off..off + size];
        let (tag, msg) = decode(frame).expect("frame decodes");
        if let Wsysmsg::Twrdraw { data } = msg {
            // All draw RPCs of this capture share tag 1 (live_capture.rs).
            assert_eq!(tag, 1);
            out.push(data);
        }
        off += size;
    }
    assert_eq!(off, C2S.len(), "stream fully consumed");
    out
}

fn parse_all() -> Vec<Vec<DrawCmd>> {
    twrdraw_payloads()
        .iter()
        .map(|p| parse_drawcmds(p).expect("Twrdraw payload parses"))
        .collect()
}

#[test]
fn every_twrdraw_payload_parses_without_errors() {
    let payloads = twrdraw_payloads();
    // 30 writes ↔ 30 Rwrdraw acks (see live_capture.rs / fixtures-analysis §1).
    assert_eq!(payloads.len(), 30);
    assert!(payloads.iter().all(|p| !p.is_empty()));

    let cmds = parse_all();
    let mut total = 0;
    for c in &cmds {
        assert!(!c.is_empty(), "every payload holds at least one command");
        total += c.len();
    }
    assert!(total > 0);
    // 30 non-empty payloads alone give 30; the two 225 B paint batches
    // each hold several packed commands (a lone 'd' is 45 B).
    assert!(total >= 30);

    // Payload lengths pinned by capture.log frame sizes (size − 10):
    // JI | q d | b b A b | d+v | ... | 225-byte paint batches | b id=0x12.
    let lens: Vec<usize> = payloads.iter().map(|p| p.len()).collect();
    assert_eq!(lens[0], 2); // "JI"
    assert_eq!(lens[1], 3); // "q\x01d"
    assert_eq!(lens[2..=6], [51, 51, 14, 51, 46]); // b id1, b id2, A, b id3, d+v
    assert_eq!(lens[10], 45); // solo composite
    assert_eq!(lens[11], 5); // free id 5
    assert_eq!(lens[16], 45); // solo composite
    assert_eq!(lens[17], 5); // free id 9
    assert_eq!(lens[21], 225); // paint batch #1
    assert_eq!(lens[23], 225); // paint batch #2
    assert_eq!(lens[25], 45); // solo composite
    assert_eq!(lens[26], 5); // free id 0xf
    assert_eq!(lens[29], 51); // final b id=0x12
}

#[test]
fn opening_commands_match_capture_log() {
    let cmds = parse_all();

    // Twrdraw #1 "JI": image 0 := screen, then read its info — the reply
    // is the 144-byte "%11d " ASCII blob the next Trddraw picks up.
    assert_eq!(cmds[0], vec![DrawCmd::Image0Screen, DrawCmd::ReadInfo]);

    // Twrdraw #2 "q\x01d": exactly one dpi query.
    assert_eq!(cmds[1], vec![DrawCmd::Query { specs: vec![b'd'] }]);

    // Twrdraw #3: first allocimage — 1×1 GREY1 repl tile, value ffffffff
    // (fixtures-analysis §2/§3: chan 31 00 00 00, repl=1).
    assert!(matches!(
        &cmds[2][0],
        DrawCmd::Allocate {
            id: 1,
            screen_id: 0,
            chan: 0x31,
            repl: 1,
            value: 0xffff_ffff,
            ..
        }
    ));

    // Twrdraw #5: allocscreen id=1 on image 0 with fill image 1 — all 14
    // bytes visible in capture.log (41 01000000 00000000 01000000 00).
    assert_eq!(
        cmds[4],
        vec![DrawCmd::AllocScreen { id: 1, image_id: 0, fill_id: 1, public: 0 }]
    );

    // Twrdraw #6: the window image — screenid u16 @5 == 1, chan
    // 0x68081828 == "x8r8g8b8" (matches the 'I' reply string), repl 0
    // (window rule), R == clipR == full screen.
    assert!(matches!(
        &cmds[5][0],
        DrawCmd::Allocate {
            id: 3,
            screen_id: 1,
            chan: 0x6808_1828,
            repl: 0,
            r,
            clip_r,
            ..
        } if *r == screen_rect() && *clip_r == screen_rect()
    ));

    // Twrdraw #7: count=46 = 45+1 — one composite onto the window, one flush.
    assert_eq!(cmds[6].len(), 2);
    assert!(matches!(
        &cmds[6][0],
        DrawCmd::Draw { dst_id: 3, src_id: 1, mask_id: 1, .. }
    ));
    assert_eq!(cmds[6][1], DrawCmd::Flush);

    // Twrdraw #8: 17×16 masks come as GREY8 (chan 0x38) repl tiles.
    assert!(matches!(
        &cmds[7][0],
        DrawCmd::Allocate { id: 4, chan: 0x38, repl: 1, .. }
    ));
}

#[test]
fn composites_frees_and_batches_match_capture_log() {
    let cmds = parse_all();

    // Solo composites: (dst, src, mask) straight from capture.log payload
    // heads (64 06000000 05000000 04000000 / 640a… / 640e0000000f…).
    assert_eq!(cmds[10].len(), 1);
    assert!(matches!(
        &cmds[10][0],
        DrawCmd::Draw { dst_id: 0x6, src_id: 0x5, mask_id: 0x4, .. }
    ));
    assert_eq!(cmds[16].len(), 1);
    assert!(matches!(
        &cmds[16][0],
        DrawCmd::Draw { dst_id: 0xa, src_id: 0x9, mask_id: 0x4, .. }
    ));
    assert_eq!(cmds[25].len(), 1);
    assert!(matches!(
        &cmds[25][0],
        DrawCmd::Draw { dst_id: 0xe, src_id: 0xf, mask_id: 0x1, .. }
    ));

    // Frees are immediate (66 05000000 / 6609… / 660f…): the whole payload
    // is one 5-byte Free.
    assert_eq!(cmds[11], vec![DrawCmd::Free { id: 0x5 }]);
    assert_eq!(cmds[17], vec![DrawCmd::Free { id: 0x9 }]);
    assert_eq!(cmds[26], vec![DrawCmd::Free { id: 0xf }]);

    // The two 225-byte paint batches start with a composite onto the
    // column windows (dst 0x0d / 0x0e, src 0x06, mask 0x01) and must pack
    // several commands: 225 > 45, and parse_drawcmds consumed everything.
    for (i, dst) in [(21usize, 0x0du32), (23, 0x0e)] {
        assert!(cmds[i].len() >= 2, "batch #{i} packs multiple commands");
        assert!(
            matches!(&cmds[i][0], DrawCmd::Draw { dst_id, src_id: 0x6, mask_id: 0x1, .. } if *dst_id == dst),
            "batch #{i} opens with the documented composite"
        );
    }

    // Final Twrdraw (fixtures-analysis §2 item 5): allocimage id=0x12.
    assert!(matches!(
        &cmds[29][0],
        DrawCmd::Allocate { id: 0x12, chan: 0x6808_1828, repl: 1, .. }
    ));

    // No free of id 0 appears anywhere — image 0 is the screen itself.
    assert!(!cmds
        .iter()
        .flatten()
        .any(|c| matches!(c, DrawCmd::Free { id: 0 } | DrawCmd::FreeScreen { id: 0 })));
}
