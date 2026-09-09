use crate::services::{Event, EventTx, MediaState};
use futures_lite::StreamExt;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use zbus::proxy;
use zbus::zvariant::OwnedValue;
use zbus::Connection;

const PREFIX: &str = "org.mpris.MediaPlayer2.";

#[proxy(interface = "org.mpris.MediaPlayer2.Player", assume_defaults = true)]
trait Player {
    #[zbus(property)]
    fn metadata(&self) -> zbus::Result<HashMap<String, OwnedValue>>;
    #[zbus(property)]
    fn playback_status(&self) -> zbus::Result<String>;
    #[zbus(property)]
    fn shuffle(&self) -> zbus::Result<bool>;
    #[zbus(property)]
    fn loop_status(&self) -> zbus::Result<String>;
    #[zbus(property)]
    fn position(&self) -> zbus::Result<i64>;
    #[zbus(property)]
    fn can_play(&self) -> zbus::Result<bool>;
    #[zbus(property)]
    fn can_go_next(&self) -> zbus::Result<bool>;
    #[zbus(property)]
    fn can_go_previous(&self) -> zbus::Result<bool>;
    #[zbus(property)]
    fn can_seek(&self) -> zbus::Result<bool>;

    #[zbus(property)]
    fn set_shuffle(&self, value: bool) -> zbus::Result<()>;
    #[zbus(property)]
    fn set_loop_status(&self, value: &str) -> zbus::Result<()>;

    fn play_pause(&self) -> zbus::Result<()>;
    fn next(&self) -> zbus::Result<()>;
    fn previous(&self) -> zbus::Result<()>;
    fn raise(&self) -> zbus::Result<()>;
    fn set_position(
        &self,
        trackid: zbus::zvariant::ObjectPath<'_>,
        position: i64,
    ) -> zbus::Result<()>;
    fn seek(&self, offset: i64) -> zbus::Result<()>;
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub enum MediaCmd {
    PlayPause,
    Next,
    Prev,
    Raise,
    SeekAbs(i64),
    SeekRel(i64),
    SetShuffle(bool),
    SetLoop(u8),
}

fn friendly(bus: &str) -> String {
    bus.strip_prefix(PREFIX)
        .unwrap_or(bus)
        .split('.')
        .next()
        .unwrap_or("player")
        .to_string()
}

pub struct MediaHandle {
    pub cmd_tx: tokio::sync::mpsc::UnboundedSender<MediaCmd>,
}

const PLAYER_PATH: &str = "/org/mpris/MediaPlayer2";
const PLAYER_INTERFACE: &str = "org.mpris.MediaPlayer2.Player";
const DBUS_TIMEOUT: Duration = Duration::from_secs(2);

pub async fn run(tx: EventTx) -> zbus::Result<MediaHandle> {
    let conn = Connection::session().await?;
    run_with_connection(conn, tx).await
}

async fn run_with_connection(conn: Connection, tx: EventTx) -> zbus::Result<MediaHandle> {
    let refresh = Arc::new(tokio::sync::Notify::new());
    // Subscribe before the first snapshot so changes during startup are retained.
    let player_rule = zbus::MatchRule::builder()
        .msg_type(zbus::message::Type::Signal)
        .path(PLAYER_PATH)?
        .build();
    let owner_rule = zbus::MatchRule::builder()
        .msg_type(zbus::message::Type::Signal)
        .sender("org.freedesktop.DBus")?
        .interface("org.freedesktop.DBus")?
        .member("NameOwnerChanged")?
        .build();
    for (rule, owner_changes) in [(player_rule, false), (owner_rule, true)] {
        let mut stream = zbus::MessageStream::for_match_rule(rule, &conn, Some(32)).await?;
        let refresh = refresh.clone();
        tokio::spawn(async move {
            while let Some(Ok(message)) = stream.next().await {
                if owner_changes {
                    let Ok((name, _, _)) = message.body().deserialize::<(String, String, String)>()
                    else {
                        continue;
                    };
                    if !name.starts_with(PREFIX) {
                        continue;
                    }
                }
                refresh.notify_one();
            }
        });
    }

    let (cmd_tx, mut cmd_rx) = tokio::sync::mpsc::unbounded_channel::<MediaCmd>();
    tokio::spawn(async move {
        let mut last: Option<MediaState> = None;
        // Signals provide instant updates; the timer repairs broken player implementations.
        let mut tick = tokio::time::interval(Duration::from_secs(15));
        tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            tokio::select! {
                _ = tick.tick() => {},
                _ = refresh.notified() => {
                    // Collapse metadata/status bursts into one snapshot.
                    tokio::time::sleep(Duration::from_millis(60)).await;
                },
                cmd = cmd_rx.recv() => {
                    let Some(cmd) = cmd else { break };
                    if let Some(current) = last.as_ref() {
                        match tokio::time::timeout(DBUS_TIMEOUT, execute(&conn, &current.bus, cmd)).await {
                            Ok(Ok(())) => {},
                            Ok(Err(e)) => log::debug!("mpris command failed: {e}"),
                            Err(_) => log::debug!("mpris command timed out"),
                        }
                    }
                },
            }
            let next = scan(&conn, last.as_ref().map(|s| s.bus.as_str()), &refresh).await;
            if next != last {
                tx.send(Event::Media(next.clone()));
                last = next;
            }
        }
    });
    Ok(MediaHandle { cmd_tx })
}

