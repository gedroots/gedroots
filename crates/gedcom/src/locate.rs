//! Locating a place an agent proposes: the located places of a person's nearby events, to tell
//! homonyms apart among a gazetteer's results (“Saint-Martin” in Manche, near the family, rather
//! than in Savoie).
//!
//! Nothing is decided here: the candidates come with their distance to these places, and the
//! agent chooses, the user reviewing its choice.

use crate::document::Gedcom;
use crate::gazetteer::{Candidate, join_parts};
use crate::model::{Event, Place, short_place};
use std::collections::HashMap;

/// Located place of an event close to the one being proposed.
#[derive(Debug, Clone, PartialEq)]
pub struct Nearby {
    /// The place, as the tree writes it (`PLAC`).
    pub place: String,
    pub latitude: f64,
    pub longitude: f64,
    /// “Birth”, “Marriage”…
    pub event: String,
    /// Whose event: the person's name.
    pub person: String,
    /// `self`, `father`, `mother`, `spouse`, `child`, `sibling`.
    pub relation: &'static str,
    pub year: Option<i32>,
}

/// Mean Earth radius, in kilometres.
const EARTH_KM: f64 = 6371.0;

/// Great-circle distance between two points, in kilometres.
pub fn distance_km(a: (f64, f64), b: (f64, f64)) -> f64 {
    let (la1, lo1, la2, lo2) = (a.0.to_radians(), a.1.to_radians(), b.0.to_radians(), b.1.to_radians());
    let h = ((la2 - la1) / 2.0).sin().powi(2) + la1.cos() * la2.cos() * ((lo2 - lo1) / 2.0).sin().powi(2);
    2.0 * EARTH_KM * h.sqrt().min(1.0).asin()
}

/// A gazetteer result with its distance to the nearby events.
#[derive(Debug, Clone, PartialEq)]
pub struct Ranked {
    pub candidate: Candidate,
    /// `PLAC` value in the file's place format.
    pub place: String,
    /// Distance to the closest nearby place, and that place's index in the list given.
    pub nearest: Option<(f64, usize)>,
}

impl Gedcom {
    /// Located places of the events of these people and of their close relatives (parents,
    /// spouses, children, siblings), the closest in time to `year` first. A place without
    /// coordinates is left out.
    pub fn nearby_places(&self, people: &[&str], year: Option<i32>, limit: usize) -> Vec<Nearby> {
        let located: HashMap<String, (f64, f64)> =
            self.places().into_iter().filter_map(|p| Some((p.name.clone(), p.coords()?))).collect();
        let mut seen: Vec<(String, &'static str)> = Vec::new();
        let mut out: Vec<Nearby> = Vec::new();
        let mut add = |who: &str, relation: &'static str, out: &mut Vec<Nearby>| {
            if seen.iter().any(|(x, _)| x == who) {
                return;
            }
            seen.push((who.to_string(), relation));
            let Some(ind) = self.individual(who) else { return };
            let name = ind.display_name();
            let mut events: Vec<Event> = ind.events();
            for f in ind.fams() {
                if let Some(fam) = self.family(f) {
                    events.extend(fam.events());
                }
            }
            for e in events {
                let Some(place) = e.place().filter(|p| !p.trim().is_empty()) else { continue };
                let Some(&(lat, lon)) = located.get(place) else { continue };
                out.push(Nearby {
                    place: place.to_string(),
                    latitude: lat,
                    longitude: lon,
                    event: e.label(),
                    person: name.clone(),
                    relation,
                    year: e.date().and_then(|d| d.year()),
                });
            }
        };
        for &x in people {
            add(x, "self", &mut out);
        }
        for &x in people {
            let (father, mother) = self.parents(x);
            if let Some(f) = father {
                add(f.xref(), "father", &mut out);
            }
            if let Some(m) = mother {
                add(m.xref(), "mother", &mut out);
            }
            for (spouse, children) in self.spouses_and_children(x) {
                if let Some(s) = spouse {
                    add(s.xref(), "spouse", &mut out);
                }
                for c in children {
                    add(c.xref(), "child", &mut out);
                }
            }
            let Some(ind) = self.individual(x) else { continue };
            for f in ind.famc() {
                for c in self.family(f).map(|f| f.children()).unwrap_or_default() {
                    add(c, "sibling", &mut out);
                }
            }
        }
        // The closest in time first; undated events last, the person's own before relatives'.
        let rank = |r: &str| ["self", "spouse", "father", "mother", "child", "sibling"].iter().position(|x| *x == r);
        out.sort_by_key(|n| (year.zip(n.year).map(|(a, b)| (a - b).abs()).unwrap_or(i32::MAX), rank(n.relation)));
        // One line per place: the event closest in time.
        let mut kept: Vec<Nearby> = Vec::new();
        for n in out {
            if !kept.iter().any(|k| k.place == n.place) {
                kept.push(n);
            }
        }
        kept.truncate(limit);
        kept
    }

    /// Places of the tree whose main name (locality or municipality) is the one searched:
    /// “Saint-Martin, Manche” finds every “Saint-Martin” of the file.
    pub fn places_named(&self, query: &str) -> Vec<Place> {
        let key = |s: &str| crate::proposal::loose(s);
        let Some(want) = query.split(',').map(str::trim).find(|s| !s.is_empty()).map(key) else {
            return Vec::new();
        };
        let mut found: Vec<Place> =
            self.places().into_iter().filter(|p| p.parts().iter().take(2).any(|part| key(part) == want)).collect();
        found.sort_by_key(|p| std::cmp::Reverse(p.uses));
        found
    }
}

