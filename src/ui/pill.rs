use super::motion::{self, Spring};
use super::{fmt_mmss, hbox, label, Callback, Shared};
use gtk4::prelude::*;
use gtk4::{gdk, ApplicationWindow, EventControllerMotion, GestureClick, Label};
use std::cell::{Cell, RefCell};
use std::rc::Rc;

const PILL_H: i32 = 84;
const PILL_W: i32 = 367;

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
    right_box: gtk4::Box,
    clock_label: Label,
    battery_label: Label,
    timer_icon: Label,
    timer_count: Label,
    media_art: gtk4::Box,
    media_title: Label,
    files_icon: Label,
    files_count: Label,
    flash: Rc<Cell<f64>>,
    flash_vel: Rc<Cell<f64>>,
    flash_tick: Rc<Cell<Option<gtk4::TickCallbackId>>>,
    w_cur: Rc<Cell<f64>>,
    w_vel: Rc<Cell<f64>>,
    w_target: Rc<Cell<f64>>,
    w_tick: Rc<Cell<Option<gtk4::TickCallbackId>>>,
    last_art_path: RefCell<Option<String>>,
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
            (
                w.max(PILL_W),
                cfg.appearance.margin_top,
                cfg.clock.show_in_pill,
            )
        };

        // Resolve the capsule colors/geometry once (config + omarchy theme).
        let pal = crate::theme::resolve(&shared.cfg.borrow(), shared.dark.get());
        let fill = crate::theme::hex_triple(&pal.pill_fill).unwrap_or((0, 0, 0));
        let flash_color = pal.accent_rgb;
        let radius = shared.cfg.borrow().appearance.radius.max(18);
        // Notch geometry (matches the canonical DynamicNotchKit shape): the
        // top edge is the pill's WIDEST line — it sits flush against the top
        // of the screen, its top corners cut away by concave "ear" curves
        // (bezier control point rides the top edge). Straight inset sides,
        // then the bottom corners round outward and the bottom edge pulls in,
        // so the pill tapers toward a narrower rounded base.
        let top_r = (radius as f64 * 0.5).clamp(8.0, 18.0);
        // The bottom taper must leave the clock/battery chips fully inside the
        // capsule: constrain it so the flat side-wall region reaches past the
        // vertically-centered content before the rounding begins.
        let bot_r = ((radius as f64 * 0.9).clamp(16.0, 30.0)).min(PILL_H as f64 / 2.0 - 20.0);

        let win = ApplicationWindow::builder()
            .application(app)
            .title("naarchy-pill")
            .decorated(false)
            .resizable(false)
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
        let files_icon = label(&["na-bubble-text", "na-glyph"], super::g::FOLDER);
        left_box.append(&timer_icon);
        left_box.append(&media_art);
        left_box.append(&files_icon);

        // Center notch / island
        let notch_box = hbox(8);
        notch_box.set_css_classes(&["na-pill"]);
        notch_box.set_size_request(base_w, PILL_H);
        notch_box.set_halign(gtk4::Align::Center);
        notch_box.set_valign(gtk4::Align::Center);

        let clock_label = label(&["na-chip"], "");
        clock_label.set_visible(show_clock);
        let battery_label = label(&["na-chip"], "");
        battery_label.set_visible(false);
        notch_box.append(&clock_label);
        notch_box.append(&battery_label);

        // Right bubble — grows to the right of the notch
        let right_box = hbox(4);
        right_box.set_css_classes(&["na-bubble"]);
        right_box.set_margin_end(12);
        right_box.set_visible(false);

        let timer_count = label(&["na-bubble-text"], "00:00");
        let media_title = label(&["na-bubble-text"], "");
        media_title.set_ellipsize(gtk4::pango::EllipsizeMode::End);
        media_title.set_single_line_mode(true);
        media_title.set_max_width_chars(24);
        let files_count = label(&["na-bubble-text"], "0");
        right_box.append(&timer_count);
        right_box.append(&media_title);
        right_box.append(&files_count);

        // One continuous capsule: no spacing between the pieces, so the whole
        // strip (bubbles + notch) reads as a single contiguous black pill.
        let root = hbox(0);
        root.set_valign(gtk4::Align::Center);
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
                draw_notch(
                    cr,
                    w as f64,
                    h as f64,
                    top_r,
                    bot_r,
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
                if !sh.expanded.get() && !sh.fullscreen_hide.get() && sh.hover_enabled() {
                    sh.expand_now();
                }
            });
            win.add_controller(motion);
        }

        let p = Self {
            win,
            root,
            left_box,
            right_box,
            clock_label,
            battery_label,
            timer_icon,
            timer_count,
            media_art,
            media_title,
            files_icon,
            files_count,
            flash,
            flash_vel: Rc::new(Cell::new(0.0)),
            flash_tick: Rc::new(Cell::new(None)),
            w_cur: Rc::new(Cell::new(base_w as f64)),
            w_vel: Rc::new(Cell::new(0.0)),
            w_target: Rc::new(Cell::new(base_w as f64)),
            w_tick: Rc::new(Cell::new(None)),
            last_art_path: RefCell::new(None),
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
            self.update_mood(sh);
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
            self.update_mood(sh);
            self.relayout();
        });
    }

    pub fn update_battery(&self) {
        super::with_shared(|sh| {
            let b = *sh.battery.borrow();
            if !b.present || !sh.cfg.borrow().features.battery_chip {
                self.battery_label.set_visible(false);
                return;
            }
            self.battery_label.set_visible(true);
            let bolt = if b.charging { "⚡" } else { "" };
            self.battery_label
                .set_text(&format!("{}{:.0}%", bolt, b.percent));
        });
    }

    /// Decide which content pair is live. One pair at a time by priority:
    /// timer done > running timer > playing music > files in the drop zone.
    fn update_mood(&self, sh: &Rc<Shared>) {
        let features = sh.cfg.borrow().features.clone();
        let timer = sh.timer.borrow();
        let media = sh.media.borrow();
        let count = sh.shelf.borrow().items().len();

        let done_on = features.timer && sh.timer_done_until.get() > super::now_secs();
        let timer_on = features.timer && timer.as_ref().is_some_and(|t| t.remaining_secs() > 0);
        let music_on = features.media && media.is_some();
        let files_on = features.shelf && count > 0;

        let mood = if done_on {
            Mood::Done
        } else if timer_on {
            Mood::Timer
        } else if music_on {
            Mood::Media
        } else if files_on {
            Mood::Files
        } else {
            Mood::None
        };

        if let Some(secs) = timer.as_ref().map(|t| t.remaining_secs()) {
            self.timer_count.set_text(&fmt_mmss(secs));
        }
        if mood == Mood::Done {
            self.timer_icon.set_text(super::g::CLOCK);
            self.timer_count.set_text("Done");
        } else {
            self.timer_icon.set_text(super::g::CLOCK);
        }
        if let Some(st) = media.as_ref() {
            let txt = if !st.title.is_empty() {
                format!("{} · {}", truncate(&st.title, 22), truncate(&st.artist, 14))
            } else {
                st.player.clone()
            };
            self.media_title.set_text(&txt);
        }
        if mood == Mood::Files {
            self.files_count.set_text(&format!("{}", count.min(99)));
        }

        // Icon left of the notch, countdown/text right of it.
        let bubble_on = matches!(mood, Mood::Timer | Mood::Done | Mood::Files);
        self.left_box.set_visible(bubble_on);
        self.right_box.set_visible(mood != Mood::None);
        let timer_page = matches!(mood, Mood::Timer | Mood::Done);
        self.timer_icon.set_visible(timer_page);
        self.timer_count.set_visible(timer_page);
        self.media_title.set_visible(mood == Mood::Media);
        self.files_icon.set_visible(mood == Mood::Files);
        self.files_count.set_visible(mood == Mood::Files);
        // Album art only fills the left bubble when we actually have art.
        let art_ok = mood == Mood::Media && self.media_art.is_visible();
        self.media_art.set_visible(art_ok);
        if mood == Mood::Media && !art_ok {
            self.left_box.set_visible(false);
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

    /// Re-measure and spring the window width so it wraps notch + active bubbles.
    fn relayout(&self) {
        let (_, nat, _, _) = self.root.measure(gtk4::Orientation::Horizontal, -1);
        let target = nat.max(PILL_W) as f64;
        self.w_target.set(target);
        if (self.w_cur.get() - target).abs() < 1.0 {
            self.win.set_width_request(target as i32);
            return;
        }
        let w_cur = self.w_cur.clone();
        let w_vel = self.w_vel.clone();
        let w_target = self.w_target.clone();
        let slot = self.w_tick.clone();
        let win = self.win.clone();
        let win_tick = win.clone();
        motion::drive(&slot, &win_tick, move |dt| {
            let tgt = w_target.get();
            let (p, v) = Spring::SNAP.step(w_cur.get(), w_vel.get(), tgt, dt);
            w_cur.set(p);
            w_vel.set(v);
            win.set_width_request(p.round() as i32);
            if Spring::SNAP.settled(p, v, tgt) {
                w_cur.set(tgt);
                w_vel.set(0.0);
                win.set_width_request(tgt as i32);
                false
            } else {
                true
            }
        });
    }
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

    // Start at the top-left corner and walk clockwise (y-down).
    cr.move_to(0.0, 0.0);
    // Top-left ear: concave carve (control point rides the top edge).
    quad_to(cr, 0.0, 0.0, rt, 0.0, rt, rt);
    // Straight left wall, inset by rt.
    cr.line_to(rt, h - rb);
    // Bottom-left convex corner (control point below, on the bottom edge).
    quad_to(cr, rt, h - rb, rt, h, rt + rb, h);
    // Bottom edge, pulled in further by the corner rounding.
    cr.line_to(w - rt - rb, h);
    // Bottom-right convex corner.
    quad_to(cr, w - rt - rb, h, w - rt, h, w - rt, h - rb);
    // Straight right wall.
    cr.line_to(w - rt, rt);
    // Top-right ear.
    quad_to(cr, w - rt, rt, w - rt, 0.0, w, 0.0);
    // Close along the top edge back to the top-left corner.
    cr.close_path();

    cr.set_source_rgb(r / 255.0, g / 255.0, b / 255.0);
    let _ = cr.fill();
}

/// Append a quadratic bezier from the current point to `(ex, ey)` with control
/// point `(cx, cy)`, via the exact cubic equivalent (control points
/// `(s+2c)/3` and `(2c+e)/3`).
fn quad_to(cr: &gtk4::cairo::Context, sx: f64, sy: f64, cx: f64, cy: f64, ex: f64, ey: f64) {
    cr.curve_to(
        (sx + 2.0 * cx) / 3.0,
        (sy + 2.0 * cy) / 3.0,
        (2.0 * cx + ex) / 3.0,
        (2.0 * cy + ey) / 3.0,
        ex,
        ey,
    );
}

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
