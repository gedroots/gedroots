//! Search by criteria in the four zones (individuals, sources, places, media).
//!
//! A query stays text: free words and `key:value` criteria. A value that contains spaces is written
//! in quotes (`place:"Bar-sur-Aube"`, `repository:"AD de l'Aube"`). In the interface, each complete
//! criterion becomes a token: [`chip_label`] gives its label and [`Gedcom::suggest`] suggests
//! criteria and values from the file.
//!
//! Individuals have their own criteria ([`crate::search::IndividualQuery`]); this module adds those
//! of the other zones:
//!
//! | Zone | Criteria |
//! | --- | --- |
//! | Sources | `type:birth` (record type), `repository:…`, `without:media`, `with:media`, `without:citation` |
//! | Places | `in:aube` (jurisdiction), `without:coordinates`, `with:coordinates` |
//! | Media | `event:birth`, `person:marchand` (linked person), `format:image\|pdf\|web`, `medium:photo` (`MEDI`), `without:link` |
//!
//! Everywhere: `id:…` (GEDCOM identifier, never suggested) and free words (title, name, file…).
//! Keywords are in English; the labels of tokens and suggestions are in the interface language.
//!
//! Media are also grouped by **type** ([`MediaIndex::types`]): the subfolders declared in File
//! Settings › Media, otherwise the GEDCOM 7 medium (`MEDI`). The type bar of the Media zone relies
//! on it.

use std::collections::{HashMap, HashSet};
use std::ops::Range;

use crate::document::{Gedcom, RecordKind};
use crate::kinds::{SourceContent, medium, medium_label};
use crate::model::{Individual, Media, Place, Repository, Source, event_labels_everywhere, sort_key};
use crate::node::Node;
use crate::search::{IndividualQuery, Kin, YearRange};
use rust_i18n::t;
use std::borrow::Cow;

/// Zone being searched.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Scope {
    Individuals,
    Sources,
    Places,
    Media,
}

/// Splits a query into words, keeping a `key:"value with spaces"` together. Returns the range
/// (bytes) and the text of each word.
pub fn words(q: &str) -> Vec<(Range<usize>, &str)> {
    let mut out = Vec::new();
    let mut start: Option<usize> = None;
    let mut quoted = false;
    for (i, c) in q.char_indices() {
        if c == '"' {
            quoted = !quoted;
            start.get_or_insert(i);
        } else if c.is_whitespace() && !quoted {
            if let Some(s) = start.take() {
                out.push((s..i, &q[s..i]));
            }
        } else {
            start.get_or_insert(i);
        }
    }
    if let Some(s) = start {
        out.push((s..q.len(), &q[s..]));
    }
    out
}

/// Value of a criterion without its quotes.
pub fn unquote(v: &str) -> &str {
    let v = v.trim();
    v.strip_prefix('"').map(|v| v.strip_suffix('"').unwrap_or(v)).unwrap_or(v)
}

/// Writes `key:value`, in quotes if the value contains a space.
pub fn criterion(key: &str, value: &str) -> String {
    if value.chars().any(char::is_whitespace) {
        format!("{key}:\"{}\"", value.replace('"', ""))
    } else {
        format!("{key}:{value}")
    }
}

/// Normalized key of a criterion (`dépôt` → `depot`), if it has the `key:value` form.
fn split(word: &str) -> Option<(String, &str)> {
    let (k, v) = word.split_once(':')?;
    if k.is_empty() || k.starts_with('"') {
        return None;
    }
    Some((sort_key(k), unquote(v)))
}

/// Criterion of the zones other than individuals.
#[derive(Debug, Clone, PartialEq)]
enum Crit {
    Id(String),
    /// Source: record type (tag or label, normalized).
    ActType(String),
    Repository(String),
    /// Source / media: `with:` (true) or `without:` (false) media.
    HasMedia(bool),
    /// Cited source, attached media.
    Linked(bool),
    Within(String),
    HasCoordinates(bool),
    Event(String),
    Person(String),
    Format(Format),
    /// GEDCOM 7 medium (`MEDI`): tag or label, normalized.
    Medium(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Image,
    Pdf,
    Web,
}

impl Format {
    fn parse(v: &str) -> Option<Self> {
        Some(match sort_key(v).trim_end_matches('s') {
            "image" | "photo" | "jpg" | "png" => Format::Image,
            "pdf" => Format::Pdf,
            "web" | "link" | "url" => Format::Web,
            _ => return None,
        })
    }
    fn label(self) -> Cow<'static, str> {
        match self {
            Format::Image => t!("search.chip.images"),
            Format::Pdf => Cow::Borrowed("PDF"),
            Format::Web => t!("search.chip.web_links"),
        }
    }
}

fn present(v: &str, wanted: &[&str]) -> bool {
    let v = sort_key(v);
    wanted.contains(&v.trim_end_matches('s'))
}

fn parse_crit(scope: Scope, key: &str, val: &str) -> Option<Crit> {
    if key == "id" {
        return (!val.is_empty()).then(|| Crit::Id(val.trim_matches('@').to_string()));
    }
    let nonempty = |v: &str| (!v.is_empty()).then(|| sort_key(v));
    match (scope, key) {
        (Scope::Sources, "type") => nonempty(val).map(Crit::ActType),
        (Scope::Sources, "repository" | "repo") => nonempty(val).map(Crit::Repository),
        (Scope::Sources, "without" | "with") => {
            let on = key == "with";
            if present(val, &["media", "image", "photo"]) {
                Some(Crit::HasMedia(on))
            } else if present(val, &["citation", "cited"]) {
                Some(Crit::Linked(on))
            } else {
                None
            }
        }
        (Scope::Places, "in") => nonempty(val).map(Crit::Within),
        (Scope::Places, "without" | "with") => {
            present(val, &["coordinate", "gp"]).then(|| Crit::HasCoordinates(key == "with"))
        }
        (Scope::Media, "event") => nonempty(val).map(Crit::Event),
        (Scope::Media, "person") => nonempty(val).map(Crit::Person),
        (Scope::Media, "format") => Format::parse(val).map(Crit::Format),
        (Scope::Media, "medium") => nonempty(val).map(Crit::Medium),
        (Scope::Media, "without" | "with") => present(val, &["link"]).then(|| Crit::Linked(key == "with")),
        _ => None,
    }
}

