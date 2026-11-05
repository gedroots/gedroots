//! Research leads: the records still to find in the tree, with an estimated period and where to
//! look.
//!
//! A lead is computed, never stored: a birth, marriage or death that is missing or has no source,
//! or the parents of an ancestor at the end of a line. It disappears as soon as the record is
//! cited. What the researcher (or an agent) did about it is kept in the file
//! ([`crate::research_log`]): a plan, the searches made, including those that found nothing.
//!
//! For each lead:
//!
//! - **period**: the years where the record should be, from everything that dates the person
//!   (their events, the events they attended as a witness, their children's births, a spouse's
//!   remarriage), narrowed by the searches that found nothing;
//! - **places**: where to look first (the last known residence for a death, the bride's parish for
//!   a marriage…);
//! - **where to look**: the kinds of records that cover this period in this country (rules below,
//!   detailed for France, generic elsewhere), each with the registers of the same place the file
//!   already cites, so that the research starts from the archive the file already uses.
//!
//! Labels are in English: they are read by agents; the interface
//! translates them from their stable identifiers ([`RecordType::id`]).

use crate::analysis::span::Span;
use crate::atlas::{self, Country, Sex};
use crate::document::{Gedcom, RecordKind};
use crate::model::{Individual, event_label, sort_key};
use crate::research::PersonRef;
use crate::research_log::{SearchResult, TaskState};
use std::collections::{BTreeMap, HashMap};

/// Record to find.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Deserialize)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum Goal {
    Birth,
    Marriage,
    Death,
    /// The parents of a person whose parents are unknown.
    Parents,
}

impl Goal {
    pub const ALL: [Goal; 4] = [Goal::Birth, Goal::Marriage, Goal::Death, Goal::Parents];

    /// Value of `_GR_TASK` in the file.
    pub fn tag(self) -> &'static str {
        match self {
            Goal::Birth => "BIRT",
            Goal::Marriage => "MARR",
            Goal::Death => "DEAT",
            Goal::Parents => "FAMC",
        }
    }

    pub fn from_tag(tag: &str) -> Option<Goal> {
        Self::ALL.into_iter().find(|g| g.tag() == tag.trim())
    }

    /// English name, for agents.
    pub fn name(self) -> &'static str {
        match self {
            Goal::Birth => "birth",
            Goal::Marriage => "marriage",
            Goal::Death => "death",
            Goal::Parents => "parents",
        }
    }

    /// Events that count as this record (a baptism for a birth, a burial for a death).
    fn tags(self) -> &'static [&'static str] {
        match self {
            Goal::Birth => &["BIRT", "BAPM", "CHR"],
            Goal::Marriage => &["MARR"],
            Goal::Death => &["DEAT", "BURI", "CREM"],
            Goal::Parents => &[],
        }
    }
}

/// Why the record is a lead.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, schemars::JsonSchema))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum LeadKind {
    /// The event (or the parents) is not in the file.
    Missing,
    /// The event is there, without a source.
    Unsourced,
}

/// Range of years, bounds included.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, schemars::JsonSchema))]
pub struct Years {
    pub from: i32,
    pub to: i32,
}

impl Years {
    pub fn new(a: i32, b: i32) -> Self {
        Years { from: a.min(b), to: a.max(b) }
    }

    fn clip(self, from: Option<i32>, to: Option<i32>) -> Option<Years> {
        let a = from.map_or(self.from, |f| self.from.max(f));
        let b = to.map_or(self.to, |t| self.to.min(t));
        (a <= b).then_some(Years { from: a, to: b })
    }

    fn overlaps(self, from: i32, to: i32) -> bool {
        self.from <= to && from <= self.to
    }
}

impl std::fmt::Display for Years {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self.from == self.to {
            true => write!(f, "{}", self.from),
            false => write!(f, "{}–{}", self.from, self.to),
        }
    }
}

/// Kind of records to search.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Deserialize)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum RecordType {
    /// Civil registers (births, marriages, deaths).
    CivilRegistration,
    /// Indexes of the civil registers by period (French decennial tables, GRO indexes).
    RegistrationIndex,
    /// Parish or church registers (baptisms, marriages, burials).
    ChurchRegisters,
    /// Census lists: presence or absence of the person in the household narrows the period.
    Census,
    /// Probate and inheritance records.
    Probate,
    /// Marriage contracts, in notarial records.
    MarriageContracts,
    /// Military registers (French registres matricules), which name the parents.
    MilitaryRegisters,
    /// National index of deaths (French INSEE file of deceased persons, from 1970).
    DeathIndex,
    /// Population registers: every household of a municipality and its moves (Belgium, the
    /// Netherlands).
    PopulationRegisters,
}

impl RecordType {
    /// Stable identifier (translation key of the interface).
    pub fn id(self) -> &'static str {
        match self {
            RecordType::CivilRegistration => "civil_registration",
            RecordType::RegistrationIndex => "registration_index",
            RecordType::ChurchRegisters => "church_registers",
            RecordType::Census => "census",
            RecordType::Probate => "probate",
            RecordType::MarriageContracts => "marriage_contracts",
            RecordType::MilitaryRegisters => "military_registers",
            RecordType::DeathIndex => "death_index",
            RecordType::PopulationRegisters => "population_registers",
        }
    }

    /// English name, for agents.
    pub fn name(self) -> &'static str {
        match self {
            RecordType::CivilRegistration => "Civil registration",
            RecordType::RegistrationIndex => "Registration indexes",
            RecordType::ChurchRegisters => "Church registers",
            RecordType::Census => "Census lists",
            RecordType::Probate => "Probate records",
            RecordType::MarriageContracts => "Marriage contracts",
            RecordType::MilitaryRegisters => "Military registers",
            RecordType::DeathIndex => "Death index",
            RecordType::PopulationRegisters => "Population registers",
        }
    }
}

/// A register of the same place that the file already cites.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, schemars::JsonSchema))]
pub struct UsedRegister {
    /// Register or collection (`PUBL`), otherwise the source's title.
    pub register: String,
    /// A source citing it, to reuse in `propose_act`.
    pub source_id: String,
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "Option::is_none"))]
    pub repository_id: Option<String>,
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "String::is_empty"))]
    pub repository: String,
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "String::is_empty"))]
    pub call_number: String,
    /// Events of this place citing it.
    pub cited: usize,
    /// Years of those events.
    pub years: Option<Years>,
}

/// Where to look: a kind of records, a place and years.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, schemars::JsonSchema))]
pub struct WhereToLook {
    pub record: RecordType,
    /// “Decennial tables (tables décennales): deaths”.
    pub label: String,
    /// Name in the country (“tables décennales”), for the interface.
    #[cfg_attr(feature = "serde", serde(skip))]
    pub local_name: String,
    pub place: String,
    pub years: Option<Years>,
    /// Census years, for `census`.
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "Vec::is_empty"))]
    pub census_years: Vec<i32>,
    /// Last year open to everyone, when the years go beyond it: later records are given on request
    /// to the people concerned and their descendants (French births and marriages: 75 years).
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "Option::is_none"))]
    pub public_until: Option<i32>,
    /// Registers of this place the file already cites, most cited first.
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "Vec::is_empty"))]
    pub used_registers: Vec<UsedRegister>,
}

/// A search already made, from the file's research log.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, schemars::JsonSchema))]
pub struct PastSearch {
    /// Day of the search (GEDCOM date).
    pub date: String,
    /// What was searched: register, place, years, names.
    pub searched: String,
    pub result: SearchResult,
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "Option::is_none"))]
    pub years: Option<Years>,
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "String::is_empty"))]
    pub place: String,
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "String::is_empty"))]
    pub url: String,
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "String::is_empty"))]
    pub note: String,
    /// Noted by an AI agent.
    pub by_agent: bool,
}

/// How a person was seen alive, for [`Reason::Seen`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Sighting {
    /// One of their events or of their unions (event tag).
    Event(String),
    /// Present at someone else's event (tag, whose).
    Witness { tag: String, whose: String },
    /// Birth of their child (name).
    ChildBirth(String),
}

/// What an estimated period rests on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Reason {
    /// The record's date is known (a birth, marriage or death without a source).
    Dated(Goal, Years),
    /// Seen alive in a year.
    Seen(Sighting, i32),
    /// A parent was born that year.
    ParentBorn(i32),
    /// The person was born in these years.
    Born(Years),
    /// A spouse (name) remarried that year.
    Remarried(String, i32),
    /// The spouses were born from that year.
    SpousesBornFrom(i32),
    /// The first child (name) was born that year.
    FirstChild(String, i32),
    /// Searched in vain over these years.
    NotFound(Years),
}

