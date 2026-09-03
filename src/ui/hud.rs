use super::motion::{self, Spring};
use crate::services::Banner;
use gtk4::gdk;
use gtk4::prelude::*;
use gtk4::{glib, ApplicationWindow, DrawingArea, Label};
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Instant;

pub struct HudManager {
    pub app: glib::WeakRef<gtk4::Application>,
    win: Option<ApplicationWindow>,
    arc: Option<Rc<HudArc>>,
    label: Option<Label>,
    timeout: Rc<Cell<Option<glib::SourceId>>>,
    tick: Rc<Cell<Option<gtk4::TickCallbackId>>>,
    banners: Vec<ApplicationWindow>,
    bells: Vec<ApplicationWindow>,
    bell_src: Rc<Cell<Option<glib::SourceId>>>,
}

impl HudManager {
    pub fn new(app: &gtk4::Application) -> Self {
        Self {
            app: app.downgrade(),
            win: None,
            arc: None,
            label: None,
            timeout: Rc::new(Cell::new(None)),
            tick: Rc::new(Cell::new(None)),
            banners: Vec::new(),
            bells: Vec::new(),
            bell_src: Rc::new(Cell::new(None)),
        }
    }

    pub fn show(
        &mut self,
        kind: &str,
        value: Option<f64>,
        step: Option<f64>,
        icon: Option<String>,
        label_txt: Option<String>,
    ) {
        let Some(app) = self.app.upgrade() else {
            return;
        };
        super::with_shared(|sh| {
            if self.win.is_none() {
                let (w, a, l) = build_hud_window(&app);
                self.win = Some(w);
                self.arc = Some(a);
                self.label = Some(l);
            }
            let arc = self.arc.as_ref().unwrap();
            let lab = self.label.as_ref().unwrap();
            let win = self.win.as_ref().unwrap();

            let (glyph, pct, lbl) = hud_params(kind, value, step, icon, label_txt);
            arc.set_glyph(glyph);
            arc.set_target(pct);
            lab.set_text(&lbl);

            if let Some(src) = self.timeout.take() {
                src.remove();
            }
            let ms = sh.cfg.borrow().hud.timeout_ms.max(300);
            let appearing = !win.is_visible();
            win.set_visible(true);
            if appearing {
                win.set_opacity(0.0);
            }
            let tick = self.tick.clone();
            let arc2 = arc.clone();
            let win2 = win.clone();
            let op_cur = Rc::new(Cell::new(if appearing { 0.0 } else { 1.0 }));
            let op_vel = Rc::new(Cell::new(0.0));
            motion::drive(&tick, win, move |dt| {
                let (p, v) = Spring::SNAP.step(op_cur.get(), op_vel.get(), 1.0, dt);
                op_cur.set(p);
                op_vel.set(v);
                win2.set_opacity(p.clamp(0.0, 1.0));
                let going = arc2.tick(dt);
                let open = !Spring::SNAP.settled(p, v, 1.0);
                going || open
            });
            let weak = win.downgrade();
            let tflag = self.timeout.clone();
            let src =
                glib::timeout_add_local_once(std::time::Duration::from_millis(ms), move || {
                    if let Some(w) = weak.upgrade() {
                        let w3 = w.clone();
                        motion::tween(&w, 180, move |t| w3.set_opacity(1.0 - t), {
                            let w4 = w.clone();
                            move || w4.set_visible(false)
                        });
                    }
                    tflag.set(None);
                });
            self.timeout.set(Some(src));
        });
    }

    pub fn show_banner(&mut self, b: Banner, shared: &Rc<super::Shared>) {
        let Some(app) = self.app.upgrade() else {
            return;
        };
        // cap concurrent banners — destroy oldest to free layer-shell surface
        while self.banners.len() >= 3 {
            if let Some(old) = self.banners.first() {
                old.close();
            }
            self.banners.remove(0);
        }
        let (win, _card) = make_banner_window(&app, &b, shared);
        win.set_opacity(0.0);
        let w_in = win.clone();
        motion::tween(&win, 220, move |t| w_in.set_opacity(t), || {});
        self.banners.push(win);
        self.reflow();
        // auto-dismiss non-critical (critical stays until dismissed)
        if b.urgency != 2 && b.id != u32::MAX - 1 {
            let wref = self.banners.last().map(|w| w.downgrade());
            let cmd_tx = shared.notif_cmd.borrow().clone();
            let id = b.id;
            glib::timeout_add_local_once(std::time::Duration::from_millis(5_000), move || {
                if let Some(w) = wref.and_then(|x| x.upgrade()) {
                    let w2 = w.clone();
                    motion::tween(
                        &w,
                        180,
                        {
                            let w3 = w.clone();
                            move |t| w3.set_opacity(1.0 - t)
                        },
                        move || {
                            w2.set_visible(false);
                            if id != u32::MAX - 1 {
                                if let Some(tx) = &cmd_tx {
                                    let _ = tx.send(crate::services::notifd::NotifCmd::Close {
                                        id,
                                        reason: 1,
                                    });
                                }
                            }
                        },
                    );
                }
            });
        }
    }

