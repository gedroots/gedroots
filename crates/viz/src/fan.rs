//! Fans: of ancestors (each generation a ring, the father's side on the left) and of descendants
//! (each child's sector inside their parent's), the cells coloured by where people come from.

use crate::palette::Palette;
use crate::svg::{esc, open};
use crate::{Figure, LegendItem, Line, Table, TableRow, Tree};
use std::collections::HashMap;
use std::fmt::Write;

/// A cell of the fan of ancestors: Sosa number, labels and colour.
#[derive(Debug, Clone)]
pub struct FanCell {
    pub sosa: usize,
    pub given: String,
    pub surname: String,
    pub years: String,
    /// `None`: place unknown (empty cell).
    pub color: Option<String>,
}

/// Widths of the rings: they narrow outwards, the outer generations only show their colour.
const WIDTHS: [f64; 9] = [95., 70., 65., 60., 50., 40., 32., 28., 22.];

fn radii(rings: usize) -> (Vec<f64>, f64) {
    let total: f64 = WIDTHS[..rings].iter().sum();
    let scale = 440. / total;
    let mut radii = vec![0.];
    for wd in &WIDTHS[..rings] {
        let last = *radii.last().expect("starts with 0");
        radii.push(last + wd * scale);
    }
    (radii, scale)
}

/// Writes a label, rotated around its centre.
#[allow(clippy::too_many_arguments)]
fn text(s: &mut String, x: f64, y: f64, rot: f64, size: f64, weight: u32, fill: &str, t: &str) {
    let _ = write!(
        s,
        r##"<text x="{x:.1}" y="{y:.1}" transform="rotate({rot:.1} {x:.1} {y:.1})" text-anchor="middle" dominant-baseline="middle" font-size="{size}" font-weight="{weight}" fill="{fill}">{}</text>"##,
        esc(t)
    );
}

