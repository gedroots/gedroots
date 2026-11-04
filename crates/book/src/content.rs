//! What goes into a book, computed from the tree: the people (by generation), their records
//! (complete, short or a line in a table, depending on what the tree knows about them), the charts
//! and the index. The result is data for the Typst template, plus the files it shows.

use crate::charts::{self, DescCell, FanCell, Life, MapPoint};
use crate::prose;
use crate::spec::{BookSpec, Line};
use crate::writing::{Block, Writing};
use gedcom::document::Gedcom;
use gedcom::model::{Individual, sort_key};
use serde::Serialize;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::Path;

/// From this many facts (dated or placed events, marriages), a person gets a complete record.
pub const FULL_FACTS: usize = 5;
/// From this many, a short record; below, a line in the generation's table.
pub const SHORT_FACTS: usize = 2;

/// Which part of a book to typeset: all of it, or one volume of a book too long for one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Part {
    /// The chapters (generations, both included); `None`: all of them.
    pub chapters: Option<(u32, u32)>,
    /// “Volume 2 of 3”: (2, 3).
    pub volume: Option<(usize, usize)>,
    /// The opening charts (fan, map, lives): in the first volume.
    pub opening: bool,
    /// The figures: in the last volume.
    pub figures: bool,
    /// For a printer: pages with bleed, a title page in place of the cover.
    pub print: bool,
}

impl Part {
    pub fn whole() -> Part {
        Part { chapters: None, volume: None, opening: true, figures: true, print: false }
    }

    fn has(&self, generation: u32) -> bool {
        self.chapters.is_none_or(|(a, b)| (a..=b).contains(&generation))
    }
}

/// Bleed around the pages and the cover for a printer: 0.125 in, as Lulu and Blurb ask.
pub const BLEED_MM: f64 = 3.175;

/// A file the template shows: a chart or a portrait.
pub struct Asset {
    pub path: String,
    pub bytes: Vec<u8>,
}

#[derive(Serialize, Default)]
pub struct Data {
    pub title: String,
    pub subtitle: String,
    pub summary: String,
    pub colophon: String,
    pub lang: String,
    pub width_mm: f64,
    pub height_mm: f64,
    pub labels: BTreeMap<&'static str, String>,
    /// A fan (of ancestors or descendants) on the cover.
    pub fan: bool,
    /// The fan's own page, with its title.
    pub fan_page: Option<FanPage>,
    pub legend: Vec<Legend>,
    pub map: Option<MapInfo>,
    pub lifespans: bool,
    pub completeness: bool,
    pub longevity: Option<String>,
    pub occupations: Vec<(String, usize)>,
    pub chapters: Vec<Chapter>,
    pub index: Vec<IndexGroup>,
    /// Context or hypotheses in the book: the contents page says how to read them.
    pub levels: bool,
    /// “Volume 2 of 3”, or empty.
    pub volume: String,
    /// For a printer: a title page instead of the cover, and the bleed around each page.
    pub print: bool,
    pub bleed_mm: f64,
}

#[derive(Serialize)]
pub struct FanPage {
    pub title: String,
    pub subtitle: String,
}

/// A block an agent wrote: text, context or hypothesis.
#[derive(Serialize)]
pub struct BlockData {
    pub kind: &'static str,
    pub title: String,
    pub paragraphs: Vec<String>,
    /// The sources of a context, the clues of a hypothesis.
    pub notes: Vec<String>,
}

#[derive(Serialize)]
pub struct Legend {
    pub label: String,
    pub color: String,
    pub count: usize,
    pub unknown: bool,
}

#[derive(Serialize)]
pub struct MapInfo {
    pub title: String,
    pub caption: String,
}

#[derive(Serialize, Default)]
pub struct Chapter {
    pub title: String,
    pub subtitle: String,
    pub intro: Vec<BlockData>,
    pub full: Vec<Sheet>,
    pub short: Vec<Sheet>,
    pub rows: Vec<Row>,
}

#[derive(Serialize, Default)]
pub struct Sheet {
    pub key: String,
    pub number: String,
    pub name: String,
    pub years: String,
    pub subtitle: String,
    pub strip: Vec<StripPoint>,
    pub paragraphs: Vec<String>,
    /// The agent's narrative, after the paragraphs (which it replaces when it has text).
    pub blocks: Vec<BlockData>,
    pub family: Vec<FamilyLine>,
    pub sources: Vec<String>,
    pub portrait: Option<String>,
}