async fn player_proxy<'a>(conn: &'a Connection, bus: &str) -> Option<PlayerProxy<'a>> {
    PlayerProxy::builder(conn)
        .destination(bus.to_string())
        .ok()?
        .path(PLAYER_PATH)
        .ok()?
        .cache_properties(zbus::proxy::CacheProperties::No)
        .build()
        .await
        .ok()
}

async fn execute(conn: &Connection, bus: &str, cmd: MediaCmd) -> zbus::Result<()> {
    // Raise belongs to the root interface, not the Player interface.
    if matches!(cmd, MediaCmd::Raise) {
        let root = zbus::Proxy::new(conn, bus, PLAYER_PATH, "org.mpris.MediaPlayer2").await?;
        return root.call("Raise", &()).await;
    }
    let p = player_proxy(conn, bus)
        .await
        .ok_or_else(|| zbus::Error::Failure("player disappeared".into()))?;
    match cmd {
        MediaCmd::PlayPause => p.play_pause().await,
        MediaCmd::Next => p.next().await,
        MediaCmd::Prev => p.previous().await,
        MediaCmd::SeekAbs(us) => seek_abs(&p, us.max(0)).await,
        MediaCmd::SeekRel(us) => p.seek(us).await,
        MediaCmd::SetShuffle(v) => p.set_shuffle(v).await,
        MediaCmd::SetLoop(l) => {
            p.set_loop_status(match l {
                1 => "Track",
                2 => "Playlist",
                _ => "None",
            })
            .await
        }
        MediaCmd::Raise => unreachable!(),
    }
}

async fn seek_abs(p: &PlayerProxy<'_>, us: i64) -> zbus::Result<()> {
    let meta = p.metadata().await.unwrap_or_default();
    let tid = meta.get("mpris:trackid").and_then(|v| match &**v {
        zbus::zvariant::Value::ObjectPath(op) => Some(op.clone()),
        _ => None,
    });
    match tid {
        Some(op) => p.set_position(op.as_ref(), us).await,
        None => {
            let cur = p.position().await.unwrap_or(0);
            p.seek(us.saturating_sub(cur)).await
        }
    }
}

async fn list_player_names(conn: &Connection) -> Vec<String> {
    let query = async {
        let dbus = zbus::fdo::DBusProxy::new(conn).await?;
        dbus.list_names().await
    };
    let Ok(Ok(names)) = tokio::time::timeout(DBUS_TIMEOUT, query).await else {
        return vec![];
    };
    let mut out: Vec<String> = names
        .into_iter()
        .map(|n| n.to_string())
        .filter(|s| s.starts_with(PREFIX))
        .collect();
    out.sort_by(|a, b| {
        a.contains(".instance")
            .cmp(&b.contains(".instance"))
            .then_with(|| a.cmp(b))
    });
    out
}

async fn scan(
    conn: &Connection,
    active: Option<&str>,
    refresh: &Arc<tokio::sync::Notify>,
) -> Option<MediaState> {
    let names = list_player_names(conn).await;
    let mut snapshots = Vec::new();
    // A hung player must not delay each healthy player in turn.
    for group in names.chunks(8) {
        let mut tasks = tokio::task::JoinSet::new();
        for bus in group {
            let conn = conn.clone();
            let bus = bus.clone();
            tasks.spawn(async move {
                tokio::time::timeout(DBUS_TIMEOUT, snapshot(&conn, &bus))
                    .await
                    .ok()
                    .and_then(Result::ok)
            });
        }
        while let Some(result) = tasks.join_next().await {
            if let Ok(Some(state)) = result {
                if state.is_live() {
                    snapshots.push(state);
                }
            }
        }
    }
    snapshots.sort_by(|a, b| {
        b.playing
            .cmp(&a.playing)
            .then_with(|| (Some(b.bus.as_str()) == active).cmp(&(Some(a.bus.as_str()) == active)))
            .then_with(|| {
                a.bus
                    .contains(".instance")
                    .cmp(&b.bus.contains(".instance"))
            })
            .then_with(|| a.bus.cmp(&b.bus))
    });
    let mut state = snapshots.into_iter().next()?;
    // Resolve only the selected player's artwork, outside the async runtime.
    let refresh = refresh.clone();
    state = tokio::task::spawn_blocking(move || {
        resolve_art(&mut state, refresh);
        state
    })
    .await
    .ok()?;
    Some(state)
}

