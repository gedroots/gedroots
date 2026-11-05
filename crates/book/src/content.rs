//! What goes into a book, computed from the tree: the people (by generation), their records
//! (complete, short or a line in a table, depending on what the tree knows about them), the charts
//! and the index. The result is data for the Typst template, plus the files it shows.

use crate::prose;
use crate::spec::{BookSpec, Line};
use crate::writing::{Block, Figure, Writing};
use gedcom::document::Gedcom;
use gedcom::model::{Individual, sort_key};
use gedroots_viz::{Kind, Member, Palette, Scope, Tree};
use serde::Serialize;
use std::collections::{BTreeMap, HashMap};
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

/// A block an agent wrote: text, context, hypothesis or figure.
#[derive(Serialize, Default)]
pub struct BlockData {
    pub kind: &'static str,
    pub title: String,
    pub paragraphs: Vec<String>,
    /// The sources of a context, the clues of a hypothesis, a figure's caption.
    pub notes: Vec<String>,
    /// A figure's drawing, and the legend of its colours.
    pub image: Option<String>,
    pub legend: Vec<Legend>,
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

/// Label of a person's record in the template: Typst labels only take letters, digits, `-`, `_`.
pub fn key_of(xref: &str) -> String {
    let clean: String = xref.chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '_' }).collect();
    format!("p-{clean}")
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

/// The people of a book, as the visualizations see them.
fn scope(spec: &BookSpec) -> Scope {
    Scope { subject: spec.subject.clone(), line: spec.line, generations: spec.generations }
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
    let tree = Tree::new(g, scope(spec))?;
    let members = &tree.members;
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

    let palette = Palette::PRINT;
    let mut all_places = tree.origins.all.clone();
    // A visualization of the book's own people placed in a chapter is shown there, not again in
    // the opening pages or the figures.
    let placed = |kind: Kind| {
        spec.chapters.values().flat_map(Writing::blocks).any(|b| {
            matches!(b, Block::Figure(f) if f.kind == kind && f.of_book(&spec.subject, spec.line, spec.generations))
        })
    };

    // Fan, of ancestors or descendants, and the same without its names for the cover.
    let fan = tree.fan(&palette, true);
    assets.push(Asset { path: "fan.svg".into(), bytes: fan.svg.into_bytes() });
    assets.push(Asset { path: "fan-cover.svg".into(), bytes: tree.fan(&palette, false).svg.into_bytes() });
    data.fan = true;
    data.legend = fan
        .legend
        .into_iter()
        .map(|l| Legend { label: l.label, color: l.color, count: l.count, unknown: l.unknown })
        .collect();
    if part.opening && !placed(Kind::Fan) {
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
    if part.opening
        && !placed(Kind::Places)
        && let Some(map) = tree.places(&palette)
    {
        assets.push(Asset { path: "map.svg".into(), bytes: map.svg.into_bytes() });
        data.map = Some(MapInfo { title: places_count(map.count), caption: map.caption });
    }
    all_places.sort();
    all_places.dedup();

    // Lifespans: the first generations after the subject.
    if part.opening
        && !placed(Kind::Lives)
        && let Some(lives) = tree.lives(&palette, today.0)
    {
        assets.push(Asset { path: "lifespans.svg".into(), bytes: lives.svg.into_bytes() });
        data.lifespans = true;
    }

    // Chapters, one per generation.
    let name_of = |x: &str| g.individual(x).map(|i| i.display_name());
    let figures = Blocks { g, spec, year: today.0, name: &name_of, drawn: std::cell::Cell::new(0) };
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
            intro: writing.map(|w| figures.blocks(&w.intro, &mut assets)).unwrap_or_default(),
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
                sheet.blocks = figures.blocks(written, &mut assets);
            }
            if full { ch.full.push(sheet) } else { ch.short.push(sheet) }
        }
        data.chapters.push(ch);
    }
    data.levels = spec.chapters.values().flat_map(Writing::blocks).any(Block::level);

    // Figures, in the last volume.
    if part.figures {
        if !placed(Kind::Generations)
            && let Some(found) = tree.figure(Kind::Generations, &palette, today.0)
        {
            assets.push(Asset { path: "completeness.svg".into(), bytes: found.svg.into_bytes() });
            data.completeness = true;
        }
        if !placed(Kind::Longevity)
            && let Some(ages) = tree.longevity(&palette)
        {
            assets.push(Asset { path: "longevity.svg".into(), bytes: ages.svg.into_bytes() });
            data.longevity = Some(ages.caption);
        }
        if !placed(Kind::Occupations) {
            data.occupations = tree.jobs();
        }
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

/// What turns the agent's blocks into the template's: the people's names in place of their ids,
/// and the figures drawn.
struct Blocks<'a, N: Fn(&str) -> Option<String>> {
    g: &'a Gedcom,
    spec: &'a BookSpec,
    /// This year, which ends the lives still open.
    year: i32,
    name: &'a N,
    /// Figures drawn so far, which name their files.
    drawn: std::cell::Cell<usize>,
}

