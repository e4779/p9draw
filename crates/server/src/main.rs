//! p9draw-server — observation tooling for the plan9port devdraw wire
//! protocol (drawfcall; SPEC.md). std-only; all decoding goes through
//! p9draw-protocol, never hand-rolled.
//!
//! Subcommands (details in USAGE):
//! - `serve` — unix-socket accept loop; every incoming frame is decoded
//!   and logged (no replies: this is the observation half of the future
//!   full server, ARCHITECTURE.md).
//! - `capture` — MITM between a client and the real server unix socket;
//!   raw bytes are forwarded unchanged in both directions while frames
//!   are decoded, logged and dumped per direction (c2s.bin / s2c.bin).
//! - `capture-pipe` — the same MITM for the legacy pipe transport
//!   (SPEC.md §2.1): the real devdraw runs as a child behind our
//!   stdin/stdout, exactly where a plan9port client expects devdraw.

mod capture;
mod frameread;
mod logfmt;
mod net;
mod pump;
mod serve;

use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;

use pump::Logger;

const USAGE: &str = "\
p9draw-server — drawfcall traffic tools (SPEC.md)

usage:
  p9draw-server serve --socket PATH [--log-file PATH]
      accept loop on a unix socket; every incoming drawfcall frame is
      decoded and logged (observation only, no replies).

  p9draw-server capture --listen PATH --upstream PATH [--dump-dir DIR]
                        [--log-file PATH]
      MITM between a client and the real server socket: raw bytes are
      forwarded unchanged in both directions while every frame is decoded
      and logged; raw per-direction dumps go to DIR/c2s.bin, DIR/s2c.bin.

  p9draw-server capture-pipe [--dump-dir DIR] [--log-file PATH] -- CMD [ARGS...]
      the same MITM for the legacy pipe transport (SPEC.md 2.1): CMD (the
      real devdraw) runs as a child behind our stdin/stdout. Logs default
      to stderr here, because stdout carries the forwarded stream.
";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match run(&args) {
        Ok(code) => code,
        Err(e) => {
            eprintln!("p9draw-server: {e}");
            ExitCode::FAILURE
        }
    }
}

fn run(args: &[String]) -> Result<ExitCode, String> {
    let Some((sub, rest)) = args.split_first() else {
        return Err(format!("missing subcommand\n{USAGE}"));
    };
    match sub.as_str() {
        "serve" => cmd_serve(rest),
        "capture" => cmd_capture(rest),
        "capture-pipe" => cmd_capture_pipe(rest),
        "-h" | "--help" | "help" => {
            println!("{USAGE}");
            Ok(ExitCode::SUCCESS)
        }
        other => Err(format!("unknown subcommand {other:?}\n{USAGE}")),
    }
}

fn cmd_serve(args: &[String]) -> Result<ExitCode, String> {
    let mut socket = None;
    let mut log_file = None;
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--socket" => socket = Some(next_value(&mut it, "--socket")?),
            "--log-file" => log_file = Some(next_value(&mut it, "--log-file")?),
            other => return Err(format!("serve: unexpected argument {other:?}")),
        }
    }
    let socket = socket.ok_or("serve: --socket is required")?;
    let logger = make_logger(&log_file, Logger::stdout)?;
    serve::run_serve(&socket, logger).map_err(|e| format!("serve: {e}"))?;
    Ok(ExitCode::SUCCESS)
}

fn cmd_capture(args: &[String]) -> Result<ExitCode, String> {
    let mut listen = None;
    let mut upstream = None;
    let mut dump_dir = None;
    let mut log_file = None;
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--listen" => listen = Some(next_value(&mut it, "--listen")?),
            "--upstream" => upstream = Some(next_value(&mut it, "--upstream")?),
            "--dump-dir" => dump_dir = Some(next_value(&mut it, "--dump-dir")?),
            "--log-file" => log_file = Some(next_value(&mut it, "--log-file")?),
            other => return Err(format!("capture: unexpected argument {other:?}")),
        }
    }
    let listen = listen.ok_or("capture: --listen is required")?;
    let upstream = upstream.ok_or("capture: --upstream is required")?;
    let logger = make_logger(&log_file, Logger::stdout)?;
    let dumps = make_dumps(&dump_dir)?;
    capture::run_capture_unix(&listen, &upstream, dumps, logger)
        .map_err(|e| format!("capture: {e}"))?;
    Ok(ExitCode::SUCCESS)
}

fn cmd_capture_pipe(args: &[String]) -> Result<ExitCode, String> {
    let mut dump_dir = None;
    let mut log_file = None;
    let mut cmd: Vec<String> = Vec::new();
    let mut after_dashdash = false;
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        if after_dashdash {
            cmd.push(arg.clone());
            continue;
        }
        match arg.as_str() {
            "--dump-dir" => dump_dir = Some(next_value(&mut it, "--dump-dir")?),
            "--log-file" => log_file = Some(next_value(&mut it, "--log-file")?),
            "--" => after_dashdash = true,
            other => {
                return Err(format!(
                    "capture-pipe: unexpected argument {other:?} (the child command goes after `--`)"
                ))
            }
        }
    }
    // Default log target is stderr: stdout carries the forwarded stream.
    let logger = make_logger(&log_file, Logger::stderr)?;
    let dumps = make_dumps(&dump_dir)?;
    let status = capture::run_capture_pipe(&cmd, dumps, logger)
        .map_err(|e| format!("capture-pipe: {e}"))?;
    Ok(exit_code(status))
}

/// Map a child exit status onto our own exit code, so the devdraw wrapper
/// can `exec` us transparently (128+n for signal deaths).
fn exit_code(status: std::process::ExitStatus) -> ExitCode {
    use std::os::unix::process::ExitStatusExt;
    let code = status
        .code()
        .or_else(|| status.signal().map(|s| 128 + s))
        .unwrap_or(1);
    ExitCode::from(code as u8)
}

fn next_value<'a, I>(it: &mut I, flag: &str) -> Result<PathBuf, String>
where
    I: Iterator<Item = &'a String>,
{
    it.next()
        .map(PathBuf::from)
        .ok_or_else(|| format!("{flag} needs a value"))
}

fn make_logger(
    log_file: &Option<PathBuf>,
    default: fn() -> Logger,
) -> Result<Arc<Logger>, String> {
    match log_file {
        Some(p) => {
            let f = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(p)
                .map_err(|e| format!("cannot open log file {}: {e}", p.display()))?;
            Ok(Arc::new(Logger::file(f)))
        }
        None => Ok(Arc::new(default())),
    }
}

fn make_dumps(dir: &Option<PathBuf>) -> Result<capture::CaptureDumps, String> {
    match dir {
        Some(d) => capture::dump_files(d)
            .map_err(|e| format!("cannot create dumps in {}: {e}", d.display())),
        None => Ok(capture::CaptureDumps { c2s: None, s2c: None }),
    }
}
