//! `sounds.json` — audio event map for a car or content pack.
//!
//! Port of ksengine `src/simulator/SoundsIniParser.*`. The original reads
//! `sounds.ini` sections; here the same data is stored as a JSON array of
//! event objects:
//!
//! | key | field | note |
//! | --- | --- | --- |
//! | `name` | [`SoundEvent::name`] | event name (`ENGINE_EXT`, `GEAR`, …) |
//! | `guid` | [`SoundEvent::guid`] | identifier of the event in the bank |
//! | `event` | [`SoundEvent::event`] | event path (`event:/…`) |
//! | `source` | [`SoundEvent::source`] | source file / sample |
//! | `surface` | [`SoundEvent::surface`] | surface the event is bound to |
//! | `volume` | [`SoundEvent::volume`] | default [`DEFAULT_VOLUME`] |
//! | `min_rpm` / `max_rpm` | [`SoundEvent::rpm_range`] | crossfade range |
//! | `loop` | [`SoundEvent::looped`] | default `false` |
//!
//! All other keys remain available in [`SoundEvent::extras`], so the mixer
//! can read them without this parser needing to know about them.

use std::fs;
use std::path::Path;

use serde_json::{Map, Value};

/// Volume applied when the event does not declare it.
pub const DEFAULT_VOLUME: f32 = 1.0;

/// An event declared in `sounds.json`.
#[derive(Clone, Debug, PartialEq)]
pub struct SoundEvent {
    /// Event name (`ENGINE_EXT`, `GEAR`, `SKID`, …).
    pub name: String,
    /// `guid` of the event in the bank, if declared.
    pub guid: Option<String>,
    /// Event path (`event`).
    pub event: Option<String>,
    /// Source file or sample (`source`).
    pub source: Option<String>,
    /// Surface the event is bound to (`surface`), if present.
    pub surface: Option<String>,
    /// Event volume.
    pub volume: f32,
    /// Lower bound of the RPM range (`min_rpm`).
    pub min_rpm: Option<f32>,
    /// Upper bound of the RPM range (`max_rpm`).
    pub max_rpm: Option<f32>,
    /// `true` when the event loops (`loop`).
    pub looped: bool,
    /// Unrecognised keys, in file order.
    pub extras: Vec<(String, String)>,
}

impl SoundEvent {
    /// Event with defaults and the given name.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            guid: None,
            event: None,
            source: None,
            surface: None,
            volume: DEFAULT_VOLUME,
            min_rpm: None,
            max_rpm: None,
            looped: false,
            extras: Vec::new(),
        }
    }

    /// Declared RPM range, if both bounds are present.
    pub fn rpm_range(&self) -> Option<(f32, f32)> {
        match (self.min_rpm, self.max_rpm) {
            (Some(min), Some(max)) if max > min => Some((min, max)),
            _ => None,
        }
    }

    /// Name to use when resolving the event: `event`, then `guid`, then `source`.
    pub fn resolve_target(&self) -> Option<&str> {
        self.event
            .as_deref()
            .or(self.guid.as_deref())
            .or(self.source.as_deref())
    }
}

/// A parsed `sounds.json`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SoundsJson {
    /// Events in file order.
    pub events: Vec<SoundEvent>,
}

impl SoundsJson {
    /// Reads and parses a `sounds.json`.
    pub fn load(path: impl AsRef<Path>) -> Result<Self, String> {
        let path = path.as_ref();
        let text = fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
        Self::parse(&text)
    }

    /// Parses the contents of a `sounds.json`.
    ///
    /// Accepts either a top-level array of event objects or an object whose
    /// values are event objects keyed by name.
    pub fn parse(text: &str) -> Result<Self, String> {
        let value: Value = serde_json::from_str(text).map_err(|e| format!("invalid JSON: {e}"))?;
        let events = match value {
            Value::Array(items) => items
                .into_iter()
                .filter_map(|item| Self::event_from_value(item, None))
                .collect(),
            Value::Object(map) => map
                .into_iter()
                .filter_map(|(key, item)| Self::event_from_value(item, Some(key)))
                .collect(),
            _ => return Err("sounds.json must be an array or an object".to_string()),
        };
        Ok(Self { events })
    }

