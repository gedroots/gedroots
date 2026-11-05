//! Seasons: births, marriages and deaths by month of the year, as historians of population draw
//! them. Each month gets an index corrected for its length (100: as many as an average month
//! would have), so that February is not short of births because it has fewer days.

use crate::palette::Palette;
use crate::svg::{esc, open};
use crate::{Figure, Table, TableRow, Tree};
use gedcom::date::Date;
use gedcom::model::Event;
use std::collections::HashSet;
use std::fmt::Write;

/// Fewest events of a kind with their month for its panel to be drawn.
pub const MIN_EVENTS: usize = 12;

/// Days in each month, February counting a quarter of a leap day.
const DAYS: [f64; 12] = [31., 28.25, 31., 30., 31., 30., 31., 31., 30., 31., 30., 31.];

/// The month (1 to 12, Gregorian) of an event dated to the month exactly.
fn month(e: &Event) -> Option<usize> {
    let Date::Exact(d) = e.date()? else { return None };
    d.month?;
    let m = d.sort_key().1 as usize;
    (1..=12).contains(&m).then_some(m)
}

/// The month of the first of these events dated to the month.
fn first_month<'a>(events: impl IntoIterator<Item = Option<Event<'a>>>) -> Option<usize> {
    events.into_iter().flatten().find_map(|e| month(&e))
}

/// How many events fall in each month, and their index corrected for the month's length.
#[derive(Debug, Clone, PartialEq)]
pub struct Months {
    pub counts: [usize; 12],
}

impl Months {
    fn new() -> Months {
        Months { counts: [0; 12] }
    }

    pub fn total(&self) -> usize {
        self.counts.iter().sum()
    }

    /// The index of each month: 100 is as many per day as over the whole year.
    pub fn index(&self) -> [f64; 12] {
        let per_day = self.total() as f64 / 365.25;
        std::array::from_fn(|i| if per_day > 0. { self.counts[i] as f64 / DAYS[i] / per_day * 100. } else { 0. })
    }
}

/// The names of the months, in the language being written.
fn month_names(key: &str) -> Vec<String> {
    t!(key).split(',').map(|m| m.trim().to_string()).collect()
}

/// The panels side by side: (title, months), with the same scale, a line at 100.
pub fn seasons(panels: &[(String, &Months)], p: &Palette) -> String {
    let (w, h) = (1000., 280.);
    let (top, base) = (44., 236.);
    let (gap, left) = (36., 30.);
    let pw = (w - left - gap * (panels.len().max(1) - 1) as f64) / panels.len().max(1) as f64;
    let max = panels.iter().flat_map(|(_, m)| m.index()).fold(150., f64::max);
    let max = (max / 50.).ceil() * 50.;
    let y = |v: f64| base - v / max * (base - top);
    let initials = month_names("viz.months_initial");
    let mut s = open(w, h);
    for (k, (title, months)) in panels.iter().enumerate() {
        let x0 = left + k as f64 * (pw + gap);
        let _ = write!(
            s,
            r##"<text x="{x0:.1}" y="16" font-size="14" font-weight="600" fill="{}">{}</text><text x="{x0:.1}" y="32" font-size="11" fill="{}">{}</text>"##,
            p.ink,
            esc(title),
            p.muted,
            esc(&t!("viz.seasons.events", n = months.total()))
        );
        let col = pw / 12.;
        let index = months.index();
        for (i, v) in index.iter().enumerate() {
            let x = x0 + i as f64 * col + col * 0.15;
            let _ = write!(
                s,
                r##"<rect x="{x:.1}" y="{:.1}" width="{:.1}" height="{:.1}" rx="2" fill="{}" fill-opacity="{}"/>"##,
                y(*v),
                col * 0.7,
                base - y(*v),
                p.accent,
                if *v >= 100. { "0.85" } else { "0.4" }
            );
            let _ = write!(
                s,
                r##"<text x="{:.1}" y="{}" text-anchor="middle" font-size="11" fill="{}">{}</text>"##,
                x + col * 0.35,
                base + 18.,
                p.muted,
                esc(initials.get(i).map(String::as_str).unwrap_or(""))
            );
        }
        let _ = write!(
            s,
            r##"<line x1="{x0:.1}" y1="{base}" x2="{:.1}" y2="{base}" stroke="{}" stroke-width="0.8"/><line x1="{x0:.1}" y1="{y100:.1}" x2="{:.1}" y2="{y100:.1}" stroke="{}" stroke-width="1" stroke-dasharray="4 3"/>"##,
            x0 + pw,
            p.rule,
            x0 + pw,
            p.ink,
            y100 = y(100.)
        );
        if k == 0 {
            let _ = write!(
                s,
                r##"<text x="{:.1}" y="{:.1}" text-anchor="end" font-size="10" fill="{}">100</text>"##,
                x0 - 6.,
                y(100.) + 3.5,
                p.muted
            );
        }
    }
    s.push_str("</svg>");
    s
}

