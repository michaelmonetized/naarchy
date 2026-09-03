pub mod calendar;
pub mod clipboard;
pub mod hyprland;
pub mod location;
pub mod mpris;
pub mod notifd;
pub mod settings;

use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::OnceLock;

/// GTK main context, installed once the UI thread is up. Workers wake it
/// instead of the UI spinning a 60 Hz poll.
static GTK_CTX: OnceLock<gtk4::glib::MainContext> = OnceLock::new();
static WAKE_PENDING: AtomicBool = AtomicBool::new(false);

pub fn install_wake(ctx: gtk4::glib::MainContext) {
    let _ = GTK_CTX.set(ctx);
}

pub fn clear_wake_pending() {
    WAKE_PENDING.store(false, Ordering::Release);
}

/// Coalesce bursts of service events into one GTK idle drain.
pub fn wake_ui() {
    if GTK_CTX.get().is_none() {
        return;
    }
    if WAKE_PENDING.swap(true, Ordering::AcqRel) {
        return;
    }
    if let Some(ctx) = GTK_CTX.get() {
        ctx.invoke(|| {
            gtk4::glib::idle_add_local_once(|| {
                crate::app::pump_once();
            });
        });
    }
}

/// Thread-safe event outlet. `send` wakes the GTK loop.
#[derive(Clone)]
pub struct EventTx {
    inner: mpsc::Sender<Event>,
}

impl EventTx {
    pub fn pair() -> (Self, mpsc::Receiver<Event>) {
        let (tx, rx) = mpsc::channel();
        (Self { inner: tx }, rx)
    }

    pub fn send(&self, ev: Event) {
        if self.inner.send(ev).is_ok() {
            wake_ui();
        }
    }
}

/// Events flowing from async services (tokio side) into the GTK main loop.
#[derive(Debug, Clone)]
pub enum Event {
    Media(Option<MediaState>),
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
    /// Enriched with travel times (directions + leave label)
    CalendarEnriched(Vec<crate::services::calendar::CalEvent>),
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

impl MediaState {
    /// True when any MPRIS player is showing a real track.
    ///
    /// Playing always counts. Paused still counts if title or artist is set.
    /// A Stopped Chromium leftover (no title, Chrome icon as art) does not —
    /// that is "nothing playing", not a dead transport row.
    ///
    /// Arguments: none (uses `playing`, `title`, `artist`).
    ///
    /// Returns: whether the Home media widget should show transport.
    pub fn is_live(&self) -> bool {
        self.playing || !self.title.trim().is_empty() || !self.artist.trim().is_empty()
    }
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

#[cfg(test)]
mod tests {
    use super::MediaState;

    #[test]
    fn hollow_chromium_is_not_live() {
        let st = MediaState {
            bus: "org.mpris.MediaPlayer2.chromium.instance1".into(),
            player: "chromium".into(),
            playing: false,
            art_url: Some("file:///tmp/.org.chromium.Chromium.J6SAdm".into()),
            ..Default::default()
        };
        assert!(!st.is_live());
    }

    #[test]
    fn paused_track_is_live() {
        let st = MediaState {
            title: "Royalty".into(),
            artist: "Måneskin".into(),
            playing: false,
            player: "chromium".into(),
            ..Default::default()
        };
        assert!(st.is_live());
    }

    #[test]
    fn playing_without_metadata_is_live() {
        let st = MediaState {
            playing: true,
            player: "mpv".into(),
            ..Default::default()
        };
        assert!(st.is_live());
    }

    #[test]
    fn whitespace_title_is_not_live() {
        let st = MediaState {
            title: "   ".into(),
            player: "firefox".into(),
            ..Default::default()
        };
        assert!(!st.is_live());
    }
}
