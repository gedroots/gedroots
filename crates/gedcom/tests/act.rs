use gedcom::*;

const SAMPLE: &str = include_str!("fixtures/example.ged");

/// These tests check the texts the app writes in French, the language of the sample files.
fn french() {
    rust_i18n::set_locale("fr");
}

fn person(role: ActRole, surname: &str, given: &str) -> Participant {
    Participant::new(role).with(PersonField::Surname, surname).with(PersonField::Given, given)
}

/// Empty source, as “New Source” creates it.
fn new_source(g: &mut Gedcom) -> String {
    g.create(RecordKind::Source)
}

#[test]
fn birth_act_links_existing_and_creates_new() {
    french();
    let mut g = Gedcom::parse(SAMPLE).unwrap();
    let mut e = Act::new("S00001", "BIRT");
    e.date = "3 FEB 1914".into();
    e.place = ", Saint-Lô, 50000, Manche, Normandie, France".into();
    e.page = "Acte n° 27, vue 12".into();
    e.transcription = "L'an mil neuf cent quatorze…".into();
    e.media = MediaChoice::NewFiles(vec!["Actes/naissance_1914_27.jpg".into()]);
    e.participants = vec![
        person(ActRole::Principal, "", "Louise").with(PersonField::Sex, "F"),
        person(ActRole::Father, "MARCHAND", "Émile")
            .with(PersonField::Age, "40 ans")
            .with(PersonField::Occupation, "Cultivateur"),
        person(ActRole::Mother, "ROUSSEL", "Jeanne"),
        person(ActRole::Witness, "LEFEBVRE", "François").with(PersonField::Kinship, "voisin"),
    ];

    // The existing father and mother are suggested.
    let s = e.suggestions(&g, 1);
    assert_eq!(s[0].xref, "I00002", "{s:?}");
    assert!(!s.iter().any(|s| s.xref == "I00001"), "Paul (né en 1912) ne peut pas être le père : {s:?}");
    e.participants[1].choice = PersonChoice::Link("I00002".into());
    let s = e.suggestions(&g, 2);
    assert_eq!(s[0].xref, "I00003");
    // Without a typed name, the mother is inferred from the linked father.
    let mut blank = e.clone();
    blank.participants[2] = Participant::new(ActRole::Mother);
    let s = blank.suggestions(&g, 2);
    assert_eq!(s[0].xref, "I00003", "{s:?}");
    assert!(s[0].reason.contains("conjoint"));
    e.participants[2].choice = PersonChoice::Link("I00003".into());

    let preview = e.preview(&g).unwrap();
    assert!(preview.contains("3 PAGE Acte n° 27, vue 12"), "{preview}");

    let res = e.save(&mut g).unwrap();
    assert_eq!(res.created.len(), 3, "enfant, témoin, média : {res:?}");
    assert!(res.families_created.is_empty(), "la famille existante doit être réutilisée");

    let child = res.persons[0].clone().unwrap();
    let louise = g.individual(&child).unwrap();
    assert_eq!(louise.display_name(), "Louise MARCHAND", "nom hérité du père");
    assert_eq!(louise.sex(), "F");
    assert_eq!(louise.famc(), vec!["F00001"]);
    let birt = louise.node.child("BIRT").unwrap();
    assert_eq!(birt.child_val("DATE"), Some("3 FEB 1914"));
    let cite = birt.child("SOUR").unwrap();
    assert_eq!(cite.child_val("PAGE"), Some("Acte n° 27, vue 12"));
    assert_eq!(cite.path_val(&["EVEN", "ROLE"]), Some("CHIL"));
    assert!(cite.child("OBJE").is_some());
    assert!(cite.path_val(&["DATA", "TEXT"]).unwrap().starts_with("L'an"));
    // Types: the source contains births of 1914, the image is a manuscript.
    let content = SourceContent::read(g.get("S00001").unwrap());
    assert!(content.events.contains(&"BIRT".to_string()), "{content:?}");
    assert!(content.period.contains("1914"), "{content:?}");
    assert_eq!(medium(g.get(res.media.as_deref().unwrap()).unwrap()), Some("MANUSCRIPT"));
    // Witness associated with the event.
    let witness = res.persons[3].clone().unwrap();
    let asso = birt.child("ASSO").unwrap();
    assert_eq!(asso.val(), format!("@{witness}@"));
    assert_eq!(asso.path_val(&["ROLE", "PHRASE"]), Some("voisin"));

    // The father: citation with his role, dated occupation; no estimated birth (he has one).
    let emile = g.get("I00002").unwrap();
    assert!(emile.children.iter().any(|c| c.tag == "SOUR" && c.path_val(&["EVEN", "ROLE"]) == Some("FATH")));
    let occu = emile.children.iter().find(|c| c.tag == "OCCU").unwrap();
    assert_eq!(occu.val(), "Cultivateur");
    assert_eq!(occu.child_val("DATE"), Some("3 FEB 1914"));
    assert_eq!(emile.children.iter().filter(|c| c.tag == "BIRT").count(), 1);

    // Source: image linked, and no extension written.
    let src = g.get("S00001").unwrap();
    assert_eq!(src.children_with("OBJE").count(), 2);
    let text = g.to_gedcom_string();
    assert!(!text.contains("_GR_"), "{text}");
    // The file stays valid GEDCOM that reads back identically.
    assert_eq!(Gedcom::parse(&text).unwrap().to_gedcom_string(), text);
}

