pub mod calendar;
pub mod clipview;
pub mod drawer;
pub mod home;
pub mod hud;
pub mod liquid;
pub mod media;
pub mod motion;
pub mod panel;
pub mod pill;
pub mod shelfview;
pub mod timer;

/// Nerd Font glyphs used as UI chrome (not content). One family, one size
/// scale — never mix emoji into the chrome.
#[allow(dead_code)]
pub mod g {
    pub const HOME: &str = "\u{f015}";
    pub const INBOX: &str = "\u{f01c}";
    pub const CLIP: &str = "\u{f0ea}";
    pub const GRID: &str = "\u{f00a}";
    pub const CAL: &str = "\u{f133}";
    pub const PLAY: &str = "\u{f04b}";
    pub const PAUSE: &str = "\u{f04c}";
    pub const PREV: &str = "\u{f048}";
    pub const NEXT: &str = "\u{f051}";
    pub const SHUFFLE: &str = "\u{f074}";
    pub const REPEAT: &str = "\u{f01e}";
    pub const CHEV_L: &str = "\u{f053}";
    pub const CHEV_R: &str = "\u{f054}";
    pub const CLOCK: &str = "\u{f017}";
    pub const MUSIC: &str = "\u{f001}";
    pub const CHECK: &str = "\u{f00c}";
    pub const FOLDER: &str = "\u{f07b}";
    pub const IMAGE: &str = "\u{f03e}";
    pub const FILE: &str = "\u{f15b}";
    pub const TEXT: &str = "\u{f0f6}";
    pub const SETTINGS: &str = "\u{f013}";
    pub const PLUS: &str = "\u{f067}";
}

use crate::clip_store::ClipStore;
use crate::config::Config;
use crate::services::{BatteryState, MediaState};
use crate::shelf_store::ShelfStore;
use crate::widget_store::WidgetStore;
use gtk4::gdk;
use gtk4::prelude::*;
use gtk4::Orientation;
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::mpsc::Sender;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tab {
    Home,
    Inbox,
    Clipboard,
    Widgets,
    Calendar,
}

impl Tab {
    pub fn label(self) -> &'static str {
        match self {
            Tab::Home => "Home",
            Tab::Inbox => "Inbox",
            Tab::Clipboard => "Clip",
            Tab::Widgets => "Widgets",
            Tab::Calendar => "Calendar",
        }
    }
    pub fn all() -> [Tab; 5] {
        [
            Tab::Home,
            Tab::Inbox,
            Tab::Clipboard,
            Tab::Widgets,
            Tab::Calendar,
        ]
    }

    /// Parse a CLI tab name.
    ///
    /// Accepts the dock names and the cheap aliases (`shelf` → Inbox, `clip` →
    /// Clipboard, etc.). Unknown names return `None`.
    ///
    /// Arguments:
    /// - `s`: token after `naarchy tab`
    ///
    /// Returns: the tab, or `None` if the name is not recognized.
    pub fn from_cli(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "home" | "start" => Some(Tab::Home),
            "inbox" | "files" | "shelf" | "drops" => Some(Tab::Inbox),
            "clipboard" | "clip" => Some(Tab::Clipboard),
            "widgets" | "drawer" | "grid" => Some(Tab::Widgets),
            "calendar" | "cal" => Some(Tab::Calendar),
            _ => None,
        }
    }
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct TimerState {
    pub end_at: u64,
    pub paused_remaining: Option<u64>,
    pub total: u64,
}

impl TimerState {
    pub fn remaining_secs(&self) -> u64 {
        match self.paused_remaining {
            Some(r) => r,
            None => {
                let now = now_secs();
                self.end_at.saturating_sub(now)
            }
        }
    }
    pub fn running(&self) -> bool {
        self.paused_remaining.is_none() && self.remaining_secs() > 0
    }
}

pub fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// State shared across every UI surface. Main-thread only (Rc).
pub struct Shared {
    pub cfg: RefCell<Config>,
    pub dark: Cell<bool>,
    pub expanded: Cell<bool>,
    pub pinned: Cell<bool>,
    pub fullscreen_hide: Cell<bool>,
    pub shelf: RefCell<ShelfStore>,
    pub clips: RefCell<ClipStore>,
    pub media: RefCell<Option<MediaState>>,
    pub battery: RefCell<BatteryState>,
    pub tab: Cell<Tab>,
    pub timer: RefCell<Option<TimerState>>,
    /// Widgets placed on the Home shelf (persisted).
    pub widgets: RefCell<WidgetStore>,
    /// Meetings for today (refreshed from ICS feeds), sorted by start.
    pub cal_events: RefCell<Vec<crate::services::calendar::CalEvent>>,
    /// Show a transient "Done" state in the pill after a timer finishes.
    pub timer_done_until: Cell<u64>,
    pub media_cmd: RefCell<Option<std::sync::mpsc::Sender<crate::services::mpris::MediaCmd>>>,
    pub notif_cmd: RefCell<Option<std::sync::mpsc::Sender<crate::services::notifd::NotifCmd>>>,
    /// UI-originated events (timer done, etc.) fed back into the event pump
    pub ui_tx: RefCell<Option<Sender<crate::services::Event>>>,
    /// App-level "expand all panels" closure, set once by app::run
    pub expand_all_cb: RefCell<Option<Box<dyn Fn()>>>,
    /// Cached palette to avoid per-frame omarchy file I/O (see theme::resolve)
    cached_palette: RefCell<crate::theme::Palette>,
}

