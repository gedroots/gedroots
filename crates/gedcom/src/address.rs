//! Street address of an event (`ADDR`), kept apart from its place (`PLAC`).
//!
//! The place names the municipality and its jurisdictions, shared by every event that happened
//! there; the address says where in it: “43, rue de la République”, “Camp de Satory”, “Hôtel-Dieu”.
//! GEDCOM 7 writes it as the payload of `ADDR`, line by line (`CONT`); the `ADR1`–`ADR3` lines are
//! deprecated and only read, from older files. `CITY`, `POST`, `STAE` and `CTRY` repeat the place
//! and are left as they are.

use crate::node::Node;

/// Deprecated address lines, read when the payload is empty.
const LINES: [&str; 3] = ["ADR1", "ADR2", "ADR3"];

/// Address of an event or fact, on one line (“43, rue de la République”): its `ADDR` payload, its
/// lines joined by commas, otherwise its `ADR1`–`ADR3` lines. `None` when there is none.
pub fn address_of(node: &Node) -> Option<String> {
    let addr = node.child("ADDR")?;
    let text = one_line(&addr.text());
    if !text.is_empty() {
        return Some(text);
    }
    let lines: Vec<&str> =
        LINES.iter().filter_map(|t| addr.child_val(t)).map(str::trim).filter(|l| !l.is_empty()).collect();
    (!lines.is_empty()).then(|| lines.join(", "))
}

/// Lines of an address joined by commas, blank lines dropped.
pub fn one_line(text: &str) -> String {
    text.lines().map(str::trim).filter(|l| !l.is_empty()).collect::<Vec<_>>().join(", ")
}

/// `ADDR` structure of a typed address, or `None` if blank.
pub fn address_node(text: &str) -> Option<Node> {
    let text = text.trim();
    (!text.is_empty()).then(|| Node::with_value("ADDR", text))
}

/// Sets the address of an event or fact: blank removes `ADDR`; the address it already shows
/// leaves it untouched (lines, `CITY`, `POST`…); another one replaces its text, and the
/// deprecated `ADR1`–`ADR3` lines that spelled the old one. A new `ADDR` goes right after `PLAC`.
pub fn set_address(node: &mut Node, text: &str) {
    let text = text.trim();
    if text.is_empty() {
        node.children.retain(|c| c.tag != "ADDR");
        return;
    }
    if address_of(node).as_deref() == Some(text) {
        return;
    }
    match node.child_mut("ADDR") {
        Some(a) => {
            a.set_text(text);
            a.children.retain(|c| !LINES.contains(&c.tag.as_str()));
        }
        None => {
            let new = Node::with_value("ADDR", text);
            match node.children.iter().position(|c| c.tag == "PLAC") {
                Some(i) => node.children.insert(i + 1, new),
                None => node.insert_child(new),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse::node_from_string;

    /// An event written from level 1, as in a record.
    fn ev(s: &str) -> Node {
        let text: String = s
            .lines()
            .map(|l| {
                let (level, rest) = l.split_once(' ').unwrap();
                format!("{} {rest}\n", level.parse::<u8>().unwrap() - 1)
            })
            .collect();
        node_from_string(&text).unwrap()
    }

    #[test]
    fn reads_payload_lines_or_deprecated_lines() {
        assert_eq!(
            address_of(&ev("1 RESI\n2 ADDR 43, Rue de la République\n")).as_deref(),
            Some("43, Rue de la République")
        );
        assert_eq!(
            address_of(&ev("1 RESI\n2 ADDR Hôtel-Dieu\n3 CONT 1, place de l'Hôpital\n")).as_deref(),
            Some("Hôtel-Dieu, 1, place de l'Hôpital")
        );
        assert_eq!(
            address_of(&ev("1 DEAT\n2 ADDR\n3 ADR1 Hôpital de l'Hôtel-Dieu\n")).as_deref(),
            Some("Hôpital de l'Hôtel-Dieu")
        );
        assert_eq!(address_of(&ev("1 DEAT\n2 PLAC Lyon\n")), None);
    }

    #[test]
    fn writes_after_the_place_and_keeps_an_unchanged_address() {
        let mut e = ev("1 RESI\n2 DATE 1901\n2 PLAC Lyon\n2 SOUR @S1@\n");
        set_address(&mut e, " 7, rue Mercière ");
        assert_eq!(e.children[2].tag, "ADDR");
        assert_eq!(e.children[2].val(), "7, rue Mercière");

        // Shown on one line, kept as written.
        let mut e = ev("1 DEAT\n2 ADDR\n3 ADR1 Hôpital de l'Hôtel-Dieu\n3 CITY Lyon\n");
        let before = e.clone();
        set_address(&mut e, "Hôpital de l'Hôtel-Dieu");
        assert_eq!(e, before);

        // Changed: the payload replaces the deprecated lines, the city stays.
        set_address(&mut e, "Hôtel-Dieu, 1 place de l'Hôpital");
        let a = e.child("ADDR").unwrap();
        assert_eq!(a.val(), "Hôtel-Dieu, 1 place de l'Hôpital");
        assert!(a.child("ADR1").is_none());
        assert_eq!(a.child_val("CITY"), Some("Lyon"));

        set_address(&mut e, "");
        assert!(e.child("ADDR").is_none());
    }
}
