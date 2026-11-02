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

const COUPLE: &str = "0 HEAD
1 GEDC
2 VERS 7.0
0 @I1@ INDI
1 NAME Jean /Guyot/
1 BIRT
2 DATE 3 MAR 1850
1 DEAT
2 DATE 1920
1 FAMS @F1@
1 FAMS @F2@
0 @I2@ INDI
1 NAME Marie /Dupont/
1 FAMS @F1@
0 @F1@ FAM
1 HUSB @I1@
1 WIFE @I2@
1 MARR
2 DATE 12 JUN 1875
2 SOUR @S1@
1 DIV Y
0 @F2@ FAM
1 HUSB @I1@
1 MARR
2 DATE 1890
0 @S1@ SOUR
1 TITL Registre
0 @M1@ OBJE
1 FILE Actes/mariage.jpg
2 FORM image/jpeg
0 TRLR
";

#[test]
fn person_events_merge_unions_chronologically() {
    let g = Gedcom::parse(COUPLE).unwrap();
    let ev = g.person_events("I1");
    let seq: Vec<(&str, &str, bool)> = ev.iter().map(|e| (e.owner.as_str(), e.event.tag(), e.family)).collect();
    assert_eq!(
        seq,
        vec![
            ("I1", "BIRT", false),
            ("F1", "MARR", true),
            ("F2", "MARR", true),
            ("I1", "DEAT", false),
            ("F1", "DIV", true), // sans date : à la fin
        ]
    );
    assert_eq!(ev[1].spouse.as_deref(), Some("I2"));
    assert_eq!(ev[2].spouse, None);
    // Le conjoint voit le même mariage, porté par la même famille.
    let other = g.person_events("I2");
    assert_eq!(other.len(), 2);
    assert_eq!(other[0].owner, "F1");
    assert_eq!(other[0].spouse.as_deref(), Some("I1"));
}

#[test]
fn media_on_family_event() {
    let mut g = Gedcom::parse(COUPLE).unwrap();
    let marr = g.events_of("F1").iter().find(|(_, e)| e.tag() == "MARR").unwrap().0;
    g.attach_media_to_event("F1", marr, "M1").unwrap();
    g.attach_media_to_event("F1", marr, "M1").unwrap(); // pas de doublon
    assert_eq!(g.event_media("F1", marr), vec!["M1"]);
    let out = g.to_gedcom_string();
    assert!(out.contains("1 MARR\n2 DATE 12 JUN 1875\n2 SOUR @S1@\n2 OBJE @M1@\n"), "{out}");
    // Le média est « utilisé par » la famille.
    assert!(g.referrers("M1").iter().any(|r| r.xref.as_deref() == Some("F1")));

    // « 1 DIV Y » prend le média et perd son Y ; le retirer le remet.
    let div = g.events_of("F1").iter().find(|(_, e)| e.tag() == "DIV").unwrap().0;
    g.attach_media_to_event("F1", div, "M1").unwrap();
    assert!(g.to_gedcom_string().contains("1 DIV\n2 OBJE @M1@\n"));
    g.detach_media_from_event("F1", div, "M1").unwrap();
    assert!(g.to_gedcom_string().contains("1 DIV Y\n"));

    g.detach_media_from_event("F1", marr, "M1").unwrap();
    assert!(g.event_media("F1", marr).is_empty());
    assert!(g.attach_media_to_event("F1", marr, "S1").is_err());
}

#[test]
fn deleting_media_strips_event_links() {
    let mut g = Gedcom::parse(COUPLE).unwrap();
    let marr = g.events_of("F1").iter().find(|(_, e)| e.tag() == "MARR").unwrap().0;
    g.attach_media_to_event("F1", marr, "M1").unwrap();
    g.delete_record("M1");
    assert!(g.event_media("F1", marr).is_empty());
}
