//! Figures of the tree: the ancestors found in each generation, the age at death, the occupations.

use crate::palette::Palette;
use crate::svg::{esc, open};
use crate::{Figure, Line, Table, TableRow, Tree};
use gedcom::analysis::span::{Age, Span};
use gedcom::model::Event;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fmt::Write;

/// Bars of the ancestors found in each generation: (found, expected).
pub fn completeness(gens: &[(usize, usize)], p: &Palette) -> String {
    let (w, h) = (1000., 300.);
    let bw = w / gens.len().max(6) as f64;
    let mut s = open(w, h);
    for (g, &(found, expected)) in gens.iter().enumerate() {
        let pct = found as f64 / expected.max(1) as f64;
        let x = g as f64 * bw + 14.;
        let inner = bw - 28.;
        let bar = 220.;
        let mid = x + inner / 2.;
        let _ = write!(
            s,
            r##"<rect x="{x:.1}" y="20" width="{inner:.1}" height="{bar}" fill="{}"/><rect x="{x:.1}" y="{:.1}" width="{inner:.1}" height="{:.1}" fill="{}" fill-opacity="0.85"/>"##,
            p.track,
            20. + bar * (1. - pct),
            bar * pct,
            p.accent
        );
        let _ = write!(
            s,
            r##"<text x="{mid:.1}" y="{:.1}" text-anchor="middle" font-size="12" font-weight="600" fill="{ink}">{} %</text><text x="{mid:.1}" y="262" text-anchor="middle" font-size="11" fill="{ink}">{found} / {expected}</text><text x="{mid:.1}" y="282" text-anchor="middle" font-size="10" fill="{}">G{}</text>"##,
            20. + bar * (1. - pct) - 6.,
            (pct * 100.).round(),
            p.muted,
            g + 1,
            ink = p.ink
        );
    }
    s.push_str("</svg>");
    s
}

