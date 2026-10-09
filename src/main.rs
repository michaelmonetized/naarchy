mod app;
mod chime;
mod cli;
mod clip_store;
mod config;
mod diagnostics;
mod first_run;
mod ipc;
mod omarchy;
mod seasonal;
mod services;
mod shelf_store;
mod theme;
mod timefmt;
mod ui;
mod util;
mod widget_store;

use config::Config;
use gtk4::prelude::*;
use services::{Event, Verb};

struct Startup {
    cfg: Config,
    first_run: Option<first_run::FirstRun>,
    event_rx: mpsc::Receiver<Event>,
    verb_rx: mpsc::Receiver<Verb>,
    event_tx: services::EventTx,
}

static STARTUP: std::sync::Mutex<Option<Startup>> = std::sync::Mutex::new(None);
use std::sync::mpsc;

/// Print `$XDG_DATA_HOME/naarchy/shelf.json` as a pretty JSON array.
///
/// Client-side. Does not talk to the daemon. Missing file → `[]`.
fn print_shelf_list() {
    let path = util::data_dir().join("shelf.json");
    match std::fs::read_to_string(&path) {
        Ok(s) if s.trim().is_empty() => println!("[]"),
        Ok(s) => match serde_json::from_str::<Vec<crate::shelf_store::ShelfItem>>(&s) {
            Ok(v) => match serde_json::to_string_pretty(&v) {
                Ok(pretty) => println!("{pretty}"),
                Err(_) => print!("{s}"),
            },
            Err(_) => {
                eprintln!("naarchy: shelf.json is not valid JSON");
                std::process::exit(1);
            }
        },
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => println!("[]"),
        Err(error) => {
            eprintln!("naarchy: cannot read shelf: {error}");
            std::process::exit(1);
        }
    }
}

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("warn")).init();

    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(|s| s.as_str()) {
        None | Some("run") | Some("daemon") => start_daemon(),
        Some("--version") | Some("-V") | Some("version") => {
            println!("naarchy {}", env!("CARGO_PKG_VERSION"))
        }
        Some("doctor") => diagnostics::run(),
        Some("install-binds") => cli::print_binds(),
        Some("--help") | Some("-h") | Some("help") => cli::print_help(),
        Some("shelf") if args.get(1).map(|s| s.as_str()) == Some("list") => print_shelf_list(),
        Some(verb) => cli::forward_verb(verb, &args[1..]),
    }
}

fn start_daemon() {
    if std::env::var_os("WAYLAND_DISPLAY").is_none() {
        eprintln!("naarchy: WAYLAND_DISPLAY is not set");
        std::process::exit(1);
    }

    let server = match crate::ipc::Server::bind() {
        Ok(Some(server)) => server,
        Ok(None) => {
            eprintln!("naarchy already running");
            return;
        }
        Err(error) => {
            eprintln!("naarchy: {error}");
            std::process::exit(1);
        }
    };

    let cfg_path = util::config_file();
    let first_run =
        match first_run::FirstRun::inspect(&cfg_path, &util::data_dir(), timefmt::today_parts()) {
            Ok(first_run) => Some(first_run),
            Err(error) => {
                log::warn!("first-run welcome unavailable: {error}");
                None
            }
        };
    Config::save_default_if_missing(&cfg_path);
    let cfg = match Config::load(&cfg_path) {
        Ok(cfg) => cfg,
        Err(error) => {
            eprintln!("naarchy: {error}");
            drop(server);
            std::process::exit(1);
        }
    };

    let (event_tx, event_rx) = services::EventTx::pair();
    let (verb_tx, verb_rx) = mpsc::channel::<Verb>();

    server.listen(verb_tx);

    // One tokio runtime for every zbus/ICS task. Keep it alive with pending().
    {
        let tx = event_tx.clone();
        let feeds = cfg.calendar.feeds.clone();
        let refresh = cfg.calendar.refresh_min;
        let features = cfg.features.clone();
        std::thread::spawn(move || {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .max_blocking_threads(2)
                .thread_name("naarchy-io")
                .build()
                .expect("tokio runtime");
            rt.block_on(async move {
                if features.media {
                    let media_tx = tx.clone();
                    tokio::spawn(async move {
                        match services::mpris::run(media_tx.clone()).await {
                            Ok(handle) => media_tx.send(Event::MediaReady(handle.cmd_tx)),
                            Err(error) => log::warn!("media controls unavailable: {error}"),
                        }
                    });
                }

                {
                    let tx = tx.clone();
                    tokio::spawn(async move {
                        if let Err(e) = services::settings::run(tx).await {
                            log::debug!("portal settings unavailable: {e}");
                        }
                    });
                }

                if features.calendar && !feeds.is_empty() {
                    let tx = tx.clone();
                    tokio::spawn(async move {
                        services::calendar::run(tx, feeds, refresh).await;
                    });
                }

                if features.notifications {
                    let notification_tx = tx.clone();
                    tokio::spawn(async move {
                        match services::notifd::run(notification_tx.clone()).await {
                            Ok(sender) => notification_tx.send(Event::NotificationsReady(sender)),
                            Err(error) => log::warn!("notification service unavailable: {error}"),
                        }
                    });
                }

                std::future::pending::<()>().await;
            });
        });
    }

    // Disabled clipboard history must never capture clipboard data.
    if cfg.features.clipboard {
        let tx = event_tx.clone();
        services::clipboard::spawn(tx);
    }

    // Config hot-reload → forwarded as events
    {
        let tx = event_tx.clone();
        std::thread::spawn(move || {
            let (ctx, crx) = mpsc::channel::<Config>();
            let _watcher = config::ConfigWatcher::spawn(cfg_path, ctx);
            while let Ok(new_cfg) = crx.recv() {
                tx.send(Event::ConfigChanged(Box::new(new_cfg)));
            }
        });
    }

    // GTK application
    let gtk_app = gtk4::Application::builder()
        .application_id("app.naarchy.Naarchy")
        .flags(gtk4::gio::ApplicationFlags::NON_UNIQUE)
        .build();

    {
        *STARTUP.lock().unwrap() = Some(Startup {
            cfg,
            first_run,
            event_rx,
            verb_rx,
            event_tx,
        });
        gtk_app.connect_activate(|gtk_app| {
            gtk4::Window::set_default_icon_name("app.naarchy.Naarchy");
            if let Some(s) = STARTUP.lock().unwrap().take() {
                app::run(
                    gtk_app,
                    s.cfg,
                    s.event_rx,
                    s.verb_rx,
                    s.event_tx,
                    s.first_run,
                );
            } else {
                app::request_expand_all();
            }
        });
    }

    let _hold = gtk_app.hold();
    gtk_app.connect_shutdown(|_| {
        crate::app::dismiss_welcome();
        crate::app::stop_seasonal();
        crate::chime::alarm_stop();
    });
    gtk_app.run_with_args(&["naarchy"]);
}
