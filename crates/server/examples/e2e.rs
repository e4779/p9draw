//! End-to-end harness for `p9draw-server serve` (README, §End-to-end
//! test). A synthetic drawfcall client (SPEC.md §2) speaks the real wire
//! protocol over the child's stdin/stdout — the exact fd pair a plan9port
//! client dup2()s its pipe onto:
//!
//!   1. init     — `Tinit{winsize, label}` → `Rinit`
//!   2. alloc    — Twrdraw `'b'` allocimage id=1, whole-screen rect,
//!                 XRGB32, value = DPaleyellow 0xFFFFAAFF (the image is
//!                 born filled — allocimage fill semantics, SPEC.md §6)
//!   3. fill     — Twrdraw `'d'` src=1 over image 0, full rect, plus
//!                 `'v'` flush in the same stream (count = 45+1 = 46,
//!                 like the live capture): paleyellow composes over the
//!                 whole screen
//!   4. readback — Twrdraw `'r'` (image 0, full rect), then Trddraw →
//!                 Rrddraw; the bytes must equal the render-semantics
//!                 expectation [B,G,R,X] = AA FF FF 00 per pixel
//!   5. mouse    — Tbouncemouse (synthetic event) then Trdmouse →
//!                 Rrdmouse within 5 s; SKIP without an X display
//!
//! Every step prints PASSED / SKIP / FAILED on stdout; exit 0 unless a
//! step FAILED. Without `$DISPLAY` all steps SKIP (the serve window
//! cannot open), so the harness is CI-safe on headless machines.
//!
//! The serve binary is located in `target/{release,debug}` (or via
//! `$P9DRAW_SERVER_BIN`); `cargo build --bins` first if it is missing.
//! The child runs with P9DRAW_STATS=1, so its stderr ends with the
//! serve stats line — a smoke test of the diagnostic counters for free.

use std::io::{Read, Write};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::thread;
use std::time::Duration;

use p9draw_protocol::{DrawCmd, decode, encode, encode_drawcmds, Point, Rect, Wsysmsg};
use p9draw_render::Chan;

/// Client winsize hint for the child's window — small on purpose, the
/// readback must fit one 64 KiB Rrddraw.
const W: u32 = 120;
const H: u32 = 90;
/// draw.h DPaleyellow: canonical RGBA (alpha in the low byte).
const PALEYELLOW: u32 = 0xFFFF_AAFF;
/// Per-reply timeout; SPEC.md §5: Rrdmouse is answered asynchronously.
const RPC_TIMEOUT: Duration = Duration::from_secs(5);

fn main() {
    std::process::exit(run());
}

