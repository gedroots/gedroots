use gedcom::*;

const SAMPLE: &str = include_str!("fixtures/example.ged");

fn names(g: &Gedcom, q: &str) -> Vec<String> {
    let mut v: Vec<String> = g.search_individuals(q).iter().map(|x| g.individual(x).unwrap().given()).collect();
    v.sort();
    v
}

#[test]
fn search_queries() {
    let g = Gedcom::parse(SAMPLE).unwrap();
    assert_eq!(names(&g, "marchand"), vec!["Paul Émile", "Émile"]);
    assert_eq!(names(&g, "emile"), vec!["Paul Émile", "Émile"]);
    assert_eq!(names(&g, "paul marchand"), vec!["Paul Émile"]);
    assert_eq!(names(&g, "sex:f"), vec!["Jeanne"]);
    assert_eq!(names(&g, "born:1880-1920"), vec!["Jeanne", "Paul Émile"]);
    assert_eq!(names(&g, "born:<1800"), vec!["Émile"]); // an IV = 1796
    assert_eq!(names(&g, "place:coutances"), vec!["Jeanne"]);
    assert_eq!(names(&g, "place:saint-lo"), vec!["Paul Émile"]);
    assert_eq!(names(&g, "occupation:instit"), vec!["Paul Émile"]);
    assert_eq!(names(&g, "without:parents"), vec!["Jeanne", "Émile"]);
    assert_eq!(names(&g, "with:media"), vec!["Paul Émile"]);
    assert_eq!(names(&g, "sosa"), vec!["Paul Émile", "Émile"]);
    assert_eq!(names(&g, "sosa:2-3"), vec!["Émile"]);
    assert_eq!(names(&g, "@I00003@"), vec!["Jeanne"]);
    assert_eq!(names(&g, "alive:1900"), vec!["Jeanne", "Émile"]);
    assert_eq!(names(&g, "without:source"), vec!["Jeanne", "Paul Émile", "Émile"]);
    let q = IndividualQuery::parse("born:abc dupont");
    assert_eq!(q.unknown, vec!["born:abc"]);
}

#[test]
fn toggle_tokens() {
    assert_eq!(toggle_token("", "sex:m"), "sex:m ");
    assert_eq!(toggle_token("dupont sex:m ", "sex:f"), "dupont sex:f ");
    assert_eq!(toggle_token("dupont without:source ", "without:source"), "dupont ");
    assert!(has_token("a WITHOUT:SOURCE", "without:source"));
}

fn consistent(g: &Gedcom) {
    // Each family ↔ person pointer exists in both directions.
    for f in g.records_of(RecordKind::Family) {
        let fam = Family::new(f);
        let x = format!("@{}@", fam.xref());
        for p in [fam.husband(), fam.wife()].into_iter().flatten() {
            assert!(g.get(p).unwrap().children.iter().any(|c| c.tag == "FAMS" && c.val() == x), "FAMS {p}");
        }
        for c in fam.children() {
            assert!(g.get(c).unwrap().children.iter().any(|n| n.tag == "FAMC" && n.val() == x), "FAMC {c}");
        }
    }
    for r in g.records_of(RecordKind::Individual) {
        let i = Individual::new(r);
        for f in i.fams() {
            let fam = g.family(f).expect("famille");
            assert!(fam.husband() == Some(i.xref()) || fam.wife() == Some(i.xref()));
        }
        for f in i.famc() {
            assert!(g.family(f).expect("famille").children().contains(&i.xref()));
        }
    }
}