    /// Full-screen visual bell on every monitor. Click or any key dismisses
    /// the timer (and this overlay).
    pub fn ring_bell(&mut self) {
        self.silence_bell();
        let Some(app) = self.app.upgrade() else {
            return;
        };
        let Some(display) = gdk::Display::default() else {
            return;
        };
        let n = display.monitors().n_items();
        let started = Instant::now();
        let areas: Rc<RefCell<Vec<DrawingArea>>> = Rc::new(RefCell::new(Vec::new()));
        let mut grabbed = false;
        for i in 0..n {
            let Some(m) = display.monitors().item(i).and_downcast::<gdk::Monitor>() else {
                continue;
            };
            let grab = !grabbed;
            grabbed = true;
            let (win, area) = make_bell_window(&app, Some(&m), started, grab);
            areas.borrow_mut().push(area);
            self.bells.push(win);
        }
        if self.bells.is_empty() {
            let (win, area) = make_bell_window(&app, None, started, true);
            areas.borrow_mut().push(area);
            self.bells.push(win);
        }
        let src_slot = self.bell_src.clone();
        let areas2 = areas.clone();
        let src = glib::timeout_add_local(std::time::Duration::from_millis(32), move || {
            for a in areas2.borrow().iter() {
                a.queue_draw();
            }
            glib::ControlFlow::Continue
        });
        src_slot.set(Some(src));
    }

    pub fn silence_bell(&mut self) {
        if let Some(src) = self.bell_src.take() {
            src.remove();
        }
        for w in self.bells.drain(..) {
            w.close();
        }
    }

    fn reflow(&mut self) {
        use gtk4_layer_shell::{Edge, LayerShell};
        // drop closed/hidden banners and free their surfaces
        self.banners.retain(|w| w.is_visible());
        for (i, w) in self.banners.iter().enumerate() {
            w.set_margin(Edge::Top, 52 + (i as i32 * 78));
        }
    }
}

fn hud_params(
    kind: &str,
    value: Option<f64>,
    step: Option<f64>,
    icon: Option<String>,
    label_txt: Option<String>,
) -> (String, f64, String) {
    let base = match kind {
        "volume" | "vol" => ("🔊", value.unwrap_or(50.0), "Volume".to_string()),
        "brightness" | "bright" => ("☀", value.unwrap_or(60.0), "Brightness".to_string()),
        "mic" => ("🎙", value.unwrap_or(0.0), "Microphone".to_string()),
        "battery" => ("🔋", value.unwrap_or(80.0), "Battery".to_string()),
        "caps" => ("⇪", 100.0, "Caps Lock".to_string()),
        _ => ("●", value.unwrap_or(50.0), "HUD".to_string()),
    };
    let pct = match step {
        Some(s) => (base.1 + s).clamp(0.0, 100.0),
        None => base.1.clamp(0.0, 100.0),
    };
    let lbl = label_txt.unwrap_or_else(|| format!("{:.0}%", pct));
    (icon.unwrap_or_else(|| base.0.to_string()), pct, lbl)
}

fn build_hud_window(app: &gtk4::Application) -> (ApplicationWindow, Rc<HudArc>, Label) {
    let win = ApplicationWindow::builder()
        .application(app)
        .title("naarchy-hud")
        .decorated(false)
        .resizable(false)
        .default_width(250)
        .default_height(76)
        .build();
    super::setup_layer(&win, None);
    use gtk4_layer_shell::{Edge, LayerShell};
    win.set_margin(Edge::Top, 52);
    win.set_visible(false);

    let root = gtk4::Box::new(gtk4::Orientation::Horizontal, 12);
    root.set_css_classes(&["na-hud"]);
    root.set_margin_start(16);
    root.set_margin_end(16);
    root.set_margin_top(10);
    root.set_margin_bottom(10);

    let arc = Rc::new(HudArc::new());
    arc.area.set_size_request(56, 56);
    root.append(&arc.area);

    let label = Label::new(Some("50%"));
    label.set_hexpand(true);
    label.set_xalign(0.0);
    root.append(&label);

    let pad = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    pad.append(&root);
    win.set_child(Some(&pad));

    let click = gtk4::GestureClick::new();
    {
        let w2 = win.clone();
        click.connect_released(move |_g, _n, _x, _y| w2.set_visible(false));
    }
    root.add_controller(click);
    (win, arc, label)
}

