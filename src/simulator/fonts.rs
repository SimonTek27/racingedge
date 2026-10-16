//! Font bitmap in stile Assetto Corsa (`*.acf`) — port di ksengine `AcfFile`
//! (`src/sdk/kseditor/modules/fontEditor/fonteditor_acffile.*`, GPL-3.0).
//! Originale: <https://github.com/SimonTek27/ksengine>
//!
//! AC descrive un font bitmap con **una sola sezione INI piatta** `[CONFIG]`:
//! poche chiavi di famiglia/peso/stile più una terna `HPAD_i` / `VPAD_i` /
//! `WIDTH_i` per ogni codice ASCII, indicizzata da `0` = spazio (32) a
//! `94` = tilde (126) — l'indice è sempre `codice - 32`.
//!
//! | chiave | campo | note |
//! | --- | --- | --- |
//! | `FONT` | [`AcfFont::font_name`] | nome visualizzato |
//! | `FAMILY` | [`AcfFont::family`] | famiglia TrueType di partenza |
//! | `SIZE` | [`AcfFont::size_pt`] | corpo in punti |
//! | `WEIGHT` | [`AcfFont::bold`] | `B` = grassetto, `R` = normale |
//! | `STYLE` | [`AcfFont::italic`] | `I` = corsivo, `N` = normale |
//! | `HEIGHT` | [`AcfFont::height`] | altezza cella condivisa, in pixel |
//! | `WIDTH_i` | [`CharMetrics::pixel_width`] | larghezza della cella = avanzamento |
//! | `HPAD_i` | [`CharMetrics::h_padding`] | offset orizzontale nel disegno |
//! | `VPAD_i` | [`CharMetrics::v_padding`] | offset verticale nel disegno |
//!
//! L'avanzamento orizzontale è `WIDTH_i` (l'atlante accoda le celle una dopo
//! l'altra); `HPAD_i`/`VPAD_i` spostano soltanto il glifo dentro la propria
//! cella `WIDTH_i × HEIGHT`, quindi non entrano nella misura del testo.
//!
//! Il parser riusa [`crate::editor::ini`] (Rule 3): le chiavi sono
//! confrontate senza distinzione di maiuscole e una chiave ripetuta vince
//! l'ultima occorrenza, come per gli altri file di contenuto.

use std::fmt::Write as _;
use std::fs;
use std::path::Path;

use crate::editor::ini::{parse_f32, parse_i32, Ini, IniSection};

/// Primo codice ASCII descritto da un font AC (`' '`).
pub const FIRST_CHAR_CODE: u8 = 32;
/// Ultimo codice ASCII descritto da un font AC (`'~'`).
pub const LAST_CHAR_CODE: u8 = 126;
/// Numero di voci per carattere (`LAST - FIRST + 1` = 95).
pub const CHAR_COUNT: usize = (LAST_CHAR_CODE - FIRST_CHAR_CODE) as usize + 1;

/// Sezione che contiene tutte le chiavi di un `.acf`.
const SECTION: &str = "CONFIG";

/// Metriche di un singolo glifo.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CharMetrics {
    /// Offset orizzontale del disegno nella cella (`HPAD_i`), pixel.
    pub h_padding: i32,
    /// Offset verticale del disegno nella cella (`VPAD_i`), pixel.
    pub v_padding: i32,
    /// Larghezza della cella, cioè l'avanzamento (`WIDTH_i`), pixel.
    pub pixel_width: i32,
}

impl CharMetrics {
    /// Avanzamento orizzontale in pixel, mai negativo.
    pub fn advance(&self) -> f32 {
        self.pixel_width.max(0) as f32
    }
}

/// Un font bitmap AC completo, con una voce per ogni codice ASCII 32..=126.
#[derive(Clone, Debug, PartialEq)]
pub struct AcfFont {
    /// Nome visualizzato del font (`FONT`).
    pub font_name: String,
    /// Famiglia TrueType di partenza (`FAMILY`).
    pub family: String,
    /// Corpo in punti (`SIZE`).
    pub size_pt: f32,
    /// `true` quando `WEIGHT` vale `B`.
    pub bold: bool,
    /// `true` quando `STYLE` vale `I`.
    pub italic: bool,
    /// Altezza condivisa di una riga, in pixel (`HEIGHT`).
    pub height: f32,
    chars: Vec<CharMetrics>,
}

impl Default for AcfFont {
    fn default() -> Self {
        Self::empty()
    }
}