/// Parsed query for sources, places or media.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct RecordQuery {
    /// Free words, normalized.
    pub words: Vec<String>,
    crits: Vec<Crit>,
    /// Malformed criteria (`without:foo`), to report them.
    pub unknown: Vec<String>,
}

impl RecordQuery {
    pub fn parse(scope: Scope, q: &str) -> Self {
        let mut out = Self::default();
        for (_, w) in words(q) {
            match split(w) {
                Some((k, v)) => match parse_crit(scope, &k, v) {
                    Some(c) => out.crits.push(c),
                    None if is_key(scope, &k) => out.unknown.push(w.to_string()),
                    // “10:30” or an unknown key: a word like any other.
                    None => out.words.push(sort_key(w)),
                },
                None => out.words.push(sort_key(unquote(w))),
            }
        }
        out
    }

    pub fn is_empty(&self) -> bool {
        self.words.is_empty() && self.crits.is_empty()
    }

    fn words_in(&self, hay: &str) -> bool {
        let hay = sort_key(hay);
        self.words.iter().all(|w| hay.contains(w.as_str()))
    }

    fn id_ok(&self, xref: &str) -> bool {
        self.crits.iter().all(|c| match c {
            Crit::Id(x) => xref.eq_ignore_ascii_case(x) || sort_key(xref).contains(&sort_key(x)),
            _ => true,
        })
    }
}

/// Known keys of a zone, to tell a “misspelled criterion” from a “word with a colon”.
fn is_key(scope: Scope, key: &str) -> bool {
    let keys: &[&str] = match scope {
        Scope::Individuals => &[],
        Scope::Sources => &["type", "repository", "repo", "without", "with"],
        Scope::Places => &["in", "without", "with"],
        Scope::Media => &["event", "person", "format", "medium", "without", "with"],
    };
    key == "id" || keys.contains(&key)
}

/// What is known about a media for search (computed once per version of the file).
#[derive(Debug, Clone, Default)]
pub struct MediaInfo {
    /// Index in [`MediaIndex::types`].
    pub kind: Option<usize>,
    /// Illustrated event (“Birth”), normalized.
    event: String,
    event_label: String,
    /// Linked people (normalized names).
    people: String,
    linked: bool,
    format: Option<Format>,
}

/// Types and context of every media.
#[derive(Debug, Clone, Default)]
pub struct MediaIndex {
    /// Types in display order (without “All”). The last one can be “Others”.
    pub types: Vec<String>,
    pub media: HashMap<String, MediaInfo>,
}

/// Type of the media that fit no other (“Others”).
pub fn other_type() -> String {
    t!("search.type.other").into()
}

/// Type of web links.
pub fn web_type() -> String {
    t!("search.chip.web_links").into()
}

impl MediaIndex {
    /// Type of a media (label).
    pub fn type_of(&self, xref: &str) -> Option<&str> {
        self.media.get(xref).and_then(|m| m.kind).map(|k| self.types[k].as_str())
    }
}

fn format_of(m: &Media) -> Option<Format> {
    if m.web_url().is_some() {
        return Some(Format::Web);
    }
    if m.is_image() {
        return Some(Format::Image);
    }
    let pdf =
        m.form().is_some_and(|f| f.ends_with("pdf")) || m.file().is_some_and(|f| f.to_lowercase().ends_with(".pdf"));
    pdf.then_some(Format::Pdf)
}

/// Last element of a folder path (“Media/Records” → “Records”).
fn dir_label(dir: &str) -> &str {
    dir.trim_matches('/').rsplit('/').next().unwrap_or(dir).trim()
}

impl Gedcom {
    /// Every pointer of the file (for “not cited”, “not attached”).
    fn all_pointers(&self) -> HashSet<String> {
        let mut out = HashSet::new();
        for r in &self.records {
            for c in &r.children {
                c.walk(&mut |n| {
                    if let Some(p) = n.pointer() {
                        out.insert(p.to_string());
                    }
                });
            }
        }
        out
    }

