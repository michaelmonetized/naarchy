//! Opt-in native fixture: runs only against a separate /tmp Wayland compositor,
//! with isolated XDG paths. Explicit dates exercise policy without clock changes.

use super::*;
use gtk4::glib;
use std::path::PathBuf;
use std::time::{Duration, Instant};
use wayland_client::{protocol::wl_registry, Connection, Dispatch, QueueHandle};
use wayland_protocols_wlr::virtual_pointer::v1::client::{
    zwlr_virtual_pointer_manager_v1::ZwlrVirtualPointerManagerV1,
    zwlr_virtual_pointer_v1::ZwlrVirtualPointerV1,
};

#[derive(Default)]
struct PointerState(Option<ZwlrVirtualPointerManagerV1>);

impl Dispatch<wl_registry::WlRegistry, ()> for PointerState {
    fn event(
        state: &mut Self,
        registry: &wl_registry::WlRegistry,
        event: wl_registry::Event,
        _: &(),
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        if let wl_registry::Event::Global {
            name, interface, ..
        } = event
        {
            if interface == "zwlr_virtual_pointer_manager_v1" {
                state.0 = Some(registry.bind(name, 1, qh, ()));
            }
        }
    }
}
wayland_client::delegate_noop!(PointerState: ignore ZwlrVirtualPointerManagerV1);
wayland_client::delegate_noop!(PointerState: ignore ZwlrVirtualPointerV1);

fn spin(seconds: f64) {
    let end = Instant::now() + Duration::from_secs_f64(seconds);
    while Instant::now() < end {
        while glib::MainContext::default().iteration(false) {}
        std::thread::sleep(Duration::from_millis(5));
    }
}

fn capture(name: &str) {
    if let Some(dir) = std::env::var_os("NAARCHY_NATIVE_CAPTURE_DIR") {
        let dir = PathBuf::from(dir);
        assert!(dir.starts_with("/tmp/naarchy-seasonal-visual"));
        std::fs::create_dir_all(&dir).unwrap();
        assert!(std::process::Command::new("grim")
            .arg(dir.join(format!("{name}.png")))
            .status()
            .unwrap()
            .success());
    }
}

fn pointer_click(x: i32, y: i32) {
    let connection = Connection::connect_to_env().unwrap();
    let mut queue = connection.new_event_queue();
    let qh = queue.handle();
    let mut state = PointerState::default();
    let _registry = connection.display().get_registry(&qh, ());
    queue.roundtrip(&mut state).unwrap();
    let manager = state
        .0
        .as_ref()
        .expect("isolated compositor virtual pointer");
    let pointer = manager.create_virtual_pointer(None, &qh, ());
    pointer.motion_absolute(1, x as u32, y as u32, 1280, 720);
    pointer.frame();
    connection.flush().unwrap();
    spin(0.1);
    pointer.button(
        2,
        0x110,
        wayland_client::protocol::wl_pointer::ButtonState::Pressed,
    );
    pointer.frame();
    connection.flush().unwrap();
    spin(0.05);
    pointer.button(
        3,
        0x110,
        wayland_client::protocol::wl_pointer::ButtonState::Released,
    );
    pointer.frame();
    connection.flush().unwrap();
    spin(0.1);
    pointer.destroy();
    manager.destroy();
    connection.flush().unwrap();
}

fn type_text(text: &str) {
    // Keep processing GTK events while the virtual keyboard is attached.
    let mut child = std::process::Command::new("wtype")
        .args(["-s", "100", "-d", "10", text])
        .spawn()
        .unwrap();
    while child.try_wait().unwrap().is_none() {
        spin(0.02);
    }
    assert!(child.wait().unwrap().success());
    spin(0.1);
}

