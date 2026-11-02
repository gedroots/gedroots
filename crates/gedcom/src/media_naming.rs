//! Filing and naming media files (File Settings › Media).
//!
//! The setting belongs to the file, so it is stored in the header with the `_GR_MEDIA` extension
//! (see docs/extensions.md): a root folder, then a list of folders, each with what it receives and
//! its name template.
//!
//! ```text
//! 1 _GR_MEDIA Media
//! 2 _GR_DIR Records
//! 3 _GR_TYPES BIRT, BAPM, MARR, DEAT
//! 3 _GR_NAME {sosa:6}_{branch}_{type:1}_{surname}_{given}_{spouse}_{n}
//! 2 _GR_DIR Portraits
//! 3 _GR_TYPES INDI
//! 3 _GR_NAME {sosa:6}_{surname}_{given}
//! ```
//!
//! The first folder whose types contain the media's type wins; a folder without types receives
//! everything else. The template receives the data of what the media illustrates: the cited event
//! (type, date, place), the person (with their Sosa-d'Aboville number computed from the root
//! individual), the source and the reference (`PAGE`).

use std::collections::{HashMap, HashSet};

use crate::date::Date;
use crate::document::Gedcom;
use crate::model::{Event, Family, Individual, event_label, short_place};
use crate::node::Node;

pub const MEDIA_TAG: &str = "_GR_MEDIA";
pub const MEDIA_URI: &str = extension_uri!("_gr_media");
/// Under `_GR_MEDIA`: a folder.
const DIR_TAG: &str = "_GR_DIR";
/// Under `_GR_DIR`: what it receives.
const TYPES_TAG: &str = "_GR_TYPES";
/// Under `_GR_DIR`: name template of the files.
const NAME_TAG: &str = "_GR_NAME";

/// Variables offered by the panel, in order.
pub const MEDIA_TOKENS: &[&str] = &[
    "type",
    "type:1",
    "date",
    "year",
    "place",
    "surname",
    "surname:simple",
    "given",
    "spouse",
    "sosa:6",
    "branch",
    "source",
    "page",
    "title",
    "n",
];

/// Sort key of a date (year, month, day).
type DateKey = (i32, u8, u8);

/// Attributes (occupation, residence…): they often cite a record that is not primarily about them.
const ATTRIBUTES: &[&str] =
    &["OCCU", "RESI", "EDUC", "RELI", "TITL", "NATI", "CAST", "DSCR", "IDNO", "NCHI", "NMR", "PROP", "SSN", "FACT"];

/// A media subfolder: where, for what, under which name.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MediaFolder {
    /// Subfolder of the root folder (can be empty).
    pub dir: String,
    /// Types received: tags (`BIRT`), custom types (`TYPE` of an `EVEN`), `INDI` / `FAM` for a
    /// media attached directly. Empty: everything else.
    pub types: Vec<String>,
    pub template: String,
}

impl MediaFolder {
    fn accepts(&self, key: &str) -> bool {
        self.types.is_empty()
            || self.types.iter().any(|t| t.eq_ignore_ascii_case(key) || t.to_lowercase() == key.to_lowercase())
    }
}

/// Media folder (relative to the .ged) and its subfolders.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MediaNaming {
    pub root: String,
    pub folders: Vec<MediaFolder>,
}

impl MediaNaming {
    /// Nothing set: GEDRoots keeps the files where they are.
    pub fn is_empty(&self) -> bool {
        self.root.trim().is_empty() && self.folders.is_empty()
    }

    /// Folder that receives a media of this context: the one that names its type, the most specific
    /// (fewest types) if there are several, the first on a tie; otherwise, the first “everything
    /// else” folder.
    pub fn folder_for(&self, ctx: &MediaContext) -> Option<&MediaFolder> {
        self.folders
            .iter()
            .enumerate()
            .filter(|(_, f)| !f.types.is_empty() && f.accepts(&ctx.event))
            .min_by_key(|(i, f)| (f.types.len(), *i))
            .map(|(_, f)| f)
            .or_else(|| self.folders.iter().find(|f| f.types.is_empty()))
    }
}

/// Labels of the types of [`MediaFolder::types`] for the panel, without duplicates.
pub fn media_type_labels(types: &[String]) -> String {
    let mut out: Vec<String> = Vec::new();
    for t in types {
        let l = match t.as_str() {
            "INDI" => rust_i18n::t!("media_naming.portrait").into_owned(),
            "FAM" => rust_i18n::t!("media_naming.family").into_owned(),
            _ => event_label(t).map(|l| l.to_lowercase()).unwrap_or_else(|| crate::model::event_type_label(t)),
        };
        if !out.contains(&l) {
            out.push(l);
        }
    }
    out.join(", ")
}

/// Sosa-d'Aboville number: ancestor `sosa` and, for a collateral, their place among the children
/// (“2”, “B1” for the 1st child of the 2nd union, “2.3”).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Numbering {
    pub sosa: u64,
    pub branch: String,
}

