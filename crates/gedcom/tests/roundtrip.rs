use gedcom::*;

const SAMPLE: &str = include_str!("fixtures/example.ged");

#[test]
fn roundtrip_is_byte_identical() {
    let g = Gedcom::parse(SAMPLE).unwrap();
    assert_eq!(g.to_gedcom_string(), SAMPLE);
}

#[test]
fn counts_and_views() {
    let g = Gedcom::parse(SAMPLE).unwrap();
    assert_eq!(g.version(), "7.0.13");
    assert_eq!(g.count(RecordKind::Individual), 3);
    assert_eq!(g.count(RecordKind::Family), 1);
    let paul = g.individual("I00001").unwrap();
    assert_eq!(paul.display_name(), "Paul Émile MARCHAND");
    assert_eq!(paul.years(), "1912–1987");
    assert_eq!(paul.sosa().as_deref(), Some("1"));
    assert_eq!(paul.sourced_ratio(), (1, 3));
    let (father, mother) = g.parents("I00001");
    assert_eq!(father.unwrap().xref(), "I00002");
    assert_eq!(mother.unwrap().given(), "Jeanne");
    assert_eq!(g.individual("I00002").unwrap().years(), "1796–<1944");
    assert_eq!(g.individual("I00003").unwrap().notes(), vec!["Couturière à Coutances.\nMariée à Saint-Lô."]);
    let anc = g.ancestry("I00001", 3);
    assert_eq!(anc[2].as_deref(), Some("I00002"));
    assert_eq!(anc[4], None);
    assert_eq!(g.record_label("F00001"), "Émile MARCHAND & Jeanne ROUSSEL");
    assert!(Media::new(g.get("M00001").unwrap()).is_image());
}

#[test]
fn places_index() {
    let g = Gedcom::parse(SAMPLE).unwrap();
    let places = g.places();
    let st_lo = places.iter().find(|p| p.short() == "Saint-Lô (Manche)").unwrap();
    assert_eq!(st_lo.uses, 2);
    assert_eq!(st_lo.latitude.as_deref(), Some("N49.1157"));
    assert_eq!(st_lo.records, vec!["I00001", "F00001"]);
}

#[test]
fn rename_place_everywhere() {
    let mut g = Gedcom::parse(SAMPLE).unwrap();
    let old = ", Saint-Lô, 50000, Manche, Normandie, France";
    let n = g.update_place(old, ", Saint-Lô, 50000, Manche, Normandie, France ", "N49.1", "W1.09");
    assert_eq!(n, 2);
    assert!(g.places().iter().all(|p| p.name != old || p.latitude.as_deref() == Some("N49.1")));
}

#[test]
fn individual_form_edits_only_its_fields() {
    let mut g = Gedcom::parse(SAMPLE).unwrap();
    let node = g.get_mut("I00001").unwrap();
    let before = node_to_string(node);
    let mut form = IndividualForm::read(node);
    form.given = "Paul".into();
    form.note = "Ligne 1\nLigne 2".into();
    form.apply(node);
    let text = node_to_string(node);
    assert!(text.contains("1 NAME Paul /MARCHAND/\n2 GIVN Paul\n"));
    assert!(text.contains("3 PAGE Acte n° 112, vue 37"), "la citation de source est conservée");
    // Birth and death are events of the timeline: the form leaves them as they are.
    for tag in ["BIRT", "DEAT"] {
        let event = |t: &str| t.split("\n1 ").find(|b| b.starts_with(tag)).map(str::to_string);
        assert!(event(&before).is_some());
        assert_eq!(event(&text), event(&before));
    }
    assert!(text.contains("1 NOTE Ligne 1\n2 CONT Ligne 2\n"));
    assert!(text.contains("1 _SOSADABOVILLE 1 G1"), "les balises propriétaires sont conservées");
}