impl<N: Fn(&str) -> Option<String>> Blocks<'_, N> {
    fn paragraphs(&self, t: &str) -> Vec<String> {
        crate::writing::expand(t, self.name)
            .split('\n')
            .map(str::trim)
            .filter(|p| !p.is_empty())
            .map(String::from)
            .collect()
    }

    /// The blocks, in order; a figure the tree has too little for is left out.
    fn blocks(&self, blocks: &[Block], assets: &mut Vec<Asset>) -> Vec<BlockData> {
        blocks
            .iter()
            .filter_map(|b| match b {
                Block::Text(t) => {
                    Some(BlockData { kind: "text", paragraphs: self.paragraphs(t), ..Default::default() })
                }
                Block::Context { title, text, sources } => Some(BlockData {
                    kind: "context",
                    title: title.clone(),
                    paragraphs: self.paragraphs(text),
                    notes: sources
                        .iter()
                        .map(|s| if s.url.is_empty() { s.title.clone() } else { format!("{} — {}", s.title, s.url) })
                        .collect(),
                    ..Default::default()
                }),
                Block::Hypothesis { text, basis } => Some(BlockData {
                    kind: "hypothesis",
                    paragraphs: self.paragraphs(text),
                    notes: basis.clone(),
                    ..Default::default()
                }),
                Block::Figure(f) => self.figure(f, assets),
            })
            .collect()
    }

    fn figure(&self, f: &Figure, assets: &mut Vec<Asset>) -> Option<BlockData> {
        let spec = self.spec;
        let subject = f.subject.clone().unwrap_or_else(|| spec.subject.clone());
        let scope = Scope::new(&subject, f.line.unwrap_or(spec.line), f.generations.unwrap_or(spec.generations));
        if !f.kind.supports(scope.line) {
            return None;
        }
        let tree = Tree::new(self.g, scope).ok()?;
        let drawn = tree.figure(f.kind, &Palette::PRINT, self.year)?;
        let n = self.drawn.get() + 1;
        self.drawn.set(n);
        let path = format!("figure-{n}.svg");
        assets.push(Asset { path: path.clone(), bytes: drawn.svg.into_bytes() });
        // Another person's: the title says whose.
        let title = if f.of_book(&spec.subject, spec.line, spec.generations) {
            f.kind.title()
        } else {
            let name = (self.name)(&subject).unwrap_or_default();
            let whose = match tree.scope.line {
                Line::Ancestors => t!("book.ancestors_of", name = name),
                Line::Descendants => t!("book.descendants_of", name = name),
            };
            format!("{} · {whose}", f.kind.title())
        };
        Some(BlockData {
            kind: "figure",
            title,
            paragraphs: self.paragraphs(&f.text),
            notes: [drawn.caption].into_iter().filter(|c| !c.is_empty()).collect(),
            image: Some(path),
            legend: drawn
                .legend
                .into_iter()
                .map(|l| Legend { label: l.label, color: l.color, count: l.count, unknown: l.unknown })
                .collect(),
        })
    }
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
    let members = gedroots_viz::scope::members(g, &scope(spec));
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
    fn keys_are_valid_labels() {
        assert_eq!(key_of("I307"), "p-I307");
        assert_eq!(key_of("I 3@"), "p-I_3_");
    }
}
