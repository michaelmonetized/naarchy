//! Open-state backdrop: a notch-shaped bloom that hangs off the island.
//!
//! No card, no hairline, no box. Black at the island, through the omarchy
//! background, to transparent — fast falloff, dead before the window edge.

use gtk4::cairo::{self, Context, RadialGradient};
use gtk4::gdk::prelude::*;
use gtk4::prelude::*;

/// Closed-island size. Match the 16" MacBook camera housing
/// (narrower and taller than a Dynamic Island pancake).
pub const NOTCH_W: f64 = 392.0;
pub const NOTCH_H: f64 = 72.0;

/// Transparent gutter so the bloom never kisses the layer-shell rectangle.
const EDGE_GUTTER: f64 = 40.0;

/// Panel layer width relative to `appearance.panel_width` (veil hangs past content).
pub const PANEL_WINDOW_SCALE: f64 = 1.75;

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

pub const DOCK_RESERVE: f64 = 78.0;

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
    let top_r = (NOTCH_H * 0.16).clamp(8.0, 14.0);
    let bot_r = (NOTCH_H * 0.24).clamp(10.0, 18.0).min(NOTCH_H * 0.32);
    (top_r, bot_r)
}

/// Paint the open-state bloom.
///
/// `fill` is the omarchy background the shadow passes through. `alpha` scales
/// the whole effect (no floor — a floor was painting a visible card).
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
    let (rt, rb) = notch_radii();

    let nw = NOTCH_W.min(cap.w.max(8.0));
    let nh = NOTCH_H.min(cap.h.max(8.0));
    let nx = cap.x + (cap.w - nw) * 0.5;
    let ny = cap.y;
    let cx = nx + nw * 0.5;

    // Droppy-scale veil: dense under the island, through the theme across
    // the content, then a cliff to transparent *inside* `cap` so the
    // layer-shell rectangle never reads as an edge.
    //
    // Plateau then crash — large, not a 40px smudge.
    let falloff = |t: f64| -> f64 {
        let t = t.clamp(0.0, 1.0);
        let cliff = ((t - 0.52) / 0.48).clamp(0.0, 1.0);
        (1.0 - t * 0.22) * (1.0 - cliff).powi(3)
    };

    let cy = ny + nh * 0.40;
    // Wide and shallow — window is 1.75× content; bloom is a squat veil.
    const BLOOM_H: f64 = 0.50;
    let rx = ((cx - cap.x).min(cap.x + cap.w - cx)).max(8.0);
    let ry = ((cap.y + cap.h - cy) * BLOOM_H).max(8.0);
    let _ = cr.save();
    cr.translate(cx, cy);
    cr.scale(rx, ry);
    let wash = RadialGradient::new(0.0, 0.0, 0.0, 0.0, 0.0, 1.0);
    wash.add_color_stop_rgba(0.00, 0.0, 0.0, 0.0, 0.96 * gain);
    wash.add_color_stop_rgba(0.12, 0.0, 0.0, 0.0, 0.84 * gain);
    wash.add_color_stop_rgba(0.28, 0.0, 0.0, 0.0, 0.62 * gain);
    wash.add_color_stop_rgba(0.48, 0.0, 0.0, 0.0, 0.34 * gain);
    wash.add_color_stop_rgba(0.64, tr, tg, tb, 0.16 * gain);
    wash.add_color_stop_rgba(0.82, tr, tg, tb, 0.04 * gain);
    wash.add_color_stop_rgba(1.00, tr, tg, tb, 0.0);
    cr.arc(0.0, 0.0, 1.0, 0.0, std::f64::consts::TAU);
    cr.clip();
    let _ = cr.set_source(&wash);
    let _ = cr.paint();
    let _ = cr.restore();

    // Notch-following umbra. Grows farther *down* than sideways — a pool
    // hanging off the island, still inside the gutter. Stay black for most
    // of the radius; mix to theme only on the cliff so a dark desktop
    // still shows a Droppy-sized veil.
    let pad_x = ((cap.w - nw) * 0.5 * 0.92).max(0.0);
    let pad_y = ((cap.h - nh) * 0.90 * BLOOM_H).max(0.0);
    const COPIES: i32 = 18;
    for i in (1..=COPIES).rev() {
        let t = i as f64 / COPIES as f64;
        let fade = falloff(t);
        if fade < 0.015 {
            continue;
        }
        let mix = ((t - 0.50) / 0.38).clamp(0.0, 1.0);
        let mix = mix * mix;
        let a = (0.28 * fade * gain).min(0.50);
        let px = pad_x * t;
        let py = pad_y * t;
        path_notch(
            cr,
            nx - px,
            ny,
            nw + px * 2.0,
            nh + py,
            rt + px * 0.10,
            rb + py * 0.08,
        );
        cr.set_source_rgba(tr * mix, tg * mix, tb * mix, a);
        let _ = cr.fill();
    }

    // Stroke on the true island path — the silhouette of the drop.
    let max_stroke = (pad_x.min(pad_y) * 1.6).max(0.0);
    if max_stroke > 2.0 {
        cr.set_line_join(cairo::LineJoin::Round);
        cr.set_line_cap(cairo::LineCap::Round);
        const LAYERS: i32 = 8;
        for i in (1..=LAYERS).rev() {
            let t = i as f64 / LAYERS as f64;
            let fade = falloff(t);
            if fade < 0.02 {
                continue;
            }
            let mix = ((t - 0.50) / 0.38).clamp(0.0, 1.0);
            let mix = mix * mix;
            path_notch(cr, nx, ny, nw, nh, rt, rb);
            cr.set_source_rgba(tr * mix, tg * mix, tb * mix, (fade * 0.28 * gain).min(0.45));
            cr.set_line_width(max_stroke * t);
            let _ = cr.stroke();
        }
    }

    path_notch(cr, nx, ny, nw, nh, rt, rb);
    cr.set_source_rgb(0.0, 0.0, 0.0);
    let _ = cr.fill();
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
}
