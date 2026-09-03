use super::motion::{self, Spring};
use super::{fmt_mmss, hbox, label, Callback, Shared};
use gtk4::prelude::*;
use gtk4::{gdk, ApplicationWindow, EventControllerMotion, GestureClick, Label};
use std::cell::{Cell, RefCell};
use std::rc::Rc;

const PILL_H: i32 = super::liquid::NOTCH_H as i32;
const LIVE_H: i32 = super::liquid::LIVE_H as i32;

/// Which content pair is currently wrapped around the notch.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Mood {
    None,
    Done,
    Timer,
    Media,
    Files,
}

pub struct PillUi {
    pub win: ApplicationWindow,
    root: gtk4::Box,
    left_box: gtk4::Box,
    mid_box: gtk4::Box,
    right_box: gtk4::Box,
    clock_label: Label,
    battery_label: Label,
    timer_icon: Label,
    timer_count: Label,
    media_art: gtk4::Box,
    media_icon: Label,
    media_title: Label,
    files_icon: Label,
    files_count: Label,
    files_pile: gtk4::Box,
    last_pile: RefCell<String>,
    flash: Rc<Cell<f64>>,
    flash_vel: Rc<Cell<f64>>,
    flash_tick: Rc<Cell<Option<gtk4::TickCallbackId>>>,
    w_cur: Rc<Cell<f64>>,
    w_vel: Rc<Cell<f64>>,
    w_target: Rc<Cell<f64>>,
    h_cur: Rc<Cell<f64>>,
    h_vel: Rc<Cell<f64>>,
    h_target: Rc<Cell<f64>>,
    w_tick: Rc<Cell<Option<gtk4::TickCallbackId>>>,
    last_art_path: RefCell<Option<String>>,
    last_mood: Cell<Mood>,
    base_w: i32,
    show_clock: bool,
}

