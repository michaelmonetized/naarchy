//! Runs each installed plugin as a child process and turns its stdout into
//! [`Update`]s. One supervisor thread per plugin; no shell, no network, no
//! access to Naarchy state beyond what the protocol carries.

use super::manifest::{Package, Restart};
use super::protocol::{self, Message};
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// What the island needs to know, tagged with the plugin it came from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Update {
    Upsert {
        plugin: String,
        activity: protocol::Activity,
    },
    Clear {
        plugin: String,
        id: Option<String>,
    },
}

pub type Sink = Arc<dyn Fn(Update) + Send + Sync>;

const FIRST_BACKOFF: Duration = Duration::from_secs(2);
const MAX_BACKOFF: Duration = Duration::from_secs(300);
/// A run this long counts as healthy and resets the backoff.
const HEALTHY_RUN: Duration = Duration::from_secs(60);

pub struct Host {
    stop: Arc<AtomicBool>,
    pids: Arc<Mutex<HashMap<String, u32>>>,
}

impl Host {
    /// Start every package. Returns immediately; supervisors run in threads.
    pub fn start(packages: Vec<Package>, data_root: std::path::PathBuf, sink: Sink) -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let pids = Arc::new(Mutex::new(HashMap::new()));
        for package in packages {
            let stop = stop.clone();
            let pids = pids.clone();
            let sink = sink.clone();
            let data = data_root.join(&package.manifest.name);
            let name = format!("naarchy-plugin-{}", package.manifest.name);
            if let Err(error) = std::thread::Builder::new()
                .name(name)
                .spawn(move || supervise(package, data, sink, stop, pids))
            {
                log::warn!("cannot start plugin supervisor: {error}");
            }
        }
        Self { stop, pids }
    }

    /// Stop supervising and terminate running plugins.
    pub fn shutdown(&self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Ok(pids) = self.pids.lock() {
            for pid in pids.values() {
                terminate(*pid);
            }
        }
    }
}

impl Drop for Host {
    fn drop(&mut self) {
        self.shutdown();
    }
}

fn terminate(pid: u32) {
    extern "C" {
        fn kill(pid: i32, sig: i32) -> i32;
    }
    const SIGTERM: i32 = 15;
    if let Ok(pid) = i32::try_from(pid) {
        if pid > 0 {
            // SAFETY: kill only signals the child we spawned and recorded.
            unsafe {
                kill(pid, SIGTERM);
            }
        }
    }
}

/// Root of per-plugin private data directories (`<root>/<name>`).
pub fn data_root() -> std::path::PathBuf {
    crate::util::data_dir().join("plugins")
}

fn supervise(
    package: Package,
    data: std::path::PathBuf,
    sink: Sink,
    stop: Arc<AtomicBool>,
    pids: Arc<Mutex<HashMap<String, u32>>>,
) {
    let name = package.manifest.name.clone();
    let mut backoff = FIRST_BACKOFF;
    while !stop.load(Ordering::SeqCst) {
        let started = Instant::now();
        let success = run_once(&package, &data, &sink, &stop, &pids);
        sink(Update::Clear {
            plugin: name.clone(),
            id: None,
        });
        let restart = match package.manifest.restart {
            Restart::Never => false,
            Restart::OnFailure => !success,
            Restart::Always => true,
        };
        if !restart || stop.load(Ordering::SeqCst) {
            break;
        }
        if started.elapsed() >= HEALTHY_RUN {
            backoff = FIRST_BACKOFF;
        }
        log::info!("plugin {name} exited; restarting in {}s", backoff.as_secs());
        let until = Instant::now() + backoff;
        while Instant::now() < until && !stop.load(Ordering::SeqCst) {
            std::thread::sleep(Duration::from_millis(250));
        }
        backoff = (backoff * 2).min(MAX_BACKOFF);
    }
}

/// Run the plugin until it exits. Returns whether it exited successfully.
fn run_once(
    package: &Package,
    data: &std::path::Path,
    sink: &Sink,
    stop: &AtomicBool,
    pids: &Mutex<HashMap<String, u32>>,
) -> bool {
    let name = &package.manifest.name;
    {
        use std::os::unix::fs::DirBuilderExt;
        let _ = std::fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(data);
    }
    let mut child = match Command::new(&package.program)
        .args(&package.manifest.args)
        .current_dir(&package.dir)
        .env("NAARCHY_API", super::manifest::API_VERSION.to_string())
        .env("NAARCHY_PLUGIN", name)
        .env("NAARCHY_PLUGIN_DIR", &package.dir)
        .env("NAARCHY_PLUGIN_DATA", data)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
    {
        Ok(child) => child,
        Err(error) => {
            log::warn!("plugin {name} failed to start: {error}");
            return false;
        }
    };
    if let Ok(mut pids) = pids.lock() {
        pids.insert(name.clone(), child.id());
    }
    if stop.load(Ordering::SeqCst) {
        terminate(child.id());
    }
    // Keep stdin open for the life of the child: EOF means "Naarchy is gone".
    let mut stdin = child.stdin.take();
    if let Some(input) = stdin.as_mut() {
        if let Ok(hello) = serde_json::to_string(&protocol::Hello::new(name)) {
            let _ = writeln!(input, "{hello}");
            let _ = input.flush();
        }
    }
    if let Some(stderr) = child.stderr.take() {
        let label = name.clone();
        let _ = std::thread::Builder::new()
            .name(format!("naarchy-plugin-{label}-stderr"))
            .spawn(move || {
                for line in BufReader::new(stderr)
                    .lines()
                    .map_while(Result::ok)
                    .take(10_000)
                {
                    log::debug!(
                        "plugin {label}: {}",
                        line.chars().take(512).collect::<String>()
                    );
                }
            });
    }
    if let Some(stdout) = child.stdout.take() {
        read_messages(stdout, |message| match message {
            Message::Upsert(activity) => sink(Update::Upsert {
                plugin: name.clone(),
                activity,
            }),
            Message::Clear(id) => sink(Update::Clear {
                plugin: name.clone(),
                id,
            }),
            Message::Log { level, message } => log::log!(level, "plugin {name}: {message}"),
        });
    }
    drop(stdin);
    let status = child.wait();
    if let Ok(mut pids) = pids.lock() {
        pids.remove(name);
    }
    match status {
        Ok(status) => {
            if !status.success() {
                log::warn!("plugin {name} exited with {status}");
            }
            status.success()
        }
        Err(_) => false,
    }
}