/// What the media illustrates, to fill in the template.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MediaContext {
    /// Filing key: tag of the event (`BIRT`), `TYPE` of an `EVEN`, `INDI` / `FAM` for a direct
    /// attachment, empty otherwise.
    pub event: String,
    /// “Birth”, “Marriage”, or the `TYPE` of an `EVEN`.
    pub kind: String,
    /// Date GEDCOM brute (`14 MAR 1912`).
    pub date: String,
    pub place: String,
    pub surname: String,
    /// Given names (all of them).
    pub given: String,
    /// Surname of the wife, for a couple's event.
    pub spouse: String,
    pub number: Option<Numbering>,
    pub source: String,
    pub page: String,
    /// Title of the media.
    pub title: String,
}

impl MediaContext {
    /// Value of a variable, with its options: `{sosa:6}` pads a number with zeros, `{type:1}` keeps
    /// the first letters, `{surname:simple}` removes accents, spaces and apostrophes.
    fn value(&self, token: &str) -> String {
        let mut opts = token.split(':').map(str::trim);
        let name = opts.next().unwrap_or("");
        let opts: Vec<&str> = opts.collect();
        let width = opts.iter().find_map(|o| o.parse::<usize>().ok());
        let simple = opts.contains(&"simple");
        let date = Date::parse(&self.date);
        let v = match name {
            "type" => self.kind.clone(),
            "date" => match date.primary() {
                Some(d) if d.calendar == crate::date::Calendar::Gregorian && d.year.is_some() => {
                    let mut s = format!("{:04}", d.year.unwrap_or(0));
                    if let Some(m) = d.month {
                        s.push_str(&format!("-{m:02}"));
                        if let Some(day) = d.day {
                            s.push_str(&format!("-{day:02}"));
                        }
                    }
                    s
                }
                _ => date.year().map(|y| y.to_string()).unwrap_or_default(),
            },
            "year" => date.year().map(|y| y.to_string()).unwrap_or_default(),
            "place" => {
                let s = short_place(&self.place);
                s.split(" (").next().unwrap_or("").to_string()
            }
            "surname" => self.surname.clone(),
            "given" => self.given.split_whitespace().next().unwrap_or("").to_string(),
            "given_names" => self.given.clone(),
            "spouse" => self.spouse.clone(),
            "sosa" => self.number.as_ref().map(|n| n.sosa.to_string()).unwrap_or_default(),
            "branch" => self.number.as_ref().map(|n| n.branch.clone()).unwrap_or_default(),
            "source" => self.source.clone(),
            "page" => self.page.clone(),
            "title" => self.title.clone(),
            _ => String::new(),
        };
        let v = if simple { simplify(&v) } else { v };
        match width {
            Some(w) if !v.is_empty() && v.chars().all(|c| c.is_ascii_digit()) => format!("{v:0>w$}"),
            Some(w) => v.chars().take(w).collect(),
            None => v,
        }
    }
}

/// Without accents, spaces or apostrophes: “EL KAIM” → “ELKAIM”, “Médioni” → “Medioni”.
fn simplify(v: &str) -> String {
    let mut out = String::new();
    for c in v.chars() {
        let plain = match c {
            'à' | 'á' | 'â' | 'ã' | 'ä' | 'å' => "a",
            'À' | 'Á' | 'Â' | 'Ã' | 'Ä' | 'Å' => "A",
            'ç' => "c",
            'Ç' => "C",
            'è' | 'é' | 'ê' | 'ë' => "e",
            'È' | 'É' | 'Ê' | 'Ë' => "E",
            'ì' | 'í' | 'î' | 'ï' => "i",
            'Ì' | 'Í' | 'Î' | 'Ï' => "I",
            'ñ' => "n",
            'Ñ' => "N",
            'ò' | 'ó' | 'ô' | 'õ' | 'ö' | 'ø' => "o",
            'Ò' | 'Ó' | 'Ô' | 'Õ' | 'Ö' | 'Ø' => "O",
            'ù' | 'ú' | 'û' | 'ü' => "u",
            'Ù' | 'Ú' | 'Û' | 'Ü' => "U",
            'ý' | 'ÿ' => "y",
            'Ý' | 'Ÿ' => "Y",
            'œ' => "oe",
            'Œ' => "OE",
            'æ' => "ae",
            'Æ' => "AE",
            'ß' => "ss",
            ' ' | '\'' | '’' => "",
            '\u{300}'..='\u{36f}' => "",
            _ => {
                out.push(c);
                continue;
            }
        };
        out.push_str(plain);
    }
    out
}

