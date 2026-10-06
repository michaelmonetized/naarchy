//! Small passive layer attached visually to the pill. Its entire input region
//! is empty, including the artwork hanging below the island.

use super::{liquid, motion};
use crate::seasonal::Costume;
use gtk4::cairo::Context;
use gtk4::prelude::*;
use gtk4_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};
use std::cell::{Cell, RefCell};
use std::rc::Rc;

pub struct Decoration {
    costume: Costume,
    win: gtk4::ApplicationWindow,
    canvas: gtk4::DrawingArea,
    pill_height: Rc<Cell<f64>>,
    tick: Rc<Cell<Option<gtk4::TickCallbackId>>>,
}

impl Decoration {
    fn build(pill: &gtk4::ApplicationWindow, costume: Costume) -> Self {
        let win = gtk4::ApplicationWindow::builder()
            .application(&pill.application().expect("pill application"))
            .title("naarchy-halloween")
            .decorated(false)
            .focusable(false)
            .resizable(true)
            .build();
        win.init_layer_shell();
        win.set_layer(Layer::Overlay);
        win.set_monitor(pill.monitor().as_ref());
        win.set_anchor(Edge::Top, true);
        win.set_margin(Edge::Top, pill.margin(Edge::Top));
        win.set_exclusive_zone(-1);
        win.set_keyboard_mode(KeyboardMode::None);
        win.set_namespace(Some("naarchy-halloween"));
        win.add_css_class("naarchy");
        let canvas = gtk4::DrawingArea::new();
        canvas.set_can_target(false);
        canvas.set_hexpand(true);
        canvas.set_vexpand(true);
        let pill_height = Rc::new(Cell::new(pill.height().max(32) as f64));
        let gain = Rc::new(Cell::new(0.0));
        let height = pill_height.clone();
        let gain_draw = gain.clone();
        canvas.set_draw_func(move |_, cr, w, _| {
            draw(cr, costume, w as f64 / 2.0, height.get(), gain_draw.get());
        });
        win.set_child(Some(&canvas));
        win.connect_realize(liquid::clear_input_region);
        let weak_win = win.downgrade();
        canvas.connect_resize(move |_, _, _| {
            let weak_win = weak_win.clone();
            gtk4::glib::idle_add_local_once(move || {
                if let Some(win) = weak_win.upgrade() {
                    liquid::clear_input_region(&win);
                }
            });
        });
        let tick = Rc::new(Cell::new(None));
        let decoration = Self {
            costume,
            win,
            canvas,
            pill_height,
            tick,
        };
        decoration.resize(pill);
        decoration.win.set_visible(true);
        let area = decoration.canvas.clone();
        let velocity = Cell::new(0.0);
        motion::drive(&decoration.tick, &decoration.canvas, move |dt| {
            let (p, v) = motion::Spring::SNAP.step(gain.get(), velocity.get(), 1.0, dt);
            gain.set(p.clamp(0.0, 1.0));
            velocity.set(v);
            let settled = motion::Spring::SNAP.settled(p, v, 1.0);
            if settled {
                gain.set(1.0);
            }
            area.queue_draw();
            !settled
        });
        decoration
    }

    fn resize(&self, pill: &gtk4::ApplicationWindow) {
        let height = pill.height().max(32);
        self.pill_height.set(height as f64);
        self.canvas
            .set_size_request(pill.width().max(190), height + 52);
        self.win
            .set_default_size(pill.width().max(190), height + 52);
        self.canvas.queue_draw();
    }
}

impl Drop for Decoration {
    fn drop(&mut self) {
        if let Some(tick) = self.tick.take() {
            tick.remove();
        }
        liquid::clear_input_region(&self.win);
        self.win.destroy();
    }
}

pub fn update(
    slot: &RefCell<Option<Decoration>>,
    pill: &gtk4::ApplicationWindow,
    costume: Option<Costume>,
) {
    let costume = costume.filter(|_| pill.is_visible());
    let mut current = slot.borrow_mut();
    if current.as_ref().map(|d| d.costume) != costume {
        current.take();
        if let Some(costume) = costume {
            *current = Some(Decoration::build(pill, costume));
        }
    } else if let Some(decoration) = current.as_ref() {
        decoration.resize(pill);
    }
}

pub(super) fn draw(cr: &Context, costume: Costume, cx: f64, top: f64, gain: f64) {
    let _ = cr.save();
    cr.translate(cx, top - 2.0);
    cr.scale(1.0, gain.max(0.001));
    match costume {
        Costume::Fangs => {
            for x in [-38.0, 38.0] {
                cr.move_to(x - 8.0, 0.0);
                cr.line_to(x + 8.0, 0.0);
                cr.line_to(x + 1.0, 23.0);
                cr.close_path();
            }
            cr.set_source_rgb(0.95, 0.91, 0.8);
            let _ = cr.fill();
        }
        Costume::Drips => {
            cr.set_source_rgb(0.7, 0.13, 0.21);
            for (x, length, radius) in [
                (-49.0, 16.0, 3.5),
                (-20.0, 28.0, 4.0),
                (28.0, 20.0, 3.0),
                (47.0, 12.0, 3.0),
            ] {
                cr.rectangle(x - radius, 0.0, radius * 2.0, length);
                cr.arc(x, length, radius, 0.0, std::f64::consts::TAU);
                let _ = cr.fill();
            }
        }
        Costume::Bat => {
            cr.set_source_rgb(0.63, 0.55, 0.79);
            cr.set_line_width(1.5);
            cr.move_to(0.0, 0.0);
            cr.line_to(0.0, 12.0);
            let _ = cr.stroke();
            // Soft scalloped wings and a little upside-down face; no flashing.
            cr.move_to(0.0, 22.0);
            cr.curve_to(-12.0, 6.0, -30.0, 12.0, -36.0, 30.0);
            cr.curve_to(-24.0, 21.0, -24.0, 38.0, -15.0, 32.0);
            cr.curve_to(-9.0, 28.0, -8.0, 37.0, 0.0, 33.0);
            cr.curve_to(8.0, 37.0, 9.0, 28.0, 15.0, 32.0);
            cr.curve_to(24.0, 38.0, 24.0, 21.0, 36.0, 30.0);
            cr.curve_to(30.0, 12.0, 12.0, 6.0, 0.0, 22.0);
            let _ = cr.fill();
            cr.arc(0.0, 27.0, 7.0, 0.0, std::f64::consts::TAU);
            let _ = cr.fill();
            for x in [-3.0, 3.0] {
                cr.set_source_rgb(0.96, 0.86, 0.57);
                cr.arc(x, 29.0, 1.2, 0.0, std::f64::consts::TAU);
                let _ = cr.fill();
            }
        }
    }
    let _ = cr.restore();
}