async fn snapshot(conn: &Connection, bus: &str) -> zbus::Result<MediaState> {
    // One GetAll replaces nine per-property reads and transient cache subscriptions.
    let properties = zbus::fdo::PropertiesProxy::builder(conn)
        .destination(bus.to_string())?
        .path(PLAYER_PATH)?
        .build()
        .await?;
    let values = properties
        .get_all(zbus::names::InterfaceName::try_from(PLAYER_INTERFACE)?)
        .await?;
    Ok(state_from_properties(bus, values))
}

fn state_from_properties(bus: &str, mut values: HashMap<String, OwnedValue>) -> MediaState {
    let string = |v: Option<&OwnedValue>| -> String {
        v.and_then(|v| <&str>::try_from(v).ok())
            .unwrap_or_default()
            .to_string()
    };
    let boolean = |key| {
        values
            .get(key)
            .and_then(|v| bool::try_from(v).ok())
            .unwrap_or(false)
    };
    let mut st = MediaState {
        bus: bus.to_string(),
        player: friendly(bus),
        playing: string(values.get("PlaybackStatus")) == "Playing",
        shuffle: boolean("Shuffle"),
        repeat: match string(values.get("LoopStatus")).as_str() {
            "Track" => 1,
            "Playlist" => 2,
            _ => 0,
        },
        can_play: boolean("CanPlay"),
        can_next: boolean("CanGoNext"),
        can_prev: boolean("CanGoPrevious"),
        can_seek: boolean("CanSeek"),
        position_us: values
            .get("Position")
            .and_then(|v| i64::try_from(v).ok())
            .unwrap_or(0)
            .max(0),
        ..Default::default()
    };
    if let Some(meta) = values
        .remove("Metadata")
        .and_then(|v| HashMap::<String, OwnedValue>::try_from(v).ok())
    {
        st.title = string(meta.get("xesam:title"));
        st.album = string(meta.get("xesam:album"));
        st.artist = meta
            .get("xesam:artist")
            .and_then(|v| match &**v {
                zbus::zvariant::Value::Array(a) => Some(
                    a.iter()
                        .filter_map(|v| <&str>::try_from(v).ok())
                        .collect::<Vec<_>>()
                        .join(", "),
                ),
                _ => None,
            })
            .unwrap_or_default();
        let url = string(meta.get("mpris:artUrl"));
        st.art_url = (!url.is_empty()).then_some(url);
        st.length_us = meta
            .get("mpris:length")
            .and_then(|v| i64::try_from(v).ok())
            .unwrap_or(0)
            .max(0);
        if let Some(tid) = meta.get("mpris:trackid") {
            if let zbus::zvariant::Value::ObjectPath(op) = &**tid {
                st.track_id = op.as_str().to_string();
            }
        }
    }
    st
}

fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let (Some(h), Some(l)) = (
                bytes.get(i + 1).and_then(|c| (*c as char).to_digit(16)),
                bytes.get(i + 2).and_then(|c| (*c as char).to_digit(16)),
            ) {
                out.push(((h << 4) | l) as u8);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn resolve_art(st: &mut MediaState, refresh: Arc<tokio::sync::Notify>) {
    let Some(url) = st.art_url.clone() else {
        return;
    };
    if let Some(rest) = url.strip_prefix("file://") {
        let local = if rest.starts_with('/') {
            rest
        } else if let Some(path) = rest.strip_prefix("localhost") {
            path
        } else {
            return;
        };
        if !local.starts_with('/') {
            return;
        }
        let path = percent_decode(local);
        st.art_path = cache_local_art(&url, &path);
        return;
    }
    if url.starts_with("http://") || url.starts_with("https://") {
        let dir = crate::util::cache_dir().join("art");
        let key = crate::util::cache_key(url.as_bytes());
        let dest = dir.join(key);
        if dest.is_file() {
            st.art_path = Some(dest.to_string_lossy().into_owned());
            return;
        }
        let _ = std::fs::create_dir_all(&dir);
        let url2 = url.clone();
        let dest2 = dest.clone();
        {
            let mut g = art_fetching().lock().unwrap_or_else(|e| e.into_inner());
            if g.get(&url)
                .is_some_and(|at| at.elapsed() < Duration::from_secs(60))
            {
                return;
            }
            g.retain(|_, at| at.elapsed() < Duration::from_secs(60));
            if g.len() >= 8 {
                return;
            }
            g.insert(url.clone(), Instant::now());
        }
        std::thread::spawn(move || {
            let agent = ureq::AgentBuilder::new()
                .timeout_connect(Duration::from_secs(4))
                .timeout(Duration::from_secs(8))
                .build();
            if let Ok(resp) = agent.get(&url2).call() {
                if let Ok(buf) = super::read_limited(resp.into_reader(), 8 * 1024 * 1024) {
                    let pending = dest2.with_extension("pending");
                    if !buf.is_empty()
                        && std::fs::write(&pending, &buf).is_ok()
                        && std::fs::rename(&pending, &dest2).is_ok()
                    {
                        refresh.notify_one();
                    }
                    let _ = std::fs::remove_file(pending);
                }
            }
        });
    }
}

fn art_fetching() -> &'static Mutex<HashMap<String, Instant>> {
    static S: std::sync::OnceLock<Mutex<HashMap<String, Instant>>> = std::sync::OnceLock::new();
    S.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Copy a local `file://` art path into the naarchy cache.
///
/// Chromium hands out `/tmp/.org.chromium.Chromium.*` for both the Chrome
/// icon (hollow leftover) and real album art. The leftover is filtered by
/// `MediaState::is_live`. The tmp file itself can vanish between scans, so
/// we snapshot bytes into `$XDG_CACHE_HOME/naarchy/art/`.
///
/// Arguments:
/// - `url`: original `mpris:artUrl` (cache key)
/// - `path`: decoded local path
///
/// Returns: a path that still exists, or `None` if the source is already gone.
fn cache_local_art(url: &str, path: &str) -> Option<String> {
    let src = std::path::Path::new(path);
    let dir = crate::util::cache_dir().join("art");
    if src.starts_with(&dir) && src.is_file() {
        return Some(path.to_string());
    }
    let key = crate::util::cache_key(url.as_bytes());
    let ext = src
        .extension()
        .and_then(|e| e.to_str())
        .filter(|e| e.len() <= 5)
        .map(|e| format!(".{e}"))
        .unwrap_or_default();
    let dest = dir.join(format!("{key}{ext}"));
    if dest.is_file() {
        return Some(dest.to_string_lossy().into_owned());
    }
    if !src.is_file() || src.metadata().ok()?.len() > 8 * 1024 * 1024 {
        return None;
    }
    let _ = std::fs::create_dir_all(&dir);
    match std::fs::copy(src, &dest) {
        Ok(_) => Some(dest.to_string_lossy().into_owned()),
        Err(_) => Some(path.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_local_art_is_none() {
        assert!(cache_local_art("file:///nope", "/nope/missing.png").is_none());
    }

    #[test]
    fn copies_tmp_art_into_cache() {
        let dir = std::env::temp_dir().join("naarchy-art-test");
        let _ = std::fs::create_dir_all(&dir);
        let src = dir.join("cover.bin");
        std::fs::write(&src, b"not-a-real-png").unwrap();
        let got = cache_local_art(
            "file:///tmp/.org.chromium.Chromium.TEST",
            src.to_str().unwrap(),
        )
        .expect("copied");
        assert!(std::path::Path::new(&got).is_file());
        assert_ne!(got, src.to_string_lossy());
        let _ = std::fs::remove_file(&src);
        // Chromium temporary artwork often vanishes after the first snapshot.
        assert_eq!(
            cache_local_art(
                "file:///tmp/.org.chromium.Chromium.TEST",
                src.to_str().unwrap()
            ),
            Some(got.clone())
        );
        let _ = std::fs::remove_file(&got);
    }

    #[test]
    fn snapshot_preserves_capabilities_and_clamps_invalid_position() {
        let mut props = HashMap::new();
        props.insert(
            "PlaybackStatus".into(),
            OwnedValue::try_from(zbus::zvariant::Value::from("Playing")).unwrap(),
        );
        props.insert("CanPlay".into(), OwnedValue::from(true));
        props.insert("CanGoNext".into(), OwnedValue::from(false));
        props.insert("Position".into(), OwnedValue::from(-100_i64));
        let state = state_from_properties("org.mpris.MediaPlayer2.test", props);
        assert!(state.playing);
        assert!(state.can_play);
        assert!(!state.can_next);
        assert!(!state.can_seek);
        assert_eq!(state.position_us, 0);
    }

    struct MockPlayer {
        playing: Arc<std::sync::atomic::AtomicBool>,
        commands: Arc<std::sync::atomic::AtomicUsize>,
    }

    #[zbus::interface(name = "org.mpris.MediaPlayer2.Player")]
    impl MockPlayer {
        #[zbus(property)]
        fn metadata(&self) -> HashMap<String, OwnedValue> {
            [(
                "xesam:title".into(),
                OwnedValue::try_from(zbus::zvariant::Value::from("Integration track")).unwrap(),
            )]
            .into_iter()
            .collect()
        }

        #[zbus(property)]
        fn playback_status(&self) -> &str {
            if self.playing.load(std::sync::atomic::Ordering::Relaxed) {
                "Playing"
            } else {
                "Paused"
            }
        }

        #[zbus(property)]
        fn can_play(&self) -> bool {
            true
        }

        fn play_pause(&self) {
            self.commands
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            self.playing
                .fetch_xor(true, std::sync::atomic::Ordering::Relaxed);
        }
    }

    struct MockRoot(Arc<std::sync::atomic::AtomicUsize>);

    #[zbus::interface(name = "org.mpris.MediaPlayer2")]
    impl MockRoot {
        fn raise(&self) {
            self.0.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        }
    }

    struct PrivateBus(std::process::Child);

    impl Drop for PrivateBus {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }

    async fn media_event(rx: &std::sync::mpsc::Receiver<Event>) -> MediaState {
        tokio::time::timeout(Duration::from_secs(3), async {
            loop {
                if let Ok(Event::Media(Some(state))) = rx.try_recv() {
                    return state;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("media event within signal deadline")
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn isolated_bus_delivers_media_commands_and_property_signals() {
        use std::io::BufRead;
        use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
        let child = std::process::Command::new("dbus-daemon")
            .args(["--session", "--nofork", "--print-address=1", "--nopidfile"])
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .spawn();
        let mut bus = match child {
            Ok(child) => PrivateBus(child),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                eprintln!("Skipping isolated D-Bus integration: dbus-daemon is unavailable");
                return;
            }
            Err(error) => panic!("start isolated bus: {error}"),
        };
        let mut address = String::new();
        std::io::BufReader::new(bus.0.stdout.take().unwrap())
            .read_line(&mut address)
            .unwrap();
        let playing = Arc::new(AtomicBool::new(true));
        let commands = Arc::new(AtomicUsize::new(0));
        let raised = Arc::new(AtomicUsize::new(0));
        let server = zbus::connection::Builder::address(address.trim())
            .unwrap()
            .name("org.mpris.MediaPlayer2.naarchytest")
            .unwrap()
            .serve_at(
                PLAYER_PATH,
                MockPlayer {
                    playing: playing.clone(),
                    commands: commands.clone(),
                },
            )
            .unwrap()
            .serve_at(PLAYER_PATH, MockRoot(raised.clone()))
            .unwrap()
            .build()
            .await
            .unwrap();
        let connection = zbus::connection::Builder::address(address.trim())
            .unwrap()
            .build()
            .await
            .unwrap();
        let (tx, rx) = EventTx::pair();
        let handle = run_with_connection(connection, tx).await.unwrap();
        let initial = media_event(&rx).await;
        assert_eq!(initial.title, "Integration track");
        assert!(initial.playing && initial.can_play);
        handle.cmd_tx.send(MediaCmd::PlayPause).unwrap();
        assert!(!media_event(&rx).await.playing);
        assert_eq!(commands.load(Ordering::Relaxed), 1);

        // An external player's property signal must arrive without a 15s repair poll.
        playing.store(true, Ordering::Relaxed);
        let iface = server
            .object_server()
            .interface::<_, MockPlayer>(PLAYER_PATH)
            .await
            .unwrap();
        iface
            .get()
            .await
            .playback_status_changed(iface.signal_emitter())
            .await
            .unwrap();
        assert!(media_event(&rx).await.playing);

        handle.cmd_tx.send(MediaCmd::Raise).unwrap();
        tokio::time::timeout(Duration::from_secs(2), async {
            while raised.load(Ordering::Relaxed) == 0 {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("Raise dispatched to root MPRIS interface");
        assert_eq!(raised.load(Ordering::Relaxed), 1);
        drop(handle);
    }
}
