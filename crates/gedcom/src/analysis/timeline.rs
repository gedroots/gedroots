//! Chronologie d'une personne avec son âge à chaque événement, enrichie des
//! événements de ses proches (comme MacFamilyTree et Heredis).

use super::span::{Age, Span};
use crate::date::Date;
use crate::document::Gedcom;
use crate::model::Individual;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryKind {
    /// Événement de la personne.
    Own,
    /// Événement de son couple (mariage, divorce…).
    Family,
    /// Événement d'un proche (naissance d'un enfant, décès d'un parent…).
    Relative,
}

#[derive(Debug, Clone)]
pub struct Entry {
    pub kind: EntryKind,
    pub label: String,
    pub date: Option<Date>,
    pub place: Option<String>,
    /// Âge de la personne à cette date.
    pub age: Option<String>,
    /// Personne liée à l'événement (conjoint, enfant…).
    pub related: Option<String>,
    pub sourced: bool,
    sort: (i32, u8, u8),
}

fn pick(sex: &str, m: &'static str, f: &'static str, u: &'static str) -> &'static str {
    match sex {
        "M" => m,
        "F" => f,
        _ => u,
    }
}

/// Construit la chronologie de `xref`, triée par date.
pub fn timeline(g: &Gedcom, xref: &str) -> Vec<Entry> {
    let Some(me) = g.individual(xref) else { return vec![] };
    let birth = me.birth().and_then(|e| e.date_raw().and_then(Span::parse));
    let death = me.death().and_then(|e| e.date_raw().and_then(Span::parse));
    let mut out: Vec<Entry> = Vec::new();
    let mut add = |kind, label: String, node: &crate::node::Node, related: Option<String>| {
        let date = node.child_val("DATE").map(Date::parse);
        let span = date.as_ref().and_then(Span::of);
        // Événements des proches : seulement pendant la vie de la personne.
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
        });
    };

    for e in me.events() {
        let label = match e.value().filter(|v| !v.trim().is_empty() && *v != "Y") {
            Some(v) => format!("{} : {v}", e.label()),
            None => e.label(),
        };
        add(EntryKind::Own, label, e.node, None);
    }

    let name = |x: &str| g.record_label(x);
    // Parents.
    let (father, mother) = g.parents(xref);
    for (p, rel) in [(father, "son père"), (mother, "sa mère")] {
        if let Some(p) = p
            && let Some(d) = p.death()
        {
            add(EntryKind::Relative, format!("Décès de {rel}, {}", p.display_name()), d.node, Some(p.xref().into()));
        }
    }
    // Frères et sœurs.
    if let Some(fam) = me.famc().first().and_then(|f| g.family(f)) {
        for s in fam.children().into_iter().filter(|s| *s != xref).filter_map(|s| g.individual(s)) {
            let rel = pick(s.sex(), "son frère", "sa sœur", "son frère ou sa sœur");
            if let Some(b) = s.event("BIRT") {
                add(EntryKind::Relative, format!("Naissance de {rel} {}", s.given()), b.node, Some(s.xref().into()));
            }
            if let Some(d) = s.event("DEAT") {
                add(EntryKind::Relative, format!("Décès de {rel} {}", s.given()), d.node, Some(s.xref().into()));
            }
        }
    }
    // Couples et enfants.
    for f in me.fams() {
        let Some(fam) = g.family(f) else { continue };
        let spouse = [fam.husband(), fam.wife()].into_iter().flatten().find(|x| *x != xref);
        for e in fam.events() {
            let label = match spouse {
                Some(s) => format!("{} avec {}", e.label(), name(s)),
                None => e.label(),
            };
            add(EntryKind::Family, label, e.node, spouse.map(str::to_string));
        }
        if let Some(s) = spouse.and_then(|s| g.individual(s))
            && let Some(d) = s.event("DEAT")
        {
            let rel = pick(s.sex(), "son époux", "son épouse", "son conjoint");
            add(EntryKind::Relative, format!("Décès de {rel}, {}", s.display_name()), d.node, Some(s.xref().into()));
        }
        for c in fam.children().into_iter().filter_map(|c| g.individual(c)) {
            let rel = pick(c.sex(), "son fils", "sa fille", "son enfant");
            if let Some(b) = c.birth() {
                add(EntryKind::Relative, format!("Naissance de {rel} {}", c.given()), b.node, Some(c.xref().into()));
            }
            if let Some(d) = c.event("DEAT") {
                add(EntryKind::Relative, format!("Décès de {rel} {}", c.given()), d.node, Some(c.xref().into()));
            }
            // Petits-enfants.
            for gf in c.fams().into_iter().filter_map(|f| g.family(f)) {
                for gc in gf.children().into_iter().filter_map(|x| g.individual(x)) {
                    if let Some(b) = gc.birth() {
                        let rel = pick(gc.sex(), "son petit-fils", "sa petite-fille", "son petit-enfant");
                        add(
                            EntryKind::Relative,
                            format!("Naissance de {rel} {}", gc.display_name()),
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

/// Âge de la personne à une date GEDCOM donnée.
pub fn age_at(ind: &Individual<'_>, date: &str) -> Option<String> {
    let b = ind.birth().and_then(|e| e.date_raw().and_then(Span::parse))?;
    Age::between(&b, &Span::parse(date)?).display()
}
