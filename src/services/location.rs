//! Opt-in calendar travel estimates using approximate IP location, Nominatim,
//! and OSRM. Blocking network work must run outside GTK and Tokio worker threads.

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use super::encode_uri_component;
use serde_json::{json, Value};

type Coordinates = (f64, f64);
type Cache = HashMap<String, Value>;
type CachedPosition = Option<(Instant, Option<Coordinates>)>;

fn agent() -> &'static ureq::Agent {
    static AGENT: OnceLock<ureq::Agent> = OnceLock::new();
    AGENT.get_or_init(|| {
        ureq::AgentBuilder::new()
            .timeout_connect(Duration::from_secs(3))
            .timeout(Duration::from_secs(8))
            .user_agent(concat!(
                "naarchy/",
                env!("CARGO_PKG_VERSION"),
                " (calendar directions)"
            ))
            .build()
    })
}

fn fetch_json(url: &str) -> Option<Value> {
    let response = agent().get(url).call().ok()?;
    let bytes = super::read_limited(response.into_reader(), 256 * 1024).ok()?;
    serde_json::from_slice(&bytes).ok()
}

fn load_cache(path: &Path) -> Cache {
    std::fs::File::open(path)
        .ok()
        .and_then(|file| super::read_limited(file, 2 * 1024 * 1024).ok())
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

fn save_cache(path: &Path, cache: &Cache) {
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(bytes) = serde_json::to_vec(cache) {
        let pending = path.with_extension("pending");
        if std::fs::write(&pending, bytes).is_ok() {
            let _ = std::fs::rename(&pending, path);
        }
        let _ = std::fs::remove_file(pending);
    }
}

fn valid_coords(lon: f64, lat: f64) -> Option<Coordinates> {
    (lon.is_finite()
        && lat.is_finite()
        && (-180.0..=180.0).contains(&lon)
        && (-90.0..=90.0).contains(&lat))
    .then_some((lon, lat))
}

fn coords_from_json(value: &Value) -> Option<Coordinates> {
    if let Some((lat, lon)) = value
        .get("loc")
        .and_then(Value::as_str)
        .and_then(|s| s.split_once(','))
    {
        return valid_coords(lon.parse().ok()?, lat.parse().ok()?);
    }
    let number = |name| {
        value
            .get(name)
            .and_then(|v| v.as_f64().or_else(|| v.as_str()?.parse().ok()))
    };
    valid_coords(
        number("lon").or_else(|| number("longitude"))?,
        number("lat").or_else(|| number("latitude"))?,
    )
}

/// Approximate (longitude, latitude), cached for five minutes. No portal prompt.
/// Call only after the user enables calendar travel estimates.
pub fn current_coords() -> Option<Coordinates> {
    static POSITION: OnceLock<Mutex<CachedPosition>> = OnceLock::new();
    let mut cached = POSITION
        .get_or_init(|| Mutex::new(None))
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    if let Some((at, value)) = *cached {
        let ttl = if value.is_some() { 300 } else { 60 };
        if at.elapsed() < Duration::from_secs(ttl) {
            return value;
        }
    }
    let value = ["https://ipinfo.io/json", "https://ipapi.co/json/"]
        .into_iter()
        .find_map(|url| fetch_json(url).and_then(|v| coords_from_json(&v)));
    *cached = Some((Instant::now(), value));
    value
}

fn cached_value<'a>(cache: &'a Cache, key: &str, ttl: u64) -> Option<&'a Value> {
    let entry = cache.get(key)?;
    let at = entry.get("at")?.as_u64()?;
    let now = crate::timefmt::now_epoch();
    if at > now || now - at >= ttl {
        return None;
    }
    entry.get("value")
}

fn remember(cache: &mut Cache, key: String, value: Value) {
    if cache.len() >= 1024 && !cache.contains_key(&key) {
        if let Some(oldest) = cache
            .iter()
            .min_by_key(|(_, v)| v.get("at").and_then(Value::as_u64).unwrap_or(0))
            .map(|(k, _)| k.clone())
        {
            cache.remove(&oldest);
        }
    }
    cache.insert(
        key,
        json!({"at": crate::timefmt::now_epoch(), "value": value}),
    );
}

