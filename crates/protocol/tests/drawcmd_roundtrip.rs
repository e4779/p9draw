//! Round-trip regression for the inner draw-command codec (SPEC.md §6):
//! every Twrdraw payload of the REAL plan9port-acme captures must satisfy
//! encode_drawcmds ∘ parse_drawcmds == identity — byte for byte. This is
//! the contract the e2e client relies on when it stops hand-rolling 'b'/
/// 'd' bytes and uses the shared encoder instead.

use p9draw_protocol::{decode, encode_drawcmds, parse_drawcmds, TWDRAW, Wsysmsg};

const CAPTURES: &[&str] = &[
    "fixtures/live-acme/c2s.bin",
    "fixtures/live-acme-interactive/c2s.bin",
];

/// All Twrdraw data segments of a capture stream, in wire order.
fn twrdraw_payloads(path: &str) -> Vec<Vec<u8>> {
    let c2s = std::fs::read(path).expect("fixture readable");
    let mut off = 0;
    let mut out = Vec::new();
    while off < c2s.len() {
        assert!(off + 6 <= c2s.len(), "truncated header at {off} in {path}");
        let size =
            u32::from_be_bytes([c2s[off], c2s[off + 1], c2s[off + 2], c2s[off + 3]]) as usize;
        let frame = &c2s[off..off + size];
        let (_, msg) = decode(frame).expect("frame decodes");
        if let Wsysmsg::Twrdraw { data } = msg {
            assert_eq!(frame[5], TWDRAW);
            out.push(data);
        }
        off += size;
    }
    assert_eq!(off, c2s.len(), "stream fully consumed");
    out
}

#[test]
fn encode_parse_is_identity_on_live_acme_writes() {
    for path in CAPTURES {
        let payloads = twrdraw_payloads(&format!(
            "{}/../../{path}",
            env!("CARGO_MANIFEST_DIR")
        ));
        assert!(!payloads.is_empty(), "{path}: capture has Twrdraws");
        for (i, data) in payloads.iter().enumerate() {
            let cmds = parse_drawcmds(data)
                .unwrap_or_else(|e| panic!("{path} payload {i}: parse failed: {e}"));
            assert_eq!(
                encode_drawcmds(&cmds),
                *data,
                "{path} payload {i}: encode(parse(x)) != x"
            );
        }
    }
}
