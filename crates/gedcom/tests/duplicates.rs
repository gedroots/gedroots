use gedcom::duplicates::{Exclusion, Level};
use gedcom::merge::{RowKind, Side};
use gedcom::*;

/// A witness entered twice (I10, I11), a father and son with the same name (I1, I2), two sisters
/// with close names (I3, I4), a namesake born a century later (I12), and a father entered twice
/// with his wife (I20 and I21, both married to I22).
const FILE: &str = "0 HEAD
1 GEDC
2 VERS 7.0
0 @I1@ INDI
1 NAME Pierre /GUYOT/
1 SEX M
1 FAMS @F1@
0 @I2@ INDI
1 NAME Pierre /GUYOT/
1 SEX M
1 FAMC @F1@
0 @I3@ INDI
1 NAME Marie Louise /ROBERT/
1 SEX F
1 BIRT
2 DATE 1859
1 FAMC @F1@
0 @I4@ INDI
1 NAME Marie Augustine /ROBERT/
1 SEX F
1 BIRT
2 DATE 1861
1 FAMC @F1@
0 @I5@ INDI
1 NAME Léon /BLOCH/
1 SEX M
1 BIRT
2 DATE 9 DEC 1892
2 PLAC , Médéa, Algérie
2 ASSO @I10@
3 ROLE WITN
0 @I6@ INDI
1 NAME Rachel /BLOCH/
1 SEX F
1 BIRT
2 DATE 22 JUL 1893
2 PLAC , Médéa, Algérie
2 ASSO @I11@
3 ROLE WITN
0 @I10@ INDI
1 NAME Abraham /DARMON/
1 SEX M
1 OCCU Propriétaire
2 DATE 9 DEC 1892
0 @I11@ INDI
1 NAME Abraham /DARMON/
1 SEX M
1 OCCU Clerc d'huissier
2 DATE 22 JUL 1893
0 @I12@ INDI
1 NAME Abraham /DARMON/
1 SEX M
1 BIRT
2 DATE 1990
0 @I20@ INDI
1 NAME Jean /BLANC/
1 SEX M
1 BIRT
2 DATE ABT 1830
2 SOUR @S1@
1 FAMS @F2@
1 FAMC @F4@
0 @I21@ INDI
1 NAME Jean /BLANC/
1 SEX M
1 BIRT
2 DATE 12 MAR 1831
2 PLAC Lyon
2 SOUR @S2@
1 FAMS @F3@
0 @I22@ INDI
1 NAME Claudine /SIMAND/
1 SEX F
1 FAMS @F2@
1 FAMS @F3@
0 @I23@ INDI
1 NAME Jean /BLANC/
1 SEX M
1 FAMC @F2@
1 FAMC @F3@
0 @I24@ INDI
1 NAME Marie /BLANC/
1 SEX F
1 FAMC @F3@
0 @I25@ INDI
1 NAME Pierre /BLANC/
1 SEX M
1 FAMS @F4@
0 @F1@ FAM
1 HUSB @I1@
1 CHIL @I2@
1 CHIL @I3@
1 CHIL @I4@
0 @F2@ FAM
1 HUSB @I20@
1 WIFE @I22@
1 CHIL @I23@
1 MARR
2 DATE 1855
0 @F3@ FAM
1 HUSB @I21@
1 WIFE @I22@
1 CHIL @I23@
1 CHIL @I24@
1 MARR
2 DATE 1855
2 PLAC Lyon
0 @F4@ FAM
1 HUSB @I25@
1 CHIL @I20@
0 @S1@ SOUR
1 TITL Naissances
0 @S2@ SOUR
1 TITL Mariages
0 TRLR
";

fn ged() -> Gedcom {
    Gedcom::parse(FILE).unwrap()
}

fn pairs(g: &Gedcom) -> Vec<(String, String)> {
    g.duplicates().into_iter().map(|d| (d.a, d.b)).collect()
}

