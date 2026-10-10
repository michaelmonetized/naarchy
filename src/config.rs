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
    /// Minimize spatial animation while preserving all interaction feedback.
    pub reduce_motion: bool,
    /// Small local-calendar costumes on October 31.
    pub halloween: bool,
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
            reduce_motion: false,
            halloween: true,
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
    /// Start plugin packages from ~/.config/naarchy/plugins (restart to apply).
    pub plugins: bool,
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
            plugins: true,
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

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct CalendarCfg {
    /// Public iCloud or Google Calendar ICS feed URLs. Fetched periodically.
    pub feeds: Vec<String>,
    pub refresh_min: u64,
    /// Opt in to sending event addresses to geocoding/routing providers.
    pub travel_times: bool,
}

impl Default for CalendarCfg {
    fn default() -> Self {
        Self {
            feeds: Vec::new(),
            refresh_min: 5,
            travel_times: false,
        }
    }
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
    /// Apply only changed preferences, keeping unknown/advanced settings intact.
    pub fn save_patch(changes: &[(&str, &str, toml::Value)]) -> Result<(), String> {
        Self::save_patch_at(&crate::util::config_file(), changes)
    }

    fn save_patch_at(path: &Path, changes: &[(&str, &str, toml::Value)]) -> Result<(), String> {
        let source = match std::fs::read_to_string(path) {
            Ok(source) => source,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => String::new(),
            Err(error) => return Err(format!("Cannot read preferences: {error}")),
        };
        let mut document: toml::Table = toml::from_str(&source)
            .map_err(|error| format!("Fix the existing configuration before saving: {error}"))?;
        for (section, key, value) in changes {
            let table = document
                .entry(*section)
                .or_insert_with(|| toml::Value::Table(toml::Table::new()));
            let table = table
                .as_table_mut()
                .ok_or_else(|| format!("[{section}] must be a table"))?;
            table.insert((*key).into(), value.clone());
        }
        let text = toml::to_string_pretty(&document).map_err(|e| e.to_string())?;
        Self::from_toml(&text).ok_or("These preferences are not valid")?;
        crate::util::atomic_write_private(path, text.as_bytes())
            .map_err(|e| format!("Cannot save preferences: {e}"))
    }

    pub fn load(path: &Path) -> Result<Config, String> {
        match std::fs::read_to_string(path) {
            Ok(source) => Self::from_toml(&source).ok_or_else(|| {
                format!(
                    "invalid configuration at {}; fix it before starting Naarchy",
                    path.display()
                )
            }),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Config::default()),
            Err(error) => Err(format!(
                "cannot read configuration at {}: {error}",
                path.display()
            )),
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
        let a = &mut self.appearance;
        if a.omarchy && a.accent.as_deref() == Some("#7aa2f7") {
            a.accent = None;
        }
        a.radius = a.radius.clamp(0, 64);
        a.margin_top = a.margin_top.clamp(0, 256);
        a.pill_width_notch = a.pill_width_notch.clamp(64, 800);
        a.pill_width_island = a.pill_width_island.clamp(64, 800);
        a.panel_width = a.panel_width.clamp(480, 1200);
        a.panel_height = a.panel_height.clamp(400, 1000);
        a.opacity = if a.opacity.is_finite() {
            a.opacity.clamp(0.1, 1.0)
        } else {
            0.98
        };
        if !matches!(a.theme.as_str(), "auto" | "light" | "dark") {
            a.theme = "auto".into();
        }
        self.behavior.hover_ms = self.behavior.hover_ms.clamp(50, 2000);
        self.behavior.hover_band_px = self.behavior.hover_band_px.clamp(1, 64);
        self.behavior.collapse_on_leave_ms = self.behavior.collapse_on_leave_ms.clamp(100, 10_000);
        self.clipboard.max_entries = self.clipboard.max_entries.min(2000);
        self.clipboard.max_image_bytes =
            self.clipboard.max_image_bytes.clamp(1024, 32 * 1024 * 1024);
        self.hud.timeout_ms = self.hud.timeout_ms.clamp(300, 30_000);
        self.calendar.refresh_min = self.calendar.refresh_min.clamp(1, 1440);
    }

