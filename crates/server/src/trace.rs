//! `P9DRAW_TRACE=1` — one log line per applied draw command (op letter,
//! image id, rect, data byte count): the live diagnostic for the next
//! acme session. The gate is read once at serve startup, so a disabled
//! session pays a single `OnceLock` lookup per command.

use std::sync::{Arc, OnceLock};

use p9draw_protocol::{DrawCmd, Rect};

use crate::pump::Logger;

static TRACE: OnceLock<Option<Arc<Logger>>> = OnceLock::new();

/// Read `P9DRAW_TRACE` once (`1` enables per-command tracing). Later
/// calls are no-ops — the serve subcommand inits exactly once.
pub fn init_from_env(logger: Arc<Logger>) {
    let on = std::env::var("P9DRAW_TRACE")
        .map(|v| v.trim() == "1")
        .unwrap_or(false);
    init(on, logger);
}

fn init(on: bool, logger: Arc<Logger>) {
    let _ = TRACE.set(if on { Some(logger) } else { None });
}

/// Log one line per command from the Twrdraw stream. Called just before
/// the command applies; a command that fails surfaces as the Twrdraw
/// Rerror on top of its trace line. No-op unless the gate is on.
pub fn log_cmd(cmd: &DrawCmd) {
    if let Some(Some(logger)) = TRACE.get() {
        logger.log(&line(cmd));
    }
}

/// The `op=<…>` token of [`line`], reused to prefix Rerror reasons
/// (`draw op 'd': …`). Errors are rare, so re-formatting is fine.
pub fn op_letter(cmd: &DrawCmd) -> String {
    for tok in line(cmd).split_whitespace() {
        if let Some(op) = tok.strip_prefix("op=") {
            return op.to_string();
        }
    }
    String::new()
}

