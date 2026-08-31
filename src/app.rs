use crate::config::Config;
use crate::services::{self, Banner, Event, Verb};
use crate::ui::panel::PanelUi;
use crate::ui::pill::PillUi;
use crate::ui::{hud, Shared};
use gtk4::glib;
use gtk4::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::mpsc::{Receiver, Sender};

pub struct App {
    pub shared: Rc<Shared>,
    pills: RefCell<Vec<PillUi>>,
    panels: RefCell<Vec<PanelUi>>,
    huds: RefCell<hud::HudManager>,
}

thread_local! {
    static APP: RefCell<Option<Rc<App>>> = const { RefCell::new(None) };
}

pub fn with_app<R>(f: impl FnOnce(&Rc<App>) -> R) -> Option<R> {
    APP.with(|a| a.borrow().as_ref().map(f))
}

/// Public helpers used from ui modules (they run on the GTK thread).
pub fn request_expand_all() {
    with_app(|app| {
        for p in app.panels.borrow().iter() {
            p.expand();
        }
    });
}

pub fn request_collapse_all() {
    with_app(|app| {
        for p in app.panels.borrow().iter() {
            p.collapse();
        }
    });
}

/// User interacted with a panel (tab click) — keep it open.
pub fn poke_panels() {
    with_app(|app| {
        for p in app.panels.borrow().iter() {
            p.poke_collapse_timer();
        }
    });
}

pub fn surface_pointer_enter() {
    with_app(|app| {
        for p in app.panels.borrow().iter() {
            p.note_pointer(true);
        }
    });
}

pub fn surface_pointer_leave() {
    with_app(|app| {
        for p in app.panels.borrow().iter() {
            p.note_pointer(false);
            p.schedule_collapse_if_unhovered();
        }
    });
}

