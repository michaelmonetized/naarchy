use crate::services::{Event, EventTx, MediaState};
use std::collections::{HashMap, HashSet};
use std::io::Read;
use std::sync::{Arc, Mutex};
use std::time::Duration;
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

#[derive(Clone)]
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

pub async fn run(tx: EventTx) -> zbus::Result<MediaHandle> {
    let conn = Connection::session().await?;
    let active = Arc::new(Mutex::new(None::<String>));

    let _ = scan_and_emit(&conn, &active, &tx).await;

    {
        let tx = tx.clone();
        let conn2 = conn.clone();
        let active = active.clone();
        tokio::spawn(async move {
            let mut tick = tokio::time::interval(Duration::from_secs(4));
            tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            loop {
                tick.tick().await;
                let _ = scan_and_emit(&conn2, &active, &tx).await;
            }
        });
    }

    let (cmd_tx, mut cmd_rx) = tokio::sync::mpsc::unbounded_channel::<MediaCmd>();
    {
        let conn = conn.clone();
        let tx = tx.clone();
        let active = active.clone();
        tokio::spawn(async move {
            while let Some(cmd) = cmd_rx.recv().await {
                let names = list_player_names(&conn).await;
                let current = active.lock().ok().and_then(|g| g.clone());
                let mut target: Option<String> = None;
                if let Some(cur) = current {
                    if names.contains(&cur) {
                        target = Some(cur);
                    }
                }
                if target.is_none() {
                    for n in &names {
                        if matches!(quick_status(&conn, n).await.as_deref(), Some("Playing")) {
                            target = Some(n.clone());
                            break;
                        }
                    }
                }
                if target.is_none() {
                    target = names.first().cloned();
                }
                let Some(bus) = target else { continue };
                let Some(p) = player_proxy(&conn, &bus).await else {
                    continue;
                };
                let res: zbus::Result<()> = match cmd {
                    MediaCmd::PlayPause => p.play_pause().await,
                    MediaCmd::Next => p.next().await,
                    MediaCmd::Prev => p.previous().await,
                    MediaCmd::Raise => p.raise().await,
                    MediaCmd::SeekAbs(us) => seek_abs(&p, us).await,
                    MediaCmd::SeekRel(us) => p.seek(us).await,
                    MediaCmd::SetShuffle(v) => p.set_shuffle(v).await,
                    MediaCmd::SetLoop(l) => {
                        let s = match l {
                            1 => "Track",
                            2 => "Playlist",
                            _ => "None",
                        };
                        p.set_loop_status(s).await
                    }
                };
                if let Err(e) = res {
                    log::debug!("mpris cmd failed: {e}");
                }
                let _ = scan_and_emit(&conn, &active, &tx).await;
            }
        });
    }

    Ok(MediaHandle { cmd_tx })
}

async fn player_proxy<'a>(conn: &'a Connection, bus: &str) -> Option<PlayerProxy<'a>> {
    PlayerProxy::builder(conn)
        .destination(bus.to_string())
        .ok()?
        .path("/org/mpris/MediaPlayer2")
        .ok()?
        .build()
        .await
        .ok()
}

async fn seek_abs(p: &PlayerProxy<'_>, us: i64) -> zbus::Result<()> {
    let meta = p.metadata().await.unwrap_or_default();
    let tid = meta.get("mpris:trackid").and_then(|v| match &**v {
        zbus::zvariant::Value::ObjectPath(op) => Some(op.clone()),
        _ => None,
    });
    match tid {
        // ObjectPath<'static> coerces to the borrowed form set_position expects
        Some(op) => p.set_position(op.as_ref(), us).await,
        None => {
            let cur = p.position().await.unwrap_or(0);
            p.seek(us - cur).await
        }
    }
}

async fn list_player_names(conn: &Connection) -> Vec<String> {
    let Ok(dbus) = zbus::fdo::DBusProxy::new(conn).await else {
        return vec![];
    };
    let Ok(names) = dbus.list_names().await else {
        return vec![];
    };
    let mut out: Vec<String> = names
        .iter()
        .map(|n| n.as_str().to_string())
        .filter(|s| s.starts_with(PREFIX))
        .collect();
    out.sort_by_key(|a| {
        // prefer non-instance names first for stability
        if a.contains(".instance") {
            1
        } else {
            0
        }
    });
    out
}

fn set_active(active: &Mutex<Option<String>>, v: Option<String>) {
    *active.lock().unwrap_or_else(|e| e.into_inner()) = v;
}

async fn scan_and_emit(
    conn: &Connection,
    active: &Mutex<Option<String>>,
    tx: &EventTx,
) -> zbus::Result<()> {
    let names = list_player_names(conn).await;
    if names.is_empty() {
        set_active(active, None);
        tx.send(Event::Media(None));
        return Ok(());
    }
    let mut ranked: Vec<(bool, usize, &String)> = Vec::new();
    for (i, bus) in names.iter().enumerate() {
        let playing = matches!(quick_status(conn, bus).await.as_deref(), Some("Playing"));
        ranked.push((playing, i, bus));
    }
    ranked.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
    for (_, _, bus) in ranked {
        if let Ok(st) = snapshot(conn, bus).await {
            set_active(active, Some(bus.clone()));
            tx.send(Event::Media(Some(st)));
            return Ok(());
        }
    }
    set_active(active, None);
    tx.send(Event::Media(None));
    Ok(())
}