impl PillUi {
    pub fn build(
        app: &gtk4::Application,
        shared: &Rc<Shared>,
        monitor: Option<&gdk::Monitor>,
        on_click: Callback,
    ) -> Self {
        let (base_w, margin_top, show_clock) = {
            let cfg = shared.cfg.borrow();
            let w = if cfg.appearance.notch_mode {
                cfg.appearance.pill_width_notch
            } else {
                cfg.appearance.pill_width_island
            };
            (w.max(64), cfg.appearance.margin_top, cfg.clock.show_in_pill)
        };

        // Resolve the capsule colors/geometry once (config + omarchy theme).
        // use cached palette to avoid per-build file I/O
        let pal = shared.palette();
        let fill = (0u8, 0u8, 0u8);
        let flash_color = pal.accent_rgb;

        let win = ApplicationWindow::builder()
            .application(app)
            .title("naarchy-pill")
            .decorated(false)
            .resizable(true)
            .default_width(base_w)
            .default_height(PILL_H)
            .build();

        super::setup_layer(&win, monitor);
        use gtk4_layer_shell::{Edge, LayerShell};
        win.set_margin(Edge::Top, margin_top);
        win.set_width_request(base_w);
        win.set_height_request(PILL_H);

        // Left bubble — grows to the left of the notch while something is active.
        // Inset from the window edge so the glyph never rides the capsule's
        // top-left ear carve (or spills off the window during the width spring).
        let left_box = hbox(4);
        left_box.set_css_classes(&["na-bubble"]);
        left_box.set_margin_start(12);
        left_box.set_visible(false);

        let timer_icon = label(&["na-bubble-text", "na-glyph"], super::g::CLOCK);
        let media_art = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
        media_art.set_css_classes(&["na-media-art"]);
        media_art.set_valign(gtk4::Align::Center);
        media_art.set_overflow(gtk4::Overflow::Hidden);
        media_art.set_size_request(22, 22);
        media_art.set_visible(false);
        let media_icon = label(&["na-bubble-text", "na-glyph"], super::g::MUSIC);
        media_icon.set_visible(false);
        let files_icon = label(&["na-bubble-text", "na-glyph"], super::g::FOLDER);
        let files_pile = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
        files_pile.set_valign(gtk4::Align::Center);
        files_pile.set_visible(false);
        left_box.append(&timer_icon);
        left_box.append(&media_art);
        left_box.append(&media_icon);
        left_box.append(&files_icon);
        left_box.append(&files_pile);

        // Center of the island. Idle: this IS the pill. Live: it hexpands so
        // leading/trailing activities sit on the ears, not on top of each other.
        let notch_box = hbox(8);
        notch_box.set_css_classes(&["na-pill"]);
        notch_box.set_size_request(base_w, PILL_H);
        notch_box.set_halign(gtk4::Align::Fill);
        notch_box.set_valign(gtk4::Align::Center);
        notch_box.set_hexpand(true);

        let clock_label = label(&["na-chip"], "");
        clock_label.set_visible(show_clock);
        let battery_label = label(&["na-chip"], "");
        battery_label.set_visible(false);
        let mid_spacer = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
        mid_spacer.set_hexpand(true);
        notch_box.append(&clock_label);
        notch_box.append(&mid_spacer);
        notch_box.append(&battery_label);

        // Right bubble — grows to the right of the notch
        let right_box = hbox(4);
        right_box.set_css_classes(&["na-bubble"]);
        right_box.set_margin_end(12);
        right_box.set_visible(false);

        let timer_count = label(&["na-bubble-text", "na-pill-count"], "00:00");
        timer_count.set_width_chars(5);
        timer_count.set_xalign(1.0);
        let media_title = label(&["na-bubble-text"], "");
        media_title.set_ellipsize(gtk4::pango::EllipsizeMode::End);
        media_title.set_single_line_mode(true);
        media_title.set_max_width_chars(18);
        let files_count = label(&["na-bubble-text"], "0");
        right_box.append(&timer_count);
        right_box.append(&media_title);
        right_box.append(&files_count);

        // One continuous capsule: no spacing between the pieces, so the whole
        // strip (bubbles + notch) reads as a single contiguous black pill.
        let root = hbox(0);
        root.set_halign(gtk4::Align::Fill);
        root.set_valign(gtk4::Align::Fill);
        root.set_hexpand(true);
        root.set_vexpand(true);
        root.append(&left_box);
        root.append(&notch_box);
        root.append(&right_box);

        // The capsule silhouette is drawn in cairo behind the content: convex
        // bottom corners + inverted (concave) top corners = MacBook notch look.
        let flash = Rc::new(Cell::new(0.0));
        let overlay = gtk4::Overlay::new();
        let canvas = gtk4::DrawingArea::new();
        canvas.set_hexpand(true);
        canvas.set_vexpand(true);
        // canvas is the *main* child (painted first, behind the widgets);
        // the content stack is added as an overlay child so chips/text win.
        overlay.set_child(Some(&canvas));
        {
            let flash2 = flash.clone();
            let colors = NotchColors {
                fill,
                accent: flash_color,
                pulse: 0.0,
            };
            canvas.set_draw_func(move |_da, cr, w, h| {
                let (rt, rb) = super::liquid::notch_radii_for(h as f64);
                draw_notch(
                    cr,
                    w as f64,
                    h as f64,
                    rt,
                    rb,
                    NotchColors {
                        pulse: flash2.get(),
                        ..colors
                    },
                );
            });
        }
        overlay.add_overlay(&root);
        set_pill_canvas(canvas.clone());
        win.set_child(Some(&overlay));

        // Click toggles
        let click = GestureClick::new();
        {
            let cb = on_click.clone();
            click.connect_released(move |_g, _n, _x, _y| {
                if let Some(f) = cb.borrow().as_ref() {
                    f();
                }
            });
        }
        win.add_controller(click);

        // Hover directly on pill expands too
        {
            let sh = shared.clone();
            let motion = EventControllerMotion::new();
            motion.connect_enter(move |_m, _x, _y| {
                crate::app::surface_pointer_enter();
                if !sh.expanded.get() && !sh.fullscreen_hide.get() && sh.hover_enabled() {
                    sh.expand_now();
                }
            });
            motion.connect_leave(move |_m| {
                crate::app::surface_pointer_leave();
            });
            win.add_controller(motion);
        }
        super::panel::attach_file_drop(&win);

        let p = Self {
            win,
            root,
            left_box,
            mid_box: notch_box,
            right_box,
            clock_label,
            battery_label,
            timer_icon,
            timer_count,
            media_art,
            media_icon,
            media_title,
            files_icon,
            files_count,
            files_pile,
            last_pile: RefCell::new(String::new()),
            flash,
            flash_vel: Rc::new(Cell::new(0.0)),
            flash_tick: Rc::new(Cell::new(None)),
            w_cur: Rc::new(Cell::new(base_w as f64)),
            w_vel: Rc::new(Cell::new(0.0)),
            w_target: Rc::new(Cell::new(base_w as f64)),
            h_cur: Rc::new(Cell::new(PILL_H as f64)),
            h_vel: Rc::new(Cell::new(0.0)),
            h_target: Rc::new(Cell::new(PILL_H as f64)),
            w_tick: Rc::new(Cell::new(None)),
            last_art_path: RefCell::new(None),
            last_mood: Cell::new(Mood::None),
            base_w,
            show_clock,
        };
        p.win.present();
        p.tick();
        p.update_media();
        p.update_battery();
        p
    }

