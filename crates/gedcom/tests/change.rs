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
        pair: None,
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
        cause: None,
    };
    let c = change(&g, "death_before_birth", Action::Edit(vec![edit]), "The act says 1869");
    let checked = c.check(&g, &rerun).unwrap();
    assert!(checked.new.is_empty() && checked.resolves == Some(true));
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
fn a_change_that_leaves_its_issue_says_so() {
    let g = ged();
    // Another kind of edit than the usual one, on another record: allowed, the issue stays.
    let sex = Edit::SetSex { person: "I1".into(), sex: "F".into() };
    let c = change(&g, "death_before_birth", Action::Edit(vec![sex]), "x");
    assert_eq!(c.check(&g, &rerun).unwrap().resolves, Some(false));
    let edit = Edit::SetName { person: "I2".into(), index: 0, given: Some("Marie".into()), surname: None };
    let c = change(&g, "death_before_birth", Action::Edit(vec![edit]), "x");
    assert_eq!(c.check(&g, &rerun).unwrap().resolves, Some(false));
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
    let cite = |s: &str| Edit::CiteSource {
        record: "I2".into(),
        index: 2,
        source: s.into(),
        page: "1800, view 31".into(),
        quality: None,
    };
    let issues = rerun(&g);
    let ids = issue_ids(&issues);
    let i = issues.iter().position(|i| i.code == "unsourced" && i.xref == "I2").expect("unsourced birth");
    let mut c = QualityChange {
        issue: Some(issues[i].clone()),
        issue_id: Some(ids[i].clone()),
        action: Action::Edit(vec![cite("S9")]),
        pair: None,
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
        Action::Edit(vec![Edit::SetCoordinates {
            place: "Agneaux".into(),
            latitude: 49.1,
            longitude: -1.1,
            exid: Some(("1234".into(), "https://www.openstreetmap.org/relation/".into())),
        }]),
        "Agneaux, Manche",
    );
    c.check(&g, &rerun).unwrap();
    assert_eq!(c.rows(&g)[0].proposed, "N49.1, W1.1");
    assert!(c.diff(&g).unwrap().iter().any(|(k, l)| *k == '+' && l.contains("EXID 1234")));
    let far = Edit::SetCoordinates { place: "Agneaux".into(), latitude: 120.0, longitude: 0.0, exid: None };
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
        pair: None,
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
            .contains("give that `issue`")
    );
    let hide = QualityChange { action: Action::Hide, ..correction(vec![], evidence) };
    assert!(hide.check(&g, &rerun).unwrap_err().contains("needs the `issue`"));
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
        pair: None,
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
        cause: None,
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
        pair: None,
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
        cause: None,
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

#[test]
fn a_correction_sets_the_repository_of_a_source() {
    let g = ged();
    let correction = |edit: Edit| QualityChange {
        issue: None,
        issue_id: None,
        action: Action::Edit(vec![edit]),
        pair: None,
        reason: "The register is kept by the archives of Manche".into(),
        evidence: Evidence { url: "https://archives.manche.fr/ark:/1".into(), ..Default::default() },
    };
    let new = gedcom::proposal::NewRepository {
        name: "Archives départementales de la Manche".into(),
        address: "Saint-Lô".into(),
        www: "https://archives.manche.fr".into(),
    };
    let c = correction(Edit::SetRepository {
        source: "S1".into(),
        repository: None,
        new: Some(new.clone()),
        call_number: Some("5 Mi 1".into()),
    });
    c.check(&g, &rerun).unwrap();
    let row = &c.rows(&g)[0];
    assert!(
        row.proposed.contains("Archives départementales de la Manche") && row.proposed.contains("5 Mi 1"),
        "{row:?}"
    );
    let mut g2 = g.clone();
    c.apply(&mut g2).unwrap();
    let x = g2.repository_named("Archives departementales de la Manche").unwrap();
    let text = g2.to_gedcom_string();
    assert!(text.contains(&format!("1 REPO @{x}@\n2 CALN 5 Mi 1")), "{text}");
    assert!(text.contains("1 WWW https://archives.manche.fr"), "{text}");
    // Then another source to the same repository, by its id: no second repository.
    let c = correction(Edit::SetRepository {
        source: "S1".into(),
        repository: Some(x.clone()),
        new: None,
        call_number: None,
    });
    c.check(&g2, &rerun).unwrap();
    // Unknown source or repository, or neither given.
    let bad = |e: Edit| correction(e).check(&g, &rerun).unwrap_err();
    assert!(
        bad(Edit::SetRepository { source: "I1".into(), repository: Some("R9".into()), new: None, call_number: None })
            .contains("not a source")
    );
    assert!(
        bad(Edit::SetRepository { source: "S1".into(), repository: Some("R9".into()), new: None, call_number: None })
            .contains("no repository")
    );
    assert!(
        bad(Edit::SetRepository { source: "S1".into(), repository: None, new: None, call_number: None })
            .contains("needs")
    );
}

