//! Where people come from, grouped for the colours: the broadest level of the places that tells
//! the people apart (countries, then regions…).

use crate::palette::{GROUP_COLORS, Palette};
use crate::scope::Member;
use gedcom::document::Gedcom;
use gedcom::model::Individual;
use std::collections::HashMap;

/// The place that says where a person comes from: birth, baptism, then marriage or death.
pub fn origin_place(g: &Gedcom, ind: &Individual) -> Option<String> {
    let own = ["BIRT", "CHR", "BAPM"].iter().filter_map(|t| ind.event(t)).find_map(|e| e.place().map(str::to_string));
    own.or_else(|| {
        ind.fams()
            .into_iter()
            .filter_map(|f| g.family(f))
            .find_map(|f| f.marriage().and_then(|m| m.place().map(str::to_string)))
    })
    .or_else(|| ["DEAT", "BURI"].iter().filter_map(|t| ind.event(t)).find_map(|e| e.place().map(str::to_string)))
}

/// Jurisdictions of a place from the broadest (country) to the most precise, empty ones kept so
/// that the levels stay aligned (“, Troyes, 10000, Aube, Grand Est, France”).
fn levels(place: &str) -> Vec<String> {
    // Postcodes and other codes name no region: a legend shows names.
    place
        .split(',')
        .rev()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.chars().any(|c| c.is_ascii_digit()) || s.chars().any(char::is_alphabetic))
        .collect()
}

/// Groups of places for the colours: the broadest level where no group holds more than 70 % of
/// the people (countries, then regions…), the six largest groups and “Other” (`other`).
/// Returns the group of each place (`usize::MAX`: other), the groups with their counts, and how
/// many groups are named.
pub fn place_groups(places: &[String], other: &str) -> (HashMap<String, usize>, Vec<(String, usize)>, usize) {
    let at = |p: &String, level: usize| -> Option<String> {
        let l = levels(p);
        l.get(level..)
            .and_then(|rest| rest.iter().find(|s| !s.is_empty()))
            .cloned()
            .or_else(|| l.iter().rev().find(|s| !s.is_empty()).cloned())
    };
    let mut level = 0;
    for candidate in 0..4 {
        level = candidate;
        let mut counts: HashMap<String, usize> = HashMap::new();
        for p in places {
            if let Some(v) = at(p, candidate) {
                *counts.entry(v).or_default() += 1;
            }
        }
        let top = counts.values().max().copied().unwrap_or(0);
        if counts.len() >= 3 && (top as f64) < 0.7 * places.len() as f64 {
            break;
        }
    }
    let mut counts: HashMap<String, usize> = HashMap::new();
    for p in places {
        if let Some(v) = at(p, level) {
            *counts.entry(v).or_default() += 1;
        }
    }
    let mut ranked: Vec<(String, usize)> = counts.into_iter().collect();
    ranked.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    let keep = if ranked.len() > GROUP_COLORS.len() { GROUP_COLORS.len() - 1 } else { ranked.len() };
    let mut group_of = HashMap::new();
    for p in places {
        let g = at(p, level).and_then(|v| ranked.iter().take(keep).position(|(n, _)| *n == v)).unwrap_or(usize::MAX);
        group_of.insert(p.clone(), g);
    }
    let mut groups: Vec<(String, usize)> = ranked.iter().take(keep).cloned().collect();
    let rest: usize = ranked.iter().skip(keep).map(|(_, n)| n).sum();
    if rest > 0 {
        groups.push((other.to_string(), rest));
    }
    (group_of, groups, keep)
}

/// The place each member comes from, and its group.
pub struct Origins {
    /// Place of each person (by id).
    pub place: HashMap<String, String>,
    /// Every place, one per person who has one (for the counts).
    pub all: Vec<String>,
    group_of: HashMap<String, usize>,
    /// The groups, with how many people each holds (the last may be “Other”).
    pub groups: Vec<(String, usize)>,
    /// Number of named groups.
    pub keep: usize,
}

impl Origins {
    pub fn new(g: &Gedcom, members: &[Member]) -> Origins {
        let place: HashMap<String, String> =
            members.iter().filter_map(|m| Some((m.xref.clone(), origin_place(g, &g.individual(&m.xref)?)?))).collect();
        let all: Vec<String> = members.iter().filter_map(|m| place.get(&m.xref).cloned()).collect();
        let (group_of, groups, keep) = place_groups(&all, &t!("viz.other"));
        Origins { place, all, group_of, groups, keep }
    }

    /// Group of a person: `None` when their place is unknown, `usize::MAX` for “Other”.
    pub fn group(&self, xref: &str) -> Option<usize> {
        self.place.get(xref).map(|p| *self.group_of.get(p).unwrap_or(&usize::MAX))
    }

    /// Colour of a person, `None` when their place is unknown.
    pub fn color(&self, xref: &str, palette: &Palette) -> Option<String> {
        self.group(xref).map(|i| palette.group(i, self.keep).to_string())
    }

    /// Name of a person's group (“Grand Est”, “Other”).
    pub fn group_label(&self, xref: &str) -> Option<String> {
        let i = self.group(xref)?;
        let i = if i < self.keep { i } else { self.groups.len().checked_sub(1)? };
        self.groups.get(i).map(|(n, _)| n.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn groups_places_at_the_level_that_tells_them_apart() {
        let places: Vec<String> = [
            ", Troyes, 10000, Aube, Grand Est, France",
            ", Piney, 10220, Aube, Grand Est, France",
            ", Saint-Étienne, 42100, Loire, Auvergne-Rhône-Alpes, France",
            ", Médéa, 26000, , Médéa, Algérie",
            ", Paris, 75000, Paris, Île-de-France, France",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        let (of, groups, _) = place_groups(&places, "Other");
        // Countries: France holds 80 %, so the regions are used.
        assert_eq!(groups[0], ("Grand Est".to_string(), 2));
        assert_eq!(groups.len(), 4);
        assert_eq!(of[&places[0]], of[&places[1]]);
        assert_ne!(of[&places[0]], of[&places[3]]);
    }
}
