//! Changes proposed by an AI agent for a Data Quality issue: apply the Quality page's fix, edit the
//! records the issue is about, or hide the issue with the reason it is right. Without an issue, a
//! change is a correction the agent read in a document (a given name misread, a date, a sex): the
//! same typed edits, on any record, with the reason and the evidence the user checks.
//!
//! Like an act proposal ([`crate::proposal`]), a change is only a description: the app shows it
//! in To Review and applies it when the user saves it. Before it is queued, [`QualityChange::check`]
//! applies it to a copy of the file and checks the issue is gone. A change that does not fix its
//! issue, or that edits records the issue is not about, is refused with the reason, which the
//! agent reads to correct it. Edits are typed (never raw GEDCOM) and each kind of issue allows only
//! some of them ([`edits_for`]). A possible duplicate is merged (`Action::Merge`), with the value
//! to keep for each contradictory fact, which the user reviews in the merge window.

use crate::act::quality_label;
use crate::analysis::check::{IGNORE_TAG, Issue, code_label};
use crate::date::Date;
use crate::document::Gedcom;
use crate::geo::{format_latitude, format_longitude};
use crate::merge::{MergePlan, RowKind, Side};
use crate::model::{NameForm, short_place};
use crate::node::Node;
use crate::parse::node_to_string;
use crate::place_merge::{PlacePlan, RowKind as PlaceRow};
use crate::proposal::{Change, DiffLine, line_diff};
use crate::quality::{self, Category, issue_ids};
use crate::relations::Relation;
use std::collections::HashMap;

/// A typed edit of the records an issue is about.
#[derive(Debug, Clone, PartialEq)]
pub enum Edit {
    /// New value (an occupation, a residence's description, a custom event's type), date and/or
    /// place of an event (position `index` in the record).
    SetEvent {
        record: String,
        index: usize,
        value: Option<String>,
        date: Option<String>,
        place: Option<String>,
        /// Street address within the place (`ADDR`); empty to remove it.
        address: Option<String>,
        /// Cause (`CAUS`: of a death, a divorce…); empty to remove it.
        cause: Option<String>,
    },
    /// Age of a person at an event (`AGE`, GEDCOM form `42y`, `6m`, `3d`): on a person's event,
    /// theirs; on a family event, the age of `spouse` (one of the couple). Empty to remove it.
    SetAge {
        record: String,
        index: usize,
        spouse: Option<String>,
        age: String,
    },
    /// A new event or attribute of a person or family (`OCCU` with its value, `RESI`, `BIRT`…).
    AddEvent {
        record: String,
        tag: String,
        value: String,
        date: String,
        place: String,
        address: String,
    },
    /// A place renamed everywhere it is used (spelling, jurisdictions), its coordinates kept.
    RenamePlace {
        place: String,
        name: String,
    },
    DeleteEvent {
        record: String,
        index: usize,
    },
    /// `M`, `F`, `X` or `U`.
    SetSex {
        person: String,
        sex: String,
    },
    /// Coordinates of a place, in decimal degrees.
    SetCoordinates {
        place: String,
        latitude: f64,
        longitude: f64,
        /// Identifier in the gazetteer (`EXID` value and `TYPE` URI).
        exid: Option<(String, String)>,
    },
    /// The repository of a source (`REPO`, with its call number `CALN`): an existing one, or one
    /// to create. A repository of the tree with the same name is used instead of a new one.
    SetRepository {
        source: String,
        repository: Option<String>,
        new: Option<crate::proposal::NewRepository>,
        call_number: Option<String>,
    },
    /// New path of the media file the issue designates.
    SetMediaFile {
        file: String,
    },
    /// Cites an existing source, with the reference in it, on an event.
    CiteSource {
        record: String,
        index: usize,
        source: String,
        page: String,
        /// Quality of the evidence (`QUAY` 0 to 3).
        quality: Option<u8>,
    },
    /// New value of the structure the issue designates.
    SetValue {
        value: String,
    },
    /// Removes the structure the issue designates.
    DeleteStructure,
    /// New given name and/or surname of a person's `NAME` (position `index` among their names, 0
    /// being the main one). The rest of the name (type, particle, sources…) is kept.
    SetName {
        person: String,
        index: usize,
        given: Option<String>,
        surname: Option<String>,
    },
    /// Another name of a person (`NAME`, with its `TYPE` when given: `AKA`, `BIRTH`, `MARRIED`…),
    /// after their other names.
    AddName {
        person: String,
        given: String,
        surname: String,
        kind: String,
    },
    /// Links two people of the tree: `other` becomes the `relation` of `person`, in the family
    /// that already exists (the other parent's, the union) or a new one.
    LinkRelative {
        person: String,
        relation: Relation,
        other: String,
    },
    /// Removes a wrong link: `other` is no longer the `relation` of `person`. The family is
    /// deleted when it no longer connects anyone; the people stay.
    UnlinkRelative {
        person: String,
        relation: Relation,
        other: String,
    },
    /// How a child belongs to their parents' family (`FAMC.PEDI`: `BIRTH`, `ADOPTED`, `FOSTER`,
    /// `SEALING`, `OTHER`). `parent`: one of the parents, to pick the family when there are several.
    SetPedigree {
        person: String,
        parent: Option<String>,
        pedigree: String,
    },
    /// A note on a record, or on one of its events (`event`: its position).
    AddNote {
        record: String,
        event: Option<usize>,
        text: String,
    },
    /// New text of a note (`note`: its position among the notes of the record or of the event);
    /// empty to remove it.
    SetNote {
        record: String,
        event: Option<usize>,
        note: usize,
        text: String,
    },
    /// A person who took part in an event without being its principal (`ASSO` / `ROLE`): witness,
    /// godparent, officiant… `phrase` says more (“uncle of the bride”); required for `OTHER`.
    AddAssociate {
        record: String,
        index: usize,
        person: String,
        role: String,
        phrase: String,
    },
    /// Removes a person associated with an event.
    RemoveAssociate {
        record: String,
        index: usize,
        person: String,
    },
    /// Links a media to a record or to one of its events: an existing one (`media`), or the
    /// media of a file of the tree's folder (`file`), created if needed.
    AttachMedia {
        record: String,
        event: Option<usize>,
        media: Option<String>,
        file: Option<String>,
    },
    /// Unlinks a media from a record or one of its events; the media stays in the file.
    DetachMedia {
        record: String,
        event: Option<usize>,
        media: String,
    },
}

/// GEDCOM 7 roles of a person associated with an event (`ASSO.ROLE`).
pub const ASSOCIATE_ROLES: &[&str] = &["WITN", "GODP", "OFFICIATOR", "CLERGY", "NGHBR", "FRIEND", "PARENT", "OTHER"];

/// GEDCOM 7 values of `FAMC.PEDI`.
pub const PEDIGREES: &[&str] = &["BIRTH", "ADOPTED", "FOSTER", "SEALING", "OTHER"];

impl Edit {
    /// Name of the edit, as the agent gives it.
    pub fn name(&self) -> &'static str {
        match self {
            Edit::SetEvent { .. } => "set_event",
            Edit::AddEvent { .. } => "add_event",
            Edit::RenamePlace { .. } => "rename_place",
            Edit::DeleteEvent { .. } => "delete_event",
            Edit::SetSex { .. } => "set_sex",
            Edit::SetCoordinates { .. } => "set_coordinates",
            Edit::SetRepository { .. } => "set_repository",
            Edit::SetMediaFile { .. } => "set_media_file",
            Edit::CiteSource { .. } => "cite_source",
            Edit::SetValue { .. } => "set_value",
            Edit::DeleteStructure => "delete_structure",
            Edit::SetName { .. } => "set_name",
            Edit::AddName { .. } => "add_name",
            Edit::SetAge { .. } => "set_age",
            Edit::LinkRelative { .. } => "link_relative",
            Edit::UnlinkRelative { .. } => "unlink_relative",
            Edit::SetPedigree { .. } => "set_pedigree",
            Edit::AddNote { .. } => "add_note",
            Edit::SetNote { .. } => "set_note",
            Edit::AddAssociate { .. } => "add_associate",
            Edit::RemoveAssociate { .. } => "remove_associate",
            Edit::AttachMedia { .. } => "attach_media",
            Edit::DetachMedia { .. } => "detach_media",
        }
    }

    /// The record the edit changes, when it names one.
    pub fn record(&self) -> Option<&str> {
        match self {
            Edit::SetEvent { record, .. }
            | Edit::AddEvent { record, .. }
            | Edit::DeleteEvent { record, .. }
            | Edit::CiteSource { record, .. }
            | Edit::SetAge { record, .. }
            | Edit::AddNote { record, .. }
            | Edit::SetNote { record, .. }
            | Edit::AddAssociate { record, .. }
            | Edit::RemoveAssociate { record, .. }
            | Edit::AttachMedia { record, .. }
            | Edit::DetachMedia { record, .. } => Some(record),
            Edit::SetRepository { source, .. } => Some(source),
            Edit::SetSex { person, .. }
            | Edit::SetName { person, .. }
            | Edit::AddName { person, .. }
            | Edit::LinkRelative { person, .. }
            | Edit::UnlinkRelative { person, .. }
            | Edit::SetPedigree { person, .. } => Some(person),
            _ => None,
        }
    }
}