/// Name of an event in a language, lowercase (“marriage”, « mariage »).
fn event_in(tag: &str, lang: &str) -> String {
    crate::i18n::lookup_in(&format!("event.{tag}"), lang)
        .map(|l| l.to_lowercase())
        .unwrap_or_else(|| tag.to_lowercase())
}

impl std::fmt::Display for Reason {
    /// English, for agents: “present at the marriage of Louis MARCHAND in 1852”.
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            Reason::Dated(goal, y) => write!(f, "{} dated {y}", goal.name()),
            Reason::Seen(Sighting::Event(tag), y) => write!(f, "{} in {y}", event_in(tag, "en")),
            Reason::Seen(Sighting::Witness { tag, whose }, y) => {
                write!(f, "present at the {} of {whose} in {y}", event_in(tag, "en"))
            }
            Reason::Seen(Sighting::ChildBirth(name), y) => write!(f, "birth of their child {name} in {y}"),
            Reason::ParentBorn(y) => write!(f, "a parent born in {y}"),
            Reason::Born(y) => write!(f, "born {y}"),
            Reason::Remarried(name, y) => write!(f, "{name} remarried in {y}"),
            Reason::SpousesBornFrom(y) => write!(f, "spouses born from {y}"),
            Reason::FirstChild(name, y) => write!(f, "first child {name} born in {y}"),
            Reason::NotFound(y) => write!(f, "not found in {y}"),
        }
    }
}

impl Reason {
    /// In the interface's language: « Naissance de son enfant Yvonne OHL en 1903 ».
    pub fn label(&self) -> String {
        let lang = crate::i18n::language();
        let text = match self {
            Reason::Dated(goal, y) => match goal {
                Goal::Birth => t!("leads.reason.birth_dated", years = y),
                Goal::Marriage => t!("leads.reason.marriage_dated", years = y),
                _ => t!("leads.reason.death_dated", years = y),
            },
            Reason::Seen(Sighting::Event(tag), y) => {
                t!("leads.reason.seen", event = event_in(tag, &lang), year = y)
            }
            Reason::Seen(Sighting::Witness { tag, whose }, y) => {
                t!("leads.reason.witness", event = event_in(tag, &lang), whose = whose, year = y)
            }
            Reason::Seen(Sighting::ChildBirth(name), y) => t!("leads.reason.child_birth", name = name, year = y),
            Reason::ParentBorn(y) => t!("leads.reason.parent_born", year = y),
            Reason::Born(y) => t!("leads.reason.born", years = y),
            Reason::Remarried(name, y) => t!("leads.reason.remarried", name = name, year = y),
            Reason::SpousesBornFrom(y) => t!("leads.reason.spouses_born", year = y),
            Reason::FirstChild(name, y) => t!("leads.reason.first_child", name = name, year = y),
            Reason::NotFound(y) => t!("leads.reason.not_found", years = y),
        };
        let mut c = text.chars();
        c.next().map(|f| f.to_uppercase().chain(c).collect()).unwrap_or_default()
    }
}

#[cfg(feature = "serde")]
fn reasons_as_text<S: serde::Serializer>(reasons: &[Reason], s: S) -> Result<S::Ok, S::Error> {
    s.collect_seq(reasons.iter().map(ToString::to_string))
}

/// A record to find.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, schemars::JsonSchema))]
pub struct Lead {
    pub person: PersonRef,
    pub goal: Goal,
    pub kind: LeadKind,
    /// Sosa number, for a direct ancestor of the root person.
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "Option::is_none"))]
    pub sosa: Option<u64>,
    /// For a marriage: the family and the spouse.
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "Option::is_none"))]
    pub family_id: Option<String>,
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "Option::is_none"))]
    pub spouse: Option<PersonRef>,
    /// Date and place already known (event without a source).
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "String::is_empty"))]
    pub date: String,
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "String::is_empty"))]
    pub place: String,
    /// Estimated years of the record.
    pub years: Option<Years>,
    /// What the estimate rests on (“seen alive in 1852”, “born about 1797”): English text for
    /// agents, translated by [`Reason::label`] in the interface.
    #[cfg_attr(feature = "serde", serde(serialize_with = "reasons_as_text"))]
    #[cfg_attr(feature = "serde", schemars(with = "Vec<String>"))]
    pub years_reasons: Vec<Reason>,
    /// Places to search, best first.
    pub places: Vec<String>,
    pub where_to_look: Vec<WhereToLook>,
    /// State in the research log: suggested (nothing noted yet), planned, not found, found,
    /// ignored.
    pub state: TaskState,
    /// Why it was planned, or why it was ignored.
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "String::is_empty"))]
    pub note: String,
    /// Searches already made, oldest first.
    pub searches: Vec<PastSearch>,
}

impl Lead {
    /// Record holding the research log of this lead: the family for a marriage, otherwise the
    /// person.
    pub fn owner(&self) -> &str {
        self.family_id.as_deref().filter(|_| self.goal == Goal::Marriage).unwrap_or(&self.person.id)
    }

    /// “Death of Jean MARCHAND”, “Marriage of Jean MARCHAND and Rose LEFÈVRE”.
    pub fn title(&self) -> String {
        match (self.goal, &self.spouse) {
            (Goal::Marriage, Some(s)) => format!("Marriage of {} and {}", self.person.name, s.name),
            (Goal::Birth, _) => format!("Birth of {}", self.person.name),
            (Goal::Marriage, None) => format!("Marriage of {}", self.person.name),
            (Goal::Death, _) => format!("Death of {}", self.person.name),
            (Goal::Parents, _) => format!("Parents of {}", self.person.name),
        }
    }
}

/// Default focus: the root person's ancestors over six generations.
pub const DEFAULT_FOCUS: &str = "sosa:1-63";

/// Oldest age considered for a death without a known date.
const MAX_AGE: i32 = 105;

/// A dated sighting of a person, with its place.
#[derive(Debug, Clone)]
struct Seen {
    /// Year the person was alive.
    year: i32,
    /// Year of the event, for the reason (a father is alive 9 months before the birth).
    shown: i32,
    place: String,
    what: Sighting,
}

/// Municipality of a place, normalized (“Saint-Lô, Manche” → `saint-lo`): the municipality
/// jurisdiction of the place format (`town`: its index and the format's length) when the place
/// follows it, so that a locality counts with its municipality, otherwise the first filled level.
fn place_key(p: &str, town: Option<(usize, usize)>) -> String {
    let parts: Vec<&str> = p.split(',').map(str::trim).collect();
    if let Some((i, len)) = town
        && parts.len() == len
        && !parts[i].is_empty()
    {
        return town_text(parts[i]);
    }
    town_text(p.split([',', '(']).map(str::trim).find(|s| !s.is_empty()).unwrap_or(""))
}

/// Whether a normalized text names a municipality (a whole word: `troyes`, not `troyesville`).
fn names(text: &str, town: &str) -> bool {
    !town.is_empty()
        && text.match_indices(town).any(|(i, _)| {
            let before = text[..i].chars().next_back();
            let after = text[i + town.len()..].chars().next();
            !before.is_some_and(char::is_alphanumeric) && !after.is_some_and(char::is_alphanumeric)
        })
}

/// A source cited by the events of a town, maybe one of its registers.
struct Candidate {
    reg: UsedRegister,
    /// Its title or publication names the town.
    named: bool,
    /// Titles and publications of the sources filed under it ([`town_text`]).
    texts: Vec<String>,
}

