//! Proposals of an AI agent: a record to review before it is written.
//!
//! An agent (connected through the app's MCP server) never modifies the tree: it describes a record
//! with the same model as record entry ([`Act`]), and the source that contains it (existing, or new
//! with its register and repository). Before it is shown, a proposal is checked
//! ([`Proposal::check`]): it must fit into the tree's structure (repository → source → file → event
//! → people) and follow the file's habits (one source per record, or per register). An inconsistent
//! proposal is rejected with the reason, which the agent reads to correct it.
//!
//! For the review, [`Proposal::compare`] puts what the tree contains next to what is proposed, and
//! [`Proposal::diff`] gives the exact GEDCOM lines that will change, computed on a copy of the
//! file.

use crate::act::{Act, ActResult, ActRole, PersonChoice, PersonField, Suggestion};
use crate::document::{Gedcom, RecordKind};
use crate::node::Node;
use crate::parse::node_to_string;

/// Score from which an individual of the tree is a plausible duplicate of a person to create
/// ([`Act::suggestions`]): same surname and first given name, or a known relative, without a date
/// that rules them out.
pub const PLAUSIBLE_MATCH: f32 = 5.5;

/// New source, created when the proposal is saved.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct NewSource {
    /// Title; empty: automatic title of the record (“Birth record of…”).
    pub title: String,
    /// Register (`PUBL`): “Rennes, births year X”.
    pub register: String,
    /// Existing repository (`REPO`).
    pub repository: Option<String>,
    /// Call number in the repository (`CALN`).
    pub call_number: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SourceChoice {
    Existing(String),
    New(NewSource),
}

/// A proposed record, with its source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Proposal {
    pub source: SourceChoice,
    /// The record; its source (`act.source`) is replaced by the one created if it is new.
    pub act: Act,
    /// Web page of the record (the archives' viewer), kept as a web link of the source.
    pub url: String,
}

/// What a line of the comparison changes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Change {
    /// Already in the tree.
    Same,
    /// Nothing in the tree: the value is added.
    Added,
    /// The tree's value is refined (more precise, compatible date).
    Refined,
    /// The proposed value contradicts the tree's.
    Differs,
    /// Person created.
    Create,
    /// Person linked to an individual of the tree.
    Link,
    /// Person only mentioned in the citation.
    Mention,
}

/// Field of a comparison line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Field {
    /// Date of the event (GEDCOM values).
    Date,
    /// Place of the event.
    Place,
    /// Person of the record (index in `act.participants`) and their displayed role.
    Person { index: usize, role: String },
    /// Other fact `fact` of the person `person` (“Birth”, of “Émile MARCHAND”).
    Fact { person: usize, fact: usize, label: String, name: String },
}

/// “In the tree” / “Proposed” line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    pub field: Field,
    pub current: String,
    pub proposed: String,
    pub change: Change,
    /// Person created: individuals of the tree they resemble (“Marie DURAND (1770–1840)”).
    pub similar: Vec<String>,
}

/// Link of the attachment chain: already exists, or will be created.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Link {
    pub value: String,
    pub detail: String,
    pub created: bool,
}

/// Where the proposal attaches in the tree.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Chain {
    pub repository: Option<Link>,
    pub source: Option<Link>,
    /// Sources of the file taken from the same register.
    pub same_register: usize,
}

/// Line of the GEDCOM diff: `'+'` added, `'-'` removed, `' '` unchanged.
pub type DiffLine = (char, String);

impl Proposal {
    /// Attaches the agent's input to the tree: a place named like a place of the file (“Agneaux”)
    /// becomes that place, and linked people take their name from the tree (record titles, file
    /// names).
    pub fn complete(&mut self, ged: &Gedcom) {
        let act = &mut self.act;
        // Dates in GEDCOM 7 syntax (`FRENCH_R 30 GERM 4`, not `@#DFRENCH R@ 30 GERM 4`).
        for d in [&mut act.date, &mut act.record_date] {
            let parsed = crate::Date::parse(d);
            if !d.trim().is_empty() && parsed.year().is_some() {
                *d = parsed.to_gedcom();
            }
        }
        act.place = ged.known_place(&act.place);
        for p in &mut act.participants {
            for f in [PersonField::Residence, PersonField::Birthplace] {
                let v = p.get(f).to_string();
                if !v.is_empty() {
                    p.set(f, ged.known_place(&v));
                }
            }
            for f in &mut p.facts {
                let parsed = crate::Date::parse(&f.date);
                if !f.date.trim().is_empty() && parsed.year().is_some() {
                    f.date = parsed.to_gedcom();
                }
                if !f.place.trim().is_empty() {
                    f.place = ged.known_place(&f.place);
                }
            }
            if let PersonChoice::Link(x) = &p.choice
                && let Some(ind) = ged.individual(x)
            {
                if p.get(PersonField::Given).is_empty() && p.get(PersonField::Surname).is_empty() {
                    let (given, surname) = (ind.given(), ind.surname());
                    p.set(PersonField::Given, given);
                    p.set(PersonField::Surname, surname);
                }
                if p.get(PersonField::Sex).is_empty() && matches!(ind.sex(), "M" | "F") {
                    let sex = ind.sex().to_string();
                    p.set(PersonField::Sex, sex);
                }
            }
        }
        // Everything that will be written is shown in the review, deduced facts included.
        act.make_facts_explicit(ged);
    }