/// Characters forbidden in a file name (Windows included).
fn clean(v: &str) -> String {
    v.chars().map(|c| if matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|') { '-' } else { c }).collect()
}

fn is_separator(s: &str) -> bool {
    !s.is_empty() && s.chars().all(|c| matches!(c, ' ' | '_' | '-' | ',' | ';' | '.'))
}

enum Piece<'a> {
    Text(&'a str),
    Token(&'a str),
    Slash,
}

fn pieces(template: &str) -> Vec<Piece<'_>> {
    fn text<'a>(t: &'a str, out: &mut Vec<Piece<'a>>) {
        for (i, part) in t.split('/').enumerate() {
            if i > 0 {
                out.push(Piece::Slash);
            }
            if !part.is_empty() {
                out.push(Piece::Text(part));
            }
        }
    }
    let mut out = Vec::new();
    let mut rest = template;
    while let Some(i) = rest.find('{') {
        text(&rest[..i], &mut out);
        match rest[i..].find('}') {
            Some(j) => {
                out.push(Piece::Token(rest[i + 1..i + j].trim()));
                rest = &rest[i + j + 1..];
            }
            None => {
                text(&rest[i..], &mut out);
                rest = "";
            }
        }
    }
    text(rest, &mut out);
    out
}

/// Replaces the template's `{variable}`s (`n`: page number). A separator (`_`, ` - `…) between two
/// variables disappears with an empty variable; `/` creates subfolders.
fn fill_template(template: &str, ctx: &MediaContext, page: Option<usize>) -> String {
    let mut segments: Vec<String> = vec![String::new()];
    let mut pending: Option<&str> = None;
    for p in pieces(template) {
        let seg = segments.last_mut().unwrap();
        match p {
            Piece::Slash => {
                segments.push(String::new());
                pending = None;
            }
            Piece::Text(t) if is_separator(t) => {
                pending.get_or_insert(t);
            }
            Piece::Text(t) | Piece::Token(t) => {
                let v = match p {
                    Piece::Token("n") => page.map(|n| n.to_string()).unwrap_or_default(),
                    Piece::Token(tok) => clean(&ctx.value(tok)),
                    _ => t.to_string(),
                };
                if v.is_empty() {
                    continue;
                }
                if let Some(sep) = pending.take()
                    && !seg.is_empty()
                {
                    seg.push_str(sep);
                }
                seg.push_str(&v);
            }
        }
    }
    segments
        .iter()
        .map(|seg| {
            let seg = seg.split_whitespace().collect::<Vec<_>>().join(" ");
            seg.trim_matches(|c: char| c == '-' || c == '_' || c == ',' || c == '.' || c.is_whitespace()).to_string()
        })
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("/")
}

/// Path (relative to the .ged) of a file according to the setting: root folder, folder of the media
/// type, template, page number for a media with several files (`{n}`, otherwise “p2” at the end of
/// the name), original extension. Without a folder for this media, the original path is kept; if
/// the template gives nothing, the original name is kept in the folder.
pub fn media_path(naming: &MediaNaming, ctx: &MediaContext, original: &str, page: Option<usize>) -> String {
    let Some(folder) = naming.folder_for(ctx) else { return original.to_string() };
    let file = original.rsplit(['/', '\\']).next().unwrap_or(original);
    let (stem, ext) = match file.rfind('.') {
        Some(i) if i > 0 => (&file[..i], file[i + 1..].to_ascii_lowercase()),
        _ => (file, String::new()),
    };
    let mut name = fill_template(&folder.template, ctx, page);
    if name.is_empty() {
        name = stem.to_string();
    } else if let Some(n) = page
        && !folder.template.contains("{n}")
    {
        name.push_str(&format!(" p{n}"));
    }
    if !ext.is_empty() {
        name.push('.');
        name.push_str(&ext);
    }
    [naming.root.trim().trim_matches('/'), folder.dir.trim().trim_matches('/'), &name]
        .into_iter()
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("/")
}

/// A citation that designates a media: (strength, record, event, citation).
type Found<'a> = (u8, &'a Node, &'a Node, &'a Node);

/// Links of the media, collected in one pass (see [`Gedcom::media_contexts`]).
#[derive(Default)]
struct MediaLinks<'a> {
    found: HashMap<String, Vec<Found<'a>>>,
    carriers: HashMap<String, Vec<&'a Node>>,
    direct: HashMap<String, &'a Node>,
}

impl Gedcom {
    pub fn media_naming(&self) -> MediaNaming {
        let Some(n) = self.header.child(MEDIA_TAG) else { return MediaNaming::default() };
        MediaNaming {
            root: n.val().to_string(),
            folders: n
                .children_with(DIR_TAG)
                .map(|d| MediaFolder {
                    dir: d.val().to_string(),
                    types: d
                        .child_val(TYPES_TAG)
                        .map(|v| v.split(',').map(|t| t.trim().to_string()).filter(|t| !t.is_empty()).collect())
                        .unwrap_or_default(),
                    template: d.child_val(NAME_TAG).unwrap_or("").to_string(),
                })
                .collect(),
        }
    }

