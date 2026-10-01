//! Small unix-socket helpers shared by the `serve` and `capture` modes.

use std::io;
use std::os::unix::net::UnixListener;
use std::path::Path;

/// Bind `path`, removing a stale socket file first (a leftover from a
/// previous run would fail bind with EADDRINUSE otherwise).
pub fn bind_listener(path: &Path) -> io::Result<UnixListener> {
    match std::fs::remove_file(path) {
        Ok(()) => {}
        Err(e) if e.kind() == io::ErrorKind::NotFound => {}
        Err(e) => return Err(e),
    }
    UnixListener::bind(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bind_removes_stale_socket_files() {
        let dir = std::env::temp_dir().join(format!("p9draw-net-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("s.sock");

        // Even a plain regular file at the socket path must be removed.
        std::fs::write(&path, b"stale").unwrap();
        let l1 = bind_listener(&path).unwrap();
        drop(l1);

        // Leftover socket file from the previous bind must go too.
        let l2 = bind_listener(&path).unwrap();
        drop(l2);

        let _ = std::fs::remove_dir_all(&dir);
    }
}