/// Age at death by year of birth: (birth year, age, sex). Men as squares, women as circles, with
/// the median per half-century.
pub fn longevity(points: &[(i32, f64, char)], p: &Palette) -> String {
    let (w, h, left, base) = (1000., 420., 50., 380.);
    let min = points.iter().map(|p| p.0).min().unwrap_or(1800) / 50 * 50;
    let max = (points.iter().map(|p| p.0).max().unwrap_or(1950) / 50 + 1) * 50;
    let x = |y: f64| left + (y - min as f64) / (max - min) as f64 * (w - left - 20.);
    let y = |a: f64| base - a / 100. * (base - 20.);
    let mut s = open(w, h);
    for a in (0..=100).step_by(20) {
        let yy = y(a as f64);
        let _ = write!(
            s,
            r##"<line x1="{left}" y1="{yy:.1}" x2="{}" y2="{yy:.1}" stroke="{}" stroke-width="0.6"/><text x="{}" y="{:.1}" text-anchor="end" font-size="10" fill="{}">{a}</text>"##,
            w - 20.,
            p.rule,
            left - 8.,
            yy + 4.,
            p.muted
        );
    }
    let mut year = min;
    while year <= max {
        let _ = write!(
            s,
            r##"<text x="{:.1}" y="{}" text-anchor="middle" font-size="10" fill="{}">{year}</text>"##,
            x(year as f64),
            base + 20.,
            p.muted
        );
        year += 50;
    }
    for &(b, a, sex) in points {
        let (px, py) = (x(b as f64), y(a));
        if sex == 'F' {
            let _ = write!(
                s,
                r##"<circle cx="{px:.1}" cy="{py:.1}" r="4" fill="none" stroke="{}" stroke-width="1.4"/>"##,
                p.maternal
            );
        } else {
            let _ = write!(
                s,
                r##"<rect x="{:.1}" y="{:.1}" width="7" height="7" fill="{}" fill-opacity="0.75"/>"##,
                px - 3.5,
                py - 3.5,
                p.paternal
            );
        }
    }
    let mut by: BTreeMap<i32, Vec<f64>> = Default::default();
    for &(b, a, _) in points {
        by.entry(b.div_euclid(50) * 50).or_default().push(a);
    }
    let line: Vec<(f64, f64)> = by
        .into_iter()
        .filter(|(_, v)| v.len() >= 5)
        .map(|(k, mut v)| {
            v.sort_by(f64::total_cmp);
            (x(k as f64 + 25.), y(v[v.len() / 2]))
        })
        .collect();
    if line.len() >= 2 {
        s.push_str(r##"<path d="M"##);
        for (i, (px, py)) in line.iter().enumerate() {
            let _ = write!(s, "{}{px:.1},{py:.1}", if i > 0 { " L" } else { "" });
        }
        let _ = write!(s, r##"" fill="none" stroke="{}" stroke-width="2"/>"##, p.ink);
    }
    s.push_str("</svg>");
    s
}

/// Horizontal bars of the most frequent occupations: (occupation, people).
pub fn occupations(jobs: &[(String, usize)], p: &Palette) -> String {
    let (w, row, left) = (1000., 30., 260.);
    let h = jobs.len().max(1) as f64 * row + 10.;
    let max = jobs.first().map(|j| j.1).unwrap_or(1).max(1) as f64;
    let mut s = open(w, h);
    for (i, (job, n)) in jobs.iter().enumerate() {
        let yy = 5. + i as f64 * row + row / 2.;
        let bw = (*n as f64 / max) * (w - left - 80.);
        let _ = write!(
            s,
            r##"<text x="{}" y="{:.1}" text-anchor="end" font-size="13" fill="{}">{}</text><rect x="{left}" y="{:.1}" width="{:.1}" height="12" rx="3" fill="{}" fill-opacity="0.75"/><text x="{:.1}" y="{:.1}" font-size="12" fill="{}">{n}</text>"##,
            left - 14.,
            yy + 4.5,
            p.ink,
            esc(job),
            yy - 6.,
            bw.max(2.),
            p.accent,
            left + bw + 8.,
            yy + 4.5,
            p.muted
        );
    }
    s.push_str("</svg>");
    s
}

/// The age in whole years between two events, when it is known to the year.
pub fn age_years(birth: &Event, at: &Event) -> Option<u32> {
    let b = Span::parse(birth.date_raw()?)?;
    let a = Span::parse(at.date_raw()?)?;
    let age = Age::between(&b, &a);
    let (lo, hi) = (age.min_years()?, age.max_years()?);
    (hi - lo < 1.5 && lo >= 0.).then(|| ((lo + hi) / 2.).floor() as u32)
}

fn upper_first(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
        None => String::new(),
    }
}

/// Fewest ages at death for the longevity chart.
pub const MIN_AGES: usize = 8;

impl Tree<'_> {
    /// Ancestors found in each generation, against the number each generation has (ancestors
    /// only).
    pub fn generations(&self, p: &Palette) -> Option<Figure> {
        if self.scope.line != Line::Ancestors {
            return None;
        }
        let found: Vec<(usize, usize)> = (1..=self.scope.generations.min(12))
            .map(|n| {
                let lo = 1usize << (n - 1);
                let hi = 1usize << n;
                let count = self
                    .members
                    .iter()
                    .map(|m| m.sosa.iter().chain(m.also.iter()).filter(|s| (lo..hi).contains(*s)).count())
                    .sum();
                (count, hi - lo)
            })
            .collect();
        let mut table =
            Table::new(&[t!("viz.col.generation"), t!("viz.col.found"), t!("viz.col.expected"), t!("viz.col.share")]);
        for (i, &(f, e)) in found.iter().enumerate() {
            table.rows.push(TableRow {
                cells: vec![
                    (i + 1).to_string(),
                    f.to_string(),
                    e.to_string(),
                    t!("viz.percent", n = (f as f64 / e.max(1) as f64 * 100.).round()).into_owned(),
                ],
                record: None,
            });
        }
        Some(Figure {
            svg: completeness(&found, p),
            legend: Vec::new(),
            caption: String::new(),
            count: found.len(),
            table,
        })
    }

    /// Age at death by year of birth, when at least [`MIN_AGES`] are known to the year.
    pub fn longevity(&self, p: &Palette) -> Option<Figure> {
        let g = self.g;
        let mut ages = Vec::new();
        let mut table = Table::new(&[t!("viz.col.person"), t!("viz.col.birth"), t!("viz.col.age")]);
        for m in &self.members {
            let Some(ind) = g.individual(&m.xref) else { continue };
            if let (Some(b), Some(d)) = (ind.birth(), ind.event("DEAT"))
                && let (Some(by), Some(age)) = (ind.birth_year(), age_years(&b, &d))
                && age < 110
            {
                ages.push((by, age as f64, ind.sex().chars().next().unwrap_or('U')));
                table.rows.push(TableRow {
                    cells: vec![ind.display_name(), by.to_string(), age.to_string()],
                    record: Some(m.xref.clone()),
                });
            }
        }
        if ages.len() < MIN_AGES {
            return None;
        }
        Some(Figure {
            svg: longevity(&ages, p),
            legend: Vec::new(),
            caption: t!("viz.longevity_caption", n = ages.len()).into_owned(),
            count: ages.len(),
            table,
        })
    }

    /// The most frequent occupations, held by at least two people: (occupation, people), at most
    /// ten.
    pub fn jobs(&self) -> Vec<(String, usize)> {
        let g = self.g;
        let mut jobs: HashMap<String, usize> = HashMap::new();
        for m in &self.members {
            let Some(ind) = g.individual(&m.xref) else { continue };
            let mine: HashSet<String> = ind
                .events()
                .iter()
                .filter(|e| e.tag() == "OCCU")
                .filter_map(|e| e.value())
                .map(|v| upper_first(v.trim()))
                .collect();
            for j in mine {
                *jobs.entry(j).or_default() += 1;
            }
        }
        let mut jobs: Vec<(String, usize)> = jobs.into_iter().filter(|(_, n)| *n >= 2).collect();
        jobs.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
        jobs.truncate(10);
        jobs
    }

    /// The most frequent occupations ([`Tree::jobs`]), when there are some.
    pub fn occupations(&self, p: &Palette) -> Option<Figure> {
        let jobs = self.jobs();
        if jobs.is_empty() {
            return None;
        }
        let mut table = Table::new(&[t!("viz.col.occupation"), t!("viz.col.people")]);
        for (j, n) in &jobs {
            table.rows.push(TableRow { cells: vec![j.clone(), n.to_string()], record: None });
        }
        Some(Figure {
            svg: occupations(&jobs, p),
            legend: Vec::new(),
            caption: String::new(),
            count: jobs.len(),
            table,
        })
    }
}
