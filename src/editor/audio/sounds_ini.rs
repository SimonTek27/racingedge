//! `sounds.ini` — mappa eventi audio di una vettura o di un contenuto.
//!
//! Port di ksengine `src/simulator/SoundsIniParser.*`. Il file associa ogni
//! sezione a un evento sonoro del bank; i campi letti sono:
//!
//! | chiave | campo | note |
//! | --- | --- | --- |
//! | `GUID` | [`SoundEvent::guid`] | identificatore dell'evento nel bank |
//! | `EVENT` | [`SoundEvent::event`] | nome evento (`event:/…`) |
//! | `SOURCE` | [`SoundEvent::source`] | file/campione sorgente |
//! | `SURFACE` | [`SoundEvent::surface`] | superficie a cui l'evento è legato |
//! | `VOLUME` | [`SoundEvent::volume`] | default [`DEFAULT_VOLUME`] |
//! | `MIN_RPM` / `MAX_RPM` | [`SoundEvent::rpm_range`] | fascia di crossfade |
//! | `LOOP` | [`SoundEvent::looped`] | default `false` |
//!
//! Tutte le altre chiavi restano disponibili in [`SoundEvent::extras`], così il
//! mixer può leggerle senza che questo parser debba conoscerle.

use std::fs;
use std::path::Path;

use crate::editor::ini::Ini;

/// Volume applicato quando la sezione non lo dichiara.
pub const DEFAULT_VOLUME: f32 = 1.0;

/// Un evento dichiarato da `sounds.ini`.
#[derive(Clone, Debug, PartialEq)]
pub struct SoundEvent {
    /// Nome della sezione (`ENGINE_EXT`, `GEAR`, `SKID`, …).
    pub name: String,
    /// `GUID` dell'evento nel bank, se dichiarato.
    pub guid: Option<String>,
    /// Nome dell'evento (`EVENT`).
    pub event: Option<String>,
    /// Campione o file sorgente (`SOURCE`).
    pub source: Option<String>,
    /// Superficie a cui l'evento è legato (`SURFACE`), se presente.
    pub surface: Option<String>,
    /// Volume dell'evento.
    pub volume: f32,
    /// Estremo inferiore della fascia RPM (`MIN_RPM`).
    pub min_rpm: Option<f32>,
    /// Estremo superiore della fascia RPM (`MAX_RPM`).
    pub max_rpm: Option<f32>,
    /// `true` quando l'evento è in loop (`LOOP`).
    pub looped: bool,
    /// Chiavi non interpretate, in ordine di file.
    pub extras: Vec<(String, String)>,
}

impl SoundEvent {
    /// Evento con i default e il nome indicato.
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

    /// Fascia RPM dichiarata, se entrambi gli estremi sono presenti.
    pub fn rpm_range(&self) -> Option<(f32, f32)> {
        match (self.min_rpm, self.max_rpm) {
            (Some(min), Some(max)) if max > min => Some((min, max)),
            _ => None,
        }
    }

    /// Nome da usare per risolvere l'evento: `EVENT`, poi `GUID`, poi `SOURCE`.
    pub fn resolve_target(&self) -> Option<&str> {
        self.event
            .as_deref()
            .or(self.guid.as_deref())
            .or(self.source.as_deref())
    }
}

/// Un `sounds.ini` interpretato.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SoundsIni {
    /// Eventi in ordine di file.
    pub events: Vec<SoundEvent>,
}

impl SoundsIni {
    /// Legge e interpreta un `sounds.ini`.
    pub fn load(path: impl AsRef<Path>) -> Result<Self, String> {
        let path = path.as_ref();
        let text = fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
        Ok(Self::parse(&text))
    }

    /// Interpreta il contenuto di un `sounds.ini`.
    pub fn parse(text: &str) -> Self {
        let ini = Ini::parse(text);
        Self {
            events: ini
                .sections()
                .iter()
                .filter(|s| !s.name().starts_with('_'))
                .map(|section| {
                    let mut ev = SoundEvent::new(section.name());
                    ev.guid = section.get("GUID").map(str::to_string);
                    ev.event = section.get("EVENT").map(str::to_string);
                    ev.source = section.get("SOURCE").map(str::to_string);
                    ev.surface = section.get("SURFACE").map(str::to_string);
                    ev.volume = section.get_f32("VOLUME").unwrap_or(DEFAULT_VOLUME);
                    ev.min_rpm = section.get_f32("MIN_RPM");
                    ev.max_rpm = section.get_f32("MAX_RPM");
                    ev.looped = section.get_bool("LOOP").unwrap_or(false);
                    ev.extras = section
                        .iter()
                        .filter(|(k, _)| {
                            !matches!(
                                *k,
                                "GUID"
                                    | "EVENT"
                                    | "SOURCE"
                                    | "SURFACE"
                                    | "VOLUME"
                                    | "MIN_RPM"
                                    | "MAX_RPM"
                                    | "LOOP"
                            )
                        })
                        .map(|(k, v)| (k.to_string(), v.to_string()))
                        .collect();
                    ev
                })
                .collect(),
        }
    }