#[test]
fn source_lists_its_acts_from_citations() {
    french();
    let mut g = Gedcom::parse(SAMPLE).unwrap();
    // Paul's existing citation (“Acte n° 112, vue 37”) is already a record.
    let acts = g.acts("S00001");
    assert_eq!(acts.len(), 1);
    assert_eq!(acts[0].event, "BIRT");
    assert_eq!(acts[0].page, "Acte n° 112, vue 37");
    assert_eq!(acts[0].participants[0].choice, PersonChoice::Link("I00001".into()));
    assert_eq!(acts[0].title(), "Naissance · Paul Émile MARCHAND");

    let mut e = Act::new("S00001", "BIRT");
    e.date = "3 FEB 1914".into();
    e.page = "Acte n° 27".into();
    e.participants = vec![
        person(ActRole::Principal, "MARCHAND", "Louise").with(PersonField::Sex, "F"),
        person(ActRole::Father, "MARCHAND", "Émile")
            .with(PersonField::Age, "40")
            .with(PersonField::Occupation, "Cultivateur"),
        person(ActRole::Godmother, "LEFEBVRE", "Marie").with(PersonField::Kinship, "tante"),
        person(ActRole::Witness, "DUPONT", "Jean").with(PersonField::Age, "32 ans"),
    ];
    e.participants[1].choice = PersonChoice::Link("I00002".into());
    e.participants[3].choice = PersonChoice::Mention;
    e.transcription = "Texte".into();
    e.quality = Some(3);
    e.save(&mut g).unwrap();

    let acts = g.acts("S00001");
    assert_eq!(acts.len(), 2, "{acts:#?}");
    let a = acts.iter().find(|a| a.page == "Acte n° 27").unwrap();
    assert_eq!(a.event, "BIRT");
    assert_eq!(a.date, "3 FEB 1914");
    assert_eq!(a.record_date, "", "même date que l'événement");
    assert_eq!(a.transcription, "Texte");
    assert_eq!(a.quality, Some(3));
    let roles: Vec<ActRole> = a.participants.iter().map(|p| p.role).collect();
    assert_eq!(roles, vec![ActRole::Principal, ActRole::Father, ActRole::Godmother, ActRole::Witness]);
    let father = &a.participants[1];
    assert_eq!(father.choice, PersonChoice::Link("I00002".into()));
    assert_eq!(father.get(PersonField::Occupation), "Cultivateur");
    let godmother = &a.participants[2];
    assert_eq!(godmother.get(PersonField::Kinship), "tante");
    // The mentioned person is read back from the citation's note.
    let witness = &a.participants[3];
    assert_eq!(witness.choice, PersonChoice::Mention);
    assert_eq!(witness.get(PersonField::Given), "Jean");
    assert_eq!(witness.get(PersonField::Surname), "DUPONT");
    assert_eq!(witness.get(PersonField::Note), "32 ans");
    assert_eq!(a.subtitle(), "Acte n° 27 · 3 févr. 1914");
}

