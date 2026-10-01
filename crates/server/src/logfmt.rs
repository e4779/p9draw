//! Human-readable one-line rendering of decoded drawfcall messages for
//! the serve/capture logs: direction, tag, type name, key fields, frame
//! size (SPEC.md §4).

use p9draw_protocol::{encoded_size, Point, Rect, Wsysmsg};

/// Render one decoded frame as a single log line, e.g.
/// `c2s tag=1 Tinit winsize="640x480" label="acme" (25B)`.
pub fn frame_line(dir: &str, tag: u8, msg: &Wsysmsg) -> String {
    let name = type_name(msg);
    let fields = fields(msg);
    let size = encoded_size(msg);
    if fields.is_empty() {
        format!("{dir} tag={tag} {name} ({size}B)")
    } else {
        format!("{dir} tag={tag} {name} {fields} ({size}B)")
    }
}

fn type_name(msg: &Wsysmsg) -> &'static str {
    match msg {
        Wsysmsg::Rerror { .. } => "Rerror",
        Wsysmsg::Trdmouse => "Trdmouse",
        Wsysmsg::Rrdmouse { .. } => "Rrdmouse",
        Wsysmsg::Tmoveto { .. } => "Tmoveto",
        Wsysmsg::Rmoveto => "Rmoveto",
        Wsysmsg::Tcursor { .. } => "Tcursor",
        Wsysmsg::Rcursor => "Rcursor",
        Wsysmsg::Tbouncemouse { .. } => "Tbouncemouse",
        Wsysmsg::Rbouncemouse => "Rbouncemouse",
        Wsysmsg::Trdkbd => "Trdkbd",
        Wsysmsg::Rrdkbd { .. } => "Rrdkbd",
        Wsysmsg::Tlabel { .. } => "Tlabel",
        Wsysmsg::Rlabel => "Rlabel",
        Wsysmsg::Tinit { .. } => "Tinit",
        Wsysmsg::Rinit => "Rinit",
        Wsysmsg::Trdsnarf => "Trdsnarf",
        Wsysmsg::Rrdsnarf { .. } => "Rrdsnarf",
        Wsysmsg::Twrsnarf { .. } => "Twrsnarf",
        Wsysmsg::Rwrsnarf => "Rwrsnarf",
        Wsysmsg::Trddraw { .. } => "Trddraw",
        Wsysmsg::Rrddraw { .. } => "Rrddraw",
        Wsysmsg::Twrdraw { .. } => "Twrdraw",
        Wsysmsg::Rwrdraw { .. } => "Rwrdraw",
        Wsysmsg::Ttop => "Ttop",
        Wsysmsg::Rtop => "Rtop",
        Wsysmsg::Tresize { .. } => "Tresize",
        Wsysmsg::Rresize => "Rresize",
        Wsysmsg::Tcursor2 { .. } => "Tcursor2",
        Wsysmsg::Rcursor2 => "Rcursor2",
        Wsysmsg::Tctxt { .. } => "Tctxt",
        Wsysmsg::Rctxt => "Rctxt",
        Wsysmsg::Trdkbd4 => "Trdkbd4",
        Wsysmsg::Rrdkbd4 { .. } => "Rrdkbd4",
    }
}

/// Key fields per type, formatted for grep-ability. Long strings and data
/// segments are truncated.
fn fields(msg: &Wsysmsg) -> String {
    const MAX_STR: usize = 64;
    const MAX_DATA_HEX: usize = 16; // bytes -> 32 hex chars
    match msg {
        Wsysmsg::Rerror { error } => format!("error={}", quote(error, MAX_STR)),
        Wsysmsg::Trdmouse
        | Wsysmsg::Rmoveto
        | Wsysmsg::Rcursor
        | Wsysmsg::Rbouncemouse
        | Wsysmsg::Trdkbd
        | Wsysmsg::Rlabel
        | Wsysmsg::Rinit
        | Wsysmsg::Trdsnarf
        | Wsysmsg::Rwrsnarf
        | Wsysmsg::Ttop
        | Wsysmsg::Rtop
        | Wsysmsg::Rresize
        | Wsysmsg::Rcursor2
        | Wsysmsg::Rctxt
        | Wsysmsg::Trdkbd4 => String::new(),
        Wsysmsg::Rrdmouse { x, y, buttons, msec, resized } => {
            format!("x={x} y={y} buttons={buttons} msec={msec} resized={resized}")
        }
        Wsysmsg::Tmoveto { x, y } => format!("x={x} y={y}"),
        Wsysmsg::Tbouncemouse { x, y, buttons } => {
            format!("x={x} y={y} buttons={buttons}")
        }
        Wsysmsg::Tcursor { cursor } => {
            format!("arrow={} off={}", cursor.arrow, point(&cursor.offset))
        }
        Wsysmsg::Tcursor2 { cursor } => {
            format!(
                "arrow={} off={} off2={}",
                cursor.arrow,
                point(&cursor.offset),
                point(&cursor.offset2)
            )
        }
        Wsysmsg::Rrdkbd { rune } => format!("rune={rune}"),
        Wsysmsg::Rrdkbd4 { rune } => format!("rune={rune}"),
        Wsysmsg::Tlabel { label } => format!("label={}", quote(label, MAX_STR)),
        Wsysmsg::Tinit { winsize, label } => {
            format!("winsize={} label={}", quote(winsize, MAX_STR), quote(label, MAX_STR))
        }
        Wsysmsg::Rrdsnarf { snarf } | Wsysmsg::Twrsnarf { snarf } => {
            format!("snarf={}", quote(snarf, MAX_STR))
        }
        Wsysmsg::Trddraw { count } => format!("count={count}"),
        Wsysmsg::Rwrdraw { count } => format!("count={count}"),
        Wsysmsg::Rrddraw { data } | Wsysmsg::Twrdraw { data } => {
            format!("data={}", hex_trunc(data, MAX_DATA_HEX))
        }
        Wsysmsg::Tresize { rect } => format!("rect={}", rect_str(rect)),
        Wsysmsg::Tctxt { id } => format!("id={}", quote(id, MAX_STR)),
    }
}

