//! Records of a source: one generic editor, no templates.
//!
//! A record (birth, marriage, census, any GEDCOM event) is entered in its source's page: pick the
//! event, then add the people it names with their standard GEDCOM 7 role (`CHIL`, `FATH`, `MOTH`,
//! `HUSB`, `WIFE`, `SPOU`, `WITN`, `GODP`, `OFFICIATOR`…). The fields offered for each person and
//! the family links created follow from the role. Each person is **linked** to an existing
//! individual, **created**, or just **mentioned** (in the citation's note).
//!
//! [`Act::save`] writes to the tree right away, in standard GEDCOM 7: event on the principal person
//! (or on the spouses' family), citation with `PAGE`, `DATA`, `EVEN` / `ROLE` on each person,
//! `ASSO` for witnesses. No extension is written.
//!
//! The list of a source's records ([`Gedcom::acts`]) is rebuilt from the citations that point to
//! it, grouped by reference (`PAGE`) and record date (`DATA.DATE`). Reopening a record edits it:
//! the old version is removed ([`Gedcom::remove_act`]), then the new one is written.

use crate::document::{Gedcom, RecordKind};
use crate::model::{Individual, sort_key};
use crate::node::Node;
use rust_i18n::t;
use std::borrow::Cow;
use std::collections::BTreeMap;

// ----------------------------------------------------------------------
// Events and roles
// ----------------------------------------------------------------------

/// Family events (held by the spouses' `FAM` record).
pub const FAMILY_ACT_EVENTS: &[&str] = &["MARR", "MARB", "MARC", "MARL", "MARS", "ENGA", "DIV", "DIVF", "ANUL"];

fn is_birth_like(tag: &str) -> bool {
    matches!(tag, "BIRT" | "CHR" | "BAPM")
}

fn is_death_like(tag: &str) -> bool {
    matches!(tag, "DEAT" | "BURI" | "CREM")
}

/// Data that can be entered for a person named in the record.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PersonField {
    Surname,
    Given,
    Sex,
    Age,
    Occupation,
    Residence,
    /// Declared birthplace (censuses, marriages, deaths).
    Birthplace,
    /// Declared relationship (“son”, “wife”, “neighbour”…).
    Kinship,
    /// Details of a person who is only mentioned.
    Note,
}

impl PersonField {
    pub fn label(self) -> Cow<'static, str> {
        match self {
            PersonField::Surname => t!("act.field.surname"),
            PersonField::Given => t!("act.field.given"),
            PersonField::Sex => t!("act.field.sex"),
            PersonField::Age => t!("act.field.age"),
            PersonField::Occupation => t!("act.field.occupation"),
            PersonField::Residence => t!("act.field.residence"),
            PersonField::Birthplace => t!("act.field.birthplace"),
            PersonField::Kinship => t!("act.field.kinship"),
            PersonField::Note => t!("act.field.note"),
        }
    }
}

/// Role of a person in the record. Each role maps to a GEDCOM 7 role.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ActRole {
    /// Holds the event: the child, the deceased, the spouses, each person in a census…
    Principal,
    Father,
    Mother,
    Spouse,
    Witness,
    Godfather,
    Godmother,
    Declarant,
    Officiator,
    Clergy,
    Neighbor,
    Friend,
    Other,
}

use PersonField::*;

impl ActRole {
    /// Roles offered, in display order.
    pub const ALL: [ActRole; 13] = [
        ActRole::Principal,
        ActRole::Father,
        ActRole::Mother,
        ActRole::Spouse,
        ActRole::Godfather,
        ActRole::Godmother,
        ActRole::Witness,
        ActRole::Declarant,
        ActRole::Officiator,
        ActRole::Clergy,
        ActRole::Neighbor,
        ActRole::Friend,
        ActRole::Other,
    ];

    /// Linked to a principal person (father, mother or spouse of…).
    pub fn relative(self) -> bool {
        matches!(self, ActRole::Father | ActRole::Mother | ActRole::Spouse)
    }

    /// Participant associated with the event (`ASSO`) rather than linked through the family.
    pub fn associate(self) -> bool {
        !matches!(self, ActRole::Principal | ActRole::Father | ActRole::Mother | ActRole::Spouse)
    }

    /// Label of the role. `nth`: rank among the principal people.
    pub fn label(self, event: &str, family: bool, nth: usize) -> Cow<'static, str> {
        self.label_in(&crate::i18n::language(), event, family, nth)
    }

    /// Label of the role in the `lang` language.
    fn label_in(self, lang: &str, event: &str, family: bool, nth: usize) -> Cow<'static, str> {
        match self {
            ActRole::Principal if family => {
                if nth == 0 {
                    t!("act.role.husband", locale = lang)
                } else {
                    t!("act.role.wife", locale = lang)
                }
            }
            ActRole::Principal if is_birth_like(event) => t!("act.role.child", locale = lang),
            ActRole::Principal if is_death_like(event) => t!("act.role.deceased", locale = lang),
            ActRole::Principal if event == "CENS" => {
                if nth == 0 {
                    t!("act.role.head", locale = lang)
                } else {
                    t!("act.role.household", locale = lang)
                }
            }
            ActRole::Principal => t!("act.role.person", locale = lang),
            ActRole::Father => t!("act.role.father", locale = lang),
            ActRole::Mother => t!("act.role.mother", locale = lang),
            ActRole::Spouse => t!("act.role.spouse", locale = lang),
            ActRole::Witness => t!("act.role.witness", locale = lang),
            ActRole::Godfather => t!("act.role.godfather", locale = lang),
            ActRole::Godmother => t!("act.role.godmother", locale = lang),
            ActRole::Declarant => t!("act.role.declarant", locale = lang),
            ActRole::Officiator => t!("act.role.officiator", locale = lang),
            ActRole::Clergy => t!("act.role.clergy", locale = lang),
            ActRole::Neighbor => t!("act.role.neighbor", locale = lang),
            ActRole::Friend => t!("act.role.friend", locale = lang),
            ActRole::Other => t!("act.role.other", locale = lang),
        }
    }

    /// Does the role have this label, in one of the app's languages?
    fn has_label(self, event: &str, family: bool, label: &str) -> bool {
        crate::i18n::LANGUAGES.iter().any(|l| self.label_in(l, event, family, 0) == label)
    }

    /// GEDCOM 7 role written in citations (`EVEN` / `ROLE`).
    fn gedcom(self, event: &str, family: bool, nth: usize) -> Option<&'static str> {
        Some(match self {
            ActRole::Principal if family => {
                if nth == 0 {
                    "HUSB"
                } else {
                    "WIFE"
                }
            }
            ActRole::Principal if is_birth_like(event) => "CHIL",
            ActRole::Principal => return None,
            ActRole::Father => "FATH",
            ActRole::Mother => "MOTH",
            ActRole::Spouse => "SPOU",
            ActRole::Witness => "WITN",
            ActRole::Godfather | ActRole::Godmother => "GODP",
            ActRole::Declarant | ActRole::Other => "OTHER",
            ActRole::Officiator => "OFFICIATOR",
            ActRole::Clergy => "CLERGY",
            ActRole::Neighbor => "NGHBR",
            ActRole::Friend => "FRIEND",
        })
    }

    /// Fixed detail of the role (`ROLE` / `PHRASE`), in the `lang` language.
    fn base_phrase(self, lang: &str) -> Option<Cow<'static, str>> {
        match self {
            ActRole::Godfather | ActRole::Godmother | ActRole::Declarant => Some(self.label_in(lang, "", false, 0)),
            _ => None,
        }
    }

    /// Sex required by the role.
    pub fn sex(self, family: bool, nth: usize) -> Option<&'static str> {
        match self {
            ActRole::Father | ActRole::Godfather => Some("M"),
            ActRole::Mother | ActRole::Godmother => Some("F"),
            ActRole::Principal if family => Some(if nth == 0 { "M" } else { "F" }),
            _ => None,
        }
    }

    /// Fields entered for this role.
    pub fn fields(self, event: &str, family: bool, nth: usize) -> &'static [PersonField] {
        match self {
            ActRole::Principal if family => &[Surname, Given, Age, Occupation, Residence, Birthplace],
            ActRole::Principal if is_birth_like(event) => &[Surname, Given, Sex],
            ActRole::Principal if nth == 0 => &[Surname, Given, Sex, Age, Occupation, Residence, Birthplace],
            ActRole::Principal => &[Surname, Given, Sex, Age, Kinship, Occupation, Birthplace],
            ActRole::Father | ActRole::Mother | ActRole::Spouse => &[Surname, Given, Age, Occupation, Residence],
            ActRole::Officiator | ActRole::Clergy => &[Surname, Given, Occupation],
            ActRole::Godfather | ActRole::Godmother => &[Surname, Given, Age, Occupation, Residence, Kinship],
            _ => &[Surname, Given, Sex, Age, Occupation, Residence, Kinship],
        }
    }

    /// Role from the citation (`ROLE`, its `PHRASE`) and the person's sex.
    fn from_gedcom(role: &str, phrase: &str, sex: &str) -> ActRole {
        let p = sort_key(phrase);
        match role {
            "FATH" => ActRole::Father,
            "MOTH" => ActRole::Mother,
            "SPOU" => ActRole::Spouse,
            "WITN" => ActRole::Witness,
            "GODP"
                if starts_with_label(&p, ActRole::Godmother)
                    || (!starts_with_label(&p, ActRole::Godfather) && sex == "F") =>
            {
                ActRole::Godmother
            }
            "GODP" => ActRole::Godfather,
            "OFFICIATOR" => ActRole::Officiator,
            "CLERGY" => ActRole::Clergy,
            "NGHBR" => ActRole::Neighbor,
            "FRIEND" => ActRole::Friend,
            "OTHER" if starts_with_label(&p, ActRole::Declarant) => ActRole::Declarant,
            _ => ActRole::Other,
        }
    }
}

/// Does the normalized detail `p` start with the role's fixed detail, in one of the app's languages
/// (“marraine”, “godmother”)?
fn starts_with_label(p: &str, role: ActRole) -> bool {
    crate::i18n::LANGUAGES.iter().filter_map(|l| role.base_phrase(l)).any(|b| p.starts_with(&sort_key(&b)))
}

/// Role of an associated person (`ASSO` / `ROLE` and its `PHRASE`) in words: the role
/// (“Witness”, “Godmother”, or the phrase itself for `OTHER`: “neighbour”) and the rest of the
/// phrase (“aunt”). The parts of the phrase that only repeat the role are dropped.
pub fn association_role(role: &str, phrase: &str, sex: &str) -> (String, Option<String>) {
    let r = ActRole::from_gedcom(role, phrase, sex);
    let same = |part: &str| {
        let k = sort_key(part);
        crate::i18n::LANGUAGES.iter().any(|l| sort_key(&r.label_in(l, "", false, 0)) == k)
    };
    let mut extra: Vec<&str> = phrase.split(',').map(str::trim).filter(|p| !p.is_empty() && !same(p)).collect();
    let label = if r == ActRole::Other && !extra.is_empty() {
        // `OTHER` with a phrase: the phrase says it better than “Other”.
        extra.remove(0).to_string()
    } else {
        r.label("", false, 0).into_owned()
    };
    (label, (!extra.is_empty()).then(|| extra.join(", ")))
}

// ----------------------------------------------------------------------
// Record being entered
// ----------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum MediaChoice {
    #[default]
    None,
    /// Existing media (`OBJE` identifier).
    Existing(String),
    /// New files, one page each, paths preferably relative to the `.ged` folder.
    NewFiles(Vec<String>),
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum PersonChoice {
    /// Create a new individual.
    #[default]
    Create,
    /// Link to an existing individual.
    Link(String),
    /// Mention in the citation without creating an individual.
    Mention,
}

/// Another fact the record gives about a person, besides its event: a birth date in a baptism, a
/// death in a marginal mention, an occupation without a date… Written on the person with the
/// record's citation.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Fact {
    /// GEDCOM tag (`BIRT`, `DEAT`, `OCCU`, `RESI`, `EVEN`…).
    pub tag: String,
    /// `TYPE` (required with `EVEN`).
    pub event_type: String,
    /// Value of an attribute (the occupation of `OCCU`).
    pub value: String,
    /// Date in GEDCOM form; empty: undated.
    pub date: String,
    pub place: String,
    /// Street address within the place (`ADDR`).
    pub address: String,
}

