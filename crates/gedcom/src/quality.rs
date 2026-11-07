//! Data quality: every check of the file, grouped by category, with their fix when it is safe.
//!
//! Consistency checks (dates, ages, family links) come from [`crate::analysis::check`]; this module
//! adds conformance to the GEDCOM 7 schema, empty items, records that nothing uses and, when the
//! caller gives access to the files ([`Files`]), the state of the media. A problem is hidden like
//! the others, with `_GR_IGNORE <code>` under its record.
//!
//! To add a check: produce [`Issue`]s with a new code in [`run`], give it a category in
//! [`category`], a label in [`crate::analysis::check::code_label`] and, if relevant, a fix in
//! [`fix_of`].

use crate::analysis::check::{self, Issue, Severity, Thresholds};
use crate::date::Date;
use crate::document::{Gedcom, RecordKind};
use crate::model::{Place, event_label, place_label, sort_key};
use crate::node::Node;
use crate::relations::mime_for;
use crate::schema::{self, Payload, StructDef};
use crate::weblink::is_web_url;
use std::borrow::Cow;
use std::collections::{HashMap, HashSet, VecDeque};

/// The two blocks of the page.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Group {
    /// Errors to fix.
    Fix,
    /// Missing data to complete.
    Complete,
}

impl Group {
    pub fn label(self) -> Cow<'static, str> {
        match self {
            Group::Fix => t!("quality.group.fix"),
            Group::Complete => t!("quality.group.complete"),
        }
    }
}

/// Category of problems, in page order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Category {
    Duplicates,
    Dates,
    Family,
    Structure,
    Empty,
    Media,
    Places,
    Sources,
    Files,
    Coords,
}

impl Category {
    pub const ALL: [Category; 10] = [
        Category::Duplicates,
        Category::Dates,
        Category::Family,
        Category::Structure,
        Category::Empty,
        Category::Media,
        Category::Places,
        Category::Sources,
        Category::Files,
        Category::Coords,
    ];

    pub fn label(self) -> Cow<'static, str> {
        match self {
            Category::Duplicates => t!("quality.category.duplicates"),
            Category::Dates => t!("quality.category.dates"),
            Category::Family => t!("quality.category.family"),
            Category::Structure => t!("quality.category.structure"),
            Category::Empty => t!("quality.category.empty"),
            Category::Media => t!("quality.category.media"),
            Category::Places => t!("quality.category.places"),
            Category::Coords => t!("quality.category.coords"),
            Category::Sources => t!("quality.category.sources"),
            Category::Files => t!("quality.category.files"),
        }
    }

    /// Stable identifier (key of the list).
    pub fn id(self) -> &'static str {
        match self {
            Category::Duplicates => "duplicates",
            Category::Dates => "dates",
            Category::Family => "family",
            Category::Structure => "structure",
            Category::Empty => "empty",
            Category::Media => "media",
            Category::Places => "places",
            Category::Coords => "coords",
            Category::Sources => "sources",
            Category::Files => "files",
        }
    }

    pub fn from_id(id: &str) -> Option<Category> {
        Self::ALL.into_iter().find(|c| c.id() == id)
    }

    pub fn group(self) -> Group {
        match self {
            Category::Sources | Category::Files | Category::Coords => Group::Complete,
            _ => Group::Fix,
        }
    }
}

/// Category of a problem code.
pub fn category(code: &str) -> Category {
    match code {
        "husband_not_male" | "wife_not_female" | "own_ancestor" | "missing_backlink" | "role_sex_mismatch" => {
            Category::Family
        }
        "broken_link" | "tag_not_allowed" | "missing_required" | "too_many" | "wrong_pointer" | "invalid_value" => {
            Category::Structure
        }
        "empty_fact" | "empty_note" | "empty_family" | "unused_record" | "disconnected" => Category::Empty,
        "missing_file" | "moved_file" | "duplicate_media" | "missing_form" => Category::Media,
        "unsourced" => Category::Sources,
        "unattached_file" => Category::Files,
        "possible_duplicate_place" | "place_hierarchy" | "bad_coords" | "place_format" => Category::Places,
        "missing_coords" => Category::Coords,
        "possible_duplicate" => Category::Duplicates,
        _ => Category::Dates,
    }
}

/// Automatic fix of a problem.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fix {
    /// Label of the button (“Delete”).
    pub label: Cow<'static, str>,
    /// Applicable to the whole list at once (“Delete All”).
    pub batch: Option<Cow<'static, str>>,
}