fn quote(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        format!("\"{s}\"")
    } else {
        let head: String = s.chars().take(max).collect();
        format!("\"{head}...\"")
    }
}

fn point(p: &Point) -> String {
    format!("({},{})", p.x, p.y)
}

fn rect_str(r: &Rect) -> String {
    format!("{}-{}", point(&r.min), point(&r.max))
}

fn hex_trunc(data: &[u8], max: usize) -> String {
    let mut s: String = data.iter().take(max).map(|b| format!("{b:02x}")).collect();
    if data.len() > max {
        s.push_str("...");
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tinit_with_strings() {
        let msg = Wsysmsg::Tinit { winsize: "640x480".into(), label: "acme".into() };
        assert_eq!(
            frame_line("c2s", 1, &msg),
            "c2s tag=1 Tinit winsize=\"640x480\" label=\"acme\" (25B)"
        );
    }

    #[test]
    fn mouse_event_fields() {
        let msg = Wsysmsg::Rrdmouse { x: 100, y: 200, buttons: 4, msec: 12_345, resized: 1 };
        assert_eq!(
            frame_line("s2c", 7, &msg),
            "s2c tag=7 Rrdmouse x=100 y=200 buttons=4 msec=12345 resized=1 (23B)"
        );
    }

    #[test]
    fn empty_frames_have_no_fields() {
        assert_eq!(frame_line("c2s", 2, &Wsysmsg::Trdmouse), "c2s tag=2 Trdmouse (6B)");
        assert_eq!(frame_line("s2c", 9, &Wsysmsg::Rctxt), "s2c tag=9 Rctxt (6B)");
    }

    #[test]
    fn error_and_attach() {
        let msg = Wsysmsg::Rerror { error: "bad draw command".into() };
        assert_eq!(
            frame_line("s2c", 3, &msg),
            "s2c tag=3 Rerror error=\"bad draw command\" (26B)"
        );
        let msg = Wsysmsg::Tctxt { id: "wsys.42".into() };
        assert_eq!(frame_line("c2s", 1, &msg), "c2s tag=1 Tctxt id=\"wsys.42\" (17B)");
    }

    #[test]
    fn draw_data_hex_is_truncated() {
        let data: Vec<u8> = (0u8..=20).collect(); // 21 bytes
        let msg = Wsysmsg::Twrdraw { data };
        let line = frame_line("c2s", 5, &msg);
        // 16 bytes hex (32 chars), then "..."; frame = 6+4+21 = 31B.
        assert!(
            line.starts_with("c2s tag=5 Twrdraw data=000102030405060708090a0b0c0d0e0f... (31B)"),
            "{line}"
        );
    }

    #[test]
    fn rect_and_kbd4() {
        let msg = Wsysmsg::Tresize {
            rect: Rect { min: Point { x: 0, y: 0 }, max: Point { x: 640, y: 480 } },
        };
        assert_eq!(
            frame_line("c2s", 11, &msg),
            "c2s tag=11 Tresize rect=(0,0)-(640,480) (22B)"
        );
        let msg = Wsysmsg::Rrdkbd4 { rune: 0x1F600 };
        assert_eq!(frame_line("s2c", 6, &msg), "s2c tag=6 Rrdkbd4 rune=128512 (10B)");
    }
}
