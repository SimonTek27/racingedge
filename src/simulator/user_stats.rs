//! Driver career stats — port of ksengine `src/simulator/UserStats.h`
//! (GPL-3.0). Original: <https://github.com/SimonTek27/ksengine>
//!
//! The persisted half of a driver profile: wins, poles, podiums, races
//! started, the personal-best lap and how many personal-best records the
//! [`PersonalBestStore`](super::personal_best::PersonalBestStore) holds. Stored
//! as `user/<player>/stats.json`, which starts as `{}` — no key is carried
//! until there is a value for it — so the per-player folder stays readable.
//!
//! The reference merges into an existing snapshot: keys missing from the file
//! keep the value already in `out`, so a `{}` or partially written file is a
//! partial merge rather than a wipe. [`DriverStats::load_into`] reproduces that.
//!
//! The two profile helpers in the header (`driverStatsFromProfile`,
//! `applyDriverStats`) operate on `DriverProfile`, which is menu state that
//! RacingEdge does not have yet; they land with the D11 menus.

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

/// Career statistics outliving a single run (port of `DriverStats`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct DriverStats {
    /// Races won.
    pub wins: i32,
    /// Pole positions.
    pub poles: i32,
    /// Podium finishes.
    pub podiums: i32,
    /// Races started.
    #[serde(rename = "totalRaces")]
    pub total_races: i32,
    /// Personal-best lap in seconds; `0` when none has been set.
    #[serde(rename = "bestLapTime")]
    pub best_lap_time: f32,
    /// Personal-best records held (`user/pb`).
    #[serde(rename = "pbRecords")]
    pub pb_records: i32,
}

impl DriverStats {
    /// Reads `stats.json`, starting from default values.
    ///
    /// Returns `None` when the file is missing or malformed.
    ///
    /// # Example
    ///
    /// ```
    /// use racingedge::simulator::user_stats::DriverStats;
    /// let dir = std::env::temp_dir().join(format!("racingedge-stats-doc-{}", std::process::id()));
    /// let path = dir.join("stats.json");
    /// let stats = DriverStats { wins: 3, poles: 1, ..Default::default() };
    /// stats.save(&path).expect("scrive stats.json");
    /// assert_eq!(DriverStats::load(&path).unwrap().wins, 3);
    /// let _ = std::fs::remove_dir_all(&dir);
    /// ```
    pub fn load(path: impl AsRef<Path>) -> Option<Self> {
        let mut stats = DriverStats::default();
        if Self::load_into(path, &mut stats) {
            Some(stats)
        } else {
            None
        }
    }

    /// Merges `stats.json` into `out` (missing keys keep the existing value).
    ///
    /// Returns `false` and leaves `out` untouched when the file is missing, not
    /// JSON, or not a JSON object.
    pub fn load_into(path: impl AsRef<Path>, out: &mut Self) -> bool {
        let text = match fs::read_to_string(path.as_ref()) {
            Ok(text) => text,
            Err(_) => return false,
        };
        let root: serde_json::Value = match serde_json::from_str(&text) {
            Ok(value) => value,
            Err(_) => return false,
        };
        let object = match root.as_object() {
            Some(object) => object,
            None => return false,
        };

        let mut merged = *out;
        let int_at = |key: &str, fallback: i32| match object.get(key) {
            Some(value) if value.is_number() => {
                let number = value.as_f64().unwrap_or(0.0);
                if number >= 0.0 {
                    number as i32
                } else {
                    fallback
                }
            }
            _ => fallback,
        };
        merged.wins = int_at("wins", merged.wins);
        merged.poles = int_at("poles", merged.poles);
        merged.podiums = int_at("podiums", merged.podiums);
        merged.total_races = int_at("totalRaces", merged.total_races);
        merged.pb_records = int_at("pbRecords", merged.pb_records);
        if let Some(number) = object.get("bestLapTime").and_then(|v| v.as_f64()) {
            if (0.0..1.0e6).contains(&number) {
                merged.best_lap_time = number as f32;
            }
        }

        *out = merged;
        true
    }

    /// Writes `stats.json`, creating the parent folder when needed.
    pub fn save(&self, path: impl AsRef<Path>) -> Result<(), String> {
        let path = path.as_ref();
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
            }
        }
        let text = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        fs::write(path, text).map_err(|e| format!("{}: {e}", path.display()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_path(tag: &str) -> std::path::PathBuf {
        std::env::temp_dir()
            .join(format!("racingedge-stats-{tag}-{}", std::process::id()))
            .join("stats.json")
    }

    #[test]
    fn save_then_load_round_trips() {
        let path = temp_path("roundtrip");
        let stats = DriverStats {
            wins: 5,
            poles: 2,
            podiums: 9,
            total_races: 20,
            best_lap_time: 81.25,
            pb_records: 4,
        };
        stats.save(&path).expect("salva");
        let loaded = DriverStats::load(&path).expect("carica");
        assert_eq!(loaded, stats);
        let _ = fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn json_uses_camel_case_keys() {
        let path = temp_path("keys");
        let stats = DriverStats {
            total_races: 7,
            best_lap_time: 90.0,
            pb_records: 3,
            ..Default::default()
        };
        stats.save(&path).expect("salva");
        let text = fs::read_to_string(&path).unwrap();
        assert!(text.contains("\"totalRaces\": 7"), "{text}");
        assert!(text.contains("\"bestLapTime\": 90.0"), "{text}");
        assert!(text.contains("\"pbRecords\": 3"), "{text}");
        let _ = fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn empty_object_is_a_partial_merge() {
        let path = temp_path("merge");
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, "{}").unwrap();
        let mut stats = DriverStats {
            wins: 3,
            best_lap_time: 80.0,
            ..Default::default()
        };
        assert!(DriverStats::load_into(&path, &mut stats));
        assert_eq!(stats.wins, 3);
        assert_eq!(stats.best_lap_time, 80.0);
        let _ = fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn present_keys_override_and_malformed_is_rejected() {
        let path = temp_path("override");
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, "{\"wins\": 12}").unwrap();
        let mut stats = DriverStats {
            wins: 3,
            poles: 4,
            ..Default::default()
        };
        assert!(DriverStats::load_into(&path, &mut stats));
        assert_eq!(stats.wins, 12);
        assert_eq!(stats.poles, 4);

        fs::write(&path, "{ not json").unwrap();
        let mut untouched = DriverStats {
            wins: 1,
            ..Default::default()
        };
        assert!(!DriverStats::load_into(&path, &mut untouched));
        assert_eq!(untouched.wins, 1);
        let _ = fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn missing_file_returns_none() {
        assert_eq!(DriverStats::load(temp_path("missing")), None);
    }

    #[test]
    fn negative_numbers_are_ignored() {
        let path = temp_path("negative");
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, "{\"wins\": -5, \"bestLapTime\": -1.0}").unwrap();
        let mut stats = DriverStats {
            wins: 2,
            best_lap_time: 79.0,
            ..Default::default()
        };
        assert!(DriverStats::load_into(&path, &mut stats));
        assert_eq!(stats.wins, 2);
        assert_eq!(stats.best_lap_time, 79.0);
        let _ = fs::remove_dir_all(path.parent().unwrap());
    }
}
