//! Relationship calculator (“second cousin”, “great-uncle”…), like Heredis, Gramps or
//! MacFamilyTree.

use super::check::parents_of;
use crate::document::Gedcom;
use std::collections::{HashMap, VecDeque};

/// Result of the calculation: “B is the … of A”.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Kinship {
    /// Label of the relationship, in B's gender (e.g. “cousine issue de germain”).
    pub label: String,
    /// Civil degree (number of generations in the path), for a blood relationship.
    pub degree: Option<u32>,
    /// Path from A to B through the common ancestor (or the spouse).
    pub path: Vec<String>,
    /// Closest common ancestors.
    pub common: Vec<String>,
}

/// Ancestors of `x` with their distance and the next link towards `x`.
fn ancestors(g: &Gedcom, x: &str) -> HashMap<String, (u32, Option<String>)> {
    let mut out = HashMap::new();
    out.insert(x.to_string(), (0, None));
    let mut q = VecDeque::from([x.to_string()]);
    while let Some(c) = q.pop_front() {
        let d = out[&c].0;
        for p in parents_of(g, &c) {
            if !out.contains_key(p) {
                out.insert(p.to_string(), (d + 1, Some(c.clone())));
                q.push_back(p.to_string());
            }
        }
    }
    out
}

fn chain(map: &HashMap<String, (u32, Option<String>)>, from: &str) -> Vec<String> {
    let mut v = vec![from.to_string()];
    let mut cur = from.to_string();
    while let Some((_, Some(next))) = map.get(&cur) {
        v.push(next.clone());
        cur = next.clone();
    }
    v
}

