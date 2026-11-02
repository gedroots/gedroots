use gedcom::*;

const LEGACY: &str = "0 HEAD
1 GEDC
2 VERS 7.0
1 SCHMA
2 TAG _URL https://example.org/url
0 @I1@ INDI
1 NAME Jean /Guyot/
1 BIRT
2 DATE 1850
2 _URL https://www.archives43.fr/ark:1/naissance
0 @S1@ SOUR
1 TITL Mariage Guyot × Martin
1 _URL https://archives.saint-etienne.fr/visionneuse/49190
1 CREA
2 DATE 3 NOV 2025
0 @S2@ SOUR
1 TITL Doublon
1 _URL https://archives.saint-etienne.fr/visionneuse/49190
0 @R1@ REPO
1 NAME AD43
1 WWW https://www.archives43.fr
0 @R2@ REPO
1 NAME Archives
1 _URL archives.example.fr
0 TRLR
";

#[test]
fn add_edit_remove_a_web_link() {
    let mut g = Gedcom::parse(LEGACY).unwrap();
    g.add_web_link("I1", "geneanet.org/fiche/jean", "Fiche Geneanet").unwrap();
    let links = g.web_links("I1");
    assert_eq!(links.len(), 1);
    let WebLinkKind::Media(m) = links[0].kind.clone() else { panic!("multimedia link expected") };
    assert_eq!(links[0].url, "https://geneanet.org/fiche/jean");
    assert_eq!(links[0].label(), "Fiche Geneanet");

    // Written in standard GEDCOM 7: OBJE link to a media whose FILE is the URL.
    let text = g.to_gedcom_string();
    assert!(text.contains(&format!("1 OBJE @{m}@\n")));
    assert!(text.contains(&format!(
        "0 @{m}@ OBJE\n1 FILE https://geneanet.org/fiche/jean\n2 FORM text/html\n2 TITL Fiche Geneanet\n"
    )));
    let media = Media::new(g.get(&m).unwrap());
    assert!(media.is_web() && !media.is_image());
    assert!(g.images_of("I1").is_empty());

    g.update_web_link("I1", &links[0].kind, "https://geneanet.org/fiche/jean2", "").unwrap();
    let l = &g.web_links("I1")[0];
    assert_eq!((l.url.as_str(), l.label()), ("https://geneanet.org/fiche/jean2", "geneanet.org".to_string()));

    // Removing the link deletes the web media left orphaned.
    g.remove_web_link("I1", &l.kind.clone());
    assert!(g.web_links("I1").is_empty());
    assert!(g.get(&m).is_none());

    assert!(g.add_web_link("I1", "pas une adresse", "").is_err());
}

#[test]
fn repositories_use_www() {
    let mut g = Gedcom::parse(LEGACY).unwrap();
    assert_eq!(g.web_links("R1")[0].kind, WebLinkKind::Www(0));
    g.add_web_link("R1", "https://www.archives43.fr/recherche", "").unwrap();
    assert!(
        g.to_gedcom_string().contains("1 WWW https://www.archives43.fr\n1 WWW https://www.archives43.fr/recherche\n")
    );
    g.remove_web_link("R1", &WebLinkKind::Www(0));
    assert_eq!(g.web_links("R1")[0].url, "https://www.archives43.fr/recherche");
}

#[test]
fn migrate_legacy_url_tags() {
    let mut g = Gedcom::parse(LEGACY).unwrap();
    assert_eq!(g.web_links("S1")[0].kind, WebLinkKind::Legacy(0));
    assert_eq!(g.legacy_url_count(), 4);
    assert_eq!(g.migrate_url_tags(), 4);
    assert_eq!(g.legacy_url_count(), 0);

    let text = g.to_gedcom_string();
    assert!(!text.contains("_URL"), "{text}");
    // Same URL on two sources: a single media, linked twice, titled like the first source.
    let s1 = g.web_links("S1");
    let s2 = g.web_links("S2");
    assert_eq!(s1.len(), 1);
    assert_eq!(s1[0].kind, s2[0].kind);
    assert_eq!(s1[0].label(), "Mariage Guyot × Martin");
    // The link takes the place of the `_URL` (before CREA).
    assert!(text.contains("1 TITL Mariage Guyot × Martin\n1 OBJE @M"), "{text}");
    // `_URL` under an event: multimedia link of the event (allowed by GEDCOM 7).
    assert!(text.contains("1 BIRT\n2 DATE 1850\n2 OBJE @M"), "{text}");
    // Repository: no multimedia link allowed, the address becomes a completed `WWW`.
    assert!(text.contains("1 NAME Archives\n1 WWW https://archives.example.fr\n"), "{text}");
    // Read back without loss.
    let again = Gedcom::parse(&text).unwrap();
    assert_eq!(again.to_gedcom_string(), text);
}

#[test]
fn mime_for_urls() {
    assert_eq!(mime_for("https://www.archives43.fr/ark:47539/s0051b032d062eee"), "text/html");
    assert_eq!(mime_for("https://x.fr/photo.jpg?w=200"), "image/jpeg");
    assert_eq!(mime_for("Actes/acte.jpg"), "image/jpeg");
}

#[test]
fn import_wizard_checks() {
    let mut g = Gedcom::parse(LEGACY).unwrap();
    let found = import::analyze(&g);
    assert_eq!(found.len(), 1);
    assert_eq!((found[0].id, found[0].label.as_str()), ("url", "4 _URL links"));
    assert_eq!(import::apply(&mut g, &[]), []);
    assert_eq!(import::apply(&mut g, &["url"]), [("url", 4)]);
    assert!(import::analyze(&g).is_empty());
}

#[test]
fn web_links_have_no_pages() {
    let mut g = Gedcom::parse(LEGACY).unwrap();
    g.add_web_link("I1", "https://geneanet.org/fiche", "").unwrap();
    let WebLinkKind::Media(web) = g.web_links("I1")[0].kind.clone() else { panic!() };
    let photo = g.media_for_file("Actes/acte.jpg");
    assert!(g.merge_media(&[photo.clone(), web.clone()]).is_err());
    assert!(g.add_media_pages(&web, &["Actes/p2.jpg".into()]).is_err());
    assert_eq!(Media::new(g.get(&web).unwrap()).page_count(), 1);
}