/// Ring + glyph widget painted via cairo on GTK's snapshot draw func.
pub struct HudArc {
    area: DrawingArea,
    state: Rc<RefCell<(String, f64)>>,
    shown: Rc<Cell<f64>>,
    vel: Rc<Cell<f64>>,
    target: Rc<Cell<f64>>,
}

impl HudArc {
    pub fn new() -> Self {
        let area = DrawingArea::new();
        let state: Rc<RefCell<(String, f64)>> = Rc::new(RefCell::new(("🔊".into(), 50.0)));
        let shown = Rc::new(Cell::new(50.0));
        {
            let shown2 = shown.clone();
            let st = state.clone();
            area.set_draw_func(move |_area, cr, w, h| {
                let (glyph, _) = st.borrow().clone();
                let pct: f64 = shown2.get();
                let cx = w as f64 / 2.0;
                let cy = h as f64 / 2.0;
                let r = (w.min(h) as f64 / 2.0) - 5.0;

                cr.set_source_rgba(1.0, 1.0, 1.0, 0.18);
                cr.set_line_width(6.0);
                cr.arc(cx, cy, r, 0.0, std::f64::consts::TAU);
                let _ = cr.stroke();

                let frac = pct.clamp(0.0, 100.0) / 100.0;
                if frac > 0.001 {
                    cr.set_source_rgba(1.0, 1.0, 1.0, 0.95);
                    cr.set_line_cap(gdk4_cairo_hack_line_cap());
                    cr.arc(
                        cx,
                        cy,
                        r,
                        -std::f64::consts::FRAC_PI_2,
                        -std::f64::consts::FRAC_PI_2 + frac * std::f64::consts::TAU,
                    );
                    let _ = cr.stroke();
                }

                cr.set_source_rgba(1.0, 1.0, 1.0, 1.0);
                cr.select_font_face(
                    "sans",
                    gtk4::cairo::FontSlant::Normal,
                    gtk4::cairo::FontWeight::Bold,
                );
                cr.set_font_size(r * 0.8);
                if let Ok(e) = cr.text_extents(&glyph) {
                    cr.move_to(
                        cx - e.width() / 2.0 - e.x_bearing(),
                        cy - e.height() / 2.0 - e.y_bearing(),
                    );
                } else {
                    cr.move_to(cx, cy);
                }
                let _ = cr.show_text(&glyph);
            });
        }
        Self {
            area,
            state,
            shown,
            vel: Rc::new(Cell::new(0.0)),
            target: Rc::new(Cell::new(50.0)),
        }
    }

    pub fn set_glyph(&self, glyph: impl Into<String>) {
        self.state.borrow_mut().0 = glyph.into();
    }

    pub fn set_target(&self, pct: f64) {
        self.target.set(pct.clamp(0.0, 100.0));
        self.state.borrow_mut().1 = pct;
    }

    fn tick(&self, dt: f64) -> bool {
        let (p, v) = Spring::SNAP.step(self.shown.get(), self.vel.get(), self.target.get(), dt);
        self.shown.set(p);
        self.vel.set(v);
        self.area.queue_draw();
        !Spring::SNAP.settled(p, v, self.target.get())
    }
}

fn gdk4_cairo_hack_line_cap() -> gtk4::cairo::LineCap {
    gtk4::cairo::LineCap::Round
}