#[test]
fn add_relatives() {
    let mut g = Gedcom::parse(SAMPLE).unwrap();
    // Jeanne has no parents: her father is created.
    let father = g.add_relative("I00003", Relation::Father, None).unwrap();
    assert_eq!(g.individual(&father).unwrap().surname(), "ROUSSEL");
    assert_eq!(g.individual(&father).unwrap().sex(), "M");
    assert_eq!(g.parents("I00003").0.unwrap().xref(), father);
    assert!(g.add_relative("I00003", Relation::Father, None).is_err());
    let mother = g.add_relative("I00003", Relation::Mother, None).unwrap();
    assert_eq!(g.parents("I00003").1.unwrap().xref(), mother);
    // Both parents are in the same family.
    assert_eq!(g.individual(&father).unwrap().fams(), g.individual(&mother).unwrap().fams());
    let sib = g.add_relative("I00003", Relation::Sibling, None).unwrap();
    assert_eq!(g.parents(&sib).0.unwrap().xref(), father);
    // Paul: a spouse, then a child who takes the father's surname.
    let wife = g.add_relative("I00001", Relation::Spouse, None).unwrap();
    assert_eq!(g.individual(&wife).unwrap().sex(), "F");
    let kid = g.add_relative("I00001", Relation::Child, None).unwrap();
    assert_eq!(g.individual(&kid).unwrap().surname(), "MARCHAND");
    assert_eq!(g.parents(&kid).1.unwrap().xref(), wife);
    consistent(&g);
    // The file stays readable.
    let again = Gedcom::parse(&g.to_gedcom_string()).unwrap();
    assert_eq!(again.count(RecordKind::Individual), g.count(RecordKind::Individual));
}

#[test]
fn link_and_unlink() {
    let mut g = Gedcom::parse(SAMPLE).unwrap();
    let x = g.create_individual("Louise", "BERTIN", "F");
    g.link_relative("I00001", Relation::Spouse, &x).unwrap();
    assert!(g.link_relative("I00001", Relation::Spouse, &x).is_err());
    assert!(g.link_relative("I00001", Relation::Father, "I00003").is_err(), "a déjà un père");
    // A man cannot become the mother, and nothing is left behind.
    let families = g.count(RecordKind::Family);
    let man = g.create_individual("Henri", "LAMY", "M");
    let orphan = g.create_individual("Lucie", "LAMY", "F");
    assert!(g.link_relative(&orphan, Relation::Mother, &man).is_err());
    assert_eq!(g.count(RecordKind::Family), families);
    g.link_relative(&orphan, Relation::Father, &man).unwrap();
    assert_eq!(g.parents(&orphan).0.unwrap().xref(), man);
    // An ancestor cannot become a child.
    assert!(g.link_relative("I00001", Relation::Child, "I00002").is_err());
    let kid = g.create_individual("Marc", "MARCHAND", "M");
    g.link_relative("I00001", Relation::Child, &kid).unwrap();
    assert_eq!(g.parents(&kid).1.unwrap().xref(), x);
    consistent(&g);
    let fam = g.individual(&kid).unwrap().famc()[0].to_string();
    g.unlink_from_family(&kid, &fam);
    assert!(g.parents(&kid).0.is_none());
    consistent(&g);
    // Separating the couple empties the family, which disappears.
    g.unlink_from_family(&x, &fam);
    assert!(g.family(&fam).is_none());
    consistent(&g);
}

#[test]
fn marriage_criterion() {
    let mut g = Gedcom::parse(SAMPLE).unwrap();
    assert!(names(&g, "without:marriage").is_empty());
    assert_eq!(names(&g, "with:marriage"), vec!["Jeanne", "Émile"]);
    // A union without a marriage; a person without a union does not count.
    g.add_relative("I00001", Relation::Spouse, None).unwrap();
    let mut without = names(&g, "without:marriage");
    without.retain(|n| !n.is_empty());
    assert_eq!(without, vec!["Paul Émile"]);
}

#[test]
fn spouse_joins_single_parent_union() {
    let mut g = Gedcom::parse(SAMPLE).unwrap();
    // Paul has a child with no known spouse: the union has only one parent.
    let kid = g.add_relative("I00001", Relation::Child, None).unwrap();
    let families = g.count(RecordKind::Family);
    // The spouse added next completes this union instead of creating another one.
    let wife = g.add_relative("I00001", Relation::Spouse, None).unwrap();
    assert_eq!(g.count(RecordKind::Family), families);
    assert_eq!(g.parents(&kid).1.unwrap().xref(), wife);
    // Complete union: a second spouse opens a new union.
    let x = g.create_individual("Louise", "BERTIN", "F");
    g.link_relative("I00001", Relation::Spouse, &x).unwrap();
    assert_eq!(g.count(RecordKind::Family), families + 1);
    assert_eq!(g.parents(&kid).1.unwrap().xref(), wife);
    consistent(&g);
}

