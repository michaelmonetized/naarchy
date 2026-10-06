//! Native preferences write only edited keys so advanced configuration survives.

use super::{hbox, label, vbox};
use gtk4::prelude::*;
use gtk4::{ApplicationWindow, Button, DropDown, SpinButton, Switch};
use std::cell::RefCell;
use std::rc::Rc;
use toml::Value;

thread_local! {
    static WINDOW: RefCell<Option<ApplicationWindow>> = const { RefCell::new(None) };
}

struct Preference {
    section: &'static str,
    key: &'static str,
    original: RefCell<Value>,
    read: Box<dyn Fn() -> Value>,
}

pub fn show(app: &gtk4::Application) {
    crate::app::request_collapse_all_now();
    if WINDOW.with(|window| {
        if let Some(window) = window.borrow().as_ref() {
            window.present();
            true
        } else {
            false
        }
    }) {
        return;
    }
    let cfg = super::with_shared(|sh| sh.cfg.borrow().clone()).unwrap_or_default();
    let size = gtk4::gdk::Display::default()
        .and_then(|display| {
            app.active_window()
                .and_then(|w| w.surface())
                .and_then(|s| display.monitor_at_surface(&s))
                .or_else(|| {
                    display
                        .monitors()
                        .item(0)
                        .and_downcast::<gtk4::gdk::Monitor>()
                })
        })
        .map(|monitor| monitor.geometry());
    let width = size
        .as_ref()
        .map(|r| (r.width() - 80).clamp(420, 560))
        .unwrap_or(560);
    let height = size
        .as_ref()
        .map(|r| (r.height() - 80).clamp(360, 700))
        .unwrap_or(700);
    let win = ApplicationWindow::builder()
        .application(app)
        .title("Naarchy Preferences")
        .default_width(width)
        .default_height(height)
        .resizable(false)
        .build();
    win.set_css_classes(&["naarchy", "na-preferences"]);
    let root = vbox(16);
    root.set_margin_top(24);
    root.set_margin_bottom(20);
    root.set_margin_start(24);
    root.set_margin_end(24);
    root.append(&super::page_heading(
        "Your island, your way",
        "Small details that make it feel like home.",
    ));

    let scroll = gtk4::ScrolledWindow::new();
    scroll.set_policy(gtk4::PolicyType::Never, gtk4::PolicyType::Automatic);
    scroll.set_vexpand(true);
    scroll.add_css_class("na-scroll");
    let form = vbox(10);
    form.set_margin_end(8);
    scroll.set_child(Some(&form));
    root.append(&scroll);
    let mut preferences = Vec::new();

    section(&form, "Appearance");
    let theme = DropDown::from_strings(&["Follow desktop", "Dark", "Light"]);
    theme.set_selected(match cfg.appearance.theme.as_str() {
        "dark" => 1,
        "light" => 2,
        _ => 0,
    });
    row(
        &form,
        "Color scheme",
        "Use the desktop palette or choose a mode.",
        &theme,
    );
    preferences.push(Preference {
        section: "appearance",
        key: "theme",
        original: RefCell::new(Value::String(cfg.appearance.theme.clone())),
        read: Box::new(move || {
            Value::String(
                match theme.selected() {
                    1 => "dark",
                    2 => "light",
                    _ => "auto",
                }
                .into(),
            )
        }),
    });
    switch(
        &form,
        &mut preferences,
        "appearance",
        "omarchy",
        "Match desktop colors",
        "Use your current Omarchy accent and palette.",
        cfg.appearance.omarchy,
    );
    number(
        &form,
        &mut preferences,
        "appearance",
        "opacity",
        "Surface opacity",
        "Percent · lower values let the desktop show through.",
        cfg.appearance.opacity * 100.0,
        10.0,
        100.0,
        1.0,
        0.01,
    );
    switch(
        &form,
        &mut preferences,
        "appearance",
        "reduce_motion",
        "Reduce motion",
        "Show changes immediately without spring animations.",
        cfg.appearance.reduce_motion,
    );
    switch(
        &form,
        &mut preferences,
        "appearance",
        "halloween",
        "Halloween costumes",
        "Occasional fangs, cartoon drips, and a bat on October 31.",
        cfg.appearance.halloween,
    );
    switch(
        &form,
        &mut preferences,
        "clock",
        "show_in_pill",
        "Clock on the island",
        "Keep the current time visible while the island is closed.",
        cfg.clock.show_in_pill,
    );

    section(&form, "Interaction");
    switch(
        &form,
        &mut preferences,
        "behavior",
        "hover_open",
        "Open on hover",
        "Move your pointer to the island to reveal it.",
        cfg.behavior.hover_open,
    );
    number(
        &form,
        &mut preferences,
        "behavior",
        "hover_ms",
        "Hover delay",
        "Milliseconds before the top edge opens the island.",
        cfg.behavior.hover_ms as f64,
        50.0,
        2000.0,
        50.0,
        1.0,
    );
    number(
        &form,
        &mut preferences,
        "behavior",
        "collapse_on_leave_ms",
        "Close delay",
        "Milliseconds to keep the panel open after your pointer leaves.",
        cfg.behavior.collapse_on_leave_ms as f64,
        100.0,
        10000.0,
        50.0,
        1.0,
    );
    switch(
        &form,
        &mut preferences,
        "behavior",
        "hide_fullscreen",
        "Hide in fullscreen",
        "Let your movies, games and presentations take the stage.",
        cfg.behavior.hide_fullscreen,
    );

    section(&form, "Features");
    let restart = label(
        &["na-dim"],
        "Changes to connected services take effect after restarting Naarchy.",
    );
    restart.set_wrap(true);
    restart.set_xalign(0.0);
    form.append(&restart);
    for (key, title, detail, active) in [
        (
            "shelf",
            "File inbox",
            "A temporary place for files, images and text.",
            cfg.features.shelf,
        ),
        (
            "timer",
            "Focus timer",
            "A countdown with presets and a finish chime.",
            cfg.features.timer,
        ),
        (
            "media",
            "Media controls",
            "Control music from compatible desktop apps.",
            cfg.features.media,
        ),
        (
            "clipboard",
            "Clipboard history",
            "Keep recent copies locally on this computer.",
            cfg.features.clipboard,
        ),
        (
            "calendar",
            "Calendar",
            "Your month and the events in your connected feeds.",
            cfg.features.calendar,
        ),
        (
            "notifications",
            "Notification banners",
            "Receive desktop notifications when no other notification service is active.",
            cfg.features.notifications,
        ),
    ] {
        switch(
            &form,
            &mut preferences,
            "features",
            key,
            title,
            detail,
            active,
        );
    }
    number(
        &form,
        &mut preferences,
        "clipboard",
        "max_entries",
        "Clipboard history size",
        "Recent items to keep, plus any pinned items.",
        cfg.clipboard.max_entries as f64,
        0.0,
        2000.0,
        10.0,
        1.0,
    );

    section(&form, "Calendar & privacy");
    let feeds_label = label(&["na-title"], "Calendar feed URLs");
    feeds_label.set_xalign(0.0);
    form.append(&feeds_label);
    let feeds_help = label(&["na-dim"], "One iCalendar feed URL per line. Leave empty to use the calendar without connected events. Feed changes take effect after restarting.");
    feeds_help.set_wrap(true);
    feeds_help.set_xalign(0.0);
    form.append(&feeds_help);
    let feeds = gtk4::TextView::new();
    feeds.add_css_class("na-entry");
    feeds.set_wrap_mode(gtk4::WrapMode::Char);
    feeds.set_top_margin(8);
    feeds.set_bottom_margin(8);
    feeds.set_left_margin(8);
    feeds.set_right_margin(8);
    feeds.set_height_request(76);
    feeds.buffer().set_text(&cfg.calendar.feeds.join("\n"));
    super::describe(&feeds, "Calendar feed URLs, one per line");
    form.append(&feeds);
    preferences.push(Preference {
        section: "calendar",
        key: "feeds",
        original: RefCell::new(Value::Array(
            cfg.calendar
                .feeds
                .iter()
                .cloned()
                .map(Value::String)
                .collect(),
        )),
        read: Box::new(move || {
            let buffer = feeds.buffer();
            Value::Array(
                buffer
                    .text(&buffer.start_iter(), &buffer.end_iter(), false)
                    .lines()
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(|s| Value::String(s.into()))
                    .collect(),
            )
        }),
    });
    switch(&form, &mut preferences, "calendar", "travel_times", "Travel estimates", "Optional: sends event addresses and your approximate location to geocoding and routing providers. Requires restart.", cfg.calendar.travel_times);
    let privacy = label(&["na-dim"], "Files and clipboard history stay on this computer. Removing a file from the inbox leaves the original file in place. Connected calendar feeds are fetched over the network.");
    privacy.set_xalign(0.0);
    privacy.set_wrap(true);
    form.append(&privacy);

    let footer = hbox(10);
    let advanced = Button::with_label("Open config");
    advanced.set_css_classes(&["na-btn", "ghost"]);
    advanced.connect_clicked(|_| crate::util::open_config_in_editor());
    let status = label(&["na-dim"], "");
    status.set_wrap(true);
    status.set_hexpand(true);
    status.set_xalign(0.0);
    let save = Button::with_label("Save changes");
    save.set_css_classes(&["na-btn", "na-timer-start"]);
    save.set_valign(gtk4::Align::Center);
    footer.append(&advanced);
    footer.append(&status);
    footer.append(&save);
    let preferences = Rc::new(preferences);
    save.connect_clicked(move |_| {
        let edits: Vec<_> = preferences
            .iter()
            .filter_map(|p| {
                let value = (p.read)();
                (value != *p.original.borrow()).then_some((p.section, p.key, value))
            })
            .collect();
        if edits.is_empty() {
            status.set_text("All up to date");
            return;
        }
        match crate::config::Config::save_patch(&edits) {
            Ok(()) => {
                let restart = edits.iter().any(|(section, key, _)| {
                    *section == "calendar"
                        || (*section == "features" && !matches!(*key, "shelf" | "timer"))
                });
                for p in preferences.iter() {
                    *p.original.borrow_mut() = (p.read)();
                }
                status.set_text(if restart {
                    "Saved · Restart for service changes"
                } else {
                    "Saved. Make yourself at home."
                });
            }
            Err(error) => status.set_text(&format!("Could not save: {error}")),
        }
    });
    root.append(&footer);
    win.set_child(Some(&root));
    win.connect_close_request(|_| {
        WINDOW.with(|window| window.borrow_mut().take());
        gtk4::glib::Propagation::Proceed
    });
    WINDOW.with(|window| *window.borrow_mut() = Some(win.clone()));
    win.present();
}