    /// Creates the source if it is new and returns the record pointing to it (to pick it up in
    /// record entry).
    pub fn materialize(&self, ged: &mut Gedcom) -> Result<Act, String> {
        let mut act = self.act.clone();
        match &self.source {
            SourceChoice::Existing(x) => act.source = x.clone(),
            SourceChoice::New(s) => {
                let x = ged.create(RecordKind::Source);
                let node = ged.get_mut(&x).ok_or("source not created")?;
                node.children.retain(|c| c.tag != "TITL");
                if !s.title.trim().is_empty() {
                    node.insert_child(Node::with_value("TITL", s.title.trim()));
                }
                if !s.register.trim().is_empty() {
                    node.insert_child(Node::with_value("PUBL", s.register.trim()));
                }
                if let Some(r) = &s.repository {
                    let mut repo = Node::with_value("REPO", format!("@{r}@"));
                    if !s.call_number.trim().is_empty() {
                        repo = repo.push(Node::with_value("CALN", s.call_number.trim()));
                    }
                    node.insert_child(repo);
                }
                act.source = x;
            }
        }
        self.link_page(ged, &act)?;
        Ok(act)
    }

    /// Web link to the record's page, like the rest of the app: media `FILE <url>` (`FORM
    /// text/html`) linked to the source, where the record's image goes too.
    fn link_page(&self, ged: &mut Gedcom, act: &Act) -> Result<(), String> {
        if self.url.trim().is_empty() {
            return Ok(());
        }
        let url = crate::weblink::normalize_url(&self.url).map_err(|e| format!("url: {e}"))?;
        let source = match ged.get(&act.source).and_then(|n| n.child("TITL")).map(|t| t.text()) {
            Some(t) if !t.trim().is_empty() => t,
            _ => act.auto_title(),
        };
        let title =
            [source, act.page.trim().to_string()].into_iter().filter(|s| !s.is_empty()).collect::<Vec<_>>().join(", ");
        ged.add_web_link(&act.source, &url, &title)
    }

    /// Writes the proposal into the tree (source if new, then the record).
    pub fn apply(&self, ged: &mut Gedcom) -> Result<ActResult, String> {
        let mut copy = ged.clone();
        let act = self.materialize(&mut copy)?;
        let mut res = act.save(&mut copy)?;
        if (matches!(self.source, SourceChoice::New(_)) || !self.url.trim().is_empty())
            && !res.touched.contains(&act.source)
        {
            res.touched.insert(0, act.source.clone());
        }
        if let Ok(url) = crate::weblink::normalize_url(&self.url)
            && let Some(m) = copy.media_by_file(&url)
            && !res.touched.contains(&m)
            && ged.get(&m).is_none_or(|before| Some(before) != copy.get(&m))
        {
            res.touched.push(m);
        }
        *ged = copy;
        Ok(res)
    }

    /// Does the proposal fit into the tree? The error says what to correct.
    pub fn check(&self, ged: &Gedcom) -> Result<(), String> {
        self.check_source(ged)?;
        self.check_people(ged)?;
        self.apply(&mut ged.clone()).map(|_| ())
    }

