use gedcom::*;

const GED: &str = "0 HEAD
1 GEDC
2 VERS 7.0
0 @I1@ INDI
1 NAME Julien /Prévost/
1 SEX M
1 BIRT
2 DATE 20 MAR 1829
0 @I2@ INDI
1 NAME Eugénie /Ferrand/
1 SEX F
0 @I3@ INDI
1 NAME Louis /Ferrand/
1 SEX M
1 BAPM
2 DATE 3 MAY 1875
2 ASSO @I1@
3 ROLE GODP
4 PHRASE Parrain, oncle
2 ASSO @I2@
3 ROLE GODP
0 @I4@ INDI
1 NAME Pierre /Martin/
0 @F1@ FAM
1 HUSB @I1@
1 WIFE @I2@
1 MARR
2 DATE 19 MAR 1855
2 ASSO @I4@
3 ROLE OTHER
4 PHRASE voisin
2 ASSO @I3@
3 ROLE WITN
0 @F2@ FAM
1 HUSB @I3@
1 MARR
2 DATE 12 FEB 1868
2 ASSO @I1@
3 ROLE WITN
0 TRLR";

#[test]
fn associates_of_an_event_with_their_roles() {
    rust_i18n::set_locale("en");
    let g = Gedcom::parse(GED).unwrap();
    let marr = g.events_of("F1")[0].0;
    let a = g.event_associates("F1", marr);
    assert_eq!(a.len(), 2);
    assert_eq!(a[0], Associate { person: "I4".into(), role: "voisin".into(), detail: None });
    assert_eq!(a[1].person, "I3");
    assert_eq!(a[1].role, "Witness");
}

#[test]
fn events_a_person_attended() {
    rust_i18n::set_locale("en");
    let g = Gedcom::parse(GED).unwrap();
    let att = g.attended_events("I1");
    // Sorted by date; their own union (F1) is not “attended”.
    assert_eq!(att.len(), 2);
    assert_eq!((att[0].owner.as_str(), att[0].event.tag()), ("F2", "MARR"));
    assert_eq!(att[0].role, "Witness");
    assert_eq!(att[0].principals, vec!["I3".to_string()]);
    assert_eq!((att[1].owner.as_str(), att[1].event.tag()), ("I3", "BAPM"));
    // The phrase repeating the role is dropped, the rest is kept.
    assert_eq!((att[1].role.as_str(), att[1].detail.as_deref()), ("Godfather", Some("oncle")));
    assert_eq!(g.attended_events("I2")[0].role, "Godmother");
    assert!(g.attended_events("I4").iter().all(|e| e.owner == "F1"));
}