    /// Refresh clock + decide which bubbles wrap the notch, then resize the
    /// window to exactly notch width + whatever is active. Called every second
    /// and whenever media/battery/shelf change.
    pub fn tick(&self) {
        super::with_shared(|sh| {
            let fmt = sh.cfg.borrow().clock.format.clone();
            self.clock_label
                .set_text(&crate::timefmt::strftime_local(super::now_secs(), &fmt));
            let mood = self.update_mood(sh);
            self.last_mood.set(mood);
            self.relayout();
        });
    }

    pub fn update_media(&self) {
        super::with_shared(|sh| {
            let st = sh.media.borrow().clone();
            match st {
                Some(st) => {
                    if let Some(path) = st.art_path.clone() {
                        if self.last_art_path.borrow().as_deref() != Some(path.as_str()) {
                            *self.last_art_path.borrow_mut() = Some(path.clone());
                            while let Some(c) = self.media_art.first_child() {
                                self.media_art.remove(&c);
                            }
                            if let Ok(tex) =
                                gdk::Texture::from_filename(std::path::Path::new(&path))
                            {
                                let pic = gtk4::Picture::for_paintable(&tex);
                                pic.set_size_request(22, 22);
                                pic.set_content_fit(gtk4::ContentFit::Cover);
                                self.media_art.append(&pic);
                                self.media_art.set_visible(true);
                            }
                        }
                    } else {
                        self.media_art.set_visible(false);
                    }
                }
                None => {
                    *self.last_art_path.borrow_mut() = None;
                    while let Some(c) = self.media_art.first_child() {
                        self.media_art.remove(&c);
                    }
                    self.media_art.set_visible(false);
                }
            }
            let mood = self.update_mood(sh);
            self.last_mood.set(mood);
            self.relayout();
        });
    }

    pub fn update_battery(&self) {
        super::with_shared(|sh| {
            let b = *sh.battery.borrow();
            let bolt = if b.charging { "⚡" } else { "" };
            self.battery_label
                .set_text(&format!("{}{:.0}%", bolt, b.percent));
            let mood = self.update_mood(sh);
            self.last_mood.set(mood);
            self.relayout();
        });
    }