    /// Replaces a received type in the subfolders (renamed event type).
    pub(crate) fn rename_media_type(&mut self, old: &str, new: &str) {
        let mut m = self.media_naming();
        let mut changed = false;
        for f in &mut m.folders {
            if f.types.iter().any(|t| t.eq_ignore_ascii_case(old)) {
                let mut types: Vec<String> = Vec::new();
                for t in &f.types {
                    let t = if t.eq_ignore_ascii_case(old) { new.to_string() } else { t.clone() };
                    if !types.contains(&t) {
                        types.push(t);
                    }
                }
                f.types = types;
                changed = true;
            }
        }
        if changed {
            self.set_media_naming(&m);
        }
    }

    /// Saves the setting. Returns `false` if nothing changed.
    pub fn set_media_naming(&mut self, m: &MediaNaming) -> bool {
        let m = MediaNaming {
            root: m.root.trim().trim_matches('/').to_string(),
            folders: m
                .folders
                .iter()
                .map(|f| MediaFolder {
                    dir: f.dir.trim().trim_matches('/').to_string(),
                    types: f.types.clone(),
                    template: f.template.trim().to_string(),
                })
                .collect(),
        };
        if m == self.media_naming() {
            return false;
        }
        self.header.children.retain(|c| c.tag != MEDIA_TAG);
        for t in [MEDIA_TAG, DIR_TAG, TYPES_TAG, NAME_TAG] {
            self.undeclare_extension(t);
        }
        if m.is_empty() {
            return true;
        }
        let mut n = Node::new(MEDIA_TAG);
        n.value = (!m.root.is_empty()).then(|| m.root.clone());
        self.declare_extension(MEDIA_TAG, MEDIA_URI);
        for f in &m.folders {
            let mut d = Node::new(DIR_TAG);
            d.value = (!f.dir.is_empty()).then(|| f.dir.clone());
            self.declare_extension(DIR_TAG, MEDIA_URI);
            if !f.types.is_empty() {
                d.children.push(Node::with_value(TYPES_TAG, f.types.join(", ")));
                self.declare_extension(TYPES_TAG, MEDIA_URI);
            }
            if !f.template.is_empty() {
                d.children.push(Node::with_value(NAME_TAG, &f.template));
                self.declare_extension(NAME_TAG, MEDIA_URI);
            }
            n.children.push(d);
        }
        self.header.insert_child(n);
        true
    }

    /// Sosa-d'Aboville numbers computed from the root individual: the ancestors (2k the father,
    /// 2k+1 the mother), then their other descendants, numbered in birth order from the closest
    /// ancestor (the father first).
    pub fn sosa_numbers(&self) -> HashMap<String, Numbering> {
        let mut out: HashMap<String, Numbering> = HashMap::new();
        let Some(root) = self.root() else { return out };
        let mut queue = std::collections::VecDeque::from([(root, 1u64)]);
        let mut ancestors: Vec<(String, u64)> = Vec::new();
        while let Some((x, k)) = queue.pop_front() {
            if out.contains_key(&x) {
                continue;
            }
            out.insert(x.clone(), Numbering { sosa: k, branch: String::new() });
            ancestors.push((x.clone(), k));
            if k > u64::MAX / 4 {
                continue;
            }
            let (f, m) = self.parents(&x);
            if let Some(f) = f {
                queue.push_back((f.xref().to_string(), 2 * k));
            }
            if let Some(m) = m {
                queue.push_back((m.xref().to_string(), 2 * k + 1));
            }
        }
        ancestors.sort_by_key(|(_, k)| *k);
        let mut done: HashSet<String> = HashSet::new();
        // (person, ancestor, branch): descendants to number, breadth first.
        let mut todo: std::collections::VecDeque<(String, u64, String)> =
            ancestors.into_iter().map(|(x, k)| (x, k, String::new())).collect();
        while let Some((x, k, branch)) = todo.pop_front() {
            let Some(ind) = self.individual(&x) else { continue };
            let mut fams: Vec<&str> = ind.fams();
            // Unions in marriage order, undated ones first (like Ancestris).
            let married = |f: &str| {
                self.family(f)
                    .and_then(|f| f.events().into_iter().find(|e| e.node.tag == "MARR"))
                    .and_then(|e| e.date())
                    .filter(|d| d.year().is_some())
                    .map(|d| d.sort_key())
            };
            fams.sort_by_key(|f| married(f));
            for (u, fam) in fams.iter().enumerate() {
                if !done.insert(fam.to_string()) || branch.split('.').count() > 8 {
                    continue;
                }
                let Some(fam) = self.family(fam) else { continue };
                let mut kids: Vec<(usize, &str, Option<DateKey>)> = fam
                    .children()
                    .into_iter()
                    .enumerate()
                    .map(|(i, c)| {
                        let d = self.individual(c).and_then(|p| p.birth()).and_then(|e| e.date());
                        (i, c, d.filter(|d| d.year().is_some()).map(|d| d.sort_key()))
                    })
                    .collect();
                // Birth order, children without a date first (like Ancestris).
                kids.sort_by_key(|(i, _, d)| (*d, *i));
                for (rank, (_, c, _)) in kids.iter().enumerate() {
                    if out.contains_key(*c) {
                        continue;
                    }
                    let mut b = branch.clone();
                    if !b.is_empty() {
                        b.push('.');
                    }
                    if fams.len() > 1 && u > 0 {
                        b.push((b'A' + (u as u8).min(25)) as char);
                    }
                    b.push_str(&(rank + 1).to_string());
                    out.insert(c.to_string(), Numbering { sosa: k, branch: b.clone() });
                    todo.push_back((c.to_string(), k, b));
                }
            }
        }
        out
    }

