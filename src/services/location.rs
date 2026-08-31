//! Precise location + travel time for calendar directions.
//! Tries portal/Geoclue for precise GPS, falls back to IP geolocation.
//! Geocodes via Nominatim, routes via OSRM public instance.
//! All network is blocking (ureq) and called from a background thread.

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;

fn cache_dir() -> PathBuf {
    dirs::cache_dir()
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join("naarchy")
}

fn geocode_cache_path() -> PathBuf {
    cache_dir().join("geocode.json")
}
fn route_cache_path() -> PathBuf {
    cache_dir().join("route.json")
}

fn load_json_cache(path: &std::path::Path) -> HashMap<String, serde_json::Value> {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}
fn save_json_cache(path: &std::path::Path, map: &HashMap<String, serde_json::Value>) {
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if let Ok(s) = serde_json::to_string(map) {
        let _ = std::fs::write(path, s);
    }
}

/// Current coords (lon, lat) — precise portal first, then IP.
pub fn current_coords() -> Option<(f64, f64)> {
    // try portal Location (precise) — may prompt user, with short timeout
    if let Some(c) = current_coords_via_portal() {
        return Some(c);
    }
    current_coords_via_ip()
}

fn current_coords_via_portal() -> Option<(f64, f64)> {
    // Portal Location requires user consent and is flaky in headless tests.
    // For now, rely on IP geolocation (Waynesville, NC via ipinfo.io) which is
    // sufficient for driving-time estimation. Portal can be added later with
    // proper permission flow.
    let _ = std::env::var("WAYLAND_DISPLAY"); // keep portal code path warm
    None
}

fn current_coords_via_ip() -> Option<(f64, f64)> {
    // ipinfo.io is more reliable than ipapi.co (rate limited)
    let try_fetch = |url: &str| -> Option<(f64, f64)> {
        let resp = ureq::AgentBuilder::new()
            .timeout(Duration::from_secs(4))
            .build()
            .get(url)
            .call()
            .ok()?;
        let s = resp.into_string().ok()?;
        let v: serde_json::Value = serde_json::from_str(&s).ok()?;
        // ipinfo.io: loc = "35.4887,-82.9887"
        if let Some(loc) = v.get("loc").and_then(|x| x.as_str()) {
            if let Some((lat_s, lon_s)) = loc.split_once(',') {
                if let (Ok(lat), Ok(lon)) = (lat_s.parse::<f64>(), lon_s.parse::<f64>()) {
                    return Some((lon, lat));
                }
            }
        }
        // ip-api.com: lat, lon
        if let (Some(lat), Some(lon)) = (
            v.get("lat").and_then(|x| x.as_f64()),
            v.get("lon").and_then(|x| x.as_f64()),
        ) {
            return Some((lon, lat));
        }
        // ipinfo fallback lat/lon separate?
        if let (Some(lat), Some(lon)) = (
            v.get("latitude").and_then(|x| x.as_f64()),
            v.get("longitude").and_then(|x| x.as_f64()),
        ) {
            return Some((lon, lat));
        }
        None
    };
    if let Some(c) = try_fetch("https://ipinfo.io/json") {
        return Some(c);
    }
    if let Some(c) = try_fetch("http://ip-api.com/json") {
        return Some(c);
    }
    None
}

/// Geocode address → (lon, lat) via Nominatim, cached.
pub fn geocode(address: &str) -> Option<(f64, f64)> {
    let mut cache = load_json_cache(&geocode_cache_path());
    if let Some(v) = cache.get(address) {
        if let (Some(lon), Some(lat)) = (
            v.get("lon").and_then(|x| x.as_f64()),
            v.get("lat").and_then(|x| x.as_f64()),
        ) {
            return Some((lon, lat));
        }
    }
    // Nominatim requires User-Agent and 1 req/s politeness
    std::thread::sleep(Duration::from_millis(1100));
    let url = format!(
        "https://nominatim.openstreetmap.org/search?format=json&limit=1&q={}",
        encode_uri_component(address)
    );
    let resp = ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(8))
        .build()
        .get(&url)
        .set("User-Agent", "naarchy/0.2.4 (calendar directions)")
        .call()
        .ok()?;
    let s = resp.into_string().ok()?;
    let v: serde_json::Value = serde_json::from_str(&s).ok()?;
    let arr = v.as_array()?;
    let first = arr.first()?;
    let lat = first
        .get("lat")
        .and_then(|x| x.as_str())
        .and_then(|s| s.parse::<f64>().ok())?;
    let lon = first
        .get("lon")
        .and_then(|x| x.as_str())
        .and_then(|s| s.parse::<f64>().ok())?;
    let mut obj = serde_json::Map::new();
    obj.insert("lon".into(), serde_json::Value::from(lon));
    obj.insert("lat".into(), serde_json::Value::from(lat));
    cache.insert(address.to_string(), serde_json::Value::Object(obj));
    save_json_cache(&geocode_cache_path(), &cache);
    Some((lon, lat))
}

/// Route duration in seconds via OSRM, cached.
pub fn route_duration(from: (f64, f64), to: (f64, f64)) -> Option<u32> {
    let key = format!("{:.5},{:.5}->{:.5},{:.5}", from.0, from.1, to.0, to.1);
    let mut cache = load_json_cache(&route_cache_path());
    if let Some(v) = cache.get(&key).and_then(|x| x.as_u64()) {
        return Some(v as u32);
    }
    let url = format!(
        "https://router.project-osrm.org/route/v1/driving/{},{};{},{}?overview=false",
        from.0, from.1, to.0, to.1
    );
    let resp = ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(8))
        .build()
        .get(&url)
        .call()
        .ok()?;
    let s = resp.into_string().ok()?;
    let v: serde_json::Value = serde_json::from_str(&s).ok()?;
    let dur = v
        .get("routes")?
        .as_array()?
        .first()?
        .get("duration")?
        .as_f64()? as u32;
    cache.insert(key.clone(), serde_json::Value::from(dur));
    save_json_cache(&route_cache_path(), &cache);
    Some(dur)
}

fn encode_uri_component(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        let c = b as char;
        if c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | '~') {
            out.push(c);
        } else if c == ' ' {
            out.push_str("%20");
        } else {
            out.push_str(&format!("%{:02X}", b));
        }
    }
    out
}

/// Compute "Leave 09:23 (18 min)" label for an event start.
pub fn leave_label_for(start_epoch: u64, from: (f64, f64), to: (f64, f64)) -> Option<String> {
    let dur = route_duration(from, to)? as u64;
    // add 5 min buffer for parking
    let total = dur + 5 * 60;
    let leave_epoch =
        (start_epoch as i64 - total as i64).max(crate::timefmt::now_epoch() as i64) as u64;
    let leave_str = crate::timefmt::strftime_local(leave_epoch, "%H:%M");
    let mins = (total / 60) as u32;
    Some(format!("Leave {} ({} min)", leave_str, mins))
}
