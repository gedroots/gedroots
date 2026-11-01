use gedcom::*;

const SAMPLE: &str = include_str!("fixtures/exemple.ged");

fn template(id: &str) -> Template {
    builtin_templates().into_iter().find(|t| t.id == id).unwrap()
}

fn person(role: &str, surname: &str, given: &str) -> Participant {
    Participant::new(role).with(PersonField::Surname, surname).with(PersonField::Given, given)
}

#[test]
fn birth_entry_links_existing_and_creates_new() {
    let mut g = Gedcom::parse(SAMPLE).unwrap();
    let mut e = Entry::new(template("NAISSANCE"), SourceChoice::Existing("S00001".into()));
    e.date = "3 FEB 1914".into();
    e.place = ", Saint-Lô, 50000, Manche, Normandie, France".into();
    e.set_field("numero", "27");
    e.set_field("vue", "12");
    e.transcription = "L'an mil neuf cent quatorze…".into();
    e.media = MediaChoice::NewFile("Actes/naissance_1914_27.jpg".into());
    e.participants = vec![
        person("enfant", "", "Louise").with(PersonField::Sex, "F"),
        person("pere", "MARCHAND", "Émile")
            .with(PersonField::Age, "40 ans")
            .with(PersonField::Occupation, "Cultivateur"),
        person("mere", "ROUSSEL", "Jeanne"),
        person("temoin", "LEFEBVRE", "François").with(PersonField::Kinship, "voisin"),
    ];

    // Le père et la mère existants sont proposés.
    let s = e.suggestions(&g, 1);
    assert_eq!(s[0].xref, "I00002", "{s:?}");
    assert!(!s.iter().any(|s| s.xref == "I00001"), "Paul (né en 1912) ne peut pas être le père : {s:?}");
    e.participants[1].choice = PersonChoice::Link("I00002".into());
    let s = e.suggestions(&g, 2);
    assert_eq!(s[0].xref, "I00003");
    // Sans nom saisi, la mère est déduite du père lié.
    let mut blank = e.clone();
    blank.participants[2] = Participant::new("mere");
    let s = blank.suggestions(&g, 2);
    assert_eq!(s[0].xref, "I00003", "{s:?}");
    assert!(s[0].reason.contains("conjoint"));
    e.participants[2].choice = PersonChoice::Link("I00003".into());

    let preview = e.preview(&g).unwrap();
    assert!(preview.contains("3 PAGE Acte n° 27, vue 12"), "{preview}");

    let res = e.apply(&mut g).unwrap();
    assert_eq!(res.created.len(), 3, "enfant, témoin, média : {res:?}");
    assert!(res.families_created.is_empty(), "la famille existante doit être réutilisée");

    let child = res.persons[0].clone().unwrap();
    let louise = g.individual(&child).unwrap();
    assert_eq!(louise.display_name(), "Louise MARCHAND", "nom hérité du père");
    assert_eq!(louise.sex(), "F");
    assert_eq!(louise.famc(), vec!["F00001"]);
    assert!(g.family("F00001").unwrap().children().contains(&child.as_str()));
    let birt = louise.node.child("BIRT").unwrap();
    assert_eq!(birt.child_val("DATE"), Some("3 FEB 1914"));
    let cite = birt.child("SOUR").unwrap();
    assert_eq!(cite.child_val("PAGE"), Some("Acte n° 27, vue 12"));
    assert_eq!(cite.path_val(&["EVEN", "ROLE"]), Some("CHIL"));
    assert!(cite.child("OBJE").is_some());
    assert_eq!(cite.child_val("_GR_FLD"), Some("numero 27"));
    assert!(cite.path_val(&["DATA", "TEXT"]).unwrap().starts_with("L'an"));
    // Témoin associé à l'événement.
    let witness = res.persons[3].clone().unwrap();
    let asso = birt.child("ASSO").unwrap();
    assert_eq!(asso.val(), format!("@{witness}@"));
    assert_eq!(asso.path_val(&["ROLE", "PHRASE"]), Some("voisin"));

    // Le père : citation avec son rôle, profession datée ; pas de naissance estimée (il en a une).
    let emile = g.get("I00002").unwrap();
    assert!(emile.children.iter().any(|c| c.tag == "SOUR" && c.path_val(&["EVEN", "ROLE"]) == Some("FATH")));
    let occu = emile.children.iter().find(|c| c.tag == "OCCU").unwrap();
    assert_eq!(occu.val(), "Cultivateur");
    assert_eq!(occu.child_val("DATE"), Some("3 FEB 1914"));
    assert_eq!(emile.children.iter().filter(|c| c.tag == "BIRT").count(), 1);

    // Source : modèle mémorisé, image liée, extensions déclarées.
    let src = g.get("S00001").unwrap();
    assert_eq!(src.child_val("_GR_TPL"), Some("NAISSANCE"));
    assert_eq!(src.children_with("OBJE").count(), 2);
    assert_eq!(g.source_template("S00001").unwrap().name, "Naissance");
    assert!(g.header.child("SCHMA").unwrap().children.iter().any(|c| c.val().starts_with("_GR_TPL ")));

    // Le fichier reste du GEDCOM valide qui se relit à l'identique.
    let text = g.to_gedcom_string();
    assert_eq!(Gedcom::parse(&text).unwrap().to_gedcom_string(), text);
}

