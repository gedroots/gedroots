//! Possible duplicate places and merging them (`gedcom::place_merge`).
use gedcom::document::Gedcom;
use gedcom::duplicates::Level;
use gedcom::merge::Side;
use gedcom::place_merge::{Coords, RowKind};
use gedcom::quality;

const FILE: &str = "0 HEAD
1 GEDC
2 VERS 7.0
1 PLAC
2 FORM Locality, Town, Postcode, County, Region, Country
0 @I1@ INDI
1 BIRT
2 PLAC , Oakham, 11000, Rutland, East, England
1 DEAT
2 PLAC , Oakham-on-Hill, 11000, Rutland, East Midlands, England
3 MAP
4 LATI N52.6700
4 LONG W0.7300
3 NOTE Parish register
1 RESI
2 PLAC Mill Lane, Oakham, 11000, Rutland, East, England
1 BURI
2 PLAC , Paris XVII, 75017, Paris, IDF, France
1 CHR
2 PLAC , Paris XVIII, 75017, Paris, IDF, France
1 EDUC
2 PLAC , Ashby, 22000, Leics, East Midlands, England
3 MAP
4 LATI N52.7600
4 LONG W1.4700
1 GRAD
2 PLAC , Ashby-Folville, 22100, Leics, East Midlands, England
3 MAP
4 LATI N52.7000
4 LONG W0.9600
1 OCCU
2 PLAC , Exton, 33000, Rutland, East, England
1 EVEN
2 PLAC , Exton Park, 33000, Rutland, East, England
0 TRLR
";

fn pairs(g: &Gedcom) -> Vec<(String, String, Level)> {
    g.place_duplicates().into_iter().map(|d| (d.a, d.b, d.level)).collect()
}

#[test]
fn former_and_longer_names_are_suggested() {
    let g = Gedcom::parse(FILE).unwrap();
    let p = pairs(&g);
    // Same postcode, a name that starts the other one.
    assert!(p.iter().any(|(a, b, _)| a == ", Oakham, 11000, Rutland, East, England"
        && b == ", Oakham-on-Hill, 11000, Rutland, East Midlands, England"));
    assert!(p.iter().any(|(a, b, l)| a.contains("Exton,") && b.contains("Exton Park") && *l == Level::Possible));
    // Two arrondissements are not the start of one another; two towns 34 km apart with
    // different postcodes are two places; a locality is never compared to its own town.
    assert!(!p.iter().any(|(a, b, _)| a.contains("Paris") || b.contains("Paris")));
    assert!(!p.iter().any(|(a, _, _)| a.contains("Ashby")));
    assert!(!p.iter().any(|(a, b, _)| a.contains("Mill Lane") || b.contains("Mill Lane")));
    assert_eq!(p.len(), 2);
}

#[test]
fn quality_lists_them_without_a_bulk_fix() {
    let g = Gedcom::parse(FILE).unwrap();
    let issues: Vec<_> = quality::run(&g).into_iter().filter(|i| i.code == "possible_duplicate_place").collect();
    assert_eq!(issues.len(), 2);
    assert!(quality::fix_of("possible_duplicate_place").unwrap().batch.is_none());
}

#[test]
fn the_plan_compares_level_by_level() {
    let g = Gedcom::parse(FILE).unwrap();
    let a = ", Oakham, 11000, Rutland, East, England";
    let b = ", Oakham-on-Hill, 11000, Rutland, East Midlands, England";
    let plan = g.place_merge_plan(a, b).unwrap();
    let labels: Vec<&str> = plan.rows.iter().map(|r| r.label.as_str()).collect();
    assert_eq!(labels, ["Locality", "Town", "Postcode", "County", "Region", "Country"]);
    assert_eq!(plan.rows[1].kind, RowKind::Conflict(0));
    assert_eq!(plan.rows[4].kind, RowKind::Conflict(1));
    assert_eq!(plan.rows[2].kind, RowKind::Same);
    assert_eq!(plan.coords, Coords::FromB);
    assert_eq!(plan.conflicts, 2);
    // The locality of the first town follows it.
    assert_eq!(plan.localities.0.len(), 1);
    assert_eq!(plan.preview(&[None, Some(Side::Other)]), ", , 11000, Rutland, East Midlands, England");
}