impl Shared {
    pub fn new(cfg: Config) -> Rc<Self> {
        let dark = true;
        let palette = crate::theme::resolve(&cfg, dark);
        Rc::new(Self {
            cfg: RefCell::new(cfg),
            dark: Cell::new(dark),
            expanded: Cell::new(false),
            pinned: Cell::new(false),
            fullscreen_hide: Cell::new(false),
            shelf: RefCell::new(ShelfStore::load()),
            clips: RefCell::new(ClipStore::load()),
            media: RefCell::new(None),
            battery: RefCell::new(BatteryState::default()),
            tab: Cell::new(Tab::Home),
            timer: RefCell::new(None),
            widgets: RefCell::new(WidgetStore::load()),
            cal_events: RefCell::new(Vec::new()),
            timer_done_until: Cell::new(0),
            media_cmd: RefCell::new(None),
            notif_cmd: RefCell::new(None),
            ui_tx: RefCell::new(None),
            expand_all_cb: RefCell::new(None),
            cached_palette: RefCell::new(palette),
        })
    }

    pub fn restyle(&self) {
        // refresh cached palette first so draw funcs use fresh colors without file I/O
        {
            let cfg = self.cfg.borrow();
            let pal = crate::theme::resolve(&cfg, self.dark.get());
            *self.cached_palette.borrow_mut() = pal;
        }
        let cfg = self.cfg.borrow();
        let css = crate::theme::build_css(&cfg, self.dark.get());
        let provider = gtk4::CssProvider::new();
        provider.load_from_string(&css);
        if let Some(display) = gdk::Display::default() {
            gtk4::style_context_add_provider_for_display(
                &display,
                &provider,
                gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
            );
        }
    }

    pub fn palette(&self) -> crate::theme::Palette {
        self.cached_palette.borrow().clone()
    }

    pub fn accent_rgb(&self) -> (u8, u8, u8) {
        self.cached_palette.borrow().accent_rgb
    }
}

thread_local! {
    /// Process-wide handle to Shared, set once by app::run on the GTK thread.
    pub static SHARED: RefCell<Option<Rc<Shared>>> = const { RefCell::new(None) };
}

pub fn with_shared<R>(f: impl FnOnce(&Rc<Shared>) -> R) -> Option<R> {
    SHARED.with(|s| s.borrow().as_ref().map(f))
}

// ---------- tiny widget helpers ----------

pub(crate) fn vbox(spacing: i32) -> gtk4::Box {
    gtk4::Box::new(Orientation::Vertical, spacing)
}
pub(crate) fn hbox(spacing: i32) -> gtk4::Box {
    gtk4::Box::new(Orientation::Horizontal, spacing)
}

pub(crate) fn label(classes: &[&str], text: &str) -> gtk4::Label {
    let l = gtk4::Label::new(Some(text));
    if !classes.is_empty() {
        l.set_css_classes(classes);
    }
    l
}

pub(crate) fn glyph_btn(classes: &[&str], glyph: &str) -> gtk4::Button {
    let b = gtk4::Button::new();
    b.set_has_frame(false);
    b.set_css_classes(classes);
    b.set_child(Some(&label(&["na-glyph"], glyph)));
    b
}

/// Map a gtk4::ApplicationWindow onto the layer-shell protocol, top-center overlay.
pub(crate) fn setup_layer(win: &gtk4::ApplicationWindow, monitor: Option<&gdk::Monitor>) {
    setup_layer_with(win, monitor, gtk4_layer_shell::Layer::Overlay);
}

/// Variant that pins a window to a specific layer. The pill floats on
/// `Overlay` so it always sits above the panel (`Top`) and other bars.
pub(crate) fn setup_layer_with(
    win: &gtk4::ApplicationWindow,
    monitor: Option<&gdk::Monitor>,
    layer: gtk4_layer_shell::Layer,
) {
    use gtk4_layer_shell::{Edge, KeyboardMode, LayerShell};
    win.init_layer_shell();
    win.set_layer(layer);
    win.set_exclusive_zone(-1);
    win.set_monitor(monitor);
    win.set_anchor(Edge::Top, true);
    win.set_keyboard_mode(KeyboardMode::OnDemand);
    win.set_namespace(Some("naarchy"));
    win.add_css_class("naarchy");
}

pub(crate) type Callback = Rc<RefCell<Option<Box<dyn Fn()>>>>;

pub(crate) fn fmt_mmss(secs: u64) -> String {
    format!("{:02}:{:02}", secs / 60, secs % 60)
}
