//! `fast_lane.ai` — linea AI di Assetto Corsa (port di ksengine `AiFileReader.*`
//! e `AiFileWriter.*`).
//!
//! Layout binario little-endian (intercambiabile con `AiSplineEditor` dell'editor Qt):
//! ```text
//! uint32 magic   = 0x00414900 ("\0AI")
//! uint32 version = 1
//! uint32 pointCount (<= 100000)
//! pointCount * { float x, y, z, curvature, speed }
//! ```
//! `distance` e `lap` sono derivati a runtime (`finish_spline`), mai su disco.
//! Se il magic non corrisponde, il reader ripiega su un file di testo separato da
//! virgole (`x,y,z[,curvature[,speed]]`).

use std::path::Path;

/// Magic di un file AI (`0x00414900`, ovvero `"\0AI"`).
pub const AI_MAGIC: u32 = 0x0041_4900;
/// Versione scritta dall'editor.
pub const AI_VERSION: u32 = 1;
/// Numero massimo di punti accettati.
pub const MAX_POINTS: u32 = 100_000;

/// Vettore 3D minimale (port di `AiVec3`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct AiVec3 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

impl AiVec3 {
    pub fn new(x: f32, y: f32, z: f32) -> Self {
        Self { x, y, z }
    }

    /// Distanza da un altro punto.
    pub fn distance_to(self, other: AiVec3) -> f32 {
        let dx = other.x - self.x;
        let dy = other.y - self.y;
        let dz = other.z - self.z;
        (dx * dx + dy * dy + dz * dz).sqrt()
    }
}

/// Un punto della linea AI.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct AiSplinePoint {
    pub position: AiVec3,
    pub curvature: f32,
    pub speed: f32,
    /// Distanza cumulata lungo la linea (derivata).
    pub distance: f32,
    /// Indice di giro (0 su disco; l'AIController lo deriva).
    pub lap: i32,
}

/// Una linea AI completa.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AiSpline {
    pub points: Vec<AiSplinePoint>,
    pub total_distance: f32,
    pub closed: bool,
}

impl AiSpline {
    /// Almeno due punti (port di `isValid`).
    pub fn is_valid(&self) -> bool {
        self.points.len() >= 2
    }

    pub fn empty(&self) -> bool {
        self.points.is_empty()
    }

    pub fn len(&self) -> usize {
        self.points.len()
    }

    pub fn is_empty(&self) -> bool {
        self.points.is_empty()
    }

    /// Ricalcola `distance` cumulata e il flag `closed` (port di `finishSpline`).
    pub fn finish(&mut self) {
        let mut cum = 0.0f32;
        for i in 0..self.points.len() {
            if i > 0 {
                cum += self.points[i - 1]
                    .position
                    .distance_to(self.points[i].position);
            }
            self.points[i].distance = cum;
        }
        self.total_distance = cum;

        if self.points.len() >= 2 {
            let gap = self.points[0]
                .position
                .distance_to(self.points[self.points.len() - 1].position);
            self.closed = self.total_distance > 0.0 && gap < self.total_distance * 0.05;
        } else {
            self.closed = false;
        }
    }
}

/// Legge una linea AI da disco (binaria, con fallback testo).
pub fn read_spline(path: impl AsRef<Path>) -> AiSpline {
    let path = path.as_ref();
    let bytes = match std::fs::read(path) {
        Ok(b) => b,
        Err(_) => return AiSpline::default(),
    };
    if bytes.len() >= 4 {
        let magic = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
        if magic == AI_MAGIC {
            return parse_binary(&bytes).unwrap_or_default();
        }
    }
    match std::str::from_utf8(&bytes) {
        Ok(text) => parse_text(text),
        Err(_) => AiSpline::default(),
    }
}

