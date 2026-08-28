//! Tiny system-chime for timer completion: writes a short two-tone WAV to the
//! cache once, then plays it with whatever player is installed.

use std::path::PathBuf;

fn path() -> PathBuf {
    let mut p = dirs::cache_dir().unwrap_or_else(|| PathBuf::from("/tmp"));
    p.push("naarchy");
    p.push("chime.wav");
    p
}

fn ensure_wav() -> PathBuf {
    let p = path();
    if p.exists() {
        return p;
    }
    let sr: u32 = 22050;
    let mut samples: Vec<i16> = Vec::new();
    let mut add_tone = |freq: f64, dur: f64, amp: f64| {
        let n = (sr as f64 * dur) as usize;
        for i in 0..n {
            let t = i as f64 / sr as f64;
            let env = (t / 0.02).min(1.0) * ((dur - t) / 0.06).min(1.0);
            let s = (t * freq * std::f64::consts::TAU).sin() * amp * env;
            samples.push((s * 32000.0) as i16);
        }
    };
    add_tone(880.0, 0.16, 0.7);
    add_tone(1318.51, 0.28, 0.7);
    while samples.len() % 4 != 0 {
        samples.push(0);
    }
    let data_len = samples.len() * 2;
    let mut wav = Vec::new();
    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&(36 + data_len as u32).to_le_bytes());
    wav.extend_from_slice(b"WAVEfmt ");
    wav.extend_from_slice(&16u32.to_le_bytes()); // fmt chunk size
    wav.extend_from_slice(&1u16.to_le_bytes()); // PCM
    wav.extend_from_slice(&1u16.to_le_bytes()); // mono
    wav.extend_from_slice(&sr.to_le_bytes());
    wav.extend_from_slice(&(sr * 2).to_le_bytes()); // byte rate
    wav.extend_from_slice(&2u16.to_le_bytes()); // block align
    wav.extend_from_slice(&16u16.to_le_bytes()); // bits
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

/// Fire-and-forget playback through the first available player.
pub fn play() {
    let p = ensure_wav();
    let p_str = p.to_string_lossy().into_owned();
    for player in ["pw-play", "paplay", "aplay", "ffplay"] {
        let mut cmd = std::process::Command::new(player);
        if player == "ffplay" {
            cmd.arg("-nodisp").arg("-autoexit");
        }
        if cmd.arg(&p_str).spawn().is_ok() {
            return;
        }
    }
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
    }
}