#[derive(Serialize)]
pub struct StripPoint {
    pub year: i32,
    pub label: String,
}

#[derive(Serialize)]
pub struct FamilyLine {
    pub label: String,
    pub people: Vec<PersonRef>,
}

#[derive(Serialize)]
pub struct PersonRef {
    pub name: String,
    /// Label of their record in the book, for the page reference.
    pub key: Option<String>,
}

#[derive(Serialize)]
pub struct Row {
    pub key: String,
    pub number: String,
    pub name: String,
    pub birth: String,
    pub death: String,
}

#[derive(Serialize)]
pub struct IndexGroup {
    pub surname: String,
    pub people: Vec<IndexEntry>,
}

#[derive(Serialize)]
pub struct IndexEntry {
    pub given: String,
    pub years: String,
    pub key: String,
}

/// A person of the book, with their number and generation (1 = the subject).
struct Member {
    xref: String,
    number: String,
    generation: u32,
    /// Other Sosa numbers of an ancestor found several times (pedigree collapse).
    also: Vec<usize>,
    sosa: Option<usize>,
}

/// Label of a person's record in the template: Typst labels only take letters, digits, `-`, `_`.
pub fn key_of(xref: &str) -> String {
    let clean: String = xref.chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '_' }).collect();
    format!("p-{clean}")
}

fn ancestors(g: &Gedcom, subject: &str, generations: u32) -> Vec<Member> {
    let tree = g.ancestry(subject, generations);
    let mut seen: HashMap<String, usize> = HashMap::new();
    let mut out: Vec<Member> = Vec::new();
    for (sosa, x) in tree.iter().enumerate() {
        let Some(x) = x else { continue };
        if let Some(&i) = seen.get(x) {
            out[i].also.push(sosa);
            continue;
        }
        seen.insert(x.clone(), out.len());
        out.push(Member {
            xref: x.clone(),
            number: sosa.to_string(),
            generation: usize::BITS - sosa.leading_zeros(),
            also: vec![],
            sosa: Some(sosa),
        });
    }
    out
}

fn descendants(g: &Gedcom, subject: &str, generations: u32) -> Vec<Member> {
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    let mut queue = vec![(subject.to_string(), "1".to_string(), 1u32)];
    while !queue.is_empty() {
        let mut next = Vec::new();
        for (x, number, gen_) in queue {
            if !seen.insert(x.clone()) {
                continue;
            }
            if gen_ < generations {
                let mut k = 0;
                for (_, kids) in g.spouses_and_children(&x) {
                    for c in kids {
                        k += 1;
                        next.push((c.xref().to_string(), format!("{number}.{k}"), gen_ + 1));
                    }
                }
            }
            out.push(Member { xref: x, number, generation: gen_, also: vec![], sosa: None });
        }
        queue = next;
    }
    out
}

/// Number of facts the tree knows about a person.
fn facts(g: &Gedcom, ind: &Individual) -> usize {
    let own = ind.events().iter().filter(|e| e.date_raw().is_some() || e.place().is_some()).count();
    let unions = ind
        .fams()
        .into_iter()
        .filter_map(|f| g.family(f))
        .filter(|f| f.events().iter().any(|e| e.date_raw().is_some() || e.place().is_some()))
        .count();
    own + unions
}

