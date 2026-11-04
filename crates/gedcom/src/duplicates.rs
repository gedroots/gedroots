//! Possible duplicate individuals: two records that could describe the same person.
//!
//! Most duplicates come from record entry: a witness or a parent named in two records is created
//! twice, with little more than a name, an occupation and the date of the record. So a person is
//! dated by everything that dates them (their events, the records they attended, their children's
//! births), not only by their birth and death.
//!
//! The search has three steps, like record linkage tools (Fellegi–Sunter, Splink), with fixed and
//! explained weights instead of trained ones:
//!
//! 1. **Blocking**: only people with the same phonetic key of surname and first given name are
//!    compared ([`crate::phonetic`]).
//! 2. **Exclusions**: never suggested if their sexes differ, if they are close relatives (direct
//!    line, siblings, uncle and nephew, first cousins, spouses), if they appear in the same event,
//!    if their lives cannot overlap, or if the user said they are different people
//!    ([`DISTINCT_TAG`]).
//! 3. **Clues**: each matching or differing detail adds or removes weight, with a sentence the user
//!    reads (“Same parents’ names”). The total gives a [`Level`].

use crate::analysis::kinship::blood_degrees;
use crate::analysis::span::Span;
use crate::document::{Gedcom, RecordKind};
use crate::model::{Individual, event_label, sort_key};
use crate::node::Node;
use crate::phonetic;
use std::collections::{BTreeSet, HashMap};

/// “Not a duplicate”: pointer from an individual to another individual that is a different person.
pub const DISTINCT_TAG: &str = "_GR_DISTINCT";
pub const DISTINCT_URI: &str = extension_uri!("_gr_distinct");

/// Score from which a pair is shown as “Possible”.
pub const POSSIBLE: f32 = 7.0;
/// Score from which a pair is shown as “Likely”.
pub const LIKELY: f32 = 10.0;

/// How likely two records describe the same person.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Level {
    Possible,
    Likely,
}

impl Level {
    pub fn label(self) -> std::borrow::Cow<'static, str> {
        match self {
            Level::Possible => t!("duplicates.level.possible"),
            Level::Likely => t!("duplicates.level.likely"),
        }
    }

    /// Code used by the AI agents' tools.
    pub fn id(self) -> &'static str {
        match self {
            Level::Possible => "possible",
            Level::Likely => "likely",
        }
    }
}

/// A detail for or against the pair, in words.
#[derive(Debug, Clone, PartialEq)]
pub struct Clue {
    pub weight: f32,
    pub text: String,
}

impl Clue {
    pub fn is_for(&self) -> bool {
        self.weight > 0.0
    }
}

/// Two individuals that could be the same person.
#[derive(Debug, Clone, PartialEq)]
pub struct Duplicate {
    pub a: String,
    pub b: String,
    pub score: f32,
    pub level: Level,
    /// Strongest first.
    pub clues: Vec<Clue>,
}

/// Why a pair cannot be a duplicate (also given to an AI agent that proposes a merge).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Exclusion {
    SamePerson,
    NotIndividuals,
    OppositeSex,
    CloseRelatives,
    SameEvent,
    LivesApart,
    MarkedDistinct,
}

impl Exclusion {
    pub fn label(self) -> std::borrow::Cow<'static, str> {
        match self {
            Exclusion::SamePerson => t!("duplicates.exclusion.same_person"),
            Exclusion::NotIndividuals => t!("duplicates.exclusion.not_individuals"),
            Exclusion::OppositeSex => t!("duplicates.exclusion.opposite_sex"),
            Exclusion::CloseRelatives => t!("duplicates.exclusion.close_relatives"),
            Exclusion::SameEvent => t!("duplicates.exclusion.same_event"),
            Exclusion::LivesApart => t!("duplicates.exclusion.lives_apart"),
            Exclusion::MarkedDistinct => t!("duplicates.exclusion.marked_distinct"),
        }
    }
}

/// Years of the earliest and latest possible birth (open bounds: `None`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
struct Window {
    lo: Option<i32>,
    hi: Option<i32>,
}

