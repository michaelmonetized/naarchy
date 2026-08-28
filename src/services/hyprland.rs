use std::io::{BufRead, BufReader};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::sync::mpsc::Sender;
use std::time::{Duration, Instant};

use crate::services::Event;

fn hypr_dirs() -> Option<(PathBuf, PathBuf)> {
    let sig = std::env::var("HYPRLAND_INSTANCE_SIGNATURE").ok()?;
    let base: PathBuf = std::env::var("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(format!("/run/user/{}", unsafe { uid() })))
        .join("hypr")
        .join(&sig);
    Some((base.join(".socket.sock"), base.join(".socket2.sock")))
}

unsafe fn uid() -> u32 {
    extern "C" {
        fn getuid() -> u32;
    }
    getuid()
}

pub fn available() -> bool {
    hypr_dirs().map(|(a, _)| a.exists()).unwrap_or(false)
}

/// One-shot IPC request (e.g. "cursorpos", "monitors", "activewindow").
pub fn request(req: &str) -> Option<String> {
    let (cmd, _) = hypr_dirs()?;
    let mut stream = UnixStream::connect(cmd).ok()?;
    stream
        .set_read_timeout(Some(Duration::from_millis(300)))
        .ok()?;
    use std::io::Write;
    stream.write_all(req.as_bytes()).ok()?;
    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    reader.read_line(&mut line).ok()?;
    Some(line.trim_end().to_string())
}

pub struct HyprlandHandle {
    #[allow(dead_code)]
    pub stop: std::sync::Arc<std::sync::atomic::AtomicBool>,
}

/// Spawns the hover sampler + fullscreen/monitor event watcher.
/// Falls back silently on non-Hyprland compositors.
pub fn spawn(
    tx: Sender<Event>,
    hover_band_px: i32,
    hover_ms: u64,
    hover_open: bool,
) -> HyprlandHandle {
    let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));

    if !available() {
        return HyprlandHandle { stop };
    }

    // Event watcher thread (socket2): fullscreen + monitor hotplug
    {
        let tx = tx.clone();
        let stop2 = stop.clone();
        std::thread::spawn(move || {
            let Some((_, ev)) = hypr_dirs() else { return };
            while !stop2.load(std::sync::atomic::Ordering::Relaxed) {
                let Ok(stream) = UnixStream::connect(&ev) else {
                    std::thread::sleep(Duration::from_secs(2));
                    continue;
                };
                let reader = BufReader::new(stream);
                for line in reader.lines() {
                    if stop2.load(std::sync::atomic::Ordering::Relaxed) {
                        return;
                    }
                    let Ok(line) = line else { break };
                    if let Some(rest) = line.strip_prefix("fullscreen>>") {
                        // "0"/"1"/"2" — any nonzero means a window is fullscreen
                        let on = rest.trim() != "0";
                        let _ = tx.send(Event::Fullscreen(on));
                    } else if line.starts_with("activewindow>>") {
                        // A regular window grabbed focus — not our layer surfaces.
                        let _ = tx.send(Event::FocusLost);
                    } else if line.starts_with("monitoradded>>") {
                        if let Some(name) = line.split(">>").nth(1) {
                            let _ = tx.send(Event::MonitorAdded(name.to_string()));
                        }
                    }
                }
                std::thread::sleep(Duration::from_millis(500));
            }
        });
    }

    // Hover sampler thread: sample cursorpos, dwell → HoverOpen
    if hover_open {
        let tx = tx.clone();
        let stop2 = stop.clone();
        std::thread::spawn(move || {
            let band = hover_band_px.max(1) as f64;
            let dwell = Duration::from_millis(hover_ms.clamp(60, 2000));
            let mut in_band_since: Option<Instant> = None;
            let mut sent_open = false;
            let idle = Duration::from_millis(33);
            while !stop2.load(std::sync::atomic::Ordering::Relaxed) {
                let pos = request("cursorpos");
                match pos.and_then(|p| parse_pos(&p)) {
                    Some((x, y)) => {
                        if y <= band {
                            let since = *in_band_since.get_or_insert_with(Instant::now);
                            if !sent_open && since.elapsed() >= dwell {
                                let _ = tx.send(Event::HoverOpen);
                                sent_open = true;
                            }
                            let _ = x; // full-width band for now
                        } else {
                            in_band_since = None;
                            if sent_open {
                                // pointer left the band; tell UI to consider collapse
                                let _ = tx.send(Event::HoverEnd);
                                sent_open = false;
                            }
                        }
                    }
                    None => {
                        in_band_since = None;
                    }
                }
                std::thread::sleep(idle);
            }
        });
    }

    HyprlandHandle { stop }
}

fn parse_pos(s: &str) -> Option<(f64, f64)> {
    let (x, y) = s.split_once(',')?;
    Some((x.trim().parse().ok()?, y.trim().parse().ok()?))
}
