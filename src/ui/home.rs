//! Home shelf: renders whichever widgets are enabled in the widget store
//! (Timer, Media, Clock, Battery). Widgets are dragged in from the drawer via
//! the `text/x-naarchy-widget` mime type.

use super::media::MediaPage;
use super::timer::TimerUi;
use super::{label, vbox, Shared};
use crate::widget_store::WidgetKind;
use gtk4::prelude::*;
use gtk4::{gdk, glib, DropTarget, Label};
use std::rc::Rc;

pub const WIDGET_MIME: &str = "text/x-naarchy-widget";

struct Slot {
    kind: WidgetKind,
    box_: gtk4::Box,
    clock_lbl: Option<Label>,
    clock_date: Option<Label>,
    battery_lbl: Option<Label>,
    battery_sub: Option<Label>,
    media: Option<MediaPage>,
    timer: Option<TimerUi>,
}

pub struct HomePage {
    root: gtk4::Box,
    grid: gtk4::Grid,
    slots: Vec<Slot>,
}

impl HomePage {
    pub fn build(shared: &Rc<Shared>) -> Self {
        let root = vbox(0);
        root.set_css_classes(&["na-panel-pad"]);

        let grid = gtk4::Grid::new();
        grid.set_column_spacing(12);
        grid.set_row_spacing(12);
        grid.set_column_homogeneous(true);
        grid.set_hexpand(true);
        grid.set_vexpand(true);
        root.append(&grid);

        let mut slots = Vec::new();
        for kind in WidgetKind::all() {
            let slot_box = vbox(4);
            slot_box.set_css_classes(&["na-widget"]);
            slot_box.set_hexpand(true);
            slot_box.set_vexpand(true);

            let body = vbox(4);
            slot_box.append(&body);

            let mut clock_lbl = None;
            let mut clock_date = None;
            let mut battery_lbl = None;
            let mut battery_sub = None;
            let mut media = None;
            let mut timer = None;
            match kind {
                WidgetKind::Timer => timer = Some(TimerUi::build(shared)),
                WidgetKind::Media => media = Some(MediaPage::build(shared)),
                WidgetKind::Clock => {
                    let now = super::now_secs();
                    let fmt = shared.cfg.borrow().clock.format.clone();
                    clock_lbl = Some(label(&["na-clock-big"], &fmt_clock(now, &fmt)));
                    clock_date = Some(label(
                        &["na-dim"],
                        &crate::timefmt::strftime_local(now, "%A, %b %e"),
                    ));
                    body.append(clock_lbl.as_ref().unwrap());
                    body.append(clock_date.as_ref().unwrap());
                }
                WidgetKind::Battery => {
                    battery_lbl = Some(label(&["na-clock-big", "na-batt"], "—"));
                    battery_sub = Some(label(&["na-dim"], ""));
                    body.append(battery_lbl.as_ref().unwrap());
                    body.append(battery_sub.as_ref().unwrap());
                }
            }
            if let Some(t) = timer.as_ref() {
                body.append(t.root());
            }
            if let Some(m) = media.as_ref() {
                body.append(m.root());
            }
            slots.push(Slot {
                kind,
                box_: slot_box,
                clock_lbl,
                clock_date,
                battery_lbl,
                battery_sub,
                media,
                timer,
            });
        }

        let formats = gdk::ContentFormats::builder()
            .add_mime_type(WIDGET_MIME)
            .build();
        let dt = DropTarget::builder()
            .formats(&formats)
            .actions(gdk::DragAction::COPY)
            .build();
        {
            let sh = shared.clone();
            dt.connect_drop(move |_dt, value, _x, _y| {
                let s = value.get::<String>().ok();
                if let Some(s) = s {
                    if let Some(kind) = WidgetKind::from_name(s.trim()) {
                        let added = sh.widgets.borrow_mut().add(kind);
                        if added {
                            crate::app::refresh_home();
                        }
                    }
                }
                true
            });
        }
        grid.add_controller(dt);

        let p = Self { root, grid, slots };
        p.apply_store();
        p
    }

    /// Reorder + show children to match the current widget store.
    pub fn apply_store(&self) {
        while let Some(c) = self.grid.first_child() {
            self.grid.remove(&c);
        }
        let kinds: Vec<WidgetKind> =
            super::with_shared(|sh| sh.widgets.borrow().widgets.clone()).unwrap_or_default();
        let n = kinds.len();
        for (i, kind) in kinds.into_iter().enumerate() {
            if let Some(slot) = self.slots.iter().find(|s| s.kind == kind) {
                let col = if n == 1 { 0 } else { (i % 2) as i32 };
                let row = if n == 1 { 0 } else { (i / 2) as i32 };
                let span = if n == 1 { 2 } else { 1 };
                self.grid.attach(&slot.box_, col, row, span, 1);
            }
        }
    }

    pub fn root(&self) -> &gtk4::Box {
        &self.root
    }

    pub fn tick(&self) {
        for s in &self.slots {
            if !self.in_store(s.kind) {
                continue;
            }
            if let Some(t) = s.timer.as_ref() {
                t.tick();
            }
            if let (Some(l), Some(d)) = (s.clock_lbl.as_ref(), s.clock_date.as_ref()) {
                let now = super::now_secs();
                let fmt = super::with_shared(|sh| sh.cfg.borrow().clock.format.clone())
                    .unwrap_or_else(|| "%H:%M".into());
                l.set_text(&fmt_clock(now, &fmt));
                d.set_text(&crate::timefmt::strftime_local(now, "%A, %b %e"));
            }
            if let (Some(b), Some(sub)) = (s.battery_lbl.as_ref(), s.battery_sub.as_ref()) {
                super::with_shared(|sh| {
                    let st = *sh.battery.borrow();
                    if st.present {
                        b.set_text(&format!("{}%", st.percent.round() as i64));
                        sub.set_text(if st.charging { "Charging" } else { "Battery" });
                    } else {
                        b.set_text("—");
                        sub.set_text("No battery");
                    }
                });
            }
        }
    }

    pub fn refresh(&self) {
        for s in &self.slots {
            if let Some(m) = s.media.as_ref() {
                m.update();
            }
        }
    }

    pub fn timer_start(&self, secs: u64) {
        for s in &self.slots {
            if s.kind == WidgetKind::Timer {
                if let Some(t) = s.timer.as_ref() {
                    t.start(secs);
                }
            }
        }
    }

    fn in_store(&self, kind: WidgetKind) -> bool {
        super::with_shared(|sh| sh.widgets.borrow().has(kind)).unwrap_or(false)
    }
}

fn fmt_clock(now: u64, fmt: &str) -> String {
    crate::timefmt::strftime_local(now, if fmt.is_empty() { "%H:%M" } else { fmt })
}

pub fn drag_content(kind: WidgetKind) -> gdk::ContentProvider {
    let bytes = glib::Bytes::from(kind.name().as_bytes());
    gdk::ContentProvider::for_bytes(WIDGET_MIME, &bytes)
}