/// Text normalized to compare town names: accents, case, hyphens and apostrophes, `St`/`Ste`
/// (“St Étienne” and “Saint-Etienne” → `saint etienne`).
fn town_text(s: &str) -> String {
    sort_key(s)
        .replace(['-', '\'', '’', '.'], " ")
        .split_whitespace()
        .map(|w| match w {
            "st" => "saint",
            "ste" => "sainte",
            w => w,
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Years written in a text (“Naissances (1863-1892)” → 1863–1892), between 1500 and 2100.
fn period_in(text: &str) -> Option<Years> {
    let years: Vec<i32> = text
        .split(|c: char| !c.is_ascii_digit())
        .filter(|w| w.len() == 4)
        .filter_map(|w| w.parse().ok())
        .filter(|y| (1500..=2100).contains(y))
        .collect();
    Some(Years::new(*years.iter().min()?, *years.iter().max()?))
}

/// Letters and digits only, to compare call numbers and titles (“5MI 3679 - Décès”).
fn compact(s: &str) -> String {
    sort_key(s).chars().filter(|c| c.is_alphanumeric()).collect()
}

/// What the file says, read once for every lead.
struct Context<'g> {
    g: &'g Gedcom,
    this_year: i32,
    sosa: HashMap<String, u64>,
    /// Sightings of each person (their events, family events, events attended, children born).
    seen: HashMap<String, Vec<Seen>>,
    /// Registers cited by the events of each place: place key → register key → register.
    registers: HashMap<String, BTreeMap<String, UsedRegister>>,
    /// Sources cited by the events of each place, before [`Context::keep_registers`].
    candidates: HashMap<String, BTreeMap<String, Candidate>>,
    /// Towns of the file (place keys).
    towns: std::collections::HashSet<String>,
    /// Registers cited, by place key and event tag, to rank those of the same kind first.
    register_tags: HashMap<(String, String), BTreeMap<String, usize>>,
    /// Country of places that name none: the most frequent one of the file.
    default_country: &'static Country,
    /// Index of the municipality in the place format, and the format's length.
    town: Option<(usize, usize)>,
}

impl<'g> Context<'g> {
    fn new(g: &'g Gedcom, this_year: i32) -> Self {
        let sosa =
            g.sosa_numbers().into_iter().filter(|(_, n)| n.branch.is_empty()).map(|(x, n)| (x, n.sosa)).collect();
        let mut ctx = Context {
            g,
            this_year,
            sosa,
            seen: HashMap::new(),
            registers: HashMap::new(),
            candidates: HashMap::new(),
            towns: Default::default(),
            register_tags: HashMap::new(),
            default_country: atlas::generic(),
            town: None,
        };
        let form = g.place_form();
        ctx.town = form
            .iter()
            .position(|f| crate::gazetteer::Level::of(f) == crate::gazetteer::Level::Municipality)
            .map(|i| (i, form.len()));
        ctx.read();
        ctx
    }

    fn read(&mut self) {
        let g = self.g;
        let mut countries: HashMap<&'static str, (&'static Country, usize)> = HashMap::new();
        let name = |x: &str| g.individual(x).map(|i| i.display_name()).unwrap_or_default();
        for r in g.records_of(RecordKind::Individual).chain(g.records_of(RecordKind::Family)) {
            let Some(owner) = r.xref.as_deref() else { continue };
            let principals: Vec<&str> = match r.tag.as_str() {
                "FAM" => r
                    .children
                    .iter()
                    .filter(|c| c.tag == "HUSB" || c.tag == "WIFE")
                    .filter_map(|c| c.pointer())
                    .collect(),
                _ => vec![owner],
            };
            for ev in r.children.iter().filter(|e| event_label(&e.tag).is_some()) {
                let place = ev.path_val(&["PLAC"]).unwrap_or("").to_string();
                if let Some(c) = atlas::in_place(&place) {
                    countries.entry(c.code.as_str()).or_insert((c, 0)).1 += 1;
                }
                let years = Span::years_of(ev.path_val(&["DATE"]).unwrap_or(""));
                // Registers cited for this place, by the events its civil or church registers
                // record (a military register or a census cited by another event is not one).
                if !place.is_empty() {
                    self.towns.insert(self.key(&place));
                }
                let recorded =
                    [Goal::Birth, Goal::Marriage, Goal::Death].iter().any(|g| g.tags().contains(&ev.tag.as_str()));
                if !place.is_empty() && recorded {
                    for s in ev.children_with("SOUR").filter_map(|s| s.pointer()) {
                        self.cite(s, &place, &ev.tag, years);
                    }
                }
                let Some((lo, _)) = years else { continue };
                // A dead person is not seen alive at their own death or burial.
                if !Goal::Death.tags().contains(&ev.tag.as_str()) {
                    for p in &principals {
                        self.seen.entry(p.to_string()).or_default().push(Seen {
                            year: lo,
                            shown: lo,
                            place: place.clone(),
                            what: Sighting::Event(ev.tag.clone()),
                        });
                    }
                }
                for a in ev.children_with("ASSO").filter_map(|a| a.pointer()) {
                    if principals.contains(&a) {
                        continue;
                    }
                    let whose = principals.iter().map(|p| name(p)).collect::<Vec<_>>().join(" and ");
                    self.seen.entry(a.to_string()).or_default().push(Seen {
                        year: lo,
                        shown: lo,
                        place: place.clone(),
                        what: Sighting::Witness { tag: ev.tag.clone(), whose: whose.clone() },
                    });
                }
            }
        }
        // A parent is alive at their children's birth (a father nine months before).
        for r in g.records_of(RecordKind::Family) {
            let fam = crate::model::Family::new(r);
            for c in fam.children() {
                let Some(child) = g.individual(c) else { continue };
                let Some(b) = child.birth() else { continue };
                let Some((lo, _)) = b.date_raw().and_then(Span::years_of) else { continue };
                let place = b.place().unwrap_or("").to_string();
                let what = Sighting::ChildBirth(child.display_name());
                if let Some(h) = fam.husband() {
                    self.seen.entry(h.to_string()).or_default().push(Seen {
                        year: lo - 1,
                        shown: lo,
                        place: place.clone(),
                        what: what.clone(),
                    });
                }
                if let Some(w) = fam.wife() {
                    self.seen.entry(w.to_string()).or_default().push(Seen { year: lo, shown: lo, place, what });
                }
            }
        }
        self.keep_registers();
        self.default_country = countries
            .into_values()
            .max_by_key(|(c, n)| (*n, std::cmp::Reverse(c.code.as_str())))
            .map_or(atlas::generic(), |(c, _)| c);
    }

    fn cite(&mut self, source: &str, place: &str, tag: &str, years: Option<(i32, i32)>) {
        let g = self.g;
        let Some(node) = g.get(source).filter(|n| n.tag == "SOUR") else { return };
        let src = crate::model::Source::new(node);
        let pk = self.key(place);
        let title = src.title();
        let publ = src.publication().filter(|p| !p.trim().is_empty());
        let text = town_text(&format!("{title} {}", publ.as_deref().unwrap_or("")));
        let named = names(&text, &pk);
        // The period in the title or publication (“Naissances (1863-1892)”) tells registers sharing
        // a call number apart, and is more exact than the years of the events citing it.
        let period = period_in(&text);
        let register = publ.unwrap_or(title);
        let repo = src.repository();
        let call = src.call_number().unwrap_or_default();
        let id = if call.trim().is_empty() { compact(&register) } else { compact(call) };
        let key = format!("{}|{id}|{}", repo.unwrap_or(""), period.map(|p| p.to_string()).unwrap_or_default());
        let c = self.candidates.entry(pk.clone()).or_default().entry(key.clone()).or_insert_with(|| Candidate {
            reg: UsedRegister {
                register: register.clone(),
                source_id: source.to_string(),
                repository_id: repo.map(str::to_string),
                repository: repo.and_then(|r| g.get(r)).and_then(|n| n.child_val("NAME")).unwrap_or("").to_string(),
                call_number: call.to_string(),
                cited: 0,
                years: period,
            },
            named: false,
            texts: Vec::new(),
        });
        // A source naming the town gives the register its name.
        if named && !c.named {
            c.named = true;
            c.reg.register = register;
            c.reg.source_id = source.to_string();
        }
        c.texts.push(text);
        c.reg.cited += 1;
        if let Some((lo, hi)) = years
            && period.is_none()
        {
            c.reg.years = Some(match c.reg.years {
                Some(y) => Years::new(y.from.min(lo), y.to.max(hi)),
                None => Years::new(lo, hi),
            });
        }
        *self.register_tags.entry((pk, tag.to_string())).or_default().entry(key).or_default() += 1;
    }

    /// Registers of each town, among the sources cited by its events: those naming the town, and
    /// the acts filed in a register of the town (same archive and call number cited twice, or a
    /// call number of the same series as a register naming it), unless they name another town. A
    /// national file, an act without call number or another town's register is not one.
    fn keep_registers(&mut self) {
        let towns = std::mem::take(&mut self.towns);
        let series = |call: &str| {
            let c = call.trim();
            c.rfind([' ', '_', '/', '-']).map(|i| compact(&c[..i])).unwrap_or_default()
        };
        for (pk, cands) in std::mem::take(&mut self.candidates) {
            let named_series: Vec<(Option<String>, String)> = cands
                .values()
                .filter(|c| c.named && !c.reg.call_number.trim().is_empty())
                .map(|c| (c.reg.repository_id.clone(), series(&c.reg.call_number)))
                .filter(|(_, s)| !s.is_empty())
                .collect();
            let kept: BTreeMap<String, UsedRegister> = cands
                .into_iter()
                .filter(|(_, c)| {
                    if c.named {
                        return true;
                    }
                    if c.reg.call_number.trim().is_empty() {
                        return false;
                    }
                    let elsewhere = c.texts.iter().any(|t| towns.iter().any(|o| *o != pk && names(t, o)));
                    let same_series = named_series.contains(&(c.reg.repository_id.clone(), series(&c.reg.call_number)));
                    !elsewhere && (c.reg.cited >= 2 || same_series)
                })
                .map(|(k, c)| (k, c.reg))
                .collect();
            if !kept.is_empty() {
                self.registers.insert(pk, kept);
            }
        }
    }

    fn person(&self, x: &str) -> PersonRef {
        self.g.person_ref(x)
    }

    fn seen(&self, x: &str) -> &[Seen] {
        self.seen.get(x).map(Vec::as_slice).unwrap_or(&[])
    }

    fn key(&self, place: &str) -> String {
        place_key(place, self.town)
    }

    fn country(&self, place: &str) -> &'static Country {
        atlas::in_place(place).unwrap_or(self.default_country)
    }

    /// Registers of `place` already cited, those of the same kind of event first.
    fn used_registers(&self, place: &str, goal: Goal, near: Option<Years>) -> Vec<UsedRegister> {
        let pk = self.key(place);
        let Some(all) = self.registers.get(&pk) else { return Vec::new() };
        let same_kind = |key: &str| {
            goal.tags()
                .iter()
                .chain(if goal == Goal::Parents { ["BIRT", "BAPM", "MARR"].iter() } else { [].iter() })
                .filter_map(|t| self.register_tags.get(&(pk.clone(), t.to_string())))
                .filter_map(|m| m.get(key))
                .sum::<usize>()
        };
        // Registers of other periods are other registers.
        let close = |r: &UsedRegister| match (near, r.years) {
            (Some(n), Some(y)) => y.overlaps(n.from - 20, n.to + 20),
            _ => true,
        };
        let mut regs: Vec<(usize, &String, &UsedRegister)> =
            all.iter().filter(|(_, r)| close(r)).map(|(k, r)| (same_kind(k), k, r)).collect();
        // Same kind of event, then close in time, then most cited.
        let distance = |r: &UsedRegister| match (near, r.years) {
            (Some(n), Some(y)) if y.overlaps(n.from - 10, n.to + 10) => 0,
            _ => 1,
        };
        regs.sort_by(|a, b| {
            (b.0 > 0).cmp(&(a.0 > 0)).then(distance(a.2).cmp(&distance(b.2))).then(b.2.cited.cmp(&a.2.cited))
        });
        regs.into_iter().take(3).map(|(_, _, r)| r.clone()).collect()
    }
}