thread_local! {
    static IGNORE_DROP_LEAVE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Dragging a file over the notch or any tab: open, fade in the drop target.
pub fn drop_hover(on: bool) {
    with_app(|app| {
        if on {
            IGNORE_DROP_LEAVE.with(|c| c.set(false));
            if !app.shared.fullscreen_hide.get() {
                for p in app.panels.borrow().iter() {
                    p.expand();
                    p.note_pointer(true);
                    p.set_drop_veil(true);
                }
            }
        } else if IGNORE_DROP_LEAVE.with(|c| c.replace(false)) {
            for p in app.panels.borrow().iter() {
                p.set_drop_veil(false);
            }
        } else {
            for p in app.panels.borrow().iter() {
                p.set_drop_veil(false);
                p.note_pointer(false);
                p.schedule_collapse_if_unhovered();
            }
        }
    });
}

/// Drop completed: park in the Inbox and show it.
pub fn drop_commit(value: &gtk4::glib::Value) {
    with_app(|app| {
        IGNORE_DROP_LEAVE.with(|c| c.set(true));
        crate::ui::panel::handle_dropped_value(&app.shared, value);
        app.shared.tab.set(crate::ui::Tab::Inbox);
        for p in app.panels.borrow().iter() {
            p.show_tab(crate::ui::Tab::Inbox);
            p.set_drop_veil(false);
            p.note_pointer(true);
        }
    });
}

pub fn refresh_after_shelf_change() {
    with_app(|app| {
        for p in app.panels.borrow().iter() {
            p.shelf_reload();
        }
        for p in app.pills.borrow().iter() {
            p.tick();
        }
    });
}

/// Widget set on the Home shelf changed (drawer drag-drop).
pub fn refresh_home() {
    with_app(|app| {
        for p in app.panels.borrow().iter() {
            p.home_reload();
        }
        for p in app.pills.borrow().iter() {
            p.tick();
        }
    });
}

/// Visual pulse on every pill (timer completion, etc.).
pub fn flash_pills() {
    with_app(|app| {
        for p in app.pills.borrow().iter() {
            p.flash();
        }
    });
}

pub fn refresh_clips() {
    with_app(|app| {
        for p in app.panels.borrow().iter() {
            p.clip_reload();
        }
    });
}

pub fn notify_ui(summary: &str, body: &str) {
    with_app(|app| {
        if let Some(tx) = app.shared.ui_tx.borrow().as_ref() {
            let _ = tx.send(Event::Notify(Banner {
                id: u32::MAX - 1,
                app_name: "naarchy".into(),
                icon: String::new(),
                summary: summary.into(),
                body: body.into(),
                actions: vec![],
                urgency: 1,
            }));
        }
    });
}

pub fn run(
    app: &gtk4::Application,
    cfg: Config,
    events_rx: Receiver<Event>,
    verb_rx: Receiver<Verb>,
    event_tx: Sender<Event>,
    media_cmd: Option<Sender<services::mpris::MediaCmd>>,
    notif_cmd: Option<Sender<services::notifd::NotifCmd>>,
) {
    let shared = Shared::new(cfg);
    crate::ui::SHARED.with(|s| *s.borrow_mut() = Some(shared.clone()));
    shared.restyle();
    *shared.ui_tx.borrow_mut() = Some(event_tx.clone());
    *shared.media_cmd.borrow_mut() = media_cmd;
    *shared.notif_cmd.borrow_mut() = notif_cmd;

    let a = Rc::new(App {
        shared: shared.clone(),
        pills: RefCell::new(Vec::new()),
        panels: RefCell::new(Vec::new()),
        huds: RefCell::new(hud::HudManager::new(app)),
    });

    // Global expand callback used by pill hover + DnD
    {
        let a2 = Rc::downgrade(&a);
        *shared.expand_all_cb.borrow_mut() = Some(Box::new(move || {
            if let Some(a) = a2.upgrade() {
                if !a.shared.fullscreen_hide.get() {
                    for p in a.panels.borrow().iter() {
                        p.expand();
                    }
                }
            }
        }));
    }

    APP.with(|slot| *slot.borrow_mut() = Some(a.clone()));

    // Build surfaces per monitor
    build_surfaces(&a, app);

    // Event pump — drain mpsc channels on a short timeout rather than a
    // busy `idle` source. `DEFAULT_IDLE` busy-loops while the queue is empty
    // and kept the CPU awake 1000s/sec even when idle.
    {
        let a2 = a.clone();
        glib::timeout_add_local(std::time::Duration::from_millis(16), move || {
            let mut did_work = false;
            for _ in 0..12 {
                match events_rx.try_recv() {
                    Ok(ev) => {
                        handle_event(&a2, ev);
                        did_work = true;
                    }
                    Err(_) => break,
                }
            }
            for _ in 0..8 {
                match verb_rx.try_recv() {
                    Ok(v) => {
                        handle_verb(&a2, v);
                        did_work = true;
                    }
                    Err(_) => break,
                }
            }
            // keep polling; timeout guarantees ~60Hz max, not busy-loop
            let _ = did_work;
            glib::ControlFlow::Continue
        });
    }

    // One-second tick: clock, media slider interpolation, timer
    {
        let a3 = a.clone();
        glib::timeout_add_seconds_local(1, move || {
            for p in a3.pills.borrow().iter() {
                p.tick();
            }
            for p in a3.panels.borrow().iter() {
                p.tick();
            }
            glib::ControlFlow::Continue
        });
    }
}

fn build_surfaces(app: &Rc<App>, gtk_app: &gtk4::Application) {
    use gtk4::gdk;
    let Some(display) = gdk::Display::default() else {
        return;
    };
    let n = display.monitors().n_items();

    let sel = app.shared.cfg.borrow().behavior.monitors.clone();

    for i in 0..n {
        let Some(m) = display.monitors().item(i).and_downcast::<gdk::Monitor>() else {
            continue;
        };
        let name = m.connector().unwrap_or_default();
        // Empty connector always wants: never paint zero pills. Named
        // `monitors = ["DP-1"]` lists still match when connector() is present.
        if !name.is_empty() && !sel.wants(&name, i == 0) {
            continue;
        }

        // Pill (collapsed state)
        let on_click: crate::ui::Callback = Rc::new(RefCell::new(None));
        let pill = PillUi::build(gtk_app, &app.shared, Some(&m), on_click.clone());

        // Panel (expanded state)
        let panel = PanelUi::build(gtk_app, &app.shared, Some(&m));

        // Wire the click callback now that panels exist
        let sh_click = app.shared.clone();
        *on_click.borrow_mut() = Some(Box::new(move || {
            if sh_click.expanded.get() {
                request_collapse_all();
            } else {
                request_expand_all();
            }
        }));

        app.pills.borrow_mut().push(pill);
        app.panels.borrow_mut().push(panel);
    }
}

fn handle_event(app: &Rc<App>, ev: Event) {
    match ev {
        Event::Media(st) => {
            *app.shared.media.borrow_mut() = st;
            for p in app.pills.borrow().iter() {
                p.update_media();
            }
            for p in app.panels.borrow().iter() {
                p.media_update();
            }
        }
        Event::Battery(b) => {
            *app.shared.battery.borrow_mut() = b;
            for p in app.pills.borrow().iter() {
                p.update_battery();
            }
        }
        Event::SchemeDark(dark) => {
            if app.shared.dark.get() != dark {
                app.shared.dark.set(dark);
                app.shared.restyle();
            }
        }
        Event::HoverOpen => {
            if !app.shared.fullscreen_hide.get() {
                for p in app.panels.borrow().iter() {
                    p.expand();
                }
            }
        }
        Event::HoverEnd => {
            for p in app.panels.borrow().iter() {
                p.schedule_collapse_if_unhovered();
            }
        }
        Event::FocusLost => {
            for p in app.panels.borrow().iter() {
                p.schedule_collapse_if_unhovered();
            }
        }
        Event::Fullscreen(on) => {
            let hide = on && app.shared.cfg.borrow().behavior.hide_fullscreen;
            app.shared.fullscreen_hide.set(hide);
            for p in app.pills.borrow().iter() {
                p.win.set_visible(!hide);
            }
            if hide && app.shared.expanded.get() && !app.shared.pinned.get() {
                for p in app.panels.borrow().iter() {
                    p.collapse_now();
                }
            }
        }
        Event::MonitorAdded(name) => {
            log::info!("monitor added: {name} (restart naarchy to pick up)");
        }
        Event::ClipNew(raw) => {
            let max_e = app.shared.cfg.borrow().clipboard.max_entries;
            let max_i = app.shared.cfg.borrow().clipboard.max_image_bytes;
            let added = app
                .shared
                .clips
                .borrow_mut()
                .add_raw(&raw.mime, &raw.data, max_e, max_i);
            if added {
                refresh_clips();
            }
        }
        Event::Notify(b) => {
            app.huds.borrow_mut().show_banner(b, &app.shared);
        }
        Event::ConfigChanged(cfg) => {
            *app.shared.cfg.borrow_mut() = *cfg;
            app.shared.restyle();
            for p in app.panels.borrow().iter() {
                p.redraw();
            }
        }
        Event::CalendarReload => {
            let events = services::calendar::today_from_cache();
            *app.shared.cal_events.borrow_mut() = events.clone();
            for p in app.panels.borrow().iter() {
                p.cal_reload();
            }
            // async travel-time enrichment for physical addresses (driving + leave time)
            if events.iter().any(|e| e.directions_url.is_some()) {
                if let Some(tx) = app.shared.ui_tx.borrow().clone() {
                    let evs = events.clone();
                    std::thread::spawn(move || {
                        let enriched = services::calendar::enrich_with_travel(evs);
                        if enriched.iter().any(|e| e.leave_label.is_some()) {
                            let _ = tx.send(Event::CalendarEnriched(enriched));
                        }
                    });
                }
            }
        }
        Event::CalendarEnriched(enriched) => {
            *app.shared.cal_events.borrow_mut() = enriched;
            for p in app.panels.borrow().iter() {
                p.cal_reload();
            }
        }
    }
}

fn handle_verb(app: &Rc<App>, v: Verb) {
    match v {
        Verb::Toggle => {
            if app.shared.expanded.get() {
                for p in app.panels.borrow().iter() {
                    p.collapse();
                }
            } else {
                for p in app.panels.borrow().iter() {
                    p.expand();
                }
            }
        }
        Verb::Expand => {
            for p in app.panels.borrow().iter() {
                p.expand();
            }
        }
        Verb::Collapse => {
            for p in app.panels.borrow().iter() {
                p.collapse();
            }
        }
        Verb::Tab(t) => {
            if let Ok(tab) = t.parse::<TabStr>() {
                app.shared.tab.set(tab.0);
                for p in app.panels.borrow().iter() {
                    p.show_tab(tab.0);
                    if !app.shared.expanded.get() {
                        p.expand();
                    }
                }
            }
        }
        Verb::Hud {
            kind,
            value,
            step,
            icon,
            label,
        } => {
            app.huds.borrow_mut().show(&kind, value, step, icon, label);
        }
        Verb::ShelfAdd(paths) => {
            for p in paths {
                if p.starts_with("http") {
                    app.shared.shelf.borrow_mut().add_text(&p);
                } else {
                    app.shared.shelf.borrow_mut().add_file(&p);
                }
            }
            refresh_after_shelf_change();
        }
        Verb::ShelfClear => {
            app.shared.shelf.borrow_mut().clear();
            refresh_after_shelf_change();
        }
        Verb::ShelfRemove(id) => {
            app.shared.shelf.borrow_mut().remove(&id);
            refresh_after_shelf_change();
        }
        Verb::ClipboardPasteLast => {
            let entry = app.shared.clips.borrow().entries.first().cloned();
            if let Some(e) = entry {
                let store = app.shared.clips.borrow();
                crate::ui::clipview::copy_entry_to_clipboard(&e, &store);
            }
        }
        Verb::Timer(secs) => {
            for p in app.panels.borrow().iter() {
                p.timer_start(secs);
            }
        }
        Verb::TimerStop => {
            *app.shared.timer.borrow_mut() = None;
            app.shared.timer_done_until.set(0);
            for p in app.pills.borrow().iter() {
                p.tick();
            }
            for p in app.panels.borrow().iter() {
                p.tick();
            }
        }
        Verb::Notify { summary, body } => {
            app.huds.borrow_mut().show_banner(
                Banner {
                    id: u32::MAX - 1,
                    app_name: "naarchy".into(),
                    icon: String::new(),
                    summary,
                    body,
                    actions: vec![],
                    urgency: 1,
                },
                &app.shared,
            );
        }
        Verb::Quit => {
            std::process::exit(0);
        }
    }
}

struct TabStr(crate::ui::Tab);
impl std::str::FromStr for TabStr {
    type Err = ();
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        crate::ui::Tab::from_cli(s).map(TabStr).ok_or(())
    }
}

#[cfg(test)]
mod tests {
    use super::TabStr;
    use crate::ui::Tab;
    use std::str::FromStr;

    #[test]
    fn tab_aliases_map_to_inbox() {
        for name in ["shelf", "inbox", "files", "drops", "SHELF"] {
            let t = TabStr::from_str(name).expect(name);
            assert_eq!(t.0, Tab::Inbox);
        }
    }

    #[test]
    fn unknown_tab_is_err() {
        assert!(TabStr::from_str("media").is_err());
        assert!(TabStr::from_str("nosuch").is_err());
        assert!(TabStr::from_str("settings").is_err());
    }
}
