//! Calculateur de lien de parenté (« cousin issu de germain », « grand-oncle »…),
//! comme Heredis, Gramps ou MacFamilyTree.

use super::check::parents_of;
use crate::document::Gedcom;
use std::collections::{HashMap, VecDeque};

/// Résultat du calcul : « B est le … de A ».
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Kinship {
    /// Libellé du lien, au genre de B (ex. « cousine issue de germain »).
    pub label: String,
    /// Degré civil (nombre de générations du chemin), si lien de sang.
    pub degree: Option<u32>,
    /// Chemin de A à B en passant par l'ancêtre commun (ou le conjoint).
    pub path: Vec<String>,
    /// Ancêtres communs les plus proches.
    pub common: Vec<String>,
}

/// Ancêtres de `x` avec leur distance et le maillon suivant vers `x`.
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

/// Famille dont `parent` est un des époux et `child` un enfant.
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

fn pick(s: Sex, m: &str, f: &str, u: &str) -> String {
    match s {
        Sex::M => m,
        Sex::F => f,
        Sex::U => u,
    }
    .to_string()
}

fn arriere(n: u32) -> String {
    "arrière-".repeat(n as usize)
}

/// Ascendant à `g` générations : père, grand-mère, arrière-grand-père…
fn ancestor_name(n: u32, s: Sex) -> String {
    match n {
        1 => pick(s, "père", "mère", "parent"),
        2..=5 => arriere(n - 2) + &pick(s, "grand-père", "grand-mère", "grand-parent"),
        _ => format!("ancêtre, {n} générations au-dessus"),
    }
}

fn descendant_name(n: u32, s: Sex) -> String {
    match n {
        1 => pick(s, "fils", "fille", "enfant"),
        2..=5 => arriere(n - 2) + &pick(s, "petit-fils", "petite-fille", "petit-enfant"),
        _ => format!("descendant{}, {n} générations en dessous", if s == Sex::F { "e" } else { "" }),
    }
}

fn cousin_name(n: u32, s: Sex) -> String {
    match n {
        2 => pick(s, "cousin germain", "cousine germaine", "cousin(e) germain(e)"),
        3 => pick(s, "cousin issu de germain", "cousine issue de germain", "cousin(e) issu(e) de germain"),
        _ => format!("{} au {n}e degré canonique", pick(s, "cousin", "cousine", "cousin(e)")),
    }
}

/// Nom du lien de sang, `a` = générations de A à l'ancêtre commun, `b` = de B.
fn blood_label(a: u32, b: u32, s: Sex, half: bool) -> String {
    let demi = if half { "demi-" } else { "" };
    match (a, b) {
        (0, 0) => "même personne".into(),
        (0, b) => descendant_name(b, s),
        (a, 0) => ancestor_name(a, s),
        (1, 1) => format!("{demi}{}", pick(s, "frère", "sœur", "frère ou sœur")),
        (a, 1) => {
            let base = pick(s, "oncle", "tante", "oncle ou tante");
            let x = match a {
                2 => base,
                _ => format!("{}grand-{base}", arriere(a - 3)),
            };
            format!("{demi}{x}")
        }
        (1, b) => {
            let x = match b {
                2 => pick(s, "neveu", "nièce", "neveu ou nièce"),
                _ => arriere(b - 3) + &pick(s, "petit-neveu", "petite-nièce", "petit-neveu ou petite-nièce"),
            };
            format!("{demi}{x}")
        }
        (3, 2) => pick(
            s,
            "oncle à la mode de Bretagne",
            "tante à la mode de Bretagne",
            "oncle ou tante à la mode de Bretagne",
        ),
        (2, 3) => pick(
            s,
            "neveu à la mode de Bretagne",
            "nièce à la mode de Bretagne",
            "neveu ou nièce à la mode de Bretagne",
        ),
        (a, b) if a == b => format!("{demi}{}", cousin_name(a, s)),
        (a, b) if a > b => format!("{demi}{} d'un {}", cousin_name(b, s), ancestor_name(a - b, Sex::U)),
        (a, b) => format!("{} d'un {demi}{}", descendant_name(b - a, s), cousin_name(a, Sex::U)),
    }
}

fn blood(g: &Gedcom, a: &str, b: &str) -> Option<Kinship> {
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
    Some(Kinship { label: blood_label(da, db, sex_of(g, b), half), degree: Some(da + db), path, common: best })
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

/// Lien de B par rapport à A (« B est le … de A »), par le sang ou par alliance.
pub fn relationship(g: &Gedcom, a: &str, b: &str) -> Option<Kinship> {
    g.individual(a)?;
    g.individual(b)?;
    if let Some(k) = blood(g, a, b) {
        return Some(k);
    }
    let sb = sex_of(g, b);
    // Conjoint.
    if spouses(g, a).iter().any(|s| s == b) {
        return Some(Kinship {
            label: pick(sb, "époux", "épouse", "conjoint"),
            degree: None,
            path: vec![a.into(), b.into()],
            common: vec![],
        });
    }
    // Parent par alliance du côté du conjoint de A.
    for s in spouses(g, a) {
        if let Some(k) = blood(g, &s, b) {
            let sibling = k.degree == Some(2) && (k.label.ends_with("frère") || k.label.ends_with("sœur"));
            let label = if k.degree == Some(1) && parents_of(g, &s).contains(&b) {
                pick(sb, "beau-père", "belle-mère", "beau-parent")
            } else if sibling {
                pick(sb, "beau-frère", "belle-sœur", "beau-frère ou belle-sœur")
            } else {
                format!("{} du conjoint", k.label)
            };
            let mut path = vec![a.to_string()];
            path.extend(k.path);
            return Some(Kinship { label, degree: None, path, common: k.common });
        }
    }
    // Conjoint d'un parent de sang de A.
    for s in spouses(g, b) {
        if let Some(k) = blood(g, a, &s) {
            let label = if k.label.starts_with("fils") || k.label.starts_with("fille") || k.label == "enfant" {
                pick(sb, "gendre", "belle-fille", "gendre ou belle-fille")
            } else if k.degree == Some(2) && (k.label.ends_with("frère") || k.label.ends_with("sœur")) {
                pick(sb, "beau-frère", "belle-sœur", "beau-frère ou belle-sœur")
            } else if k.degree == Some(1) {
                pick(sb, "beau-père", "belle-mère", "beau-parent")
            } else {
                format!("{} d'un(e) {}", pick(sb, "conjoint", "conjointe", "conjoint(e)"), k.label)
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
        assert_eq!(blood_label(2, 2, Sex::F, false), "cousine germaine");
        assert_eq!(blood_label(3, 3, Sex::M, false), "cousin issu de germain");
        assert_eq!(blood_label(1, 1, Sex::M, true), "demi-frère");
        assert_eq!(blood_label(3, 1, Sex::F, false), "grand-tante");
        assert_eq!(blood_label(1, 3, Sex::M, false), "petit-neveu");
        assert_eq!(blood_label(0, 3, Sex::F, false), "arrière-petite-fille");
        assert_eq!(blood_label(4, 0, Sex::M, false), "arrière-arrière-grand-père");
        assert_eq!(blood_label(3, 2, Sex::M, false), "oncle à la mode de Bretagne");
        assert_eq!(blood_label(4, 2, Sex::M, false), "cousin germain d'un grand-parent");
        assert_eq!(blood_label(2, 4, Sex::F, false), "petite-fille d'un cousin(e) germain(e)");
    }
}
