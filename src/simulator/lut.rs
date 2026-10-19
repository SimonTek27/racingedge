//! LUT parser: `x|y` per line, `;` comments.
//! Supporta anche 2Dlut semplici (griglia).

use std::fs;
use std::path::Path;

/// Lookup table 1D con interpolazione lineare.
#[derive(Clone, Debug, Default)]
pub struct Lut1D {
    pub points: Vec<(f32, f32)>,
}

impl Lut1D {
    pub fn load(path: impl AsRef<Path>) -> Result<Self, String> {
        let text = fs::read_to_string(path.as_ref())
            .map_err(|e| format!("{}: {e}", path.as_ref().display()))?;
        Self::parse(&text)
    }

    pub fn parse(text: &str) -> Result<Self, String> {
        let mut points: Vec<(f32, f32)> = Vec::new();
        for line in text.lines() {
            let line = line.split(';').next().unwrap_or("").trim();
            if line.is_empty() {
                continue;
            }
            // formats: x|y  or  x,y  or  x y
            let parts: Vec<&str> = if line.contains('|') {
                line.split('|').collect()
            } else if line.contains(',') {
                line.split(',').collect()
            } else {
                line.split_whitespace().collect()
            };
            if parts.len() >= 2 {
                if let (Ok(x), Ok(y)) = (parts[0].trim().parse(), parts[1].trim().parse()) {
                    points.push((x, y));
                }
            }
        }
        if points.is_empty() {
            return Err("empty LUT".into());
        }
        points.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
        Ok(Self { points })
    }

    pub fn from_pairs(points: Vec<(f32, f32)>) -> Self {
        let mut points = points;
        points.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
        Self { points }
    }

    /// Interpolazione lineare; clamp fuori range.
    pub fn sample(&self, x: f32) -> f32 {
        if self.points.is_empty() {
            return 0.0;
        }
        if x <= self.points[0].0 {
            return self.points[0].1;
        }
        let last = self.points.last().unwrap();
        if x >= last.0 {
            return last.1;
        }
        for i in 0..self.points.len() - 1 {
            let (x0, y0) = self.points[i];
            let (x1, y1) = self.points[i + 1];
            if x >= x0 && x <= x1 {
                let t = (x - x0) / (x1 - x0).max(1e-9);
                return y0 + t * (y1 - y0);
            }
        }
        last.1
    }

    pub fn is_empty(&self) -> bool {
        self.points.is_empty()
    }
}

/// Synthesizes a slip → grip-multiplier curve (peak ~1 at `slip_peak`, falloff after).
pub fn synthesize_slip_curve(slip_peak: f32, falloff: f32, points: usize) -> Lut1D {
    let mut pts = Vec::with_capacity(points);
    let max_slip = slip_peak * 4.0;
    for i in 0..points {
        let x = -max_slip + 2.0 * max_slip * (i as f32) / (points as f32 - 1.0);
        let ax = x.abs();
        let y = if ax <= slip_peak {
            // rise to 1
            let t = ax / slip_peak.max(1e-4);
            // smooth approx of Pacejka shape
            (t * std::f32::consts::FRAC_PI_2).sin()
        } else {
            let over = (ax - slip_peak) / slip_peak.max(1e-4);
            (1.0 - falloff * over).clamp(0.25, 1.0)
        };
        pts.push((
            x,
            y * x.signum().abs().max(0.0) * if x < 0.0 { -1.0 } else { 1.0 },
        ));
        // actually for force we want signed: sign(slip) * magnitude
        let mag = if ax <= slip_peak {
            (ax / slip_peak.max(1e-4) * std::f32::consts::FRAC_PI_2).sin()
        } else {
            let over = (ax - slip_peak) / slip_peak.max(1e-4);
            (1.0 - falloff * over).clamp(0.25, 1.0)
        };
        pts.pop();
        pts.push((x, mag * x.signum()));
    }
    Lut1D::from_pairs(pts)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn sample_lerp() {
        let l = Lut1D::from_pairs(vec![(0.0, 0.0), (10.0, 100.0)]);
        assert!((l.sample(5.0) - 50.0).abs() < 0.01);
    }
}