#[test]
fn raw_editor_replace_and_rename_xref() {
    let mut g = Gedcom::parse(SAMPLE).unwrap();
    let text = node_to_string(g.get("I00002").unwrap()).replace("@I00002@", "@I00099@");
    let node = node_from_string(&text).unwrap();
    g.replace("I00002", node).unwrap();
    assert!(g.get("I00002").is_none());
    assert_eq!(g.family("F00001").unwrap().husband(), Some("I00099"));
    assert!(node_from_string("0 @X@ INDI\n2 NAME saut de niveau").is_err());
}

#[test]
fn create_and_delete() {
    let mut g = Gedcom::parse(SAMPLE).unwrap();
    assert_eq!(g.next_xref(RecordKind::Individual), "I00004");
    let x = g.create(RecordKind::Source);
    assert_eq!(x, "S00002");
    g.delete("S00001");
    assert!(g.individual("I00001").unwrap().cited_sources().is_empty());
}

/// Tests a real file: `GEDROOTS_TEST_GED=/path/to/file.ged cargo test`.
#[test]
fn real_file_roundtrip() {
    let Ok(path) = std::env::var("GEDROOTS_TEST_GED") else { return };
    let text = std::fs::read_to_string(&path).unwrap();
    let g = Gedcom::parse(&text).unwrap();
    assert_eq!(g.to_gedcom_string(), text, "relecture/réécriture à l'identique");
    for k in RecordKind::ALL {
        eprintln!("{:>14} : {}", k.label(), g.count(k));
    }
    eprintln!("{:>14} : {}", "Lieux", g.places().len());
}

#[test]
fn short_place_labels() {
    assert_eq!(short_place(", Troyes, 10000, Aube, Grand Est, France"), "Troyes (Aube)");
    assert_eq!(short_place("La Gare, Amions, 42260, Loire, Auvergne-Rhône-Alpes, France"), "La Gare, Amions (Loire)");
    assert_eq!(short_place(", , , , Pays Rhénan, Allemagne"), "Pays Rhénan (Allemagne)");
    assert_eq!(short_place(", Aïn Temouchent, 46008, , Aïn Témouchent, Algérie"), "Aïn Temouchent (Aïn Témouchent)");
    assert_eq!(short_place("Paris, France"), "Paris, France");
}

#[test]
fn new_fields_go_before_change_date() {
    let mut n = node_from_string("0 @I1@ INDI\n1 NAME Jean /DUPONT/\n1 CHAN\n2 DATE 1 JAN 2026\n").unwrap();
    n.set_child_val("OCCU", "Notaire");
    n.touch();
    let text = node_to_string(&n);
    assert!(text.starts_with("0 @I1@ INDI\n1 NAME Jean /DUPONT/\n1 OCCU Notaire\n1 CHAN\n2 DATE "), "{text}");
    assert!(text.contains("3 TIME "));
}

#[test]
fn root_extension() {
    let mut g = Gedcom::parse(SAMPLE).unwrap();
    assert_eq!(g.root().as_deref(), Some("I00001"), "Sosa 1 par défaut");
    g.set_root("I00003");
    g.set_root("I00003");
    assert_eq!(g.root().as_deref(), Some("I00003"));
    let text = g.to_gedcom_string();
    assert_eq!(text.matches("2 TAG _GR_ROOT ").count(), 1, "extension déclarée une fois");
    assert!(text.contains("1 _GR_ROOT @I00003@\n"));
    // Read back: the root survives saving.
    assert_eq!(Gedcom::parse(&text).unwrap().root().as_deref(), Some("I00003"));
    g.delete("I00003");
    assert_eq!(g.root().as_deref(), Some("I00001"));
}

#[test]
fn portraits_and_images() {
    let g = Gedcom::parse(SAMPLE).unwrap();
    assert_eq!(g.portrait("I00001").map(|m| m.xref().to_string()).as_deref(), Some("M00001"));
    assert!(g.portrait("I00002").is_none());
    assert_eq!(g.images_of("S00001").len(), 1);
}
