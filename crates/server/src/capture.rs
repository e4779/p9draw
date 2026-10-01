//! MITM capture: forward raw bytes between a plan9port client and the
//! real devdraw while decoding and logging the drawfcall stream, and
//! (optionally) writing raw per-direction dumps (c2s.bin / s2c.bin).
//!
//! Two transports, because clients reach devdraw two ways (SPEC.md §2.1):
//! - server mode (`$wsysid`): the client dials `unix!$NAMESPACE/name`
//!   directly → [`run_capture_unix`] bridges listen-socket ↔ real-server
//!   socket;
//! - legacy mode (default): the client fork+execs devdraw and speaks over
//!   stdin/stdout pipes → [`run_capture_pipe`] bridges our stdio ↔ the
//!   real binary as a child process.

use std::io::{self, Write};
use std::net::Shutdown;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::Path;
use std::process::{Command, ExitStatus, Stdio};
use std::sync::Arc;
use std::thread;

use crate::net::bind_listener;
use crate::pump::{pump, DumpSink, Logger};

/// Pair of per-direction dump sinks.
pub struct CaptureDumps {
    pub c2s: Option<DumpSink>,
    pub s2c: Option<DumpSink>,
}

/// Create `c2s.bin` / `s2c.bin` inside `dir` (created if missing).
pub fn dump_files(dir: &Path) -> io::Result<CaptureDumps> {
    std::fs::create_dir_all(dir)?;
    let open = |name: &str| -> io::Result<Option<DumpSink>> {
        let f = std::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(dir.join(name))?;
        Ok(Some(DumpSink::File(f)))
    };
    Ok(CaptureDumps {
        c2s: open("c2s.bin")?,
        s2c: open("s2c.bin")?,
    })
}

/// Socket MITM: accept clients on `listen`, connect one upstream
/// (`upstream`) per client, and bridge each pair until both ends close.
pub fn run_capture_unix(
    listen: &Path,
    upstream: &Path,
    dumps: CaptureDumps,
    logger: Arc<Logger>,
) -> io::Result<()> {
    let listener = bind_listener(listen)?;
    logger.log(&format!(
        "capture: listening on {} -> {} (Ctrl-C to stop)",
        listen.display(),
        upstream.display()
    ));
    for (i, conn) in listener.incoming().enumerate() {
        let conn = conn?;
        let n = i + 1;
        let ups = match UnixStream::connect(upstream) {
            Ok(u) => u,
            Err(e) => {
                logger.log(&format!(
                    "capture: conn #{n}: upstream connect failed: {e}; client dropped"
                ));
                continue;
            }
        };
        logger.log(&format!("capture: conn #{n}: bridged"));
        let dumps = CaptureDumps {
            c2s: dumps.c2s.clone(),
            s2c: dumps.s2c.clone(),
        };
        let bridge_logger = Arc::clone(&logger);
        let err_logger = Arc::clone(&logger);
        thread::spawn(move || {
            if let Err(e) = bridge_sockets(&conn, &ups, dumps, bridge_logger) {
                err_logger.log(&format!("capture: conn #{n}: bridge error: {e}"));
            }
        });
    }
    Ok(())
}

/// Full-duplex bridge over a socket pair with half-close propagation: EOF
/// in one direction shuts the other side's write half down, so the real
/// server (and the client) observe the same EOFs they would see without
/// the MITM in between.
pub fn bridge_sockets(
    client: &UnixStream,
    upstream: &UnixStream,
    dumps: CaptureDumps,
    logger: Arc<Logger>,
) -> io::Result<()> {
    let CaptureDumps { c2s: c2s_dump, s2c: s2c_dump } = dumps;
    let c_read = client.try_clone()?;
    let u_write = upstream.try_clone()?;
    let c2s_logger = Arc::clone(&logger);
    let c2s = thread::spawn(move || -> io::Result<()> {
        let (_, w) = pump(c_read, u_write, "c2s", c2s_dump, c2s_logger)?;
        w.shutdown(Shutdown::Write)
    });

    let s2c = pump(
        upstream.try_clone()?,
        client.try_clone()?,
        "s2c",
        s2c_dump,
        logger,
    );

    let c2s_res = c2s.join().map_err(|_| {
        io::Error::new(io::ErrorKind::Other, "c2s pump thread panicked")
    })?;
    s2c.map(|_| ())?;
    c2s_res
}

