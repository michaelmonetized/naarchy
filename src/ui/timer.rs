use super::{fmt_hms, label, now_secs, Shared, TimerState};
use gtk4::cairo::{Context, FontSlant, FontWeight, LineCap};
use gtk4::gdk;
use gtk4::prelude::*;
use gtk4::{Button, EventControllerScroll, EventControllerScrollFlags, GestureDrag, Label};
use std::cell::Cell;
use std::rc::Rc;

const PX_PER_MIN: f64 = 12.0;
const MIN_SECS: u64 = 1;
const MAX_SECS: u64 = 24 * 3600;
const DEFAULT_SECS: u64 = 5 * 60;
const CLICK_PX: f64 = 6.0;

/// Timer card: a horizontal minute ruler and a big `h:mm:ss` readout.
/// Release the ruler to start. Click it while running to pause, click again
/// to resume.
pub struct TimerUi {
    root: gtk4::Box,
    ruler: gtk4::DrawingArea,
    time_lbl: Label,
    start_btn: Button,
    picked: Rc<Cell<u64>>,
}

impl TimerUi {
    pub fn build(shared: &Rc<Shared>) -> Self {
        let picked = Rc::new(Cell::new(DEFAULT_SECS));

        let root = super::vbox(10);
        root.set_halign(gtk4::Align::Fill);
        root.set_valign(gtk4::Align::Center);
        root.set_hexpand(true);

        let ruler = gtk4::DrawingArea::new();
        ruler.set_hexpand(true);
        ruler.set_height_request(72);
        ruler.set_halign(gtk4::Align::Fill);
        ruler.set_cursor(gdk::Cursor::from_name("sb_h_double_arrow", None).as_ref());
        {
            let sh = shared.clone();
            let picked = picked.clone();
            ruler.set_draw_func(move |_da, cr, w, h| {
                let minutes = display_minutes(&sh, picked.get());
                let (ar, ag, ab) = sh.accent_rgb();
                let done = sh.timer_done_until.get() > now_secs();
                draw_ruler(cr, w as f64, h as f64, minutes, (ar, ag, ab), done);
            });
        }
        root.append(&ruler);

        let time_lbl = label(&["na-timer-hms"], &fmt_hms(DEFAULT_SECS));
        time_lbl.set_halign(gtk4::Align::End);
        time_lbl.set_hexpand(true);
        time_lbl.set_valign(gtk4::Align::Center);

        let row = super::hbox(10);
        row.set_halign(gtk4::Align::Fill);
        row.set_hexpand(true);
        row.set_valign(gtk4::Align::Center);

        let start_btn = Button::with_label("Start Timer");
        start_btn.set_css_classes(&["na-timer-start"]);
        start_btn.set_valign(gtk4::Align::Center);
        {
            let sh = shared.clone();
            let picked = picked.clone();
            let time2 = time_lbl.clone();
            let ruler2 = ruler.clone();
            start_btn.connect_clicked(move |btn| {
                handle_press(&sh, picked.get(), false, &time2, btn, &ruler2);
            });
        }

        let drag_origin = Rc::new(Cell::new(0.0_f64));
        let drag = GestureDrag::new();
        {
            let sh = shared.clone();
            let picked = picked.clone();
            let origin = drag_origin.clone();
            drag.connect_drag_begin(move |_, _, _| {
                if is_running(&sh) || is_done(&sh) {
                    return;
                }
                origin.set(scrub_origin(&sh, picked.get()));
            });
        }
        {
            let sh = shared.clone();
            let picked = picked.clone();
            let origin = drag_origin.clone();
            let ruler2 = ruler.clone();
            let time2 = time_lbl.clone();
            drag.connect_drag_update(move |_, dx, _| {
                if is_running(&sh) || is_done(&sh) {
                    return;
                }
                let mins = origin.get() - dx / PX_PER_MIN;
                let secs = clamp_secs((mins * 60.0).round().max(0.0) as u64);
                picked.set(secs);
                time2.set_text(&fmt_hms(secs));
                ruler2.queue_draw();
            });
        }
        {
            let sh = shared.clone();
            let picked = picked.clone();
            let ruler2 = ruler.clone();
            let time2 = time_lbl.clone();
            let btn2 = start_btn.clone();
            drag.connect_drag_end(move |_, dx, dy| {
                let dragged = dx.hypot(dy) >= CLICK_PX;
                if !is_running(&sh) && !is_done(&sh) {
                    let mins = ((picked.get() as f64) / 60.0).round().max(1.0);
                    picked.set(clamp_secs((mins as u64).saturating_mul(60)));
                }
                handle_press(&sh, picked.get(), dragged, &time2, &btn2, &ruler2);
            });
        }
        ruler.add_controller(drag);

        let scroll = EventControllerScroll::new(EventControllerScrollFlags::BOTH_AXES);
        {
            let sh = shared.clone();
            let picked = picked.clone();
            let ruler2 = ruler.clone();
            let time2 = time_lbl.clone();
            scroll.connect_scroll(move |_, dx, dy| {
                if is_running(&sh) || is_done(&sh) {
                    return gtk4::glib::Propagation::Stop;
                }
                let delta = if dy.abs() >= dx.abs() { dy } else { dx };
                let mins = picked.get() as f64 / 60.0 - delta;
                let snapped = mins.round().clamp(1.0, (MAX_SECS / 60) as f64);
                let secs = clamp_secs((snapped as u64).saturating_mul(60));
                picked.set(secs);
                time2.set_text(&fmt_hms(secs));
                ruler2.queue_draw();
                gtk4::glib::Propagation::Stop
            });
        }
        ruler.add_controller(scroll);

        row.append(&start_btn);
        row.append(&time_lbl);
        root.append(&row);

        let p = Self {
            root,
            ruler,
            time_lbl,
            start_btn,
            picked,
        };
        p.refresh();
        p
    }

