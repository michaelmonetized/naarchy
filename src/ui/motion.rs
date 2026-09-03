//! Frame-clock springs and tweens.
//!
//! Every expand, collapse, pill-width change and HUD uses the same
//! damped-spring stepper so motion has one feel: a slight overshoot that
//! settles, like a drop hitting glass.

use gtk4::glib;
use gtk4::prelude::*;
use std::cell::{Cell, RefCell};
use std::rc::Rc;

/// Stiffness / damping / mass for a damped harmonic oscillator.
#[derive(Clone, Copy, Debug)]
pub struct Spring {
    pub stiffness: f64,
    pub damping: f64,
    pub mass: f64,
}

impl Spring {
    /// Juicy open: ~0.45 s settle, a hair of overshoot.
    pub const OPEN: Spring = Spring {
        stiffness: 240.0,
        damping: 22.0,
        mass: 1.0,
    };
    /// Snappier close: less overshoot, gone before it gets cute.
    pub const CLOSE: Spring = Spring {
        stiffness: 340.0,
        damping: 34.0,
        mass: 1.0,
    };
    /// Pill width / HUD value — tight, almost critical.
    pub const SNAP: Spring = Spring {
        stiffness: 280.0,
        damping: 28.0,
        mass: 1.0,
    };

    /// Advance one frame. `dt` is seconds, clamped by the caller.
    pub fn step(self, pos: f64, vel: f64, target: f64, dt: f64) -> (f64, f64) {
        let dt = dt.clamp(1.0 / 240.0, 1.0 / 30.0);
        let acc = (-self.stiffness * (pos - target) - self.damping * vel) / self.mass;
        let vel = vel + acc * dt;
        let pos = pos + vel * dt;
        (pos, vel)
    }

    pub fn settled(self, pos: f64, vel: f64, target: f64) -> bool {
        (pos - target).abs() < 0.002 && vel.abs() < 0.02
    }
}

/// Cubic smoothstep, clamped to 0..=1. Used to fade content in *after*
/// the capsule has already started growing.
pub fn smoothstep(t: f64) -> f64 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Content fades in once the shelf is ~40% open, fully there by ~80%.
pub fn content_opacity(progress: f64) -> f64 {
    smoothstep((progress - 0.38) / 0.42)
}

/// Dock lags the shelf so the capsule lands first, then the icons.
pub fn dock_opacity(progress: f64) -> f64 {
    smoothstep((progress - 0.52) / 0.36)
}

/// Drive `step(dt) -> keep_going` off the widget's frame clock.
/// Calling this while a previous drive is live replaces it.
pub fn drive<W, F>(slot: &Rc<Cell<Option<gtk4::TickCallbackId>>>, widget: &W, step: F)
where
    W: IsA<gtk4::Widget>,
    F: FnMut(f64) -> bool + 'static,
{
    if let Some(id) = slot.take() {
        id.remove();
    }
    let last = Cell::new(None::<i64>);
    let slot2 = slot.clone();
    let step = RefCell::new(step);
    let id = widget.add_tick_callback(move |_w, clock| {
        let now = clock.frame_time();
        let dt = match last.get() {
            Some(prev) => ((now - prev) as f64 / 1_000_000.0).clamp(1.0 / 240.0, 1.0 / 30.0),
            None => 1.0 / 120.0,
        };
        last.set(Some(now));
        let keep = (step.borrow_mut())(dt);
        if keep {
            glib::ControlFlow::Continue
        } else {
            slot2.set(None);
            glib::ControlFlow::Break
        }
    });
    slot.set(Some(id));
}

/// Ease-out cubic tween over `ms`, calling `apply(t)` with t in 0..=1.
pub fn tween<W, F, D>(widget: &W, ms: u32, apply: F, done: D)
where
    W: IsA<gtk4::Widget>,
    F: Fn(f64) + 'static,
    D: FnOnce() + 'static,
{
    let start = Rc::new(Cell::new(None::<i64>));
    let done = Rc::new(RefCell::new(Some(done)));
    let dur = (ms.max(1) as f64) * 1_000.0; // µs
    widget.add_tick_callback(move |_w, clock| {
        let now = clock.frame_time();
        let t0 = match start.get() {
            Some(s) => s,
            None => {
                start.set(Some(now));
                now
            }
        };
        let t = ((now - t0) as f64 / dur).clamp(0.0, 1.0);
        let eased = 1.0 - (1.0 - t) * (1.0 - t) * (1.0 - t);
        apply(eased);
        if t >= 1.0 {
            if let Some(d) = done.borrow_mut().take() {
                d();
            }
            glib::ControlFlow::Break
        } else {
            glib::ControlFlow::Continue
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spring_settles_on_target() {
        let mut x = 0.0;
        let mut v = 0.0;
        let s = Spring::OPEN;
        for _ in 0..360 {
            let n = s.step(x, v, 1.0, 1.0 / 120.0);
            x = n.0;
            v = n.1;
        }
        assert!((x - 1.0).abs() < 0.01, "pos {x}");
        assert!(v.abs() < 0.05, "vel {v}");
        assert!(s.settled(x, v, 1.0));
    }

    #[test]
    fn open_spring_overshoots_once() {
        let mut x = 0.0;
        let mut v = 0.0;
        let s = Spring::OPEN;
        let mut saw_over = false;
        for _ in 0..240 {
            let n = s.step(x, v, 1.0, 1.0 / 120.0);
            x = n.0;
            v = n.1;
            if x > 1.0 {
                saw_over = true;
            }
        }
        assert!(saw_over, "open spring should overshoot a hair");
    }

    #[test]
    fn smoothstep_ends() {
        assert_eq!(smoothstep(-1.0), 0.0);
        assert_eq!(smoothstep(0.0), 0.0);
        assert_eq!(smoothstep(1.0), 1.0);
        assert_eq!(smoothstep(2.0), 1.0);
        let mid = smoothstep(0.5);
        assert!((mid - 0.5).abs() < 1e-9);
    }

    #[test]
    fn content_stays_hidden_early() {
        assert_eq!(content_opacity(0.0), 0.0);
        assert_eq!(content_opacity(0.2), 0.0);
        assert!(content_opacity(0.6) > 0.4);
        assert_eq!(content_opacity(1.0), 1.0);
    }
}
