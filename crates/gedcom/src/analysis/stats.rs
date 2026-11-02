//! Tree statistics (Heredis, Geneanet): surnames, given names, longevity, age at marriage,
//! completeness of the ancestry and pedigree collapse.

use super::check::parents_of;
use super::span::{Age, Span};
use crate::document::{Gedcom, RecordKind};
use crate::model::{Individual, short_place};
use std::collections::{BTreeMap, HashMap, HashSet};

#[derive(Debug, Clone, Default)]
pub struct Generation {
    /// Generation 1 = the reference person.
    pub number: u32,
    /// Theoretical slots (2^(n-1)).
    pub expected: u64,
    /// Filled slots (the same ancestor can fill several).
    pub found: u64,
    /// Distinct ancestors.
    pub distinct: u64,
}

impl Generation {
    pub fn completeness(&self) -> f64 {
        self.found as f64 / self.expected as f64
    }
}

#[derive(Debug, Clone, Default)]
pub struct Century {
    /// Century of birth, e.g. 1800 for the 19th.
    pub start: i32,
    pub births: usize,
    /// Average age at death (people born in this century).
    pub mean_lifespan: Option<f64>,
    pub deaths_counted: usize,
}

#[derive(Debug, Clone, Default)]
pub struct Stats {
    pub individuals: usize,
    pub families: usize,
    pub men: usize,
    pub women: usize,
    pub unknown_sex: usize,
    pub surnames: Vec<(String, usize)>,
    pub given_men: Vec<(String, usize)>,
    pub given_women: Vec<(String, usize)>,
    pub places: Vec<(String, usize)>,
    pub centuries: Vec<Century>,
    /// The longest lives: (xref, years).
    pub longest: Vec<(String, f64)>,
    pub mean_marriage_age_men: Option<f64>,
    pub mean_marriage_age_women: Option<f64>,
    pub mean_children: Option<f64>,
    pub events: usize,
    pub sourced_events: usize,
    /// Ancestry of the reference person.
    pub root: Option<String>,
    pub generations: Vec<Generation>,
    /// Overall pedigree collapse rate of the ancestry (0 = no ancestor twice).
    pub implexe: f64,
}

fn top(map: HashMap<String, usize>, n: usize) -> Vec<(String, usize)> {
    let mut v: Vec<_> = map.into_iter().filter(|(k, _)| !k.is_empty()).collect();
    v.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    v.truncate(n);
    v
}

fn mean(v: &[f64]) -> Option<f64> {
    (!v.is_empty()).then(|| v.iter().sum::<f64>() / v.len() as f64)
}

fn span(ind: &Individual<'_>, birth: bool) -> Option<Span> {
    let e = if birth { ind.birth() } else { ind.death() }?;
    Span::parse(e.date_raw()?)
}

