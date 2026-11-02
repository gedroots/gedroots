use gedcom::Gedcom;
use gedcom::gedzip::{self, PackedFile};

const GED: &str = "0 HEAD
1 GEDC
2 VERS 7.0
0 @O1@ OBJE
1 FILE Actes/naissance Jean.jpg
2 FORM image/jpeg
1 FILE ./Actes\\page 2.jpg
2 FORM image/jpeg
0 @O2@ OBJE
1 FILE /Users/moi/Photos/portrait.jpg
2 FORM image/jpeg
0 @O3@ OBJE
1 FILE Actes/naissance Jean.jpg
2 FORM image/jpeg
0 @O4@ OBJE
1 FILE file:///Users/moi/Autres/portrait.jpg
2 FORM image/jpeg
0 @O5@ OBJE
1 FILE https://www.archives43.fr/ark:1/x
2 FORM text/html
0 @O6@ OBJE
1 FILE ../ailleurs/gedcom.ged
2 FORM text/plain
0 @O7@ OBJE
1 FILE C:\\Généalogie\\Recensement 1906 #3.png
2 FORM image/png
0 TRLR
";

fn file_of(g: &Gedcom, x: &str) -> Vec<String> {
    g.get(x).unwrap().children_with("FILE").map(|f| f.val().to_string()).collect()
}

#[test]
fn local_files_get_archive_names() {
    let g = Gedcom::parse(GED).unwrap();
    let p = gedzip::pack(&g);
    let names: Vec<(&str, &str)> = p.files.iter().map(|f| (f.original.as_str(), f.name.as_str())).collect();
    assert_eq!(
        names,
        [
            ("Actes/naissance Jean.jpg", "Actes/naissance Jean.jpg"),
            ("./Actes\\page 2.jpg", "Actes/page 2.jpg"),
            ("/Users/moi/Photos/portrait.jpg", "media/portrait.jpg"),
            ("file:///Users/moi/Autres/portrait.jpg", "media/portrait (2).jpg"),
            ("../ailleurs/gedcom.ged", "media/_gedcom.ged"),
            ("C:\\Généalogie\\Recensement 1906 #3.png", "media/Recensement 1906 #3.png"),
        ]
    );
    // Un fichier cité par deux médias n'est mis qu'une fois.
    assert_eq!(
        p.files[0],
        PackedFile {
            name: "Actes/naissance Jean.jpg".into(),
            original: "Actes/naissance Jean.jpg".into(),
            media: vec!["O1".into(), "O3".into()],
        }
    );
}

#[test]
fn file_values_are_rewritten_as_urls() {
    let g = Gedcom::parse(GED).unwrap();
    let p = gedzip::pack(&g);
    assert_eq!(file_of(&p.ged, "O1"), ["Actes/naissance%20Jean.jpg", "Actes/page%202.jpg"]);
    assert_eq!(file_of(&p.ged, "O2"), ["media/portrait.jpg"]);
    assert_eq!(file_of(&p.ged, "O4"), ["media/portrait%20(2).jpg"]);
    assert_eq!(file_of(&p.ged, "O7"), ["media/Recensement%201906%20%233.png"]);
    // Les liens web ne bougent pas, ni le fichier d'origine.
    assert_eq!(file_of(&p.ged, "O5"), ["https://www.archives43.fr/ark:1/x"]);
    assert_eq!(file_of(&g, "O1"), ["Actes/naissance Jean.jpg", "./Actes\\page 2.jpg"]);
    // La copie se relit.
    let again = Gedcom::parse(&p.ged.to_gedcom_string()).unwrap();
    assert_eq!(file_of(&again, "O2"), ["media/portrait.jpg"]);
}

#[test]
fn local_or_web() {
    assert!(gedzip::is_local("Actes/x.jpg"));
    assert!(gedzip::is_local("C:\\x.jpg"));
    assert!(gedzip::is_local("file:///x.jpg"));
    assert!(!gedzip::is_local("https://x.fr/a.jpg"));
    assert!(!gedzip::is_local("ftp://x.fr/a.jpg"));
    assert!(!gedzip::is_local(""));
}