/// What the change does.
#[derive(Debug, Clone, PartialEq)]
pub enum Action {
    /// The Quality page's own fix.
    Fix,
    /// The issue is right: hidden (`_GR_IGNORE`), with the reason.
    Hide,
    Edit(Vec<Edit>),
    /// Two possible duplicates become one person. For each contradictory fact (`NAME`, `BIRT`…),
    /// the person whose value is kept. For two possible duplicate places: for each level that
    /// differs (its label, or `MAP` for the coordinates), the place whose value is kept.
    Merge(Vec<(String, String)>),
}

/// What supports a change: a source of the tree and the reference in it, a web page, a quote.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Evidence {
    pub source: Option<String>,
    pub page: String,
    pub url: String,
    pub quote: String,
}

impl Evidence {
    pub fn is_empty(&self) -> bool {
        self.source.is_none()
            && self.page.trim().is_empty()
            && self.url.trim().is_empty()
            && self.quote.trim().is_empty()
    }
}

/// A change proposed for an issue, or a correction without one.
#[derive(Debug, Clone, PartialEq)]
pub struct QualityChange {
    /// The issue, as it was when the change was proposed, and its identifier. `None`: a
    /// correction the agent read in a document.
    pub issue: Option<Issue>,
    pub issue_id: Option<String>,
    pub action: Action,
    pub reason: String,
    pub evidence: Evidence,
}

/// Line of the comparison: what the tree has, what the change makes of it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChangeRow {
    pub label: String,
    pub current: String,
    pub proposed: String,
    pub change: Change,
}

/// Edits a correction without an issue may make: those that name their record.
pub const CORRECTIONS: &[&str] = &[
    "set_name",
    "add_name",
    "set_event",
    "add_event",
    "delete_event",
    "set_sex",
    "cite_source",
    "set_coordinates",
    "rename_place",
    "set_repository",
    "set_age",
    "link_relative",
    "unlink_relative",
    "set_pedigree",
    "add_note",
    "set_note",
    "add_associate",
    "remove_associate",
    "attach_media",
    "detach_media",
];

/// Edits each kind of issue allows. Changing a filiation (own ancestor) or a person's links stays
/// with the user.
pub fn edits_for(code: &str) -> &'static [&'static str] {
    match code {
        "husband_not_male" | "wife_not_female" | "role_sex_mismatch" => &["set_sex"],
        // A possible duplicate is merged (`Action::Merge`) or marked as two people (`Action::Hide`).
        "own_ancestor" | "disconnected" | "possible_duplicate" | "possible_duplicate_place" => &[],
        "tag_not_allowed" | "missing_required" | "too_many" | "wrong_pointer" | "invalid_value" => {
            &["set_value", "delete_structure"]
        }
        "unsourced" => &["cite_source"],
        "missing_coords" | "bad_coords" => &["set_coordinates", "rename_place"],
        "place_format" => &["rename_place", "set_coordinates"],
        "missing_file" => &["set_media_file"],
        code if quality::category(code) == Category::Dates => &["set_event", "delete_event"],
        _ => &[],
    }
}

/// Can the issue be hidden? An issue about a record (`_GR_IGNORE` is written under it), or two
/// places that are not the same (`_GR_DISTINCT` in the header).
pub fn can_hide(g: &Gedcom, issue: &Issue) -> bool {
    g.get(&issue.xref).is_some() || issue.code == "possible_duplicate_place"
}

/// The issue with this identifier ([`quality::issue_ids`]), among `issues`.
pub fn find_issue<'a>(issues: &'a [Issue], id: &str) -> Option<&'a Issue> {
    issue_ids(issues).iter().position(|x| x == id).map(|i| &issues[i])
}

/// Key under which issues are counted to know whether a change resolved its issue: positions in
/// the record move when a structure is removed.
fn count_key(i: &Issue) -> (&'static str, String, String) {
    (i.code, i.xref.clone(), i.related.join("|"))
}

fn counts(issues: &[Issue]) -> HashMap<(&'static str, String, String), usize> {
    let mut out = HashMap::new();
    for i in issues.iter().filter(|i| !i.ignored) {
        *out.entry(count_key(i)).or_default() += 1;
    }
    out
}

impl Gedcom {
    /// Hides an issue like [`Gedcom::ignore_issue`], keeping the reason (`2 NOTE`) when there is
    /// one.
    pub fn ignore_issue_because(&mut self, xref: &str, code: &str, reason: &str) -> bool {
        if !self.ignore_issue(xref, code) {
            return false;
        }
        let reason = reason.trim();
        if let Some(n) =
            self.get_mut(xref).and_then(|r| r.children.iter_mut().find(|c| c.tag == IGNORE_TAG && c.val() == code))
            && !reason.is_empty()
        {
            let mut note = Node::new("NOTE");
            note.set_text(reason);
            n.children.push(note);
        }
        true
    }
}

impl QualityChange {
    /// Records the change may edit: the issue's record and the records involved; any record for a
    /// correction.
    fn allowed(&self, xref: &str) -> bool {
        match &self.issue {
            Some(i) => i.xref == xref || i.related.iter().any(|r| r == xref),
            None => true,
        }
    }

    /// The issue, for the actions that need one.
    fn the_issue(&self) -> Result<&Issue, String> {
        self.issue.as_ref().ok_or_else(|| {
            "without an issue, a change can only `edit`: `fix`, `hide` and `merge` are for an issue of `quality_issues`"
                .to_string()
        })
    }

    /// Checks a correction without an issue: edits that name their record, a reason and evidence.
    fn validate_correction(&self, g: &Gedcom) -> Result<(), String> {
        let Action::Edit(edits) = &self.action else { return self.the_issue().map(|_| ()) };
        if edits.is_empty() {
            return Err("give at least one edit".into());
        }
        if self.reason.trim().is_empty() {
            return Err("give the reason for the correction in `reason`: what you read".into());
        }
        if self.evidence.is_empty() {
            return Err(
                "a correction without an issue needs `evidence`: the source of the tree, the viewer page or a quote of the document"
                    .into(),
            );
        }
        for e in edits {
            if !CORRECTIONS.contains(&e.name()) {
                return Err(format!(
                    "`{}` needs an issue of `quality_issues`; without one, use {}",
                    e.name(),
                    CORRECTIONS.join(", ")
                ));
            }
            if let Some(x) = e.record()
                && g.get(x).is_none()
            {
                return Err(format!("no record {x} in the tree"));
            }
            self.validate_edit(g, e)?;
        }
        Ok(())
    }

