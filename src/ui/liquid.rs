//! Open-state backdrop: a solid glass card that grows out of the island.
//!
//! Same silhouette as the closed pill (flush top, concave ears, rounded
//! base). No hanging bloom, no fog. The card is the product; the rest of
//! the layer-shell surface stays transparent so clicks fall through.

use gtk4::cairo::{self, Context, LinearGradient};
use gtk4::gdk::prelude::*;
use gtk4::prelude::*;

/// 16" MacBook Pro camera hole, measured on 3456×2234 @ scale 1: 370×67.
/// Idle fills that hole. Live activities hang a few pixels below it.
pub const NOTCH_W: f64 = 370.0;
pub const NOTCH_H: f64 = 67.0;
pub const LIVE_H: f64 = 72.0;

/// Transparent gutter so the card shadow never kisses the layer-shell rectangle.
const EDGE_GUTTER: f64 = 40.0;

/// Panel layer width relative to `appearance.panel_width` (gutter around the card).
pub const PANEL_WINDOW_SCALE: f64 = 1.22;

#[derive(Clone, Copy, Debug)]
pub struct Capsule {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
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
    let max_w = (win_w - EDGE_GUTTER * 2.0).max(120.0);
    let max_h = (win_h - dock_reserve - EDGE_GUTTER).max(64.0);
    let min_w = 168.0_f64.min(max_w);
    let min_h = 32.0_f64.min(max_h);
    let w = (min_w + (max_w - min_w) * t).clamp(8.0, win_w);
    let h = (min_h + (max_h - min_h) * t).clamp(8.0, max_h + 12.0);
    Capsule {
        x: ((win_w - w) * 0.5).max(0.0),
        y: 0.0,
        w,
        h,
    }
}

pub const DOCK_RESERVE: f64 = 62.0;

/// Append the island silhouette: top is the widest line (flush), concave
/// ears, inset walls, convex bottom that pulls in.
pub fn path_notch(cr: &Context, x: f64, y: f64, w: f64, h: f64, rt: f64, rb: f64) {
    let rt = rt.min(w * 0.45).min(h * 0.45).max(0.0);
    let rb = rb.min(w * 0.45).min(h * 0.45).max(0.0);
    cr.new_path();
    if w < 4.0 || h < 4.0 || rt + rb > h - 1.0 {
        cr.rectangle(x, y, w.max(0.0), h.max(0.0));
        return;
    }
    cr.move_to(x, y);
    quad_to(cr, x, y, x + rt, y, x + rt, y + rt);
    cr.line_to(x + rt, y + h - rb);
    quad_to(cr, x + rt, y + h - rb, x + rt, y + h, x + rt + rb, y + h);
    cr.line_to(x + w - rt - rb, y + h);
    quad_to(
        cr,
        x + w - rt - rb,
        y + h,
        x + w - rt,
        y + h,
        x + w - rt,
        y + h - rb,
    );
    cr.line_to(x + w - rt, y + rt);
    quad_to(cr, x + w - rt, y + rt, x + w - rt, y, x + w, y);
    cr.close_path();
}

fn quad_to(cr: &Context, sx: f64, sy: f64, cx: f64, cy: f64, ex: f64, ey: f64) {
    cr.curve_to(
        (sx + 2.0 * cx) / 3.0,
        (sy + 2.0 * cy) / 3.0,
        (2.0 * cx + ex) / 3.0,
        (2.0 * cy + ey) / 3.0,
        ex,
        ey,
    );
}

pub fn notch_radii() -> (f64, f64) {
    notch_radii_for(NOTCH_H)
}

pub fn notch_radii_for(h: f64) -> (f64, f64) {
    let top_r = (h * 0.16).clamp(8.0, 22.0);
    let bot_r = (h * 0.28).clamp(10.0, 26.0).min(h * 0.36);
    (top_r, bot_r)
}

/// Corner radii for the open card. Starts at the closed-pill ears and
/// grows toward a Droppy-style even radius as the capsule gets taller.
pub fn card_radii(w: f64, h: f64) -> (f64, f64) {
    let (nrt, nrb) = notch_radii();
    let t = ((h - NOTCH_H) / 140.0).clamp(0.0, 1.0);
    let r = (h * 0.08).clamp(16.0, 28.0).min(w * 0.08);
    (nrt + (r - nrt) * t, nrb + (r - nrb) * t)
}

/// Paint the open-state card.
///
/// `fill` tints the otherwise-black island so an omarchy background still
/// shows through a hair. `alpha` scales the whole card (no floor).
pub fn draw(cr: &Context, cap: Capsule, fill: (u8, u8, u8), alpha: f64) {
    if cap.w < 4.0 || cap.h < 4.0 {
        return;
    }
    let (tr, tg, tb) = (
        fill.0 as f64 / 255.0,
        fill.1 as f64 / 255.0,
        fill.2 as f64 / 255.0,
    );
    let gain = alpha.clamp(0.0, 1.0);
    let (rt, rb) = card_radii(cap.w, cap.h);

    for (oy, grow, a) in [(10.0, 8.0, 0.22), (4.0, 3.0, 0.14)] {
        path_notch(
            cr,
            cap.x - grow * 0.35,
            cap.y,
            cap.w + grow * 0.7,
            cap.h + oy,
            (rt + grow * 0.08).min(cap.w * 0.4),
            (rb + grow * 0.18).min(cap.h * 0.4),
        );
        cr.set_source_rgba(0.0, 0.0, 0.0, a * gain);
        let _ = cr.fill();
    }

    path_notch(cr, cap.x, cap.y, cap.w, cap.h, rt, rb);
    cr.set_source_rgba(
        tr * 0.10,
        tg * 0.10,
        tb * 0.10,
        (0.96 * gain).clamp(0.0, 1.0),
    );
    let _ = cr.fill();

    let _ = cr.save();
    path_notch(cr, cap.x, cap.y, cap.w, cap.h, rt, rb);
    cr.clip();
    let sheen = LinearGradient::new(cap.x, cap.y, cap.x, cap.y + 20.0);
    sheen.add_color_stop_rgba(0.0, 1.0, 1.0, 1.0, 0.07 * gain);
    sheen.add_color_stop_rgba(1.0, 1.0, 1.0, 1.0, 0.0);
    let _ = cr.set_source(&sheen);
    let _ = cr.paint();
    let _ = cr.restore();
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
    }

    #[test]
    fn geom_stays_inside_the_window() {
        let c = geom(400.0, 300.0, 1.08, DOCK_RESERVE);
        assert!(c.x >= 0.0);
        assert!(c.x + c.w <= 400.0 + 0.01);
        assert!(c.h < 300.0);
    }

    #[test]
    fn card_radii_grow_with_the_capsule() {
        let (a, b) = card_radii(NOTCH_W, NOTCH_H);
        let (c, d) = card_radii(680.0, 400.0);
        assert!(c > a);
        assert!(d > b);
        assert!(c <= 28.0);
    }

    #[test]
    fn live_hangs_below_the_hole() {
        assert_eq!(NOTCH_W, 370.0);
        assert_eq!(NOTCH_H, 67.0);
        assert_eq!(LIVE_H, 72.0);
        assert!(LIVE_H > NOTCH_H);
    }
}