    fn check_source(&self, ged: &Gedcom) -> Result<(), String> {
        match &self.source {
            SourceChoice::Existing(x) => {
                if ged.get(x).is_none_or(|n| n.tag != "SOUR") {
                    return Err(format!("no source with id {x}"));
                }
                // Source of a single record (without a reference): the file keeps one record per
                // source.
                let acts = ged.acts(x);
                if self.act.original.is_none() && acts.len() == 1 && acts[0].page.trim().is_empty() {
                    let s = crate::model::Source::new(ged.get(x).unwrap());
                    return Err(format!(
                        "source {x} (\"{}\") already holds one act: this file keeps one source per act; \
                         propose a new source with the same register and repository",
                        s.title()
                    ));
                }
            }
            SourceChoice::New(s) => {
                if let Some(r) = &s.repository
                    && ged.get(r).is_none_or(|n| n.tag != "REPO")
                {
                    return Err(format!("no repository with id {r}"));
                }
                if s.register.trim().is_empty() && s.title.trim().is_empty() {
                    return Err("give the new source's register (e.g. \"Rennes, births 1802\")".into());
                }
                // Register already described by a source that gathers several records.
                if let Some(x) = ged.register_sources(&s.register).into_iter().find(|x| ged.acts(x).len() > 1) {
                    return Err(format!(
                        "this file keeps one source per register: use source {x} with the act's reference as page"
                    ));
                }
            }
        }
        Ok(())
    }

    fn check_people(&self, ged: &Gedcom) -> Result<(), String> {
        let act = &self.act;
        for (i, p) in act.participants.iter().enumerate() {
            if let PersonChoice::Link(x) = &p.choice
                && ged.individual(x).is_none()
            {
                return Err(format!("{}: no person with id {x}", act.role_label(i)));
            }
        }
        // Parents: those of the tree are not replaced silently.
        for (i, p) in act.participants.iter().enumerate() {
            if !matches!(p.role, ActRole::Father | ActRole::Mother) || p.choice == PersonChoice::Mention || p.is_blank()
            {
                continue;
            }
            let Some(child) = self.linked_principal(p.of) else { continue };
            let (f, m) = ged.parents(&child);
            let current = if p.role == ActRole::Father { f } else { m };
            let Some(current) = current else { continue };
            if p.choice != PersonChoice::Link(current.xref().to_string()) {
                return Err(format!(
                    "{}: the tree already has {} ({}) here; link that person, or mention this one",
                    act.role_label(i),
                    current.display_name(),
                    current.xref()
                ));
            }
        }
        Ok(())
    }

    /// Individual linked as the `nth` principal person.
    fn linked_principal(&self, nth: usize) -> Option<String> {
        self.act.participants.iter().enumerate().find_map(|(i, p)| match &p.choice {
            PersonChoice::Link(x) if p.role == ActRole::Principal && self.act.nth_principal(i) == nth => {
                Some(x.clone())
            }
            _ => None,
        })
    }

    /// Event of the tree that the record completes (on the linked principal person).
    fn current_event(&self, ged: &Gedcom) -> Option<(String, String)> {
        let x = self.linked_principal(0)?;
        let node = if self.act.is_family() {
            let other = self.linked_principal(1)?;
            let ind = ged.individual(&x)?;
            let fam = ind
                .fams()
                .into_iter()
                .find(|f| ged.family(f).is_some_and(|f| [f.husband(), f.wife()].contains(&Some(other.as_str()))))?;
            ged.get(fam)?
        } else {
            ged.get(&x)?
        };
        let ev = node.children.iter().find(|c| c.tag == self.act.event)?;
        Some((ev.child_val("DATE").unwrap_or("").to_string(), ev.child_val("PLAC").unwrap_or("").to_string()))
    }