impl Fact {
    pub fn new(tag: &str) -> Self {
        Self { tag: tag.to_string(), ..Default::default() }
    }

    /// Attribute whose detail is the line's value (`1 OCCU Miller`).
    pub fn has_value(&self) -> bool {
        crate::events::detail_in_value(&self.tag)
    }

    pub fn is_blank(&self) -> bool {
        [&self.value, &self.date, &self.place, &self.address].iter().all(|v| v.trim().is_empty())
    }

    /// “Birth”, or the type of a custom event.
    pub fn label(&self) -> String {
        if self.tag == "EVEN" && !self.event_type.trim().is_empty() {
            return self.event_type.trim().to_string();
        }
        crate::model::event_label(&self.tag).map(|l| l.into_owned()).unwrap_or_else(|| self.tag.clone())
    }

    /// “tisserand, 30 germinal an IV, Agneaux (Manche)”: value, date, address and place, as
    /// displayed.
    pub fn summary(&self) -> String {
        let date = if self.date.trim().is_empty() { String::new() } else { crate::Date::parse(&self.date).display() };
        [self.value.trim().to_string(), date, self.address.trim().to_string(), crate::model::short_place(&self.place)]
            .into_iter()
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join(", ")
    }

    fn node(&self) -> Node {
        let mut n =
            if self.has_value() { Node::with_value(&self.tag, self.value.trim()) } else { Node::new(&self.tag) };
        if !self.event_type.trim().is_empty() {
            n.children.push(Node::with_value("TYPE", self.event_type.trim()));
        }
        if let Some(d) = crate::date::date_node(self.date.trim(), "") {
            n.children.push(d);
        }
        if !self.place.trim().is_empty() {
            n.children.push(Node::with_value("PLAC", self.place.trim()));
        }
        n.children.extend(crate::address::address_node(&self.address));
        n
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Participant {
    pub role: ActRole,
    /// For a father, mother or spouse: rank of the principal person they relate to.
    pub of: usize,
    pub values: Vec<(PersonField, String)>,
    pub choice: PersonChoice,
    /// Other facts the record gives about them.
    pub facts: Vec<Fact>,
}

impl Participant {
    pub fn new(role: ActRole) -> Self {
        Self { role, of: 0, values: Vec::new(), choice: PersonChoice::Create, facts: Vec::new() }
    }

    pub fn get(&self, f: PersonField) -> &str {
        self.values.iter().find(|(k, _)| *k == f).map(|(_, v)| v.trim()).unwrap_or("")
    }

    pub fn set(&mut self, f: PersonField, v: impl Into<String>) {
        let v = v.into();
        match self.values.iter_mut().find(|(k, _)| *k == f) {
            Some(slot) => slot.1 = v,
            None => self.values.push((f, v)),
        }
    }

    pub fn with(mut self, f: PersonField, v: &str) -> Self {
        self.set(f, v);
        self
    }

    pub fn of(mut self, nth: usize) -> Self {
        self.of = nth;
        self
    }

    pub fn is_blank(&self) -> bool {
        self.values.iter().all(|(_, v)| v.trim().is_empty())
    }

    /// “Given names SURNAME”.
    pub fn display_name(&self) -> String {
        let (g, s) = (self.get(Given), self.get(Surname).to_uppercase());
        format!("{g} {s}").trim().to_string()
    }
}

/// Identifies a record among a source's citations: reference (`PAGE`) and record date
/// (`DATA.DATE`).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub struct ActKey {
    pub page: String,
    pub date: String,
}

/// What to do with a value of the record that contradicts the tree's (a different date or place).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Conflict {
    /// The record's event is added after the tree's, which stays preferred (GEDCOM order).
    #[default]
    KeepBoth,
    /// The record's event is added first, preferred; the tree's is kept after it.
    Replace,
    /// The tree's values stay: the record only adds its citation and what does not contradict.
    Ignore,
}

/// Choices for a record's contradictions: its event, and each person's other facts (index in
/// `participants`, index in their `facts`).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Conflicts {
    pub event: Conflict,
    pub facts: BTreeMap<(usize, usize), Conflict>,
}

impl Conflicts {
    fn fact(&self, person: usize, fact: usize) -> Conflict {
        self.facts.get(&(person, fact)).copied().unwrap_or_default()
    }
}

/// A record of a source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Act {
    pub source: String,
    /// Event tag (`BIRT`, `MARR`, `CENS`, `EVEN`…).
    pub event: String,
    /// `TYPE` of the event (custom `EVEN` types).
    pub event_type: String,
    /// For `EVEN`: family event (on the spouses' family).
    pub family_even: bool,
    /// Date of the event (GEDCOM format: `14 MAR 1912`, `ABT 1850`…).
    pub date: String,
    pub date_phrase: String,
    pub place: String,
    /// Street address of the event within the place (`ADDR`).
    pub address: String,
    /// Reference of the record in the source (`PAGE`): “Record no. 27, image 12”.
    pub page: String,
    /// Date the record was written, if different (`DATA.DATE` of the citation).
    pub record_date: String,
    pub record_date_phrase: String,
    /// Transcription (`DATA.TEXT`).
    pub transcription: String,
    /// Quality of the evidence (`QUAY` 0 to 3).
    pub quality: Option<u8>,
    pub media: MediaChoice,
    pub participants: Vec<Participant>,
    /// Create an estimated birth (`EST`) from the declared age when there is none.
    pub estimate_births: bool,
    /// Citation note, without the mentioned people (kept as is).
    pub note: String,
    /// Record read from the file: it is replaced on save.
    pub original: Option<ActKey>,
}

/// What saving a record produced.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ActResult {
    pub source: String,
    pub media: Option<String>,
    /// Identifier of each participant (`None` for a mere mention), in entry order.
    pub persons: Vec<Option<String>>,
    pub created: Vec<String>,
    pub families_created: Vec<String>,
    /// Every record created or modified.
    pub touched: Vec<String>,
}

impl ActResult {
    pub fn summary(&self) -> String {
        let linked = self.persons.iter().flatten().count();
        let mut s = tn!("act.summary.linked", linked).into_owned();
        let new = self.persons.iter().flatten().filter(|x| self.created.contains(x)).count();
        if new > 0 {
            s += &tn!("act.summary.created", new);
        }
        let fams = self.families_created.len();
        if fams > 0 {
            s += &tn!("act.summary.families", fams);
        }
        s
    }
}

/// An existing individual who could be a person named in the record.
#[derive(Debug, Clone, PartialEq)]
pub struct Suggestion {
    pub xref: String,
    pub label: String,
    pub score: f32,
    /// Why it is suggested (“father of Paul MARCHAND”, “born about 1880”…).
    pub reason: String,
}

/// Title of a source just created (see [`Gedcom::create`]), in one of the app's languages.
fn is_new_source_title(title: &str) -> bool {
    crate::i18n::LANGUAGES.iter().any(|l| t!("record.new_source", locale = l) == title)
}

impl Act {
    /// New empty record, with the people expected for this event.
    pub fn new(source: &str, event: &str) -> Self {
        let mut a = Self {
            source: source.to_string(),
            event: event.to_string(),
            event_type: String::new(),
            family_even: false,
            date: String::new(),
            date_phrase: String::new(),
            place: String::new(),
            address: String::new(),
            page: String::new(),
            record_date: String::new(),
            record_date_phrase: String::new(),
            transcription: String::new(),
            quality: None,
            media: MediaChoice::None,
            participants: Vec::new(),
            estimate_births: true,
            note: String::new(),
            original: None,
        };
        a.participants = a.default_participants();
        a
    }

    /// People offered in an empty record for this event.
    pub fn default_participants(&self) -> Vec<Participant> {
        use ActRole::*;
        let p = Participant::new;
        if self.is_family() {
            vec![p(Principal), p(Principal), p(Father), p(Mother), p(Father).of(1), p(Mother).of(1)]
        } else if is_birth_like(&self.event) {
            let mut v = vec![p(Principal), p(Father), p(Mother)];
            if self.event != "BIRT" {
                v.extend([p(Godfather), p(Godmother)]);
            }
            v
        } else {
            vec![p(Principal)]
        }
    }

    /// Is the event a family event (held by the spouses' `FAM` record)?
    pub fn is_family(&self) -> bool {
        FAMILY_ACT_EVENTS.contains(&self.event.as_str()) || (self.event == "EVEN" && self.family_even)
    }

    /// Label of the event.
    pub fn event_label(&self) -> String {
        if !self.event_type.trim().is_empty() {
            return crate::model::event_type_label(&self.event_type);
        }
        crate::model::event_name(&self.event)
    }

    /// Rank of person `idx` among the principal people.
    pub fn nth_principal(&self, idx: usize) -> usize {
        self.participants[..idx.min(self.participants.len())].iter().filter(|p| p.role == ActRole::Principal).count()
    }

    /// Nombre de personnes principales.
    pub fn principal_count(&self) -> usize {
        self.participants.iter().filter(|p| p.role == ActRole::Principal).count()
    }