/// Computes the statistics; `root` = reference person for the ancestry.
pub fn stats(g: &Gedcom, root: Option<&str>) -> Stats {
    let mut s = Stats::default();
    let mut surnames = HashMap::new();
    let mut given_m = HashMap::new();
    let mut given_f = HashMap::new();
    let mut places: HashMap<String, usize> = HashMap::new();
    let mut centuries: BTreeMap<i32, (usize, Vec<f64>)> = BTreeMap::new();
    let mut lifespans: Vec<(String, f64)> = Vec::new();

    for r in g.records_of(RecordKind::Individual) {
        let i = Individual::new(r);
        s.individuals += 1;
        let first = i.given().split_whitespace().next().unwrap_or("").to_string();
        match i.sex() {
            "M" => {
                s.men += 1;
                *given_m.entry(first).or_insert(0) += 1;
            }
            "F" => {
                s.women += 1;
                *given_f.entry(first).or_insert(0) += 1;
            }
            _ => s.unknown_sex += 1,
        }
        *surnames.entry(i.surname().to_uppercase()).or_insert(0) += 1;
        for e in i.events() {
            s.events += 1;
            if e.is_sourced() {
                s.sourced_events += 1;
            }
            if let Some(p) = e.place() {
                *places.entry(short_place(p)).or_insert(0) += 1;
            }
        }
        let b = span(&i, true);
        if let Some(y) = b.and_then(|b| b.mid()).map(|d| 1970 + (d as f64 / 365.2425).floor() as i32) {
            let c = centuries.entry(y.div_euclid(100) * 100).or_default();
            c.0 += 1;
            if let (Some(b), Some(d)) = (b, span(&i, false))
                && let Some(age) = Age::between(&b, &d).mid_years()
                && (0. ..130.).contains(&age)
                && b.hi.zip(b.lo).is_some_and(|(h, l)| h - l <= 400)
            {
                c.1.push(age);
                lifespans.push((i.xref().to_string(), age));
            }
        }
    }
    s.surnames = top(surnames, 20);
    s.given_men = top(given_m, 10);
    s.given_women = top(given_f, 10);
    s.places = top(places, 15);
    s.centuries = centuries
        .into_iter()
        .map(|(start, (births, ages))| Century {
            start,
            births,
            mean_lifespan: mean(&ages),
            deaths_counted: ages.len(),
        })
        .collect();
    lifespans.sort_by(|a, b| b.1.total_cmp(&a.1));
    lifespans.truncate(5);
    s.longest = lifespans;

    // Families: age at first marriage, number of children.
    let mut ages_m = Vec::new();
    let mut ages_f = Vec::new();
    let mut kids = Vec::new();
    let mut first_marriage: HashMap<String, (i64, f64)> = HashMap::new();
    for r in g.records_of(RecordKind::Family) {
        s.families += 1;
        let f = crate::model::Family::new(r);
        kids.push(f.children().len() as f64);
        let Some(m) = f.marriage().and_then(|e| e.date_raw().and_then(Span::parse)) else { continue };
        let Some(mid) = m.mid() else { continue };
        for p in [f.husband(), f.wife()].into_iter().flatten() {
            let Some(i) = g.individual(p) else { continue };
            if let Some(b) = span(&i, true)
                && let Some(age) = Age::between(&b, &m).mid_years()
                && (10. ..100.).contains(&age)
            {
                let e = first_marriage.entry(p.to_string()).or_insert((mid, age));
                if mid < e.0 {
                    *e = (mid, age);
                }
            }
        }
    }
    for (x, (_, age)) in first_marriage {
        match g.individual(&x).map(|i| i.sex().to_string()).as_deref() {
            Some("M") => ages_m.push(age),
            Some("F") => ages_f.push(age),
            _ => {}
        }
    }
    s.mean_marriage_age_men = mean(&ages_m);
    s.mean_marriage_age_women = mean(&ages_f);
    s.mean_children = mean(&kids);

    // Ancestry and pedigree collapse.
    if let Some(root) = root.filter(|r| g.individual(r).is_some()) {
        s.root = Some(root.to_string());
        let mut level: Vec<String> = vec![root.to_string()];
        let mut total_found = 0u64;
        let mut all_distinct: HashSet<String> = HashSet::new();
        let mut n = 1u32;
        while !level.is_empty() && n <= 30 {
            let distinct: HashSet<&String> = level.iter().collect();
            s.generations.push(Generation {
                number: n,
                expected: 1u64 << (n - 1),
                found: level.len() as u64,
                distinct: distinct.len() as u64,
            });
            if n > 1 {
                total_found += level.len() as u64;
                all_distinct.extend(level.iter().cloned());
            }
            // Each filled slot yields its two parents (an ancestor present twice counts twice).
            level = level.iter().flat_map(|x| parents_of(g, x)).map(str::to_string).collect();
            n += 1;
        }
        if total_found > 0 {
            s.implexe = 1. - all_distinct.len() as f64 / total_found as f64;
        }
    }
    s
}
