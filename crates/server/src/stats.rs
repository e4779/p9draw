//! `P9DRAW_STATS=1` — diagnostic counters for the serve subcommand.
//!
//! When the env var is set, every wire frame is bucketed by its drawfcall
//! type (SPEC.md §4): undecodable frames under the "bad" bucket, decoded
//! requests and sent replies under their type code 1..=33. A reporter
//! thread prints the snapshot to the logger (stderr by default — stdout
//! carries the protocol) every [`STATS_PERIOD`], and one final line is
//! logged on exit, so even a sub-30 s session leaves evidence:
//!
//! ```text
//! serve stats: Tinit: 1, 25 bytes; Rinit: 1, 6 bytes; Twrdraw: 30, 1933 bytes
//! ```
//!
//! Counting lives behind a global because replies are sent from deep
//! inside `screen.rs` (`send`); the process is single-client, so a
//! `Mutex` around a plain struct is plenty. When the env var is unset
//! every call no-ops — zero cost on the hot path.

use std::sync::{Arc, Mutex, OnceLock};
use std::thread;
use std::time::Duration;

use p9draw_protocol::{encoded_size, Wsysmsg};

use crate::pump::Logger;

/// Cadence of the periodic stderr stats line.
pub const STATS_PERIOD: Duration = Duration::from_secs(30);

/// Bucket for frames that never decoded (no type code to file them
/// under). Real types occupy 1..=33.
pub const BAD: u8 = 0;

/// Wire type name (SPEC.md §4) for a frame bucket; `BAD` and any other
/// code render as "bad". Single source of truth — `logfmt::type_name`
/// delegates here.
pub fn type_name(code: u8) -> &'static str {
    match code {
        1 => "Rerror",
        2 => "Trdmouse",
        3 => "Rrdmouse",
        4 => "Tmoveto",
        5 => "Rmoveto",
        6 => "Tcursor",
        7 => "Rcursor",
        8 => "Tbouncemouse",
        9 => "Rbouncemouse",
        10 => "Trdkbd",
        11 => "Rrdkbd",
        12 => "Tlabel",
        13 => "Rlabel",
        14 => "Tinit",
        15 => "Rinit",
        16 => "Trdsnarf",
        17 => "Rrdsnarf",
        18 => "Twrsnarf",
        19 => "Rwrsnarf",
        20 => "Trddraw",
        21 => "Rrddraw",
        22 => "Twrdraw",
        23 => "Rwrdraw",
        24 => "Ttop",
        25 => "Rtop",
        26 => "Tresize",
        27 => "Rresize",
        28 => "Tcursor2",
        29 => "Rcursor2",
        30 => "Tctxt",
        31 => "Rctxt",
        32 => "Trdkbd4",
        33 => "Rrdkbd4",
        _ => "bad",
    }
}

/// Per-type frame counters: full frames (size prefix included) bucketed
/// by wire type. Index 0 = undecodable, 1..=33 = the drawfcall types.
pub struct Stats {
    count: [u64; 34],
    bytes: [u64; 34],
}

impl Default for Stats {
    // Manual impl: derive(Default) covers [T; N] only up to N = 32.
    fn default() -> Self {
        Stats { count: [0; 34], bytes: [0; 34] }
    }
}

impl Stats {
    /// File one frame of `code` wire bytes under its bucket. Codes
    /// outside 1..=33 land in the "bad" bucket (only reachable for
    /// undecodable frames — `decode` rejects unknown types).
    pub fn record(&mut self, code: u8, bytes: usize) {
        let b = match code {
            1..=33 => code as usize,
            _ => BAD as usize,
        };
        self.count[b] += 1;
        self.bytes[b] += bytes as u64;
    }