impl AcfFont {
    /// Font vuoto: campi azzerati e una voce di default per ogni carattere.
    pub fn empty() -> Self {
        Self {
            font_name: String::new(),
            family: String::new(),
            size_pt: 0.0,
            bold: false,
            italic: false,
            height: 0.0,
            chars: vec![CharMetrics::default(); CHAR_COUNT],
        }
    }

    /// Indice della voce di `ch` nella tabella (ASCII 32..=126).
    ///
    /// # Esempio
    ///
    /// ```
    /// use racingedge::simulator::fonts::AcfFont;
    /// assert_eq!(AcfFont::index_of(' '), Some(0));
    /// assert_eq!(AcfFont::index_of('~'), Some(94));
    /// assert_eq!(AcfFont::index_of('€'), None);
    /// ```
    pub fn index_of(ch: char) -> Option<usize> {
        let code = ch as u32;
        if (FIRST_CHAR_CODE as u32..=LAST_CHAR_CODE as u32).contains(&code) {
            Some((code - FIRST_CHAR_CODE as u32) as usize)
        } else {
            None
        }
    }

    /// Metriche di `ch`, se il font lo descrive.
    pub fn metrics(&self, ch: char) -> Option<CharMetrics> {
        Self::index_of(ch).and_then(|i| self.chars.get(i).copied())
    }

    /// Imposta le metriche di `ch`; `false` se il carattere è fuori da ASCII 32..=126.
    ///
    /// # Esempio
    ///
    /// ```
    /// use racingedge::simulator::fonts::{AcfFont, CharMetrics};
    /// let mut font = AcfFont::empty();
    /// assert!(font.set_metrics('A', CharMetrics { pixel_width: 24, ..Default::default() }));
    /// assert!(!font.set_metrics('€', CharMetrics::default()));
    /// ```
    pub fn set_metrics(&mut self, ch: char, metrics: CharMetrics) -> bool {
        match Self::index_of(ch) {
            Some(i) => {
                self.chars[i] = metrics;
                true
            }
            None => false,
        }
    }

    /// Tutte le metriche, una per codice ASCII 32..=126.
    pub fn chars(&self) -> &[CharMetrics] {
        &self.chars
    }

    /// Avanzamento orizzontale di `ch` in pixel (`0` per caratteri non coperti).
    pub fn advance(&self, ch: char) -> f32 {
        self.metrics(ch).map_or(0.0, |m| m.advance())
    }

    /// Altezza di una riga in pixel (`HEIGHT`).
    pub fn line_height(&self) -> f32 {
        self.height
    }

    /// Larghezza in pixel della stringa `text`: somma degli avanzamenti.
    ///
    /// I caratteri fuori da ASCII 32..=126 non sono nell'atlante e valgono `0`.
    ///
    /// # Esempio
    ///
    /// ```
    /// use racingedge::simulator::fonts::{AcfFont, CharMetrics};
    /// let mut font = AcfFont::empty();
    /// font.set_metrics('0', CharMetrics { pixel_width: 20, ..Default::default() });
    /// font.set_metrics('1', CharMetrics { pixel_width: 12, ..Default::default() });
    /// assert_eq!(font.measure("01"), 32.0);
    /// assert_eq!(font.measure("01\n"), 32.0); // non in atlante
    /// ```
    pub fn measure(&self, text: &str) -> f32 {
        text.chars().map(|c| self.advance(c)).sum()
    }

