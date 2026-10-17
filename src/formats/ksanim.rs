//! `.ksanim` — contenitore di animazioni nodali (port di ksengine `KsAnim.*`).
//!
//! Formato binario little-endian:
//! ```text
//! int32 version      = 0 | 1 | 2
//! int32 entryCount
//! per ogni entry:
//!   int32 nameLen + nameLen bytes (UTF-8/ASCII)
//!   int32 keyframeCount
//!   version 0/1: keyframeCount * 16 float (matrice 4x4 per keyframe)
//!   version 2  : keyframeCount * 10 float (quat xyzw + trans xyz + scale xyz)
//! ```
//!
//! La matrice è row-major con la traslazione in [12..14] — lo stesso layout
//! dei nodi KN5, quindi una matrice animata può essere inserita direttamente
//! in un nodo senza conversione.
//!
//! Il port segue la struttura del reader C++ di ksengine: controlli di limite
//! su entry/keyframe/name, tolleranza ai byte finali, e stessa semantica di
//! sampling (slerp per v2, snap al keyframe più vicino per v0/v1).

use std::fs;
use std::path::Path;

/// Versione minima supportata del formato `.ksanim`.
pub const MIN_VERSION: i32 = 0;
/// Versione massima supportata del formato `.ksanim`.
pub const MAX_VERSION: i32 = 2;
/// Numero di float in una matrice 4x4.
pub const MATRIX_FLOATS: usize = 16;
/// Numero di float in un keyframe v2 (quat + trans + scale).
pub const KEYFRAME_FLOATS: usize = 10;

/// Limite massimo di entry (specurato dal corpus reale: max 605).
const MAX_ENTRIES: i32 = 100_000;
/// Limite massimo di keyframe per entry (max osservato: 193).
const MAX_KEYFRAMES: i32 = 1_000_000;
/// Limite massimo di byte per il nome di un nodo.
const MAX_NAME_BYTES: u32 = 4096;
/// Flag high-bit per la codifica Latin-1 di Qt (QDataStream).
const QT_LATIN1_FLAG: u32 = 0x8000_0000;

/// Un keyframe di animazione (versione 2).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Keyframe {
    /// Quaternione di rotazione (x, y, z, w).
    pub rotation: [f32; 4],
    /// Traslazione (x, y, z).
    pub translation: [f32; 3],
    /// Scala (x, y, z).
    pub scale: [f32; 3],
}

impl Keyframe {
    /// Converte il keyframe in una matrice 4x4 row-major (S * R * T).
    ///
    /// La traslazione vive in [12..14], come nei nodi KN5.
    pub fn to_matrix(&self) -> [f32; MATRIX_FLOATS] {
        let [x, y, z, w] = self.rotation;

        // Matrice di rotazione 3x3 dal quaternione.
        let r = [
            [
                1.0 - 2.0 * (y * y + z * z),
                2.0 * (x * y - z * w),
                2.0 * (x * z + y * w),
            ],
            [
                2.0 * (x * y + z * w),
                1.0 - 2.0 * (x * x + z * z),
                2.0 * (y * z - x * w),
            ],
            [
                2.0 * (x * z - y * w),
                2.0 * (y * z + x * w),
                1.0 - 2.0 * (x * x + y * y),
            ],
        ];

        let mut m = [0.0f32; MATRIX_FLOATS];
        for row in 0..3 {
            let s = self.scale[row];
            for col in 0..3 {
                m[row * 4 + col] = s * r[row][col];
            }
            m[row * 4 + 3] = 0.0;
        }
        m[12] = self.translation[0];
        m[13] = self.translation[1];
        m[14] = self.translation[2];
        m[15] = 1.0;
        m
    }
}

/// Una entry di animazione: un nodo con i suoi keyframe.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AnimEntry {
    /// Nome del nodo (es. `DOOR_R`, `WHEEL_FL`).
    pub name: String,
    /// Matrici 4x4 per keyframe (versione 0/1).
    pub matrices: Vec<[f32; MATRIX_FLOATS]>,
    /// Keyframe strutturati (versione 2).
    pub keyframes: Vec<Keyframe>,
}

impl AnimEntry {
    /// Numero di keyframe (matrices o keyframes a seconda della versione).
    pub fn keyframe_count(&self) -> usize {
        if self.keyframes.is_empty() {
            self.matrices.len()
        } else {
            self.keyframes.len()
        }
    }
}

