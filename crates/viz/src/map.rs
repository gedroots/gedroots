//! Map of the places people come from, fitted to them, with a line from each child's place to
//! their parents'.

use crate::palette::Palette;
use crate::svg::{esc, open};
use crate::{Figure, Table, TableRow, Tree};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fmt::Write;

/// Outlines of the countries (Natural Earth 1:50m, public domain): one ring per line,
/// `lon,lat lon,lat …`.
const COUNTRIES: &str = include_str!("../assets/countries.txt");

#[derive(Debug, Clone)]
pub struct MapPoint {
    pub lon: f64,
    pub lat: f64,
    pub label: String,
    pub count: usize,
}

/// How a line between two places is drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stroke {
    /// A thin grey line (child and parent born in different places).
    Plain,
    /// A move, with an arrow at its end: on the father's side, solid; on the mother's, dashed.
    Paternal,
    Maternal,
}

/// A line between two places: from (lon, lat) to (lon, lat).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Link {
    pub from: (f64, f64),
    pub to: (f64, f64),
    pub stroke: Stroke,
}

/// Map of the places with the lines between them, fitted to `frame` (lon, lat) when given, so
/// that the frame stays the same while the lines shown change, otherwise to the points.
/// `dot`: colour of the places' circles.
pub fn map(points: &[MapPoint], links: &[Link], frame: &[(f64, f64)], dot: &str, p: &Palette) -> String {
    let (w, h) = (1000., 820.);
    let (mut lon0, mut lon1, mut lat0, mut lat1) = (f64::MAX, f64::MIN, f64::MAX, f64::MIN);
    let fit: Vec<(f64, f64)> =
        if frame.is_empty() { points.iter().map(|pt| (pt.lon, pt.lat)).collect() } else { frame.to_vec() };
    for &(lon, lat) in &fit {
        lon0 = lon0.min(lon);
        lon1 = lon1.max(lon);
        lat0 = lat0.min(lat);
        lat1 = lat1.max(lat);
    }
    if fit.is_empty() {
        (lon0, lon1, lat0, lat1) = (0., 1., 0., 1.);
    }
    // Margin around the points, and at least a region's width.
    let pad_lon = ((lon1 - lon0) * 0.12).max(1.2);
    let pad_lat = ((lat1 - lat0) * 0.12).max(0.8);
    lon0 -= pad_lon;
    lon1 += pad_lon;
    lat0 -= pad_lat;
    lat1 += pad_lat;
    let k = ((lat0 + lat1) / 2.).to_radians().cos().max(0.2);
    // Same scale both ways: widen the shorter side to fill the frame.
    let sc = (w / ((lon1 - lon0) * k)).min(h / (lat1 - lat0));
    let extra_lon = (w / sc / k - (lon1 - lon0)) / 2.;
    let extra_lat = (h / sc - (lat1 - lat0)) / 2.;
    lon0 -= extra_lon;
    lon1 += extra_lon;
    lat0 -= extra_lat;
    lat1 += extra_lat;
    let at = |lon: f64, lat: f64| ((lon - lon0) * k * sc, (lat1 - lat) * sc);

    let mut s = open(w, h);
    let _ = write!(s, r##"<rect width="{w}" height="{h}" fill="{}"/>"##, p.sea);
    for line in COUNTRIES.lines() {
        let ring: Vec<(f64, f64)> = line
            .split(' ')
            .filter_map(|xy| {
                let (x, y) = xy.split_once(',')?;
                Some((x.parse().ok()?, y.parse().ok()?))
            })
            .collect();
        let inside = ring.iter().any(|&(x, y)| x >= lon0 - 5. && x <= lon1 + 5. && y >= lat0 - 5. && y <= lat1 + 5.);
        if !inside || ring.len() < 3 {
            continue;
        }
        s.push_str(r##"<path d="M"##);
        for (i, &(x, y)) in ring.iter().enumerate() {
            let (px, py) = at(x, y);
            let _ = write!(s, "{}{px:.1},{py:.1}", if i > 0 { " L" } else { "" });
        }
        let _ = write!(s, r##"Z" fill="{}" stroke="{}" stroke-width="0.8"/>"##, p.land, p.rule);
    }
    for l in links {
        let (x1, y1) = at(l.from.0, l.from.1);
        let (x2, y2) = at(l.to.0, l.to.1);
        let (mx, my) = ((x1 + x2) / 2., (y1 + y2) / 2.);
        let (dx, dy) = (x2 - x1, y2 - y1);
        let (cx, cy) = (mx - dy * 0.15, my + dx * 0.15);
        let (color, style) = match l.stroke {
            Stroke::Plain => {
                let _ = write!(
                    s,
                    r##"<path d="M{x1:.1},{y1:.1} Q{cx:.1},{cy:.1} {x2:.1},{y2:.1}" fill="none" stroke="{}" stroke-opacity="0.28" stroke-width="0.8"/>"##,
                    p.ink
                );
                continue;
            }
            Stroke::Paternal => (p.paternal, ""),
            Stroke::Maternal => (p.maternal, r##" stroke-dasharray="5 3""##),
        };
        let _ = write!(
            s,
            r##"<path d="M{x1:.1},{y1:.1} Q{cx:.1},{cy:.1} {x2:.1},{y2:.1}" fill="none" stroke="{color}" stroke-opacity="0.8" stroke-width="1.6"{style}/>"##
        );
        // The arrow, along the curve's end.
        let (tx, ty) = (x2 - cx, y2 - cy);
        let len = (tx * tx + ty * ty).sqrt().max(0.001);
        let (ux, uy) = (tx / len, ty / len);
        let (bx, by) = (x2 - ux * 8., y2 - uy * 8.);
        let _ = write!(
            s,
            r##"<path d="M{x2:.1},{y2:.1} L{:.1},{:.1} L{:.1},{:.1} Z" fill="{color}" fill-opacity="0.9"/>"##,
            bx - uy * 3.5,
            by + ux * 3.5,
            bx + uy * 3.5,
            by - ux * 3.5
        );
    }
    let mut sorted: Vec<&MapPoint> = points.iter().collect();
    sorted.sort_by_key(|pt| std::cmp::Reverse(pt.count));
    for pt in sorted.iter().rev() {
        let (x, y) = at(pt.lon, pt.lat);
        let r = 2.6 + 2.6 * (pt.count as f64).sqrt();
        let _ = write!(
            s,
            r##"<circle cx="{x:.1}" cy="{y:.1}" r="{r:.1}" fill="{dot}" fill-opacity="0.78" stroke="{}" stroke-width="1"/>"##,
            p.land
        );
    }
    // Labels of the main places, without overlaps.
    let mut taken: Vec<(f64, f64, f64)> = Vec::new();
    for pt in sorted.iter().take(24) {
        let (x, y) = at(pt.lon, pt.lat);
        let r = 2.6 + 2.6 * (pt.count as f64).sqrt();
        let label = format!("{} {}", pt.label, pt.count);
        let lw = label.chars().count() as f64 * 6.2;
        let (lx, ly) = (x + r + 3., y + 4.);
        if lx + lw > w || taken.iter().any(|&(tx, ty, tw)| lx < tx + tw && tx < lx + lw && (ly - ty).abs() < 14.) {
            continue;
        }
        taken.push((lx, ly, lw));
        let _ = write!(
            s,
            r##"<text x="{lx:.1}" y="{ly:.1}" font-size="11" font-weight="500" fill="{}" paint-order="stroke" stroke="{}" stroke-width="3">{} <tspan fill="{}" font-weight="400">{}</tspan></text>"##,
            p.ink,
            p.land,
            esc(&pt.label),
            p.muted,
            pt.count
        );
    }
    let _ = write!(s, r##"<rect width="{w}" height="{h}" fill="none" stroke="{}"/>"##, p.rule);
    s.push_str("</svg>");
    s
}

/// Town of a place, without its context: “Troyes”.
pub fn town(raw: &str) -> String {
    let short = gedcom::model::short_place(raw);
    match short.find(" (") {
        Some(i) if short.ends_with(')') => short[..i].to_string(),
        _ => short,
    }
}

/// The located places of the tree: name → (lon, lat).
pub(crate) fn coordinates(g: &gedcom::document::Gedcom) -> HashMap<String, (f64, f64)> {
    g.places()
        .into_iter()
        .filter_map(|pl| {
            let lat = gedcom::geo::parse_latitude(pl.latitude.as_deref()?)?;
            let lon = gedcom::geo::parse_longitude(pl.longitude.as_deref()?)?;
            Some((pl.name, (lon, lat)))
        })
        .collect()
}

impl Tree<'_> {
    /// Map of the places people come from, when at least two are located. The figure's `count`
    /// is the number of places.
    pub fn places(&self, p: &Palette) -> Option<Figure> {
        let g = self.g;
        let coords = coordinates(g);
        let origin = |x: &str| self.origins.place.get(x);
        let mut points: BTreeMap<String, MapPoint> = BTreeMap::new();
        let mut located = 0;
        for m in &self.members {
            let Some(pl) = origin(&m.xref) else { continue };
            let Some(&(lon, lat)) = coords.get(pl) else { continue };
            located += 1;
            points.entry(pl.clone()).or_insert_with(|| MapPoint { lon, lat, label: town(pl), count: 0 }).count += 1;
        }
        if points.len() < 2 {
            return None;
        }
        let in_scope: HashSet<&str> = self.members.iter().map(|m| m.xref.as_str()).collect();
        let mut links = Vec::new();
        for m in &self.members {
            let Some(a) = origin(&m.xref).and_then(|pl| coords.get(pl)) else { continue };
            let (f, mo) = g.parents(&m.xref);
            for parent in [f, mo].into_iter().flatten() {
                if in_scope.contains(parent.xref())
                    && let Some(b) = origin(parent.xref()).and_then(|pl| coords.get(pl))
                    && a != b
                {
                    links.push(Link { from: *a, to: *b, stroke: Stroke::Plain });
                }
            }
        }
        let mut table = Table::new(&[t!("viz.col.place"), t!("viz.col.people")]);
        let mut ranked: Vec<(&String, &MapPoint)> = points.iter().collect();
        ranked.sort_by(|a, b| b.1.count.cmp(&a.1.count).then(a.0.cmp(b.0)));
        for (name, pt) in ranked {
            table
                .rows
                .push(TableRow { cells: vec![gedcom::model::short_place(name), pt.count.to_string()], record: None });
        }
        let pts: Vec<MapPoint> = points.into_values().collect();
        Some(Figure {
            svg: map(&pts, &links, &[], p.accent, p),
            legend: Vec::new(),
            caption: t!("viz.places_caption", known = located, total = self.members.len()).into_owned(),
            count: pts.len(),
            table,
        })
    }
}