/// `cmd op=<wire letter> id=<n|-> rect=(x0,y0)-(x1,y1)|- bytes=<n>`.
/// `bytes` counts the command's variable data tail (pixel data, string
/// indices, vertex lists); 0 for fixed-size commands. Fields absent on
/// the wire print as `-`: 'v' is drawflush (devdraw.c:1406) — a 1-byte
/// op with no id/rect — so `op=v id=- rect=-` in a live trace is the
/// correct rendering, not a formatting bug.
fn line(cmd: &DrawCmd) -> String {
    let (op, id, r, bytes): (String, Option<u32>, Option<Rect>, usize) = match cmd {
        DrawCmd::Allocate { id, r, .. } => ("b".into(), Some(*id), Some(*r), 0),
        DrawCmd::AllocScreen { id, .. } => ("A".into(), Some(*id), None, 0),
        DrawCmd::PublicScreen { id, .. } => ("S".into(), Some(*id), None, 0),
        DrawCmd::ReplClip { dst_id, clip_r, .. } => ("c".into(), Some(*dst_id), Some(*clip_r), 0),
        DrawCmd::Draw { dst_id, r, .. } => ("d".into(), Some(*dst_id), Some(*r), 0),
        DrawCmd::Debug { .. } => ("D".into(), None, None, 0),
        DrawCmd::Ellipse { filled, dst_id, .. } => {
            (if *filled { "E" } else { "e" }.into(), Some(*dst_id), None, 0)
        }
        DrawCmd::Free { id } => ("f".into(), Some(*id), None, 0),
        DrawCmd::FreeScreen { id } => ("F".into(), Some(*id), None, 0),
        DrawCmd::InitFont { font_id, .. } => ("i".into(), Some(*font_id), None, 0),
        DrawCmd::Image0Screen => ("J".into(), Some(0), None, 0),
        DrawCmd::ReadInfo => ("I".into(), Some(0), None, 0),
        DrawCmd::Query { specs } => ("q".into(), None, None, specs.len()),
        DrawCmd::LoadFont { font_id, r, .. } => ("l".into(), Some(*font_id), Some(*r), 0),
        DrawCmd::AttachNamed { dst_id, name } => ("n".into(), Some(*dst_id), None, name.len()),
        DrawCmd::NameImage { dst_id, name, .. } => ("N".into(), Some(*dst_id), None, name.len()),
        DrawCmd::Line { dst_id, .. } => ("L".into(), Some(*dst_id), None, 0),
        DrawCmd::Position { id, .. } => ("o".into(), Some(*id), None, 0),
        DrawCmd::SetOp { .. } => ("O".into(), None, None, 0),
        DrawCmd::Polygon { dst_id, pts, .. } => ("p".into(), Some(*dst_id), None, pts.len() * 8),
        DrawCmd::FillPolygon { dst_id, pts, .. } => ("P".into(), Some(*dst_id), None, pts.len() * 8),
        DrawCmd::ReadPixels { id, r } => ("r".into(), Some(*id), Some(*r), 0),
        DrawCmd::String { dst_id, clip_r, indices, .. } => {
            ("s".into(), Some(*dst_id), Some(*clip_r), indices.len() * 2)
        }
        DrawCmd::StringBg { dst_id, clip_r, indices, .. } => {
            ("x".into(), Some(*dst_id), Some(*clip_r), indices.len() * 2)
        }
        DrawCmd::Top { ids, .. } => ("t".into(), None, None, ids.len() * 4),
        DrawCmd::Flush => ("v".into(), None, None, 0),
        DrawCmd::WritePixels { id, r, data } => ("y".into(), Some(*id), Some(*r), data.len()),
        DrawCmd::WriteCompressed { id, r, data } => ("Y".into(), Some(*id), Some(*r), data.len()),
        DrawCmd::Unknown { op } => (format!("#{op:02x}"), None, None, 0),
    };
    let id = id.map(|v| v.to_string()).unwrap_or_else(|| "-".into());
    let rect = r
        .map(|r| format!("rect=({},{})-({},{})", r.min.x, r.min.y, r.max.x, r.max.y))
        .unwrap_or_else(|| "rect=-".into());
    format!("cmd op={op} id={id} {rect} bytes={bytes}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use p9draw_protocol::Point;

    fn rect(x0: u32, y0: u32, x1: u32, y1: u32) -> Rect {
        Rect { min: Point { x: x0, y: y0 }, max: Point { x: x1, y: y1 } }
    }

    #[test]
    fn write_pixels_line_has_op_id_rect_bytes() {
        let cmd = DrawCmd::WritePixels { id: 5, r: rect(1, 2, 5, 6), data: vec![0; 8] };
        assert_eq!(line(&cmd), "cmd op=y id=5 rect=(1,2)-(5,6) bytes=8");
    }

    #[test]
    fn data_tails_count_bytes() {
        let cmd = DrawCmd::String {
            dst_id: 3,
            src_id: 1,
            font_id: 2,
            p: Point { x: 0, y: 0 },
            clip_r: rect(0, 0, 10, 10),
            sp: Point { x: 0, y: 0 },
            indices: vec![1, 2, 3],
        };
        assert_eq!(line(&cmd), "cmd op=s id=3 rect=(0,0)-(10,10) bytes=6");
    }

    #[test]
    fn flush_and_unknown_have_no_image() {
        assert_eq!(line(&DrawCmd::Flush), "cmd op=v id=- rect=- bytes=0");
        let s = line(&DrawCmd::Unknown { op: 0x7f });
        assert!(s.starts_with("cmd op=#7f id=- rect=- bytes=0"), "s: {s}");
    }

    #[test]
    fn op_letter_maps_wire_letters() {
        assert_eq!(op_letter(&DrawCmd::Flush), "v");
        assert_eq!(
            op_letter(&DrawCmd::WritePixels { id: 1, r: rect(0, 0, 1, 1), data: vec![] }),
            "y"
        );
        // Uppercase wire letters survive verbatim ('Y' is the compressed
        // writeimage acme uses for its GREY1 glyph images).
        assert_eq!(
            op_letter(&DrawCmd::WriteCompressed { id: 1, r: rect(0, 0, 1, 1), data: vec![7] }),
            "Y"
        );
        assert_eq!(
            line(&DrawCmd::WriteCompressed { id: 1, r: rect(0, 0, 1, 1), data: vec![7] }),
            "cmd op=Y id=1 rect=(0,0)-(1,1) bytes=1"
        );
        // Font ops: the letters devdraw.c dispatches on (885/991/1273).
        assert_eq!(op_letter(&DrawCmd::InitFont { font_id: 9, nchars: 4, ascent: 7 }), "i");
        assert_eq!(
            op_letter(&DrawCmd::LoadFont {
                font_id: 9,
                src_id: 1,
                index: 0,
                r: rect(0, 0, 1, 1),
                sp: Point { x: 0, y: 0 },
                left: 0,
                width: 3,
            }),
            "l"
        );
        assert_eq!(
            op_letter(&DrawCmd::String {
                dst_id: 1,
                src_id: 0,
                font_id: 9,
                p: Point { x: 0, y: 0 },
                clip_r: rect(0, 0, 1, 1),
                sp: Point { x: 0, y: 0 },
                indices: vec![0],
            }),
            "s"
        );
        assert_eq!(
            op_letter(&DrawCmd::StringBg {
                dst_id: 1,
                src_id: 0,
                font_id: 9,
                p: Point { x: 0, y: 0 },
                clip_r: rect(0, 0, 1, 1),
                sp: Point { x: 0, y: 0 },
                bg_id: 0,
                bg_pt: Point { x: 0, y: 0 },
                indices: vec![0],
            }),
            "x"
        );
        assert_eq!(op_letter(&DrawCmd::Unknown { op: 0x7f }), "#7f");
    }

    #[test]
    fn log_cmd_never_panics_without_init() {
        // Whatever the process-global gate state is, a disabled trace is
        // a silent no-op.
        log_cmd(&DrawCmd::Flush);
    }

    #[test]
    fn enabled_gate_logs_into_the_logger() {
        let (logger, logbuf) = Logger::memory();
        init(true, Arc::new(logger));
        let cmd = DrawCmd::WritePixels { id: 9, r: rect(0, 0, 2, 2), data: vec![0xAA; 4] };
        log_cmd(&cmd);
        let log = String::from_utf8(logbuf.lock().unwrap().clone()).unwrap();
        assert!(log.contains("op=y id=9 rect=(0,0)-(2,2) bytes=4"), "log: {log}");
    }
}
