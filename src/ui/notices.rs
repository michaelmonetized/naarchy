//! Notifications, island style.
//!
//! When Naarchy is the notification daemon (`features.notifications`), every
//! notification lands in an [`Inbox`]. The island shows a bell with the count;
//! expanding the island shows the list on Home. A new notification gets one
//! brief "peek" card under the notch (never a pile), unless do-not-disturb is
//! on: then it is only counted.
//!
//! Gestures, everywhere a notification is drawn:
//! - left click: run its default action (the freedesktop `default` key, else
//!   the first action, or Omarchy's `omarchy-exec` command), focus or open the
//!   sending app, and dismiss;
//! - right click (or middle): dismiss.
//!
//! There is no close button.
//!
//! Do-not-disturb is on when Omarchy's shell says so
//! (`~/.local/state/omarchy/notifications.json`, `"dnd": true`, toggled by
//! `omarchy-toggle-notification-silencing`) or while any client holds a
//! freedesktop notification inhibition (`Inhibit`/`UnInhibit`). Like Omarchy,
//! `omarchy-action` notifications and critical `notify-send` ones still pop.

use super::{hbox, label, vbox, Shared};
use crate::services::{notifd::NotifCmd, Banner, Event};
use gtk4::prelude::*;
use gtk4::{gdk, glib, ApplicationWindow, GestureClick};
use std::cell::Cell;
use std::rc::Rc;

/// Id used for Naarchy's own banners (`naarchy notify`, timer done).
pub const INTERNAL_ID: u32 = u32::MAX - 1;
/// Notifications kept in the list; the oldest beyond this expire.
pub const MAX_INBOX: usize = 50;
const PEEK_DEFAULT_MS: u64 = 6000;
const PEEK_MIN_MS: u64 = 1500;
const PEEK_MAX_MS: u64 = 30_000;

pub const BELL: &str = "\u{f0f3}";
pub const BELL_OFF: &str = "\u{f1f6}";

/// What the island should do with a notification that just arrived.
#[derive(Debug, PartialEq, Eq)]
pub enum Arrival {
    /// Show the peek card.
    Peek,
    /// Do-not-disturb: counted on the bell, no card.
    Silenced,
    /// Not kept and not shown (stale replacement, or transient under DND).
    Dropped,
}

/// The notification list plus do-not-disturb state. Pure data.
#[derive(Default)]
pub struct Inbox {
    items: Vec<Banner>,
    omarchy_dnd: bool,
    inhibited: bool,
}

impl Inbox {
    pub fn dnd(&self) -> bool {
        self.omarchy_dnd || self.inhibited
    }

    /// Returns true when the effective DND state changed.
    pub fn set_omarchy_dnd(&mut self, on: bool) -> bool {
        let before = self.dnd();
        self.omarchy_dnd = on;
        before != self.dnd()
    }

    pub fn set_inhibited(&mut self, on: bool) -> bool {
        let before = self.dnd();
        self.inhibited = on;
        before != self.dnd()
    }