/// Places, without duplicates, in order.
fn dedup_places(ctx: &Context, places: impl IntoIterator<Item = String>) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for p in places {
        if !p.trim().is_empty() && !out.iter().any(|o| ctx.key(o) == ctx.key(&p)) {
            out.push(p);
        }
    }
    out.truncate(3);
    out
}

/// A kind of records to search: type, label, years, census years and last public year.
type Rule = (RecordType, String, String, Option<Years>, Vec<i32>, Option<i32>);

/// Kinds of records for a goal, in a country and a period, best first: those of the country's
/// file in the Atlas, or the generic ones when it kept none for the period (records still closed to the
/// public do not count as none).
fn rules(country: &'static Country, goal: Goal, years: Option<Years>, male: bool, this_year: i32) -> Vec<Rule> {
    let y = years.unwrap_or(Years { from: i32::MIN / 2, to: i32::MAX / 2 });
    let covered = country
        .records
        .iter()
        .any(|r| r.goals.contains(&goal) && y.overlaps(r.from.unwrap_or(i32::MIN), r.until.unwrap_or(i32::MAX)));
    match covered || country.is_generic() {
        true => country_rules(country, goal, years, male, this_year),
        false => country_rules(atlas::generic(), goal, years, male, this_year),
    }
}

fn country_rules(country: &Country, goal: Goal, years: Option<Years>, male: bool, this_year: i32) -> Vec<Rule> {
    let mut out = Vec::new();
    let y = years.unwrap_or(Years { from: i32::MIN / 2, to: i32::MAX / 2 });
    let known = years.is_some();
    for r in country.records.iter().filter(|r| r.goals.contains(&goal)) {
        if r.sex.is_some_and(|s| (s == Sex::Male) != male) {
            continue;
        }
        // An index is worth it when the period is long: one table gives the year of the act.
        if r.index && known && y.to - y.from < 3 {
            continue;
        }
        let from = r.from;
        let to = [r.until, r.online_until].into_iter().flatten().min();
        // Last year open to everyone: later records are given on request to the people concerned
        // and their descendants.
        let public = r.public_after.as_ref().and_then(|p| p.years(goal)).map(|d| this_year - d);
        if !y.overlaps(from.unwrap_or(i32::MIN), to.unwrap_or(i32::MAX)) {
            continue;
        }
        let mut found = if known { y.clip(from, to) } else { None };
        if let Some([a, b]) = r.born {
            // Kept for people born in these years, `offset` years after the birth.
            let Some(v) = y.clip(Some(a), Some(b)) else { continue };
            found = Some(Years::new(v.from + r.offset, v.to + r.offset));
        }
        if r.extends_by != 0 {
            found = found.map(|v| Years::new(v.from, v.to + r.extends_by));
        }
        let census: Vec<i32> = r
            .years
            .iter()
            .copied()
            .filter(|c| *c >= y.from && *c <= y.to && [to, public].into_iter().flatten().all(|t| *c <= t))
            .collect();
        if !r.years.is_empty() && (!known || census.is_empty()) {
            continue;
        }
        let public = public.filter(|p| found.is_none_or(|f| f.to > *p) && r.years.is_empty());
        out.push((r.kind, label(r, goal), r.local_name.clone(), found, census, public));
    }
    out
}

/// “Civil registration (état civil): births”.
fn label(r: &atlas::Record, goal: Goal) -> String {
    let what = match (r.kind, goal) {
        (RecordType::CivilRegistration | RecordType::RegistrationIndex, Goal::Birth) => "births".to_string(),
        (RecordType::CivilRegistration | RecordType::RegistrationIndex, Goal::Marriage) => "marriages".to_string(),
        (RecordType::CivilRegistration | RecordType::RegistrationIndex, Goal::Death) => "deaths".to_string(),
        (RecordType::CivilRegistration | RecordType::RegistrationIndex, Goal::Parents) => {
            "births, marriages and deaths".to_string()
        }
        (RecordType::ChurchRegisters, Goal::Birth) => "baptisms".to_string(),
        (RecordType::ChurchRegisters, Goal::Marriage) => "marriages".to_string(),
        (RecordType::ChurchRegisters, Goal::Death) => "burials".to_string(),
        (RecordType::ChurchRegisters, Goal::Parents) => "baptisms and marriages".to_string(),
        (RecordType::Census | RecordType::PopulationRegisters, Goal::Death) => {
            "present or absent from the household".to_string()
        }
        (RecordType::Census | RecordType::PopulationRegisters, _) => "the household with its parents".to_string(),
        (RecordType::MilitaryRegisters, _) => format!("class of the year of birth + {}", r.offset),
        _ => String::new(),
    };
    let name = match r.local_name.is_empty() {
        true => r.kind.name().to_string(),
        false => format!("{} ({})", r.kind.name(), r.local_name),
    };
    match what.is_empty() {
        true => name,
        false => format!("{name}: {what}"),
    }
}

impl Gedcom {
    /// Leads of the people matching `focus` (search syntax: `sosa:1-63`, `place:troyes
    /// born:<1850`…; empty: [`DEFAULT_FOCUS`]), sorted by Sosa number then name.
    pub fn research_leads(&self, focus: &str, this_year: i32) -> Vec<Lead> {
        let focus = if focus.trim().is_empty() { DEFAULT_FOCUS } else { focus };
        let ctx = Context::new(self, this_year);
        // Sosa numbers are computed from the root person here, not read from Ancestris' tags.
        let mut sosa: Option<(u64, u64)> = None;
        let rest: Vec<&str> = focus
            .split_whitespace()
            .filter(|t| {
                let k = sort_key(t);
                if k == "sosa" {
                    sosa = Some((1, u64::MAX));
                    return false;
                }
                match k.strip_prefix("sosa:").and_then(crate::search::YearRange::parse) {
                    Some(r) => {
                        sosa = Some((r.from.unwrap_or(1).max(1) as u64, r.to.map_or(u64::MAX, |t| t.max(0) as u64)));
                        false
                    }
                    None => true,
                }
            })
            .collect();
        let rest = rest.join(" ");
        let mut people: Vec<String> = match rest.trim().is_empty() {
            true => self.records_of(RecordKind::Individual).filter_map(|r| r.xref.clone()).collect(),
            false => self.search_individuals(&rest),
        };
        if let Some((a, b)) = sosa {
            people.retain(|x| ctx.sosa.get(x).is_some_and(|n| *n >= a && *n <= b));
        }
        let mut out = Vec::new();
        let mut families_done = std::collections::HashSet::new();
        for x in &people {
            out.extend(self.person_leads(&ctx, x, &mut families_done));
        }
        let name = |l: &Lead| sort_key(&l.person.name);
        out.sort_by(|a, b| {
            let sa = a.sosa.unwrap_or(u64::MAX);
            let sb = b.sosa.unwrap_or(u64::MAX);
            sa.cmp(&sb).then_with(|| name(a).cmp(&name(b))).then(a.goal.cmp(&b.goal))
        });
        out
    }