impl Window {
    fn raise(&mut self, y: i32) {
        self.lo = Some(self.lo.map_or(y, |l| l.max(y)));
    }
    fn lower(&mut self, y: i32) {
        self.hi = Some(self.hi.map_or(y, |h| h.min(y)));
    }
    fn overlaps(&self, o: &Window) -> bool {
        let lo = match (self.lo, o.lo) {
            (Some(a), Some(b)) => Some(a.max(b)),
            (a, b) => a.or(b),
        };
        let hi = match (self.hi, o.hi) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (a, b) => a.or(b),
        };
        !matches!((lo, hi), (Some(l), Some(h)) if l > h)
    }
}

/// An event that mentions the person: their own, or one they attended (`ASSO`).
#[derive(Debug, Clone)]
struct Seen {
    /// Record and index of the event, to recognise the same event.
    owner: String,
    index: usize,
    years: Option<(i32, i32)>,
    /// The person attended someone else's event.
    attended: bool,
}

/// What is compared, read once per individual.
#[derive(Debug, Clone, Default)]
struct Profile {
    surname: String,
    /// Given names, normalized (`jean baptiste`).
    given: Vec<String>,
    sex: String,
    birth: Option<(i32, i32)>,
    birth_date: String,
    birth_place: String,
    death: Option<(i32, i32)>,
    death_place: String,
    window: Window,
    famc: Vec<String>,
    fams: Vec<String>,
    father: Option<String>,
    mother: Option<String>,
    spouses: Vec<(String, String)>,
    children: BTreeSet<String>,
    places: BTreeSet<String>,
    occupations: BTreeSet<String>,
    sources: BTreeSet<String>,
    seen: Vec<Seen>,
    /// People whose events this person attended.
    attended_for: BTreeSet<String>,
}

/// First filled level of a place (“Lyon, Rhône, France” → `lyon`; “, Médéa, 26000” → `medea`).
fn place_key(p: &str) -> String {
    sort_key(first_level(p))
}

fn first_level(p: &str) -> &str {
    p.split(',').map(str::trim).find(|s| !s.is_empty()).unwrap_or("")
}

fn words(s: &str) -> Vec<String> {
    sort_key(s).split(|c: char| c.is_whitespace() || c == '-').filter(|w| !w.is_empty()).map(str::to_string).collect()
}

/// “Jean BLANC” as a comparable name: first given name and surname.
fn name_key(i: &Individual) -> String {
    format!("{} {}", words(&i.given()).first().cloned().unwrap_or_default(), sort_key(&i.surname()))
}

const SINGLE_BIRTH: &[&str] = &["BIRT", "CHR", "BAPM"];
const SINGLE_DEATH: &[&str] = &["DEAT", "BURI", "CREM"];