    /// Decide which content pair is live. One pair at a time by priority:
    /// timer done > running timer > files on the shelf > playing music.
    fn update_mood(&self, sh: &Rc<Shared>) -> Mood {
        let features = sh.cfg.borrow().features.clone();
        let timer = sh.timer.borrow();
        let media = sh.media.borrow();
        let count = sh.shelf.borrow().items().len();

        let done_on = features.timer && sh.timer_done_until.get() > super::now_secs();
        let timer_on = features.timer && timer.as_ref().is_some_and(|t| t.remaining_secs() > 0);
        let music_on = features.media && media.as_ref().is_some_and(|s| s.playing);
        let files_on = features.shelf && count > 0;

        let mood = if done_on {
            Mood::Done
        } else if timer_on {
            Mood::Timer
        } else if files_on {
            Mood::Files
        } else if music_on {
            Mood::Media
        } else {
            Mood::None
        };

        if let Some(secs) = timer.as_ref().map(|t| t.remaining_secs()) {
            self.timer_count.set_text(&fmt_mmss(secs));
        }
        if mood == Mood::Done {
            self.timer_count.set_text("Done");
        }
        if mood == Mood::Files {
            self.files_count.set_text(&format!("{}", count.min(99)));
            self.rebuild_pile(sh);
        } else if !self.last_pile.borrow().is_empty() {
            *self.last_pile.borrow_mut() = String::new();
            while let Some(c) = self.files_pile.first_child() {
                self.files_pile.remove(&c);
            }
        }
        if mood == Mood::Media {
            if let Some(st) = media.as_ref() {
                let t = if st.title.is_empty() {
                    st.player.as_str()
                } else {
                    st.title.as_str()
                };
                self.media_title.set_text(t);
            }
        }

        let live = mood != Mood::None;
        let has_art = mood == Mood::Media && self.last_art_path.borrow().is_some();
        self.media_icon.set_visible(mood == Mood::Media && !has_art);
        self.media_art.set_visible(has_art);
        self.media_art
            .set_size_request(if live { 40 } else { 22 }, if live { 40 } else { 22 });
        if live {
            self.root.add_css_class("na-pill-live");
        } else {
            self.root.remove_css_class("na-pill-live");
        }

        self.left_box.set_visible(live);
        self.right_box.set_visible(live);
        let timer_page = matches!(mood, Mood::Timer | Mood::Done);
        self.timer_icon.set_visible(timer_page);
        self.timer_count.set_visible(timer_page);
        self.media_title.set_visible(mood == Mood::Media);
        let pile_on = mood == Mood::Files && self.files_pile.first_child().is_some();
        self.files_icon.set_visible(mood == Mood::Files && !pile_on);
        self.files_pile.set_visible(pile_on);
        self.files_count.set_visible(mood == Mood::Files);

        // Camera hole stays `base_w` in the center. Live content lives on
        // the ears so a physical notch does not eat the countdown.
        self.mid_box
            .set_size_request(self.base_w, if live { LIVE_H } else { PILL_H });
        self.mid_box.set_hexpand(live);
        self.clock_label
            .set_visible(self.show_clock && mood == Mood::None);
        let batt_ok = sh.battery.borrow().present && sh.cfg.borrow().features.battery_chip;
        self.battery_label
            .set_visible(batt_ok && mood == Mood::None);
        mood
    }

    fn rebuild_pile(&self, sh: &Rc<Shared>) {
        let items = sh.shelf.borrow().items().to_vec();
        let key: String = items.iter().rev().take(3).map(|i| i.id.as_str()).collect();
        if *self.last_pile.borrow() == key {
            return;
        }
        *self.last_pile.borrow_mut() = key;
        while let Some(c) = self.files_pile.first_child() {
            self.files_pile.remove(&c);
        }
        let size = 28;
        for (i, item) in items.iter().rev().take(3).enumerate() {
            let shot = mini_thumb(item, size);
            if i > 0 {
                shot.set_margin_start(-12);
            }
            self.files_pile.append(&shot);
        }
    }