#[test]
fn finds_a_witness_entered_twice() {
    let g = ged();
    let d = g.duplicates();
    let w = d.iter().find(|d| d.a == "I10" && d.b == "I11").expect("the witness");
    assert_eq!(w.level, Level::Likely);
    let texts: Vec<&str> = w.clues.iter().map(|c| c.text.as_str()).collect();
    assert!(texts.contains(&"Same given names and surname"), "{texts:?}");
    assert!(texts.contains(&"Both seen between 1892 and 1893"), "{texts:?}");
    assert!(texts.contains(&"Both seen in Médéa"), "{texts:?}");
}

#[test]
fn never_suggests_relatives_or_people_who_lived_apart() {
    let g = ged();
    let p = pairs(&g);
    assert!(!p.contains(&("I1".into(), "I2".into())), "father and son");
    assert!(!p.contains(&("I3".into(), "I4".into())), "sisters");
    assert!(!p.iter().any(|(a, b)| a == "I12" || b == "I12"), "born a century later");
    assert!(!p.iter().any(|(a, b)| a == "I23" || b == "I23"), "son of the duplicate father");
    assert_eq!(g.compare_individuals("I1", "I2").unwrap_err(), Exclusion::CloseRelatives);
    assert_eq!(g.compare_individuals("I3", "I4").unwrap_err(), Exclusion::CloseRelatives);
    assert_eq!(g.compare_individuals("I10", "I12").unwrap_err(), Exclusion::LivesApart);
    assert_eq!(g.compare_individuals("I10", "I5").unwrap_err(), Exclusion::SameEvent);
    assert_eq!(g.compare_individuals("I10", "I6").unwrap_err(), Exclusion::OppositeSex);
    assert_eq!(g.compare_individuals("I10", "S1").unwrap_err(), Exclusion::NotIndividuals);
}

#[test]
fn the_same_spouse_makes_a_likely_pair() {
    let g = ged();
    let d = g.compare_individuals("I21", "I20").unwrap();
    assert_eq!((d.a.as_str(), d.b.as_str()), ("I20", "I21"));
    assert_eq!(d.level, Level::Likely);
    assert!(d.clues.iter().any(|c| c.text == "Same spouse"));
}

#[test]
fn not_a_duplicate_is_remembered_in_the_file() {
    let mut g = ged();
    assert!(g.mark_distinct("I11", "I10"));
    assert!(!g.mark_distinct("I10", "I11"), "already marked");
    assert!(g.is_distinct("I10", "I11"));
    assert!(!pairs(&g).contains(&("I10".into(), "I11".into())));
    assert_eq!(g.compare_individuals("I10", "I11").unwrap_err(), Exclusion::MarkedDistinct);
    let text = g.to_gedcom_string();
    assert!(text.contains("1 _GR_DISTINCT @I10@"), "{text}");
    assert!(text.contains("2 TAG _GR_DISTINCT https://"), "{text}");
    assert_eq!(g.distinct_pairs(), vec![("I11".to_string(), "I10".to_string())]);
    assert!(g.unmark_distinct("I10", "I11"));
    assert!(pairs(&g).contains(&("I10".into(), "I11".into())));
}

#[test]
fn merging_a_witness_keeps_both_occupations_and_events() {
    let mut g = ged();
    let plan = g.merge_plan("I10", "I11").unwrap();
    assert!(plan.conflicts.is_empty());
    let occu = plan.rows.iter().find(|r| r.label == "Occupation").unwrap();
    assert_eq!(occu.kind, RowKind::Added);
    let keep = g.merge_individuals(&plan, &[]).unwrap();
    assert!(g.get(&plan.other).is_none());
    let i = g.individual(&keep).unwrap();
    assert_eq!(i.events().iter().filter(|e| e.tag() == "OCCU").count(), 2);
    // Both births point to the kept witness.
    for x in ["I5", "I6"] {
        let n = g.get(x).unwrap();
        assert_eq!(n.path_val(&["BIRT", "ASSO"]), Some(format!("@{keep}@").as_str()));
    }
    assert!(!g.to_gedcom_string().contains(&format!("@{}@", plan.other)));
}