    /// Checks what an edit needs, whatever the issue.
    fn validate_edit(&self, g: &Gedcom, e: &Edit) -> Result<(), String> {
        match e {
            Edit::SetCoordinates { latitude, longitude, .. }
                if !(-90.0..=90.0).contains(latitude) || !(-180.0..=180.0).contains(longitude) =>
            {
                Err("latitude must be within ±90 and longitude within ±180 degrees".into())
            }
            Edit::CiteSource { source, .. } if g.get(source).is_none_or(|n| n.tag != "SOUR") => Err(format!(
                "no source {source} in the tree: cite an existing one (`search_sources`), or propose the act with `propose_act`"
            )),
            Edit::SetRepository { source, repository, new, .. } => {
                if g.get(source).is_none_or(|n| n.tag != "SOUR") {
                    return Err(format!("{source} is not a source of the tree"));
                }
                match (repository, new) {
                    (Some(r), _) if g.get(r).is_none_or(|n| n.tag != "REPO") => {
                        Err(format!("no repository {r} in the tree (`search_sources` lists them)"))
                    }
                    (Some(_), _) => Ok(()),
                    (None, Some(n)) if n.name.trim().is_empty() => Err("the new repository needs its `name`".into()),
                    (None, Some(_)) => Ok(()),
                    (None, None) => Err("`set_repository` needs `repository_id` or a new `repository`".into()),
                }
            }
            Edit::RenamePlace { place, name } => {
                if name.trim().is_empty() {
                    return Err("`rename_place` needs the new `name`".into());
                }
                if !g.places().iter().any(|p| &p.name == place) {
                    return Err(format!("no place `{place}` in the tree: give it as `person_sheet` shows it"));
                }
                Ok(())
            }
            Edit::AddEvent { tag, .. } if crate::model::event_label(&tag.trim().to_uppercase()).is_none() => {
                Err(format!("`{tag}` is not an event or attribute GEDCOM knows (`OCCU`, `RESI`, `BIRT`…)"))
            }
            Edit::SetName { person, index, given, surname } => {
                let r = g.get(person).filter(|n| n.tag == "INDI").ok_or_else(|| format!("no person {person}"))?;
                let names = r.children_with("NAME").count();
                if *index >= names.max(1) {
                    return Err(format!("{person} has {names} name(s): `name` is 0 for the main one"));
                }
                if given.is_none() && surname.is_none() {
                    return Err("`set_name` needs `given` and/or `surname`".into());
                }
                Ok(())
            }
            Edit::AddName { person, given, surname, kind } => {
                g.get(person).filter(|n| n.tag == "INDI").ok_or_else(|| format!("no person {person}"))?;
                if given.trim().is_empty() && surname.trim().is_empty() {
                    return Err("`add_name` needs `given` and/or `surname`".into());
                }
                let kind = kind.trim().to_uppercase();
                if !kind.is_empty() && !crate::model::NAME_TYPES.contains(&kind.as_str()) {
                    return Err(format!("`name_type` is one of {}", crate::model::NAME_TYPES.join(", ")));
                }
                Ok(())
            }
            Edit::SetAge { record, index, spouse, age } => {
                event_ref(g, record, *index)?;
                if !age.trim().is_empty() && age_value(age).is_none() {
                    return Err(format!("`{age}` is not an age: give it as `42y`, `6m` or `3d`"));
                }
                if let Some(f) = g.family(record) {
                    let s = spouse.as_deref().ok_or("on a family event, give `spouse`: whose age it is")?;
                    if f.husband() != Some(s) && f.wife() != Some(s) {
                        return Err(format!("{s} is not one of the couple of {record}"));
                    }
                }
                Ok(())
            }
            Edit::LinkRelative { person, other, .. } => {
                individual(g, person)?;
                individual(g, other)?;
                Ok(())
            }
            Edit::UnlinkRelative { person, relation, other } => {
                individual(g, person)?;
                individual(g, other)?;
                link_family(g, person, *relation, other).map(|_| ()).ok_or_else(|| {
                    format!("{other} is not the {} of {person} in the tree (`person_sheet`)", relation_name(*relation))
                })
            }
            Edit::SetPedigree { person, parent, pedigree } => {
                individual(g, person)?;
                if !PEDIGREES.contains(&pedigree.trim().to_uppercase().as_str()) {
                    return Err(format!("`pedigree` is one of {}", PEDIGREES.join(", ")));
                }
                parents_family(g, person, parent.as_deref()).map(|_| ())
            }
            Edit::AddNote { record, event, text } => {
                if text.trim().is_empty() {
                    return Err("`add_note` needs the note's `text`".into());
                }
                note_holder(g, record, *event).map(|_| ())
            }
            Edit::SetNote { record, event, note, .. } => {
                let holder = note_holder(g, record, *event)?;
                let n = holder.children_with("NOTE").nth(*note).ok_or_else(|| {
                    format!("{record} has {} note(s) there: `note` counts from 0", holder.children_with("NOTE").count())
                })?;
                if n.pointer().is_some() {
                    return Err("this note is shared between records: the user changes it in GEDRoots".into());
                }
                Ok(())
            }
            Edit::AddAssociate { record, index, person, role, phrase } => {
                event_ref(g, record, *index)?;
                individual(g, person)?;
                let role = role.trim().to_uppercase();
                if !ASSOCIATE_ROLES.contains(&role.as_str()) {
                    return Err(format!("`role` is one of {}", ASSOCIATE_ROLES.join(", ")));
                }
                if role == "OTHER" && phrase.trim().is_empty() {
                    return Err(
                        "for the role `OTHER`, say which in `phrase` (“neighbour”, “uncle of the bride”)".into()
                    );
                }
                Ok(())
            }
            Edit::RemoveAssociate { record, index, person } => {
                let ev = event_ref(g, record, *index)?;
                if !ev.children_with("ASSO").any(|a| a.pointer() == Some(person.as_str())) {
                    return Err(format!("{person} is not associated with this event"));
                }
                Ok(())
            }
            Edit::AttachMedia { record, event, media, file } => {
                match (media, file) {
                    (Some(m), None) if g.get(m).is_none_or(|n| n.tag != "OBJE") => {
                        return Err(format!("no media {m} in the tree"));
                    }
                    (Some(_), None) => {}
                    (None, Some(f)) if f.trim().is_empty() => return Err("give the file's path".into()),
                    (None, Some(_)) => {}
                    _ => return Err("`attach_media` needs `media` (an existing one) or `file`, not both".into()),
                }
                match event {
                    Some(i) => event_ref(g, record, *i).map(|_| ()),
                    None if g.get(record).is_some_and(|n| matches!(n.tag.as_str(), "INDI" | "FAM" | "SOUR")) => Ok(()),
                    None => Err(format!("{record} cannot hold a media: give a person, a family or a source")),
                }
            }
            Edit::DetachMedia { record, event, media } => {
                let holder = match event {
                    Some(i) => event_ref(g, record, *i)?,
                    None => g.get(record).ok_or_else(|| format!("no record {record}"))?,
                };
                if !holder.children_with("OBJE").any(|o| o.pointer() == Some(media.as_str())) {
                    return Err(format!("{media} is not linked there"));
                }
                Ok(())
            }
            _ => Ok(()),
        }
    }

    /// Checks the change on its own: allowed for this kind of issue, with what it needs.
    fn validate(&self, g: &Gedcom) -> Result<(), String> {
        let Some(issue) = &self.issue else { return self.validate_correction(g) };
        let code = issue.code;
        match &self.action {
            Action::Fix => {
                // A possible duplicate's “fix” is the comparison window: merged or hidden instead.
                if quality::fix_of(code).is_none() || matches!(code, "possible_duplicate" | "possible_duplicate_place")
                {
                    return Err(format!(
                        "`{code}` has no automatic fix: propose edits ({}) or hide it with a reason",
                        allowed_list(code)
                    ));
                }
            }
            Action::Hide => {
                if !can_hide(g, issue) {
                    return Err("this issue is about places or files, not a record: it cannot be hidden".into());
                }
                if self.reason.trim().is_empty() {
                    return Err("give the reason the issue is right in `reason`".into());
                }
            }
            Action::Merge(_) => {
                if !matches!(code, "possible_duplicate" | "possible_duplicate_place") {
                    return Err(format!(
                        "only a `possible_duplicate` or a `possible_duplicate_place` can be merged, not `{code}`"
                    ));
                }
                if self.reason.trim().is_empty() {
                    return Err("give the reason they are the same in `reason`".into());
                }
                if code == "possible_duplicate" {
                    self.merge_choices(g)?;
                } else {
                    self.place_merge_choices(g)?;
                }
            }
            Action::Edit(edits) => {
                if edits.is_empty() {
                    return Err("give at least one edit".into());
                }
                if self.reason.trim().is_empty() {
                    return Err("give the reason for the change in `reason`".into());
                }
                let allowed = edits_for(code);
                for e in edits {
                    if !allowed.contains(&e.name()) {
                        return Err(format!("`{}` is not allowed for `{code}`: use {}", e.name(), allowed_list(code)));
                    }
                    if let Some(x) = e.record()
                        && !self.allowed(x)
                    {
                        return Err(format!("{x} is not one of the records this issue is about"));
                    }
                    if let Edit::SetCoordinates { place, .. } = e
                        && !issue.related.iter().any(|p| p == place)
                    {
                        return Err(format!("`{place}` is not the place of this issue"));
                    }
                    self.validate_edit(g, e)?;
                }
            }
        }
        if let Some(s) = &self.evidence.source
            && g.get(s).is_none_or(|n| n.tag != "SOUR")
        {
            return Err(format!("the evidence cites {s}, which is not a source of the tree"));
        }
        Ok(())
    }