/// The place that says where a person comes from: birth, baptism, then marriage or death.
fn origin_place(g: &Gedcom, ind: &Individual) -> Option<String> {
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
/// the people (countries, then regions…), the six largest groups and “Other”.
fn place_groups(places: &[String]) -> (HashMap<String, usize>, Vec<(String, usize)>, usize) {
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
    let keep = if ranked.len() > charts::GROUP_COLORS.len() { charts::GROUP_COLORS.len() - 1 } else { ranked.len() };
    let mut group_of = HashMap::new();
    for p in places {
        let g = at(p, level).and_then(|v| ranked.iter().take(keep).position(|(n, _)| *n == v)).unwrap_or(usize::MAX);
        group_of.insert(p.clone(), g);
    }
    let mut groups: Vec<(String, usize)> = ranked.iter().take(keep).cloned().collect();
    let other: usize = ranked.iter().skip(keep).map(|(_, n)| n).sum();
    if other > 0 {
        groups.push((t!("book.other").into_owned(), other));
    }
    (group_of, groups, keep)
}

fn group_color(i: usize, keep: usize) -> String {
    if i < keep { charts::GROUP_COLORS[i].to_string() } else { charts::OTHER_COLOR.to_string() }
}

fn year_of(raw: Option<&str>) -> Option<i32> {
    raw.map(gedcom::date::Date::parse).and_then(|d| d.year())
}

/// “1864–1948”, or “b. 1864”.
fn years(ind: &Individual) -> String {
    ind.years()
}

fn labels() -> BTreeMap<&'static str, String> {
    let keys: [(&str, std::borrow::Cow<str>); 24] = [
        ("contents", t!("book.contents")),
        ("places_sub", t!("book.places_sub")),
        ("lives", t!("book.lives")),
        ("lives_sub", t!("book.lives_sub")),
        ("birth", t!("book.birth")),
        ("death", t!("book.death")),
        ("parents", t!("book.parents")),
        ("children", t!("book.children")),
        ("sources", t!("book.sources")),
        ("figures", t!("book.figures")),
        ("found", t!("book.found")),
        ("longevity", t!("book.longevity")),
        ("occupations", t!("book.occupations")),
        ("index", t!("book.index")),
        ("index_note", t!("book.index_note")),
        ("also_known", t!("book.also_known")),
        ("generation", t!("book.generation_short")),
        ("chapter", t!("book.chapter", n = "")),
        ("page", t!("book.see_page", n = "")),
        ("spouse", t!("book.spouse")),
        ("context", t!("book.context")),
        ("hypothesis", t!("book.hypothesis")),
        ("basis", t!("book.basis")),
        ("levels", t!("book.levels")),
    ];
    keys.into_iter().map(|(k, v)| (k, v.trim().to_string())).collect()
}

fn generation_title(line: Line, n: u32) -> String {
    match (line, n) {
        (_, 1) => t!("book.generation_1"),
        (Line::Ancestors, 2) => t!("book.ancestors_gen.2"),
        (Line::Ancestors, 3) => t!("book.ancestors_gen.3"),
        (Line::Ancestors, 4) => t!("book.ancestors_gen.4"),
        (Line::Ancestors, n) => t!("book.ancestors_gen.n", n = n),
        (Line::Descendants, 2) => t!("book.descendants_gen.2"),
        (Line::Descendants, 3) => t!("book.descendants_gen.3"),
        (Line::Descendants, n) => t!("book.descendants_gen.n", n = n),
    }
    .into_owned()
}

fn people_count(n: usize) -> String {
    if n == 1 { t!("book.people.one").into_owned() } else { t!("book.people.other", n = n).into_owned() }
}

fn places_count(n: usize) -> String {
    if n == 1 { t!("book.places.one").into_owned() } else { t!("book.places.other", n = n).into_owned() }
}

/// Title of a book left untitled: “The GUYOT family”.
pub fn default_title(g: &Gedcom, spec: &BookSpec) -> String {
    let Some(subject) = g.individual(&spec.subject) else { return String::new() };
    match subject.surname().trim() {
        "" => subject.display_name(),
        s => t!("book.default_title", surname = s).into_owned(),
    }
}

