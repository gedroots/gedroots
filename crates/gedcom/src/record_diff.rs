//! What changed in a record between two of its states, field by field, in the genealogist's
//! words: “Birth › Date: 1796 → 30 germinal an IV”, “Father: — → Jacques MARCHAND”.
//!
//! Used by the record history (`versions::Store::record_history`). Facts are matched by tag (and
//! `TYPE`), in order; a fact that only moved is not a change. Change dates (`CHAN`, `CREA`) are
//! left out: they change with every edit.

use crate::date::Date;
use crate::node::Node;
use crate::schema::{self, StructDef};
use rust_i18n::t;

/// One field that changed: its label, and its value before and after (`None`: absent).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldChange {
    /// Label of the field, with the facts it is in: “Birth › Date”.
    pub label: String,
    pub before: Option<String>,
    pub after: Option<String>,
}

/// Fields that changed from `before` to `after` (two states of the same record). `name` gives the
/// name of a linked record (father, source…).
pub fn field_changes(before: Option<&Node>, after: Option<&Node>, name: &dyn Fn(&str) -> String) -> Vec<FieldChange> {
    let Some(tag) = after.or(before).map(|n| n.tag.as_str()) else { return Vec::new() };
    let def = schema::record(tag);
    let mut out = Vec::new();
    let empty = Node::new(tag);
    let (a, b) = (before.unwrap_or(&empty), after.unwrap_or(&empty));
    if a.value != b.value {
        // A shared note's text is the record's value.
        out.push(FieldChange { label: t!("record_diff.text").into(), before: text(a, name), after: text(b, name) });
    }
    diff_children(def, "", &a.children, &b.children, name, &mut out);
    out
}

/// Labels of the facts that changed, once each: “Birth, Occupation”.
pub fn summary(changes: &[FieldChange]) -> String {
    let mut seen: Vec<&str> = Vec::new();
    for c in changes {
        let top = c.label.split(" › ").next().unwrap_or(&c.label);
        if !seen.contains(&top) {
            seen.push(top);
        }
    }
    seen.join(", ")
}

fn ignored(n: &Node) -> bool {
    matches!(n.tag.as_str(), "CHAN" | "CREA")
}

/// Key that pairs two states of a fact: its tag, and its type for custom events.
fn key(n: &Node) -> (String, String) {
    (n.tag.clone(), n.child_val("TYPE").unwrap_or_default().to_string())
}

fn label(parent: Option<&'static StructDef>, n: &Node) -> String {
    if matches!(n.tag.as_str(), "EVEN" | "FACT")
        && let Some(ty) = n.child_val("TYPE").filter(|t| !t.is_empty())
    {
        return crate::model::event_type_label(ty);
    }
    if let Some(def) = parent.and_then(|p| p.sub(&n.tag)).map(|s| s.def()) {
        return schema::label(&n.tag, def).into_owned();
    }
    crate::model::event_name(&n.tag)
}

fn diff_children(
    parent: Option<&'static StructDef>,
    prefix: &str,
    a: &[Node],
    b: &[Node],
    name: &dyn Fn(&str) -> String,
    out: &mut Vec<FieldChange>,
) {
    let mut left: Vec<&Node> = a.iter().filter(|n| !ignored(n)).collect();
    let mut right: Vec<&Node> = b.iter().filter(|n| !ignored(n)).collect();
    // The same fact on both sides (perhaps moved): not a change.
    left.retain(|n| match right.iter().position(|m| same(n, m)) {
        Some(i) => {
            right.remove(i);
            false
        }
        None => true,
    });
    let mut keys: Vec<(String, String)> = Vec::new();
    for n in left.iter().chain(right.iter()) {
        if !keys.contains(&key(n)) {
            keys.push(key(n));
        }
    }
    for k in keys {
        let olds: Vec<&Node> = left.iter().copied().filter(|n| key(n) == k).collect();
        let news: Vec<&Node> = right.iter().copied().filter(|n| key(n) == k).collect();
        for i in 0..olds.len().max(news.len()) {
            let (o, n) = (olds.get(i).copied(), news.get(i).copied());
            let here = o.or(n).unwrap();
            let l = format!("{prefix}{}", label(parent, here));
            match (o, n) {
                (Some(o), Some(n)) => {
                    let def = parent.and_then(|p| p.sub(&here.tag)).map(|s| s.def());
                    if o.value != n.value {
                        out.push(FieldChange { label: l.clone(), before: text(o, name), after: text(n, name) });
                    }
                    diff_children(def, &format!("{l} › "), &o.children, &n.children, name, out);
                }
                _ => out.push(FieldChange {
                    label: l,
                    before: o.map(|o| describe(o, name)),
                    after: n.map(|n| describe(n, name)),
                }),
            }
        }
    }
}