#[test]
fn compatible_births_are_combined_and_families_of_the_same_couple_become_one() {
    let mut g = ged();
    let plan = g.merge_plan("I20", "I21").unwrap();
    let birth = plan.rows.iter().find(|r| r.label == "Birth").unwrap();
    assert_eq!(birth.kind, RowKind::Combined);
    assert!(plan.conflicts.is_empty());
    let keep = g.merge_individuals(&plan, &[]).unwrap();
    let n = g.get(&keep).unwrap();
    let birt = n.child("BIRT").unwrap();
    assert_eq!(birt.path_val(&["DATE"]), Some("12 MAR 1831"), "the more precise date");
    assert_eq!(birt.path_val(&["PLAC"]), Some("Lyon"));
    assert_eq!(birt.children_with("SOUR").count(), 2, "both citations");
    // F2 and F3 had the same spouses: one family, with each child once.
    assert_eq!(n.children_with("FAMS").count(), 1);
    let fam = g.individual(&keep).unwrap().fams()[0].to_string();
    let f = g.family(&fam).unwrap();
    assert_eq!(f.children(), vec!["I23", "I24"]);
    assert_eq!(g.individual("I23").unwrap().famc(), vec![fam.as_str()]);
    assert_eq!(g.individual("I22").unwrap().fams(), vec![fam.as_str()]);
    assert_eq!(g.get(&fam).unwrap().children_with("MARR").count(), 1, "the same marriage");
    // The parents of the other record are kept.
    assert_eq!(g.individual(&keep).unwrap().famc(), vec!["F4"]);
}

#[test]
fn a_conflict_keeps_only_the_value_chosen_by_the_user() {
    let mut g = Gedcom::parse(
        "0 HEAD
1 GEDC
2 VERS 7.0
0 @I1@ INDI
1 NAME Jean /BLANC/
1 BIRT
2 DATE 1830
0 @I2@ INDI
1 NAME Jean Baptiste /BLANC/
1 BIRT
2 DATE 1850
0 TRLR
",
    )
    .unwrap();
    let plan = g.merge_plan("I1", "I2").unwrap();
    let tags: Vec<&str> = plan.conflicts.iter().map(|c| c.tag.as_str()).collect();
    assert_eq!(tags, vec!["NAME", "BIRT"]);
    assert!(g.merge_individuals(&plan, &[]).is_err(), "nothing chosen yet");
    assert_eq!(g.count(RecordKind::Individual), 2);
    let keep = g.merge_individuals(&plan, &[Side::Other, Side::Keep]).unwrap();
    let n = g.get(&keep).unwrap();
    assert_eq!(n.children_with("NAME").map(|c| c.val()).collect::<Vec<_>>(), vec!["Jean Baptiste /BLANC/"]);
    assert_eq!(n.children_with("BIRT").count(), 1);
    assert_eq!(n.path_val(&["BIRT", "DATE"]), Some("1830"));
}

#[test]
fn the_root_person_is_always_kept() {
    let mut g = ged();
    g.set_root("I11");
    assert_eq!(g.merge_survivor("I10", "I11"), ("I11".to_string(), "I10".to_string()));
    let plan = g.merge_plan("I10", "I11").unwrap();
    g.merge_individuals(&plan, &[]).unwrap();
    assert_eq!(g.root().as_deref(), Some("I11"));
}

#[test]
fn the_quality_page_lists_duplicates_and_hides_distinct_pairs() {
    use gedcom::analysis::check::Severity;
    use gedcom::quality::{self, Category};
    let mut g = ged();
    let dup = |g: &Gedcom| -> Vec<_> {
        quality::run(g).into_iter().filter(|i| i.code == "possible_duplicate").collect::<Vec<_>>()
    };
    let issues = dup(&g);
    assert!(issues.iter().all(|i| quality::category(i.code) == Category::Duplicates && !i.detail.is_empty()));
    let blanc = issues.iter().find(|i| i.xref == "I20" || i.related.first().is_some_and(|r| r == "I20")).unwrap();
    assert_eq!(blanc.severity, Severity::Warning);
    assert!(!blanc.ignored);
    assert!(g.mark_distinct("I20", "I21"));
    let blanc = dup(&g).into_iter().find(|i| i.xref == "I20" || i.related.first().is_some_and(|r| r == "I20"));
    assert!(blanc.is_some_and(|i| i.ignored));
}
