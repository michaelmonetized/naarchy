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
    // Costumes extend the island's black silhouette, without colored details.
    cr.set_source_rgb(0.0, 0.0, 0.0);
    match costume {
        Costume::Fangs => {
            for x in [-38.0, 38.0] {
                cr.move_to(x - 8.0, 0.0);
                cr.line_to(x + 8.0, 0.0);
                cr.line_to(x + 1.0, 23.0);
                cr.close_path();
            }
            let _ = cr.fill();
        }
        Costume::Drips => {
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
            // Two splayed feet grip the edge directly. Short paired legs lead
            // into the hips; there is no central thread or suspension line.
            for x in [-6.0, 6.0] {
                cr.move_to(x - 5.0, -2.0);
                cr.line_to(x + 4.0, -2.0);
                cr.line_to(x + 5.0, 3.0);
                cr.line_to(x + 2.0, 1.0);
                cr.line_to(x + 1.8, 12.0);
                cr.line_to(x - 1.8, 12.0);
                cr.line_to(x - 2.0, 1.0);
                cr.line_to(x - 5.0, 3.0);
                cr.close_path();
                let _ = cr.fill();
            }
            // Scalloped wings frame a body whose head and pointed ears are
            // below its feet: the roosting bat is visibly upside down.
            for direction in [-1.0, 1.0] {
                let _ = cr.save();
                cr.scale(direction, 1.0);
                cr.move_to(5.0, 15.0);
                cr.curve_to(11.0, 10.0, 22.0, 12.0, 28.0, 22.0);
                cr.line_to(21.0, 20.0);
                cr.curve_to(23.0, 25.0, 21.0, 31.0, 17.0, 34.0);
                cr.line_to(13.0, 29.0);
                cr.curve_to(11.0, 32.0, 8.0, 32.0, 6.0, 30.0);
                cr.close_path();
                let _ = cr.fill();
                let _ = cr.restore();
            }
            cr.move_to(0.0, 8.0);
            cr.curve_to(11.0, 8.0, 12.0, 23.0, 7.0, 32.0);
            cr.line_to(-7.0, 32.0);
            cr.curve_to(-12.0, 23.0, -11.0, 8.0, 0.0, 8.0);
            let _ = cr.fill();
            cr.arc(0.0, 35.0, 7.0, 0.0, std::f64::consts::TAU);
            let _ = cr.fill();
            for direction in [-1.0, 1.0] {
                cr.move_to(direction * 6.0, 34.0);
                cr.line_to(direction * 8.0, 43.0);
                cr.line_to(direction, 40.0);
                cr.close_path();
                let _ = cr.fill();
            }
        }
    }
    let _ = cr.restore();
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pixels(costume: Costume) -> Vec<u8> {
        let mut surface =
            gtk4::cairo::ImageSurface::create(gtk4::cairo::Format::ARgb32, 160, 80).unwrap();
        {
            let cr = Context::new(&surface).unwrap();
            draw(&cr, costume, 80.0, 20.0, 1.0);
        }
        surface.flush();
        let pixels = surface.data().unwrap().to_vec();
        pixels
    }

    #[test]
    fn all_costumes_render_only_black_ink() {
        for costume in [Costume::Fangs, Costume::Drips, Costume::Bat] {
            let pixels = pixels(costume);
            assert!(pixels.as_chunks::<4>().0.iter().any(|p| p[3] > 0));
            assert!(
                pixels
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .all(|p| p[..3] == [0, 0, 0]),
                "colored pixel in {costume:?}"
            );
        }
    }

    #[test]
    fn upside_down_bat_has_two_edge_grips_and_a_head_below_them() {
        let pixels = pixels(Costume::Bat);
        let alpha = |x: usize, y: usize| pixels[(y * 160 + x) * 4 + 3];
        // Separate feet touch the notch; its center has no hanging string.
        assert!(alpha(74, 20) > 0 && alpha(86, 20) > 0);
        assert_eq!(alpha(80, 20), 0);
        assert!(alpha(80, 53) > 0, "head must be below feet/body");
        assert!(alpha(73, 59) > 0 && alpha(87, 59) > 0, "ears point down");
    }
}
