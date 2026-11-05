//! Lifespans: a bar from birth to death for the first generations, with the marriages.

use crate::palette::Palette;
use crate::svg::{esc, open};
use crate::{Figure, LegendItem, Table, TableRow, Tree};
use std::fmt::Write;

/// Most lives drawn.
pub const MAX_LIVES: usize = 30;

#[derive(Debug, Clone)]
pub struct Life {
    pub label: String,
    pub birth: i32,
    pub death: Option<i32>,
    pub marriages: Vec<i32>,
    pub color: String,
    /// Separator above this row (a new generation starts).
    pub break_before: bool,
}

/// Horizontal bars from birth to death, with the marriages; `now` ends the open lives.
pub fn lifespans(
    lives: &[Life],
    now: i32,
    marriage: &str,
    ongoing: &str,
    age_label: impl Fn(i32) -> String,
    p: &Palette,
) -> String {
    let y0 = lives.iter().map(|l| l.birth).min().unwrap_or(now - 100);
    let y1 = lives.iter().map(|l| l.death.unwrap_or(now)).max().unwrap_or(now).max(y0 + 20);
    let step = if y1 - y0 > 150 {
        50
    } else if y1 - y0 > 60 {
        20
    } else {
        10
    };
    let y0 = y0 / step * step;
    let y1 = (y1 + step - 1) / step * step + step / 2;
    let (w, left, right, row, top) = (1000., 210., 60., 24., 20.);
    let x = |y: f64| left + (y - y0 as f64) / (y1 - y0) as f64 * (w - left - right);
    let bottom = top + lives.len() as f64 * row;
    let h = bottom + 64.;
    let (ink, muted, rule) = (p.ink, p.muted, p.rule);
    let mut s = open(w, h);
    let mut tick = y0;
    while tick <= y1 {
        let tx = x(tick as f64);
        let _ = write!(
            s,
            r##"<line x1="{tx:.1}" y1="{top}" x2="{tx:.1}" y2="{bottom}" stroke="{rule}" stroke-width="0.6"/><text x="{tx:.1}" y="{}" text-anchor="middle" font-size="10" fill="{muted}">{tick}</text>"##,
            bottom + 16.
        );
        tick += step;
    }
    for (i, l) in lives.iter().enumerate() {
        let yy = top + i as f64 * row + row / 2.;
        if l.break_before && i > 0 {
            let _ = write!(
                s,
                r##"<line x1="0" y1="{0:.1}" x2="{w}" y2="{0:.1}" stroke="{rule}" stroke-width="0.8"/>"##,
                yy - row / 2.
            );
        }
        let _ = write!(
            s,
            r##"<text x="{}" y="{:.1}" text-anchor="end" font-size="11" fill="{ink}">{}</text>"##,
            left - 12.,
            yy + 4.,
            esc(&l.label)
        );
        let end = l.death.unwrap_or(now);
        let opacity = if l.death.is_some() { "1" } else { "0.45" };
        let _ = write!(
            s,
            r##"<rect x="{:.1}" y="{:.1}" width="{:.1}" height="10" rx="5" fill="{}" fill-opacity="{opacity}"/>"##,
            x(l.birth as f64),
            yy - 5.,
            (x(end as f64) - x(l.birth as f64)).max(4.),
            l.color
        );
        for m in &l.marriages {
            let mx = x(*m as f64);
            let _ = write!(
                s,
                r##"<line x1="{mx:.1}" y1="{:.1}" x2="{mx:.1}" y2="{:.1}" stroke="{ink}" stroke-width="1.4"/>"##,
                yy - 8.,
                yy + 8.
            );
        }
        if let Some(d) = l.death {
            let _ = write!(
                s,
                r##"<text x="{:.1}" y="{:.1}" font-size="9.5" fill="{muted}">{}</text>"##,
                x(d as f64) + 5.,
                yy + 4.,
                esc(&age_label(d - l.birth))
            );
        }
    }
    let ly = bottom + 36.;
    let _ = write!(
        s,
        r##"<line x1="{left}" y1="{ly}" x2="{left}" y2="{}" stroke="{ink}" stroke-width="1.4"/><text x="{}" y="{}" font-size="10" fill="{muted}">{}</text>"##,
        ly + 14.,
        left + 8.,
        ly + 11.,
        esc(marriage)
    );
    let ox = left + 30. + marriage.chars().count() as f64 * 6.;
    let _ = write!(
        s,
        r##"<rect x="{ox:.1}" y="{}" width="30" height="10" rx="5" fill="{}" fill-opacity="0.45"/><text x="{:.1}" y="{}" font-size="10" fill="{muted}">{}</text>"##,
        ly + 2.,
        p.other,
        ox + 38.,
        ly + 11.,
        esc(ongoing)
    );
    s.push_str("</svg>");
    s
}

