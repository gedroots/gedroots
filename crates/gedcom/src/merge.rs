//! Merging two individuals that are the same person.
//!
//! Nothing is lost silently: identical data is kept once, data that does not contradict is
//! combined (an occupation from each record, the citations of both births…), and only what
//! contradicts (two different births, two names, two sexes) is a [`Conflict`] whose value the user
//! chooses. Every pointer to the merged-away record (families, witnesses, photos, root person) then
//! points to the kept one, and two families with the same spouses become one.

use crate::analysis::span::Span;
use crate::document::Gedcom;
use crate::duplicates::DISTINCT_TAG;
use crate::model::{event_label, event_name, sort_key};
use crate::node::Node;

/// Facts a person has only once: when both records have one and they contradict, the user
/// chooses.
const SINGLE: &[&str] = &["NAME", "SEX", "BIRT", "CHR", "BAPM", "DEAT", "BURI", "CREM"];
/// Events a family has once per type.
const SINGLE_FAM: &[&str] = &["ENGA", "MARR", "MARC", "MARL", "MARS", "DIV", "DIVF", "ANUL"];
/// Never copied from the merged-away record.
const SKIPPED: &[&str] = &["CHAN", "CREA"];

/// Which record a conflicting value comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    Keep,
    Other,
}

/// What a line of the comparison does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RowKind {
    /// The same in both records.
    Same,
    /// In only one record, or in both without contradiction: everything is kept.
    Added,
    /// Both records have a compatible value: they become one (the more precise date, both
    /// citations).
    Combined,
    /// Contradictory values: index in [`MergePlan::conflicts`].
    Conflict(usize),
}

/// A line of the comparison, in words.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    /// “Birth”, “Name”, “Occupation”…
    pub label: String,
    pub kind: RowKind,
    /// Values of the kept record and of the other one (empty: none).
    pub keep: Vec<String>,
    pub other: Vec<String>,
}

/// Contradictory values of a fact that a person has only once.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Conflict {
    /// `NAME`, `SEX`, `BIRT`…
    pub tag: String,
    pub keep: String,
    pub other: String,
}

/// What merging two individuals will do, before anything is written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MergePlan {
    /// Individual kept (the root person, otherwise the one with the most links).
    pub keep: String,
    /// Individual merged into `keep`, then deleted.
    pub other: String,
    pub rows: Vec<Row>,
    pub conflicts: Vec<Conflict>,
}

/// Date and place of an event, in words.
fn describe(n: &Node) -> String {
    if n.tag == "NAME" {
        return n.val().replace('/', "").split_whitespace().collect::<Vec<_>>().join(" ");
    }
    if n.tag == "SEX" {
        return match n.val() {
            "M" => t!("sex.male").into_owned(),
            "F" => t!("sex.female").into_owned(),
            _ => t!("sex.unknown").into_owned(),
        };
    }
    let mut parts = Vec::new();
    if !n.val().is_empty() && n.pointer().is_none() && n.val() != "Y" {
        parts.push(n.val().to_string());
    }
    if let Some(d) = n.path_val(&["DATE"]).filter(|d| !d.is_empty()) {
        parts.push(crate::date::Date::parse(d).display());
    }
    if let Some(p) = n.path_val(&["PLAC"]).filter(|p| !p.is_empty()) {
        parts.push(p.split(',').map(str::trim).filter(|s| !s.is_empty()).collect::<Vec<_>>().join(", "));
    }
    if parts.is_empty() { t!("merge.no_detail").into_owned() } else { parts.join(" · ") }
}

fn label_of(tag: &str) -> String {
    match tag {
        "NAME" => t!("merge.name").into_owned(),
        "SEX" => t!("merge.sex").into_owned(),
        _ => event_name(tag),
    }
}

fn first_level(p: &str) -> String {
    sort_key(p.split(',').map(str::trim).find(|s| !s.is_empty()).unwrap_or(""))
}

/// Number of days a date covers (smaller: more precise); `None` if it cannot be read.
fn width(raw: &str) -> Option<i64> {
    let s = Span::parse(raw)?;
    Some(s.hi? - s.lo?)
}

