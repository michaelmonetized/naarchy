use super::{fmt_mmss, g, glyph_btn, label, now_secs, Shared, TimerState};
use gtk4::prelude::*;
use gtk4::{Button, Entry, Label, Revealer};
use std::cell::Cell;
use std::rc::Rc;

/// Polished timer: circular progress + big clock + quick presets + custom entry
/// Visual bell pulses when done, audible chime repeats until dismissed.
pub struct TimerUi {
    root: gtk4::Box,
    ring: gtk4::DrawingArea,
    big: Label,
    status: Label,
    sub: Label,
    pause_btn: Button,
    reset_btn: Button,
    dismiss_btn: Button,
    bell_revealer: Revealer,
    #[allow(dead_code)]
    custom_entry: Entry,
    last_chime: Rc<Cell<u64>>,
}

impl TimerUi {
    pub fn build(shared: &Rc<Shared>) -> Self {
        let root = super::vbox(12);
        root.set_halign(gtk4::Align::Fill);
        root.set_valign(gtk4::Align::Center);
        root.set_hexpand(true);

        // Presets row
        let presets = super::hbox(6);
        presets.set_halign(gtk4::Align::Center);
        for (lbl, secs) in [
            ("30s", 30u64),
            ("1m", 60),
            ("5m", 300),
            ("10m", 600),
            ("25m", 1500),
        ] {
            let b = Button::with_label(lbl);
            b.set_css_classes(&["na-preset"]);
            let sh = shared.clone();
            b.connect_clicked(move |_| start_timer(&sh, secs));
            presets.append(&b);
        }
        // Custom entry + Go
        let custom_entry = Entry::new();
        custom_entry.set_placeholder_text(Some("25m / 90s"));
        custom_entry.set_width_request(90);
        custom_entry.set_css_classes(&["na-entry", "na-timer-entry"]);
        custom_entry.set_max_length(8);
        let go = Button::with_label("Go");
        go.set_css_classes(&["na-btn", "ghost"]);
        {
            let sh = shared.clone();
            let entry = custom_entry.clone();
            let entry2 = entry.clone();
            let do_go = Rc::new(move || {
                let txt = entry.text().to_string();
                if let Some(secs) = parse_custom(&txt) {
                    start_timer(&sh, secs);
                    entry.set_text("");
                }
            });
            let d2 = do_go.clone();
            go.connect_clicked(move |_| d2());
            let d3 = do_go.clone();
            entry2.connect_activate(move |_| d3());
        }
        presets.append(&custom_entry);
        presets.append(&go);
        root.append(&presets);

        // Ring + clock overlay
        let ring = gtk4::DrawingArea::new();
        ring.set_size_request(148, 148);
        ring.set_halign(gtk4::Align::Center);
        ring.set_valign(gtk4::Align::Center);
        {
            let sh = shared.clone();
            ring.set_draw_func(move |_da, cr, w, h| {
                draw_ring(cr, &sh, w as f64, h as f64);
            });
        }
        let big = label(&["na-clock-big", "na-timer-big"], "00:00");
        big.set_halign(gtk4::Align::Center);
        big.set_valign(gtk4::Align::Center);

        let status = label(&["na-dim", "na-timer-status"], "Pick a time");
        status.set_halign(gtk4::Align::Center);
        let sub = label(&["na-mute"], "");
        sub.set_halign(gtk4::Align::Center);

        // Bell overlay (revealed when done)
        let bell_box = super::vbox(6);
        bell_box.set_halign(gtk4::Align::Center);
        let bell_icon = label(&["na-timer-bell"], "🔔");
        bell_icon.set_halign(gtk4::Align::Center);
        let bell_text = label(&["na-timer-bell-text"], "Time’s up!");
        bell_text.set_halign(gtk4::Align::Center);
        bell_box.append(&bell_icon);
        bell_box.append(&bell_text);
        let bell_revealer = Revealer::new();
        bell_revealer.set_transition_type(gtk4::RevealerTransitionType::SlideDown);
        bell_revealer.set_transition_duration(220);
        bell_revealer.set_reveal_child(false);
        bell_revealer.set_child(Some(&bell_box));
        bell_revealer.set_halign(gtk4::Align::Center);

        let center = gtk4::Overlay::new();
        center.set_halign(gtk4::Align::Center);
        center.set_size_request(168, 168);
        center.set_child(Some(&ring));
        center.add_overlay(&big);

        let middle = super::vbox(4);
        middle.set_halign(gtk4::Align::Center);
        middle.append(&center);
        middle.append(&status);
        middle.append(&sub);
        middle.append(&bell_revealer);
        root.append(&middle);

        let controls = super::hbox(8);
        controls.set_halign(gtk4::Align::Center);
        let pause_btn = glyph_btn(&["na-btn", "play"], g::PAUSE);
        pause_btn.set_tooltip_text(Some("Pause / Resume"));
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
        let dismiss_btn = Button::with_label("Dismiss");
        dismiss_btn.set_css_classes(&["na-btn", "ghost", "na-timer-dismiss"]);
        dismiss_btn.set_visible(false);
        {
            let sh = shared.clone();
            dismiss_btn.connect_clicked(move |_| {
                *sh.timer.borrow_mut() = None;
                sh.timer_done_until.set(0);
            });
        }
        controls.append(&pause_btn);
        controls.append(&reset_btn);
        controls.append(&dismiss_btn);
        root.append(&controls);

        let p = Self {
            root,
            ring,
            big,
            status,
            sub,
            pause_btn,
            reset_btn,
            dismiss_btn,
            bell_revealer,
            custom_entry,
            last_chime: Rc::new(Cell::new(0)),
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
            let done_on = sh.timer_done_until.get() > now_secs();
            match sh.timer.borrow().as_ref() {
                Some(t) => {
                    let rem = t.remaining_secs();
                    self.big.set_text(&fmt_mmss(rem));
                    if done_on || rem == 0 && !t.running() && t.paused_remaining.is_none() {
                        self.status.set_text("Done — Time’s up!");
                        self.sub.set_text("🔔  Dismiss to stop the bell");
                        self.bell_revealer.set_reveal_child(true);
                        self.big.add_css_class("na-timer-done");
                        self.pause_btn.set_sensitive(false);
                        self.reset_btn.set_sensitive(true);
                        self.dismiss_btn.set_visible(true);
                        self.pause_btn
                            .set_child(Some(&super::label(&["na-glyph"], g::PLAY)));
                    } else if t.paused_remaining.is_some() {
                        self.status.set_text("Paused");
                        self.sub.set_text(&format!("{} remaining", fmt_mmss(rem)));
                        self.bell_revealer.set_reveal_child(false);
                        self.big.remove_css_class("na-timer-done");
                        self.pause_btn.set_sensitive(true);
                        self.reset_btn.set_sensitive(true);
                        self.dismiss_btn.set_visible(false);
                        self.pause_btn
                            .set_child(Some(&super::label(&["na-glyph"], g::PLAY)));
                    } else {
                        self.status.set_text("Running");
                        let total = t.total;
                        self.sub.set_text(&format!(
                            "{} / {} elapsed",
                            fmt_mmss(total.saturating_sub(rem)),
                            fmt_mmss(total)
                        ));
                        self.bell_revealer.set_reveal_child(false);
                        self.big.remove_css_class("na-timer-done");
                        self.pause_btn.set_sensitive(true);
                        self.reset_btn.set_sensitive(true);
                        self.dismiss_btn.set_visible(false);
                        self.pause_btn
                            .set_child(Some(&super::label(&["na-glyph"], g::PAUSE)));
                    }
                }
                None => {
                    if done_on {
                        self.big.set_text("00:00");
                        self.status.set_text("Done — Time’s up!");
                        self.sub.set_text("🔔  Dismiss to stop the bell");
                        self.bell_revealer.set_reveal_child(true);
                        self.big.add_css_class("na-timer-done");
                        self.pause_btn.set_sensitive(false);
                        self.reset_btn.set_sensitive(true);
                        self.dismiss_btn.set_visible(true);
                    } else {
                        self.big.set_text("00:00");
                        self.status.set_text("Pick a time");
                        self.sub.set_text("Presets or type 25m / 90s above");
                        self.bell_revealer.set_reveal_child(false);
                        self.big.remove_css_class("na-timer-done");
                        self.pause_btn
                            .set_child(Some(&super::label(&["na-glyph"], g::PLAY)));
                        self.pause_btn.set_sensitive(false);
                        self.reset_btn.set_sensitive(false);
                        self.dismiss_btn.set_visible(false);
                    }
                }
            }
            // only redraw ring when it actually shows progress or done pulse
            let needs_ring = sh.timer.borrow().is_some() || sh.timer_done_until.get() > now_secs();
            if needs_ring {
                self.ring.queue_draw();
            }
        });
    }

    pub fn tick(&self) {
        super::with_shared(|sh| {
            let now = now_secs();
            let fired = {
                match sh.timer.borrow().as_ref() {
                    Some(t) if t.remaining_secs() == 0 && t.running() => {
                        // keep ringing for 60s, not 6s, so bell and sound loop have time
                        sh.timer_done_until.set(now + 60);
                        true
                    }
                    _ => false,
                }
            };
            if fired {
                self.last_chime.set(now);
                crate::app::request_collapse_all();
                crate::app::notify_ui("Timer done", "Time is up — dismiss to stop the bell.");
                crate::chime::play();
                // also try system bell as fallback
                crate::chime::system_bell();
                crate::app::flash_pills();
                // ensure panel is visible if user wants visual bell inside?
            } else {
                // looping audible alarm every 3s while done
                let done = sh.timer_done_until.get();
                if done > now && done > 0 {
                    let last = self.last_chime.get();
                    if now.saturating_sub(last) >= 3 {
                        self.last_chime.set(now);
                        crate::chime::play();
                        crate::chime::system_bell();
                        crate::app::flash_pills();
                    }
                }
            }
            let done = sh.timer_done_until.get();
            if done > 0 && now >= done {
                sh.timer_done_until.set(0);
                *sh.timer.borrow_mut() = None;
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

fn remaining_frac(t: &TimerState) -> f64 {
    1.0 - elapsed_frac(t)
}

fn draw_ring(cr: &gtk4::cairo::Context, sh: &Rc<Shared>, w: f64, h: f64) {
    let (ar, ag, ab) = sh.accent_rgb();
    let cx = w * 0.5;
    let cy = h * 0.5;
    let radius = (w.min(h) * 0.5 - 8.0).max(8.0);
    let line_w = 7.0;

    let done_on = sh.timer_done_until.get() > now_secs();
    let frac = if let Some(t) = sh.timer.borrow().as_ref() {
        if done_on {
            1.0
        } else {
            remaining_frac(t)
        }
    } else if done_on {
        1.0
    } else {
        0.0
    };

    // track
    cr.set_line_width(line_w);
    cr.set_line_cap(gtk4::cairo::LineCap::Round);
    cr.set_source_rgba(1.0, 1.0, 1.0, 0.10);
    cr.arc(cx, cy, radius, 0.0, std::f64::consts::TAU);
    let _ = cr.stroke();

    if frac <= 0.001 && !done_on {
        return;
    }

    // glow backdrop when done
    if done_on {
        cr.set_source_rgba(
            ar as f64 / 255.0,
            ag as f64 / 255.0,
            ab as f64 / 255.0,
            0.18,
        );
        cr.arc(cx, cy, radius + 6.0, 0.0, std::f64::consts::TAU);
        let _ = cr.fill();
    }

    // progress arc (remaining)
    let start = -std::f64::consts::FRAC_PI_2;
    let end = start + frac * std::f64::consts::TAU;
    cr.set_source_rgba(
        ar as f64 / 255.0,
        ag as f64 / 255.0,
        ab as f64 / 255.0,
        if done_on { 0.95 } else { 0.92 },
    );
    cr.arc(cx, cy, radius, start, end);
    let _ = cr.stroke();

    // knob at end of arc when running
    if !done_on && frac > 0.01 && frac < 0.999 {
        let ang = end;
        let kx = cx + radius * ang.cos();
        let ky = cy + radius * ang.sin();
        cr.set_source_rgba(1.0, 1.0, 1.0, 0.95);
        cr.arc(kx, ky, 5.0, 0.0, std::f64::consts::TAU);
        let _ = cr.fill();
        cr.set_source_rgba(
            ar as f64 / 255.0,
            ag as f64 / 255.0,
            ab as f64 / 255.0,
            0.55,
        );
        cr.set_line_width(2.0);
        cr.arc(kx, ky, 5.0, 0.0, std::f64::consts::TAU);
        let _ = cr.stroke();
    }

    // center done pulse
    if done_on {
        let pulse = (now_secs() % 2) as f64 * 0.5;
        cr.set_source_rgba(
            ar as f64 / 255.0,
            ag as f64 / 255.0,
            ab as f64 / 255.0,
            0.12 + pulse * 0.08,
        );
        cr.arc(cx, cy, radius * 0.62, 0.0, std::f64::consts::TAU);
        let _ = cr.fill();
    }
}

fn parse_custom(s: &str) -> Option<u64> {
    let s = s.trim().to_lowercase();
    if s.is_empty() {
        return None;
    }
    // accept "25m" "90s" "1h" "1:30" "90" etc.
    if let Some((m, sec)) = s.split_once(':') {
        if let (Ok(mm), Ok(ss)) = (m.parse::<u64>(), sec.parse::<u64>()) {
            return Some(mm * 60 + ss);
        }
    }
    let (num, unit): (String, String) = s.chars().partition(|c| c.is_ascii_digit());
    if num.is_empty() {
        return None;
    }
    let n: u64 = num.parse().ok()?;
    match unit.as_str() {
        "s" | "sec" | "secs" | "seconds" => Some(n),
        "m" | "min" | "mins" | "minutes" => Some(n * 60),
        "h" | "hr" | "hour" | "hours" => Some(n * 3600),
        "" => Some(n * 60), // bare number = minutes for UX
        _ => None,
    }
}

fn start_timer(shared: &Rc<Shared>, secs: u64) {
    *shared.timer.borrow_mut() = Some(TimerState {
        end_at: now_secs() + secs,
        paused_remaining: None,
        total: secs,
    });
    shared.timer_done_until.set(0);
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