/// Builds the data of the book and the files it shows. `media_dir`: folder the media paths are
/// relative to (the `.ged`'s), for the portraits; `today`: (year, date in words) for the colophon.
pub fn build(
    g: &Gedcom,
    spec: &BookSpec,
    file_name: &str,
    media_dir: Option<&Path>,
    today: (i32, String),
    part: &Part,
) -> Result<(Data, Vec<Asset>), String> {
    let subject = g.individual(&spec.subject).ok_or_else(|| t!("book.no_subject").into_owned())?;
    let members = match spec.line {
        Line::Ancestors => ancestors(g, &spec.subject, spec.generations),
        Line::Descendants => descendants(g, &spec.subject, spec.generations),
    };
    let in_book: HashMap<&str, &Member> = members.iter().map(|m| (m.xref.as_str(), m)).collect();
    let mut assets = Vec::new();
    let (w, h) = spec.size.mm();
    let mut data = Data {
        title: match spec.title.trim() {
            "" => default_title(g, spec),
            t => t.to_string(),
        },
        subtitle: match spec.line {
            Line::Ancestors => t!("book.ancestors_of", name = subject.display_name()),
            Line::Descendants => t!("book.descendants_of", name = subject.display_name()),
        }
        .into_owned(),
        lang: gedcom::i18n::language().into_owned(),
        width_mm: w,
        height_mm: h,
        labels: labels(),
        colophon: t!("book.made_with", date = today.1, file = file_name).into_owned(),
        volume: part.volume.map(|(n, of)| t!("book.volume", n = n, of = of).into_owned()).unwrap_or_default(),
        print: part.print,
        bleed_mm: if part.print { BLEED_MM } else { 0. },
        ..Default::default()
    };

    // Groups of places, for the fan and the lifespans.
    let origins: HashMap<&str, String> =
        members.iter().filter_map(|m| Some((m.xref.as_str(), origin_place(g, &g.individual(&m.xref)?)?))).collect();
    let mut all_places: Vec<String> = members.iter().filter_map(|m| origins.get(m.xref.as_str()).cloned()).collect();
    let (group_of, groups, keep) = place_groups(&all_places);
    let color_of = |x: &str| -> Option<String> {
        origins.get(x).map(|p| group_color(*group_of.get(p).unwrap_or(&usize::MAX), keep))
    };

    // Fan (ancestors only).
    if spec.line == Line::Ancestors {
        let mut cells = Vec::new();
        let mut unknown = 0;
        for m in &members {
            let Some(ind) = g.individual(&m.xref) else { continue };
            for sosa in m.sosa.iter().chain(m.also.iter()) {
                if *sosa >= 1 << spec.generations.min(9) {
                    continue;
                }
                let color = color_of(&m.xref);
                unknown += usize::from(color.is_none());
                cells.push(FanCell {
                    sosa: *sosa,
                    given: ind.given().split_whitespace().next().unwrap_or_default().to_string(),
                    surname: ind.surname(),
                    years: years(&ind),
                    color,
                });
            }
        }
        let gens = spec.generations.min(9);
        assets.push(Asset { path: "fan.svg".into(), bytes: charts::fan(&cells, gens, true).into_bytes() });
        assets.push(Asset { path: "fan-cover.svg".into(), bytes: charts::fan(&cells, gens, false).into_bytes() });
        data.fan = true;
        data.legend = groups
            .iter()
            .enumerate()
            .map(|(i, (label, count))| Legend {
                label: label.clone(),
                color: group_color(i, keep),
                count: *count,
                unknown: false,
            })
            .collect();
        if unknown > 0 {
            data.legend.push(Legend {
                label: t!("book.unknown_place").into_owned(),
                color: "#FFFFFF".into(),
                count: unknown,
                unknown: true,
            });
        }
    }

    // Fan of descendants: each family shares its parent's sector by its number of descendants.
    if spec.line == Line::Descendants {
        let rings = spec.generations.min(9) as usize;
        let parent = |n: &str| n.rsplit_once('.').map(|(p, _)| p.to_string());
        // Weight of a member: the leaves under it, within the drawn rings.
        let mut weight: HashMap<String, f64> = HashMap::new();
        for m in members.iter().rev().filter(|m| (m.generation as usize) <= rings) {
            let w = weight.get(&m.number).copied().unwrap_or(0.).max(1.);
            weight.insert(m.number.clone(), w);
            if let Some(p) = parent(&m.number) {
                let pw = weight.entry(p).or_insert(0.);
                *pw += w;
            }
        }
        let mut cells = Vec::new();
        let mut start: HashMap<String, f64> = HashMap::new();
        let mut angle: HashMap<String, (f64, f64)> = HashMap::new();
        for m in members.iter().filter(|m| (m.generation as usize) <= rings) {
            let Some(ind) = g.individual(&m.xref) else { continue };
            let (a0, a1) = match parent(&m.number) {
                None => (180., 0.),
                Some(p) => {
                    let Some(&(pa0, pa1)) = angle.get(&p) else { continue };
                    let total = weight.get(&p).copied().unwrap_or(1.).max(1.);
                    let from = *start.get(&p).unwrap_or(&pa0);
                    let span = (pa0 - pa1) * weight.get(&m.number).copied().unwrap_or(1.) / total;
                    start.insert(p, from - span);
                    (from, from - span)
                }
            };
            angle.insert(m.number.clone(), (a0, a1));
            cells.push(DescCell {
                ring: m.generation as usize - 1,
                a0,
                a1,
                given: ind.given().split_whitespace().next().unwrap_or_default().to_string(),
                surname: ind.surname(),
                years: years(&ind),
                color: color_of(&m.xref),
            });
        }
        // Only the rings that hold someone.
        let rings = cells.iter().map(|c| c.ring + 1).max().unwrap_or(1).max(2);
        assets.push(Asset { path: "fan.svg".into(), bytes: charts::descendant_fan(&cells, rings, true).into_bytes() });
        assets.push(Asset {
            path: "fan-cover.svg".into(),
            bytes: charts::descendant_fan(&cells, rings, false).into_bytes(),
        });
        data.fan = true;
        data.legend = groups
            .iter()
            .enumerate()
            .map(|(i, (label, count))| Legend {
                label: label.clone(),
                color: group_color(i, keep),
                count: *count,
                unknown: false,
            })
            .collect();
    }
    if data.fan && part.opening {
        data.fan_page = Some(match spec.line {
            Line::Ancestors => {
                FanPage { title: t!("book.origins").into_owned(), subtitle: t!("book.origins_sub").into_owned() }
            }
            Line::Descendants => {
                FanPage { title: t!("book.descent").into_owned(), subtitle: t!("book.descent_sub").into_owned() }
            }
        });
    }

    // Map of the places of origin.
    let coords: HashMap<String, (f64, f64)> = g
        .places()
        .into_iter()
        .filter_map(|p| {
            let lat = gedcom::geo::parse_latitude(p.latitude.as_deref()?)?;
            let lon = gedcom::geo::parse_longitude(p.longitude.as_deref()?)?;
            Some((p.name, (lon, lat)))
        })
        .collect();
    let mut points: BTreeMap<String, MapPoint> = BTreeMap::new();
    let mut located = 0;
    for m in &members {
        let Some(p) = origins.get(m.xref.as_str()) else { continue };
        let Some(&(lon, lat)) = coords.get(p) else { continue };
        located += 1;
        points.entry(p.clone()).or_insert_with(|| MapPoint { lon, lat, label: prose::town(p), count: 0 }).count += 1;
    }
    if points.len() >= 2 && part.opening {
        let mut links = Vec::new();
        for m in &members {
            let Some(a) = origins.get(m.xref.as_str()).and_then(|p| coords.get(p)) else { continue };
            let (f, mo) = g.parents(&m.xref);
            for parent in [f, mo].into_iter().flatten() {
                if in_book.contains_key(parent.xref())
                    && let Some(b) = origins.get(parent.xref()).and_then(|p| coords.get(p))
                    && a != b
                {
                    links.push((*a, *b));
                }
            }
        }
        let pts: Vec<MapPoint> = points.into_values().collect();
        assets.push(Asset { path: "map.svg".into(), bytes: charts::map(&pts, &links).into_bytes() });
        data.map = Some(MapInfo {
            title: places_count(pts.len()),
            caption: t!("book.places_caption", known = located, total = members.len()).into_owned(),
        });
    }
    all_places.sort();
    all_places.dedup();

    // Lifespans: the first generations after the subject, at most 30 lives.
    let mut lives = Vec::new();
    let mut last_gen = 0;
    for m in members.iter().filter(|m| m.generation >= 2) {
        let Some(ind) = g.individual(&m.xref) else { continue };
        let Some(birth) = ind.birth_year() else { continue };
        if lives.len() >= 30 {
            break;
        }
        let marriages = ind
            .fams()
            .into_iter()
            .filter_map(|f| g.family(f))
            .filter_map(|f| f.marriage().and_then(|e| e.date()).and_then(|d| d.year()))
            .collect();
        lives.push(Life {
            label: format!("{} {}", prose::first_name(&ind), title_case(&ind.surname())),
            birth,
            death: year_of(ind.death().as_ref().and_then(|e| e.date_raw())),
            marriages,
            color: color_of(&m.xref).unwrap_or_else(|| charts::OTHER_COLOR.to_string()),
            break_before: m.generation != last_gen,
        });
        last_gen = m.generation;
    }
    if lives.len() >= 3 && part.opening {
        let svg = charts::lifespans(&lives, today.0, &t!("book.marriage"), &t!("book.ongoing"), |a| {
            t!("book.age_years", n = a).into_owned()
        });
        assets.push(Asset { path: "lifespans.svg".into(), bytes: svg.into_bytes() });
        data.lifespans = true;
    }

    // Chapters, one per generation.
    let name_of = |x: &str| g.individual(x).map(|i| i.display_name());
    let generations = members.iter().map(|m| m.generation).max().unwrap_or(1);
    for n in 1..=generations {
        let here: Vec<&Member> = members.iter().filter(|m| m.generation == n).collect();
        if here.is_empty() || !part.has(n) {
            continue;
        }
        let writing = spec.chapters.get(&n);
        let mut ch = Chapter {
            title: generation_title(spec.line, n),
            subtitle: format!("{} · {}", t!("book.generation", n = n), people_count(here.len())),
            intro: writing.map(|w| blocks(&w.intro, &name_of)).unwrap_or_default(),
            ..Default::default()
        };
        for m in here {
            let Some(ind) = g.individual(&m.xref) else { continue };
            let written = writing.and_then(|w| w.person(&m.xref));
            let record = record_of(facts(g, &ind), n, written.is_some());
            let mut number = match spec.line {
                Line::Ancestors => t!("book.sosa", n = &m.number).into_owned(),
                Line::Descendants => m.number.clone(),
            };
            if !m.also.is_empty() {
                let also: Vec<String> = m.also.iter().map(|s| s.to_string()).collect();
                number = format!("{number} ({})", t!("book.also_sosa", n = also.join(", ")));
            }
            if record == Record::Line {
                let ev = |e: Option<gedcom::model::Event>| {
                    e.map(|e| {
                        let d = e.date().map(|d| d.display()).unwrap_or_default();
                        let p = e.place().map(prose::town).unwrap_or_default();
                        [d, p].into_iter().filter(|s| !s.is_empty()).collect::<Vec<_>>().join(", ")
                    })
                    .unwrap_or_default()
                };
                ch.rows.push(Row {
                    key: key_of(&m.xref),
                    number: m.number.clone(),
                    name: ind.display_name(),
                    birth: ev(ind.birth()),
                    death: ev(ind.death()),
                });
                continue;
            }
            let full = record == Record::Full;
            let mut sheet = sheet(g, &ind, number, full, &in_book, media_dir, &mut assets);
            if let Some(written) = written {
                // The agent's narrative tells the facts: the engine's sentences would repeat them.
                if written.iter().any(|b| matches!(b, Block::Text(_))) {
                    sheet.paragraphs.clear();
                }
                sheet.blocks = blocks(written, &name_of);
            }
            if full { ch.full.push(sheet) } else { ch.short.push(sheet) }
        }
        data.chapters.push(ch);
    }
    data.levels = spec.chapters.values().flat_map(Writing::blocks).any(Block::optional);

    // Figures, in the last volume.
    if spec.line == Line::Ancestors && part.figures {
        let found: Vec<(usize, usize)> = (1..=spec.generations.min(12))
            .map(|n| {
                let lo = 1usize << (n - 1);
                let hi = 1usize << n;
                let count = members
                    .iter()
                    .map(|m| m.sosa.iter().chain(m.also.iter()).filter(|s| (lo..hi).contains(*s)).count())
                    .sum();
                (count, hi - lo)
            })
            .collect();
        assets.push(Asset { path: "completeness.svg".into(), bytes: charts::completeness(&found).into_bytes() });
        data.completeness = true;
    }
    let mut ages = Vec::new();
    for m in &members {
        let Some(ind) = g.individual(&m.xref) else { continue };
        if let (Some(b), Some(d)) = (ind.birth(), ind.event("DEAT"))
            && let (Some(by), Some(age)) = (ind.birth_year(), prose::age_years(&b, &d))
            && age < 110
        {
            ages.push((by, age as f64, ind.sex().chars().next().unwrap_or('U')));
        }
    }
    if ages.len() >= 8 && part.figures {
        assets.push(Asset { path: "longevity.svg".into(), bytes: charts::longevity(&ages).into_bytes() });
        data.longevity = Some(t!("book.longevity_caption", n = ages.len()).into_owned());
    }
    let mut jobs: HashMap<String, usize> = HashMap::new();
    for m in &members {
        let Some(ind) = g.individual(&m.xref) else { continue };
        let mine: HashSet<String> = ind
            .events()
            .iter()
            .filter(|e| e.tag() == "OCCU")
            .filter_map(|e| e.value())
            .map(|v| upper_first(v.trim()))
            .collect();
        for j in mine {
            *jobs.entry(j).or_default() += 1;
        }
    }
    let mut jobs: Vec<(String, usize)> = jobs.into_iter().filter(|(_, n)| *n >= 2).collect();
    jobs.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    jobs.truncate(10);
    if part.figures {
        data.occupations = jobs;
    }

    // Index of the people in this part.
    let mut index: BTreeMap<String, (String, Vec<(String, IndexEntry)>)> = BTreeMap::new();
    for m in members.iter().filter(|m| part.has(m.generation)) {
        let Some(ind) = g.individual(&m.xref) else { continue };
        let surname = ind.surname();
        let group = index.entry(sort_key(&surname)).or_insert_with(|| (surname.clone(), vec![]));
        group.1.push((
            sort_key(&ind.given()),
            IndexEntry { given: ind.given(), years: years(&ind), key: key_of(&m.xref) },
        ));
    }
    data.index = index
        .into_values()
        .map(|(surname, mut people)| {
            people.sort_by(|a, b| a.0.cmp(&b.0));
            IndexGroup { surname, people: people.into_iter().map(|(_, e)| e).collect() }
        })
        .collect();

    data.summary = t!(
        "book.summary",
        people = people_count(members.len()),
        generations = generations,
        places = places_count(all_places.len())
    )
    .into_owned();
    Ok((data, assets))
}