#[test]
fn merging_rewrites_both_places_and_their_localities() {
    let mut g = Gedcom::parse(FILE).unwrap();
    let a = ", Oakham, 11000, Rutland, East, England";
    let b = ", Oakham-on-Hill, 11000, Rutland, East Midlands, England";
    let plan = g.place_merge_plan(a, b).unwrap();
    assert!(g.merge_places(&plan, &[Side::Keep]).is_none(), "a choice is missing");
    let result = g.merge_places(&plan, &[Side::Keep, Side::Other]).unwrap();
    assert_eq!(result, ", Oakham, 11000, Rutland, East Midlands, England");
    let places = g.places();
    let merged = places.iter().find(|p| p.name == result).unwrap();
    assert_eq!(merged.uses, 2);
    // The coordinates of the only place that had them now apply to both occurrences.
    let text = g.to_gedcom_string();
    assert_eq!(text.matches("4 LATI N52.6700").count(), 2);
    // Other substructures of each PLAC stay where they were.
    assert!(text.contains("3 NOTE Parish register"));
    assert!(places.iter().any(|p| p.name == "Mill Lane, Oakham, 11000, Rutland, East Midlands, England"));
    assert!(!places.iter().any(|p| p.name == a || p.name == b));
    assert!(g.place_duplicates_of(&result).is_empty());
}

#[test]
fn not_a_duplicate_is_kept_in_the_header_and_follows_renames() {
    let mut g = Gedcom::parse(FILE).unwrap();
    let a = ", Exton, 33000, Rutland, East, England";
    let b = ", Exton Park, 33000, Rutland, East, England";
    assert!(g.mark_places_distinct(a, b));
    assert!(!g.mark_places_distinct(a, b));
    assert_eq!(pairs(&g).len(), 1);
    let text = g.to_gedcom_string();
    assert!(text.contains(&format!("1 _GR_DISTINCT {a}\n2 PLAC {b}\n")));
    assert!(text.contains("2 TAG _GR_DISTINCT "));
    // Hidden on the Quality page, and shown again from there.
    let hidden: Vec<_> =
        quality::run(&g).into_iter().filter(|i| i.code == "possible_duplicate_place" && i.ignored).collect();
    assert_eq!(hidden.len(), 1);
    // Renaming a place keeps the mark on its new name.
    let renamed = ", Exton Village, 33000, Rutland, East, England";
    g.update_place(a, renamed, "", "");
    assert_eq!(g.distinct_places(), vec![(renamed.to_string(), b.to_string())]);
    assert!(g.unmark_places_distinct(b, renamed));
    assert!(g.distinct_places().is_empty());
    assert!(!g.to_gedcom_string().contains("1 _GR_DISTINCT"));
}

#[test]
fn coordinates_far_apart_are_a_choice() {
    let text = "0 HEAD\n1 GEDC\n2 VERS 7.0\n0 @I1@ INDI\n1 BIRT\n2 PLAC Exton, Rutland\n3 MAP\n4 LATI N52.69\n4 LONG W0.63\n1 DEAT\n2 PLAC Exton Park, Rutland\n3 MAP\n4 LATI N52.70\n4 LONG W0.60\n0 TRLR\n";
    let mut g = Gedcom::parse(text).unwrap();
    let plan = g.place_merge_plan("Exton, Rutland", "Exton Park, Rutland").unwrap();
    assert!(matches!(plan.coords, Coords::Conflict(1, d) if d > 1.0 && d < 3.0));
    // Without a place format, levels are numbered.
    assert_eq!(plan.rows[0].label, "Level 1");
    let r = g.merge_places(&plan, &[Side::Other, Side::Keep]).unwrap();
    assert_eq!(r, "Exton Park, Rutland");
    assert_eq!(g.places()[0].latitude.as_deref(), Some("N52.69"));
}

