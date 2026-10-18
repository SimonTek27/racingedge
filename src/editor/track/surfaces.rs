//! `<track>/data/surfaces.ini` — griglia di superfici della pista.
//!
//! Port di ksengine `adapters/assetto_corsa/AcSurfacesLoader`. Ogni sezione
//! `[SURFACE_n]` describes a surface; the section index is the index with
//! cui la mesh della pista la referenzia.
//!
//! Chiavi lette:
//!
//! | chiave | default | significato |
//! | --- | --- | --- |
//! | `KEY` | nome sezione | nome logico (`ROAD`, `GRASS`, `KERB`, …) |
//! | `FRICTION` | [`DEFAULT_FRICTION`] | grip di riferimento |
//! | `DAMPING` | [`DEFAULT_DAMPING`] | smorzamento verticale |
//! | `DIRT_ADDITIVE` | [`DEFAULT_DIRT_ADDITIVE`] | sporco accumulato dalle gomme |
//!
//! Tutte le altre chiavi sono conservate in [`SurfaceDef::extras`].

use std::fs;
use std::path::Path;

use crate::editor::ini::Ini;

/// Grip di default di AC per una superficie non dichiarata.
pub const DEFAULT_FRICTION: f32 = 0.94;
/// Smorzamento verticale di default.
pub const DEFAULT_DAMPING: f32 = 0.0;
/// Sporco di default.
pub const DEFAULT_DIRT_ADDITIVE: f32 = 0.0;

/// Chiave `KEY` della superficie stradale (fallback `ASPHALT`).
pub const ROAD_KEY: &str = "ROAD";
/// Chiave alternativa per l'asfalto.
pub const ASPHALT_KEY: &str = "ASPHALT";

/// Una superficie dichiarata da `surfaces.ini`.
#[derive(Clone, Debug, PartialEq)]
pub struct SurfaceDef {
    /// Logical surface name (`KEY`, or the section name).
    pub key: String,
    /// Grip di riferimento.
    pub friction: f32,
    /// Smorzamento verticale.
    pub damping: f32,
    /// Sporco che la superficie deposita sulle gomme.
    pub dirt_additive: f32,
    /// Chiavi non interpretate, in ordine di file.
    pub extras: Vec<(String, String)>,
}

impl SurfaceDef {
    /// Superficie con i default di AC e la chiave indicata.
    pub fn new(key: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            friction: DEFAULT_FRICTION,
            damping: DEFAULT_DAMPING,
            dirt_additive: DEFAULT_DIRT_ADDITIVE,
            extras: Vec::new(),
        }
    }

    /// Grip che la superficie offre alle gomme.
    pub fn grip(&self) -> f32 {
        self.friction
    }

    /// `true` per la superficie stradale (`ROAD` o `ASPHALT`).
    pub fn is_road(&self) -> bool {
        self.key.eq_ignore_ascii_case(ROAD_KEY) || self.key.eq_ignore_ascii_case(ASPHALT_KEY)
    }
}

/// Legge e interpreta un `surfaces.ini`.
pub fn load(path: impl AsRef<Path>) -> Result<Vec<SurfaceDef>, String> {
    let path = path.as_ref();
    let text = fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(parse(&text))
}

/// Interpreta il contenuto di un `surfaces.ini`.
///
/// L'ordine di ritorno è quello del file, così l'indice della superficie
/// coincide con l'indice della sezione `[SURFACE_n]`.
pub fn parse(text: &str) -> Vec<SurfaceDef> {
    Ini::parse(text)
        .sections()
        .iter()
        .filter(|s| !s.name().starts_with('_'))
        .map(|section| {
            let mut def = SurfaceDef::new(
                section
                    .get("KEY")
                    .map(str::to_string)
                    .unwrap_or_else(|| section.name().to_string()),
            );
            def.friction = section.get_f32("FRICTION").unwrap_or(DEFAULT_FRICTION);
            def.damping = section.get_f32("DAMPING").unwrap_or(DEFAULT_DAMPING);
            def.dirt_additive = section
                .get_f32("DIRT_ADDITIVE")
                .unwrap_or(DEFAULT_DIRT_ADDITIVE);
            def.extras = section
                .iter()
                .filter(|(k, _)| !matches!(*k, "KEY" | "FRICTION" | "DAMPING" | "DIRT_ADDITIVE"))
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect();
            def
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "[SURFACE_0]\n\
        KEY=ROAD\n\
        FRICTION=0.94\n\
        DAMPING=0.0\n\
        DIRT_ADDITIVE=0.0\n\
        IS_VALID_TRACK=1\n\
        [SURFACE_1]\n\
        KEY=GRASS\n\
        FRICTION=0,45\n\
        DIRT_ADDITIVE=0.6\n\
        [SURFACE_2]\n\
        FRICTION=1.02\n";

    #[test]
    fn parses_in_file_order() {
        let s = parse(SAMPLE);
        assert_eq!(s.len(), 3);
        assert_eq!(s[0].key, "ROAD");
        assert_eq!(s[1].key, "GRASS");
        assert_eq!(s[2].key, "SURFACE_2", "senza KEY usa il nome sezione");
    }

    #[test]
    fn reads_known_keys_and_defaults() {
        let s = parse(SAMPLE);
        assert_eq!(s[0].grip(), 0.94);
        assert_eq!(s[1].friction, 0.45, "virgola decimale");
        assert_eq!(s[1].dirt_additive, 0.6);
        assert_eq!(s[1].damping, DEFAULT_DAMPING, "chiave assente → default");
    }

    #[test]
    fn keeps_unknown_keys_as_extras() {
        let s = parse(SAMPLE);
        assert_eq!(
            s[0].extras,
            vec![("IS_VALID_TRACK".to_string(), "1".to_string())]
        );
        assert!(s[1].extras.is_empty());
    }

    #[test]
    fn detects_the_road_surface() {
        let s = parse(SAMPLE);
        assert!(s[0].is_road());
        assert!(!s[1].is_road());
        assert!(SurfaceDef::new("asphalt").is_road());
    }

    #[test]
    fn empty_input_yields_no_surfaces() {
        assert!(parse("").is_empty());
        assert!(parse("; solo commenti\n").is_empty());
    }
}
