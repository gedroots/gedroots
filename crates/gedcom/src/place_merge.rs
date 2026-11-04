//! Places written twice, and merging them.
//!
//! A GEDCOM place is not a record: it is the text of the `PLAC` tags, one level per jurisdiction
//! of the file's format (`HEAD.PLAC.FORM`). Two spellings of the same place are found by
//! [`Gedcom::place_duplicates`]:
//!
//! 1. **Variants**: the same name once case, accents, hyphens, “St” and codes are put aside, or a
//!    shortened form of exactly one other place (“Unias” for “Unias, Loire, …, France”).
//! 2. **Former or longer names**: the lowest level of one is the first words of the other’s
//!    (“Leudon” and “Leudon-en-Brie”), under the same higher level (the same department, or the
//!    same municipality for two localities), with the same code (postal, INSEE) or coordinates
//!    less than [`NEAR_KM`] apart. Coordinates more than [`FAR_KM`] apart rule the pair out.
//!
//! A pair the user answered **Not a Duplicate** is written in the header (`_GR_DISTINCT`, see
//! [`DISTINCT_TAG`]) and never suggested again.
//!
//! [`Gedcom::place_merge_plan`] compares two places level by level: identical levels stay, a level
//! present on one side only is added, and two different values are a conflict the user settles.
//! [`Gedcom::merge_places`] then rewrites every `PLAC` of both places, and of their localities,
//! with the result; the other substructures of each `PLAC` (notes, `EXID`, translations) are kept.

use crate::document::Gedcom;
use crate::duplicates::{DISTINCT_TAG, DISTINCT_URI, Level};
use crate::geo::{parse_latitude, parse_longitude};
use crate::locate::distance_km;
use crate::merge::Side;
use crate::model::{Place, sort_key};
use crate::node::Node;
use crate::quality::{abbreviates, place_parts};
use rust_i18n::t;
use std::collections::HashMap;

/// Coordinates closer than this (km) back a pair of similar names.
pub const NEAR_KM: f64 = 2.0;
/// Coordinates further apart than this (km) are two different places.
pub const FAR_KM: f64 = 5.0;
/// Two coordinates closer than this (km) are the same point: no choice to make.
pub const SAME_POINT_KM: f64 = 0.2;

/// Two places that could be the same.
#[derive(Debug, Clone, PartialEq)]
pub struct PlaceDuplicate {
    /// The place that seems to be `b` (variant, shorter or former name).
    pub a: String,
    pub b: String,
    pub level: Level,
    /// For the pair, in words, strongest first.
    pub clues: Vec<String>,
}

/// Levels of a place name, at their position (empty ones included).
pub fn levels(name: &str) -> Vec<String> {
    name.split(',').map(|s| s.trim().to_string()).collect()
}

fn is_code(s: &str) -> bool {
    !s.is_empty() && s.chars().all(|c| c.is_ascii_digit() || c == ' ')
}

/// A name to compare: lower case, without accents, punctuation as spaces, “St” expanded.
fn words(s: &str) -> Vec<String> {
    let clean: String = sort_key(s).chars().map(|c| if c.is_alphanumeric() { c } else { ' ' }).collect();
    clean
        .split_whitespace()
        .map(|w| match w {
            "st" => "saint".to_string(),
            "ste" => "sainte".to_string(),
            w => w.to_string(),
        })
        .collect()
}

/// Position of the lowest non-empty level.
fn lowest(l: &[String]) -> Option<usize> {
    l.iter().position(|x| !x.is_empty())
}

/// The first higher level that is not a code, with its position.
fn parent(l: &[String], low: usize) -> Option<&str> {
    l[low + 1..].iter().find(|x| !x.is_empty() && !is_code(x)).map(String::as_str)
}

fn codes(l: &[String]) -> Vec<&str> {
    l.iter().filter(|x| is_code(x)).map(String::as_str).collect()
}

fn km(a: &Place, b: &Place) -> Option<f64> {
    Some(distance_km(a.coords()?, b.coords()?))
}

