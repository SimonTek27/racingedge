//! Track data editor.
//!
//! Port of ksengine's track data/surfaces loaders (see
//! `docs/KSENGINE_PORT_BACKLOG.md` for the source map): reads a track's `data/`
//! folder and exposes its metadata and the surface grid used by the physics.
//!
//! * [`data`] — `data.json`: name, spline length, width.
//! * [`surfaces`] — `surfaces.ini`: grip, damping and dirt per surface.
//!
//! Both loaders are **tolerant**: a missing file is not an error and every
//! missing key falls back to the format's defaults. [`Track::discover`]
//! returns `None` only when there is neither `data.json` nor
//! `surfaces.ini` in the folder.
//!
//! ```ignore
//! let track = Track::discover("content/tracks/ks_mugello").unwrap();
//! println!("{} — {:.0} m", track.data.name, track.data.spline_length);
//! for s in &track.surfaces { println!("{} grip={}", s.key, s.grip()); }
//! ```

use std::path::{Path, PathBuf};

pub mod data;
pub mod surfaces;

pub use data::TrackData;
pub use surfaces::{
    SurfaceDef, ASPHALT_KEY, DEFAULT_DAMPING, DEFAULT_DIRT_ADDITIVE, DEFAULT_FRICTION, ROAD_KEY,
};

/// Una pista letta dalla sua cartella di contenuto.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Track {
    /// Cartella `data/` da cui sono stati letti i file.
    pub data_dir: PathBuf,
    /// Metadati da `data.json`.
    pub data: TrackData,
    /// Superfici da `surfaces.ini`, in ordine di file.
    pub surfaces: Vec<SurfaceDef>,
}

impl Track {
    /// Cartella `data/` di una pista: `<track>/data` se esiste, altrimenti la
    /// cartella indicata (che potrebbe *essere* `data/`).
    pub fn resolve_data_dir(track_dir: impl AsRef<Path>) -> PathBuf {
        let dir = track_dir.as_ref();
        let nested = dir.join("data");
        if nested.is_dir() {
            nested
        } else {
            dir.to_path_buf()
        }
    }

    /// Carica una pista da `<track>/data/{data.json,surfaces.ini}`.
    ///
    /// Returns `None` if there is nothing to read; a single present file
    /// is sufficient and the other remains at defaults.
    pub fn discover(track_dir: impl AsRef<Path>) -> Option<Self> {
        let track_dir = track_dir.as_ref();
        let data_dir = Self::resolve_data_dir(track_dir);

        let folder_name = track_dir
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let fallback = TrackData {
            name: folder_name,
            ..Default::default()
        };

        let json_path = data_dir.join("data.json");
        let surfaces_path = data_dir.join("surfaces.ini");

        let mut data = if json_path.is_file() {
            TrackData::load(&json_path).unwrap_or_default()
        } else {
            TrackData::default()
        };
        data.fill_from(&fallback);

        let surfaces = if surfaces_path.is_file() {
            surfaces::load(&surfaces_path).unwrap_or_default()
        } else {
            Vec::new()
        };

        if !json_path.is_file() && !surfaces_path.is_file() {
            return None;
        }

        Some(Self {
            data_dir,
            data,
            surfaces,
        })
    }

    /// Superficie con la chiave indicata (confronto case-insensitive).
    pub fn surface(&self, key: &str) -> Option<&SurfaceDef> {
        self.surfaces
            .iter()
            .find(|s| s.key.eq_ignore_ascii_case(key))
    }

    /// Superficie stradale (`ROAD`, altrimenti `ASPHALT`).
    pub fn base_surface(&self) -> Option<&SurfaceDef> {
        self.surface(ROAD_KEY).or_else(|| self.surface(ASPHALT_KEY))
    }

    /// Grip to use when the indicated surface is not declared.
    pub fn grip_of(&self, key: &str) -> f32 {
        self.surface(key)
            .or_else(|| self.base_surface())
            .map(SurfaceDef::grip)
            .unwrap_or(DEFAULT_FRICTION)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn temp_track_dir(tag: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("racingedge-track-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("data")).expect("crea cartella temporanea");
        dir
    }

    #[test]
    fn discovers_both_files() {
        let dir = temp_track_dir("both");
        fs::write(dir.join("data/data.json"), r#"{"name":"T","length":1000}"#).unwrap();
        fs::write(
            dir.join("data/surfaces.ini"),
            "[SURFACE_0]\nKEY=ROAD\nFRICTION=0.9\n[SURFACE_1]\nKEY=GRASS\n",
        )
        .unwrap();

        let track = Track::discover(&dir).expect("pista trovata");
        assert_eq!(track.data.name, "T");
        assert_eq!(track.data.spline_length, 1000.0);
        assert_eq!(track.surfaces.len(), 2);
        assert_eq!(
            track.grip_of("GRASS"),
            DEFAULT_FRICTION,
            "GRASS non sovrascrive il grip"
        );
        assert_eq!(track.grip_of("ROAD"), 0.9);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn accepts_a_track_with_only_surfaces() {
        let dir = temp_track_dir("surfaces");
        fs::write(
            dir.join("data/surfaces.ini"),
            "[SURFACE_0]\nKEY=ASPHALT\nFRICTION=1.0\n",
        )
        .unwrap();

        let track = Track::discover(&dir).expect("pista trovata");
        assert!(
            !track.data.name.is_empty(),
            "il nome viene dedotto dalla cartella quando manca data.json"
        );
        assert_eq!(track.base_surface().map(SurfaceDef::grip), Some(1.0));
        assert_eq!(track.grip_of("KERB"), 1.0, "fallback sull'asfalto");

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn returns_none_for_a_folder_without_data() {
        let dir =
            std::env::temp_dir().join(format!("racingedge-track-empty-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();

        assert!(Track::discover(&dir).is_none());

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn also_accepts_a_data_directory_directly() {
        let dir = temp_track_dir("direct");
        fs::write(dir.join("data/data.json"), r#"{"name":"Direct"}"#).unwrap();

        let track = Track::discover(dir.join("data")).expect("pista trovata");
        assert_eq!(track.data.name, "Direct");
        assert_eq!(track.data_dir, dir.join("data"));

        let _ = fs::remove_dir_all(&dir);
    }
}
