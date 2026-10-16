//! `<track>/data/data.json` — metadati della pista.
//!
//! Port di ksengine `adapters/assetto_corsa/TrackDataJsonLoader.h`. Come nel
//! riferimento il reader è un **semplice string-scan** invece di un parser JSON
//! completo: cerca `"chiave"` seguito da `:` e legge il primo valore scalare.
//! È sufficiente perché questi file non annidano le chiavi che ci interessano,
//! e non aggiunge dipendenze.
//!
//! Chiavi lette:
//!
//! | chiave JSON | campo | default |
//! | --- | --- | --- |
//! | `name` | [`TrackData::name`] | nome cartella |
//! | `length` | [`TrackData::spline_length`] | `0.0` |
//! | `width` | [`TrackData::width`] | `0.0` |

use std::fs;
use std::path::Path;

/// Metadati di una pista letti da `data.json`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TrackData {
    /// Nome visualizzato della pista.
    pub name: String,
    /// Lunghezza dello spline in metri (`length`).
    pub spline_length: f32,
    /// Larghezza nominale della pista in metri (`width`).
    pub width: f32,
}

impl TrackData {
    /// Legge e interpreta un `data.json`.
    pub fn load(path: impl AsRef<Path>) -> Result<Self, String> {
        let path = path.as_ref();
        let text = fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
        Ok(Self::from_json(&text))
    }

    /// Interpreta il contenuto di un `data.json`.
    pub fn from_json(text: &str) -> Self {
        Self {
            name: json_string(text, "name").unwrap_or_default(),
            spline_length: json_number(text, "length").unwrap_or(0.0),
            width: json_number(text, "width").unwrap_or(0.0),
        }
    }

    /// Copia i campi mancanti da `fallback` (usato quando `data.json` è assente
    /// e il nome va dedotto dalla cartella).
    pub fn fill_from(&mut self, fallback: &TrackData) {
        if self.name.is_empty() {
            self.name = fallback.name.clone();
        }
    }
}

/// Prima stringa JSON associata a `key`, se presente.
pub fn json_string(text: &str, key: &str) -> Option<String> {
    let rest = value_after_key(text, key)?;
    let rest = rest.trim_start();
    let rest = rest.strip_prefix('"')?;
    let end = rest.find('"')?;
    Some(rest[..end].to_string())
}

/// Primo numero JSON associato a `key`, se presente.
pub fn json_number(text: &str, key: &str) -> Option<f32> {
    let rest = value_after_key(text, key)?.trim_start();
    let end = rest
        .find(|c: char| !(c.is_ascii_digit() || matches!(c, '.' | '-' | '+' | 'e' | 'E')))
        .unwrap_or(rest.len());
    crate::editor::ini::parse_f32(&rest[..end])
}

/// Testo immediatamente successivo a `"key"` `:`, ignorando le occorrenze di
/// `key` che non sono una chiave JSON.
fn value_after_key<'a>(text: &'a str, key: &str) -> Option<&'a str> {
    let needle = format!("\"{key}\"");
    let mut from = 0;
    while let Some(rel) = text[from..].find(&needle) {
        let at = from + rel + needle.len();
        let after = text[at..].trim_start();
        if let Some(rest) = after.strip_prefix(':') {
            return Some(rest);
        }
        from = at;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"{
  "name": "ks_mugello",
  "length": 5245.61,
  "width": 14.0,
  "pitboxes": 24,
  "run": "Mugello",
  "nested": { "length": 1.0 }
}"#;

    #[test]
    fn reads_name_length_and_width() {
        let t = TrackData::from_json(SAMPLE);
        assert_eq!(t.name, "ks_mugello");
        assert_eq!(t.spline_length, 5245.61);
        assert_eq!(t.width, 14.0);
    }

    #[test]
    fn first_occurrence_wins_over_nested_ones() {
        assert_eq!(json_number(SAMPLE, "length"), Some(5245.61));
    }

    #[test]
    fn missing_keys_fall_back_to_defaults() {
        let t = TrackData::from_json("{}");
        assert!(t.name.is_empty());
        assert_eq!(t.spline_length, 0.0);
        assert_eq!(t.width, 0.0);
    }

    #[test]
    fn tolerates_whitespace_and_compact_json() {
        let t = TrackData::from_json(r#"{"name":"a","length":10,"width":8}"#);
        assert_eq!(t.name, "a");
        assert_eq!(t.spline_length, 10.0);
        assert_eq!(t.width, 8.0);
    }

    #[test]
    fn ignores_a_key_that_is_only_a_value() {
        // `"length"` compare prima come *valore*, poi come chiave.
        let text = r#"{"run": "length", "length": 42}"#;
        assert_eq!(json_number(text, "length"), Some(42.0));
    }

    #[test]
    fn fill_from_borrows_only_the_missing_name() {
        let mut t = TrackData::from_json(r#"{"length": 100}"#);
        t.fill_from(&TrackData {
            name: "from_folder".into(),
            ..Default::default()
        });
        assert_eq!(t.name, "from_folder");
        assert_eq!(t.spline_length, 100.0);
    }
}