#[test]
fn reopened_act_is_replaced_not_duplicated() {
    french();
    let mut g = Gedcom::parse(SAMPLE).unwrap();
    let s = new_source(&mut g);
    let mut e = Act::new(&s, "DEAT");
    e.date = "1950".into();
    e.place = "Caen".into();
    e.participants = vec![
        person(ActRole::Principal, "MARTIN", "Jules")
            .with(PersonField::Sex, "M")
            .with(PersonField::Occupation, "Maçon"),
        person(ActRole::Declarant, "MARTIN", "Pierre").with(PersonField::Kinship, "fils"),
    ];
    let res = e.save(&mut g).unwrap();
    let jules = res.persons[0].clone().unwrap();
    let pierre = res.persons[1].clone().unwrap();
    assert_eq!(g.record_label(&s), "Acte de décès de Jules MARTIN", "titre automatique d'une nouvelle source");

    let mut a = g.acts(&s).pop().unwrap();
    assert_eq!(a.participants[1].role, ActRole::Declarant);
    assert_eq!(a.participants[1].get(PersonField::Kinship), "fils");
    // Correction: precise date, another occupation, without an informant.
    a.date = "12 MAR 1950".into();
    a.participants[0].set(PersonField::Occupation, "Charpentier");
    a.participants.truncate(1);
    a.save(&mut g).unwrap();

    let node = g.get(&jules).unwrap();
    let deaths: Vec<&Node> = node.children_with("DEAT").collect();
    assert_eq!(deaths.len(), 1, "{node:?}");
    assert_eq!(deaths[0].child_val("DATE"), Some("12 MAR 1950"));
    assert_eq!(deaths[0].child_val("PLAC"), Some("Caen"));
    assert!(deaths[0].child("ASSO").is_none(), "le déclarant retiré n'est plus associé");
    let occus: Vec<&str> = node.children_with("OCCU").map(|o| o.val()).collect();
    assert_eq!(occus, vec!["Charpentier"]);
    // The removed informant keeps their record, without a citation of the record.
    let p = g.get(&pierre).unwrap();
    assert!(!p.children.iter().any(|c| c.tag == "SOUR"));
    assert_eq!(g.acts(&s).len(), 1);
    // Events keep their place in the person's page.
    let mut a = g.acts("S00001").remove(0);
    a.date = "15 MAR 1912".into();
    a.save(&mut g).unwrap();
    let paul: Vec<&str> = g.get("I00001").unwrap().children.iter().map(|c| c.tag.as_str()).collect();
    assert_eq!(&paul[..4], &["NAME", "_SOSADABOVILLE", "SEX", "BIRT"]);
    assert_eq!(g.get("I00001").unwrap().path_val(&["BIRT", "DATE"]), Some("15 MAR 1912"));
}

#[test]
fn removing_an_act_keeps_other_data() {
    french();
    let mut g = Gedcom::parse(SAMPLE).unwrap();
    // Deleting Paul's record removes the citation, not the birth.
    let key = g.acts("S00001")[0].original.clone().unwrap();
    g.remove_act("S00001", &key);
    let birt = g.get("I00001").unwrap().child("BIRT").unwrap();
    assert!(birt.child("SOUR").is_none());
    assert_eq!(birt.child_val("DATE"), Some("14 MAR 1912"));
    assert!(birt.child("PLAC").is_some());
    assert!(g.acts("S00001").is_empty());
    // The individual and their family stay.
    assert_eq!(g.individual("I00001").unwrap().famc(), vec!["F00001"]);
}

