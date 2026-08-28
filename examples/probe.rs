use gtk4::glib;
use gtk4::prelude::*;
use gtk4_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};

fn main() {
    let app = gtk4::Application::builder()
        .application_id("probe.naarchy")
        .flags(gtk4::gio::ApplicationFlags::NON_UNIQUE)
        .build();
    app.connect_activate(|app| {
        let win = gtk4::ApplicationWindow::builder()
            .application(app)
            .decorated(false)
            .resizable(false)
            .build();
        win.init_layer_shell();
        win.set_layer(Layer::Overlay);
        win.set_exclusive_zone(-1);
        win.set_anchor(Edge::Top, true);
        win.set_keyboard_mode(KeyboardMode::OnDemand);
        win.set_width_request(300);
        win.set_height_request(40);
        let lbl = gtk4::Label::new(Some("NAARCHY PROBE"));
        win.set_child(Some(&lbl));
        win.present();
        let w = win.clone();
        let app2 = app.clone();
        glib::timeout_add_seconds_local(1, move || {
            println!("mapped size: {}x{}", w.width(), w.height());
            app2.quit();
            glib::ControlFlow::Break
        });
    });
    app.run_with_args(&["probe"]);
}
