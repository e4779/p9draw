//! `serve` subcommand: a unix-socket accept loop that decodes and logs
//! every incoming drawfcall frame. This is the observation half of the
//! future p9draw server (ARCHITECTURE.md): no dispatch and no replies yet —
//! traffic is logged and dropped into a null sink.

use std::io;
use std::path::Path;
use std::sync::Arc;
use std::thread;

use crate::net::bind_listener;
use crate::pump::{pump, DumpSink, Logger};

/// Listen on `socket` and log one line per decoded frame per connection,
/// until the process is killed.
pub fn run_serve(socket: &Path, logger: Arc<Logger>) -> io::Result<()> {
    let listener = bind_listener(socket)?;
    logger.log(&format!(
        "serve: listening on {} (decode-only, no replies)",
        socket.display()
    ));
    for (i, conn) in listener.incoming().enumerate() {
        let conn = conn?;
        let n = i + 1;
        logger.log(&format!("serve: connection #{n} accepted"));
        let logger = Arc::clone(&logger);
        thread::spawn(move || {
            // Forward into a null sink: the point is decode+log, not reply.
            match pump(conn, io::sink(), "c2s", None::<DumpSink>, Arc::clone(&logger)) {
                Ok((bytes, _sink)) => {
                    logger.log(&format!("serve: connection #{n} closed after {bytes} bytes"))
                }
                Err(e) => logger.log(&format!("serve: connection #{n} error: {e}")),
            }
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use p9draw_protocol::{encode, Wsysmsg};
    use std::io::Write;
    use std::os::unix::net::UnixStream;
    use std::sync::Mutex;
    use std::time::Duration;

    fn wait_for_log(logbuf: &Arc<Mutex<Vec<u8>>>, needle: &str) -> String {
        for _ in 0..500 {
            let snapshot = String::from_utf8(logbuf.lock().unwrap().clone()).unwrap();
            if snapshot.contains(needle) {
                return snapshot;
            }
            thread::sleep(Duration::from_millis(10));
        }
        panic!("log never contained {needle:?}");
    }

    #[test]
    fn serve_logs_decoded_frames_per_connection() {
        let dir = std::env::temp_dir().join(format!("p9draw-serve-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let sock = dir.join("srv.sock");

        let (logger, logbuf) = Logger::memory();
        let l = Arc::new(logger);
        let s = sock.clone();
        // Detached on purpose: run_serve loops until the process exits.
        thread::spawn(move || run_serve(&s, l));

        let mut conn = None;
        for _ in 0..200 {
            match UnixStream::connect(&sock) {
                Ok(c) => {
                    conn = Some(c);
                    break;
                }
                Err(_) => thread::sleep(Duration::from_millis(10)),
            }
        }
        let mut conn = conn.expect("serve socket never came up");

        let frame = encode(
            &Wsysmsg::Tinit { winsize: "640x480".into(), label: "acme".into() },
            3,
        );
        conn.write_all(&frame).unwrap();
        drop(conn); // EOF -> the serve pump finishes and logs the close

        let log = wait_for_log(&logbuf, "Tinit");
        assert!(log.contains("winsize=\"640x480\""), "log: {log}");
        assert!(log.contains("connection #1 accepted"), "log: {log}");

        let _ = std::fs::remove_dir_all(&dir);
    }
}