/// Un file `.ksanim` completo.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct KsAnimFile {
    /// Versione del formato (0, 1 o 2).
    pub version: i32,
    /// Entry di animazione.
    pub entries: Vec<AnimEntry>,
}

impl KsAnimFile {
    /// Cerca una entry per nome.
    pub fn find(&self, name: &str) -> Option<&AnimEntry> {
        self.entries.iter().find(|e| e.name == name)
    }

    /// Numero totale di keyframe in tutte le entry.
    pub fn total_keyframes(&self) -> usize {
        self.entries.iter().map(|e| e.keyframe_count()).sum()
    }

    /// Nomi di tutti i nodi animati.
    pub fn animated_nodes(&self) -> Vec<&str> {
        self.entries.iter().map(|e| e.name.as_str()).collect()
    }

    /// Campiona la matrice del nodo `name` al tempo normalizzato `t` in [0, 1].
    ///
    /// Versione 2: slerp della rotazione, lerp di traslazione e scala.
    /// Versione 0/1: snap al keyframe più vicino.
    /// Restituisce `false` se il nodo non esiste o non ha keyframe.
    pub fn sample(&self, name: &str, t: f32, out: &mut [f32; MATRIX_FLOATS]) -> bool {
        let entry = match self.find(name) {
            Some(e) => e,
            None => return false,
        };

        if !entry.keyframes.is_empty() {
            let count = entry.keyframes.len();
            if count == 1 {
                *out = entry.keyframes[0].to_matrix();
                return true;
            }
            let clamped = t.clamp(0.0, 1.0);
            let raw = clamped * (count as f32 - 1.0);
            let i0 = raw.floor() as usize;
            let i1 = (i0 + 1).min(count - 1);
            let frac = raw - i0 as f32;

            let a = &entry.keyframes[i0];
            let b = &entry.keyframes[i1];
            let blended = Keyframe {
                rotation: quat_slerp(a.rotation, b.rotation, frac),
                translation: lerp3(a.translation, b.translation, frac),
                scale: lerp3(a.scale, b.scale, frac),
            };
            *out = blended.to_matrix();
            return true;
        }

        if entry.matrices.is_empty() {
            return false;
        }
        let clamped = t.clamp(0.0, 1.0);
        let last = (entry.matrices.len() - 1) as f32;
        let mut index = (clamped * last).round() as usize;
        if index >= entry.matrices.len() {
            index = entry.matrices.len() - 1;
        }
        *out = entry.matrices[index];
        true
    }
}

/// Risultato del parsing di un file `.ksanim`.
#[derive(Clone, Debug)]
pub struct ParseResult {
    /// File parsato (valido solo se `error` è vuoto).
    pub file: KsAnimFile,
    /// Errore di parsing, vuoto in caso di successo.
    pub error: String,
}

impl ParseResult {
    /// `true` se il parsing è riuscito.
    pub fn ok(&self) -> bool {
        self.error.is_empty()
    }
}

/// `true` se i primi 4 bytes sono una versione `.ksanim` plausibile (0..2).
pub fn is_ksanim(bytes: &[u8]) -> bool {
    if bytes.len() < 4 {
        return false;
    }
    let version = i32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
    (MIN_VERSION..=MAX_VERSION).contains(&version)
}

