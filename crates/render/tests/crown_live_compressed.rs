//! CROWN TEST (task: 'Y' compressed writeimage): decode the REAL 'Y'
//! frames of the live interactive acme capture (fixtures/live-acme-interactive/
//! c2s.bin) through the render decompressor and prove the decoded images hold
//! glyph pixels — dark ink (< 100) present, exact packed size decoded.

use std::collections::HashMap;

use p9draw_protocol::{decode, parse_drawcmds, DrawCmd, Wsysmsg};
use p9draw_render::{grey_at, write_bytes_compressed, Chan, Image, Point, Rect};

fn fixture(name: &str) -> Vec<u8> {
    std::fs::read(format!(
        "{}/../../fixtures/live-acme-interactive/{name}",
        env!("CARGO_MANIFEST_DIR")
    ))
    .unwrap()
}

/// Walk complete size-prefixed drawfcall frames (a live tail is skipped).
fn twrdraw_payloads(buf: &[u8]) -> Vec<Vec<u8>> {
    let mut out = Vec::new();
    let mut off = 0;
    while off + 4 <= buf.len() {
        let size = u32::from_be_bytes([buf[off], buf[off + 1], buf[off + 2], buf[off + 3]]) as usize;
        if size < 6 || off + size > buf.len() {
            break;
        }
        let (_, msg) = decode(&buf[off..off + size]).expect("complete frame must decode");
        if let Wsysmsg::Twrdraw { data } = msg {
            out.push(data);
        }
        off += size;
    }
    out
}

fn dx(r: Rect) -> i32 {
    r.max.x.wrapping_sub(r.min.x) as i32
}
fn dy(r: Rect) -> i32 {
    r.max.y.wrapping_sub(r.min.y) as i32
}

#[test]
fn crown_live_y_frames_decode_with_dark_ink() {
    let payloads = twrdraw_payloads(&fixture("c2s.bin"));
    assert!(payloads.len() >= 600, "interactive capture Twrdraw count");

    let mut chans: HashMap<u32, (Chan, Rect)> = HashMap::new();
    let mut y_count = 0usize;
    let mut report: Vec<String> = Vec::new();

    for (fi, payload) in payloads.iter().enumerate() {
        let cmds = parse_drawcmds(payload)
            .unwrap_or_else(|e| panic!("frame {fi} fails to parse: {e}"));
        for cmd in &cmds {
            match cmd {
                DrawCmd::Allocate { id, chan, r, .. } => {
                    chans.insert(*id, (Chan(*chan), *r));
                }
                DrawCmd::WriteCompressed { id, r, data } => {
                    y_count += 1;
                    let (chan, _) = chans
                        .get(id)
                        .unwrap_or_else(|| panic!("'Y' for unallocated id {id} (frame {fi})"));
                    let depth = chan.depth() as usize;
                    let w = dx(*r) as usize;
                    let h = dy(*r) as usize;
                    let bpl = (w * depth + 7) / 8;
                    let mut img = Image::with_packed(
                        *id,
                        *r,
                        *r,
                        *chan,
                        false,
                        vec![0u8; bpl * h],
                    )
                    .unwrap_or_else(|e| panic!("decode target id {id}: {e}"));
                    let used = write_bytes_compressed(&mut img, *r, data)
                        .unwrap_or_else(|e| panic!("frame {fi} id {id} rect {r:?}: {e}"));

                    // Luminance scan over the decoded rect.
                    let mut dark = 0usize;
                    let mut bright = 0usize;
                    let mut total_px = 0usize;
                    for yy in 0..h as i32 {
                        for xx in 0..w as i32 {
                            let px = Point { x: (xx as u32).wrapping_add(r.min.x), y: (yy as u32).wrapping_add(r.min.y) };
                            let lum: Option<u8> = if chan.is_grey() {
                                grey_at(&img, px.x as i32, px.y as i32)
                            } else {
                                // byte-aligned RGBA-ish: average the first
                                // three channel bytes (b g r x layout)
                                let bpp = depth / 8;
                                let lx = xx as usize;
                                let ly = yy as usize;
                                let o = ly * bpl + lx * bpp;
                                if o + 2 < img.pixels.len() {
                                    Some(((img.pixels[o] as u32
                                        + img.pixels[o + 1] as u32
                                        + img.pixels[o + 2] as u32)
                                        / 3) as u8)
                                } else {
                                    None
                                }
                            };
                            if let Some(l) = lum {
                                total_px += 1;
                                if l < 100 {
                                    dark += 1;
                                }
                                if l > 155 {
                                    bright += 1;
                                }
                            }
                        }
                    }
                    report.push(format!(
                        "frame {fi} id {id} chan {chan} rect ({},{})-({},{}) data={} used={} px={total_px} dark={dark} bright={bright}",
                        r.min.x as i32, r.min.y as i32, r.max.x as i32, r.max.y as i32, data.len(), used
                    ));
                    // Exact decoded size: Ok(used) from write_bytes_compressed
                    // means bytesperline(rect, depth)·Dy(rect) bytes were
                    // produced (the loop errors out on a short stream), and
                    // acme sends 'Y' as the last op, so the whole tail is
                    // consumed: used == data.len().
                    assert_eq!(used, data.len(), "compressed tail fully consumed");
                    assert_eq!(total_px, w * h, "scanned every decoded pixel");
                    // The crown (task acceptance): decoded ink exists. GREY1
                    // font MASK polarity — plan9port glyph images store the
                    // mask, so glyph ink is the BRIGHT (0xff) part and the
                    // background is dark 0. Both present = real glyphs, not
                    // an all-zero or all-ones decode.
                    assert!(dark > 0, "no dark pixels (< 100) in frame {fi} id {id}");
                    assert!(bright > 0, "no bright ink pixels in frame {fi} id {id} — mask decoded to flat background");
                }
                _ => {}
            }
        }
    }
    for line in &report {
        eprintln!("Y {line}");
    }
    // Golden: exactly ONE 'Y' command in the whole live capture — the
    // GREY1 glyph image 19 (frame 32). Raw \x59 byte hits elsewhere in
    // the .bin are binary payload noise, not op codes.
    assert_eq!(y_count, 1, "live capture holds exactly one 'Y' frame");
}