    fn event_from_value(value: Value, fallback_name: Option<String>) -> Option<SoundEvent> {
        let mut map = match value {
            Value::Object(map) => map,
            _ => return None,
        };
        let name = take_string(&mut map, "name")
            .unwrap_or_else(|| fallback_name.unwrap_or_else(|| "EVENT".to_string()));
        let mut ev = SoundEvent::new(name);
        ev.guid = take_string(&mut map, "guid");
        ev.event = take_string(&mut map, "event");
        ev.source = take_string(&mut map, "source");
        ev.surface = take_string(&mut map, "surface");
        ev.volume = take_f32(&mut map, "volume").unwrap_or(DEFAULT_VOLUME);
        ev.min_rpm = take_f32(&mut map, "min_rpm");
        ev.max_rpm = take_f32(&mut map, "max_rpm");
        ev.looped = take_bool(&mut map, "loop").unwrap_or(false);
        ev.extras = map
            .into_iter()
            .map(|(k, v)| (k, value_to_display(v)))
            .collect();
        Some(ev)
    }

    /// Event with the given name (case-insensitive).
    pub fn event(&self, name: &str) -> Option<&SoundEvent> {
        self.events
            .iter()
            .find(|e| e.name.eq_ignore_ascii_case(name))
    }

    /// Events bound to a surface (case-insensitive).
    pub fn for_surface(&self, surface: &str) -> Vec<&SoundEvent> {
        self.events
            .iter()
            .filter(|e| {
                e.surface
                    .as_deref()
                    .is_some_and(|s| s.eq_ignore_ascii_case(surface))
            })
            .collect()
    }