/// Parsing di un'immagine `.ksanim` in memoria (binary-safe).
pub fn parse_ksanim(bytes: &[u8]) -> ParseResult {
    let mut result = ParseResult {
        file: KsAnimFile::default(),
        error: String::new(),
    };

    let mut cursor = Cursor::new(bytes);

    // Versione.
    let version = match cursor.read_i32() {
        Some(v) => v,
        None => {
            result.error = format!("offset {}: unexpected end of file", cursor.pos());
            return result;
        }
    };
    if !(MIN_VERSION..=MAX_VERSION).contains(&version) {
        result.error = format!(
            "offset {}: unsupported ksanim version {version}",
            cursor.pos() - 4
        );
        return result;
    }
    result.file.version = version;

    // Numero di entry.
    let count = match cursor.read_count(MAX_ENTRIES, "entry") {
        Some(c) => c,
        None => {
            result.error = cursor.take_error();
            return result;
        }
    };

    let v2 = version == 2;
    result.file.entries.reserve(count as usize);

    for _ in 0..count {
        let mut entry = AnimEntry::default();

        // Nome del nodo.
        match cursor.read_name() {
            Some(name) => entry.name = name,
            None => {
                result.error = cursor.take_error();
                return result;
            }
        }

        // Numero di keyframe.
        let keyframes = match cursor.read_count(MAX_KEYFRAMES, "keyframe") {
            Some(k) => k,
            None => {
                result.error = cursor.take_error();
                return result;
            }
        };

        if v2 {
            // Controllo dimensione: keyframes * 10 * 4 bytes.
            let needed = keyframes as u64 * KEYFRAME_FLOATS as u64 * 4;
            if needed > cursor.remaining() as u64 {
                result.error =
                    format!("offset {}: keyframes extend past end of file", cursor.pos());
                return result;
            }
            entry.keyframes.reserve(keyframes as usize);
            for _ in 0..keyframes {
                let mut values = [0.0f32; KEYFRAME_FLOATS];
                if !cursor.read_f32s(&mut values) {
                    result.error = cursor.take_error();
                    return result;
                }
                entry.keyframes.push(Keyframe {
                    rotation: [values[0], values[1], values[2], values[3]],
                    translation: [values[4], values[5], values[6]],
                    scale: [values[7], values[8], values[9]],
                });
            }
        } else {
            // Controllo dimensione: keyframes * 16 * 4 bytes.
            let needed = keyframes as u64 * MATRIX_FLOATS as u64 * 4;
            if needed > cursor.remaining() as u64 {
                result.error =
                    format!("offset {}: keyframes extend past end of file", cursor.pos());
                return result;
            }
            entry.matrices.reserve(keyframes as usize);
            for _ in 0..keyframes {
                let mut m = [0.0f32; MATRIX_FLOATS];
                if !cursor.read_f32s(&mut m) {
                    result.error = cursor.take_error();
                    return result;
                }
                entry.matrices.push(m);
            }
        }

        result.file.entries.push(entry);
    }

    result
}

/// Legge e parsia un file `.ksanim` da disco.
pub fn parse_ksanim_file(path: impl AsRef<Path>) -> ParseResult {
    let path = path.as_ref();
    match fs::read(path) {
        Ok(bytes) => parse_ksanim(&bytes),
        Err(e) => ParseResult {
            file: KsAnimFile::default(),
            error: format!("cannot open file {}: {e}", path.display()),
        },
    }
}

// ── Cursor binario little-endian ─────────────────────────────────────────────

struct Cursor<'a> {
    data: &'a [u8],
    pos: usize,
    error: Option<String>,
}

impl<'a> Cursor<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self {
            data,
            pos: 0,
            error: None,
        }
    }

    fn pos(&self) -> usize {
        self.pos
    }

    fn remaining(&self) -> usize {
        self.data.len().saturating_sub(self.pos)
    }

    fn fail(&mut self, msg: String) {
        if self.error.is_none() {
            self.error = Some(format!("offset {}: {msg}", self.pos));
        }
    }

    fn take_error(&mut self) -> String {
        self.error.take().unwrap_or_default()
    }

    fn read_bytes(&mut self, n: usize) -> Option<&'a [u8]> {
        if n > self.remaining() {
            self.fail("unexpected end of file".into());
            return None;
        }
        let slice = &self.data[self.pos..self.pos + n];
        self.pos += n;
        Some(slice)
    }

    fn read_i32(&mut self) -> Option<i32> {
        let b = self.read_bytes(4)?;
        Some(i32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }

    fn read_f32s(&mut self, out: &mut [f32]) -> bool {
        let n = out.len() * 4;
        let b = match self.read_bytes(n) {
            Some(b) => b,
            None => return false,
        };
        for (i, slot) in out.iter_mut().enumerate() {
            *slot = f32::from_le_bytes([b[i * 4], b[i * 4 + 1], b[i * 4 + 2], b[i * 4 + 3]]);
        }
        true
    }

    fn read_count(&mut self, limit: i32, what: &str) -> Option<i32> {
        let v = self.read_i32()?;
        if v < 0 || v > limit {
            self.fail(format!("implausible {what} count {v}"));
            return None;
        }
        Some(v)
    }

    fn read_name(&mut self) -> Option<String> {
        let raw = {
            let b = self.read_bytes(4)?;
            u32::from_le_bytes([b[0], b[1], b[2], b[3]])
        };
        let length = raw & !QT_LATIN1_FLAG;
        if length > MAX_NAME_BYTES {
            self.fail(format!("implausible node name length {length}"));
            return None;
        }
        if length as usize > self.remaining() {
            self.fail("node name extends past end of file".into());
            return None;
        }
        let b = &self.data[self.pos..self.pos + length as usize];
        self.pos += length as usize;
        Some(String::from_utf8_lossy(b).into_owned())
    }
}

