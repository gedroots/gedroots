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
