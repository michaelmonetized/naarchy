//! Timer alarm: writes a looping two-tone WAV to the cache once, then plays
//! it through whatever player is installed until `alarm_stop`.

use gtk4::prelude::DisplayExt as _;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

fn path() -> PathBuf {
    let mut p = dirs::cache_dir().unwrap_or_else(|| PathBuf::from("/tmp"));
    p.push("naarchy");
    p.push("alarm-v2.wav");
    p
}

/// Build a short, urgent alarm clip (alternating high/low beeps).
///
/// Arguments: none.
///
/// Returns: path to the WAV on disk, creating it if missing.
fn ensure_wav() -> PathBuf {
    let p = path();
    if p.exists() {
        return p;
    }
    let sr: u32 = 22050;
    let mut samples: Vec<i16> = Vec::new();
    for _ in 0..3 {
        push_tone(&mut samples, sr, 1480.0, 0.12, 0.95);
        samples.extend(std::iter::repeat(0i16).take((sr as f64 * 0.05) as usize));
        push_tone(&mut samples, sr, 1480.0, 0.12, 0.95);
        samples.extend(std::iter::repeat(0i16).take((sr as f64 * 0.22) as usize));
    }
    while samples.len() % 4 != 0 {
        samples.push(0);
    }
    let data_len = samples.len() * 2;
    let mut wav = Vec::new();
    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&(36 + data_len as u32).to_le_bytes());
    wav.extend_from_slice(b"WAVEfmt ");
    wav.extend_from_slice(&16u32.to_le_bytes());
    wav.extend_from_slice(&1u16.to_le_bytes());
    wav.extend_from_slice(&1u16.to_le_bytes());
    wav.extend_from_slice(&sr.to_le_bytes());
    wav.extend_from_slice(&(sr * 2).to_le_bytes());
    wav.extend_from_slice(&2u16.to_le_bytes());
    wav.extend_from_slice(&16u16.to_le_bytes());
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&(data_len as u32).to_le_bytes());
    for s in samples {
        wav.extend_from_slice(&s.to_le_bytes());
    }
    if let Some(dir) = p.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let _ = std::fs::write(&p, wav);
    p
}

fn push_tone(samples: &mut Vec<i16>, sr: u32, freq: f64, dur: f64, amp: f64) {
    let n = (sr as f64 * dur) as usize;
    for i in 0..n {
        let t = i as f64 / sr as f64;
        let attack = (t / 0.006).min(1.0);
        let release = ((dur - t) / 0.018).min(1.0).max(0.0);
        let env = attack * release;
        let s = ((t * freq * std::f64::consts::TAU).sin()
            + 0.35 * (t * freq * 3.0 * std::f64::consts::TAU).sin()
            + 0.12 * (t * freq * 5.0 * std::f64::consts::TAU).sin())
            * amp
            * env;
        samples.push((s * 31000.0).clamp(-32767.0, 32767.0) as i16);
    }
}

struct Alarm {
    stop: Arc<AtomicBool>,
    child: Arc<Mutex<Option<Child>>>,
}

static ALARM: Mutex<Option<Alarm>> = Mutex::new(None);

fn spawn_player(path: &str) -> Option<Child> {
    for player in ["pw-play", "paplay", "aplay", "ffplay", "canberra-gtk-play"] {
        let mut cmd = Command::new(player);
        if player == "ffplay" {
            cmd.arg("-nodisp")
                .arg("-autoexit")
                .arg("-loglevel")
                .arg("quiet")
                .arg(path);
        } else if player == "canberra-gtk-play" {
            cmd.arg("-f").arg(path);
        } else {
            cmd.arg(path);
        }
        cmd.stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        if let Ok(child) = cmd.spawn() {
            return Some(child);
        }
    }
    None
}

/// Loop the alarm until `alarm_stop`. Safe to call while already ringing.
pub fn alarm_start() {
    alarm_stop();
    let stop = Arc::new(AtomicBool::new(false));
    let child: Arc<Mutex<Option<Child>>> = Arc::new(Mutex::new(None));
    {
        let mut slot = ALARM.lock().unwrap_or_else(|e| e.into_inner());
        *slot = Some(Alarm {
            stop: stop.clone(),
            child: child.clone(),
        });
    }
    let p = ensure_wav().to_string_lossy().into_owned();
    thread::spawn(move || {
        let mut warned = false;
        while !stop.load(Ordering::SeqCst) {
            match spawn_player(&p) {
                Some(c) => {
                    let started = std::time::Instant::now();
                    if let Ok(mut g) = child.lock() {
                        *g = Some(c);
                    }
                    loop {
                        if stop.load(Ordering::SeqCst) {
                            return;
                        }
                        thread::sleep(Duration::from_millis(40));
                        let done = match child.lock() {
                            Ok(mut g) => match g.as_mut() {
                                Some(proc) => match proc.try_wait() {
                                    Ok(Some(_)) | Err(_) => {
                                        *g = None;
                                        true
                                    }
                                    Ok(None) => false,
                                },
                                None => true,
                            },
                            Err(_) => true,
                        };
                        if done {
                            let min = Duration::from_millis(900);
                            if let Some(rest) = min.checked_sub(started.elapsed()) {
                                thread::sleep(rest);
                            }
                            break;
                        }
                    }
                }
                None => {
                    if !warned {
                        system_bell();
                        warned = true;
                    }
                    thread::sleep(Duration::from_millis(1500));
                }
            }
            if stop.load(Ordering::SeqCst) {
                break;
            }
            thread::sleep(Duration::from_millis(80));
        }
    });
}

/// Kill the looping alarm, if any.
pub fn alarm_stop() {
    let mut slot = ALARM.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(alarm) = slot.take() {
        alarm.stop.store(true, Ordering::SeqCst);
        if let Ok(mut g) = alarm.child.lock() {
            if let Some(mut c) = g.take() {
                let _ = c.kill();
                let _ = c.wait();
            }
        }
    }
}

/// GDK beep plus a terminal bell, used when no player is installed.
pub fn system_bell() {
    if let Some(display) = gtk4::gdk::Display::default() {
        display.beep();
    }
    eprint!("\x07");
    let _ = std::io::Write::flush(&mut std::io::stderr());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wav_header_is_valid() {
        let p = ensure_wav();
        let bytes = std::fs::read(&p).unwrap();
        assert_eq!(&bytes[0..4], b"RIFF");
        assert_eq!(&bytes[8..12], b"WAVE");
        assert_eq!(&bytes[12..16], b"fmt ");
        assert_eq!(&bytes[36..40], b"data");
        assert!(bytes.len() > 44);
    }
}
