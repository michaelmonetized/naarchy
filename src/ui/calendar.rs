//! Calendar page: month grid on the left, today's meetings on the right.

use super::{g, glyph_btn, label, Shared};
use crate::services::calendar::CalEvent;
use crate::timefmt;
use gtk4::prelude::*;
use gtk4::{Align, Label, ScrolledWindow};
use std::cell::Cell;
use std::rc::Rc;

pub struct CalendarPage {
    root: gtk4::Box,
    month_lbl: Label,
    grid: gtk4::Grid,
    next: Label,
    list: gtk4::Box,
    empty: Label,
    year: Cell<i32>,
    month: Cell<u32>,
}

impl CalendarPage {
    pub fn new(_shared: &Rc<Shared>) -> Self {
        let (y, m, _) = timefmt::today_parts();
        let root = super::hbox(18);
        root.set_css_classes(&["na-panel-pad"]);
        root.set_hexpand(true);

        let left = super::vbox(8);
        left.set_hexpand(false);
        left.set_width_request(280);

        let head = super::hbox(6);
        let month_lbl = label(&["na-cal-head"], "");
        month_lbl.set_hexpand(true);
        month_lbl.set_xalign(0.0);
        let prev = glyph_btn(&["na-btn"], g::CHEV_L);
        let next_m = glyph_btn(&["na-btn"], g::CHEV_R);
        head.append(&month_lbl);
        head.append(&prev);
        head.append(&next_m);
        left.append(&head);

        let grid = gtk4::Grid::new();
        grid.set_column_homogeneous(true);
        grid.set_row_homogeneous(true);
        grid.set_column_spacing(2);
        grid.set_row_spacing(2);
        grid.set_hexpand(true);
        left.append(&grid);

        let right = super::vbox(8);
        right.set_hexpand(true);
        let today_h = label(&["na-title"], "TODAY");
        today_h.set_xalign(0.0);
        right.append(&today_h);

        let next = label(&["na-cal-next", "na-tier1"], "");
        next.set_wrap(true);
        next.set_xalign(0.0);
        next.set_valign(Align::Start);
        right.append(&next);

        let scroll = ScrolledWindow::new();
        scroll.set_vexpand(true);
        scroll.set_policy(gtk4::PolicyType::Never, gtk4::PolicyType::Automatic);
        scroll.set_css_classes(&["na-scroll"]);
        let list = super::vbox(6);
        scroll.set_child(Some(&list));
        right.append(&scroll);

        let empty = label(&["na-empty"], "Nothing on the books today.");
        empty.set_wrap(true);
        empty.set_xalign(0.0);
        right.append(&empty);

        root.append(&left);
        root.append(&right);

        let p = Self {
            root,
            month_lbl: month_lbl.clone(),
            grid: grid.clone(),
            next,
            list,
            empty,
            year: Cell::new(y),
            month: Cell::new(m),
        };

        {
            let year = p.year.clone();
            let month = p.month.clone();
            let grid = p.grid.clone();
            let month_lbl = p.month_lbl.clone();
            prev.connect_clicked(move |_| {
                let mut y = year.get();
                let mut mo = month.get();
                if mo == 1 {
                    mo = 12;
                    y -= 1;
                } else {
                    mo -= 1;
                }
                year.set(y);
                month.set(mo);
                paint_month(&grid, &month_lbl, y, mo);
            });
        }
        {
            let year = p.year.clone();
            let month = p.month.clone();
            let grid = p.grid.clone();
            let month_lbl = p.month_lbl.clone();
            next_m.connect_clicked(move |_| {
                let mut y = year.get();
                let mut mo = month.get();
                if mo == 12 {
                    mo = 1;
                    y += 1;
                } else {
                    mo += 1;
                }
                year.set(y);
                month.set(mo);
                paint_month(&grid, &month_lbl, y, mo);
            });
        }

        p.rebuild();
        p
    }

    pub fn root(&self) -> &gtk4::Box {
        &self.root
    }

    pub fn rebuild(&self) {
        self.rebuild_grid();
        self.rebuild_agenda();
    }

    fn rebuild_grid(&self) {
        paint_month(
            &self.grid,
            &self.month_lbl,
            self.year.get(),
            self.month.get(),
        );
    }

    fn rebuild_agenda(&self) {
        let events = self.events();
        while let Some(c) = self.list.first_child() {
            self.list.remove(&c);
        }
        self.empty.set_visible(events.is_empty());

        let now = super::now_secs();
        let next_idx = events.iter().position(|e| e.start_epoch > now).unwrap_or(0);
        if events.is_empty() {
            self.next.set_text("");
            self.next.set_visible(false);
            return;
        }
        let e = &events[next_idx];
        let when = if e.time_str.is_empty() {
            "".into()
        } else {
            format!("  ·  {}", e.time_str)
        };
        let loc = if e.location.is_empty() {
            String::new()
        } else {
            format!("\n{}", e.location)
        };
        self.next.set_text(&format!("{}{when}{loc}", e.summary));
        self.next.set_visible(true);
        for (j, ev) in events.iter().enumerate() {
            if j != next_idx {
                self.list.append(&row(ev));
            }
        }
    }

    fn events(&self) -> Vec<CalEvent> {
        super::with_shared(|sh| sh.cal_events.borrow().clone()).unwrap_or_default()
    }

    pub fn tick(&self) {
        let (y, m, _) = timefmt::today_parts();
        if self.year.get() == 0 {
            self.year.set(y);
            self.month.set(m);
            self.rebuild();
        }
    }
}

fn paint_month(grid: &gtk4::Grid, month_lbl: &Label, y: i32, m: u32) {
    while let Some(c) = grid.first_child() {
        grid.remove(&c);
    }
    month_lbl.set_text(&format!("{}  {y}", timefmt::month_name(m).to_uppercase()));

    const WDS: [&str; 7] = ["M", "T", "W", "T", "F", "S", "S"];
    for (i, wd) in WDS.iter().enumerate() {
        let l = label(&["na-cal-wd"], wd);
        l.set_halign(Align::Center);
        grid.attach(&l, i as i32, 0, 1, 1);
    }

    let (ty, tm, td) = timefmt::today_parts();
    let first = timefmt::weekday_of_first(y, m);
    let dim = timefmt::days_in_month(y, m);
    let mut day: i32 = 1 - first as i32;
    for row in 0..6 {
        for col in 0..7 {
            if day < 1 || day > dim as i32 {
                let l = label(&["na-cal-day", "other"], "");
                grid.attach(&l, col, row + 1, 1, 1);
            } else {
                let is_today = y == ty && m == tm && day as u32 == td;
                let classes: &[&str] = if is_today {
                    &["na-cal-day", "today"]
                } else {
                    &["na-cal-day"]
                };
                let l = label(classes, &format!("{day}"));
                l.set_halign(Align::Center);
                grid.attach(&l, col, row + 1, 1, 1);
            }
            day += 1;
        }
    }
}

fn row(e: &CalEvent) -> gtk4::Box {
    let r = super::hbox(10);
    r.set_css_classes(&["na-cal-row"]);
    let time = label(&["na-cal-time"], &e.time_str);
    time.set_width_chars(5);
    let s = if e.location.is_empty() {
        e.summary.clone()
    } else {
        format!("{}  {}", e.summary, e.location)
    };
    let sum = label(&["na-dim"], &s);
    sum.set_ellipsize(gtk4::pango::EllipsizeMode::End);
    sum.set_hexpand(true);
    r.append(&time);
    r.append(&sum);
    r
}
