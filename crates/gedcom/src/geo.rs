//! Coordonnées des lieux (`PLAC.MAP.LATI` / `LONG`) et projection de la carte.
//!
//! GEDCOM 7 écrit la latitude `N48.8463` / `S12.5` et la longitude `E2.3522` / `W1.0906`
//! (degrés décimaux, préfixe cardinal). On lit aussi les variantes rencontrées dans
//! les fichiers réels : nombre signé, virgule décimale, espaces.

use crate::model::Place;

/// Lit une latitude GEDCOM (`N48.85`, `S12,5`, `-12.5`…), en degrés.
pub fn parse_latitude(s: &str) -> Option<f64> {
    parse_coord(s, 'N', 'S').filter(|v| (-90.0..=90.0).contains(v))
}

/// Lit une longitude GEDCOM (`E2.35`, `W1.09`, `-1.09`…), en degrés.
pub fn parse_longitude(s: &str) -> Option<f64> {
    parse_coord(s, 'E', 'W').filter(|v| (-180.0..=180.0).contains(v))
}

fn parse_coord(s: &str, pos: char, neg: char) -> Option<f64> {
    let s = s.trim();
    let first = s.chars().next()?.to_ascii_uppercase();
    let (sign, rest) = if first == pos {
        (1.0, &s[1..])
    } else if first == neg || (neg == 'W' && first == 'O') {
        // « O » (ouest) se rencontre dans des fichiers francophones.
        (-1.0, &s[1..])
    } else {
        (1.0, s)
    };
    let v: f64 = rest.trim().replace(',', ".").parse().ok()?;
    v.is_finite().then_some(sign * v)
}

/// Écrit une latitude au format GEDCOM 7 (`N45.76404`).
pub fn format_latitude(v: f64) -> String {
    format_coord(v, 'N', 'S')
}

/// Écrit une longitude au format GEDCOM 7 (`E4.83566`).
pub fn format_longitude(v: f64) -> String {
    format_coord(v, 'E', 'W')
}

fn format_coord(v: f64, pos: char, neg: char) -> String {
    let c = if v < 0.0 { neg } else { pos };
    // 5 décimales ≈ 1 m : bien assez pour une commune ou un lieu-dit.
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
    /// Coordonnées `(latitude, longitude)` en degrés, si elles sont lisibles.
    pub fn coords(&self) -> Option<(f64, f64)> {
        Some((parse_latitude(self.latitude.as_deref()?)?, parse_longitude(self.longitude.as_deref()?)?))
    }

    /// Requêtes de géocodage à essayer, de la plus précise à la plus large.
    ///
    /// Au format Ancestris (`Lieudit, Commune, Code_Postal, Département, Région, Pays`),
    /// les codes (postal, INSEE) gênent la recherche plein texte : on les retire.
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
            // Sans le lieu-dit, puis commune + pays seulement.
            push(vec![at(1), at(3), country]);
            push(vec![at(1), country]);
        } else if raw.len() >= 2 {
            push(vec![raw[0], raw[raw.len() - 1]]);
        }
        out
    }
}

/// Taille d'une tuile de carte, en pixels.
pub const TILE_SIZE: f64 = 256.0;

/// Projection Web Mercator : position « monde » au zoom 0 (0‥256 sur chaque axe).
pub fn project(lat: f64, lon: f64) -> (f64, f64) {
    let lat = lat.clamp(-85.05112878, 85.05112878).to_radians();
    let x = (lon + 180.0) / 360.0 * TILE_SIZE;
    let y = (1.0 - (lat.tan() + 1.0 / lat.cos()).ln() / std::f64::consts::PI) / 2.0 * TILE_SIZE;
    (x, y)
}

/// Inverse de [`project`] : `(latitude, longitude)` en degrés.
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