    pub fn root(&self) -> &gtk4::Box {
        &self.root
    }

    pub fn start(&self, secs: u64) {
        let secs = clamp_secs(secs.max(MIN_SECS));
        self.picked.set(secs);
        super::with_shared(|sh| start_timer(sh, secs));
        self.refresh();
    }

    pub fn refresh(&self) {
        super::with_shared(|sh| {
            paint(
                sh,
                self.picked.get(),
                &self.time_lbl,
                &self.start_btn,
                &self.ruler,
            );
        });
    }

    pub fn tick(&self) {
        self.refresh();
    }
}

fn is_done(sh: &Rc<Shared>) -> bool {
    sh.timer_done_until.get() > now_secs()
}

fn is_running(sh: &Rc<Shared>) -> bool {
    sh.timer.borrow().as_ref().is_some_and(|t| t.running())
}

fn is_paused(sh: &Rc<Shared>) -> bool {
    sh.timer
        .borrow()
        .as_ref()
        .is_some_and(|t| t.paused_remaining.is_some() && t.remaining_secs() > 0)
}

fn scrub_origin(sh: &Rc<Shared>, picked: u64) -> f64 {
    if let Some(t) = sh.timer.borrow().as_ref() {
        if t.paused_remaining.is_some() {
            return t.remaining_secs() as f64 / 60.0;
        }
    }
    picked as f64 / 60.0
}

/// Ruler release / Start button.
///
/// Idle: start the picked duration. Running: pause. Paused click: resume.
/// Paused after a real drag: start the newly picked duration. Done: dismiss.
fn handle_press(
    sh: &Rc<Shared>,
    picked: u64,
    dragged: bool,
    time_lbl: &Label,
    start_btn: &Button,
    ruler: &gtk4::DrawingArea,
) {
    if is_done(sh) {
        crate::app::dismiss_timer();
        paint(sh, picked, time_lbl, start_btn, ruler);
        return;
    }
    if is_running(sh) {
        toggle_pause(sh);
        paint(sh, picked, time_lbl, start_btn, ruler);
        return;
    }
    if is_paused(sh) && !dragged {
        toggle_pause(sh);
        paint(sh, picked, time_lbl, start_btn, ruler);
        return;
    }
    start_timer(sh, picked.max(MIN_SECS));
    paint(sh, picked, time_lbl, start_btn, ruler);
}

