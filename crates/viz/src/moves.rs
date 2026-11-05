//! Moves from one generation to the next: from a parent's birthplace to their child's. The
//! migrations map draws them over a period; the horizon measures how far each generation was born
//! from its parents.

use crate::map::{Link, MapPoint, Stroke, coordinates, map, town};
use crate::palette::Palette;
use crate::svg::open;
use crate::{Figure, LegendItem, Line, Table, TableRow, Tree};
use std::collections::{BTreeMap, HashMap};
use std::fmt::Write;

/// A child and one of their parents, both born in a located place.
#[derive(Debug, Clone)]
pub struct Move {
    pub child: String,
    pub parent: String,
    /// The parent's place, then the child's.
    pub from: String,
    pub to: String,
    pub from_at: (f64, f64),
    pub to_at: (f64, f64),
    /// The child's year of birth.
    pub year: Option<i32>,
    /// Kilometres between the two places, 0 when they are the same.
    pub km: f64,
    /// On the mother's side: for ancestors, the subject's maternal branch; for descendants, the
    /// child's mother.
    pub maternal: bool,
    /// The child's generation (1 = the subject).
    pub generation: u32,
}

/// Great-circle distance, in kilometres.
pub fn km((lon1, lat1): (f64, f64), (lon2, lat2): (f64, f64)) -> f64 {
    let (p1, p2) = (lat1.to_radians(), lat2.to_radians());
    let (dp, dl) = ((lat2 - lat1).to_radians(), (lon2 - lon1).to_radians());
    let a = (dp / 2.).sin().powi(2) + p1.cos() * p2.cos() * (dl / 2.).sin().powi(2);
    6371. * 2. * a.sqrt().min(1.).asin()
}

/// Whether an ancestor's Sosa number is on the subject's mother's side (3, 6, 7, 12…).
fn maternal_sosa(n: usize) -> bool {
    n >= 2 && {
        let top = usize::BITS - 1 - n.leading_zeros();
        (n >> (top - 1)) & 1 == 1
    }
}

/// “1830–1940”.
fn period_label((a, b): (i32, i32)) -> String {
    if a == b { a.to_string() } else { format!("{a}–{b}") }
}

/// Fewest moves for the horizon.
pub const MIN_MOVES: usize = 4;

