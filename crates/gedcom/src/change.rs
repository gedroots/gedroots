//! Changes proposed by an AI agent for a Data Quality issue: apply the Quality page's fix, edit the
//! records the issue is about, or hide the issue with the reason it is right.
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
use crate::node::Node;
use crate::parse::node_to_string;
use crate::proposal::{Change, DiffLine, line_diff};
use crate::quality::{self, Category, issue_ids};
use rust_i18n::t;
use std::collections::HashMap;

/// A typed edit of the records an issue is about.
#[derive(Debug, Clone, PartialEq)]
pub enum Edit {
    /// New date and/or place of an event (position `index` in the record).
    SetEvent {
        record: String,
        index: usize,
        date: Option<String>,
        place: Option<String>,
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
}

impl Edit {
    /// Name of the edit, as the agent gives it.
    pub fn name(&self) -> &'static str {
        match self {
            Edit::SetEvent { .. } => "set_event",
            Edit::DeleteEvent { .. } => "delete_event",
            Edit::SetSex { .. } => "set_sex",
            Edit::SetCoordinates { .. } => "set_coordinates",
            Edit::SetMediaFile { .. } => "set_media_file",
            Edit::CiteSource { .. } => "cite_source",
            Edit::SetValue { .. } => "set_value",
            Edit::DeleteStructure => "delete_structure",
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

/// A change proposed for an issue.
#[derive(Debug, Clone, PartialEq)]
pub struct QualityChange {
    /// The issue, as it was when the change was proposed, and its identifier.
    pub issue: Issue,
    pub issue_id: String,
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
    /// Records the change may edit: the issue's record and the records involved.
    fn allowed(&self, xref: &str) -> bool {
        self.issue.xref == xref || self.issue.related.iter().any(|r| r == xref)
    }

    /// Checks the change on its own: allowed for this kind of issue, with what it needs.
    fn validate(&self, g: &Gedcom) -> Result<(), String> {
        let code = self.issue.code;
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
                if !can_hide(g, &self.issue) {
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
                    let target = match e {
                        Edit::SetEvent { record, .. }
                        | Edit::DeleteEvent { record, .. }
                        | Edit::CiteSource { record, .. } => Some(record.as_str()),
                        Edit::SetSex { person, .. } => Some(person.as_str()),
                        _ => None,
                    };
                    if let Some(x) = target
                        && !self.allowed(x)
                    {
                        return Err(format!("{x} is not one of the records this issue is about"));
                    }
                    if let Edit::SetCoordinates { place, latitude, longitude } = e {
                        if !self.issue.related.iter().any(|p| p == place) {
                            return Err(format!("`{place}` is not the place of this issue"));
                        }
                        if !(-90.0..=90.0).contains(latitude) || !(-180.0..=180.0).contains(longitude) {
                            return Err("latitude must be within ±90 and longitude within ±180 degrees".into());
                        }
                    }
                    if let Edit::CiteSource { source, .. } = e
                        && g.get(source).is_none_or(|n| n.tag != "SOUR")
                    {
                        return Err(format!(
                            "no source {source} in the tree: cite an existing one (`search_sources`), or propose the act with `propose_act`"
                        ));
                    }
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
        let i = &self.issue;
        match &self.action {
            Action::Fix => {
                if !g.fix_issue(i) {
                    return Err(t!("change.outdated").into());
                }
            }
            // Two people who are not the same: recorded as distinct, never hidden by code.
            Action::Hide if i.code == "possible_duplicate" => {
                let other = i.related.first().ok_or_else(|| t!("change.outdated").to_string())?;
                if !g.mark_distinct(&i.xref, other) {
                    return Err(t!("change.outdated").into());
                }
            }
            Action::Hide => {
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
                for x in std::iter::once(&i.xref).chain(&i.related) {
                    if let Some(r) = g.get_mut(x) {
                        r.touch();
                    }
                }
            }
        }
        Ok(())
    }

    /// The two people of a possible duplicate, if they can still be merged.
    pub fn merge_pair(&self) -> Option<(String, String)> {
        (self.issue.code == "possible_duplicate")
            .then(|| Some((self.issue.xref.clone(), self.issue.related.first()?.clone())))
            .flatten()
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
        let i = &self.issue;
        let outdated = || -> String { t!("change.outdated").into() };
        match e {
            Edit::SetEvent { record, index, date, place } => {
                let mut ev = g.read_event(record, *index).ok_or_else(|| format!("no event {index} in {record}"))?;
                if let Some(d) = date {
                    let parsed = Date::parse(d);
                    if !d.trim().is_empty() && parsed.year().is_none() {
                        return Err(format!("`{d}` is not a GEDCOM date (e.g. `12 MAR 1869`, `ABT 1850`)"));
                    }
                    ev.date = if d.trim().is_empty() { String::new() } else { parsed.to_gedcom() };
                    ev.date_phrase.clear();
                }
                if let Some(p) = place {
                    ev.place = g.known_place(p);
                }
                g.update_event(record, *index, &ev)
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
                let n = node_at(g, &i.xref, &i.path).ok_or_else(outdated)?;
                n.value = (!value.trim().is_empty()).then(|| value.trim().to_string());
                Ok(())
            }
            Edit::DeleteStructure => {
                let (last, parents) =
                    i.path.split_last().ok_or_else(|| "this issue is about a whole record".to_string())?;
                let parent = node_at(g, &i.xref, parents).ok_or_else(outdated)?;
                if *last >= parent.children.len() {
                    return Err(outdated());
                }
                parent.children.remove(*last);
                Ok(())
            }
        }
    }

    /// Checks the change before it is queued: it fits the issue, and once applied to a copy of the
    /// file (`rerun` gives the issues of a file, like the Quality page), the issue is gone. Returns
    /// the issues the change would create.
    pub fn check(&self, g: &Gedcom, rerun: &dyn Fn(&Gedcom) -> Vec<Issue>) -> Result<Vec<Issue>, String> {
        let before = rerun(g);
        if find_issue(&before, &self.issue_id).is_none() {
            return Err(format!("no issue `{}` any more: call `quality_issues` again", self.issue_id));
        }
        let mut copy = g.clone();
        self.apply(&mut copy)?;
        let after = rerun(&copy);
        let (b, a) = (counts(&before), counts(&after));
        let key = count_key(&self.issue);
        if a.get(&key).copied().unwrap_or(0) >= b.get(&key).copied().unwrap_or(0) {
            return Err(format!("after this change, GEDRoots still reports: {}", self.issue.message));
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

    /// Short title: the kind of issue and its record (“Death Before Birth · Jean GUYOT”).
    pub fn title(&self, g: &Gedcom) -> String {
        let kind = code_label(self.issue.code).into_owned();
        match g.get(&self.issue.xref) {
            Some(_) => format!("{kind} · {}", g.record_label(&self.issue.xref)),
            None => match self.issue.related.first() {
                Some(v) => format!("{kind} · {v}"),
                None => kind,
            },
        }
    }

    /// “In the tree” / “Proposed” lines.
    pub fn rows(&self, g: &Gedcom) -> Vec<ChangeRow> {
        let row =
            |label: String, current: String, proposed: String, change| ChangeRow { label, current, proposed, change };
        let issue = &self.issue;
        match &self.action {
            Action::Fix => {
                let fix = quality::fix_of(issue.code).map(|f| f.label.into_owned()).unwrap_or_default();
                vec![row(code_label(issue.code).into_owned(), issue.message.clone(), fix, Change::Differs)]
            }
            Action::Hide => {
                vec![row(t!("change.issue").into(), issue.message.clone(), t!("change.hidden").into(), Change::Same)]
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
                    Edit::SetEvent { record, index, date, place } => {
                        let label = g.event_title(record, *index).unwrap_or_default();
                        let ev = g.read_event(record, *index).unwrap_or_default();
                        let show = |d: &str, p: &str| {
                            let d = if d.trim().is_empty() { String::new() } else { Date::parse(d).display() };
                            [d, crate::short_place(p)]
                                .into_iter()
                                .filter(|s| !s.is_empty())
                                .collect::<Vec<_>>()
                                .join(", ")
                        };
                        let new_date = date.as_deref().map(|d| Date::parse(d).to_gedcom()).unwrap_or(ev.date.clone());
                        let new_place = place.as_deref().map(|p| g.known_place(p)).unwrap_or(ev.place.clone());
                        row(
                            label,
                            or_dash(show(&ev.date, &ev.place)),
                            or_dash(show(&new_date, &new_place)),
                            Change::Differs,
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
                        let current = g
                            .get(&issue.xref)
                            .and_then(|m| issue.path.first().and_then(|i| m.children.get(*i)))
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
                        let current = g.get(&issue.xref).and_then(|r| node_ref(r, &issue.path));
                        let label = current.map(|n| n.tag.clone()).unwrap_or_default();
                        let current = current.map(|n| n.val().to_string()).unwrap_or_default();
                        row(label, or_dash(current), or_dash(value.clone()), Change::Differs)
                    }
                    Edit::DeleteStructure => {
                        let current = g.get(&issue.xref).and_then(|r| node_ref(r, &issue.path));
                        let label = current.map(|n| n.tag.clone()).unwrap_or_default();
                        let current = current.map(|n| n.val().to_string()).unwrap_or_default();
                        row(label, or_dash(current), t!("change.deleted").into(), Change::Differs)
                    }
                })
                .collect(),
        }
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
