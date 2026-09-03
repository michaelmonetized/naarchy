use crate::services::{Event, EventTx, RawClip};
use std::io::Read;
use std::time::Duration;
use wl_clipboard_rs::paste::{self as wpaste};

fn read_current() -> Option<RawClip> {
    // Prefer images, then text.
    for mime in ["image/png", "text/plain;charset=utf-8", "text/plain"] {
        let attempt = wpaste::get_contents(
            wpaste::ClipboardType::Regular,
            wpaste::Seat::Unspecified,
            wpaste::MimeType::Specific(mime),
        );
        if let Ok((mut pipe, _actual)) = attempt {
            let mut buf = Vec::new();
            if pipe.read_to_end(&mut buf).is_ok() && !buf.is_empty() {
                return Some(RawClip {
                    mime: mime.to_string(),
                    data: buf,
                });
            }
        }
    }
    None
}

fn cheap_hash(c: &RawClip) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    c.mime.hash(&mut h);
    c.data.len().hash(&mut h);
    let n = c.data.len();
    let head = n.min(256);
    c.data[..head].hash(&mut h);
    if n > 256 {
        c.data[n - 256..].hash(&mut h);
    }
    h.finish()
}

/// Polls the Wayland clipboard and reports genuinely-new content.
pub fn spawn(tx: EventTx) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        let mut last_hash: u64 = 0;

        if let Some(clip) = read_current() {
            last_hash = cheap_hash(&clip);
            tx.send(Event::ClipNew(clip));
        }

        loop {
            std::thread::sleep(Duration::from_millis(1500));
            let Some(clip) = read_current() else { continue };
            let hv = cheap_hash(&clip);
            if hv != last_hash {
                last_hash = hv;
                tx.send(Event::ClipNew(clip));
            }
        }
    })
}