    /// Evento con il nome indicato (case-insensitive).
    pub fn event(&self, name: &str) -> Option<&SoundEvent> {
        self.events
            .iter()
            .find(|e| e.name.eq_ignore_ascii_case(name))
    }

    /// Eventi legati a una superficie (case-insensitive).
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

    /// Eventi con una fascia RPM dichiarata, **ordinati per `MIN_RPM`** — l'ordine
    /// con cui il mixer deve fare crossfade.
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

    /// `true` quando il file non dichiara eventi.
    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "\
[ENGINE_EXT]\n\
GUID={2a1f9c2e-0000-4000-8000-000000000001}\n\
EVENT=event:/cars/gt3/engine_ext\n\
VOLUME=0.9\n\
MIN_RPM=800\n\
MAX_RPM=8200\n\
LOOP=1\n\
\n\
[ENGINE_INT]\n\
GUID={2a1f9c2e-0000-4000-8000-000000000002}\n\
MIN_RPM=800\n\
MAX_RPM=7000\n\
\n\
[SKID_GRASS]\n\
SURFACE=GRASS\n\
SOURCE=sfx/grass_skid.wav\n\
VOLUME=0,6\n\
\n\
[GEAR]\n\
GUID={2a1f9c2e-0000-4000-8000-000000000003}\n";

    #[test]
    fn parses_events_in_file_order() {
        let ini = SoundsIni::parse(SAMPLE);
        assert_eq!(ini.events.len(), 4);
        assert_eq!(ini.events[0].name, "ENGINE_EXT");
        assert_eq!(ini.events[3].name, "GEAR");
    }

    #[test]
    fn reads_guid_event_volume_and_loop() {
        let ini = SoundsIni::parse(SAMPLE);
        let ext = ini.event("engine_ext").expect("engine_ext");
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
        let ini = SoundsIni::parse(SAMPLE);
        let gear = ini.event("GEAR").expect("gear");
        assert_eq!(gear.volume, DEFAULT_VOLUME);
        assert!(!gear.looped);
        assert_eq!(gear.rpm_range(), None, "manca MAX_RPM");
    }

    #[test]
    fn resolves_by_surface_and_accepts_comma_decimals() {
        let ini = SoundsIni::parse(SAMPLE);
        let grass = ini.for_surface("grass");
        assert_eq!(grass.len(), 1);
        assert_eq!(grass[0].name, "SKID_GRASS");
        assert_eq!(grass[0].volume, 0.6);
        assert!(ini.for_surface("KERB").is_empty());
    }

    #[test]
    fn rpm_layers_are_sorted_by_min_rpm() {
        let ini =
            SoundsIni::parse("[C]\nMIN_RPM=4000\nMAX_RPM=6000\n[A]\nMIN_RPM=800\nMAX_RPM=3000\n");
        let names: Vec<&str> = ini.rpm_layers().iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, vec!["A", "C"]);
    }

    #[test]
    fn resolve_target_prefers_event_over_guid() {
        let ini = SoundsIni::parse(SAMPLE);
        assert_eq!(
            ini.event("ENGINE_EXT").unwrap().resolve_target(),
            Some("event:/cars/gt3/engine_ext")
        );
        assert_eq!(
            ini.event("SKID_GRASS").unwrap().resolve_target(),
            Some("sfx/grass_skid.wav")
        );
    }

    #[test]
    fn extras_keep_unrecognised_keys() {
        let ini = SoundsIni::parse("[WIND]\nPITCH=1.3\nOFFSET=0.02\n");
        assert_eq!(
            ini.event("WIND").unwrap().extras,
            vec![
                ("PITCH".to_string(), "1.3".to_string()),
                ("OFFSET".to_string(), "0.02".to_string())
            ]
        );
    }
}