    /// “In the tree” / “Proposed” lines.
    pub fn compare(&self, ged: &Gedcom) -> Vec<Row> {
        let act = &self.act;
        let (cur_date, cur_place) = self.current_event(ged).unwrap_or_default();
        let similar = self.duplicates(ged);
        let mut rows = Vec::new();
        if !act.date.trim().is_empty() || !cur_date.is_empty() {
            rows.push(Row {
                field: Field::Date,
                change: date_change(&cur_date, &act.date),
                current: cur_date,
                proposed: act.date.trim().to_string(),
                similar: Vec::new(),
            });
        }
        if !act.place.trim().is_empty() || !cur_place.is_empty() {
            let change = text_change(&cur_place, &act.place);
            rows.push(Row {
                field: Field::Place,
                change,
                current: cur_place,
                proposed: act.place.trim().to_string(),
                similar: Vec::new(),
            });
        }
        for (i, p) in act.participants.iter().enumerate() {
            if p.is_blank() && p.choice == PersonChoice::Create {
                continue;
            }
            // In the tree: the linked person themselves, or the parent already known.
            let current = match (&p.choice, p.role) {
                (_, ActRole::Father | ActRole::Mother) => self.linked_principal(p.of).and_then(|c| {
                    let (f, m) = ged.parents(&c);
                    if p.role == ActRole::Father { f } else { m }.map(|i| i.display_name())
                }),
                (PersonChoice::Link(x), ActRole::Principal) => ged.individual(x).map(|i| i.display_name()),
                _ => None,
            }
            .unwrap_or_default();
            let (proposed, change) = match &p.choice {
                PersonChoice::Link(x) => {
                    let ind = ged.individual(x);
                    let name = ind.map(|i| format!("{} {}", i.display_name(), i.years()).trim().to_string());
                    let change = if !current.is_empty() { Change::Same } else { Change::Link };
                    (name.unwrap_or_default(), change)
                }
                PersonChoice::Create => (person_text(p), Change::Create),
                PersonChoice::Mention => (person_text(p), Change::Mention),
            };
            let similar =
                similar.iter().find(|(x, _)| *x == i).map(|(_, s)| s.iter().map(|s| s.label.clone()).collect());
            rows.push(Row {
                field: Field::Person { index: i, role: act.role_label(i).to_string() },
                current,
                proposed,
                change,
                similar: similar.unwrap_or_default(),
            });
        }
        for (i, p) in act.participants.iter().enumerate() {
            if !matches!(p.choice, PersonChoice::Create | PersonChoice::Link(_)) {
                continue;
            }
            let linked = match &p.choice {
                PersonChoice::Link(x) => ged.get(x),
                _ => None,
            };
            for (k, f) in p.facts.iter().enumerate().filter(|(_, f)| !f.is_blank()) {
                // In the tree: the person's fact of the same kind.
                let same = linked.and_then(|n| {
                    n.children
                        .iter()
                        .find(|c| c.tag == f.tag && c.child_val("TYPE").unwrap_or("") == f.event_type.trim())
                });
                let current = same.map(|c| crate::act::Fact {
                    tag: c.tag.clone(),
                    value: if f.has_value() { c.val().to_string() } else { String::new() },
                    date: c.child_val("DATE").unwrap_or("").to_string(),
                    place: c.child_val("PLAC").unwrap_or("").to_string(),
                    ..Default::default()
                });
                let change = match &current {
                    None => Change::Added,
                    Some(c) if f.has_value() => text_change(&c.value, &f.value),
                    Some(c) if !f.date.trim().is_empty() => date_change(&c.date, &f.date),
                    Some(c) => text_change(&c.place, &f.place),
                };
                rows.push(Row {
                    field: Field::Fact {
                        person: i,
                        fact: k,
                        label: f.label(),
                        name: Some(p.get(PersonField::Given))
                            .filter(|g| !g.is_empty())
                            .map(str::to_string)
                            .unwrap_or_else(|| p.display_name()),
                    },
                    current: current.map(|c| c.summary()).unwrap_or_default(),
                    proposed: f.summary(),
                    change,
                    similar: Vec::new(),
                });
            }
        }
        rows
    }

    /// People to create who may already be in the tree: index in `act.participants` and the
    /// individuals they resemble, most likely first.
    pub fn duplicates(&self, ged: &Gedcom) -> Vec<(usize, Vec<Suggestion>)> {
        self.act
            .participants
            .iter()
            .enumerate()
            .filter(|(_, p)| p.choice == PersonChoice::Create && !p.is_blank())
            .filter_map(|(i, _)| {
                let found: Vec<Suggestion> =
                    self.act.suggestions(ged, i).into_iter().filter(|s| s.score >= PLAUSIBLE_MATCH).collect();
                (!found.is_empty()).then_some((i, found))
            })
            .collect()
    }

    /// Attachment: repository and source.
    pub fn chain(&self, ged: &Gedcom) -> Chain {
        let mut chain = Chain::default();
        let repo_link = |x: &str, created: bool, caln: &str| {
            ged.get(x).map(|r| Link {
                value: r.child_val("NAME").unwrap_or(x).to_string(),
                detail: caln.to_string(),
                created,
            })
        };
        match &self.source {
            SourceChoice::Existing(x) => {
                if let Some(n) = ged.get(x) {
                    let s = crate::model::Source::new(n);
                    chain.repository =
                        s.repository().and_then(|r| repo_link(r, false, s.call_number().unwrap_or_default()));
                    let register = s.publication().unwrap_or_default();
                    chain.same_register = ged.register_sources(&register).len().saturating_sub(1);
                    chain.source = Some(Link { value: s.title(), detail: register, created: false });
                }
            }
            SourceChoice::New(s) => {
                chain.repository = s.repository.as_deref().and_then(|r| repo_link(r, false, &s.call_number));
                chain.same_register = ged.register_sources(&s.register).len();
                let title = if s.title.trim().is_empty() { self.act.auto_title() } else { s.title.trim().to_string() };
                chain.source = Some(Link { value: title, detail: s.register.trim().to_string(), created: true });
            }
        }
        chain
    }

