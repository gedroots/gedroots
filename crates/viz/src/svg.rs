//! Small helpers to write SVG by hand.

/// Escapes text for SVG.
pub fn esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

/// Opening tag of a drawing of `w` × `h` units, in Inter (the fonts of the app and the books).
pub fn open(w: f64, h: f64) -> String {
    format!(r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {w} {h}" font-family="Inter">"##)
}

/// Size of a drawing from its `viewBox`.
pub fn size(svg: &str) -> Option<(f64, f64)> {
    let rest = &svg[svg.find("viewBox=\"")? + 9..];
    let mut n = rest[..rest.find('"')?].split_whitespace().filter_map(|v| v.parse::<f64>().ok());
    let (_, _, w, h) = (n.next()?, n.next()?, n.next()?, n.next()?);
    Some((w, h))
}