    pub fn save_default_if_missing(path: &Path) {
        // Never rewrite an existing user config during startup.
        if path.exists() {
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
reduce_motion = false
halloween = true

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
plugins = true          # run installed packages from ~/.config/naarchy/plugins (see naarchy plugin list)

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
travel_times = false   # optional: IP location + event addresses sent to routing providers
"##;
        if let Err(error) = crate::util::atomic_write_private(path, default.as_bytes()) {
            log::warn!("cannot create configuration: {error}");
        }
    }
}

/// Watches the config file for changes and sends a fresh Config each time.
pub struct ConfigWatcher {
    stop: std::sync::Arc<std::sync::atomic::AtomicBool>,
}

impl ConfigWatcher {
    pub fn spawn(path: PathBuf, tx: mpsc::Sender<Config>) -> Self {
        let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let stop2 = stop.clone();
        std::thread::spawn(move || {
            let mut last_stamp = None;
            let mut last_content = std::fs::read_to_string(&path).ok();
            loop {
                if stop2.load(std::sync::atomic::Ordering::Relaxed) {
                    break;
                }
                let stamp = std::fs::metadata(&path)
                    .ok()
                    .and_then(|m| m.modified().ok().map(|t| (t, m.len())));
                if stamp != last_stamp {
                    last_stamp = stamp;
                    if let Ok(content) = std::fs::read_to_string(&path) {
                        if last_content.as_deref() != Some(&content) {
                            // Remember invalid contents to avoid repeated log spam.
                            last_content = Some(content.clone());
                            match Config::from_toml(&content) {
                                Some(cfg) => {
                                    if tx.send(cfg).is_err() {
                                        break;
                                    }
                                }
                                None => log::warn!(
                                    "config reload rejected; keeping last valid settings"
                                ),
                            }
                        }
                    }
                }
                std::thread::sleep(Duration::from_millis(1100));
            }
        });
        Self { stop }
    }
}

impl Drop for ConfigWatcher {
    fn drop(&mut self) {
        self.stop.store(true, std::sync::atomic::Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loaded_config_bounds_expensive_and_invalid_values() {
        let cfg = Config::from_toml("[appearance]\npanel_width = -20\nopacity = nan\n[clipboard]\nmax_entries = 999999999\n[behavior]\nhover_ms = 0\n").unwrap();
        assert_eq!(cfg.appearance.panel_width, 480);
        assert!(cfg.appearance.opacity.is_finite());
        assert_eq!(cfg.clipboard.max_entries, 2000);
        assert_eq!(cfg.behavior.hover_ms, 50);
        assert!(!cfg.calendar.travel_times);
    }

    #[test]
    fn preferences_preserve_unknown_keys_and_reject_invalid_existing_config() {
        let path =
            std::env::temp_dir().join(format!("naarchy-config-test-{}.toml", std::process::id()));
        std::fs::write(&path, "custom = 42\n[appearance]\naccent = \"#abcdef\"\n").unwrap();
        Config::save_patch_at(
            &path,
            &[("appearance", "reduce_motion", toml::Value::Boolean(true))],
        )
        .unwrap();
        let value: toml::Value = toml::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(value["custom"].as_integer(), Some(42));
        assert_eq!(value["appearance"]["accent"].as_str(), Some("#abcdef"));
        std::fs::write(&path, "[broken").unwrap();
        assert!(Config::load(&path).is_err());
        assert!(Config::save_patch_at(
            &path,
            &[("appearance", "reduce_motion", toml::Value::Boolean(false))]
        )
        .is_err());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "[broken");
        std::fs::remove_file(path).unwrap();
    }

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