    /// GEDCOM lines that will change, record by record.
    pub fn diff(&self, ged: &Gedcom) -> Result<Vec<DiffLine>, String> {
        let mut copy = ged.clone();
        let res = self.apply(&mut copy)?;
        let mut out = Vec::new();
        for x in &res.touched {
            let before = ged.get(x).map(node_to_string).unwrap_or_default();
            let after = copy.get(x).map(node_to_string).unwrap_or_default();
            if !out.is_empty() {
                out.push((' ', String::new()));
            }
            out.extend(line_diff(&before, &after));
        }
        Ok(out)
    }
}

fn person_text(p: &crate::act::Participant) -> String {
    let mut s = p.display_name();
    for f in [PersonField::Age, PersonField::Occupation] {
        let v = p.get(f).trim();
        if !v.is_empty() {
            s = format!("{s}, {v}");
        }
    }
    s
}

fn norm(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ").to_lowercase()
}

fn text_change(current: &str, proposed: &str) -> Change {
    let (c, p) = (norm(current), norm(proposed));
    if c.is_empty() {
        Change::Added
    } else if p.is_empty() || c == p {
        Change::Same
    } else if p.starts_with(&c) || c.starts_with(&p) {
        Change::Refined
    } else {
        Change::Differs
    }
}

/// Proposed date against the tree's: identical, more precise (within the interval of the tree's, or
/// close to an approximate date) or contrary.
fn date_change(current: &str, proposed: &str) -> Change {
    let (c, p) = (current.trim(), proposed.trim());
    if c.is_empty() {
        return Change::Added;
    }
    let (dc, dp) = (crate::Date::parse(c), crate::Date::parse(p));
    // Same date, written differently (`FRENCH_R` / `@#DFRENCH R@`, case).
    if p.is_empty() || c.eq_ignore_ascii_case(p) || dc.to_gedcom() == dp.to_gedcom() {
        return Change::Same;
    }
    match (dc.rd_range_any(), dp.rd_range_any()) {
        (Some((c0, c1)), Some((p0, p1))) if p0 >= c0 && p1 <= c1 => Change::Refined,
        _ => match (dc.year(), dp.year()) {
            // “about 1802”, “1802”: a date of the same order refines it.
            (Some(a), Some(b)) if dc.kind().is_some() && (a - b).abs() <= 2 => Change::Refined,
            _ => Change::Differs,
        },
    }
}

impl crate::Date {
    /// Interval (days) covered by the main simple date.
    fn rd_range_any(&self) -> Option<(i64, i64)> {
        self.primary()?.rd_range()
    }
}

impl Gedcom {
    /// Place of the file designated by `typed`: its full label, or its municipality if there is
    /// only one with this name. Otherwise, the input as is.
    pub fn known_place(&self, typed: &str) -> String {
        let value = self.place_value(typed, &[]);
        let t = norm(typed);
        if value != typed.trim() || t.is_empty() || t.contains(',') {
            return value;
        }
        let first = |name: &str| name.split(',').map(str::trim).find(|s| !s.is_empty()).map(norm);
        let same: Vec<String> = self
            .places()
            .into_iter()
            .filter(|p| first(&p.name).as_deref() == Some(t.as_str()))
            .map(|p| p.name)
            .collect();
        match same.as_slice() {
            [one] => one.clone(),
            _ => value,
        }
    }

    /// Sources whose register (`PUBL`) is this one.
    pub fn register_sources(&self, register: &str) -> Vec<String> {
        let r = norm(register);
        if r.is_empty() {
            return Vec::new();
        }
        self.records_of(RecordKind::Source)
            .filter(|n| n.child_val("PUBL").is_some_and(|p| norm(p) == r))
            .filter_map(|n| n.xref.clone())
            .collect()
    }
}

