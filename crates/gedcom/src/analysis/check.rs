//! Consistency check of the tree, like Gramps' “Verify the Data” tool, RootsMagic's “Problem
//! Search” or Heredis' “inconsistencies”.
//!
//! Problems are only reported when they are certain given how imprecise the dates are (see
//! [`Span`]). A checked problem can be hidden by adding `_GR_IGNORE <code>` under the record.

use super::span::{Age, Span};
use crate::document::{Gedcom, RecordKind};
use crate::model::{Individual, event_label};
use crate::node::Node;
use rust_i18n::t;
use std::borrow::Cow;
use std::collections::{HashMap, HashSet};

/// Extension tag that hides a problem (value = problem code).
pub const IGNORE_TAG: &str = "_GR_IGNORE";
pub const IGNORE_URI: &str = extension_uri!("_gr_ignore");

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Severity {
    Error,
    Warning,
    Info,
}

impl Severity {
    pub fn label(self) -> Cow<'static, str> {
        match self {
            Severity::Error => t!("check.severity.error"),
            Severity::Warning => t!("check.severity.warning"),
            Severity::Info => t!("check.severity.info"),
        }
    }
}

/// A problem found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Issue {
    pub severity: Severity,
    /// Stable identifier of the kind of problem (used by `_GR_IGNORE`).
    pub code: &'static str,
    /// Main record (the one opened on click).
    pub xref: String,
    /// Other records involved.
    pub related: Vec<String>,
    pub message: String,
    /// Second line under the message; empty: the label of the code.
    pub detail: String,
    /// Position of the faulty structure in the record (indices of the substructures from the
    /// record); empty for the whole record.
    pub path: Vec<usize>,
    /// Problem hidden by `_GR_IGNORE`.
    pub ignored: bool,
}

/// Thresholds used by the check (defaults close to Gramps').
#[derive(Debug, Clone)]
pub struct Thresholds {
    pub max_lifespan: f64,
    pub min_parent_age: f64,
    pub max_father_age: f64,
    pub max_mother_age: f64,
    pub min_marriage_age: f64,
    pub max_marriage_age_gap: f64,
}

impl Default for Thresholds {
    fn default() -> Self {
        Self {
            max_lifespan: 110.,
            min_parent_age: 14.,
            max_father_age: 80.,
            max_mother_age: 55.,
            min_marriage_age: 14.,
            max_marriage_age_gap: 30.,
        }
    }
}

/// Label of a problem code.
pub fn code_label(code: &str) -> Cow<'static, str> {
    match code {
        "death_before_birth" => t!("check.code.death_before_birth"),
        "burial_before_death" => t!("check.code.burial_before_death"),
        "event_before_birth" => t!("check.code.event_before_birth"),
        "event_after_death" => t!("check.code.event_after_death"),
        "too_old" => t!("check.code.too_old"),
        "parent_too_young" => t!("check.code.parent_too_young"),
        "father_too_old" => t!("check.code.father_too_old"),
        "mother_too_old" => t!("check.code.mother_too_old"),
        "birth_after_mother_death" => t!("check.code.birth_after_mother_death"),
        "birth_after_father_death" => t!("check.code.birth_after_father_death"),
        "child_before_parent" => t!("check.code.child_before_parent"),
        "marriage_too_young" => t!("check.code.marriage_too_young"),
        "marriage_after_death" => t!("check.code.marriage_after_death"),
        "marriage_age_gap" => t!("check.code.marriage_age_gap"),
        "siblings_too_close" => t!("check.code.siblings_too_close"),
        "husband_not_male" => t!("check.code.husband_not_male"),
        "wife_not_female" => t!("check.code.wife_not_female"),
        "own_ancestor" => t!("check.code.own_ancestor"),
        "broken_link" => t!("check.code.broken_link"),
        "missing_backlink" => t!("check.code.missing_backlink"),
        "unsourced" => t!("check.code.unsourced"),
        "tag_not_allowed" => t!("check.code.tag_not_allowed"),
        "missing_required" => t!("check.code.missing_required"),
        "too_many" => t!("check.code.too_many"),
        "wrong_pointer" => t!("check.code.wrong_pointer"),
        "invalid_value" => t!("check.code.invalid_value"),
        "empty_fact" => t!("check.code.empty_fact"),
        "empty_note" => t!("check.code.empty_note"),
        "empty_family" => t!("check.code.empty_family"),
        "unused_record" => t!("check.code.unused_record"),
        "disconnected" => t!("check.code.disconnected"),
        "missing_file" => t!("check.code.missing_file"),
        "moved_file" => t!("check.code.moved_file"),
        "duplicate_media" => t!("check.code.duplicate_media"),
        "missing_form" => t!("check.code.missing_form"),
        "unattached_file" => t!("check.code.unattached_file"),
        "missing_coords" => t!("check.code.missing_coords"),
        "bad_coords" => t!("check.code.bad_coords"),
        "place_variant" => t!("check.code.place_variant"),
        "place_hierarchy" => t!("check.code.place_hierarchy"),
        "role_sex_mismatch" => t!("check.code.role_sex_mismatch"),
        "outside_source_period" => t!("check.code.outside_source_period"),
        "possible_duplicate" => t!("check.code.possible_duplicate"),
        _ => t!("check.code.other"),
    }
}

