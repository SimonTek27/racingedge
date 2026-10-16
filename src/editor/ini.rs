//! Parser INI minimale con **ordine delle sezioni preservato**.
//!
//! I file di contenuto del gioco (`surfaces.ini`, `sounds.ini`, `setup.ini`, …)
//! sono INI piatti in stile Assetto Corsa: sezioni `[NOME]`, coppie
//! `chiave = valore`, commenti `;`. L'ordine conta — ad esempio in
//! `surfaces.ini` l'indice della sezione è l'indice della superficie — quindi
//! questo parser conserva l'ordine di inserimento invece di usare una
//! `HashMap` come fa il reader delle auto.
//!
//! * le chiavi sono confrontate **senza distinzione di maiuscole/minuscole**
//! * `parse_f32` accetta la virgola decimale (`0,94`), che AC usa a volte
//! * una chiave ripetuta nella stessa sezione vince l'ultima occorrenza
//! * il BOM UTF-8 iniziale viene ignorato

use std::fs;
use std::path::Path;

/// Una sezione `[NOME]` con le sue coppie chiave/valore, in ordine di file.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct IniSection {
    name: String,
    values: Vec<(String, String)>,
}

impl IniSection {
    /// Nome della sezione, sempre normalizzato in maiuscolo.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Valore grezzo di `key` (confronto case-insensitive).
    pub fn get(&self, key: &str) -> Option<&str> {
        let key = key.to_ascii_uppercase();
        self.values
            .iter()
            .find(|(k, _)| *k == key)
            .map(|(_, v)| v.as_str())
    }

    /// Valore numerico di `key`, tollerante a virgola decimale e virgolette.
    pub fn get_f32(&self, key: &str) -> Option<f32> {
        self.get(key).and_then(parse_f32)
    }

    /// Valore booleano di `key`: accetta `1`/`0`, `true`/`false`, `yes`/`no`.
    pub fn get_bool(&self, key: &str) -> Option<bool> {
        let raw = self.get(key)?;
        match raw.to_ascii_lowercase().as_str() {
            "1" | "true" | "yes" | "on" => Some(true),
            "0" | "false" | "no" | "off" => Some(false),
            _ => None,
        }
    }

    /// Numero di chiavi presenti.
    pub fn len(&self) -> usize {
        self.values.len()
    }

    /// `true` quando la sezione non ha chiavi.
    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }

    /// Coppie chiave/valore in ordine di file.
    pub fn iter(&self) -> impl Iterator<Item = (&str, &str)> {
        self.values.iter().map(|(k, v)| (k.as_str(), v.as_str()))
    }
}

/// Un file INI completo.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Ini {
    sections: Vec<IniSection>,
}

impl Ini {
    /// Legge e interpreta un file INI.
    pub fn load(path: impl AsRef<Path>) -> Result<Self, String> {
        let path = path.as_ref();
        let text = fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
        Ok(Self::parse(&text))
    }

    /// Interpreta il contenuto di un file INI.
    pub fn parse(text: &str) -> Self {
        let text = text.strip_prefix('\u{feff}').unwrap_or(text);
        let mut ini = Ini::default();
        let mut current: Option<usize> = None;

        for raw in text.lines() {
            let line = strip_comment(raw).trim();
            if line.is_empty() {
                continue;
            }

            if let Some(rest) = line.strip_prefix('[') {
                if let Some(name) = rest.strip_suffix(']') {
                    ini.sections.push(IniSection {
                        name: name.trim().to_ascii_uppercase(),
                        values: Vec::new(),
                    });
                    current = Some(ini.sections.len() - 1);
                    continue;
                }
            }

            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            let key = key.trim().to_ascii_uppercase();
            let value = value.trim().trim_matches('"').to_string();

            if current.is_none() {
                // Chiavi prima di qualunque sezione finiscono in `_ROOT`.
                ini.sections.insert(
                    0,
                    IniSection {
                        name: "_ROOT".to_string(),
                        values: Vec::new(),
                    },
                );
                current = Some(0);
            }
            let section = &mut ini.sections[current.expect("appena impostata")];
            match section.values.iter_mut().find(|(k, _)| *k == key) {
                Some(slot) => slot.1 = value,
                None => section.values.push((key, value)),
            }
        }

        ini
    }

