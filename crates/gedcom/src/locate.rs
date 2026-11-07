//! Locating a place an agent proposes: the located places of a person's nearby events, to tell
//! homonyms apart among a gazetteer's results (“Saint-Martin” in Manche, near the family, rather
//! than in Savoie).
//!
//! Nothing is decided here: the candidates come with their distance to these places, and the
//! agent chooses, the user reviewing its choice.

use crate::document::Gedcom;
use crate::gazetteer::{Candidate, Level, join_parts};
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

/// How well a gazetteer result fits the act, for the agent to weigh (it decides, the user
/// reviews).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Fit {
    Unlikely,
    Possible,
    Likely,
}

impl Fit {
    pub fn as_str(self) -> &'static str {
        match self {
            Fit::Likely => "likely",
            Fit::Possible => "possible",
            Fit::Unlikely => "unlikely",
        }
    }
}

/// A gazetteer result with what speaks for and against it.
#[derive(Debug, Clone, PartialEq)]
pub struct Ranked {
    pub candidate: Candidate,
    /// `PLAC` value in the file's place format.
    pub place: String,
    /// Distance to the closest nearby place, and that place's index in the list given.
    pub nearest: Option<(f64, usize)>,
    /// Nearby places within [`CLOSE_KM`].
    pub close: usize,
    pub score: i32,
    pub fit: Fit,
    /// The reasons of the score, in English: “12 km from …”, “not in Manche (Calvados)”.
    pub why: Vec<String>,
}

/// A place this close to the family's events is in their neighbourhood (a day's walk).
pub const CLOSE_KM: f64 = 30.0;

/// Country of a place or a gazetteer result, by the Atlas when it knows the name.
fn country_key(name: &str) -> String {
    match crate::atlas::in_place(name).filter(|c| !c.is_generic()) {
        Some(c) => c.code.clone(),
        None => crate::proposal::loose(name.rsplit(',').next().unwrap_or(name)),
    }
}

