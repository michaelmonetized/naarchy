use super::calendar::CalendarPage;
use super::clipview::ClipPage;
use super::drawer::DrawerPage;
use super::home::HomePage;
use super::liquid::{self, DOCK_RESERVE};
use super::motion::{self, Spring};
use super::shelfview::ShelfPage;
use super::{g, hbox, label, vbox, Shared, Tab};
use gtk4::prelude::*;
use gtk4::{gdk, glib, ApplicationWindow, DropTarget, Stack, ToggleButton};
use std::cell::Cell;
use std::rc::Rc;

pub struct PanelUi {
    pub win: ApplicationWindow,
    stack: Stack,
    dock: gtk4::Box,
    dock_wrap: gtk4::Box,
    content: gtk4::Box,
    bg: gtk4::DrawingArea,
    dock_buttons: Vec<(Tab, ToggleButton)>,
    pointer_flag: Rc<Cell<bool>>,
    collapse_src: Rc<Cell<Option<glib::SourceId>>>,
    progress: Rc<Cell<f64>>,
    vel: Rc<Cell<f64>>,
    target: Rc<Cell<f64>>,
    tick: Rc<Cell<Option<gtk4::TickCallbackId>>>,
    home_page: Rc<HomePage>,
    shelf_page: Rc<ShelfPage>,
    clip_page: Rc<ClipPage>,
    drawer_page: Rc<DrawerPage>,
    cal_page: Rc<CalendarPage>,
    drop_revealer: gtk4::Revealer,
}

