use serde::Deserialize;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::Duration;

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct Appearance {
    pub theme: String, // "auto" | "dark" | "light"
    /// Accent color; `None` follows the active omarchy theme accent.
    pub accent: Option<String>,
    /// Capsule background color; `None` follows the omarchy background.
    pub pill_bg: Option<String>,
    /// Shelf background tint; `None` follows the omarchy background.
    pub bg: Option<String>,
    /// Foreground color; `None` follows the omarchy foreground.
    pub fg: Option<String>,
    /// Follow the active omarchy theme colors when set (default true).
    pub omarchy: bool,
    /// Dock icon glyph font. `None` discovers the desktop font, falling
    /// back to "JetBrainsMono Nerd Font".
    pub icon_font: Option<String>,
    pub radius: i32,
    /// true = hug a physical notch (narrow pill) on the MacBook display
    pub notch_mode: bool,
    /// pixels below the top edge for the pill (0 = flush)
    pub margin_top: i32,
    pub pill_width_notch: i32,
    pub pill_width_island: i32,
    pub panel_width: i32,
    pub panel_height: i32,
    pub opacity: f64,
}

impl Default for Appearance {
    fn default() -> Self {
        Self {
            theme: "auto".into(),
            accent: None,
            pill_bg: None,
            bg: None,
            fg: None,
            omarchy: true,
            icon_font: None,
            radius: 24,
            notch_mode: false,
            margin_top: 0,
            pill_width_notch: 190,
            pill_width_island: 370,
            panel_width: 680,
            panel_height: 460,
            opacity: 0.98,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct Behavior {
    pub hover_open: bool,
    pub hover_ms: u64,
    pub hover_band_px: i32,
    pub collapse_on_leave_ms: u64,
    pub hide_fullscreen: bool,
    pub monitors: MonitorSel,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum MonitorSel {
    All(String), // "all" | "primary"
    List(Vec<String>),
}

impl Default for MonitorSel {
    fn default() -> Self {
        MonitorSel::All("all".into())
    }
}

impl MonitorSel {
    pub fn wants(&self, name: &str, primary: bool) -> bool {
        match self {
            MonitorSel::All(s) if s == "primary" => primary,
            MonitorSel::All(_) => true,
            MonitorSel::List(names) => names.iter().any(|n| n == name),
        }
    }
}

impl Default for Behavior {
    fn default() -> Self {
        Self {
            hover_open: true,
            hover_ms: 180,
            hover_band_px: 8,
            collapse_on_leave_ms: 180,
            hide_fullscreen: true,
            monitors: MonitorSel::default(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct Features {
    pub media: bool,
    pub shelf: bool,
    pub clipboard: bool,
    pub calendar: bool,
    pub timer: bool,
    pub notifications: bool,
    pub battery_chip: bool,
}

impl Default for Features {
    fn default() -> Self {
        Self {
            media: true,
            shelf: true,
            clipboard: true,
            calendar: true,
            timer: true,
            notifications: false,
            battery_chip: true,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct ClipboardCfg {
    pub max_entries: usize,
    pub max_image_bytes: usize,
}

impl Default for ClipboardCfg {
    fn default() -> Self {
        Self {
            max_entries: 80,
            max_image_bytes: 8 * 1024 * 1024,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct HudCfg {
    pub timeout_ms: u64,
}

impl Default for HudCfg {
    fn default() -> Self {
        Self { timeout_ms: 1400 }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct ClockCfg {
    pub format: String,
    pub show_in_pill: bool,
}

impl Default for ClockCfg {
    fn default() -> Self {
        Self {
            format: "%H:%M".into(),
            show_in_pill: false,
        }
    }
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default)]
pub struct CalendarCfg {
    /// Public iCloud or Google Calendar ICS feed URLs. Fetched periodically.
    pub feeds: Vec<String>,
    pub refresh_min: u64,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default)]
pub struct Config {
    pub appearance: Appearance,
    pub behavior: Behavior,
    pub features: Features,
    pub clipboard: ClipboardCfg,
    pub hud: HudCfg,
    pub clock: ClockCfg,
    pub calendar: CalendarCfg,
}

impl Config {
    pub fn load(path: &Path) -> Config {
        match std::fs::read_to_string(path) {
            Ok(s) => Self::from_toml(&s).unwrap_or_else(|| {
                log::warn!("config parse error in {:?}; using defaults", path);
                Config::default()
            }),
            Err(_) => Config::default(),
        }
    }

    pub(crate) fn from_toml(s: &str) -> Option<Config> {
        let mut cfg: Config = toml::from_str(s).ok()?;
        cfg.normalize();
        Some(cfg)
    }

    /// Keep the loaded config in a canonical state. Legacy configs carried an
    /// `accent = "#7aa2f7"` default meant as "no override"; treat that as
    /// unspecified on purpose so the omarchy theme accent wins.
    fn normalize(&mut self) {
        if self.appearance.omarchy && self.appearance.accent.as_deref() == Some("#7aa2f7") {
            self.appearance.accent = None;
        }
    }

    pub fn save_default_if_missing(path: &Path) {
        if path.exists() {
            // migrate: ensure [calendar] exists for discoverability (old 0.1 installs)
            if let Ok(content) = std::fs::read_to_string(path) {
                if !content.contains("[calendar]") {
                    let snippet = "\n[calendar]\nfeeds = []            # public iCloud / Google ICS feed URLs (one per line)\n# feeds = [\"https://calendar.google.com/calendar/ical/xxxx/basic.ics\"]\nrefresh_min = 5\n";
                    let _ = std::fs::write(path, format!("{}{}", content.trim_end(), snippet));
                }
            }
            return;
        }
        let _ = std::fs::create_dir_all(path.parent().unwrap_or(Path::new("/")));
        let default = r##"# naarchy configuration — hot-reloads on save
[appearance]
theme = "auto"          # auto | dark | light
omarchy = true          # pull accent/background/foreground from the active omarchy theme
                        # (set false to use the values below)
# accent = "#89b4fa"    # uncomment to override the theme accent
# pill_bg = "#000000"   # capsule color (default: solid black / omarchy background)
# bg = "rgba(0,0,0,0.62)"
# fg = "#cdd6f4"
# icon_font = "JetBrainsMono Nerd Font"   # dock icon glyphs
radius = 24
notch_mode = false        # true = hug a physical notch (~190px pill)
# pill_width_notch = 190
# pill_width_island = 370
# margin_top = 0            # pixels below the top edge (0 = flush)
panel_width = 680
panel_height = 460
opacity = 0.98

[behavior]
hover_open = true
hover_ms = 180
hover_band_px = 8
collapse_on_leave_ms = 180
hide_fullscreen = true
monitors = "all"        # all | primary

[features]
media = true
shelf = true
clipboard = true
calendar = true
timer = true
notifications = false   # own org.freedesktop.Notifications (leave false to keep mako/dunst)
battery_chip = true

[clipboard]
max_entries = 80
max_image_bytes = 8388608

[hud]
timeout_ms = 1400

[clock]
format = "%H:%M"
show_in_pill = false     # the bar already has a clock

[calendar]
feeds = []            # public iCloud / Google ICS feed URLs (one per line)
refresh_min = 5
"##;
        let _ = std::fs::write(path, default);
    }
}

/// Watches the config file for changes and sends a fresh Config each time.
pub struct ConfigWatcher {
    #[allow(dead_code)]
    stop: std::sync::Arc<std::sync::atomic::AtomicBool>,
}

impl ConfigWatcher {
    pub fn spawn(path: PathBuf, tx: mpsc::Sender<Config>) -> Self {
        let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let stop2 = stop.clone();
        std::thread::spawn(move || {
            let mut last_content: Option<String> = None;
            loop {
                if stop2.load(std::sync::atomic::Ordering::Relaxed) {
                    break;
                }
                let content = std::fs::read_to_string(&path).unwrap_or_default();
                let changed = last_content.as_deref() != Some(content.as_str());
                if changed && !content.is_empty() {
                    last_content = Some(content.clone());
                    let cfg = Config::from_toml(&content).unwrap_or_else(|| {
                        log::warn!("config reload parse error");
                        Config::load(&path)
                    });
                    if tx.send(cfg).is_err() {
                        break;
                    }
                }
                std::thread::sleep(Duration::from_millis(1100));
            }
        });
        Self { stop }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_notifications_off() {
        assert!(!Config::default().features.notifications);
    }

    #[test]
    fn from_toml_defaults_and_unknown_keys() {
        let cfg = Config::from_toml("surprise = 1\n[appearance]\nradius = 12\n").unwrap();
        assert_eq!(cfg.appearance.radius, 12);
        assert!(!cfg.features.notifications);
        assert!(cfg.features.media);
    }

    #[test]
    fn normalize_strips_legacy_accent() {
        let cfg = Config::from_toml(
            r##"
[appearance]
omarchy = true
accent = "#7aa2f7"
"##,
        )
        .unwrap();
        assert!(cfg.appearance.accent.is_none());
    }

    #[test]
    fn monitors_primary_and_list() {
        let p = Config::from_toml("[behavior]\nmonitors = \"primary\"\n").unwrap();
        match p.behavior.monitors {
            MonitorSel::All(s) => assert_eq!(s, "primary"),
            _ => panic!("expected All"),
        }
        let l = Config::from_toml("[behavior]\nmonitors = [\"DP-1\", \"HDMI-A-1\"]\n").unwrap();
        match l.behavior.monitors {
            MonitorSel::List(names) => assert_eq!(names, vec!["DP-1", "HDMI-A-1"]),
            _ => panic!("expected List"),
        }
        assert!(MonitorSel::List(vec!["DP-1".into()]).wants("DP-1", false));
        assert!(!MonitorSel::List(vec!["DP-1".into()]).wants("HDMI-A-1", true));
        assert!(MonitorSel::All("primary".into()).wants("whatever", true));
        assert!(!MonitorSel::All("primary".into()).wants("whatever", false));
    }
}
