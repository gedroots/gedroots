use gedcom::Gedcom;
use gedcom::analysis::check::Issue;
use gedcom::change::{Action, Edit, Evidence, QualityChange, find_issue};
use gedcom::quality::{self, issue_ids};

const FILE: &str = "0 HEAD
1 GEDC
2 VERS 7.0
0 @I1@ INDI
1 NAME Jean /GUYOT/
1 SEX M
1 BIRT
2 DATE 4 MAY 1796
2 PLAC Agneaux
2 SOUR @S1@
1 DEAT
2 DATE 12 MAR 1769
2 SOUR @S1@
1 RESI
1 FAMS @F1@
0 @I2@ INDI
1 NAME Marie /LEROUX/
1 SEX M
1 BIRT
2 DATE 1800
1 FAMS @F1@
0 @F1@ FAM
1 HUSB @I1@
1 WIFE @I2@
1 MARR
2 DATE 1817
0 @S1@ SOUR
1 TITL Agneaux, état civil
0 TRLR
";

fn ged() -> Gedcom {
    Gedcom::parse(FILE).unwrap()
}

fn rerun(g: &Gedcom) -> Vec<Issue> {
    quality::run(g)
}

fn change(g: &Gedcom, code: &str, action: Action, reason: &str) -> QualityChange {
    let issues = rerun(g);
    let ids = issue_ids(&issues);
    let i = issues.iter().position(|i| i.code == code).unwrap_or_else(|| panic!("no {code} in {issues:#?}"));
    QualityChange {
        issue: Some(issues[i].clone()),
        issue_id: Some(ids[i].clone()),
        action,
        reason: reason.into(),
        evidence: Evidence::default(),
    }
}

#[test]
fn an_edit_that_fixes_the_date_is_accepted_and_applied() {
    let g = ged();
    let edit = Edit::SetEvent {
        record: "I1".into(),
        index: 3,
        value: None,
        date: Some("12 MAR 1869".into()),
        place: None,
        address: None,
    };
    let c = change(&g, "death_before_birth", Action::Edit(vec![edit]), "The act says 1869");
    assert!(c.check(&g, &rerun).unwrap().is_empty());
    let rows = c.rows(&g);
    assert_eq!(rows.len(), 1);
    assert!(rows[0].current.contains("1769") && rows[0].proposed.contains("1869"), "{rows:?}");
    assert!(c.diff(&g).unwrap().iter().any(|(k, l)| *k == '+' && l.contains("12 MAR 1869")));
    let mut g2 = g.clone();
    c.apply(&mut g2).unwrap();
    assert!(rerun(&g2).iter().all(|i| i.code != "death_before_birth"));
    assert!(find_issue(&rerun(&g2), c.issue_id.as_deref().unwrap()).is_none());
}

#[test]
fn changes_that_do_not_fit_the_issue_are_refused() {
    let g = ged();
    // Not an edit of this kind of issue.
    let sex = Edit::SetSex { person: "I1".into(), sex: "F".into() };
    let c = change(&g, "death_before_birth", Action::Edit(vec![sex]), "x");
    assert!(c.check(&g, &rerun).unwrap_err().contains("not allowed"));
    // Still before the birth.
    let edit = Edit::SetEvent {
        record: "I1".into(),
        index: 3,
        value: None,
        date: Some("1770".into()),
        place: None,
        address: None,
    };
    let c = change(&g, "death_before_birth", Action::Edit(vec![edit]), "x");
    assert!(c.check(&g, &rerun).unwrap_err().contains("still reports"));
    // Another person's record.
    let edit = Edit::SetEvent {
        record: "I2".into(),
        index: 3,
        value: None,
        date: Some("1700".into()),
        place: None,
        address: None,
    };
    let c = change(&g, "death_before_birth", Action::Edit(vec![edit]), "x");
    assert!(c.check(&g, &rerun).unwrap_err().contains("not one of the records"));
    // No automatic fix, no reason.
    assert!(
        change(&g, "death_before_birth", Action::Fix, "").check(&g, &rerun).unwrap_err().contains("no automatic fix")
    );
    assert!(change(&g, "death_before_birth", Action::Hide, " ").check(&g, &rerun).unwrap_err().contains("reason"));
}