impl PanelUi {
    pub fn build(
        app: &gtk4::Application,
        shared: &Rc<Shared>,
        monitor: Option<&gdk::Monitor>,
    ) -> Self {
        let (content_w, h) = {
            let c = shared.cfg.borrow();
            (c.appearance.panel_width, c.appearance.panel_height)
        };
        // Bloom is 75% wider than the content card; extra width is veil only.
        let w = ((content_w as f64) * super::liquid::PANEL_WINDOW_SCALE).round() as i32;

        let win = ApplicationWindow::builder()
            .application(app)
            .title("naarchy-panel")
            .decorated(false)
            .resizable(false)
            .default_width(w)
            .default_height(h)
            .build();
        super::setup_layer_with(&win, monitor, gtk4_layer_shell::Layer::Top);
        win.set_width_request(w);
        win.set_height_request(h);
        win.set_visible(false);

        let progress = Rc::new(Cell::new(0.0));
        let vel = Rc::new(Cell::new(0.0));
        let target = Rc::new(Cell::new(0.0));
        let tick = Rc::new(Cell::new(None));

        let bg = gtk4::DrawingArea::new();
        bg.set_hexpand(true);
        bg.set_vexpand(true);
        {
            let progress = progress.clone();
            bg.set_draw_func(move |_da, cr, w, h| {
                let (fill, alpha) = super::with_shared(|sh| {
                    let pal = sh.palette();
                    let fill = crate::theme::hex_triple(&pal.bg).unwrap_or((10, 10, 15));
                    let alpha = sh.cfg.borrow().appearance.opacity;
                    (fill, alpha)
                })
                .unwrap_or(((10, 10, 15), 0.92));
                let cap = liquid::geom(w as f64, h as f64, progress.get(), DOCK_RESERVE);
                liquid::draw(cr, cap, fill, alpha);
            });
        }

        let content = vbox(0);
        content.set_margin_top((super::liquid::NOTCH_H as i32) + 10);
        content.set_margin_start(28);
        content.set_margin_end(28);
        content.set_margin_bottom(8);
        content.set_hexpand(false);
        content.set_halign(gtk4::Align::Center);
        content.set_width_request(content_w);
        content.set_vexpand(true);
        content.set_opacity(0.0);

        let home_page = Rc::new(HomePage::build(shared));
        let shelf_page = Rc::new(ShelfPage::build(shared));
        let clip_page = Rc::new(ClipPage::build(shared));
        let drawer_page = Rc::new(DrawerPage::build(shared));
        let cal_page = Rc::new(CalendarPage::new(shared));

        let stack = Stack::new();
        stack.set_transition_type(gtk4::StackTransitionType::Crossfade);
        stack.set_transition_duration(180);
        stack.set_vexpand(true);
        let first = ToggleButton::new();

        let mut dock_buttons = Vec::new();
        let dock = hbox(4);
        dock.set_css_classes(&["na-dock"]);
        dock.set_halign(gtk4::Align::Center);
        dock.set_valign(gtk4::Align::Center);
        dock.set_opacity(0.0);
        {
            let tabs: Vec<Tab> = Tab::all()
                .into_iter()
                .filter(|t| match t {
                    Tab::Home => true,
                    Tab::Inbox => shared.cfg.borrow().features.shelf,
                    Tab::Clipboard => shared.cfg.borrow().features.clipboard,
                    Tab::Widgets => true,
                    Tab::Calendar => shared.cfg.borrow().features.calendar,
                })
                .collect();
            let mut first_used = false;
            for t in &tabs {
                let add = match t {
                    Tab::Home => Some(("home", home_page.root())),
                    Tab::Inbox => shared
                        .cfg
                        .borrow()
                        .features
                        .shelf
                        .then(|| ("inbox", shelf_page.root())),
                    Tab::Clipboard => shared
                        .cfg
                        .borrow()
                        .features
                        .clipboard
                        .then(|| ("clip", clip_page.root())),
                    Tab::Widgets => Some(("widgets", drawer_page.root())),
                    Tab::Calendar => shared
                        .cfg
                        .borrow()
                        .features
                        .calendar
                        .then(|| ("cal", cal_page.root())),
                };
                if let Some((name, page_box)) = add {
                    stack.add_titled(page_box, Some(name), t.label());
                }
            }

            for t in &tabs {
                let (glyph, tip) = match t {
                    Tab::Home => (g::HOME, "Home"),
                    Tab::Inbox => (g::INBOX, "Inbox"),
                    Tab::Clipboard => (g::CLIP, "Clipboard"),
                    Tab::Widgets => (g::GRID, "Widgets"),
                    Tab::Calendar => (g::CAL, "Calendar"),
                };
                let b = if !first_used {
                    first_used = true;
                    first.clone()
                } else {
                    let nb = gtk4::ToggleButton::new();
                    nb.set_group(Some(&first));
                    nb
                };
                b.set_css_classes(&["na-dock-btn"]);
                let l = label(&["na-dock-glyph"], glyph);
                b.set_child(Some(&l));
                b.set_tooltip_text(Some(tip));
                let sh = shared.clone();
                let stack2 = stack.clone();
                let t2 = *t;
                b.connect_toggled(move |btn| {
                    if btn.is_active() {
                        sh.tab.set(t2);
                        stack2.set_visible_child_name(tab_name(t2));
                        crate::app::poke_panels();
                    }
                });
                dock.append(&b);
                dock_buttons.push((*t, b));
            }

            let outer_sc = gtk4::ScrolledWindow::builder()
                .hscrollbar_policy(gtk4::PolicyType::Never)
                .vscrollbar_policy(gtk4::PolicyType::Automatic)
                .vexpand(true)
                .overlay_scrolling(true)
                .build();
            outer_sc.set_css_classes(&["na-scroll"]);
            outer_sc.set_child(Some(&stack));
            content.append(&outer_sc);
        }

        let dock_wrap = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
        dock_wrap.set_halign(gtk4::Align::Center);
        dock_wrap.set_margin_bottom(14);
        dock_wrap.set_margin_top(4);
        dock_wrap.set_opacity(0.0);
        dock_wrap.append(&dock);
        // Settings — opens ~/.config/naarchy/config.toml in nvim via omarchy-launch-config-editor
        {
            let settings = crate::ui::glyph_btn(&["na-dock-btn", "na-settings-btn"], g::SETTINGS);
            settings.set_tooltip_text(Some("Settings — Edit config in Neovim"));
            // ensure pointer cursor not text caret
            settings.set_cursor(gdk::Cursor::from_name("pointer", None).as_ref());
            settings.set_focusable(true);
            settings.connect_clicked(|_| {
                crate::util::open_config_in_editor();
            });
            dock_wrap.append(&settings);
        }

        let shell = vbox(0);
        shell.set_hexpand(true);
        shell.set_vexpand(true);
        shell.append(&content);
        shell.append(&dock_wrap);

        let drop_box = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        drop_box.set_css_classes(&["na-drop-veil"]);
        drop_box.set_hexpand(true);
        drop_box.set_vexpand(true);
        let drop_revealer = gtk4::Revealer::new();
        drop_revealer.set_transition_type(gtk4::RevealerTransitionType::Crossfade);
        drop_revealer.set_transition_duration(180);
        drop_revealer.set_reveal_child(false);
        drop_revealer.set_can_target(false);
        drop_revealer.set_hexpand(true);
        drop_revealer.set_vexpand(true);
        drop_revealer.set_halign(gtk4::Align::Center);
        drop_revealer.set_width_request(content_w);
        drop_revealer.set_margin_top(super::liquid::NOTCH_H as i32 + 8);
        drop_revealer.set_margin_bottom(DOCK_RESERVE as i32);
        drop_revealer.set_margin_start(24);
        drop_revealer.set_margin_end(24);
        drop_revealer.set_child(Some(&drop_box));

        let overlay = gtk4::Overlay::new();
        overlay.set_child(Some(&bg));
        overlay.add_overlay(&shell);
        overlay.add_overlay(&drop_revealer);
        win.set_child(Some(&overlay));
        attach_file_drop(&win);

        {
            let key = gtk4::EventControllerKey::new();
            key.connect_key_pressed(move |_k, keyval, _code, _mod| {
                if keyval == gdk::Key::Escape {
                    crate::app::request_collapse_all();
                    gtk4::glib::Propagation::Stop
                } else {
                    gtk4::glib::Propagation::Proceed
                }
            });
            win.add_controller(key);
        }

        let pointer_flag = Rc::new(Cell::new(false));
        {
            let motion = gtk4::EventControllerMotion::new();
            motion.connect_enter(move |_m, _x, _y| {
                crate::app::surface_pointer_enter();
            });
            motion.connect_leave(move |_m| {
                crate::app::surface_pointer_leave();
            });
            win.add_controller(motion);
        }

        let p = Self {
            win,
            stack,
            dock,
            dock_wrap: dock_wrap.clone(),
            content,
            bg,
            dock_buttons,
            pointer_flag,
            collapse_src: Rc::new(Cell::new(None)),
            progress,
            vel,
            target,
            tick,
            home_page,
            shelf_page,
            clip_page,
            drawer_page,
            cal_page,
            drop_revealer,
        };
        p.shelf_page.reload();
        p.clip_page.reload(None);
        p.cal_page.rebuild();
        p.media_update();
        p
    }

