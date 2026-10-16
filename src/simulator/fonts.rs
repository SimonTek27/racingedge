//! RacingEdge bitmap-font definitions (JSON).
//!
//! Defines per-glyph metrics for cockpit digital instruments —
//! numeric digits, decimal point, minus, plus — used by
//! `src/simulator/cockpit.rs` when laying out `DigitalInstrument` values.
//!
//! The format is a single JSON file designed for this project's needs:
//! speedometer numerals, lap-time displays, gear indicator, and telemetry
//! readouts. It is **not** tied to any external game format.
//!
//! Esempio di `fonts/digital_64.json` (formato reale, non eseguibile):
//!
//! ```text
//! {
//!   "name": "Racing-Digital",
//!   "size_px": 64.0,
//!   "baseline_px": 72.0,
//!   "glyphs": {
//!     "0": {"width": 24, "x_offset": 0, "y_offset": 0},
//!     "1": {"width": 14, "x_offset": 0, "y_offset": 0},
//!     "2": {"width": 24, "x_offset": 0, "y_offset": 0},
//!     "3": {"width": 24, "x_offset": 0, "y_offset": 0},
//!     "4": {"width": 20, "x_offset": 0, "y_offset": 0},
//!     "5": {"width": 24, "x_offset": 0, "y_offset": 0},
//!     "6": {"width": 24, "x_offset": 0, "y_offset": 0},
//!     "7": {"width": 16, "x_offset": 0, "y_offset": 0},
//!     "8": {"width": 24, "x_offset": 0, "y_offset": 0},
//!     "9": {"width": 24, "x_offset": 0, "y_offset": 0},
//!     ".": {"width": 8,  "x_offset": 0, "y_offset": 0},
//!     "-": {"width": 12, "x_offset": 0, "y_offset": 0},
//!     "+": {"width": 12, "x_offset": 0, "y_offset": 0}
//!   }
//! }
//! ```
//
//! L'avanzamento di una stringa è la somma dei `width` dei caratteri;
//! i campi `x_offset`/`y_offset` sono offset facoltativi per allineamento
//! all'interno della cella (es. allineamento a destra dei numeri).
//!
//! Formato JSON parsato con `serde_json` (già disponibile tramite Bevy).
//! Le dimensioni (`width`) sono in pixel; l'avanzamento è la somma delle
//! larghezze dei caratteri.

use serde::Deserialize;
use serde_json;
use std::fs;

/// Metriche di un singolo carattere per un font racing-digit.
#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct GlyphMetrics {
    /// Larghezza del glifo in pixel (usata per l'avanzamento).
    pub width: f32,
    /// Offset opzionale x all'interno della cella (di solito 0 per fonts
    /// a passo fisso dove l'atlante accoda le celle).
    pub x_offset: f32,
    /// Offset opzionale y all'interno della cella (di solito 0).
    pub y_offset: f32,
}

/// Definizione completa di un font racing-digit.
#[derive(Clone, Debug, Deserialize)]
pub struct FontDef {
    /// Nome del font (solo descrittivo, usato per logging/errore).
    pub name: String,
    /// Corpo del font in pixel.
    pub size_px: f32,
    /// Altezza della riga di base in pixel (può differire da `size_px`
    /// per i font digitali che hanno uno spazio tra una riga e l'altra).
    pub baseline_px: f32,
    /// Mappa `carattere → metrics`. Sono supportati i caratteri che
    /// interessano il cockpit: `0-9`, `.`, `-`, `+`.
    pub glyphs: std::collections::HashMap<String, GlyphMetrics>,
}

impl FontDef {
    /// Carica un font da un file `.json`.
    ///
    /// Restituisce un errore se il file non esiste, non è JSON valido o
    /// mancano campi obbligati.
    ///
    /// # Esempio
    ///
    /// ```
    /// use racingedge::simulator::fonts::FontDef;
    /// let font = FontDef::load("fonts/digital_64.json")
    ///     .expect("font presente nei test fixture");
    /// assert_eq!(font.name, "Racing-Digital");
    /// assert_eq!(font.size_px, 64.0);
    /// ```
    pub fn load(path: &str) -> Result<Self, String> {
        let text = fs::read_to_string(path)
            .map_err(|e| format!("{}: {e}", std::path::Path::new(path).display()))?;
        serde_json::from_str(&text).map_err(|e| format!("JSON non valido in {path}: {e}"))
    }