#[test]
fn hiding_keeps_the_reason_under_the_tag() {
    let g = ged();
    let c = change(&g, "death_before_birth", Action::Hide, "Checked: two different acts");
    c.check(&g, &rerun).unwrap();
    let mut g2 = g.clone();
    c.apply(&mut g2).unwrap();
    let text = g2.to_gedcom_string();
    assert!(text.contains("1 _GR_IGNORE death_before_birth\n2 NOTE Checked: two different acts\n"), "{text}");
    assert!(text.contains("_GR_IGNORE https://"));
}

#[test]
fn automatic_fix_sex_and_citation() {
    let g = ged();
    // Empty residence: the Quality page's fix.
    let c = change(&g, "empty_fact", Action::Fix, "");
    c.check(&g, &rerun).unwrap();
    // A wife recorded as a man.
    let sex = Edit::SetSex { person: "I2".into(), sex: "f".into() };
    let c = change(&g, "wife_not_female", Action::Edit(vec![sex]), "Marie, wife in the marriage act");
    c.check(&g, &rerun).unwrap();
    // Marie's birth without a source: cite an existing source only.
    let cite =
        |s: &str| Edit::CiteSource { record: "I2".into(), index: 2, source: s.into(), page: "1800, view 31".into() };
    let issues = rerun(&g);
    let ids = issue_ids(&issues);
    let i = issues.iter().position(|i| i.code == "unsourced" && i.xref == "I2").expect("unsourced birth");
    let mut c = QualityChange {
        issue: Some(issues[i].clone()),
        issue_id: Some(ids[i].clone()),
        action: Action::Edit(vec![cite("S9")]),
        reason: "Birth act found".into(),
        evidence: Evidence::default(),
    };
    assert!(c.check(&g, &rerun).unwrap_err().contains("no source S9"));
    c.action = Action::Edit(vec![cite("S1")]);
    c.check(&g, &rerun).unwrap();
    assert!(c.diff(&g).unwrap().iter().any(|(k, l)| *k == '+' && l.contains("PAGE 1800, view 31")));
}

#[test]
fn coordinates_of_a_place() {
    let g = ged();
    let c = change(
        &g,
        "missing_coords",
        Action::Edit(vec![Edit::SetCoordinates { place: "Agneaux".into(), latitude: 49.1, longitude: -1.1 }]),
        "Agneaux, Manche",
    );
    c.check(&g, &rerun).unwrap();
    assert_eq!(c.rows(&g)[0].proposed, "N49.1, W1.1");
    let far = Edit::SetCoordinates { place: "Agneaux".into(), latitude: 120.0, longitude: 0.0 };
    let c = QualityChange { action: Action::Edit(vec![far]), ..c };
    assert!(c.check(&g, &rerun).is_err());
}

#[test]
fn a_correction_without_an_issue_renames_a_person() {
    let g = ged();
    let evidence =
        Evidence { url: "https://example.org/register/51".into(), quote: "Jean Pierre".into(), ..Default::default() };
    let correction = |edits: Vec<Edit>, evidence: Evidence| QualityChange {
        issue: None,
        issue_id: None,
        action: Action::Edit(edits),
        reason: "The marriage act names him Jean Pierre".into(),
        evidence,
    };
    let rename = Edit::SetName { person: "I1".into(), index: 0, given: Some("Jean Pierre".into()), surname: None };
    let c = correction(vec![rename.clone()], evidence.clone());
    c.check(&g, &rerun).unwrap();
    assert!(c.title(&g).contains("GUYOT"), "{}", c.title(&g));
    let rows = c.rows(&g);
    assert_eq!((rows[0].current.as_str(), rows[0].proposed.as_str()), ("Jean GUYOT", "Jean Pierre GUYOT"));
    let mut g2 = g.clone();
    c.apply(&mut g2).unwrap();
    assert_eq!(g2.individual("I1").unwrap().display_name(), "Jean Pierre GUYOT");
    // A variant spelling kept as another name, after the main one.
    let variant =
        Edit::AddName { person: "I1".into(), given: "Johann".into(), surname: "GUYOT".into(), kind: "aka".into() };
    let c = correction(vec![variant], evidence.clone());
    let mut g3 = g.clone();
    c.apply(&mut g3).unwrap();
    let names = g3.individual("I1").unwrap().names();
    assert_eq!((names.len(), names[1].given.as_str(), names[1].kind.as_str()), (2, "Johann", "AKA"));
    // Evidence is needed, and the edits that only an issue designates are refused.
    assert!(correction(vec![rename], Evidence::default()).check(&g, &rerun).unwrap_err().contains("evidence"));
    assert!(
        correction(vec![Edit::DeleteStructure], evidence.clone())
            .check(&g, &rerun)
            .unwrap_err()
            .contains("needs an issue")
    );
    let hide = QualityChange { action: Action::Hide, ..correction(vec![], evidence) };
    assert!(hide.check(&g, &rerun).unwrap_err().contains("only `edit`"));
}

