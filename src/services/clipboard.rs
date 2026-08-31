use crate::services::{Event, RawClip};
use std::io::Read;
use std::sync::mpsc::Sender;
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

/// Polls the Wayland clipboard and reports genuinely-new content.
pub fn spawn(tx: Sender<Event>) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        use std::hash::{Hash, Hasher};

        fn hash_clip(c: &RawClip) -> u64 {
            let mut h = std::collections::hash_map::DefaultHasher::new();
            c.mime.hash(&mut h);
            c.data.hash(&mut h);
            h.finish()
        }

        let mut last_hash: u64 = 0;

        // Initial snapshot
        if let Some(clip) = read_current() {
            last_hash = hash_clip(&clip);
            let _ = tx.send(Event::ClipNew(clip));
        }

        loop {
            std::thread::sleep(Duration::from_millis(900));
            let Some(clip) = read_current() else { continue };
            let hv = hash_clip(&clip);
            if hv != last_hash {
                last_hash = hv;
                let _ = tx.send(Event::ClipNew(clip));
            }
        }
    })
}