#[test]
fn marriage_creates_family_and_parents() {
    let mut g = Gedcom::parse(SAMPLE).unwrap();
    let mut e = Entry::new(
        template("MARIAGE"),
        SourceChoice::New {
            title: "Mariages, Saint-Lô, 1936".into(),
            publication: String::new(),
            repository: Some("R00001".into()),
            call_number: "3 E 502/140".into(),
        },
    );
    e.date = "12 SEP 1936".into();
    e.set_field("numero", "40");
    e.participants = vec![
        person("epoux", "MARCHAND", "Paul Émile")
            .with(PersonField::Age, "24")
            .with(PersonField::Occupation, "Instituteur"),
        person("epouse", "DUVAL", "Marie").with(PersonField::Age, "21 ans").with(PersonField::Birthplace, "Caen"),
        person("pere_epoux", "MARCHAND", "Émile"),
        person("pere_epouse", "DUVAL", "Henri"),
        person("mere_epouse", "LEROY", "Anne"),
        person("temoin", "LEFEBVRE", "François"),
    ];
    let s = e.suggestions(&g, 0);
    assert_eq!(s[0].xref, "I00001");
    e.participants[0].choice = PersonChoice::Link("I00001".into());
    e.participants[2].choice = PersonChoice::Link("I00002".into());
    e.participants[5].choice = PersonChoice::Mention;

    let res = e.apply(&mut g).unwrap();
    let marie = res.persons[1].clone().unwrap();
    let src = g.get(&res.source).unwrap();
    assert_eq!(src.child_val("TITL"), Some("Mariages, Saint-Lô, 1936"));
    assert_eq!(src.path_val(&["REPO", "CALN"]), Some("3 E 502/140"));

    // Famille du couple avec MARR, âges des époux et mention du témoin.
    let fam = g.records_of(RecordKind::Family).find(|f| f.child_val("WIFE") == Some(&format!("@{marie}@"))).unwrap();
    assert_eq!(fam.child_val("HUSB"), Some("@I00001@"));
    let marr = fam.child("MARR").unwrap();
    assert_eq!(marr.path_val(&["HUSB", "AGE"]), Some("24y"));
    assert_eq!(marr.path_val(&["WIFE", "AGE"]), Some("21y"));
    assert!(marr.path_val(&["SOUR", "NOTE"]).unwrap().contains("François LEFEBVRE"));
    assert_eq!(res.families_created.len(), 2, "couple + parents de l'épouse");

    // Naissance estimée de l'épouse d'après son âge, au lieu déclaré.
    let birt = g.get(&marie).unwrap().child("BIRT").unwrap();
    assert_eq!(birt.child_val("DATE"), Some("EST 1915"));
    assert_eq!(birt.child_val("PLAC"), Some("Caen"));
    // Ses parents sont liés.
    let (f, m) = g.parents(&marie);
    assert_eq!(f.unwrap().display_name(), "Henri DUVAL");
    assert_eq!(m.unwrap().display_name(), "Anne LEROY");
    // Paul est déjà enfant d'Émile : pas de nouvelle famille d'origine.
    assert_eq!(g.individual("I00001").unwrap().famc(), vec!["F00001"]);
}

