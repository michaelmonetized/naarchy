pub mod calendar;
pub mod clipboard;
pub mod hyprland;
pub mod mpris;
pub mod notifd;
pub mod settings;
pub mod upower;

use serde::{Deserialize, Serialize};

/// Events flowing from async services (tokio side) into the GTK main loop.
#[derive(Debug, Clone)]
pub enum Event {
    Media(Option<MediaState>),
    Battery(BatteryState),
    SchemeDark(bool),
    HoverOpen,
    HoverEnd,
    /// A regular window was activated (another app took focus) — panels
    /// should consider collapsing back to the notch.
    FocusLost,
    Fullscreen(bool),
    MonitorAdded(String),
    ClipNew(RawClip),
    Notify(Banner),
    ConfigChanged(Box<crate::config::Config>),
    /// ICS feeds were refreshed; today's meetings lived in shared.cal_events.
    CalendarReload,
}

/// Fresh content observed on the Wayland clipboard.
#[derive(Debug, Clone)]
pub struct RawClip {
    pub mime: String,
    pub data: Vec<u8>,
}

#[derive(Debug, Clone, Default)]
#[allow(dead_code)]
pub struct MediaState {
    pub bus: String,
    pub player: String, // friendly name
    pub title: String,
    pub artist: String,
    pub album: String,
    pub art_url: Option<String>,
    pub art_path: Option<String>, // resolved local cache path (set by service)
    pub playing: bool,
    pub length_us: i64,
    pub position_us: i64,
    pub shuffle: bool,
    pub repeat: u8, // 0 off 1 track 2 playlist
    pub track_id: String,
    pub can_play: bool,
    pub can_next: bool,
    pub can_prev: bool,
    pub can_seek: bool,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct BatteryState {
    pub percent: f64,
    pub charging: bool,
    pub present: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClipEntry {
    pub id: String,
    pub kind: ClipKind,
    pub mime: String,
    #[serde(default)]
    pub preview: String,
    /// Inline bytes for text; blob ref for images (data_ref holds file name).
    #[serde(default)]
    pub text: String,
    #[serde(default)]
    pub data_ref: String,
    pub at: u64,
    pub pinned: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ClipKind {
    Text,
    Image,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct Banner {
    pub id: u32,
    pub app_name: String,
    pub icon: String,
    pub summary: String,
    pub body: String,
    pub actions: Vec<(String, String)>,
    pub urgency: u8, // 0 low 1 normal 2 critical
}

/// Verbs sent from CLI/IPC into the running instance.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Verb {
    Toggle,
    Expand,
    Collapse,
    Tab(String),
    Hud {
        kind: String,
        value: Option<f64>,
        step: Option<f64>,
        icon: Option<String>,
        label: Option<String>,
    },
    ShelfAdd(Vec<String>),
    ShelfClear,
    ShelfRemove(String),
    ClipboardPasteLast,
    Timer(u64),
    TimerStop,
    Notify {
        summary: String,
        body: String,
    },
    Quit,
}