/// Two values of the same fact that can be the same (one date within the other, same place or
/// one missing).
fn compatible(a: &Node, b: &Node) -> bool {
    match a.tag.as_str() {
        "NAME" => sort_key(a.val()).split_whitespace().eq(sort_key(b.val()).split_whitespace()),
        "SEX" => a.val() == b.val() || !matches!(a.val(), "M" | "F") || !matches!(b.val(), "M" | "F"),
        _ => {
            let (da, db) = (a.path_val(&["DATE"]).unwrap_or(""), b.path_val(&["DATE"]).unwrap_or(""));
            let dates = da.is_empty()
                || db.is_empty()
                || da == db
                || match (Span::parse(da), Span::parse(db)) {
                    (Some(x), Some(y)) => !x.surely_before(&y) && !y.surely_before(&x),
                    _ => false,
                };
            let (pa, pb) = (a.path_val(&["PLAC"]).unwrap_or(""), b.path_val(&["PLAC"]).unwrap_or(""));
            let places = pa.is_empty() || pb.is_empty() || first_level(pa) == first_level(pb);
            dates && places
        }
    }
}

/// Adds `child` to `n` unless an identical structure is already there.
fn add_unique(n: &mut Node, child: Node) {
    if !n.children.contains(&child) {
        n.insert_child(child);
    }
}

/// Combines two compatible values of a fact: the more precise date and place, the substructures
/// of both.
fn combine(a: &Node, b: &Node) -> Node {
    let mut out = a.clone();
    if a.tag == "SEX" {
        if !matches!(a.val(), "M" | "F") {
            out.value = b.value.clone();
        }
        return out;
    }
    if (out.val().is_empty() || out.val() == "Y") && !b.val().is_empty() && b.val() != "Y" {
        out.value = b.value.clone();
    }
    let (da, db) = (a.child("DATE"), b.child("DATE"));
    if let Some(db) = db {
        let better = match da {
            None => true,
            Some(da) => match (width(da.val()), width(db.val())) {
                (Some(x), Some(y)) => y < x,
                (None, Some(_)) => true,
                _ => false,
            },
        };
        if better {
            out.children.retain(|c| c.tag != "DATE");
            out.insert_child(db.clone());
        }
    }
    let (pa, pb) = (a.path_val(&["PLAC"]).unwrap_or(""), b.path_val(&["PLAC"]).unwrap_or(""));
    if !pb.is_empty() && pb.len() > pa.len() {
        out.children.retain(|c| c.tag != "PLAC");
        out.insert_child(b.child("PLAC").unwrap().clone());
    }
    for c in b.children.iter().filter(|c| !matches!(c.tag.as_str(), "DATE" | "PLAC")) {
        add_unique(&mut out, c.clone());
    }
    out
}

/// Merges `b` into `a` for a fact a person can have several times (occupation, residence…): an
/// identical one is skipped, one with the same value, date and place gets the other's
/// substructures, any other is added.
fn merge_multi(a: &mut Node, c: &Node) {
    if a.children.contains(c) {
        return;
    }
    let same = |x: &Node| {
        x.tag == c.tag
            && event_label(&x.tag).is_some()
            && x.val() == c.val()
            && x.path_val(&["DATE"]) == c.path_val(&["DATE"])
            && x.path_val(&["PLAC"]).map(first_level) == c.path_val(&["PLAC"]).map(first_level)
    };
    if let Some(x) = a.children.iter_mut().find(|x| same(x)) {
        for s in &c.children {
            add_unique(x, s.clone());
        }
        return;
    }
    a.insert_child(c.clone());
}

/// Links of a person: families, events attended, other records pointing to them.
fn links(g: &Gedcom, x: &str) -> usize {
    let own = g.get(x).map_or(0, |n| n.children.len());
    g.referrers(x).len() * 3 + own
}

impl Gedcom {
    /// Which of the two individuals is kept: the root person, otherwise the one with the most
    /// links, otherwise the first one in the file.
    pub fn merge_survivor(&self, a: &str, b: &str) -> (String, String) {
        let root = self.root();
        let keep_b = if root.as_deref() == Some(b) {
            true
        } else if root.as_deref() == Some(a) {
            false
        } else {
            links(self, b) > links(self, a)
        };
        if keep_b { (b.to_string(), a.to_string()) } else { (a.to_string(), b.to_string()) }
    }

