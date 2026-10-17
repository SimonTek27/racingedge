//! Personal-best store — port of ksengine `src/simulator/PersonalBestStore.h`
//! (GPL-3.0). Original: <https://github.com/SimonTek27/ksengine>
//!
//! One `.pb` file per track/car pair holds the best lap, the three best
//! sectors, the completed-lap count and the last driver. Files are plain
//! `Key=Value` lines (`BestLap`, `S0`..`S2`, `Laps`, `Driver`), so a per-player
//! folder stays hand-readable. Track and car ids are sanitized before they
//! reach the filesystem, and unsafe directories (`..`, embedded NUL, empty,
//! longer than 512 bytes) are rejected.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

/// One track/car personal-best record.
#[derive(Clone, Debug, PartialEq)]
pub struct PersonalBestRecord {
    /// Sanitized track id.
    pub track_id: String,
    /// Sanitized car id.
    pub car_id: String,
    /// Driver that last improved the record.
    pub driver: String,
    /// Best lap time in seconds (`1e9` when none has been set).
    pub best_lap: f32,
    /// Best cumulative sector times in seconds (`1e9` when unset).
    pub sectors: [f32; 3],
    /// Number of laps submitted for this pair.
    pub laps_completed: i32,
}

impl Default for PersonalBestRecord {
    fn default() -> Self {
        Self {
            track_id: String::new(),
            car_id: String::new(),
            driver: String::new(),
            best_lap: 1e9,
            sectors: [1e9, 1e9, 1e9],
            laps_completed: 0,
        }
    }
}

/// File-backed per-track/car personal bests.
///
/// # Example
///
/// ```
/// use racingedge::simulator::personal_best::PersonalBestStore;
/// let dir = std::env::temp_dir().join(format!("racingedge-pb-doc-{}", std::process::id()));
/// let mut store = PersonalBestStore::new();
/// assert!(store.set_directory(&dir));
/// assert!(store.submit_lap("monza", "ks_ferrari", "simon", 83.5, None));
/// assert_eq!(store.get("monza", "ks_ferrari").best_lap, 83.5);
/// let _ = std::fs::remove_dir_all(&dir);
/// ```
#[derive(Clone, Debug, Default)]
pub struct PersonalBestStore {
    dir: PathBuf,
    cache: BTreeMap<String, PersonalBestRecord>,
}

impl PersonalBestStore {
    /// An empty store with no directory.
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the storage directory and loads every existing `.pb` file.
    ///
    /// Returns `false` (and leaves the store untouched) when the directory is
    /// missing, unsafe, or cannot be created.
    pub fn set_directory(&mut self, dir: impl AsRef<Path>) -> bool {
        let dir = dir.as_ref();
        if !is_safe_dir(&dir.to_string_lossy()) {
            return false;
        }
        if fs::create_dir_all(dir).is_err() {
            return false;
        }
        self.dir = dir.to_path_buf();
        self.load_all();
        true
    }

    /// The current storage directory.
    pub fn directory(&self) -> &Path {
        &self.dir
    }

    /// The cache key for a track/car pair (`<track>__<car>`, both sanitized).
    pub fn key(track: &str, car: &str) -> String {
        format!("{}__{}", sanitize_id(track), sanitize_id(car))
    }

    /// The record for a track/car pair, or a default record carrying the raw
    /// ids when nothing has been stored yet.
    pub fn get(&self, track: &str, car: &str) -> PersonalBestRecord {
        self.cache
            .get(&Self::key(track, car))
            .cloned()
            .unwrap_or(PersonalBestRecord {
                track_id: track.to_string(),
                car_id: car.to_string(),
                ..Default::default()
            })
    }

    /// Submits a completed lap. Returns `true` when it improved the record —
    /// either the lap or one of the (optional) sector times.
    ///
    /// Rejects non-finite laps outside `(1.0, 600.0]` and empty ids.
    pub fn submit_lap(
        &mut self,
        track: &str,
        car: &str,
        driver: &str,
        lap_time: f32,
        sectors: Option<[f32; 3]>,
    ) -> bool {
        if !lap_time.is_finite() || lap_time <= 1.0 || lap_time > 600.0 {
            return false;
        }
        if track.is_empty() || car.is_empty() {
            return false;
        }

        let record = self.cache.entry(Self::key(track, car)).or_default();
        record.track_id = sanitize_id(track);
        record.car_id = sanitize_id(car);
        if !driver.is_empty() {
            record.driver = driver.to_string();
        }
        record.laps_completed += 1;

        let mut improved = false;
        if lap_time < record.best_lap {
            record.best_lap = lap_time;
            improved = true;
        }
        if let Some(sectors) = sectors {
            for (slot, value) in record.sectors.iter_mut().zip(sectors) {
                if value > 0.0 && value.is_finite() && value < *slot {
                    *slot = value;
                    improved = true;
                }
            }
        }

        if improved {
            let snapshot = record.clone();
            self.save_one(&snapshot);
        }
        improved
    }

