use gedcom::*;

/// A record in two one-page media, as in an Ancestris export: the source points to both, a person
/// is placed on page 2.
const ACTE: &str = "0 HEAD
1 GEDC
2 VERS 7.0
0 @I1@ INDI
1 NAME Michel /OHL/
1 OBJE @M2@
2 CROP
3 TOP 10
3 LEFT 20
3 HEIGHT 30
3 WIDTH 40
1 BIRT
2 SOUR @S1@
3 PAGE Acte 52
3 OBJE @M1@
3 OBJE @M2@
0 @I2@ INDI
1 NAME Jeanne /BOUST/
1 OBJE @M1@
2 CROP
3 TOP 1
3 LEFT 2
3 HEIGHT 3
3 WIDTH 4
0 @S1@ SOUR
1 TITL Acte de mariage
1 OBJE @M1@
1 OBJE @M2@
0 @M1@ OBJE
1 FILE Actes/acte_1.jpg
2 FORM image/jpeg
2 TITL Acte de mariage de Michel OHL (1/2)
0 @M2@ OBJE
1 FILE Actes/acte_2.jpg
2 FORM image/jpeg
2 TITL Acte de mariage de Michel OHL (2/2)
1 NOTE Page abîmée
0 TRLR
";

fn acte() -> Gedcom {
    Gedcom::parse(ACTE).unwrap()
}

fn links(g: &Gedcom, rec: &str) -> Vec<String> {
    let mut out = Vec::new();
    g.get(rec).unwrap().walk(&mut |n| {
        if n.tag == "OBJE" && n.pointer().is_some() {
            out.push(format!("{}@{}", n.val(), page_of_link(n)));
        }
    });
    out
}

#[test]
fn merge_two_single_page_media() {
    let mut g = acte();
    let m = g.merge_media(&["M1".into(), "M2".into()]).unwrap();
    assert_eq!(m, "M1");
    assert!(g.get("M2").is_none());

    let media = Media::new(g.get("M1").unwrap());
    let files = media.files();
    assert_eq!(files.iter().map(|f| f.path).collect::<Vec<_>>(), ["Actes/acte_1.jpg", "Actes/acte_2.jpg"]);
    // Common title on the first page; “(2/2)” disappears.
    assert_eq!(media.title(), "Acte de mariage de Michel OHL");
    assert_eq!(files[1].title, None);
    // The note of the merged media is kept.
    assert_eq!(g.get("M1").unwrap().child_val("NOTE"), Some("Page abîmée"));

    // The source and the citation now point to the media only once.
    assert_eq!(links(&g, "S1"), ["@M1@@1"]);
    assert_eq!(links(&g, "I1"), ["@M1@@2", "@M1@@1"]);
    // Michel is placed on page 2, Jeanne stays on page 1.
    let michel = g.media_links("M1").into_iter().find(|l| l.owner == "I1").unwrap();
    assert_eq!((michel.page, michel.crop.map(|c| c.left)), (2, Some(20)));
    assert_eq!(g.link_page("I2", "M1"), 1);

    let text = g.to_gedcom_string();
    assert!(text.contains("1 OBJE @M1@\n2 CROP\n3 TOP 10\n3 LEFT 20\n3 HEIGHT 30\n3 WIDTH 40\n2 _GR_FILE 2\n"));
    assert!(text.contains("2 TAG _GR_FILE https://"));
    assert!(text.contains("0 @M1@ OBJE\n1 FILE Actes/acte_1.jpg\n2 FORM image/jpeg\n2 TITL Acte de mariage de Michel OHL\n1 FILE Actes/acte_2.jpg\n2 FORM image/jpeg\n"));
}

#[test]
fn split_restores_the_original_media() {
    let mut g = acte();
    g.merge_media(&["M1".into(), "M2".into()]).unwrap();
    let new = g.split_media_page("M1", 2).unwrap();

    let a = Media::new(g.get("M1").unwrap());
    let b = Media::new(g.get(&new).unwrap());
    assert_eq!(a.page_count(), 1);
    assert_eq!(b.files()[0].path, "Actes/acte_2.jpg");
    assert_eq!(b.title(), "Acte de mariage de Michel OHL (2/2)");
    // Michel follows his page, without a page number; the source points to both media.
    assert_eq!(g.link_page("I1", &new), 1);
    assert!(g.link_crop("I1", &new).is_some());
    assert_eq!(links(&g, "S1"), ["@M1@@1".to_string(), format!("@{new}@@1")]);
    assert!(!g.to_gedcom_string().contains("1 _GR_FILE") && !g.to_gedcom_string().contains("2 _GR_FILE"));
}

#[test]
fn reorder_and_remove_pages_follow_links() {
    let mut g = acte();
    g.merge_media(&["M1".into(), "M2".into()]).unwrap();
    g.add_media_pages("M1", &["Actes/acte_3.jpg".into(), "Actes/acte_1.jpg".into()]).unwrap();
    assert_eq!(Media::new(g.get("M1").unwrap()).page_count(), 3);
    assert_eq!(Media::new(g.get("M1").unwrap()).files()[2].form, Some("image/jpeg"));

    // Page 2 first: Michel moves to page 1, Jeanne to page 2; the title follows the first page.
    g.move_media_page("M1", 2, 1).unwrap();
    let m = Media::new(g.get("M1").unwrap());
    assert_eq!(m.page_file(1), Some("Actes/acte_2.jpg"));
    assert_eq!(m.title(), "Acte de mariage de Michel OHL");
    assert_eq!(m.files()[1].title, None);
    assert_eq!(g.link_page("I1", "M1"), 1);
    assert_eq!(g.link_page("I2", "M1"), 2);

    // Removing Michel's page: he stays linked to the document, without an area.
    g.remove_media_page("M1", 1).unwrap();
    assert_eq!(g.link_page("I2", "M1"), 1);
    assert_eq!(g.link_crop("I1", "M1"), None);
    assert!(g.remove_media_page("M1", 1).is_ok());
    assert!(g.remove_media_page("M1", 1).is_err(), "un média garde au moins une page");
}

#[test]
fn tag_person_on_a_given_page() {
    let mut g = acte();
    g.merge_media(&["M1".into(), "M2".into()]).unwrap();
    let crop = Crop::from_rect((5, 6, 7, 8));
    // Jeanne is also on page 2: two links, each with its page.
    g.tag_person_on("I2", "M1", 2, crop).unwrap();
    let pages: Vec<usize> = g.media_links("M1").iter().filter(|l| l.owner == "I2").map(|l| l.page).collect();
    assert_eq!(pages, [1, 2]);
    g.untag_person_on("I2", "M1", 1);
    let left: Vec<_> = g.media_links("M1").into_iter().filter(|l| l.owner == "I2").collect();
    assert_eq!((left.len(), left[0].page, left[0].crop), (1, 2, Some(crop)));
    // Without an area, the link becomes a link to the whole document again.
    g.set_link_crop_on("I2", "M1", 2, None).unwrap();
    assert_eq!(links(&g, "I2"), ["@M1@@1"]);
}

#[test]
fn merge_needs_two_media() {
    let mut g = acte();
    assert!(g.merge_media(&["M1".into()]).is_err());
    assert!(g.merge_media(&["M1".into(), "S1".into()]).is_err());
    assert!(g.split_media_page("M1", 1).is_err());
}