    /// What the media illustrates: the event whose citation holds the media, otherwise the one that
    /// cites a source holding it (preferably the event the source describes, then the one its title
    /// names, rather than an attribute); then the source holding it; then the linked person or
    /// family.
    pub fn media_context(&self, media: &str) -> MediaContext {
        self.media_context_with(media, &self.sosa_numbers(), Some(&self.media_naming()))
    }

    /// [`Self::media_context`] with numbers already computed (several media).
    pub fn media_context_with(
        &self,
        media: &str,
        numbers: &HashMap<String, Numbering>,
        naming: Option<&MediaNaming>,
    ) -> MediaContext {
        let links = self.media_citations(Some(media));
        self.resolve_media_context(media, &links, numbers, naming)
    }

    /// Context of every media in a single pass over the file.
    pub fn media_contexts(
        &self,
        numbers: &HashMap<String, Numbering>,
        naming: Option<&MediaNaming>,
    ) -> HashMap<String, MediaContext> {
        let links = self.media_citations(None);
        self.records
            .iter()
            .filter(|r| r.tag == "OBJE")
            .filter_map(|r| r.xref.as_deref())
            .map(|x| (x.to_string(), self.resolve_media_context(x, &links, numbers, naming)))
            .collect()
    }

    /// Citations, holding sources and direct attachments of the media (of `only`, or of all), in
    /// file order.
    fn media_citations(&self, only: Option<&str>) -> MediaLinks<'_> {
        let wanted = |m: &str| only.is_none_or(|o| o == m);
        let media_of = |n: &Node| -> Vec<String> {
            n.children
                .iter()
                .filter(|c| c.tag == "OBJE")
                .filter_map(|c| c.pointer())
                .filter(|m| wanted(m))
                .map(str::to_string)
                .collect()
        };
        let mut links = MediaLinks::default();
        for r in self.records.iter().filter(|r| r.tag == "SOUR") {
            for m in media_of(r) {
                links.carriers.entry(m).or_default().push(&**r);
            }
        }
        for r in self.records.iter().filter(|r| r.tag == "INDI" || r.tag == "FAM") {
            for m in media_of(r) {
                links.direct.entry(m).or_insert(&**r);
            }
            for e in r.children.iter().filter(|e| event_label(&e.tag).is_some()) {
                // Media linked to the event itself (announcement, record attached without a
                // source).
                for m in media_of(e) {
                    links.found.entry(m).or_default().push((0, &**r, e, e));
                }
                for c in e.children.iter().filter(|c| c.tag == "SOUR") {
                    let on_citation = media_of(c);
                    for m in &on_citation {
                        links.found.entry(m.clone()).or_default().push((0, &**r, e, c));
                    }
                    // Media held by the cited source.
                    let Some(s) = c.pointer().and_then(|x| self.get(x)) else { continue };
                    let carried = media_of(s);
                    if carried.iter().all(|m| on_citation.contains(m)) {
                        continue;
                    }
                    let content = crate::kinds::SourceContent::read(s);
                    let title = s.child_val("TITL").unwrap_or("").to_lowercase();
                    let label = Event { node: e }.label().to_lowercase();
                    let score = if content.events.contains(&e.tag) {
                        1
                    } else if !label.is_empty() && title.contains(&label) {
                        2
                    } else if !ATTRIBUTES.contains(&e.tag.as_str()) {
                        3
                    } else {
                        4
                    };
                    for m in carried.into_iter().filter(|m| !on_citation.contains(m)) {
                        links.found.entry(m).or_default().push((score, &**r, e, c));
                    }
                }
            }
        }
        links
    }

    fn resolve_media_context(
        &self,
        media: &str,
        links: &MediaLinks<'_>,
        numbers: &HashMap<String, Numbering>,
        naming: Option<&MediaNaming>,
    ) -> MediaContext {
        let mut ctx = MediaContext {
            title: self.get(media).map(|m| crate::model::Media::new(m).title()).unwrap_or_default(),
            ..Default::default()
        };
        let person = |r: &Node, ctx: &mut MediaContext| match r.tag.as_str() {
            "INDI" => {
                let i = Individual::new(r);
                ctx.surname = i.surname();
                ctx.given = i.given();
                ctx.number = r.xref.as_ref().and_then(|x| numbers.get(x)).cloned();
            }
            "FAM" => {
                let f = Family::new(r);
                let (h, w) = (f.husband().and_then(|x| self.individual(x)), f.wife().and_then(|x| self.individual(x)));
                ctx.surname = h.or(w).map(|i| i.surname()).unwrap_or_default();
                ctx.spouse = if h.is_some() { w.map(|i| i.surname()).unwrap_or_default() } else { String::new() };
                ctx.number = [f.husband(), f.wife()].into_iter().flatten().find_map(|x| numbers.get(x)).cloned();
            }
            _ => {}
        };
        // 1. Citation of an event, the strongest first.
        let mut found: Vec<Found<'_>> = links.found.get(media).cloned().unwrap_or_default();
        // With an equal score, the most cited event type wins (a military register cited by three
        // military events and by the death).
        let key =
            |e: &Node| if e.tag == "EVEN" { e.child_val("TYPE").unwrap_or("EVEN").to_string() } else { e.tag.clone() };
        // Among the strongest citations, the event that the first relevant folder receives (a
        // military register cited by the death and by the military service goes into “Military” if
        // that folder comes before “Records”), then the most cited type.
        let tier = found.iter().map(|(s, ..)| *s).min();
        found.retain(|(s, ..)| Some(*s) == tier);
        if let Some(naming) = naming
            && let Some(folder) = naming
                .folders
                .iter()
                .find(|f| !f.types.is_empty() && found.iter().any(|(_, _, e, _)| f.accepts(&key(e))))
        {
            found.retain(|(_, _, e, _)| folder.accepts(&key(e)));
        }
        let count = |e: &Node| found.iter().filter(|(_, _, x, _)| key(x) == key(e)).count();
        let best =
            found.iter().enumerate().min_by_key(|(i, (_, _, e, _))| (std::cmp::Reverse(count(e)), *i)).map(|(_, f)| *f);
        if let Some((_, r, e, cit)) = best {
            let ev = Event { node: e };
            person(r, &mut ctx);
            ctx.event = key(e);
            ctx.kind = ev.label();
            ctx.date = ev.date_raw().unwrap_or("").to_string();
            ctx.place = ev.place().unwrap_or("").to_string();
            let src = cit.pointer().and_then(|s| self.get(s));
            ctx.source = src.and_then(|s| s.child_val("TITL")).unwrap_or("").to_string();
            ctx.page = cit.child_val("PAGE").unwrap_or("").to_string();
            return ctx;
        }
        // 2. Source holding the media: its contents (`DATA.EVEN`) when they designate a single
        //    record type, a precise date rather than a period.
        if let Some(s) = links.carriers.get(media).and_then(|c| c.first()) {
            let content = crate::kinds::SourceContent::read(s);
            if let [t] = content.events.as_slice() {
                ctx.event = t.clone();
                ctx.kind = crate::model::event_name(t);
            }
            if !content.period.split_whitespace().any(|w| matches!(w, "FROM" | "TO" | "BET" | "AND")) {
                ctx.date = content.period;
            }
            ctx.place = content.place;
            ctx.source = s.child_val("TITL").unwrap_or("").to_string();
        }
        // 3. Person or family linked directly (portrait, family photo).
        if let Some(r) = links.direct.get(media) {
            person(r, &mut ctx);
            if ctx.event.is_empty() {
                ctx.event = r.tag.clone();
            }
        }
        ctx
    }

    /// Paths suggested for the files of a media (one per `FILE`, web links excluded): `(position of
    /// the FILE, current path, new path)`.
    pub fn media_renames(&self, media: &str) -> Vec<(usize, String, String)> {
        let naming = self.media_naming();
        let Some(m) = self.get(media) else { return Vec::new() };
        let ctx = self.media_context(media);
        let files: Vec<&Node> = m.children_with("FILE").collect();
        let multi = files.len() > 1;
        files
            .iter()
            .enumerate()
            .filter(|(_, f)| !f.val().is_empty() && !crate::weblink::is_web_url(f.val()))
            .map(|(i, f)| (i, f.val().to_string(), media_path(&naming, &ctx, f.val(), multi.then_some(i + 1))))
            .collect()
    }

    /// Replaces the path of `FILE` number `index` of a media, if it is still `old` (without
    /// touching the change date). Returns `true` if it changed.
    pub fn replace_media_file(&mut self, media: &str, index: usize, old: &str, path: &str) -> bool {
        let matches =
            |g: &Gedcom| g.get(media).and_then(|m| m.children_with("FILE").nth(index)).is_some_and(|f| f.val() == old);
        if !matches(self) {
            return false;
        }
        if let Some(f) = self.get_mut(media).and_then(|m| m.children.iter_mut().filter(|c| c.tag == "FILE").nth(index))
        {
            f.value = Some(path.to_string());
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const G: &str = "0 HEAD\n1 GEDC\n2 VERS 7.0\n\
0 @I1@ INDI\n1 NAME Jean Paul /GUYOT/\n1 BIRT\n2 DATE 14 MAR 1856\n2 PLAC , Troyes, 10000, Aube, Grand Est, France\n\
2 SOUR @S1@\n3 PAGE Acte n° 12\n3 OBJE @M1@\n1 OBJE @M2@\n1 FAMC @F1@\n\
0 @I2@ INDI\n1 NAME Louis /GUYOT/\n1 FAMS @F1@\n1 FAMS @F2@\n\
0 @I3@ INDI\n1 NAME Marie /DE LA RUE/\n1 FAMS @F1@\n\
0 @I4@ INDI\n1 NAME Pierre /GUYOT/\n1 FAMC @F1@\n1 BIRT\n2 DATE 1850\n1 OCCU Soldat\n2 SOUR @S2@\n\
1 DEAT\n2 SOUR @S2@\n1 EVEN\n2 TYPE Service militaire\n2 SOUR @S2@\n\
0 @I5@ INDI\n1 NAME Anne /GUYOT/\n1 FAMC @F2@\n\
0 @F1@ FAM\n1 HUSB @I2@\n1 WIFE @I3@\n1 CHIL @I1@\n1 CHIL @I4@\n1 MARR\n2 DATE 1840\n2 SOUR @S3@\n\
0 @F2@ FAM\n1 HUSB @I2@\n1 CHIL @I5@\n1 MARR\n2 DATE 1870\n\
0 @S1@ SOUR\n1 TITL État civil de Troyes\n1 OBJE @M1@\n\
0 @S2@ SOUR\n1 TITL Matricule de Pierre GUYOT\n1 OBJE @M3@\n\
0 @S3@ SOUR\n1 TITL Acte de mariage\n1 OBJE @M4@\n\
0 @M1@ OBJE\n1 FILE /Users/x/Downloads/scan 001.JPG\n2 FORM image/jpeg\n1 FILE scan2.jpg\n2 FORM image/jpeg\n\
0 @M2@ OBJE\n1 FILE Portraits/jean.png\n2 FORM image/png\n\
0 @M3@ OBJE\n1 FILE m.jpg\n2 FORM image/jpeg\n\
0 @M4@ OBJE\n1 FILE mar.jpg\n2 FORM image/jpeg\n0 TRLR\n";

    fn folder(dir: &str, types: &[&str], t: &str) -> MediaFolder {
        MediaFolder { dir: dir.into(), types: types.iter().map(|s| s.to_string()).collect(), template: t.into() }
    }

    fn naming(root: &str, folders: Vec<MediaFolder>) -> MediaNaming {
        MediaNaming { root: root.into(), folders }
    }

    #[test]
    fn stores_naming_in_header() {
        let mut g = Gedcom::parse(G).unwrap();
        assert!(g.media_naming().is_empty());
        let n = naming(
            "Médias/",
            vec![folder("Actes", &["BIRT", "MARR"], "{type}/{year} {surname} {given}"), folder("Divers", &[], "")],
        );
        assert!(g.set_media_naming(&n));
        let out = g.to_gedcom_string();
        assert!(out.contains(
            "1 _GR_MEDIA Médias\n2 _GR_DIR Actes\n3 _GR_TYPES BIRT, MARR\n3 _GR_NAME {type}/{year} {surname} {given}\n2 _GR_DIR Divers\n"
        ));
        assert!(out.contains("2 TAG _GR_MEDIA https://") && out.contains("2 TAG _GR_TYPES https://"));
        let back = Gedcom::parse(&out).unwrap().media_naming();
        assert_eq!((back.root.as_str(), back.folders.len(), back.folders[0].types.len()), ("Médias", 2, 2));
        assert!(g.set_media_naming(&MediaNaming::default()));
        assert!(g.header.child(MEDIA_TAG).is_none());
        assert!(g.header.child("SCHMA").is_none());
    }

    #[test]
    fn media_on_an_event_takes_its_context() {
        let mut g = Gedcom::parse(G).unwrap();
        let x = g.media_for_file("faire-part.jpg");
        g.attach_media_to_event("F2", 2, &x).unwrap();
        let c = g.media_context(&x);
        assert_eq!((c.event.as_str(), c.date.as_str(), c.surname.as_str()), ("MARR", "1870", "GUYOT"));
    }

    #[test]
    fn labels_types() {
        let types: Vec<String> = ["BIRT", "BAPM", "CHR", "BURI", "INDI", "Service militaire"].map(String::from).into();
        assert_eq!(media_type_labels(&types), "birth, baptism, christening, burial, portrait, Service militaire");
    }

    #[test]
    fn numbers_from_root() {
        let mut g = Gedcom::parse(G).unwrap();
        g.set_root("I1");
        let n = g.sosa_numbers();
        let num = |x: &str| n.get(x).map(|n| format!("{}-{}", n.sosa, n.branch));
        assert_eq!(num("I1").as_deref(), Some("1-"));
        assert_eq!(num("I2").as_deref(), Some("2-"));
        assert_eq!(num("I3").as_deref(), Some("3-"));
        // Brother born before Jean (rank 1, Jean counts in the rank); half-sister from the 2nd
        // union.
        assert_eq!(num("I4").as_deref(), Some("2-1"));
        assert_eq!(num("I5").as_deref(), Some("2-B1"));
    }

    #[test]
    fn context_prefers_the_described_event() {
        let mut g = Gedcom::parse(G).unwrap();
        g.set_root("I1");
        let c = g.media_context("M1");
        assert_eq!(
            (c.event.as_str(), c.kind.as_str(), c.surname.as_str(), c.source.as_str(), c.page.as_str()),
            ("BIRT", "Birth", "GUYOT", "État civil de Troyes", "Acte n° 12")
        );
        assert_eq!(g.media_context("M2").event, "INDI");
        // Military register cited by an occupation, a military service and the death: with an equal
        // score, the first relevant folder wins.
        assert_eq!(g.media_context("M3").event, "DEAT");
        g.set_media_naming(&naming(
            "",
            vec![folder("Matricules", &["Service militaire"], ""), folder("Actes", &[], "")],
        ));
        assert_eq!(g.media_context("M3").event, "Service militaire");
        // Marriage record: the couple, the husband's number, the wife's surname.
        let c = g.media_context("M4");
        assert_eq!((c.event.as_str(), c.surname.as_str(), c.spouse.as_str()), ("MARR", "GUYOT", "DE LA RUE"));
        assert_eq!(c.number.map(|n| n.sosa), Some(2));
    }

    #[test]
    fn renders_paths() {
        let mut g = Gedcom::parse(G).unwrap();
        g.set_root("I1");
        let actes = "{sosa:6}_{branch}_{type:1}_{surname:simple}_{given}_{spouse:simple}_{n}";
        g.set_media_naming(&naming(
            "Médias",
            vec![
                folder("Actes", &["BIRT", "MARR", "DEAT"], actes),
                folder("Portraits", &["INDI"], "{sosa:6}_{surname}_{given}"),
                folder("Autres", &[], "{type}/{date} {surname} {given_names} - {place} - {page}"),
            ],
        ));
        let r = g.media_renames("M1");
        assert_eq!(r[0].2, "Médias/Actes/000001_B_GUYOT_Jean_1.jpg");
        assert_eq!(r[1].2, "Médias/Actes/000001_B_GUYOT_Jean_2.jpg");
        assert_eq!(g.media_renames("M2")[0].2, "Médias/Portraits/000001_GUYOT_Jean.png");
        assert_eq!(g.media_renames("M4")[0].2, "Médias/Actes/000002_M_GUYOT_DELARUE.jpg");
        assert_eq!(g.media_renames("M3")[0].2, "Médias/Actes/000002_1_D_GUYOT_Pierre.jpg");
        // No orphan separator, “p2” pages without {n}, no folder: path unchanged.
        let ctx = MediaContext { kind: "Naissance".into(), surname: "GUYOT".into(), ..Default::default() };
        let n = naming("", vec![folder("", &["BIRT"], "{type}/{date} {surname} {given} - {page}")]);
        let birth = MediaContext { event: "BIRT".into(), ..ctx.clone() };
        assert_eq!(media_path(&n, &birth, "a/b.JPG", Some(2)), "Naissance/GUYOT p2.jpg");
        assert_eq!(media_path(&n, &ctx, "a/b.JPG", None), "a/b.JPG");
        // Forbidden characters replaced.
        let ctx = MediaContext { source: "AD 10 : 4E/123".into(), ..Default::default() };
        assert_eq!(
            media_path(&naming("", vec![folder("", &[], "{source}")]), &ctx, "a.pdf", None),
            "AD 10 - 4E-123.pdf"
        );
    }

    #[test]
    fn the_most_specific_folder_wins() {
        let n = naming(
            "",
            vec![
                folder("Divers", &[], ""),
                folder("Actes", &["BIRT", "DEAT", "BURI"], ""),
                folder("Sépultures", &["BURI"], ""),
            ],
        );
        let at =
            |e: &str| n.folder_for(&MediaContext { event: e.into(), ..Default::default() }).map(|f| f.dir.as_str());
        assert_eq!(at("BURI"), Some("Sépultures"));
        assert_eq!(at("BIRT"), Some("Actes"));
        assert_eq!(at("CENS"), Some("Divers"));
    }
}