const FAMILY: &str = "0 HEAD
1 GEDC
2 VERS 7.0
0 @I1@ INDI
1 NAME Jean /GUYOT/
1 SEX M
1 FAMS @F1@
0 @I2@ INDI
1 NAME Marie /LEROUX/
1 SEX F
1 FAMS @F1@
0 @I3@ INDI
1 NAME Pierre /GUYOT/
1 SEX M
1 BIRT
2 DATE 1820
1 DEAT
2 DATE 1880
1 FAMC @F1@
0 @I4@ INDI
1 NAME Louis /MARTIN/
1 SEX M
0 @I5@ INDI
1 NAME Anne /MARTIN/
1 SEX F
0 @F1@ FAM
1 HUSB @I1@
1 WIFE @I2@
1 CHIL @I3@
1 MARR
2 DATE 1817
0 @M1@ OBJE
1 FILE images/acte.jpg
2 FORM image/jpeg
0 TRLR
";

fn correction(edits: Vec<Edit>) -> QualityChange {
    QualityChange {
        issue: None,
        issue_id: None,
        action: Action::Edit(edits),
        pair: None,
        reason: "The register says so".into(),
        evidence: Evidence { quote: "fils de Louis Martin".into(), ..Default::default() },
    }
}

#[test]
fn a_correction_moves_a_child_to_the_right_father() {
    use gedcom::relations::Relation;
    let g = Gedcom::parse(FAMILY).unwrap();
    let unlink = Edit::UnlinkRelative { person: "I3".into(), relation: Relation::Father, other: "I1".into() };
    let link = Edit::LinkRelative { person: "I3".into(), relation: Relation::Father, other: "I4".into() };
    let c = correction(vec![unlink.clone(), link]);
    c.check(&g, &rerun).unwrap();
    let rows = c.rows(&g);
    assert_eq!(rows[0].proposed, "Deleted");
    assert_eq!(rows[1].proposed, "Louis MARTIN");
    let mut g2 = g.clone();
    c.apply(&mut g2).unwrap();
    let (father, mother) = g2.parents("I3");
    assert_eq!(father.map(|f| f.xref().to_string()).as_deref(), Some("I4"));
    // The mother stays, in a family of her own with Louis; her marriage with Jean stays too.
    assert_eq!(mother.map(|m| m.xref().to_string()).as_deref(), Some("I2"));
    let f1 = g2.family("F1").unwrap();
    assert_eq!((f1.husband(), f1.wife(), f1.children().len()), (Some("I1"), Some("I2"), 0));
    // A link that does not exist is refused.
    let wrong = Edit::UnlinkRelative { person: "I3".into(), relation: Relation::Father, other: "I5".into() };
    assert!(correction(vec![wrong]).check(&g, &rerun).unwrap_err().contains("not the father"));
}

#[test]
fn a_correction_sets_an_adoption() {
    let g = Gedcom::parse(FAMILY).unwrap();
    let c = correction(vec![Edit::SetPedigree {
        person: "I3".into(),
        parent: Some("I2".into()),
        pedigree: "adopted".into(),
    }]);
    let mut g2 = g.clone();
    c.apply(&mut g2).unwrap();
    assert!(gedcom::parse::node_to_string(g2.get("I3").unwrap()).contains("1 FAMC @F1@\n2 PEDI ADOPTED"));
    assert_eq!(g2.person_sheet("I3").unwrap().pedigree.as_deref(), Some("ADOPTED"));
    let bad = correction(vec![Edit::SetPedigree { person: "I3".into(), parent: None, pedigree: "STEP".into() }]);
    assert!(bad.check(&g, &rerun).unwrap_err().contains("ADOPTED"));
}

#[test]
fn a_correction_adds_and_changes_notes() {
    let g = Gedcom::parse(FAMILY).unwrap();
    let add = Edit::AddNote { record: "I3".into(), event: Some(3), text: "Two acts disagree on the date".into() };
    let person = Edit::AddNote { record: "I3".into(), event: None, text: "Called Pierrot".into() };
    let mut g2 = g.clone();
    correction(vec![add, person]).apply(&mut g2).unwrap();
    let sheet = g2.person_sheet("I3").unwrap();
    assert_eq!(sheet.events.iter().find(|e| e.tag == "DEAT").unwrap().notes, ["Two acts disagree on the date"]);
    assert_eq!(sheet.notes, ["Called Pierrot"]);
    let set = Edit::SetNote { record: "I3".into(), event: Some(3), note: 0, text: "Acts agree: 1880".into() };
    let remove = Edit::SetNote { record: "I3".into(), event: None, note: 0, text: String::new() };
    let mut g3 = g2.clone();
    correction(vec![set, remove]).apply(&mut g3).unwrap();
    let sheet = g3.person_sheet("I3").unwrap();
    assert_eq!(sheet.events.iter().find(|e| e.tag == "DEAT").unwrap().notes, ["Acts agree: 1880"]);
    assert!(sheet.notes.is_empty());
    let missing = Edit::SetNote { record: "I3".into(), event: None, note: 3, text: "x".into() };
    assert!(correction(vec![missing]).check(&g2, &rerun).unwrap_err().contains("note(s)"));
}