impl Tree<'_> {
    /// Every child and parent of the scope born in located places, the same place included.
    pub fn moves(&self) -> Vec<Move> {
        let g = self.g;
        let coords = coordinates(g);
        let at = |x: &str| self.origins.place.get(x).and_then(|pl| Some((pl.clone(), *coords.get(pl)?)));
        let in_scope: HashMap<&str, &crate::Member> = self.members.iter().map(|m| (m.xref.as_str(), m)).collect();
        let mut out = Vec::new();
        for m in &self.members {
            let Some((to, to_at)) = at(&m.xref) else { continue };
            let year = g.individual(&m.xref).and_then(|i| i.birth_year());
            let (father, mother) = g.parents(&m.xref);
            for (parent, is_mother) in [(father, false), (mother, true)] {
                let Some(parent) = parent else { continue };
                let Some(pm) = in_scope.get(parent.xref()) else { continue };
                let Some((from, from_at)) = at(parent.xref()) else { continue };
                let maternal = match self.scope.line {
                    Line::Ancestors => pm.sosa.is_some_and(maternal_sosa),
                    Line::Descendants => is_mother,
                };
                out.push(Move {
                    child: m.xref.clone(),
                    parent: parent.xref().to_string(),
                    km: if from == to { 0. } else { km(from_at, to_at) },
                    from,
                    to: to.clone(),
                    from_at,
                    to_at,
                    year,
                    maternal,
                    generation: m.generation,
                });
            }
        }
        out
    }

    /// The years of the moves (the children's births), for a timeline: (first, last).
    pub fn move_years(&self) -> Option<(i32, i32)> {
        let years = self.moves().into_iter().filter(|m| m.km > 0.).filter_map(|m| m.year).collect::<Vec<_>>();
        Some((*years.iter().min()?, *years.iter().max()?))
    }

    /// Map of the moves from a parent's birthplace to their child's, on the father's side and the
    /// mother's, for children born in `period` (all of them when `None`); when at least two
    /// places are located. The frame stays the same whatever the period.
    pub fn migrations(&self, p: &Palette, period: Option<(i32, i32)>) -> Option<Figure> {
        let all: Vec<Move> = self.moves().into_iter().filter(|m| m.km > 0.).collect();
        if all.is_empty() {
            return None;
        }
        let frame: Vec<(f64, f64)> = all.iter().flat_map(|m| [m.from_at, m.to_at]).collect();
        let shown: Vec<&Move> =
            all.iter().filter(|m| period.is_none_or(|(a, b)| m.year.is_some_and(|y| (a..=b).contains(&y)))).collect();
        let mut points: BTreeMap<&str, MapPoint> = BTreeMap::new();
        for m in &shown {
            for (name, (lon, lat)) in [(&m.from, m.from_at), (&m.to, m.to_at)] {
                points.entry(name).or_insert_with(|| MapPoint { lon, lat, label: town(name), count: 0 });
            }
            for name in [&m.from, &m.to] {
                points.get_mut(name.as_str()).expect("inserted above").count += 1;
            }
        }
        let links: Vec<Link> = shown
            .iter()
            .map(|m| Link {
                from: m.from_at,
                to: m.to_at,
                stroke: if m.maternal { Stroke::Maternal } else { Stroke::Paternal },
            })
            .collect();
        let g = self.g;
        let name = |x: &str| g.individual(x).map(|i| i.display_name()).unwrap_or_default();
        let mut table = Table::new(&[
            t!("viz.col.person"),
            t!("viz.col.birth"),
            t!("viz.col.parent"),
            t!("viz.col.from"),
            t!("viz.col.to"),
            t!("viz.col.distance"),
        ]);
        let mut rows: Vec<&&Move> = shown.iter().collect();
        rows.sort_by_key(|m| (m.year.unwrap_or(i32::MAX), m.child.clone()));
        for m in rows {
            table.rows.push(TableRow {
                cells: vec![
                    name(&m.child),
                    m.year.map(|y| y.to_string()).unwrap_or_default(),
                    name(&m.parent),
                    town(&m.from),
                    town(&m.to),
                    distance(m.km),
                ],
                record: Some(m.child.clone()),
            });
        }
        let maternal = shown.iter().filter(|m| m.maternal).count();
        let (father, mother) = match self.scope.line {
            Line::Ancestors => (t!("viz.paternal_side"), t!("viz.maternal_side")),
            Line::Descendants => (t!("viz.through_father"), t!("viz.through_mother")),
        };
        let legend = vec![
            LegendItem {
                label: father.into_owned(),
                color: p.paternal.into(),
                count: shown.len() - maternal,
                unknown: false,
            },
            LegendItem { label: mother.into_owned(), color: p.maternal.into(), count: maternal, unknown: false },
        ];
        let span = period.or_else(|| {
            let years: Vec<i32> = shown.iter().filter_map(|m| m.year).collect();
            Some((*years.iter().min()?, *years.iter().max()?))
        });
        let moves = if shown.len() == 1 {
            t!("viz.moves.one").into_owned()
        } else {
            t!("viz.moves.other", n = shown.len()).into_owned()
        };
        let pts: Vec<MapPoint> = points.into_values().collect();
        Some(Figure {
            svg: map(&pts, &links, &frame, p.muted, p),
            legend,
            caption: match span {
                Some(s) => format!("{} · {moves}", period_label(s)),
                None => moves,
            },
            count: shown.len(),
            table,
        })
    }

    /// How far each generation was born from its parents' birthplace, on a log scale, with the
    /// median; when at least [`MIN_MOVES`] are known.
    pub fn horizon(&self, p: &Palette) -> Option<Figure> {
        let moves = self.moves();
        if moves.len() < MIN_MOVES {
            return None;
        }
        let mut by: BTreeMap<u32, Vec<&Move>> = BTreeMap::new();
        for m in &moves {
            by.entry(m.generation).or_default().push(m);
        }
        // Oldest on the left: for ancestors, the highest generation.
        let mut gens: Vec<(u32, Vec<&Move>)> = by.into_iter().collect();
        if self.scope.line == Line::Ancestors {
            gens.reverse();
        }
        let median = |v: &[&Move]| {
            let mut d: Vec<f64> = v.iter().map(|m| m.km).collect();
            d.sort_by(f64::total_cmp);
            d[d.len() / 2]
        };
        let mut table = Table::new(&[
            t!("viz.col.generation"),
            t!("viz.col.births"),
            t!("viz.col.moves"),
            t!("viz.col.median"),
            t!("viz.col.same_place"),
        ]);
        for (n, v) in &gens {
            let years: Vec<i32> = v.iter().filter_map(|m| m.year).collect();
            let births = match (years.iter().min(), years.iter().max()) {
                (Some(&a), Some(&b)) => period_label((a, b)),
                _ => String::new(),
            };
            let same = v.iter().filter(|m| m.km == 0.).count();
            table.rows.push(TableRow {
                cells: vec![
                    n.to_string(),
                    births,
                    v.len().to_string(),
                    distance(median(v)),
                    t!("viz.percent", n = (same as f64 / v.len() as f64 * 100.).round()).into_owned(),
                ],
                record: None,
            });
        }
        let svg = horizon(
            &gens.iter().map(|(n, v)| (*n, v.iter().map(|m| (m.km, m.maternal)).collect())).collect::<Vec<_>>(),
            p,
        );
        Some(Figure {
            svg,
            legend: Vec::new(),
            caption: t!("viz.horizon_caption", n = moves.len()).into_owned(),
            count: moves.len(),
            table,
        })
    }
}