    /// Interpreta il contenuto di un file `.acf`.
    ///
    /// # Esempio
    ///
    /// ```
    /// use racingedge::simulator::fonts::{AcfFont, CHAR_COUNT};
    /// let mut text = String::from(
    ///     "[CONFIG]\nFONT = Fixation\nSIZE = 48\nFAMILY = Fixation\n\
    ///      WEIGHT = R\nSTYLE = N\nHEIGHT = 85\n",
    /// );
    /// for i in 0..CHAR_COUNT {
    ///     text.push_str(&format!("HPAD_{i} = 0\nVPAD_{i} = 13\nWIDTH_{i} = 21\n"));
    /// }
    /// let font = AcfFont::parse(&text).expect("preset valido");
    /// assert_eq!(font.line_height(), 85.0);
    /// assert_eq!(font.measure("ABC"), 63.0);
    /// ```
    pub fn parse(text: &str) -> Result<Self, String> {
        let ini = Ini::parse(text);
        let section = ini
            .section(SECTION)
            .ok_or_else(|| format!("sezione [{SECTION}] mancante nel file .acf"))?;

        let font_name = require(section, "FONT")?.to_string();
        let size_pt = require_f32(section, "SIZE")?;
        let family = require(section, "FAMILY")?.to_string();
        let bold = require(section, "WEIGHT")?.eq_ignore_ascii_case("B");
        let italic = require(section, "STYLE")?.eq_ignore_ascii_case("I");
        let height = require_f32(section, "HEIGHT")?;

        let mut chars = Vec::with_capacity(CHAR_COUNT);
        for index in 0..CHAR_COUNT {
            let read = |name: &str| -> Result<i32, String> {
                require_i32(section, &format!("{name}_{index}"))
                    .map_err(|e| format!("metriche mancanti per il carattere indice {index}: {e}"))
            };
            chars.push(CharMetrics {
                h_padding: read("HPAD")?,
                v_padding: read("VPAD")?,
                pixel_width: read("WIDTH")?,
            });
        }

        Ok(Self {
            font_name,
            family,
            size_pt,
            bold,
            italic,
            height,
            chars,
        })
    }

    /// Legge e interpreta un file `.acf`.
    pub fn load(path: impl AsRef<Path>) -> Result<Self, String> {
        let path = path.as_ref();
        let text = fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
        Self::parse(&text)
    }

    /// Serializza il font nel formato `.acf`; l'uscita rientra in [`AcfFont::parse`].
    ///
    /// # Esempio
    ///
    /// ```
    /// use racingedge::simulator::fonts::AcfFont;
    /// let font = AcfFont::empty();
    /// let text = font.to_ini().unwrap();
    /// assert!(text.starts_with("[CONFIG]\nFONT = \n"));
    /// assert_eq!(AcfFont::parse(&text).unwrap(), font);
    /// ```
    pub fn to_ini(&self) -> Result<String, String> {
        if self.chars.len() != CHAR_COUNT {
            return Err(format!(
                "dati dei caratteri incompleti (attesi {CHAR_COUNT}, trovati {})",
                self.chars.len()
            ));
        }

        let mut out = String::new();
        out.push_str("[CONFIG]\n");
        let _ = writeln!(out, "FONT = {}", self.font_name);
        let _ = writeln!(out, "SIZE = {}", self.size_pt);
        let _ = writeln!(out, "FAMILY = {}", self.family);
        let _ = writeln!(out, "WEIGHT = {}", if self.bold { "B" } else { "R" });
        let _ = writeln!(out, "STYLE = {}", if self.italic { "I" } else { "N" });
        let _ = writeln!(out, "HEIGHT = {}", self.height);
        for (index, metrics) in self.chars.iter().enumerate() {
            let _ = writeln!(out, "HPAD_{index} = {}", metrics.h_padding);
            let _ = writeln!(out, "VPAD_{index} = {}", metrics.v_padding);
            let _ = writeln!(out, "WIDTH_{index} = {}", metrics.pixel_width);
        }
        Ok(out)
    }

    /// Scrive il font su disco nel formato `.acf`.
    pub fn save(&self, path: impl AsRef<Path>) -> Result<(), String> {
        let text = self.to_ini()?;
        let path = path.as_ref();
        fs::write(path, text).map_err(|e| format!("{}: {e}", path.display()))
    }
}

/// Chiave obbligatoria della sezione `[CONFIG]`.
fn require<'a>(section: &'a IniSection, key: &str) -> Result<&'a str, String> {
    section
        .get(key)
        .ok_or_else(|| format!("chiave mancante nel file .acf: {key}"))
}

/// Chiave obbligatoria interpretata come numero reale.
fn require_f32(section: &IniSection, key: &str) -> Result<f32, String> {
    let raw = require(section, key)?;
    parse_f32(raw).ok_or_else(|| format!("valore non numerico per {key}: {raw:?}"))
}

