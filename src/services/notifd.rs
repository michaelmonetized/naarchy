use crate::services::{Banner, Event};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::mpsc::Sender;
use zbus::object_server::SignalEmitter;
use zbus::Connection;

pub enum NotifCmd {
    Close { id: u32, reason: u8 },
    Action { id: u32, key: String },
}

struct Notifications {
    tx: Sender<Event>,
    counter: AtomicU32,
    cmd_tx: std::sync::mpsc::Sender<NotifCmd>,
}

#[zbus::interface(name = "org.freedesktop.Notifications")]
impl Notifications {
    fn get_capabilities(&self) -> Vec<String> {
        vec![
            "actions".into(),
            "body".into(),
            "body-markup".into(),
            "inline-reply".into(),
        ]
    }

    #[allow(clippy::too_many_arguments)]
    fn notify(
        &self,
        app_name: String,
        _replaces_id: u32,
        app_icon: String,
        summary: String,
        body: String,
        actions: Vec<String>,
        _hints: std::collections::HashMap<String, zbus::zvariant::OwnedValue>,
        _expire_timeout: i32,
    ) -> u32 {
        let id = self.counter.fetch_add(1, Ordering::Relaxed) + 1;
        let mut pairs = Vec::new();
        for c in actions.chunks(2) {
            if c.len() == 2 {
                pairs.push((c[0].clone(), c[1].clone()));
            }
        }
        let _ = self.tx.send(Event::Notify(Banner {
            id,
            app_name,
            icon: app_icon,
            summary,
            body,
            actions: pairs,
            urgency: 1,
        }));
        id
    }

    fn close_notification(&self, id: u32) {
        let _ = self.cmd_tx.send(NotifCmd::Close { id, reason: 3 });
    }

    fn get_server_information(&self) -> (String, String, String, String) {
        (
            "naarchy".into(),
            "https://github.com/michaelmonetized/naarchy".into(),
            env!("CARGO_PKG_VERSION").into(),
            "1.2".into(),
        )
    }

    #[zbus(signal)]
    async fn notification_closed(
        emitter: &SignalEmitter<'_>,
        id: u32,
        reason: u32,
    ) -> zbus::Result<()>;

    #[zbus(signal)]
    async fn action_invoked(
        emitter: &SignalEmitter<'_>,
        id: u32,
        action_key: String,
    ) -> zbus::Result<()>;
}

/// Try to own org.freedesktop.Notifications and serve it.
/// Returns the command sender used by the UI to close banners / invoke actions.
/// Err when another daemon already owns the name (dunst/mako/etc).
pub async fn run(tx: Sender<Event>) -> zbus::Result<Sender<NotifCmd>> {
    let conn = Connection::session().await?;

    let (cmd_tx, cmd_rx) = std::sync::mpsc::channel::<NotifCmd>();
    let server = Notifications {
        tx: tx.clone(),
        counter: AtomicU32::new(100),
        cmd_tx: cmd_tx.clone(),
    };
    conn.object_server()
        .at("/org/freedesktop/Notifications", server)
        .await?;

    let name = "org.freedesktop.Notifications";
    let wkn = zbus::names::WellKnownName::try_from(name).expect("valid well-known name");
    if let Err(e) = conn.request_name(wkn).await {
        log::info!("another notification daemon owns {name} ({e}); banners disabled");
        return Err(e);
    }

    // Relay close/action commands into DBus signals
    let conn2 = conn.clone();
    std::thread::spawn(move || {
        while let Ok(cmd) = cmd_rx.recv() {
            let conn3 = conn2.clone();
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap()
                .block_on(async move {
                    if let Ok(iface_ref) = conn3
                        .object_server()
                        .interface::<_, Notifications>("/org/freedesktop/Notifications")
                        .await
                    {
                        let em = iface_ref.signal_emitter();
                        match cmd {
                            NotifCmd::Close { id, reason } => {
                                let _ =
                                    Notifications::notification_closed(em, id, reason as u32).await;
                            }
                            NotifCmd::Action { id, key } => {
                                let _ = Notifications::action_invoked(em, id, key).await;
                                let _ = Notifications::notification_closed(em, id, 2).await;
                            }
                        }
                    }
                });
        }
    });

    log::info!("naarchy owns org.freedesktop.Notifications");
    Ok(cmd_tx)
}
