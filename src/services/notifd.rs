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
    /// freedesktop inhibitions: cookie -> unique bus name of the holder.
    inhibitors: Arc<Mutex<Inhibitors>>,
}

/// Cookies handed out by `Inhibit`, released by `UnInhibit` or when the
/// holder drops off the bus.
#[derive(Default)]
struct Inhibitors {
    next: u32,
    held: HashMap<u32, String>,
}

impl Inhibitors {
    const MAX: usize = 64;

    fn add(&mut self, owner: String) -> Option<u32> {
        if self.held.len() >= Self::MAX {
            return None;
        }
        loop {
            self.next = self.next.wrapping_add(1);
            if self.next != 0 && !self.held.contains_key(&self.next) {
                break;
            }
        }
        self.held.insert(self.next, owner);
        Some(self.next)
    }

    /// Only the holder may release its own cookie.
    fn remove(&mut self, cookie: u32, owner: &str) -> bool {
        if self.held.get(&cookie).map(String::as_str) == Some(owner) {
            self.held.remove(&cookie);
            return true;
        }
        false
    }

    fn drop_owner(&mut self, owner: &str) -> bool {
        let before = self.held.len();
        self.held.retain(|_, o| o != owner);
        before != self.held.len()
    }

    fn active(&self) -> bool {
        !self.held.is_empty()
    }
}

fn hint_str(hints: &HashMap<String, zbus::zvariant::OwnedValue>, key: &str, max: usize) -> String {
    hints
        .get(key)
        .and_then(|v| <&str>::try_from(&**v).ok())
        .filter(|v| v.len() <= max && !v.chars().any(char::is_control))
        .unwrap_or_default()
        .to_string()
}