/// Line by line diff (longest common subsequence; records are short).
fn line_diff(before: &str, after: &str) -> Vec<DiffLine> {
    let a: Vec<&str> = before.lines().collect();
    let b: Vec<&str> = after.lines().collect();
    let (n, m) = (a.len(), b.len());
    let mut lcs = vec![vec![0u32; m + 1]; n + 1];
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            lcs[i][j] = if a[i] == b[j] { lcs[i + 1][j + 1] + 1 } else { lcs[i + 1][j].max(lcs[i][j + 1]) };
        }
    }
    let (mut i, mut j, mut out) = (0, 0, Vec::new());
    while i < n || j < m {
        if i < n && j < m && a[i] == b[j] {
            out.push((' ', a[i].to_string()));
            i += 1;
            j += 1;
        } else if j < m && (i == n || lcs[i][j + 1] >= lcs[i + 1][j]) {
            out.push(('+', b[j].to_string()));
            j += 1;
        } else {
            out.push(('-', a[i].to_string()));
            i += 1;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::act::{MediaChoice, Participant};

    const FILE: &str = "0 HEAD\n1 GEDC\n2 VERS 7.0\n\
0 @R1@ REPO\n1 NAME Archives d'Ille-et-Vilaine\n\
0 @S1@ SOUR\n1 TITL Acte de naissance de Louis BLANC\n1 PUBL Rennes, naissances an VIII\n1 REPO @R1@\n\
0 @I1@ INDI\n1 NAME Louis /BLANC/\n1 SEX M\n1 BIRT\n2 DATE 1800\n2 SOUR @S1@\n1 DEAT\n2 DATE 1870\n1 FAMC @F1@\n\
0 @I2@ INDI\n1 NAME Jean /BLANC/\n1 SEX M\n1 BIRT\n2 DATE ABT 1802\n2 PLAC Rennes\n1 DEAT\n2 DATE 1880\n1 FAMC @F1@\n\
0 @I3@ INDI\n1 NAME Pierre /BLANC/\n1 SEX M\n1 DEAT\n2 DATE 1840\n1 FAMS @F1@\n\
0 @F1@ FAM\n1 HUSB @I3@\n1 CHIL @I1@\n1 CHIL @I2@\n\
0 TRLR\n";

    fn ged() -> Gedcom {
        Gedcom::parse(FILE).unwrap()
    }

    fn birth(source: SourceChoice) -> Proposal {
        let mut act = Act::new("", "BIRT");
        act.date = "4 APR 1802".into();
        act.place = "Rennes".into();
        act.page = "vue 45".into();
        act.media = MediaChoice::None;
        act.participants = vec![
            Participant { choice: PersonChoice::Link("I2".into()), ..Participant::new(ActRole::Principal) },
            Participant { choice: PersonChoice::Link("I3".into()), ..Participant::new(ActRole::Father) },
            Participant::new(ActRole::Mother).with(PersonField::Given, "Marie").with(PersonField::Surname, "DURAND"),
        ];
        Proposal { source, act, url: String::new() }
    }

    fn new_source() -> SourceChoice {
        SourceChoice::New(NewSource {
            register: "Rennes, naissances an X".into(),
            repository: Some("R1".into()),
            call_number: "10 NUM 35238".into(),
            ..Default::default()
        })
    }

    #[test]
    fn coherent_proposal_compares_and_applies() {
        let g = ged();
        let mut p = birth(new_source());
        p.act.place = "rennes".into();
        p.act.date = "@#DFRENCH R@ 14 GERM 10".into();
        p.complete(&g);
        assert_eq!(p.act.date, "FRENCH_R 14 GERM 10");
        p.act.date = "4 APR 1802".into();
        assert_eq!(p.act.place, "Rennes");
        assert_eq!(p.act.participants[0].get(PersonField::Given), "Jean");
        p.check(&g).unwrap();
        let rows = p.compare(&g);
        assert_eq!(rows[0].field, Field::Date);
        assert_eq!(rows[0].change, Change::Refined, "{rows:?}");
        assert_eq!(rows[1].change, Change::Same);
        let father = rows.iter().find(|r| matches!(&r.field, Field::Person { index: 1, .. })).unwrap();
        assert_eq!(father.change, Change::Same);
        let mother = rows.iter().find(|r| matches!(&r.field, Field::Person { index: 2, .. })).unwrap();
        assert_eq!(mother.change, Change::Create);

        let chain = p.chain(&g);
        assert!(chain.source.as_ref().unwrap().created);
        assert_eq!(chain.repository.as_ref().unwrap().value, "Archives d'Ille-et-Vilaine");

        let diff = p.diff(&g).unwrap();
        assert!(diff.iter().any(|(c, l)| *c == '+' && l.contains("PUBL Rennes, naissances an X")));
        assert!(diff.iter().any(|(c, l)| *c == '+' && l.contains("CALN 10 NUM 35238")));
        // Date of the tree replaced: the old line removed.
        assert!(diff.iter().any(|(c, l)| *c == '-' && l.contains("DATE ABT 1802")), "{diff:?}");

        let mut g2 = g.clone();
        let res = p.apply(&mut g2).unwrap();
        let src = g2.get(&res.source).unwrap();
        assert_eq!(src.child_val("PUBL"), Some("Rennes, naissances an X"));
        // Nothing is written to the original.
        assert_eq!(g.count(RecordKind::Source), 1);
    }

    #[test]
    fn people_to_create_who_resemble_the_tree_are_flagged() {
        let g = ged();
        // Pierre BLANC, father of Jean, proposed as a new person: he is already in the tree.
        let mut p = birth(new_source());
        p.act.participants[1] =
            Participant::new(ActRole::Father).with(PersonField::Given, "Pierre").with(PersonField::Surname, "BLANC");
        let dups = p.duplicates(&g);
        assert_eq!(dups.len(), 1, "{dups:?}");
        assert_eq!(dups[0].0, 1);
        assert_eq!(dups[0].1[0].xref, "I3");
        let father = p.compare(&g).into_iter().find(|r| matches!(&r.field, Field::Person { index: 1, .. })).unwrap();
        assert!(father.similar[0].contains("Pierre BLANC"), "{father:?}");
        // Marie DURAND has no one like her; a mention is never checked.
        assert!(!dups.iter().any(|(i, _)| *i == 2));
        p.act.participants[1].choice = PersonChoice::Mention;
        assert!(p.duplicates(&g).is_empty());
        // A namesake ruled out by the dates is not a duplicate: Louis BLANC (1800–1870) is not a
        // father in 1802.
        p.act.participants[1] =
            Participant::new(ActRole::Father).with(PersonField::Given, "Louis").with(PersonField::Surname, "BLANC");
        assert!(p.duplicates(&g).is_empty(), "{:?}", p.duplicates(&g));
    }

    #[test]
    fn every_fact_written_is_a_row_of_the_review() {
        let g = ged();
        let mut p = birth(new_source());
        p.act.event = "BAPM".into();
        // Jean (I2) is born ABT 1802 in the tree: the baptism gives the exact date. The mother,
        // new, is a weaver aged 25.
        let mut b = crate::act::Fact::new("BIRT");
        b.date = "2 APR 1802".into();
        p.act.participants[0].facts.push(b);
        p.act.participants[2].set(PersonField::Occupation, "tisserande");
        p.act.participants[2].set(PersonField::Age, "25");
        p.complete(&g);
        let mother = &p.act.participants[2];
        assert_eq!(mother.get(PersonField::Occupation), "", "the deduced occupation became a fact");
        let tags: Vec<(&str, &str)> = mother.facts.iter().map(|f| (f.tag.as_str(), f.date.as_str())).collect();
        assert_eq!(tags, [("OCCU", "4 APR 1802"), ("BIRT", "EST 1777")]);
        let facts: Vec<Row> = p.compare(&g).into_iter().filter(|r| matches!(r.field, Field::Fact { .. })).collect();
        assert_eq!(facts.len(), 3, "{facts:?}");
        assert_eq!(facts[0].change, Change::Refined, "{facts:?}");
        assert!(facts[1..].iter().all(|r| r.change == Change::Added));
        // Written as before: the occupation at the record's date, the estimated birth.
        let mut copy = g.clone();
        let res = p.apply(&mut copy).unwrap();
        let m = copy.get(res.persons[2].as_ref().unwrap()).unwrap();
        assert_eq!(m.path_val(&["OCCU", "DATE"]), Some("4 APR 1802"));
        assert_eq!(m.path_val(&["BIRT", "DATE"]), Some("EST 1777"));
        assert_eq!(copy.get("I2").unwrap().path_val(&["BIRT", "DATE"]), Some("2 APR 1802"));
        // Back in record entry: the deduced facts return to their fields.
        let mut act = p.act.clone();
        act.fold_deduced_facts();
        assert_eq!(act.participants[2].get(PersonField::Occupation), "tisserande");
        assert!(act.participants[2].facts.is_empty() && act.estimate_births);
        assert_eq!(act.participants[0].facts.len(), 1, "the baptism's birth date stays a fact");
    }

    #[test]
    fn incoherent_proposals_are_refused_with_reason() {
        let g = ged();
        // Source of a single record: the file keeps one record per source.
        let e = birth(SourceChoice::Existing("S1".into())).check(&g).unwrap_err();
        assert!(e.contains("one source per act"), "{e}");
        // Unknown repository.
        let mut s = NewSource { register: "X".into(), repository: Some("R9".into()), ..Default::default() };
        let e = birth(SourceChoice::New(s.clone())).check(&g).unwrap_err();
        assert!(e.contains("no repository"), "{e}");
        s.repository = None;
        // Another father than the tree's.
        let mut p = birth(SourceChoice::New(s));
        p.act.participants[1] = Participant::new(ActRole::Father).with(PersonField::Given, "Paul");
        let e = p.check(&g).unwrap_err();
        assert!(e.contains("Pierre BLANC"), "{e}");
        // Impossible date (record entry check).
        let mut p = birth(new_source());
        p.act.date = "31 FEB 1802".into();
        assert!(p.check(&g).is_err());
    }

    #[test]
    fn custom_event_keeps_its_type() {
        let g = ged();
        let mut p = birth(new_source());
        p.act.event = "EVEN".into();
        p.act.date = "1822".into();
        p.act.participants.truncate(1);
        p.complete(&g);
        // EVEN without a type is refused, with the reason.
        assert!(p.check(&g).is_err());
        p.act.event_type = "Military service".into();
        p.check(&g).unwrap();
        let mut g2 = g.clone();
        p.apply(&mut g2).unwrap();
        let even = g2.get("I2").unwrap().children.iter().find(|c| c.tag == "EVEN").unwrap();
        assert_eq!(even.child_val("TYPE"), Some("Military service"));
        assert_eq!(even.child_val("DATE"), Some("1822"));
    }

    #[test]
    fn act_page_becomes_a_web_link_of_the_source() {
        let g = ged();
        let mut p = birth(new_source());
        p.url = "https://archives.ille-et-vilaine.fr/ark:/12345/v45".into();
        p.check(&g).unwrap();
        let diff = p.diff(&g).unwrap();
        assert!(
            diff.iter()
                .any(|(c, l)| *c == '+' && l.contains("FILE https://archives.ille-et-vilaine.fr/ark:/12345/v45")),
            "{diff:?}"
        );
        assert!(diff.iter().any(|(c, l)| *c == '+' && l.contains("FORM text/html")));
        let mut after = g.clone();
        let res = p.apply(&mut after).unwrap();
        let source = after.get(&res.touched[0]).unwrap().xref.clone().unwrap();
        let links = after.web_links(&source);
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].title.as_deref(), Some(format!("{}, vue 45", p.act.auto_title()).as_str()));
        p.url = "not a url".into();
        assert!(p.check(&g).unwrap_err().starts_with("url:"));
    }

    #[test]
    fn act_image_and_web_page_stay_two_media() {
        let g = ged();
        let mut p = birth(new_source());
        p.act.media = MediaChoice::NewFiles(vec!["/tmp/cache/acte p2.jpg".into()]);
        p.url = "https://archives.marne.fr/ark:/86869/r6w32ml0z8k7/36f5".into();
        let mut after = g.clone();
        let res = p.apply(&mut after).unwrap();
        let source = res.touched[0].clone();
        let image = res.media.clone().unwrap();
        let image_media = crate::model::Media::new(after.get(&image).unwrap());
        assert!(image_media.is_image() && !image_media.is_web());
        assert_eq!(image_media.files().len(), 1);
        let links = after.web_links(&source);
        assert_eq!(links.len(), 1);
        assert!(matches!(&links[0].kind, crate::WebLinkKind::Media(m) if *m != image));
        assert_eq!(after.images_of(&source).len(), 1);
        // Read back, the act's image is the image, never the web page.
        let act = after.acts(&source).into_iter().next().unwrap();
        assert_eq!(act.media, MediaChoice::Existing(image));
        // And a web page cannot be chosen as an act's image.
        let mut wrong = act.clone();
        let crate::WebLinkKind::Media(web) = &links[0].kind else { unreachable!() };
        wrong.media = MediaChoice::Existing(web.clone());
        assert!(wrong.validate(&after).is_err());
    }

    #[test]
    fn diff_marks_added_and_removed_lines() {
        let d = line_diff("a\nb\nc\n", "a\nc\nd\n");
        assert_eq!(d, vec![(' ', "a".into()), ('-', "b".into()), (' ', "c".into()), ('+', "d".into())]);
        assert_eq!(date_change("ABT 1802", "4 APR 1802"), Change::Refined);
        assert_eq!(date_change("1802", "4 APR 1802"), Change::Refined);
        assert_eq!(date_change("1810", "4 APR 1802"), Change::Differs);
        assert_eq!(date_change("", "1802"), Change::Added);
        assert_eq!(date_change("FRENCH_R 30 GERM 4", "@#DFRENCH R@ 30 GERM 4"), Change::Same);
    }
}