    /// Leads of one person (their marriages included).
    pub fn leads_of(&self, xref: &str, this_year: i32) -> Vec<Lead> {
        let ctx = Context::new(self, this_year);
        self.person_leads(&ctx, xref, &mut std::collections::HashSet::new())
    }

    /// The lead of a goal for a person; `family`: the union, for a marriage (optional when there
    /// is only one).
    pub fn lead(&self, xref: &str, goal: Goal, family: Option<&str>, this_year: i32) -> Result<Lead, String> {
        let leads = self.leads_of(xref, this_year);
        let mut matching: Vec<Lead> = leads.into_iter().filter(|l| l.goal == goal).collect();
        if let Some(f) = family {
            matching.retain(|l| l.family_id.as_deref() == Some(f));
        }
        match matching.len() {
            1 => Ok(matching.remove(0)),
            0 => Err(format!(
                "nothing to search: the {} of this person is already sourced, or unknown here",
                goal.name()
            )),
            _ => Err("this person has several unions: give the spouse".to_string()),
        }
    }

    fn person_leads(&self, ctx: &Context, x: &str, families_done: &mut std::collections::HashSet<String>) -> Vec<Lead> {
        let mut out = Vec::new();
        let Some(ind) = self.individual(x) else { return out };
        let sosa = ctx.sosa.get(x).copied();
        let events = ind.events();
        let pick = |tags: &[&str]| {
            let all: Vec<_> = events.iter().filter(|e| tags.contains(&e.tag())).collect();
            all.iter().find(|e| e.is_sourced()).or(all.first()).map(|e| (*e).clone())
        };
        let birth_years = self.birth_years(ctx, &ind);
        let male = ind.sex() == "M";
        let new_lead = |goal: Goal, kind: LeadKind, date: &str, place: &str| Lead {
            person: ctx.person(x),
            goal,
            kind,
            sosa,
            family_id: None,
            spouse: None,
            date: date.to_string(),
            place: place.to_string(),
            years: None,
            years_reasons: Vec::new(),
            places: Vec::new(),
            where_to_look: Vec::new(),
            state: TaskState::Suggested,
            note: String::new(),
            searches: Vec::new(),
        };
        // Birth.
        let birth = pick(Goal::Birth.tags());
        if birth.as_ref().is_none_or(|e| !e.is_sourced()) {
            let mut l = new_lead(
                Goal::Birth,
                if birth.is_some() { LeadKind::Unsourced } else { LeadKind::Missing },
                birth.as_ref().and_then(|e| e.date_raw()).unwrap_or(""),
                birth.as_ref().and_then(|e| e.place()).unwrap_or(""),
            );
            (l.years, l.years_reasons) = birth_years.clone();
            l.places = dedup_places(
                ctx,
                std::iter::once(l.place.clone()).chain(self.parents_places(x)).chain(self.earliest_places(ctx, x)),
            );
            out.push(l);
        }
        // Marriages: one lead per union, held by the family.
        for f in ind.fams() {
            if families_done.contains(f) {
                continue;
            }
            let Some(fam) = self.family(f) else { continue };
            let spouse = [fam.husband(), fam.wife()].into_iter().flatten().find(|s| *s != x);
            let fev = fam.events();
            let marr: Vec<_> = fev.iter().filter(|e| e.tag() == "MARR").collect();
            let found = marr.iter().find(|e| e.is_sourced()).or(marr.first()).copied();
            // A union without any marriage and without children is not a lead.
            if found.is_none() && fam.children().is_empty() && spouse.is_none() {
                continue;
            }
            families_done.insert(f.to_string());
            if found.is_some_and(|e| e.is_sourced()) {
                continue;
            }
            let mut l = new_lead(
                Goal::Marriage,
                if found.is_some() { LeadKind::Unsourced } else { LeadKind::Missing },
                found.and_then(|e| e.date_raw()).unwrap_or(""),
                found.and_then(|e| e.place()).unwrap_or(""),
            );
            // The husband carries the lead, so that it is listed under the father's Sosa number.
            let husband = fam.husband().unwrap_or(x);
            l.person = ctx.person(husband);
            l.sosa = ctx.sosa.get(husband).copied();
            l.spouse = [fam.husband(), fam.wife()].into_iter().flatten().find(|s| *s != husband).map(|s| ctx.person(s));
            l.family_id = Some(f.to_string());
            (l.years, l.years_reasons) = self.marriage_years(ctx, f, found.and_then(|e| e.date_raw()));
            // The marriage usually took place in the bride's parish.
            let wife_places = fam.wife().map(|w| self.earliest_places(ctx, w)).unwrap_or_default();
            let child_places: Vec<String> = fam
                .children()
                .iter()
                .filter_map(|c| self.individual(c))
                .filter_map(|c| c.birth().and_then(|b| b.place().map(str::to_string)))
                .collect();
            l.places = dedup_places(
                ctx,
                std::iter::once(l.place.clone())
                    .chain(wife_places)
                    .chain(fam.husband().map(|h| self.earliest_places(ctx, h)).unwrap_or_default())
                    .chain(child_places),
            );
            out.push(l);
        }
        // Death.
        let death = pick(Goal::Death.tags());
        if death.as_ref().is_none_or(|e| !e.is_sourced()) {
            let mut l = new_lead(
                Goal::Death,
                if death.is_some() { LeadKind::Unsourced } else { LeadKind::Missing },
                death.as_ref().and_then(|e| e.date_raw()).unwrap_or(""),
                death.as_ref().and_then(|e| e.place()).unwrap_or(""),
            );
            (l.years, l.years_reasons) =
                self.death_years(ctx, x, &ind, death.as_ref().and_then(|e| e.date_raw()), &birth_years.0);
            let mut seen: Vec<&Seen> = ctx.seen(x).iter().collect();
            seen.sort_by_key(|s| std::cmp::Reverse(s.year));
            l.places = dedup_places(ctx, std::iter::once(l.place.clone()).chain(seen.iter().map(|s| s.place.clone())));
            out.push(l);
        }
        // Parents of an ancestor at the end of a line.
        let has_parents =
            ind.famc().iter().filter_map(|f| self.family(f)).any(|f| f.husband().is_some() || f.wife().is_some());
        if sosa.is_some() && !has_parents {
            let mut l = new_lead(Goal::Parents, LeadKind::Missing, "", "");
            (l.years, l.years_reasons) = birth_years.clone();
            // The parents are searched where the person was born; without a birth place, where
            // they married (the act names the parents), then where they were first seen.
            let birth_place = birth.as_ref().and_then(|e| e.place().map(str::to_string));
            let marriage_places = ind
                .fams()
                .iter()
                .filter_map(|f| self.family(f))
                .flat_map(|f| f.events().iter().filter_map(|e| e.place().map(str::to_string)).collect::<Vec<_>>())
                .collect::<Vec<_>>();
            l.places = match birth_place {
                Some(p) => dedup_places(ctx, [p]),
                None => dedup_places(ctx, marriage_places.into_iter().chain(self.earliest_places(ctx, x)))
                    .into_iter()
                    .take(1)
                    .collect(),
            };
            out.push(l);
        }
        let males = male;
        for l in &mut out {
            self.where_to_look(ctx, l, males);
            self.apply_log(l);
        }
        out
    }

    /// Kinds of records and registers to search for a lead.
    fn where_to_look(&self, ctx: &Context, l: &mut Lead, male: bool) {
        let places: Vec<String> = if l.places.is_empty() { vec![String::new()] } else { l.places.clone() };
        for (i, place) in places.iter().enumerate() {
            let country = ctx.country(place);
            for (record, label, local_name, years, census, public_until) in
                rules(country, l.goal, l.years, male, ctx.this_year)
            {
                // The other places only get the main registers.
                if i > 0 && !matches!(record, RecordType::CivilRegistration | RecordType::ChurchRegisters) {
                    continue;
                }
                l.where_to_look.push(WhereToLook {
                    record,
                    label,
                    local_name,
                    place: place.clone(),
                    years,
                    census_years: census,
                    public_until,
                    // Registers of the place: civil or church registers and their indexes.
                    used_registers: match record {
                        RecordType::CivilRegistration | RecordType::ChurchRegisters | RecordType::RegistrationIndex
                            if !place.is_empty() =>
                        {
                            ctx.used_registers(place, l.goal, years.or(l.years))
                        }
                        _ => Vec::new(),
                    },
                });
            }
        }
    }

