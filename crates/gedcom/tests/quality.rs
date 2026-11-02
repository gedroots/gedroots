use gedcom::quality::{self, Category};
use gedcom::*;

const FILE: &str = "0 HEAD
1 GEDC
2 VERS 7.0
0 @I1@ INDI
1 NAME Jean /MARTIN/
1 SEX M
1 SEX M
1 BIRT
2 DATE 31 FEB 1900
1 DEAT
1 RESI
2 PLAC Lyon
2 NOTE
1 FOO bar
1 FAMS @F1@
1 FAMC @F2@
1 ASSO @I3@
2 ROLE WITN
0 @I2@ INDI
1 NAME Marie /DURAND/
1 SEX X
1 FAMS @F1@
0 @I3@ INDI
1 NAME Paul /TÉMOIN/
0 @I4@ INDI
1 NAME Seul /ISOLÉ/
1 SOUR @I2@
0 @F1@ FAM
1 HUSB @I1@
0 @F2@ FAM
0 @F3@ FAM
0 @N1@ SNOTE
0 @N2@ SNOTE Texte jamais cité
0 @R1@ REPO
1 NAME Archives
0 TRLR
";

fn codes(issues: &[gedcom::analysis::check::Issue]) -> Vec<&'static str> {
    issues.iter().filter(|i| !i.ignored).map(|i| i.code).collect()
}

#[test]
fn detects_structure_empty_and_unused() {
    let g = Gedcom::parse(FILE).unwrap();
    let issues = quality::run(&g);
    let c = codes(&issues);
    for code in [
        "too_many",
        "invalid_value",
        "tag_not_allowed",
        "wrong_pointer",
        "empty_fact",
        "empty_note",
        "empty_family",
        "unused_record",
        "missing_backlink",
        "disconnected",
    ] {
        assert!(c.contains(&code), "{code} absent de {c:?}");
    }
    // Le témoin (ASSO) est relié à l'individu racine, pas l'individu isolé.
    let disc: Vec<&str> = issues.iter().filter(|i| i.code == "disconnected").map(|i| i.xref.as_str()).collect();
    assert_eq!(disc, ["I4"]);
    // La famille F2 est vide (F1 a un membre) ; les deux sont signalées une fois chacune au plus.
    let empty: Vec<&str> = issues.iter().filter(|i| i.code == "empty_family").map(|i| i.xref.as_str()).collect();
    assert_eq!(empty, ["F2", "F3"]);
    // Le dépôt sans citation est inutilisé, la note vide n'est signalée qu'une fois.
    assert_eq!(issues.iter().filter(|i| i.xref == "N1").count(), 1);
    assert!(issues.iter().any(|i| i.code == "unused_record" && i.xref == "N2"));
    // Catégories.
    assert_eq!(quality::category("empty_note"), Category::Empty);
    assert_eq!(quality::category("too_many"), Category::Structure);
}

#[test]
fn batch_fixes_keep_other_structures() {
    let mut g = Gedcom::parse(FILE).unwrap();
    let issues = quality::run(&g);
    let fixable: Vec<_> = issues
        .iter()
        .filter(|i| matches!(i.code, "empty_fact" | "empty_note" | "empty_family" | "missing_backlink"))
        .cloned()
        .collect();
    let n = g.fix_issues(&fixable);
    assert_eq!(n, fixable.len());
    let i1 = g.get("I1").unwrap();
    assert!(i1.child("DEAT").is_none(), "événement vide retiré");
    assert!(i1.child("RESI").is_some_and(|r| r.child("NOTE").is_none()), "note vide retirée, résidence gardée");
    assert!(i1.child("BIRT").is_some());
    assert!(g.get("F2").is_none() && g.get("F3").is_none() && g.get("N1").is_none());
    // I1 est cité dans F2 (FAMC) : la famille a été supprimée, donc le lien aussi.
    assert!(g.get("I1").unwrap().child("FAMC").is_none());
    // I2 se dit membre de F1 sans y figurer : rétabli en WIFE.
    assert!(g.get("F1").unwrap().children_with("WIFE").any(|n| n.pointer() == Some("I2")));
    let after = codes(&quality::run(&g));
    for code in ["empty_fact", "empty_note", "empty_family", "missing_backlink"] {
        assert!(!after.contains(&code), "{code} encore présent : {after:?}");
    }
}

#[test]
fn ignore_and_unignore() {
    let mut g = Gedcom::parse(FILE).unwrap();
    assert!(g.ignore_issue("I4", "disconnected"));
    let i = quality::run(&g).into_iter().find(|i| i.code == "disconnected").unwrap();
    assert!(i.ignored);
    assert!(g.unignore_issue("I4", "disconnected"));
    assert!(!quality::run(&g).into_iter().find(|i| i.code == "disconnected").unwrap().ignored);
}