fn profiles(g: &Gedcom) -> HashMap<String, Profile> {
    let mut out: HashMap<String, Profile> = HashMap::new();
    let name_of = |x: &str| g.individual(x).map(|i| name_key(&i));
    for r in g.records_of(RecordKind::Individual) {
        let Some(x) = r.xref.clone() else { continue };
        let i = Individual::new(r);
        let mut p = Profile {
            surname: sort_key(&i.surname()),
            given: words(&i.given()),
            sex: i.sex().to_string(),
            famc: i.famc().iter().map(|s| s.to_string()).collect(),
            fams: i.fams().iter().map(|s| s.to_string()).collect(),
            ..Default::default()
        };
        for (index, ev) in r.children.iter().enumerate() {
            if event_label(&ev.tag).is_none() {
                continue;
            }
            let date = ev.path_val(&["DATE"]).unwrap_or("");
            let y = Span::years_of(date);
            let place = ev.path_val(&["PLAC"]).unwrap_or("").to_string();
            if SINGLE_BIRTH.contains(&ev.tag.as_str()) && p.birth.is_none() {
                p.birth = y;
                p.birth_date = date.to_string();
                p.birth_place = place_key(&place);
            } else if SINGLE_DEATH.contains(&ev.tag.as_str()) && p.death.is_none() {
                p.death = y;
                p.death_place = place_key(&place);
            }
            if ev.tag == "OCCU" && !ev.val().is_empty() {
                p.occupations.insert(sort_key(ev.val()));
            }
            if !place.is_empty() {
                p.places.insert(place_key(&place));
            }
            ev.walk(&mut |n| {
                if n.tag == "SOUR"
                    && let Some(s) = n.pointer()
                {
                    p.sources.insert(s.to_string());
                }
            });
            p.seen.push(Seen { owner: x.clone(), index, years: y, attended: false });
        }
        for f in &p.famc {
            if let Some(fam) = g.family(f) {
                p.father = p.father.clone().or_else(|| fam.husband().and_then(name_of));
                p.mother = p.mother.clone().or_else(|| fam.wife().and_then(name_of));
            }
        }
        for f in &p.fams {
            let Some(fam) = g.family(f) else { continue };
            for s in [fam.husband(), fam.wife()].into_iter().flatten().filter(|s| *s != x) {
                if let Some(n) = name_of(s) {
                    p.spouses.push((s.to_string(), n));
                }
            }
            for c in fam.children() {
                if let Some(n) = name_of(c) {
                    p.children.insert(n);
                }
            }
            // Family events: the person was there as a spouse.
            if let Some(node) = g.get(f) {
                for (index, ev) in node.children.iter().enumerate().filter(|(_, e)| event_label(&e.tag).is_some()) {
                    let place = place_key(ev.path_val(&["PLAC"]).unwrap_or(""));
                    if !place.is_empty() {
                        p.places.insert(place.clone());
                    }
                    let y = Span::years_of(ev.path_val(&["DATE"]).unwrap_or(""));
                    p.seen.push(Seen { owner: f.clone(), index, years: y, attended: false });
                }
            }
        }
        out.insert(x, p);
    }
    // Events attended (witness, godparent…): one pass over every event.
    for r in g.records_of(RecordKind::Individual).chain(g.records_of(RecordKind::Family)) {
        let Some(owner) = r.xref.as_deref() else { continue };
        let principals: Vec<String> = if r.tag == "FAM" {
            r.children
                .iter()
                .filter(|c| c.tag == "HUSB" || c.tag == "WIFE")
                .filter_map(|c| c.pointer())
                .map(str::to_string)
                .collect()
        } else {
            vec![owner.to_string()]
        };
        for (index, ev) in r.children.iter().enumerate().filter(|(_, e)| event_label(&e.tag).is_some()) {
            let y = Span::years_of(ev.path_val(&["DATE"]).unwrap_or(""));
            let place = place_key(ev.path_val(&["PLAC"]).unwrap_or(""));
            for a in ev.children_with("ASSO").filter_map(|a| a.pointer()) {
                let Some(p) = out.get_mut(a) else { continue };
                if principals.iter().any(|x| x == a) {
                    continue;
                }
                if !place.is_empty() {
                    p.places.insert(place.clone());
                }
                p.attended_for.extend(principals.iter().cloned());
                p.seen.push(Seen { owner: owner.to_string(), index, years: y, attended: true });
            }
        }
    }
    // Possible birth years, from everything that dates the person.
    let births: HashMap<String, Option<(i32, i32)>> = out.iter().map(|(x, p)| (x.clone(), p.birth)).collect();
    for (x, p) in out.iter_mut() {
        let mut w = Window::default();
        if let Some((lo, hi)) = p.birth {
            w.raise(lo);
            w.lower(hi);
        }
        if let Some((lo, hi)) = p.death {
            w.raise(lo - 110);
            w.lower(hi);
        }
        for s in &p.seen {
            if let Some((lo, hi)) = s.years
                && !(s.owner == *x && p.birth.is_some())
            {
                w.raise(lo - 100);
                w.lower(hi);
            }
        }
        if let Some(i) = g.individual(x) {
            for f in i.fams() {
                for c in g.family(f).map(|f| f.children()).unwrap_or_default() {
                    if let Some(Some((lo, hi))) = births.get(c) {
                        w.raise(lo - 70);
                        w.lower(hi - 12);
                    }
                }
            }
            for f in i.famc() {
                if let Some(fam) = g.family(f) {
                    for par in [fam.husband(), fam.wife()].into_iter().flatten() {
                        if let Some(Some((lo, _))) = births.get(par) {
                            w.raise(lo + 12);
                        }
                    }
                }
            }
        }
        p.window = w;
    }
    out
}