async fn quick_status(conn: &Connection, bus: &str) -> Option<String> {
    let p = player_proxy(conn, bus).await?;
    p.playback_status().await.ok()
}

async fn snapshot(
    conn: &Connection,
    bus: &str,
) -> Result<MediaState, Box<dyn std::error::Error + Send + Sync>> {
    let p = player_proxy(conn, bus)
        .await
        .ok_or_else(|| std::io::Error::other("no player proxy"))?;
    let mut st = MediaState {
        bus: bus.to_string(),
        player: friendly(bus),
        ..Default::default()
    };

    if let Ok(meta) = p.metadata().await {
        let s = |v: Option<&OwnedValue>| -> String {
            v.and_then(|v| match &**v {
                zbus::zvariant::Value::Str(s) => Some(s.to_string()),
                _ => None,
            })
            .unwrap_or_default()
        };
        st.title = s(meta.get("xesam:title"));
        st.artist = meta
            .get("xesam:artist")
            .and_then(|v| match &**v {
                zbus::zvariant::Value::Array(a) => a.first().and_then(|f| match f {
                    zbus::zvariant::Value::Str(s) => Some(s.to_string()),
                    _ => None,
                }),
                _ => None,
            })
            .unwrap_or_default();
        st.album = s(meta.get("xesam:album"));
        let url = s(meta.get("mpris:artUrl"));
        st.art_url = if url.is_empty() { None } else { Some(url) };
        st.length_us = meta
            .get("mpris:length")
            .and_then(|v| i64::try_from(&**v).ok())
            .unwrap_or(0);
        if let Some(tid) = meta.get("mpris:trackid") {
            if let zbus::zvariant::Value::ObjectPath(op) = &**tid {
                st.track_id = op.as_str().to_string();
            }
        }
    }
    st.playing = p.playback_status().await.unwrap_or_default() == "Playing";
    st.shuffle = p.shuffle().await.unwrap_or(false);
    st.repeat = match p.loop_status().await.unwrap_or_default().as_str() {
        "Track" => 1,
        "Playlist" => 2,
        _ => 0,
    };
    st.can_play = p.can_play().await.unwrap_or(true);
    st.can_next = p.can_go_next().await.unwrap_or(true);
    st.can_prev = p.can_go_previous().await.unwrap_or(true);
    st.can_seek = p.can_seek().await.unwrap_or(true);
    st.position_us = p.position().await.unwrap_or(0);

    resolve_art(&mut st);
    Ok(st)
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

fn resolve_art(st: &mut MediaState) {
    let Some(url) = st.art_url.clone() else {
        return;
    };
    if let Some(rest) = url.strip_prefix("file://") {
        let path = percent_decode(rest.trim_start_matches("localhost"));
        st.art_path = Some(path);
        return;
    }
    if url.starts_with("http://") || url.starts_with("https://") {
        let dir = crate::util::cache_dir().join("art");
        let key = crate::util::cache_key(url.as_bytes());
        let ext = url
            .rsplit('.')
            .next()
            .filter(|e| e.len() <= 5)
            .map(|e| format!(".{e}"))
            .unwrap_or_default();
        let dest = dir.join(format!("{key}{ext}"));
        if dest.exists() {
            st.art_path = Some(dest.to_string_lossy().into_owned());
            return;
        }
        let _ = std::fs::create_dir_all(&dir);
        let url2 = url.clone();
        let dest2 = dest.clone();
        {
            let mut g = art_fetching().lock().unwrap_or_else(|e| e.into_inner());
            if !g.insert(url.clone()) {
                return;
            }
        }
        std::thread::spawn(move || {
            let agent = ureq::AgentBuilder::new()
                .timeout_connect(Duration::from_secs(4))
                .timeout(Duration::from_secs(8))
                .build();
            if let Ok(resp) = agent.get(&url2).call() {
                let mut src = resp.into_reader();
                let mut buf = Vec::new();
                let mut chunk = [0u8; 65536];
                const CAP: usize = 24 * 1024 * 1024;
                loop {
                    match src.read(&mut chunk) {
                        Ok(0) => break,
                        Ok(n) => {
                            if buf.len() + n > CAP {
                                break;
                            }
                            buf.extend_from_slice(&chunk[..n]);
                        }
                        Err(_) => break,
                    }
                }
                if !buf.is_empty() {
                    let _ = std::fs::write(&dest2, &buf);
                }
            }
            art_fetching()
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .remove(&url2);
        });
    }
}

fn art_fetching() -> &'static Mutex<HashSet<String>> {
    static S: std::sync::OnceLock<Mutex<HashSet<String>>> = std::sync::OnceLock::new();
    S.get_or_init(|| Mutex::new(HashSet::new()))
}