    /// Applies the change. Fails if the file no longer fits it.
    pub fn apply(&self, g: &mut Gedcom) -> Result<(), String> {
        self.validate(g)?;
        match &self.action {
            Action::Fix => {
                if !g.fix_issue(self.the_issue()?) {
                    return Err(t!("change.outdated").into());
                }
            }
            // Two people who are not the same: recorded as distinct, never hidden by code.
            Action::Hide if self.issue.as_ref().is_some_and(|i| i.code == "possible_duplicate") => {
                let i = self.the_issue()?;
                let other = i.related.first().ok_or_else(|| t!("change.outdated").to_string())?;
                if !g.mark_distinct(&i.xref, other) {
                    return Err(t!("change.outdated").into());
                }
            }
            // Two places that are not the same: recorded in the header.
            Action::Hide if self.place_pair().is_some() => {
                let (a, b) = self.place_pair().unwrap_or_default();
                if !g.mark_places_distinct(&a, &b) {
                    return Err(t!("change.outdated").into());
                }
            }
            Action::Hide => {
                let i = self.the_issue()?;
                if !g.ignore_issue_because(&i.xref, i.code, &self.reason) {
                    return Err(t!("change.outdated").into());
                }
            }
            Action::Merge(_) if self.place_pair().is_some() => {
                let (plan, sides) = self.place_merge_choices(g)?;
                g.merge_places(&plan, &sides).ok_or_else(|| t!("change.outdated").to_string())?;
            }
            Action::Merge(_) => {
                let (plan, sides) = self.merge_choices(g)?;
                g.merge_individuals(&plan, &sides)?;
            }
            Action::Edit(edits) => {
                // Changes in place first; then removals, from the end, and what moves the
                // record's structures (a name goes before the events): the positions given stay
                // right. What is added goes at the end of the record.
                let mut order: Vec<&Edit> = edits.iter().collect();
                order.sort_by_key(|e| match e {
                    Edit::DeleteEvent { index, .. } => (1, usize::MAX - index),
                    Edit::SetNote { note, text, .. } if text.trim().is_empty() => (2, usize::MAX - note),
                    Edit::UnlinkRelative { .. } | Edit::DetachMedia { event: None, .. } => (2, 0),
                    Edit::AddName { .. } | Edit::LinkRelative { .. } => (3, 0),
                    Edit::DeleteStructure => (4, 0),
                    _ => (0, 0),
                });
                for e in order {
                    self.apply_edit(g, e)?;
                }
                let touched: Vec<String> = match &self.issue {
                    Some(i) => std::iter::once(&i.xref).chain(&i.related).cloned().collect(),
                    None => edits.iter().filter_map(|e| e.record().map(str::to_string)).collect(),
                };
                for x in touched {
                    if let Some(r) = g.get_mut(&x) {
                        r.touch();
                    }
                }
            }
        }
        Ok(())
    }

    /// The two people of a possible duplicate, if they can still be merged.
    pub fn merge_pair(&self) -> Option<(String, String)> {
        let i = self.issue.as_ref()?;
        (i.code == "possible_duplicate").then(|| Some((i.xref.clone(), i.related.first()?.clone()))).flatten()
    }

    /// The two places of a possible duplicate place.
    pub fn place_pair(&self) -> Option<(String, String)> {
        let i = self.issue.as_ref()?;
        match (i.code, &i.related[..]) {
            ("possible_duplicate_place", [a, b]) => Some((a.clone(), b.clone())),
            _ => None,
        }
    }

    /// The comparison of two places and, for each choice it asks for, the place the agent chose.
    pub fn place_merge_choices(&self, g: &Gedcom) -> Result<(PlacePlan, Vec<Side>), String> {
        let Action::Merge(choices) = &self.action else { return Err("not a merge".into()) };
        let (a, b) = self.place_pair().ok_or_else(|| t!("change.outdated").to_string())?;
        let plan = g.place_merge_plan(&a, &b).ok_or_else(|| t!("change.outdated").to_string())?;
        let conflicts = plan.choices();
        let list = || {
            if conflicts.is_empty() {
                "nothing: every level agrees, give empty `choices`".to_string()
            } else {
                conflicts.iter().map(|c| format!("`{}`", c.fact)).collect::<Vec<_>>().join(", ")
            }
        };
        for (fact, _) in choices {
            if !conflicts.iter().any(|c| c.is(fact)) {
                return Err(format!("`{fact}` does not differ between the two places: choose only for {}", list()));
            }
        }
        let sides = conflicts
            .iter()
            .map(|c| {
                let which = choices
                    .iter()
                    .find(|(fact, _)| c.is(fact))
                    .map(|(_, which)| which.trim())
                    .ok_or_else(|| format!("choose which place's `{}` to keep in `choices`", c.fact))?;
                if which == plan.a.trim() {
                    Ok(Side::Keep)
                } else if which == plan.b.trim() {
                    Ok(Side::Other)
                } else {
                    Err(format!("`{}`: give one of the two places, `{}` or `{}`", c.fact, plan.a, plan.b))
                }
            })
            .collect::<Result<Vec<_>, String>>()?;
        Ok((plan, sides))
    }

    /// The merge plan and, for each of its conflicts, the side the agent chose.
    pub fn merge_choices(&self, g: &Gedcom) -> Result<(MergePlan, Vec<Side>), String> {
        let Action::Merge(choices) = &self.action else { return Err("not a merge".into()) };
        let (a, b) = self.merge_pair().ok_or_else(|| t!("change.outdated").to_string())?;
        if let Err(e) = g.compare_individuals(&a, &b) {
            return Err(format!("these two people cannot be the same: {}", e.label()));
        }
        let plan = g.merge_plan(&a, &b)?;
        for (tag, _) in choices {
            if !plan.conflicts.iter().any(|c| c.tag.eq_ignore_ascii_case(tag)) {
                return Err(format!("`{tag}` does not differ between them: choose only for {}", conflict_list(&plan)));
            }
        }
        let sides = plan
            .conflicts
            .iter()
            .map(|c| {
                let who = choices
                    .iter()
                    .find(|(tag, _)| tag.eq_ignore_ascii_case(&c.tag))
                    .map(|(_, who)| who.trim().trim_matches('@'))
                    .ok_or_else(|| {
                        format!("choose whose `{}` to keep ({} or {}) in `choices`", c.tag, plan.keep, plan.other)
                    })?;
                if who == plan.keep {
                    Ok(Side::Keep)
                } else if who == plan.other {
                    Ok(Side::Other)
                } else {
                    Err(format!("`{}`: give {} or {}, not {who}", c.tag, plan.keep, plan.other))
                }
            })
            .collect::<Result<Vec<_>, String>>()?;
        Ok((plan, sides))
    }

