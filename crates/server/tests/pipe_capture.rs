//! End-to-end: `p9draw-server capture-pipe` must forward a synthetic
//! drawfcall stream between our stdio and a child process unchanged,
//! decoding it on the way and writing raw per-direction dumps.

use std::io::{Read, Write};
use std::process::{Command, Stdio};

use p9draw_protocol::{encode, Wsysmsg};

#[test]
fn capture_pipe_forwards_decodes_and_dumps() {
    let dir = std::env::temp_dir().join(format!("p9draw-pipe-test-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);

    let mut child = Command::new(env!("CARGO_BIN_EXE_p9draw-server"))
        .args([
            "capture-pipe",
            "--dump-dir",
            dir.to_str().expect("utf-8 temp dir"),
            "--",
            "cat", // echo child: whatever we send comes back on stdout
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn p9draw-server capture-pipe");

    let tctxt = encode(&Wsysmsg::Tctxt { id: "wsys.1".into() }, 1);
    let tinit = encode(
        &Wsysmsg::Tinit { winsize: "640x480".into(), label: "acme".into() },
        2,
    );
    let mut sent = tctxt.clone();
    sent.extend_from_slice(&tinit);

    child
        .stdin
        .take()
        .expect("piped stdin")
        .write_all(&sent)
        .unwrap();
    // ChildStdin dropped above -> EOF -> `cat` exits -> stdout EOF.

    let mut out = Vec::new();
    child
        .stdout
        .take()
        .expect("piped stdout")
        .read_to_end(&mut out)
        .unwrap();
    let mut stderr = Vec::new();
    child
        .stderr
        .take()
        .expect("piped stderr")
        .read_to_end(&mut stderr)
        .unwrap();
    let status = child.wait().unwrap();

    assert!(status.success(), "capture-pipe failed: {status}");
    assert_eq!(out, sent, "raw bytes must survive the MITM unchanged");

    let c2s = std::fs::read(dir.join("c2s.bin")).unwrap();
    let s2c = std::fs::read(dir.join("s2c.bin")).unwrap();
    assert_eq!(c2s, sent, "c2s dump must hold the raw client->child bytes");
    assert_eq!(s2c, sent, "s2c dump must hold the raw child->client bytes");

    let log = String::from_utf8(stderr).unwrap();
    assert!(log.contains("Tctxt"), "decoded log: {log}");
    assert!(log.contains("Tinit"), "decoded log: {log}");
    assert!(log.contains("winsize=\"640x480\""), "decoded log: {log}");

    let _ = std::fs::remove_dir_all(&dir);
}