#[test]
fn a_correction_sets_ages_causes_and_witnesses() {
    let g = Gedcom::parse(FAMILY).unwrap();
    let edits = vec![
        Edit::SetEvent {
            record: "I3".into(),
            index: 3,
            value: None,
            date: None,
            place: None,
            address: None,
            cause: Some("fièvre".into()),
        },
        Edit::SetAge { record: "I3".into(), index: 3, spouse: None, age: "60 ans".into() },
        Edit::SetAge { record: "F1".into(), index: 3, spouse: Some("I2".into()), age: "19y".into() },
        Edit::AddAssociate {
            record: "F1".into(),
            index: 3,
            person: "I4".into(),
            role: "witn".into(),
            phrase: "cousin of the bride".into(),
        },
    ];
    let c = correction(edits);
    c.check(&g, &rerun).unwrap();
    let rows = c.rows(&g);
    assert!(rows[0].proposed.contains("fièvre"), "{:?}", rows[0]);
    assert_eq!(rows[1].proposed, "60y");
    assert!(rows[3].proposed.starts_with("Louis MARTIN ("), "{:?}", rows[3]);
    let mut g2 = g.clone();
    c.apply(&mut g2).unwrap();
    let text = |x: &str| gedcom::parse::node_to_string(g2.get(x).unwrap());
    assert!(text("I3").contains("1 DEAT\n2 DATE 1880\n2 CAUS fièvre\n2 AGE 60y"), "{}", text("I3"));
    assert!(
        text("F1")
            .contains("1 MARR\n2 DATE 1817\n2 WIFE\n3 AGE 19y\n2 ASSO @I4@\n3 ROLE WITN\n4 PHRASE cousin of the bride"),
        "{}",
        text("F1")
    );
    let sheet = g2.person_sheet("I1").unwrap();
    let marr = &sheet.unions[0].events[0];
    assert_eq!((marr.wife_age.as_str(), marr.associates[0].role.as_str()), ("19y", "WITN"));
    // Removed again.
    let mut g3 = g2.clone();
    correction(vec![
        Edit::RemoveAssociate { record: "F1".into(), index: 3, person: "I4".into() },
        Edit::SetAge { record: "F1".into(), index: 3, spouse: Some("I2".into()), age: String::new() },
    ])
    .apply(&mut g3)
    .unwrap();
    let f1 = gedcom::parse::node_to_string(g3.get("F1").unwrap());
    assert!(f1.contains("1 MARR\n2 DATE 1817\n") && !f1.contains("ASSO") && !f1.contains("WIFE\n"), "{f1}");
    // A family event needs to know whose age it is, and `OTHER` needs its phrase.
    let age = Edit::SetAge { record: "F1".into(), index: 3, spouse: None, age: "19y".into() };
    assert!(correction(vec![age]).check(&g, &rerun).unwrap_err().contains("spouse"));
    let other = Edit::AddAssociate {
        record: "F1".into(),
        index: 3,
        person: "I4".into(),
        role: "OTHER".into(),
        phrase: String::new(),
    };
    assert!(correction(vec![other]).check(&g, &rerun).unwrap_err().contains("phrase"));
}

#[test]
fn a_correction_attaches_and_detaches_media() {
    let g = Gedcom::parse(FAMILY).unwrap();
    let existing = Edit::AttachMedia { record: "I3".into(), event: Some(2), media: Some("M1".into()), file: None };
    let new =
        Edit::AttachMedia { record: "I3".into(), event: None, media: None, file: Some("images/portrait.jpg".into()) };
    let mut g2 = g.clone();
    correction(vec![existing, new]).apply(&mut g2).unwrap();
    let sheet = g2.person_sheet("I3").unwrap();
    assert_eq!(sheet.events[0].media[0].id, "M1");
    assert_eq!(sheet.media[0].files, ["images/portrait.jpg"]);
    let detach = Edit::DetachMedia { record: "I3".into(), event: Some(2), media: "M1".into() };
    let mut g3 = g2.clone();
    correction(vec![detach.clone()]).apply(&mut g3).unwrap();
    assert!(g3.person_sheet("I3").unwrap().events[0].media.is_empty());
    assert!(correction(vec![detach]).check(&g3, &rerun).unwrap_err().contains("not linked"));
    let both =
        Edit::AttachMedia { record: "I3".into(), event: None, media: Some("M1".into()), file: Some("a.jpg".into()) };
    assert!(correction(vec![both]).check(&g, &rerun).unwrap_err().contains("not both"));
}
