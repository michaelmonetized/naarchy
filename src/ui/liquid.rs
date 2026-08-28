//! The expanded shelf silhouette: a single rounded glass capsule that
//! grows out of the notch, with a hairline highlight and a bottom fade
//! so Hyprland blur (if the user set a layer rule) reads as liquid.

use gtk4::cairo::{self, Context, LinearGradient};
use gtk4::gdk::prelude::*;
use gtk4::prelude::*;

const PI: f64 = std::f64::consts::PI;

#[derive(Clone, Copy, Debug)]
pub struct Capsule {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
    pub r: f64,
}

impl Capsule {
    pub fn rect_i(self) -> cairo::RectangleInt {
        cairo::RectangleInt::new(
            self.x.floor() as i32,
            self.y.floor() as i32,
            self.w.ceil() as i32,
            self.h.ceil() as i32,
        )
    }
}

/// Map a 0..=1(+overshoot) progress onto a capsule inside the panel window.
/// `dock_reserve` is the band at the bottom left for the floating dock.
pub fn geom(win_w: f64, win_h: f64, progress: f64, dock_reserve: f64) -> Capsule {
    // Guard against zero / tiny allocations (pre-map draws): a clamp with
    // min > max would panic, and a nonsense capsule fills the whole surface.
    let win_w = win_w.max(120.0);
    let win_h = win_h.max(dock_reserve + 24.0);
    let t = progress.max(0.0);
    let max_w = (win_w - 8.0).max(120.0);
    let max_h = (win_h - dock_reserve - 6.0).max(64.0);
    let min_w = 168.0_f64.min(max_w);
    let min_h = 32.0_f64.min(max_h);
    let w = (min_w + (max_w - min_w) * t).clamp(8.0, win_w);
    let h = (min_h + (max_h - min_h) * t).clamp(8.0, max_h + 12.0);
    let r = (22.0 + 18.0 * t.min(1.0))
        .min(h * 0.5)
        .min(w * 0.5)
        .min(42.0);
    Capsule {
        x: ((win_w - w) * 0.5).max(0.0),
        y: 0.0,
        w,
        h,
        r,
    }
}

pub const DOCK_RESERVE: f64 = 78.0;

/// Paint the glass capsule. `fill` is the theme background, `alpha` the
/// configured opacity. Leaves the rest of the surface untouched (transparent).
pub fn draw(cr: &Context, cap: Capsule, fill: (u8, u8, u8), alpha: f64) {
    if cap.w < 4.0 || cap.h < 4.0 {
        return;
    }
    let (fr, fg, fb) = (
        fill.0 as f64 / 255.0,
        fill.1 as f64 / 255.0,
        fill.2 as f64 / 255.0,
    );
    let a = alpha.clamp(0.35, 1.0);

    // Soft contact shadow — a few offset copies, no real blur available.
    for i in 1..=5 {
        let o = i as f64;
        rounded_rect(cr, cap.x, cap.y + o * 1.1, cap.w, cap.h + o * 0.4, cap.r);
        cr.set_source_rgba(0.0, 0.0, 0.0, 0.055 / o);
        let _ = cr.fill();
    }

    rounded_rect(cr, cap.x, cap.y, cap.w, cap.h, cap.r);
    cr.clip();

    let g = LinearGradient::new(0.0, cap.y, 0.0, cap.y + cap.h);
    g.add_color_stop_rgba(0.00, fr, fg, fb, a);
    g.add_color_stop_rgba(0.74, fr, fg, fb, a * 0.97);
    g.add_color_stop_rgba(1.00, fr, fg, fb, a * 0.58);
    let _ = cr.set_source(&g);
    let _ = cr.paint();

    // Inner top sheen — the glass lip.
    let sheen = LinearGradient::new(0.0, cap.y, 0.0, cap.y + cap.h * 0.22);
    sheen.add_color_stop_rgba(0.0, 1.0, 1.0, 1.0, 0.10);
    sheen.add_color_stop_rgba(1.0, 1.0, 1.0, 1.0, 0.0);
    let _ = cr.set_source(&sheen);
    let _ = cr.paint();

    cr.reset_clip();

    // Hairline rim.
    rounded_rect(
        cr,
        cap.x + 0.5,
        cap.y + 0.5,
        cap.w - 1.0,
        cap.h - 1.0,
        (cap.r - 0.5).max(0.0),
    );
    cr.set_source_rgba(1.0, 1.0, 1.0, 0.10);
    cr.set_line_width(1.0);
    let _ = cr.stroke();
}

fn rounded_rect(cr: &Context, x: f64, y: f64, w: f64, h: f64, r: f64) {
    let r = r.min(w * 0.5).min(h * 0.5).max(0.0);
    cr.new_path();
    if r < 0.5 {
        cr.rectangle(x, y, w, h);
        return;
    }
    cr.move_to(x + r, y);
    cr.line_to(x + w - r, y);
    cr.arc(x + w - r, y + r, r, -PI * 0.5, 0.0);
    cr.line_to(x + w, y + h - r);
    cr.arc(x + w - r, y + h - r, r, 0.0, PI * 0.5);
    cr.line_to(x + r, y + h);
    cr.arc(x + r, y + h - r, r, PI * 0.5, PI);
    cr.line_to(x, y + r);
    cr.arc(x + r, y + r, r, PI, PI * 1.5);
    cr.close_path();
}

/// Restrict pointer hits to the capsule + dock so empty glass around them
/// never steals clicks from windows underneath.
pub fn apply_input_region(
    win: &impl IsA<gtk4::Native>,
    cap: Capsule,
    dock: Option<(i32, i32, i32, i32)>,
) {
    let Some(surf) = win.surface() else {
        return;
    };
    let region = cairo::Region::create();
    let _ = region.union_rectangle(&cap.rect_i());
    if let Some((x, y, w, h)) = dock {
        if w > 0 && h > 0 {
            let _ = region.union_rectangle(&cairo::RectangleInt::new(x, y, w, h));
        }
    }
    surf.set_input_region(Some(&region));
}

pub fn clear_input_region(win: &impl IsA<gtk4::Native>) {
    let Some(surf) = win.surface() else {
        return;
    };
    let empty = cairo::Region::create();
    surf.set_input_region(Some(&empty));
}

pub fn widget_rect_in(
    win: &impl IsA<gtk4::Widget>,
    child: &impl IsA<gtk4::Widget>,
) -> Option<(i32, i32, i32, i32)> {
    let b = child.compute_bounds(win)?;
    Some((
        b.x().floor() as i32,
        b.y().floor() as i32,
        b.width().ceil() as i32,
        b.height().ceil() as i32,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn geom_grows_from_a_blob() {
        let a = geom(800.0, 540.0, 0.0, DOCK_RESERVE);
        let b = geom(800.0, 540.0, 1.0, DOCK_RESERVE);
        assert!(a.w < b.w);
        assert!(a.h < b.h);
        assert!(b.w > 700.0);
        assert!(a.r > 0.0 && b.r > a.r - 1.0);
    }

    #[test]
    fn geom_stays_inside_the_window() {
        let c = geom(400.0, 300.0, 1.08, DOCK_RESERVE);
        assert!(c.x >= 0.0);
        assert!(c.x + c.w <= 400.0 + 0.01);
        assert!(c.h < 300.0);
    }
}