/// How a person appears in the book.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Record {
    /// A complete record: life strip, narrative, family, sources.
    Full,
    /// A short paragraph.
    Short,
    /// A line in the generation's table.
    Line,
}

/// The record a person gets from what the tree knows of them: the book's person always a
/// complete one, someone the agent wrote about at least a short one.
fn record_of(facts: usize, generation: u32, written: bool) -> Record {
    if facts >= FULL_FACTS || generation == 1 {
        Record::Full
    } else if facts >= SHORT_FACTS || written {
        Record::Short
    } else {
        Record::Line
    }
}

/// The agent's blocks, with the people's names in place of their ids.
fn blocks(blocks: &[Block], name: &impl Fn(&str) -> Option<String>) -> Vec<BlockData> {
    let paragraphs = |t: &str| -> Vec<String> {
        crate::writing::expand(t, name).split('\n').map(str::trim).filter(|p| !p.is_empty()).map(String::from).collect()
    };
    blocks
        .iter()
        .map(|b| match b {
            Block::Text(t) => {
                BlockData { kind: "text", title: String::new(), paragraphs: paragraphs(t), notes: vec![] }
            }
            Block::Context { title, text, sources } => BlockData {
                kind: "context",
                title: title.clone(),
                paragraphs: paragraphs(text),
                notes: sources
                    .iter()
                    .map(|s| if s.url.is_empty() { s.title.clone() } else { format!("{} — {}", s.title, s.url) })
                    .collect(),
            },
            Block::Hypothesis { text, basis } => BlockData {
                kind: "hypothesis",
                title: String::new(),
                paragraphs: paragraphs(text),
                notes: basis.clone(),
            },
        })
        .collect()
}