#[test]
fn census_builds_household() {
    let mut g = Gedcom::parse(SAMPLE).unwrap();
    let mut e = Entry::new(template("RECENSEMENT"), SourceChoice::Existing("S00001".into()));
    e.date = "1921".into();
    e.participants = vec![
        person("chef", "MARTIN", "Jules").with(PersonField::Sex, "M").with(PersonField::Age, "45"),
        person("membre", "DURAND", "Rose").with(PersonField::Sex, "F").with(PersonField::Kinship, "épouse"),
        person("membre", "MARTIN", "Lucien")
            .with(PersonField::Sex, "M")
            .with(PersonField::Kinship, "fils")
            .with(PersonField::Age, "12"),
        person("membre", "MARTIN", "Paul").with(PersonField::Kinship, "petit-fils"),
    ];
    let res = e.apply(&mut g).unwrap();
    let ids: Vec<String> = res.persons.iter().map(|x| x.clone().unwrap()).collect();
    let (f, m) = g.parents(&ids[2]);
    assert_eq!(f.unwrap().xref(), ids[0]);
    assert_eq!(m.unwrap().xref(), ids[1]);
    assert!(g.individual(&ids[3]).unwrap().famc().is_empty(), "petit-fils : pas de lien direct");
    // Chaque membre a son propre recensement.
    for x in &ids {
        assert!(g.get(x).unwrap().child("CENS").is_some() || g.get(x).unwrap().child("SOUR").is_some());
    }
    assert_eq!(g.get(&ids[0]).unwrap().path_val(&["CENS", "AGE"]), Some("45y"));
    assert_eq!(g.get(&ids[2]).unwrap().path_val(&["BIRT", "DATE"]), Some("EST 1909"));
}

#[test]
fn single_parent_joins_their_only_family() {
    let mut g = Gedcom::parse(SAMPLE).unwrap();
    let mut e = Entry::new(template("NAISSANCE"), SourceChoice::Existing("S00001".into()));
    e.date = "1915".into();
    e.participants = vec![person("enfant", "", "Louis"), Participant::new("pere"), Participant::new("mere")];
    e.participants[1].choice = PersonChoice::Link("I00002".into());
    let res = e.apply(&mut g).unwrap();
    assert!(res.families_created.is_empty());
    assert_eq!(g.individual(res.persons[0].as_ref().unwrap()).unwrap().famc(), vec!["F00001"]);
    assert_eq!(res.summary(), "Acte enregistré : 2 personnes dont 1 créée");
}

#[test]
fn new_source_gets_automatic_title() {
    let mut g = Gedcom::parse(SAMPLE).unwrap();
    let new = SourceChoice::New {
        title: String::new(),
        publication: String::new(),
        repository: None,
        call_number: String::new(),
    };
    let mut e = Entry::new(template("NAISSANCE"), new.clone());
    e.participants = vec![person("enfant", "Guyot", "Rémi")];
    assert_eq!(e.auto_title(), "Acte de naissance de Rémi GUYOT");
    let res = e.apply(&mut g).unwrap();
    assert_eq!(g.record_label(&res.source), "Acte de naissance de Rémi GUYOT");
    let mut m = Entry::new(template("MARIAGE"), new);
    m.participants = vec![person("epoux", "GUYOT", "Rémi"), person("epouse", "FAUVEL", "Jeanne")];
    assert_eq!(m.auto_title(), "Acte de mariage de Rémi GUYOT et Jeanne FAUVEL");
    let d = Entry::new(template("INHUMATION"), SourceChoice::Existing("S00001".into()));
    assert_eq!(d.auto_title(), "Acte d'inhumation");
}