#[test]
fn delete_cleans_up() {
    let mut g = Gedcom::parse(SAMPLE).unwrap();
    let wife = g.add_relative("I00001", Relation::Spouse, None).unwrap();
    let fam = g.individual(&wife).unwrap().fams()[0].to_string();
    g.delete_individual(&wife);
    assert!(g.family(&fam).is_none(), "la famille d'un seul membre est supprimée");
    assert!(g.individual("I00001").unwrap().fams().is_empty());
    // Deleting Paul keeps family F00001 (both parents stay).
    g.delete_record("I00001");
    assert!(g.family("F00001").is_some());
    assert!(g.family("F00001").unwrap().children().is_empty());
    consistent(&g);
}

#[test]
fn places_and_media() {
    let mut g = Gedcom::parse(SAMPLE).unwrap();
    let n = g.delete_place(", Coutances, 50200, Manche, Normandie, France");
    assert_eq!(n, 1);
    assert!(g.individual("I00003").unwrap().birth().unwrap().place().is_none());
    assert!(g.individual("I00003").unwrap().birth().unwrap().date().is_some());

    let m = g.media_for_file("Actes/Naissance_Jeanne_Roussel_1885.jpg");
    assert_eq!(g.media_for_file("Actes/Naissance_Jeanne_Roussel_1885.jpg"), m, "réutilisé");
    let media = Media::new(g.get(&m).unwrap());
    assert_eq!(media.form(), Some("image/jpeg"));
    assert_eq!(media.title(), "Naissance Jeanne Roussel 1885");
    g.attach_media("I00003", &m).unwrap();
    g.attach_media("I00003", &m).unwrap();
    assert_eq!(g.individual("I00003").unwrap().media_xrefs(), vec![m.as_str()]);
    assert!(g.attach_media(&m, &m).is_err());
    g.detach_media("I00003", &m);
    assert!(g.individual("I00003").unwrap().media_xrefs().is_empty());
    assert!(g.referenced_files().contains("Actes/Naissance_Jeanne_Roussel_1885.jpg"));
    assert!(is_media_file("a.PDF") && !is_media_file(".DS_Store") && !is_media_file("x.ged"));
}

#[test]
fn descendants_tree() {
    let mut g = Gedcom::parse(SAMPLE).unwrap();
    let d = g.descendants("I00002", 3).unwrap();
    assert_eq!(d.unions.len(), 1);
    assert_eq!(d.unions[0].spouse.as_deref(), Some("I00003"));
    assert_eq!(d.unions[0].children[0].xref, "I00001");
    assert_eq!(d.count(), 2);
    let kid = g.add_relative("I00001", Relation::Child, None).unwrap();
    assert_eq!(g.descendant_count("I00002"), 2);
    let d = g.descendants("I00002", 2).unwrap();
    assert!(d.unions[0].children[0].truncated);
    let _ = kid;
}

#[test]
fn add_events() {
    let mut g = Gedcom::parse(SAMPLE).unwrap();
    let x = g.records_of(RecordKind::Individual).next().unwrap().xref.clone().unwrap();
    let src = g.records_of(RecordKind::Source).next().map(|s| s.xref.clone().unwrap());
    let before = g.individual(&x).unwrap().events().len();
    let ev = NewEvent {
        tag: "RESI".into(),
        date: "12 mar 1901".into(),
        place: "Troyes, Aube".into(),
        source: src.clone(),
        ..Default::default()
    };
    g.add_event(&x, &ev).unwrap();
    g.add_event(&x, &NewEvent { tag: "OCCU".into(), detail: "Meunier".into(), ..Default::default() }).unwrap();
    g.add_event(&x, &NewEvent { tag: "BURI".into(), ..Default::default() }).unwrap();
    assert!(g.add_event(&x, &NewEvent { tag: "EVEN".into(), ..Default::default() }).is_err());
    let i = g.individual(&x).unwrap();
    let evs = i.events();
    assert_eq!(evs.len(), before + 3);
    let resi = evs.iter().find(|e| e.tag() == "RESI" && e.date_raw() == Some("12 MAR 1901")).unwrap();
    assert_eq!(resi.place(), Some("Troyes, Aube"));
    assert_eq!(resi.is_sourced(), src.is_some());
    assert!(evs.iter().any(|e| e.tag() == "OCCU" && e.value() == Some("Meunier")));
    assert!(evs.iter().any(|e| e.tag() == "BURI" && e.node.val() == "Y"));
}