/// Same fact, change dates aside.
fn same(a: &Node, b: &Node) -> bool {
    a.tag == b.tag
        && a.value == b.value
        && a.children.iter().filter(|c| !ignored(c)).count() == b.children.iter().filter(|c| !ignored(c)).count()
        && a.children
            .iter()
            .filter(|c| !ignored(c))
            .zip(b.children.iter().filter(|c| !ignored(c)))
            .all(|(x, y)| same(x, y))
}

/// Value of a field as read: a date in words, the name of a linked record, a sex.
fn text(n: &Node, name: &dyn Fn(&str) -> String) -> Option<String> {
    let v = n.val();
    if v.is_empty() {
        return None;
    }
    if let Some(x) = n.pointer() {
        return Some(name(x));
    }
    Some(match n.tag.as_str() {
        "DATE" => Date::parse(v).display(),
        "NAME" => v.replace('/', "").split_whitespace().collect::<Vec<_>>().join(" "),
        "SEX" => match v {
            "M" => t!("sex.male").into_owned(),
            "F" => t!("sex.female").into_owned(),
            _ => t!("sex.unknown").into_owned(),
        },
        "PLAC" => v.split(',').map(str::trim).filter(|s| !s.is_empty()).collect::<Vec<_>>().join(", "),
        _ => {
            let one: String = v.split_whitespace().collect::<Vec<_>>().join(" ");
            match one.char_indices().nth(120) {
                Some((i, _)) => format!("{}…", &one[..i]),
                None => one,
            }
        }
    })
}

/// A whole fact in one line: its value, date and place.
fn describe(n: &Node, name: &dyn Fn(&str) -> String) -> String {
    let mut parts: Vec<String> = Vec::new();
    if let Some(v) = text(n, name).filter(|v| v != "Y") {
        parts.push(v);
    }
    for tag in ["DATE", "PLAC"] {
        if let Some(c) = n.child(tag).and_then(|c| text(c, name)) {
            parts.push(c);
        }
    }
    if parts.is_empty() { t!("record_diff.present").into_owned() } else { parts.join(" · ") }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Gedcom;

    fn record(lines: &str) -> Node {
        let g = Gedcom::parse(&format!("0 HEAD\n1 GEDC\n2 VERS 7.0\n{lines}0 TRLR\n")).unwrap();
        (*g.records[0]).clone()
    }

    #[test]
    fn changes_are_listed_by_field() {
        let before = record(
            "0 @I1@ INDI\n1 NAME Émile /MARCHAND/\n1 BIRT\n2 DATE 1796\n1 OCCU laboureur\n1 CHAN\n2 DATE 1 JAN 2020\n",
        );
        let after = record(
            "0 @I1@ INDI\n1 NAME Émile /MARCHAND/\n1 OCCU cultivateur\n1 BIRT\n2 DATE 1796\n2 PLAC Agneaux\n1 FAMC @F1@\n1 CHAN\n2 DATE 3 NOV 2026\n",
        );
        let name = |x: &str| if x == "F1" { "Jacques MARCHAND & Marie HÉBERT".to_string() } else { x.to_string() };
        let c = field_changes(Some(&before), Some(&after), &name);
        assert_eq!(c.len(), 3, "{c:?}");
        assert!(
            c.iter().any(|c| c.before.as_deref() == Some("laboureur") && c.after.as_deref() == Some("cultivateur"))
        );
        assert!(
            c.iter().any(|c| c.label.contains(" › ") && c.before.is_none() && c.after.as_deref() == Some("Agneaux"))
        );
        assert!(c.iter().any(|c| c.before.is_none() && c.after.as_deref() == Some("Jacques MARCHAND & Marie HÉBERT")));
        // A record that only moved its facts and got a new change date did not change.
        let moved = record("0 @I1@ INDI\n1 BIRT\n2 DATE 1796\n1 NAME Émile /MARCHAND/\n1 OCCU laboureur\n");
        assert!(field_changes(Some(&before), Some(&moved), &name).is_empty());
        // A new record: every field is added.
        assert_eq!(field_changes(None, Some(&before), &name).len(), 3);
    }
}