    /// Newest last.
    pub fn items(&self) -> &[Banner] {
        &self.items
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// Record an arrival. Returns what to show and the (id, generation) of
    /// notifications pushed out of the list, which should be closed as expired.
    pub fn arrive(&mut self, b: &Banner) -> (Arrival, Vec<(u32, u64)>) {
        if let Some(old) = self.items.iter().find(|o| o.id == b.id) {
            if old.generation > b.generation {
                return (Arrival::Dropped, Vec::new());
            }
        }
        self.items.retain(|o| o.id != b.id);
        let keep = !is_ephemeral(b);
        let mut evicted = Vec::new();
        if keep {
            self.items.push(b.clone());
            while self.items.len() > MAX_INBOX {
                let old = self.items.remove(0);
                evicted.push((old.id, old.generation));
            }
        }
        let arrival = if !self.dnd() || bypasses_dnd(b) {
            Arrival::Peek
        } else if keep {
            Arrival::Silenced
        } else {
            Arrival::Dropped
        };
        (arrival, evicted)
    }

    /// Remove exactly this generation (a stale close must not remove a replacement).
    pub fn remove(&mut self, id: u32, generation: u64) -> bool {
        let before = self.items.len();
        self.items
            .retain(|o| o.id != id || o.generation != generation);
        before != self.items.len()
    }

    pub fn ids(&self) -> Vec<(u32, u64)> {
        self.items.iter().map(|b| (b.id, b.generation)).collect()
    }
}

/// Never kept in the list: Naarchy's own banners and `transient` notifications.
pub fn is_ephemeral(b: &Banner) -> bool {
    b.transient || b.id == INTERNAL_ID
}

/// Omarchy's rule: its own action toasts and critical notify-send still pop.
pub fn bypasses_dnd(b: &Banner) -> bool {
    b.app_name == "omarchy-action" || (b.app_name == "notify-send" && b.urgency == 2)
}

/// The affirmative action a left click runs: `default`, else the first one.
pub fn default_action(b: &Banner) -> Option<&str> {
    b.actions
        .iter()
        .find(|(k, _)| k == "default")
        .or_else(|| b.actions.first())
        .map(|(k, _)| k.as_str())
}

/// How long the peek card stays. None: until clicked (critical, no timeout).
pub fn peek_ms(b: &Banner) -> Option<u64> {
    match b.timeout_ms {
        Some(ms) => Some(ms.clamp(PEEK_MIN_MS, PEEK_MAX_MS)),
        None if b.urgency == 2 => None,
        None => Some(PEEK_DEFAULT_MS),
    }
}

/// Parse Omarchy's notifications.json; None when absent or unreadable.
pub fn parse_omarchy_dnd(raw: &str) -> Option<bool> {
    serde_json::from_str::<serde_json::Value>(raw)
        .ok()?
        .get("dnd")?
        .as_bool()
}

pub fn omarchy_dnd() -> bool {
    let Some(home) = std::env::var_os("HOME") else {
        return false;
    };
    let path = std::path::Path::new(&home).join(".local/state/omarchy/notifications.json");
    std::fs::read_to_string(path)
        .ok()
        .and_then(|raw| parse_omarchy_dnd(&raw))
        .unwrap_or(false)
}

// ---------- gestures ----------

/// Close a notification on the bus and in every view.
pub fn dismiss(shared: &Rc<Shared>, id: u32, generation: u64, reason: u8) {
    if let Some(tx) = shared.ui_tx.borrow().as_ref() {
        tx.send(Event::CloseBanner { id, generation });
    }
    if let Some(tx) = shared.notif_cmd.borrow().as_ref() {
        let _ = tx.send(NotifCmd::Close {
            id,
            reason,
            generation,
        });
    }
}

/// Left click: default action + focus/open the source app, then dismiss.
pub fn activate(shared: &Rc<Shared>, b: &Banner) {
    if b.id == INTERNAL_ID {
        dismiss(shared, b.id, b.generation, 2);
        return;
    }
    if !b.exec.is_empty() {
        spawn_detached(&["sh", "-c", &b.exec]);
        dismiss(shared, b.id, b.generation, 2);
        return;
    }
    let action = default_action(b).map(str::to_string);
    match (&action, shared.notif_cmd.borrow().as_ref()) {
        (Some(key), Some(tx)) => {
            // notifd emits ActionInvoked + NotificationClosed(2) and closes the views.
            let _ = tx.send(NotifCmd::Action {
                id: b.id,
                generation: b.generation,
                key: key.clone(),
            });
            if let Some(tx) = shared.ui_tx.borrow().as_ref() {
                tx.send(Event::CloseBanner {
                    id: b.id,
                    generation: b.generation,
                });
            }
        }
        _ => dismiss(shared, b.id, b.generation, 2),
    }
    focus_source(
        b.desktop_entry.clone(),
        b.app_name.clone(),
        action.is_none(),
    );
}

fn spawn_detached(argv: &[&str]) {
    use std::os::unix::process::CommandExt;
    use std::process::{Command, Stdio};
    let mut cmd = Command::new(argv[0]);
    cmd.args(&argv[1..])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .process_group(0);
    match cmd.spawn() {
        // Reap in the background so the child never lingers as a zombie.
        Ok(mut child) => {
            std::thread::spawn(move || {
                let _ = child.wait();
            });
        }
        Err(e) => log::warn!("notification click: could not run {}: {e}", argv[0]),
    }
}

/// Names to try when focusing the sender: desktop entry, its last dotted
/// segment (`org.mozilla.firefox` → `firefox`), then the app name.
pub fn focus_candidates(entry: &str, app_name: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let tail = entry.rsplit('.').next().unwrap_or("");
    for name in [entry, tail, app_name] {
        let name = name.trim();
        let ok = !name.is_empty()
            && name.len() <= 128
            && !name.eq_ignore_ascii_case("naarchy")
            && !name.eq_ignore_ascii_case("notify-send")
            && !name.chars().any(char::is_control);
        if ok && !out.iter().any(|o| o.eq_ignore_ascii_case(name)) {
            out.push(name.to_string());
        }
    }
    out
}

/// Address of the first Hyprland client whose class matches `name`.
pub fn pick_client(clients_json: &str, name: &str) -> Option<String> {
    let clients: Vec<serde_json::Value> = serde_json::from_str(clients_json).ok()?;
    let class_of = |c: &serde_json::Value, key: &str| {
        c.get(key)
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string()
    };
    clients
        .iter()
        .find(|c| {
            class_of(c, "class").eq_ignore_ascii_case(name)
                || class_of(c, "initialClass").eq_ignore_ascii_case(name)
        })
        .and_then(|c| {
            c.get("address")
                .and_then(|a| a.as_str())
                .map(str::to_string)
        })
}

fn valid_desktop_id(entry: &str) -> bool {
    !entry.is_empty()
        && entry.len() <= 255
        && entry
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_'))
}

fn which(name: &str) -> Option<std::path::PathBuf> {
    std::env::var_os("PATH").and_then(|paths| {
        std::env::split_paths(&paths)
            .map(|d| d.join(name))
            .find(|p| p.is_file())
    })
}

fn omarchy_focus_helper() -> Option<std::path::PathBuf> {
    let mut dirs = Vec::new();
    if let Some(p) = std::env::var_os("OMARCHY_PATH") {
        dirs.push(std::path::PathBuf::from(p).join("bin"));
    }
    if let Some(h) = std::env::var_os("HOME") {
        dirs.push(std::path::PathBuf::from(h).join(".local/share/omarchy/bin"));
    }
    dirs.push("/usr/share/omarchy/bin".into());
    dirs.into_iter()
        .map(|d| d.join("omarchy-hyprland-focus-app"))
        .find(|p| p.is_file())
        .or_else(|| which("omarchy-hyprland-focus-app"))
}

/// Raise the sender's window (Omarchy's helper when present, else Hyprland
/// IPC). When nothing matched and no action was invoked, launch its desktop
/// entry. Runs off the GTK thread.
fn focus_source(entry: String, app_name: String, launch_if_missing: bool) {
    std::thread::spawn(move || {
        let names = focus_candidates(&entry, &app_name);
        let helper = omarchy_focus_helper();
        for name in &names {
            if let Some(helper) = &helper {
                let ok = std::process::Command::new(helper)
                    .arg(name)
                    .stdin(std::process::Stdio::null())
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null())
                    .status()
                    .map(|s| s.success())
                    .unwrap_or(false);
                if ok {
                    return;
                }
            }
            let address = crate::services::hyprland::request("j/clients")
                .and_then(|json| pick_client(&json, name));
            if let Some(address) = address {
                let reply = crate::services::hyprland::request(&format!(
                    "dispatch focuswindow address:{address}"
                ));
                if reply.as_deref() == Some("ok") {
                    return;
                }
                let lua = format!("dispatch hl.dsp.focus({{ window = \"address:{address}\" }})");
                if crate::services::hyprland::request(&lua).as_deref() == Some("ok") {
                    return;
                }
            }
        }
        if launch_if_missing && valid_desktop_id(&entry) {
            let id = entry.trim_end_matches(".desktop");
            if which("gtk-launch").is_some() {
                spawn_detached(&["gtk-launch", id]);
            }
        }
    });
}