    fn apply_edit(&self, g: &mut Gedcom, e: &Edit) -> Result<(), String> {
        let outdated = || -> String { t!("change.outdated").into() };
        match e {
            Edit::SetEvent { record, index, value, date, place, address, cause } => {
                let mut ev = g.read_event(record, *index).ok_or_else(|| format!("no event {index} in {record}"))?;
                if let Some(v) = value {
                    ev.detail = v.trim().to_string();
                }
                if let Some(d) = date {
                    ev.date = gedcom_date(d)?;
                    ev.date_phrase.clear();
                }
                if let Some(p) = place {
                    ev.place = g.known_place(p);
                }
                if let Some(a) = address {
                    ev.address = a.trim().to_string();
                }
                g.update_event(record, *index, &ev)?;
                if let Some(c) = cause {
                    event_mut(g, record, *index)?.set_child_val("CAUS", c);
                }
                Ok(())
            }
            Edit::SetAge { record, index, spouse, age } => {
                let age = age_value(age).unwrap_or_default();
                let tag = g.family(record).map(|f| if f.husband() == spouse.as_deref() { "HUSB" } else { "WIFE" });
                let ev = event_mut(g, record, *index)?;
                match tag {
                    None => ev.set_child_val("AGE", &age),
                    Some(t) => {
                        if ev.child(t).is_none() {
                            // Before the citations, as an act writes it.
                            let at = ev.children.iter().position(|c| c.tag == "SOUR").unwrap_or(ev.children.len());
                            ev.children.insert(at, Node::new(t));
                        }
                        let mut empty = false;
                        if let Some(h) = ev.child_mut(t) {
                            h.set_child_val("AGE", &age);
                            empty = h.children.is_empty();
                        }
                        if empty {
                            ev.children.retain(|c| c.tag != t);
                        }
                    }
                }
                Ok(())
            }
            Edit::AddEvent { record, tag, value, date, place, address } => {
                let ev = crate::NewEvent {
                    tag: tag.trim().to_uppercase(),
                    detail: value.trim().to_string(),
                    date: gedcom_date(date)?,
                    place: if place.trim().is_empty() { String::new() } else { g.known_place(place) },
                    address: address.trim().to_string(),
                    ..Default::default()
                };
                g.add_event(record, &ev)
            }
            Edit::RenamePlace { place, name } => {
                let old = g.places().into_iter().find(|p| &p.name == place).ok_or_else(outdated)?;
                let (lat, long) = (old.latitude.unwrap_or_default(), old.longitude.unwrap_or_default());
                if g.update_place(place, name, &lat, &long) == 0 { Err(outdated()) } else { Ok(()) }
            }
            Edit::DeleteEvent { record, index } => g.delete_event(record, *index).map(|_| ()),
            Edit::SetSex { person, sex } => {
                let sex = sex.trim().to_uppercase();
                if !matches!(sex.as_str(), "M" | "F" | "X" | "U") {
                    return Err("sex is `M`, `F`, `X` or `U`".into());
                }
                let r = g.get_mut(person).filter(|n| n.tag == "INDI").ok_or_else(|| format!("no person {person}"))?;
                r.set_child_val("SEX", &sex);
                Ok(())
            }
            Edit::SetCoordinates { place, latitude, longitude, exid } => {
                let n =
                    g.locate_place(place, &format_latitude(*latitude), &format_longitude(*longitude), exid.as_ref());
                if n == 0 { Err(outdated()) } else { Ok(()) }
            }
            Edit::SetRepository { source, repository, new, call_number } => {
                let repo = match (repository, new) {
                    (Some(r), _) => r.clone(),
                    (None, Some(new)) => g.create_repository(new)?,
                    (None, None) => return Err("`set_repository` needs `repository_id` or a new `repository`".into()),
                };
                let node = g.get_mut(source).filter(|n| n.tag == "SOUR").ok_or_else(outdated)?;
                let caln = call_number
                    .clone()
                    .or_else(|| node.child("REPO").and_then(|r| r.child_val("CALN")).map(str::to_string))
                    .unwrap_or_default();
                node.children.retain(|c| c.tag != "REPO");
                let mut r = Node::with_value("REPO", format!("@{repo}@"));
                if !caln.trim().is_empty() {
                    r = r.push(Node::with_value("CALN", caln.trim()));
                }
                node.insert_child(r);
                Ok(())
            }
            Edit::SetMediaFile { file } => {
                if file.trim().is_empty() {
                    return Err("give the file's path, relative to the tree's folder".into());
                }
                let i = self.the_issue()?;
                if g.set_media_file(&i.xref, &i.path, file.trim()) { Ok(()) } else { Err(outdated()) }
            }
            Edit::CiteSource { record, index, source, page, quality } => {
                let node = g
                    .get_mut(record)
                    .and_then(|n| n.children.get_mut(*index))
                    .filter(|c| crate::model::event_label(&c.tag).is_some())
                    .ok_or_else(|| format!("no event {index} in {record}"))?;
                let mut cit = Node::with_value("SOUR", format!("@{}@", source.trim_matches('@')));
                if !page.trim().is_empty() {
                    cit.children.push(Node::with_value("PAGE", page.trim()));
                }
                crate::act::set_quality(&mut cit, *quality);
                node.insert_child(cit);
                Ok(())
            }
            Edit::SetValue { value } => {
                let i = self.the_issue()?;
                let n = node_at(g, &i.xref, &i.path).ok_or_else(outdated)?;
                n.value = (!value.trim().is_empty()).then(|| value.trim().to_string());
                Ok(())
            }
            Edit::DeleteStructure => {
                let i = self.the_issue()?;
                let (last, parents) =
                    i.path.split_last().ok_or_else(|| "this issue is about a whole record".to_string())?;
                let parent = node_at(g, &i.xref, parents).ok_or_else(outdated)?;
                if *last >= parent.children.len() {
                    return Err(outdated());
                }
                parent.children.remove(*last);
                Ok(())
            }
            Edit::SetName { person, index, given, surname } => {
                let r = g.get_mut(person).filter(|n| n.tag == "INDI").ok_or_else(|| format!("no person {person}"))?;
                if r.child("NAME").is_none() {
                    r.insert_child(Node::new("NAME"));
                }
                let node = r.children.iter_mut().filter(|c| c.tag == "NAME").nth(*index).ok_or_else(outdated)?;
                let mut form = NameForm::read(node);
                if let Some(v) = given {
                    form.given = v.trim().to_string();
                }
                if let Some(v) = surname {
                    form.surname = v.trim().to_string();
                }
                form.apply(node);
                Ok(())
            }
            Edit::AddName { person, given, surname, kind } => {
                let r = g.get_mut(person).filter(|n| n.tag == "INDI").ok_or_else(|| format!("no person {person}"))?;
                let form = NameForm {
                    given: given.trim().to_string(),
                    surname: surname.trim().to_string(),
                    kind: kind.trim().to_uppercase(),
                    ..Default::default()
                };
                let mut node = Node::new("NAME");
                form.apply(&mut node);
                // After the last name, before the rest of the record.
                let at = r.children.iter().rposition(|c| c.tag == "NAME").map(|p| p + 1).unwrap_or(0);
                r.children.insert(at, node);
                Ok(())
            }
            Edit::LinkRelative { person, relation, other } => g.link_relative(person, *relation, other),
            Edit::UnlinkRelative { person, relation, other } => {
                let fam = link_family(g, person, *relation, other).ok_or_else(outdated)?;
                match relation {
                    // The couple stays: the child leaves it, with the other parent if any.
                    Relation::Father | Relation::Mother => detach_child(g, person, other, &fam),
                    Relation::Child => detach_child(g, other, person, &fam),
                    Relation::Spouse | Relation::Sibling => {
                        g.unlink_from_family(other, &fam);
                        Ok(())
                    }
                }
            }
            Edit::SetPedigree { person, parent, pedigree } => {
                let fam = parents_family(g, person, parent.as_deref())?;
                let p = format!("@{fam}@");
                let r = g.get_mut(person).ok_or_else(outdated)?;
                let link = r.children.iter_mut().find(|c| c.tag == "FAMC" && c.val() == p).ok_or_else(outdated)?;
                link.set_child_val("PEDI", &pedigree.trim().to_uppercase());
                Ok(())
            }
            Edit::AddNote { record, event, text } => {
                let mut n = Node::new("NOTE");
                n.set_text(text.trim());
                let holder = note_holder_mut(g, record, *event)?;
                // An event's note after its own details; a record's before CHAN.
                match event {
                    Some(_) => {
                        let at = holder
                            .children
                            .iter()
                            .rposition(|c| matches!(c.tag.as_str(), "NOTE" | "SNOTE"))
                            .map(|i| i + 1)
                            .unwrap_or(holder.children.len());
                        holder.children.insert(at, n);
                    }
                    None => holder.insert_child(n),
                }
                Ok(())
            }
            Edit::SetNote { record, event, note, text } => {
                let holder = note_holder_mut(g, record, *event)?;
                let at = holder
                    .children
                    .iter()
                    .enumerate()
                    .filter(|(_, c)| c.tag == "NOTE")
                    .nth(*note)
                    .map(|(i, _)| i)
                    .ok_or_else(outdated)?;
                if text.trim().is_empty() {
                    holder.children.remove(at);
                } else {
                    holder.children[at].set_text(text.trim());
                }
                Ok(())
            }
            Edit::AddAssociate { record, index, person, role, phrase } => {
                let mut r = Node::with_value("ROLE", role.trim().to_uppercase());
                if !phrase.trim().is_empty() {
                    r.children.push(Node::with_value("PHRASE", phrase.trim()));
                }
                let asso = Node::with_value("ASSO", format!("@{person}@")).push(r);
                let ev = event_mut(g, record, *index)?;
                if !ev.children.contains(&asso) {
                    let at = ev.children.iter().position(|c| c.tag == "SOUR").unwrap_or(ev.children.len());
                    ev.children.insert(at, asso);
                }
                if ev.value.as_deref() == Some("Y") {
                    ev.value = None;
                }
                Ok(())
            }
            Edit::RemoveAssociate { record, index, person } => {
                let p = format!("@{person}@");
                let ev = event_mut(g, record, *index)?;
                ev.children.retain(|c| !(c.tag == "ASSO" && c.val() == p));
                if ev.children.is_empty() && crate::model::event_label(&ev.tag).is_some() && ev.value.is_none() {
                    ev.value = Some("Y".into());
                }
                Ok(())
            }
            Edit::AttachMedia { record, event, media, file } => {
                let media = match (media, file) {
                    (Some(m), _) => m.clone(),
                    (None, Some(f)) => g.media_for_file(f.trim()),
                    (None, None) => return Err(outdated()),
                };
                match event {
                    Some(i) => g.attach_media_to_event(record, *i, &media),
                    None => g.attach_media(record, &media),
                }
            }
            Edit::DetachMedia { record, event, media } => match event {
                Some(i) => g.detach_media_from_event(record, *i, media),
                None => {
                    g.detach_media(record, media);
                    Ok(())
                }
            },
        }
    }

    /// Checks the change before it is queued: it fits the issue, and once applied to a copy of the
    /// file (`rerun` gives the issues of a file, like the Quality page), the issue is gone. Returns
    /// the issues the change would create.
    /// A correction without an issue only has to apply.
    pub fn check(&self, g: &Gedcom, rerun: &dyn Fn(&Gedcom) -> Vec<Issue>) -> Result<Vec<Issue>, String> {
        let before = rerun(g);
        if let (Some(_), Some(id)) = (&self.issue, &self.issue_id)
            && find_issue(&before, id).is_none()
        {
            return Err(format!("no issue `{id}` any more: call `quality_issues` again"));
        }
        let mut copy = g.clone();
        self.apply(&mut copy)?;
        let after = rerun(&copy);
        let (b, a) = (counts(&before), counts(&after));
        if let Some(issue) = &self.issue {
            let key = count_key(issue);
            if a.get(&key).copied().unwrap_or(0) >= b.get(&key).copied().unwrap_or(0) {
                return Err(format!("after this change, GEDRoots still reports: {}", issue.message));
            }
        }
        // Issues that appear: more of their kind on their record than before.
        let mut seen: HashMap<_, usize> = HashMap::new();
        let mut new = Vec::new();
        for i in after.iter().filter(|i| !i.ignored) {
            let k = count_key(i);
            let n = seen.entry(k.clone()).or_default();
            *n += 1;
            if *n > b.get(&k).copied().unwrap_or(0) {
                new.push(i.clone());
            }
        }
        Ok(new)
    }