/// Family where `parent` is one of the spouses and `child` a child.
fn family_of(g: &Gedcom, child: &str, parent: &str) -> Option<String> {
    let c = g.get(child)?;
    c.children_with("FAMC").filter_map(|n| n.pointer()).find_map(|f| {
        let fam = g.get(f)?;
        fam.children
            .iter()
            .any(|n| (n.tag == "HUSB" || n.tag == "WIFE") && n.pointer() == Some(parent))
            .then(|| f.to_string())
    })
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Sex {
    M,
    F,
    U,
}

fn sex_of(g: &Gedcom, x: &str) -> Sex {
    match g.individual(x).map(|i| i.sex().to_string()).as_deref() {
        Some("M") => Sex::M,
        Some("F") => Sex::F,
        _ => Sex::U,
    }
}

/// Word in the person's gender, read from the catalogue (`kin.father`, `kin.mother`, `kin.parent`).
macro_rules! word {
    ($lang:expr, $s:expr, $m:literal, $f:literal, $u:literal) => {
        match $s {
            Sex::M => rust_i18n::t!($m, locale = $lang),
            Sex::F => rust_i18n::t!($f, locale = $lang),
            Sex::U => rust_i18n::t!($u, locale = $lang),
        }
        .into_owned()
    };
}

/// “arrière-” / “great-”, repeated.
fn great(lang: &str, n: u32) -> String {
    rust_i18n::t!("kin.great", locale = lang).repeat(n as usize)
}

/// “demi-” / “half-”.
fn half_prefix(lang: &str, half: bool) -> String {
    if half { rust_i18n::t!("kin.half", locale = lang).into_owned() } else { String::new() }
}

/// Ancestor `n` generations up: father, grandmother, great-grandfather…
fn ancestor_name(lang: &str, n: u32, s: Sex) -> String {
    match n {
        1 => word!(lang, s, "kin.father", "kin.mother", "kin.parent"),
        2..=5 => great(lang, n - 2) + &word!(lang, s, "kin.grandfather", "kin.grandmother", "kin.grandparent"),
        _ => rust_i18n::t!("kin.ancestor_n", n = n, locale = lang).into_owned(),
    }
}

fn descendant_name(lang: &str, n: u32, s: Sex) -> String {
    match n {
        1 => word!(lang, s, "kin.son", "kin.daughter", "kin.child"),
        2..=5 => great(lang, n - 2) + &word!(lang, s, "kin.grandson", "kin.granddaughter", "kin.grandchild"),
        _ => word!(lang, s, "kin.descendant_n.m", "kin.descendant_n.f", "kin.descendant_n.m")
            .replace("%{n}", &n.to_string()),
    }
}

/// Cousin of the same generation (`n` generations up to the common ancestor), in French.
fn cousin_fr(lang: &str, n: u32, s: Sex) -> String {
    match n {
        2 => word!(lang, s, "kin.fr.first_cousin.m", "kin.fr.first_cousin.f", "kin.fr.first_cousin.u"),
        3 => word!(lang, s, "kin.fr.second_cousin.m", "kin.fr.second_cousin.f", "kin.fr.second_cousin.u"),
        _ => {
            word!(lang, s, "kin.cousin", "kin.cousin_f", "kin.cousin_u")
                + &rust_i18n::t!("kin.fr.canonical_degree", n = n, locale = lang)
        }
    }
}

/// Cousins, in English: “second cousin twice removed”.
fn cousin_en(lang: &str, a: u32, b: u32, s: Sex, half: bool) -> String {
    let degree = a.min(b) - 1;
    let ordinal = match degree {
        1 => rust_i18n::t!("kin.en.first", locale = lang),
        2 => rust_i18n::t!("kin.en.second", locale = lang),
        3 => rust_i18n::t!("kin.en.third", locale = lang),
        4 => rust_i18n::t!("kin.en.fourth", locale = lang),
        5 => rust_i18n::t!("kin.en.fifth", locale = lang),
        n => rust_i18n::t!("kin.en.nth", n = n, locale = lang),
    };
    let mut out = format!(
        "{}{ordinal} {}",
        half_prefix(lang, half),
        word!(lang, s, "kin.cousin", "kin.cousin_f", "kin.cousin_u")
    );
    match a.abs_diff(b) {
        0 => {}
        1 => out += &rust_i18n::t!("kin.en.once_removed", locale = lang),
        2 => out += &rust_i18n::t!("kin.en.twice_removed", locale = lang),
        k => out += &rust_i18n::t!("kin.en.times_removed", n = k, locale = lang),
    }
    out
}

/// Name of the blood relationship, `a` = generations from A to the common ancestor, `b` = from B.
fn blood_label(a: u32, b: u32, s: Sex, half: bool) -> String {
    blood_label_in(&crate::i18n::language(), a, b, s, half)
}

fn blood_label_in(lang: &str, a: u32, b: u32, s: Sex, half: bool) -> String {
    let demi = half_prefix(lang, half);
    match (a, b) {
        (0, 0) => rust_i18n::t!("kin.same_person", locale = lang).into_owned(),
        (0, b) => descendant_name(lang, b, s),
        (a, 0) => ancestor_name(lang, a, s),
        (1, 1) => demi + &word!(lang, s, "kin.brother", "kin.sister", "kin.sibling"),
        (2, 1) => demi + &word!(lang, s, "kin.uncle", "kin.aunt", "kin.uncle_or_aunt"),
        (a, 1) => {
            demi + &great(lang, a - 3)
                + &word!(lang, s, "kin.granduncle", "kin.grandaunt", "kin.granduncle_or_grandaunt")
        }
        (1, 2) => demi + &word!(lang, s, "kin.nephew", "kin.niece", "kin.nephew_or_niece"),
        (1, b) => {
            demi + &great(lang, b - 3)
                + &word!(lang, s, "kin.grandnephew", "kin.grandniece", "kin.grandnephew_or_grandniece")
        }
        (a, b) if lang != "fr" => cousin_en(lang, a, b, s, half),
        (3, 2) => word!(lang, s, "kin.fr.breton_uncle.m", "kin.fr.breton_uncle.f", "kin.fr.breton_uncle.u"),
        (2, 3) => word!(lang, s, "kin.fr.breton_nephew.m", "kin.fr.breton_nephew.f", "kin.fr.breton_nephew.u"),
        (a, b) if a == b => demi + &cousin_fr(lang, a, s),
        (a, b) if a > b => rust_i18n::t!(
            "kin.fr.of_a",
            who = demi + &cousin_fr(lang, b, s),
            whom = ancestor_name(lang, a - b, Sex::U),
            locale = lang
        )
        .into_owned(),
        (a, b) => rust_i18n::t!(
            "kin.fr.of_a",
            who = descendant_name(lang, b - a, s),
            whom = demi + &cousin_fr(lang, a, Sex::U),
            locale = lang
        )
        .into_owned(),
    }
}

/// Blood relationship and generations from A and from B up to the common ancestor.
fn blood(g: &Gedcom, a: &str, b: &str) -> Option<(Kinship, u32, u32)> {
    let ma = ancestors(g, a);
    let mb = ancestors(g, b);
    let mut common: Vec<(u32, u32, &String)> =
        ma.iter().filter_map(|(x, (da, _))| mb.get(x).map(|(db, _)| (*da, *db, x))).collect();
    if common.is_empty() {
        return None;
    }
    common.sort_by_key(|(da, db, x)| (da + db, da.abs_diff(*db), (*x).clone()));
    let (da, db, _) = common[0];
    let best: Vec<String> = common.iter().filter(|c| c.0 == da && c.1 == db).map(|c| c.2.clone()).collect();
    let anc = &best[0];
    let side_a = chain(&ma, anc); // anc → … → a
    let side_b = chain(&mb, anc); // anc → … → b
    let half = da > 0 && db > 0 && {
        let fa = family_of(g, &side_a[1], anc);
        let fb = family_of(g, &side_b[1], anc);
        fa.is_none() || fa != fb
    };
    let mut path: Vec<String> = side_a.into_iter().rev().collect();
    path.extend(side_b.into_iter().skip(1));
    Some((
        Kinship { label: blood_label(da, db, sex_of(g, b), half), degree: Some(da + db), path, common: best },
        da,
        db,
    ))
}

fn spouses(g: &Gedcom, x: &str) -> Vec<String> {
    let Some(ind) = g.get(x) else { return vec![] };
    ind.children_with("FAMS")
        .filter_map(|n| n.pointer())
        .filter_map(|f| g.get(f))
        .flat_map(|fam| fam.children.iter().filter(|c| c.tag == "HUSB" || c.tag == "WIFE").filter_map(|c| c.pointer()))
        .filter(|p| *p != x)
        .map(str::to_string)
        .collect()
}

/// Relationship of B to A (“B is the … of A”), by blood or by marriage.
pub fn relationship(g: &Gedcom, a: &str, b: &str) -> Option<Kinship> {
    g.individual(a)?;
    g.individual(b)?;
    if let Some((k, _, _)) = blood(g, a, b) {
        return Some(k);
    }
    let sb = sex_of(g, b);
    let lang = crate::i18n::language();
    let lang = &*lang;
    // Conjoint.
    if spouses(g, a).iter().any(|s| s == b) {
        return Some(Kinship {
            label: word!(lang, sb, "kin.husband", "kin.wife", "kin.spouse"),
            degree: None,
            path: vec![a.into(), b.into()],
            common: vec![],
        });
    }
    // Relative by marriage on the side of A's spouse.
    for s in spouses(g, a) {
        if let Some((k, da, db)) = blood(g, &s, b) {
            let label = match (da, db) {
                (1, 0) => word!(lang, sb, "kin.father_in_law", "kin.mother_in_law", "kin.parent_in_law"),
                (1, 1) => word!(lang, sb, "kin.brother_in_law", "kin.sister_in_law", "kin.sibling_in_law"),
                _ => rust_i18n::t!("kin.of_spouse", who = k.label, locale = lang).into_owned(),
            };
            let mut path = vec![a.to_string()];
            path.extend(k.path);
            return Some(Kinship { label, degree: None, path, common: k.common });
        }
    }
    // Spouse of a blood relative of A.
    for s in spouses(g, b) {
        if let Some((k, da, db)) = blood(g, a, &s) {
            let label = match (da, db) {
                (0, 1) => word!(lang, sb, "kin.son_in_law", "kin.daughter_in_law", "kin.child_in_law"),
                (1, 1) => word!(lang, sb, "kin.brother_in_law", "kin.sister_in_law", "kin.sibling_in_law"),
                (1, 0) => word!(lang, sb, "kin.stepfather", "kin.stepmother", "kin.stepparent"),
                _ => rust_i18n::t!(
                    "kin.spouse_of",
                    who = word!(lang, sb, "kin.spouse_m", "kin.spouse_f", "kin.spouse_u"),
                    whom = k.label,
                    locale = lang
                )
                .into_owned(),
            };
            let mut path = k.path;
            path.push(b.to_string());
            return Some(Kinship { label, degree: None, path, common: k.common });
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn labels() {
        let fr = |a, b, s, half| blood_label_in("fr", a, b, s, half);
        assert_eq!(fr(2, 2, Sex::F, false), "cousine germaine");
        assert_eq!(fr(3, 3, Sex::M, false), "cousin issu de germain");
        assert_eq!(fr(4, 4, Sex::F, false), "cousine au 4e degré canonique");
        assert_eq!(fr(1, 1, Sex::M, true), "demi-frère");
        assert_eq!(fr(3, 1, Sex::F, false), "grand-tante");
        assert_eq!(fr(4, 1, Sex::M, false), "arrière-grand-oncle");
        assert_eq!(fr(1, 3, Sex::M, false), "petit-neveu");
        assert_eq!(fr(0, 3, Sex::F, false), "arrière-petite-fille");
        assert_eq!(fr(0, 7, Sex::F, false), "descendante, 7 générations en dessous");
        assert_eq!(fr(4, 0, Sex::M, false), "arrière-arrière-grand-père");
        assert_eq!(fr(3, 2, Sex::M, false), "oncle à la mode de Bretagne");
        assert_eq!(fr(4, 2, Sex::M, false), "cousin germain d'un grand-parent");
        assert_eq!(fr(2, 4, Sex::F, false), "petite-fille d'un cousin(e) germain(e)");
    }

    #[test]
    fn english_labels() {
        let en = |a, b, s, half| blood_label_in("en", a, b, s, half);
        assert_eq!(en(2, 2, Sex::F, false), "first cousin");
        assert_eq!(en(3, 3, Sex::M, false), "second cousin");
        assert_eq!(en(1, 1, Sex::M, true), "half-brother");
        assert_eq!(en(3, 1, Sex::F, false), "great-aunt");
        assert_eq!(en(4, 1, Sex::M, false), "great-great-uncle");
        assert_eq!(en(1, 3, Sex::M, false), "great-nephew");
        assert_eq!(en(0, 3, Sex::F, false), "great-granddaughter");
        assert_eq!(en(4, 0, Sex::M, false), "great-great-grandfather");
        assert_eq!(en(3, 2, Sex::M, false), "first cousin once removed");
        assert_eq!(en(2, 4, Sex::F, false), "first cousin twice removed");
        assert_eq!(en(5, 5, Sex::U, false), "fourth cousin");
    }
}