    /// Possible years of birth, and why.
    fn birth_years(&self, ctx: &Context, ind: &Individual) -> (Option<Years>, Vec<Reason>) {
        let birth = ind.events().into_iter().find(|e| Goal::Birth.tags().contains(&e.tag()));
        if let Some((lo, hi)) = birth.as_ref().and_then(|e| e.date_raw()).and_then(Span::years_of) {
            return (Some(Years::new(lo, hi)), vec![Reason::Dated(Goal::Birth, Years::new(lo, hi))]);
        }
        let mut reasons = Vec::new();
        let mut hi: Option<i32> = None;
        if let Some(first) = ctx.seen(ind.xref()).iter().min_by_key(|s| s.year) {
            hi = Some(first.year - 15);
            reasons.push(Reason::Seen(first.what.clone(), first.shown));
        }
        let (f, m) = self.parents(ind.xref());
        let parent_birth = [f, m]
            .into_iter()
            .flatten()
            .filter_map(|p| p.birth().and_then(|b| b.date_raw().and_then(Span::years_of)))
            .map(|(lo, _)| lo)
            .max();
        let lo = match (hi, parent_birth) {
            (_, Some(p)) => {
                reasons.push(Reason::ParentBorn(p));
                p + 15
            }
            (Some(h), None) => h - 30,
            (None, None) => return (None, reasons),
        };
        let hi = hi.unwrap_or(lo + 35);
        (Some(Years::new(lo, hi.max(lo))), reasons)
    }

    /// Possible years of death: after the last time the person was seen alive, before a spouse's
    /// remarriage, and at most [`MAX_AGE`] years after their birth.
    fn death_years(
        &self,
        ctx: &Context,
        x: &str,
        ind: &Individual,
        known: Option<&str>,
        birth: &Option<Years>,
    ) -> (Option<Years>, Vec<Reason>) {
        if let Some((lo, hi)) = known.and_then(Span::years_of) {
            return (Some(Years::new(lo, hi)), vec![Reason::Dated(Goal::Death, Years::new(lo, hi))]);
        }
        let mut reasons = Vec::new();
        let last = ctx.seen(x).iter().max_by_key(|s| s.year);
        let lo = match (last, birth) {
            (Some(s), _) => {
                // Last seen at their own birth: the birth reason below already says so.
                let own_birth = matches!(&s.what, Sighting::Event(tag) if tag == "BIRT");
                if !(own_birth && birth.is_some()) {
                    reasons.push(Reason::Seen(s.what.clone(), s.shown));
                }
                s.year
            }
            (None, Some(b)) => b.from,
            (None, None) => return (None, reasons),
        };
        let mut hi = match birth {
            Some(b) => {
                reasons.push(Reason::Born(*b));
                (b.to + MAX_AGE).max(lo)
            }
            None => lo + 60,
        };
        // A spouse who remarried was widowed.
        for f in ind.fams() {
            let Some(fam) = self.family(f) else { continue };
            let Some(spouse) = [fam.husband(), fam.wife()].into_iter().flatten().find(|s| *s != x) else { continue };
            let Some(sp) = self.individual(spouse) else { continue };
            for other in sp.fams().into_iter().filter(|o| *o != f).filter_map(|o| self.family(o)) {
                let married = other
                    .events()
                    .iter()
                    .filter(|e| e.tag() == "MARR")
                    .filter_map(|e| e.date_raw().and_then(Span::years_of))
                    .map(|(_, h)| h)
                    .min();
                if let Some(y) = married.filter(|y| *y >= lo && *y < hi) {
                    hi = y;
                    reasons.push(Reason::Remarried(sp.display_name(), y));
                }
            }
        }
        hi = hi.min(ctx.this_year);
        (Some(Years::new(lo, hi.max(lo))), reasons)
    }

    /// Possible years of a marriage: after both spouses reached the legal age of their country,
    /// before the first child.
    fn marriage_years(&self, ctx: &Context, f: &str, known: Option<&str>) -> (Option<Years>, Vec<Reason>) {
        if let Some((lo, hi)) = known.and_then(Span::years_of) {
            return (Some(Years::new(lo, hi)), vec![Reason::Dated(Goal::Marriage, Years::new(lo, hi))]);
        }
        let Some(fam) = self.family(f) else { return (None, Vec::new()) };
        let mut reasons = Vec::new();
        let spouses: Vec<(Individual, bool)> = [(fam.husband(), true), (fam.wife(), false)]
            .into_iter()
            .filter_map(|(s, male)| s.and_then(|s| self.individual(s)).map(|s| (s, male)))
            .collect();
        // Country of the marriage, otherwise of a spouse's birth.
        let country = fam
            .events()
            .iter()
            .filter_map(|e| e.place().map(str::to_string))
            .chain(spouses.iter().filter_map(|(s, _)| s.birth().and_then(|b| b.place().map(str::to_string))))
            .find_map(|p| atlas::in_place(&p))
            .unwrap_or(ctx.default_country);
        let births: Vec<(i32, i32)> = spouses
            .iter()
            .filter_map(|(s, male)| self.birth_years(ctx, s).0.map(|y| (y.from, *male)))
            .map(|(b, male)| (b, b + country.marriage_age(b + 15, male)))
            .collect();
        let first_child = fam
            .children()
            .iter()
            .filter_map(|c| self.individual(c))
            .filter_map(|c| {
                c.birth().and_then(|b| b.date_raw().and_then(Span::years_of)).map(|(lo, _)| (lo, c.display_name()))
            })
            .min();
        let lo = births.iter().map(|(_, adult)| *adult).max();
        if let Some(born) = births.iter().map(|(b, _)| *b).max() {
            reasons.push(Reason::SpousesBornFrom(born));
        }
        let hi = first_child.as_ref().map(|(y, n)| {
            reasons.push(Reason::FirstChild(n.clone(), *y));
            *y
        });
        match (lo, hi) {
            (Some(a), Some(b)) => (Some(Years::new(a.min(b), b)), reasons),
            (Some(a), None) => (Some(Years::new(a, a + 30)), reasons),
            (None, Some(b)) => (Some(Years::new(b - 15, b)), reasons),
            (None, None) => (None, reasons),
        }
    }

    /// Places of the person's earliest sightings.
    fn earliest_places(&self, ctx: &Context, x: &str) -> Vec<String> {
        let mut seen: Vec<&Seen> = ctx.seen(x).iter().collect();
        seen.sort_by_key(|s| s.year);
        let mut out: Vec<String> = self
            .individual(x)
            .and_then(|i| i.birth())
            .and_then(|b| b.place().map(str::to_string))
            .into_iter()
            .collect();
        out.extend(seen.iter().map(|s| s.place.clone()));
        out
    }

    /// Places of the parents' marriage and of the siblings' births.
    fn parents_places(&self, x: &str) -> Vec<String> {
        let Some(ind) = self.individual(x) else { return Vec::new() };
        let mut out = Vec::new();
        for fam in ind.famc().iter().filter_map(|f| self.family(f)) {
            out.extend(fam.events().iter().filter_map(|e| e.place().map(str::to_string)));
            for c in fam.children().iter().filter(|c| **c != x).filter_map(|c| self.individual(c)) {
                out.extend(c.birth().and_then(|b| b.place().map(str::to_string)));
            }
        }
        out
    }

    /// Fills in the state, note and past searches from the file's research log, and narrows the
    /// years with the searches that found nothing.
    fn apply_log(&self, l: &mut Lead) {
        let Some(task) = self.research_task(l.owner(), l.goal) else { return };
        l.state = task.state;
        l.note = task.note;
        l.searches = task.searches;
        // A death searched in vain over the first years of the period happened later (and a birth
        // searched over its last years, earlier): only when the search covers an end of the period.
        let Some(mut y) = l.years else { return };
        for s in l.searches.iter().filter(|s| s.result == SearchResult::Nothing) {
            let Some(sy) = s.years else { continue };
            match l.goal {
                Goal::Death if sy.from <= y.from && sy.to >= y.from && sy.to < y.to => {
                    y.from = sy.to + 1;
                    l.years_reasons.push(Reason::NotFound(sy));
                }
                Goal::Birth | Goal::Marriage | Goal::Parents
                    if sy.to >= y.to && sy.from <= y.to && sy.from > y.from =>
                {
                    y.to = sy.from - 1;
                    l.years_reasons.push(Reason::NotFound(sy));
                }
                _ => {}
            }
        }
        l.years = Some(y);
    }
}

