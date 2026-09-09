//! Private, bounded local command transport and process lifetime lock.
use crate::services::Verb;
use std::fs::{File, OpenOptions};
use std::io::{self, BufRead, BufReader, Read, Write};
use std::os::unix::fs::{FileTypeExt, OpenOptionsExt, PermissionsExt};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{mpsc, Arc};
use std::time::Duration;

const MAX_COMMAND_BYTES: usize = 64 * 1024;
const MAX_CLIENTS: usize = 16;

pub fn socket_path() -> PathBuf {
    crate::util::runtime_dir().join("naarchy.sock")
}

pub struct Server {
    listener: UnixListener,
    path: PathBuf,
    // The lock remains held until after the socket is removed in Drop.
    _lock: File,
}

impl Server {
    pub fn bind() -> io::Result<Option<Self>> {
        Self::bind_at(&socket_path())
    }

    fn bind_at(path: &Path) -> io::Result<Option<Self>> {
        let lock = OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .mode(0o600)
            .open(path.with_extension("lock"))?;
        match lock.try_lock() {
            Ok(()) => {}
            Err(std::fs::TryLockError::WouldBlock) => return Ok(None),
            Err(std::fs::TryLockError::Error(error)) => return Err(error),
        }
        // Also recognize older daemons that predate the lock file.
        if UnixStream::connect(path).is_ok() {
            return Ok(None);
        }
        match std::fs::symlink_metadata(path) {
            Ok(meta) if meta.file_type().is_socket() => std::fs::remove_file(path)?,
            Ok(_) => {
                return Err(io::Error::new(
                    io::ErrorKind::AlreadyExists,
                    "IPC path is not a socket",
                ))
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }
        let listener = UnixListener::bind(path)?;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
        Ok(Some(Self {
            listener,
            path: path.into(),
            _lock: lock,
        }))
    }

    pub fn listen(&self, tx: mpsc::Sender<Verb>) {
        let listener = self.listener.try_clone().expect("clone IPC listener");
        std::thread::spawn(move || {
            let active = Arc::new(AtomicUsize::new(0));
            for stream in listener.incoming() {
                let Ok(stream) = stream else { continue };
                if active.load(Ordering::Relaxed) >= MAX_CLIENTS {
                    continue;
                }
                active.fetch_add(1, Ordering::Relaxed);
                let active = active.clone();
                let tx = tx.clone();
                std::thread::spawn(move || {
                    let _ = receive(stream, &tx);
                    active.fetch_sub(1, Ordering::Relaxed);
                });
            }
        });
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

fn receive(mut stream: UnixStream, tx: &mpsc::Sender<Verb>) -> io::Result<()> {
    stream.set_read_timeout(Some(Duration::from_millis(750)))?;
    stream.set_write_timeout(Some(Duration::from_millis(750)))?;
    let mut line = Vec::new();
    BufReader::new((&stream).take((MAX_COMMAND_BYTES + 1) as u64)).read_until(b'\n', &mut line)?;
    if line.len() > MAX_COMMAND_BYTES || line.last() != Some(&b'\n') {
        stream.write_all(b"error: incomplete or oversized command\n")?;
        return Ok(());
    }
    match serde_json::from_slice::<Verb>(&line) {
        Ok(verb) if valid_verb(&verb) => {
            if tx.send(verb).is_ok() {
                crate::services::wake_ui();
                stream.write_all(b"ok\n")?;
            }
        }
        _ => stream.write_all(b"error: invalid command\n")?,
    }
    Ok(())
}

fn valid_verb(verb: &Verb) -> bool {
    match verb {
        Verb::Timer(seconds) => (1..=crate::ui::timer::MAX_SECS).contains(seconds),
        Verb::Tab(name) => crate::ui::Tab::from_cli(name).is_some(),
        Verb::Hud { value, step, .. } => value.iter().chain(step.iter()).all(|v| v.is_finite()),
        Verb::ShelfAdd(paths) => !paths.is_empty() && paths.len() <= 1024,
        _ => true,
    }
}

pub fn send(verb: &Verb) -> Result<(), String> {
    let mut stream = UnixStream::connect(socket_path())
        .map_err(|_| "daemon not running (start with: naarchy run)".to_string())?;
    stream
        .set_write_timeout(Some(Duration::from_secs(2)))
        .map_err(|e| e.to_string())?;
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .map_err(|e| e.to_string())?;
    let mut payload = serde_json::to_vec(verb).map_err(|e| e.to_string())?;
    payload.push(b'\n');
    if payload.len() > MAX_COMMAND_BYTES {
        return Err("command exceeds 64 KiB; add fewer files at a time".into());
    }
    stream
        .write_all(&payload)
        .map_err(|e| format!("cannot send command: {e}"))?;
    let mut reply = String::new();
    // Older daemons close without an acknowledgment; retain compatibility.
    BufReader::new(stream.take(256))
        .read_line(&mut reply)
        .map_err(|e| format!("daemon did not acknowledge command: {e}"))?;
    if reply.is_empty() || reply.trim() == "ok" {
        Ok(())
    } else {
        Err(reply.trim().into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_unsafe_timer_and_unknown_tab() {
        assert!(!valid_verb(&Verb::Timer(0)));
        assert!(!valid_verb(&Verb::Timer(u64::MAX)));
        assert!(valid_verb(&Verb::Timer(1500)));
        assert!(!valid_verb(&Verb::Tab("missing".into())));
    }

    #[test]
    fn exclusive_lock_and_cleanup() {
        let path = std::env::temp_dir().join(format!("naarchy-ipc-{}.sock", std::process::id()));
        let server = Server::bind_at(&path).unwrap().unwrap();
        assert!(Server::bind_at(&path).unwrap().is_none());
        assert_eq!(
            std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        drop(server);
        assert!(!path.exists());
        let replacement = Server::bind_at(&path).unwrap().unwrap();
        drop(replacement);
        let _ = std::fs::remove_file(path.with_extension("lock"));
    }

    #[test]
    fn invalid_json_is_not_queued() {
        let (mut client, server) = UnixStream::pair().unwrap();
        let (tx, rx) = mpsc::channel();
        client.write_all(b"not-json\n").unwrap();
        receive(server, &tx).unwrap();
        assert!(rx.try_recv().is_err());
        let mut reply = String::new();
        client.read_to_string(&mut reply).unwrap();
        assert!(reply.contains("invalid command"));
    }
}