pub fn make_banner_window(
    app: &gtk4::Application,
    b: &Banner,
    shared: &Rc<super::Shared>,
) -> (ApplicationWindow, gtk4::Box) {
    let win = ApplicationWindow::builder()
        .application(app)
        .title("naarchy-banner")
        .decorated(false)
        .resizable(false)
        .default_width(400)
        .default_height(66)
        .build();
    super::setup_layer(&win, None);
    use gtk4_layer_shell::{Edge, LayerShell};
    win.set_margin(Edge::Top, 52);
    win.set_visible(true);

    let card = gtk4::Box::new(gtk4::Orientation::Vertical, 4);
    let mut classes: Vec<&str> = vec!["na-banner"];
    if b.urgency == 2 {
        classes.push("critical");
    }
    card.set_css_classes(&classes);
    card.set_margin_start(12);
    card.set_margin_end(12);
    card.set_margin_top(6);
    card.set_margin_bottom(6);

    let head = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
    let ic = super::label(&["na-glyph"], super::g::INBOX);
    let sum = super::label(&["na-title"], &b.summary);
    sum.set_ellipsize(gtk4::pango::EllipsizeMode::End);
    let sp = super::label(&[""], "");
    sp.set_hexpand(true);
    head.append(&ic);
    head.append(&sum);
    head.append(&sp);
    if !b.app_name.is_empty() && b.app_name != "naarchy" {
        head.append(&super::label(&["na-dim"], &b.app_name));
    }
    card.append(&head);

    if !b.body.is_empty() {
        let body = super::label(&["na-dim"], &strip_markup(&b.body));
        body.set_wrap(true);
        body.set_wrap_mode(gtk4::pango::WrapMode::WordChar);
        body.set_max_width_chars(48);
        card.append(&body);
    }

    if !b.actions.is_empty() {
        let acts = gtk4::Box::new(gtk4::Orientation::Horizontal, 6);
        for (key, labeltxt) in &b.actions {
            let btn = gtk4::Button::with_label(labeltxt);
            btn.set_css_classes(&["na-banner-action"]);
            let cmd_tx = shared.notif_cmd.borrow().clone();
            let id = b.id;
            let key2 = key.clone();
            let w2 = win.clone();
            btn.connect_clicked(move |_| {
                if let Some(tx) = &cmd_tx {
                    let _ = tx.send(crate::services::notifd::NotifCmd::Action {
                        id,
                        key: key2.clone(),
                    });
                }
                w2.set_visible(false);
            });
            acts.append(&btn);
        }
        card.append(&acts);
    }

    win.set_child(Some(&card));

    let click = gtk4::GestureClick::new();
    {
        let cmd_tx = shared.notif_cmd.borrow().clone();
        let id = b.id;
        let w2 = win.clone();
        click.connect_released(move |_g, _n, _x, _y| {
            w2.set_visible(false);
            if id != u32::MAX - 1 {
                if let Some(tx) = &cmd_tx {
                    let _ = tx.send(crate::services::notifd::NotifCmd::Close { id, reason: 1 });
                }
            }
        });
    }
    card.add_controller(click);

    (win, card)
}

fn make_bell_window(
    app: &gtk4::Application,
    monitor: Option<&gdk::Monitor>,
    started: Instant,
    grab_keyboard: bool,
) -> (ApplicationWindow, DrawingArea) {
    let win = ApplicationWindow::builder()
        .application(app)
        .title("naarchy-bell")
        .decorated(false)
        .resizable(true)
        .build();
    use gtk4_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};
    win.init_layer_shell();
    win.set_layer(Layer::Overlay);
    win.set_anchor(Edge::Top, true);
    win.set_anchor(Edge::Bottom, true);
    win.set_anchor(Edge::Left, true);
    win.set_anchor(Edge::Right, true);
    win.set_exclusive_zone(-1);
    win.set_monitor(monitor);
    // Own namespace so Hyprland's `layerrule = blur, naarchy` does not
    // eat the takeover into a fog.
    win.set_namespace(Some("naarchy-bell"));
    win.set_keyboard_mode(if grab_keyboard {
        KeyboardMode::Exclusive
    } else {
        KeyboardMode::OnDemand
    });
    win.add_css_class("naarchy");

    let area = DrawingArea::new();
    area.set_hexpand(true);
    area.set_vexpand(true);
    area.set_draw_func(move |_da, cr, w, h| {
        let t = started.elapsed().as_secs_f64();
        let a = bell_alpha(t);
        cr.set_source_rgba(1.0, 0.97, 0.92, a);
        let _ = cr.paint();
        let label = "Time's up";
        cr.select_font_face(
            "sans",
            gtk4::cairo::FontSlant::Normal,
            gtk4::cairo::FontWeight::Bold,
        );
        cr.set_font_size((h as f64 * 0.08).clamp(28.0, 72.0));
        cr.set_source_rgba(0.07, 0.07, 0.08, (a * 1.15).clamp(0.0, 1.0));
        if let Ok(e) = cr.text_extents(label) {
            cr.move_to(
                w as f64 * 0.5 - e.width() / 2.0 - e.x_bearing(),
                h as f64 * 0.5 - e.height() / 2.0 - e.y_bearing(),
            );
            let _ = cr.show_text(label);
        }
    });
    win.set_child(Some(&area));

    let click = gtk4::GestureClick::new();
    click.connect_released(move |_g, _n, _x, _y| {
        crate::app::dismiss_timer();
    });
    win.add_controller(click);

    let key = gtk4::EventControllerKey::new();
    key.connect_key_pressed(move |_k, _keyval, _code, _mod| {
        crate::app::dismiss_timer();
        gtk4::glib::Propagation::Stop
    });
    win.add_controller(key);

    win.present();
    (win, area)
}

/// Visual-bell envelope: slam on, then a hard strobe, then a slower pulse.
fn bell_alpha(t: f64) -> f64 {
    if t < 0.06 {
        0.94
    } else if t < 3.0 {
        0.22 + 0.72 * (t * std::f64::consts::PI * 8.0).sin().abs()
    } else {
        0.28 + 0.52 * ((t * 2.2).sin().abs())
    }
}

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
    out
}