    /// Label of the role of person `idx`.
    pub fn role_label(&self, idx: usize) -> Cow<'static, str> {
        let p = &self.participants[idx];
        p.role.label(&self.event, self.is_family(), self.nth_principal(idx))
    }

    /// Fields to enter for person `idx`.
    pub fn fields_of(&self, idx: usize) -> Vec<PersonField> {
        let p = &self.participants[idx];
        let mut f = p.role.fields(&self.event, self.is_family(), self.nth_principal(idx)).to_vec();
        if p.choice == PersonChoice::Mention || !p.get(Note).is_empty() {
            f.push(Note);
        }
        f
    }

    /// Sex required for person `idx`.
    pub fn imposed_sex(&self, idx: usize) -> Option<&'static str> {
        self.participants[idx].role.sex(self.is_family(), self.nth_principal(idx))
    }

    /// Short label of a principal person (“the groom”, “Paul MARCHAND”).
    pub fn principal_name(&self, nth: usize) -> String {
        if self.is_family() {
            return if nth == 0 { t!("act.the_husband").into() } else { t!("act.the_wife").into() };
        }
        self.participants
            .iter()
            .filter(|p| p.role == ActRole::Principal)
            .nth(nth)
            .map(|p| p.display_name())
            .filter(|n| !n.is_empty())
            .unwrap_or_else(|| t!("act.person_n", n = nth + 1).into_owned())
    }

    /// Year of the event, if the date has one.
    pub fn year(&self) -> Option<i32> {
        crate::Date::parse(&self.date).year()
    }

    /// Prepares the next record of the same register: same event, place and image; the first number
    /// of the reference is incremented (“Record no. 27” → “Record no. 28”).
    pub fn next(&self) -> Self {
        let mut e = Act::new(&self.source, &self.event);
        e.event_type = self.event_type.clone();
        e.family_even = self.family_even;
        e.participants = e.default_participants();
        e.place = self.place.clone();
        e.media = self.media.clone();
        e.quality = self.quality;
        e.estimate_births = self.estimate_births;
        e.page = increment_first_number(&self.page);
        e
    }

    /// Title of a source specific to this record: “Birth record of Paul MARCHAND”.
    pub fn auto_title(&self) -> String {
        let names: Vec<String> = self
            .participants
            .iter()
            .filter(|p| p.role == ActRole::Principal && !p.is_blank())
            .map(|p| p.display_name())
            .collect();
        let what = self.event_label().to_lowercase();
        // “Burial record”, “Birth record”.
        let elided = what.starts_with(|c: char| "aeéèêiouyh".contains(c));
        let title = match (names.is_empty(), elided) {
            (true, true) => t!("act.auto_title.elided", what = what),
            (true, false) => t!("act.auto_title.plain", what = what),
            (false, true) => t!("act.auto_title.elided_of", what = what, names = join_and(&names)),
            (false, false) => t!("act.auto_title.plain_of", what = what, names = join_and(&names)),
        };
        capitalize(&title)
    }

    /// Label of the record in its source's list: “Birth · Louise MARCHAND”.
    pub fn title(&self) -> String {
        let names: Vec<String> = self
            .participants
            .iter()
            .filter(|p| p.role == ActRole::Principal && !p.is_blank())
            .map(|p| p.display_name())
            .collect();
        let what = if self.event.is_empty() { t!("act.record").into_owned() } else { self.event_label() };
        if names.is_empty() { what } else { format!("{what} · {}", join_and(&names)) }
    }

    /// Reference and date, for the list of records: “Record no. 27 · 3 February 1914”.
    pub fn subtitle(&self) -> String {
        let date = if self.date.trim().is_empty() { &self.record_date } else { &self.date };
        let date = if date.trim().is_empty() { String::new() } else { crate::Date::parse(date).display() };
        [self.page.trim().to_string(), date].into_iter().filter(|s| !s.is_empty()).collect::<Vec<_>>().join(" · ")
    }

    /// Readable summary of the record, to start the transcription.
    pub fn auto_text(&self) -> String {
        let mut lines = vec![format!(
            "{}{}{}",
            self.event_label(),
            if self.date.is_empty() {
                String::new()
            } else {
                t!("act.text.on_date", date = crate::Date::parse(&self.date).display()).into_owned()
            },
            if self.place.is_empty() {
                String::new()
            } else {
                t!("act.text.in_place", place = crate::short_place(&self.place)).into_owned()
            }
        )];
        for (i, p) in self.participants.iter().enumerate() {
            if p.is_blank() {
                continue;
            }
            let mut s = labelled(&self.role_label(i), &p.display_name());
            for f in [Age, Occupation, Residence, Birthplace, Kinship] {
                let v = p.get(f);
                if !v.is_empty() {
                    s += &match f {
                        Age => format!(", {}", age_text(v)),
                        Residence => t!("act.text.residing", place = crate::short_place(v)).into_owned(),
                        Birthplace => t!("act.text.born_in", place = crate::short_place(v)).into_owned(),
                        _ => format!(", {v}"),
                    };
                }
            }
            lines.push(s);
        }
        lines.join("\n")
    }

    /// Turns the facts deduced from the fields (occupation and residence at the record's date,
    /// birth estimated from the age, birthplace) into explicit facts of each person, written the
    /// same way: everything the record will write can then be shown and unchecked one by one.
    pub fn make_facts_explicit(&mut self, ged: &Gedcom) {
        let year = self.year();
        let date = self.date.trim().to_string();
        let event = self.event.clone();
        let estimate = self.estimate_births;
        for p in &mut self.participants {
            if !matches!(p.choice, PersonChoice::Create | PersonChoice::Link(_)) || p.is_blank() {
                continue;
            }
            let mut out = Vec::new();
            let occu = p.get(Occupation).to_string();
            if !occu.is_empty() {
                out.push(Fact { tag: "OCCU".into(), value: occu, date: date.clone(), ..Default::default() });
                p.set(Occupation, "");
            }
            let resi = p.get(Residence).to_string();
            if !resi.is_empty() {
                out.push(Fact { tag: "RESI".into(), place: resi, date: date.clone(), ..Default::default() });
                p.set(Residence, "");
            }
            let has_birth = p.facts.iter().any(|f| is_birth_like(&f.tag))
                || matches!(&p.choice, PersonChoice::Link(x) if ged.get(x).is_some_and(|n| n.children.iter().any(|c| is_birth_like(&c.tag))));
            if !(is_birth_like(&event) && p.role == ActRole::Principal) && !has_birth {
                let est = match (estimate, year, parse_age_years(p.get(Age))) {
                    (true, Some(y), Some(a)) => format!("EST {}", y - a),
                    _ => String::new(),
                };
                let place = p.get(Birthplace).to_string();
                if !est.is_empty() || !place.is_empty() {
                    out.push(Fact { tag: "BIRT".into(), date: est, place, ..Default::default() });
                    p.set(Birthplace, "");
                }
            }
            p.facts.splice(0..0, out);
        }
        self.estimate_births = false;
    }

    /// The reverse of [`Act::make_facts_explicit`], for record entry: the occupation and residence
    /// at the record's date go back to their fields, the birth estimated from the age goes back to
    /// the “estimate” option.
    pub fn fold_deduced_facts(&mut self) {
        let date = self.date.trim().to_string();
        let year = self.year();
        let birth_record = is_birth_like(&self.event);
        let mut estimated = false;
        for p in &mut self.participants {
            // The principal person of a baptism has no birthplace field: their birth stays a fact.
            let deduced_birth = !(birth_record && p.role == ActRole::Principal);
            let mut kept = Vec::new();
            for f in std::mem::take(&mut p.facts) {
                let at_date = f.date.trim() == date && f.event_type.is_empty();
                match f.tag.as_str() {
                    "OCCU" if at_date && p.get(Occupation).is_empty() => p.set(Occupation, f.value.trim()),
                    "RESI"
                        if at_date
                            && p.get(Residence).is_empty()
                            && f.value.is_empty()
                            && f.address.trim().is_empty() =>
                    {
                        p.set(Residence, f.place.trim())
                    }
                    "BIRT"
                        if deduced_birth
                            && f.event_type.is_empty()
                            && f.value.is_empty()
                            && (f.date.trim().is_empty()
                                || f.date
                                    .trim()
                                    .strip_prefix("EST ")
                                    .and_then(|y| y.trim().parse::<i32>().ok())
                                    .is_some_and(|b| {
                                        year.zip(parse_age_years(p.get(Age))).is_some_and(|(y, a)| y - a == b)
                                    })) =>
                    {
                        estimated |= !f.date.trim().is_empty();
                        if !f.place.trim().is_empty() {
                            p.set(Birthplace, f.place.trim());
                        }
                    }
                    _ => kept.push(f),
                }
            }
            p.facts = kept;
        }
        self.estimate_births = estimated || self.estimate_births;
    }

    /// Checks the record before writing it.
    pub fn validate(&self, ged: &Gedcom) -> Result<(), String> {
        if ged.get(&self.source).is_none_or(|n| n.tag != "SOUR") {
            return Err(t!("act.error.no_source").into());
        }
        if self.event.trim().is_empty() {
            return Err(t!("act.error.no_event").into());
        }
        if self.event == "EVEN" && self.event_type.trim().is_empty() {
            return Err(t!("act.error.no_event_type").into());
        }
        // A web page (`FILE https://…`) is a link of the source, not the act's image.
        if let MediaChoice::Existing(x) = &self.media
            && ged.get(x).is_none_or(|n| n.tag != "OBJE" || crate::model::Media::new(n).is_web())
        {
            return Err(t!("act.error.no_media").into());
        }
        for (record, d) in [(false, &self.date), (true, &self.record_date)] {
            if !d.trim().is_empty() {
                let parsed = crate::Date::parse(d);
                if parsed.year().is_none() {
                    return Err(if record {
                        t!("act.error.record_date_unknown", text = d.trim())
                    } else {
                        t!("act.error.date_unknown", text = d.trim())
                    }
                    .into());
                }
                parsed.validate().map_err(|e| {
                    if record {
                        t!("act.error.record_date_impossible", error = e)
                    } else {
                        t!("act.error.date_impossible", error = e)
                    }
                    .into_owned()
                })?;
            }
        }
        for (i, p) in self.participants.iter().enumerate() {
            for f in p.facts.iter().filter(|f| !f.is_blank()) {
                let label = self.role_label(i);
                if f.tag.trim().is_empty() || (f.tag == "EVEN" && f.event_type.trim().is_empty()) {
                    return Err(t!("act.error.fact_type", role = label).into());
                }
                if !f.date.trim().is_empty() {
                    let parsed = crate::Date::parse(&f.date);
                    if parsed.year().is_none() {
                        return Err(
                            t!("act.error.fact_date", role = label, fact = f.label(), text = f.date.trim()).into()
                        );
                    }
                    parsed.validate().map_err(|e| t!("act.error.date_impossible", error = e).into_owned())?;
                }
            }
        }
        let principals = self.principal_count();
        let mut kept = 0;
        for (i, p) in self.participants.iter().enumerate() {
            let label = self.role_label(i);
            if p.role.relative() && p.of >= principals.max(1) {
                return Err(t!("act.error.principal_missing", role = label).into());
            }
            let counts = match &p.choice {
                PersonChoice::Link(x) => {
                    if ged.individual(x).is_none() {
                        return Err(t!("act.error.person_missing", role = label).into());
                    }
                    true
                }
                // Empty line: ignored.
                PersonChoice::Create if p.is_blank() => false,
                PersonChoice::Create => {
                    if p.get(Surname).is_empty() && p.get(Given).is_empty() {
                        return Err(t!("act.error.no_name", role = label).into());
                    }
                    true
                }
                PersonChoice::Mention if p.role == ActRole::Principal => {
                    return Err(t!("act.error.principal_mention", role = label).into());
                }
                PersonChoice::Mention => false,
            };
            if counts && p.role == ActRole::Principal {
                kept += 1;
            }
        }
        if kept == 0 {
            return Err(t!("act.error.no_principal").into());
        }
        if self.is_family() && kept < 2 {
            return Err(t!("act.error.both_spouses").into());
        }
        Ok(())
    }

    /// Effective values: a child without a surname takes their father's, a father without a surname
    /// takes the child's.
    fn effective(&self) -> Vec<Participant> {
        let mut ps = self.participants.clone();
        let nth: Vec<usize> = (0..ps.len()).map(|i| self.nth_principal(i)).collect();
        let surname_of = |ps: &[Participant], pred: &dyn Fn(usize, &Participant) -> bool| {
            ps.iter()
                .enumerate()
                .find(|(i, p)| pred(*i, p) && !p.get(Surname).is_empty())
                .map(|(_, p)| p.get(Surname).to_string())
                .unwrap_or_default()
        };
        for i in 0..ps.len() {
            if !ps[i].get(Surname).is_empty() || ps[i].choice != PersonChoice::Create {
                continue;
            }
            let inherited = match ps[i].role {
                ActRole::Father => {
                    let of = ps[i].of;
                    surname_of(&ps, &|j, p| p.role == ActRole::Principal && nth[j] == of)
                }
                ActRole::Principal if !self.is_family() => {
                    let me = nth[i];
                    surname_of(&ps, &|_, p| p.role == ActRole::Father && p.of == me)
                }
                _ => String::new(),
            };
            if !inherited.is_empty() {
                ps[i].set(Surname, inherited);
            }
        }
        ps
    }

    /// GEDCOM that would be written, without touching the document: text of the records created or
    /// modified.
    pub fn preview(&self, ged: &Gedcom) -> Result<String, String> {
        let mut copy = ged.clone();
        let res = self.save(&mut copy)?;
        let mut out = String::new();
        for x in &res.touched {
            if let Some(n) = copy.get(x) {
                out.push_str(&crate::parse::node_to_string(n));
                if !out.ends_with('\n') {
                    out.push('\n');
                }
            }
        }
        Ok(out)
    }

    /// Writes the record into the document. A reopened record replaces its old version. Nothing is
    /// changed on error.
    pub fn save(&self, ged: &mut Gedcom) -> Result<ActResult, String> {
        self.save_with(ged, &Conflicts::default())
    }

    /// Like [`Act::save`], with a choice for each value that contradicts the tree.
    pub fn save_with(&self, ged: &mut Gedcom, conflicts: &Conflicts) -> Result<ActResult, String> {
        self.validate(ged)?;
        let mut copy = ged.clone();
        let mut removed = Vec::new();
        if let Some(k) = &self.original {
            removed = copy.unlink_act(&self.source, k, true);
        }
        let mut res = self.apply(&mut copy, conflicts)?;
        copy.drop_empty_events(&removed);
        for x in removed {
            if !res.touched.contains(&x) {
                res.touched.push(x);
            }
        }
        *ged = copy;
        Ok(res)
    }

    fn apply(&self, ged: &mut Gedcom, conflicts: &Conflicts) -> Result<ActResult, String> {
        self.validate(ged)?;
        let mut w = Writer { ged, res: ActResult::default() };
        let source = self.source.clone();
        let family = self.is_family();
        w.res.source = source.clone();

        // 1. Source: automatic title of a brand new source, contents (`DATA` / `EVEN` + period).
        let src = w.ged.get_mut(&source).unwrap();
        if src.child_val("TITL").is_none_or(|t| t.trim().is_empty() || is_new_source_title(t.trim())) {
            let title = self.auto_title();
            src.set_child_text("TITL", &title);
            w.touch(&source);
        }
        let when = self.citation_date().map(|d| d.val().to_string()).unwrap_or_default();
        if crate::kinds::record_in_source(w.ged.get_mut(&source).unwrap(), &self.event, &when) {
            w.touch(&source);
        }

        // 2. Image de l'acte.
        let media = match &self.media {
            MediaChoice::None => None,
            MediaChoice::Existing(x) => Some(x.clone()),
            MediaChoice::NewFiles(paths) if paths.iter().any(|p| !p.trim().is_empty()) => {
                let x = w.ged.create(RecordKind::Media);
                let title = [w.ged.record_label(&source), self.page.trim().to_string()]
                    .into_iter()
                    .filter(|s| !s.is_empty())
                    .collect::<Vec<_>>()
                    .join(", ");
                let node = w.ged.get_mut(&x).unwrap();
                node.children.retain(|c| c.tag != "FILE");
                // One page per file, in order; the title on the first one.
                for (i, path) in paths.iter().map(|p| p.trim()).filter(|p| !p.is_empty()).enumerate() {
                    let mut file = Node::with_value("FILE", path).push(Node::with_value("FORM", crate::mime_for(path)));
                    if i == 0 {
                        file.set_child_val("TITL", &title);
                    }
                    match node.children.iter().rposition(|c| c.tag == "FILE") {
                        Some(at) => node.children.insert(at + 1, file),
                        None => node.insert_child(file),
                    }
                }
                // A record is a handwritten document (`FORM` / `MEDI MANUSCRIPT`).
                crate::kinds::set_medium(node, "MANUSCRIPT");
                w.mark_created(&x);
                Some(x)
            }
            MediaChoice::NewFiles(_) => None,
        };
        if let Some(m) = &media {
            let ptr = format!("@{m}@");
            let src = w.ged.get_mut(&source).unwrap();
            if !src.children.iter().any(|c| c.tag == "OBJE" && c.val() == ptr) {
                src.insert_child(Node::with_value("OBJE", ptr));
                w.touch(&source);
            }
        }
        w.res.media = media.clone();

        // 3. People: link or create.
        let people = self.effective();
        let nth: Vec<usize> = (0..people.len()).map(|i| self.nth_principal(i)).collect();
        let mut ids: Vec<Option<String>> = Vec::new();
        for (i, p) in people.iter().enumerate() {
            let sex = p.role.sex(family, nth[i]).map(str::to_string).unwrap_or_else(|| p.get(Sex).to_uppercase());
            let id = match &p.choice {
                PersonChoice::Mention => None,
                PersonChoice::Create if p.is_blank() => None,
                PersonChoice::Link(x) => {
                    // Fills in the sex if it was unknown.
                    if matches!(sex.as_str(), "M" | "F")
                        && let Some(n) = w.ged.get_mut(x)
                        && n.child_val("SEX").is_none_or(|s| s == "U")
                    {
                        n.set_child_val("SEX", &sex);
                        w.touch(x);
                    }
                    Some(x.clone())
                }
                PersonChoice::Create => {
                    let x = w.ged.create(RecordKind::Individual);
                    let (given, surname) = (p.get(Given), p.get(Surname));
                    let node = w.ged.get_mut(&x).unwrap();
                    let name = node.ensure_child("NAME");
                    name.value = Some(format!("{given} /{surname}/").trim_start().to_string());
                    name.set_child_val("GIVN", given);
                    name.set_child_val("SURN", surname);
                    node.set_child_val("SEX", if matches!(sex.as_str(), "M" | "F" | "X") { &sex } else { "U" });
                    w.mark_created(&x);
                    Some(x)
                }
            };
            ids.push(id);
        }
        w.res.persons = ids.clone();
        let principal_id = |k: usize| -> Option<String> {
            people
                .iter()
                .zip(&ids)
                .enumerate()
                .find(|(i, (p, id))| p.role == ActRole::Principal && nth[*i] == k && id.is_some())
                .and_then(|(_, (_, id))| id.clone())
        };

        // 4. Family links inferred from the roles.
        let mut parents: Vec<(String, Option<String>, Option<String>)> = Vec::new();
        for (p, id) in people.iter().zip(&ids) {
            let Some(id) = id else { continue };
            match p.role {
                ActRole::Father | ActRole::Mother => {
                    let Some(child) = principal_id(p.of) else { continue };
                    let slot = match parents.iter_mut().position(|(c, _, _)| *c == child) {
                        Some(i) => &mut parents[i],
                        None => {
                            parents.push((child.clone(), None, None));
                            parents.last_mut().unwrap()
                        }
                    };
                    if p.role == ActRole::Father {
                        slot.1 = Some(id.clone());
                    } else {
                        slot.2 = Some(id.clone());
                    }
                }
                ActRole::Spouse => {
                    if let Some(other) = principal_id(p.of) {
                        w.ensure_couple(&other, id);
                    }
                }
                _ => {}
            }
        }
        for (child, f, m) in parents {
            w.ensure_parents(&child, f.as_deref(), m.as_deref());
        }
        if !family {
            self.link_household(&mut w, &people, &ids, &nth);
        }

        // 5. Event and citations.
        let year = self.year();
        let fam = if family {
            let spouses: Vec<String> = people
                .iter()
                .zip(&ids)
                .filter(|(p, id)| p.role == ActRole::Principal && id.is_some())
                .filter_map(|(_, id)| id.clone())
                .collect();
            Some(w.ensure_couple(&spouses[0], &spouses[1]))
        } else {
            None
        };
        let note = self.citation_note(&people, &nth);

        // Main event: on the family, or on each principal person. Remember where it is to attach
        // the witnesses (ASSO).
        let mut main_event: Option<(String, usize)> = None;
        if let Some(fam) = &fam {
            let mut ev = self.event_node(None);
            let mut cite = self.citation(media.as_deref(), None, true);
            if !note.is_empty() {
                cite.children.push(Node::with_value("NOTE", &note));
            }
            ev.children.push(cite);
            for (i, (p, id)) in people.iter().zip(&ids).enumerate() {
                if id.is_none() || p.role != ActRole::Principal {
                    continue;
                }
                if let Some(age) = gedcom_age(p.get(Age)) {
                    let tag = if nth[i] == 0 { "HUSB" } else { "WIFE" };
                    let at = ev.children.iter().position(|c| c.tag == "SOUR").unwrap_or(ev.children.len());
                    ev.children.insert(at, Node::new(tag).push(Node::with_value("AGE", age)));
                }
            }
            let i = w.add_event(fam, ev, conflicts.event);
            main_event = Some((fam.clone(), i));
        }

        let mut first_principal = true;
        for (i, (p, id)) in people.iter().zip(&ids).enumerate() {
            let Some(x) = id else { continue };
            let age = p.get(Age);
            let role = self.citation_role(p, nth[i], &people, &nth);
            if p.role == ActRole::Principal && fam.is_none() {
                let mut ev = self.event_node(gedcom_age(age));
                let mut cite = self.citation(media.as_deref(), role.clone(), first_principal);
                if first_principal && !note.is_empty() {
                    cite.children.push(Node::with_value("NOTE", &note));
                }
                ev.children.push(cite);
                let at = w.add_event(x, ev, conflicts.event);
                if first_principal {
                    main_event = Some((x.clone(), at));
                }
                first_principal = false;
            } else {
                // Citation at the individual's level, with their role in the event.
                let cite = self.citation(media.as_deref(), role.clone(), false);
                if !w.has_citation(x, &cite) {
                    let node = w.ged.get_mut(x).unwrap();
                    let before = node.children.len();
                    merge_citation(&mut node.children, cite);
                    // New citation: placed before CREA/CHAN.
                    if node.children.len() > before {
                        let c = node.children.pop().unwrap();
                        node.insert_child(c);
                    }
                    w.touch(x);
                }
            }

            // Inferred facts: occupation, residence, estimated birth.
            let occu = p.get(Occupation);
            if !occu.is_empty() {
                let mut n = Node::with_value("OCCU", occu);
                if let Some(d) = self.date_node() {
                    n.children.push(d);
                }
                n.children.push(self.citation(media.as_deref(), role.clone(), false));
                w.add_fact(x, n);
            }
            let resi = p.get(Residence);
            if !resi.is_empty() {
                let mut n = Node::new("RESI");
                if let Some(d) = self.date_node() {
                    n.children.push(d);
                }
                n.children.push(Node::with_value("PLAC", resi));
                n.children.push(self.citation(media.as_deref(), role.clone(), false));
                w.add_fact(x, n);
            }
            let is_birth_event = is_birth_like(&self.event) && p.role == ActRole::Principal;
            if !is_birth_event {
                let birthplace = p.get(Birthplace);
                let est = match (self.estimate_births, year, parse_age_years(age)) {
                    (true, Some(y), Some(a)) => Some(format!("EST {}", y - a)),
                    _ => None,
                };
                let birth_fact = p.facts.iter().any(|f| is_birth_like(&f.tag) && !f.is_blank());
                if (est.is_some() || !birthplace.is_empty()) && !birth_fact && !w.has_birth(x) {
                    let mut n = Node::new("BIRT");
                    if let Some(d) = est {
                        n.children.push(Node::with_value("DATE", d));
                    }
                    if !birthplace.is_empty() {
                        n.children.push(Node::with_value("PLAC", birthplace));
                    }
                    n.children.push(self.citation(media.as_deref(), role.clone(), false));
                    w.ged.get_mut(x).unwrap().insert_child(n);
                    w.touch(x);
                }
            }

            // Other facts of the record, with its citation.
            for (k, f) in p.facts.iter().enumerate().filter(|(_, f)| !f.is_blank()) {
                let mut n = f.node();
                n.children.push(self.citation(media.as_deref(), role.clone(), false));
                let conflict = conflicts.fact(i, k);
                if f.has_value() || f.tag == "RESI" {
                    w.add_fact_with(x, n, conflict);
                } else {
                    w.add_event(x, n, conflict);
                }
            }
        }

        // 6. Witnesses, godparents, informants… associated with the main event.
        if let Some((holder, idx)) = main_event {
            for (i, (p, id)) in people.iter().zip(&ids).enumerate() {
                let Some(x) = id else { continue };
                if !p.role.associate() {
                    continue;
                }
                let Some((role, phrase)) = self.citation_role(p, nth[i], &people, &nth) else { continue };
                let mut r = Node::with_value("ROLE", role);
                if let Some(ph) = phrase {
                    r.children.push(Node::with_value("PHRASE", ph));
                }
                let asso = Node::with_value("ASSO", format!("@{x}@")).push(r);
                let ev = &mut w.ged.get_mut(&holder).unwrap().children[idx];
                if !ev.children.contains(&asso) {
                    let at = ev.children.iter().position(|c| c.tag == "SOUR").unwrap_or(ev.children.len());
                    ev.children.insert(at, asso);
                }
            }
        }

        let mut res = w.res;
        res.touched.dedup();
        Ok(res)
    }

    /// GEDCOM role and detail written in a person's citation.
    fn citation_role(
        &self,
        p: &Participant,
        nth: usize,
        people: &[Participant],
        nths: &[usize],
    ) -> Option<(String, Option<String>)> {
        let family = self.is_family();
        let mut phrase: Vec<String> = Vec::new();
        if let Some(b) = p.role.base_phrase(&crate::i18n::language()) {
            phrase.push(b.into_owned());
        }
        if p.role.relative() && people.iter().zip(nths).filter(|(q, _)| q.role == ActRole::Principal).count() > 1 {
            phrase.push(
                t!("act.role_of", role = p.role.label(&self.event, family, 0), name = self.principal_name(p.of))
                    .into_owned(),
            );
        }
        let kin = p.get(Kinship);
        if !kin.is_empty() {
            phrase.push(kin.to_string());
        }
        let role = match p.role.gedcom(&self.event, family, nth) {
            Some(r) => r.to_string(),
            // Principal person without a standard role: `OTHER` only to carry their declared
            // relationship.
            None if !phrase.is_empty() => "OTHER".to_string(),
            None => return None,
        };
        Some((role, (!phrase.is_empty()).then(|| phrase.join(", "))))
    }

    /// Note of the main citation: mentioned people, then the record's note.
    fn citation_note(&self, people: &[Participant], nth: &[usize]) -> String {
        let family = self.is_family();
        let mut parts: Vec<String> = people
            .iter()
            .enumerate()
            .filter(|(_, p)| p.choice == PersonChoice::Mention && !p.is_blank())
            .map(|(i, p)| {
                let mut s = labelled(&p.role.label(&self.event, family, nth[i]), &p.display_name());
                for f in [Age, Occupation, Residence, Kinship, Note] {
                    let v = p.get(f);
                    if !v.is_empty() {
                        s += &format!(", {}", if f == Age { age_text(v) } else { v.to_string() });
                    }
                }
                s
            })
            .collect();
        if !self.note.trim().is_empty() {
            parts.push(self.note.trim().to_string());
        }
        parts.join("; ")
    }

    /// `DATE` (+ `PHRASE`) of the event.
    fn date_node(&self) -> Option<Node> {
        crate::date::date_node(&self.date, &self.date_phrase)
    }

    /// Date of the citation (`DATA.DATE`): date written, otherwise date of the event.
    fn citation_date(&self) -> Option<Node> {
        let own = !self.record_date.trim().is_empty() || !self.record_date_phrase.trim().is_empty();
        if own { crate::date::date_node(&self.record_date, &self.record_date_phrase) } else { self.date_node() }
    }

    /// Node of the record's event (without citation).
    fn event_node(&self, age: Option<String>) -> Node {
        let mut ev = Node::new(&self.event);
        if !self.event_type.trim().is_empty() {
            ev.children.push(Node::with_value("TYPE", self.event_type.trim()));
        }
        if let Some(d) = self.date_node() {
            ev.children.push(d);
        }
        if !self.place.trim().is_empty() {
            ev.children.push(Node::with_value("PLAC", self.place.trim()));
        }
        ev.children.extend(crate::address::address_node(&self.address));
        if let Some(a) = age {
            ev.children.push(Node::with_value("AGE", a));
        }
        ev
    }

    /// Citation of the source. `full` adds the transcription (written only once, on the main
    /// event).
    fn citation(&self, media: Option<&str>, role: Option<(String, Option<String>)>, full: bool) -> Node {
        let mut c = Node::with_value("SOUR", format!("@{}@", self.source));
        if !self.page.trim().is_empty() {
            c.children.push(Node::with_value("PAGE", self.page.trim()));
        }
        let date = self.citation_date();
        let text = self.transcription.trim();
        if date.is_some() || (full && !text.is_empty()) {
            let mut data = Node::new("DATA");
            if let Some(d) = date {
                data.children.push(d);
            }
            if full && !text.is_empty() {
                let mut t = Node::new("TEXT");
                t.set_text(text);
                data.children.push(t);
            }
            c.children.push(data);
        }
        let mut ev = Node::with_value("EVEN", &self.event);
        if let Some((r, phrase)) = role {
            let mut rn = Node::with_value("ROLE", r);
            if let Some(p) = phrase {
                rn.children.push(Node::with_value("PHRASE", p));
            }
            ev.children.push(rn);
        }
        c.children.push(ev);
        if let Some(q) = self.quality {
            c.children.push(Node::with_value("QUAY", q.min(3).to_string()));
        }
        if let Some(m) = media {
            c.children.push(Node::with_value("OBJE", format!("@{m}@")));
        }
        c
    }

    /// Household (census…): spouse and children of the first principal person, from the “declared
    /// relationship” of the next ones.
    fn link_household(&self, w: &mut Writer, people: &[Participant], ids: &[Option<String>], nth: &[usize]) {
        let principals: Vec<(&Participant, String, usize)> = people
            .iter()
            .zip(ids)
            .enumerate()
            .filter(|(_, (p, id))| p.role == ActRole::Principal && id.is_some())
            .map(|(i, (p, id))| (p, id.clone().unwrap(), nth[i]))
            .collect();
        let Some((_, head, 0)) = principals.first().cloned() else { return };
        let members = &principals[1..];
        let kin = |p: &Participant| sort_key(p.get(Kinship));
        let spouse = members.iter().find(|(p, _, _)| {
            let k = kin(p);
            ["epou", "femme", "mari", "conjoint", "wife", "husband", "spouse"].iter().any(|s| k.contains(s))
        });
        if let Some((_, s, _)) = spouse {
            w.ensure_couple(&head, s);
        }
        for (p, x, _) in members {
            let k = kin(p);
            if !["fils", "fille", "enfant", "son", "daughter", "child"].iter().any(|s| k.contains(s))
                || ["petit", "beau", "belle", "grand", "law", "step"].iter().any(|s| k.contains(s))
            {
                continue;
            }
            let couple: Vec<String> = std::iter::once(head.clone()).chain(spouse.map(|(_, s, _)| s.clone())).collect();
            let sex = |x: &str| w.ged.individual(x).map(|i| i.sex().to_string()).unwrap_or_default();
            let father = couple.iter().find(|c| sex(c) == "M").cloned();
            let mother = couple.iter().find(|c| sex(c) == "F").cloned();
            let (father, mother) = match (father, mother) {
                (None, None) => (Some(head.clone()), None),
                fm => fm,
            };
            w.ensure_parents(x, father.as_deref(), mother.as_deref());
        }
    }
}