/// Events that can legitimately follow death.
const POST_MORTEM: &[&str] = &["BURI", "CREM", "PROB", "WILL", "EVEN", "FACT", "RESI", "OCCU", "TITL", "RELI", "NATI"];
/// Attributes (not dated events in the strict sense).
const ATTRIBUTES: &[&str] = &["OCCU", "RESI", "EDUC", "RELI", "TITL", "NATI", "FACT"];

fn ignored(node: &Node) -> HashSet<String> {
    let mut out = HashSet::new();
    node.walk(&mut |n| {
        if n.tag == IGNORE_TAG {
            out.insert(n.val().trim().to_string());
        }
    });
    out
}

struct Life {
    birth: Option<Span>,
    death: Option<Span>,
}

fn span_of(node: &Node, tag: &str) -> Option<Span> {
    node.children_with(tag).find_map(|e| e.child_val("DATE").and_then(Span::parse))
}

fn life(ind: &Node) -> Life {
    Life {
        birth: span_of(ind, "BIRT").or_else(|| span_of(ind, "CHR")).or_else(|| span_of(ind, "BAPM")),
        death: span_of(ind, "DEAT").or_else(|| span_of(ind, "BURI")).or_else(|| span_of(ind, "CREM")),
    }
}

fn years(x: f64) -> String {
    if x < 1. { t!("check.less_than_a_year").into() } else { tn!("check.years", x.floor() as usize).into() }
}

/// Runs every consistency check, without the hidden problems.
pub fn check(g: &Gedcom, t: &Thresholds) -> Vec<Issue> {
    let mut issues = check_all(g, t);
    issues.retain(|i| !i.ignored);
    issues
}

/// Marks the problems hidden by `_GR_IGNORE` under their record.
pub(crate) fn mark_ignored(g: &Gedcom, issues: &mut [Issue]) {
    let mut cache: HashMap<String, HashSet<String>> = HashMap::new();
    for i in issues {
        let ign = cache.entry(i.xref.clone()).or_insert_with(|| g.get(&i.xref).map(ignored).unwrap_or_default());
        i.ignored = ign.contains(i.code);
    }
}