    /// Groups the media by type and computes what they illustrate.
    ///
    /// The type of a media is, in order:
    /// 1. the subfolder (File Settings › Media) that contains its file, otherwise the one that
    ///    would receive this media from what it illustrates;
    /// 2. without subfolders set, the first folder of its file (“Records”, “Portraits”…), when the
    ///    files are filed in several folders;
    /// 3. otherwise its GEDCOM medium (`MEDI`).
    ///
    /// The rest goes into “Others”.
    pub fn media_index(&self) -> MediaIndex {
        let naming = self.media_naming();
        let contexts = self.media_contexts(&HashMap::new(), Some(&naming));
        let pointers = self.all_pointers();
        let root = naming.root.trim().trim_matches('/').to_string();
        let mut people: HashMap<String, Vec<String>> = HashMap::new();
        for r in self.records.iter().filter(|r| r.tag == "INDI") {
            let i = Individual::new(r);
            for o in r.children.iter().filter(|c| c.tag == "OBJE") {
                if let Some(p) = o.pointer() {
                    people.entry(p.to_string()).or_default().push(format!("{} {}", i.surname(), i.given()));
                }
            }
        }
        // Path of a local file, relative to the media folder.
        let local = |f: &Node| -> Option<String> {
            let path = f.val().replace('\\', "/");
            if path.contains("://") || path.starts_with('/') || path.get(1..2) == Some(":") {
                return None;
            }
            let path = path.trim_start_matches("./").to_string();
            Some(match root.is_empty() {
                true => path,
                false => path.strip_prefix(&format!("{root}/")).map(str::to_string).unwrap_or(path),
            })
        };
        let first_dir = |r: &Node| -> Option<String> {
            r.children
                .iter()
                .filter(|c| c.tag == "FILE")
                .find_map(&local)
                .and_then(|p| p.split_once('/').map(|(d, _)| d.to_string()))
                .filter(|d| !d.is_empty() && d != "." && d != "..")
        };

        // Types: subfolders set, otherwise folders of the files, otherwise media.
        let mut types: Vec<String> = Vec::new();
        for f in &naming.folders {
            let l = dir_label(&f.dir);
            if !l.is_empty() && !types.iter().any(|t| t == l) {
                types.push(l.to_string());
            }
        }
        let mode_folders = !types.is_empty();
        let mut kind_of: HashMap<String, Option<String>> = HashMap::new();
        let medias: Vec<&Node> = self.records_of(RecordKind::Media).collect();
        if mode_folders {
            for r in &medias {
                let x = r.xref.clone().unwrap_or_default();
                let mut label = None;
                // 1. The folder where the file is.
                for path in r.children.iter().filter(|c| c.tag == "FILE").filter_map(&local) {
                    if let Some(d) = naming.folders.iter().find(|d| {
                        let dir = d.dir.trim().trim_matches('/');
                        !dir.is_empty() && path.starts_with(&format!("{dir}/"))
                    }) {
                        label = Some(dir_label(&d.dir).to_string());
                        break;
                    }
                }
                // 2. The folder that would receive it.
                if label.is_none()
                    && let Some(ctx) = contexts.get(&x).filter(|c| !c.event.is_empty())
                {
                    label = naming.folder_for(ctx).map(|f| dir_label(&f.dir).to_string()).filter(|l| !l.is_empty());
                }
                kind_of.insert(x, label);
            }
        } else {
            let dirs: Vec<Option<String>> = medias.iter().map(|r| first_dir(r)).collect();
            let mut counts: HashMap<&str, usize> = HashMap::new();
            for d in dirs.iter().flatten() {
                *counts.entry(d.as_str()).or_default() += 1;
            }
            if counts.len() >= 2 {
                let mut v: Vec<(&str, usize)> = counts.into_iter().collect();
                v.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| sort_key(a.0).cmp(&sort_key(b.0))));
                types = v.into_iter().map(|(d, _)| d.to_string()).collect();
                for (r, d) in medias.iter().zip(dirs) {
                    kind_of.insert(r.xref.clone().unwrap_or_default(), d);
                }
            } else {
                let labels: Vec<Option<String>> =
                    medias.iter().map(|r| medium(r).and_then(medium_label).map(Cow::into_owned)).collect();
                types = crate::kinds::MEDIUMS
                    .iter()
                    .filter_map(|m| medium_label(m).map(Cow::into_owned))
                    .filter(|l| labels.contains(&Some(l.clone())))
                    .collect();
                for (r, l) in medias.iter().zip(labels) {
                    kind_of.insert(r.xref.clone().unwrap_or_default(), l);
                }
            }
        }
        // Web links have no folder: a type of their own.
        for r in &medias {
            let x = r.xref.clone().unwrap_or_default();
            if Media::new(r).web_url().is_some() && kind_of.get(&x).cloned().flatten().is_none() {
                if !types.is_empty() && !types.contains(&web_type()) {
                    types.push(web_type());
                }
                if !types.is_empty() {
                    kind_of.insert(x, Some(web_type()));
                }
            }
        }
        let mut index = MediaIndex::default();
        let mut other = false;
        for r in &medias {
            let x = r.xref.clone().unwrap_or_default();
            let m = Media::new(r);
            let ctx = contexts.get(&x).cloned().unwrap_or_default();
            let kind = match kind_of.get(&x).cloned().flatten() {
                Some(l) => types.iter().position(|t| *t == l),
                None => {
                    other = true;
                    Some(usize::MAX)
                }
            };
            let info = MediaInfo {
                kind,
                event: sort_key(&ctx.kind),
                event_label: ctx.kind.clone(),
                people: sort_key(&format!(
                    "{} {} {}",
                    ctx.surname,
                    ctx.given,
                    people.get(&x).map(|v| v.join(" ")).unwrap_or_default()
                )),
                linked: pointers.contains(&x),
                format: format_of(&m),
            };
            index.media.insert(x, info);
        }
        // “Others” for what fits no type, only if there are other types.
        let n = types.len();
        for m in index.media.values_mut() {
            if m.kind == Some(usize::MAX) {
                m.kind = (n > 0).then_some(n);
            }
        }
        if other && n > 0 {
            types.push(other_type());
        }
        index.types = types;
        index
    }

    /// Keys (identifiers, or full name for a place) of the zone's records that match the query.
    /// `media_type` restricts the media to one type of [`MediaIndex::types`].
    pub fn search(
        &self,
        scope: Scope,
        q: &str,
        media: Option<&MediaIndex>,
        media_type: Option<&str>,
    ) -> HashSet<String> {
        match scope {
            Scope::Individuals => {
                let query = IndividualQuery::parse(q);
                self.records_of(RecordKind::Individual)
                    .map(Individual::new)
                    .filter(|i| query.matches(self, i))
                    .map(|i| i.xref().to_string())
                    .collect()
            }
            Scope::Sources => {
                let query = RecordQuery::parse(scope, q);
                let pointers = query.crits.iter().any(|c| matches!(c, Crit::Linked(_))).then(|| self.all_pointers());
                self.records_of(RecordKind::Source)
                    .filter(|r| self.source_matches(&query, r, pointers.as_ref()))
                    .filter_map(|r| r.xref.clone())
                    .collect()
            }
            Scope::Places => {
                let query = RecordQuery::parse(scope, q);
                self.places().into_iter().filter(|p| place_matches(&query, p)).map(|p| p.name).collect()
            }
            Scope::Media => {
                let query = RecordQuery::parse(scope, q);
                let owned;
                let index = match media {
                    Some(i) => i,
                    None => {
                        owned = self.media_index();
                        &owned
                    }
                };
                self.records_of(RecordKind::Media)
                    .filter(|r| {
                        let x = r.xref.as_deref().unwrap_or("");
                        let info = index.media.get(x);
                        media_type.is_none_or(|t| index.type_of(x) == Some(t)) && media_matches(&query, r, info)
                    })
                    .filter_map(|r| r.xref.clone())
                    .collect()
            }
        }
    }

    fn source_matches(&self, q: &RecordQuery, r: &Node, pointers: Option<&HashSet<String>>) -> bool {
        let s = Source::new(r);
        if !q.id_ok(s.xref()) {
            return false;
        }
        let content = SourceContent::read(r);
        if !q.words.is_empty() {
            let hay = format!(
                "{} {} {} {} {}",
                s.title(),
                s.author().unwrap_or_default(),
                s.publication().unwrap_or_default(),
                content.summary(),
                s.call_number().unwrap_or_default()
            );
            if !q.words_in(&hay) {
                return false;
            }
        }
        q.crits.iter().all(|c| match c {
            Crit::ActType(t) => content.events.iter().any(|e| {
                sort_key(e) == *t
                    || event_labels_everywhere(e).iter().any(|l| sort_key(l).starts_with(t.trim_end_matches('s')))
            }),
            Crit::Repository(name) => s
                .repository()
                .and_then(|x| self.get(x))
                .is_some_and(|n| sort_key(&Repository::new(n).name()).contains(name.as_str())),
            Crit::HasMedia(on) => s.media_xrefs().is_empty() != *on,
            Crit::Linked(on) => pointers.is_some_and(|p| p.contains(s.xref())) == *on,
            _ => true,
        })
    }
}

