use gedcom::analysis::{check, kinship, stats, timeline};
use gedcom::*;

const SAMPLE: &str = include_str!("fixtures/example.ged");

/// Test tree: two brothers, their children (first cousins), a half-sister.
const FAMILY: &str = "0 HEAD
1 GEDC
2 VERS 7.0
0 @GP@ INDI
1 SEX M
1 BIRT
2 DATE 1850
1 FAMS @F1@
1 FAMS @F4@
0 @GM@ INDI
1 SEX F
1 FAMS @F1@
0 @A@ INDI
1 SEX M
1 FAMC @F1@
1 FAMS @F2@
0 @B@ INDI
1 SEX M
1 FAMC @F1@
1 FAMS @F3@
0 @A1@ INDI
1 NAME Louis /DURAND/
1 SEX M
1 BIRT
2 DATE 10 JAN 1920
1 FAMC @F2@
0 @B1@ INDI
1 SEX F
1 FAMC @F3@
0 @H@ INDI
1 SEX F
1 FAMC @F4@
0 @AW@ INDI
1 SEX F
1 FAMS @F2@
0 @F1@ FAM
1 HUSB @GP@
1 WIFE @GM@
1 CHIL @A@
1 CHIL @B@
0 @F2@ FAM
1 HUSB @A@
1 WIFE @AW@
1 CHIL @A1@
0 @F3@ FAM
1 HUSB @B@
1 CHIL @B1@
0 @F4@ FAM
1 HUSB @GP@
1 CHIL @H@
0 TRLR
";

/// These tests check the texts the app writes in French, the language of the sample files.
fn french() {
    rust_i18n::set_locale("fr");
}

#[test]
fn consistency_check_on_sample() {
    french();
    let g = Gedcom::parse(SAMPLE).unwrap();
    let issues = check::check(&g, &check::Thresholds::default());
    // Émile (born in 1796) would have been 115 at Paul's birth (1912).
    let old = issues.iter().find(|i| i.code == "father_too_old").expect("father too old");
    assert_eq!(old.xref, "I00001");
    assert_eq!(old.related, vec!["I00002"]);
    assert!(!issues.iter().any(|i| i.code == "broken_link"));
}

#[test]
fn ignored_issue_disappears_and_extension_is_declared() {
    french();
    let mut g = Gedcom::parse(SAMPLE).unwrap();
    assert!(g.ignore_issue("I00001", "father_too_old"));
    assert!(!g.ignore_issue("I00001", "father_too_old"));
    let issues = check::check(&g, &check::Thresholds::default());
    assert!(!issues.iter().any(|i| i.code == "father_too_old"));
    let out = g.to_gedcom_string();
    assert!(out.contains("1 _GR_IGNORE father_too_old"));
    assert!(out.contains("2 TAG _GR_IGNORE https://"));
    // The file reads back and stays stable.
    let again = Gedcom::parse(&out).unwrap();
    assert_eq!(again.to_gedcom_string(), out);
}

#[test]
fn detects_impossible_dates() {
    french();
    let ged = "0 HEAD\n1 GEDC\n2 VERS 7.0\n0 @I1@ INDI\n1 BIRT\n2 DATE 1900\n1 DEAT\n2 DATE 1890\n1 FAMC @X@\n0 TRLR\n";
    let g = Gedcom::parse(ged).unwrap();
    let codes: Vec<&str> = check::check(&g, &Default::default()).iter().map(|i| i.code).collect();
    assert!(codes.contains(&"death_before_birth"), "{codes:?}");
    assert!(codes.contains(&"broken_link"), "{codes:?}");
}

#[test]
fn relationships() {
    french();
    let g = Gedcom::parse(FAMILY).unwrap();
    let rel = |a: &str, b: &str| kinship::relationship(&g, a, b).map(|k| k.label);
    assert_eq!(rel("A1", "B1").as_deref(), Some("cousine germaine"));
    assert_eq!(rel("A1", "B").as_deref(), Some("oncle"));
    assert_eq!(rel("B1", "A1").as_deref(), Some("cousin germain"));
    assert_eq!(rel("A1", "GP").as_deref(), Some("grand-père"));
    assert_eq!(rel("A", "H").as_deref(), Some("demi-sœur"));
    assert_eq!(rel("A", "B").as_deref(), Some("frère"));
    assert_eq!(rel("A", "AW").as_deref(), Some("épouse"));
    assert_eq!(rel("B", "AW").as_deref(), Some("belle-sœur"));
    assert_eq!(rel("AW", "B").as_deref(), Some("beau-frère"));
    assert_eq!(rel("AW", "GP").as_deref(), Some("beau-père"));
    assert_eq!(rel("GP", "AW").as_deref(), Some("belle-fille"));
    let k = kinship::relationship(&g, "A1", "B1").unwrap();
    assert_eq!(k.degree, Some(4));
    assert_eq!(k.path.len(), 5);
    assert!(k.common.contains(&k.path[2]));
    assert_eq!(k.common.len(), 2);
}

#[test]
fn timeline_with_ages() {
    french();
    let g = Gedcom::parse(FAMILY).unwrap();
    let t = timeline::timeline(&g, "GP");
    let birth = t.iter().find(|e| e.label.contains("petit-fils")).expect("grandson’s birth");
    assert_eq!(birth.age.as_deref(), Some("~69 ans"));
    let g = Gedcom::parse(SAMPLE).unwrap();
    let t = timeline::timeline(&g, "I00003");
    assert!(t.iter().any(|e| e.label.starts_with("Mariage avec Émile")));
    assert!(t.iter().any(|e| e.label.starts_with("Naissance de son fils Paul")));
}

#[test]
fn statistics() {
    french();
    let g = Gedcom::parse(FAMILY).unwrap();
    let s = stats::stats(&g, Some("A1"));
    assert_eq!(s.individuals, 8);
    assert_eq!(s.generations.len(), 3);
    assert_eq!(s.generations[1].found, 2);
    assert_eq!(s.generations[2].found, 2);
    assert_eq!(s.implexe, 0.);
    // B1 and A1 descend from the same couple; a marriage between their children would create
    // pedigree collapse.
    let g = Gedcom::parse(SAMPLE).unwrap();
    let s = stats::stats(&g, Some("I00001"));
    assert_eq!(s.surnames[0], ("MARCHAND".to_string(), 2));
    assert_eq!(s.mean_children, Some(1.));
}