fn paint(
    sh: &Rc<Shared>,
    picked: u64,
    time_lbl: &Label,
    start_btn: &Button,
    ruler: &gtk4::DrawingArea,
) {
    let done_on = is_done(sh);
    match sh.timer.borrow().as_ref() {
        Some(t) => {
            let rem = t.remaining_secs();
            time_lbl.set_text(&fmt_hms(rem));
            if done_on || rem == 0 && !t.running() && t.paused_remaining.is_none() {
                time_lbl.add_css_class("na-timer-done");
                start_btn.set_label("Dismiss");
            } else if t.paused_remaining.is_some() {
                time_lbl.remove_css_class("na-timer-done");
                start_btn.set_label("Resume");
            } else {
                time_lbl.remove_css_class("na-timer-done");
                start_btn.set_label("Pause");
            }
        }
        None => {
            if done_on {
                time_lbl.set_text(&fmt_hms(0));
                time_lbl.add_css_class("na-timer-done");
                start_btn.set_label("Dismiss");
            } else {
                time_lbl.set_text(&fmt_hms(picked));
                time_lbl.remove_css_class("na-timer-done");
                start_btn.set_label("Start Timer");
            }
        }
    }
    let grab = !is_running(sh) && !done_on;
    ruler.set_cursor(
        gdk::Cursor::from_name(if grab { "sb_h_double_arrow" } else { "pointer" }, None).as_ref(),
    );
    ruler.queue_draw();
}

fn display_minutes(sh: &Rc<Shared>, picked: u64) -> f64 {
    if is_done(sh) {
        return 0.0;
    }
    if let Some(t) = sh.timer.borrow().as_ref() {
        return t.remaining_secs() as f64 / 60.0;
    }
    picked as f64 / 60.0
}

fn clamp_secs(secs: u64) -> u64 {
    secs.clamp(MIN_SECS, MAX_SECS)
}

fn draw_ruler(cr: &Context, w: f64, h: f64, minutes: f64, accent: (u8, u8, u8), done: bool) {
    if w < 8.0 || h < 8.0 {
        return;
    }
    let (ar, ag, ab) = (
        accent.0 as f64 / 255.0,
        accent.1 as f64 / 255.0,
        accent.2 as f64 / 255.0,
    );
    let cx = w * 0.5;
    let tick_top = 22.0;
    let major_h = 26.0;
    let minor_h = 12.0;
    let pulse = if done {
        0.55 + 0.45 * ((now_secs() % 2) as f64)
    } else {
        1.0
    };

    let min_visible = minutes - (cx / PX_PER_MIN) - 1.0;
    let max_visible = minutes + (cx / PX_PER_MIN) + 1.0;
    let start = min_visible.floor() as i64;
    let end = max_visible.ceil() as i64;
    let max_min = (MAX_SECS / 60) as i64;

    cr.set_line_cap(LineCap::Round);
    cr.select_font_face("sans", FontSlant::Normal, FontWeight::Bold);
    cr.set_font_size(11.0);

    for m in start..=end {
        if m < 0 || m > max_min {
            continue;
        }
        let x = cx + (m as f64 - minutes) * PX_PER_MIN;
        if x < -20.0 || x > w + 20.0 {
            continue;
        }
        let edge = ((x - cx).abs() / (cx.max(1.0))).clamp(0.0, 1.0);
        let fade = (1.0 - edge * edge) * pulse;
        let major = m % 5 == 0;
        let th = if major { major_h } else { minor_h };
        cr.set_line_width(if major { 2.0 } else { 1.2 });
        cr.set_source_rgba(ar, ag, ab, (if major { 0.92 } else { 0.45 }) * fade);
        cr.move_to(x, tick_top);
        cr.line_to(x, tick_top + th);
        let _ = cr.stroke();
        if major && fade > 0.12 {
            let txt = format!("{m}");
            if let Ok(e) = cr.text_extents(&txt) {
                cr.set_source_rgba(ar, ag, ab, 0.95 * fade);
                cr.move_to(x - e.width() / 2.0 - e.x_bearing(), 14.0);
                let _ = cr.show_text(&txt);
            }
        }
    }

    let py = tick_top + major_h + 8.0;
    cr.set_source_rgba(ar, ag, ab, 0.95 * pulse);
    cr.move_to(cx, py);
    cr.line_to(cx - 6.0, py + 9.0);
    cr.line_to(cx + 6.0, py + 9.0);
    cr.close_path();
    let _ = cr.fill();
}

fn start_timer(shared: &Rc<Shared>, secs: u64) {
    crate::chime::alarm_stop();
    crate::app::silence_bell();
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clamp_keeps_the_ruler_in_range() {
        assert_eq!(clamp_secs(0), 1);
        assert_eq!(clamp_secs(MAX_SECS + 10), MAX_SECS);
        assert_eq!(clamp_secs(300), 300);
    }
}