fn section(form: &gtk4::Box, text: &str) {
    let title = label(&["na-widget-heading"], text);
    title.set_xalign(0.0);
    title.set_margin_top(14);
    title.set_margin_bottom(2);
    form.append(&title);
}

fn row(form: &gtk4::Box, title: &str, detail: &str, control: &impl IsA<gtk4::Widget>) {
    let row = hbox(18);
    row.set_margin_top(5);
    row.set_margin_bottom(5);
    let text = vbox(3);
    text.set_hexpand(true);
    let title_label = label(&["na-preference-title"], title);
    title_label.set_xalign(0.0);
    let detail_label = label(&["na-dim"], detail);
    detail_label.set_wrap(true);
    detail_label.set_max_width_chars(38);
    detail_label.set_xalign(0.0);
    text.append(&title_label);
    text.append(&detail_label);
    control.set_valign(gtk4::Align::Center);
    super::describe(control, title);
    row.append(&text);
    row.append(control);
    form.append(&row);
}

#[allow(clippy::too_many_arguments)]
fn switch(
    form: &gtk4::Box,
    values: &mut Vec<Preference>,
    section: &'static str,
    key: &'static str,
    title: &str,
    detail: &str,
    active: bool,
) {
    let control = Switch::new();
    control.set_active(active);
    row(form, title, detail, &control);
    values.push(Preference {
        section,
        key,
        original: RefCell::new(Value::Boolean(active)),
        read: Box::new(move || Value::Boolean(control.is_active())),
    });
}

#[allow(clippy::too_many_arguments)]
fn number(
    form: &gtk4::Box,
    values: &mut Vec<Preference>,
    section: &'static str,
    key: &'static str,
    title: &str,
    detail: &str,
    value: f64,
    min: f64,
    max: f64,
    step: f64,
    factor: f64,
) {
    // Preserve advanced values when the UI's suggested range changes between
    // releases; opening preferences must never turn an untouched key into an edit.
    let control = SpinButton::with_range(min.min(value), max.max(value), step);
    control.set_value(value);
    control.set_digits(0);
    control.set_width_chars(4);
    row(form, title, detail, &control);
    let as_value = move |value: f64| {
        if factor == 1.0 {
            Value::Integer(value.round() as i64)
        } else {
            Value::Float((value * factor * 100.0).round() / 100.0)
        }
    };
    values.push(Preference {
        section,
        key,
        original: RefCell::new(as_value(value)),
        read: Box::new(move || {
            control.update();
            as_value(control.value())
        }),
    });
}
