//! A short, bounded, keyboard-transparent first-run celebration.

use super::{liquid, motion};
use gtk4::cairo::{self, Context};
use gtk4::glib;
use gtk4::prelude::*;
use gtk4_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::{Duration, Instant};

pub const MESSAGE: &str = "thanks for installing naarchy happy 10/10";
const LINES: [&str; 3] = ["thanks for installing", "naarchy", "happy 10/10"];
const DURATION: f64 = 6.2;
const REDUCED_DURATION: f64 = 4.0;

struct Surface {
    win: gtk4::ApplicationWindow,
    tick: Option<gtk4::TickCallbackId>,
}

pub struct Welcome {
    surfaces: RefCell<Vec<Surface>>,
    expiry: Cell<Option<glib::SourceId>>,
    finished: Cell<bool>,
}

impl Welcome {
    pub fn show(
        app: &gtk4::Application,
        monitors: &[gtk4::gdk::Monitor],
        accent: (u8, u8, u8),
    ) -> Rc<Self> {
        let welcome = Rc::new(Self {
            surfaces: RefCell::new(Vec::new()),
            expiry: Cell::new(None),
            finished: Cell::new(false),
        });
        let reduced = motion::reduced();
        let started = Instant::now();
        // Cap simultaneous output surfaces, independent of unusual monitor setups.
        for monitor in monitors.iter().take(8) {
            let surface = build_surface(&welcome, app, monitor, accent, started, reduced);
            welcome.surfaces.borrow_mut().push(surface);
        }
        let weak = Rc::downgrade(&welcome);
        let duration = if reduced { REDUCED_DURATION } else { DURATION };
        welcome.expiry.set(Some(glib::timeout_add_local_once(
            Duration::from_secs_f64(duration),
            move || {
                if let Some(welcome) = weak.upgrade() {
                    // This source has fired; do not remove it a second time.
                    welcome.expiry.take();
                    welcome.finish();
                }
            },
        )));
        welcome
    }

    /// Idempotent for timeout, pointer dismissal, hotplug, reload and shutdown.
    pub fn finish(&self) {
        if self.finished.replace(true) {
            return;
        }
        if let Some(source) = self.expiry.take() {
            source.remove();
        }
        for mut surface in self.surfaces.borrow_mut().drain(..) {
            if let Some(tick) = surface.tick.take() {
                tick.remove();
            }
            liquid::clear_input_region(&surface.win);
            surface.win.destroy();
        }
    }
}

impl Drop for Welcome {
    fn drop(&mut self) {
        self.finish();
    }
}

fn build_surface(
    welcome: &Rc<Welcome>,
    app: &gtk4::Application,
    monitor: &gtk4::gdk::Monitor,
    accent: (u8, u8, u8),
    started: Instant,
    reduced: bool,
) -> Surface {
    let win = gtk4::ApplicationWindow::builder()
        .application(app)
        .title("naarchy-welcome")
        .decorated(false)
        .resizable(true)
        .focusable(false)
        .build();
    win.init_layer_shell();
    win.set_layer(Layer::Overlay);
    for edge in [Edge::Top, Edge::Bottom, Edge::Left, Edge::Right] {
        win.set_anchor(edge, true);
    }
    win.set_monitor(Some(monitor));
    win.set_exclusive_zone(-1);
    // No key controller, focus request, clipboard access or exclusive zone.
    win.set_keyboard_mode(KeyboardMode::None);
    win.set_namespace(Some("naarchy-welcome"));
    win.add_css_class("naarchy");

    let canvas = gtk4::DrawingArea::new();
    canvas.set_hexpand(true);
    canvas.set_vexpand(true);
    canvas.set_can_target(false);
    canvas.update_property(&[gtk4::accessible::Property::Label(MESSAGE)]);
    let scene = Scene::new();
    canvas.set_draw_func(move |_, cr, w, h| {
        scene.draw(
            cr,
            w as f64,
            h as f64,
            started.elapsed().as_secs_f64(),
            reduced,
            accent,
        );
    });

    let root = gtk4::Overlay::new();
    root.set_child(Some(&canvas));
    let dismiss = gtk4::Button::with_label("Dismiss");
    dismiss.set_focusable(false);
    dismiss.set_focus_on_click(false);
    dismiss.set_halign(gtk4::Align::End);
    dismiss.set_valign(gtk4::Align::End);
    dismiss.set_margin_end(24);
    dismiss.set_margin_bottom(24);
    dismiss.set_size_request(112, 40);
    root.add_overlay(&dismiss);
    win.set_child(Some(&root));
    let weak = Rc::downgrade(welcome);
    dismiss.connect_clicked(move |_| {
        if let Some(welcome) = weak.upgrade() {
            welcome.finish();
        }
    });

    // The whole drawing is click-through. Only the small Dismiss control gets
    // an input region, calculated after GTK has allocated its actual bounds.
    win.connect_realize(liquid::clear_input_region);
    let weak_win = win.downgrade();
    let weak_button = dismiss.downgrade();
    canvas.connect_resize(move |_, _, _| {
        let weak_win = weak_win.clone();
        let weak_button = weak_button.clone();
        glib::idle_add_local_once(move || {
            if let (Some(win), Some(button)) = (weak_win.upgrade(), weak_button.upgrade()) {
                if let (Some(surface), Some((x, y, w, h))) =
                    (win.surface(), liquid::widget_rect_in(&win, &button))
                {
                    let region = cairo::Region::create();
                    let _ = region.union_rectangle(&cairo::RectangleInt::new(x, y, w, h));
                    surface.set_input_region(Some(&region));
                }
            }
        });
    });
    let weak = Rc::downgrade(welcome);
    win.connect_unrealize(move |_| {
        if let Some(welcome) = weak.upgrade() {
            welcome.finish();
        }
    });

    let tick = if reduced {
        None
    } else {
        let weak_button = dismiss.downgrade();
        Some(canvas.add_tick_callback(move |canvas, _| {
            let t = started.elapsed().as_secs_f64();
            if let Some(button) = weak_button.upgrade() {
                button.set_opacity(alpha(t));
            }
            canvas.queue_draw();
            // The independent wall-time expiry also cleans up if frames stop.
            glib::ControlFlow::Continue
        }))
    };
    // set_visible maps the layer without asking the compositor to focus it.
    win.set_visible(true);
    Surface { win, tick }
}