/// Fix offered for a code, if there is a safe one.
pub fn fix_of(code: &str) -> Option<Fix> {
    Some(match code {
        "empty_fact" => Fix { label: t!("quality.fix.delete"), batch: Some(t!("quality.fix.delete_empty_facts")) },
        "empty_note" => Fix { label: t!("quality.fix.delete"), batch: Some(t!("quality.fix.delete_empty_notes")) },
        "empty_family" => Fix { label: t!("quality.fix.delete"), batch: Some(t!("quality.fix.delete_empty_families")) },
        "unused_record" => Fix { label: t!("quality.fix.delete"), batch: None },
        "missing_backlink" => {
            Fix { label: t!("quality.fix.restore_link"), batch: Some(t!("quality.fix.restore_links")) }
        }
        "broken_link" => Fix { label: t!("quality.fix.remove_link"), batch: Some(t!("quality.fix.remove_links")) },
        "moved_file" => Fix { label: t!("quality.fix.fix_path"), batch: Some(t!("quality.fix.fix_paths")) },
        "missing_form" => Fix { label: t!("quality.fix.infer_format"), batch: Some(t!("quality.fix.infer_formats")) },
        "duplicate_media" => Fix { label: t!("quality.fix.merge"), batch: Some(t!("quality.fix.merge_all")) },
        "unattached_file" => {
            Fix { label: t!("quality.fix.create_media"), batch: Some(t!("quality.fix.create_all_media")) }
        }
        "place_hierarchy" => Fix { label: t!("quality.fix.harmonize"), batch: None },
        // Compared and merged in the merge window, never in bulk.
        "possible_duplicate" | "possible_duplicate_place" => Fix { label: t!("quality.fix.compare"), batch: None },
        _ => return None,
    })
}

/// Identifier of a problem, stable while the file does not change around it: its code, its record,
/// the position of the faulty structure and the other records or values involved
/// (`death_before_birth:I214`, `empty_fact:I12@3`, `missing_backlink:I5>F2`,
/// `missing_coords:>Agneaux, Manche`). Two problems can share it; [`issue_ids`] tells them apart.
pub fn issue_key(i: &Issue) -> String {
    let mut key = format!("{}:{}", i.code, i.xref);
    if !i.path.is_empty() {
        let path: Vec<String> = i.path.iter().map(usize::to_string).collect();
        key.push('@');
        key.push_str(&path.join("."));
    }
    if !i.related.is_empty() {
        key.push('>');
        key.push_str(&i.related.join("|"));
    }
    key
}

/// Identifiers of a list of problems, in the same order: [`issue_key`], followed by `#2`, `#3`…
/// for the following problems with the same key.
pub fn issue_ids(issues: &[Issue]) -> Vec<String> {
    let mut seen: HashMap<String, usize> = HashMap::new();
    issues
        .iter()
        .map(|i| {
            let key = issue_key(i);
            let n = seen.entry(key.clone()).or_default();
            *n += 1;
            if *n == 1 { key } else { format!("{key}#{n}") }
        })
        .collect()
}

/// Access to the media files, provided by the application (the library does not read the disk).
pub struct Files<'a> {
    /// Does the file designated by a `FILE` value exist?
    pub exists: &'a dyn Fn(&str) -> bool,
    /// Media files of the `.ged` folder, as relative paths (`Records/x.jpg`).
    pub folder: &'a [String],
}

/// Every problem of the file, hidden ones included (`ignored` field), sorted by category then
/// severity. Without access to the files.
pub fn run(g: &Gedcom) -> Vec<Issue> {
    run_with(g, None)
}

/// Like [`run`], plus the checks that look at the media files (missing, moved, unattached).
pub fn run_with(g: &Gedcom, files: Option<&Files>) -> Vec<Issue> {
    let mut issues = check::check_all(g, &Thresholds::default());
    let mut more = Vec::new();
    if g.version().starts_with('7') {
        structure(g, &mut more);
    }
    empty(g, &mut more);
    unused(g, &mut more);
    media(g, files, &mut more);
    places(g, &mut more);
    check::mark_ignored(g, &mut more);
    // Hidden with `_GR_DISTINCT`, not `_GR_IGNORE`.
    duplicates(g, &mut more);
    distinct_places(g, &mut more);
    issues.extend(more);
    issues.sort_by(|a, b| {
        (category(a.code), a.severity, a.code, &a.message, &a.path).cmp(&(
            category(b.code),
            b.severity,
            b.code,
            &b.message,
            &b.path,
        ))
    });
    issues.dedup();
    issues
}

fn issue(severity: Severity, code: &'static str, xref: &str, path: Vec<usize>, message: String) -> Issue {
    Issue {
        severity,
        code,
        xref: xref.to_string(),
        related: Vec::new(),
        message,
        detail: String::new(),
        path,
        ignored: false,
    }
}

// ---------------------------------------------------------------------------
// Possible duplicates
// ---------------------------------------------------------------------------