fn place_matches(q: &RecordQuery, p: &Place) -> bool {
    if !q.words.is_empty() && !q.words_in(&p.name) {
        return false;
    }
    q.crits.iter().all(|c| match c {
        Crit::Id(_) => false,
        Crit::Within(j) => p.parts().iter().skip(1).any(|part| sort_key(part).contains(j.as_str())),
        Crit::HasCoordinates(on) => p.latitude.is_some() == *on,
        _ => true,
    })
}

fn media_matches(q: &RecordQuery, r: &Node, info: Option<&MediaInfo>) -> bool {
    let m = Media::new(r);
    if !q.id_ok(m.xref()) {
        return false;
    }
    if !q.words.is_empty() {
        let mut hay = m.title();
        for f in r.children.iter().filter(|c| c.tag == "FILE") {
            hay.push(' ');
            hay.push_str(f.val());
        }
        if let Some(l) = medium(r).and_then(medium_label) {
            hay.push(' ');
            hay.push_str(&l);
        }
        if !q.words_in(&hay) {
            return false;
        }
    }
    q.crits.iter().all(|c| match c {
        Crit::Event(e) => info.is_some_and(|i| i.event.starts_with(e.trim_end_matches('s'))),
        Crit::Person(p) => info.is_some_and(|i| p.split_whitespace().all(|w| i.people.contains(w))),
        Crit::Format(f) => info.is_some_and(|i| i.format == Some(*f)),
        Crit::Linked(on) => info.is_some_and(|i| i.linked == *on),
        Crit::Medium(v) => medium(r).is_some_and(|m| {
            sort_key(m) == *v
                || crate::i18n::LANGUAGES.iter().any(|lang| {
                    crate::i18n::lookup_in(&format!("medium.{m}"), lang)
                        .is_some_and(|l| sort_key(&l).starts_with(v.as_str()))
                })
        }),
        _ => true,
    })
}

// ----------------------------------------------------------------------
// Tokens and suggestions
// ----------------------------------------------------------------------

fn capitalize(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
        None => String::new(),
    }
}

/// “in 1850”, “about 1850 (± 5 years)”, “1800–1850”, “before 1800”.
fn years_label(r: &str) -> String {
    let r = r.trim();
    if let Some(y) = r.strip_prefix('<') {
        t!("search.years.before", year = y).into()
    } else if let Some(y) = r.strip_prefix('>') {
        t!("search.years.after", year = y).into()
    } else if let Some((y, m)) = r.split_once(['±', '~']) {
        t!("search.years.about", year = y.trim(), margin = m.trim()).into()
    } else if r.chars().all(|c| c.is_ascii_digit()) {
        t!("search.years.in", year = r).into()
    } else {
        r.replace('-', "–")
    }
}

/// “Jean MARCHAND (1850–1912)”, for relative suggestions.
fn person_label(i: &Individual) -> String {
    let y = i.years();
    if y.is_empty() { i.display_name() } else { format!("{} ({y})", i.display_name()) }
}

/// Label of the token of a complete criterion, or `None` if this word is not a valid criterion of
/// the zone (it then stays text). `g` gives the names of people (`ancestors:@I1@`).
pub fn chip_label(scope: Scope, word: &str, g: Option<&Gedcom>) -> Option<String> {
    if scope == Scope::Individuals {
        if sort_key(word) == "sosa" {
            return Some("Sosa".into());
        }
        let (k, v) = split(word)?;
        let q = IndividualQuery::parse(word);
        if !q.unknown.is_empty() || !q.words.is_empty() {
            return None;
        }
        return Some(match k.as_str() {
            "born" | "b" => t!("search.chip.born", years = years_label(v)).into(),
            "died" | "d" => t!("search.chip.died", years = years_label(v)).into(),
            "alive" => t!("search.chip.lived", year = v).into(),
            "place" => t!("search.chip.place", place = v).into(),
            "occupation" => t!("search.chip.occupation", occupation = v).into(),
            "name" => match v.strip_prefix('=') {
                Some(v) => t!("search.chip.exact_name", name = v.trim()).into(),
                None => t!("search.chip.name", name = v).into(),
            },
            "sex" => match q.sex {
                Some('M') => t!("search.chip.men").into(),
                Some('F') => t!("search.chip.women").into(),
                _ => t!("search.chip.unknown_sex").into(),
            },
            "without" => feature_label(false, v),
            "with" => feature_label(true, v),
            "sosa" => format!("Sosa {}", years_label(v)),
            "id" => format!("ID {v}"),
            k => {
                let (kin, x) = (Kin::parse(k)?, v.trim_matches('@'));
                let name = match g {
                    Some(g) => g.individual(x)?.display_name(),
                    None => x.to_string(),
                };
                kin.label(&name)
            }
        });
    }
    let (k, v) = split(word)?;
    let c = parse_crit(scope, &k, v)?;
    Some(match c {
        Crit::Id(x) => format!("ID {x}"),
        Crit::ActType(_) => t!("search.chip.act_type", kind = v.to_lowercase()).into(),
        Crit::Repository(_) => t!("search.chip.repository", name = v).into(),
        Crit::HasMedia(true) => t!("search.chip.with_media").into(),
        Crit::HasMedia(false) => t!("search.chip.without_media").into(),
        Crit::Linked(true) if scope == Scope::Sources => t!("search.chip.cited").into(),
        Crit::Linked(false) if scope == Scope::Sources => t!("search.chip.not_cited").into(),
        Crit::Linked(true) => t!("search.chip.attached").into(),
        Crit::Linked(false) => t!("search.chip.not_attached").into(),
        Crit::Within(_) => t!("search.chip.within", place = v).into(),
        Crit::HasCoordinates(true) => t!("search.chip.with_coordinates").into(),
        Crit::HasCoordinates(false) => t!("search.chip.without_coordinates").into(),
        Crit::Event(_) => t!("search.chip.event", event = capitalize(v)).into(),
        Crit::Person(_) => t!("search.chip.person", name = v).into(),
        Crit::Format(f) => f.label().into(),
        Crit::Medium(_) => t!("search.chip.medium", medium = v.to_lowercase()).into(),
    })
}

