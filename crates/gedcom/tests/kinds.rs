use gedcom::analysis::check;
use gedcom::*;

const TYPED: &str = "0 HEAD
1 GEDC
2 VERS 7.0
0 @S1@ SOUR
1 TITL Registre des naissances 1793-1802
1 DATA
2 EVEN BIRT
3 DATE FROM 1793 TO 1802
0 @I1@ INDI
1 NAME Jeanne /MARTIN/
1 SEX F
1 BIRT
2 DATE 1850
2 SOUR @S1@
3 EVEN BIRT
4 ROLE CHIL
1 OCCU Cultivatrice
2 SOUR @S1@
3 EVEN BIRT
4 ROLE FATH
1 OBJE @M1@
1 OBJE @M2@
2 _PRIM Y
0 @M1@ OBJE
1 FILE Actes/acte.jpg
2 FORM image/jpeg
3 MEDI MANUSCRIPT
0 @M2@ OBJE
1 FILE Photos/jeanne.jpg
2 FORM image/jpeg
0 TRLR
";

fn codes(g: &Gedcom) -> Vec<&'static str> {
    check::check(g, &check::Thresholds::default()).into_iter().map(|i| i.code).collect()
}

#[test]
fn typed_citations_are_checked() {
    let g = Gedcom::parse(TYPED).unwrap();
    let c = codes(&g);
    assert!(c.contains(&"outside_source_period"), "{c:?}");
    assert!(c.contains(&"role_sex_mismatch"), "{c:?}");
}

#[test]
fn portrait_skips_documents_and_follows_link_order() {
    let mut g = Gedcom::parse(TYPED).unwrap();
    // The scanned record (MANUSCRIPT) is not a portrait.
    assert_eq!(g.portrait("I1").unwrap().xref(), "M2");
    // _PRIM: the link moves to the top.
    assert_eq!(import::analyze(&g).iter().find(|f| f.id == "prim").map(|f| f.count), Some(1));
    import::apply(&mut g, &["prim"]);
    let links: Vec<&str> = g.get("I1").unwrap().children_with("OBJE").map(|n| n.val()).collect();
    assert_eq!(links, vec!["@M2@", "@M1@"]);
    assert_eq!(g.prim_count(), 0);
    // Set as portrait: link at the top and PHOTO medium if it had none.
    assert!(g.set_portrait("I1", "M1"));
    let links: Vec<&str> = g.get("I1").unwrap().children_with("OBJE").map(|n| n.val()).collect();
    assert_eq!(links, vec!["@M1@", "@M2@"]);
    assert_eq!(medium(g.get("M1").unwrap()), Some("MANUSCRIPT"), "un support déjà choisi est gardé");
    assert!(g.set_portrait("I1", "M2"));
    assert_eq!(medium(g.get("M2").unwrap()), Some("PHOTO"));
    assert_eq!(g.portrait("I1").unwrap().xref(), "M2");
    assert!(!g.set_portrait("I1", "M2"));
}