/// “Abraham DARMON · Abraham DARMON”: the pair, likely ones as warnings; pairs marked “not a
/// duplicate” come back as hidden issues (View › Filter List › Hidden Issues).
fn duplicates(g: &Gedcom, out: &mut Vec<Issue>) {
    use crate::duplicates::Level;
    let pair = |a: &str, b: &str| format!("{} · {}", g.record_label(a), g.record_label(b));
    for d in g.duplicates() {
        let severity = if d.level == Level::Likely { Severity::Warning } else { Severity::Info };
        let mut i = issue(severity, "possible_duplicate", &d.a, vec![], pair(&d.a, &d.b));
        i.related = vec![d.b.clone()];
        let why: Vec<&str> = d.clues.iter().filter(|c| c.is_for()).take(3).map(|c| c.text.as_str()).collect();
        i.detail = std::iter::once(d.level.label().into_owned())
            .chain(why.into_iter().map(str::to_string))
            .collect::<Vec<_>>()
            .join(" · ");
        out.push(i);
    }
    for (a, b) in g.distinct_pairs() {
        let mut i = issue(Severity::Info, "possible_duplicate", &a, vec![], pair(&a, &b));
        i.related = vec![b];
        i.ignored = true;
        out.push(i);
    }
}

// ---------------------------------------------------------------------------
// Conformance to the GEDCOM 7 schema
// ---------------------------------------------------------------------------

fn structure(g: &Gedcom, out: &mut Vec<Issue>) {
    for r in &g.records {
        let (Some(x), Some(def)) = (r.xref.as_deref(), schema::record(&r.tag)) else { continue };
        let who = g.record_label(x);
        validate(g, x, &who, r, def, &mut Vec::new(), out);
    }
}

fn validate(g: &Gedcom, x: &str, who: &str, node: &Node, def: &StructDef, path: &mut Vec<usize>, out: &mut Vec<Issue>) {
    let here = schema::label(&node.tag, def);
    for (i, c) in node.children.iter().enumerate() {
        // Extensions (`_XXX`): free, do not go into them. CONT is a continuation line, not a
        // structure.
        if c.tag.starts_with('_') || c.tag == "CONT" {
            continue;
        }
        path.push(i);
        match def.sub(&c.tag) {
            None => {
                // The genealogist's word for the tag when there is one (“Title”), else the tag.
                let what = crate::i18n::lookup(&format!("tag.{}", c.tag))
                    .or_else(|| crate::i18n::lookup(&format!("event.{}", c.tag)))
                    .unwrap_or(Cow::Borrowed(c.tag.as_str()));
                out.push(issue(
                    Severity::Warning,
                    "tag_not_allowed",
                    x,
                    path.clone(),
                    t!("quality.tag_not_allowed", who = who, what = what, here = here).into(),
                ))
            }
            Some(sub) => {
                let d = sub.def();
                if let Some(problem) = payload_problem(g, c, d.payload) {
                    let what = schema::label(&c.tag, d);
                    let code = if problem.0 { "wrong_pointer" } else { "invalid_value" };
                    out.push(issue(
                        Severity::Warning,
                        code,
                        x,
                        path.clone(),
                        t!("quality.bad_value", who = who, what = what, problem = problem.1).into(),
                    ));
                }
                validate(g, x, who, c, d, path, out);
            }
        }
        path.pop();
    }
    for sub in def.subs {
        let n = node.children.iter().filter(|c| c.tag == sub.tag).count();
        let what = schema::label(sub.tag, sub.def());
        // `FILE` without `FORM`: reported (and fixed) by `missing_form`.
        if n == 0 && sub.card.required() && !(sub.tag == "FORM" && node.tag == "FILE") {
            out.push(issue(
                Severity::Warning,
                "missing_required",
                x,
                path.clone(),
                t!("quality.missing_required", who = who, here = here, what = what, tag = sub.tag).into(),
            ));
        } else if n > 1 && sub.card.single() {
            out.push(issue(
                Severity::Warning,
                "too_many",
                x,
                path.clone(),
                t!("quality.too_many", who = who, n = n, what = what, tag = sub.tag, here = here).into(),
            ));
        }
    }
}

