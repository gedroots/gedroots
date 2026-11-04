//! Proposals of an AI agent: a record to review before it is written.
//!
//! An agent (connected through the app's MCP server) never modifies the tree: it describes a record
//! with the same model as record entry ([`Act`]), and the source that contains it (existing, or new
//! with its register and repository). Before it is shown, a proposal is checked
//! ([`Proposal::check`]): it must fit into the tree's structure (repository → source → file → event
//! → people). An inconsistent proposal is rejected with the reason, which the agent reads to correct
//! it. How the file writes its records (one source per act or per register, titles…) is not
//! checked: the agent learns it from the file's context ([`crate::context`]) and the user reviews
//! the proposal.
//!
//! For the review, [`Proposal::compare`] puts what the tree contains next to what is proposed, and
//! [`Proposal::diff`] gives the exact GEDCOM lines that will change, computed on a copy of the
//! file.

use crate::DateKind;
use crate::act::{Act, ActResult, ActRole, Conflicts, PersonChoice, PersonField, Suggestion};
use crate::document::{Gedcom, RecordKind};
use crate::geo::{format_latitude, format_longitude};
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
    /// Repository to create with the source, when `repository` is empty: an archive the tree does
    /// not cite yet. One of the file named the same way is used instead.
    pub new_repository: Option<NewRepository>,
    /// Call number in the repository (`CALN`).
    pub call_number: String,
}

/// New repository (`REPO` record), created with the new source that cites it.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct NewRepository {
    /// `NAME`: “Archives départementales de la Manche”.
    pub name: String,
    /// Postal address (`ADDR`), lines separated by newlines.
    pub address: String,
    /// Website (`WWW`).
    pub www: String,
}

/// Coordinates for a place the proposal names and the tree has not located yet, chosen by the
/// agent among a gazetteer's results.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Location {
    /// The place, as the proposal writes it (`PLAC`).
    pub place: String,
    pub latitude: f64,
    pub longitude: f64,
    /// Identifier in the gazetteer (`EXID` value and `TYPE` URI), e.g. an OpenStreetMap relation.
    pub exid: Option<(String, String)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SourceChoice {
    Existing(String),
    New(NewSource),
}