// ── Interpolazione ────────────────────────────────────────────────────────────

/// Lerp componente per componente su [f32; 3].
fn lerp3(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    [
        a[0] + (b[0] - a[0]) * t,
        a[1] + (b[1] - a[1]) * t,
        a[2] + (b[2] - a[2]) * t,
    ]
}

/// Normalizza un quaternione.
fn normalize_quat(q: [f32; 4]) -> [f32; 4] {
    let len = (q[0] * q[0] + q[1] * q[1] + q[2] * q[2] + q[3] * q[3]).sqrt();
    if len <= 1e-8 {
        return [0.0, 0.0, 0.0, 1.0];
    }
    let inv = 1.0 / len;
    [q[0] * inv, q[1] * inv, q[2] * inv, q[3] * inv]
}

/// Slerp tra due quaternioni. Il segno di `b` viene invertito quando i quaternioni
/// puntano in direzioni opposte, per prendere l'arco più corto.
fn quat_slerp(a: [f32; 4], b: [f32; 4], t: f32) -> [f32; 4] {
    let mut dot = a[0] * b[0] + a[1] * b[1] + a[2] * b[2] + a[3] * b[3];
    let mut sign = 1.0f32;
    if dot < 0.0 {
        dot = -dot;
        sign = -1.0;
    }
    if dot > 0.9999 {
        let mixed = [
            a[0] + (b[0] * sign - a[0]) * t,
            a[1] + (b[1] * sign - a[1]) * t,
            a[2] + (b[2] * sign - a[2]) * t,
            a[3] + (b[3] * sign - a[3]) * t,
        ];
        return normalize_quat(mixed);
    }
    let angle = dot.acos();
    let sin_angle = angle.sin();
    let w1 = ((1.0 - t) * angle).sin() / sin_angle;
    let w2 = (t * angle).sin() / sin_angle * sign;
    normalize_quat([
        a[0] * w1 + b[0] * w2,
        a[1] * w1 + b[1] * w2,
        a[2] * w1 + b[2] * w2,
        a[3] * w1 + b[3] * w2,
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Costruisce un file `.ksanim` v1 minimale con un entry e un keyframe
    /// identità (matrice 4x4 con traslazione in [12..14]).
    fn build_v1_anim() -> Vec<u8> {
        let mut buf = Vec::new();
        buf.extend_from_slice(&1i32.to_le_bytes()); // version
        buf.extend_from_slice(&1i32.to_le_bytes()); // entry count
        let name = b"DOOR_R";
        buf.extend_from_slice(&(name.len() as i32).to_le_bytes());
        buf.extend_from_slice(name);
        buf.extend_from_slice(&1i32.to_le_bytes()); // keyframe count
                                                    // Matrice identità con traslazione (10, 20, 30).
        let m = [
            1.0f32, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 10.0, 20.0, 30.0, 1.0,
        ];
        for v in m.iter() {
            buf.extend_from_slice(&v.to_le_bytes());
        }
        buf
    }

    /// Costruisce un file `.ksanim` v2 minimale con un entry e due keyframe.
    fn build_v2_anim() -> Vec<u8> {
        let mut buf = Vec::new();
        buf.extend_from_slice(&2i32.to_le_bytes()); // version
        buf.extend_from_slice(&1i32.to_le_bytes()); // entry count
        let name = b"WHEEL_FL";
        buf.extend_from_slice(&(name.len() as i32).to_le_bytes());
        buf.extend_from_slice(name);
        buf.extend_from_slice(&2i32.to_le_bytes()); // keyframe count
                                                    // Keyframe 0: identità.
        let k0 = [0.0f32, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 1.0, 1.0];
        for v in k0.iter() {
            buf.extend_from_slice(&v.to_le_bytes());
        }
        // Keyframe 1: rotazione 90° attorno a Z + traslazione.
        let angle = std::f32::consts::FRAC_PI_2;
        let k1 = [
            0.0f32,
            0.0,
            angle.sin(),
            angle.cos(),
            5.0,
            0.0,
            0.0,
            1.0,
            1.0,
            1.0,
        ];
        for v in k1.iter() {
            buf.extend_from_slice(&v.to_le_bytes());
        }
        buf
    }

    #[test]
    fn is_ksanim_rejects_short_input() {
        assert!(!is_ksanim(&[]));
        assert!(!is_ksanim(&[0, 0, 0]));
    }

    #[test]
    fn is_ksanim_accepts_valid_versions() {
        assert!(is_ksanim(&0i32.to_le_bytes()));
        assert!(is_ksanim(&1i32.to_le_bytes()));
        assert!(is_ksanim(&2i32.to_le_bytes()));
    }

    #[test]
    fn is_ksanim_rejects_invalid_version() {
        assert!(!is_ksanim(&3i32.to_le_bytes()));
        assert!(!is_ksanim(&(-1i32).to_le_bytes()));
    }

    #[test]
    fn parse_v1_anim() {
        let bytes = build_v1_anim();
        let result = parse_ksanim(&bytes);
        assert!(result.ok(), "errore: {}", result.error);
        assert_eq!(result.file.version, 1);
        assert_eq!(result.file.entries.len(), 1);
        assert_eq!(result.file.entries[0].name, "DOOR_R");
        assert_eq!(result.file.entries[0].matrices.len(), 1);
        assert_eq!(result.file.entries[0].keyframes.len(), 0);
        assert_eq!(result.file.total_keyframes(), 1);
    }

    #[test]
    fn parse_v2_anim() {
        let bytes = build_v2_anim();
        let result = parse_ksanim(&bytes);
        assert!(result.ok(), "errore: {}", result.error);
        assert_eq!(result.file.version, 2);
        assert_eq!(result.file.entries.len(), 1);
        assert_eq!(result.file.entries[0].name, "WHEEL_FL");
        assert_eq!(result.file.entries[0].keyframes.len(), 2);
        assert_eq!(result.file.entries[0].matrices.len(), 0);
    }

    #[test]
    fn parse_rejects_bad_version() {
        let mut bytes = build_v1_anim();
        bytes[0..4].copy_from_slice(&99i32.to_le_bytes());
        let result = parse_ksanim(&bytes);
        assert!(!result.ok());
        assert!(result.error.contains("unsupported ksanim version"));
    }

    #[test]
    fn parse_rejects_truncated_file() {
        let bytes = build_v1_anim();
        let result = parse_ksanim(&bytes[..bytes.len() - 4]);
        assert!(!result.ok());
        // Truncating the last float corrupts the keyframe count read or the
        // matrix data read; either way parsing must fail.
        assert!(!result.error.is_empty());
    }

    #[test]
    fn parse_rejects_implausible_entry_count() {
        let mut bytes = build_v1_anim();
        bytes[4..8].copy_from_slice(&(-1i32).to_le_bytes());
        let result = parse_ksanim(&bytes);
        assert!(!result.ok());
        assert!(result.error.contains("implausible entry count"));
    }

    #[test]
    fn parse_rejects_implausible_keyframe_count() {
        let mut bytes = build_v1_anim();
        // entry count = 1, poi nameLen = 5, poi "DOOR_R", poi keyframe count = -1.
        let name_off = 8;
        let kf_off = name_off + 4 + 6;
        bytes[kf_off..kf_off + 4].copy_from_slice(&(-1i32).to_le_bytes());
        let result = parse_ksanim(&bytes);
        assert!(!result.ok());
        assert!(result.error.contains("implausible keyframe count"));
    }

    #[test]
    fn parse_accepts_trailing_bytes() {
        let mut bytes = build_v1_anim();
        bytes.extend_from_slice(&[0xAA, 0xBB, 0xCC]);
        let result = parse_ksanim(&bytes);
        assert!(result.ok(), "errore: {}", result.error);
    }

    #[test]
    fn parse_accepts_zero_keyframe_entry() {
        let mut buf = Vec::new();
        buf.extend_from_slice(&1i32.to_le_bytes());
        buf.extend_from_slice(&1i32.to_le_bytes());
        let name = b"EMPTY";
        buf.extend_from_slice(&(name.len() as i32).to_le_bytes());
        buf.extend_from_slice(name);
        buf.extend_from_slice(&0i32.to_le_bytes()); // zero keyframes
        let result = parse_ksanim(&buf);
        assert!(result.ok(), "errore: {}", result.error);
        assert_eq!(result.file.entries.len(), 1);
        assert_eq!(result.file.entries[0].name, "EMPTY");
        assert_eq!(result.file.entries[0].keyframe_count(), 0);
    }

    #[test]
    fn parse_accepts_qt_latin1_name_flag() {
        let mut buf = Vec::new();
        buf.extend_from_slice(&1i32.to_le_bytes());
        buf.extend_from_slice(&1i32.to_le_bytes());
        let name = b"TEST";
        let flag = (name.len() as u32) | QT_LATIN1_FLAG;
        buf.extend_from_slice(&flag.to_le_bytes());
        buf.extend_from_slice(name);
        buf.extend_from_slice(&0i32.to_le_bytes());
        let result = parse_ksanim(&buf);
        assert!(result.ok(), "errore: {}", result.error);
        assert_eq!(result.file.entries[0].name, "TEST");
    }

    #[test]
    fn sample_v1_snaps_to_keyframe() {
        let bytes = build_v1_anim();
        let file = parse_ksanim(&bytes).file;
        let mut out = [0.0f32; MATRIX_FLOATS];
        assert!(file.sample("DOOR_R", 0.0, &mut out));
        assert_eq!(out[12], 10.0);
        assert_eq!(out[13], 20.0);
        assert_eq!(out[14], 30.0);
        assert_eq!(out[15], 1.0);
    }

    #[test]
    fn sample_v2_interpolates() {
        let bytes = build_v2_anim();
        let file = parse_ksanim(&bytes).file;
        let mut out = [0.0f32; MATRIX_FLOATS];
        assert!(file.sample("WHEEL_FL", 0.0, &mut out));
        // A t=0 il primo keyframe è identità.
        assert_eq!(out[12], 0.0);
        assert!(file.sample("WHEEL_FL", 1.0, &mut out));
        // A t=1 il secondo keyframe ha traslazione x=5.
        assert_eq!(out[12], 5.0);
    }

    #[test]
    fn sample_returns_false_for_unknown_node() {
        let bytes = build_v1_anim();
        let file = parse_ksanim(&bytes).file;
        let mut out = [0.0f32; MATRIX_FLOATS];
        assert!(!file.sample("NONEXISTENT", 0.5, &mut out));
    }

    #[test]
    fn sample_returns_false_for_empty_entry() {
        let mut buf = Vec::new();
        buf.extend_from_slice(&1i32.to_le_bytes());
        buf.extend_from_slice(&1i32.to_le_bytes());
        let name = b"EMPTY";
        buf.extend_from_slice(&(name.len() as i32).to_le_bytes());
        buf.extend_from_slice(name);
        buf.extend_from_slice(&0i32.to_le_bytes());
        let file = parse_ksanim(&buf).file;
        let mut out = [0.0f32; MATRIX_FLOATS];
        assert!(!file.sample("EMPTY", 0.5, &mut out));
    }

    #[test]
    fn animated_nodes_returns_all_names() {
        let mut buf = Vec::new();
        buf.extend_from_slice(&1i32.to_le_bytes());
        buf.extend_from_slice(&2i32.to_le_bytes());
        for name in [b"A", b"B"] {
            buf.extend_from_slice(&(name.len() as i32).to_le_bytes());
            buf.extend_from_slice(name);
            buf.extend_from_slice(&0i32.to_le_bytes());
        }
        let file = parse_ksanim(&buf).file;
        let nodes = file.animated_nodes();
        assert_eq!(nodes, vec!["A", "B"]);
    }

    #[test]
    fn keyframe_to_matrix_identity() {
        let kf = Keyframe {
            rotation: [0.0, 0.0, 0.0, 1.0],
            translation: [1.0, 2.0, 3.0],
            scale: [1.0, 1.0, 1.0],
        };
        let m = kf.to_matrix();
        assert_eq!(m[0], 1.0);
        assert_eq!(m[5], 1.0);
        assert_eq!(m[10], 1.0);
        assert_eq!(m[12], 1.0);
        assert_eq!(m[13], 2.0);
        assert_eq!(m[14], 3.0);
        assert_eq!(m[15], 1.0);
    }

    #[test]
    fn keyframe_to_matrix_applies_scale() {
        let kf = Keyframe {
            rotation: [0.0, 0.0, 0.0, 1.0],
            translation: [0.0, 0.0, 0.0],
            scale: [2.0, 3.0, 4.0],
        };
        let m = kf.to_matrix();
        assert_eq!(m[0], 2.0);
        assert_eq!(m[5], 3.0);
        assert_eq!(m[10], 4.0);
    }

    #[test]
    fn parse_file_missing_returns_error() {
        let result = parse_ksanim_file("nonexistent.ksanim");
        assert!(!result.ok());
        assert!(result.error.contains("cannot open file"));
    }
}