/// Increments the first number of a reference (“Record no. 27, image 12” → “Record no. 28, image
/// 12”).
pub fn increment_first_number(s: &str) -> String {
    let Some(start) = s.find(|c: char| c.is_ascii_digit()) else { return s.to_string() };
    let end = s[start..].find(|c: char| !c.is_ascii_digit()).map_or(s.len(), |e| start + e);
    match s[start..end].parse::<u64>() {
        Ok(n) => {
            let width = end - start;
            format!("{}{:0width$}{}", &s[..start], n + 1, &s[end..])
        }
        Err(_) => s.to_string(),
    }
}

/// Age in years from free input (“32 ans”, “32”, “6 mois” → 0).
pub fn parse_age_years(s: &str) -> Option<i32> {
    let s = sort_key(s.trim());
    if s.is_empty() {
        return None;
    }
    let digits: String = s.chars().take_while(|c| c.is_ascii_digit()).collect();
    let n: i32 = digits.parse().ok()?;
    let unit = s[digits.len()..].trim();
    if unit.starts_with('m')
        || unit.starts_with('j')
        || unit.starts_with('s')
        || unit.starts_with('d')
        || unit.starts_with('w')
    {
        Some(0)
    } else {
        Some(n)
    }
}

/// Age in GEDCOM 7 format (`32y`, `6m`, `12d`).
pub fn gedcom_age(s: &str) -> Option<String> {
    let k = sort_key(s.trim());
    let digits: String = k.chars().take_while(|c| c.is_ascii_digit()).collect();
    let n: u32 = digits.parse().ok()?;
    let unit = k[digits.len()..].trim();
    Some(match unit.chars().next() {
        Some('m') => format!("{n}m"),
        Some('j') | Some('d') => format!("{n}d"),
        Some('s') | Some('w') => format!("{n}w"),
        _ => format!("{n}y"),
    })
}