fn feature_label(with: bool, v: &str) -> String {
    let v = sort_key(v);
    let text = match (v.trim_end_matches('s'), with) {
        ("source", true) => t!("search.chip.with_source"),
        ("source", false) => t!("search.chip.without_source"),
        ("parent", true) => t!("search.chip.with_parents"),
        ("parent", false) => t!("search.chip.without_parents"),
        ("date", true) => t!("search.chip.with_date"),
        ("date", false) => t!("search.chip.without_date"),
        ("spouse" | "family", true) => t!("search.chip.with_spouse"),
        ("spouse" | "family", false) => t!("search.chip.without_spouse"),
        ("media" | "photo" | "image", true) => t!("search.chip.with_media"),
        ("media" | "photo" | "image", false) => t!("search.chip.without_media"),
        ("note", true) => t!("search.chip.with_note"),
        ("note", false) => t!("search.chip.without_note"),
        ("marriage", true) => t!("search.chip.married"),
        ("marriage", false) => t!("search.chip.union_without_marriage"),
        (other, true) => t!("search.chip.with", what = other),
        (other, false) => t!("search.chip.without", what = other),
    };
    text.into()
}

/// Complete criteria of a query: (range, text, token label). The last word is only included if a
/// space follows it (it is still being typed).
pub fn chips(scope: Scope, q: &str, g: Option<&Gedcom>) -> Vec<(Range<usize>, String, String)> {
    let ended = q.ends_with(char::is_whitespace);
    let ws = words(q);
    let n = ws.len();
    ws.into_iter()
        .enumerate()
        .filter(|(i, _)| ended || *i + 1 < n)
        .filter_map(|(_, (r, w))| chip_label(scope, w, g).map(|l| (r, w.to_string(), l)))
        .collect()
}

/// A suggestion under the search field.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Suggestion {
    /// Group the suggestion is listed under.
    pub group: Group,
    pub label: String,
    /// Criterion to insert in place of the current word.
    pub insert: String,
    /// Number of words replaced, counting back from the current word (2 for “jean guy” →
    /// `ancestors:@I1@`).
    pub consumes: usize,
}

/// Group of a [`Suggestion`] (“Place”, “Name”, “Criteria”…).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Group {
    Family,
    Dates,
    Place,
    Name,
    Occupation,
    ActType,
    Repository,
    Jurisdiction,
    Event,
    Person,
    Medium,
    Criteria,
}

impl Group {
    /// Heading shown above the group, in the interface language.
    pub fn label(self) -> Cow<'static, str> {
        match self {
            Group::Family => t!("search.group.family"),
            Group::Dates => t!("search.group.dates"),
            Group::Place => t!("search.group.place"),
            Group::Name => t!("search.group.name"),
            Group::Occupation => t!("search.group.occupation"),
            Group::ActType => t!("search.group.act_type"),
            Group::Repository => t!("search.group.repository"),
            Group::Jurisdiction => t!("search.group.jurisdiction"),
            Group::Event => t!("search.group.event"),
            Group::Person => t!("search.group.person"),
            Group::Medium => t!("search.group.medium"),
            Group::Criteria => t!("search.group.criteria"),
        }
    }
}

fn sugg(group: Group, label: impl Into<String>, insert: impl Into<String>) -> Suggestion {
    Suggestion { group, label: label.into(), insert: insert.into(), consumes: 1 }
}

/// Criteria without a value offered in each zone.
fn fixed(scope: Scope) -> Vec<Suggestion> {
    let list: Vec<(Cow<'static, str>, &str)> = match scope {
        Scope::Individuals => vec![
            (t!("search.fixed.sosa"), "sosa"),
            (t!("search.chip.men"), "sex:m"),
            (t!("search.chip.women"), "sex:f"),
            (t!("search.chip.without_source"), "without:source"),
            (t!("search.chip.without_parents"), "without:parents"),
            (t!("search.chip.without_date"), "without:date"),
            (t!("search.chip.without_spouse"), "without:spouse"),
            (t!("search.chip.union_without_marriage"), "without:marriage"),
            (t!("search.chip.with_media"), "with:media"),
            (t!("search.chip.without_media"), "without:media"),
            (t!("search.chip.with_note"), "with:note"),
        ],
        Scope::Sources => vec![
            (t!("search.chip.without_media"), "without:media"),
            (t!("search.chip.with_media"), "with:media"),
            (t!("search.chip.not_cited"), "without:citation"),
        ],
        Scope::Places => vec![
            (t!("search.chip.without_coordinates"), "without:coordinates"),
            (t!("search.chip.with_coordinates"), "with:coordinates"),
        ],
        Scope::Media => vec![
            (t!("search.chip.images"), "format:image"),
            (t!("search.fixed.pdf"), "format:pdf"),
            (t!("search.chip.web_links"), "format:web"),
            (t!("search.chip.not_attached"), "without:link"),
        ],
    };
    list.into_iter().map(|(l, t)| sugg(Group::Criteria, l, t)).collect()
}

/// Adds `(value → count)`, keeping the most frequent ones.
fn top(counts: HashMap<String, usize>, n: usize) -> Vec<String> {
    let mut v: Vec<(String, usize)> = counts.into_iter().collect();
    v.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| sort_key(&a.0).cmp(&sort_key(&b.0))));
    v.into_iter().take(n).map(|(k, _)| k).collect()
}

fn matches_word(value: &str, partial: &str) -> bool {
    let v = sort_key(value);
    v.starts_with(partial) || v.split([' ', '-', '\'']).any(|w| w.starts_with(partial))
}

