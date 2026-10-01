//! Raw byte forwarding with parallel drawfcall decoding, logging and
//! optional per-direction dumps. One [`pump`] moves bytes in ONE
//! direction; callers run two pumps (on threads) to bridge a full-duplex
//! pair, and get the write half back on EOF for half-close semantics.

use std::io::{self, Read, Write};
use std::sync::{Arc, Mutex};

use p9draw_protocol::decode;

use crate::frameread::FrameAssembler;
use crate::logfmt::frame_line;

/// Where raw per-direction dumps go. `Memory` exists so tests can inspect
/// dumps without touching the filesystem; the CLI uses `File`.
pub enum DumpSink {
    File(std::fs::File),
    Memory(Arc<Mutex<Vec<u8>>>),
}

impl Clone for DumpSink {
    fn clone(&self) -> Self {
        match self {
            // dup(): both handles share the file offset, which is what a
            // multi-connection capture wants from one dump file.
            DumpSink::File(f) => DumpSink::File(f.try_clone().expect("dump file try_clone")),
            DumpSink::Memory(m) => DumpSink::Memory(Arc::clone(m)),
        }
    }
}

impl Write for DumpSink {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        match self {
            DumpSink::File(f) => f.write(buf),
            DumpSink::Memory(m) => {
                m.lock().expect("dump mutex poisoned").extend_from_slice(buf);
                Ok(buf.len())
            }
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        match self {
            DumpSink::File(f) => f.flush(),
            DumpSink::Memory(_) => Ok(()),
        }
    }
}

/// Destination for decoded-frame log lines: stdout for the socket modes,
/// stderr for `capture-pipe` (stdout carries the forwarded stream there),
/// a file with `--log-file`, memory in tests.
pub struct Logger {
    target: Mutex<LogTarget>,
}

enum LogTarget {
    Stdout,
    Stderr,
    File(std::fs::File),
    Memory(Arc<Mutex<Vec<u8>>>),
}

impl Logger {
    pub fn stdout() -> Self {
        Self { target: Mutex::new(LogTarget::Stdout) }
    }

    pub fn stderr() -> Self {
        Self { target: Mutex::new(LogTarget::Stderr) }
    }

    pub fn file(f: std::fs::File) -> Self {
        Self { target: Mutex::new(LogTarget::File(f)) }
    }

    #[cfg(test)]
    pub fn memory() -> (Self, Arc<Mutex<Vec<u8>>>) {
        let shared = Arc::new(Mutex::new(Vec::new()));
        (
            Self { target: Mutex::new(LogTarget::Memory(Arc::clone(&shared))) },
            shared,
        )
    }

    /// Write one timestamped (epoch ms) line. A failing log target must
    /// never take down a capture session, so errors are swallowed.
    pub fn log(&self, line: &str) {
        let ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0);
        let mut target = self.target.lock().expect("logger mutex poisoned");
        let res = match &mut *target {
            LogTarget::Stdout => {
                let stdout = io::stdout();
                writeln!(stdout.lock(), "{ms} {line}")
            }
            LogTarget::Stderr => {
                let stderr = io::stderr();
                writeln!(stderr.lock(), "{ms} {line}")
            }
            LogTarget::File(f) => writeln!(f, "{ms} {line}"),
            LogTarget::Memory(m) => {
                m.lock()
                    .expect("log mutex poisoned")
                    .extend_from_slice(format!("{ms} {line}\n").as_bytes());
                Ok(())
            }
        };
        let _ = res;
    }
}

/// Copy `r` -> `w` verbatim, mirroring every byte into `dump` and decoding
/// every complete drawfcall frame for the log. Flushes after each chunk
/// (latency matters more than throughput for a debugging tap).
///
/// Protocol errors never stop the forwarding: a payload that fails to
/// decode is only logged (framing stays intact); an impossible declared
/// size marks the stream as desynchronized and disables decoding for the
/// rest of this direction while raw bytes keep flowing.
///
/// Returns the total byte count and `w` back, so callers can perform a
/// socket write-half shutdown or drop a child stdin for EOF propagation.
pub fn pump<R: Read, W: Write>(
    mut r: R,
    mut w: W,
    dir: &'static str,
    mut dump: Option<DumpSink>,
    logger: Arc<Logger>,
) -> io::Result<(u64, W)> {
    let mut buf = vec![0u8; 64 * 1024];
    let mut assembler = FrameAssembler::new();
    let mut frames: Vec<Vec<u8>> = Vec::new();
    let mut decoding = true;
    let mut total: u64 = 0;

    loop {
        let n = r.read(&mut buf)?;
        if n == 0 {
            break; // EOF: orderly end of this direction
        }
        total += n as u64;
        w.write_all(&buf[..n])?;
        w.flush()?;
        if let Some(d) = dump.as_mut() {
            d.write_all(&buf[..n])?;
            d.flush()?;
        }
        if decoding {
            frames.clear();
            match assembler.feed(&buf[..n], &mut frames) {
                Ok(()) => {
                    for frame in &frames {
                        match decode(frame) {
                            Ok((tag, msg)) => logger.log(&frame_line(dir, tag, &msg)),
                            Err(e) => logger.log(&format!(
                                "{dir} UNDECODABLE frame ({} bytes): {e}",
                                frame.len()
                            )),
                        }
                    }
                }
                Err(e) => {
                    decoding = false;
                    logger.log(&format!(
                        "{dir} FRAMING DESYNC ({e}); forwarding raw bytes without decoding"
                    ));
                }
            }
        }
    }

    Ok((total, w))
}