fn agent_change(g: &Gedcom, a: &str, action: gedcom::change::Action) -> gedcom::change::QualityChange {
    let issues = quality::run(g);
    let ids = quality::issue_ids(&issues);
    let n = issues
        .iter()
        .position(|i| i.code == "possible_duplicate_place" && i.related.first().map(String::as_str) == Some(a))
        .expect("the pair is suggested");
    gedcom::change::QualityChange {
        issue: Some(issues[n].clone()),
        issue_id: Some(ids[n].clone()),
        action,
        reason: "The parish took the name of the hill in 1850".into(),
        evidence: Default::default(),
    }
}

#[test]
fn an_agent_merge_of_places_needs_a_choice_for_each_level_that_differs() {
    use gedcom::change::Action;
    let mut g = Gedcom::parse(FILE).unwrap();
    let a = ", Oakham, 11000, Rutland, East, England";
    let b = ", Oakham-on-Hill, 11000, Rutland, East Midlands, England";
    let rerun = |g: &Gedcom| quality::run(g);
    let merge = |choices: &[(&str, &str)]| {
        Action::Merge(choices.iter().map(|(f, w)| (f.to_uppercase(), w.to_string())).collect())
    };
    // The agent sees what it has to choose.
    let issues = quality::run(&g);
    let report = g.quality_report(&issues, &Default::default(), 0, 100).unwrap();
    let info = report.issues.iter().find_map(|i| i.merge.clone().filter(|m| m.keep == a)).unwrap();
    assert_eq!(info.conflicts.iter().map(|c| c.fact.as_str()).collect::<Vec<_>>(), ["Town", "Region"]);
    assert!(report.issues.iter().any(|i| i.code == "possible_duplicate_place" && i.can_hide && !i.automatic_fix));

    let err = agent_change(&g, a, merge(&[("Town", a)])).check(&g, &rerun).unwrap_err();
    assert!(err.contains("Region"), "{err}");
    let err = agent_change(&g, a, merge(&[("Town", a), ("Region", a), ("Country", a)])).check(&g, &rerun).unwrap_err();
    assert!(err.contains("does not differ"), "{err}");
    assert!(agent_change(&g, a, merge(&[("Town", a), ("Region", "Elsewhere")])).check(&g, &rerun).is_err());
    assert!(agent_change(&g, a, Action::Fix).check(&g, &rerun).is_err());

    // The agent's keys come upper-cased: “RÉGION” is “Région”.
    let ok = agent_change(&g, a, merge(&[("town", a), ("REGION", b)]));
    ok.check(&g, &rerun).unwrap();
    let rows = ok.rows(&g);
    assert!(rows.iter().any(|r| r.label == "Region" && r.proposed == "East Midlands"));
    ok.apply(&mut g).unwrap();
    assert!(g.places().iter().any(|p| p.name == ", Oakham, 11000, Rutland, East Midlands, England" && p.uses == 2));
}

#[test]
fn an_agent_can_say_two_places_are_different() {
    use gedcom::change::Action;
    let mut g = Gedcom::parse(FILE).unwrap();
    let a = ", Exton, 33000, Rutland, East, England";
    let hide = agent_change(&g, a, Action::Hide);
    assert!(hide.title(&g).ends_with("Exton (Rutland)"));
    hide.check(&g, &|g: &Gedcom| quality::run(g)).unwrap();
    hide.apply(&mut g).unwrap();
    assert_eq!(g.distinct_places().len(), 1);
}

#[test]
fn level_keys_ignore_case_with_accents() {
    let c = gedcom::place_merge::PlaceConflict { fact: "Région".into(), a: "a".into(), b: "b".into() };
    assert!(c.is("RÉGION") && c.is(" région ") && !c.is("Region"));
}