/// The pair is marked “not a duplicate” (on either side).
fn marked_distinct(g: &Gedcom, a: &str, b: &str) -> bool {
    let points =
        |x: &str, y: &str| g.get(x).is_some_and(|n| n.children_with(DISTINCT_TAG).any(|c| c.pointer() == Some(y)));
    points(a, b) || points(b, a)
}

fn exclusion(g: &Gedcom, a: &str, b: &str, pa: &Profile, pb: &Profile) -> Option<Exclusion> {
    if a == b {
        return Some(Exclusion::SamePerson);
    }
    if matches!(pa.sex.as_str(), "M" | "F") && matches!(pb.sex.as_str(), "M" | "F") && pa.sex != pb.sex {
        return Some(Exclusion::OppositeSex);
    }
    if marked_distinct(g, a, b) {
        return Some(Exclusion::MarkedDistinct);
    }
    if pa.fams.iter().any(|f| pb.fams.contains(f)) || pa.famc.iter().any(|f| pb.famc.contains(f)) {
        return Some(Exclusion::CloseRelatives);
    }
    if let Some((da, db)) = blood_degrees(g, a, b)
        && (da == 0 || db == 0 || (da <= 2 && db <= 2))
    {
        return Some(Exclusion::CloseRelatives);
    }
    if pa.seen.iter().any(|s| pb.seen.iter().any(|t| s.owner == t.owner && s.index == t.index)) {
        return Some(Exclusion::SameEvent);
    }
    if !pa.window.overlaps(&pb.window) {
        return Some(Exclusion::LivesApart);
    }
    None
}

