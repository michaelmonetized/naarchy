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
    // Resolve runtime-directory aliases before connecting: sockaddr_un has a
    // 108-byte path limit even when a longer symlink names a valid socket.
    let base = socket_base(base);
    Some((base.join(".socket.sock"), base.join(".socket2.sock")))
}

fn socket_base(base: PathBuf) -> PathBuf {
    match base.canonicalize() {
        Ok(canonical) if canonical.as_os_str().len() < base.as_os_str().len() => canonical,
        _ => base,
    }
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
    stream
        .set_write_timeout(Some(Duration::from_millis(300)))
        .ok()?;
    use std::io::Write;
    stream.write_all(req.as_bytes()).ok()?;
    // JSON replies span multiple lines; read_line returned only "[" for monitors.
    let bytes = super::read_limited(stream, 1024 * 1024).ok()?;
    Some(String::from_utf8(bytes).ok()?.trim_end().to_string())
}

pub struct HyprlandHandle {
    #[allow(dead_code)]
    pub stop: std::sync::Arc<std::sync::atomic::AtomicBool>,
}

impl Drop for HyprlandHandle {
    fn drop(&mut self) {
        self.stop.store(true, std::sync::atomic::Ordering::Relaxed);
    }
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
            let mut last_fullscreen = None;
            while !stop2.load(std::sync::atomic::Ordering::Relaxed) {
                let Ok(stream) = UnixStream::connect(&ev) else {
                    std::thread::sleep(Duration::from_secs(2));
                    continue;
                };
                let _ = stream.set_read_timeout(Some(Duration::from_secs(1)));
                let mut reader = BufReader::new(stream);
                let mut line = String::new();
                refresh_fullscreen(&tx, &mut last_fullscreen);
                loop {
                    if stop2.load(std::sync::atomic::Ordering::Relaxed) {
                        return;
                    }
                    match reader.read_line(&mut line) {
                        Ok(0) => break,
                        Ok(_) => {}
                        Err(e)
                            if matches!(
                                e.kind(),
                                std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                            ) =>
                        {
                            continue
                        }
                        Err(_) => break,
                    }
                    if line.starts_with("activewindow>>") {
                        tx.send(Event::FocusLost);
                    } else if line.starts_with("monitoradded>>") {
                        if let Some(name) = line.split(">>").nth(1) {
                            tx.send(Event::MonitorAdded(name.trim().to_string()));
                        }
                    }
                    if [
                        "fullscreen>>",
                        "activewindow>>",
                        "workspace>>",
                        "focusedmon>>",
                        "activespecial>>",
                    ]
                    .iter()
                    .any(|event| line.starts_with(event))
                    {
                        // Events describe a window transition, not necessarily the
                        // active workspace. Query current state so leaving a full-
                        // screen workspace reliably restores the island.
                        refresh_fullscreen(&tx, &mut last_fullscreen);
                    }
                    line.clear();
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
            let mut opened_monitor = String::new();
            let mut monitors = monitor_boxes();
            let mut mon_tick = Instant::now();
            let mut last_y = 9999.0_f64;
            while !stop2.load(std::sync::atomic::Ordering::Relaxed) {
                if mon_tick.elapsed() > Duration::from_secs(8) {
                    let updated = monitor_boxes();
                    if !updated.is_empty() {
                        monitors = updated;
                    }
                    mon_tick = Instant::now();
                }
                let pos = request("cursorpos");
                match pos.and_then(|p| parse_pos(&p)) {
                    Some((x, y)) => {
                        let monitor = monitors.iter().find(|m| m.contains(x, y));
                        last_y = monitor.map(|m| y - m.y).unwrap_or(9999.0);
                        let in_open_strip = monitor
                            .map(|m| {
                                last_y <= band
                                    && (x - m.x - m.width * 0.5).abs() <= zone.pill_w * 0.5 + 16.0
                            })
                            .unwrap_or(false);
                        let on_surface = monitor
                            .map(|m| over_surface(x, y - m.y, m.x, m.width, &zone))
                            .unwrap_or(false);
                        if !sent_open {
                            if in_open_strip {
                                let since = *in_band_since.get_or_insert_with(Instant::now);
                                if since.elapsed() >= dwell {
                                    if let Some(monitor) = monitor {
                                        opened_monitor = monitor.name.clone();
                                        tx.send(Event::HoverOpen(monitor.name.clone()));
                                    }
                                    sent_open = true;
                                }
                            } else {
                                in_band_since = None;
                            }
                        } else if (in_open_strip || on_surface)
                            && monitor.is_some_and(|m| m.name == opened_monitor)
                        {
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
                    Duration::from_millis(300)
                };
                std::thread::sleep(idle);
            }
        });
    }

    HyprlandHandle { stop }
}

fn fullscreen_from_window(raw: &str) -> Option<bool> {
    let value: serde_json::Value = serde_json::from_str(raw).ok()?;
    let state = value.get("fullscreen");
    Some(state.and_then(|v| v.as_bool()).unwrap_or_else(|| {
        // Modern Hyprland: 0 none, 1 maximized, 2 fullscreen, 3 both.
        state
            .and_then(|v| v.as_u64())
            .is_some_and(|mode| mode & 2 != 0)
    }))
}

fn refresh_fullscreen(tx: &EventTx, last: &mut Option<bool>) {
    let Some(on) = request("j/activewindow").and_then(|raw| fullscreen_from_window(&raw)) else {
        return;
    };
    if *last != Some(on) {
        *last = Some(on);
        tx.send(Event::Fullscreen(on));
    }
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

struct MonitorBox {
    name: String,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
}

impl MonitorBox {
    fn contains(&self, x: f64, y: f64) -> bool {
        x >= self.x && x < self.x + self.width && y >= self.y && y < self.y + self.height
    }
}

fn monitor_boxes() -> Vec<MonitorBox> {
    request("j/monitors")
        .map(|raw| parse_monitors(&raw))
        .unwrap_or_default()
}

fn parse_monitors(raw: &str) -> Vec<MonitorBox> {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(raw) else {
        return vec![];
    };
    let Some(monitors) = value.as_array() else {
        return vec![];
    };
    monitors
        .iter()
        .filter_map(|m| {
            let scale = m.get("scale").and_then(json_f64).unwrap_or(1.0);
            if !scale.is_finite() || scale <= 0.0 {
                return None;
            }
            let mut width = json_f64(m.get("width")?)? / scale;
            let mut height = json_f64(m.get("height")?)? / scale;
            if m.get("transform").and_then(|v| v.as_u64()).unwrap_or(0) % 2 == 1 {
                std::mem::swap(&mut width, &mut height);
            }
            Some(MonitorBox {
                name: m
                    .get("name")
                    .and_then(|n| n.as_str())
                    .unwrap_or_default()
                    .to_string(),
                x: json_f64(m.get("x")?)?,
                y: json_f64(m.get("y")?)?,
                width,
                height,
            })
        })
        .collect()
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn socket_aliases_work_in_both_length_directions() {
        use std::os::unix::{fs::symlink, net::UnixListener};
        let temp = std::env::temp_dir().join(format!("naarchy-socket-path-{}", std::process::id()));
        std::fs::create_dir_all(&temp).unwrap();
        let long = temp.join("a".repeat(120));
        std::fs::create_dir(&long).unwrap();
        let short_alias = temp.join("short");
        symlink(&long, &short_alias).unwrap();
        let listener = UnixListener::bind(short_alias.join(".socket.sock")).unwrap();
        assert!(UnixStream::connect(socket_base(short_alias).join(".socket.sock")).is_ok());
        drop(listener);

        let short = temp.join("real");
        std::fs::create_dir(&short).unwrap();
        let long_alias = temp.join("b".repeat(120));
        symlink(&short, &long_alias).unwrap();
        let listener = UnixListener::bind(short.join(".socket.sock")).unwrap();
        assert!(UnixStream::connect(socket_base(long_alias).join(".socket.sock")).is_ok());
        drop(listener);
        std::fs::remove_dir_all(temp).unwrap();
    }

    #[test]
    fn fullscreen_state_distinguishes_maximized_and_empty_workspaces() {
        assert_eq!(fullscreen_from_window("{\"fullscreen\":2}"), Some(true));
        assert_eq!(fullscreen_from_window("{\"fullscreen\":1}"), Some(false));
        assert_eq!(fullscreen_from_window("{\"fullscreen\":true}"), Some(true));
        assert_eq!(fullscreen_from_window("{}"), Some(false));
        assert_eq!(fullscreen_from_window("not-json"), None);
    }

    #[test]
    fn monitor_hit_testing_uses_logical_pixels_and_offsets() {
        let monitors = parse_monitors("[\n {\"x\":1920,\"y\":-900,\"width\":3840,\"height\":2160,\"scale\":2},\n {\"x\":0,\"y\":0,\"width\":1920,\"height\":1080,\"scale\":1,\"transform\":1}\n]");
        assert_eq!(monitors.len(), 2);
        assert_eq!(monitors[0].width, 1920.0);
        assert!(monitors[0].contains(2880.0, -899.0));
        assert!(!monitors[0].contains(100.0, 0.0));
        assert_eq!(monitors[1].width, 1080.0);
        let zone = HoverZone {
            band_px: 8.0,
            pill_w: 200.0,
            pill_h: 30.0,
            panel_w: 500.0,
            panel_h: 350.0,
        };
        assert!(over_surface(
            2880.0,
            1.0,
            monitors[0].x,
            monitors[0].width,
            &zone
        ));
        assert!(!over_surface(
            2000.0,
            1.0,
            monitors[0].x,
            monitors[0].width,
            &zone
        ));
    }
}