    /// Short title: the kind of issue and its record (“Death Before Birth · Jean GUYOT”), or
    /// “Correction · Jean GUYOT”.
    pub fn title(&self, g: &Gedcom) -> String {
        let Some(issue) = &self.issue else {
            let kind = t!("change.correction").into_owned();
            let record = match &self.action {
                Action::Edit(edits) => edits.iter().find_map(|e| e.record()).map(|x| g.record_label(x)),
                _ => None,
            };
            return match record {
                Some(r) => format!("{kind} · {r}"),
                None => kind,
            };
        };
        let kind = code_label(issue.code).into_owned();
        match g.get(&issue.xref) {
            Some(_) => format!("{kind} · {}", g.record_label(&issue.xref)),
            None => match issue.related.first() {
                Some(v) if quality::category(issue.code) == Category::Places => format!("{kind} · {}", short_place(v)),
                Some(v) => format!("{kind} · {v}"),
                None => kind,
            },
        }
    }

    /// What the change answers: the issue's message, or the agent's reason for a correction.
    pub fn subject(&self) -> &str {
        match &self.issue {
            Some(i) => &i.message,
            None => &self.reason,
        }
    }

    /// “In the tree” / “Proposed” lines.
    pub fn rows(&self, g: &Gedcom) -> Vec<ChangeRow> {
        let row =
            |label: String, current: String, proposed: String, change| ChangeRow { label, current, proposed, change };
        let message = self.subject().to_string();
        let code = self.issue.as_ref().map(|i| i.code).unwrap_or_default();
        match &self.action {
            Action::Fix => {
                let fix = quality::fix_of(code).map(|f| f.label.into_owned()).unwrap_or_default();
                vec![row(code_label(code).into_owned(), message, fix, Change::Differs)]
            }
            Action::Hide => {
                vec![row(t!("change.issue").into(), message, t!("change.hidden").into(), Change::Same)]
            }
            // What the place will be: level by level, then its coordinates.
            Action::Merge(_) if self.place_pair().is_some() => {
                let Ok((plan, sides)) = self.place_merge_choices(g) else { return vec![] };
                let pick = |c: usize, a: &str, b: &str| match sides.get(c) {
                    Some(Side::Keep) => a.to_string(),
                    _ => b.to_string(),
                };
                // A level both places leave empty (no locality) is not shown.
                let mut rows: Vec<ChangeRow> = plan
                    .rows
                    .iter()
                    .filter(|r| !(r.kind == PlaceRow::Same && r.a.is_empty()))
                    .map(|r| match r.kind {
                        PlaceRow::Same => {
                            row(r.label.clone(), or_dash(r.a.clone()), or_dash(r.a.clone()), Change::Same)
                        }
                        PlaceRow::Added => {
                            let v = if r.a.is_empty() { r.b.clone() } else { r.a.clone() };
                            row(r.label.clone(), v.clone(), v, Change::Refined)
                        }
                        PlaceRow::Conflict(c) => {
                            row(r.label.clone(), format!("{} / {}", r.a, r.b), pick(c, &r.a, &r.b), Change::Differs)
                        }
                    })
                    .collect();
                if let crate::place_merge::Coords::Conflict(c, _) = plan.coords {
                    let text = |side| plan.coords_text(side).unwrap_or_default();
                    let (a, b) = (text(Side::Keep), text(Side::Other));
                    rows.push(row(
                        t!("change.coordinates").into(),
                        format!("{a} / {b}"),
                        pick(c, &a, &b),
                        Change::Differs,
                    ));
                }
                rows
            }
            // What the person will have: kept, added, or the chosen value.
            Action::Merge(_) => {
                let Ok((plan, sides)) = self.merge_choices(g) else { return vec![] };
                plan.rows
                    .iter()
                    .map(|r| {
                        let both = |v: &[String]| v.join(", ");
                        let current = match r.kind {
                            RowKind::Same => both(&r.keep),
                            _ => [both(&r.keep), both(&r.other)]
                                .into_iter()
                                .filter(|s| !s.is_empty())
                                .collect::<Vec<_>>()
                                .join(" / "),
                        };
                        let (proposed, change) = match r.kind {
                            RowKind::Same => (both(&r.keep), Change::Same),
                            RowKind::Combined => (both(&r.keep), Change::Refined),
                            RowKind::Added => {
                                let mut all = r.keep.clone();
                                all.extend(r.other.iter().filter(|v| !r.keep.contains(v)).cloned());
                                (all.join(" / "), Change::Added)
                            }
                            RowKind::Conflict(c) => match sides.get(c) {
                                Some(Side::Other) => (both(&r.other), Change::Differs),
                                _ => (both(&r.keep), Change::Differs),
                            },
                        };
                        row(r.label.clone(), or_dash(current), or_dash(proposed), change)
                    })
                    .collect()
            }
            Action::Edit(edits) => edits
                .iter()
                .map(|e| match e {
                    Edit::SetEvent { record, index, value, date, place, address, cause } => {
                        let label = g.event_title(record, *index).unwrap_or_default();
                        let ev = g.read_event(record, *index).unwrap_or_default();
                        let new_value = value.as_deref().map(str::trim).unwrap_or(&ev.detail).to_string();
                        let new_date = date.as_deref().map(|d| Date::parse(d).to_gedcom()).unwrap_or(ev.date.clone());
                        let new_place = place.as_deref().map(|p| g.known_place(p)).unwrap_or(ev.place.clone());
                        let new_address = address.as_deref().map(str::trim).unwrap_or(&ev.address).to_string();
                        let old_cause = event_ref(g, record, *index)
                            .ok()
                            .and_then(|e| e.child_val("CAUS"))
                            .unwrap_or_default()
                            .to_string();
                        let new_cause = cause.as_deref().map(str::trim).unwrap_or(&old_cause).to_string();
                        let with_cause = |text: String, cause: &str| {
                            if cause.is_empty() { text } else { format!("{text} ({cause})") }
                        };
                        row(
                            label,
                            or_dash(with_cause(event_text(&ev.detail, &ev.date, &ev.address, &ev.place), &old_cause)),
                            or_dash(with_cause(
                                event_text(&new_value, &new_date, &new_address, &new_place),
                                &new_cause,
                            )),
                            Change::Differs,
                        )
                    }
                    Edit::SetAge { record, index, spouse, age } => {
                        let ev = event_ref(g, record, *index).ok();
                        let holder = match g.family(record) {
                            Some(f) => {
                                let t = if f.husband() == spouse.as_deref() { "HUSB" } else { "WIFE" };
                                ev.and_then(|e| e.child(t))
                            }
                            None => ev,
                        };
                        let current = holder.and_then(|h| h.child_val("AGE")).unwrap_or_default().to_string();
                        let who = spouse.as_deref().map(|s| format!(" · {}", g.record_label(s))).unwrap_or_default();
                        let label = format!(
                            "{} · {}{who}",
                            g.event_title(record, *index).unwrap_or_default(),
                            t!("change.age")
                        );
                        let proposed = age_value(age).unwrap_or_default();
                        let change = if current.is_empty() { Change::Added } else { Change::Differs };
                        row(label, or_dash(current), or_dash(proposed), change)
                    }
                    Edit::LinkRelative { person, relation, other } => {
                        let label = format!("{} · {}", relation_label(*relation), g.record_label(person));
                        row(label, "—".into(), g.record_label(other), Change::Added)
                    }
                    Edit::UnlinkRelative { person, relation, other } => {
                        let label = format!("{} · {}", relation_label(*relation), g.record_label(person));
                        row(label, g.record_label(other), t!("change.deleted").into(), Change::Differs)
                    }
                    Edit::SetPedigree { person, parent, pedigree } => {
                        let fam = parents_family(g, person, parent.as_deref()).ok();
                        let current = fam
                            .and_then(|f| {
                                let p = format!("@{f}@");
                                g.get(person)?
                                    .children
                                    .iter()
                                    .find(|c| c.tag == "FAMC" && c.val() == p)?
                                    .child_val("PEDI")
                                    .map(str::to_string)
                            })
                            .unwrap_or_default();
                        let pedi =
                            |v: &str| crate::schema::enum_label(v).map(|l| l.into_owned()).unwrap_or(v.to_string());
                        let label = format!("{} · {}", t!("change.pedigree"), g.record_label(person));
                        let change = if current.is_empty() { Change::Added } else { Change::Differs };
                        let current = if current.is_empty() { String::new() } else { pedi(&current) };
                        row(label, or_dash(current), pedi(&pedigree.trim().to_uppercase()), change)
                    }
                    Edit::AddNote { record, event, text } => {
                        row(note_label(g, record, *event), "—".into(), text.trim().to_string(), Change::Added)
                    }
                    Edit::SetNote { record, event, note, text } => {
                        let current = note_holder(g, record, *event)
                            .ok()
                            .and_then(|h| h.children_with("NOTE").nth(*note))
                            .map(|n| n.text())
                            .unwrap_or_default();
                        let proposed = if text.trim().is_empty() {
                            t!("change.deleted").into_owned()
                        } else {
                            text.trim().to_string()
                        };
                        row(note_label(g, record, *event), or_dash(current), proposed, Change::Differs)
                    }
                    Edit::AddAssociate { record, index, person, role, phrase } => {
                        let sex = g.individual(person).map(|i| i.sex().to_string()).unwrap_or_default();
                        let (role, detail) =
                            crate::act::association_role(&role.trim().to_uppercase(), phrase.trim(), &sex);
                        let role = match detail {
                            Some(d) => format!("{role}, {d}"),
                            None => role,
                        };
                        let label = g.event_title(record, *index).unwrap_or_default();
                        row(label, "—".into(), format!("{} ({role})", g.record_label(person)), Change::Added)
                    }
                    Edit::RemoveAssociate { record, index, person } => {
                        let label = g.event_title(record, *index).unwrap_or_default();
                        row(label, g.record_label(person), t!("change.deleted").into(), Change::Differs)
                    }
                    Edit::AttachMedia { record, event, media, file } => {
                        let title = match (media, file) {
                            (Some(m), _) => g.record_label(m),
                            (None, Some(f)) => {
                                g.media_by_file(f.trim()).map(|m| g.record_label(&m)).unwrap_or(f.clone())
                            }
                            (None, None) => String::new(),
                        };
                        row(media_label(g, record, *event), "—".into(), or_dash(title), Change::Added)
                    }
                    Edit::DetachMedia { record, event, media } => row(
                        media_label(g, record, *event),
                        g.record_label(media),
                        t!("change.deleted").into(),
                        Change::Differs,
                    ),
                    Edit::AddEvent { tag, value, date, place, address, .. } => {
                        let label = crate::model::event_label(&tag.trim().to_uppercase())
                            .map(|l| l.to_string())
                            .unwrap_or_else(|| tag.clone());
                        row(
                            label,
                            "—".into(),
                            or_dash(event_text(value, &Date::parse(date).to_gedcom(), address, place)),
                            Change::Added,
                        )
                    }
                    Edit::DeleteEvent { record, index } => {
                        let label = g.event_title(record, *index).unwrap_or_default();
                        let ev = g.read_event(record, *index).unwrap_or_default();
                        let d = if ev.date.is_empty() { String::new() } else { Date::parse(&ev.date).display() };
                        row(label, or_dash(d), t!("change.deleted").into(), Change::Differs)
                    }
                    Edit::SetSex { person, sex } => {
                        let current = g.individual(person).map(|i| i.sex_label().into_owned()).unwrap_or_default();
                        row(g.record_label(person), current, sex_label(sex), Change::Differs)
                    }
                    Edit::SetRepository { source, repository, new, call_number } => {
                        let src = g.get(source).map(crate::model::Source::new);
                        let name_of = |x: &str| g.get(x).and_then(|n| n.child_val("NAME")).unwrap_or(x).to_string();
                        let with_caln = |name: String, caln: &str| {
                            if caln.trim().is_empty() { name } else { format!("{name} · {}", caln.trim()) }
                        };
                        let current = src
                            .and_then(|s| {
                                Some(with_caln(name_of(s.repository()?), s.call_number().unwrap_or_default()))
                            })
                            .unwrap_or_default();
                        let caln = call_number
                            .clone()
                            .or_else(|| src.and_then(|s| s.call_number()).map(str::to_string))
                            .unwrap_or_default();
                        let (name, created) = match (repository, new) {
                            (Some(r), _) => (name_of(r), false),
                            (None, Some(n)) => match g.repository_named(&n.name) {
                                Some(r) => (name_of(&r), false),
                                None => (n.name.trim().to_string(), true),
                            },
                            (None, None) => (String::new(), false),
                        };
                        let name = if created { t!("change.new_repository", name = name).into_owned() } else { name };
                        let label =
                            format!("{} · {}", t!("change.repository"), src.map(|s| s.title()).unwrap_or_default());
                        let change = if current.is_empty() { Change::Added } else { Change::Differs };
                        row(label, or_dash(current), with_caln(name, &caln), change)
                    }
                    Edit::SetCoordinates { place, latitude, longitude, .. } => {
                        let current = g
                            .places()
                            .into_iter()
                            .find(|p| &p.name == place)
                            .and_then(|p| Some(format!("{}, {}", p.latitude?, p.longitude?)))
                            .unwrap_or_default();
                        let proposed = format!("{}, {}", format_latitude(*latitude), format_longitude(*longitude));
                        let change = if current.is_empty() { Change::Added } else { Change::Differs };
                        row(crate::short_place(place), or_dash(current), proposed, change)
                    }
                    Edit::RenamePlace { place, name } => {
                        row(t!("change.place").into(), place.clone(), name.trim().to_string(), Change::Differs)
                    }
                    Edit::SetMediaFile { file } => {
                        let current = self
                            .issue
                            .as_ref()
                            .and_then(|issue| g.get(&issue.xref).zip(issue.path.first()))
                            .and_then(|(m, i)| m.children.get(*i))
                            .map(|f| f.val().to_string())
                            .unwrap_or_default();
                        row(t!("change.file").into(), or_dash(current), file.clone(), Change::Differs)
                    }
                    Edit::CiteSource { record, index, source, page, quality } => {
                        let label = g.event_title(record, *index).unwrap_or_default();
                        let proposed = [g.record_label(source), page.trim().to_string(), quality_label(*quality)]
                            .into_iter()
                            .filter(|s| !s.is_empty())
                            .collect::<Vec<_>>()
                            .join(" · ");
                        row(label, t!("change.no_source").into(), proposed, Change::Added)
                    }
                    Edit::SetValue { value } => {
                        let current = self.issue_node(g);
                        let label = current.map(|n| n.tag.clone()).unwrap_or_default();
                        let current = current.map(|n| n.val().to_string()).unwrap_or_default();
                        row(label, or_dash(current), or_dash(value.clone()), Change::Differs)
                    }
                    Edit::DeleteStructure => {
                        let current = self.issue_node(g);
                        let label = current.map(|n| n.tag.clone()).unwrap_or_default();
                        let current = current.map(|n| n.val().to_string()).unwrap_or_default();
                        row(label, or_dash(current), t!("change.deleted").into(), Change::Differs)
                    }
                    Edit::SetName { person, index, given, surname } => {
                        let current = g
                            .get(person)
                            .and_then(|r| r.children_with("NAME").nth(*index).map(NameForm::read))
                            .unwrap_or_default();
                        let mut new = current.clone();
                        if let Some(v) = given {
                            new.given = v.trim().to_string();
                        }
                        if let Some(v) = surname {
                            new.surname = v.trim().to_string();
                        }
                        row(
                            t!("change.name").into(),
                            or_dash(name_text(&current)),
                            or_dash(name_text(&new)),
                            Change::Differs,
                        )
                    }
                    Edit::AddName { given, surname, .. } => {
                        let new = NameForm { given: given.clone(), surname: surname.clone(), ..Default::default() };
                        row(t!("change.other_name").into(), "—".into(), or_dash(name_text(&new)), Change::Added)
                    }
                })
                .collect(),
        }
    }