#[test]
fn marriage_creates_family_and_parents() {
    french();
    let mut g = Gedcom::parse(SAMPLE).unwrap();
    let s = new_source(&mut g);
    let mut e = Act::new(&s, "MARR");
    assert_eq!(e.participants.len(), 6, "époux, épouse et leurs parents");
    e.date = "12 SEP 1936".into();
    e.page = "Acte n° 40".into();
    e.participants = vec![
        person(ActRole::Principal, "MARCHAND", "Paul Émile")
            .with(PersonField::Age, "24")
            .with(PersonField::Occupation, "Instituteur"),
        person(ActRole::Principal, "DUVAL", "Marie")
            .with(PersonField::Age, "21 ans")
            .with(PersonField::Birthplace, "Caen"),
        person(ActRole::Father, "MARCHAND", "Émile"),
        person(ActRole::Father, "DUVAL", "Henri").of(1),
        person(ActRole::Mother, "LEROY", "Anne").of(1),
        person(ActRole::Witness, "LEFEBVRE", "François"),
    ];
    assert_eq!(e.role_label(1), "Épouse");
    let s0 = e.suggestions(&g, 0);
    assert_eq!(s0[0].xref, "I00001");
    e.participants[0].choice = PersonChoice::Link("I00001".into());
    e.participants[2].choice = PersonChoice::Link("I00002".into());
    e.participants[5].choice = PersonChoice::Mention;

    let res = e.save(&mut g).unwrap();
    let marie = res.persons[1].clone().unwrap();
    assert_eq!(g.record_label(&s), "Acte de mariage de Paul Émile MARCHAND et Marie DUVAL");

    // Family of the couple with MARR, ages of the spouses and mention of the witness.
    let fam = g.records_of(RecordKind::Family).find(|f| f.child_val("WIFE") == Some(&format!("@{marie}@"))).unwrap();
    assert_eq!(fam.child_val("HUSB"), Some("@I00001@"));
    let marr = fam.child("MARR").unwrap();
    assert_eq!(marr.path_val(&["HUSB", "AGE"]), Some("24y"));
    assert_eq!(marr.path_val(&["WIFE", "AGE"]), Some("21y"));
    assert!(marr.path_val(&["SOUR", "NOTE"]).unwrap().contains("François LEFEBVRE"));
    assert_eq!(res.families_created.len(), 2, "couple + parents de l'épouse");

    // Estimated birth of the wife from her age, at the declared place.
    let birt = g.get(&marie).unwrap().child("BIRT").unwrap();
    assert_eq!(birt.child_val("DATE"), Some("EST 1915"));
    assert_eq!(birt.child_val("PLAC"), Some("Caen"));
    let (f, m) = g.parents(&marie);
    assert_eq!(f.unwrap().display_name(), "Henri DUVAL");
    assert_eq!(m.unwrap().display_name(), "Anne LEROY");
    // Paul is already Émile's child: no new family of origin.
    assert_eq!(g.individual("I00001").unwrap().famc(), vec!["F00001"]);
    // Detailed role: father of the bride.
    let henri = res.persons[3].clone().unwrap();
    let cite = g.get(&henri).unwrap().children.iter().find(|c| c.tag == "SOUR").unwrap().clone();
    assert_eq!(cite.path_val(&["EVEN", "ROLE", "PHRASE"]), Some("Père de l'épouse"));

    // Read back: the spouses, then each one's parents.
    let a = g.acts(&s).pop().unwrap();
    assert!(a.is_family());
    assert_eq!(a.participants[0].choice, PersonChoice::Link("I00001".into()));
    assert_eq!(a.participants[1].get(PersonField::Age), "21 ans");
    let henri_row = a.participants.iter().find(|p| p.choice == PersonChoice::Link(henri.clone())).unwrap();
    assert_eq!((henri_row.role, henri_row.of), (ActRole::Father, 1));
    let emile = a.participants.iter().find(|p| p.choice == PersonChoice::Link("I00002".into())).unwrap();
    assert_eq!((emile.role, emile.of), (ActRole::Father, 0));
}