    pub fn expand(&self) {
        super::with_shared(|sh| {
            sh.expanded.set(true);
            if !self.win.is_visible() {
                self.win.present();
            }
            self.win.set_visible(true);
            self.show_tab(sh.tab.get());
            self.cancel_collapse_timer();
            self.target.set(1.0);
            self.start_anim();
        });
    }

    pub fn collapse(&self) {
        self.target.set(0.0);
        self.start_anim();
    }

    pub fn collapse_now(&self) {
        self.target.set(0.0);
        self.start_anim();
    }

    pub fn poke_collapse_timer(&self) {
        self.cancel_collapse_timer();
    }

    pub fn note_pointer(&self, on: bool) {
        self.pointer_flag.set(on);
        if on {
            self.cancel_collapse_timer();
        }
    }

    pub fn set_drop_veil(&self, on: bool) {
        self.drop_revealer.set_reveal_child(on);
    }

    pub fn schedule_collapse_if_unhovered(&self) {
        super::with_shared(|sh| {
            if !sh.expanded.get() || sh.pinned.get() || self.pointer_flag.get() {
                return;
            }
            let ms = sh.cfg.borrow().behavior.collapse_on_leave_ms;
            self.cancel_collapse_timer();
            let pin = self.pointer_flag.clone();
            let src_slot = self.collapse_src.clone();
            let src =
                glib::timeout_add_local_once(std::time::Duration::from_millis(ms), move || {
                    src_slot.set(None);
                    if !pin.get() {
                        crate::app::request_collapse_all();
                    }
                });
            self.collapse_src.set(Some(src));
        });
    }