/// Center of the nearby places closest in time (the first ones), to bias the gazetteer.
pub fn center(nearby: &[Nearby]) -> Option<(f64, f64)> {
    let first: Vec<&Nearby> = nearby.iter().take(5).collect();
    if first.is_empty() {
        return None;
    }
    let n = first.len() as f64;
    Some((first.iter().map(|p| p.latitude).sum::<f64>() / n, first.iter().map(|p| p.longitude).sum::<f64>() / n))
}

/// Each candidate with its place in the file's format and the closest nearby place, in the
/// gazetteer's order.
pub fn rank(candidates: Vec<Candidate>, nearby: &[Nearby], form: &[String]) -> Vec<Ranked> {
    candidates
        .into_iter()
        .map(|c| {
            let nearest = nearby
                .iter()
                .enumerate()
                .map(|(i, n)| (distance_km((c.lat, c.lon), (n.latitude, n.longitude)), i))
                .min_by(|a, b| a.0.total_cmp(&b.0));
            Ranked { place: join_parts(&c.parts(form)), candidate: c, nearest }
        })
        .collect()
}

/// “Birth of Jean MARCHAND, 1802, Agneaux (Manche)”.
pub fn describe(n: &Nearby) -> String {
    let year = n.year.map(|y| format!(", {y}")).unwrap_or_default();
    format!("{} of {}{year}, {}", n.event, n.person, short_place(&n.place))
}

#[cfg(test)]
mod tests {
    use super::*;

    const FILE: &str = "0 HEAD\n1 GEDC\n2 VERS 7.0\n\
0 @I1@ INDI\n1 NAME Jean /MARCHAND/\n1 BIRT\n2 DATE 1802\n2 PLAC Agneaux, Manche, France\n1 FAMC @F1@\n\
0 @I2@ INDI\n1 NAME Pierre /MARCHAND/\n1 DEAT\n2 DATE 1840\n2 PLAC Saint-Lô, Manche, France\n1 FAMS @F1@\n\
0 @I3@ INDI\n1 NAME Louis /MARCHAND/\n1 BIRT\n2 DATE 1700\n2 PLAC Chambéry, Savoie, France\n1 FAMC @F1@\n\
0 @F1@ FAM\n1 HUSB @I2@\n1 CHIL @I1@\n1 CHIL @I3@\n1 MARR\n2 DATE 1800\n2 PLAC Saint-Martin, Savoie, France\n\
0 @I9@ INDI\n1 NAME X /Y/\n1 RESI\n2 PLAC Saint-Martin, Manche, France\n\
0 TRLR\n";

    fn ged() -> Gedcom {
        let mut g = Gedcom::parse(FILE).unwrap();
        g.update_place("Agneaux, Manche, France", "Agneaux, Manche, France", "N49.1188", "W1.1061");
        g.update_place("Saint-Lô, Manche, France", "Saint-Lô, Manche, France", "N49.1157", "W1.0906");
        g.update_place("Chambéry, Savoie, France", "Chambéry, Savoie, France", "N45.5646", "E5.9178");
        g
    }

    #[test]
    fn distances() {
        let paris = (48.8566, 2.3522);
        let lyon = (45.764, 4.8357);
        assert!((distance_km(paris, lyon) - 392.0).abs() < 5.0);
        assert_eq!(distance_km(paris, paris), 0.0);
    }

    #[test]
    fn nearby_closest_in_time_first() {
        let g = ged();
        let near = g.nearby_places(&["I1"], Some(1805), 10);
        let places: Vec<(&str, &str)> = near.iter().map(|n| (n.place.as_str(), n.relation)).collect();
        // Own birth (1802), the father's death (1840), then a sibling born in 1700; the parents'
        // marriage place has no coordinates.
        assert_eq!(
            places,
            vec![
                ("Agneaux, Manche, France", "self"),
                ("Saint-Lô, Manche, France", "father"),
                ("Chambéry, Savoie, France", "sibling")
            ]
        );
        assert_eq!(describe(&near[0]), "Birth of Jean MARCHAND, 1802, Agneaux, Manche, France");
        let c = center(&near[..2]).unwrap();
        assert!((c.0 - 49.117).abs() < 0.01);
    }

    #[test]
    fn candidates_ranked_by_distance() {
        let g = ged();
        let near = g.nearby_places(&["I1"], Some(1805), 10);
        let cand = |name: &str, county: &str, lat: f64, lon: f64| Candidate {
            name: name.into(),
            lat,
            lon,
            address: vec![
                ("village".into(), name.into()),
                ("county".into(), county.into()),
                ("country".into(), "France".into()),
            ],
            exid: None,
        };
        let form = ["Commune", "Département", "Pays"].map(String::from).to_vec();
        let ranked = rank(
            vec![cand("Saint-Martin", "Savoie", 45.5, 6.4), cand("Saint-Martin", "Manche", 49.05, -1.2)],
            &near,
            &form,
        );
        assert_eq!(ranked[1].place, "Saint-Martin, Manche, France");
        let (km, i) = ranked[1].nearest.unwrap();
        assert!(km < 15.0, "{km}");
        assert_eq!(near[i].place, "Agneaux, Manche, France");
        // The Savoie homonym is near a sibling's birth a century earlier: the agent weighs it.
        let (km, i) = ranked[0].nearest.unwrap();
        assert!(km > 30.0, "{km}");
        assert_eq!(near[i].relation, "sibling");
    }

    #[test]
    fn places_of_the_same_name() {
        let g = ged();
        let found: Vec<String> = g.places_named("saint martin, Manche").into_iter().map(|p| p.name).collect();
        assert_eq!(found.len(), 2);
        assert!(found.contains(&"Saint-Martin, Savoie, France".to_string()));
        assert!(g.places_named("Lyon").is_empty());
    }
}