impl Tree<'_> {
    /// Births, marriages and deaths of the scope's people by month (marriages: those of their
    /// families, once each).
    pub fn months(&self) -> [Months; 3] {
        let g = self.g;
        let (mut births, mut marriages, mut deaths) = (Months::new(), Months::new(), Months::new());
        let mut families = HashSet::new();
        for m in &self.members {
            let Some(ind) = g.individual(&m.xref) else { continue };
            if let Some(b) = first_month(["BIRT", "CHR", "BAPM"].map(|t| ind.event(t))) {
                births.counts[b - 1] += 1;
            }
            if let Some(d) = first_month(["DEAT", "BURI"].map(|t| ind.event(t))) {
                deaths.counts[d - 1] += 1;
            }
            for f in ind.fams() {
                if families.insert(f)
                    && let Some(fam) = g.family(f)
                    && let Some(mm) = first_month([fam.marriage()])
                {
                    marriages.counts[mm - 1] += 1;
                }
            }
        }
        [births, marriages, deaths]
    }

    /// Births, marriages and deaths by month, each when at least [`MIN_EVENTS`] are dated to the
    /// month.
    pub fn seasons(&self, p: &Palette) -> Option<Figure> {
        let months = self.months();
        let titles = [t!("viz.col.births"), t!("viz.col.marriages"), t!("viz.col.deaths")];
        let panels: Vec<(String, &Months)> = titles
            .iter()
            .zip(&months)
            .filter(|(_, m)| m.total() >= MIN_EVENTS)
            .map(|(t, m)| (t.to_string(), m))
            .collect();
        if panels.is_empty() {
            return None;
        }
        let mut columns = vec![t!("viz.col.month").into_owned()];
        for (title, _) in &panels {
            columns.push(title.clone());
            columns.push(t!("viz.col.index").into_owned());
        }
        let mut table = Table::new(&columns);
        let names = month_names("viz.months");
        let indexes: Vec<[f64; 12]> = panels.iter().map(|(_, m)| m.index()).collect();
        for i in 0..12 {
            let mut cells = vec![names.get(i).cloned().unwrap_or_default()];
            for ((_, m), index) in panels.iter().zip(&indexes) {
                cells.push(m.counts[i].to_string());
                cells.push(format!("{:.0}", index[i]));
            }
            table.rows.push(TableRow { cells, record: None });
        }
        let events: usize = panels.iter().map(|(_, m)| m.total()).sum();
        Some(Figure {
            svg: seasons(&panels, p),
            legend: Vec::new(),
            caption: t!("viz.seasons_caption").into_owned(),
            count: events,
            table,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Line, Scope};

    #[test]
    fn counts_the_months_of_each_kind() {
        let mut ged = String::from("0 HEAD\n1 GEDC\n2 VERS 7.0\n0 @I1@ INDI\n1 NAME A /B/\n1 FAMC @F1@\n");
        ged += "0 @F1@ FAM\n1 HUSB @I2@\n1 WIFE @I3@\n1 CHIL @I1@\n1 MARR\n2 DATE 12 FEB 1850\n";
        ged += "0 @I2@ INDI\n1 NAME C /B/\n1 FAMS @F1@\n1 BIRT\n2 DATE 1820\n1 CHR\n2 DATE 3 MAR 1820\n1 DEAT\n2 DATE ABT JAN 1880\n";
        ged += "0 @I3@ INDI\n1 NAME D /E/\n1 FAMS @F1@\n1 BIRT\n2 DATE FRENCH_R 15 BRUM 12\n1 BURI\n2 DATE AUG 1890\n0 TRLR\n";
        let g = gedcom::Gedcom::parse(&ged).expect("tree");
        let t = Tree::new(&g, Scope::new("I1", Line::Ancestors, 3)).expect("tree");
        let [births, marriages, deaths] = t.months();
        // The christening when the birth has no month; 15 Brumaire XII is 7 November 1803.
        assert_eq!((births.counts[2], births.counts[10], births.total()), (1, 1, 2));
        // The family's marriage once, though both spouses are in the tree.
        assert_eq!((marriages.counts[1], marriages.total()), (1, 1));
        // An approximate death is left out, the burial counts.
        assert_eq!((deaths.counts[7], deaths.total()), (1, 1));
        assert!(t.seasons(&Palette::PRINT).is_none(), "too few events");

        let even = Months { counts: [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31] };
        assert!(even.index().iter().all(|v| (v - 100.).abs() < 1.5), "{:?}", even.index());
        let svg = seasons(&[("Births".into(), &even)], &Palette::PRINT);
        assert!(svg.starts_with("<svg") && svg.ends_with("</svg>") && svg.contains("stroke-dasharray"));
    }
}