    fn cancel_collapse_timer(&self) {
        if let Some(src) = self.collapse_src.take() {
            src.remove();
        }
    }

    fn start_anim(&self) {
        let progress = self.progress.clone();
        let vel = self.vel.clone();
        let target = self.target.clone();
        let bg = self.bg.clone();
        let bg_tick = bg.clone();
        let content = self.content.clone();
        let dock = self.dock.clone();
        let dock_wrap = self.dock_wrap.clone();
        let win = self.win.clone();
        let tick = self.tick.clone();
        let frame = Rc::new(Cell::new(0u32));
        motion::drive(&tick, &bg_tick, move |dt| {
            let tgt = target.get();
            let spring = if tgt > 0.5 {
                Spring::OPEN
            } else {
                Spring::CLOSE
            };
            let (p, v) = spring.step(progress.get(), vel.get(), tgt, dt);
            progress.set(p);
            vel.set(v);
            bg.queue_draw();
            content.set_opacity(motion::content_opacity(p));
            let d_op = motion::dock_opacity(p);
            dock.set_opacity(d_op);
            dock_wrap.set_opacity(d_op);
            dock.set_margin_bottom(motion::lerp(-8.0, 0.0, d_op) as i32);

            // throttle input region updates to ~20Hz to avoid per-frame layout + Wayland round-trip
            let f = frame.get().wrapping_add(1);
            frame.set(f);
            let should_update_region = f % 3 == 0 || spring.settled(p, v, tgt);
            if should_update_region {
                let ww = win.width().max(1) as f64;
                let wh = win.height().max(1) as f64;
                let cap = liquid::geom(ww, wh, p, DOCK_RESERVE);
                let dock_hit = if d_op > 0.05 {
                    // include settings gear which lives in dock_wrap, not dock
                    liquid::widget_rect_in(&win, &dock_wrap)
                } else {
                    None
                };
                liquid::apply_input_region(&win, cap, dock_hit);
            }

            if spring.settled(p, v, tgt) {
                progress.set(tgt);
                vel.set(0.0);
                content.set_opacity(motion::content_opacity(tgt));
                dock.set_opacity(motion::dock_opacity(tgt));
                dock_wrap.set_opacity(motion::dock_opacity(tgt));
                bg.queue_draw();
                if tgt < 0.5 {
                    liquid::clear_input_region(&win);
                    win.set_visible(false);
                    super::with_shared(|sh| sh.expanded.set(false));
                } else {
                    let ww = win.width().max(1) as f64;
                    let wh = win.height().max(1) as f64;
                    let cap = liquid::geom(ww, wh, 1.0, DOCK_RESERVE);
                    liquid::apply_input_region(&win, cap, liquid::widget_rect_in(&win, &dock_wrap));
                }
                return false;
            }
            true
        });
    }

    pub fn show_tab(&self, t: Tab) {
        for (tab, b) in &self.dock_buttons {
            b.set_active(*tab == t);
        }
        self.stack.set_visible_child_name(tab_name(t));
    }

    pub fn media_update(&self) {
        self.home_page.refresh();
    }
    pub fn shelf_reload(&self) {
        self.shelf_page.reload();
    }
    pub fn clip_reload(&self) {
        self.clip_page.reload(None);
    }
    pub fn cal_reload(&self) {
        self.cal_page.rebuild();
        self.poke_collapse_timer();
    }
    pub fn home_reload(&self) {
        self.home_page.apply_store();
        self.drawer_page.rebuild();
    }
    pub fn tick(&self) {
        self.home_page.tick();
        self.cal_page.tick();
    }
    pub fn timer_start(&self, secs: u64) {
        self.home_page.timer_start(secs);
    }
    pub fn redraw(&self) {
        self.bg.queue_draw();
    }
}

