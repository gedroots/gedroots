use gedcom::*;

const SAMPLE: &str = include_str!("fixtures/exemple.ged");

/// Une photo de groupe ajoutée au fichier d'exemple.
fn with_group_photo() -> Gedcom {
    let mut g = Gedcom::parse(SAMPLE).unwrap();
    let m = g.create(RecordKind::Media);
    let node = g.get_mut(&m).unwrap();
    let f = node.ensure_child("FILE");
    f.value = Some("Photos/mariage.jpg".into());
    f.set_child_val("FORM", "image/jpeg");
    assert_eq!(m, "M00002");
    g
}

#[test]
fn tag_people_on_a_photo() {
    let mut g = with_group_photo();
    let crop = Crop::from_rect((340, 120, 200, 260));
    g.tag_person("I00002", "M00002", crop).unwrap();
    g.tag_person("I00003", "M00002", Crop::from_rect((600, 100, 180, 240))).unwrap();

    let people = g.people_on("M00002");
    assert_eq!(people.iter().map(|l| l.owner.as_str()).collect::<Vec<_>>(), ["I00002", "I00003"]);
    assert_eq!(g.link_crop("I00002", "M00002"), Some(crop));

    // Écriture standard GEDCOM 7, relue à l'identique.
    let text = g.to_gedcom_string();
    assert!(text.contains("1 OBJE @M00002@\n2 CROP\n3 TOP 120\n3 LEFT 340\n3 HEIGHT 260\n3 WIDTH 200\n"));
    let g2 = Gedcom::parse(&text).unwrap();
    assert_eq!(g2.link_crop("I00002", "M00002"), Some(crop));
    assert_eq!(g2.to_gedcom_string(), text);

    // Redessiner remplace la zone ; un seul lien est conservé.
    let moved = Crop::from_rect((350, 130, 190, 250));
    g.tag_person("I00002", "M00002", moved).unwrap();
    assert_eq!(g.link_crop("I00002", "M00002"), Some(moved));
    assert_eq!(g.media_links("M00002").iter().filter(|l| l.owner == "I00002").count(), 1);

    g.untag_person("I00002", "M00002");
    assert_eq!(g.people_on("M00002").len(), 1);
    assert!(g.individual("I00002").unwrap().media_xrefs().is_empty());
}

#[test]
fn crop_on_existing_link_keeps_other_substructures() {
    let mut g = Gedcom::parse(SAMPLE).unwrap();
    // Le lien source → acte existe déjà : on ne fait qu'y ajouter le recadrage.
    let r = g.get_mut("S00001").unwrap();
    let link = r.children.iter_mut().find(|c| c.tag == "OBJE").unwrap();
    link.children.push(Node::with_value("TITL", "Vue 37"));
    g.set_link_crop("S00001", "M00001", Some(Crop { top: 10, left: 0, height: None, width: Some(400) })).unwrap();
    let link = g.get("S00001").unwrap().children.iter().find(|c| c.tag == "OBJE").unwrap();
    assert_eq!(link.children.iter().map(|c| c.tag.as_str()).collect::<Vec<_>>(), ["CROP", "TITL"]);
    assert_eq!(g.media_links("M00001").len(), 2);

    g.set_link_crop("S00001", "M00001", None).unwrap();
    assert_eq!(g.link_crop("S00001", "M00001"), None);
    assert!(g.media_links("M00001").iter().all(|l| l.crop.is_none()));
}

#[test]
fn errors() {
    let mut g = with_group_photo();
    assert!(g.tag_person("S00001", "M00002", Crop::default()).is_err());
    assert!(g.tag_person("I00001", "I00002", Crop::default()).is_err());
    // Retirer un recadrage inexistant ne crée pas de lien.
    g.set_link_crop("I00002", "M00002", None).unwrap();
    assert!(g.media_links("M00002").is_empty());
}