// ---------- widgets ----------

fn strip_markup(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut in_tag = false;
    for ch in s.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => in_tag = false,
            c if !in_tag => out.push(c),
            _ => {}
        }
    }
    out.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
}

/// One notification card. The whole card is the click target.
pub fn card(b: &Banner, shared: &Rc<Shared>, body_lines: i32) -> gtk4::Box {
    let card = vbox(2);
    card.add_css_class("na-notice");
    if b.urgency == 2 {
        card.add_css_class("critical");
    }
    card.set_cursor_from_name(Some("pointer"));

    let head = hbox(8);
    let sum = label(&["na-title"], &b.summary);
    sum.set_ellipsize(gtk4::pango::EllipsizeMode::End);
    sum.set_hexpand(true);
    sum.set_xalign(0.0);
    sum.set_single_line_mode(true);
    head.append(&sum);
    if !b.app_name.is_empty() && b.app_name != "naarchy" {
        let app_name = label(&["na-dim"], &b.app_name);
        app_name.set_max_width_chars(14);
        app_name.set_ellipsize(gtk4::pango::EllipsizeMode::End);
        app_name.set_single_line_mode(true);
        head.append(&app_name);
    }
    card.append(&head);

    if !b.body.is_empty() && body_lines > 0 {
        let body = label(&["na-dim"], &strip_markup(&b.body));
        body.set_wrap(true);
        body.set_wrap_mode(gtk4::pango::WrapMode::WordChar);
        body.set_lines(body_lines);
        body.set_ellipsize(gtk4::pango::EllipsizeMode::End);
        body.set_xalign(0.0);
        card.append(&body);
    }

    let what = default_action(b)
        .and_then(|k| b.actions.iter().find(|(key, _)| key == k))
        .map(|(_, label)| label.as_str())
        .filter(|l| !l.is_empty() && *l != "default")
        .map(|l| format!("{l}. "))
        .unwrap_or_default();
    super::describe(
        &card,
        &format!(
            "{}: {}. {what}Click to open, right-click to dismiss.",
            if b.app_name.is_empty() {
                "Notification"
            } else {
                &b.app_name
            },
            b.summary
        ),
    );

    let click = GestureClick::new();
    click.set_button(0);
    {
        let shared = shared.clone();
        let b = b.clone();
        click.connect_released(move |g, _n, _x, _y| match g.current_button() {
            gdk::BUTTON_PRIMARY => activate(&shared, &b),
            gdk::BUTTON_SECONDARY | gdk::BUTTON_MIDDLE => dismiss(&shared, b.id, b.generation, 2),
            _ => {}
        });
    }
    card.add_controller(click);
    card
}

