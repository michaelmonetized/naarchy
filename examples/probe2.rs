use gtk4::glib;
use gtk4::prelude::*;
use gtk4_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};

fn setup(
    win: &gtk4::ApplicationWindow,
    mon: Option<&gtk4::gdk::Monitor>,
    w: i32,
    h: i32,
    margin: i32,
) {
    win.init_layer_shell();
    win.set_layer(Layer::Overlay);
    win.set_exclusive_zone(-1);
    win.set_monitor(mon);
    win.set_anchor(Edge::Top, true);
    win.set_keyboard_mode(KeyboardMode::OnDemand);
    win.set_margin(Edge::Top, margin);
    win.set_width_request(w);
    win.set_height_request(h);
}

fn main() {
    let app = gtk4::Application::builder()
        .application_id("probe2.naarchy")
        .flags(gtk4::gio::ApplicationFlags::NON_UNIQUE)
        .build();
    app.connect_activate(|app| {
        let display = gtk4::gdk::Display::default().unwrap();
        let mon = display
            .monitors()
            .item(0)
            .and_downcast::<gtk4::gdk::Monitor>();

        // pill-like: default sizes in builder + title
        let pill = gtk4::ApplicationWindow::builder()
            .application(app)
            .title("naarchy-pill")
            .decorated(false)
            .resizable(false)
            .default_width(300)
            .default_height(34)
            .build();
        setup(&pill, mon.as_ref(), 300, 34, 0);
        pill.set_child(Some(&gtk4::Label::new(Some("PILL 08:25"))));
        // load the same reset-all CSS the real app uses
        let prov = gtk4::CssProvider::new();
        prov.load_from_string(
            "* { all: unset; }\n.na-pill { background: #000000cc; border-radius: 17px; }",
        );
        gtk4::style_context_add_provider_for_display(
            &display,
            &prov,
            gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
        pill.set_css_classes(&["na-pill"]);
        pill.present();

        // panel-like: hidden window
        let panel = gtk4::ApplicationWindow::builder()
            .application(app)
            .title("naarchy-panel")
            .decorated(false)
            .resizable(false)
            .default_width(760)
            .default_height(400)
            .build();
        setup(&panel, mon.as_ref(), 760, 400, 0);
        panel.set_visible(false);
        // give the hidden panel a real child like the real app does
        let pv = gtk4::Box::new(gtk4::Orientation::Vertical, 8);
        pv.append(&gtk4::Label::new(Some("PANEL CONTENT")));
        panel.set_child(Some(&pv));

        let p2 = pill.clone();
        let app2 = app.clone();
        glib::timeout_add_seconds_local(1, move || {
            println!("pill mapped: {}x{}", p2.width(), p2.height());
            app2.quit();
            glib::ControlFlow::Break
        });
    });
    app.run_with_args(&["probe2"]);
}