/// Problem with a value: `(pointer to the wrong type?, description)`.
fn payload_problem(g: &Gedcom, n: &Node, payload: Payload) -> Option<(bool, String)> {
    let v = n.val().trim();
    match payload {
        Payload::Pointer(tag) => match n.pointer() {
            Some("VOID") | None if v.is_empty() => None,
            Some("VOID") => None,
            // Link to a missing record: reported by `broken_link`.
            Some(p) => {
                let target = g.get(p)?;
                (target.tag != tag).then(|| {
                    let kind = RecordKind::from_tag(&target.tag).map(|k| k.label()).unwrap_or(t!("quality.record"));
                    let want = RecordKind::from_tag(tag).map(|k| k.label()).unwrap_or(Cow::Borrowed(tag));
                    let (kind, want) = (kind.to_lowercase(), want.to_lowercase());
                    (true, t!("quality.wrong_pointer", kind = kind, want = want).into())
                })
            }
            None => Some((false, t!("quality.not_a_pointer", value = v).into())),
        },
        Payload::Enum { list, values } => {
            if v.is_empty() {
                return None;
            }
            let items: Vec<&str> = if list { v.split(',').map(str::trim).collect() } else { vec![v] };
            let bad: Vec<&str> = items.into_iter().filter(|i| !i.starts_with('_') && !values.contains(i)).collect();
            (!bad.is_empty()).then(|| (false, t!("quality.unknown_value", value = bad.join(", ")).into()))
        }
        Payload::Date => {
            if v.is_empty() {
                return None;
            }
            match Date::parse(v) {
                Date::Other(_) => Some((false, t!("quality.unreadable_date", date = v).into())),
                d => d.validate().err().map(|e| (false, t!("quality.invalid_date", date = v, error = e).into())),
            }
        }
        Payload::YOrEmpty => (!v.is_empty() && v != "Y").then(|| (false, t!("quality.not_y", value = v).into())),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Empty items
// ---------------------------------------------------------------------------

fn note_is_empty(n: &Node) -> bool {
    n.text().trim().is_empty() && n.children.iter().all(|c| c.tag == "CONT")
}

fn empty(g: &Gedcom, out: &mut Vec<Issue>) {
    for r in &g.records {
        let Some(x) = r.xref.as_deref() else { continue };
        if r.tag == "SNOTE" {
            if r.text().trim().is_empty() {
                out.push(issue(Severity::Warning, "empty_note", x, vec![], t!("quality.empty_shared_note").into()));
            }
            continue;
        }
        let who = g.record_label(x);
        for (i, c) in r.children.iter().enumerate() {
            if let Some(label) = event_label(&c.tag)
                && matches!(r.tag.as_str(), "INDI" | "FAM")
                && c.val().trim().is_empty()
                && c.children.is_empty()
            {
                out.push(issue(
                    Severity::Warning,
                    "empty_fact",
                    x,
                    vec![i],
                    t!("quality.empty_fact", who = who, event = label).into(),
                ));
            }
        }
        // Empty notes, at any depth.
        let mut path = Vec::new();
        empty_notes(x, &who, r, &mut path, out);
    }
}

fn empty_notes(x: &str, who: &str, node: &Node, path: &mut Vec<usize>, out: &mut Vec<Issue>) {
    for (i, c) in node.children.iter().enumerate() {
        path.push(i);
        if c.tag == "NOTE" && note_is_empty(c) {
            out.push(issue(
                Severity::Warning,
                "empty_note",
                x,
                path.clone(),
                t!("quality.empty_note", who = who).into(),
            ));
        } else if !c.tag.starts_with('_') {
            empty_notes(x, who, c, path, out);
        }
        path.pop();
    }
}

// ---------------------------------------------------------------------------
// Records that nothing uses
// ---------------------------------------------------------------------------

fn unused(g: &Gedcom, out: &mut Vec<Issue>) {
    // Everything cited somewhere (except by itself).
    let mut cited: HashSet<&str> = HashSet::new();
    for (owner, n) in std::iter::once((None, &g.header)).chain(g.records.iter().map(|r| (r.xref.as_deref(), &**r))) {
        n.walk(&mut |c| {
            if let Some(p) = c.pointer()
                && Some(p) != owner
            {
                cited.insert(p);
            }
        });
    }

    for r in &g.records {
        let Some(x) = r.xref.as_deref() else { continue };
        match r.tag.as_str() {
            "FAM" => {
                let members = r.children.iter().filter(|c| matches!(c.tag.as_str(), "HUSB" | "WIFE" | "CHIL"));
                if members.filter(|c| c.pointer().is_some_and(|p| g.get(p).is_some())).count() == 0 {
                    out.push(issue(Severity::Warning, "empty_family", x, vec![], t!("quality.empty_family").into()));
                }
            }
            "OBJE" | "SOUR" | "REPO" | "SNOTE" if !cited.contains(x) => {
                // An empty note is already reported as such.
                if r.tag == "SNOTE" && r.text().trim().is_empty() {
                    continue;
                }
                let kind = RecordKind::from_tag(&r.tag).map(|k| k.label()).unwrap_or_default();
                out.push(issue(
                    Severity::Info,
                    "unused_record",
                    x,
                    vec![],
                    t!("quality.unused_record", kind = kind, name = g.record_label(x)).into(),
                ));
            }
            _ => {}
        }
    }

    // Individuals without any link to the root individual: one report per group of people connected
    // to each other (a separate tree). A link is a common family or an association (`ASSO`,
    // witnesses of a record…), in either direction.
    let Some(root) = g.root() else { return };
    let linked = |n: &Node| matches!(n.tag.as_str(), "INDI" | "FAM");
    let mut edges: HashMap<&str, Vec<&str>> = HashMap::new();
    for r in g.records.iter().filter(|r| linked(r)) {
        let Some(x) = r.xref.as_deref() else { continue };
        r.walk(&mut |c| {
            if let Some(p) = c.pointer()
                && p != x
                && matches!(c.tag.as_str(), "FAMC" | "FAMS" | "HUSB" | "WIFE" | "CHIL" | "ASSO")
                && g.get(p).is_some_and(linked)
            {
                edges.entry(x).or_default().push(p);
                edges.entry(p).or_default().push(x);
            }
        });
    }
    fn component<'a>(
        g: &'a Gedcom,
        edges: &HashMap<&'a str, Vec<&'a str>>,
        start: &str,
        seen: &mut HashSet<&'a str>,
    ) -> usize {
        let Some(start) = g.get(start).and_then(|r| r.xref.as_deref()) else { return 0 };
        let mut people = 0;
        seen.insert(start);
        let mut queue = VecDeque::from([start]);
        while let Some(x) = queue.pop_front() {
            if g.get(x).is_some_and(|r| r.tag == "INDI") {
                people += 1;
            }
            for &n in edges.get(x).into_iter().flatten() {
                if seen.insert(n) {
                    queue.push_back(n);
                }
            }
        }
        people
    }
    let mut seen: HashSet<&str> = HashSet::new();
    component(g, &edges, &root, &mut seen);
    for r in g.records_of(RecordKind::Individual) {
        let Some(x) = r.xref.as_deref() else { continue };
        if seen.contains(x) {
            continue;
        }
        let others = component(g, &edges, x, &mut seen).saturating_sub(1);
        let who = g.record_label(x);
        let message = match others {
            0 => t!("quality.no_link_to_root", who = who).into(),
            n => tn!("quality.separate_tree", n, who = who).into(),
        };
        out.push(issue(Severity::Info, "disconnected", x, vec![], message));
    }
}

// ---------------------------------------------------------------------------
// Media and files
// ---------------------------------------------------------------------------

/// `FILE` path comparable with those of the folder (`./Records\\x.jpg` → `Records/x.jpg`).
fn normalize(file: &str) -> String {
    let f = file.trim().strip_prefix("file://").unwrap_or(file.trim()).replace('\\', "/");
    f.strip_prefix("./").unwrap_or(&f).to_string()
}

fn file_name(path: &str) -> String {
    path.rsplit('/').next().unwrap_or(path).to_lowercase()
}

fn media(g: &Gedcom, files: Option<&Files>, out: &mut Vec<Issue>) {
    // Files of the folder by name, to find a moved file.
    let mut by_name: HashMap<String, Vec<&str>> = HashMap::new();
    for f in files.map(|f| f.folder).unwrap_or_default() {
        by_name.entry(file_name(f)).or_default().push(f);
    }
    let mut seen: HashMap<Vec<String>, &str> = HashMap::new();
    let mut referenced: HashSet<String> = HashSet::new();
    for r in g.records.iter().filter(|r| r.tag == "OBJE") {
        let Some(x) = r.xref.as_deref() else { continue };
        let who = g.record_label(x);
        let mut paths = Vec::new();
        for (i, f) in r.children.iter().enumerate().filter(|(_, c)| c.tag == "FILE") {
            let v = f.val().trim();
            if v.is_empty() {
                continue;
            }
            paths.push(normalize(v));
            referenced.insert(normalize(v));
            if f.child("FORM").is_none() {
                out.push(issue(
                    Severity::Warning,
                    "missing_form",
                    x,
                    vec![i],
                    t!("quality.missing_form", who = who).into(),
                ));
            }
            let Some(files) = files.filter(|_| !is_web_url(v)) else { continue };
            if (files.exists)(v) {
                continue;
            }
            match by_name.get(&file_name(&normalize(v))).map(Vec::as_slice) {
                Some([found]) => {
                    // The found file is not “to attach”: fixing the path takes care of it.
                    referenced.insert(found.to_string());
                    let mut i = issue(
                        Severity::Warning,
                        "moved_file",
                        x,
                        vec![i],
                        t!("quality.moved_file", who = who, found = found).into(),
                    );
                    i.related = vec![found.to_string()];
                    out.push(i);
                }
                _ => out.push(issue(
                    Severity::Error,
                    "missing_file",
                    x,
                    vec![i],
                    t!("quality.missing_file", who = who, file = v).into(),
                )),
            }
        }
        if paths.is_empty() {
            continue;
        }
        match seen.get(&paths) {
            Some(first) => {
                let mut i = issue(
                    Severity::Warning,
                    "duplicate_media",
                    x,
                    vec![],
                    t!("quality.duplicate_media", a = who, b = g.record_label(first)).into(),
                );
                i.related = vec![first.to_string()];
                out.push(i);
            }
            None => {
                seen.insert(paths, x);
            }
        }
    }
    // Files of the folder that no media designates.
    for f in files.map(|f| f.folder).unwrap_or_default() {
        if !referenced.contains(f) {
            let mut i = issue(Severity::Info, "unattached_file", "", vec![], f.clone());
            i.related = vec![f.clone()];
            out.push(i);
        }
    }
}

// ---------------------------------------------------------------------------
// Lieux
// ---------------------------------------------------------------------------

/// Meaningful parts of a place name, to compare two spellings: without case, accents, hyphens or
/// punctuation, without codes (postal, INSEE), “St” and “Ste” expanded. Empty parts are removed.
pub(crate) fn place_parts(name: &str) -> Vec<String> {
    name.split(',')
        .map(|part| {
            let clean: String = sort_key(part).chars().map(|c| if c.is_alphanumeric() { c } else { ' ' }).collect();
            clean
                .split_whitespace()
                .map(|w| match w {
                    "st" => "saint",
                    "ste" => "sainte",
                    w => w,
                })
                .collect::<Vec<_>>()
                .join(" ")
        })
        .filter(|p| !p.is_empty() && !p.chars().all(|c| c.is_ascii_digit() || c == ' '))
        .collect()
}

/// Is `short` an abbreviated spelling of `long` (same parts in the same order, some missing, the
/// first one identical)?
pub(crate) fn abbreviates(short: &[String], long: &[String]) -> bool {
    if short.len() >= long.len() || short.first() != long.first() {
        return false;
    }
    let mut rest = long.iter();
    short.iter().all(|s| rest.any(|l| l == s))
}

fn place_issue(code: &'static str, severity: Severity, places: &[&str], message: String) -> Issue {
    let mut i = issue(severity, code, "", vec![], message);
    i.related = places.iter().map(|p| p.to_string()).collect();
    i
}

fn places(g: &Gedcom, out: &mut Vec<Issue>) {
    let all = g.places();
    let form = g.place_form();
    for p in &all {
        // Outside the file's place format, its jurisdictions cannot be told apart (map, search,
        // completion from the gazetteer).
        if !crate::gazetteer::follows_form(&p.name, &form) {
            out.push(place_issue(
                "place_format",
                Severity::Info,
                &[&p.name],
                t!(
                    "quality.place_format",
                    place = p.name.clone(),
                    n = p.name.split(',').count(),
                    expected = form.len(),
                    form = form.join(", ")
                )
                .into(),
            ));
        }
        match (&p.latitude, &p.longitude) {
            (None, None) => out.push(place_issue("missing_coords", Severity::Info, &[&p.name], place_label(&p.name))),
            _ if p.coords().is_none() => out.push(place_issue(
                "bad_coords",
                Severity::Warning,
                &[&p.name],
                t!(
                    "quality.bad_coords",
                    place = place_label(&p.name),
                    lat = p.latitude.as_deref().unwrap_or("?"),
                    long = p.longitude.as_deref().unwrap_or("?")
                )
                .into(),
            )),
            _ => {}
        }
    }

    hierarchy(&all, out);
    let uses = |name: &str| all.iter().find(|p| p.name == name).map_or(0, |p| p.uses);
    let n = |name: &str| tn!("quality.uses", uses(name));
    for d in g.place_duplicates() {
        let severity = if d.level == crate::duplicates::Level::Likely { Severity::Warning } else { Severity::Info };
        let mut i = place_issue("possible_duplicate_place", severity, &[&d.a, &d.b], {
            let (la, lb) = place_labels(&d.a, &d.b);
            t!("quality.possible_duplicate_place", a = la, a_uses = n(&d.a), b = lb, b_uses = n(&d.b)).into()
        });
        // The message already names both places: the first clue (“X starts Y”) is left out when
        // others back it.
        let skip = usize::from(d.clues.len() > 1);
        i.detail = std::iter::once(d.level.label().into_owned())
            .chain(d.clues.into_iter().skip(skip))
            .collect::<Vec<_>>()
            .join(" · ");
        out.push(i);
    }
}

/// Pairs of places marked as different, as hidden issues (shown again from the page).
fn distinct_places(g: &Gedcom, out: &mut Vec<Issue>) {
    let all = g.places();
    let uses = |name: &str| all.iter().find(|p| p.name == name).map_or(0, |p| p.uses);
    let n = |name: &str| tn!("quality.uses", uses(name));
    for (a, b) in g.distinct_places() {
        let mut i = place_issue("possible_duplicate_place", Severity::Info, &[&a, &b], {
            let (la, lb) = place_labels(&a, &b);
            t!("quality.possible_duplicate_place", a = la, a_uses = n(&a), b = lb, b_uses = n(&b)).into()
        });
        i.ignored = true;
        out.push(i);
    }
}

/// Short labels of two places (“Leudon (Seine-et-Marne)”), unless they are the same for both.
fn place_labels(a: &str, b: &str) -> (String, String) {
    let (sa, sb) = (crate::model::short_place(a), crate::model::short_place(b));
    if sa == sb { (place_label(a), place_label(b)) } else { (sa, sb) }
}

/// Levels of a place name, at their position (empty ones included).
fn levels(name: &str) -> Vec<&str> {
    name.split(',').map(str::trim).collect()
}

/// The same division attached to two different higher divisions (“Loire” sometimes in
/// “Rhône-Alpes”, sometimes in “Auvergne-Rhône-Alpes”). The lowest level of a place (its
/// municipality, its locality) is not compared: two municipalities with the same name are often in
/// two departments.
fn hierarchy(all: &[Place], out: &mut Vec<Issue>) {
    // (parent level, parent) → value at the next level → number of places.
    let mut seen: HashMap<(usize, String), HashMap<&str, usize>> = HashMap::new();
    for p in all {
        let l = levels(&p.name);
        let Some(lowest) = l.iter().position(|x| !x.is_empty()) else { continue };
        for m in (lowest + 2)..l.len() {
            let (parent, child) = (l[m - 1], l[m]);
            let code = |x: &str| x.chars().all(|c| c.is_ascii_digit() || c == ' ');
            // Codes (postal, INSEE): two municipalities with the same name have different ones.
            if parent.is_empty() || child.is_empty() || code(parent) || code(child) {
                continue;
            }
            *seen.entry((m - 1, sort_key(parent))).or_default().entry(child).or_default() += p.uses;
        }
    }
    let mut keys: Vec<_> = seen.keys().cloned().collect();
    keys.sort();
    for key in keys {
        let values = &seen[&key];
        if values.len() < 2 {
            continue;
        }
        let mut v: Vec<(&str, usize)> = values.iter().map(|(k, n)| (*k, *n)).collect();
        v.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(b.0)));
        let (to, _) = v[0];
        let parent = all
            .iter()
            .find_map(|p| levels(&p.name).get(key.0).filter(|x| sort_key(x) == key.1).map(|x| x.to_string()))
            .unwrap_or_default();
        for (from, n) in &v[1..] {
            let mut i = place_issue(
                "place_hierarchy",
                Severity::Warning,
                &[&parent, from, to],
                tn!("quality.place_hierarchy", *n, parent = parent, from = from, to = to).into(),
            );
            i.path = vec![key.0];
            out.push(i);
        }
    }
}