struct PeekWin {
    id: u32,
    generation: u64,
    ephemeral: bool,
    win: ApplicationWindow,
    timeout: Rc<Cell<Option<glib::SourceId>>>,
}

impl Drop for PeekWin {
    fn drop(&mut self) {
        if let Some(source) = self.timeout.take() {
            source.remove();
        }
        self.win.destroy();
    }
}

/// The single peek card under the notch.
pub struct Peek {
    app: glib::WeakRef<gtk4::Application>,
    cur: Option<PeekWin>,
}

impl Peek {
    pub fn new(app: &gtk4::Application) -> Self {
        Self {
            app: app.downgrade(),
            cur: None,
        }
    }

    /// Replace whatever is peeking with `b`.
    pub fn show(&mut self, b: &Banner, shared: &Rc<Shared>) {
        self.cur = None;
        let Some(app) = self.app.upgrade() else {
            return;
        };
        let win = ApplicationWindow::builder()
            .application(&app)
            .title("naarchy-notification")
            .decorated(false)
            .resizable(false)
            .default_width(380)
            .build();
        super::setup_layer(&win, None);
        {
            use gtk4_layer_shell::{Edge, LayerShell};
            win.set_margin(Edge::Top, 52);
        }
        let card = card(b, shared, 2);
        card.add_css_class("peek");
        card.set_margin_start(10);
        card.set_margin_end(10);
        card.set_margin_top(6);
        card.set_margin_bottom(10);
        card.set_width_request(360);
        win.set_child(Some(&card));
        let key = gtk4::EventControllerKey::new();
        {
            let shared = shared.clone();
            let (id, generation) = (b.id, b.generation);
            key.connect_key_pressed(move |_, key, _, _| {
                if key == gdk::Key::Escape {
                    dismiss(&shared, id, generation, 2);
                    glib::Propagation::Stop
                } else {
                    glib::Propagation::Proceed
                }
            });
        }
        win.add_controller(key);
        win.set_opacity(0.0);
        win.present();
        {
            let weak = win.downgrade();
            super::motion::tween(
                &win,
                160,
                move |t| {
                    if let Some(w) = weak.upgrade() {
                        w.set_opacity(t);
                    }
                },
                || {},
            );
        }
        let timeout = Rc::new(Cell::new(None));
        if let Some(ms) = peek_ms(b) {
            let slot = timeout.clone();
            let (id, generation) = (b.id, b.generation);
            let source =
                glib::timeout_add_local_once(std::time::Duration::from_millis(ms), move || {
                    slot.set(None);
                    crate::app::peek_expired(id, generation);
                });
            timeout.set(Some(source));
        }
        self.cur = Some(PeekWin {
            id: b.id,
            generation: b.generation,
            ephemeral: is_ephemeral(b),
            win,
            timeout,
        });
    }

