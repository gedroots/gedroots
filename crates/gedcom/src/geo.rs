//! Place coordinates (`PLAC.MAP.LATI` / `LONG`) and map projection.
//!
//! GEDCOM 7 writes the latitude `N48.8463` / `S12.5` and the longitude `E2.3522` / `W1.0906`
//! (decimal degrees, cardinal prefix). The variants found in real files are read too: signed
//! number, decimal comma, spaces.

use crate::model::Place;

/// Reads a GEDCOM latitude (`N48.85`, `S12,5`, `-12.5`…), in degrees.
pub fn parse_latitude(s: &str) -> Option<f64> {
    parse_coord(s, 'N', 'S').filter(|v| (-90.0..=90.0).contains(v))
}

/// Reads a GEDCOM longitude (`E2.35`, `W1.09`, `-1.09`…), in degrees.
pub fn parse_longitude(s: &str) -> Option<f64> {
    parse_coord(s, 'E', 'W').filter(|v| (-180.0..=180.0).contains(v))
}

fn parse_coord(s: &str, pos: char, neg: char) -> Option<f64> {
    let s = s.trim();
    let first = s.chars().next()?.to_ascii_uppercase();
    let (sign, rest) = if first == pos {
        (1.0, &s[1..])
    } else if first == neg || (neg == 'W' && first == 'O') {
        // “O” (ouest, west) is found in French-language files.
        (-1.0, &s[1..])
    } else {
        (1.0, s)
    };
    let v: f64 = rest.trim().replace(',', ".").parse().ok()?;
    v.is_finite().then_some(sign * v)
}

/// Writes a latitude in GEDCOM 7 format (`N45.76404`).
pub fn format_latitude(v: f64) -> String {
    format_coord(v, 'N', 'S')
}

/// Writes a longitude in GEDCOM 7 format (`E4.83566`).
pub fn format_longitude(v: f64) -> String {
    format_coord(v, 'E', 'W')
}

fn format_coord(v: f64, pos: char, neg: char) -> String {
    let c = if v < 0.0 { neg } else { pos };
    // 5 decimals ≈ 1 m: plenty for a municipality or a locality.
    let mut n = format!("{:.5}", v.abs());
    while n.ends_with('0') {
        n.pop();
    }
    if n.ends_with('.') {
        n.push('0');
    }
    format!("{c}{n}")
}

impl Place {
    /// Coordinates `(latitude, longitude)` in degrees, if they are readable.
    pub fn coords(&self) -> Option<(f64, f64)> {
        Some((parse_latitude(self.latitude.as_deref()?)?, parse_longitude(self.longitude.as_deref()?)?))
    }

    /// Geocoding queries to try, from the most precise to the broadest.
    ///
    /// In Ancestris' format (`Lieudit, Commune, Code_Postal, Département, Région, Pays`), codes
    /// (postal, INSEE) get in the way of full-text search: they are removed.
    pub fn geocode_queries(&self) -> Vec<String> {
        let raw: Vec<&str> = self.name.split(',').map(str::trim).collect();
        let is_code = |s: &&str| !s.is_empty() && s.chars().all(|c| c.is_ascii_digit() || c == ' ');
        let mut out: Vec<String> = Vec::new();
        let mut push = |parts: Vec<&str>| {
            let q = parts.into_iter().filter(|s| !s.is_empty() && !is_code(s)).collect::<Vec<_>>().join(", ");
            if !q.is_empty() && !out.contains(&q) {
                out.push(q);
            }
        };
        push(raw.clone());
        if raw.len() >= 4 {
            let at = |i: usize| raw.get(i).copied().unwrap_or("");
            let country = at(raw.len() - 1);
            // Without the locality, then municipality + country only.
            push(vec![at(1), at(3), country]);
            push(vec![at(1), country]);
        } else if raw.len() >= 2 {
            push(vec![raw[0], raw[raw.len() - 1]]);
        }
        out
    }
}

/// Size of a map tile, in pixels.
pub const TILE_SIZE: f64 = 256.0;

/// Web Mercator projection: “world” position at zoom 0 (0‥256 on each axis).
pub fn project(lat: f64, lon: f64) -> (f64, f64) {
    let lat = lat.clamp(-85.05112878, 85.05112878).to_radians();
    let x = (lon + 180.0) / 360.0 * TILE_SIZE;
    let y = (1.0 - (lat.tan() + 1.0 / lat.cos()).ln() / std::f64::consts::PI) / 2.0 * TILE_SIZE;
    (x, y)
}

/// Inverse of [`project`]: `(latitude, longitude)` in degrees.
pub fn unproject(x: f64, y: f64) -> (f64, f64) {
    let lon = x / TILE_SIZE * 360.0 - 180.0;
    let n = std::f64::consts::PI * (1.0 - 2.0 * y / TILE_SIZE);
    let lat = n.sinh().atan().to_degrees();
    (lat, lon)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_variants() {
        assert_eq!(parse_latitude("N49.1157"), Some(49.1157));
        assert_eq!(parse_latitude("S12,5"), Some(-12.5));
        assert_eq!(parse_latitude(" -3.25 "), Some(-3.25));
        assert_eq!(parse_longitude("W1.0906"), Some(-1.0906));
        assert_eq!(parse_longitude("O1.5"), Some(-1.5));
        assert_eq!(parse_longitude("E2.35"), Some(2.35));
        assert_eq!(parse_latitude("N95"), None);
        assert_eq!(parse_latitude(""), None);
        assert_eq!(parse_longitude("abc"), None);
    }

    #[test]
    fn format_roundtrip() {
        assert_eq!(format_latitude(45.764043), "N45.76404");
        assert_eq!(format_longitude(-1.09), "W1.09");
        assert_eq!(format_longitude(3.0), "E3.0");
        assert_eq!(parse_longitude(&format_longitude(-0.12345)), Some(-0.12345));
    }

    #[test]
    fn mercator() {
        let (x, y) = project(0.0, 0.0);
        assert!((x - 128.0).abs() < 1e-9 && (y - 128.0).abs() < 1e-9);
        let (lat, lon) = unproject(project(48.8566, 2.3522).0, project(48.8566, 2.3522).1);
        assert!((lat - 48.8566).abs() < 1e-9 && (lon - 2.3522).abs() < 1e-9);
    }

    #[test]
    fn queries_ancestris() {
        let p = Place {
            name: "La Gare, Amions, 42260, Loire, Auvergne-Rhône-Alpes, France".into(),
            latitude: None,
            longitude: None,
            uses: 1,
            records: vec![],
        };
        assert_eq!(
            p.geocode_queries(),
            vec!["La Gare, Amions, Loire, Auvergne-Rhône-Alpes, France", "Amions, Loire, France", "Amions, France"]
        );
        let p = Place { name: ", Troyes, 10000, Aube, Grand Est, France".into(), ..p };
        assert_eq!(
            p.geocode_queries(),
            vec!["Troyes, Aube, Grand Est, France", "Troyes, Aube, France", "Troyes, France"]
        );
        let p = Place { name: "Paris".into(), ..p };
        assert_eq!(p.geocode_queries(), vec!["Paris"]);
    }
}
