use crate::services::{Banner, Event, EventTx};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use zbus::object_server::SignalEmitter;
use zbus::Connection;

#[derive(Debug)]
pub enum NotifCmd {
    Close {
        id: u32,
        generation: u64,
        reason: u8,
    },
    Action {
        id: u32,
        generation: u64,
        key: String,
    },
}

struct Notifications {
    tx: EventTx,
    counter: AtomicU32,
    cmd_tx: tokio::sync::mpsc::UnboundedSender<NotifCmd>,
    active: Arc<Mutex<HashMap<u32, u64>>>,
}

#[zbus::interface(name = "org.freedesktop.Notifications")]
impl Notifications {
    fn get_capabilities(&self) -> Vec<String> {
        vec!["actions".into(), "body".into()]
    }

    #[allow(clippy::too_many_arguments)]
    fn notify(
        &self,
        app_name: String,
        replaces_id: u32,
        app_icon: String,
        summary: String,
        body: String,
        actions: Vec<String>,
        hints: std::collections::HashMap<String, zbus::zvariant::OwnedValue>,
        expire_timeout: i32,
    ) -> zbus::fdo::Result<u32> {
        validate_notification(&app_name, &app_icon, &summary, &body, &actions)?;
        // Publish while holding the revision lock: CloseNotification cannot
        // enqueue a close for this generation before its Notify event exists.
        let mut active = self.active.lock().unwrap_or_else(|e| e.into_inner());
        let generation = super::next_banner_generation();
        let id = if replaces_id != 0 && active.contains_key(&replaces_id) {
            replaces_id
        } else {
            loop {
                let candidate = self.counter.fetch_add(1, Ordering::Relaxed).wrapping_add(1);
                if candidate != 0 && !active.contains_key(&candidate) {
                    break candidate;
                }
            }
        };
        active.insert(id, generation);
        let urgency = hints
            .get("urgency")
            .and_then(|v| u8::try_from(v).ok())
            .filter(|v| *v <= 2)
            .unwrap_or(1);
        let mut pairs = Vec::new();
        for c in actions.chunks(2) {
            if c.len() == 2 {
                pairs.push((c[0].clone(), c[1].clone()));
            }
        }
        self.tx.send(Event::Notify(Banner {
            id,
            generation,
            app_name,
            icon: app_icon,
            summary,
            body,
            actions: pairs,
            urgency,
            timeout_ms: notification_timeout(expire_timeout, urgency),
        }));
        drop(active);
        Ok(id)
    }