#[test]
fn census_builds_household() {
    french();
    let mut g = Gedcom::parse(SAMPLE).unwrap();
    let mut e = Act::new("S00001", "CENS");
    e.date = "1921".into();
    e.participants = vec![
        person(ActRole::Principal, "MARTIN", "Jules").with(PersonField::Sex, "M").with(PersonField::Age, "45"),
        person(ActRole::Principal, "DURAND", "Rose").with(PersonField::Sex, "F").with(PersonField::Kinship, "épouse"),
        person(ActRole::Principal, "MARTIN", "Lucien")
            .with(PersonField::Sex, "M")
            .with(PersonField::Kinship, "fils")
            .with(PersonField::Age, "12"),
        person(ActRole::Principal, "MARTIN", "Paul").with(PersonField::Kinship, "petit-fils"),
    ];
    assert_eq!(e.role_label(0), "Chef de ménage");
    assert_eq!(e.role_label(2), "Membre du ménage");
    let res = e.save(&mut g).unwrap();
    let ids: Vec<String> = res.persons.iter().map(|x| x.clone().unwrap()).collect();
    let (f, m) = g.parents(&ids[2]);
    assert_eq!(f.unwrap().xref(), ids[0]);
    assert_eq!(m.unwrap().xref(), ids[1]);
    assert!(g.individual(&ids[3]).unwrap().famc().is_empty(), "petit-fils : pas de lien direct");
    // Each member has their own census.
    for x in &ids {
        assert!(g.get(x).unwrap().child("CENS").is_some());
    }
    assert_eq!(g.get(&ids[0]).unwrap().path_val(&["CENS", "AGE"]), Some("45y"));
    assert_eq!(g.get(&ids[2]).unwrap().path_val(&["BIRT", "DATE"]), Some("EST 1909"));
    let lucien = g.get(&ids[2]).unwrap().child("CENS").unwrap().child("SOUR").unwrap().clone();
    assert_eq!(lucien.path_val(&["EVEN", "ROLE"]), Some("OTHER"));
    assert_eq!(lucien.path_val(&["EVEN", "ROLE", "PHRASE"]), Some("fils"));
}

#[test]
fn any_event_and_custom_types() {
    french();
    let mut g = Gedcom::parse(SAMPLE).unwrap();
    let s = new_source(&mut g);
    let mut e = Act::new(&s, "EVEN");
    e.event_type = "Registre matricule".into();
    e.page = "Classe 1932, matricule n° 412".into();
    e.participants = vec![person(ActRole::Principal, "GUYOT", "Rémi").with(PersonField::Sex, "M")];
    e.save(&mut g).unwrap();
    let a = g.acts(&s).pop().unwrap();
    assert_eq!(a.event, "EVEN");
    assert_eq!(a.event_type, "Registre matricule");
    assert_eq!(a.title(), "Registre matricule · Rémi GUYOT");
    assert_eq!(a.next().page, "Classe 1933, matricule n° 412");
}

#[test]
fn single_parent_joins_their_only_family() {
    french();
    let mut g = Gedcom::parse(SAMPLE).unwrap();
    let mut e = Act::new("S00001", "BIRT");
    e.date = "1915".into();
    e.participants[0] = person(ActRole::Principal, "", "Louis");
    e.participants[1].choice = PersonChoice::Link("I00002".into());
    let res = e.save(&mut g).unwrap();
    assert!(res.families_created.is_empty());
    assert_eq!(g.individual(res.persons[0].as_ref().unwrap()).unwrap().famc(), vec!["F00001"]);
    assert_eq!(res.summary(), "Acte enregistré : 2 personnes dont 1 créée");
}