    /// The peek timed out: collapse into the bell. Ephemeral notifications
    /// are not in the list, so they close as expired. Returns that id.
    pub fn expire(&mut self, id: u32, generation: u64) -> Option<(u32, u64)> {
        let cur = self.cur.as_ref()?;
        if cur.id != id || cur.generation != generation {
            return None;
        }
        let ephemeral = cur.ephemeral;
        self.cur = None;
        ephemeral.then_some((id, generation))
    }

    pub fn hide_if(&mut self, id: u32, generation: u64) {
        if self
            .cur
            .as_ref()
            .is_some_and(|c| c.id == id && c.generation == generation)
        {
            self.cur = None;
        }
    }

    pub fn hide(&mut self) {
        self.cur = None;
    }
}

/// The list shown on Home while the island is expanded.
pub struct ListSection {
    root: gtk4::Box,
    title: gtk4::Label,
    rows: gtk4::Box,
}

impl ListSection {
    pub fn build(shared: &Rc<Shared>) -> Self {
        let root = vbox(6);
        root.add_css_class("na-notices");
        let head = hbox(8);
        let bell = label(&["na-glyph"], BELL);
        let title = label(&["na-title"], "Notifications");
        title.set_xalign(0.0);
        title.set_hexpand(true);
        let hint = label(&["na-dim"], "click to open · right-click to dismiss");
        let clear = gtk4::Button::with_label("Clear all");
        clear.set_css_classes(&["na-btn", "ghost"]);
        {
            let shared = shared.clone();
            clear.connect_clicked(move |_| {
                let ids = shared.notices.borrow().ids();
                for (id, generation) in ids {
                    dismiss(&shared, id, generation, 2);
                }
            });
        }
        head.append(&bell);
        head.append(&title);
        head.append(&hint);
        head.append(&clear);
        root.append(&head);
        let rows = vbox(6);
        let scroll = gtk4::ScrolledWindow::new();
        scroll.set_policy(gtk4::PolicyType::Never, gtk4::PolicyType::Automatic);
        scroll.set_max_content_height(300);
        scroll.set_propagate_natural_height(true);
        scroll.add_css_class("na-scroll");
        scroll.set_child(Some(&rows));
        root.append(&scroll);
        root.set_visible(false);
        Self { root, title, rows }
    }

    pub fn root(&self) -> &gtk4::Box {
        &self.root
    }