/// Half-circle fan over `generations` rings, the father's side on the left. `labels`: names in the
/// inner rings (off for a book's cover).
pub fn fan(cells: &[FanCell], generations: u32, labels: bool, p: &Palette) -> String {
    let g_max = generations.clamp(2, 9) as usize;
    let (w, h) = (1000., 560.);
    let (cx, cy) = (500., 520.);
    let (radii, scale) = radii(g_max);
    let pt = |r: f64, a: f64| (cx + r * a.to_radians().cos(), cy - r * a.to_radians().sin());
    let mut s = open(w, h);
    for g in 0..g_max {
        let n = 1usize << g;
        let span = 180. / n as f64;
        let (r0, r1) = (radii[g], radii[g + 1]);
        for k in 0..n {
            let sosa = n + k;
            let cell = cells.iter().find(|c| c.sosa == sosa);
            let a0 = 180. - k as f64 * span;
            let a1 = a0 - span;
            let fill = cell.and_then(|c| c.color.as_deref()).unwrap_or(p.paper);
            let stroke = if cell.is_some() { p.paper } else { p.rule };
            let path = if g == 0 {
                format!("M{},{cy} A{r1},{r1} 0 0 1 {},{cy} Z", cx - r1, cx + r1)
            } else {
                let (x0, y0) = pt(r1, a0);
                let (x1, y1) = pt(r1, a1);
                let (x2, y2) = pt(r0, a1);
                let (x3, y3) = pt(r0, a0);
                format!(
                    "M{x0:.1},{y0:.1} A{r1:.1},{r1:.1} 0 0 1 {x1:.1},{y1:.1} L{x2:.1},{y2:.1} A{r0:.1},{r0:.1} 0 0 0 {x3:.1},{y3:.1} Z"
                )
            };
            let dash = if cell.is_some() { "" } else { r##" stroke-dasharray="3 3""## };
            let sw = if g < 6 { 1.4 } else { 0.8 };
            let _ = write!(s, r##"<path d="{path}" fill="{fill}" stroke="{stroke}" stroke-width="{sw}"{dash}/>"##);
            let (Some(c), true) = (cell, labels) else { continue };
            let ink = if c.color.is_some() { p.on_group } else { p.ink };
            let mid = (a0 + a1) / 2.;
            if g == 0 {
                text(&mut s, cx, cy - 46., 0., 17., 600, ink, &c.given);
                text(&mut s, cx, cy - 27., 0., 12., 600, ink, &c.surname);
                text(&mut s, cx, cy - 10., 0., 11., 400, ink, &c.years);
            } else if g <= 2 {
                let rm = (r0 + r1) / 2.;
                let rot = 90. - mid;
                let lines = [(&c.given, 12., 600), (&c.surname, 10., 600), (&c.years, 9.5, 400)];
                for (j, (t, size, weight)) in lines.iter().enumerate() {
                    let (x, y) = pt(rm - (j as f64 - 1.) * 14. * scale.min(1.2), mid);
                    text(&mut s, x, y, rot, *size, *weight, ink, t);
                }
            } else if g <= 5 {
                let rm = (r0 + r1) / 2.;
                let rot = if mid < 90. { -mid } else { 180. - mid };
                let size = [0., 0., 0., 9.5, 8., 6.5][g];
                let mut lines: Vec<&String> = if g < 5 { vec![&c.given, &c.surname] } else { vec![&c.surname] };
                if g == 3 {
                    lines.push(&c.years);
                }
                for (j, t) in lines.iter().enumerate() {
                    let da = (j as f64 - (lines.len() - 1) as f64 / 2.) * span * 0.3;
                    let a = if mid < 90. { mid + da } else { mid - da };
                    let (x, y) = pt(rm, a);
                    text(&mut s, x, y, rot, size, if j < 2 { 600 } else { 400 }, ink, t);
                }
            }
        }
    }
    if labels {
        for g in 1..g_max {
            let rm = (radii[g] + radii[g + 1]) / 2.;
            for x in [cx - rm, cx + rm] {
                let _ = write!(
                    s,
                    r##"<text x="{x:.1}" y="{}" text-anchor="middle" font-size="9" fill="{}">{}</text>"##,
                    cy + 16.,
                    p.muted,
                    g + 1
                );
            }
        }
    }
    s.push_str("</svg>");
    s
}

/// A cell of the fan of descendants: its ring (0 = the person) and its angles, in degrees from
/// the left (180) to the right (0), each family sharing its parent's angle by the number of its
/// descendants.
#[derive(Debug, Clone)]
pub struct DescCell {
    pub ring: usize,
    pub a0: f64,
    pub a1: f64,
    pub given: String,
    pub surname: String,
    pub years: String,
    pub color: Option<String>,
}

/// Half-circle fan of descendants over `rings` rings: the person in the middle, each child's
/// sector inside their parent's.
pub fn descendant_fan(cells: &[DescCell], rings: usize, labels: bool, p: &Palette) -> String {
    let rings = rings.clamp(2, 9);
    let (w, h) = (1000., 560.);
    let (cx, cy) = (500., 520.);
    let (radii, scale) = radii(rings);
    let pt = |r: f64, a: f64| (cx + r * a.to_radians().cos(), cy - r * a.to_radians().sin());
    let mut s = open(w, h);
    for c in cells.iter().filter(|c| c.ring < rings) {
        let (r0, r1) = (radii[c.ring], radii[c.ring + 1]);
        let fill = c.color.as_deref().unwrap_or(p.paper);
        let path = if c.ring == 0 {
            format!("M{},{cy} A{r1},{r1} 0 0 1 {},{cy} Z", cx - r1, cx + r1)
        } else {
            let (x0, y0) = pt(r1, c.a0);
            let (x1, y1) = pt(r1, c.a1);
            let (x2, y2) = pt(r0, c.a1);
            let (x3, y3) = pt(r0, c.a0);
            let large = if c.a0 - c.a1 > 180. { 1 } else { 0 };
            format!(
                "M{x0:.1},{y0:.1} A{r1:.1},{r1:.1} 0 {large} 1 {x1:.1},{y1:.1} L{x2:.1},{y2:.1} A{r0:.1},{r0:.1} 0 {large} 0 {x3:.1},{y3:.1} Z"
            )
        };
        let sw = if c.ring < 6 { 1.4 } else { 0.8 };
        let _ = write!(s, r##"<path d="{path}" fill="{fill}" stroke="{}" stroke-width="{sw}"/>"##, p.paper);
        if !labels {
            continue;
        }
        let ink = if c.color.is_some() { p.on_group } else { p.ink };
        let span = c.a0 - c.a1;
        let mid = (c.a0 + c.a1) / 2.;
        let rm = (r0 + r1) / 2.;
        if c.ring == 0 {
            text(&mut s, cx, cy - 46., 0., 17., 600, ink, &c.given);
            text(&mut s, cx, cy - 27., 0., 12., 600, ink, &c.surname);
            text(&mut s, cx, cy - 10., 0., 11., 400, ink, &c.years);
        } else if span * rm.to_radians() >= 34. && c.ring <= 2 {
            // Wide enough: the names along the ring.
            let rot = 90. - mid;
            for (j, (t, size, weight)) in
                [(&c.given, 12., 600), (&c.surname, 10., 600), (&c.years, 9.5, 400)].iter().enumerate()
            {
                let (x, y) = pt(rm - (j as f64 - 1.) * 14. * scale.min(1.2), mid);
                text(&mut s, x, y, rot, *size, *weight, ink, t);
            }
        } else if span * rm.to_radians() >= 11. && c.ring <= 5 {
            // Narrow: the given name across the ring.
            let rot = if mid < 90. { -mid } else { 180. - mid };
            let (x, y) = pt(rm, mid);
            text(&mut s, x, y, rot, [0., 0., 9.5, 9., 8., 6.5][c.ring], 600, ink, &c.given);
        }
    }
    s.push_str("</svg>");
    s
}

impl Tree<'_> {
    /// The fan of the scope, coloured by the places people come from, with its legend. `labels`:
    /// names in the cells (off for a book's cover).
    pub fn fan(&self, p: &Palette, labels: bool) -> Figure {
        let g = self.g;
        let first = |given: String| given.split_whitespace().next().unwrap_or_default().to_string();
        let gens = self.scope.generations.min(9);
        let mut legend: Vec<LegendItem> = self
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
        let mut table =
            Table::new(&[t!("viz.col.number"), t!("viz.col.person"), t!("viz.col.years"), t!("viz.col.origin")]);
        for m in &self.members {
            if m.generation > gens {
                continue;
            }
            let Some(ind) = g.individual(&m.xref) else { continue };
            table.rows.push(TableRow {
                cells: vec![
                    m.number.clone(),
                    ind.display_name(),
                    ind.years(),
                    self.origins.group_label(&m.xref).unwrap_or_default(),
                ],
                record: Some(m.xref.clone()),
            });
        }
        let svg = match self.scope.line {
            Line::Ancestors => {
                let mut cells = Vec::new();
                let mut unknown = 0;
                for m in &self.members {
                    let Some(ind) = g.individual(&m.xref) else { continue };
                    for sosa in m.sosa.iter().chain(m.also.iter()) {
                        if *sosa >= 1 << gens {
                            continue;
                        }
                        let color = self.origins.color(&m.xref, p);
                        unknown += usize::from(color.is_none());
                        cells.push(FanCell {
                            sosa: *sosa,
                            given: first(ind.given()),
                            surname: ind.surname(),
                            years: ind.years(),
                            color,
                        });
                    }
                }
                if unknown > 0 {
                    legend.push(LegendItem {
                        label: t!("viz.unknown_place").into_owned(),
                        color: p.paper.to_string(),
                        count: unknown,
                        unknown: true,
                    });
                }
                fan(&cells, gens, labels, p)
            }
            Line::Descendants => {
                let rings = gens as usize;
                let parent = |n: &str| n.rsplit_once('.').map(|(p, _)| p.to_string());
                let drawn = || self.members.iter().filter(|m| (m.generation as usize) <= rings);
                // Weight of a member: the leaves under it, within the drawn rings.
                let mut weight: HashMap<String, f64> = HashMap::new();
                for m in drawn().collect::<Vec<_>>().into_iter().rev() {
                    let w = weight.get(&m.number).copied().unwrap_or(0.).max(1.);
                    weight.insert(m.number.clone(), w);
                    if let Some(p) = parent(&m.number) {
                        *weight.entry(p).or_insert(0.) += w;
                    }
                }
                let mut cells = Vec::new();
                let mut start: HashMap<String, f64> = HashMap::new();
                let mut angle: HashMap<String, (f64, f64)> = HashMap::new();
                for m in drawn() {
                    let Some(ind) = g.individual(&m.xref) else { continue };
                    let (a0, a1) = match parent(&m.number) {
                        None => (180., 0.),
                        Some(pn) => {
                            let Some(&(pa0, pa1)) = angle.get(&pn) else { continue };
                            let total = weight.get(&pn).copied().unwrap_or(1.).max(1.);
                            let from = *start.get(&pn).unwrap_or(&pa0);
                            let span = (pa0 - pa1) * weight.get(&m.number).copied().unwrap_or(1.) / total;
                            start.insert(pn, from - span);
                            (from, from - span)
                        }
                    };
                    angle.insert(m.number.clone(), (a0, a1));
                    cells.push(DescCell {
                        ring: m.generation as usize - 1,
                        a0,
                        a1,
                        given: first(ind.given()),
                        surname: ind.surname(),
                        years: ind.years(),
                        color: self.origins.color(&m.xref, p),
                    });
                }
                // Only the rings that hold someone.
                let rings = cells.iter().map(|c| c.ring + 1).max().unwrap_or(1).max(2);
                descendant_fan(&cells, rings, labels, p)
            }
        };
        Figure { svg, legend, caption: String::new(), count: table.rows.len(), table }
    }
}