/// Weighs the details of two individuals that passed the exclusions.
fn clues(g: &Gedcom, pa: &Profile, pb: &Profile) -> Vec<Clue> {
    let mut out = Vec::new();
    let mut clue =
        |weight: f32, text: std::borrow::Cow<'static, str>| out.push(Clue { weight, text: text.into_owned() });

    // Names.
    let full = |p: &Profile| p.given.join(" ");
    if pa.surname == pb.surname && pa.given == pb.given {
        clue(7.0, t!("duplicates.clue.same_name"));
    } else {
        if pa.surname == pb.surname {
            clue(3.0, t!("duplicates.clue.same_surname"));
        } else {
            clue(1.5, t!("duplicates.clue.surname_variant"));
        }
        let (ga, gb) = (&pa.given, &pb.given);
        if sort_key(&full(pa)).replace(' ', "") == sort_key(&full(pb)).replace(' ', "") {
            clue(4.0, t!("duplicates.clue.same_given"));
        } else if ga.iter().all(|w| gb.contains(w)) || gb.iter().all(|w| ga.contains(w)) {
            clue(2.5, t!("duplicates.clue.given_subset"));
        } else if ga.first() == gb.first() {
            // Pierre Hilaire and Pierre Joseph: often brothers or cousins.
            clue(-1.0, t!("duplicates.clue.other_given_differ"));
        } else if ga.iter().any(|w| gb.contains(w)) {
            clue(0.5, t!("duplicates.clue.given_shared"));
        } else if phonetic::similar(&full(pa), &full(pb)) {
            clue(2.0, t!("duplicates.clue.given_variant"));
        } else {
            clue(-4.0, t!("duplicates.clue.given_differ"));
        }
    }

    // Birth and death.
    match (pa.birth, pb.birth) {
        (Some(_), Some(_))
            if pa.birth_date == pb.birth_date
                && crate::date::Date::parse(&pa.birth_date).primary().is_some_and(|d| d.day.is_some()) =>
        {
            clue(5.0, t!("duplicates.clue.same_birth_date"))
        }
        (Some(a), Some(b)) if a.0 <= b.1 && b.0 <= a.1 => clue(2.5, t!("duplicates.clue.birth_years_match")),
        (Some(_), Some(_)) => clue(-3.0, t!("duplicates.clue.birth_differs")),
        _ => {}
    }
    if !pa.birth_place.is_empty() && !pb.birth_place.is_empty() {
        if pa.birth_place == pb.birth_place {
            clue(2.0, t!("duplicates.clue.same_birth_place"));
        } else {
            clue(-1.5, t!("duplicates.clue.birth_place_differs"));
        }
    }
    match (pa.death, pb.death) {
        (Some(a), Some(b)) if a.0 <= b.1 && b.0 <= a.1 => clue(2.5, t!("duplicates.clue.death_years_match")),
        (Some(_), Some(_)) => clue(-3.0, t!("duplicates.clue.death_differs")),
        _ => {}
    }

    // Relatives.
    match (&pa.father, &pb.father) {
        (Some(a), Some(b)) if a == b => clue(3.0, t!("duplicates.clue.same_father")),
        (Some(_), Some(_)) => clue(-3.0, t!("duplicates.clue.father_differs")),
        _ => {}
    }
    match (&pa.mother, &pb.mother) {
        (Some(a), Some(b)) if a == b => clue(3.0, t!("duplicates.clue.same_mother")),
        (Some(_), Some(_)) => clue(-3.0, t!("duplicates.clue.mother_differs")),
        _ => {}
    }
    if pa.spouses.iter().any(|(x, _)| pb.spouses.iter().any(|(y, _)| x == y)) {
        clue(5.0, t!("duplicates.clue.same_spouse"));
    } else if pa.spouses.iter().any(|(_, n)| pb.spouses.iter().any(|(_, m)| n == m)) {
        clue(3.0, t!("duplicates.clue.spouse_names_match"));
    } else if !pa.spouses.is_empty() && !pb.spouses.is_empty() {
        clue(-1.0, t!("duplicates.clue.spouses_differ"));
    }
    let kids = pa.children.intersection(&pb.children).count();
    if kids > 0 {
        clue((2.0 * kids as f32).min(4.0), t!("duplicates.clue.children_names_match", n = kids));
    }

    // Where and when they were seen.
    let seen_years = |p: &Profile| -> Option<(i32, i32)> {
        let ys: Vec<i32> =
            p.seen.iter().filter_map(|s| s.years).filter(|(lo, hi)| hi - lo <= 1).map(|(lo, _)| lo).collect();
        Some((*ys.iter().min()?, *ys.iter().max()?))
    };
    if let (Some((a0, a1)), Some((b0, b1))) = (seen_years(pa), seen_years(pb)) {
        let gap = (b0 - a1).max(a0 - b1).max(0);
        if gap <= 10 {
            let (first, last) = (a0.min(b0), a1.max(b1));
            let text = if first == last {
                t!("duplicates.clue.seen_in", year = first)
            } else {
                t!("duplicates.clue.seen_between", from = first, to = last)
            };
            clue(if gap <= 3 { 2.0 } else { 1.0 }, text);
        }
    }
    let witnessed = |p: &Profile| p.seen.iter().filter(|s| s.attended).count();
    if witnessed(pa) > 0 && witnessed(pb) > 0 && pa.attended_for.iter().any(|x| pb.attended_for.contains(x)) {
        clue(2.0, t!("duplicates.clue.same_principal"));
    }
    let places: Vec<&String> = pa.places.intersection(&pb.places).collect();
    if let Some(first) = places.first() {
        let name = g_place_name(g, first);
        clue(if places.len() > 1 { 2.0 } else { 1.0 }, t!("duplicates.clue.same_place", place = name));
    } else if !pa.places.is_empty() && !pb.places.is_empty() {
        clue(-1.0, t!("duplicates.clue.places_differ"));
    }
    if pa.occupations.intersection(&pb.occupations).next().is_some() {
        clue(1.5, t!("duplicates.clue.same_occupation"));
    }
    if pa.sources.intersection(&pb.sources).next().is_some() {
        clue(1.0, t!("duplicates.clue.same_source"));
    }
    out.sort_by(|a, b| b.weight.abs().total_cmp(&a.weight.abs()));
    out
}

/// Place as written in the file, from its key (first level).
fn g_place_name(g: &Gedcom, key: &str) -> String {
    g.places()
        .into_iter()
        .map(|p| first_level(&p.name).to_string())
        .find(|n| sort_key(n) == key)
        .unwrap_or_else(|| key.to_string())
}

fn blocking_key(p: &Profile, g: &Gedcom, x: &str) -> Option<(String, String)> {
    let i = g.individual(x)?;
    let s = phonetic::key(&i.surname());
    let first = p.given.first().map(|w| phonetic::key(w))?;
    (!s.is_empty() && !first.is_empty()).then_some((s, first))
}