/// A person of a chapter, for the agent.
#[derive(Debug, Clone)]
pub struct OutlinePerson {
    pub xref: String,
    pub name: String,
    pub years: String,
    /// Sosa or d'Aboville number.
    pub number: String,
    pub record: Record,
    /// Facts the tree knows (dated or placed events, marriages).
    pub facts: usize,
}

/// A chapter of a book: a generation and its people, in the book's order.
#[derive(Debug, Clone)]
pub struct OutlineChapter {
    pub generation: u32,
    pub title: String,
    pub people: Vec<OutlinePerson>,
}

/// The chapters of a book and who is in each, as the book shows them.
pub fn outline(g: &Gedcom, spec: &BookSpec) -> Result<Vec<OutlineChapter>, String> {
    g.individual(&spec.subject).ok_or_else(|| t!("book.no_subject").into_owned())?;
    let members = match spec.line {
        Line::Ancestors => ancestors(g, &spec.subject, spec.generations),
        Line::Descendants => descendants(g, &spec.subject, spec.generations),
    };
    let mut out: Vec<OutlineChapter> = Vec::new();
    for m in &members {
        let Some(ind) = g.individual(&m.xref) else { continue };
        if out.last().is_none_or(|c| c.generation != m.generation) {
            out.push(OutlineChapter {
                generation: m.generation,
                title: generation_title(spec.line, m.generation),
                people: Vec::new(),
            });
        }
        let facts = facts(g, &ind);
        let written = spec.chapters.get(&m.generation).is_some_and(|w| w.person(&m.xref).is_some());
        out.last_mut().expect("pushed above").people.push(OutlinePerson {
            xref: m.xref.clone(),
            name: ind.display_name(),
            years: ind.years(),
            number: m.number.clone(),
            record: record_of(facts, m.generation, written),
            facts,
        });
    }
    Ok(out)
}