#[test]
fn isolated_native_seasonal_lifecycle() {
    let Ok(case) = std::env::var("NAARCHY_NATIVE_SEASONAL") else {
        return;
    };
    for key in [
        "XDG_RUNTIME_DIR",
        "XDG_CONFIG_HOME",
        "XDG_DATA_HOME",
        "XDG_CACHE_HOME",
    ] {
        assert!(PathBuf::from(std::env::var_os(key).expect(key))
            .starts_with("/tmp/naarchy-seasonal-visual"));
    }
    assert!(std::env::var_os("HYPRLAND_INSTANCE_SIGNATURE").is_none());
    assert!(
        PathBuf::from(std::env::var_os("SWAYSOCK").expect("isolated sway IPC"))
            .starts_with("/tmp/naarchy-seasonal-visual")
    );
    gtk4::init().unwrap();
    assert!(gtk4_layer_shell::is_supported());
    let app = gtk4::Application::builder()
        .application_id("app.naarchy.SeasonalTest")
        .flags(gtk4::gio::ApplicationFlags::NON_UNIQUE)
        .build();
    app.register(None::<&gtk4::gio::Cancellable>).unwrap();
    let mut cfg = Config::default();
    cfg.appearance.omarchy = false;
    cfg.appearance.reduce_motion = case.starts_with("reduced");
    cfg.behavior.hover_open = false;
    let first = crate::first_run::FirstRun::inspect(
        &crate::util::config_file(),
        &crate::util::data_dir(),
        if case == "offdate" {
            (2026, 10, 6)
        } else {
            (2026, 10, 10)
        },
    )
    .unwrap();
    let shared = Shared::new(cfg);
    SHARED.with(|slot| *slot.borrow_mut() = Some(shared.clone()));
    shared.restyle();
    let display = gdk::Display::default().unwrap();
    let monitor = display
        .monitors()
        .item(0)
        .and_downcast::<gdk::Monitor>()
        .unwrap();
    let pill = pill::PillUi::build(&app, &shared, Some(&monitor), Rc::new(RefCell::new(None)));
    spin(0.2);
    // A normal application remains usable under the transparent overlay.
    let probe = gtk4::ApplicationWindow::builder()
        .application(&app)
        .title("isolated input probe")
        .default_width(1280)
        .default_height(720)
        .build();
    let input = gtk4::Entry::new();
    input.set_placeholder_text(Some("isolated typing check"));
    input.set_valign(gtk4::Align::End);
    input.set_margin_bottom(80);
    input.set_margin_start(80);
    input.set_margin_end(80);
    probe.set_child(Some(&input));
    let clicks = Rc::new(Cell::new(0));
    let clicks2 = clicks.clone();
    let click = gtk4::GestureClick::new();
    click.set_propagation_phase(gtk4::PropagationPhase::Capture);
    click.connect_released(move |_, _, _, _| clicks2.set(clicks2.get() + 1));
    probe.add_controller(click);
    probe.present();
    input.grab_focus();
    display.clipboard().set_text("isolated clipboard sentinel");
    spin(0.2);
    input.grab_focus();
    type_text("baseline");
    assert_eq!(
        input.text().as_str(),
        "baseline",
        "isolated input probe needs focus before overlay"
    );
    input.set_text("");
    pointer_click(
        monitor.geometry().width() / 2,
        monitor.geometry().height() / 2,
    );
    assert_eq!(
        clicks.get(),
        1,
        "isolated pointer probe needs input before overlay"
    );
    clicks.set(0);
    if ["fangs", "drips", "bat"]
        .iter()
        .any(|motif| case.ends_with(motif))
    {
        let motif = if case.ends_with("fangs") {
            crate::seasonal::Costume::Fangs
        } else if case.ends_with("drips") {
            crate::seasonal::Costume::Drips
        } else {
            crate::seasonal::Costume::Bat
        };
        pill.set_costume(Some(motif));
        spin(0.8);
        pointer_click(monitor.geometry().width() / 2, pill.win.height() + 14);
        assert_eq!(clicks.get(), 1, "costume must be click-through");
        capture(&case);
        assert!(app
            .windows()
            .iter()
            .any(|w| w.title().as_deref() == Some("naarchy-halloween")));
        // Hiding a pill (expanded panel/fullscreen) immediately clears costume.
        pill.win.set_visible(false);
        spin(0.1);
        assert!(!app
            .windows()
            .iter()
            .any(|w| w.title().as_deref() == Some("naarchy-halloween")));
        pill.win.set_visible(true);
        pill.set_costume(Some(motif));
        spin(0.2);
        pill.set_costume(None); // midnight, quiet stretch, disable, or shutdown
        spin(0.1);
        assert!(!app
            .windows()
            .iter()
            .any(|w| w.title().as_deref() == Some("naarchy-halloween")));
    } else if case == "offdate" {
        assert!(!first.consume().unwrap());
        assert!(!crate::first_run::FirstRun::inspect(
            &crate::util::config_file(),
            &crate::util::data_dir(),
            (2026, 10, 10)
        )
        .unwrap()
        .consume()
        .unwrap());
        assert!(!app
            .windows()
            .iter()
            .any(|w| w.title().as_deref() == Some("naarchy-welcome")));
        capture("offdate");
    } else {
        assert!(first.consume().unwrap());
        let welcome = welcome::Welcome::show(&app, std::slice::from_ref(&monitor), (122, 162, 247));
        spin(0.7);
        type_text("normal typing continues");
        assert_eq!(input.text().as_str(), "normal typing continues");
        pointer_click(
            monitor.geometry().width() / 2,
            monitor.geometry().height() / 2,
        );
        assert_eq!(clicks.get(), 1, "welcome drawing must be click-through");
        capture(&format!("{case}-burst"));
        if case == "dismiss" {
            let win = app
                .windows()
                .into_iter()
                .find(|w| w.title().as_deref() == Some("naarchy-welcome"))
                .unwrap();
            let root = win.child().unwrap().downcast::<gtk4::Overlay>().unwrap();
            let button = root
                .last_child()
                .unwrap()
                .downcast::<gtk4::Button>()
                .unwrap();
            let (x, y, w, h) = liquid::widget_rect_in(&win, &button).unwrap();
            pointer_click(x + w / 2, y + h / 2);
            spin(0.1);
        } else if case == "interrupt" {
            welcome.finish();
            welcome.finish();
            spin(0.1);
        } else {
            spin(if case == "reduced" { 0.8 } else { 2.5 });
            assert!(
                app.windows()
                    .iter()
                    .any(|w| w.title().as_deref() == Some("naarchy-welcome")),
                "capture the message before its deadline"
            );
            capture(&format!("{case}-settled"));
            spin(3.1);
        }
        assert!(!app
            .windows()
            .iter()
            .any(|w| w.title().as_deref() == Some("naarchy-welcome")));
        assert!(!crate::first_run::FirstRun::inspect(
            &crate::util::config_file(),
            &crate::util::data_dir(),
            (2026, 10, 10)
        )
        .unwrap()
        .consume()
        .unwrap());
        assert!(pill.win.is_visible());
        capture(&format!("{case}-restored"));
    }
    let clipboard = glib::MainContext::default()
        .block_on(display.clipboard().read_text_future())
        .unwrap();
    assert_eq!(clipboard.as_deref(), Some("isolated clipboard sentinel"));
    drop(pill);
    for window in app.windows() {
        window.destroy();
    }
    spin(0.1);
    assert!(app.windows().is_empty());
    SHARED.with(|slot| slot.borrow_mut().take());
}