    pub fn reload(&self, shared: &Rc<Shared>) {
        while let Some(c) = self.rows.first_child() {
            self.rows.remove(&c);
        }
        let inbox = shared.notices.borrow();
        let items: Vec<Banner> = inbox.items().iter().rev().cloned().collect();
        let dnd = inbox.dnd();
        drop(inbox);
        self.root.set_visible(!items.is_empty());
        self.title.set_text(&match (items.len(), dnd) {
            (n, true) => format!("Notifications ({n}) · do not disturb"),
            (n, false) => format!("Notifications ({n})"),
        });
        for b in &items {
            self.rows.append(&card(b, shared, 3));
        }
        // Home is busy; give the list room for up to three cards, scroll the rest.
        if let Some(scroll) = self.rows.parent().and_then(|p| p.parent()) {
            if let Ok(scroll) = scroll.downcast::<gtk4::ScrolledWindow>() {
                scroll.set_min_content_height((items.len().min(3) as i32) * 64);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn banner(id: u32, generation: u64) -> Banner {
        Banner {
            id,
            generation,
            app_name: "Slack".into(),
            icon: String::new(),
            summary: "hi".into(),
            body: String::new(),
            actions: vec![],
            urgency: 1,
            timeout_ms: Some(5000),
            desktop_entry: "com.slack.Slack".into(),
            exec: String::new(),
            transient: false,
        }
    }

    #[test]
    fn arrivals_peek_or_count_under_dnd() {
        let mut inbox = Inbox::default();
        assert_eq!(inbox.arrive(&banner(1, 1)).0, Arrival::Peek);
        assert!(inbox.set_omarchy_dnd(true));
        assert_eq!(inbox.arrive(&banner(2, 2)).0, Arrival::Silenced);
        assert_eq!(inbox.len(), 2);
        // transient under DND: neither shown nor kept
        let mut t = banner(3, 3);
        t.transient = true;
        assert_eq!(inbox.arrive(&t).0, Arrival::Dropped);
        assert_eq!(inbox.len(), 2);
        // Omarchy's bypass rules
        let mut action = banner(4, 4);
        action.app_name = "omarchy-action".into();
        assert_eq!(inbox.arrive(&action).0, Arrival::Peek);
        let mut crit = banner(5, 5);
        crit.app_name = "notify-send".into();
        crit.urgency = 2;
        assert_eq!(inbox.arrive(&crit).0, Arrival::Peek);
        crit.urgency = 1;
        crit.generation = 6;
        assert_eq!(inbox.arrive(&crit).0, Arrival::Silenced);
        // inhibition alone also silences
        assert!(inbox.set_omarchy_dnd(false));
        assert!(inbox.set_inhibited(true));
        assert_eq!(inbox.arrive(&banner(7, 7)).0, Arrival::Silenced);
        assert!(!inbox.set_omarchy_dnd(true), "still DND, no change");
    }

    #[test]
    fn replacements_and_stale_closes_respect_generation() {
        let mut inbox = Inbox::default();
        inbox.arrive(&banner(1, 5));
        assert_eq!(inbox.arrive(&banner(1, 4)).0, Arrival::Dropped);
        inbox.arrive(&banner(1, 9));
        assert_eq!(inbox.len(), 1);
        assert!(!inbox.remove(1, 5), "stale close keeps the replacement");
        assert!(inbox.remove(1, 9));
        assert!(inbox.is_empty());
    }

    #[test]
    fn internal_banners_peek_but_are_not_kept_and_list_is_bounded() {
        let mut inbox = Inbox::default();
        let (arrival, _) = inbox.arrive(&banner(INTERNAL_ID, 1));
        assert_eq!(arrival, Arrival::Peek);
        assert!(inbox.is_empty());
        let mut evicted = Vec::new();
        for i in 0..(MAX_INBOX as u32 + 3) {
            evicted.extend(inbox.arrive(&banner(100 + i, 10 + i as u64)).1);
        }
        assert_eq!(inbox.len(), MAX_INBOX);
        assert_eq!(evicted, vec![(100, 10), (101, 11), (102, 12)]);
        assert_eq!(inbox.items().last().unwrap().id, 100 + MAX_INBOX as u32 + 2);
    }

    #[test]
    fn default_action_prefers_default_key() {
        let mut b = banner(1, 1);
        assert_eq!(default_action(&b), None);
        b.actions = vec![
            ("reply".into(), "Reply".into()),
            ("default".into(), "".into()),
        ];
        assert_eq!(default_action(&b), Some("default"));
        b.actions = vec![
            ("open".into(), "Open".into()),
            ("later".into(), "Later".into()),
        ];
        assert_eq!(default_action(&b), Some("open"));
    }

    #[test]
    fn peek_durations() {
        let mut b = banner(1, 1);
        assert_eq!(peek_ms(&b), Some(5000));
        b.timeout_ms = Some(1);
        assert_eq!(peek_ms(&b), Some(PEEK_MIN_MS));
        b.timeout_ms = None;
        assert_eq!(peek_ms(&b), Some(PEEK_DEFAULT_MS));
        b.urgency = 2;
        assert_eq!(peek_ms(&b), None);
    }

    #[test]
    fn omarchy_dnd_file() {
        assert_eq!(parse_omarchy_dnd(r#"{"version":3,"dnd":true}"#), Some(true));
        assert_eq!(
            parse_omarchy_dnd(r#"{"version":3,"dnd":false}"#),
            Some(false)
        );
        assert_eq!(parse_omarchy_dnd(r#"{"version":3}"#), None);
        assert_eq!(parse_omarchy_dnd("not json"), None);
    }

    #[test]
    fn focus_targets() {
        assert_eq!(
            focus_candidates("org.mozilla.firefox", "Firefox"),
            vec!["org.mozilla.firefox", "firefox"]
        );
        assert_eq!(focus_candidates("", "Slack"), vec!["Slack"]);
        assert!(focus_candidates("", "notify-send").is_empty());
        let clients = r#"[{"address":"0x1","class":"kitty","initialClass":"kitty"},
                          {"address":"0x2","class":"Slack","initialClass":"Slack"}]"#;
        assert_eq!(pick_client(clients, "slack").as_deref(), Some("0x2"));
        assert_eq!(pick_client(clients, "firefox"), None);
        assert_eq!(pick_client("garbage", "slack"), None);
        assert!(valid_desktop_id("org.mozilla.firefox"));
        assert!(!valid_desktop_id("x; rm -rf ~"));
    }

    #[test]
    fn markup_is_stripped_and_entities_decoded() {
        assert_eq!(strip_markup("<b>Tom</b> &amp; Jerry"), "Tom & Jerry");
    }
}