/// Consistency checks, hidden problems included (marked `ignored`).
pub(crate) fn check_all(g: &Gedcom, t: &Thresholds) -> Vec<Issue> {
    let mut issues: Vec<Issue> = Vec::new();
    let name = |x: &str| g.record_label(x);
    let lives: HashMap<&str, Life> =
        g.records_of(RecordKind::Individual).filter_map(|r| Some((r.xref.as_deref()?, life(r)))).collect();
    let mut push = |severity, code, xref: &str, related: Vec<String>, message: String| {
        issues.push(Issue {
            severity,
            code,
            xref: xref.to_string(),
            related,
            message,
            detail: String::new(),
            path: Vec::new(),
            ignored: false,
        });
    };

    // Broken links (every record).
    for r in &g.records {
        let Some(x) = r.xref.as_deref() else { continue };
        let mut seen = HashSet::new();
        r.walk(&mut |n| {
            if let Some(p) = n.pointer()
                && p != "VOID"
                && g.get(p).is_none()
                && seen.insert(p.to_string())
            {
                push(
                    Severity::Error,
                    "broken_link",
                    x,
                    vec![p.to_string()],
                    t!("check.msg.broken_link", who = name(x), tag = n.tag, target = format!("@{p}@")).into(),
                );
            }
        });
    }

    // Individus.
    for r in g.records_of(RecordKind::Individual) {
        let Some(x) = r.xref.as_deref() else { continue };
        let ind = Individual::new(r);
        let l = &lives[x];
        let who = ind.display_name();

        if let (Some(b), Some(d)) = (&l.birth, &l.death) {
            if d.surely_before(b) {
                push(
                    Severity::Error,
                    "death_before_birth",
                    x,
                    vec![],
                    t!("check.msg.death_before_birth", who = who).into(),
                );
            } else if let Some(min) = Age::between(b, d).min_years()
                && min > t.max_lifespan
            {
                push(
                    Severity::Warning,
                    "too_old",
                    x,
                    vec![],
                    t!("check.msg.too_old", who = who, years = years(min)).into(),
                );
            }
        }
        if let (Some(death), Some(buri)) = (span_of(r, "DEAT"), span_of(r, "BURI"))
            && buri.surely_before(&death)
        {
            push(
                Severity::Error,
                "burial_before_death",
                x,
                vec![],
                t!("check.msg.burial_before_death", who = who).into(),
            );
        }
        for e in &r.children {
            let Some(label) = event_label(&e.tag) else { continue };
            if matches!(e.tag.as_str(), "BIRT" | "CHR" | "BAPM" | "DEAT" | "BURI" | "CREM") {
                continue;
            }
            let Some(s) = e.child_val("DATE").and_then(Span::parse) else { continue };
            if let Some(b) = &l.birth
                && s.surely_before(b)
            {
                push(
                    Severity::Error,
                    "event_before_birth",
                    x,
                    vec![],
                    t!("check.msg.event_before_birth", who = who, event = label).into(),
                );
            }
            if let Some(d) = &l.death
                && d.surely_before(&s)
                && !POST_MORTEM.contains(&e.tag.as_str())
            {
                push(
                    Severity::Error,
                    "event_after_death",
                    x,
                    vec![],
                    t!("check.msg.event_after_death", who = who, event = label).into(),
                );
            }
        }
        // Individual events without a source: information only.
        for e in &r.children {
            if let Some(label) = event_label(&e.tag)
                && !ATTRIBUTES.contains(&e.tag.as_str())
                && e.child("SOUR").is_none()
            {
                push(
                    Severity::Info,
                    "unsourced",
                    x,
                    vec![],
                    t!("check.msg.unsourced", who = who, event = label).into(),
                );
            }
        }
        // FAMC/FAMS without their counterpart.
        for (tag, back) in [("FAMC", "CHIL"), ("FAMS", "HUSB")] {
            for f in r.children_with(tag).filter_map(|n| n.pointer()) {
                let Some(fam) = g.get(f) else { continue };
                let listed = fam
                    .children
                    .iter()
                    .any(|c| (c.tag == back || (back == "HUSB" && c.tag == "WIFE")) && c.pointer() == Some(x));
                if !listed {
                    push(
                        Severity::Warning,
                        "missing_backlink",
                        x,
                        vec![f.to_string()],
                        t!("check.msg.missing_backlink", who = who, family = name(f)).into(),
                    );
                }
            }
        }
    }

    // Familles.
    for r in g.records_of(RecordKind::Family) {
        let Some(fx) = r.xref.as_deref() else { continue };
        let husb = r.child("HUSB").and_then(|n| n.pointer()).filter(|p| lives.contains_key(p));
        let wife = r.child("WIFE").and_then(|n| n.pointer()).filter(|p| lives.contains_key(p));
        let children: Vec<&str> =
            r.children_with("CHIL").filter_map(|n| n.pointer()).filter(|p| lives.contains_key(p)).collect();

        if let Some(h) = husb
            && g.individual(h).is_some_and(|i| i.sex() == "F")
        {
            push(
                Severity::Warning,
                "husband_not_male",
                fx,
                vec![h.into()],
                t!("check.msg.husband_not_male", who = name(h)).into(),
            );
        }
        if let Some(w) = wife
            && g.individual(w).is_some_and(|i| i.sex() == "M")
        {
            push(
                Severity::Warning,
                "wife_not_female",
                fx,
                vec![w.into()],
                t!("check.msg.wife_not_female", who = name(w)).into(),
            );
        }

        // Mariage.
        let marr = span_of(r, "MARR");
        for p in [husb, wife].into_iter().flatten() {
            let l = &lives[p];
            if let Some(m) = &marr {
                if let Some(b) = &l.birth
                    && let Some(max) = Age::between(b, m).max_years()
                    && max < t.min_marriage_age
                {
                    let code = if max < 0. { "event_before_birth" } else { "marriage_too_young" };
                    push(
                        Severity::Warning,
                        code,
                        p,
                        vec![fx.into()],
                        t!("check.msg.marriage_too_young", who = name(p), years = years(max.max(0.))).into(),
                    );
                }
                if let Some(d) = &l.death
                    && d.surely_before(m)
                {
                    push(
                        Severity::Error,
                        "marriage_after_death",
                        fx,
                        vec![p.into()],
                        t!("check.msg.marriage_after_death", who = name(p)).into(),
                    );
                }
            }
        }
        if let (Some(h), Some(w)) = (husb, wife)
            && let (Some(bh), Some(bw)) = (&lives[h].birth, &lives[w].birth)
        {
            // Certain minimal gap between the two births.
            let a = Age::between(bh, bw);
            let gap = match (a.min_years(), a.max_years()) {
                (Some(min), _) if min > 0. => min,
                (_, Some(max)) if max < 0. => -max,
                _ => 0.,
            };
            if gap > t.max_marriage_age_gap {
                push(
                    Severity::Info,
                    "marriage_age_gap",
                    fx,
                    vec![h.into(), w.into()],
                    t!("check.msg.marriage_age_gap", family = name(fx), years = years(gap)).into(),
                );
            }
        }

        // Parents / enfants.
        for &c in &children {
            let Some(cb) = lives[c].birth else { continue };
            for (p, is_father) in [(husb, true), (wife, false)] {
                let Some(p) = p else { continue };
                let pl = &lives[p];
                if let Some(pb) = &pl.birth {
                    let age = Age::between(pb, &cb);
                    if let Some(max) = age.max_years() {
                        if max < 0. {
                            push(
                                Severity::Error,
                                "child_before_parent",
                                c,
                                vec![p.into()],
                                t!("check.msg.child_before_parent", child = name(c), parent = name(p)).into(),
                            );
                        } else if max < t.min_parent_age {
                            push(
                                Severity::Warning,
                                "parent_too_young",
                                c,
                                vec![p.into()],
                                t!("check.msg.parent_too_young", parent = name(p), years = years(max), child = name(c))
                                    .into(),
                            );
                        }
                    }
                    let limit = if is_father { t.max_father_age } else { t.max_mother_age };
                    if let Some(min) = age.min_years()
                        && min > limit
                    {
                        let code = if is_father { "father_too_old" } else { "mother_too_old" };
                        push(
                            Severity::Warning,
                            code,
                            c,
                            vec![p.into()],
                            t!("check.msg.parent_too_old", parent = name(p), years = years(min), child = name(c))
                                .into(),
                        );
                    }
                }
                if let Some(pd) = &pl.death {
                    // The mother must be alive; the father can have died up to ~10 months before.
                    let late =
                        if is_father { Span { lo: pd.lo.map(|d| d + 300), hi: pd.hi.map(|d| d + 300) } } else { *pd };
                    if late.surely_before(&cb) {
                        let code = if is_father { "birth_after_father_death" } else { "birth_after_mother_death" };
                        push(
                            Severity::Error,
                            code,
                            c,
                            vec![p.into()],
                            t!("check.msg.birth_after_death", child = name(c), parent = name(p)).into(),
                        );
                    }
                }
            }
        }
        // Births too close together (twins excepted) for the same mother.
        let mut births: Vec<(&str, i64)> = children
            .iter()
            .filter_map(|c| {
                let b = lives[c].birth?;
                (b.lo? == b.hi?).then_some((*c, b.lo?))
            })
            .collect();
        births.sort_by_key(|b| b.1);
        for w in births.windows(2) {
            let gap = w[1].1 - w[0].1;
            if gap > 2 && gap < 240 {
                push(
                    Severity::Warning,
                    "siblings_too_close",
                    w[1].0,
                    vec![w[0].0.into(), fx.into()],
                    t!("check.msg.siblings_too_close", first = name(w[0].0), second = name(w[1].0), days = gap).into(),
                );
            }
        }
    }

    // Citations: role and sex, period declared by the source (`DATA` / `EVEN` / `DATE`).
    for r in g.records_of(RecordKind::Individual).chain(g.records_of(RecordKind::Family)) {
        let Some(x) = r.xref.as_deref() else { continue };
        let sex = (r.tag == "INDI").then(|| Individual::new(r).sex().to_string());
        for e in &r.children {
            for c in e.children_with("SOUR") {
                let cited_ev = c.child("EVEN");
                if let (Some(sex), Some(role)) = (&sex, cited_ev.and_then(|ev| ev.child_val("ROLE"))) {
                    let wrong = match role {
                        "FATH" | "HUSB" => sex == "F",
                        "MOTH" | "WIFE" => sex == "M",
                        _ => false,
                    };
                    if wrong {
                        let src = c.pointer().unwrap_or("");
                        push(
                            Severity::Warning,
                            "role_sex_mismatch",
                            x,
                            vec![src.to_string()],
                            if sex == "F" {
                                t!(
                                    "check.msg.role_sex_female",
                                    who = name(x),
                                    role = role_label(role),
                                    source = name(src)
                                )
                            } else {
                                t!(
                                    "check.msg.role_sex_male",
                                    who = name(x),
                                    role = role_label(role),
                                    source = name(src)
                                )
                            }
                            .into(),
                        );
                    }
                }
                let Some(src) = c.pointer().and_then(|p| g.get(p)) else { continue };
                let tag = cited_ev.map(|ev| ev.val()).filter(|v| !v.is_empty()).unwrap_or(&e.tag);
                let Some(date) = c.path_val(&["DATA", "DATE"]).or_else(|| e.child_val("DATE")) else { continue };
                if let Some(period) = crate::kinds::outside_period(src, tag, date) {
                    let sx = src.xref.as_deref().unwrap_or("");
                    push(
                        Severity::Warning,
                        "outside_source_period",
                        x,
                        vec![sx.to_string()],
                        t!(
                            "check.msg.outside_source_period",
                            who = name(x),
                            date = crate::date::Date::parse(date).display(),
                            source = name(sx),
                            period = crate::date::Date::parse(period).display()
                        )
                        .into(),
                    );
                }
            }
        }
    }

    // Loops in the ancestry.
    for x in lives.keys() {
        if is_own_ancestor(g, x) {
            push(Severity::Error, "own_ancestor", x, vec![], t!("check.msg.own_ancestor", who = name(x)).into());
        }
    }

    mark_ignored(g, &mut issues);
    issues.sort_by(|a, b| (a.severity, a.code, &a.message).cmp(&(b.severity, b.code, &b.message)));
    issues.dedup();
    issues
}