#[test]
fn a_correction_changes_and_adds_occupations() {
    let g = Gedcom::parse(
        "0 HEAD\n1 GEDC\n2 VERS 7.0\n0 @I1@ INDI\n1 NAME Jean /GUYOT/\n1 OCCU cultivateur\n2 DATE 1850\n0 TRLR\n",
    )
    .unwrap();
    let evidence = Evidence { quote: "Jean Guyot, tisserand".into(), ..Default::default() };
    let correction = |edits: Vec<Edit>| QualityChange {
        issue: None,
        issue_id: None,
        action: Action::Edit(edits),
        reason: "The 1852 census says weaver".into(),
        evidence: evidence.clone(),
    };
    let set = Edit::SetEvent {
        record: "I1".into(),
        index: 1,
        value: Some("tisserand".into()),
        date: None,
        place: None,
        address: None,
    };
    let add = Edit::AddEvent {
        record: "I1".into(),
        tag: "occu".into(),
        value: "tisserand".into(),
        date: "1852".into(),
        place: String::new(),
        address: String::new(),
    };
    let c = correction(vec![set, add]);
    c.check(&g, &rerun).unwrap();
    let rows = c.rows(&g);
    assert!(rows[0].current.contains("cultivateur") && rows[0].proposed.contains("tisserand"), "{rows:?}");
    let mut g2 = g.clone();
    c.apply(&mut g2).unwrap();
    let occu: Vec<_> = g2.get("I1").unwrap().children_with("OCCU").map(|n| n.val().to_string()).collect();
    assert_eq!(occu, ["tisserand", "tisserand"]);
    let bad = Edit::AddEvent {
        record: "I1".into(),
        tag: "XYZ".into(),
        value: String::new(),
        date: String::new(),
        place: String::new(),
        address: String::new(),
    };
    assert!(correction(vec![bad]).check(&g, &rerun).unwrap_err().contains("XYZ"));
}

#[test]
fn a_correction_moves_an_address_and_renames_a_place() {
    let g = Gedcom::parse(
        "0 HEAD\n1 GEDC\n2 VERS 7.0\n0 @I1@ INDI\n1 NAME Jean /GUYOT/\n1 RESI\n2 DATE 1872\n\
         2 PLAC Agneau\n3 MAP\n4 LATI N49.1\n4 LONG W1.1\n1 DEAT\n2 PLAC Agneau\n0 TRLR\n",
    )
    .unwrap();
    let correction = |edits: Vec<Edit>| QualityChange {
        issue: None,
        issue_id: None,
        action: Action::Edit(edits),
        reason: "The census gives the street; the commune is Agneaux".into(),
        evidence: Evidence { quote: "rue Haute".into(), ..Default::default() },
    };
    let address = Edit::SetEvent {
        record: "I1".into(),
        index: 1,
        value: None,
        date: None,
        place: None,
        address: Some("4 rue Haute".into()),
    };
    let rename = Edit::RenamePlace { place: "Agneau".into(), name: "Agneaux".into() };
    let c = correction(vec![address, rename]);
    c.check(&g, &rerun).unwrap();
    assert!(c.rows(&g)[0].proposed.contains("4 rue Haute"), "{:?}", c.rows(&g));
    let mut g2 = g.clone();
    c.apply(&mut g2).unwrap();
    let text = gedcom::node_to_string(g2.get("I1").unwrap());
    assert!(text.contains("ADDR 4 rue Haute") && !text.contains("PLAC Agneau\n"), "{text}");
    assert_eq!(text.matches("PLAC Agneaux").count(), 2, "{text}");
    assert!(text.contains("LATI N49.1"), "{text}");
    let unknown = Edit::RenamePlace { place: "Nowhere".into(), name: "X".into() };
    assert!(correction(vec![unknown]).check(&g, &rerun).unwrap_err().contains("Nowhere"));
}