/// GEDCOM age (`32y`, `6m`) in French (“32 ans”, “6 mois”).
fn age_from_gedcom(s: &str) -> String {
    let s = s.trim();
    let mut out = Vec::new();
    for part in s.split_whitespace() {
        let digits: String = part.chars().take_while(|c| c.is_ascii_digit()).collect();
        let unit = &part[digits.len()..];
        if digits.is_empty() {
            out.push(part.to_string());
            continue;
        }
        let n: usize = digits.parse().unwrap_or(0);
        out.push(match unit {
            "y" => tn!("act.age.years", n).into_owned(),
            "m" => tn!("act.age.months", n).into_owned(),
            "w" => tn!("act.age.weeks", n).into_owned(),
            "d" => tn!("act.age.days", n).into_owned(),
            _ => part.to_string(),
        });
    }
    out.join(" ")
}

fn age_text(s: &str) -> String {
    let s = s.trim();
    match s.parse::<usize>() {
        Ok(n) if s.chars().all(|c| c.is_ascii_digit()) => tn!("act.age.years", n).into_owned(),
        _ => s.to_string(),
    }
}

/// “Témoin : Paul MARCHAND” (“Witness: Paul MARCHAND”).
pub(crate) fn labelled(label: &str, value: &str) -> String {
    t!("act.labelled", label = label, value = value).into_owned()
}

/// Splits “Témoin : Paul MARCHAND” or “Witness: Paul MARCHAND”.
fn split_labelled(s: &str) -> Option<(&str, &str)> {
    s.split_once(" : ").or_else(|| s.split_once(": "))
}

/// “Paul et Marie” (“Paul and Marie”).
fn join_and(names: &[String]) -> String {
    names.join(&t!("act.and"))
}

fn capitalize(s: &str) -> String {
    let mut c = s.chars();
    c.next().map(|f| f.to_uppercase().chain(c).collect()).unwrap_or_default()
}

// ----------------------------------------------------------------------
// Writing to the document
// ----------------------------------------------------------------------

struct Writer<'a> {
    ged: &'a mut Gedcom,
    res: ActResult,
}