/// “GUYOT” → “Guyot”, for the narrow labels of the charts.
pub fn title_case(s: &str) -> String {
    s.split(' ')
        .map(|w| {
            let mut c = w.chars();
            match c.next() {
                Some(f) => f.to_uppercase().collect::<String>() + &c.as_str().to_lowercase(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

impl Tree<'_> {
    /// The lives of the first generations after the subject (at most [`MAX_LIVES`]), when at least
    /// three are dated; `now` (a year) ends the open lives.
    pub fn lives(&self, p: &Palette, now: i32) -> Option<Figure> {
        let g = self.g;
        let mut lives = Vec::new();
        let mut table = Table::new(&[
            t!("viz.col.person"),
            t!("viz.col.birth"),
            t!("viz.col.death"),
            t!("viz.col.age"),
            t!("viz.col.marriages"),
        ]);
        let mut last_gen = 0;
        for m in self.members.iter().filter(|m| m.generation >= 2) {
            let Some(ind) = g.individual(&m.xref) else { continue };
            let Some(birth) = ind.birth_year() else { continue };
            if lives.len() >= MAX_LIVES {
                break;
            }
            let marriages: Vec<i32> = ind
                .fams()
                .into_iter()
                .filter_map(|f| g.family(f))
                .filter_map(|f| f.marriage().and_then(|e| e.date()).and_then(|d| d.year()))
                .collect();
            let death =
                ind.death().as_ref().and_then(|e| e.date_raw()).map(gedcom::date::Date::parse).and_then(|d| d.year());
            let given = ind.given();
            let first = given.split_whitespace().next().map(str::to_string).unwrap_or_else(|| ind.display_name());
            table.rows.push(TableRow {
                cells: vec![
                    ind.display_name(),
                    birth.to_string(),
                    death.map(|d| d.to_string()).unwrap_or_default(),
                    death.map(|d| (d - birth).to_string()).unwrap_or_default(),
                    marriages.iter().map(|y| y.to_string()).collect::<Vec<_>>().join(", "),
                ],
                record: Some(m.xref.clone()),
            });
            lives.push(Life {
                label: format!("{first} {}", title_case(&ind.surname())),
                birth,
                death,
                marriages,
                color: self.origins.color(&m.xref, p).unwrap_or_else(|| p.other.to_string()),
                break_before: m.generation != last_gen,
            });
            last_gen = m.generation;
        }
        if lives.len() < 3 {
            return None;
        }
        let marriage = t!("viz.marriage");
        let ongoing = t!("viz.ongoing");
        let svg = lifespans(&lives, now, &marriage, &ongoing, |a| t!("viz.age_years", n = a).into_owned(), p);
        let legend = self
            .origins
            .groups
            .iter()
            .enumerate()
            .map(|(i, (label, count))| LegendItem {
                label: label.clone(),
                color: p.group(i, self.origins.keep).to_string(),
                count: *count,
                unknown: false,
            })
            .collect();
        Some(Figure { svg, legend, caption: String::new(), count: lives.len(), table })
    }
}