/// Each candidate scored against the act: its distance to the nearby events (the closest in
/// time count most, as they come first), the jurisdictions the query names (“Saint-Martin,
/// Manche”), its level (a municipality, not a département, when the query names a town) and its
/// country (the family's). The best first; on a tie, the gazetteer's order.
pub fn rank(candidates: Vec<Candidate>, nearby: &[Nearby], form: &[String], query: &str) -> Vec<Ranked> {
    let loose = crate::proposal::loose;
    let mut parts = query.split(',').map(str::trim).filter(|s| !s.is_empty());
    let want_name = parts.next().map(loose).unwrap_or_default();
    // Jurisdictions named after the place, digits (postal codes) left out.
    let want: Vec<(String, String)> =
        parts.filter(|p| !p.chars().any(|c| c.is_ascii_digit())).map(|p| (p.to_string(), loose(p))).collect();
    // The family's country: the most frequent among the nearby places.
    let mut countries: Vec<(String, String, usize)> = Vec::new();
    for n in nearby {
        let name = n.place.rsplit(',').map(str::trim).find(|s| !s.is_empty()).unwrap_or("").to_string();
        if name.is_empty() {
            continue;
        }
        let key = country_key(&name);
        match countries.iter_mut().find(|(k, _, _)| *k == key) {
            Some(c) => c.2 += 1,
            None => countries.push((key, name, 1)),
        }
    }
    countries.sort_by_key(|c| std::cmp::Reverse(c.2));
    let family_country = countries.first().map(|(k, n, _)| (k.clone(), n.clone()));

    let mut out: Vec<(usize, Ranked)> = candidates
        .into_iter()
        .enumerate()
        .map(|(order, c)| {
            let mut score = 0;
            let mut why = Vec::new();
            let dist: Vec<(f64, usize)> = nearby
                .iter()
                .enumerate()
                .map(|(i, n)| (distance_km((c.lat, c.lon), (n.latitude, n.longitude)), i))
                .collect();
            let nearest = dist.iter().copied().min_by(|a, b| a.0.total_cmp(&b.0));
            let close = dist.iter().filter(|(d, _)| *d <= CLOSE_KM).count();
            if let Some((km, i)) = nearest {
                let n = &nearby[i];
                // The first nearby places are the closest in time.
                let recent = i < 5;
                score += match km {
                    k if k <= 10.0 => 3,
                    k if k <= CLOSE_KM => 2,
                    k if k <= 100.0 => 1,
                    k if k > 300.0 => -2,
                    _ => 0,
                } + i32::from(recent && km <= CLOSE_KM);
                why.push(format!("{} km from {}", km.round(), describe(n)));
                if nearby.len() > 1 {
                    why.push(format!(
                        "{close} of the {} located places of the family within {CLOSE_KM} km",
                        nearby.len()
                    ));
                }
            }
            let values: Vec<String> = c.address.iter().map(|(_, v)| loose(v)).collect();
            for (label, key) in &want {
                if values.iter().any(|v| v == key) {
                    score += 2;
                    why.push(format!("in {label}, as the query says"));
                } else {
                    score -= 2;
                    let theirs = c.level(Level::County).or(c.level(Level::State)).unwrap_or("elsewhere");
                    why.push(format!("not in {label} ({theirs})"));
                }
            }
            let level = c.own_level();
            if matches!(level, Level::County | Level::State | Level::Country) {
                score -= 2;
                let what = match level {
                    Level::County => "a county or département",
                    Level::State => "a region or state",
                    _ => "a country",
                };
                why.push(format!("{what}, not a municipality"));
            }
            if !want_name.is_empty() && loose(&c.name) == want_name {
                score += 1;
            } else if !want_name.is_empty() {
                why.push(format!("another name: {}", c.name));
            }
            if let (Some((key, name)), Some(theirs)) = (&family_country, c.level(Level::Country)) {
                if country_key(theirs) == *key {
                    score += 1;
                } else {
                    score -= 1;
                    why.push(format!("in {theirs}, the family's places are in {name}"));
                }
            }
            let fit = match score {
                s if s >= 4 => Fit::Likely,
                s if s >= 1 => Fit::Possible,
                _ => Fit::Unlikely,
            };
            let place = join_parts(&c.parts(form));
            (order, Ranked { candidate: c, place, nearest, close, score, fit, why })
        })
        .collect();
    out.sort_by_key(|(order, r)| (std::cmp::Reverse(r.score), *order));
    out.into_iter().map(|(_, r)| r).collect()
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

/// “12 km from Agneaux (Manche): Birth of Jean MARCHAND, 1802”, for the review.
pub fn near_label(km: f64, n: &Nearby) -> String {
    // Whole kilometres: no decimal separator to localize, and finer means nothing here.
    let km = km.round().max(1.0) as i64;
    let place = short_place(&n.place);
    match n.year {
        Some(y) => t!("locate.near", km = km, place = place, event = n.event, person = n.person, year = y),
        None => t!("locate.near_undated", km = km, place = place, event = n.event, person = n.person),
    }
    .into_owned()
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
            "Saint-Martin",
        );
        // The homonym near the person's own birth comes first, though the gazetteer gave it second.
        assert_eq!(ranked[0].place, "Saint-Martin, Manche, France");
        let (km, i) = ranked[0].nearest.unwrap();
        assert!(km < 15.0, "{km}");
        assert_eq!(near[i].place, "Agneaux, Manche, France");
        assert_eq!(ranked[0].fit, Fit::Likely);
        assert!(ranked[0].why[0].contains("km from Birth of Jean MARCHAND, 1802"), "{:?}", ranked[0].why);
        // The Savoie homonym is near a sibling's birth a century earlier: only possible.
        let (km, i) = ranked[1].nearest.unwrap();
        assert!(km > 30.0, "{km}");
        assert_eq!(near[i].relation, "sibling");
        assert_eq!(ranked[1].fit, Fit::Possible);

        // The jurisdiction the act names settles it.
        let ranked = rank(
            vec![cand("Saint-Martin", "Manche", 49.05, -1.2), cand("Saint-Martin", "Savoie", 45.5, 6.4)],
            &near,
            &form,
            "Saint-Martin, Savoie",
        );
        assert_eq!(ranked[0].place, "Saint-Martin, Savoie, France");
        assert!(ranked[1].why.iter().any(|w| w == "not in Savoie (Manche)"), "{:?}", ranked[1].why);
    }

    #[test]
    fn broader_jurisdictions_and_other_countries_rank_lower() {
        let g = ged();
        let near = g.nearby_places(&["I1"], Some(1805), 10);
        let form = ["Commune", "Département", "Pays"].map(String::from).to_vec();
        let county = Candidate {
            name: "Manche".into(),
            lat: 49.08,
            lon: -1.17,
            address: vec![("county".into(), "Manche".into()), ("country".into(), "France".into())],
            exid: None,
        };
        let abroad = Candidate {
            name: "Manche".into(),
            lat: 49.1,
            lon: -1.1,
            address: vec![("village".into(), "Manche".into()), ("country".into(), "Deutschland".into())],
            exid: None,
        };
        let ranked = rank(vec![county, abroad], &near, &form, "Manche");
        let c = ranked.iter().find(|r| r.candidate.own_level() == Level::County).unwrap();
        assert_eq!(c.place, ", Manche, France");
        assert!(c.why.iter().any(|w| w.contains("not a municipality")), "{:?}", c.why);
        let a = ranked.iter().find(|r| r.candidate.own_level() == Level::Municipality).unwrap();
        assert!(a.why.iter().any(|w| w == "in Deutschland, the family's places are in France"), "{:?}", a.why);
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