fn run() -> i32 {
    println!("e2e: p9draw-server serve (synthetic drawfcall client)");

    // The serve window needs X; without a display every step is honestly
    // SKIP and the harness still exits 0 (CI-safe).
    let has_x = std::env::var("DISPLAY")
        .map(|v| !v.trim().is_empty())
        .unwrap_or(false);
    if !has_x {
        for step in ["init", "alloc", "fill", "readback", "mouse"] {
            report(step, "SKIP", "no X display ($DISPLAY is empty)");
        }
        return 0;
    }

    let Some(bin) = server_bin() else {
        report("spawn", "FAILED", "p9draw-server binary not found; run `cargo build --bins` or set $P9DRAW_SERVER_BIN");
        return 1;
    };

    let mut srv = match Srv::spawn(&bin) {
        Ok(s) => s,
        Err(e) => {
            report("spawn", "FAILED", &e);
            return 1;
        }
    };
    println!("e2e: child {} (DISPLAY inherited, P9DRAW_STATS=1)", bin.display());

    let full = rect(0, 0, W, H);
    let mut failed = false;
    let mut init_ok = false;

    // --- 1. init: Tinit -> Rinit ------------------------------------------
    match srv.rpc(1, &Wsysmsg::Tinit {
        winsize: format!("{W}x{H}"),
        label: "p9draw-e2e".into(),
    }) {
        Ok((tag, Wsysmsg::Rinit)) if tag == 1 => {
            report("init", "PASSED", "Rinit received");
            init_ok = true;
        }
        Ok((_, Wsysmsg::Rerror { error })) => {
            report("init", "FAILED", &format!("Rerror: {error}"));
            failed = true;
        }
        Ok((tag, msg)) => {
            report("init", "FAILED", &format!("tag={tag} unexpected {msg:?}"));
            failed = true;
        }
        Err(e) => {
            report("init", "FAILED", &e);
            failed = true;
        }
    }

    if !init_ok {
        for step in ["alloc", "fill", "readback", "mouse"] {
            report(step, "SKIP", "init failed");
        }
        let _ = srv.child.wait();
        return 1;
    }

    // --- 2. alloc: Twrdraw 'b' — image 1 born paleyellow -------------------
    // Shared SPEC.md §6 encoder (roundtrip-verified against the live acme
    // captures): 'b' is 51 bytes — id, screenid[4] (u16 + 2 ignored high
    // bytes), refresh, chan, repl, r, clipr, value — all LE. The old
    // hand-rolled version wrote screen_id as a bare u16 (49-byte command)
    // and devdraw answered "bad draw command".
    let alloc = encode_drawcmds(&[DrawCmd::Allocate {
        id: 1,
        screen_id: 0, // plain image, no window
        refresh: 0,   // Refbackup
        chan: Chan::XRGB32.0,
        repl: 0,
        r: full,
        clip_r: full,
        value: PALEYELLOW,
    }]);
    debug_assert_eq!(alloc.len(), 51);
    let _ = step_rpc(&mut srv, "alloc", 2, &Wsysmsg::Twrdraw { data: alloc }, &mut failed);

    // --- 3. fill: 'd' src=1 over image 0 (whole screen) + 'v' flush -------
    // 'd' composite (45 bytes: dst, src, mask, r, srcpt, maskpt) plus the
    // 1-byte 'v' flush — 46 total, like the fixture's d+v pair.
    let fill = encode_drawcmds(&[
        DrawCmd::Draw {
            dst_id: 0, // screen image
            src_id: 1, // paleyellow image
            mask_id: 0,
            r: full,
            src_pt: Point { x: 0, y: 0 },
            mask_pt: Point { x: 0, y: 0 },
        },
        DrawCmd::Flush,
    ]);
    debug_assert_eq!(fill.len(), 46);
    let _ = step_rpc(&mut srv, "fill", 3, &Wsysmsg::Twrdraw { data: fill }, &mut failed);

    // --- 4. readback: 'r' image 0 full rect, then drain Trddraw ------------
    let need = (W * H * 4) as usize;
    let r = encode_drawcmds(&[DrawCmd::ReadPixels { id: 0, r: full }]);
    debug_assert_eq!(r.len(), 21);
    let mut got = Vec::with_capacity(need);
    let mut read_failed = failed;
    if step_rpc(&mut srv, "readback-r", 4, &Wsysmsg::Twrdraw { data: r }, &mut read_failed).is_some() {
        while got.len() < need {
            match srv.rpc(5, &Wsysmsg::Trddraw { count: (need - got.len()) as u32 }) {
                Ok((_, Wsysmsg::Rrddraw { data })) => got.extend_from_slice(&data),
                Ok((_, Wsysmsg::Rerror { error })) => {
                    report("readback", "FAILED", &format!("Rerror: {error}"));
                    read_failed = true;
                    break;
                }
                Ok((tag, msg)) => {
                    report("readback", "FAILED", &format!("tag={tag} unexpected {msg:?}"));
                    read_failed = true;
                    break;
                }
                Err(e) => {
                    report("readback", "FAILED", &e);
                    read_failed = true;
                    break;
                }
            }
        }
        if !read_failed {
            // Render semantics: x8r8g8b8 stores [B,G,R,X]; DPaleyellow
            // 0xFFFFAAFF converts via Chan::rgbatoimg -> 0x00FFFFAA, i.e.
            // bytes AA FF FF 00 per pixel (screen.rs make_image unit test).
            let px = [0xAA, 0xFF, 0xFF, 0x00];
            let mismatch = got
                .chunks_exact(4)
                .position(|p| p != px)
                .map(|i| format!("pixel {i} (byte offset {}) = {:02x?}, want {px:02x?}", i * 4, &got[i * 4..i * 4 + 4]))
                .filter(|_| got.len() == need)
                .or_else(|| (got.len() != need).then(|| format!("got {} bytes, want {need}", got.len())));
            read_failed = mismatch.is_some();
            match &mismatch {
                Some(why) => report("readback", "FAILED", why),
                None => report("readback", "PASSED", &format!("{} bytes of paleyellow [AA FF FF 00]", got.len())),
            }
        }
    }
    failed |= read_failed;

    // --- 5. mouse: Tbouncemouse parks fresh state, Trdmouse answers -------
    if failed {
        report("mouse", "SKIP", "a previous step failed");
    } else {
        match srv.rpc(6, &Wsysmsg::Tbouncemouse { x: 9, y: 7, buttons: 0 }) {
            Ok((_, Wsysmsg::Rbouncemouse)) => match srv.rpc(7, &Wsysmsg::Trdmouse) {
                Ok((_, Wsysmsg::Rrdmouse { x, y, buttons, msec, resized })) => {
                    // A real host event may race the bounce and win; any
                    // well-formed event inside the window proves the
                    // async mouse path (SPEC.md §5).
                    let in_win = x < W && y < H;
                    if in_win {
                        report("mouse", "PASSED", &format!("Rrdmouse x={x} y={y} buttons={buttons} msec={msec} resized={resized}"));
                    } else {
                        report("mouse", "FAILED", &format!("Rrdmouse outside the window: x={x} y={y}"));
                        failed = true;
                    }
                }
                Ok((tag, msg)) => {
                    report("mouse", "FAILED", &format!("tag={tag} unexpected {msg:?}"));
                    failed = true;
                }
                Err(e) => {
                    report("mouse", "FAILED", &e);
                    failed = true;
                }
            },
            Ok((tag, msg)) => {
                report("mouse", "FAILED", &format!("tag={tag} unexpected {msg:?}"));
                failed = true;
            }
            Err(e) => {
                report("mouse", "FAILED", &e);
                failed = true;
            }
        }
    }

    // Drop stdin: EOF is the session lifetime (serve exits 0 on it).
    drop(srv.stdin.take());
    let _ = srv.child.wait();
    println!("e2e: {}", if failed { "FAILED" } else { "PASSED" });
    i32::from(failed)
}