    /// The structure the issue designates.
    fn issue_node<'a>(&self, g: &'a Gedcom) -> Option<&'a Node> {
        let i = self.issue.as_ref()?;
        g.get(&i.xref).and_then(|r| node_ref(r, &i.path))
    }

    /// GEDCOM lines that change, record by record, computed on a copy of the file.
    pub fn diff(&self, g: &Gedcom) -> Result<Vec<DiffLine>, String> {
        let mut copy = g.clone();
        self.apply(&mut copy)?;
        let mut out = Vec::new();
        let mut xrefs: Vec<String> = g.records.iter().filter_map(|r| r.xref.clone()).collect();
        xrefs.extend(copy.records.iter().filter_map(|r| r.xref.clone()).filter(|x| g.get(x).is_none()));
        for x in xrefs {
            // CHAN changes on every touched record: not a difference worth reading.
            let text = |n: &Node| {
                let mut n = n.clone();
                n.children.retain(|c| c.tag != "CHAN");
                node_to_string(&n)
            };
            let before = g.get(&x).map(text).unwrap_or_default();
            let after = copy.get(&x).map(text).unwrap_or_default();
            if before == after {
                continue;
            }
            if !out.is_empty() {
                out.push((' ', String::new()));
            }
            out.extend(line_diff(&before, &after));
        }
        Ok(out)
    }
}