    /// Loads a single track/car record from disk. Returns `false` when the file
    /// is missing or unreadable.
    pub fn load_one(&mut self, track: &str, car: &str) -> bool {
        let text = match fs::read_to_string(self.file_path(track, car)) {
            Ok(text) => text,
            Err(_) => return false,
        };
        let mut record = PersonalBestRecord {
            track_id: track.to_string(),
            car_id: car.to_string(),
            ..Default::default()
        };
        for line in text.lines() {
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            match key.trim() {
                "BestLap" => {
                    if let Ok(v) = value.trim().parse::<f32>() {
                        record.best_lap = v;
                    }
                }
                "S0" => {
                    if let Ok(v) = value.trim().parse::<f32>() {
                        record.sectors[0] = v;
                    }
                }
                "S1" => {
                    if let Ok(v) = value.trim().parse::<f32>() {
                        record.sectors[1] = v;
                    }
                }
                "S2" => {
                    if let Ok(v) = value.trim().parse::<f32>() {
                        record.sectors[2] = v;
                    }
                }
                "Laps" => {
                    if let Ok(v) = value.trim().parse::<i32>() {
                        record.laps_completed = v;
                    }
                }
                "Driver" => record.driver = value.to_string(),
                _ => {}
            }
        }
        self.cache.insert(Self::key(track, car), record);
        true
    }

    /// Loads every `.pb` file in the storage directory.
    pub fn load_all(&mut self) {
        if self.dir.as_os_str().is_empty() {
            return;
        }
        let entries = match fs::read_dir(&self.dir) {
            Ok(entries) => entries,
            Err(_) => return,
        };
        for entry in entries.flatten() {
            if !entry.file_type().map(|t| t.is_file()).unwrap_or(false) {
                continue;
            }
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.len() < 5 || !name.ends_with(".pb") {
                continue;
            }
            let stem = &name[..name.len() - 3];
            let Some((track, car)) = stem.split_once("__") else {
                continue;
            };
            self.load_one(track, car);
        }
    }