#[test]
fn existing_citation_is_completed_not_duplicated() {
    french();
    let mut g = Gedcom::parse(SAMPLE).unwrap();
    // Paul already has a birth citing S00001 (“Acte n° 112, vue 37”): it is completed.
    let mut e = Act::new("S00001", "BIRT");
    e.page = "Acte n° 112, vue 37".into();
    e.participants = vec![person(ActRole::Principal, "MARCHAND", "Paul")];
    e.participants[0].choice = PersonChoice::Link("I00001".into());
    e.save(&mut g).unwrap();
    let birt = g.get("I00001").unwrap().child("BIRT").unwrap();
    assert_eq!(birt.children_with("SOUR").count(), 1);
    assert_eq!(birt.path_val(&["SOUR", "EVEN", "ROLE"]), Some("CHIL"));
    assert_eq!(birt.path_val(&["SOUR", "PAGE"]), Some("Acte n° 112, vue 37"));
}

#[test]
fn validation_errors() {
    french();
    let g = Gedcom::parse(SAMPLE).unwrap();
    let e = Act::new("S00001", "BIRT");
    assert!(e.validate(&g).is_err(), "acte vide");
    let mut e = e;
    e.participants[0] = person(ActRole::Principal, "X", "Y");
    e.date = "n'importe quoi".into();
    assert!(e.validate(&g).unwrap_err().contains("date"));
    e.date = "1900".into();
    assert!(e.validate(&g).is_ok());
    e.participants[0].choice = PersonChoice::Mention;
    assert!(e.validate(&g).is_err());
    let mut m = Act::new("S00001", "MARR");
    m.participants[0] = person(ActRole::Principal, "A", "B");
    assert!(m.validate(&g).unwrap_err().contains("époux"));
    let mut t = Act::new("S00001", "EVEN");
    t.participants[0] = person(ActRole::Principal, "A", "B");
    assert!(t.validate(&g).unwrap_err().contains("type"));
}

#[test]
fn next_act_increments_reference() {
    french();
    let mut e = Act::new("S00001", "BIRT");
    e.page = "Acte n° 112, vue 37".into();
    e.place = "Saint-Lô".into();
    let n = e.next();
    assert_eq!(n.page, "Acte n° 113, vue 37");
    assert_eq!(n.place, "Saint-Lô");
    assert!(n.participants.iter().all(|p| p.is_blank()));
    assert_eq!(act::increment_first_number("f° 009"), "f° 010");
    assert_eq!(act::increment_first_number("sans numéro"), "sans numéro");
}

#[test]
fn ages() {
    french();
    assert_eq!(act::gedcom_age("32 ans").as_deref(), Some("32y"));
    assert_eq!(act::gedcom_age("6 mois").as_deref(), Some("6m"));
    assert_eq!(act::parse_age_years("6 mois"), Some(0));
    assert_eq!(act::parse_age_years("81"), Some(81));
    assert_eq!(act::parse_age_years(""), None);
}

#[test]
fn act_with_several_files_makes_one_paged_media_named_by_template() {
    french();
    let mut g = Gedcom::parse(SAMPLE).unwrap();
    g.set_media_naming(&MediaNaming {
        root: "Médias".into(),
        folders: vec![MediaFolder {
            dir: "Actes".into(),
            types: vec!["BIRT".into()],
            template: "{type:1}_{surname}_{given}".into(),
        }],
    });
    let mut e = Act::new("S00001", "BIRT");
    e.date = "3 FEB 1914".into();
    e.media = MediaChoice::NewFiles(vec!["/tmp/scan 1.JPG".into(), "/tmp/scan 2.pdf".into()]);
    e.participants = vec![person(ActRole::Principal, "MARCHAND", "Louise").with(PersonField::Sex, "F")];
    let res = e.save(&mut g).unwrap();
    let m = res.media.clone().unwrap();
    let files = Media::new(g.get(&m).unwrap()).files();
    assert_eq!(files.len(), 2, "une page par fichier");
    assert_eq!(files[0].form, Some("image/jpeg"));
    assert_eq!(files[1].form, Some("application/pdf"));
    assert!(files[0].title.is_some() && files[1].title.is_none(), "titre sur la première page seulement");
    assert_eq!(medium(g.get(&m).unwrap()), Some("MANUSCRIPT"));
    // Naming by the template, page number at the end of the name, original extension in lowercase.
    let renames: Vec<String> = g.media_renames(&m).into_iter().map(|(_, _, new)| new).collect();
    assert_eq!(renames, vec!["Médias/Actes/N_MARCHAND_Louise p1.jpg", "Médias/Actes/N_MARCHAND_Louise p2.pdf"]);
}