/// Is `short` the first words of `long`, at a word boundary?
fn word_prefix(short: &[String], long: &[String]) -> bool {
    !short.is_empty() && short.len() < long.len() && long.starts_with(short)
}

/// Distance in words, with a comma as decimal separator in French.
fn km_text(d: f64) -> String {
    let s = format!("{d:.1}");
    if rust_i18n::locale().starts_with("fr") { s.replace('.', ",") } else { s }
}

impl Gedcom {
    /// Places that could be written twice, without the pairs marked as different.
    pub fn place_duplicates(&self) -> Vec<PlaceDuplicate> {
        let all = self.places();
        let distinct = self.distinct_places();
        let marked = |a: &str, b: &str| distinct.iter().any(|(x, y)| (x == a && y == b) || (x == b && y == a));
        let mut out: Vec<PlaceDuplicate> = Vec::new();
        let mut paired: std::collections::HashSet<(usize, usize)> = Default::default();
        let mut push = |out: &mut Vec<PlaceDuplicate>, i: usize, j: usize, level: Level, clues: Vec<String>| {
            let key = (i.min(j), i.max(j));
            if i != j && !marked(&all[i].name, &all[j].name) && paired.insert(key) {
                out.push(PlaceDuplicate { a: all[i].name.clone(), b: all[j].name.clone(), level, clues });
            }
        };

        // 1. Variants: same name once normalized, or abbreviated form of a single other place.
        // Each variant is matched with the most used place of its group.
        let parts: Vec<Vec<String>> = all.iter().map(|p| place_parts(&p.name)).collect();
        let better = |a: &Place, b: &Place| {
            (a.uses, a.coords().is_some(), a.name.len()) > (b.uses, b.coords().is_some(), b.name.len())
        };
        let mut groups: HashMap<&[String], Vec<usize>> = HashMap::new();
        for (i, k) in parts.iter().enumerate().filter(|(_, k)| !k.is_empty()) {
            groups.entry(k.as_slice()).or_default().push(i);
        }
        let mut target: Vec<Option<usize>> = vec![None; all.len()];
        let mut keys: Vec<&&[String]> = groups.keys().collect();
        keys.sort();
        for key in keys {
            let members = &groups[*key];
            if members.len() < 2 {
                continue;
            }
            let best = *members.iter().reduce(|a, b| if better(&all[*b], &all[*a]) { b } else { a }).unwrap();
            for &m in members.iter().filter(|&&m| m != best) {
                target[m] = Some(best);
            }
        }
        for i in 0..all.len() {
            if target[i].is_some() || parts[i].is_empty() {
                continue;
            }
            let longer: Vec<usize> =
                (0..all.len()).filter(|&j| target[j].is_none() && abbreviates(&parts[i], &parts[j])).collect();
            // Several possible places (Saint-Martin, Isère / Saint-Martin, Gard): do not guess.
            if let [j] = longer[..] {
                target[i] = Some(j);
            }
        }
        for (i, t) in target.iter().enumerate() {
            let Some(j) = *t else { continue };
            if km(&all[i], &all[j]).is_some_and(|d| d > FAR_KM) {
                continue;
            }
            let clue = if parts[i].len() == parts[j].len() {
                t!("place_merge.clue_spelling").into_owned()
            } else {
                t!("place_merge.clue_shortened").into_owned()
            };
            push(&mut out, i, j, Level::Likely, vec![clue]);
        }

        // 2. Former or longer names, under the same higher level.
        let lv: Vec<Vec<String>> = all.iter().map(|p| levels(&p.name)).collect();
        let mut by_parent: HashMap<(usize, usize, String), Vec<usize>> = HashMap::new();
        for (i, l) in lv.iter().enumerate() {
            let Some(low) = lowest(l) else { continue };
            let Some(up) = parent(l, low) else { continue };
            by_parent.entry((l.len(), low, sort_key(up))).or_default().push(i);
        }
        let mut keys: Vec<_> = by_parent.keys().cloned().collect();
        keys.sort();
        for key in keys {
            let members = &by_parent[&key];
            let low = key.1;
            for (n, &i) in members.iter().enumerate() {
                for &j in &members[n + 1..] {
                    let (wi, wj) = (words(&lv[i][low]), words(&lv[j][low]));
                    let (short, long) = if word_prefix(&wi, &wj) {
                        (i, j)
                    } else if word_prefix(&wj, &wi) {
                        (j, i)
                    } else {
                        continue;
                    };
                    let d = km(&all[i], &all[j]);
                    if d.is_some_and(|d| d > FAR_KM) {
                        continue;
                    }
                    let (ci, cj) = (codes(&lv[i]), codes(&lv[j]));
                    let same_code = !ci.is_empty() && ci == cj;
                    let near = d.is_some_and(|d| d < NEAR_KM);
                    if !same_code && !near {
                        continue;
                    }
                    let mut clues =
                        vec![t!("place_merge.clue_prefix", a = lv[short][low], b = lv[long][low]).into_owned()];
                    if same_code {
                        clues.push(t!("place_merge.clue_same_code", code = ci.join(" ")).into_owned());
                    }
                    match d {
                        Some(d) if d < SAME_POINT_KM => clues.push(t!("place_merge.clue_same_point").into_owned()),
                        Some(d) if d < NEAR_KM => clues.push(t!("place_merge.clue_near", km = km_text(d)).into_owned()),
                        _ => {}
                    }
                    let level = if same_code && near { Level::Likely } else { Level::Possible };
                    push(&mut out, short, long, level, clues);
                }
            }
        }
        out
    }