/// Run one tagged RPC step that expects `Rwrdraw` (count echoed).
fn step_rpc(srv: &mut Srv, name: &str, tag: u8, msg: &Wsysmsg, failed: &mut bool) -> Option<(u8, Wsysmsg)> {
    match srv.rpc(tag, msg) {
        Ok((t, Wsysmsg::Rwrdraw { count })) if t == tag => {
            report(name, "PASSED", &format!("Rwrdraw count={count}"));
            Some((t, Wsysmsg::Rwrdraw { count }))
        }
        Ok((_, Wsysmsg::Rerror { error })) => {
            report(name, "FAILED", &format!("Rerror: {error}"));
            *failed = true;
            None
        }
        Ok((t, m)) => {
            report(name, "FAILED", &format!("tag={t} unexpected {m:?}"));
            *failed = true;
            None
        }
        Err(e) => {
            report(name, "FAILED", &e);
            *failed = true;
            None
        }
    }
}

fn report(step: &str, status: &str, note: &str) {
    println!("[{status:^7}] {step}: {note}");
}

/// Locate the serve binary: `$P9DRAW_SERVER_BIN`, else the workspace
/// `target/` tree (examples do not get CARGO_BIN_EXE_*; tests do).
fn server_bin() -> Option<std::path::PathBuf> {
    if let Ok(p) = std::env::var("P9DRAW_SERVER_BIN") {
        let pb = std::path::PathBuf::from(p);
        if pb.is_file() {
            return Some(pb);
        }
    }
    let manifest = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let target = manifest.ancestors().nth(2)?.join("target");
    let pref = if cfg!(debug_assertions) { "debug" } else { "release" };
    let mut dirs = vec![target.join(pref)];
    dirs.extend([target.join("release"), target.join("debug")]);
    dirs.dedup();
    dirs.into_iter()
        .map(|d| d.join("p9draw-server"))
        .find(|b| b.is_file())
}

fn rect(x0: u32, y0: u32, x1: u32, y1: u32) -> Rect {
    Rect { min: Point { x: x0, y: y0 }, max: Point { x: x1, y: y1 } }
}

/// The child server plus a reader thread feeding decoded frames through
/// a channel — std Read has no timeout, so `recv_timeout` provides one.
struct Srv {
    child: Child,
    stdin: Option<ChildStdin>,
    rx: Receiver<Result<(u8, Wsysmsg), String>>,
}

impl Srv {
    fn spawn(bin: &std::path::Path) -> Result<Srv, String> {
        let mut child = Command::new(bin)
            .arg("serve")
            .env("P9DRAW_STATS", "1")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .map_err(|e| format!("spawn {}: {e}", bin.display()))?;
        let stdin = child.stdin.take().ok_or("no child stdin")?;
        let mut out = child.stdout.take().ok_or("no child stdout")?;
        let (tx, rx) = mpsc::channel();
        thread::spawn(move || {
            let mut head = [0u8; 4];
            loop {
                if out.read_exact(&mut head).is_err() {
                    let _ = tx.send(Err("EOF: server closed stdout".into()));
                    break;
                }
                let size = u32::from_be_bytes(head) as usize;
                if size < p9draw_protocol::MIN_FRAME {
                    let _ = tx.send(Err(format!("bad frame size {size}")));
                    break;
                }
                let mut frame = head.to_vec();
                frame.resize(size, 0);
                if out.read_exact(&mut frame[4..]).is_err() {
                    let _ = tx.send(Err("EOF mid-frame".into()));
                    break;
                }
                let res = decode(&frame).map_err(|e| format!("decode: {e}"));
                if tx.send(res).is_err() {
                    break;
                }
            }
        });
        Ok(Srv { child, stdin: Some(stdin), rx })
    }

    /// Send one frame, wait for the reply on the same tag.
    fn rpc(&mut self, tag: u8, msg: &Wsysmsg) -> Result<(u8, Wsysmsg), String> {
        let stdin = self.stdin.as_mut().ok_or("child stdin closed")?;
        stdin
            .write_all(&encode(msg, tag))
            .and_then(|_| stdin.flush())
            .map_err(|e| format!("write frame: {e}"))?;
        match self.rx.recv_timeout(RPC_TIMEOUT) {
            Ok(v) => v,
            Err(RecvTimeoutError::Timeout) => Err(format!(
                "timeout after {RPC_TIMEOUT:?} waiting for the reply to tag={tag} {}",
                msg.msg_type()
            )),
            Err(RecvTimeoutError::Disconnected) => Err("server reader exited".into()),
        }
    }
}