    /// Pulse the pill (timer finished, …). The cairo silhouette springs
    /// toward the accent color and back, so the flash follows the capsule.
    pub fn flash(&self) {
        self.flash.set(1.0);
        self.flash_vel.set(0.0);
        let flash = self.flash.clone();
        let vel = self.flash_vel.clone();
        let slot = self.flash_tick.clone();
        let Some(canvas) = pill_canvas() else {
            return;
        };
        canvas.queue_draw();
        let canvas2 = canvas.clone();
        motion::drive(&slot, &canvas, move |dt| {
            let (p, v) = Spring::SNAP.step(flash.get(), vel.get(), 0.0, dt);
            flash.set(p);
            vel.set(v);
            canvas2.queue_draw();
            if Spring::SNAP.settled(p, v, 0.0) {
                flash.set(0.0);
                vel.set(0.0);
                canvas2.queue_draw();
                false
            } else {
                true
            }
        });
    }

    fn min_w_for(&self, mood: Mood) -> i32 {
        // Ears have to clear the physical camera. Idle fills the notch;
        // live adds ~140px a side so "25:00" sits on the bezel, not in the hole.
        let ears = match mood {
            Mood::None => 0,
            Mood::Timer | Mood::Done => 280,
            Mood::Media => 360,
            Mood::Files => 240,
        };
        match mood {
            Mood::None => self.base_w,
            _ => (self.base_w + ears).max(560),
        }
    }

    fn h_for(mood: Mood) -> i32 {
        if mood == Mood::None {
            PILL_H
        } else {
            LIVE_H
        }
    }

    fn apply_size(&self, w: i32, h: i32) {
        apply_size(&self.win, &self.root, w, h);
    }

    /// Spring the window so live activities actually fit — wider and twice as tall.
    fn relayout(&self) {
        let mood = self.last_mood.get();
        let min = self.min_w_for(mood);
        let w_tgt = if mood == Mood::None {
            min as f64
        } else {
            let (_, nat, _, _) = self.root.measure(gtk4::Orientation::Horizontal, -1);
            nat.max(min) as f64
        };
        let h_tgt = Self::h_for(mood) as f64;
        let w_ok =
            (self.w_target.get() - w_tgt).abs() < 2.0 && (self.w_cur.get() - w_tgt).abs() < 1.0;
        let h_ok =
            (self.h_target.get() - h_tgt).abs() < 2.0 && (self.h_cur.get() - h_tgt).abs() < 1.0;
        if w_ok && h_ok {
            return;
        }
        self.w_target.set(w_tgt);
        self.h_target.set(h_tgt);
        if (self.w_cur.get() - w_tgt).abs() < 1.0 && (self.h_cur.get() - h_tgt).abs() < 1.0 {
            self.apply_size(w_tgt as i32, h_tgt as i32);
            return;
        }
        let w_cur = self.w_cur.clone();
        let w_vel = self.w_vel.clone();
        let w_target = self.w_target.clone();
        let h_cur = self.h_cur.clone();
        let h_vel = self.h_vel.clone();
        let h_target = self.h_target.clone();
        let slot = self.w_tick.clone();
        let win = self.win.clone();
        let win_tick = win.clone();
        let root = self.root.clone();
        motion::drive(&slot, &win_tick, move |dt| {
            let wt = w_target.get();
            let ht = h_target.get();
            let (pw, vw) = Spring::SNAP.step(w_cur.get(), w_vel.get(), wt, dt);
            let (ph, vh) = Spring::SNAP.step(h_cur.get(), h_vel.get(), ht, dt);
            w_cur.set(pw);
            w_vel.set(vw);
            h_cur.set(ph);
            h_vel.set(vh);
            apply_size(&win, &root, pw.round() as i32, ph.round() as i32);
            let w_done = Spring::SNAP.settled(pw, vw, wt);
            let h_done = Spring::SNAP.settled(ph, vh, ht);
            if w_done && h_done {
                w_cur.set(wt);
                w_vel.set(0.0);
                h_cur.set(ht);
                h_vel.set(0.0);
                apply_size(&win, &root, wt.round() as i32, ht.round() as i32);
                false
            } else {
                true
            }
        });
    }
}