/// Address to coordinates, with in-memory/disk caching and one request per second.
pub fn geocode(address: &str) -> Option<Coordinates> {
    let address = address.trim();
    if address.is_empty() || address.len() > 2048 {
        return None;
    }
    static GEOCODES: OnceLock<Mutex<Cache>> = OnceLock::new();
    static LAST_REQUEST: OnceLock<Mutex<Option<Instant>>> = OnceLock::new();
    let path = crate::util::cache_dir().join("geocode.json");
    let mut cache = GEOCODES
        .get_or_init(|| Mutex::new(load_cache(&path)))
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    if let Some(value) = cached_value(&cache, address, 30 * 86400) {
        if !value.is_null() {
            return coords_from_json(value);
        }
        if cached_value(&cache, address, 3600).is_some() {
            return None;
        }
    }
    // Hold the request clock while waiting to enforce a global, not per-thread rate.
    let mut last = LAST_REQUEST
        .get_or_init(|| Mutex::new(None))
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    if let Some(at) = *last {
        std::thread::sleep(Duration::from_secs(1).saturating_sub(at.elapsed()));
    }
    *last = Some(Instant::now());
    let url = format!(
        "https://nominatim.openstreetmap.org/search?format=json&limit=1&q={}",
        encode_uri_component(address)
    );
    let coords = fetch_json(&url).and_then(|v| coords_from_json(v.as_array()?.first()?));
    remember(
        &mut cache,
        address.to_string(),
        coords
            .map(|(lon, lat)| json!({"lon": lon, "lat": lat}))
            .unwrap_or(Value::Null),
    );
    save_cache(&path, &cache);
    coords
}

/// Estimated driving seconds. Routes expire after 30 minutes; failures after one minute.
pub fn route_duration(from: Coordinates, to: Coordinates) -> Option<u32> {
    valid_coords(from.0, from.1)?;
    valid_coords(to.0, to.1)?;
    static ROUTES: OnceLock<Mutex<Cache>> = OnceLock::new();
    let path = crate::util::cache_dir().join("route.json");
    let mut cache = ROUTES
        .get_or_init(|| Mutex::new(load_cache(&path)))
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let key = format!("{:.5},{:.5}->{:.5},{:.5}", from.0, from.1, to.0, to.1);
    if let Some(value) = cached_value(&cache, &key, 1800) {
        if let Some(seconds) = value.as_u64().and_then(|v| u32::try_from(v).ok()) {
            return Some(seconds);
        }
        if cached_value(&cache, &key, 60).is_some() {
            return None;
        }
    }
    let url = format!(
        "https://router.project-osrm.org/route/v1/driving/{},{};{},{}?overview=false",
        from.0, from.1, to.0, to.1
    );
    let seconds = fetch_json(&url)
        .and_then(|v| {
            v.get("routes")?
                .as_array()?
                .first()?
                .get("duration")?
                .as_f64()
        })
        .filter(|v| v.is_finite() && *v >= 0.0 && *v <= u32::MAX as f64)
        .map(|v| v.ceil() as u32);
    remember(
        &mut cache,
        key,
        seconds.map(Value::from).unwrap_or(Value::Null),
    );
    save_cache(&path, &cache);
    seconds
}

/// Includes five minutes for parking. The estimate uses approximate IP location.
pub fn leave_label_for(start_epoch: u64, from: Coordinates, to: Coordinates) -> Option<String> {
    let total = route_duration(from, to)? as u64 + 5 * 60;
    let leave_epoch = start_epoch.saturating_sub(total);
    let minutes = total.div_ceil(60);
    if leave_epoch <= crate::timefmt::now_epoch() {
        return Some(format!("Leave now (~{minutes} min)"));
    }
    let time = crate::timefmt::strftime_local(leave_epoch, "%H:%M");
    Some(format!("Leave {time} (~{minutes} min)"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_provider_coordinates() {
        assert_eq!(
            coords_from_json(&json!({"loc": "35.4,-82.9"})),
            Some((-82.9, 35.4))
        );
        assert_eq!(
            coords_from_json(&json!({"lon": "-82.9", "lat": "35.4"})),
            Some((-82.9, 35.4))
        );
        assert_eq!(coords_from_json(&json!({"loc": "NaN,0"})), None);
        assert_eq!(
            coords_from_json(&json!({"latitude": 91, "longitude": 0})),
            None
        );
    }

    #[test]
    fn cache_expires_and_stays_bounded() {
        let mut cache = Cache::new();
        cache.insert("expired".into(), json!({"at": 0, "value": 123}));
        assert!(cached_value(&cache, "expired", 60).is_none());
        remember(&mut cache, "fresh".into(), json!(42));
        assert_eq!(cached_value(&cache, "fresh", 60), Some(&json!(42)));
        for n in 0..1100 {
            remember(&mut cache, n.to_string(), Value::Null);
        }
        assert_eq!(cache.len(), 1024);
    }
}