    /// Events with a declared RPM range, **sorted by `min_rpm`** — the order
    /// in which the mixer should crossfade.
    pub fn rpm_layers(&self) -> Vec<&SoundEvent> {
        let mut layers: Vec<&SoundEvent> = self
            .events
            .iter()
            .filter(|e| e.rpm_range().is_some())
            .collect();
        layers.sort_by(|a, b| {
            a.min_rpm
                .partial_cmp(&b.min_rpm)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        layers
    }

    /// `true` when the file declares no events.
    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }
}

fn take_string(map: &mut Map<String, Value>, key: &str) -> Option<String> {
    match map.remove(key) {
        Some(Value::String(s)) => Some(s),
        Some(Value::Number(n)) => Some(n.to_string()),
        Some(Value::Bool(b)) => Some(b.to_string()),
        Some(Value::Null) | None => None,
        Some(other) => Some(value_to_display(other)),
    }
}

fn take_f32(map: &mut Map<String, Value>, key: &str) -> Option<f32> {
    match map.remove(key) {
        Some(Value::Number(n)) => n.as_f64().map(|v| v as f32),
        Some(Value::String(s)) => crate::editor::ini::parse_f32(&s),
        _ => None,
    }
}

fn take_bool(map: &mut Map<String, Value>, key: &str) -> Option<bool> {
    match map.remove(key) {
        Some(Value::Bool(b)) => Some(b),
        Some(Value::Number(n)) => n.as_f64().map(|v| v != 0.0),
        Some(Value::String(s)) => match s.to_ascii_lowercase().as_str() {
            "1" | "true" | "yes" | "on" => Some(true),
            "0" | "false" | "no" | "off" => Some(false),
            _ => None,
        },
        _ => None,
    }
}

fn value_to_display(value: Value) -> String {
    match value {
        Value::String(s) => s,
        Value::Number(n) => n.to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Null => String::new(),
        other => other.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"[
  {
    "name": "ENGINE_EXT",
    "guid": "{2a1f9c2e-0000-4000-8000-000000000001}",
    "event": "event:/cars/gt3/engine_ext",
    "volume": 0.9,
    "min_rpm": 800,
    "max_rpm": 8200,
    "loop": true
  },
  {
    "name": "ENGINE_INT",
    "guid": "{2a1f9c2e-0000-4000-8000-000000000002}",
    "min_rpm": 800,
    "max_rpm": 7000
  },
  {
    "name": "SKID_GRASS",
    "surface": "GRASS",
    "source": "sfx/grass_skid.wav",
    "volume": 0.6
  },
  {
    "name": "GEAR",
    "guid": "{2a1f9c2e-0000-4000-8000-000000000003}"
  }
]"#;

    #[test]
    fn parses_events_in_file_order() {
        let json = SoundsJson::parse(SAMPLE).expect("valid JSON");
        assert_eq!(json.events.len(), 4);
        assert_eq!(json.events[0].name, "ENGINE_EXT");
        assert_eq!(json.events[3].name, "GEAR");
    }

    #[test]
    fn reads_guid_event_volume_and_loop() {
        let json = SoundsJson::parse(SAMPLE).expect("valid JSON");
        let ext = json.event("engine_ext").expect("engine_ext");
        assert_eq!(
            ext.guid.as_deref(),
            Some("{2a1f9c2e-0000-4000-8000-000000000001}")
        );
        assert_eq!(ext.event.as_deref(), Some("event:/cars/gt3/engine_ext"));
        assert_eq!(ext.volume, 0.9);
        assert!(ext.looped);
        assert_eq!(ext.rpm_range(), Some((800.0, 8200.0)));
    }

    #[test]
    fn defaults_are_applied_when_keys_are_missing() {
        let json = SoundsJson::parse(SAMPLE).expect("valid JSON");
        let gear = json.event("GEAR").expect("gear");
        assert_eq!(gear.volume, DEFAULT_VOLUME);
        assert!(!gear.looped);
        assert_eq!(gear.rpm_range(), None, "missing max_rpm");
    }

    #[test]
    fn resolves_by_surface() {
        let json = SoundsJson::parse(SAMPLE).expect("valid JSON");
        let grass = json.for_surface("grass");
        assert_eq!(grass.len(), 1);
        assert_eq!(grass[0].name, "SKID_GRASS");
        assert_eq!(grass[0].volume, 0.6);
        assert!(json.for_surface("KERB").is_empty());
    }

    #[test]
    fn rpm_layers_are_sorted_by_min_rpm() {
        let json = SoundsJson::parse(
            r#"[
              {"name":"C","min_rpm":4000,"max_rpm":6000},
              {"name":"A","min_rpm":800,"max_rpm":3000}
            ]"#,
        )
        .expect("valid JSON");
        let names: Vec<&str> = json.rpm_layers().iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, vec!["A", "C"]);
    }

    #[test]
    fn resolve_target_prefers_event_over_guid() {
        let json = SoundsJson::parse(SAMPLE).expect("valid JSON");
        assert_eq!(
            json.event("ENGINE_EXT").unwrap().resolve_target(),
            Some("event:/cars/gt3/engine_ext")
        );
        assert_eq!(
            json.event("SKID_GRASS").unwrap().resolve_target(),
            Some("sfx/grass_skid.wav")
        );
    }

    #[test]
    fn extras_keep_unrecognised_keys() {
        let json = SoundsJson::parse(r#"[{"name":"WIND","pitch":1.3,"offset":0.02}]"#)
            .expect("valid JSON");
        assert_eq!(
            json.event("WIND").unwrap().extras,
            vec![
                ("offset".to_string(), "0.02".to_string()),
                ("pitch".to_string(), "1.3".to_string())
            ]
        );
    }

    #[test]
    fn accepts_object_form_keyed_by_name() {
        let json = SoundsJson::parse(
            r#"{
              "GEAR": {"guid": "abc"},
              "BRAKE": {"surface": "ASPHALT"}
            }"#,
        )
        .expect("valid JSON");
        assert_eq!(json.events.len(), 2);
        assert_eq!(json.event("GEAR").unwrap().guid.as_deref(), Some("abc"));
        assert_eq!(
            json.event("BRAKE").unwrap().surface.as_deref(),
            Some("ASPHALT")
        );
    }

    #[test]
    fn rejects_invalid_json() {
        assert!(SoundsJson::parse("not json").is_err());
    }

    #[test]
    fn rejects_non_array_non_object() {
        assert!(SoundsJson::parse("42").is_err());
    }
}
