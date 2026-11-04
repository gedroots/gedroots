//! The charts of a book, drawn as SVG from the data (Typst places them on the page): the fan of
//! ancestors, the map of places, the lifespans and the figures. Print colours: dark ink on white
//! paper, a few muted colours for the groups of places, and the violet of GEDRoots as the accent.

use std::fmt::Write;

pub const INK: &str = "#1F1B16";
pub const MUTED: &str = "#6B6257";
pub const RULE: &str = "#D5CAB7";
pub const ACCENT: &str = "#6D3DF0";
pub const PATERNAL: &str = "#3F5BD8";
pub const MATERNAL: &str = "#9C36B5";
/// Groups of places: muted colours that stay apart in greyscale, always named in a legend.
pub const GROUP_COLORS: [&str; 7] = ["#C9A35B", "#7E9C7A", "#8C86B8", "#C98A6B", "#5E8FA8", "#B8708A", "#D2B48C"];
pub const OTHER_COLOR: &str = "#A9A39A";

/// Escapes text for SVG.
fn esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

fn svg_open(w: f64, h: f64) -> String {
    format!(r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {w} {h}" font-family="Inter">"##)
}

// ---------------------------------------------------------------------------------------------
// Fan of ancestors
// ---------------------------------------------------------------------------------------------

/// A cell of the fan: Sosa number, labels and colour.
#[derive(Debug, Clone)]
pub struct FanCell {
    pub sosa: usize,
    pub given: String,
    pub surname: String,
    pub years: String,
    /// `None`: place unknown (white cell).
    pub color: Option<String>,
}

/// Half-circle fan over `generations` rings, the father's side on the left. `labels`: names in the
/// inner rings (off for the cover).
pub fn fan(cells: &[FanCell], generations: u32, labels: bool) -> String {
    let g_max = generations.clamp(2, 9) as usize;
    let (w, h) = (1000., 560.);
    let (cx, cy) = (500., 520.);
    // Rings narrow outwards, like the mockup: the outer generations only show their colour.
    let widths = [95., 70., 65., 60., 50., 40., 32., 28., 22.];
    let total: f64 = widths[..g_max].iter().sum();
    let scale = 440. / total;
    let mut radii = vec![0.];
    for wd in &widths[..g_max] {
        let last = *radii.last().unwrap();
        radii.push(last + wd * scale);
    }
    let pt = |r: f64, a: f64| (cx + r * a.to_radians().cos(), cy - r * a.to_radians().sin());
    let mut s = svg_open(w, h);
    for g in 0..g_max {
        let n = 1usize << g;
        let span = 180. / n as f64;
        let (r0, r1) = (radii[g], radii[g + 1]);
        for k in 0..n {
            let sosa = n + k;
            let cell = cells.iter().find(|c| c.sosa == sosa);
            let a0 = 180. - k as f64 * span;
            let a1 = a0 - span;
            let fill = cell.and_then(|c| c.color.as_deref()).unwrap_or("#FFFFFF");
            let stroke = if cell.is_some() { "#FFFFFF" } else { RULE };
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
            let mid = (a0 + a1) / 2.;
            let text = |s: &mut String, x: f64, y: f64, rot: f64, size: f64, weight: u32, t: &str| {
                let _ = write!(
                    s,
                    r##"<text x="{x:.1}" y="{y:.1}" transform="rotate({rot:.1} {x:.1} {y:.1})" text-anchor="middle" dominant-baseline="middle" font-size="{size}" font-weight="{weight}" fill="{INK}">{}</text>"##,
                    esc(t)
                );
            };
            if g == 0 {
                text(&mut s, cx, cy - 46., 0., 17., 600, &c.given);
                text(&mut s, cx, cy - 27., 0., 12., 600, &c.surname);
                text(&mut s, cx, cy - 10., 0., 11., 400, &c.years);
            } else if g <= 2 {
                let rm = (r0 + r1) / 2.;
                let rot = 90. - mid;
                let lines = [(&c.given, 12., 600), (&c.surname, 10., 600), (&c.years, 9.5, 400)];
                for (j, (t, size, weight)) in lines.iter().enumerate() {
                    let (x, y) = pt(rm - (j as f64 - 1.) * 14. * scale.min(1.2), mid);
                    text(&mut s, x, y, rot, *size, *weight, t);
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
                    text(&mut s, x, y, rot, size, if j < 2 { 600 } else { 400 }, t);
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
                    r##"<text x="{x:.1}" y="{}" text-anchor="middle" font-size="9" fill="{MUTED}">{}</text>"##,
                    cy + 16.,
                    g + 1
                );
            }
        }
    }
    s.push_str("</svg>");
    s
}

// ---------------------------------------------------------------------------------------------
// Map
// ---------------------------------------------------------------------------------------------

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

/// A line between two places: ((lon, lat), (lon, lat)).
pub type Link = ((f64, f64), (f64, f64));

/// Map of the places, fitted to the points, with the lines between them (child → parent).
pub fn map(points: &[MapPoint], links: &[Link]) -> String {
    let (w, h) = (1000., 820.);
    let (mut lon0, mut lon1, mut lat0, mut lat1) = (f64::MAX, f64::MIN, f64::MAX, f64::MIN);
    for p in points {
        lon0 = lon0.min(p.lon);
        lon1 = lon1.max(p.lon);
        lat0 = lat0.min(p.lat);
        lat1 = lat1.max(p.lat);
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
    let p = |lon: f64, lat: f64| ((lon - lon0) * k * sc, (lat1 - lat) * sc);

    let mut s = svg_open(w, h);
    let _ = write!(s, r##"<rect width="{w}" height="{h}" fill="#EEF1F2"/>"##);
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
            let (px, py) = p(x, y);
            let _ = write!(s, "{}{px:.1},{py:.1}", if i > 0 { " L" } else { "" });
        }
        let _ = write!(s, r##"Z" fill="#FFFFFF" stroke="{RULE}" stroke-width="0.8"/>"##);
    }
    for &((ax, ay), (bx, by)) in links {
        let (x1, y1) = p(ax, ay);
        let (x2, y2) = p(bx, by);
        let (mx, my) = ((x1 + x2) / 2., (y1 + y2) / 2.);
        let (dx, dy) = (x2 - x1, y2 - y1);
        let _ = write!(
            s,
            r##"<path d="M{x1:.1},{y1:.1} Q{:.1},{:.1} {x2:.1},{y2:.1}" fill="none" stroke="{INK}" stroke-opacity="0.28" stroke-width="0.8"/>"##,
            mx - dy * 0.15,
            my + dx * 0.15
        );
    }
    let mut sorted: Vec<&MapPoint> = points.iter().collect();
    sorted.sort_by_key(|p| std::cmp::Reverse(p.count));
    for pt in sorted.iter().rev() {
        let (x, y) = p(pt.lon, pt.lat);
        let r = 2.6 + 2.6 * (pt.count as f64).sqrt();
        let _ = write!(
            s,
            r##"<circle cx="{x:.1}" cy="{y:.1}" r="{r:.1}" fill="{ACCENT}" fill-opacity="0.78" stroke="#FFFFFF" stroke-width="1"/>"##
        );
    }
    // Labels of the main places, without overlaps.
    let mut taken: Vec<(f64, f64, f64)> = Vec::new();
    for pt in sorted.iter().take(24) {
        let (x, y) = p(pt.lon, pt.lat);
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
            r##"<text x="{lx:.1}" y="{ly:.1}" font-size="11" font-weight="500" fill="{INK}" paint-order="stroke" stroke="#FFFFFF" stroke-width="3">{} <tspan fill="{MUTED}" font-weight="400">{}</tspan></text>"##,
            esc(&pt.label),
            pt.count
        );
    }
    let _ = write!(s, r##"<rect width="{w}" height="{h}" fill="none" stroke="{RULE}"/>"##);
    s.push_str("</svg>");
    s
}

// ---------------------------------------------------------------------------------------------
// Lifespans
// ---------------------------------------------------------------------------------------------

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
pub fn lifespans(lives: &[Life], now: i32, marriage: &str, ongoing: &str, age_label: impl Fn(i32) -> String) -> String {
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
    let mut s = svg_open(w, h);
    let mut tick = y0;
    while tick <= y1 {
        let tx = x(tick as f64);
        let _ = write!(
            s,
            r##"<line x1="{tx:.1}" y1="{top}" x2="{tx:.1}" y2="{bottom}" stroke="{RULE}" stroke-width="0.6"/><text x="{tx:.1}" y="{}" text-anchor="middle" font-size="10" fill="{MUTED}">{tick}</text>"##,
            bottom + 16.
        );
        tick += step;
    }
    for (i, l) in lives.iter().enumerate() {
        let yy = top + i as f64 * row + row / 2.;
        if l.break_before && i > 0 {
            let _ = write!(
                s,
                r##"<line x1="0" y1="{0:.1}" x2="{w}" y2="{0:.1}" stroke="{RULE}" stroke-width="0.8"/>"##,
                yy - row / 2.
            );
        }
        let _ = write!(
            s,
            r##"<text x="{}" y="{:.1}" text-anchor="end" font-size="11" fill="{INK}">{}</text>"##,
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
                r##"<line x1="{mx:.1}" y1="{:.1}" x2="{mx:.1}" y2="{:.1}" stroke="{INK}" stroke-width="1.4"/>"##,
                yy - 8.,
                yy + 8.
            );
        }
        if let Some(d) = l.death {
            let _ = write!(
                s,
                r##"<text x="{:.1}" y="{:.1}" font-size="9.5" fill="{MUTED}">{}</text>"##,
                x(d as f64) + 5.,
                yy + 4.,
                esc(&age_label(d - l.birth))
            );
        }
    }
    let ly = bottom + 36.;
    let _ = write!(
        s,
        r##"<line x1="{left}" y1="{ly}" x2="{left}" y2="{}" stroke="{INK}" stroke-width="1.4"/><text x="{}" y="{}" font-size="10" fill="{MUTED}">{}</text>"##,
        ly + 14.,
        left + 8.,
        ly + 11.,
        esc(marriage)
    );
    let ox = left + 30. + marriage.chars().count() as f64 * 6.;
    let _ = write!(
        s,
        r##"<rect x="{ox:.1}" y="{}" width="30" height="10" rx="5" fill="{OTHER_COLOR}" fill-opacity="0.45"/><text x="{:.1}" y="{}" font-size="10" fill="{MUTED}">{}</text>"##,
        ly + 2.,
        ox + 38.,
        ly + 11.,
        esc(ongoing)
    );
    s.push_str("</svg>");
    s
}

// ---------------------------------------------------------------------------------------------
// Figures
// ---------------------------------------------------------------------------------------------

/// Bars of the ancestors found in each generation: (found, expected).
pub fn completeness(gens: &[(usize, usize)]) -> String {
    let (w, h) = (1000., 300.);
    let bw = w / gens.len().max(6) as f64;
    let mut s = svg_open(w, h);
    for (g, &(found, expected)) in gens.iter().enumerate() {
        let pct = found as f64 / expected.max(1) as f64;
        let x = g as f64 * bw + 14.;
        let inner = bw - 28.;
        let bar = 220.;
        let mid = x + inner / 2.;
        let _ = write!(
            s,
            r##"<rect x="{x:.1}" y="20" width="{inner:.1}" height="{bar}" fill="#F4EFE6"/><rect x="{x:.1}" y="{:.1}" width="{inner:.1}" height="{:.1}" fill="{ACCENT}" fill-opacity="0.85"/>"##,
            20. + bar * (1. - pct),
            bar * pct
        );
        let _ = write!(
            s,
            r##"<text x="{mid:.1}" y="{:.1}" text-anchor="middle" font-size="12" font-weight="600" fill="{INK}">{} %</text><text x="{mid:.1}" y="262" text-anchor="middle" font-size="11" fill="{INK}">{found} / {expected}</text><text x="{mid:.1}" y="282" text-anchor="middle" font-size="10" fill="{MUTED}">G{}</text>"##,
            20. + bar * (1. - pct) - 6.,
            (pct * 100.).round(),
            g + 1
        );
    }
    s.push_str("</svg>");
    s
}

/// Age at death by year of birth: (birth year, age, sex). Men as squares, women as circles, with
/// the median per half-century.
pub fn longevity(points: &[(i32, f64, char)]) -> String {
    let (w, h, left, base) = (1000., 420., 50., 380.);
    let min = points.iter().map(|p| p.0).min().unwrap_or(1800) / 50 * 50;
    let max = (points.iter().map(|p| p.0).max().unwrap_or(1950) / 50 + 1) * 50;
    let x = |y: f64| left + (y - min as f64) / (max - min) as f64 * (w - left - 20.);
    let y = |a: f64| base - a / 100. * (base - 20.);
    let mut s = svg_open(w, h);
    for a in (0..=100).step_by(20) {
        let yy = y(a as f64);
        let _ = write!(
            s,
            r##"<line x1="{left}" y1="{yy:.1}" x2="{}" y2="{yy:.1}" stroke="{RULE}" stroke-width="0.6"/><text x="{}" y="{:.1}" text-anchor="end" font-size="10" fill="{MUTED}">{a}</text>"##,
            w - 20.,
            left - 8.,
            yy + 4.
        );
    }
    let mut year = min;
    while year <= max {
        let _ = write!(
            s,
            r##"<text x="{:.1}" y="{}" text-anchor="middle" font-size="10" fill="{MUTED}">{year}</text>"##,
            x(year as f64),
            base + 20.
        );
        year += 50;
    }
    for &(b, a, sex) in points {
        let (px, py) = (x(b as f64), y(a));
        if sex == 'F' {
            let _ = write!(
                s,
                r##"<circle cx="{px:.1}" cy="{py:.1}" r="4" fill="none" stroke="{MATERNAL}" stroke-width="1.4"/>"##
            );
        } else {
            let _ = write!(
                s,
                r##"<rect x="{:.1}" y="{:.1}" width="7" height="7" fill="{PATERNAL}" fill-opacity="0.75"/>"##,
                px - 3.5,
                py - 3.5
            );
        }
    }
    let mut by: std::collections::BTreeMap<i32, Vec<f64>> = Default::default();
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
        let _ = write!(s, r##"" fill="none" stroke="{INK}" stroke-width="2"/>"##);
    }
    s.push_str("</svg>");
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn charts_are_well_formed() {
        let cells = vec![
            FanCell {
                sosa: 1,
                given: "Jean".into(),
                surname: "MARCHAND & fils".into(),
                years: "1850–1912".into(),
                color: Some(GROUP_COLORS[0].into()),
            },
            FanCell { sosa: 3, given: "Marie".into(), surname: "HÉBERT".into(), years: "".into(), color: None },
        ];
        let f = fan(&cells, 4, true);
        assert!(f.starts_with("<svg") && f.ends_with("</svg>"));
        assert!(f.contains("MARCHAND &amp; fils"));
        let m = map(
            &[
                MapPoint { lon: -1.09, lat: 49.11, label: "Saint-Lô".into(), count: 3 },
                MapPoint { lon: 2.35, lat: 48.85, label: "Paris".into(), count: 1 },
            ],
            &[((-1.09, 49.11), (2.35, 48.85))],
        );
        assert!(m.contains("Saint-Lô") && m.contains("<path"));
        let l = lifespans(
            &[Life {
                label: "Jean".into(),
                birth: 1850,
                death: Some(1912),
                marriages: vec![1875],
                color: OTHER_COLOR.into(),
                break_before: false,
            }],
            2026,
            "Marriage",
            "Living",
            |a| format!("{a} y"),
        );
        assert!(l.contains("62 y"));
        assert!(completeness(&[(1, 1), (2, 2), (3, 4)]).contains("75 %"));
        assert!(longevity(&[(1850, 62., 'M'), (1860, 70., 'F')]).contains("<circle"));
    }
}
