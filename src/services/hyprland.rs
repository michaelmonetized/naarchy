use std::io::{BufRead, BufReader};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use crate::services::{Event, EventTx};

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

/// Hit-test for stay-open while expanded: pill + panel, not the 8px open band.
pub struct HoverZone {
    pub band_px: f64,
    pub pill_w: f64,
    pub pill_h: f64,
    pub panel_w: f64,
    pub panel_h: f64,
}

/// Spawns the hover sampler + fullscreen/monitor event watcher.
/// Falls back silently on non-Hyprland compositors.
pub fn spawn(tx: EventTx, zone: HoverZone, hover_ms: u64, hover_open: bool) -> HyprlandHandle {
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
                        tx.send(Event::Fullscreen(on));
                    } else if line.starts_with("activewindow>>") {
                        tx.send(Event::FocusLost);
                    } else if line.starts_with("monitoradded>>") {
                        if let Some(name) = line.split(">>").nth(1) {
                            tx.send(Event::MonitorAdded(name.to_string()));
                        }
                    }
                }
                std::thread::sleep(Duration::from_millis(500));
            }
        });
    }

    // Hover sampler: top-edge dwell opens. Once open, stay until the
    // cursor leaves the pill *and* the panel — not the 8px open strip.
    if hover_open {
        let tx = tx.clone();
        let stop2 = stop.clone();
        std::thread::spawn(move || {
            let band = zone.band_px.max(1.0);
            let dwell = Duration::from_millis(hover_ms.clamp(60, 2000));
            let mut in_band_since: Option<Instant> = None;
            let mut sent_open = false;
            let mut mon = monitor_box();
            let mut mon_tick = Instant::now();
            let mut last_y = 9999.0_f64;
            while !stop2.load(std::sync::atomic::Ordering::Relaxed) {
                if mon_tick.elapsed() > Duration::from_secs(8) {
                    if let Some(m) = monitor_box() {
                        mon = Some(m);
                    }
                    mon_tick = Instant::now();
                }
                let pos = request("cursorpos");
                match pos.and_then(|p| parse_pos(&p)) {
                    Some((x, y)) => {
                        last_y = y;
                        let in_open_strip = y <= band;
                        let on_surface = mon
                            .map(|(mx, mw)| over_surface(x, y, mx, mw, &zone))
                            .unwrap_or(false);
                        if !sent_open {
                            if in_open_strip {
                                let since = *in_band_since.get_or_insert_with(Instant::now);
                                if since.elapsed() >= dwell {
                                    tx.send(Event::HoverOpen);
                                    sent_open = true;
                                }
                            } else {
                                in_band_since = None;
                            }
                        } else if in_open_strip || on_surface {
                            in_band_since = None;
                        } else {
                            in_band_since = None;
                            tx.send(Event::HoverEnd);
                            sent_open = false;
                        }
                    }
                    None => {
                        in_band_since = None;
                    }
                }
                let idle = if last_y <= 80.0 {
                    Duration::from_millis(40)
                } else {
                    Duration::from_millis(140)
                };
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

fn json_f64(v: &serde_json::Value) -> Option<f64> {
    v.as_f64()
        .or_else(|| v.as_i64().map(|i| i as f64))
        .or_else(|| v.as_u64().map(|i| i as f64))
}

/// Focused (or first) monitor origin and width, for centering the panel hit-test.
fn monitor_box() -> Option<(f64, f64)> {
    let raw = request("j/monitors")?;
    let v: serde_json::Value = serde_json::from_str(&raw).ok()?;
    let arr = v.as_array()?;
    let m = arr
        .iter()
        .find(|m| m.get("focused").and_then(|f| f.as_bool()) == Some(true))
        .or(arr.first())?;
    let x = json_f64(m.get("x")?)?;
    let w = json_f64(m.get("width")?)?;
    Some((x, w))
}

fn over_surface(x: f64, y: f64, mon_x: f64, mon_w: f64, zone: &HoverZone) -> bool {
    if y < 0.0 {
        return false;
    }
    let lx = x - mon_x;
    let cx = mon_w * 0.5;
    let in_x = |half: f64| (lx - cx).abs() <= half + 16.0;
    (y <= zone.pill_h && in_x(zone.pill_w * 0.5)) || (y <= zone.panel_h && in_x(zone.panel_w * 0.5))
}