    /// The possible duplicates of one place.
    pub fn place_duplicates_of(&self, name: &str) -> Vec<PlaceDuplicate> {
        self.place_duplicates().into_iter().filter(|d| d.a == name || d.b == name).collect()
    }

    /// Pairs of places marked as different (`HEAD._GR_DISTINCT` + `PLAC`).
    pub fn distinct_places(&self) -> Vec<(String, String)> {
        self.header
            .children_with(DISTINCT_TAG)
            .flat_map(|d| d.children_with("PLAC").map(move |p| (d.val().to_string(), p.val().to_string())))
            .collect()
    }

    /// Not a Duplicate: `a` and `b` are two different places.
    pub fn mark_places_distinct(&mut self, a: &str, b: &str) -> bool {
        let (a, b) = (a.trim(), b.trim());
        if a.is_empty() || b.is_empty() || a == b || self.distinct_places().iter().any(|p| *p == (a.into(), b.into())) {
            return false;
        }
        self.declare_extension(DISTINCT_TAG, DISTINCT_URI);
        let i = self.header.children.iter().position(|c| c.tag == DISTINCT_TAG && c.val() == a);
        let mark = match i {
            Some(i) => &mut self.header.children[i],
            None => {
                self.header.children.push(Node::with_value(DISTINCT_TAG, a));
                self.header.children.last_mut().unwrap()
            }
        };
        mark.children.push(Node::with_value("PLAC", b));
        true
    }

    /// Removes the “not a duplicate” mark between two places, in both directions.
    pub fn unmark_places_distinct(&mut self, a: &str, b: &str) -> bool {
        let mut done = false;
        for d in self.header.children.iter_mut().filter(|c| c.tag == DISTINCT_TAG) {
            let before = d.children.len();
            let v = d.val().to_string();
            d.children.retain(|p| !(p.tag == "PLAC" && ((v == a && p.val() == b) || (v == b && p.val() == a))));
            done |= d.children.len() != before;
        }
        self.header.children.retain(|c| c.tag != DISTINCT_TAG || !c.children.is_empty());
        done
    }

    /// Follows a renamed place in the “not a duplicate” marks (a pair that became one place goes).
    pub(crate) fn rename_in_distinct(&mut self, renamed: &[(&str, &str)]) {
        let to = |v: &str| renamed.iter().find(|(from, _)| *from == v).map_or(v.to_string(), |(_, t)| t.to_string());
        let pairs: Vec<(String, String)> = self.distinct_places();
        if pairs.is_empty() {
            return;
        }
        let new: Vec<(String, String)> = pairs.iter().map(|(a, b)| (to(a), to(b))).filter(|(a, b)| a != b).collect();
        if new == pairs {
            return;
        }
        self.header.children.retain(|c| c.tag != DISTINCT_TAG);
        for (a, b) in new {
            self.mark_places_distinct(&a, &b);
        }
    }