fn alpha(t: f64) -> f64 {
    1.0 - motion::smoothstep((t - 5.3) / (DURATION - 5.3))
}

#[derive(Clone, Copy)]
struct Particle {
    x: f64,
    y: f64,
    bit: &'static str,
    seed_x: f64,
    seed_y: f64,
}

struct Scene {
    particles: Vec<Particle>,
    columns: f64,
}

impl Scene {
    fn new() -> Self {
        let columns = (LINES.iter().map(|line| line.len()).max().unwrap() * 6 - 1) as f64;
        let mut particles = Vec::with_capacity(800);
        for (line_index, line) in LINES.iter().enumerate() {
            let offset = (columns - (line.len() * 6 - 1) as f64) / 2.0;
            for (column, ch) in line.chars().enumerate() {
                for (row, bits) in glyph(ch).iter().enumerate() {
                    for bit in 0..5 {
                        if bits & (1 << (4 - bit)) != 0 {
                            let index = particles.len();
                            particles.push(Particle {
                                x: offset + (column * 6 + bit) as f64,
                                y: (line_index * 11 + row) as f64,
                                bit: if index % 2 == 0 { "0" } else { "1" },
                                seed_x: noise(index, 0x9e3779b9),
                                seed_y: noise(index, 0x85ebca6b),
                            });
                        }
                    }
                }
            }
        }
        Self { particles, columns }
    }

    fn layout(&self, width: f64, height: f64) -> (f64, f64, f64) {
        let cell = ((width - 64.0).max(1.0) / self.columns)
            .min((height * 0.48) / 29.0)
            .min(11.0);
        (
            cell,
            (width - self.columns * cell) / 2.0,
            height / 2.0 - 14.5 * cell,
        )
    }

    fn position(&self, p: &Particle, width: f64, height: f64, t: f64) -> (f64, f64) {
        let (cell, left, top) = self.layout(width, height);
        let target = (left + (p.x + 0.5) * cell, top + (p.y + 0.5) * cell);
        let burst = (t / 1.35).clamp(0.0, 1.0);
        let flight = (
            width / 2.0 + (p.seed_x - 0.5) * width * 1.7 * burst,
            height * 0.1 - p.seed_y * height * 0.55 * burst
                + height * (0.5 + p.seed_y * 0.65) * burst * burst,
        );
        let settle = motion::smoothstep((t - 1.35) / 1.65);
        (
            flight.0 + (target.0 - flight.0) * settle,
            flight.1 + (target.1 - flight.1) * settle,
        )
    }

    fn draw(
        &self,
        cr: &Context,
        width: f64,
        height: f64,
        t: f64,
        reduced: bool,
        accent: (u8, u8, u8),
    ) {
        let a = if reduced { 1.0 } else { alpha(t) };
        if a <= 0.0 {
            return;
        }
        cr.select_font_face(
            "monospace",
            cairo::FontSlant::Normal,
            cairo::FontWeight::Normal,
        );
        if reduced {
            let size = ((width - 64.0) / 13.0).clamp(8.0, 22.0);
            card(
                cr,
                width / 2.0 - size * 7.2,
                height / 2.0 - size * 2.8,
                size * 14.4,
                size * 5.6,
                0.95,
            );
            cr.set_font_size(size);
            cr.set_source_rgb(0.94, 0.96, 1.0);
            for (i, line) in LINES.iter().enumerate() {
                if let Ok(extents) = cr.text_extents(line) {
                    cr.move_to(
                        (width - extents.width()) / 2.0 - extents.x_bearing(),
                        height / 2.0 + (i as f64 - 0.7) * size * 1.5,
                    );
                    let _ = cr.show_text(line);
                }
            }
            return;
        }
        let (cell, left, top) = self.layout(width, height);
        card(
            cr,
            left - 18.0,
            top - 18.0,
            self.columns * cell + 36.0,
            cell * 29.0 + 36.0,
            motion::smoothstep((t - 1.8) / 1.2) * a * 0.92,
        );
        cr.set_font_size(cell * 0.95);
        let advance = cr
            .text_extents("0")
            .map(|e| e.x_advance())
            .unwrap_or(cell * 0.6);
        let (r, g, b) = (
            accent.0 as f64 / 255.0,
            accent.1 as f64 / 255.0,
            accent.2 as f64 / 255.0,
        );
        for p in &self.particles {
            let (x, y) = self.position(p, width, height, t);
            if x < -cell || x > width + cell || y < -cell || y > height + cell {
                continue;
            }
            let tint = 0.3 + p.seed_y * 0.55;
            cr.set_source_rgba(
                r + (1.0 - r) * tint,
                g + (1.0 - g) * tint,
                b + (1.0 - b) * tint,
                a,
            );
            cr.move_to(x - advance / 2.0, y + cell * 0.32);
            let _ = cr.show_text(p.bit);
        }
    }
}