    /// Sezioni in ordine di file.
    pub fn sections(&self) -> &[IniSection] {
        &self.sections
    }

    /// Sezione `name` (confronto case-insensitive).
    pub fn section(&self, name: &str) -> Option<&IniSection> {
        let name = name.to_ascii_uppercase();
        self.sections.iter().find(|s| s.name == name)
    }

    /// Valore di `key` dentro `section`.
    pub fn get(&self, section: &str, key: &str) -> Option<&str> {
        self.section(section)?.get(key)
    }

    /// Valore numerico di `key` dentro `section`.
    pub fn get_f32(&self, section: &str, key: &str) -> Option<f32> {
        self.section(section)?.get_f32(key)
    }

    /// `true` quando il file non contiene sezioni.
    pub fn is_empty(&self) -> bool {
        self.sections.is_empty()
    }

    /// Numero di sezioni.
    pub fn len(&self) -> usize {
        self.sections.len()
    }
}

/// Interpreta un numero in stile AC (`0.94`, `0,94`, `"0.94"`, `1e-3`).
pub fn parse_f32(raw: &str) -> Option<f32> {
    let raw = raw.trim().trim_matches('"').trim();
    if raw.is_empty() {
        return None;
    }
    raw.parse::<f32>()
        .ok()
        .or_else(|| raw.replace(',', ".").parse::<f32>().ok())
}

/// Interpreta un intero in stile AC, tollerando la virgola decimale.
pub fn parse_i32(raw: &str) -> Option<i32> {
    let raw = raw.trim().trim_matches('"').trim();
    raw.parse::<i32>()
        .ok()
        .or_else(|| parse_f32(raw).map(|v| v as i32))
}

/// Rimuove un commento `;` o `#` non quotato.
fn strip_comment(line: &str) -> &str {
    let mut in_quotes = false;
    for (i, c) in line.char_indices() {
        match c {
            '"' => in_quotes = !in_quotes,
            ';' | '#' if !in_quotes => return &line[..i],
            _ => {}
        }
    }
    line
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "\u{feff}; commento iniziale\n\
        [SURFACE_0]\n\
        KEY = ROAD\n\
        FRICTION=0.94   ; riferimento\n\
        WIDTH = 12\n\
        \n\
        [SURFACE_1]\n\
        KEY=GRASS\n\
        FRICTION=0,55\n\
        LOOP=1\n\
        LOOP=0\n";

    #[test]
    fn sections_keep_file_order() {
        let ini = Ini::parse(SAMPLE);
        assert_eq!(ini.len(), 2);
        assert_eq!(ini.sections()[0].name(), "SURFACE_0");
        assert_eq!(ini.sections()[1].name(), "SURFACE_1");
    }

    #[test]
    fn keys_are_case_insensitive_and_comments_are_stripped() {
        let ini = Ini::parse(SAMPLE);
        let s0 = ini.section("surface_0").expect("sezione");
        assert_eq!(s0.get("key"), Some("ROAD"));
        assert_eq!(s0.get("Friction"), Some("0.94"));
        assert_eq!(s0.get_bool("loop"), None, "chiave assente");
    }

    #[test]
    fn numbers_accept_comma_decimals_and_quotes() {
        let ini = Ini::parse("[A]\nX=0,55\nY=\"3\"\n");
        assert_eq!(ini.get_f32("a", "x"), Some(0.55));
        assert_eq!(ini.get_f32("a", "y"), Some(3.0));
    }

    #[test]
    fn last_occurrence_wins() {
        let ini = Ini::parse("[A]\nX=1\nX=2\n");
        assert_eq!(ini.get_f32("A", "X"), Some(2.0));
    }

    #[test]
    fn keys_before_first_section_land_in_root() {
        let ini = Ini::parse("VERSION=3\n[A]\nX=1\n");
        assert_eq!(ini.sections()[0].name(), "_ROOT");
        assert_eq!(ini.get_f32("_ROOT", "VERSION"), Some(3.0));
        assert_eq!(ini.get_f32("A", "X"), Some(1.0));
    }

    #[test]
    fn quoted_values_keep_comment_content() {
        let ini = Ini::parse("[A]\nPATH=\"a;b\"\n");
        assert_eq!(ini.get("A", "PATH"), Some("a;b"));
    }
}