impl Writer<'_> {
    fn touch(&mut self, x: &str) {
        if !self.res.touched.iter().any(|t| t == x) {
            self.res.touched.push(x.to_string());
            if !self.res.created.iter().any(|c| c == x)
                && let Some(n) = self.ged.get_mut(x)
            {
                n.touch();
            }
        }
    }

    fn mark_created(&mut self, x: &str) {
        self.res.created.push(x.to_string());
        if !self.res.touched.iter().any(|t| t == x) {
            self.res.touched.push(x.to_string());
        }
    }

    fn sex(&self, x: &str) -> String {
        self.ged.individual(x).map(|i| i.sex().to_string()).unwrap_or_default()
    }

    fn link(&mut self, indi: &str, tag: &str, fam: &str) {
        let ptr = format!("@{fam}@");
        let n = self.ged.get_mut(indi).unwrap();
        if !n.children.iter().any(|c| c.tag == tag && c.val() == ptr) {
            n.insert_child(Node::with_value(tag, ptr));
            self.touch(indi);
        }
    }

    /// The couple's family, created if needed.
    fn ensure_couple(&mut self, a: &str, b: &str) -> String {
        let pa = format!("@{a}@");
        let pb = format!("@{b}@");
        let found = self.ged.records_of(RecordKind::Family).find(|f| {
            let spouses: Vec<&str> =
                f.children.iter().filter(|c| c.tag == "HUSB" || c.tag == "WIFE").map(|c| c.val()).collect();
            spouses.contains(&pa.as_str()) && spouses.contains(&pb.as_str())
        });
        if let Some(f) = found.and_then(|f| f.xref.clone()) {
            return f;
        }
        // A family with only one of them can take the other.
        let half = self.ged.records_of(RecordKind::Family).find_map(|f| {
            let h = f.child_val("HUSB");
            let w = f.child_val("WIFE");
            match (h, w) {
                (Some(h), None) if h == pa || h == pb => Some((f.xref.clone()?, if h == pa { b } else { a }, "WIFE")),
                (None, Some(w)) if w == pa || w == pb => Some((f.xref.clone()?, if w == pa { b } else { a }, "HUSB")),
                _ => None,
            }
        });
        if let Some((fam, other, slot)) = half
            && (self.sex(other) != if slot == "HUSB" { "F" } else { "M" })
        {
            self.ged.get_mut(&fam).unwrap().insert_child(Node::with_value(slot, format!("@{other}@")));
            self.touch(&fam);
            self.link(other, "FAMS", &fam);
            return fam;
        }
        let (h, w) = if self.sex(a) == "F" || self.sex(b) == "M" { (b, a) } else { (a, b) };
        let fam = self.ged.create(RecordKind::Family);
        let node = self.ged.get_mut(&fam).unwrap();
        node.insert_child(Node::with_value("HUSB", format!("@{h}@")));
        node.insert_child(Node::with_value("WIFE", format!("@{w}@")));
        self.mark_created(&fam);
        self.res.families_created.push(fam.clone());
        self.link(h, "FAMS", &fam);
        self.link(w, "FAMS", &fam);
        fam
    }

    /// Attaches the child to their parents' family (created or completed if needed).
    fn ensure_parents(&mut self, child: &str, father: Option<&str>, mother: Option<&str>) {
        if father.is_none() && mother.is_none() {
            return;
        }
        let pf = father.map(|f| format!("@{f}@"));
        let pm = mother.map(|m| format!("@{m}@"));
        // Famille d'origine existante compatible.
        let famc: Vec<String> =
            self.ged.individual(child).map(|i| i.famc().iter().map(|s| s.to_string()).collect()).unwrap_or_default();
        let compatible = famc.into_iter().find(|f| {
            let Some(fam) = self.ged.get(f) else { return false };
            let ok = |tag: &str, want: &Option<String>| match (fam.child_val(tag), want) {
                (_, None) | (None, _) => true,
                (Some(have), Some(w)) => have == w,
            };
            ok("HUSB", &pf) && ok("WIFE", &pm)
        });
        let fam = match compatible {
            Some(f) => {
                for (tag, p) in [("HUSB", father), ("WIFE", mother)] {
                    if let Some(p) = p
                        && self.ged.get(&f).unwrap().child(tag).is_none()
                    {
                        self.ged.get_mut(&f).unwrap().insert_child(Node::with_value(tag, format!("@{p}@")));
                        self.touch(&f);
                        self.link(p, "FAMS", &f);
                    }
                }
                f
            }
            None => match (father, mother) {
                (Some(f), Some(m)) => self.ensure_couple(f, m),
                (Some(p), None) | (None, Some(p)) if self.single_family(p).is_some() => {
                    // Only one parent named, who has only one family: the child belongs to it.
                    self.single_family(p).unwrap()
                }
                (Some(p), None) | (None, Some(p)) => {
                    let tag = if father.is_some() { "HUSB" } else { "WIFE" };
                    let fam = self.ged.create(RecordKind::Family);
                    self.ged.get_mut(&fam).unwrap().insert_child(Node::with_value(tag, format!("@{p}@")));
                    self.mark_created(&fam);
                    self.res.families_created.push(fam.clone());
                    self.link(p, "FAMS", &fam);
                    fam
                }
                (None, None) => unreachable!(),
            },
        };
        let ptr = format!("@{child}@");
        let node = self.ged.get_mut(&fam).unwrap();
        if !node.children.iter().any(|c| c.tag == "CHIL" && c.val() == ptr) {
            // Children go after the spouses and the children already there.
            let at = node
                .children
                .iter()
                .rposition(|c| matches!(c.tag.as_str(), "HUSB" | "WIFE" | "CHIL"))
                .map(|i| i + 1)
                .unwrap_or(0);
            node.children.insert(at, Node::with_value("CHIL", ptr));
            self.touch(&fam);
        }
        self.link(child, "FAMC", &fam);
    }

    /// The only family where the individual is a spouse.
    fn single_family(&self, x: &str) -> Option<String> {
        let fams = self.ged.individual(x)?.fams();
        (fams.len() == 1).then(|| fams[0].to_string())
    }

    /// Adds the record's event, or merges it with an existing event: a unique event (birth, death…)
    /// with compatible dates, or an event of the same type that is still empty (no date or source).
    /// A date or place that contradicts the existing event's is settled by `conflict`. Returns its
    /// index.
    fn add_event(&mut self, x: &str, mut ev: Node, conflict: Conflict) -> usize {
        let unique = matches!(ev.tag.as_str(), "BIRT" | "CHR" | "BAPM" | "DEAT" | "BURI" | "CREM");
        let node = self.ged.get_mut(x).unwrap();
        let existing = node.children.iter().position(|c| {
            c.tag == ev.tag
                && c.child_val("TYPE") == ev.child_val("TYPE")
                && (unique || (c.child("DATE").is_none() && c.child("SOUR").is_none()))
        });
        if let Some(i) = existing {
            let old = &node.children[i];
            let date = old.child_val("DATE").is_some_and(|d| {
                crate::proposal::date_change(d, ev.child_val("DATE").unwrap_or("")) == crate::proposal::Change::Differs
            });
            let differs = |tag: &str| {
                old.child_val(tag).is_some_and(|p| {
                    crate::proposal::text_change(p, ev.child_val(tag).unwrap_or("")) == crate::proposal::Change::Differs
                })
            };
            // The address goes with the place: another address is another place.
            let place = differs("PLAC") || differs("ADDR");
            if date || place {
                match conflict {
                    Conflict::KeepBoth | Conflict::Replace => {
                        let at = if conflict == Conflict::Replace { i } else { i + 1 };
                        node.children.insert(at, ev);
                        self.touch(x);
                        return at;
                    }
                    Conflict::Ignore => ev
                        .children
                        .retain(|c| !(date && c.tag == "DATE" || place && matches!(c.tag.as_str(), "PLAC" | "ADDR"))),
                }
            }
        }
        let idx = match existing {
            Some(i) if compatible_dates(node.children[i].child_val("DATE"), ev.child_val("DATE")) => {
                let target = &mut node.children[i];
                for c in ev.children {
                    match c.tag.as_str() {
                        "DATE" => {
                            let precise = target.child_val("DATE").is_none_or(|d| !starts_with_day_or_year(d));
                            if precise {
                                let at = target.children.iter().position(|n| n.tag == "DATE");
                                match at {
                                    Some(at) => target.children[at] = c,
                                    None => target.children.insert(0, c),
                                }
                            }
                        }
                        "PLAC" => {
                            if target.child("PLAC").is_none() {
                                let at = target
                                    .children
                                    .iter()
                                    .position(|c| c.tag == "SOUR")
                                    .unwrap_or(target.children.len());
                                target.children.insert(at, c);
                            }
                        }
                        "ADDR" => {
                            if target.child("ADDR").is_none() {
                                let at = match target.children.iter().position(|c| c.tag == "PLAC") {
                                    Some(p) => p + 1,
                                    None => target
                                        .children
                                        .iter()
                                        .position(|c| c.tag == "SOUR")
                                        .unwrap_or(target.children.len()),
                                };
                                target.children.insert(at, c);
                            }
                        }
                        "AGE" | "TYPE" => {
                            if target.child(&c.tag).is_none() {
                                target.children.insert(0, c);
                            }
                        }
                        "SOUR" => merge_citation(&mut target.children, c),
                        _ => {
                            let at =
                                target.children.iter().position(|c| c.tag == "SOUR").unwrap_or(target.children.len());
                            target.children.insert(at, c);
                        }
                    }
                }
                i
            }
            _ => {
                node.insert_child(ev);
                node.children.iter().rposition(|c| c.tag != "CHAN" && c.tag != "CREA").unwrap()
            }
        };
        self.touch(x);
        idx
    }

    /// Adds a fact (occupation, residence), unless it already exists with the same value and date.
    fn add_fact(&mut self, x: &str, fact: Node) {
        self.add_fact_with(x, fact, Conflict::KeepBoth)
    }

    /// Like `add_fact`; with [`Conflict::Replace`], a new value goes before the person's other
    /// facts of the same kind, preferred.
    fn add_fact_with(&mut self, x: &str, fact: Node, conflict: Conflict) {
        let node = self.ged.get_mut(x).unwrap();
        let same = node.children.iter_mut().find(|c| {
            c.tag == fact.tag
                && c.val() == fact.val()
                && c.child_val("DATE") == fact.child_val("DATE")
                && c.child_val("PLAC") == fact.child_val("PLAC")
                && fact.child("ADDR").is_none_or(|_| crate::address::address_of(c) == crate::address::address_of(&fact))
        });
        match same {
            Some(existing) => {
                let cite = fact.children.into_iter().find(|c| c.tag == "SOUR").unwrap();
                if !existing.children.contains(&cite) {
                    existing.children.push(cite);
                }
            }
            None => {
                let first = node
                    .children
                    .iter()
                    .position(|c| c.tag == fact.tag && c.child_val("TYPE") == fact.child_val("TYPE"));
                match (conflict, first) {
                    (Conflict::Replace, Some(at)) => node.children.insert(at, fact),
                    (Conflict::Ignore, Some(_)) => {}
                    _ => node.insert_child(fact),
                }
            }
        }
        self.touch(x);
    }

    fn has_citation(&self, x: &str, cite: &Node) -> bool {
        self.ged.get(x).is_some_and(|n| n.children.contains(cite))
    }

    fn has_birth(&self, x: &str) -> bool {
        self.ged.get(x).is_some_and(|n| n.children.iter().any(|c| matches!(c.tag.as_str(), "BIRT" | "CHR" | "BAPM")))
    }
}

/// Adds a citation, or completes an existing citation of the same source (without a reference, or
/// with the same one) rather than duplicating it.
fn merge_citation(children: &mut Vec<Node>, cite: Node) {
    let page = cite.child_val("PAGE").map(str::to_string);
    let same = children.iter_mut().find(|c| {
        c.tag == "SOUR"
            && c.value == cite.value
            && match (c.child_val("PAGE"), page.as_deref()) {
                (None, _) => c.child("EVEN").is_none() || c.child("EVEN") == cite.child("EVEN"),
                (Some(a), Some(b)) => a == b && (c.child("EVEN").is_none() || c.child("EVEN") == cite.child("EVEN")),
                (Some(_), None) => false,
            }
    });
    match same {
        Some(existing) => {
            for sub in cite.children {
                if !existing
                    .children
                    .iter()
                    .any(|e| e.tag == sub.tag && (sub.tag != "OBJE" && sub.tag != "NOTE" || *e == sub))
                {
                    // PAGE first, as other software writes it.
                    if sub.tag == "PAGE" {
                        existing.children.insert(0, sub);
                    } else {
                        existing.children.push(sub);
                    }
                }
            }
        }
        None => children.push(cite),
    }
}

/// Is an existing date (`ABT 1850`, `BEF 1900`…) compatible with the new one?
fn compatible_dates(old: Option<&str>, new: Option<&str>) -> bool {
    match (old, new) {
        (None, _) | (_, None) => true,
        (Some(a), Some(b)) if a.trim() == b.trim() => true,
        (Some(a), Some(_)) => !starts_with_day_or_year(a),
    }
}

/// Exact date (starts with a day or a year) rather than approximate (`ABT`, `EST`…).
fn starts_with_day_or_year(d: &str) -> bool {
    d.trim().chars().next().is_some_and(|c| c.is_ascii_digit())
}

// ----------------------------------------------------------------------
// Reading a source's records
// ----------------------------------------------------------------------

/// Does the citation point to this source?
fn cites(n: &Node, ptr: &str) -> bool {
    n.tag == "SOUR" && n.val() == ptr
}

/// Key of the record a citation belongs to.
pub(crate) fn key_of(cite: &Node) -> ActKey {
    ActKey {
        page: cite.child_val("PAGE").unwrap_or("").trim().to_string(),
        date: cite.path_val(&["DATA", "DATE"]).unwrap_or("").trim().to_string(),
    }
}

/// A citation of a source in a record: at the record's level (`event` empty) or on one of its
/// events / facts.
struct Hit<'a> {
    record: &'a Node,
    event: Option<&'a Node>,
    cite: &'a Node,
}

/// Citations (levels 1 and 2) of the individuals and families that point to the source.
fn hits<'a>(ged: &'a Gedcom, source: &str) -> Vec<Hit<'a>> {
    let ptr = format!("@{source}@");
    let mut out = Vec::new();
    for r in ged.records.iter().map(|r| &**r).filter(|r| r.tag == "INDI" || r.tag == "FAM") {
        for c in &r.children {
            if cites(c, &ptr) {
                out.push(Hit { record: r, event: None, cite: c });
            } else {
                for cc in c.children.iter().filter(|cc| cites(cc, &ptr)) {
                    out.push(Hit { record: r, event: Some(c), cite: cc });
                }
            }
        }
    }
    out
}

/// Role (`ROLE`) and detail (`PHRASE`) of a citation.
fn cite_role(cite: &Node) -> (&str, &str) {
    let role = cite.child("EVEN").and_then(|e| e.child("ROLE"));
    (role.map(|r| r.val()).unwrap_or(""), role.and_then(|r| r.child_val("PHRASE")).unwrap_or(""))
}