fn tab_name(t: Tab) -> &'static str {
    match t {
        Tab::Home => "home",
        Tab::Inbox => "inbox",
        Tab::Clipboard => "clip",
        Tab::Widgets => "widgets",
        Tab::Calendar => "cal",
    }
}

fn file_drop_formats() -> gdk::ContentFormats {
    gdk::ContentFormats::builder()
        .add_type(gdk::Texture::static_type())
        .add_mime_type("text/uri-list")
        .add_mime_type("text/plain;charset=utf-8")
        .add_mime_type("text/plain")
        .add_mime_type("image/png")
        .build()
}

/// File drops on the notch or any tab land in the Inbox.
pub(crate) fn attach_file_drop(widget: &impl gtk4::prelude::IsA<gtk4::Widget>) {
    let dt = DropTarget::builder()
        .formats(&file_drop_formats())
        .actions(gdk::DragAction::COPY)
        .build();
    dt.connect_enter(move |_dt, _x, _y| {
        crate::app::drop_hover(true);
        gdk::DragAction::COPY
    });
    dt.connect_leave(move |_dt| {
        crate::app::drop_hover(false);
    });
    dt.connect_drop(move |_dt, value, _x, _y| {
        crate::app::drop_commit(value);
        true
    });
    widget.add_controller(dt);
}

/// Interpret a dropped GDK Value and store it on the shelf.
pub(crate) fn handle_dropped_value(shared: &Rc<Shared>, value: &glib::Value) {
    if let Ok(tex) = value.get::<gdk::Texture>() {
        let tmp = std::env::temp_dir().join(format!("naarchy-drop-{}.png", std::process::id()));
        if tex.save_to_png(&tmp).is_ok() {
            let bytes = std::fs::read(&tmp).unwrap_or_default();
            let _ = std::fs::remove_file(&tmp);
            shared.shelf.borrow_mut().add_image(bytes);
            crate::app::refresh_after_shelf_change();
            return;
        }
    }
    let s = value
        .get::<String>()
        .or_else(|_| value.get::<glib::GString>().map(|g| g.to_string()));
    if let Ok(s) = s {
        for item in parse_payload(&s) {
            match item {
                PayloadKind::File(p) => {
                    shared.shelf.borrow_mut().add_file(&p);
                }
                PayloadKind::Text(t) => {
                    shared.shelf.borrow_mut().add_text(&t);
                }
            }
        }
        crate::app::refresh_after_shelf_change();
    }
}

pub(crate) enum PayloadKind {
    File(String),
    Text(String),
}

pub(crate) fn parse_payload(s: &str) -> Vec<PayloadKind> {
    let mut out = Vec::new();
    let single_line = s.lines().count() == 1;
    for line in s.split('\n') {
        let l = line.trim_end_matches('\r').trim();
        if l.is_empty() {
            continue;
        }
        if let Some(rest) = l.strip_prefix("file://") {
            let path = uri_unescape(rest);
            out.push(PayloadKind::File(path));
        } else if l.starts_with('/') {
            out.push(PayloadKind::File(l.to_string()));
        } else if looks_like_url(l) || single_line {
            out.push(PayloadKind::Text(l.to_string()));
        }
    }
    out
}

fn looks_like_url(s: &str) -> bool {
    s.starts_with("http://")
        || s.starts_with("https://")
        || s.starts_with("ftp://")
        || s.starts_with("mailto:")
}

pub(crate) fn uri_unescape(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let (Some(h), Some(l)) = (
                (bytes[i + 1] as char).to_digit(16),
                (bytes[i + 2] as char).to_digit(16),
            ) {
                out.push(((h << 4) | l) as u8);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}
