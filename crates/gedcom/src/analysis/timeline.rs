//! Timeline of a person with their age at each event, enriched with their relatives' events (like
//! MacFamilyTree and Heredis).

use super::span::{Age, Span};
use crate::date::Date;
use crate::document::Gedcom;
use crate::model::Individual;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryKind {
    /// Event of the person.
    Own,
    /// Event of their union (marriage, divorce…).
    Family,
    /// Event of a relative (birth of a child, death of a parent…).
    Relative,
}

#[derive(Debug, Clone)]
pub struct Entry {
    pub kind: EntryKind,
    pub label: String,
    pub date: Option<Date>,
    pub place: Option<String>,
    /// Age of the person on this date.
    pub age: Option<String>,
    /// Person linked to the event (spouse, child…).
    pub related: Option<String>,
    pub sourced: bool,
    /// The event itself: the record holding it and its position there, to open it.
    pub event: Option<(String, usize)>,
    sort: (i32, u8, u8),
}

/// Text in the relative's gender: the given key followed by `.m`, `.f` or `.u`.
macro_rules! pick {
    ($sex:expr, $key:literal, $name:expr) => {
        match $sex {
            "M" => rust_i18n::t!(concat!($key, ".m"), name = $name),
            "F" => rust_i18n::t!(concat!($key, ".f"), name = $name),
            _ => rust_i18n::t!(concat!($key, ".u"), name = $name),
        }
        .into_owned()
    };
}

/// Builds the timeline of `xref`, sorted by date.
pub fn timeline(g: &Gedcom, xref: &str) -> Vec<Entry> {
    let Some(me) = g.individual(xref) else { return vec![] };
    let birth = me.birth().and_then(|e| e.date_raw().and_then(Span::parse));
    let death = me.death().and_then(|e| e.date_raw().and_then(Span::parse));
    let mut out: Vec<Entry> = Vec::new();
    let mut add = |kind, label: String, owner: &str, node: &crate::node::Node, related: Option<String>| {
        let date = node.child_val("DATE").map(Date::parse);
        let span = date.as_ref().and_then(Span::of);
        // Relatives' events: only during the person's life.
        if kind == EntryKind::Relative {
            match &span {
                None => return,
                Some(s) => {
                    if birth.is_some_and(|b| s.surely_before(&b)) || death.is_some_and(|d| d.surely_before(s)) {
                        return;
                    }
                }
            }
        }
        let age = match (&birth, &span) {
            (Some(b), Some(s)) if kind != EntryKind::Own || node.tag != "BIRT" => Age::between(b, s).display(),
            _ => None,
        };
        out.push(Entry {
            kind,
            label,
            sort: date.as_ref().map(|d| d.sort_key()).unwrap_or((i32::MAX, 0, 0)),
            date,
            place: node.child_val("PLAC").map(str::to_string),
            age,
            related,
            sourced: node.child("SOUR").is_some(),
            event: g
                .get(owner)
                .and_then(|r| r.children.iter().position(|c| std::ptr::eq(c, node)))
                .map(|i| (owner.to_string(), i)),
        });
    };

    for e in me.events() {
        let label = match e.value().filter(|v| !v.trim().is_empty() && *v != "Y") {
            Some(v) => rust_i18n::t!("timeline.value", label = e.label(), value = v).into_owned(),
            None => e.label(),
        };
        add(EntryKind::Own, label, xref, e.node, None);
    }

    let name = |x: &str| g.record_label(x);
    // Parents.
    let (father, mother) = g.parents(xref);
    for (p, sex) in [(father, "M"), (mother, "F")] {
        if let Some(p) = p
            && let Some(d) = p.death()
        {
            add(
                EntryKind::Relative,
                pick!(sex, "timeline.death_parent", p.display_name()),
                p.xref(),
                d.node,
                Some(p.xref().into()),
            );
        }
    }
    // Brothers and sisters.
    if let Some(fam) = me.famc().first().and_then(|f| g.family(f)) {
        for s in fam.children().into_iter().filter(|s| *s != xref).filter_map(|s| g.individual(s)) {
            if let Some(b) = s.event("BIRT") {
                add(
                    EntryKind::Relative,
                    pick!(s.sex(), "timeline.birth_sibling", s.given()),
                    s.xref(),
                    b.node,
                    Some(s.xref().into()),
                );
            }
            if let Some(d) = s.event("DEAT") {
                add(
                    EntryKind::Relative,
                    pick!(s.sex(), "timeline.death_sibling", s.given()),
                    s.xref(),
                    d.node,
                    Some(s.xref().into()),
                );
            }
        }
    }
    // Unions and children.
    for f in me.fams() {
        let Some(fam) = g.family(f) else { continue };
        let spouse = [fam.husband(), fam.wife()].into_iter().flatten().find(|x| *x != xref);
        for e in fam.events() {
            let label = match spouse {
                Some(s) => rust_i18n::t!("timeline.with", event = e.label(), name = name(s)).into_owned(),
                None => e.label(),
            };
            add(EntryKind::Family, label, f, e.node, spouse.map(str::to_string));
        }
        if let Some(s) = spouse.and_then(|s| g.individual(s))
            && let Some(d) = s.event("DEAT")
        {
            add(
                EntryKind::Relative,
                pick!(s.sex(), "timeline.death_spouse", s.display_name()),
                s.xref(),
                d.node,
                Some(s.xref().into()),
            );
        }
        for c in fam.children().into_iter().filter_map(|c| g.individual(c)) {
            if let Some(b) = c.birth() {
                add(
                    EntryKind::Relative,
                    pick!(c.sex(), "timeline.birth_child", c.given()),
                    c.xref(),
                    b.node,
                    Some(c.xref().into()),
                );
            }
            if let Some(d) = c.event("DEAT") {
                add(
                    EntryKind::Relative,
                    pick!(c.sex(), "timeline.death_child", c.given()),
                    c.xref(),
                    d.node,
                    Some(c.xref().into()),
                );
            }
            // Petits-enfants.
            for gf in c.fams().into_iter().filter_map(|f| g.family(f)) {
                for gc in gf.children().into_iter().filter_map(|x| g.individual(x)) {
                    if let Some(b) = gc.birth() {
                        add(
                            EntryKind::Relative,
                            pick!(gc.sex(), "timeline.birth_grandchild", gc.display_name()),
                            gc.xref(),
                            b.node,
                            Some(gc.xref().into()),
                        );
                    }
                }
            }
        }
    }
    out.sort_by_key(|e| e.sort);
    out
}

/// Age of the person on a given GEDCOM date.
pub fn age_at(ind: &Individual<'_>, date: &str) -> Option<String> {
    let b = ind.birth().and_then(|e| e.date_raw().and_then(Span::parse))?;
    Age::between(&b, &Span::parse(date)?).display()
}