    /// Records for `track`, sorted by best lap and truncated to `max_n`.
    pub fn leaderboard(&self, track: &str, max_n: usize) -> Vec<PersonalBestRecord> {
        let wanted = sanitize_id(track);
        let mut out: Vec<PersonalBestRecord> = self
            .cache
            .values()
            .filter(|r| r.track_id == wanted && r.best_lap < 1e8)
            .cloned()
            .collect();
        out.sort_by(|a, b| {
            a.best_lap
                .partial_cmp(&b.best_lap)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        out.truncate(max_n);
        out
    }

    /// Number of cached records.
    pub fn size(&self) -> usize {
        self.cache.len()
    }

    fn file_path(&self, track: &str, car: &str) -> PathBuf {
        self.dir.join(format!("{}.pb", Self::key(track, car)))
    }

    fn save_one(&self, record: &PersonalBestRecord) {
        if self.dir.as_os_str().is_empty() {
            return;
        }
        let path = self.file_path(&record.track_id, &record.car_id);
        let body = format!(
            "BestLap={}\nS0={}\nS1={}\nS2={}\nLaps={}\nDriver={}\n",
            record.best_lap,
            record.sectors[0],
            record.sectors[1],
            record.sectors[2],
            record.laps_completed,
            record.driver,
        );
        let _ = fs::write(path, body);
    }
}

/// Keeps alphanumerics, `_`, `-` and `.`; maps space and `/` to `_`; truncates
/// to 64 bytes; falls back to `"unknown"` when nothing is left.
pub fn sanitize_id(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        if c.is_alphanumeric() || c == '_' || c == '-' || c == '.' {
            out.push(c);
        } else if c == ' ' || c == '/' {
            out.push('_');
        }
    }
    if out.len() > 64 {
        out.truncate(64);
    }
    if out.is_empty() {
        "unknown".to_string()
    } else {
        out
    }
}

/// `true` when `dir` is non-empty, at most 512 bytes, and free of `..` and NUL.
fn is_safe_dir(dir: &str) -> bool {
    if dir.is_empty() || dir.len() > 512 {
        return false;
    }
    if dir.contains("..") || dir.contains('\0') {
        return false;
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(tag: &str) -> PathBuf {
        std::env::temp_dir().join(format!("racingedge-pb-{tag}-{}", std::process::id()))
    }

    #[test]
    fn sanitize_id_matches_reference() {
        assert_eq!(sanitize_id("ks_ferrari 499p"), "ks_ferrari_499p");
        assert_eq!(sanitize_id("a/b"), "a_b");
        assert_eq!(sanitize_id("???"), "unknown");
        assert_eq!(sanitize_id(&"x".repeat(100)).len(), 64);
    }

    #[test]
    fn default_record_is_sentinel() {
        let store = PersonalBestStore::new();
        let record = store.get("monza", "car");
        assert_eq!(record.best_lap, 1e9);
        assert_eq!(record.track_id, "monza");
        assert!(store.directory().as_os_str().is_empty());
    }

    #[test]
    fn submit_rejects_out_of_range_laps() {
        let dir = temp_dir("range");
        let mut store = PersonalBestStore::new();
        assert!(store.set_directory(&dir));
        assert!(!store.submit_lap("t", "c", "d", 0.5, None));
        assert!(!store.submit_lap("t", "c", "d", 700.0, None));
        assert!(!store.submit_lap("t", "c", "d", f32::NAN, None));
        assert!(!store.submit_lap("", "c", "d", 80.0, None));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn submit_only_reports_improvements() {
        let dir = temp_dir("improve");
        let mut store = PersonalBestStore::new();
        assert!(store.set_directory(&dir));
        assert!(store.submit_lap("monza", "car", "simon", 83.0, None));
        assert!(!store.submit_lap("monza", "car", "simon", 84.0, None));
        assert!(store.submit_lap("monza", "car", "simon", 82.0, None));
        assert_eq!(store.get("monza", "car").best_lap, 82.0);
        assert_eq!(store.get("monza", "car").laps_completed, 3);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn sector_improvement_is_reported() {
        let dir = temp_dir("sectors");
        let mut store = PersonalBestStore::new();
        assert!(store.set_directory(&dir));
        store.submit_lap("t", "c", "d", 80.0, Some([30.0, 25.0, 25.0]));
        assert!(store.submit_lap("t", "c", "d", 85.0, Some([29.0, 25.0, 31.0])));
        let record = store.get("t", "c");
        assert_eq!(record.best_lap, 80.0);
        assert_eq!(record.sectors[0], 29.0);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn round_trips_through_disk() {
        let dir = temp_dir("roundtrip");
        let mut store = PersonalBestStore::new();
        assert!(store.set_directory(&dir));
        store.submit_lap("monza", "car", "simon", 82.5, Some([28.0, 27.0, 27.5]));

        let mut reloaded = PersonalBestStore::new();
        assert!(reloaded.set_directory(&dir));
        let record = reloaded.get("monza", "car");
        assert_eq!(record.best_lap, 82.5);
        assert_eq!(record.sectors, [28.0, 27.0, 27.5]);
        assert_eq!(record.driver, "simon");
        assert_eq!(record.laps_completed, 1);
        assert_eq!(reloaded.size(), 1);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn leaderboard_sorts_and_truncates() {
        let dir = temp_dir("leaderboard");
        let mut store = PersonalBestStore::new();
        assert!(store.set_directory(&dir));
        store.submit_lap("monza", "car_a", "a", 85.0, None);
        store.submit_lap("monza", "car_b", "b", 82.0, None);
        store.submit_lap("monza", "car_c", "c", 84.0, None);
        store.submit_lap("spa", "car_a", "a", 100.0, None);

        let board = store.leaderboard("monza", 2);
        assert_eq!(board.len(), 2);
        assert_eq!(board[0].car_id, "car_b");
        assert_eq!(board[1].car_id, "car_c");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn unsafe_directory_is_rejected() {
        let mut store = PersonalBestStore::new();
        assert!(!store.set_directory("../escape"));
        assert!(!store.set_directory(""));
        assert!(!store.set_directory("a\0b"));
        assert!(!store.set_directory("x".repeat(600)));
    }

    #[test]
    fn load_one_missing_file_is_false() {
        let dir = temp_dir("missing");
        let mut store = PersonalBestStore::new();
        assert!(store.set_directory(&dir));
        assert!(!store.load_one("nope", "nope"));
        let _ = fs::remove_dir_all(&dir);
    }
}
