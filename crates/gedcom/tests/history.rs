//! Copies of a file (undo history): unchanged records are shared.

use gedcom::*;
use std::sync::Arc;

const SAMPLE: &str = include_str!("fixtures/example.ged");

fn shared(a: &Gedcom, b: &Gedcom) -> usize {
    a.records.iter().zip(&b.records).filter(|(x, y)| Arc::ptr_eq(x, y)).count()
}

#[test]
fn a_change_only_copies_its_record() {
    let before = Gedcom::parse(SAMPLE).unwrap();
    let mut g = before.clone();
    assert_eq!(shared(&before, &g), before.records.len());
    g.get_mut("I00003").unwrap().set_child_val("SEX", "M");
    assert_eq!(shared(&before, &g), before.records.len() - 1);
    // The copy kept for undo did not change.
    assert_eq!(before.get("I00003").unwrap().child("SEX").unwrap().val(), "F");
    assert_eq!(before.to_gedcom_string(), SAMPLE);
}

#[test]
fn delete_only_copies_the_records_pointing_to_it() {
    let before = Gedcom::parse(SAMPLE).unwrap();
    let mut g = before.clone();
    g.delete("I00003").unwrap();
    // The family loses its link to I00003; the other records stay shared.
    let untouched = g.records.iter().filter(|r| before.records.iter().any(|b| Arc::ptr_eq(r, b))).count();
    let referrers = before.referrers("I00003").len();
    assert_eq!(untouched, g.records.len() - referrers);
    assert!(g.get("F00001").unwrap().children.iter().all(|c| c.val() != "@I00003@"));
    assert_eq!(before.to_gedcom_string(), SAMPLE);
}

#[test]
fn save_stamps_the_records_changed_without_a_stamp() {
    let saved = Gedcom::parse(SAMPLE).unwrap();
    let mut g = saved.clone();
    // An edit that forgot to stamp the record.
    g.get_mut("I00003").unwrap().set_child_val("SEX", "M");
    // An edit that stamped it: its stamp is kept.
    g.get_mut("I00001").unwrap().touch();
    let kept = g.get("I00001").unwrap().child("CHAN").cloned();
    // A record rewritten identically is not a change.
    let same = (*saved.records[1]).clone();
    let xref = same.xref.clone().unwrap();
    g.replace(&xref, same).unwrap();
    g.stamp_changes(&saved);
    assert!(g.get("I00003").unwrap().child("CHAN").is_some_and(|c| c.child("DATE").is_some()));
    assert_eq!(g.get("I00001").unwrap().child("CHAN").cloned(), kept);
    assert_eq!(g.get(&xref).unwrap().child("CHAN"), saved.get(&xref).unwrap().child("CHAN"));
    // Records not edited stay shared with the saved copy.
    let untouched = g.records.iter().zip(&saved.records).filter(|(a, b)| Arc::ptr_eq(a, b)).count();
    assert!(untouched >= saved.records.len() - 3);
}

#[test]
fn save_stamps_the_creation_of_new_records() {
    let saved = Gedcom::parse(SAMPLE).unwrap();
    let mut g = saved.clone();
    let mut n = gedcom::Node::new("INDI");
    n.xref = Some("I09999".into());
    g.records.push(Arc::new(n));
    g.reindex();
    g.stamp_changes(&saved);
    let r = g.get("I09999").unwrap();
    assert!(r.child("CREA").is_some() && r.child("CHAN").is_some());
}

#[test]
fn the_tree_identifier_is_given_once() {
    let mut g = Gedcom::parse(SAMPLE).unwrap();
    assert_eq!(g.tree_id(), None);
    let id = g.ensure_tree_id();
    assert_eq!(id.len(), 36);
    assert_eq!(g.ensure_tree_id(), id);
    let text = g.to_gedcom_string();
    assert!(text.contains(&format!("1 _GR_TREE {id}")));
    assert!(text.contains("2 TAG _GR_TREE "));
    assert_eq!(Gedcom::parse(&text).unwrap().tree_id(), Some(id.as_str()));
}
