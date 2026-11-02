//! Copies d'un fichier (historique d'annulation) : les enregistrements non modifiés sont partagés.

use gedcom::*;
use std::sync::Arc;

const SAMPLE: &str = include_str!("fixtures/exemple.ged");

fn shared(a: &Gedcom, b: &Gedcom) -> usize {
    a.records.iter().zip(&b.records).filter(|(x, y)| Arc::ptr_eq(x, y)).count()
}

#[test]
fn une_modification_ne_copie_que_son_enregistrement() {
    let before = Gedcom::parse(SAMPLE).unwrap();
    let mut g = before.clone();
    assert_eq!(shared(&before, &g), before.records.len());
    g.get_mut("I00003").unwrap().set_child_val("SEX", "M");
    assert_eq!(shared(&before, &g), before.records.len() - 1);
    // La copie gardée pour l'annulation n'a pas bougé.
    assert_eq!(before.get("I00003").unwrap().child("SEX").unwrap().val(), "F");
    assert_eq!(before.to_gedcom_string(), SAMPLE);
}

#[test]
fn supprimer_ne_copie_que_les_enregistrements_qui_pointent() {
    let before = Gedcom::parse(SAMPLE).unwrap();
    let mut g = before.clone();
    g.delete("I00003").unwrap();
    // La famille perd son lien vers I00003 ; les autres enregistrements restent partagés.
    let untouched = g.records.iter().filter(|r| before.records.iter().any(|b| Arc::ptr_eq(r, b))).count();
    let referrers = before.referrers("I00003").len();
    assert_eq!(untouched, g.records.len() - referrers);
    assert!(g.get("F00001").unwrap().children.iter().all(|c| c.val() != "@I00003@"));
    assert_eq!(before.to_gedcom_string(), SAMPLE);
}