    /// Compares two places level by level, `a` in the first column.
    pub fn place_merge_plan(&self, a: &str, b: &str) -> Option<PlacePlan> {
        if a == b {
            return None;
        }
        let places = self.places();
        let pa = places.iter().find(|p| p.name == a)?;
        let pb = places.iter().find(|p| p.name == b)?;
        let (la, lb) = (levels(a), levels(b));
        let pairs = align(&la, &lb);
        let form = self.place_form();
        let mut conflicts = 0;
        let rows: Vec<LevelRow> = pairs
            .into_iter()
            .enumerate()
            .map(|(i, (x, y))| {
                let label = form
                    .get(i)
                    .filter(|_| form.len() == la.len().max(lb.len()))
                    .map(|f| f.replace('_', " "))
                    .unwrap_or_else(|| t!("place_merge.level", n = i + 1).into_owned());
                let kind = if x == y {
                    RowKind::Same
                } else if x.is_empty() || y.is_empty() {
                    RowKind::Added
                } else {
                    conflicts += 1;
                    RowKind::Conflict(conflicts - 1)
                };
                LevelRow { label, a: x, b: y, kind }
            })
            .collect();
        let coords = match (pa.coords(), pb.coords()) {
            (Some(x), Some(y)) if distance_km(x, y) >= SAME_POINT_KM => {
                conflicts += 1;
                Coords::Conflict(conflicts - 1, distance_km(x, y))
            }
            (Some(_), Some(_)) => Coords::Same,
            (Some(_), None) => Coords::FromA,
            (None, Some(_)) => Coords::FromB,
            (None, None) => Coords::None,
        };
        let localities = |name: &str| -> Vec<Locality> {
            let l = levels(name);
            let Some(low) = lowest(&l) else { return vec![] };
            places
                .iter()
                .filter(|p| {
                    let pl = levels(&p.name);
                    pl.len() == l.len()
                        && lowest(&pl).is_some_and(|x| x < low)
                        && pl[low] == l[low]
                        && parent(&pl, low).map(sort_key) == parent(&l, low).map(sort_key)
                })
                .map(|p| (p.name.clone(), p.uses))
                .collect()
        };
        Some(PlacePlan {
            a: a.to_string(),
            b: b.to_string(),
            uses: (pa.uses, pb.uses),
            records: (pa.records.len(), pb.records.len()),
            coords_a: pa.latitude.clone().zip(pa.longitude.clone()),
            coords_b: pb.latitude.clone().zip(pb.longitude.clone()),
            rows,
            coords,
            localities: (localities(a), localities(b)),
            conflicts,
        })
    }

    /// Merges the two places of `plan`, with one choice per conflict. Returns the resulting name,
    /// or `None` if a choice is missing or the places changed.
    pub fn merge_places(&mut self, plan: &PlacePlan, choices: &[Side]) -> Option<String> {
        if choices.len() != plan.conflicts {
            return None;
        }
        let names: Vec<String> = self.places().into_iter().map(|p| p.name).collect();
        if !names.contains(&plan.a) || !names.contains(&plan.b) {
            return None;
        }
        let result = plan.result(choices);
        let coords = plan.coords_of(choices);
        // Localities follow their municipality: their own levels stay, the rest becomes the result.
        let result_levels = levels(&result);
        let mut renames: Vec<(String, String)> =
            vec![(plan.a.clone(), result.clone()), (plan.b.clone(), result.clone())];
        for (loc, _) in plan.localities.0.iter().chain(&plan.localities.1) {
            let mut l = levels(loc);
            let low = lowest(&levels(&plan.a)).or(lowest(&levels(&plan.b))).unwrap_or(0);
            if l.len() == result_levels.len() {
                l.splice(low.., result_levels[low..].iter().cloned());
            }
            renames.push((loc.clone(), l.join(", ")));
        }
        let (a, b) = (plan.a.clone(), plan.b.clone());
        let touched = |n: &Node| n.any(&|c| c.tag == "PLAC" && renames.iter().any(|(f, _)| c.val() == f));
        let renames_ref = &renames;
        for r in self.records_where_mut(touched) {
            r.walk_mut(&mut |n| {
                if n.tag != "PLAC" {
                    return;
                }
                let Some((from, to)) = renames_ref.iter().find(|(f, _)| n.val() == f) else { return };
                let own = *from == a || *from == b;
                n.value = Some(to.clone());
                if own {
                    match &coords {
                        Some((lat, long)) => {
                            let map = n.ensure_child("MAP");
                            map.set_child_val("LATI", lat);
                            map.set_child_val("LONG", long);
                        }
                        None => n.children.retain(|c| c.tag != "MAP"),
                    }
                }
            });
            r.touch();
        }
        let pairs: Vec<(&str, &str)> = renames.iter().map(|(f, t)| (f.as_str(), t.as_str())).collect();
        self.rename_in_distinct(&pairs);
        Some(result)
    }
}