// ---------------------------------------------------------------------------
// Corrections
// ---------------------------------------------------------------------------

impl Gedcom {
    /// Applies the fix of a problem. Returns `false` if it has none or if the file changed since
    /// the check.
    pub fn fix_issue(&mut self, issue: &Issue) -> bool {
        self.fix_issues(std::slice::from_ref(issue)) > 0
    }

    /// Applies the fix of several problems. Returns the number fixed.
    pub fn fix_issues(&mut self, issues: &[Issue]) -> usize {
        let mut fixable: Vec<&Issue> = issues.iter().filter(|i| fix_of(i.code).is_some()).collect();
        // Remove the structures deepest in each record first, so the positions of the others stay
        // valid; record deletions at the end.
        fixable.sort_by(|a, b| (a.path.is_empty(), &b.xref, &b.path).cmp(&(b.path.is_empty(), &a.xref, &a.path)));
        // Place variants first: harmonizing renames the places they target.
        fixable.sort_by_key(|i| i.code == "place_hierarchy");
        let mut done = 0;
        let mut touched: HashSet<String> = HashSet::new();
        for i in fixable {
            let ok = match i.code {
                "empty_fact" | "empty_note" if !i.path.is_empty() => self.remove_at(&i.xref, &i.path, &["NOTE"]),
                "empty_note" | "empty_family" | "unused_record" => self.delete(&i.xref).is_some(),
                "missing_backlink" => i.related.first().is_some_and(|f| self.add_backlink(&i.xref, f)),
                "broken_link" => i.related.first().is_some_and(|p| self.remove_pointers(&i.xref, p)),
                "moved_file" => i.related.first().is_some_and(|f| self.set_media_file(&i.xref, &i.path, f)),
                "missing_form" => self.infer_form(&i.xref, &i.path),
                "duplicate_media" => i.related.first().is_some_and(|keep| self.merge_duplicate_media(keep, &i.xref)),
                "place_hierarchy" => match (&i.related[..], i.path.first()) {
                    ([parent, from, to], Some(&level)) => self.rename_jurisdiction(level, parent, from, to) > 0,
                    _ => false,
                },
                "unattached_file" => i.related.first().is_some_and(|f| {
                    self.media_for_file(f);
                    true
                }),
                _ => false,
            };
            if ok {
                done += 1;
                touched.insert(i.xref.clone());
            }
        }
        for x in touched {
            if let Some(r) = self.get_mut(&x) {
                r.touch();
            }
        }
        done
    }