#[test]
fn existing_citation_is_completed_not_duplicated() {
    let mut g = Gedcom::parse(SAMPLE).unwrap();
    // Paul a déjà une naissance citant S00001 (« Acte n° 112, vue 37 ») : on la complète.
    let mut e = Entry::new(template("NAISSANCE"), SourceChoice::Existing("S00001".into()));
    e.date = "14 MAR 1912".into();
    e.set_field("numero", "112");
    e.set_field("vue", "37");
    e.participants = vec![person("enfant", "MARCHAND", "Paul")];
    e.participants[0].choice = PersonChoice::Link("I00001".into());
    e.apply(&mut g).unwrap();
    let birt = g.get("I00001").unwrap().child("BIRT").unwrap();
    assert_eq!(birt.children_with("SOUR").count(), 1);
    assert_eq!(birt.path_val(&["SOUR", "EVEN", "ROLE"]), Some("CHIL"));
    assert_eq!(birt.path_val(&["SOUR", "PAGE"]), Some("Acte n° 112, vue 37"));
}

#[test]
fn validation_errors() {
    let g = Gedcom::parse(SAMPLE).unwrap();
    let e = Entry::new(template("NAISSANCE"), SourceChoice::Existing("S00001".into()));
    assert!(e.validate(&g).is_err(), "acte vide");
    let mut e = e;
    e.participants[0] = person("enfant", "X", "Y");
    e.date = "n'importe quoi".into();
    assert!(e.validate(&g).unwrap_err().contains("date"));
    e.date = "1900".into();
    assert!(e.validate(&g).is_ok());
    e.participants[0].choice = PersonChoice::Mention;
    assert!(e.validate(&g).is_err());
}

#[test]
fn custom_template_stored_in_file() {
    let mut g = Gedcom::parse(SAMPLE).unwrap();
    let mut t = template("DECES");
    t.name = "Décès (table décennale)".into();
    t.page = "Table {annee}".into();
    t.fields = vec![FieldDef { key: "annee".into(), label: "Années".into() }];
    let x = g.store_template(&t);
    let text = g.to_gedcom_string();
    assert!(text.contains(&format!("0 @{x}@ _GR_TPL")), "{text}");
    let g2 = Gedcom::parse(&text).unwrap();
    let back = g2.templates().into_iter().find(|t| t.id == x).unwrap();
    assert!(back.custom);
    assert_eq!(back.page, "Table {annee}");
    assert_eq!(back.roles, t.roles);
}

#[test]
fn next_entry_increments_number() {
    let mut e = Entry::new(template("NAISSANCE"), SourceChoice::Existing("S00001".into()));
    e.set_field("numero", "112");
    e.set_field("vue", "37");
    e.place = "Saint-Lô".into();
    let n = e.next();
    assert_eq!(n.field("numero"), "113");
    assert_eq!(n.field("vue"), "37");
    assert_eq!(n.place, "Saint-Lô");
    assert!(n.participants.iter().all(|p| p.is_blank()));
}

#[test]
fn ages() {
    assert_eq!(entry::gedcom_age("32 ans").as_deref(), Some("32y"));
    assert_eq!(entry::gedcom_age("6 mois").as_deref(), Some("6m"));
    assert_eq!(entry::parse_age_years("6 mois"), Some(0));
    assert_eq!(entry::parse_age_years("81"), Some(81));
    assert_eq!(entry::parse_age_years(""), None);
}