/// A distance for the tables: whole kilometres, "< 1 km" for two places closer than that.
fn distance(km: f64) -> String {
    if km > 0. && km < 0.5 { t!("viz.under_km").into_owned() } else { t!("viz.km", n = km.round()).into_owned() }
}

/// The horizon: for each generation (oldest first), the distances (km, on the mother's side) from
/// the children's birthplace to their parents', as dots on a log scale, with the median.
pub fn horizon(gens: &[(u32, Vec<(f64, bool)>)], p: &Palette) -> String {
    let (w, h, left, top, base) = (1000., 420., 70., 20., 360.);
    // 0 km (the same place) at the bottom, then 1 to 10 000 km.
    let same = base;
    let y = |km: f64| {
        if km <= 0. {
            same
        } else {
            let l = km.max(1.).log10() / 4.;
            base - 30. - l.min(1.) * (base - 30. - top)
        }
    };
    let mut s = open(w, h);
    for (km, label) in [(1., "1 km"), (10., "10 km"), (100., "100 km"), (1000., "1 000 km"), (10000., "10 000 km")] {
        let yy = y(km);
        let _ = write!(
            s,
            r##"<line x1="{left}" y1="{yy:.1}" x2="{}" y2="{yy:.1}" stroke="{}" stroke-width="0.6"/><text x="{}" y="{:.1}" text-anchor="end" font-size="11" fill="{}">{label}</text>"##,
            w - 20.,
            p.rule,
            left - 8.,
            yy + 4.,
            p.muted
        );
    }
    let _ = write!(
        s,
        r##"<text x="{}" y="{:.1}" text-anchor="end" font-size="11" fill="{}">{}</text>"##,
        left - 8.,
        same + 4.,
        p.muted,
        crate::svg::esc(&t!("viz.same_place"))
    );
    let col = (w - left - 20.) / gens.len().max(1) as f64;
    let mut medians = Vec::new();
    for (i, (n, v)) in gens.iter().enumerate() {
        let cx = left + col * (i as f64 + 0.5);
        for (k, &(km, maternal)) in v.iter().enumerate() {
            // Spread the dots across the column, the same way each time.
            let jitter = ((k * 7919) % 97) as f64 / 97. - 0.5;
            let x = cx + jitter * col * 0.5;
            let color = if maternal { p.maternal } else { p.paternal };
            let _ =
                write!(s, r##"<circle cx="{x:.1}" cy="{:.1}" r="3.2" fill="{color}" fill-opacity="0.55"/>"##, y(km));
        }
        let mut d: Vec<f64> = v.iter().map(|m| m.0).collect();
        d.sort_by(f64::total_cmp);
        if let Some(&m) = d.get(d.len() / 2) {
            let my = y(m);
            medians.push((cx, my));
            let _ = write!(
                s,
                r##"<line x1="{:.1}" y1="{my:.1}" x2="{:.1}" y2="{my:.1}" stroke="{}" stroke-width="2.5"/>"##,
                cx - col * 0.3,
                cx + col * 0.3,
                p.ink
            );
        }
        let _ = write!(
            s,
            r##"<text x="{cx:.1}" y="{}" text-anchor="middle" font-size="11" fill="{}">G{n}</text>"##,
            base + 30.,
            p.muted
        );
    }
    if medians.len() >= 2 {
        s.push_str(r##"<path d="M"##);
        for (i, (x, yy)) in medians.iter().enumerate() {
            let _ = write!(s, "{}{x:.1},{yy:.1}", if i > 0 { " L" } else { "" });
        }
        let _ = write!(s, r##"" fill="none" stroke="{}" stroke-width="1" stroke-dasharray="3 3"/>"##, p.ink);
    }
    s.push_str("</svg>");
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn measures_distances_and_sides() {
        let paris = (2.35, 48.86);
        let troyes = (4.07, 48.30);
        assert!((km(paris, troyes) - 140.).abs() < 10., "{}", km(paris, troyes));
        assert_eq!(km(paris, paris), 0.);
        assert!(!maternal_sosa(2) && maternal_sosa(3) && !maternal_sosa(5) && maternal_sosa(6) && maternal_sosa(13));
    }

    fn born(id: &str, name: &str, sex: &str, year: i32, place: &str, (lat, lon): (f64, f64), links: &str) -> String {
        format!(
            "0 @{id}@ INDI\n1 NAME {name}\n1 SEX {sex}\n1 BIRT\n2 DATE {year}\n2 PLAC {place}\n3 MAP\n4 LATI N{lat}\n4 LONG E{lon}\n{links}"
        )
    }

    #[test]
    fn maps_the_moves_of_each_side() {
        use crate::{Palette, Scope};
        let (paris, troyes, lyon, dijon, marseille) =
            ((48.86, 2.35), (48.30, 4.07), (45.76, 4.84), (47.32, 5.04), (43.30, 5.37));
        let ged = [
            "0 HEAD\n1 GEDC\n2 VERS 7.0\n".to_string(),
            born("I1", "Louis /M/", "M", 1900, "Paris, France", paris, "1 FAMC @F1@\n"),
            born("I2", "Jean /M/", "M", 1870, "Troyes, France", troyes, "1 FAMS @F1@\n1 FAMC @F2@\n"),
            born("I3", "Marie /P/", "F", 1872, "Lyon, France", lyon, "1 FAMS @F1@\n1 FAMC @F3@\n"),
            born("I4", "Paul /M/", "M", 1840, "Troyes, France", troyes, "1 FAMS @F2@\n"),
            born("I6", "Anne /D/", "F", 1842, "Dijon, France", dijon, "1 FAMS @F2@\n"),
            born("I7", "Rose /S/", "F", 1845, "Marseille, France", marseille, "1 FAMS @F3@\n"),
            "0 @F1@ FAM\n1 HUSB @I2@\n1 WIFE @I3@\n1 CHIL @I1@\n".into(),
            "0 @F2@ FAM\n1 HUSB @I4@\n1 WIFE @I6@\n1 CHIL @I2@\n".into(),
            "0 @F3@ FAM\n1 WIFE @I7@\n1 CHIL @I3@\n".into(),
            "0 TRLR\n".into(),
        ]
        .concat();
        let g = gedcom::Gedcom::parse(&ged).expect("tree");
        let t = Tree::new(&g, Scope::new("I1", Line::Ancestors, 4)).expect("tree");
        let moves = t.moves();
        assert_eq!(moves.len(), 5);
        let same: Vec<_> = moves.iter().filter(|m| m.km == 0.).collect();
        assert_eq!((same.len(), same[0].child.as_str()), (1, "I2"));
        let side = |c: &str, p: &str| moves.iter().find(|m| m.child == c && m.parent == p).expect("move").maternal;
        // Sosa 3 and 7 are on the mother's side; 5, the father's mother, is not.
        assert!(!side("I1", "I2") && side("I1", "I3") && !side("I2", "I6") && side("I3", "I7"));
        assert_eq!(t.move_years(), Some((1870, 1900)));

        let all = t.migrations(&Palette::PRINT, None).expect("migrations");
        assert_eq!((all.count, all.table.rows.len()), (4, 4));
        assert_eq!(all.legend.iter().map(|l| l.count).collect::<Vec<_>>(), [2, 2]);
        assert!(all.svg.contains("stroke-dasharray"));
        let before = t.migrations(&Palette::PRINT, Some((1860, 1879))).expect("1860s–1870s");
        assert_eq!(before.count, 2);
        assert!(t.migrations(&Palette::PRINT, Some((1700, 1799))).is_some_and(|f| f.count == 0));

        let horizon = t.horizon(&Palette::PRINT).expect("horizon");
        assert_eq!(horizon.table.rows.len(), 2);
        assert!(horizon.svg.starts_with("<svg") && horizon.svg.ends_with("</svg>"));
    }
}