fn mini_thumb(item: &crate::shelf_store::ShelfItem, size: i32) -> gtk4::Box {
    let wrap = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
    wrap.set_css_classes(&["na-pile-shot"]);
    wrap.set_size_request(size, size);
    wrap.set_overflow(gtk4::Overflow::Hidden);
    wrap.set_valign(gtk4::Align::Center);
    if (item.kind == "image" || item.mime.starts_with("image/")) && !item.path.is_empty() {
        if let Ok(tex) = gdk::Texture::from_filename(std::path::Path::new(&item.path)) {
            let pic = gtk4::Picture::for_paintable(&tex);
            pic.set_size_request(size, size);
            pic.set_content_fit(gtk4::ContentFit::Cover);
            wrap.append(&pic);
            return wrap;
        }
    }
    let glyph = match item.mime.as_str() {
        m if m.starts_with("image/") => super::g::IMAGE,
        m if m.starts_with("text/") => super::g::TEXT,
        _ if item.kind == "text" => super::g::TEXT,
        _ => super::g::FILE,
    };
    let l = label(&["na-glyph"], glyph);
    l.set_halign(gtk4::Align::Center);
    l.set_hexpand(true);
    wrap.append(&l);
    wrap
}

fn apply_size(win: &ApplicationWindow, root: &gtk4::Box, w: i32, h: i32) {
    let w = w.max(1);
    let h = h.max(1);
    win.set_default_size(w, h);
    win.set_size_request(w, h);
    win.set_width_request(w);
    win.set_height_request(h);
    root.set_width_request(w);
    root.set_height_request(h);
    if let Some(c) = pill_canvas() {
        c.set_content_width(w);
        c.set_content_height(h);
        c.set_size_request(w, h);
        c.queue_draw();
    }
    win.queue_resize();
}

thread_local! {
    /// Handle to the pill's canvas so the flash loop can reach it.
    static PILL_CANVAS: RefCell<Option<gtk4::DrawingArea>> = const { RefCell::new(None) };
}

fn set_pill_canvas(c: gtk4::DrawingArea) {
    PILL_CANVAS.with(|w| *w.borrow_mut() = Some(c));
}

fn pill_canvas() -> Option<gtk4::DrawingArea> {
    PILL_CANVAS.with(|w| w.borrow().clone())
}

/// Draw the notch silhouette, matching the canonical macOS "dynamic island /
/// notch" shape: the top edge is the widest line (flush with the screen top),
/// its corners cut away by concave ear curves; the side walls stay straight
/// and inset; the bottom corners round outward and the bottom edge pulls in,
/// so the pill tapers toward a narrow rounded base.
struct NotchColors {
    fill: (u8, u8, u8),
    accent: (u8, u8, u8),
    pulse: f64,
}

fn draw_notch(cr: &gtk4::cairo::Context, w: f64, h: f64, rt: f64, rb: f64, colors: NotchColors) {
    let p = colors.pulse.clamp(0.0, 1.0);
    let r = colors.fill.0 as f64 * (1.0 - p) + colors.accent.0 as f64 * p;
    let g = colors.fill.1 as f64 * (1.0 - p) + colors.accent.1 as f64 * p;
    let b = colors.fill.2 as f64 * (1.0 - p) + colors.accent.2 as f64 * p;

    if w < 2.0 * (rt + rb) + 4.0 || h < rt + rb + 2.0 {
        cr.set_source_rgb(r / 255.0, g / 255.0, b / 255.0);
        let _ = cr.paint();
        return;
    }

    super::liquid::path_notch(cr, 0.0, 0.0, w, h, rt, rb);
    cr.set_source_rgb(r / 255.0, g / 255.0, b / 255.0);
    let _ = cr.fill();
}

#[allow(dead_code)]
fn truncate(s: &str, n: usize) -> String {
    s.chars().take(n).collect()
}

impl Shared {
    pub fn hover_enabled(&self) -> bool {
        self.cfg.borrow().behavior.hover_open
    }
    pub fn expand_now(&self) {
        if let Some(f) = self.expand_all_cb.borrow().as_ref() {
            f();
        }
    }
}