    /// Prepares the merge of `a` and `b`: what is the same, what is combined, what conflicts.
    pub fn merge_plan(&self, a: &str, b: &str) -> Result<MergePlan, String> {
        if a == b {
            return Err(t!("duplicates.exclusion.same_person").into());
        }
        let (keep, other) = self.merge_survivor(a, b);
        let (Some(k), Some(o)) = (self.get(&keep), self.get(&other)) else {
            return Err(t!("duplicates.exclusion.not_individuals").into());
        };
        if k.tag != "INDI" || o.tag != "INDI" {
            return Err(t!("duplicates.exclusion.not_individuals").into());
        }
        let mut rows = Vec::new();
        let mut conflicts = Vec::new();
        for tag in SINGLE {
            let (x, y) = (k.child(tag), o.child(tag));
            let row = |kind, keep: Option<&Node>, other: Option<&Node>| Row {
                label: label_of(tag),
                kind,
                keep: keep.map(describe).into_iter().collect(),
                other: other.map(describe).into_iter().collect(),
            };
            match (x, y) {
                (None, None) => {}
                (Some(x), Some(y)) if x == y || (describe(x) == describe(y) && compatible(x, y)) => {
                    rows.push(row(if x == y { RowKind::Same } else { RowKind::Combined }, Some(x), Some(y)))
                }
                (Some(x), Some(y)) if *tag == "SEX" && compatible(x, y) => {
                    rows.push(row(RowKind::Combined, Some(x), Some(y)))
                }
                (Some(x), Some(y)) if *tag != "NAME" && compatible(x, y) => {
                    rows.push(row(RowKind::Combined, Some(x), Some(y)))
                }
                (Some(x), Some(y)) => {
                    rows.push(row(RowKind::Conflict(conflicts.len()), Some(x), Some(y)));
                    conflicts.push(Conflict { tag: tag.to_string(), keep: describe(x), other: describe(y) });
                }
                (x, y) => rows.push(row(RowKind::Added, x, y)),
            }
        }
        // Other facts: all kept.
        let mut tags: Vec<&str> = Vec::new();
        for n in k.children.iter().chain(o.children.iter()) {
            if event_label(&n.tag).is_some() && !SINGLE.contains(&n.tag.as_str()) && !tags.contains(&n.tag.as_str()) {
                tags.push(&n.tag);
            }
        }
        for tag in tags {
            let of = |r: &Node| r.children_with(tag).map(describe).collect::<Vec<_>>();
            let (x, y) = (of(k), of(o));
            let kind = if x == y { RowKind::Same } else { RowKind::Added };
            rows.push(Row { label: label_of(tag), kind, keep: x, other: y });
        }
        Ok(MergePlan { keep, other, rows, conflicts })
    }

    /// Merges `plan.other` into `plan.keep`; `choices` gives, for each conflict, the value kept.
    /// Returns the kept individual.
    pub fn merge_individuals(&mut self, plan: &MergePlan, choices: &[Side]) -> Result<String, String> {
        if choices.len() != plan.conflicts.len() {
            return Err(t!("merge.error.choices").into());
        }
        let (keep, other) = (plan.keep.as_str(), plan.other.as_str());
        let (Some(k), Some(o)) = (self.get(keep).cloned(), self.get(other).cloned()) else {
            return Err(t!("duplicates.exclusion.not_individuals").into());
        };
        if k.tag != "INDI" || o.tag != "INDI" || keep == other {
            return Err(t!("duplicates.exclusion.not_individuals").into());
        }
        let choice = |tag: &str| plan.conflicts.iter().position(|c| c.tag == tag).map(|i| choices[i]);
        let mut out = k.clone();
        // Facts a person has once: the first of each.
        for tag in SINGLE {
            let (x, y) = (k.child(tag), o.child(tag));
            let merged = match (x, y) {
                (_, None) => continue,
                (None, Some(y)) => y.clone(),
                (Some(x), Some(y)) => match choice(tag) {
                    Some(Side::Keep) => x.clone(),
                    Some(Side::Other) => y.clone(),
                    None if compatible(x, y) => combine(x, y),
                    None => x.clone(),
                },
            };
            match out.children.iter().position(|c| c.tag == *tag) {
                Some(i) => out.children[i] = merged,
                None => out.insert_child(merged),
            }
        }
        // Everything else from the other record.
        let firsts: Vec<usize> = SINGLE.iter().filter_map(|t| o.children.iter().position(|c| c.tag == *t)).collect();
        for (i, c) in o.children.iter().enumerate() {
            if firsts.contains(&i) || SKIPPED.contains(&c.tag.as_str()) {
                continue;
            }
            if c.tag == DISTINCT_TAG && c.pointer() == Some(keep) {
                continue;
            }
            merge_multi(&mut out, c);
        }
        out.touch();
        self.replace(keep, out)?;
        self.rename_pointers(other, keep);
        self.delete(other);
        self.tidy_after_merge(keep);
        Ok(keep.to_string())
    }