/// A GEDCOM age (`42y`, `3y 6m`, `< 1y`) as given, or one read from words (“42 ans”, `6 months`).
fn age_value(s: &str) -> Option<String> {
    let s = s.trim();
    let gedcom = !s.is_empty()
        && s.trim_start_matches(['<', '>']).split_whitespace().all(|p| {
            let digits = p.trim_end_matches(['y', 'm', 'w', 'd']);
            !digits.is_empty() && digits.len() + 1 == p.len() && digits.chars().all(|c| c.is_ascii_digit())
        });
    if gedcom { Some(s.to_string()) } else { crate::act::gedcom_age(s) }
}

/// An event of a record, by its position.
fn event_ref<'a>(g: &'a Gedcom, record: &str, index: usize) -> Result<&'a Node, String> {
    g.get(record)
        .and_then(|n| n.children.get(index))
        .filter(|c| crate::model::event_label(&c.tag).is_some())
        .ok_or_else(|| format!("no event {index} in {record} (`index` in `person_sheet`)"))
}

fn event_mut<'a>(g: &'a mut Gedcom, record: &str, index: usize) -> Result<&'a mut Node, String> {
    g.get_mut(record)
        .and_then(|n| n.children.get_mut(index))
        .filter(|c| crate::model::event_label(&c.tag).is_some())
        .ok_or_else(|| format!("no event {index} in {record}"))
}

fn individual<'a>(g: &'a Gedcom, x: &str) -> Result<&'a Node, String> {
    g.get(x).filter(|n| n.tag == "INDI").ok_or_else(|| format!("no person {x} in the tree"))
}

/// The record, or its event, that holds notes.
fn note_holder<'a>(g: &'a Gedcom, record: &str, event: Option<usize>) -> Result<&'a Node, String> {
    match event {
        Some(i) => event_ref(g, record, i),
        None => g.get(record).ok_or_else(|| format!("no record {record} in the tree")),
    }
}

fn note_holder_mut<'a>(g: &'a mut Gedcom, record: &str, event: Option<usize>) -> Result<&'a mut Node, String> {
    match event {
        Some(i) => event_mut(g, record, i),
        None => g.get_mut(record).ok_or_else(|| format!("no record {record} in the tree")),
    }
}

/// `child` is no longer the child of `parent`: they leave the family `fam`, and stay the child of
/// the other parent of that family, in a family of their own with them.
fn detach_child(g: &mut Gedcom, child: &str, parent: &str, fam: &str) -> Result<(), String> {
    let f = g.family(fam).ok_or_else(|| t!("change.outdated").to_string())?;
    let other = [("HUSB", f.husband()), ("WIFE", f.wife())]
        .into_iter()
        .find_map(|(slot, p)| p.filter(|p| *p != parent).map(|p| (slot, p.to_string())));
    let pedi = g
        .get(child)
        .and_then(|c| c.children.iter().find(|l| l.tag == "FAMC" && l.pointer() == Some(fam)))
        .and_then(|l| l.child_val("PEDI"))
        .map(str::to_string);
    g.unlink_from_family(child, fam);
    if let Some((slot, other)) = other {
        let new = g.create_family();
        g.set_parent_slot(&new, &other, Some(slot))?;
        g.add_child_to_family(&new, child)?;
        if let Some(p) = pedi
            && let Some(l) = g
                .get_mut(child)
                .and_then(|c| c.children.iter_mut().find(|l| l.tag == "FAMC" && l.pointer() == Some(new.as_str())))
        {
            l.set_child_val("PEDI", &p);
        }
    }
    Ok(())
}

/// The family that holds the link “`other` is the `relation` of `person`”.
fn link_family(g: &Gedcom, person: &str, relation: Relation, other: &str) -> Option<String> {
    let me = g.individual(person)?;
    let (families, as_parent) = match relation {
        Relation::Father | Relation::Mother => (me.famc(), true),
        Relation::Sibling => (me.famc(), false),
        Relation::Spouse => (me.fams(), true),
        Relation::Child => (me.fams(), false),
    };
    families
        .into_iter()
        .filter_map(|f| g.family(f))
        .find(|f| match as_parent {
            true => f.husband() == Some(other) || f.wife() == Some(other),
            false => f.children().contains(&other),
        })
        .map(|f| f.xref().to_string())
}

/// The family of origin of `person` with `parent` among its parents (the only one, without
/// `parent`).
fn parents_family(g: &Gedcom, person: &str, parent: Option<&str>) -> Result<String, String> {
    let me = g.individual(person).ok_or_else(|| format!("no person {person} in the tree"))?;
    let families = me.famc();
    match parent {
        Some(p) => families
            .into_iter()
            .filter_map(|f| g.family(f))
            .find(|f| f.husband() == Some(p) || f.wife() == Some(p))
            .map(|f| f.xref().to_string())
            .ok_or_else(|| format!("{p} is not a parent of {person} in the tree")),
        None => match families.as_slice() {
            [f] => Ok(f.to_string()),
            [] => Err(format!("{person} has no parents in the tree")),
            _ => Err(format!("{person} has several families of parents: give `parent`")),
        },
    }
}

/// “Note · Death” or “Note · Jean GUYOT”.
fn note_label(g: &Gedcom, record: &str, event: Option<usize>) -> String {
    let of = match event {
        Some(i) => g.event_title(record, i).unwrap_or_default(),
        None => g.record_label(record),
    };
    format!("{} · {of}", t!("change.note"))
}

/// “Media · Baptism” or “Media · Jean GUYOT”.
fn media_label(g: &Gedcom, record: &str, event: Option<usize>) -> String {
    let of = match event {
        Some(i) => g.event_title(record, i).unwrap_or_default(),
        None => g.record_label(record),
    };
    format!("{} · {of}", t!("change.media"))
}

fn relation_name(r: Relation) -> &'static str {
    match r {
        Relation::Father => "father",
        Relation::Mother => "mother",
        Relation::Spouse => "spouse",
        Relation::Child => "child",
        Relation::Sibling => "sibling",
    }
}

fn relation_label(r: Relation) -> String {
    match r {
        Relation::Father => t!("change.relation.father"),
        Relation::Mother => t!("change.relation.mother"),
        Relation::Spouse => t!("change.relation.spouse"),
        Relation::Child => t!("change.relation.child"),
        Relation::Sibling => t!("change.relation.sibling"),
    }
    .into_owned()
}

/// A GEDCOM date as typed by the agent, checked; empty stays empty.
fn gedcom_date(d: &str) -> Result<String, String> {
    if d.trim().is_empty() {
        return Ok(String::new());
    }
    let parsed = Date::parse(d);
    if parsed.year().is_none() {
        return Err(format!("`{d}` is not a GEDCOM date (e.g. `12 MAR 1869`, `ABT 1850`)"));
    }
    Ok(parsed.to_gedcom())
}

/// “tisserand, 3 mars 1850, 4 rue Haute, Agneaux”: an event's value, date, address and place, as
/// displayed.
fn event_text(value: &str, date: &str, address: &str, place: &str) -> String {
    let d = if date.trim().is_empty() { String::new() } else { Date::parse(date).display() };
    [value.trim().to_string(), d, address.trim().to_string(), crate::short_place(place)]
        .into_iter()
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join(", ")
}

/// “Given SURNAME”, as the tree shows a name.
fn name_text(n: &NameForm) -> String {
    [n.given.trim().to_string(), n.surname.trim().to_string()]
        .into_iter()
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

fn or_dash(s: String) -> String {
    if s.trim().is_empty() { "—".into() } else { s }
}

fn sex_label(sex: &str) -> String {
    match sex.trim().to_uppercase().as_str() {
        "M" => t!("sex.male"),
        "F" => t!("sex.female"),
        "X" => t!("sex.other"),
        _ => t!("sex.unknown"),
    }
    .into_owned()
}

fn conflict_list(plan: &MergePlan) -> String {
    if plan.conflicts.is_empty() {
        "nothing: they do not contradict each other, give empty `choices`".into()
    } else {
        plan.conflicts.iter().map(|c| c.tag.as_str()).collect::<Vec<_>>().join(", ")
    }
}

fn allowed_list(code: &str) -> String {
    if code == "possible_duplicate" {
        return "nothing: merge them (`merge`, with `choices`) or, if they are two people, hide it".into();
    }
    if code == "possible_duplicate_place" {
        return "nothing: merge them (`merge`, with `choices`) or, if they are two places, hide it".into();
    }
    let e = edits_for(code);
    if e.is_empty() { "nothing: this issue can only be hidden, or fixed by the user".into() } else { e.join(", ") }
}

fn node_ref<'a>(n: &'a Node, path: &[usize]) -> Option<&'a Node> {
    path.iter().try_fold(n, |n, &i| n.children.get(i))
}

fn node_at<'a>(g: &'a mut Gedcom, xref: &str, path: &[usize]) -> Option<&'a mut Node> {
    let mut n = g.get_mut(xref)?;
    for &i in path {
        n = n.children.get_mut(i)?;
    }
    Some(n)
}