impl Gedcom {
    /// Suggestions for the current word `partial` (empty: the zone's criteria). `previous`: the
    /// free words typed just before (“jean” in “jean guy”), to suggest a person's relatives. The
    /// values come from the file (places, names, occupations, record types…). The caller counts the
    /// results of each and drops those that have none.
    pub fn suggest(
        &self,
        scope: Scope,
        previous: &[&str],
        partial: &str,
        media: Option<&MediaIndex>,
    ) -> Vec<Suggestion> {
        let p = sort_key(unquote(partial).trim());
        if p.contains(':') {
            return Vec::new();
        }
        let mut out: Vec<Suggestion> = Vec::new();
        // A year or a period: birth, death, alive.
        if scope == Scope::Individuals && !p.is_empty() && YearRange::parse(&p).is_some() {
            let year = p.chars().all(|c| c.is_ascii_digit());
            let label = years_label(&p);
            out.push(sugg(Group::Dates, t!("search.suggest.born", years = label), format!("born:{p}")));
            if year {
                out.push(sugg(Group::Dates, t!("search.suggest.born_about", year = p), format!("born:{p}±5")));
            }
            out.push(sugg(Group::Dates, t!("search.suggest.died", years = label), format!("died:{p}")));
            if year {
                out.push(sugg(Group::Dates, t!("search.suggest.died_about", year = p), format!("died:{p}±5")));
                out.push(sugg(Group::Dates, t!("search.suggest.alive_in", year = p), format!("alive:{p}")));
            }
            return out;
        }
        // The typed words designate one or two people: their relatives.
        let mut family: Vec<Suggestion> = Vec::new();
        if scope == Scope::Individuals && p.chars().count() >= 2 {
            let mut text: Vec<&str> = previous.to_vec();
            text.push(partial);
            let q = IndividualQuery::parse(&text.join(" "));
            let found: Vec<Individual> = self
                .records_of(RecordKind::Individual)
                .map(Individual::new)
                .filter(|i| q.matches(self, i))
                .take(3)
                .collect();
            if (1..=2).contains(&found.len()) {
                let kins: &[Kin] =
                    if found.len() == 1 { &Kin::ALL } else { &[Kin::Ancestors, Kin::Descendants, Kin::Spouses] };
                for i in &found {
                    for k in kins {
                        family.push(Suggestion {
                            group: Group::Family,
                            // Years only serve to tell two people apart.
                            label: if found.len() == 1 {
                                k.label(&i.display_name())
                            } else {
                                k.label(&person_label(i))
                            },
                            insert: format!("{}:@{}@", k.key(), i.xref()),
                            consumes: previous.len() + 1,
                        });
                    }
                }
            }
        }
        if p.chars().count() >= 2 {
            match scope {
                Scope::Individuals => {
                    let mut places: HashMap<String, usize> = HashMap::new();
                    for pl in self.places() {
                        if let Some(first) = pl.parts().first()
                            && matches_word(first, &p)
                        {
                            *places.entry(first.to_string()).or_default() += pl.records.len();
                        }
                    }
                    for v in top(places, 3) {
                        out.push(sugg(Group::Place, v.clone(), criterion("place", &v)));
                    }
                    let mut names: HashMap<String, usize> = HashMap::new();
                    let mut jobs: HashMap<String, usize> = HashMap::new();
                    for r in self.records_of(RecordKind::Individual) {
                        let s = Individual::new(r).surname();
                        if !s.is_empty() && matches_word(&s, &p) {
                            *names.entry(s).or_default() += 1;
                        }
                        for o in r.children_with("OCCU") {
                            let v = o.val().trim();
                            if !v.is_empty() && matches_word(v, &p) {
                                *jobs.entry(v.to_lowercase()).or_default() += 1;
                            }
                        }
                    }
                    for v in top(names, 3) {
                        out.push(sugg(Group::Name, v.clone(), criterion("name", &v)));
                    }
                    for v in top(jobs, 2) {
                        out.push(sugg(Group::Occupation, v.clone(), criterion("occupation", &v)));
                    }
                }
                Scope::Sources => {
                    let mut kinds: HashMap<String, usize> = HashMap::new();
                    for r in self.records_of(RecordKind::Source) {
                        for e in SourceContent::read(r).events {
                            let l = crate::model::event_name(&e);
                            if matches_word(&l, &p) {
                                *kinds.entry(l).or_default() += 1;
                            }
                        }
                    }
                    for v in top(kinds, 3) {
                        let lower = v.to_lowercase();
                        out.push(sugg(
                            Group::ActType,
                            t!("search.suggest.acts_of", kind = lower),
                            criterion("type", &lower),
                        ));
                    }
                    let mut repos: HashMap<String, usize> = HashMap::new();
                    for r in self.records_of(RecordKind::Repository) {
                        let n = Repository::new(r).name().trim().to_string();
                        if !n.is_empty() && matches_word(&n, &p) {
                            *repos.entry(n).or_default() += 1;
                        }
                    }
                    for v in top(repos, 3) {
                        out.push(sugg(Group::Repository, v.clone(), criterion("repository", &v)));
                    }
                }
                Scope::Places => {
                    let mut within: HashMap<String, usize> = HashMap::new();
                    for pl in self.places() {
                        for part in pl.parts().into_iter().skip(1) {
                            if matches_word(part, &p) && part.chars().any(|c| !c.is_ascii_digit()) {
                                *within.entry(part.to_string()).or_default() += 1;
                            }
                        }
                    }
                    for v in top(within, 4) {
                        out.push(sugg(Group::Jurisdiction, v.clone(), criterion("in", &v)));
                    }
                }
                Scope::Media => {
                    if let Some(index) = media {
                        let mut events: HashMap<String, usize> = HashMap::new();
                        for m in index.media.values() {
                            if !m.event_label.is_empty() && matches_word(&m.event_label, &p) {
                                *events.entry(m.event_label.clone()).or_default() += 1;
                            }
                        }
                        for v in top(events, 3) {
                            out.push(sugg(Group::Event, v.clone(), criterion("event", &v.to_lowercase())));
                        }
                    }
                    let mut names: HashMap<String, usize> = HashMap::new();
                    for r in self.records_of(RecordKind::Individual) {
                        let s = Individual::new(r).surname();
                        if !s.is_empty() && matches_word(&s, &p) {
                            *names.entry(s).or_default() += 1;
                        }
                    }
                    for v in top(names, 3) {
                        out.push(sugg(Group::Person, v.clone(), criterion("person", &v)));
                    }
                    // GEDCOM medium (`MEDI`): first word of the label (“manuscript”, “photo”).
                    let mut mediums: HashMap<String, usize> = HashMap::new();
                    for r in self.records_of(RecordKind::Media) {
                        if let Some(l) = medium(r).and_then(medium_label)
                            && matches_word(&l, &p)
                        {
                            *mediums.entry(l.to_string()).or_default() += 1;
                        }
                    }
                    for v in top(mediums, 3) {
                        let key = v.split([',', ' ']).next().unwrap_or(&v).to_lowercase();
                        out.push(sugg(Group::Medium, v.clone(), criterion("medium", &key)));
                    }
                }
            }
        }
        // Groups with a value starting with the typed word come first (“guy”: the surname GUYOT
        // before the place Villeneuve-la-Guyard).
        let starts = |g: &Group| out.iter().any(|s| s.group == *g && sort_key(&s.label).starts_with(&p));
        let mut groups: Vec<Group> = Vec::new();
        for s in &out {
            if !groups.contains(&s.group) {
                groups.push(s.group);
            }
        }
        groups.sort_by_key(|g| !starts(g));
        let mut ordered: Vec<Suggestion> = Vec::new();
        // Several words (“jean martin”): surely looking for a person.
        let (first, last) = if previous.is_empty() { (Vec::new(), family) } else { (family, Vec::new()) };
        ordered.extend(first);
        for g in groups {
            ordered.extend(out.iter().filter(|s| s.group == g).cloned());
        }
        ordered.extend(last);
        ordered.extend(fixed(scope).into_iter().filter(|s| p.is_empty() || matches_word(&s.label, &p)));
        ordered
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const GED: &str = "0 HEAD\n1 GEDC\n2 VERS 7.0\n\
        0 @I1@ INDI\n1 NAME Paul /MARCHAND/\n1 BIRT\n2 DATE 1912\n2 PLAC Saint-Lô, Manche\n2 SOUR @S1@\n1 OBJE @M2@\n\
        0 @R1@ REPO\n1 NAME AD de la Manche\n\
        0 @S1@ SOUR\n1 TITL Registre des naissances\n1 DATA\n2 EVEN BIRT\n1 REPO @R1@\n1 OBJE @M1@\n\
        0 @S2@ SOUR\n1 TITL Recensement 1906\n1 DATA\n2 EVEN CENS\n\
        0 @M1@ OBJE\n1 FILE Actes/naissance-paul.jpg\n2 FORM image/jpeg\n\
        0 @M2@ OBJE\n1 FILE Portraits/paul.jpg\n2 FORM image/jpeg\n3 MEDI PHOTO\n\
        0 @M3@ OBJE\n1 FILE divers/lettre.pdf\n2 FORM application/pdf\n\
        0 @M4@ OBJE\n1 FILE https://example.org/fiche\n2 FORM text/html\n\
        0 TRLR\n";

    fn ged() -> Gedcom {
        Gedcom::parse(GED).unwrap()
    }

    #[test]
    fn words_keep_quoted_values() {
        let w: Vec<&str> = words(r#"guyot place:"Bar sur Aube"  without:source"#).into_iter().map(|(_, w)| w).collect();
        assert_eq!(w, ["guyot", r#"place:"Bar sur Aube""#, "without:source"]);
        assert_eq!(unquote(r#""Bar sur Aube""#), "Bar sur Aube");
        assert_eq!(criterion("place", "Bar sur Aube"), r#"place:"Bar sur Aube""#);
        assert_eq!(criterion("place", "Troyes"), "place:Troyes");
    }

    #[test]
    fn chips_only_for_finished_criteria() {
        let c = chips(Scope::Individuals, "born:1800-1850 without:source guyot place:tro", None);
        let labels: Vec<&str> = c.iter().map(|c| c.2.as_str()).collect();
        assert_eq!(labels, ["Born 1800–1850", "Without Sources"]);
        let c = chips(Scope::Individuals, "place:troyes ", None);
        assert_eq!(c[0].2, "Place troyes");
        assert!(chips(Scope::Individuals, "without:truc ", None).is_empty());
        assert_eq!(chips(Scope::Media, "format:pdf ", None)[0].2, "PDF");
        assert!(chips(Scope::Places, "format:pdf ", None).is_empty());
    }

    #[test]
    fn sources_by_type_repository_media() {
        let g = ged();
        let s = |q: &str| {
            let mut v: Vec<String> = g.search(Scope::Sources, q, None, None).into_iter().collect();
            v.sort();
            v
        };
        assert_eq!(s("type:naissance"), ["S1"]);
        assert_eq!(s("type:recensement"), ["S2"]);
        assert_eq!(s(r#"repository:"AD de la Manche""#), ["S1"]);
        assert_eq!(s("without:media"), ["S2"]);
        assert_eq!(s("without:citation"), ["S2"]);
        assert_eq!(s("registre"), ["S1"]);
    }

    #[test]
    fn places_by_jurisdiction_and_coordinates() {
        let g = ged();
        assert_eq!(g.search(Scope::Places, "in:manche", None, None).len(), 1);
        assert!(g.search(Scope::Places, "in:saint", None, None).is_empty());
        assert_eq!(g.search(Scope::Places, "without:coordinates", None, None).len(), 1);
    }

    #[test]
    fn media_types_from_file_folders_then_medium() {
        let g = ged();
        let index = g.media_index();
        // No subfolders set: the files' folders, the fullest first.
        assert_eq!(index.types, ["Actes", "divers", "Portraits", &web_type()]);
        assert_eq!(index.type_of("M4"), Some(web_type().as_str()));
        assert_eq!(index.type_of("M2"), Some("Portraits"));
        let actes = g.search(Scope::Media, "", Some(&index), Some("Actes"));
        assert_eq!(actes.len(), 1);
        // Files all at the root: the GEDCOM medium, then “Others”.
        let flat = Gedcom::parse(&GED.replace("Actes/", "").replace("Portraits/", "").replace("divers/", "")).unwrap();
        let index = flat.media_index();
        assert_eq!(index.types, ["Photo", &web_type(), &other_type()]);
        assert_eq!(index.type_of("M2"), Some("Photo"));
        assert_eq!(index.type_of("M3"), Some(other_type().as_str()));
        assert_eq!(g.search(Scope::Media, "format:pdf", Some(&index), None).len(), 1);
        assert_eq!(g.search(Scope::Media, "without:link", Some(&index), None).len(), 2);
        assert_eq!(g.search(Scope::Media, "person:marchand", Some(&index), None).len(), 2);
        assert_eq!(g.search(Scope::Media, "event:birth", Some(&index), None).len(), 1);
        assert_eq!(g.search(Scope::Media, "medium:photo", Some(&index), None).len(), 1);
        assert_eq!(chip_label(Scope::Media, "medium:photo", None).as_deref(), Some("Medium photo"));
        assert!(g.suggest(Scope::Media, &[], "pho", Some(&index)).iter().any(|s| s.insert == "medium:photo"));
    }

    #[test]
    fn media_types_from_folders() {
        let mut g = ged();
        let naming = crate::MediaNaming {
            root: String::new(),
            folders: vec![
                crate::MediaFolder { dir: "Actes".into(), types: vec!["BIRT".into()], template: String::new() },
                crate::MediaFolder { dir: "Portraits".into(), types: vec!["INDI".into()], template: String::new() },
            ],
        };
        g.set_media_naming(&naming);
        let index = g.media_index();
        assert_eq!(index.types, ["Actes", "Portraits", &web_type(), &other_type()]);
        assert_eq!(index.type_of("M1"), Some("Actes"));
        assert_eq!(index.type_of("M2"), Some("Portraits"));
        assert_eq!(index.type_of("M3"), Some(other_type().as_str()));
    }

    #[test]
    fn suggestions_come_from_the_file() {
        let g = ged();
        let s = g.suggest(Scope::Individuals, &[], "sai", None);
        assert!(s.iter().any(|s| s.group == Group::Place && s.insert == "place:Saint-Lô"));
        let s = g.suggest(Scope::Individuals, &[], "march", None);
        assert_eq!(s[0].insert, "name:MARCHAND");
        let s = g.suggest(Scope::Individuals, &[], "1850", None);
        assert_eq!(s[0].insert, "born:1850");
        let s = g.suggest(Scope::Sources, &[], "bir", None);
        assert!(s.iter().any(|s| s.insert == "type:birth"));
        // Empty field: the zone's criteria.
        assert!(g.suggest(Scope::Places, &[], "", None).iter().all(|s| s.group == Group::Criteria));
    }

    const FAMILY: &str = "0 HEAD\n1 GEDC\n2 VERS 7.0\n\
        0 @I1@ INDI\n1 NAME Jean /GUYOT/\n1 BIRT\n2 DATE 1700\n1 FAMC @F0@\n1 FAMS @F1@\n\
        0 @I0@ INDI\n1 NAME Pierre /GUYAU/\n1 FAMS @F0@\n\
        0 @I2@ INDI\n1 NAME Marie /DUPONT/\n1 FAMS @F1@\n\
        0 @I3@ INDI\n1 NAME Paul /GUILLOT/\n1 BIRT\n2 DATE 1733\n1 FAMC @F1@\n1 FAMS @F2@\n\
        0 @I4@ INDI\n1 NAME Luc /GUYOT/\n1 BIRT\n2 DATE 1760\n1 FAMC @F2@\n\
        0 @I5@ INDI\n1 NAME Anne /MARTIN/\n1 BIRT\n2 DATE 1735\n1 FAMC @F1@\n\
        0 @F0@ FAM\n1 HUSB @I0@\n1 CHIL @I1@\n\
        0 @F1@ FAM\n1 HUSB @I1@\n1 WIFE @I2@\n1 CHIL @I3@\n1 CHIL @I5@\n\
        0 @F2@ FAM\n1 HUSB @I3@\n1 CHIL @I4@\n\
        0 TRLR\n";

    fn family() -> Gedcom {
        Gedcom::parse(FAMILY).unwrap()
    }

    fn ids(g: &Gedcom, q: &str) -> Vec<String> {
        let mut v: Vec<String> = g.search(Scope::Individuals, q, None, None).into_iter().collect();
        v.sort();
        v
    }

    #[test]
    fn approximate_years_and_name_variants() {
        let g = family();
        assert_eq!(ids(&g, "born:1730±5"), ["I3", "I5"]);
        assert_eq!(ids(&g, "born:1730"), Vec::<String>::new());
        assert_eq!(chip_label(Scope::Individuals, "born:1730±5", None).as_deref(), Some("Born about 1730 (± 5 years)"));
        assert_eq!(chip_label(Scope::Individuals, "died:1730", None).as_deref(), Some("Died in 1730"));
        // Guyot, Guyau, Guillot: one and the same name.
        assert_eq!(ids(&g, "name:guyot"), ["I0", "I1", "I3", "I4"]);
        assert_eq!(ids(&g, "name:=guyot"), ["I1", "I4"]);
        assert_eq!(chip_label(Scope::Individuals, "name:=guyot", None).as_deref(), Some("Exact Name guyot"));
    }

    #[test]
    fn relatives_of_a_person() {
        let g = family();
        assert_eq!(ids(&g, "ancestors:@I4@"), ["I0", "I1", "I2", "I3"]);
        assert_eq!(ids(&g, "descendants:@I1@"), ["I3", "I4", "I5"]);
        assert_eq!(ids(&g, "children:@I1@"), ["I3", "I5"]);
        assert_eq!(ids(&g, "parents:@I3@"), ["I1", "I2"]);
        assert_eq!(ids(&g, "spouses:@I2@"), ["I1"]);
        assert_eq!(ids(&g, "siblings:@I3@"), ["I5"]);
        assert_eq!(ids(&g, "descendants:@I1@ sex:f"), Vec::<String>::new());
        assert_eq!(
            chip_label(Scope::Individuals, "ancestors:@I4@", Some(&g)).as_deref(),
            Some("Ancestors of Luc GUYOT")
        );
        assert!(chip_label(Scope::Individuals, "ancestors:@I99@", Some(&g)).is_none());
        // “luc guy”: a single person, their relatives first.
        let s = g.suggest(Scope::Individuals, &["luc"], "guy", None);
        assert_eq!(s[0].group, Group::Family);
        assert_eq!(s[0].consumes, 2);
        assert!(s.iter().any(|s| s.insert == "ancestors:@I4@" && s.label == "Ancestors of Luc GUYOT"));
        let s = g.suggest(Scope::Individuals, &[], "1730", None);
        assert!(s.iter().any(|s| s.insert == "born:1730±5" && s.label == "Born about 1730"));
    }
}
