use gedcom::*;

const GED: &str = "0 HEAD
1 GEDC
2 VERS 7.0
0 @I1@ INDI
1 NAME Jean /Guyot/
1 BIRT
2 DATE 3 MAR 1850
2 PLAC Troyes, Aube
3 MAP
4 LATI N48.3
4 LONG E4.08
2 SOUR @S1@
3 PAGE folio 12
2 NOTE né à la ferme
1 OCCU Meunier
1 RESI Paris
1 DEAT Y
0 @I2@ INDI
1 BIRT
2 PLAC Reims, Marne
3 MAP
4 LATI N49.25
4 LONG E4.03
0 @S1@ SOUR
1 TITL Registre de Troyes
0 @S2@ SOUR
1 TITL Registre de Reims
0 TRLR
";

fn idx(g: &Gedcom, tag: &str) -> usize {
    g.events_of("I1").iter().find(|(_, e)| e.tag() == tag).unwrap().0
}

#[test]
fn read_event_fields() {
    let g = Gedcom::parse(GED).unwrap();
    let ev = g.read_event("I1", idx(&g, "BIRT")).unwrap();
    assert_eq!(ev.tag, "BIRT");
    assert_eq!(ev.date, "3 MAR 1850");
    assert_eq!(ev.place, "Troyes, Aube");
    assert_eq!(ev.source.as_deref(), Some("S1"));
    let occu = g.read_event("I1", idx(&g, "OCCU")).unwrap();
    assert_eq!(occu.detail, "Meunier");
    // Pas un événement : refusé.
    assert!(g.read_event("I1", 0).is_none());
}

#[test]
fn update_keeps_other_substructures() {
    let mut g = Gedcom::parse(GED).unwrap();
    let i = idx(&g, "BIRT");
    let mut ev = g.read_event("I1", i).unwrap();
    ev.date = "abt 1851".into();
    g.update_event("I1", i, &ev).unwrap();
    let n = &g.get("I1").unwrap().children[i];
    assert_eq!(n.child_val("DATE"), Some("ABT 1851"));
    // Lieu inchangé : coordonnées, page de la source et note conservées.
    assert!(n.child("PLAC").unwrap().child("MAP").is_some());
    assert_eq!(n.child("SOUR").unwrap().child_val("PAGE"), Some("folio 12"));
    assert_eq!(n.child_val("NOTE"), Some("né à la ferme"));
}

#[test]
fn update_place_and_source() {
    let mut g = Gedcom::parse(GED).unwrap();
    let i = idx(&g, "BIRT");
    let mut ev = g.read_event("I1", i).unwrap();
    ev.place = "Reims, Marne".into();
    ev.source = Some("S2".into());
    g.update_event("I1", i, &ev).unwrap();
    let n = &g.get("I1").unwrap().children[i];
    // Nouveau lieu : on reprend les coordonnées connues ailleurs.
    assert_eq!(n.child("PLAC").unwrap().path_val(&["MAP", "LATI"]), Some("N49.25"));
    // Nouvelle source : l'ancienne page ne suit pas.
    let sour = n.child("SOUR").unwrap();
    assert_eq!(sour.pointer(), Some("S2"));
    assert!(sour.child("PAGE").is_none());

    // Retirer lieu et source.
    ev.place.clear();
    ev.source = None;
    g.update_event("I1", i, &ev).unwrap();
    let n = &g.get("I1").unwrap().children[i];
    assert!(n.child("PLAC").is_none() && n.child("SOUR").is_none());
    assert!(g.update_event("I1", i, &NewEvent { source: Some("S9".into()), ..ev.clone() }).is_err());
}

#[test]
fn change_type_and_detail() {
    let mut g = Gedcom::parse(GED).unwrap();
    // Profession → événement libre : la précision passe dans TYPE.
    let i = idx(&g, "OCCU");
    g.update_event("I1", i, &NewEvent { tag: "EVEN".into(), detail: "Service militaire".into(), ..Default::default() })
        .unwrap();
    let n = &g.get("I1").unwrap().children[i];
    assert_eq!((n.tag.as_str(), n.value.as_deref()), ("EVEN", None));
    assert_eq!(n.child_val("TYPE"), Some("Service militaire"));
    assert!(g.update_event("I1", i, &NewEvent { tag: "EVEN".into(), ..Default::default() }).is_err());

    // Résidence : la valeur d'origine reste.
    let r = idx(&g, "RESI");
    let mut ev = g.read_event("I1", r).unwrap();
    ev.date = "1901".into();
    g.update_event("I1", r, &ev).unwrap();
    assert_eq!(g.get("I1").unwrap().children[r].val(), "Paris");

    // Décès « Y » : daté, la marque disparaît ; sans rien, elle revient.
    let d = idx(&g, "DEAT");
    let mut ev = g.read_event("I1", d).unwrap();
    ev.date = "1912".into();
    g.update_event("I1", d, &ev).unwrap();
    assert_eq!(g.get("I1").unwrap().children[d].value, None);
    ev.date.clear();
    g.update_event("I1", d, &ev).unwrap();
    assert_eq!(g.get("I1").unwrap().children[d].val(), "Y");
}

#[test]
fn delete_event() {
    let mut g = Gedcom::parse(GED).unwrap();
    let before = g.events_of("I1").len();
    let label = g.delete_event("I1", idx(&g, "OCCU")).unwrap();
    assert_eq!(label, "Profession : Meunier");
    assert_eq!(g.events_of("I1").len(), before - 1);
    assert!(g.delete_event("I1", 0).is_err());
}
