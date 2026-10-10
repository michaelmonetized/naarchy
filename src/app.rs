use crate::config::Config;
use crate::services::{self, Banner, Event, Verb};
use crate::ui::panel::PanelUi;
use crate::ui::pill::PillUi;
use crate::ui::{hud, notices, Shared};
use gtk4::glib;
use gtk4::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::mpsc::Receiver;

pub struct App {
    pub shared: Rc<Shared>,
    gtk_app: gtk4::Application,
    monitors: RefCell<Vec<gtk4::gdk::Monitor>>,
    active_monitor: std::cell::Cell<usize>,
    calendar_loading: std::cell::Cell<bool>,
    hyprland: RefCell<Option<services::hyprland::HyprlandHandle>>,
    pills: RefCell<Vec<PillUi>>,
    panels: RefCell<Vec<PanelUi>>,
    huds: RefCell<hud::HudManager>,
    peek: RefCell<notices::Peek>,
    welcome: RefCell<Option<Rc<crate::ui::welcome::Welcome>>>,
    halloween: RefCell<crate::seasonal::Halloween>,
    started: std::time::Instant,
}

thread_local! {
    static APP: RefCell<Option<Rc<App>>> = const { RefCell::new(None) };
    static PUMP: RefCell<Option<(Receiver<Event>, Receiver<Verb>)>> = const { RefCell::new(None) };
}

pub fn with_app<R>(f: impl FnOnce(&Rc<App>) -> R) -> Option<R> {
    APP.with(|a| a.borrow().as_ref().map(f))
}

pub fn dismiss_welcome() {
    with_app(|app| {
        if let Some(welcome) = app.welcome.borrow_mut().take() {
            welcome.finish();
        }
    });
}

pub fn stop_seasonal() {
    with_app(|app| {
        for pill in app.pills.borrow().iter() {
            pill.set_costume(None);
        }
    });
}

fn tick_seasonal(app: &App) {
    let costume = app.halloween.borrow_mut().update(
        crate::timefmt::today_parts(),
        app.shared.cfg.borrow().appearance.halloween,
        app.started.elapsed(),
    );
    for (index, pill) in app.pills.borrow().iter().enumerate() {
        pill.set_costume(costume.filter(|_| index < 8));
    }
}

/// Choose the display under direct interaction. Exactly one shelf opens at a time.
pub fn activate_monitor(monitor: Option<&gtk4::gdk::Monitor>) {
    with_app(|app| {
        let Some(index) = app
            .monitors
            .borrow()
            .iter()
            .position(|m| Some(m) == monitor)
        else {
            return;
        };
        if app.active_monitor.replace(index) != index {
            for panel in app.panels.borrow().iter() {
                panel.collapse_now();
            }
            app.shared.expanded.set(false);
            for pill in app.pills.borrow().iter() {
                pill.win.set_visible(!app.shared.fullscreen_hide.get());
            }
        }
    });
}

fn with_active_panel(app: &App, f: impl FnOnce(&PanelUi)) {
    let panels = app.panels.borrow();
    if let Some(panel) = panels.get(app.active_monitor.get()) {
        f(panel);
    }
}

/// Public helpers used from ui modules (they run on the GTK thread).
pub fn request_expand_all() {
    with_app(|app| {
        with_active_panel(app, |p| p.expand());
    });
}

pub fn request_collapse_all() {
    with_app(|app| {
        for p in app.panels.borrow().iter() {
            p.collapse();
        }
    });
}

pub fn request_collapse_all_now() {
    with_app(|app| {
        for panel in app.panels.borrow().iter() {
            panel.collapse_now();
        }
        app.shared.expanded.set(false);
        show_pills(true);
    });
}

/// User interacted with a panel (tab click) — keep it open.
pub fn poke_panels() {
    with_app(|app| {
        with_active_panel(app, |p| p.poke_collapse_timer());
    });
}

pub fn surface_pointer_enter() {
    with_app(|app| {
        with_active_panel(app, |p| p.note_pointer(true));
    });
}

