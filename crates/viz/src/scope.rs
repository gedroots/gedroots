//! Who a visualization is about: a person's ancestors (Sosa numbering) or descendants
//! (d'Aboville numbering), over a number of generations.

use gedcom::document::Gedcom;
use std::collections::{HashMap, HashSet};

pub const MIN_GENERATIONS: u32 = 2;
pub const MAX_GENERATIONS: u32 = 12;
pub const DEFAULT_GENERATIONS: u32 = 8;

/// The person's ancestors or descendants.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Line {
    #[default]
    Ancestors,
    Descendants,
}

impl Line {
    pub const ALL: [Line; 2] = [Line::Ancestors, Line::Descendants];

    /// Its value in the `.ged` (`_GR_LINE`).
    pub fn tag(self) -> &'static str {
        match self {
            Line::Ancestors => "ANCESTORS",
            Line::Descendants => "DESCENDANTS",
        }
    }

    pub fn from_tag(s: &str) -> Line {
        if s.eq_ignore_ascii_case("DESCENDANTS") { Line::Descendants } else { Line::Ancestors }
    }
}

/// The people a visualization shows: from `subject`, the `line`, over `generations` (the subject
/// is generation 1).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Scope {
    pub subject: String,
    pub line: Line,
    pub generations: u32,
}

impl Scope {
    pub fn new(subject: &str, line: Line, generations: u32) -> Scope {
        Scope {
            subject: subject.trim_matches('@').to_string(),
            line,
            generations: generations.clamp(MIN_GENERATIONS, MAX_GENERATIONS),
        }
    }
}

/// A person of the scope, with their number and generation (1 = the subject).
#[derive(Debug, Clone)]
pub struct Member {
    pub xref: String,
    /// Sosa number (ancestors) or d'Aboville number (descendants, “1.2.3”).
    pub number: String,
    pub generation: u32,
    /// Other Sosa numbers of an ancestor found several times (pedigree collapse).
    pub also: Vec<usize>,
    pub sosa: Option<usize>,
}

/// The people of a scope, generation by generation.
pub fn members(g: &Gedcom, scope: &Scope) -> Vec<Member> {
    match scope.line {
        Line::Ancestors => ancestors(g, &scope.subject, scope.generations),
        Line::Descendants => descendants(g, &scope.subject, scope.generations),
    }
}

fn ancestors(g: &Gedcom, subject: &str, generations: u32) -> Vec<Member> {
    let tree = g.ancestry(subject, generations);
    let mut seen: HashMap<String, usize> = HashMap::new();
    let mut out: Vec<Member> = Vec::new();
    for (sosa, x) in tree.iter().enumerate() {
        let Some(x) = x else { continue };
        if let Some(&i) = seen.get(x) {
            out[i].also.push(sosa);
            continue;
        }
        seen.insert(x.clone(), out.len());
        out.push(Member {
            xref: x.clone(),
            number: sosa.to_string(),
            generation: usize::BITS - sosa.leading_zeros(),
            also: vec![],
            sosa: Some(sosa),
        });
    }
    out
}

fn descendants(g: &Gedcom, subject: &str, generations: u32) -> Vec<Member> {
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    let mut queue = vec![(subject.to_string(), "1".to_string(), 1u32)];
    while !queue.is_empty() {
        let mut next = Vec::new();
        for (x, number, gen_) in queue {
            if !seen.insert(x.clone()) {
                continue;
            }
            if gen_ < generations {
                let mut k = 0;
                for (_, kids) in g.spouses_and_children(&x) {
                    for c in kids {
                        k += 1;
                        next.push((c.xref().to_string(), format!("{number}.{k}"), gen_ + 1));
                    }
                }
            }
            out.push(Member { xref: x, number, generation: gen_, also: vec![], sosa: None });
        }
        queue = next;
    }
    out
}