    /// Removes the substructure at `path` if it is indeed an event or one of the `also` tags.
    fn remove_at(&mut self, xref: &str, path: &[usize], also: &[&str]) -> bool {
        let Some((last, parents)) = path.split_last() else { return false };
        let Some(mut n) = self.get_mut(xref) else { return false };
        for &i in parents {
            let Some(c) = n.children.get_mut(i) else { return false };
            n = c;
        }
        let ok = n.children.get(*last).is_some_and(|c| event_label(&c.tag).is_some() || also.contains(&c.tag.as_str()));
        if ok {
            n.children.remove(*last);
        }
        ok
    }

    /// Adds the individual to the family they cite without it mentioning them.
    pub fn add_backlink(&mut self, indi: &str, fam: &str) -> bool {
        let Some(ind) = self.get(indi) else { return false };
        let p = format!("@{indi}@");
        let as_child = ind.children_with("FAMC").any(|n| n.pointer() == Some(fam));
        let as_spouse = ind.children_with("FAMS").any(|n| n.pointer() == Some(fam));
        let sex = ind.child_val("SEX").unwrap_or("").to_string();
        let Some(f) = self.get_mut(fam) else { return false };
        if f.children.iter().any(|c| c.value.as_deref() == Some(p.as_str())) {
            return false;
        }
        let tag = if as_child {
            "CHIL"
        } else if as_spouse {
            match (sex.as_str(), f.child("HUSB").is_some(), f.child("WIFE").is_some()) {
                ("F", _, false) => "WIFE",
                ("M", false, _) => "HUSB",
                (_, false, _) => "HUSB",
                (_, _, false) => "WIFE",
                _ => return false,
            }
        } else {
            return false;
        };
        f.insert_child(Node::with_value(tag, p));
        true
    }