fn upper_first(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
        None => String::new(),
    }
}

/// “GUYOT” → “Guyot”, for the narrow labels of the charts.
fn title_case(s: &str) -> String {
    s.split(' ')
        .map(|w| {
            let mut c = w.chars();
            match c.next() {
                Some(f) => f.to_uppercase().collect::<String>() + &c.as_str().to_lowercase(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn person_ref(ind: &Individual, in_book: &HashMap<&str, &Member>) -> PersonRef {
    let name = match ind.years() {
        y if y.is_empty() => ind.display_name(),
        y => format!("{} ({y})", ind.display_name()),
    };
    PersonRef { name, key: in_book.contains_key(ind.xref()).then(|| key_of(ind.xref())) }
}

fn sheet(
    g: &Gedcom,
    ind: &Individual,
    number: String,
    full: bool,
    in_book: &HashMap<&str, &Member>,
    media_dir: Option<&Path>,
    assets: &mut Vec<Asset>,
) -> Sheet {
    let mut s = Sheet {
        key: key_of(ind.xref()),
        number,
        name: ind.display_name(),
        years: ind.years(),
        paragraphs: prose::paragraphs(g, ind, full),
        ..Default::default()
    };
    let job = ind.events().iter().find(|e| e.tag() == "OCCU").and_then(|e| e.value().map(|v| upper_first(v.trim())));
    let place = ind.birth().and_then(|e| e.place().map(prose::town));
    s.subtitle = [job, place].into_iter().flatten().collect::<Vec<_>>().join(" · ");
    if !full {
        return s;
    }
    // Life strip: birth, marriages, death.
    if let Some(y) = ind.birth_year() {
        s.strip.push(StripPoint { year: y, label: t!("book.birth").into_owned() });
    }
    for f in ind.fams().into_iter().filter_map(|f| g.family(f)) {
        if let Some(y) = f.marriage().and_then(|e| e.date()).and_then(|d| d.year()) {
            s.strip.push(StripPoint { year: y, label: t!("book.marriage").into_owned() });
        }
    }
    if let Some(y) = year_of(ind.death().as_ref().and_then(|e| e.date_raw())) {
        s.strip.push(StripPoint { year: y, label: t!("book.death").into_owned() });
    }
    s.strip.sort_by_key(|p| p.year);
    if s.strip.len() < 2 {
        s.strip.clear();
    }
    // Family.
    let (f, m) = g.parents(ind.xref());
    let parents: Vec<PersonRef> = [f, m].into_iter().flatten().map(|p| person_ref(&p, in_book)).collect();
    if !parents.is_empty() {
        s.family.push(FamilyLine { label: t!("book.parents").into_owned(), people: parents });
    }
    for (spouse, kids) in g.spouses_and_children(ind.xref()) {
        if let Some(sp) = spouse {
            s.family.push(FamilyLine { label: t!("book.spouse").into_owned(), people: vec![person_ref(&sp, in_book)] });
        }
        if !kids.is_empty() {
            s.family.push(FamilyLine {
                label: t!("book.children").into_owned(),
                people: kids.iter().map(|k| person_ref(k, in_book)).collect(),
            });
        }
    }
    // Sources cited by the person's events.
    let mut sources: Vec<String> = Vec::new();
    for x in ind.cited_sources() {
        let label = g.record_label(&x);
        if !sources.contains(&label) {
            sources.push(label);
        }
    }
    sources.truncate(8);
    s.sources = sources;
    // Portrait, when its file is there.
    if let (Some(dir), Some(media)) = (media_dir, g.portrait(ind.xref()))
        && let Some(file) = media.file()
    {
        let path = dir.join(file);
        let ext = path.extension().and_then(|e| e.to_str()).map(str::to_lowercase).unwrap_or_default();
        if matches!(ext.as_str(), "jpg" | "jpeg" | "png")
            && let Ok(bytes) = std::fs::read(&path)
        {
            let name = format!("img/{}.{}", s.key, if ext == "png" { "png" } else { "jpg" });
            assets.push(Asset { path: name.clone(), bytes });
            s.portrait = Some(name);
        }
    }
    s
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
        let (of, groups, _) = place_groups(&places);
        // Countries: France holds 80 %, so the regions are used.
        assert_eq!(groups[0], ("Grand Est".to_string(), 2));
        assert_eq!(groups.len(), 4);
        assert_eq!(of[&places[0]], of[&places[1]]);
        assert_ne!(of[&places[0]], of[&places[3]]);
    }

    #[test]
    fn keys_are_valid_labels() {
        assert_eq!(key_of("I307"), "p-I307");
        assert_eq!(key_of("I 3@"), "p-I_3_");
    }
}