    fn close_notification(&self, id: u32) {
        let generation = self
            .active
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(&id)
            .copied();
        if let Some(generation) = generation {
            let _ = self.cmd_tx.send(NotifCmd::Close {
                id,
                generation,
                reason: 3,
            });
        }
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

fn validate_notification(
    app: &str,
    icon: &str,
    summary: &str,
    body: &str,
    actions: &[String],
) -> zbus::fdo::Result<()> {
    for (field, value, limit) in [
        ("app_name", app, 4096),
        ("app_icon", icon, 4096),
        ("summary", summary, 4096),
        ("body", body, 65536),
    ] {
        if value.len() > limit {
            return Err(zbus::fdo::Error::InvalidArgs(format!(
                "{field} exceeds {limit} bytes"
            )));
        }
    }
    if actions.len() > 64 || actions.iter().any(|value| value.len() > 4096) {
        return Err(zbus::fdo::Error::InvalidArgs(
            "actions allow up to 32 pairs with at most 4096 bytes per key or label".into(),
        ));
    }
    Ok(())
}

fn notification_timeout(requested: i32, urgency: u8) -> Option<u64> {
    match requested {
        0 => None,
        value if value > 0 => Some(value as u64),
        _ if urgency == 2 => None,
        _ => Some(5000),
    }
}

fn remove_if_current(active: &Mutex<HashMap<u32, u64>>, id: u32, generation: u64) -> bool {
    let mut active = active.lock().unwrap_or_else(|e| e.into_inner());
    if active.get(&id) != Some(&generation) {
        return false;
    }
    active.remove(&id);
    true
}

/// Try to own org.freedesktop.Notifications and serve it.
/// Returns the command sender used by the UI to close banners / invoke actions.
/// Err when another daemon already owns the name (dunst/mako/etc).
pub async fn run(tx: EventTx) -> zbus::Result<tokio::sync::mpsc::UnboundedSender<NotifCmd>> {
    let conn = Connection::session().await?;

    let (cmd_tx, mut cmd_rx) = tokio::sync::mpsc::unbounded_channel::<NotifCmd>();
    let active = Arc::new(Mutex::new(HashMap::new()));
    let server = Notifications {
        tx: tx.clone(),
        counter: AtomicU32::new(100),
        cmd_tx: cmd_tx.clone(),
        active: active.clone(),
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

    let conn2 = conn.clone();
    tokio::spawn(async move {
        while let Some(cmd) = cmd_rx.recv().await {
            let (id, generation) = match &cmd {
                NotifCmd::Close { id, generation, .. }
                | NotifCmd::Action { id, generation, .. } => (*id, *generation),
            };
            // Expiry, remote close, and user gestures may race. Only the first
            // close removes the visible banner and emits NotificationClosed.
            if !remove_if_current(&active, id, generation) {
                continue;
            }
            tx.send(Event::CloseBanner { id, generation });
            if let Ok(iface_ref) = conn2
                .object_server()
                .interface::<_, Notifications>("/org/freedesktop/Notifications")
                .await
            {
                let em = iface_ref.signal_emitter();
                match cmd {
                    NotifCmd::Close { id, reason, .. } => {
                        let _ = Notifications::notification_closed(em, id, reason as u32).await;
                    }
                    NotifCmd::Action { id, key, .. } => {
                        let _ = Notifications::action_invoked(em, id, key).await;
                        let _ = Notifications::notification_closed(em, id, 2).await;
                    }
                }
            }
        }
    });

    log::info!("naarchy owns org.freedesktop.Notifications");
    Ok(cmd_tx)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn urgency_and_action_pairs_survive_delivery() {
        let (tx, rx) = EventTx::pair();
        let (cmd_tx, _) = tokio::sync::mpsc::unbounded_channel();
        let server = Notifications {
            tx,
            counter: AtomicU32::new(u32::MAX),
            cmd_tx,
            active: Arc::new(Mutex::new(HashMap::new())),
        };
        let hints = [("urgency".into(), zbus::zvariant::OwnedValue::from(2_u8))]
            .into_iter()
            .collect();
        let id = server
            .notify(
                "Calendar".into(),
                0,
                "".into(),
                "Meeting".into(),
                "Starting now".into(),
                vec!["join".into(), "Join".into(), "orphan".into()],
                hints,
                -1,
            )
            .unwrap();
        assert_ne!(id, 0);
        let Event::Notify(banner) = rx.recv().unwrap() else {
            panic!("expected notification");
        };
        assert_eq!(banner.urgency, 2);
        assert_eq!(banner.timeout_ms, None);
        assert_eq!(banner.actions, vec![("join".into(), "Join".into())]);
        assert!(!server.get_capabilities().contains(&"inline-reply".into()));
    }

    #[test]
    fn replacement_reuses_active_id_and_honors_expiration() {
        let (tx, rx) = EventTx::pair();
        let (cmd_tx, _) = tokio::sync::mpsc::unbounded_channel();
        let server = Notifications {
            tx,
            counter: AtomicU32::new(0),
            cmd_tx,
            active: Arc::new(Mutex::new(HashMap::new())),
        };
        let send = |replace, timeout| {
            server
                .notify(
                    "App".into(),
                    replace,
                    "".into(),
                    "Update".into(),
                    "".into(),
                    vec![],
                    Default::default(),
                    timeout,
                )
                .unwrap()
        };
        let first = send(0, 1000);
        assert_eq!(send(first, 0), first);
        assert_ne!(send(999, -1), 999);
        let Event::Notify(first_banner) = rx.recv().unwrap() else {
            panic!()
        };
        let Event::Notify(replacement) = rx.recv().unwrap() else {
            panic!()
        };
        assert_eq!(first_banner.timeout_ms, Some(1000));
        assert_eq!(replacement.timeout_ms, None);
        // The old timeout may already be queued when the replacement arrives.
        // Its generation must leave the replacement active and visible.
        assert_ne!(first_banner.generation, replacement.generation);
        assert!(!remove_if_current(
            &server.active,
            first,
            first_banner.generation
        ));
        assert_eq!(
            server.active.lock().unwrap().get(&first),
            Some(&replacement.generation)
        );
        assert!(remove_if_current(
            &server.active,
            first,
            replacement.generation
        ));
        assert!(!remove_if_current(
            &server.active,
            first,
            replacement.generation
        ));
        assert_eq!(notification_timeout(-1, 1), Some(5000));
        assert_eq!(notification_timeout(1000, 2), Some(1000));
    }

    #[test]
    fn oversized_notifications_are_rejected_before_state_or_queue_changes() {
        let (tx, rx) = EventTx::pair();
        let (cmd_tx, _) = tokio::sync::mpsc::unbounded_channel();
        let server = Notifications {
            tx,
            counter: AtomicU32::new(0),
            cmd_tx,
            active: Arc::new(Mutex::new(HashMap::new())),
        };
        let oversized = "🔔".repeat(1025);
        let error = server
            .notify(
                "App".into(),
                0,
                "".into(),
                oversized,
                "".into(),
                vec![],
                Default::default(),
                -1,
            )
            .unwrap_err();
        assert!(matches!(error, zbus::fdo::Error::InvalidArgs(_)));
        assert!(server.active.lock().unwrap().is_empty());
        assert_eq!(server.counter.load(Ordering::Relaxed), 0);
        assert!(rx.try_recv().is_err());
        assert!(validate_notification("", "", "", &"x".repeat(65537), &[]).is_err());
        assert!(validate_notification("", "", "", "", &vec!["action".into(); 66]).is_err());
        assert!(validate_notification(
            "",
            "",
            &"🔔".repeat(1024),
            &"x".repeat(65536),
            &vec!["label".into(); 64]
        )
        .is_ok());
    }
}