/// Removes from the detail the part inferred from the role (“Godfather”, “Father of the groom”), to
/// keep only the declared relationship.
fn kinship_from_phrase(role: ActRole, phrase: &str) -> String {
    phrase
        .split(", ")
        .filter(|part| {
            let k = sort_key(part);
            let base = crate::i18n::LANGUAGES.iter().filter_map(|l| role.base_phrase(l)).any(|b| sort_key(&b) == k);
            let of_principal = crate::i18n::LANGUAGES.iter().any(|l| {
                let of = sort_key(&t!("act.role_of", role = role.label_in(l, "", false, 0), name = "", locale = l));
                k.starts_with(of.trim_end())
            });
            !(base || (role.relative() && of_principal))
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// Splits “Given names SURNAME” (surname in capitals at the end).
fn split_name(name: &str) -> (String, String) {
    let words: Vec<&str> = name.split_whitespace().collect();
    let upper = |w: &str| w.chars().any(char::is_alphabetic) && !w.chars().any(char::is_lowercase);
    let cut = words.iter().rposition(|w| !upper(w)).map_or(0, |i| i + 1);
    (words[..cut].join(" "), words[cut..].join(" "))
}

impl Gedcom {
    /// Records of a source, rebuilt from the citations that point to it, in chronological order.
    pub fn acts(&self, source: &str) -> Vec<Act> {
        let all = hits(self, source);
        let mut keys: Vec<ActKey> = Vec::new();
        for h in &all {
            let k = key_of(h.cite);
            if !keys.contains(&k) {
                keys.push(k);
            }
        }
        let mut acts: Vec<Act> = keys
            .into_iter()
            .map(|k| {
                let group: Vec<&Hit> = all.iter().filter(|h| key_of(h.cite) == k).collect();
                self.read_act(source, k, &group)
            })
            .collect();
        acts.sort_by_cached_key(|a| {
            let d = if a.date.is_empty() { &a.record_date } else { &a.date };
            let numbers: Vec<u64> =
                a.page.split(|c: char| !c.is_ascii_digit()).filter_map(|n| n.parse().ok()).collect();
            (d.trim().is_empty(), crate::Date::parse(d).sort_key(), numbers, a.page.clone())
        });
        acts
    }

    /// Record of the source identified by its key.
    pub fn act(&self, source: &str, key: &ActKey) -> Option<Act> {
        let all = hits(self, source);
        let group: Vec<&Hit> = all.iter().filter(|h| key_of(h.cite) == *key).collect();
        (!group.is_empty()).then(|| self.read_act(source, key.clone(), &group))
    }

    fn read_act(&self, source: &str, key: ActKey, group: &[&Hit]) -> Act {
        // Event: the one the citations point to, otherwise the one that contains them.
        let event = group
            .iter()
            .find_map(|h| h.cite.child_val("EVEN").filter(|e| !e.is_empty()))
            .or_else(|| {
                group.iter().find_map(|h| {
                    h.event.map(|e| e.tag.as_str()).filter(|t| !matches!(*t, "OCCU" | "RESI" | "NAME" | "SEX"))
                })
            })
            .unwrap_or("")
            .to_string();
        let mut act = Act::new(source, &event);
        act.participants.clear();
        act.estimate_births = true;
        act.page = key.page.clone();
        act.original = Some(key.clone());

        let is_main = |h: &Hit| h.event.is_some_and(|e| e.tag == event && !event.is_empty());
        let main: Vec<&&Hit> = group.iter().filter(|h| is_main(h)).collect();
        act.family_even = main.first().is_some_and(|h| h.record.tag == "FAM");
        if let Some(e) = main.first().and_then(|h| h.event) {
            act.event_type = e.child_val("TYPE").unwrap_or("").to_string();
            if let Some(d) = e.child("DATE") {
                act.date = d.val().to_string();
                act.date_phrase = d.child_val("PHRASE").unwrap_or("").to_string();
            }
            act.place = e.child_val("PLAC").unwrap_or("").to_string();
            act.address = crate::address::address_of(e).unwrap_or_default();
        }
        // Citation data: the main citation first.
        let ordered: Vec<&&Hit> = main.iter().copied().chain(group.iter().filter(|h| !is_main(h))).collect();
        if let Some(d) = ordered.iter().find_map(|h| h.cite.child("DATA").and_then(|d| d.child("DATE"))) {
            let (v, p) = (d.val().to_string(), d.child_val("PHRASE").unwrap_or("").to_string());
            if v != act.date || p != act.date_phrase {
                act.record_date = v;
                act.record_date_phrase = p;
            }
        }
        act.transcription = ordered
            .iter()
            .find_map(|h| h.cite.child("DATA").and_then(|d| d.child("TEXT")))
            .map(|t| t.text())
            .unwrap_or_default();
        act.quality = ordered.iter().find_map(|h| h.cite.child_val("QUAY")).and_then(|q| q.trim().parse().ok());
        act.media = ordered
            .iter()
            .find_map(|h| h.cite.child("OBJE").and_then(|o| o.pointer()))
            .map(|m| MediaChoice::Existing(m.to_string()))
            .unwrap_or_default();

        // Personnes.
        let mut people: Vec<(String, Participant)> = Vec::new();
        let person = |x: &str, role: ActRole| -> Participant {
            let mut p = Participant::new(role);
            p.choice = PersonChoice::Link(x.to_string());
            if let Some(i) = self.individual(x) {
                p.set(Surname, i.surname());
                p.set(Given, i.given());
                if !matches!(i.sex(), "" | "U") {
                    p.set(Sex, i.sex());
                }
            }
            p
        };
        // Principal people: on the event (or the spouses of the family).
        for h in &main {
            let e = h.event.unwrap();
            if h.record.tag == "FAM" {
                for tag in ["HUSB", "WIFE"] {
                    let Some(x) = h.record.child(tag).and_then(|n| n.pointer()) else { continue };
                    if people.iter().any(|(y, _)| y == x) {
                        continue;
                    }
                    let mut p = person(x, ActRole::Principal);
                    if let Some(a) = e.child(tag).and_then(|n| n.child_val("AGE")) {
                        p.set(Age, age_from_gedcom(a));
                    }
                    people.push((x.to_string(), p));
                }
            } else if let Some(x) = h.record.xref.as_deref()
                && !people.iter().any(|(y, _)| y == x)
            {
                let mut p = person(x, ActRole::Principal);
                if let Some(a) = e.child_val("AGE") {
                    p.set(Age, age_from_gedcom(a));
                }
                let (role, phrase) = cite_role(h.cite);
                if !phrase.is_empty() && role != "PRIN" {
                    p.set(Kinship, phrase);
                }
                people.push((x.to_string(), p));
            }
        }
        // The others: role from the citation, and the facts it proves.
        for h in group.iter().filter(|h| !is_main(h) && h.record.tag == "INDI") {
            let Some(x) = h.record.xref.as_deref() else { continue };
            let at = match people.iter().position(|(y, _)| y == x) {
                Some(i) => i,
                None => {
                    let (role, phrase) = cite_role(h.cite);
                    let sex = self.individual(x).map(|i| i.sex().to_string()).unwrap_or_default();
                    let r = ActRole::from_gedcom(role, phrase, &sex);
                    let mut p = person(x, r);
                    let kin = kinship_from_phrase(r, phrase);
                    if !kin.is_empty() {
                        p.set(Kinship, kin);
                    }
                    people.push((x.to_string(), p));
                    people.len() - 1
                }
            };
            let p = &mut people[at].1;
            // Occupation and residence at the record's date fill the person's fields; at another
            // date, or a second one, they are other facts.
            let at_act_date = |e: &Node| e.child_val("DATE").unwrap_or("") == act.date.trim();
            match h.event {
                Some(e) if e.tag == "OCCU" && at_act_date(e) && p.get(Occupation).is_empty() => {
                    p.set(Occupation, e.val())
                }
                // A residence with a street address stays a fact, which keeps it.
                Some(e)
                    if e.tag == "RESI"
                        && at_act_date(e)
                        && p.get(Residence).is_empty()
                        && e.child("ADDR").is_none() =>
                {
                    p.set(Residence, e.child_val("PLAC").unwrap_or(""))
                }
                // A dated birth or baptism is a fact; an estimated one comes from the age.
                Some(e)
                    if matches!(e.tag.as_str(), "BIRT" | "CHR" | "BAPM")
                        && e.child_val("DATE").is_none_or(|d| d.starts_with("EST ")) =>
                {
                    if let Some(pl) = e.child_val("PLAC") {
                        p.set(Birthplace, pl);
                    }
                    // Estimated birth from the declared age.
                    if p.get(Age).is_empty()
                        && let Some(est) = e.child_val("DATE").and_then(|d| d.strip_prefix("EST "))
                        && let (Ok(b), Some(y)) = (est.trim().parse::<i32>(), act.year())
                    {
                        p.set(Age, tn!("act.age.years", (y - b).max(0) as usize).into_owned());
                    }
                }
                Some(e) if !matches!(e.tag.as_str(), "NAME" | "SEX") && crate::model::event_label(&e.tag).is_some() => {
                    let fact = Fact {
                        tag: e.tag.clone(),
                        event_type: e.child_val("TYPE").unwrap_or("").to_string(),
                        value: if crate::events::detail_in_value(&e.tag) { e.val().to_string() } else { String::new() },
                        date: e.child_val("DATE").unwrap_or("").to_string(),
                        place: e.child_val("PLAC").unwrap_or("").to_string(),
                        address: crate::address::address_of(e).unwrap_or_default(),
                    };
                    if !p.facts.contains(&fact) {
                        p.facts.push(fact);
                    }
                }
                _ => {}
            }
        }
        // Father, mother, spouse of which principal person?
        let principals: Vec<String> =
            people.iter().filter(|(_, p)| p.role == ActRole::Principal).map(|(x, _)| x.clone()).collect();
        if principals.len() > 1 {
            let family = act.is_family();
            for (x, p) in people.iter_mut().filter(|(_, p)| p.role.relative()) {
                let phrase = group
                    .iter()
                    .find(|h| h.record.xref.as_deref() == Some(x.as_str()))
                    .map(|h| sort_key(cite_role(h.cite).1))
                    .unwrap_or_default();
                let names = |key: fn(&str) -> Cow<'static, str>| {
                    crate::i18n::LANGUAGES.iter().any(|l| phrase.contains(&sort_key(&key(l))))
                };
                let by_phrase = if family && names(|l| t!("act.the_wife", locale = l)) {
                    Some(1)
                } else if family && names(|l| t!("act.the_husband", locale = l)) {
                    Some(0)
                } else {
                    None
                };
                let by_links = principals.iter().position(|pr| {
                    let (f, m) = self.parents(pr);
                    match p.role {
                        ActRole::Spouse => {
                            self.spouses_and_children(pr).iter().any(|(s, _)| s.as_ref().is_some_and(|s| s.xref() == x))
                        }
                        _ => [f, m].into_iter().flatten().any(|i| i.xref() == x),
                    }
                });
                p.of = by_phrase.or(by_links).unwrap_or(0);
            }
        }
        // Mentioned people: in the note of the main citation.
        let note_cite = main.first().map(|h| h.cite).or_else(|| group.first().map(|h| h.cite));
        let mut kept_notes: Vec<String> = Vec::new();
        let family = act.is_family();
        for n in note_cite.into_iter().flat_map(|c| c.children_with("NOTE")) {
            for part in n.text().split("; ") {
                let parsed = split_labelled(part).and_then(|(label, rest)| {
                    let role = ActRole::ALL
                        .into_iter()
                        .find(|r| *r != ActRole::Principal && r.has_label(&event, family, label.trim()))?;
                    let mut fields = rest.split(", ");
                    let (given, surname) = split_name(fields.next().unwrap_or(""));
                    let mut p = Participant::new(role).with(Given, &given).with(Surname, &surname);
                    p.choice = PersonChoice::Mention;
                    let more: Vec<&str> = fields.collect();
                    if !more.is_empty() {
                        p.set(Note, more.join(", "));
                    }
                    Some(p)
                });
                match parsed {
                    Some(p) => people.push((String::new(), p)),
                    None if !part.trim().is_empty() => kept_notes.push(part.trim().to_string()),
                    None => {}
                }
            }
        }
        act.note = kept_notes.join("; ");

        // Order: principal people, then by role.
        let mut list: Vec<Participant> = people.into_iter().map(|(_, p)| p).collect();
        list.sort_by_key(|p| (ActRole::ALL.iter().position(|r| *r == p.role).unwrap_or(99), p.of));
        act.participants = list;
        act
    }

    /// Deletes a record: its citations and the witnesses it associated with events. Events, facts,
    /// individuals and family links stay. Returns the modified records.
    pub fn remove_act(&mut self, source: &str, key: &ActKey) -> Vec<String> {
        self.unlink_act(source, key, false)
    }

    /// Removes the record's citations. `strip` (editing the record): what only this record
    /// established is emptied too (date, place and age of its event; occupation, residence,
    /// estimated birth), so that the new version writes it again at the same place.
    fn unlink_act(&mut self, source: &str, key: &ActKey, strip: bool) -> Vec<String> {
        let ptr = format!("@{source}@");
        let act = self.act(source, key);
        let event = act.as_ref().map(|a| a.event.clone()).unwrap_or_default();
        let people: Vec<String> = act
            .map(|a| {
                a.participants
                    .iter()
                    .filter_map(|p| match &p.choice {
                        PersonChoice::Link(x) => Some(format!("@{x}@")),
                        _ => None,
                    })
                    .collect()
            })
            .unwrap_or_default();
        let matches = |n: &Node| cites(n, &ptr) && key_of(n) == *key;
        let in_record = |r: &Node| {
            (r.tag == "INDI" || r.tag == "FAM")
                && r.children.iter().any(|c| matches(c) || c.children.iter().any(&matches))
        };
        // Witness associated by the record: `ASSO` to a person of the record, with only its role.
        let act_asso = |n: &Node| {
            n.tag == "ASSO" && people.iter().any(|p| p == n.val()) && n.children.iter().all(|c| c.tag == "ROLE")
        };
        let mut touched = Vec::new();
        for r in self.records_where_mut(in_record) {
            r.children.retain(|c| !matches(c));
            let mut drop = Vec::new();
            for (i, c) in r.children.iter_mut().enumerate() {
                let before = c.children.len();
                c.children.retain(|cc| !matches(cc));
                if c.children.len() == before {
                    continue;
                }
                c.children.retain(|n| !act_asso(n));
                if !strip || c.children.iter().any(|n| n.tag == "SOUR") {
                    continue;
                }
                if c.tag == event {
                    // The record's event: the new version completes it at the same place.
                    c.children.retain(|n| {
                        !(matches!(n.tag.as_str(), "DATE" | "PLAC" | "ADDR" | "AGE")
                            || (matches!(n.tag.as_str(), "HUSB" | "WIFE") && n.children.iter().all(|a| a.tag == "AGE")))
                    });
                } else if matches!(c.tag.as_str(), "OCCU" | "RESI" | "BIRT")
                    && c.children.iter().all(|n| matches!(n.tag.as_str(), "DATE" | "PLAC"))
                    && (c.tag != "BIRT" || c.child_val("DATE").is_none_or(|d| d.starts_with("EST ")))
                {
                    // Fact inferred by the record (occupation, residence, estimated birth).
                    drop.push(i);
                }
            }
            for i in drop.into_iter().rev() {
                r.children.remove(i);
            }
            r.touch();
            if let Some(x) = r.xref.clone() {
                touched.push(x);
            }
        }
        touched
    }

    /// Removes the events left empty after a record was edited.
    fn drop_empty_events(&mut self, records: &[String]) {
        for x in records {
            let Some(r) = self.get_mut(x) else { continue };
            r.children.retain(|c| {
                let empty = c.value.as_deref().is_none_or(|v| v.trim().is_empty())
                    && c.children.iter().all(|n| n.tag == "TYPE")
                    && crate::model::event_label(&c.tag).is_some();
                !empty
            });
        }
    }
}

/// Score of the names typed for a person (`surname`, `given`, as sort keys) against an individual's
/// (`s`, `g`); `None` when they cannot be the same person.
fn name_score(surname: &str, given: &str, s: &str, g: &str) -> Option<f32> {
    let mut score = 0.0;
    if !surname.is_empty() {
        if s == surname {
            score += 3.0;
        } else if !s.is_empty() && (s.contains(surname) || surname.contains(s)) {
            score += 1.5;
        } else {
            return None;
        }
    }
    let first_given = given.split_whitespace().next().unwrap_or("");
    if !first_given.is_empty() {
        if g == given {
            score += 3.0;
        } else if g.split_whitespace().next() == Some(first_given) {
            score += 2.5;
        } else if g.split_whitespace().any(|w| w == first_given) {
            score += 1.5;
        } else if g.starts_with(&first_given.chars().take(3).collect::<String>()) {
            score += 0.5;
        } else if !g.is_empty() {
            return None;
        }
    }
    Some(score)
}

/// The surname and given names of `p` are in each other's field, compared with `ind`.
pub fn names_swapped(p: &Participant, ind: &Individual) -> bool {
    let (surname, given) = (sort_key(p.get(Surname)), sort_key(p.get(Given)));
    let (s, g) = (sort_key(&ind.surname()), sort_key(&ind.given()));
    !(surname.is_empty() && given.is_empty())
        && name_score(&surname, &given, &s, &g).is_none()
        && name_score(&given, &surname, &s, &g).is_some()
}

// ----------------------------------------------------------------------
// Matches with existing individuals
// ----------------------------------------------------------------------

impl Act {
    /// Existing individuals who could be the person named at index `idx`, most likely first.
    pub fn suggestions(&self, ged: &Gedcom, idx: usize) -> Vec<Suggestion> {
        let Some(p) = self.effective().get(idx).cloned() else { return vec![] };
        let nth = self.nth_principal(idx);
        let surname = sort_key(p.get(Surname));
        let given = sort_key(p.get(Given));
        // Nothing entered for this person (the inherited surname is not enough): inferred links
        // only.
        if self.participants.get(idx).is_none_or(|p| p.is_blank()) || (surname.is_empty() && given.is_empty()) {
            return self.context_suggestions(ged, idx);
        }
        let year = self.year();
        let expected_birth = match (year, parse_age_years(p.get(Age))) {
            (Some(y), Some(a)) => Some(y - a),
            (Some(y), None) if p.role == ActRole::Principal && is_birth_like(&self.event) => Some(y),
            _ => None,
        };
        let sex = p.role.sex(self.is_family(), nth).map(str::to_string).unwrap_or_else(|| p.get(Sex).to_uppercase());
        let context: Vec<(String, String)> =
            self.context_suggestions(ged, idx).into_iter().map(|s| (s.xref, s.reason)).collect();

        let mut out: Vec<Suggestion> = ged
            .records_of(RecordKind::Individual)
            .filter_map(|r| {
                let i = Individual::new(r);
                let s = sort_key(&i.surname());
                let g = sort_key(&i.given());
                let mut reasons: Vec<String> = Vec::new();
                // Surname and given names typed in each other's field: still a match, said so.
                let mut score = match name_score(&surname, &given, &s, &g) {
                    Some(n) => n,
                    None => {
                        let n = name_score(&given, &surname, &s, &g)?;
                        reasons.push(t!("act.reason.swapped").into_owned());
                        n
                    }
                };
                if matches!(sex.as_str(), "M" | "F") && matches!(i.sex(), "M" | "F") && i.sex() != sex {
                    return None;
                }
                if let (Some(exp), Some(b)) = (expected_birth, i.birth_year()) {
                    let d = (exp - b).abs();
                    if d <= 2 {
                        score += 2.0;
                    } else if d <= 10 {
                        score += 0.5;
                    } else {
                        score -= 3.0;
                    }
                }
                // Died before the record (except for the death or burial record itself).
                if let (Some(y), Some(d)) = (year, i.death().and_then(|e| e.date()).and_then(|d| d.year()))
                    && d < y
                    && !(p.role == ActRole::Principal && is_death_like(&self.event))
                {
                    score -= 4.0;
                }
                if let Some(b) = i.birth_year()
                    && let Some(y) = year
                {
                    // Born after the record, or too young to be a parent / spouse.
                    let min_age = match p.role {
                        ActRole::Father | ActRole::Mother => 14,
                        ActRole::Spouse => 12,
                        ActRole::Principal if self.is_family() => 12,
                        _ => 0,
                    };
                    if b > y - min_age {
                        score -= 5.0;
                    }
                }
                if let Some((_, why)) = context.iter().find(|(x, _)| x == i.xref()) {
                    score += 5.0;
                    reasons.push(why.clone());
                }
                if let Some(b) = i.birth().and_then(|e| e.date()) {
                    reasons.push(t!("act.suggestion.born", date = b.display()).into_owned());
                }
                (score >= 3.0).then(|| Suggestion {
                    xref: i.xref().to_string(),
                    label: suggestion_label(&i),
                    score,
                    reason: reasons.join(" · "),
                })
            })
            .collect();
        out.sort_by(|a, b| b.score.total_cmp(&a.score));
        out.truncate(5);
        out
    }

    /// Suggestions inferred from the people already linked (father of the linked child, spouse…).
    fn context_suggestions(&self, ged: &Gedcom, idx: usize) -> Vec<Suggestion> {
        let Some(me) = self.participants.get(idx) else { return vec![] };
        let nths: Vec<usize> = (0..self.participants.len()).map(|i| self.nth_principal(i)).collect();
        let linked_where = |pred: &dyn Fn(usize, &Participant) -> bool| -> Vec<String> {
            self.participants
                .iter()
                .enumerate()
                .filter(|(i, p)| *i != idx && pred(*i, p))
                .filter_map(|(_, p)| match &p.choice {
                    PersonChoice::Link(x) => Some(x.clone()),
                    _ => None,
                })
                .collect()
        };
        let mk = |x: &str, why: String| {
            ged.individual(x).map(|i| Suggestion {
                xref: x.to_string(),
                label: suggestion_label(&i),
                score: 10.0,
                reason: why,
            })
        };
        let mut out = Vec::new();
        let spouses_of = |x: &str, out: &mut Vec<Suggestion>| {
            let who = ged.record_label(x);
            for (s, _) in ged.spouses_and_children(x) {
                out.extend(s.and_then(|s| mk(s.xref(), t!("act.reason.spouse_of", name = who).into_owned())));
            }
        };
        match me.role {
            ActRole::Father | ActRole::Mother => {
                let of = me.of;
                for c in linked_where(&|i, p| p.role == ActRole::Principal && nths[i] == of) {
                    let (f, m) = ged.parents(&c);
                    let p = if me.role == ActRole::Father { f } else { m };
                    let child = ged.record_label(&c);
                    out.extend(p.and_then(|p| mk(p.xref(), t!("act.reason.parent_of", name = child).into_owned())));
                }
                // The other parent is linked: suggest their spouse.
                let other = if me.role == ActRole::Father { ActRole::Mother } else { ActRole::Father };
                for o in linked_where(&|_, p| p.role == other && p.of == of) {
                    spouses_of(&o, &mut out);
                }
            }
            ActRole::Spouse => {
                let of = me.of;
                for c in linked_where(&|i, p| p.role == ActRole::Principal && nths[i] == of) {
                    spouses_of(&c, &mut out);
                }
            }
            ActRole::Principal => {
                let mine = nths[idx];
                // Child of a father or mother already linked; spouse of the other spouse.
                for par in linked_where(&|_, p| matches!(p.role, ActRole::Father | ActRole::Mother) && p.of == mine) {
                    let who = ged.record_label(&par);
                    for (_, kids) in ged.spouses_and_children(&par) {
                        for kid in kids {
                            out.extend(mk(kid.xref(), t!("act.reason.child_of", name = who).into_owned()));
                        }
                    }
                }
                if self.is_family() {
                    for other in linked_where(&|_, p| p.role == ActRole::Principal) {
                        spouses_of(&other, &mut out);
                    }
                }
            }
            _ => {}
        }
        // The same individual can be suggested for several reasons: group them.
        let mut merged: Vec<Suggestion> = Vec::new();
        for s in out {
            match merged.iter_mut().find(|m| m.xref == s.xref) {
                Some(m) => {
                    m.reason = format!("{} · {}", m.reason, s.reason);
                    m.score += 1.0;
                }
                None => merged.push(s),
            }
        }
        merged.sort_by(|a, b| b.score.total_cmp(&a.score));
        merged
    }
}

fn suggestion_label(i: &Individual) -> String {
    let years = i.years();
    if years.is_empty() { i.display_name() } else { format!("{} ({years})", i.display_name()) }
}
