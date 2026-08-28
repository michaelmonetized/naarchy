use super::{fmt_mmss, g, glyph_btn, label, now_secs, Shared, TimerState};
use gtk4::cairo::{FontSlant, FontWeight};
use gtk4::prelude::*;
use gtk4::{Button, GestureClick, Label, ScrolledWindow};
use std::rc::Rc;

const MIN_PX: f64 = 7.0;
const MAX_MIN: u32 = 120;
const MARGIN: f64 = 14.0;

/// Time ruler widget: a scrollable row of ticks. Clicking a tick starts a
/// countdown of that many minutes; the elapsed portion fills in accent color
/// with a marching knob.
pub struct TimerUi {
    root: gtk4::Box,
    ruler: gtk4::DrawingArea,
    scroller: ScrolledWindow,
    big: Label,
    status: Label,
    pause_btn: Button,
    reset_btn: Button,
}

impl TimerUi {
    pub fn build(shared: &Rc<Shared>) -> Self {
        let root = super::vbox(10);
        root.set_halign(gtk4::Align::Fill);
        root.set_valign(gtk4::Align::Center);
        root.set_hexpand(true);

        let presets = super::hbox(6);
        presets.set_halign(gtk4::Align::Start);
        for (lbl, secs) in [("1m", 60u64), ("5m", 300), ("10m", 600), ("25m", 1500)] {
            let b = Button::with_label(lbl);
            b.set_css_classes(&["na-preset"]);
            let sh = shared.clone();
            b.connect_clicked(move |_| start_timer(&sh, secs));
            presets.append(&b);
        }
        root.append(&presets);

        let ruler = gtk4::DrawingArea::new();
        ruler.set_hexpand(true);
        ruler.set_height_request(84);
        ruler.set_width_request(80 + (MAX_MIN as f64 * MIN_PX) as i32);
        {
            let sh = shared.clone();
            ruler.set_draw_func(move |_da, cr, w, h| {
                draw_ruler(cr, &sh, w as f64, h as f64);
            });
        }
        {
            let sh = shared.clone();
            let click = GestureClick::new();
            click.connect_pressed(move |_g, _n, x, _y| {
                let m = ((x - MARGIN) / MIN_PX) as u32;
                if (1..=MAX_MIN).contains(&m) {
                    start_timer(&sh, m as u64 * 60);
                }
            });
            ruler.add_controller(click);
        }

        let scroller = ScrolledWindow::builder()
            .child(&ruler)
            .hscrollbar_policy(gtk4::PolicyType::Automatic)
            .vscrollbar_policy(gtk4::PolicyType::Never)
            .has_frame(false)
            .overlay_scrolling(true)
            .build();
        scroller.set_css_classes(&["na-scroll"]);
        scroller.set_hexpand(true);
        scroller.set_height_request(88);
        root.append(&scroller);

        let big = label(&["na-clock-big"], "00:00");
        big.set_halign(gtk4::Align::Start);
        let status = label(&["na-dim"], "Pick a time");
        root.append(&big);

        let controls = super::hbox(8);
        let pause_btn = glyph_btn(&["na-btn", "play"], g::PAUSE);
        pause_btn.set_tooltip_text(Some("Pause"));
        {
            let sh = shared.clone();
            pause_btn.connect_clicked(move |_| toggle_pause(&sh));
        }
        let reset_btn = Button::with_label("Reset");
        reset_btn.set_css_classes(&["na-btn", "ghost"]);
        {
            let sh = shared.clone();
            reset_btn.connect_clicked(move |_| {
                *sh.timer.borrow_mut() = None;
                sh.timer_done_until.set(0);
            });
        }
        controls.append(&pause_btn);
        controls.append(&reset_btn);
        controls.append(&status);
        root.append(&controls);

        let p = Self {
            root,
            ruler,
            scroller,
            big,
            status,
            pause_btn,
            reset_btn,
        };
        p.refresh();
        p
    }

    pub fn root(&self) -> &gtk4::Box {
        &self.root
    }

    pub fn start(&self, secs: u64) {
        super::with_shared(|sh| start_timer(sh, secs.max(1)));
        self.refresh();
    }

    pub fn refresh(&self) {
        super::with_shared(|sh| {
            match sh.timer.borrow().as_ref() {
                Some(t) => {
                    let rem = t.remaining_secs();
                    self.big.set_text(&fmt_mmss(rem));
                    self.status.set_text(if rem == 0 {
                        "Done"
                    } else if t.running() {
                        "Running"
                    } else {
                        "Paused"
                    });
                    self.pause_btn.set_child(Some(&super::label(
                        &["na-glyph"],
                        if t.paused_remaining.is_some() {
                            g::PLAY
                        } else {
                            g::PAUSE
                        },
                    )));
                    self.pause_btn.set_sensitive(true);
                    self.reset_btn.set_sensitive(true);
                }
                None => {
                    self.big.set_text("00:00");
                    self.status.set_text("Pick a time");
                    self.pause_btn
                        .set_child(Some(&super::label(&["na-glyph"], g::PLAY)));
                    self.pause_btn.set_sensitive(false);
                    self.reset_btn.set_sensitive(false);
                }
            }
            self.ruler.queue_draw();
        });
    }

