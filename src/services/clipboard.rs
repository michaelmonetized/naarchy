use crate::services::{Event, EventTx, RawClip};
use std::io::Read;
use std::os::fd::AsRawFd;
use std::time::{Duration, Instant};
use wl_clipboard_rs::paste::{self as wpaste};

fn read_current() -> Option<RawClip> {
    // Prefer images, then text.
    for mime in ["image/png", "text/plain;charset=utf-8", "text/plain"] {
        let attempt = wpaste::get_contents(
            wpaste::ClipboardType::Regular,
            wpaste::Seat::Unspecified,
            wpaste::MimeType::Specific(mime),
        );
        if let Ok((mut pipe, actual)) = attempt {
            if let Ok(buf) = read_clipboard(&mut pipe) {
                if buf.is_empty() {
                    continue;
                }
                return Some(RawClip {
                    mime: actual,
                    data: buf,
                });
            }
        }
    }
    None
}

/// Clipboard owners can stop writing without closing their pipe. Poll with a
/// total deadline so one broken owner cannot permanently stop clipboard history.
fn read_clipboard(reader: &mut (impl Read + AsRawFd)) -> std::io::Result<Vec<u8>> {
    #[repr(C)]
    struct PollFd {
        fd: i32,
        events: i16,
        revents: i16,
    }
    extern "C" {
        fn poll(fds: *mut PollFd, count: usize, timeout_ms: i32) -> i32;
    }
    let deadline = Instant::now() + Duration::from_secs(2);
    let mut bytes = Vec::new();
    let mut chunk = [0u8; 16 * 1024];
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err(std::io::ErrorKind::TimedOut.into());
        }
        let mut fd = PollFd {
            fd: reader.as_raw_fd(),
            events: 1,
            revents: 0,
        };
        // `fd` is live for the call; count matches the one-element allocation.
        let ready = unsafe { poll(&mut fd, 1, remaining.as_millis().max(1) as i32) };
        if ready < 0 {
            let error = std::io::Error::last_os_error();
            if error.kind() == std::io::ErrorKind::Interrupted {
                continue;
            }
            return Err(error);
        }
        if ready == 0 {
            return Err(std::io::ErrorKind::TimedOut.into());
        }
        let count = match reader.read(&mut chunk) {
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            result => result?,
        };
        if count == 0 {
            return Ok(bytes);
        }
        if bytes.len() + count > 8 * 1024 * 1024 {
            return Err(std::io::ErrorKind::InvalidData.into());
        }
        bytes.extend_from_slice(&chunk[..count]);
    }
}

fn content_hash(c: &RawClip) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    c.mime.hash(&mut h);
    c.data.hash(&mut h);
    h.finish()
}

/// Polls the Wayland clipboard and reports genuinely-new content.
pub fn spawn(tx: EventTx) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        let mut last_hash: u64 = 0;

        if let Some(clip) = read_current() {
            last_hash = content_hash(&clip);
            tx.send(Event::ClipNew(clip));
        }

        loop {
            std::thread::sleep(Duration::from_millis(1500));
            let Some(clip) = read_current() else { continue };
            let hv = content_hash(&clip);
            if hv != last_hash {
                last_hash = hv;
                tx.send(Event::ClipNew(clip));
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::os::unix::net::UnixStream;

    #[test]
    fn detects_edits_between_identical_head_and_tail() {
        let first = RawClip {
            mime: "text/plain".into(),
            data: vec![b'a'; 1024],
        };
        let mut second = first.clone();
        second.data[512] = b'b';
        assert_ne!(content_hash(&first), content_hash(&second));
    }

    #[test]
    fn reads_complete_pipe_and_rejects_stalled_owner() {
        let (mut reader, mut writer) = UnixStream::pair().unwrap();
        writer.write_all(b"clipboard").unwrap();
        writer.shutdown(std::net::Shutdown::Write).unwrap();
        assert_eq!(read_clipboard(&mut reader).unwrap(), b"clipboard");
        let (mut reader, _writer) = UnixStream::pair().unwrap();
        assert_eq!(
            read_clipboard(&mut reader).unwrap_err().kind(),
            std::io::ErrorKind::TimedOut
        );
    }
}