/// Chiave obbligatoria interpretata come intero.
fn require_i32(section: &IniSection, key: &str) -> Result<i32, String> {
    let raw = require(section, key)?;
    parse_i32(raw).ok_or_else(|| format!("valore non numerico per {key}: {raw:?}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Preset `.acf` con la forma di un file reale: commento iniziale, corpo
    /// 48 pt e le 95 terne `HPAD/VPAD/WIDTH` (lo spazio è più stretto).
    fn sample_acf() -> String {
        let mut text = String::from(
            "; Fixation 48pt (estratto da un preset reale)\n\
             [CONFIG]\n\
             FONT = Fixation\n\
             SIZE = 48\n\
             FAMILY = Fixation\n\
             WEIGHT = R\n\
             STYLE = N\n\
             HEIGHT = 85\n",
        );
        for index in 0..CHAR_COUNT {
            let width = if index == 0 { 21 } else { 24 };
            text.push_str(&format!(
                "HPAD_{index} = 0\nVPAD_{index} = 13\nWIDTH_{index} = {width}\n"
            ));
        }
        text
    }

    #[test]
    fn parses_headers_and_per_char_metrics() {
        let font = AcfFont::parse(&sample_acf()).expect("preset valido");

        assert_eq!(font.font_name, "Fixation");
        assert_eq!(font.family, "Fixation");
        assert_eq!(font.size_pt, 48.0);
        assert!(!font.bold);
        assert!(!font.italic);
        assert_eq!(font.line_height(), 85.0);
        assert_eq!(font.chars().len(), CHAR_COUNT);

        assert_eq!(font.metrics(' ').expect("spazio").pixel_width, 21);
        assert_eq!(font.metrics('~').expect("tilde").pixel_width, 24);
        assert_eq!(font.metrics(' ').expect("spazio").v_padding, 13);
        assert_eq!(font.advance(' '), 21.0);
        assert_eq!(font.metrics('\n'), None);
    }

    #[test]
    fn save_round_trips_through_parse() {
        let font = AcfFont::parse(&sample_acf()).expect("preset valido");
        let text = font.to_ini().expect("serializza");
        assert_eq!(AcfFont::parse(&text).expect("rilegge"), font);
    }

    #[test]
    fn bold_and_italic_flags_are_case_insensitive() {
        let text = sample_acf()
            .replace("WEIGHT = R", "weight = b")
            .replace("STYLE = N", "style = I");
        let font = AcfFont::parse(&text).expect("preset valido");
        assert!(font.bold);
        assert!(font.italic);

        let saved = font.to_ini().expect("serializza");
        assert!(saved.contains("WEIGHT = B\n"));
        assert!(saved.contains("STYLE = I\n"));
    }

    #[test]
    fn accepts_comma_decimals_like_ac() {
        let text = sample_acf()
            .replace("SIZE = 48", "SIZE = 48,5")
            .replace("HEIGHT = 85", "HEIGHT = 85.5");
        let font = AcfFont::parse(&text).expect("preset valido");
        assert_eq!(font.size_pt, 48.5);
        assert_eq!(font.line_height(), 85.5);
    }

    #[test]
    fn missing_section_is_an_error() {
        let err = AcfFont::parse("FONT = X\n").unwrap_err();
        assert!(err.contains("CONFIG"), "errore inatteso: {err}");
    }

    #[test]
    fn missing_config_key_is_an_error() {
        let text = sample_acf().replace("WEIGHT = R\n", "");
        let err = AcfFont::parse(&text).unwrap_err();
        assert!(err.contains("WEIGHT"), "errore inatteso: {err}");
    }

    #[test]
    fn missing_glyph_metrics_is_an_error() {
        let text = sample_acf().replace(&format!("WIDTH_{} = 24\n", CHAR_COUNT - 1), "");
        let err = AcfFont::parse(&text).unwrap_err();
        assert!(err.contains("indice 94"), "errore inatteso: {err}");
    }

    #[test]
    fn non_numeric_field_is_an_error() {
        let text = sample_acf().replace("SIZE = 48", "SIZE = grande");
        let err = AcfFont::parse(&text).unwrap_err();
        assert!(err.contains("SIZE"), "errore inatteso: {err}");
    }

    #[test]
    fn measure_sums_advances_and_ignores_uncovered_chars() {
        let mut font = AcfFont::empty();
        font.set_metrics(
            'A',
            CharMetrics {
                pixel_width: 30,
                ..Default::default()
            },
        );
        assert_eq!(font.measure("A A"), 60.0);
        assert_eq!(font.measure("A\nA"), 60.0);
        assert_eq!(font.measure("€"), 0.0);
    }

    #[test]
    fn negative_width_never_advances_backwards() {
        let mut font = AcfFont::empty();
        font.set_metrics(
            'A',
            CharMetrics {
                pixel_width: -5,
                ..Default::default()
            },
        );
        assert_eq!(font.advance('A'), 0.0);
    }
}