    /// After a merge: no family link twice, no link to oneself, families with the same spouses
    /// merged.
    fn tidy_after_merge(&mut self, x: &str) {
        let p = format!("@{x}@");
        if let Some(n) = self.get_mut(x) {
            // Pointers to oneself: “not a duplicate” of oneself, witness of one's own event.
            n.children.retain(|c| !(c.tag == DISTINCT_TAG && c.val() == p));
            for ev in n.children.iter_mut().filter(|c| event_label(&c.tag).is_some()) {
                ev.children.retain(|a| !(a.tag == "ASSO" && a.val() == p));
            }
            dedupe_links(n, &["FAMC", "FAMS"]);
        }
        let fams: Vec<String> =
            self.individual(x).map(|i| i.fams().iter().map(|s| s.to_string()).collect()).unwrap_or_default();
        let famc: Vec<String> =
            self.individual(x).map(|i| i.famc().iter().map(|s| s.to_string()).collect()).unwrap_or_default();
        for f in fams.iter().chain(famc.iter()) {
            if let Some(fam) = self.get_mut(f) {
                dedupe_links(fam, &["CHIL", "HUSB", "WIFE"]);
                // A spouse is not a witness of their own union.
                if fams.contains(f) {
                    for ev in fam.children.iter_mut().filter(|c| event_label(&c.tag).is_some()) {
                        ev.children.retain(|a| !(a.tag == "ASSO" && a.val() == p));
                    }
                }
            }
        }
        // Two families of the same couple: one.
        let couple = |g: &Gedcom, f: &str| {
            let fam = g.family(f)?;
            Some((fam.husband()?.to_string(), fam.wife()?.to_string()))
        };
        let mut seen: Vec<(String, (String, String))> = Vec::new();
        for f in fams {
            let Some(c) = couple(self, &f) else { continue };
            match seen.iter().find(|(_, s)| *s == c).map(|(f, _)| f.clone()) {
                Some(first) => self.merge_families(&first, &f),
                None => seen.push((f, c)),
            }
        }
    }

    /// Merges family `other` into `keep` (same spouses).
    fn merge_families(&mut self, keep: &str, other: &str) {
        let Some(o) = self.get(other).cloned() else { return };
        let Some(k) = self.get_mut(keep) else { return };
        for c in o.children.iter().filter(|c| !SKIPPED.contains(&c.tag.as_str())) {
            // The same union (marriage, divorce…, of the same type) described twice: one event.
            let twin = SINGLE_FAM.contains(&c.tag.as_str()).then(|| {
                k.children
                    .iter()
                    .position(|x| x.tag == c.tag && x.child_val("TYPE") == c.child_val("TYPE") && compatible(x, c))
            });
            match twin.flatten() {
                Some(i) => k.children[i] = combine(&k.children[i], c),
                None => merge_multi(k, c),
            }
        }
        dedupe_links(k, &["CHIL", "HUSB", "WIFE"]);
        k.touch();
        self.rename_pointers(other, keep);
        self.delete(other);
        // Children and spouses now list the same family twice.
        let members: Vec<String> = self.get(keep).map(|f| f.pointers()).unwrap_or_default();
        for m in members {
            if let Some(n) = self.get_mut(&m).filter(|n| n.tag == "INDI") {
                dedupe_links(n, &["FAMC", "FAMS"]);
            }
        }
    }
}

/// Keeps one line per pointer for these tags, gathering the substructures of the duplicates.
fn dedupe_links(n: &mut Node, tags: &[&str]) {
    let mut out: Vec<Node> = Vec::with_capacity(n.children.len());
    for c in std::mem::take(&mut n.children) {
        if tags.contains(&c.tag.as_str())
            && c.pointer().is_some()
            && let Some(first) = out.iter_mut().find(|x| x.tag == c.tag && x.value == c.value)
        {
            for s in c.children {
                add_unique(first, s);
            }
            continue;
        }
        out.push(c);
    }
    n.children = out;
}