#[test]
fn broken_link_removed() {
    let mut g =
        Gedcom::parse("0 HEAD\n1 GEDC\n2 VERS 7.0\n0 @I1@ INDI\n1 FAMC @F9@\n1 BIRT\n2 SOUR @S9@\n0 TRLR\n").unwrap();
    let issues: Vec<_> = quality::run(&g).into_iter().filter(|i| i.code == "broken_link").collect();
    assert_eq!(issues.len(), 2);
    assert_eq!(g.fix_issues(&issues), 2);
    let i1 = g.get("I1").unwrap();
    assert!(i1.child("FAMC").is_none() && i1.child("BIRT").unwrap().child("SOUR").is_none());
}

#[test]
fn gedcom_551_skips_schema() {
    let g = Gedcom::parse("0 HEAD\n1 GEDC\n2 VERS 5.5.1\n0 @I1@ INDI\n1 FOO x\n0 TRLR\n").unwrap();
    assert!(!codes(&quality::run(&g)).contains(&"tag_not_allowed"));
}

#[test]
fn citation_checks_have_categories() {
    assert_eq!(quality::category("role_sex_mismatch"), Category::Family);
    assert_eq!(quality::category("outside_source_period"), Category::Dates);
}

const MEDIA: &str = "0 HEAD
1 GEDC
2 VERS 7.0
0 @I1@ INDI
1 NAME Jean /MARTIN/
1 OBJE @M1@
1 OBJE @M2@
1 OBJE @M3@
0 @M1@ OBJE
1 FILE Actes/naissance.jpg
2 FORM image/jpeg
0 @M2@ OBJE
1 FILE Actes/naissance.jpg
2 FORM image/jpeg
1 NOTE Copie
0 @M3@ OBJE
1 FILE Vieux/deces.jpg
0 @M4@ OBJE
1 FILE Perdu.pdf
2 FORM application/pdf
0 @M5@ OBJE
1 FILE https://example.org/acte
2 FORM text/html
0 TRLR
";

fn media_issues(g: &Gedcom) -> Vec<gedcom::analysis::check::Issue> {
    let folder =
        vec!["Actes/naissance.jpg".to_string(), "Actes/deces.jpg".to_string(), "Portraits/jean.png".to_string()];
    let exists = |f: &str| folder.iter().any(|x| x == f);
    quality::run_with(g, Some(&quality::Files { exists: &exists, folder: &folder }))
}

#[test]
fn media_checks() {
    let g = Gedcom::parse(MEDIA).unwrap();
    let issues = media_issues(&g);
    let codes = |code: &str| issues.iter().filter(|i| i.code == code).map(|i| i.xref.as_str()).collect::<Vec<_>>();
    assert_eq!(codes("duplicate_media"), ["M2"]);
    assert_eq!(codes("moved_file"), ["M3"]);
    assert_eq!(codes("missing_file"), ["M4"]);
    assert_eq!(codes("missing_form"), ["M3"]);
    assert!(issues.iter().all(|i| i.code != "missing_required"));
    let files: Vec<&str> =
        issues.iter().filter(|i| i.code == "unattached_file").map(|i| i.related[0].as_str()).collect();
    assert_eq!(files, ["Portraits/jean.png"]);
    assert!(
        issues.iter().filter(|i| i.code == "unattached_file").all(|i| quality::category(i.code) == Category::Files)
    );
    // Sans accès aux fichiers : seulement ce qui se voit dans le .ged.
    let plain = quality::run(&g);
    assert!(plain.iter().all(|i| !matches!(i.code, "moved_file" | "missing_file" | "unattached_file")));
}

#[test]
fn media_fixes() {
    let mut g = Gedcom::parse(MEDIA).unwrap();
    let issues = media_issues(&g);
    let fixable: Vec<_> = issues
        .iter()
        .filter(|i| quality::category(i.code) == Category::Media || i.code == "unattached_file")
        .cloned()
        .collect();
    assert_eq!(g.fix_issues(&fixable), 4);
    // Doublon fusionné : un seul lien, la note conservée.
    assert!(g.get("M2").is_none());
    let indi = g.get("I1").unwrap();
    assert_eq!(indi.children_with("OBJE").filter(|o| o.val() == "@M1@").count(), 1);
    assert!(g.get("M1").unwrap().child("NOTE").is_some());
    // Chemin corrigé, format déduit.
    let f = g.get("M3").unwrap().child("FILE").unwrap();
    assert_eq!(f.val(), "Actes/deces.jpg");
    assert_eq!(f.child_val("FORM"), Some("image/jpeg"));
    // Médias créés pour les fichiers du dossier.
    assert!(g.media_by_file("Portraits/jean.png").is_some());
    assert!(media_issues(&g).iter().all(|i| i.code == "missing_file" || quality::category(i.code) != Category::Media));
}