    /// Non-zero buckets as `(name, count, bytes)`, "bad" first, then by
    /// ascending type code (reads like the chronology of a session).
    pub fn snapshot(&self) -> Vec<(&'static str, u64, u64)> {
        let mut out = Vec::new();
        if self.count[BAD as usize] > 0 {
            out.push(("bad", self.count[0], self.bytes[0]));
        }
        for code in 1..=33u8 {
            let i = code as usize;
            if self.count[i] > 0 {
                out.push((type_name(code), self.count[i], self.bytes[i]));
            }
        }
        out
    }
}

/// One stderr line over a [`Stats::snapshot`]: `name: count, bytes`
/// segments joined with `; `.
pub fn format_stats(entries: &[(&'static str, u64, u64)]) -> String {
    if entries.is_empty() {
        return "serve stats: no frames".to_string();
    }
    let body: Vec<String> = entries
        .iter()
        .map(|(n, c, b)| format!("{n}: {c}, {b} bytes"))
        .collect();
    format!("serve stats: {}", body.join("; "))
}

// --- img0 histogram (sampled by screen.rs present) ---------------------------

/// Latest histogram of the STORE's composited screen image (`images[0]`),
/// sampled by `Screen::present` on every dirty batch and printed by the
/// reporter next to the wire stats: the direct arbiter between "acme
/// paints past image 0" (dark) and "image 0 is fine, the bug is
/// downstream" (light). Classification by avg-RGB: dark < 64, light >
/// 200 (background fills: white / paleyellow), other in between (ink,
/// borders).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Img0Sample {
    /// image 0 geometry in physical pixels
    pub w: u32,
    pub h: u32,
    /// sampled pixels with avg-RGB < 64
    pub dark: u64,
    /// sampled pixels with avg-RGB > 200
    pub light: u64,
    /// sampled pixels in between
    pub other: u64,
    /// windows composited over image 0, `"id WxH"` comma-joined —
    /// branch-B diagnostics: which window the composite covered
    pub windows: String,
}

static IMG0: Mutex<Option<Img0Sample>> = Mutex::new(None);

/// Park the latest img0 histogram; no-op when counting is off.
pub fn record_img0(sample: Img0Sample) {
    if enabled() {
        if let Ok(mut slot) = IMG0.lock() {
            *slot = Some(sample);
        }
    }
}

/// `serve img0: 1552x880 dark=N light=M other=K wins=[39 1552x880]`.
pub fn format_img0(s: &Img0Sample) -> String {
    format!(
        "serve img0: {}x{} dark={} light={} other={} wins=[{}]",
        s.w, s.h, s.dark, s.light, s.other, s.windows
    )
}

fn img0_current() -> Option<Img0Sample> {
    IMG0.lock().ok()?.clone()
}

/// Both periodic lines: wire buckets, then the img0 histogram when the
/// screen has parked one.
fn log_current(logger: &Logger) {
    logger.log(&format_stats(&current()));
    if let Some(s) = img0_current() {
        logger.log(&format_img0(&s));
    }
}

static STATS: OnceLock<Option<Mutex<Stats>>> = OnceLock::new();

/// Read `P9DRAW_STATS` once (`1` enables counting). Later calls are
/// no-ops — the serve subcommand inits exactly once.
pub fn init_from_env() {
    let on = std::env::var("P9DRAW_STATS")
        .map(|v| v.trim() == "1")
        .unwrap_or(false);
    let _ = STATS.set(if on {
        Some(Mutex::new(Stats::default()))
    } else {
        None
    });
}

/// Is counting on? `false` before [`init_from_env`].
pub fn enabled() -> bool {
    STATS.get().map(|s| s.is_some()).unwrap_or(false)
}

fn bump(code: u8, bytes: usize) {
    if let Some(Some(stats)) = STATS.get() {
        stats
            .lock()
            .expect("stats mutex poisoned")
            .record(code, bytes);
    }
}

/// Count a request frame received from the client. `code` is the raw
/// wire type byte (frame offset 5); use `0` when the frame failed to
/// decode.
pub fn record_frame(code: u8, bytes: usize) {
    bump(code, bytes);
}

/// Count a reply about to be sent ([`crate::screen::send`] is the single
/// reply funnel).
pub fn record_reply(msg: &Wsysmsg) {
    bump(msg.msg_type(), encoded_size(msg) as usize);
}

/// Periodic reporter: prints the snapshot every [`STATS_PERIOD`] until
/// the process exits. Never joined — serve's lifetime is the process.
pub fn spawn_reporter(logger: Arc<Logger>) {
    if !enabled() {
        return;
    }
    thread::Builder::new()
        .name("p9draw-stats".into())
        .spawn(move || loop {
            thread::sleep(STATS_PERIOD);
            log_current(&logger);
        })
        .expect("spawn stats reporter");
}

/// Final line at serve exit — sub-30 s sessions would otherwise print
/// nothing. No-op when counting is off.
pub fn log_final(logger: &Logger) {
    if enabled() {
        log_current(logger);
    }
}

fn current() -> Vec<(&'static str, u64, u64)> {
    match STATS.get() {
        Some(Some(m)) => m.lock().expect("stats mutex poisoned").snapshot(),
        _ => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn type_names_cover_every_wire_type() {
        for code in 1..=33u8 {
            let n = type_name(code);
            assert!(n != "bad" && !n.is_empty(), "type {code} unnamed");
        }
        assert_eq!(type_name(0), "bad");
        assert_eq!(type_name(200), "bad");
        // spot checks against SPEC.md §4
        assert_eq!(type_name(1), "Rerror");
        assert_eq!(type_name(14), "Tinit");
        assert_eq!(type_name(22), "Twrdraw");
        assert_eq!(type_name(32), "Trdkbd4");
    }

    #[test]
    fn record_buckets_by_wire_type_and_bytes() {
        let mut s = Stats::default();
        s.record(14, 25); // Tinit
        s.record(15, 6); // Rinit
        s.record(14, 18); // another Tinit
        s.record(0, 9); // undecodable
        s.record(200, 5); // impossible code -> bad bucket
        assert_eq!(
            s.snapshot(),
            vec![("bad", 2, 14), ("Tinit", 2, 43), ("Rinit", 1, 6)]
        );
    }

    #[test]
    fn empty_snapshot_formats_the_no_frames_line() {
        assert_eq!(format_stats(&[]), "serve stats: no frames");
    }

    #[test]
    fn snapshot_formats_type_count_bytes_segments() {
        let mut s = Stats::default();
        s.record(14, 25);
        s.record(22, 1933);
        s.record(22, 46);
        assert_eq!(
            format_stats(&s.snapshot()),
            "serve stats: Tinit: 1, 25 bytes; Twrdraw: 2, 1979 bytes"
        );
    }

    #[test]
    fn replies_count_via_msg_type_and_encoded_size() {
        // the send() funnel counts the full frame, size prefix included
        let msg = Wsysmsg::Rrdmouse {
            x: 100,
            y: 200,
            buttons: 4,
            msec: 12_345,
            resized: 0,
        };
        let mut s = Stats::default();
        s.record(msg.msg_type(), encoded_size(&msg) as usize);
        assert_eq!(s.snapshot(), vec![("Rrdmouse", 1, 23)]);
    }

    #[test]
    fn img0_line_formats_geometry_counts_and_windows() {
        let s = Img0Sample {
            w: 1552,
            h: 880,
            dark: 1700,
            light: 63_000,
            other: 800,
            windows: "39 1552x880".into(),
        };
        assert_eq!(
            format_img0(&s),
            "serve img0: 1552x880 dark=1700 light=63000 other=800 wins=[39 1552x880]"
        );
    }

    #[test]
    fn img0_record_is_silent_while_counting_is_off() {
        // No init_from_env ran in this test process: enabled() is false,
        // so the record is dropped and the slot stays empty.
        record_img0(Img0Sample {
            w: 4,
            h: 4,
            dark: 1,
            light: 2,
            other: 1,
            windows: String::new(),
        });
        assert!(img0_current().is_none());
    }
}