#[test]
fn swapped_surname_and_given_names_still_match() {
    french();
    let g = Gedcom::parse(SAMPLE).unwrap();
    let mut e = Act::new("S00001", "BIRT");
    e.date = "3 FEB 1914".into();
    e.participants = vec![
        person(ActRole::Principal, "", "Louise"),
        // “Émile” typed as the surname, “Marchand” as the given names.
        person(ActRole::Father, "Émile", "Marchand"),
    ];
    let s = e.suggestions(&g, 1);
    assert_eq!(s.first().map(|s| s.xref.as_str()), Some("I00002"), "{s:?}");
    assert!(s[0].reason.contains("nom et prénom inversés"), "{s:?}");
    let father = g.individual("I00002").unwrap();
    assert!(act::names_swapped(&e.participants[1], &father));
    assert!(!act::names_swapped(&person(ActRole::Father, "MARCHAND", "Émile"), &father));
}

#[test]
fn other_facts_are_written_with_the_citation_and_reread() {
    french();
    let mut g = Gedcom::parse(SAMPLE).unwrap();
    let s = new_source(&mut g);
    let mut e = Act::new(&s, "BAPM");
    e.date = "22 APR 1796".into();
    e.place = "Agneaux".into();
    e.page = "f° 12".into();
    let mut birth = Fact::new("BIRT");
    birth.date = "19 APR 1796".into();
    birth.place = "Agneaux".into();
    let mut death = Fact::new("DEAT");
    death.date = "12 MAR 1870".into();
    death.place = "Saint-Lô".into();
    let mut trade = Fact::new("OCCU");
    trade.value = "Tisserand".into();
    let mut child = person(ActRole::Principal, "MARCHAND", "Émile").with(PersonField::Sex, "M");
    child.facts = vec![birth.clone(), death.clone(), trade.clone()];
    e.participants = vec![
        child,
        person(ActRole::Father, "MARCHAND", "Jacques")
            .with(PersonField::Age, "32 ans")
            .with(PersonField::Occupation, "Laboureur"),
    ];
    let res = e.save(&mut g).unwrap();
    let emile = res.persons[0].clone().unwrap();
    let n = g.get(&emile).unwrap();
    for (tag, date) in [("BAPM", "22 APR 1796"), ("BIRT", "19 APR 1796"), ("DEAT", "12 MAR 1870")] {
        let ev = n.children_with(tag).next().unwrap_or_else(|| panic!("{tag} missing: {n:?}"));
        assert_eq!(ev.child_val("DATE"), Some(date));
        assert_eq!(ev.child("SOUR").and_then(|c| c.child_val("PAGE")), Some("f° 12"), "{tag}");
    }
    let occu = n.children_with("OCCU").next().unwrap();
    assert_eq!((occu.val(), occu.child_val("DATE")), ("Tisserand", None), "an undated occupation stays undated");

    // Reopened: the facts come back on the person, the father's occupation in its field.
    let a = g.acts(&s).pop().unwrap();
    assert_eq!(a.event, "BAPM");
    assert_eq!(a.participants[0].facts, vec![birth, death, trade], "{:?}", a.participants[0]);
    assert_eq!(a.participants[1].get(PersonField::Occupation), "Laboureur");
    assert!(a.participants[1].facts.is_empty(), "{:?}", a.participants[1]);
    // Saved again without change: nothing is duplicated.
    a.save(&mut g).unwrap();
    let n = g.get(&emile).unwrap();
    assert_eq!(n.children_with("DEAT").count(), 1);
    assert_eq!(n.children_with("OCCU").count(), 1);
    assert_eq!(g.acts(&s).pop().unwrap().participants[0].facts.len(), 3);
}