    /// The `FILE` at `path` in the media (position among its substructures).
    fn media_file_at(&mut self, media: &str, path: &[usize]) -> Option<&mut Node> {
        let [i] = path else { return None };
        let m = self.get_mut(media).filter(|m| m.tag == "OBJE")?;
        m.children.get_mut(*i).filter(|f| f.tag == "FILE")
    }

    /// Changes the path of a media file (file moved or found by hand) and its format.
    pub fn set_media_file(&mut self, media: &str, path: &[usize], value: &str) -> bool {
        let Some(f) = self.media_file_at(media, path) else { return false };
        f.value = Some(value.to_string());
        f.set_child_val("FORM", mime_for(value));
        true
    }

    /// Infers `FORM` from the file extension.
    fn infer_form(&mut self, media: &str, path: &[usize]) -> bool {
        let Some(f) = self.media_file_at(media, path) else { return false };
        if f.child("FORM").is_some() {
            return false;
        }
        let form = mime_for(f.val());
        f.set_child_val("FORM", form);
        true
    }

    /// Merges media `dup` into `keep` (same file): links to `dup` point to `keep`, its notes and
    /// sources are added to it, then `dup` is deleted.
    pub fn merge_duplicate_media(&mut self, keep: &str, dup: &str) -> bool {
        if keep == dup || self.get(keep).is_none_or(|n| n.tag != "OBJE") {
            return false;
        }
        let Some(src) = self.get(dup).filter(|n| n.tag == "OBJE").cloned() else { return false };
        let target = self.get_mut(keep).unwrap();
        for c in src.children.iter().filter(|c| !matches!(c.tag.as_str(), "FILE" | "CREA" | "CHAN")) {
            if !target.children.contains(c) {
                target.insert_child(c.clone());
            }
        }
        target.touch();
        let p = format!("@{keep}@");
        self.rewrite_links(
            dup,
            |l| {
                l.value = Some(p.clone());
                vec![]
            },
            &[keep],
        );
        self.delete(dup).is_some()
    }

