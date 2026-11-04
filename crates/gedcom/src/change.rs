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

use crate::analysis::check::{IGNORE_TAG, Issue, code_label};
use crate::date::Date;
use crate::document::Gedcom;
use crate::geo::{format_latitude, format_longitude};
use crate::merge::{MergePlan, RowKind, Side};
use crate::model::NameForm;
use crate::node::Node;
use crate::parse::node_to_string;
use crate::proposal::{Change, DiffLine, line_diff};
use crate::quality::{self, Category, issue_ids};
use rust_i18n::t;
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
    },
    /// A new event or attribute of a person or family (`OCCU` with its value, `RESI`, `BIRT`…).
    AddEvent {
        record: String,
        tag: String,
        value: String,
        date: String,
        place: String,
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
}

impl Edit {
    /// Name of the edit, as the agent gives it.
    pub fn name(&self) -> &'static str {
        match self {
            Edit::SetEvent { .. } => "set_event",
            Edit::AddEvent { .. } => "add_event",
            Edit::DeleteEvent { .. } => "delete_event",
            Edit::SetSex { .. } => "set_sex",
            Edit::SetCoordinates { .. } => "set_coordinates",
            Edit::SetMediaFile { .. } => "set_media_file",
            Edit::CiteSource { .. } => "cite_source",
            Edit::SetValue { .. } => "set_value",
            Edit::DeleteStructure => "delete_structure",
            Edit::SetName { .. } => "set_name",
            Edit::AddName { .. } => "add_name",
        }
    }

    /// The record the edit changes, when it names one.
    fn record(&self) -> Option<&str> {
        match self {
            Edit::SetEvent { record, .. }
            | Edit::AddEvent { record, .. }
            | Edit::DeleteEvent { record, .. }
            | Edit::CiteSource { record, .. } => Some(record),
            Edit::SetSex { person, .. } | Edit::SetName { person, .. } | Edit::AddName { person, .. } => Some(person),
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
    /// the person whose value is kept.
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
pub const CORRECTIONS: &[&str] =
    &["set_name", "add_name", "set_event", "add_event", "delete_event", "set_sex", "cite_source", "set_coordinates"];

/// Edits each kind of issue allows. Changing a filiation (own ancestor) or a person's links stays
/// with the user.
pub fn edits_for(code: &str) -> &'static [&'static str] {
    match code {
        "husband_not_male" | "wife_not_female" | "role_sex_mismatch" => &["set_sex"],
        // A possible duplicate is merged (`Action::Merge`) or marked as two people (`Action::Hide`).
        "own_ancestor" | "disconnected" | "possible_duplicate" => &[],
        "tag_not_allowed" | "missing_required" | "too_many" | "wrong_pointer" | "invalid_value" => {
            &["set_value", "delete_structure"]
        }
        "unsourced" => &["cite_source"],
        "missing_coords" | "bad_coords" => &["set_coordinates"],
        "missing_file" => &["set_media_file"],
        code if quality::category(code) == Category::Dates => &["set_event", "delete_event"],
        _ => &[],
    }
}

/// Can the issue be hidden? Only an issue about a record: `_GR_IGNORE` is written under it.
pub fn can_hide(g: &Gedcom, issue: &Issue) -> bool {
    g.get(&issue.xref).is_some()
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
            _ => Ok(()),
        }
    }

    /// Checks the change on its own: allowed for this kind of issue, with what it needs.
    fn validate(&self, g: &Gedcom) -> Result<(), String> {
        let Some(issue) = &self.issue else { return self.validate_correction(g) };
        let code = issue.code;
        match &self.action {
            Action::Fix => {
                if quality::fix_of(code).is_none() {
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
                if code != "possible_duplicate" {
                    return Err(format!("only a `possible_duplicate` can be merged, not `{code}`"));
                }
                if self.reason.trim().is_empty() {
                    return Err("give the reason they are the same person in `reason`".into());
                }
                self.merge_choices(g)?;
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
            Action::Hide => {
                let i = self.the_issue()?;
                if !g.ignore_issue_because(&i.xref, i.code, &self.reason) {
                    return Err(t!("change.outdated").into());
                }
            }
            Action::Merge(_) => {
                let (plan, sides) = self.merge_choices(g)?;
                g.merge_individuals(&plan, &sides)?;
            }
            Action::Edit(edits) => {
                // Removals last, from the end, so the positions given stay right.
                let mut order: Vec<&Edit> = edits.iter().collect();
                order.sort_by_key(|e| match e {
                    Edit::DeleteEvent { index, .. } => (1, usize::MAX - index),
                    Edit::DeleteStructure => (2, 0),
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
            Edit::SetEvent { record, index, value, date, place } => {
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
                g.update_event(record, *index, &ev)
            }
            Edit::AddEvent { record, tag, value, date, place } => {
                let ev = crate::NewEvent {
                    tag: tag.trim().to_uppercase(),
                    detail: value.trim().to_string(),
                    date: gedcom_date(date)?,
                    place: if place.trim().is_empty() { String::new() } else { g.known_place(place) },
                    ..Default::default()
                };
                g.add_event(record, &ev)
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
            Edit::SetCoordinates { place, latitude, longitude } => {
                let n = g.update_place(place, place, &format_latitude(*latitude), &format_longitude(*longitude));
                if n == 0 { Err(outdated()) } else { Ok(()) }
            }
            Edit::SetMediaFile { file } => {
                if file.trim().is_empty() {
                    return Err("give the file's path, relative to the tree's folder".into());
                }
                let i = self.the_issue()?;
                if g.set_media_file(&i.xref, &i.path, file.trim()) { Ok(()) } else { Err(outdated()) }
            }
            Edit::CiteSource { record, index, source, page } => {
                let node = g
                    .get_mut(record)
                    .and_then(|n| n.children.get_mut(*index))
                    .filter(|c| crate::model::event_label(&c.tag).is_some())
                    .ok_or_else(|| format!("no event {index} in {record}"))?;
                let mut cit = Node::with_value("SOUR", format!("@{}@", source.trim_matches('@')));
                if !page.trim().is_empty() {
                    cit.children.push(Node::with_value("PAGE", page.trim()));
                }
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
                    Edit::SetEvent { record, index, value, date, place } => {
                        let label = g.event_title(record, *index).unwrap_or_default();
                        let ev = g.read_event(record, *index).unwrap_or_default();
                        let new_value = value.as_deref().map(str::trim).unwrap_or(&ev.detail).to_string();
                        let new_date = date.as_deref().map(|d| Date::parse(d).to_gedcom()).unwrap_or(ev.date.clone());
                        let new_place = place.as_deref().map(|p| g.known_place(p)).unwrap_or(ev.place.clone());
                        row(
                            label,
                            or_dash(event_text(&ev.detail, &ev.date, &ev.place)),
                            or_dash(event_text(&new_value, &new_date, &new_place)),
                            Change::Differs,
                        )
                    }
                    Edit::AddEvent { tag, value, date, place, .. } => {
                        let label = crate::model::event_label(&tag.trim().to_uppercase())
                            .map(|l| l.to_string())
                            .unwrap_or_else(|| tag.clone());
                        row(
                            label,
                            "—".into(),
                            or_dash(event_text(value, &Date::parse(date).to_gedcom(), place)),
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
                    Edit::SetCoordinates { place, latitude, longitude } => {
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
                    Edit::CiteSource { record, index, source, page } => {
                        let label = g.event_title(record, *index).unwrap_or_default();
                        let title = g.record_label(source);
                        let proposed =
                            if page.trim().is_empty() { title } else { format!("{title} · {}", page.trim()) };
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

/// “tisserand, 3 mars 1850, Agneaux”: an event's value, date and place, as displayed.
fn event_text(value: &str, date: &str, place: &str) -> String {
    let d = if date.trim().is_empty() { String::new() } else { Date::parse(date).display() };
    [value.trim().to_string(), d, crate::short_place(place)]
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