/// A lead whose record is already cited (so no longer computed), to show its research log.
pub fn closed_lead(g: &Gedcom, xref: &str, goal: Goal, family: Option<&str>) -> Lead {
    let spouse = family
        .and_then(|f| g.family(f))
        .and_then(|fam| [fam.husband(), fam.wife()].into_iter().flatten().find(|s| *s != xref).map(str::to_string));
    Lead {
        person: g.person_ref(xref),
        goal,
        kind: LeadKind::Unsourced,
        sosa: g.sosa_numbers().get(xref).filter(|n| n.branch.is_empty()).map(|n| n.sosa),
        family_id: family.map(str::to_string),
        spouse: spouse.map(|s| g.person_ref(&s)),
        date: String::new(),
        place: String::new(),
        years: None,
        years_reasons: Vec::new(),
        places: Vec::new(),
        where_to_look: Vec::new(),
        state: TaskState::Suggested,
        note: String::new(),
        searches: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FILE: &str = "0 HEAD
1 GEDC
2 VERS 7.0
1 _GR_ROOT @I1@
0 @I1@ INDI
1 NAME Paul /MARCHAND/
1 SEX M
1 BIRT
2 DATE 1 JAN 1856
2 PLAC Saint-Lô, Manche, France
2 SOUR @S1@
1 DEAT
2 DATE 1921
2 PLAC Saint-Lô, Manche, France
2 SOUR @S1@
1 FAMC @F1@
0 @I2@ INDI
1 NAME Louis /MARCHAND/
1 SEX M
1 BIRT
2 DATE 3 MAY 1825
2 PLAC Saint-Lô, Manche, France
2 SOUR @S1@
1 FAMS @F1@
1 FAMC @F2@
0 @I3@ INDI
1 NAME Victorine /DUVAL/
1 SEX F
1 FAMS @F1@
0 @I4@ INDI
1 NAME Jacques /MARCHAND/
1 SEX M
1 BIRT
2 DATE ABT 1797
2 PLAC Agneaux, Manche, France
1 FAMS @F2@
0 @I5@ INDI
1 NAME Rose /LEFÈVRE/
1 SEX F
1 BIRT
2 DATE 1801
1 DEAT
2 DATE 1870
2 PLAC Saint-Lô, Manche, France
2 SOUR @S1@
1 FAMS @F2@
1 FAMS @F3@
0 @I6@ INDI
1 NAME Pierre /HUE/
1 SEX M
1 FAMS @F3@
0 @F1@ FAM
1 HUSB @I2@
1 WIFE @I3@
1 CHIL @I1@
1 MARR
2 DATE 8 JUN 1852
2 PLAC Saint-Lô, Manche, France
2 SOUR @S1@
2 ASSO @I4@
3 ROLE WITN
0 @F2@ FAM
1 HUSB @I4@
1 WIFE @I5@
1 CHIL @I2@
1 MARR
2 DATE 14 FEB 1824
2 PLAC Agneaux, Manche, France
2 SOUR @S2@
0 @F3@ FAM
1 HUSB @I6@
1 WIFE @I5@
1 MARR
2 DATE 1860
2 PLAC Saint-Lô, Manche, France
0 @S1@ SOUR
1 TITL État civil de Saint-Lô
1 PUBL Saint-Lô, état civil
1 REPO @R1@
2 CALN 5 Mi 1234
0 @S2@ SOUR
1 TITL Mariage de Jacques MARCHAND
1 PUBL Agneaux, registres paroissiaux et état civil
1 REPO @R1@
0 @R1@ REPO
1 NAME Archives départementales de la Manche
0 TRLR
";

    fn ged() -> Gedcom {
        Gedcom::parse(FILE).unwrap()
    }

    fn find(leads: &[Lead], sosa: u64, goal: Goal) -> Option<&Lead> {
        leads.iter().find(|l| l.sosa == Some(sosa) && l.goal == goal)
    }

    #[test]
    fn leads_of_the_ancestors() {
        let g = ged();
        let leads = g.research_leads("", 2026);
        let keys: Vec<(Option<u64>, Goal)> = leads.iter().map(|l| (l.sosa, l.goal)).collect();
        assert_eq!(
            keys,
            vec![
                (Some(2), Goal::Death),
                (Some(3), Goal::Birth),
                (Some(3), Goal::Death),
                (Some(3), Goal::Parents),
                (Some(4), Goal::Birth),
                (Some(4), Goal::Death),
                (Some(4), Goal::Parents),
                (Some(5), Goal::Birth),
                (Some(5), Goal::Parents),
                // The second union of Rose, held by her second husband (no Sosa number).
                (None, Goal::Marriage),
            ]
        );
        assert_eq!(find(&leads, 4, Goal::Birth).unwrap().kind, LeadKind::Unsourced);
        assert_eq!(find(&leads, 3, Goal::Birth).unwrap().kind, LeadKind::Missing);
    }

    #[test]
    fn death_period_from_sightings_and_remarriage() {
        let g = ged();
        let leads = g.research_leads("", 2026);
        let death = find(&leads, 4, Goal::Death).unwrap();
        // Witness in 1852, his widow remarried in 1860.
        assert_eq!(death.years, Some(Years::new(1852, 1860)));
        assert!(
            death.years_reasons.iter().any(|r| r.to_string().contains("present at the marriage")),
            "{:?}",
            death.years_reasons
        );
        assert!(death.years_reasons.iter().any(|r| r.to_string().contains("remarried in 1860")));
        // Last seen in Saint-Lô.
        assert_eq!(death.places[0], "Saint-Lô, Manche, France");
        let kinds: Vec<RecordType> =
            death.where_to_look.iter().filter(|w| w.place == death.places[0]).map(|w| w.record).collect();
        assert_eq!(
            kinds,
            vec![RecordType::RegistrationIndex, RecordType::CivilRegistration, RecordType::Census, RecordType::Probate]
        );
        let census = death.where_to_look.iter().find(|w| w.record == RecordType::Census).unwrap();
        assert_eq!(census.census_years, vec![1856]);
        // The file already cites the civil registers of Saint-Lô, in this archive.
        let used = &death.where_to_look[0].used_registers[0];
        assert_eq!(used.register, "Saint-Lô, état civil");
        assert_eq!(used.repository, "Archives départementales de la Manche");
        assert_eq!(used.call_number, "5 Mi 1234");
        assert!(used.cited >= 4);
    }

    #[test]
    fn birth_before_1792_goes_to_parish_registers() {
        let g = ged();
        let leads = g.research_leads("", 2026);
        let birth = find(&leads, 5, Goal::Birth).unwrap();
        assert_eq!(birth.years, Some(Years::new(1801, 1801)));
        assert_eq!(birth.where_to_look.iter().map(|w| w.record).next(), Some(RecordType::CivilRegistration));
        let jacques = find(&leads, 4, Goal::Birth).unwrap();
        // ABT 1797: around 1797, both sides of the Revolution's civil registers.
        assert!(jacques.where_to_look.iter().any(|w| w.record == RecordType::CivilRegistration));
        assert_eq!(jacques.places[0], "Agneaux, Manche, France");
    }

    #[test]
    fn focus_and_single_person() {
        let g = ged();
        assert!(g.research_leads("sosa:1-3", 2026).iter().all(|l| l.sosa.is_some_and(|s| s <= 3)));
        let rose = g.leads_of("I5", 2026);
        assert_eq!(rose.iter().filter(|l| l.goal == Goal::Marriage).count(), 1);
        assert!(g.lead("I4", Goal::Death, None, 2026).is_ok());
        assert!(g.lead("I1", Goal::Death, None, 2026).unwrap_err().contains("already sourced"));
    }

    #[test]
    fn searches_in_vain_narrow_the_period() {
        let mut g = ged();
        let search = crate::research_log::NewSearch {
            searched: "Decennial tables of Saint-Lô, deaths".into(),
            result: Some(SearchResult::Nothing),
            years: Some(Years::new(1852, 1855)),
            ..Default::default()
        };
        assert!(g.log_research("I4", Goal::Death, &search, false));
        let death = g.lead("I4", Goal::Death, None, 2026).unwrap();
        assert_eq!(death.years, Some(Years::new(1856, 1860)));
        assert_eq!(death.state, TaskState::Planned);
        assert_eq!(death.searches.len(), 1);
        // The marriage's log is held by the family.
        g.set_research_state("F3", Goal::Marriage, TaskState::Ignored, "Not an ancestor");
        let marr = g.lead("I5", Goal::Marriage, Some("F3"), 2026).unwrap();
        assert_eq!((marr.owner(), marr.state), ("F3", TaskState::Ignored));
    }

    #[test]
    fn unknown_country_uses_generic_rules() {
        let g = Gedcom::parse(
            "0 HEAD\n1 GEDC\n2 VERS 7.0\n1 _GR_ROOT @I1@\n0 @I1@ INDI\n1 NAME Ann /SMITH/\n1 BIRT\n2 DATE 1840\n2 PLAC Montréal, Québec, Canada\n0 TRLR\n",
        )
        .unwrap();
        let leads = g.research_leads("", 2026);
        let birth = leads.iter().find(|l| l.goal == Goal::Birth).unwrap();
        assert_eq!(birth.where_to_look[0].record, RecordType::CivilRegistration);
        assert!(birth.where_to_look[0].label.starts_with("Civil registration"));
    }

    #[test]
    fn the_atlas_closes_recent_records_and_set_marriage_ages() {
        let be = atlas::by_code("BE").unwrap();
        // Belgian births are public after 100 years: those after 1926 are given on request.
        let r = rules(be, Goal::Birth, Some(Years::new(1900, 1950)), true, 2026);
        let civil = r.iter().find(|r| r.0 == RecordType::CivilRegistration).unwrap();
        assert_eq!((civil.3, civil.5), (Some(Years::new(1900, 1950)), Some(1926)));
        let r = rules(be, Goal::Birth, Some(Years::new(1800, 1850)), true, 2026);
        assert!(r.iter().all(|r| r.5.is_none()));
        // Population registers help to date a death.
        let r = rules(be, Goal::Death, Some(Years::new(1880, 1900)), true, 2026);
        assert!(r.iter().any(|r| r.0 == RecordType::PopulationRegisters));
        // Census years stop at the access delay (British census: 100 years).
        let gb = atlas::by_code("GB").unwrap();
        let r = rules(gb, Goal::Death, Some(Years::new(1900, 1940)), true, 2026);
        let census = r.iter().find(|r| r.0 == RecordType::Census).unwrap();
        assert_eq!(census.4, vec![1901, 1911, 1921]);
        // A country without a rule for the period falls back to the generic rules.
        let dz = atlas::by_code("DZ").unwrap();
        let r = rules(dz, Goal::Birth, Some(Years::new(1700, 1720)), true, 2026);
        assert!(r.iter().any(|r| r.0 == RecordType::ChurchRegisters));

        // In France, a woman could marry at 15 from 1804: the marriage is not looked for earlier.
        let g = Gedcom::parse(
            "0 HEAD\n1 GEDC\n2 VERS 7.0\n1 _GR_ROOT @I1@\n0 @I1@ INDI\n1 NAME Jean /OHL/\n1 SEX M\n1 BIRT\n2 DATE 1820\n2 PLAC Troyes, Aube, France\n1 FAMS @F1@\n\
0 @I2@ INDI\n1 NAME Rose /LEROY/\n1 SEX F\n1 BIRT\n2 DATE 1830\n2 PLAC Troyes, Aube, France\n1 FAMS @F1@\n\
0 @F1@ FAM\n1 HUSB @I1@\n1 WIFE @I2@\n0 TRLR\n",
        )
        .unwrap();
        let m = g.lead("I1", Goal::Marriage, Some("F1"), 2026).unwrap();
        assert_eq!(m.years.map(|y| y.from), Some(1845));
    }

    #[test]
    fn registers_of_the_town_only() {
        let g = Gedcom::parse(
            "0 HEAD\n1 GEDC\n2 VERS 7.0\n1 _GR_ROOT @I1@\n1 PLAC\n2 FORM Lieudit, Commune, Département, Pays\n\
0 @I1@ INDI\n1 NAME Jean /OHL/\n1 SEX M\n1 BIRT\n2 DATE 1880\n2 PLAC , Troyes, Aube, France\n2 SOUR @S1@\n\
1 CHR\n2 DATE 1905\n2 PLAC Les Noës, Troyes, Aube, France\n2 SOUR @S2@\n2 SOUR @S3@\n2 SOUR @S4@\n\
0 @S1@ SOUR\n1 TITL Troyes - Naissances\n1 REPO @R1@\n2 CALN 4E 387\n\
0 @S2@ SOUR\n1 TITL Troyes -Naissances\n1 REPO @R1@\n2 CALN 4E387\n\
0 @S3@ SOUR\n1 TITL Fichier des personnes décédées\n\
0 @S4@ SOUR\n1 TITL Saint-Etienne, Mariages\n\
0 @R1@ REPO\n1 NAME Archives de l'Aube\n0 TRLR\n",
        )
        .unwrap();
        let death = g.lead("I1", Goal::Death, None, 2026).unwrap();
        // The locality counts with its town.
        assert_eq!(death.places.len(), 1, "{:?}", death.places);
        let civil = death.where_to_look.iter().find(|w| w.record == RecordType::CivilRegistration).unwrap();
        // One register under two spellings, and no national file or other town's register.
        assert_eq!(civil.used_registers.len(), 1, "{:?}", civil.used_registers);
        assert_eq!(civil.used_registers[0].cited, 2);
        // Census and probate lists are other records.
        assert!(
            death.where_to_look.iter().filter(|w| w.record == RecordType::Census).all(|w| w.used_registers.is_empty())
        );
        // Decennial tables after 1902, the 1946 census, the national index of deaths since 1970.
        let tables = death.where_to_look.iter().find(|w| w.record == RecordType::RegistrationIndex).unwrap();
        assert_eq!(tables.years, Some(Years::new(1905, 1942)));
        let census = death.where_to_look.iter().find(|w| w.record == RecordType::Census).unwrap();
        assert!(census.census_years.contains(&1946));
        assert!(death.where_to_look.iter().any(|w| w.record == RecordType::DeathIndex));
    }

    #[test]
    fn french_algeria_until_1962() {
        let g = Gedcom::parse(
            "0 HEAD\n1 GEDC\n2 VERS 7.0\n1 _GR_ROOT @I1@\n0 @I1@ INDI\n1 NAME José /MARTINEZ/\n1 SEX M\n\
1 BIRT\n2 DATE 1891\n2 PLAC Nedroma, Tlemcen, Algérie\n0 TRLR\n",
        )
        .unwrap();
        let parents = g.lead("I1", Goal::Parents, None, 2026).unwrap();
        let kinds: Vec<RecordType> = parents.where_to_look.iter().map(|w| w.record).collect();
        assert_eq!(kinds, vec![RecordType::CivilRegistration, RecordType::MilitaryRegisters]);
        assert!(parents.where_to_look[0].label.contains("outre-mer"));
    }

    #[test]
    fn acts_filed_in_a_register_of_the_town() {
        let g = Gedcom::parse(
            "0 HEAD\n1 GEDC\n2 VERS 7.0\n1 _GR_ROOT @I1@\n\
0 @I1@ INDI\n1 NAME Michel /OHL/\n1 SEX M\n1 BIRT\n2 DATE 1870\n2 PLAC St Étienne, Loire, France\n2 SOUR @S1@\n\
1 MARR\n2 DATE 1896\n2 PLAC St Étienne, Loire, France\n2 SOUR @S2@\n\
1 CHR\n2 DATE 1900\n2 PLAC St Étienne, Loire, France\n2 SOUR @S3@\n2 SOUR @S4@\n2 SOUR @S5@\n\
1 BAPM\n2 DATE 1901\n2 PLAC St Étienne, Loire, France\n2 SOUR @S4@\n2 SOUR @S6@\n\
0 @S1@ SOUR\n1 TITL Saint-Etienne, Naissances (1870)\n1 REPO @R1@\n2 CALN 2 E 80\n\
0 @S2@ SOUR\n1 TITL Acte de mariage de Michel OHL\n1 REPO @R1@\n2 CALN 2 E 81\n\
0 @S3@ SOUR\n1 TITL Acte de naissance de Jean OHL\n1 REPO @R1@\n2 CALN 3 E 1\n\
0 @S4@ SOUR\n1 TITL Acte de décès de Paul OHL\n1 REPO @R1@\n2 CALN 4 E 9\n\
0 @S5@ SOUR\n1 TITL Lyon, Décès\n1 REPO @R1@\n2 CALN 2 E 82\n\
0 @S6@ SOUR\n1 TITL Acte de décès de Louis OHL\n\
0 @R1@ REPO\n1 NAME Archives de Saint-Étienne\n\
0 @I2@ INDI\n1 NAME X /Y/\n1 BIRT\n2 DATE 1850\n2 PLAC Lyon, Rhône, France\n0 TRLR\n",
        )
        .unwrap();
        let death = g.lead("I1", Goal::Death, None, 2026).unwrap();
        let civil = death.where_to_look.iter().find(|w| w.record == RecordType::CivilRegistration).unwrap();
        let mut calls: Vec<&str> = civil.used_registers.iter().map(|r| r.call_number.as_str()).collect();
        calls.sort();
        // “St Étienne” is “Saint-Etienne”: an act of the same series as its births register, an
        // act cited twice; not an act cited once, a register of Lyon or an act without call
        // number. The births register itself (1870) is too far from the death.
        assert_eq!(calls, vec!["2 E 81", "4 E 9"], "{:?}", civil.used_registers);
    }
}