/// Lines up the levels of two places: position by position when they have as many, otherwise
/// the parts of the shorter where they appear in the longer (“Unias” in “Unias, Loire, …”).
fn align(a: &[String], b: &[String]) -> Vec<(String, String)> {
    if a.len() == b.len() {
        return a.iter().cloned().zip(b.iter().cloned()).collect();
    }
    let (short, long, flip) = if a.len() < b.len() { (a, b, false) } else { (b, a, true) };
    let mut placed: Vec<String> = vec![String::new(); long.len()];
    let mut at = 0;
    let mut ok = true;
    for s in short.iter().filter(|s| !s.is_empty()) {
        match (at..long.len()).find(|&k| words(&long[k]) == words(s)) {
            Some(k) => {
                placed[k] = s.clone();
                at = k + 1;
            }
            None => {
                ok = false;
                break;
            }
        }
    }
    if !ok {
        placed = (0..long.len()).map(|k| short.get(k).cloned().unwrap_or_default()).collect();
    }
    placed.into_iter().zip(long.iter().cloned()).map(|(s, l)| if flip { (l, s) } else { (s, l) }).collect()
}

/// What a line of the comparison does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RowKind {
    Same,
    /// Empty on one side: the other value is kept.
    Added,
    /// Two values: index of the conflict.
    Conflict(usize),
}

/// One level of the two places.
#[derive(Debug, Clone, PartialEq)]
pub struct LevelRow {
    /// Name of the level in the file's format (“Commune”), or “Level 2”.
    pub label: String,
    pub a: String,
    pub b: String,
    pub kind: RowKind,
}

/// The coordinates of the two places.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Coords {
    None,
    Same,
    FromA,
    FromB,
    /// Index of the conflict, and the distance in km.
    Conflict(usize, f64),
}

/// A locality of a place: its full name and how often it is used.
pub type Locality = (String, usize);

/// Comparison of two places, level by level.
#[derive(Debug, Clone, PartialEq)]
pub struct PlacePlan {
    pub a: String,
    pub b: String,
    /// Occurrences of each place.
    pub uses: (usize, usize),
    /// Records citing each place.
    pub records: (usize, usize),
    pub coords_a: Option<(String, String)>,
    pub coords_b: Option<(String, String)>,
    pub rows: Vec<LevelRow>,
    pub coords: Coords,
    /// Localities of each place (name, occurrences): they follow the merge.
    pub localities: (Vec<Locality>, Vec<Locality>),
    /// Number of choices to make (levels, then coordinates).
    pub conflicts: usize,
}

/// A choice to make between two places: the level (its label, or `MAP` for the coordinates) and
/// the value of each place.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlaceConflict {
    pub fact: String,
    pub a: String,
    pub b: String,
}

impl PlaceConflict {
    /// Is it the level named `fact`, whatever its case (“RÉGION” for “Région”)?
    pub fn is(&self, fact: &str) -> bool {
        self.fact.to_lowercase() == fact.trim().to_lowercase()
    }
}

/// Key of the coordinates among the choices of a place merge.
pub const COORDINATES: &str = "MAP";