    /// Avanzamento totale in pixel della stringa `text` usando questo font.
    ///
    /// I caratteri non presenti nella mappa `glyphs` contribuiscono `0.0`
    /// all'avanzamento (il che è prudente per caratteri imprevisti).
    ///
    /// # Esempio
    ///
    /// ```
    /// use racingedge::simulator::fonts::FontDef;
    /// let font = FontDef::load("fonts/digital_64.json")
    ///     .expect("font presente nei test fixture");
    /// assert_eq!(font.measure("2024"), 92.0); // 24+24+24+20
    /// assert_eq!(font.measure("-1.5"), 58.0); // 12+14+8+20+8
    /// ```
    pub fn measure(&self, text: &str) -> f32 {
        text.chars()
            .map(|c| self.glyphs.get(&c.to_string()).map_or(0.0, |m| m.width))
            .sum()
    }

    /// Metriche del carattere `c`, se presenti.
    pub fn glyph(&self, c: char) -> Option<&GlyphMetrics> {
        self.glyphs.get(&c.to_string())
    }

    /// Altezza della riga di base in pixel.
    pub fn line_height(&self) -> f32 {
        self.baseline_px
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FONT_PATH: &str = "fonts/digital_64.json";

    /// Font di esempio usato dai test (costruito in memoria per evitare
    /// di modificare il file fixture durante i test).
    fn sample_font() -> FontDef {
        FontDef {
            name: "Racing-Digital".into(),
            size_px: 64.0,
            baseline_px: 72.0,
            glyphs: [
                (
                    "0",
                    GlyphMetrics {
                        width: 24.0,
                        x_offset: 0.0,
                        y_offset: 0.0,
                    },
                ),
                (
                    "1",
                    GlyphMetrics {
                        width: 14.0,
                        x_offset: 0.0,
                        y_offset: 0.0,
                    },
                ),
                (
                    "2",
                    GlyphMetrics {
                        width: 24.0,
                        x_offset: 0.0,
                        y_offset: 0.0,
                    },
                ),
                (
                    "3",
                    GlyphMetrics {
                        width: 24.0,
                        x_offset: 0.0,
                        y_offset: 0.0,
                    },
                ),
                (
                    "4",
                    GlyphMetrics {
                        width: 20.0,
                        x_offset: 0.0,
                        y_offset: 0.0,
                    },
                ),
                (
                    "5",
                    GlyphMetrics {
                        width: 24.0,
                        x_offset: 0.0,
                        y_offset: 0.0,
                    },
                ),
                (
                    "6",
                    GlyphMetrics {
                        width: 24.0,
                        x_offset: 0.0,
                        y_offset: 0.0,
                    },
                ),
                (
                    "7",
                    GlyphMetrics {
                        width: 16.0,
                        x_offset: 0.0,
                        y_offset: 0.0,
                    },
                ),
                (
                    "8",
                    GlyphMetrics {
                        width: 24.0,
                        x_offset: 0.0,
                        y_offset: 0.0,
                    },
                ),
                (
                    "9",
                    GlyphMetrics {
                        width: 24.0,
                        x_offset: 0.0,
                        y_offset: 0.0,
                    },
                ),
                (
                    ".",
                    GlyphMetrics {
                        width: 8.0,
                        x_offset: 0.0,
                        y_offset: 0.0,
                    },
                ),
                (
                    "-",
                    GlyphMetrics {
                        width: 12.0,
                        x_offset: 0.0,
                        y_offset: 0.0,
                    },
                ),
                (
                    "+",
                    GlyphMetrics {
                        width: 12.0,
                        x_offset: 0.0,
                        y_offset: 0.0,
                    },
                ),
            ]
            .iter()
            .map(|(k, v)| (k.to_string(), v.clone()))
            .collect(),
        }
    }

    #[test]
    fn test_load_round_trips() {
        let font = FontDef::load(FONT_PATH).expect("font fixture valido");
        assert_eq!(font.name, "Racing-Digital");
        assert_eq!(font.size_px, 64.0);
        assert_eq!(font.baseline_px, 72.0);
    }

    #[test]
    fn measure_sums_widths() {
        let font = sample_font();
        assert_eq!(font.measure("2024"), 92.0); // 24+24+24+20
        assert_eq!(font.measure("-1.5"), 58.0); // 12+14+8+20+8
        assert_eq!(font.measure("+9"), 36.0); // 12+24
        assert_eq!(font.measure("abc"), 0.0); // caratteri ignoti → 0
    }

    #[test]
    fn glyph_returns_metrics() {
        let font = sample_font();
        assert!(font.glyph('5').is_some());
        assert!(font.glyph('€').is_none());
        assert_eq!(font.glyph('5').unwrap().width, 24.0);
    }

    #[test]
    fn line_height_returns_baseline() {
        let font = sample_font();
        assert_eq!(font.line_height(), 72.0);
    }

    #[test]
    fn missing_file_is_error() {
        let err = FontDef::load("fonts/nonexistent.json").unwrap_err();
        assert!(err.contains("nonexistent"), "errore inatteso: {err}");
    }
}