/// Parsing del formato binario; errore descrittivo se non valido.
pub fn parse_binary(bytes: &[u8]) -> Result<AiSpline, String> {
    if bytes.len() < 12 {
        return Err("unexpected end of file".into());
    }
    let magic = u32::from_le_bytes(bytes[0..4].try_into().unwrap());
    if magic != AI_MAGIC {
        return Err(format!(
            "bad AI magic 0x{magic:08x} (expected 0x{AI_MAGIC:08x})"
        ));
    }
    let version = u32::from_le_bytes(bytes[4..8].try_into().unwrap());
    let count = u32::from_le_bytes(bytes[8..12].try_into().unwrap());
    if count == 0 || count > MAX_POINTS {
        return Err(format!("implausible point count {count}"));
    }
    let needed = 12 + count as usize * 20;
    if bytes.len() < needed {
        return Err("point data extends past end of file".into());
    }

    let mut spline = AiSpline {
        points: Vec::with_capacity(count as usize),
        ..Default::default()
    };
    let _ = version; // version 1 is the only writer today
    for i in 0..count as usize {
        let base = 12 + i * 20;
        let f = |o: usize| f32::from_le_bytes(bytes[base + o..base + o + 4].try_into().unwrap());
        spline.points.push(AiSplinePoint {
            position: AiVec3::new(f(0), f(4), f(8)),
            curvature: f(12),
            speed: f(16),
            ..Default::default()
        });
    }
    spline.finish();
    Ok(spline)
}

/// Parsing del formato testuale (`#` commenti, campi separati da `,`/`;`/tab).
pub fn parse_text(text: &str) -> AiSpline {
    let mut spline = AiSpline::default();
    for raw in text.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let mut values = [0.0f32; 5];
        let mut fields = 0;
        for token in line.split([',', ';', '\t', ' ']).filter(|s| !s.is_empty()) {
            if fields >= 5 {
                break;
            }
            match token.parse::<f32>() {
                Ok(v) => {
                    values[fields] = v;
                    fields += 1;
                }
                Err(_) => break,
            }
        }
        if fields < 3 {
            continue;
        }
        spline.points.push(AiSplinePoint {
            position: AiVec3::new(values[0], values[1], values[2]),
            curvature: values[3],
            speed: values[4],
            ..Default::default()
        });
        if spline.points.len() as u32 > MAX_POINTS {
            return AiSpline::default();
        }
    }
    spline.finish();
    spline
}

/// Serializza nel formato binario (senza toccare il disco).
pub fn write_to_vec(spline: &AiSpline) -> Option<Vec<u8>> {
    if spline.points.is_empty() || spline.points.len() as u32 > MAX_POINTS {
        return None;
    }
    let mut out = Vec::with_capacity(12 + spline.points.len() * 20);
    out.extend_from_slice(&AI_MAGIC.to_le_bytes());
    out.extend_from_slice(&AI_VERSION.to_le_bytes());
    out.extend_from_slice(&(spline.points.len() as u32).to_le_bytes());
    for p in &spline.points {
        for f in [
            p.position.x,
            p.position.y,
            p.position.z,
            p.curvature,
            p.speed,
        ] {
            out.extend_from_slice(&f.to_le_bytes());
        }
    }
    Some(out)
}