    /// In the places whose level `level` is `parent`, replaces `from` with `to` at the next level.
    /// Returns the number of places renamed.
    pub fn rename_jurisdiction(&mut self, level: usize, parent: &str, from: &str, to: &str) -> usize {
        let mut n = 0;
        for p in self.places() {
            let l = levels(&p.name);
            if l.get(level).is_some_and(|x| sort_key(x) == sort_key(parent)) && l.get(level + 1) == Some(&from) {
                let mut new: Vec<&str> = l.clone();
                new[level + 1] = to;
                let new = new.join(", ");
                let (lat, long) = (p.latitude.clone().unwrap_or_default(), p.longitude.clone().unwrap_or_default());
                self.update_place(&p.name, &new, &lat, &long);
                n += 1;
            }
        }
        n
    }

    /// Removes from the record the structures that point to `target`.
    fn remove_pointers(&mut self, xref: &str, target: &str) -> bool {
        let p = format!("@{target}@");
        let Some(r) = self.get_mut(xref) else { return false };
        fn strip(n: &mut Node, p: &str) -> usize {
            let before = n.children.len();
            n.children.retain(|c| c.value.as_deref() != Some(p));
            before - n.children.len() + n.children.iter_mut().map(|c| strip(c, p)).sum::<usize>()
        }
        strip(r, &p) > 0
    }
}