fn judge(g: &Gedcom, a: &str, b: &str, pa: &Profile, pb: &Profile) -> Result<Duplicate, Exclusion> {
    if let Some(e) = exclusion(g, a, b, pa, pb) {
        return Err(e);
    }
    let clues = clues(g, pa, pb);
    let score: f32 = clues.iter().map(|c| c.weight).sum();
    let level = if score >= LIKELY { Level::Likely } else { Level::Possible };
    let (a, b) = if a <= b { (a, b) } else { (b, a) };
    Ok(Duplicate { a: a.to_string(), b: b.to_string(), score, level, clues })
}

impl Gedcom {
    /// Possible duplicates of the whole file, most likely first.
    pub fn duplicates(&self) -> Vec<Duplicate> {
        let profiles = profiles(self);
        let mut blocks: HashMap<(String, String), Vec<&String>> = HashMap::new();
        for (x, p) in &profiles {
            if let Some(k) = blocking_key(p, self, x) {
                blocks.entry(k).or_default().push(x);
            }
        }
        let mut out = Vec::new();
        for xs in blocks.values_mut() {
            xs.sort();
            for (i, a) in xs.iter().enumerate() {
                for b in &xs[i + 1..] {
                    if let Ok(d) = judge(self, a, b, &profiles[*a], &profiles[*b])
                        && d.score >= POSSIBLE
                    {
                        out.push(d);
                    }
                }
            }
        }
        out.sort_by(|x, y| y.score.total_cmp(&x.score).then_with(|| x.a.cmp(&y.a)).then_with(|| x.b.cmp(&y.b)));
        out
    }

    /// Possible duplicates of one individual, most likely first.
    pub fn duplicates_of(&self, xref: &str) -> Vec<Duplicate> {
        self.duplicates().into_iter().filter(|d| d.a == xref || d.b == xref).collect()
    }

    /// Compares two chosen individuals (Person › Merge With…, an AI agent's proposal): the pair
    /// with its clues whatever the score, or the reason they cannot be the same person.
    pub fn compare_individuals(&self, a: &str, b: &str) -> Result<Duplicate, Exclusion> {
        if self.get(a).is_none_or(|n| n.tag != "INDI") || self.get(b).is_none_or(|n| n.tag != "INDI") {
            return Err(Exclusion::NotIndividuals);
        }
        let profiles = profiles(self);
        judge(self, a, b, &profiles[a], &profiles[b])
    }

    /// The user said `a` and `b` are different people.
    pub fn is_distinct(&self, a: &str, b: &str) -> bool {
        marked_distinct(self, a, b)
    }

    /// Marks `a` and `b` as different people (Not a Duplicate): `_GR_DISTINCT` on `a`.
    pub fn mark_distinct(&mut self, a: &str, b: &str) -> bool {
        if a == b || self.get(b).is_none_or(|n| n.tag != "INDI") || self.is_distinct(a, b) {
            return false;
        }
        let Some(n) = self.get_mut(a).filter(|n| n.tag == "INDI") else { return false };
        n.insert_child(Node::with_value(DISTINCT_TAG, format!("@{b}@")));
        n.touch();
        self.declare_extension(DISTINCT_TAG, DISTINCT_URI);
        true
    }

    /// Removes the “not a duplicate” mark between `a` and `b`, on both sides.
    pub fn unmark_distinct(&mut self, a: &str, b: &str) -> bool {
        let mut done = false;
        for (x, y) in [(a, b), (b, a)] {
            let p = format!("@{y}@");
            if let Some(n) = self.get_mut(x) {
                let before = n.children.len();
                n.children.retain(|c| !(c.tag == DISTINCT_TAG && c.val() == p));
                if n.children.len() != before {
                    n.touch();
                    done = true;
                }
            }
        }
        done
    }

    /// Pairs marked as different people, for File Settings.
    pub fn distinct_pairs(&self) -> Vec<(String, String)> {
        let mut out = Vec::new();
        for r in self.records_of(RecordKind::Individual) {
            let Some(a) = r.xref.as_deref() else { continue };
            for b in r.children_with(DISTINCT_TAG).filter_map(|c| c.pointer()) {
                if self.get(b).is_some() {
                    out.push((a.to_string(), b.to_string()));
                }
            }
        }
        out
    }
}
