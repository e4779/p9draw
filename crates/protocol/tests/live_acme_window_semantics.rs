//! Live-capture semantics pin (fixtures/live-acme-interactive/c2s.bin,
//! 2026-10 interactive session): acme streams ONE full-screen window
//! ('b' with screen_id != 0), every string op targets that window, its
//! font caches are GREY1 images and its ink tile is a GREY1 1×1 repl.
//! That is exactly the shape that needs the window composite at flush
//! plus the grey→x8r8g8b8 blit conversion to become visible text — the
//! missing-text symptom of the 2026-10 live run traces back to it.
use p9draw_protocol::{decode, parse_drawcmds, DrawCmd, Wsysmsg};
use std::collections::BTreeMap;

/// GREY1 channel descriptor (crates/render/src/chan.rs).
const GREY1: u32 = 0x31;

#[test]
fn live_acme_streams_window_text_on_grey_fonts() {
    let data = std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../fixtures/live-acme-interactive/c2s.bin"
    ))
    .unwrap();
    let mut off = 0usize;
    let mut windows: Vec<(u32, (u32, u32, u32, u32))> = Vec::new();
    let mut chans: BTreeMap<u32, u32> = BTreeMap::new();
    let mut fonts: Vec<u32> = Vec::new();
    let mut dsts: BTreeMap<u32, usize> = BTreeMap::new();
    let mut inks: BTreeMap<u32, usize> = BTreeMap::new();
    while off + 6 <= data.len() {
        let sz =
            u32::from_be_bytes([data[off], data[off + 1], data[off + 2], data[off + 3]]) as usize;
        let end = (off + sz).min(data.len());
        let Ok((_, msg)) = decode(&data[off..end])
            .or_else(|_| decode(&data[off..(off + sz + 4).min(data.len())]))
        else {
            break;
        };
        off += sz;
        let Wsysmsg::Twrdraw { data } = &msg else { continue };
        let Ok(cmds) = parse_drawcmds(data) else { continue };
        for c in cmds {
            match c {
                DrawCmd::Allocate { id, screen_id, chan, r, .. } => {
                    chans.insert(id, chan);
                    if screen_id != 0 {
                        windows.push((id, (r.min.x, r.min.y, r.max.x, r.max.y)));
                    }
                }
                DrawCmd::InitFont { font_id, .. } => fonts.push(font_id),
                DrawCmd::StringBg { dst_id, src_id, .. } | DrawCmd::String { dst_id, src_id, .. } => {
                    *dsts.entry(dst_id).or_default() += 1;
                    *inks.entry(src_id).or_default() += 1;
                }
                _ => {}
            }
        }
    }
    // One full-screen window carries everything acme shows.
    assert_eq!(windows.len(), 1, "exactly one window: {windows:?}");
    let (wid, (x0, y0, x1, y1)) = windows[0];
    assert_eq!((x0, y0), (0, 0), "window sits at the screen origin");
    assert!(x1 > 1900 && y1 > 1300, "window is full-screen: {x1}x{y1}");
    // Every string op draws into that window, none elsewhere.
    assert!(dsts.contains_key(&wid), "strings target the window: {dsts:?}");
    assert_eq!(dsts.len(), 1, "no string draws outside the window: {dsts:?}");
    assert!(*dsts.get(&wid).unwrap() > 100, "many text ops: {dsts:?}");
    // Fonts and ink tiles are GREY1 — the blit conversion case.
    assert!(!fonts.is_empty(), "fonts initialized");
    for f in &fonts {
        assert_eq!(chans.get(f), Some(&GREY1), "font image {f} must be GREY1");
    }
    assert!(!inks.is_empty(), "string ops seen");
    for (src, n) in &inks {
        assert_eq!(chans.get(src), Some(&GREY1), "ink tile {src} ({n} uses) must be GREY1");
    }
}