/// Read newline-delimited messages with a hard per-line memory bound.
/// Oversized lines are skipped up to their newline, never buffered whole.
pub fn read_messages(reader: impl Read, mut on_message: impl FnMut(Message)) {
    let mut reader = BufReader::new(reader);
    let mut buf = Vec::with_capacity(1024);
    loop {
        buf.clear();
        let limit = protocol::MAX_LINE_BYTES as u64 + 1;
        match reader.by_ref().take(limit).read_until(b'\n', &mut buf) {
            Ok(0) => return,
            Ok(_) => {}
            Err(_) => return,
        }
        if buf.last() != Some(&b'\n') && buf.len() as u64 >= limit {
            // Discard the rest of this oversized line.
            loop {
                buf.clear();
                match reader.by_ref().take(limit).read_until(b'\n', &mut buf) {
                    Ok(0) | Err(_) => return,
                    Ok(_) if buf.last() == Some(&b'\n') => break,
                    Ok(_) => {}
                }
            }
            continue;
        }
        if let Ok(line) = std::str::from_utf8(&buf) {
            if let Some(message) = protocol::parse_line(line.trim_end()) {
                on_message(message);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn oversized_lines_are_skipped_without_losing_the_next() {
        let mut input = Vec::new();
        input.extend_from_slice(br#"{"type":"activity","id":"a","title":"first"}"#);
        input.push(b'\n');
        input.extend(std::iter::repeat_n(b'x', protocol::MAX_LINE_BYTES * 3));
        input.push(b'\n');
        input.extend_from_slice(br#"{"type":"clear","id":"a"}"#);
        // No trailing newline: the last line still counts.
        let mut seen = Vec::new();
        read_messages(&input[..], |m| seen.push(m));
        assert_eq!(seen.len(), 2);
        assert!(matches!(&seen[0], Message::Upsert(a) if a.title == "first"));
        assert_eq!(seen[1], Message::Clear(Some("a".into())));
    }

    #[test]
    fn plugin_process_round_trip() {
        use std::os::unix::fs::PermissionsExt;
        let dir =
            std::env::temp_dir().join(format!("naarchy-host-{}", crate::shelf_store::new_id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700)).unwrap();
        std::fs::write(
            dir.join("plugin.toml"),
            "name = \"echo-test\"\nversion = \"0\"\napi = 1\nexec = \"run\"\nrestart = \"never\"\n",
        )
        .unwrap();
        // Reads the hello line, proves it got it, then emits an activity.
        std::fs::write(
            dir.join("run"),
            "#!/bin/sh\nread hello\ncase \"$hello\" in *'\"type\":\"hello\"'*) ;; *) exit 3;; esac\n\
             printf '%s\\n' '{\"type\":\"activity\",\"id\":\"x\",\"title\":\"hi\",\"detail\":\"'\"$NAARCHY_PLUGIN\"'\"}'\n",
        )
        .unwrap();
        std::fs::set_permissions(dir.join("run"), std::fs::Permissions::from_mode(0o700)).unwrap();
        let package = super::super::manifest::load(&dir).unwrap();
        let (tx, rx) = std::sync::mpsc::channel();
        let tx = Mutex::new(tx);
        let sink: Sink = Arc::new(move |u| {
            let _ = tx.lock().unwrap().send(u);
        });
        let host = Host::start(vec![package], dir.join("data"), sink);
        let first = rx.recv_timeout(Duration::from_secs(10)).unwrap();
        match first {
            Update::Upsert { plugin, activity } => {
                assert_eq!(plugin, "echo-test");
                assert_eq!(activity.detail, "echo-test");
            }
            other => panic!("unexpected {other:?}"),
        }
        // On exit the host clears everything the plugin showed.
        assert_eq!(
            rx.recv_timeout(Duration::from_secs(10)).unwrap(),
            Update::Clear {
                plugin: "echo-test".into(),
                id: None
            }
        );
        host.shutdown();
        std::fs::remove_dir_all(dir).unwrap();
    }
}