/// Legacy-transport MITM: run `cmd` (the real devdraw) as a child with our
/// stdin/stdout piped through the decoder. After the client's
/// `pipe()+fork()+dup2(0,1)+execl` we sit exactly where real devdraw sat;
/// raw bytes are forwarded unchanged both ways.
///
/// The logger default here must be stderr — stdout carries the forwarded
/// stream (main.rs picks the default; `--log-file` overrides).
pub fn run_capture_pipe(
    cmd: &[String],
    dumps: CaptureDumps,
    logger: Arc<Logger>,
) -> io::Result<ExitStatus> {
    let Some((bin, args)) = cmd.split_first() else {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "capture-pipe requires a command after `--`",
        ));
    };
    let mut child = Command::new(bin)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()?;
    let stdin = child.stdin.take().expect("child stdin was piped");
    let stdout = child.stdout.take().expect("child stdout was piped");

    let c2s_logger = Arc::clone(&logger);
    let _c2s = thread::spawn(move || -> io::Result<()> {
        let (_, mut sink) = pump(io::stdin(), stdin, "c2s", dumps.c2s, c2s_logger)?;
        sink.flush()?;
        Ok(())
        // Dropping `sink` (the child's stdin) delivers the client's EOF to
        // the child — the same half-close a direct pipe would produce.
    });

    // Child -> client on this thread. If the child dies while the client is
    // only blocked on reads (no stdin traffic), the c2s thread would block
    // forever on our stdin; killing the (already dying or dead) child and
    // returning turns the client's next read into the EOF it expects.
    let s2c = pump(stdout, io::stdout(), "s2c", dumps.s2c, logger);
    let _ = child.kill(); // no-op on the normal path (child already exiting)
    let status = child.wait()?;
    s2c.map(|_| ())?;
    // The c2s thread is deliberately not joined: on the normal path it has
    // already finished; on the child-died path it is blocked on our stdin
    // and dies with the process — which is exactly the EOF propagation the
    // client expects when devdraw goes away.
    Ok(status)
}

#[cfg(test)]
mod tests {
    use super::*;
    use p9draw_protocol::{decode, encode, Wsysmsg};
    use std::io::Read;
    use std::sync::Mutex;

    #[test]
    fn dump_files_use_canonical_names() {
        let dir = std::env::temp_dir().join(format!("p9draw-dump-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        {
            let _dumps = dump_files(&dir).unwrap();
        } // files persist after the sinks are dropped
        assert!(dir.join("c2s.bin").is_file());
        assert!(dir.join("s2c.bin").is_file());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Bridging a socket pair forwards bytes verbatim both ways, decodes
    /// both directions, fills the per-direction dumps, and propagates a
    /// client half-close to the upstream side.
    #[test]
    fn bridge_forwards_decodes_and_dumps() {
        let (mut client_end, mitm_client) = UnixStream::pair().unwrap();
        let (mitm_upstream, mut upstream_end) = UnixStream::pair().unwrap();

        let c2s_dump = Arc::new(Mutex::new(Vec::new()));
        let s2c_dump = Arc::new(Mutex::new(Vec::new()));
        let (logger, logbuf) = Logger::memory();

        let c2s_for_mitm = Arc::clone(&c2s_dump);
        let s2c_for_mitm = Arc::clone(&s2c_dump);
        let mitm = thread::spawn(move || {
            bridge_sockets(
                &mitm_client,
                &mitm_upstream,
                CaptureDumps {
                    c2s: Some(DumpSink::Memory(c2s_for_mitm)),
                    s2c: Some(DumpSink::Memory(s2c_for_mitm)),
                },
                Arc::new(logger),
            )
        });

        // C->S: attach request crosses the bridge byte-identically.
        let tctxt = encode(&Wsysmsg::Tctxt { id: "wsys.7".into() }, 1);
        client_end.write_all(&tctxt).unwrap();
        let mut buf = vec![0u8; tctxt.len()];
        upstream_end.read_exact(&mut buf).unwrap();
        assert_eq!(buf, tctxt);

        // S->C: reply crosses back.
        let rctxt = encode(&Wsysmsg::Rctxt, 1);
        upstream_end.write_all(&rctxt).unwrap();
        let mut buf2 = vec![0u8; rctxt.len()];
        client_end.read_exact(&mut buf2).unwrap();
        assert_eq!(buf2, rctxt);

        // Client half-close must propagate: the upstream read side sees EOF.
        client_end.shutdown(Shutdown::Write).unwrap();
        let mut eof = [0u8; 1];
        assert_eq!(
            upstream_end.read(&mut eof).unwrap(),
            0,
            "upstream must see EOF after client half-close"
        );
        // Closing the upstream write side lets the bridge finish.
        upstream_end.shutdown(Shutdown::Write).unwrap();
        mitm.join().unwrap().expect("bridge io");

        assert_eq!(*c2s_dump.lock().unwrap(), tctxt);
        assert_eq!(*s2c_dump.lock().unwrap(), rctxt);
        let log = String::from_utf8(logbuf.lock().unwrap().clone()).unwrap();
        assert!(log.contains("Tctxt"), "log: {log}");
        assert!(log.contains("Rctxt"), "log: {log}");
        // Decoded stream sanity: the frames we pushed through are real.
        let (_, msg) = decode(&tctxt).unwrap();
        assert_eq!(msg, Wsysmsg::Tctxt { id: "wsys.7".into() });
    }
}