pub fn surface_pointer_leave() {
    with_app(|app| {
        with_active_panel(app, |p| {
            p.note_pointer(false);
            p.schedule_collapse_if_unhovered();
        });
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
                with_active_panel(app, |p| {
                    p.expand();
                    p.note_pointer(true);
                    p.set_drop_veil(true);
                });
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

/// Drop completed: park in the Inbox, switch to it, keep the island open.
pub fn drop_commit(value: &gtk4::glib::Value) {
    with_app(|app| {
        IGNORE_DROP_LEAVE.with(|c| c.set(true));
        crate::ui::panel::handle_dropped_value(&app.shared, value);
        app.shared.tab.set(crate::ui::Tab::Inbox);
        for (index, p) in app.panels.borrow().iter().enumerate() {
            p.show_tab(crate::ui::Tab::Inbox);
            p.shelf_reload();
            p.set_drop_veil(false);
            if index == app.active_monitor.get() {
                p.expand();
                p.note_pointer(true);
            }
        }
        for p in app.pills.borrow().iter() {
            p.tick();
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

/// Full-screen visual bell on every monitor.
pub fn ring_bell() {
    with_app(|app| app.huds.borrow_mut().ring_bell());
}

pub fn silence_bell() {
    with_app(|app| app.huds.borrow_mut().silence_bell());
}

/// Clear the timer, kill the alarm, drop the visual bell.
pub fn dismiss_timer() {
    with_app(|app| {
        *app.shared.timer.borrow_mut() = None;
        app.shared.timer_done_until.set(0);
        crate::chime::alarm_stop();
        app.huds.borrow_mut().silence_bell();
        for p in app.pills.borrow().iter() {
            p.tick();
        }
        for p in app.panels.borrow().iter() {
            p.tick();
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

/// A notification arrived (from notifd, `naarchy notify`, or the UI itself).
fn notice_arrived(app: &Rc<App>, b: Banner) {
    let (arrival, evicted) = app.shared.notices.borrow_mut().arrive(&b);
    for (id, generation) in evicted {
        notices::dismiss(&app.shared, id, generation, 1);
    }
    let ephemeral = notices::is_ephemeral(&b);
    match arrival {
        notices::Arrival::Peek => {
            let wanted = ephemeral || app.shared.cfg.borrow().notifications.peek;
            // While the island is open the list on Home already shows it.
            if wanted && (ephemeral || !app.shared.expanded.get()) {
                app.peek.borrow_mut().show(&b, &app.shared);
            }
        }
        notices::Arrival::Silenced => {}
        notices::Arrival::Dropped => {
            if ephemeral && b.id != notices::INTERNAL_ID {
                notices::dismiss(&app.shared, b.id, b.generation, 1);
            }
        }
    }
    refresh_notices(app);
}

/// `naarchy notifications …` (the Omarchy notification keybinds).
fn notice_command(app: &Rc<App>, cmd: crate::services::NoticeCmd) {
    use crate::services::NoticeCmd;
    // The peeking card first (it may be an ephemeral one not in the list),
    // else the newest notification in the list.
    let target = || {
        app.peek
            .borrow()
            .current()
            .or_else(|| app.shared.notices.borrow().newest().cloned())
    };
    match cmd {
        NoticeCmd::Dismiss => {
            if let Some(b) = target() {
                notices::dismiss(&app.shared, b.id, b.generation, 2);
            }
        }
        NoticeCmd::Clear => {
            app.peek.borrow_mut().hide();
            let ids = app.shared.notices.borrow().ids();
            for (id, generation) in ids {
                notices::dismiss(&app.shared, id, generation, 2);
            }
        }
        NoticeCmd::Invoke => {
            if let Some(b) = target() {
                notices::activate(&app.shared, &b);
            }
        }
        NoticeCmd::Dnd(want) => {
            let on = want.unwrap_or_else(|| !notices::omarchy_dnd());
            if let Err(error) = notices::set_omarchy_dnd_file(on) {
                log::warn!("could not save do-not-disturb: {error}");
            }
            if app.shared.notices.borrow_mut().set_omarchy_dnd(on) && on {
                app.peek.borrow_mut().hide();
            }
            refresh_notices(app);
        }
    }
}

/// Redraw the bell on every pill and the list on every panel.
fn refresh_notices(app: &Rc<App>) {
    for p in app.panels.borrow().iter() {
        p.notices_reload(&app.shared);
    }
    for p in app.pills.borrow().iter() {
        p.tick();
    }
}

/// The peek card timed out: it collapses into the bell.
pub fn peek_expired(id: u32, generation: u64) {
    with_app(|app| {
        let expired = app.peek.borrow_mut().expire(id, generation);
        if let Some((id, generation)) = expired {
            notices::dismiss(&app.shared, id, generation, 1);
        }
    });
}

pub fn notify_ui(summary: &str, body: &str) {
    with_app(|app| {
        if let Some(tx) = app.shared.ui_tx.borrow().as_ref() {
            tx.send(Event::Notify(Banner {
                id: notices::INTERNAL_ID,
                generation: crate::services::next_banner_generation(),
                app_name: "naarchy".into(),
                icon: String::new(),
                summary: summary.into(),
                body: body.into(),
                actions: vec![],
                urgency: 1,
                timeout_ms: Some(6000),
                desktop_entry: String::new(),
                exec: String::new(),
                transient: true,
            }));
        }
    });
}

pub fn show_pills(on: bool) {
    with_app(|app| {
        if app.shared.fullscreen_hide.get() {
            return;
        }
        for (index, p) in app.pills.borrow().iter().enumerate() {
            p.win.set_visible(on || index != app.active_monitor.get());
        }
    });
}

/// Drain queued service/CLI events. Called from a GTK idle source.
pub fn pump_once() {
    services::clear_wake_pending();
    let mut events = Vec::new();
    let mut verbs = Vec::new();
    PUMP.with(|slot| {
        if let Some((erx, vrx)) = slot.borrow_mut().as_mut() {
            events.extend(erx.try_iter().take(128));
            verbs.extend(vrx.try_iter().take(64));
        }
    });
    let saturated = events.len() == 128 || verbs.len() == 64;
    with_app(|app| {
        for ev in events {
            handle_event(app, ev);
        }
        for v in verbs {
            handle_verb(app, v);
        }
    });
    if saturated {
        services::wake_ui();
    }
}

pub fn run(
    app: &gtk4::Application,
    cfg: Config,
    events_rx: Receiver<Event>,
    verb_rx: Receiver<Verb>,
    event_tx: services::EventTx,
    first_run: Option<crate::first_run::FirstRun>,
) {
    let shared = Shared::new(cfg);
    crate::ui::SHARED.with(|s| *s.borrow_mut() = Some(shared.clone()));
    shared.restyle();
    *shared.ui_tx.borrow_mut() = Some(event_tx.clone());

    let a = Rc::new(App {
        shared: shared.clone(),
        gtk_app: app.clone(),
        monitors: RefCell::new(Vec::new()),
        active_monitor: std::cell::Cell::new(0),
        calendar_loading: std::cell::Cell::new(false),
        hyprland: RefCell::new(None),
        pills: RefCell::new(Vec::new()),
        panels: RefCell::new(Vec::new()),
        huds: RefCell::new(hud::HudManager::new(app)),
        peek: RefCell::new(notices::Peek::new(app)),
        welcome: RefCell::new(None),
        halloween: RefCell::new(crate::seasonal::Halloween::new(crate::util::now_unix())),
        started: std::time::Instant::now(),
    });

    // Global expand callback used by pill hover + DnD
    {
        let a2 = Rc::downgrade(&a);
        *shared.expand_all_cb.borrow_mut() = Some(Box::new(move || {
            if let Some(a) = a2.upgrade() {
                if !a.shared.fullscreen_hide.get() {
                    with_active_panel(&a, |p| p.expand());
                }
            }
        }));
    }

    APP.with(|slot| *slot.borrow_mut() = Some(a.clone()));
    PUMP.with(|slot| *slot.borrow_mut() = Some((events_rx, verb_rx)));
    services::install_wake(glib::MainContext::default());

    build_surfaces(&a, app);
    restart_hyprland(&a);
    pump_once();
    tick_seasonal(&a);

    if let Some(first_run) = first_run {
        match first_run.consume() {
            Ok(true) if !a.shared.fullscreen_hide.get() => {
                *a.welcome.borrow_mut() = Some(crate::ui::welcome::Welcome::show(
                    app,
                    &a.monitors.borrow(),
                    a.shared.accent_rgb(),
                ));
            }
            Ok(_) => {}
            Err(error) => log::warn!("first-run welcome skipped: cannot save state: {error}"),
        }
    }

    // GDK is authoritative for display additions/removals, including non-Hyprland sessions.
    if let Some(display) = gtk4::gdk::Display::default() {
        display.monitors().connect_items_changed(|_, _, _, _| {
            glib::idle_add_local_once(|| {
                with_app(|app| sync_surfaces(app, false));
            });
        });
    }

    // Do-not-disturb follows Omarchy's shell (notifications.json `dnd`).
    {
        let a4 = a.clone();
        let poll = move || {
            let on = notices::omarchy_dnd();
            if a4.shared.notices.borrow_mut().set_omarchy_dnd(on) {
                if on {
                    a4.peek.borrow_mut().hide();
                }
                refresh_notices(&a4);
            }
        };
        poll();
        glib::timeout_add_seconds_local(2, move || {
            poll();
            glib::ControlFlow::Continue
        });
    }

    // One-second tick: clock, timer fire, live activities
    {
        let a3 = a.clone();
        glib::timeout_add_seconds_local(1, move || {
            tick_timer(&a3);
            tick_seasonal(&a3);
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

/// Ring the alarm the first second the countdown hits zero.
fn tick_timer(app: &Rc<App>) {
    let now = crate::ui::now_secs();
    let due = app
        .shared
        .timer
        .borrow()
        .as_ref()
        .is_some_and(|t| t.just_finished(app.shared.timer_done_until.get()));
    if due {
        app.shared.timer_done_until.set(now + 60);
        crate::chime::alarm_start();
        request_collapse_all();
        ring_bell();
        flash_pills();
        notify_ui("Timer done", "Time is up — click the flash to dismiss.");
    }
    let done = app.shared.timer_done_until.get();
    if done > 0 && now >= done {
        *app.shared.timer.borrow_mut() = None;
        app.shared.timer_done_until.set(0);
        crate::chime::alarm_stop();
        app.huds.borrow_mut().silence_bell();
    }
}

fn restart_hyprland(app: &App) {
    drop(app.hyprland.borrow_mut().take());
    let cfg = app.shared.cfg.borrow();
    let zone = services::hyprland::HoverZone {
        band_px: cfg.behavior.hover_band_px as f64,
        pill_w: if cfg.appearance.notch_mode {
            cfg.appearance.pill_width_notch
        } else {
            cfg.appearance.pill_width_island
        }
        .max(crate::ui::liquid::NOTCH_W as i32) as f64,
        pill_h: crate::ui::liquid::LIVE_H,
        panel_w: cfg.appearance.panel_width as f64 * crate::ui::liquid::PANEL_WINDOW_SCALE,
        panel_h: cfg.appearance.panel_height as f64,
    };
    if let Some(tx) = app.shared.ui_tx.borrow().clone() {
        *app.hyprland.borrow_mut() = Some(services::hyprland::spawn(
            tx,
            zone,
            cfg.behavior.hover_ms,
            cfg.behavior.hover_open,
        ));
    }
}

fn build_surfaces(app: &Rc<App>, _gtk_app: &gtk4::Application) {
    sync_surfaces(app, false);
}

fn sync_surfaces(app: &Rc<App>, force: bool) {
    use gtk4::gdk;
    let Some(display) = gdk::Display::default() else {
        return;
    };
    let list = display.monitors();
    let selection = app.shared.cfg.borrow().behavior.monitors.clone();
    let wanted: Vec<gdk::Monitor> = (0..list.n_items())
        .filter_map(|i| {
            let monitor = list.item(i).and_downcast::<gdk::Monitor>()?;
            let name = monitor.connector().unwrap_or_default();
            selection.wants(&name, i == 0).then_some(monitor)
        })
        .collect();
    if !force && *app.monitors.borrow() == wanted {
        return;
    }
    // A display/config change ends the transient welcome instead of remapping it.
    if let Some(welcome) = app.welcome.borrow_mut().take() {
        welcome.finish();
    }
    let was_expanded = app.shared.expanded.get();
    let active = app.monitors.borrow().get(app.active_monitor.get()).cloned();
    let old_monitors = std::mem::take(&mut *app.monitors.borrow_mut());
    let old_panels = std::mem::take(&mut *app.panels.borrow_mut());
    let old_pills = std::mem::take(&mut *app.pills.borrow_mut());
    let mut old: Vec<_> = old_monitors
        .into_iter()
        .zip(old_pills.into_iter().zip(old_panels))
        .map(Some)
        .collect();
    app.active_monitor.set(
        wanted
            .iter()
            .position(|m| Some(m) == active.as_ref())
            .unwrap_or(0),
    );
    *app.monitors.borrow_mut() = wanted.clone();
    for monitor in wanted {
        let existing = if force {
            None
        } else {
            old.iter_mut()
                .find(|entry| entry.as_ref().is_some_and(|(m, _)| m == &monitor))
                .and_then(Option::take)
        };
        let (pill, panel) = if let Some((_, surfaces)) = existing {
            surfaces
        } else {
            let on_click: crate::ui::Callback = Rc::new(RefCell::new(None));
            let pill = PillUi::build(&app.gtk_app, &app.shared, Some(&monitor), on_click.clone());
            let panel = PanelUi::build(&app.gtk_app, &app.shared, Some(&monitor));
            *on_click.borrow_mut() = Some(Box::new(|| {
                with_app(|app| {
                    // The bell is on the island: expanding shows the list on Home.
                    if !app.shared.expanded.get() && !app.shared.notices.borrow().is_empty() {
                        app.shared.tab.set(crate::ui::Tab::Home);
                    }
                    if app.shared.expanded.get() {
                        request_collapse_all();
                    } else {
                        request_expand_all();
                    }
                });
            }));
            panel.show_tab(app.shared.tab.get());
            if app.shared.fullscreen_hide.get() {
                pill.win.set_visible(false);
            }
            (pill, panel)
        };
        app.pills.borrow_mut().push(pill);
        app.panels.borrow_mut().push(panel);
    }
    // Existing outputs keep their GTK surfaces on hot-plug. Dispose only removed
    // or explicitly replaced windows, after replacement surfaces are registered.
    drop(old);

    if was_expanded && !app.shared.fullscreen_hide.get() {
        with_active_panel(app, |panel| panel.expand());
    }
}

fn handle_event(app: &Rc<App>, ev: Event) {
    match ev {
        Event::MediaReady(sender) => *app.shared.media_cmd.borrow_mut() = Some(sender),
        Event::NotificationsReady(sender) => *app.shared.notif_cmd.borrow_mut() = Some(sender),
        Event::Media(st) => {
            *app.shared.media.borrow_mut() = st;
            for p in app.pills.borrow().iter() {
                p.update_media();
            }
            for p in app.panels.borrow().iter() {
                p.media_update();
            }
        }
        Event::SchemeDark(dark) => {
            if app.shared.dark.get() != dark {
                app.shared.dark.set(dark);
                app.shared.restyle();
            }
        }
        Event::HoverOpen(name) => {
            if !app.shared.cfg.borrow().behavior.hover_open {
                return;
            }
            let Some(monitor) = app
                .monitors
                .borrow()
                .iter()
                .find(|m| m.connector().as_deref() == Some(&name))
                .cloned()
            else {
                return;
            };
            activate_monitor(Some(&monitor));
            if !app.shared.fullscreen_hide.get() {
                with_active_panel(app, |p| p.expand());
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
            if on {
                if let Some(welcome) = app.welcome.borrow_mut().take() {
                    welcome.finish();
                }
            }
            let hide = on && app.shared.cfg.borrow().behavior.hide_fullscreen;
            app.shared.fullscreen_hide.set(hide);
            for (index, p) in app.pills.borrow().iter().enumerate() {
                p.win.set_visible(
                    !hide && (!app.shared.expanded.get() || index != app.active_monitor.get()),
                );
            }
            if hide && app.shared.expanded.get() {
                request_collapse_all_now();
            }
        }
        Event::MonitorAdded(name) => {
            log::debug!("monitor added: {name}");
            sync_surfaces(app, false);
        }
        Event::ClipNew(raw) => {
            if !app.shared.cfg.borrow().features.clipboard {
                return;
            }
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
        Event::CloseBanner { id, generation } => {
            app.shared.notices.borrow_mut().remove(id, generation);
            app.peek.borrow_mut().hide_if(id, generation);
            refresh_notices(app);
        }
        Event::Notify(b) => notice_arrived(app, b),
        Event::NotificationsInhibited(on) => {
            if app.shared.notices.borrow_mut().set_inhibited(on) {
                if on {
                    app.peek.borrow_mut().hide();
                }
                refresh_notices(app);
            }
        }
        Event::ConfigChanged(cfg) => {
            if !cfg.behavior.hide_fullscreen {
                app.shared.fullscreen_hide.set(false);
            }
            *app.shared.cfg.borrow_mut() = *cfg;
            app.shared.restyle();
            sync_surfaces(app, true);
            restart_hyprland(app);
        }
        Event::CalendarReload => {
            if app.calendar_loading.replace(true) {
                return;
            }
            if let Some(tx) = app.shared.ui_tx.borrow().clone() {
                std::thread::spawn(move || {
                    tx.send(Event::CalendarLoaded(services::calendar::today_from_cache()));
                });
            }
        }
        Event::CalendarLoaded(events) => {
            app.calendar_loading.set(false);
            let enrich = app.shared.cfg.borrow().calendar.travel_times
                && events
                    .iter()
                    .any(|e| e.directions_url.is_some() && e.leave_label.is_none());
            *app.shared.cal_events.borrow_mut() = events.clone();
            for panel in app.panels.borrow().iter() {
                panel.cal_reload();
            }
            if enrich {
                if let Some(tx) = app.shared.ui_tx.borrow().clone() {
                    std::thread::spawn(move || {
                        tx.send(Event::CalendarEnriched(
                            services::calendar::enrich_with_travel(events),
                        ));
                    });
                }
            }
        }
        Event::Plugin(update) => {
            app.shared.plugins.borrow_mut().apply(update);
            for p in app.pills.borrow().iter() {
                p.tick();
            }
        }
        Event::CalendarEnriched(enriched) => {
            if !app.shared.cfg.borrow().calendar.travel_times {
                return;
            }
            let current = app.shared.cal_events.borrow();
            if current.len() != enriched.len()
                || current
                    .iter()
                    .zip(&enriched)
                    .any(|(a, b)| a.summary != b.summary || a.start_epoch != b.start_epoch)
            {
                return;
            }
            drop(current);
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
                with_active_panel(app, |p| p.expand());
            }
        }
        Verb::Expand => {
            with_active_panel(app, |p| p.expand());
        }
        Verb::Collapse => {
            for p in app.panels.borrow().iter() {
                p.collapse();
            }
        }
        Verb::Tab(t) => {
            if let Ok(tab) = t.parse::<TabStr>() {
                let enabled = {
                    let cfg = app.shared.cfg.borrow();
                    match tab.0 {
                        crate::ui::Tab::Inbox => cfg.features.shelf,
                        crate::ui::Tab::Clipboard => cfg.features.clipboard,
                        crate::ui::Tab::Calendar => cfg.features.calendar,
                        _ => true,
                    }
                };
                if !enabled {
                    notify_ui(
                        "Feature is turned off",
                        "Enable it in your Naarchy configuration and restart.",
                    );
                    return;
                }
                app.shared.tab.set(tab.0);
                for p in app.panels.borrow().iter() {
                    p.show_tab(tab.0);
                }
                with_active_panel(app, |p| p.expand());
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
            if !app.shared.cfg.borrow().features.shelf {
                return;
            }
            let (links, files): (Vec<_>, Vec<_>) = paths
                .into_iter()
                .partition(|p| p.starts_with("https://") || p.starts_with("http://"));
            let mut shelf = app.shared.shelf.borrow_mut();
            shelf.add_files(&files);
            shelf.add_texts(&links);
            drop(shelf);
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
            if !app.shared.cfg.borrow().features.timer {
                return;
            }
            crate::ui::timer::start_timer(&app.shared, secs);
        }
        Verb::TimerStop => {
            dismiss_timer();
        }
        Verb::Notify { summary, body } => {
            notice_arrived(
                app,
                Banner {
                    id: notices::INTERNAL_ID,
                    generation: crate::services::next_banner_generation(),
                    app_name: "naarchy".into(),
                    icon: String::new(),
                    summary,
                    body,
                    actions: vec![],
                    urgency: 1,
                    timeout_ms: Some(6000),
                    desktop_entry: String::new(),
                    exec: String::new(),
                    transient: true,
                },
            );
        }
        Verb::Notices(cmd) => notice_command(app, cmd),
        Verb::Quit => {
            crate::chime::alarm_stop();
            app.gtk_app.quit();
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