fn role_label(role: &str) -> Cow<'static, str> {
    match role {
        "FATH" => t!("check.role.father"),
        "MOTH" => t!("check.role.mother"),
        "HUSB" => t!("check.role.husband"),
        "WIFE" => t!("check.role.wife"),
        _ => Cow::Borrowed(""),
    }
}

fn is_own_ancestor(g: &Gedcom, x: &str) -> bool {
    let mut stack: Vec<&str> = parents_of(g, x);
    let mut seen: HashSet<&str> = HashSet::new();
    while let Some(p) = stack.pop() {
        if p == x {
            return true;
        }
        if seen.insert(p) {
            stack.extend(parents_of(g, p));
        }
    }
    false
}

pub(crate) fn parents_of<'a>(g: &'a Gedcom, x: &str) -> Vec<&'a str> {
    let Some(ind) = g.get(x) else { return vec![] };
    ind.children_with("FAMC")
        .filter_map(|n| n.pointer())
        .filter_map(|f| g.get(f))
        .flat_map(|fam| fam.children.iter().filter(|c| c.tag == "HUSB" || c.tag == "WIFE").filter_map(|c| c.pointer()))
        .filter(|p| g.get(p).is_some())
        .collect()
}

impl Gedcom {
    /// Hides a problem: adds `_GR_IGNORE <code>` to the record and declares the extension in the
    /// header. Returns `false` if nothing changed.
    pub fn ignore_issue(&mut self, xref: &str, code: &str) -> bool {
        let Some(r) = self.get_mut(xref) else { return false };
        if r.children_with(IGNORE_TAG).any(|n| n.val() == code) {
            return false;
        }
        r.insert_child(Node::with_value(IGNORE_TAG, code));
        r.touch();
        self.declare_extension(IGNORE_TAG, IGNORE_URI);
        true
    }
}