/// A proposed record, with its source.
#[derive(Debug, Clone, PartialEq)]
pub struct Proposal {
    pub source: SourceChoice,
    /// The record; its source (`act.source`) is replaced by the one created if it is new.
    pub act: Act,
    /// Web page of the record (the archives' viewer), kept as a web link of the source.
    pub url: String,
    /// What to do with the values that contradict the tree, chosen in To Review.
    pub conflicts: Conflicts,
    /// Coordinates of the new places it names (`PLAC.MAP`, `EXID`).
    pub locations: Vec<Location>,
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
    /// Street address of the event, within its place.
    Address,
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
    /// Fact with a value (an occupation): how the tree already writes a close one, the person's
    /// first, then the file's (“Employé de la Compagnie des chemins de fer”).
    pub close: Vec<String>,
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
    /// New places located by the proposal: the place, and its coordinates as detail.
    pub places: Vec<Link>,
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
        for l in &mut self.locations {
            l.place = ged.known_place(&l.place);
        }
        let act = &mut self.act;
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
        // A value the tree already has, written another way, takes the tree's form.
        for p in &mut act.participants {
            let linked = match &p.choice {
                PersonChoice::Link(x) => ged.get(x),
                _ => None,
            };
            for f in p.facts.iter_mut().filter(|f| f.has_value()) {
                if let Some(v) = ged.written_as(linked, &f.tag, &f.value) {
                    f.value = v;
                }
            }
        }
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
                let repository = match (&s.repository, &s.new_repository) {
                    (Some(r), _) => Some(r.clone()),
                    (None, Some(new)) => Some(ged.create_repository(new)?),
                    (None, None) => None,
                };
                let node = ged.get_mut(&x).ok_or("source not created")?;
                if let Some(r) = &repository {
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
        let mut res = act.save_with(&mut copy, &self.conflicts)?;
        for l in &self.locations {
            copy.locate_place(&l.place, &format_latitude(l.latitude), &format_longitude(l.longitude), l.exid.as_ref());
        }
        if (matches!(self.source, SourceChoice::New(_)) || !self.url.trim().is_empty())
            && !res.touched.contains(&act.source)
        {
            res.touched.insert(0, act.source.clone());
        }
        // A repository created with the source.
        if let SourceChoice::New(NewSource { repository: None, new_repository: Some(new), .. }) = &self.source
            && ged.repository_named(&new.name).is_none()
            && let Some(r) = copy.repository_named(&new.name)
            && !res.touched.contains(&r)
        {
            res.touched.insert(0, r);
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
        self.check_locations(ged)?;
        self.apply(&mut ged.clone()).map(|_| ())
    }

    fn check_source(&self, ged: &Gedcom) -> Result<(), String> {
        match &self.source {
            SourceChoice::Existing(x) => {
                if ged.get(x).is_none_or(|n| n.tag != "SOUR") {
                    return Err(format!("no source with id {x}"));
                }
            }
            SourceChoice::New(s) => {
                if let Some(r) = &s.repository
                    && ged.get(r).is_none_or(|n| n.tag != "REPO")
                {
                    return Err(format!("no repository with id {r}"));
                }
                if s.repository.is_none()
                    && let Some(new) = &s.new_repository
                    && new.name.trim().is_empty()
                {
                    return Err(
                        "give the new repository's `name` (e.g. \"Archives départementales de la Manche\")".into()
                    );
                }
                if s.register.trim().is_empty() && s.title.trim().is_empty() {
                    return Err("give the new source's register (e.g. \"Rennes, births 1802\")".into());
                }
            }
        }
        Ok(())
    }

    /// Places the proposal writes: the event's and its facts'.
    pub fn places(&self) -> Vec<&str> {
        let mut out: Vec<&str> = Vec::new();
        let facts = self.act.participants.iter().flat_map(|p| p.facts.iter().map(|f| f.place.as_str()));
        for p in std::iter::once(self.act.place.as_str()).chain(facts) {
            if !p.trim().is_empty() && !out.contains(&p) {
                out.push(p);
            }
        }
        out
    }

    /// Coordinates go to a new place of the proposal, never move one the tree has located.
    fn check_locations(&self, ged: &Gedcom) -> Result<(), String> {
        let places = self.places();
        for l in &self.locations {
            if !(-90.0..=90.0).contains(&l.latitude) || !(-180.0..=180.0).contains(&l.longitude) {
                return Err(format!(
                    "location of `{}`: latitude must be within ±90 and longitude within ±180 degrees",
                    l.place
                ));
            }
            if !places.contains(&l.place.as_str()) {
                return Err(format!(
                    "location of `{}`: the proposal names no such place (its places: {})",
                    l.place,
                    places.iter().map(|p| format!("`{p}`")).collect::<Vec<_>>().join(", ")
                ));
            }
            if let Some(p) = ged.places().into_iter().find(|p| p.name == l.place && p.coords().is_some()) {
                return Err(format!(
                    "`{}` is already located in the tree ({}, {}): leave it out of `locations` (to move it, propose_change set_coordinates)",
                    p.short(),
                    p.latitude.unwrap_or_default(),
                    p.longitude.unwrap_or_default()
                ));
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
    fn current_event(&self, ged: &Gedcom) -> Option<(String, String, String)> {
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
        Some((
            ev.child_val("DATE").unwrap_or("").to_string(),
            ev.child_val("PLAC").unwrap_or("").to_string(),
            crate::address::address_of(ev).unwrap_or_default(),
        ))
    }

    /// “In the tree” / “Proposed” lines.
    pub fn compare(&self, ged: &Gedcom) -> Vec<Row> {
        let act = &self.act;
        let (cur_date, cur_place, cur_address) = self.current_event(ged).unwrap_or_default();
        let similar = self.duplicates(ged);
        let mut rows = Vec::new();
        if !act.date.trim().is_empty() || !cur_date.is_empty() {
            rows.push(Row {
                field: Field::Date,
                change: date_change(&cur_date, &act.date),
                current: cur_date,
                proposed: act.date.trim().to_string(),
                similar: Vec::new(),
                close: Vec::new(),
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
                close: Vec::new(),
            });
        }
        if !act.address.trim().is_empty() || !cur_address.is_empty() {
            rows.push(Row {
                field: Field::Address,
                change: text_change(&cur_address, &act.address),
                current: cur_address,
                proposed: act.address.trim().to_string(),
                similar: Vec::new(),
                close: Vec::new(),
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
                close: Vec::new(),
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
                // In the tree: the person's fact of the same kind. An occupation, a residence…
                // can be attested again at another date: only the one of the same date compares.
                let kind = |c: &&Node| c.tag == f.tag && c.child_val("TYPE").unwrap_or("") == f.event_type.trim();
                let same = linked.and_then(|n| {
                    if crate::act::is_unique_event(&f.tag) {
                        return n.children.iter().find(kind);
                    }
                    let dated: Vec<&Node> =
                        n.children.iter().filter(kind).filter(|c| same_date(c.child_val("DATE"), &f.date)).collect();
                    dated.iter().find(|c| f.has_value() && loose(c.val()) == loose(&f.value)).or(dated.first()).copied()
                });
                let current = same.map(|c| crate::act::Fact {
                    tag: c.tag.clone(),
                    value: if f.has_value() { c.val().to_string() } else { String::new() },
                    date: c.child_val("DATE").unwrap_or("").to_string(),
                    place: c.child_val("PLAC").unwrap_or("").to_string(),
                    address: crate::address::address_of(c).unwrap_or_default(),
                    ..Default::default()
                });
                let change = match &current {
                    None => Change::Added,
                    Some(c) if f.has_value() => text_change(&c.value, &f.value),
                    Some(c) if !f.date.trim().is_empty() => date_change(&c.date, &f.date),
                    Some(c) if c.place.trim() == f.place.trim() => text_change(&c.address, &f.address),
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
                    close: if f.has_value() && change != Change::Same {
                        ged.close_wordings(linked, &f.tag, &f.value)
                    } else {
                        Vec::new()
                    },
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
                chain.repository = match (&s.repository, &s.new_repository) {
                    (Some(r), _) => repo_link(r, false, &s.call_number),
                    (None, Some(new)) => match ged.repository_named(&new.name) {
                        Some(r) => repo_link(&r, false, &s.call_number),
                        None => Some(Link {
                            value: new.name.trim().to_string(),
                            detail: [s.call_number.trim(), &crate::address::one_line(&new.address), new.www.trim()]
                                .into_iter()
                                .filter(|v| !v.is_empty())
                                .collect::<Vec<_>>()
                                .join(" · "),
                            created: true,
                        }),
                    },
                    (None, None) => None,
                };
                chain.same_register = ged.register_sources(&s.register).len();
                let title = if s.title.trim().is_empty() { self.act.auto_title() } else { s.title.trim().to_string() };
                chain.source = Some(Link { value: title, detail: s.register.trim().to_string(), created: true });
            }
        }
        chain.places = self
            .locations
            .iter()
            .map(|l| Link {
                value: crate::short_place(&l.place),
                detail: format!("{}, {}", format_latitude(l.latitude), format_longitude(l.longitude)),
                created: true,
            })
            .collect();
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

/// The text without case, accents, spaces or punctuation: “Staatseisenbahn-Gesellschaft” and
/// “Staatseisenbahngesellschaft” are the same words.
pub(crate) fn loose(s: &str) -> String {
    crate::media_naming::simplify(s).chars().filter(|c| c.is_alphanumeric()).flat_map(char::to_lowercase).collect()
}

/// Two wordings of the same thing, not written alike: a typo, a gender, a word more or less
/// (two thirds of the significant words shared). Different words (“Cultivateur”, “Propriétaire
/// cultivateur”) are not.
pub(crate) fn close_wording(a: &str, b: &str) -> bool {
    let (la, lb) = (loose(a), loose(b));
    if la.is_empty() || lb.is_empty() || la == lb {
        return false;
    }
    if edit_distance(&la, &lb) <= (la.chars().count().max(lb.chars().count()) / 10).max(1) {
        return true;
    }
    let words = |s: &str| -> Vec<String> {
        s.split(|c: char| !c.is_alphanumeric()).map(loose).filter(|w| w.chars().count() > 3).collect()
    };
    let (wa, wb) = (words(a), words(b));
    if wa.is_empty() || wb.is_empty() {
        return false;
    }
    let same = |t: &str, u: &str| {
        t == u
            || t.chars().count().min(u.chars().count()) >= 5
                && (t.contains(u) || u.contains(t) || edit_distance(t, u) <= 1)
    };
    let found = |x: &[String], y: &[String]| x.iter().filter(|t| y.iter().any(|u| same(t, u))).count();
    let shared = found(&wa, &wb).min(found(&wb, &wa));
    shared * 3 >= 2 * wa.len().max(wb.len())
}

fn edit_distance(a: &str, b: &str) -> usize {
    let b: Vec<char> = b.chars().collect();
    let mut row: Vec<usize> = (0..=b.len()).collect();
    for (i, x) in a.chars().enumerate() {
        let mut prev = row[0];
        row[0] = i + 1;
        for (j, y) in b.iter().enumerate() {
            let cur = row[j + 1];
            row[j + 1] = (prev + usize::from(x != *y)).min(row[j] + 1).min(cur + 1);
            prev = cur;
        }
    }
    row[b.len()]
}

/// Dates of the same day, written alike or not; or both missing.
fn same_date(current: Option<&str>, proposed: &str) -> bool {
    let (c, p) = (current.unwrap_or("").trim(), proposed.trim());
    if c.is_empty() || p.is_empty() {
        return c.is_empty() && p.is_empty();
    }
    date_change(c, p) == Change::Same
}

pub(crate) fn text_change(current: &str, proposed: &str) -> Change {
    let (c, p) = (norm(current), norm(proposed));
    if c.is_empty() {
        Change::Added
    } else if p.is_empty() || c == p || loose(&c) == loose(&p) {
        Change::Same
    } else if p.starts_with(&c) || c.starts_with(&p) {
        Change::Refined
    } else {
        Change::Differs
    }
}

/// Proposed date against the tree's: identical, more precise (within the interval of the tree's, or
/// close to an approximate date) or contrary.
pub(crate) fn date_change(current: &str, proposed: &str) -> Change {
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
            (Some(a), Some(b))
                if matches!(dc.kind(), Some(DateKind::About | DateKind::Calculated | DateKind::Estimated))
                    && (a - b).abs() <= 2 =>
            {
                Change::Refined
            }
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
    /// How the tree already writes `value` as a `tag` (an occupation…), when it is the same text with
    /// another case, accents, spaces or punctuation: the person's own form first, then the file's
    /// most frequent. `None` when the tree writes it this way already, or not at all.
    fn written_as(&self, person: Option<&Node>, tag: &str, value: &str) -> Option<String> {
        let key = loose(value);
        if key.is_empty() {
            return None;
        }
        let matching = |n: &Node| -> Vec<String> {
            n.children_with(tag).map(|c| c.val().trim()).filter(|v| loose(v) == key).map(str::to_string).collect()
        };
        let mut found = person.map(matching).and_then(|v| v.into_iter().next());
        if found.is_none() {
            let mut counts: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
            for r in self.records_of(RecordKind::Individual) {
                for v in matching(r) {
                    *counts.entry(v).or_default() += 1;
                }
            }
            found = counts.into_iter().max_by(|a, b| a.1.cmp(&b.1).then_with(|| b.0.cmp(&a.0))).map(|(v, _)| v);
        }
        found.filter(|v| v != value.trim())
    }

    /// Values of `tag` the tree writes close to `value` but not the same (“Employé des chemins de
    /// fer” and “Employé de la Compagnie des chemins de fer”, “Insitituteur” and “Instituteur”):
    /// the person's, or else the file's most frequent, at most three.
    pub fn close_wordings(&self, person: Option<&Node>, tag: &str, value: &str) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        let mut counts: Vec<(String, usize)> = Vec::new();
        let add = |n: &Node, counts: &mut Vec<(String, usize)>| {
            for c in n.children_with(tag) {
                let v = c.val().trim();
                if close_wording(v, value) {
                    match counts.iter_mut().find(|(w, _)| w == v) {
                        Some((_, k)) => *k += 1,
                        None => counts.push((v.to_string(), 1)),
                    }
                }
            }
        };
        if let Some(p) = person {
            add(p, &mut counts);
        }
        if counts.is_empty() {
            for r in self.records_of(RecordKind::Individual) {
                add(r, &mut counts);
            }
            counts.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        }
        out.extend(counts.into_iter().take(3).map(|(v, _)| v));
        out
    }

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

    /// Repository of the file with this name, written the same way or nearly (case, accents,
    /// punctuation).
    pub fn repository_named(&self, name: &str) -> Option<String> {
        let want = loose(name);
        if want.is_empty() {
            return None;
        }
        self.records_of(RecordKind::Repository)
            .find(|r| r.child_val("NAME").is_some_and(|n| loose(n) == want))
            .and_then(|r| r.xref.clone())
    }

    /// Creates a repository, or returns the one of the file with the same name.
    pub fn create_repository(&mut self, new: &NewRepository) -> Result<String, String> {
        if let Some(x) = self.repository_named(&new.name) {
            return Ok(x);
        }
        let x = self.create(RecordKind::Repository);
        let node = self.get_mut(&x).ok_or("repository not created")?;
        node.set_child_val("NAME", new.name.trim());
        node.set_child_text("ADDR", new.address.trim());
        node.set_child_val("WWW", new.www.trim());
        Ok(x)
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
pub(crate) fn line_diff(before: &str, after: &str) -> Vec<DiffLine> {
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
        Proposal { source, act, url: String::new(), conflicts: Conflicts::default(), locations: Vec::new() }
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
    fn occupation_written_otherwise_or_at_another_date_is_a_new_attestation() {
        let mut g = ged();
        let i3 = g.get_mut("I3").unwrap();
        i3.insert_child(
            Node::with_value("OCCU", "Beamter der Staatseisenbahn-Gesellschaft")
                .push(Node::with_value("DATE", "24 FEB 1908")),
        );
        i3.insert_child(Node::with_value("OCCU", "Tisserand").push(Node::with_value("DATE", "4 APR 1802")));
        let occupations = |p: &Proposal| {
            let mut g2 = g.clone();
            p.apply(&mut g2).unwrap();
            g2.get("I3")
                .unwrap()
                .children_with("OCCU")
                .map(|o| format!("{}, {}", o.val(), o.child_val("DATE").unwrap_or("")))
                .collect::<Vec<_>>()
        };
        let row = |p: &Proposal| {
            p.compare(&g).into_iter().find(|r| matches!(r.field, Field::Fact { .. })).map(|r| r.change).unwrap()
        };

        // Same words as the tree's, another spelling and date: the tree's form, a new dated fact.
        let mut p = birth(SourceChoice::Existing("S1".into()));
        p.act.participants[1].set(PersonField::Occupation, "Beamter der Staatseisenbahngesellschaft");
        p.act.date = "5 APR 1875".into();
        p.complete(&g);
        let f = &p.act.participants[1].facts[0];
        assert_eq!(f.value, "Beamter der Staatseisenbahn-Gesellschaft");
        assert_eq!(row(&p), Change::Added);
        assert!(occupations(&p).contains(&"Beamter der Staatseisenbahn-Gesellschaft, 5 APR 1875".to_string()));

        // Same date and words: already in the tree, only cited.
        let mut p = birth(SourceChoice::Existing("S1".into()));
        p.act.participants[1].set(PersonField::Occupation, "tisserand");
        p.complete(&g);
        assert_eq!(row(&p), Change::Same);
        assert_eq!(occupations(&p).len(), 2);

        // Another occupation the same day contradicts it.
        let mut p = birth(SourceChoice::Existing("S1".into()));
        p.act.participants[1].set(PersonField::Occupation, "Laboureur");
        p.complete(&g);
        assert_eq!(row(&p), Change::Differs);
    }

    #[test]
    fn close_wordings_are_a_typo_a_gender_or_a_word_more() {
        assert!(close_wording("Employé des chemins de fer", "Employé de la Compagnie des chemins de fer"));
        assert!(close_wording("Insitituteur", "Instituteur"));
        assert!(close_wording("Journalier", "Journalière"));
        assert!(!close_wording("Cultivateur", "Propriétaire cultivateur"));
        assert!(!close_wording("Garde forestier", "Garde champêtre"));
        assert!(!close_wording("Tisserand", "tisserand"));
        let mut g = ged();
        g.get_mut("I3").unwrap().insert_child(Node::with_value("OCCU", "Employé de la Compagnie des chemins de fer"));
        let mut p = birth(SourceChoice::Existing("S1".into()));
        p.act.participants[1].set(PersonField::Occupation, "Employé des chemins de fer");
        p.complete(&g);
        let row = p.compare(&g).into_iter().find(|r| matches!(r.field, Field::Fact { .. })).unwrap();
        assert_eq!(row.close, vec!["Employé de la Compagnie des chemins de fer".to_string()]);
    }

    #[test]
    fn text_compares_without_punctuation_or_accents() {
        assert_eq!(text_change("Staatseisenbahn-Gesellschaft", "Staatseisenbahngesellschaft"), Change::Same);
        assert_eq!(text_change("Propriétaire", "proprietaire"), Change::Same);
        assert_eq!(text_change("Tisserand", "Laboureur"), Change::Differs);
    }

    #[test]
    fn contradicting_date_is_settled_by_the_choice() {
        use crate::act::Conflict;
        let g = ged();
        let mut p = birth(SourceChoice::Existing("S1".into()));
        p.act.date = "4 APR 1812".into();
        p.complete(&g);
        assert_eq!(p.compare(&g)[0].change, Change::Differs);
        let births = |c: Conflict| {
            let mut p = p.clone();
            p.conflicts.event = c;
            let mut g2 = g.clone();
            p.apply(&mut g2).unwrap();
            g2.get("I2")
                .unwrap()
                .children
                .iter()
                .filter(|c| c.tag == "BIRT")
                .map(|b| (b.child_val("DATE").unwrap_or("").to_string(), b.child("SOUR").is_some()))
                .collect::<Vec<_>>()
        };
        let (old, new) = (("ABT 1802".to_string(), false), ("4 APR 1812".to_string(), true));
        assert_eq!(births(Conflict::KeepBoth), vec![old.clone(), new.clone()]);
        assert_eq!(births(Conflict::Replace), vec![new, old]);
        // The tree's date stays, the record's citation is added to it.
        assert_eq!(births(Conflict::Ignore), vec![("ABT 1802".to_string(), true)]);
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
        // How the file arranges its sources is left to the agent and the review.
        assert!(birth(SourceChoice::Existing("S1".into())).check(&g).is_ok());
        assert!(birth(SourceChoice::Existing("S9".into())).check(&g).unwrap_err().contains("no source"));
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
        // An exact date a year apart contradicts it.
        assert_eq!(date_change("4 MAY 1796", "4 MAY 1797"), Change::Differs);
        assert_eq!(date_change("", "1802"), Change::Added);
        assert_eq!(date_change("FRENCH_R 30 GERM 4", "@#DFRENCH R@ 30 GERM 4"), Change::Same);
    }

    #[test]
    fn street_addresses_stay_apart_from_the_place() {
        let g = ged();
        let mut p = birth(SourceChoice::Existing("S1".into()));
        p.act.address = "12, rue Saint-Malo".into();
        p.act.participants[2].facts.push(crate::act::Fact {
            tag: "RESI".into(),
            date: "4 APR 1802".into(),
            place: "Rennes".into(),
            address: "3, rue de la Monnaie".into(),
            ..Default::default()
        });
        p.complete(&g);
        let rows = p.compare(&g);
        let address = rows.iter().find(|r| r.field == Field::Address).unwrap();
        assert_eq!((address.change, address.proposed.as_str()), (Change::Added, "12, rue Saint-Malo"));
        let resi = rows.iter().find(|r| matches!(r.field, Field::Fact { .. })).unwrap();
        assert!(resi.proposed.contains("3, rue de la Monnaie"), "{}", resi.proposed);

        let mut g2 = g.clone();
        p.apply(&mut g2).unwrap();
        // The birth keeps its place and gains the address, right after it.
        let birth = g2.get("I2").unwrap().children.iter().find(|c| c.tag == "BIRT").unwrap();
        let tags: Vec<&str> = birth.children.iter().map(|c| c.tag.as_str()).collect();
        let at = tags.iter().position(|t| *t == "PLAC").unwrap();
        assert_eq!(tags[at + 1], "ADDR");
        assert_eq!(birth.child_val("PLAC"), Some("Rennes"));
        assert_eq!(birth.child_val("ADDR"), Some("12, rue Saint-Malo"));
        // The mother's residence: a fact with its place and address, not a new place.
        let mother = g2.records.iter().find(|r| r.child("RESI").is_some_and(|e| e.child("ADDR").is_some())).unwrap();
        let resi = mother.child("RESI").unwrap();
        assert_eq!(resi.child_val("PLAC"), Some("Rennes"));
        assert_eq!(resi.child_val("ADDR"), Some("3, rue de la Monnaie"));
        assert!(g2.places().iter().all(|pl| !pl.name.contains("rue")));

        // Read back from the source: the record and the residence keep their addresses.
        let act = g2.acts("S1").into_iter().find(|a| a.event == "BIRT" && a.page == "vue 45").unwrap();
        assert_eq!(act.address, "12, rue Saint-Malo");
        let facts: Vec<&crate::act::Fact> = act.participants.iter().flat_map(|p| &p.facts).collect();
        assert!(facts.iter().any(|f| f.tag == "RESI" && f.address == "3, rue de la Monnaie"), "{facts:?}");
    }

    #[test]
    fn new_repository_is_created_once() {
        let g = ged();
        let new = |name: &str| {
            SourceChoice::New(NewSource {
                register: "Saint-Lô, naissances 1802".into(),
                new_repository: Some(NewRepository {
                    name: name.into(),
                    address: "103, rue du Maréchal-Juin\nSaint-Lô".into(),
                    www: "https://archives.manche.fr".into(),
                }),
                call_number: "5 Mi 1".into(),
                ..Default::default()
            })
        };
        let p = birth(new("Archives départementales de la Manche"));
        p.check(&g).unwrap();
        let chain = p.chain(&g);
        let repo = chain.repository.unwrap();
        assert!(repo.created);
        assert!(repo.detail.contains("103, rue du Maréchal-Juin, Saint-Lô"), "{}", repo.detail);
        let mut g2 = g.clone();
        let res = p.apply(&mut g2).unwrap();
        let x = g2.repository_named("archives departementales de la manche").unwrap();
        assert!(res.touched.contains(&x), "{:?}", res.touched);
        let text = g2.to_gedcom_string();
        assert!(text.contains("1 NAME Archives départementales de la Manche\n1 ADDR 103, rue du Maréchal-Juin\n2 CONT Saint-Lô\n1 WWW https://archives.manche.fr\n"), "{text}");
        assert!(text.contains(&format!("1 REPO @{x}@\n2 CALN 5 Mi 1\n")), "{text}");
        // Named like a repository of the tree: that one is cited, nothing is created.
        let p = birth(new("Archives d’Ille et Vilaine"));
        assert!(!p.chain(&g).repository.unwrap().created);
        let mut g3 = g.clone();
        p.apply(&mut g3).unwrap();
        assert_eq!(g3.records_of(RecordKind::Repository).count(), 1);
        assert!(g3.to_gedcom_string().contains("1 REPO @R1@\n2 CALN 5 Mi 1"));
        // A name is required.
        assert!(birth(new(" ")).check(&g).unwrap_err().contains("name"));
    }

    #[test]
    fn locations_go_to_new_places_only() {
        let mut g = ged();
        let mut p = birth(SourceChoice::Existing("S1".into()));
        p.act.place = "Agneaux, Manche, France".into();
        let osm = ("1234".to_string(), "https://www.openstreetmap.org/relation/".to_string());
        p.locations = vec![Location {
            place: "Agneaux, Manche, France".into(),
            latitude: 49.1188,
            longitude: -1.1061,
            exid: Some(osm.clone()),
        }];
        p.complete(&g);
        p.check(&g).unwrap();
        assert_eq!(p.chain(&g).places[0].detail, "N49.1188, W1.1061");
        let mut g2 = g.clone();
        p.apply(&mut g2).unwrap();
        let text = g2.to_gedcom_string();
        assert!(
            text.contains("2 PLAC Agneaux, Manche, France\n3 MAP\n4 LATI N49.1188\n4 LONG W1.1061\n3 EXID 1234\n"),
            "{text}"
        );
        // A place the proposal does not name.
        let mut q = p.clone();
        q.locations[0].place = "Rennes".into();
        assert!(q.check(&g).unwrap_err().contains("names no such place"));
        // Out of range.
        let mut q = p.clone();
        q.locations[0].latitude = 120.0;
        assert!(q.check(&g).unwrap_err().contains("latitude"));
        // Already located in the tree: never moved by an act.
        g.update_place("Rennes", "Rennes", "N48.11", "W1.68");
        let mut q = birth(SourceChoice::Existing("S1".into()));
        q.locations = vec![Location { place: "Rennes".into(), latitude: 48.0, longitude: -1.0, exid: None }];
        assert!(q.check(&g).unwrap_err().contains("already located"));
    }
}