fn card(cr: &Context, x: f64, y: f64, w: f64, h: f64, a: f64) {
    let radius = 16.0_f64.min(w / 2.0).min(h / 2.0);
    for (cx, cy, start) in [
        (x + w - radius, y + radius, -std::f64::consts::FRAC_PI_2),
        (x + w - radius, y + h - radius, 0.0),
        (x + radius, y + h - radius, std::f64::consts::FRAC_PI_2),
        (x + radius, y + radius, std::f64::consts::PI),
    ] {
        cr.arc(cx, cy, radius, start, start + std::f64::consts::FRAC_PI_2);
    }
    cr.close_path();
    cr.set_source_rgba(0.025, 0.035, 0.055, a);
    let _ = cr.fill();
}

fn noise(index: usize, salt: u32) -> f64 {
    let mut x = (index as u32).wrapping_add(salt);
    x ^= x >> 16;
    x = x.wrapping_mul(0x7feb352d);
    x ^= x >> 15;
    x = x.wrapping_mul(0x846ca68b);
    x ^= x >> 16;
    x as f64 / u32::MAX as f64
}

/// Lowercase 5×7 terminal lettering; the actual ink is always a 0 or a 1.
fn glyph(ch: char) -> [u8; 7] {
    match ch {
        'a' => [0, 0, 14, 1, 15, 17, 15],
        'c' => [0, 0, 14, 17, 16, 17, 14],
        'f' => [6, 9, 8, 28, 8, 8, 8],
        'g' => [0, 15, 17, 17, 15, 1, 14],
        'h' => [16, 16, 30, 17, 17, 17, 17],
        'i' => [4, 0, 12, 4, 4, 4, 14],
        'k' => [16, 16, 18, 20, 24, 20, 18],
        'l' => [12, 4, 4, 4, 4, 4, 14],
        'n' => [0, 0, 30, 17, 17, 17, 17],
        'o' => [0, 0, 14, 17, 17, 17, 14],
        'p' => [0, 0, 30, 17, 30, 16, 16],
        'r' => [0, 0, 22, 25, 16, 16, 16],
        's' => [0, 0, 15, 16, 14, 1, 30],
        't' => [8, 8, 28, 8, 8, 9, 6],
        'y' => [0, 0, 17, 17, 15, 1, 14],
        '0' => [14, 17, 19, 21, 25, 17, 14],
        '1' => [4, 12, 4, 4, 4, 4, 14],
        '/' => [1, 2, 2, 4, 8, 8, 16],
        _ => [0; 7],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_lowercase_message_has_bounded_binary_ink_and_supported_letters() {
        assert_eq!(LINES.join(" "), MESSAGE);
        assert_eq!(MESSAGE, "thanks for installing naarchy happy 10/10");
        for ch in MESSAGE.chars().filter(|ch| *ch != ' ') {
            assert_ne!(glyph(ch), [0; 7], "missing {ch}");
        }
        let scene = Scene::new();
        assert!((400..800).contains(&scene.particles.len()));
        assert!(scene.particles.iter().all(|p| matches!(p.bit, "0" | "1")));
    }

    #[test]
    fn particles_settle_exactly_and_fit_small_scaled_and_large_outputs() {
        let scene = Scene::new();
        for (w, h) in [
            (320.0, 240.0),
            (1280.0, 720.0),
            (1920.0, 1080.0),
            (3840.0, 2160.0),
        ] {
            let (cell, left, top) = scene.layout(w, h);
            for p in &scene.particles {
                let target = (left + (p.x + 0.5) * cell, top + (p.y + 0.5) * cell);
                let position = scene.position(p, w, h, 3.0);
                assert!((position.0 - target.0).abs() < 1e-6);
                assert!((position.1 - target.1).abs() < 1e-6);
                assert!(position.0 > 0.0 && position.0 < w);
                assert!(position.1 > 0.0 && position.1 < h);
            }
        }
        assert_eq!(alpha(4.0), 1.0);
        assert!(alpha(5.8) < 0.5);
        assert_eq!(alpha(DURATION), 0.0);
        assert_eq!(alpha(60.0), 0.0);
    }
}