/// Scrive una linea AI su disco (port di `writeSpline`).
pub fn write_spline(path: impl AsRef<Path>, spline: &AiSpline) -> bool {
    match write_to_vec(spline) {
        Some(bytes) => std::fs::write(path, bytes).is_ok(),
        None => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> AiSpline {
        AiSpline {
            points: vec![
                AiSplinePoint {
                    position: AiVec3::new(0.0, 0.0, 0.0),
                    curvature: 0.1,
                    speed: 30.0,
                    ..Default::default()
                },
                AiSplinePoint {
                    position: AiVec3::new(3.0, 4.0, 0.0),
                    curvature: 0.2,
                    speed: 25.0,
                    ..Default::default()
                },
                AiSplinePoint {
                    position: AiVec3::new(3.0, 4.0, 12.0),
                    curvature: -0.1,
                    speed: 20.0,
                    ..Default::default()
                },
            ],
            ..Default::default()
        }
    }

    #[test]
    fn binary_round_trip_preserves_points() {
        let spline = sample();
        let bytes = write_to_vec(&spline).expect("writes");
        assert_eq!(&bytes[0..4], &AI_MAGIC.to_le_bytes());
        let back = parse_binary(&bytes).expect("parses");
        assert_eq!(back.points.len(), 3);
        assert_eq!(back.points[0].position, AiVec3::new(0.0, 0.0, 0.0));
        assert_eq!(back.points[2].position, AiVec3::new(3.0, 4.0, 12.0));
        assert!((back.points[1].curvature - 0.2).abs() < 1e-6);
        assert!((back.points[2].speed - 20.0).abs() < 1e-6);
    }

    #[test]
    fn finish_accumulates_distance() {
        let back = parse_binary(&write_to_vec(&sample()).unwrap()).unwrap();
        assert!((back.points[0].distance - 0.0).abs() < 1e-6);
        assert!(
            (back.points[1].distance - 5.0).abs() < 1e-6,
            "3-4-0 triangle"
        );
        assert!((back.points[2].distance - 17.0).abs() < 1e-6, "+12 along z");
        assert!((back.total_distance - 17.0).abs() < 1e-6);
    }

    #[test]
    fn closed_detection_needs_small_gap() {
        let mut open = sample();
        open.finish();
        assert!(!open.closed, "first/last are 13 apart over 17 m");

        let mut looped = AiSpline {
            points: vec![
                AiSplinePoint {
                    position: AiVec3::new(0.0, 0.0, 0.0),
                    ..Default::default()
                },
                AiSplinePoint {
                    position: AiVec3::new(100.0, 0.0, 0.0),
                    ..Default::default()
                },
                AiSplinePoint {
                    position: AiVec3::new(0.1, 0.0, 0.0),
                    ..Default::default()
                },
            ],
            ..Default::default()
        };
        looped.finish();
        assert!(looped.closed, "end returns within 5% of the lap");
    }

    #[test]
    fn bad_magic_is_rejected() {
        let mut bytes = write_to_vec(&sample()).unwrap();
        bytes[0] ^= 0xff;
        let err = parse_binary(&bytes).unwrap_err();
        assert!(err.contains("bad AI magic"), "{err}");
    }

    #[test]
    fn implausible_count_is_rejected() {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&AI_MAGIC.to_le_bytes());
        bytes.extend_from_slice(&AI_VERSION.to_le_bytes());
        bytes.extend_from_slice(&(MAX_POINTS + 1).to_le_bytes());
        let err = parse_binary(&bytes).unwrap_err();
        assert!(err.contains("implausible point count"), "{err}");
    }

    #[test]
    fn truncated_point_data_is_rejected() {
        let bytes = write_to_vec(&sample()).unwrap();
        let err = parse_binary(&bytes[..bytes.len() - 4]).unwrap_err();
        assert!(err.contains("past end of file"), "{err}");
    }

    #[test]
    fn empty_spline_is_not_written() {
        assert!(write_to_vec(&AiSpline::default()).is_none());
    }

    #[test]
    fn text_parse_reads_comma_and_tab() {
        let text = "# comment\n\
                    0, 0, 0, 0.1, 30\n\
                    3;4;0;0.2;25\n\
                    \n\
                    bad line\n\
                    3\t4\t12\n";
        let spline = parse_text(text);
        assert_eq!(spline.points.len(), 3);
        assert_eq!(spline.points[0].position, AiVec3::new(0.0, 0.0, 0.0));
        assert!((spline.points[0].speed - 30.0).abs() < 1e-6);
        assert_eq!(spline.points[1].position, AiVec3::new(3.0, 4.0, 0.0));
        // Three fields is the minimum; curvature/speed default to 0.
        assert_eq!(spline.points[2].position, AiVec3::new(3.0, 4.0, 12.0));
        assert_eq!(spline.points[2].speed, 0.0);
    }

    #[test]
    fn read_spline_falls_back_to_text() {
        let path = std::env::temp_dir().join("racingedge_ai_text.ai");
        std::fs::write(&path, "0,0,0,0.1,30\n10,0,0,0.0,40\n").unwrap();
        let spline = read_spline(&path);
        assert_eq!(spline.points.len(), 2);
        assert!((spline.total_distance - 10.0).abs() < 1e-6);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn read_spline_binary_matches_written_file() {
        let path = std::env::temp_dir().join("racingedge_ai_bin.ai");
        assert!(write_spline(&path, &sample()));
        let spline = read_spline(&path);
        assert_eq!(spline.points.len(), 3);
        assert!((spline.total_distance - 17.0).abs() < 1e-6);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn missing_file_is_empty() {
        let missing = std::env::temp_dir().join("racingedge_ai_absent.ai");
        let spline = read_spline(&missing);
        assert!(spline.empty());
        assert!(!spline.is_valid());
    }
}