impl PlacePlan {
    /// The choices to make, in order: the levels that differ, then the coordinates.
    pub fn choices(&self) -> Vec<PlaceConflict> {
        let mut out: Vec<(usize, PlaceConflict)> = self
            .rows
            .iter()
            .filter_map(|r| match r.kind {
                RowKind::Conflict(c) => {
                    Some((c, PlaceConflict { fact: r.label.clone(), a: r.a.clone(), b: r.b.clone() }))
                }
                _ => None,
            })
            .collect();
        if let Coords::Conflict(c, _) = self.coords {
            let text = |side| self.coords_text(side).unwrap_or_default();
            out.push((c, PlaceConflict { fact: COORDINATES.into(), a: text(Side::Keep), b: text(Side::Other) }));
        }
        out.sort_by_key(|(c, _)| *c);
        out.into_iter().map(|(_, c)| c).collect()
    }

    /// The merged name once every choice is made.
    pub fn merged_name(&self, choices: &[Side]) -> String {
        self.result(choices)
    }

    /// The merged name; a conflict not chosen yet (`None`) shows `missing`.
    pub fn preview_with(&self, choices: &[Option<Side>], missing: &str) -> String {
        self.rows
            .iter()
            .map(|r| match r.kind {
                RowKind::Same => r.a.clone(),
                RowKind::Added => {
                    if r.a.is_empty() {
                        r.b.clone()
                    } else {
                        r.a.clone()
                    }
                }
                RowKind::Conflict(c) => match choices.get(c).copied().flatten() {
                    Some(Side::Keep) => r.a.clone(),
                    Some(Side::Other) => r.b.clone(),
                    None => missing.to_string(),
                },
            })
            .collect::<Vec<_>>()
            .join(", ")
    }

    /// The merged name, with an empty level for a conflict not chosen yet.
    pub fn preview(&self, choices: &[Option<Side>]) -> String {
        self.preview_with(choices, "")
    }

    fn result(&self, choices: &[Side]) -> String {
        let c: Vec<Option<Side>> = choices.iter().copied().map(Some).collect();
        self.preview(&c)
    }

    /// Coordinates of the merged place.
    fn coords_of(&self, choices: &[Side]) -> Option<(String, String)> {
        match self.coords {
            Coords::None => None,
            Coords::Same | Coords::FromB => self.coords_b.clone().or(self.coords_a.clone()),
            Coords::FromA => self.coords_a.clone(),
            Coords::Conflict(c, _) => match choices.get(c) {
                Some(Side::Keep) => self.coords_a.clone(),
                _ => self.coords_b.clone(),
            },
        }
    }

    /// Coordinates shown for one side, in the file's notation.
    pub fn coords_text(&self, side: Side) -> Option<String> {
        let (lat, long) = match side {
            Side::Keep => self.coords_a.as_ref()?,
            Side::Other => self.coords_b.as_ref()?,
        };
        Some(match (parse_latitude(lat), parse_longitude(long)) {
            (Some(la), Some(lo)) => {
                format!("{} · {}", crate::geo::format_latitude(la), crate::geo::format_longitude(lo))
            }
            _ => format!("{lat} · {long}"),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn levels_line_up_by_position_or_by_name() {
        let s = |v: &[&str]| v.iter().map(|x| x.to_string()).collect::<Vec<_>>();
        assert_eq!(align(&s(&["a", "b"]), &s(&["a", "c"])), vec![("a".into(), "a".into()), ("b".into(), "c".into())]);
        let pairs = align(&s(&["Unias", "France"]), &s(&["Unias", "Loire", "France"]));
        assert_eq!(pairs[1], (String::new(), "Loire".to_string()));
        assert_eq!(pairs[2], ("France".to_string(), "France".to_string()));
    }

    #[test]
    fn word_prefixes() {
        assert!(word_prefix(&words("Leudon"), &words("Leudon-en-Brie")));
        assert!(!word_prefix(&words("Paris XVII"), &words("Paris XVIII")));
        assert!(!word_prefix(&words("Boisset"), &words("Boisset")));
    }
}