fn hint_bool(hints: &HashMap<String, zbus::zvariant::OwnedValue>, key: &str) -> bool {
    hints
        .get(key)
        .and_then(|v| bool::try_from(&**v).ok())
        .unwrap_or(false)
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
            desktop_entry: hint_str(&hints, "desktop-entry", 256),
            exec: hint_str(&hints, "omarchy-exec", 4096),
            transient: hint_bool(&hints, "transient"),
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

    /// Notification inhibition (the draft freedesktop extension KDE and GNOME
    /// clients use). While any cookie is held, Naarchy counts instead of popping.
    async fn inhibit(
        &self,
        #[zbus(header)] header: zbus::message::Header<'_>,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
        _desktop_entry: String,
        _reason: String,
        _hints: std::collections::HashMap<String, zbus::zvariant::OwnedValue>,
    ) -> zbus::fdo::Result<u32> {
        let owner = header
            .sender()
            .map(|s| s.to_string())
            .ok_or_else(|| zbus::fdo::Error::Failed("no sender".into()))?;
        let cookie = {
            let mut inh = self.inhibitors.lock().unwrap_or_else(|e| e.into_inner());
            let was = inh.active();
            let cookie = inh
                .add(owner)
                .ok_or_else(|| zbus::fdo::Error::LimitsExceeded("too many inhibitions".into()))?;
            if !was {
                self.tx.send(Event::NotificationsInhibited(true));
            }
            cookie
        };
        let _ = self.inhibited_changed(&emitter).await;
        Ok(cookie)
    }

    async fn un_inhibit(
        &self,
        #[zbus(header)] header: zbus::message::Header<'_>,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
        cookie: u32,
    ) {
        let owner = header.sender().map(|s| s.to_string()).unwrap_or_default();
        let changed = {
            let mut inh = self.inhibitors.lock().unwrap_or_else(|e| e.into_inner());
            inh.remove(cookie, &owner) && !inh.active()
        };
        if changed {
            self.tx.send(Event::NotificationsInhibited(false));
            let _ = self.inhibited_changed(&emitter).await;
        }
    }

    #[zbus(property)]
    fn inhibited(&self) -> bool {
        self.inhibitors
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .active()
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
    let inhibitors = Arc::new(Mutex::new(Inhibitors::default()));
    let server = Notifications {
        tx: tx.clone(),
        counter: AtomicU32::new(100),
        cmd_tx: cmd_tx.clone(),
        active: active.clone(),
        inhibitors: inhibitors.clone(),
    };
    conn.object_server()
        .at("/org/freedesktop/Notifications", server)
        .await?;

    let name = "org.freedesktop.Notifications";
    let wkn = zbus::names::WellKnownName::try_from(name).expect("valid well-known name");
    // Queue rather than give up: when another daemon (Omarchy's shell, mako,
    // dunst) holds the name, Naarchy takes over as soon as it lets go.
    {
        use zbus::fdo::{RequestNameFlags, RequestNameReply};
        match conn
            .request_name_with_flags(wkn, RequestNameFlags::ReplaceExisting.into())
            .await?
        {
            RequestNameReply::PrimaryOwner | RequestNameReply::AlreadyOwner => {}
            RequestNameReply::InQueue => log::warn!(
                "another notification daemon owns {name}; queued, Naarchy takes over when it exits"
            ),
            RequestNameReply::Exists => {
                log::info!("another notification daemon owns {name}; banners disabled");
                return Err(zbus::Error::NameTaken);
            }
        }
    }

    // An inhibition dies with its holder: drop cookies of names that leave the bus.
    {
        let conn = conn.clone();
        let tx = tx.clone();
        tokio::spawn(async move {
            use futures_lite::StreamExt;
            let Ok(dbus) = zbus::fdo::DBusProxy::new(&conn).await else {
                return;
            };
            let Ok(mut changes) = dbus.receive_name_owner_changed().await else {
                return;
            };
            while let Some(change) = changes.next().await {
                let Ok(args) = change.args() else { continue };
                if args.new_owner().is_some() {
                    continue;
                }
                let name = args.name().to_string();
                let released = {
                    let mut inh = inhibitors.lock().unwrap_or_else(|e| e.into_inner());
                    inh.drop_owner(&name) && !inh.active()
                };
                if released {
                    tx.send(Event::NotificationsInhibited(false));
                    if let Ok(iface) = conn
                        .object_server()
                        .interface::<_, Notifications>("/org/freedesktop/Notifications")
                        .await
                    {
                        let _ = iface
                            .get()
                            .await
                            .inhibited_changed(iface.signal_emitter())
                            .await;
                    }
                }
            }
        });
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
            inhibitors: Default::default(),
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
        assert_eq!(banner.desktop_entry, "");
        assert!(!banner.transient);
    }

    #[test]
    fn hints_carry_source_app_exec_and_transient() {
        use zbus::zvariant::{OwnedValue, Str};
        let mut hints: HashMap<String, OwnedValue> = HashMap::new();
        hints.insert(
            "desktop-entry".into(),
            OwnedValue::from(Str::from("org.mozilla.firefox")),
        );
        hints.insert(
            "omarchy-exec".into(),
            OwnedValue::from(Str::from("omarchy-update")),
        );
        hints.insert("transient".into(), OwnedValue::from(true));
        assert_eq!(
            hint_str(&hints, "desktop-entry", 256),
            "org.mozilla.firefox"
        );
        assert_eq!(hint_str(&hints, "omarchy-exec", 4096), "omarchy-update");
        assert_eq!(hint_str(&hints, "desktop-entry", 4), "");
        assert!(hint_bool(&hints, "transient"));
        assert!(!hint_bool(&hints, "resident"));
        hints.insert("bad".into(), OwnedValue::from(Str::from("a\nb")));
        assert_eq!(hint_str(&hints, "bad", 99), "");
    }

    #[test]
    fn inhibitions_belong_to_their_holder() {
        let mut inh = Inhibitors::default();
        let a = inh.add(":1.10".into()).unwrap();
        let b = inh.add(":1.11".into()).unwrap();
        assert_ne!(a, b);
        assert!(inh.active());
        assert!(!inh.remove(a, ":1.11"), "another client cannot release it");
        assert!(inh.remove(a, ":1.10"));
        assert!(inh.active());
        assert!(inh.drop_owner(":1.11"));
        assert!(!inh.active());
        for i in 0..Inhibitors::MAX {
            assert!(inh.add(format!(":1.{i}")).is_some());
        }
        assert!(inh.add(":1.999".into()).is_none());
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
            inhibitors: Default::default(),
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
            inhibitors: Default::default(),
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