#[cfg(test)]
mod tests {
    use super::*;
    use p9draw_protocol::{encode, Wsysmsg};

    fn sample_stream() -> Vec<u8> {
        let mut v = Vec::new();
        v.extend(encode(&Wsysmsg::Tctxt { id: "wsys.1".into() }, 1));
        v.extend(encode(&Wsysmsg::Rctxt, 1));
        v.extend(encode(&Wsysmsg::Twrdraw { data: b"J".to_vec() }, 2));
        v
    }

    #[test]
    fn forwards_verbatim_and_dumps() {
        let input = sample_stream();
        let (logger, logbuf) = Logger::memory();
        let dump_shared = Arc::new(Mutex::new(Vec::new()));
        let dest = Vec::new();
        let (total, dest) = pump(
            &input[..],
            dest,
            "c2s",
            Some(DumpSink::Memory(Arc::clone(&dump_shared))),
            Arc::new(logger),
        )
        .unwrap();
        assert_eq!(total, input.len() as u64);
        assert_eq!(dest, input, "forwarded bytes must be verbatim");
        assert_eq!(*dump_shared.lock().unwrap(), input, "dump must mirror input");
        let log = String::from_utf8(logbuf.lock().unwrap().clone()).unwrap();
        assert!(log.contains("Tctxt"), "log: {log}");
        assert!(log.contains("tag=1"), "log: {log}");
        assert!(log.contains("Twrdraw"), "log: {log}");
    }

    #[test]
    fn undecodable_frame_is_logged_and_forwarding_continues() {
        let input = sample_stream();
        let cut = encode(&Wsysmsg::Tctxt { id: "wsys.1".into() }, 1).len();
        // Well-formed size prefix (6), bogus type byte 0: decodable framing,
        // undecodable payload — must not disturb later frames.
        let bad = [0, 0, 0, 6, 9, 0];
        let mut spliced = input[..cut].to_vec();
        spliced.extend_from_slice(&bad);
        spliced.extend_from_slice(&input[cut..]);

        let (logger, logbuf) = Logger::memory();
        let dest = Vec::new();
        let (total, dest) = pump(&spliced[..], dest, "c2s", None, Arc::new(logger)).unwrap();
        assert_eq!(total, spliced.len() as u64);
        assert_eq!(dest, spliced);
        let log = String::from_utf8(logbuf.lock().unwrap().clone()).unwrap();
        assert!(log.contains("UNDECODABLE"), "log: {log}");
        // Framing stayed intact: frames after the bad one still decode.
        assert!(log.contains("Rctxt"), "log: {log}");
    }

    #[test]
    fn framing_desync_disables_decoder_but_forwards() {
        let mut garbage = sample_stream();
        // Declared size 2 < MIN_FRAME: framing is gone from here on.
        garbage[3] = 0x02;
        let (logger, logbuf) = Logger::memory();
        let dest = Vec::new();
        let (total, dest) = pump(&garbage[..], dest, "c2s", None, Arc::new(logger)).unwrap();
        assert_eq!(total, garbage.len() as u64);
        assert_eq!(dest, garbage, "raw forwarding must never stop");
        let log = String::from_utf8(logbuf.lock().unwrap().clone()).unwrap();
        assert!(log.contains("FRAMING DESYNC"), "log: {log}");
        // Nothing is decoded after the desync.
        assert!(!log.contains("Tctxt"), "log: {log}");
    }

    #[test]
    fn empty_input_yields_zero() {
        let (logger, _logbuf) = Logger::memory();
        let dest = Vec::new();
        let (total, dest) = pump(&[][..], dest, "s2c", None, Arc::new(logger)).unwrap();
        assert_eq!(total, 0);
        assert!(dest.is_empty());
    }
}