    pub fn tick(&self) {
        super::with_shared(|sh| {
            let now = now_secs();
            let fired = {
                match sh.timer.borrow().as_ref() {
                    Some(t) if t.remaining_secs() == 0 && t.running() => {
                        sh.timer_done_until.set(now + 6);
                        true
                    }
                    _ => false,
                }
            };
            if fired {
                crate::app::notify_ui("Timer done", "Time is up.");
                crate::chime::play();
                crate::app::flash_pills();
            }
            let done = sh.timer_done_until.get();
            if done > 0 && now >= done {
                sh.timer_done_until.set(0);
                *sh.timer.borrow_mut() = None;
            }
            if let Some(t) = sh.timer.borrow().as_ref() {
                if t.running() {
                    let frac = elapsed_frac(t);
                    let knob = MARGIN + frac * t.total as f64 * MIN_PX;
                    let adj = self.scroller.hadjustment();
                    let upper = adj.upper().max(adj.page_size());
                    let v =
                        (knob - adj.page_size() / 2.0).clamp(adj.lower(), upper - adj.page_size());
                    adj.set_value(v);
                }
            }
        });
        self.refresh();
    }
}

fn elapsed_frac(t: &TimerState) -> f64 {
    let total = t.total.max(1);
    let remaining = t.remaining_secs();
    let elapsed = total.saturating_sub(remaining) as f64;
    (elapsed / total as f64).clamp(0.0, 1.0)
}

fn draw_ruler(cr: &gtk4::cairo::Context, sh: &Rc<Shared>, w: f64, h: f64) {
    let accent = crate::theme::accent_hex(&sh.cfg.borrow());
    let (ar, ag, ab) = crate::theme::hex_triple(&accent).unwrap_or((0x7a, 0xa2, 0xf7));
    let mid = h * 0.42;
    let bar_y = mid + 22.0;
    let knob_y = mid + 22.0;

    cr.set_line_width(1.5);
    for m in 0..=MAX_MIN {
        let x = MARGIN + m as f64 * MIN_PX;
        let (th, major) = if m % 60 == 0 {
            (22.0, true)
        } else if m % 15 == 0 {
            (15.0, true)
        } else if m % 5 == 0 {
            (10.0, false)
        } else {
            (5.0, false)
        };
        cr.set_source_rgba(1.0, 1.0, 1.0, if major { 0.42 } else { 0.16 });
        cr.move_to(x, mid);
        cr.line_to(x, mid - th);
        let _ = cr.stroke();
        if major {
            let lbl = if m % 60 == 0 {
                format!("{}h", m / 60)
            } else {
                format!("{m}m")
            };
            cr.select_font_face("sans-serif", FontSlant::Normal, FontWeight::Normal);
            cr.set_font_size(9.0);
            cr.set_source_rgba(1.0, 1.0, 1.0, 0.45);
            cr.move_to(x - 8.0, mid - th - 4.0);
            let _ = cr.show_text(&lbl);
        }
    }

    if let Some(t) = sh.timer.borrow().as_ref() {
        let frac = elapsed_frac(t);
        let fill_x = MARGIN + frac * t.total as f64 * MIN_PX;
        cr.set_source_rgba(0.0, 0.0, 0.0, 0.35);
        cr.rectangle(MARGIN, mid - 24.0, w - MARGIN * 2.0, 30.0);
        let _ = cr.fill();
        cr.rectangle(MARGIN, mid - 24.0, fill_x.max(MARGIN) - MARGIN, 30.0);
        cr.set_source_rgba(
            ar as f64 / 255.0,
            ag as f64 / 255.0,
            ab as f64 / 255.0,
            0.28,
        );
        let _ = cr.fill();
        cr.set_source_rgba(
            ar as f64 / 255.0,
            ag as f64 / 255.0,
            ab as f64 / 255.0,
            0.95,
        );
        cr.rectangle(MARGIN, bar_y - 2.0, (fill_x - MARGIN).max(0.0), 4.0);
        let _ = cr.fill();
        cr.set_source_rgba(1.0, 1.0, 1.0, 0.95);
        cr.arc(fill_x, knob_y, 5.0, 0.0, std::f64::consts::TAU);
        let _ = cr.fill();
    }
}

fn start_timer(shared: &Rc<Shared>, secs: u64) {
    *shared.timer.borrow_mut() = Some(TimerState {
        end_at: now_secs() + secs,
        paused_remaining: None,
        total: secs,
    });
}

fn toggle_pause(shared: &Rc<Shared>) {
    let mut tref = shared.timer.borrow_mut();
    if let Some(t) = tref.as_mut() {
        if t.remaining_secs() == 0 {
            return;
        }
        if let Some(rem) = t.paused_remaining {
            t.end_at = now_secs() + rem;
            t.paused_remaining = None;
        } else {
            t.paused_remaining = Some(t.remaining_secs());
        }
    }
}
