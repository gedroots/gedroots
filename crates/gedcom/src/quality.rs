//! Qualité des données : tous les contrôles du fichier, rangés par catégorie,
//! avec leur correction quand elle est sûre.
//!
//! Les contrôles de cohérence (dates, âges, liens familiaux) viennent de
//! [`crate::analysis::check`] ; ce module y ajoute la conformité au schéma
//! GEDCOM 7, les éléments vides et les enregistrements que rien n'utilise.
//! Une anomalie se masque comme les autres, par `_GR_IGNORE <code>` sous son
//! enregistrement.
//!
//! Pour ajouter un contrôle : produire des [`Issue`] avec un nouveau code dans
//! [`run`], lui donner une catégorie dans [`category`], un libellé dans
//! [`crate::analysis::check::code_label`] et, s'il y a lieu, une correction
//! dans [`fix_of`].

use crate::analysis::check::{self, Issue, Severity, Thresholds};
use crate::date::Date;
use crate::document::{Gedcom, RecordKind};
use crate::model::event_label;
use crate::node::Node;
use crate::schema::{self, Payload, StructDef};
use std::collections::{HashMap, HashSet, VecDeque};

/// Les deux blocs de la page.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Group {
    /// Erreurs à corriger.
    Fix,
    /// Données manquantes à compléter.
    Complete,
}

impl Group {
    pub fn label(self) -> &'static str {
        match self {
            Group::Fix => "À corriger",
            Group::Complete => "À compléter",
        }
    }
}

/// Catégorie d'anomalies, dans l'ordre de la page.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Category {
    Dates,
    Family,
    Structure,
    Empty,
    Sources,
}

impl Category {
    pub const ALL: [Category; 5] =
        [Category::Dates, Category::Family, Category::Structure, Category::Empty, Category::Sources];

    pub fn label(self) -> &'static str {
        match self {
            Category::Dates => "Dates et âges",
            Category::Family => "Liens familiaux",
            Category::Structure => "Structure GEDCOM",
            Category::Empty => "Éléments vides ou isolés",
            Category::Sources => "Événements sans source",
        }
    }

    /// Identifiant stable (clé de la liste).
    pub fn id(self) -> &'static str {
        match self {
            Category::Dates => "dates",
            Category::Family => "family",
            Category::Structure => "structure",
            Category::Empty => "empty",
            Category::Sources => "sources",
        }
    }

    pub fn from_id(id: &str) -> Option<Category> {
        Self::ALL.into_iter().find(|c| c.id() == id)
    }

    pub fn group(self) -> Group {
        match self {
            Category::Sources => Group::Complete,
            _ => Group::Fix,
        }
    }
}

/// Catégorie d'un code d'anomalie.
pub fn category(code: &str) -> Category {
    match code {
        "husband_not_male" | "wife_not_female" | "own_ancestor" | "missing_backlink" | "role_sex_mismatch" => {
            Category::Family
        }
        "broken_link" | "tag_not_allowed" | "missing_required" | "too_many" | "wrong_pointer" | "invalid_value" => {
            Category::Structure
        }
        "empty_fact" | "empty_note" | "empty_family" | "unused_record" | "disconnected" => Category::Empty,
        "unsourced" => Category::Sources,
        _ => Category::Dates,
    }
}

/// Correction automatique d'une anomalie.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Fix {
    /// Libellé du bouton (« Supprimer »).
    pub label: &'static str,
    /// Applicable à toute la liste d'un coup (« Tout supprimer »).
    pub batch: Option<&'static str>,
}

/// Correction proposée pour un code, s'il y en a une de sûre.
pub fn fix_of(code: &str) -> Option<Fix> {
    Some(match code {
        "empty_fact" => Fix { label: "Supprimer", batch: Some("Supprimer les événements vides") },
        "empty_note" => Fix { label: "Supprimer", batch: Some("Supprimer les notes vides") },
        "empty_family" => Fix { label: "Supprimer", batch: Some("Supprimer les familles vides") },
        "unused_record" => Fix { label: "Supprimer", batch: None },
        "missing_backlink" => Fix { label: "Rétablir le lien", batch: Some("Rétablir tous les liens") },
        "broken_link" => Fix { label: "Retirer le lien", batch: Some("Retirer tous les liens cassés") },
        _ => return None,
    })
}

/// Toutes les anomalies du fichier, masquées comprises (champ `ignored`),
/// triées par catégorie puis gravité.
pub fn run(g: &Gedcom) -> Vec<Issue> {
    let t = Thresholds { report_unsourced: true, ..Thresholds::default() };
    let mut issues = check::check_all(g, &t);
    let mut more = Vec::new();
    if g.version().starts_with('7') {
        structure(g, &mut more);
    }
    empty(g, &mut more);
    unused(g, &mut more);
    check::mark_ignored(g, &mut more);
    issues.extend(more);
    issues.sort_by(|a, b| {
        (category(a.code), a.severity, a.code, &a.message, &a.path).cmp(&(
            category(b.code),
            b.severity,
            b.code,
            &b.message,
            &b.path,
        ))
    });
    issues.dedup();
    issues
}

fn issue(severity: Severity, code: &'static str, xref: &str, path: Vec<usize>, message: String) -> Issue {
    Issue { severity, code, xref: xref.to_string(), related: Vec::new(), message, path, ignored: false }
}

// ---------------------------------------------------------------------------
// Conformité au schéma GEDCOM 7
// ---------------------------------------------------------------------------

fn structure(g: &Gedcom, out: &mut Vec<Issue>) {
    for r in &g.records {
        let (Some(x), Some(def)) = (r.xref.as_deref(), schema::record(&r.tag)) else { continue };
        let who = g.record_label(x);
        validate(g, x, &who, r, def, &mut Vec::new(), out);
    }
}

fn validate(g: &Gedcom, x: &str, who: &str, node: &Node, def: &StructDef, path: &mut Vec<usize>, out: &mut Vec<Issue>) {
    let here = schema::label_fr(&node.tag, def);
    for (i, c) in node.children.iter().enumerate() {
        // Extensions (`_XXX`) : libres, on ne descend pas dedans. CONT est une ligne
        // de suite, pas une structure.
        if c.tag.starts_with('_') || c.tag == "CONT" {
            continue;
        }
        path.push(i);
        match def.sub(&c.tag) {
            None => out.push(issue(
                Severity::Warning,
                "tag_not_allowed",
                x,
                path.clone(),
                format!("{who} : balise {} non permise dans « {here} »", c.tag),
            )),
            Some(sub) => {
                let d = sub.def();
                if let Some(problem) = payload_problem(g, c, d.payload) {
                    let what = schema::label_fr(&c.tag, d);
                    let code = if problem.0 { "wrong_pointer" } else { "invalid_value" };
                    out.push(issue(
                        Severity::Warning,
                        code,
                        x,
                        path.clone(),
                        format!("{who} : {what} — {}", problem.1),
                    ));
                }
                validate(g, x, who, c, d, path, out);
            }
        }
        path.pop();
    }
    for sub in def.subs {
        let n = node.children.iter().filter(|c| c.tag == sub.tag).count();
        let what = schema::label_fr(sub.tag, sub.def());
        if n == 0 && sub.card.required() {
            out.push(issue(
                Severity::Warning,
                "missing_required",
                x,
                path.clone(),
                format!("{who} : « {here} » sans {what} ({})", sub.tag),
            ));
        } else if n > 1 && sub.card.single() {
            out.push(issue(
                Severity::Warning,
                "too_many",
                x,
                path.clone(),
                format!("{who} : {n} fois {what} ({}) dans « {here} », une seule permise", sub.tag),
            ));
        }
    }
}

/// Problème de valeur : `(pointeur vers le mauvais type ?, description)`.
fn payload_problem(g: &Gedcom, n: &Node, payload: Payload) -> Option<(bool, String)> {
    let v = n.val().trim();
    match payload {
        Payload::Pointer(tag) => match n.pointer() {
            Some("VOID") | None if v.is_empty() => None,
            Some("VOID") => None,
            // Lien vers un enregistrement absent : signalé par `broken_link`.
            Some(p) => {
                let target = g.get(p)?;
                (target.tag != tag).then(|| {
                    let kind = RecordKind::from_tag(&target.tag).map(|k| k.label_fr()).unwrap_or("enregistrement");
                    let want = RecordKind::from_tag(tag).map(|k| k.label_fr()).unwrap_or(tag);
                    (true, format!("pointe vers un(e) {} au lieu d'un(e) {}", kind.to_lowercase(), want.to_lowercase()))
                })
            }
            None => Some((false, format!("« {v} » devrait être un lien vers un enregistrement"))),
        },
        Payload::Enum { list, values } => {
            if v.is_empty() {
                return None;
            }
            let items: Vec<&str> = if list { v.split(',').map(str::trim).collect() } else { vec![v] };
            let bad: Vec<&str> = items.into_iter().filter(|i| !i.starts_with('_') && !values.contains(i)).collect();
            (!bad.is_empty()).then(|| (false, format!("valeur « {} » inconnue", bad.join(", "))))
        }
        Payload::Date => {
            if v.is_empty() {
                return None;
            }
            match Date::parse(v) {
                Date::Other(_) => Some((false, format!("date illisible « {v} »"))),
                d => d.validate().err().map(|e| (false, format!("date « {v} » : {e}"))),
            }
        }
        Payload::YOrEmpty => (!v.is_empty() && v != "Y").then(|| (false, format!("« {v} » au lieu de Y ou rien"))),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Éléments vides
// ---------------------------------------------------------------------------

fn note_is_empty(n: &Node) -> bool {
    n.text().trim().is_empty() && n.children.iter().all(|c| c.tag == "CONT")
}

fn empty(g: &Gedcom, out: &mut Vec<Issue>) {
    for r in &g.records {
        let Some(x) = r.xref.as_deref() else { continue };
        if r.tag == "SNOTE" {
            if r.text().trim().is_empty() {
                out.push(issue(Severity::Warning, "empty_note", x, vec![], "Note partagée vide".into()));
            }
            continue;
        }
        let who = g.record_label(x);
        for (i, c) in r.children.iter().enumerate() {
            if let Some(label) = event_label(&c.tag)
                && matches!(r.tag.as_str(), "INDI" | "FAM")
                && c.val().trim().is_empty()
                && c.children.is_empty()
            {
                out.push(issue(
                    Severity::Warning,
                    "empty_fact",
                    x,
                    vec![i],
                    format!("{who} : {label} sans aucune donnée"),
                ));
            }
        }
        // Notes vides, à toute profondeur.
        let mut path = Vec::new();
        empty_notes(x, &who, r, &mut path, out);
    }
}

fn empty_notes(x: &str, who: &str, node: &Node, path: &mut Vec<usize>, out: &mut Vec<Issue>) {
    for (i, c) in node.children.iter().enumerate() {
        path.push(i);
        if c.tag == "NOTE" && note_is_empty(c) {
            out.push(issue(Severity::Warning, "empty_note", x, path.clone(), format!("{who} : note vide")));
        } else if !c.tag.starts_with('_') {
            empty_notes(x, who, c, path, out);
        }
        path.pop();
    }
}

// ---------------------------------------------------------------------------
// Enregistrements que rien n'utilise
// ---------------------------------------------------------------------------

fn unused(g: &Gedcom, out: &mut Vec<Issue>) {
    // Tout ce qui est cité quelque part (sauf par soi-même).
    let mut cited: HashSet<&str> = HashSet::new();
    for (owner, n) in std::iter::once((None, &g.header)).chain(g.records.iter().map(|r| (r.xref.as_deref(), &**r))) {
        n.walk(&mut |c| {
            if let Some(p) = c.pointer()
                && Some(p) != owner
            {
                cited.insert(p);
            }
        });
    }

    for r in &g.records {
        let Some(x) = r.xref.as_deref() else { continue };
        match r.tag.as_str() {
            "FAM" => {
                let members = r.children.iter().filter(|c| matches!(c.tag.as_str(), "HUSB" | "WIFE" | "CHIL"));
                if members.filter(|c| c.pointer().is_some_and(|p| g.get(p).is_some())).count() == 0 {
                    out.push(issue(Severity::Warning, "empty_family", x, vec![], "Famille sans aucun membre".into()));
                }
            }
            "OBJE" | "SOUR" | "REPO" | "SNOTE" if !cited.contains(x) => {
                // Une note vide est déjà signalée comme telle.
                if r.tag == "SNOTE" && r.text().trim().is_empty() {
                    continue;
                }
                let kind = RecordKind::from_tag(&r.tag).map(|k| k.label_fr()).unwrap_or("");
                out.push(issue(
                    Severity::Info,
                    "unused_record",
                    x,
                    vec![],
                    format!("{kind} « {} » utilisé(e) nulle part", g.record_label(x)),
                ));
            }
            _ => {}
        }
    }

    // Individus sans aucun lien avec l'individu racine : un signalement par
    // groupe de personnes reliées entre elles (un arbre à part). Un lien, c'est
    // une famille commune ou une association (`ASSO`, témoins d'un acte…), dans
    // un sens ou dans l'autre.
    let Some(root) = g.root() else { return };
    let linked = |n: &Node| matches!(n.tag.as_str(), "INDI" | "FAM");
    let mut edges: HashMap<&str, Vec<&str>> = HashMap::new();
    for r in g.records.iter().filter(|r| linked(r)) {
        let Some(x) = r.xref.as_deref() else { continue };
        r.walk(&mut |c| {
            if let Some(p) = c.pointer()
                && p != x
                && matches!(c.tag.as_str(), "FAMC" | "FAMS" | "HUSB" | "WIFE" | "CHIL" | "ASSO")
                && g.get(p).is_some_and(linked)
            {
                edges.entry(x).or_default().push(p);
                edges.entry(p).or_default().push(x);
            }
        });
    }
    fn component<'a>(
        g: &'a Gedcom,
        edges: &HashMap<&'a str, Vec<&'a str>>,
        start: &str,
        seen: &mut HashSet<&'a str>,
    ) -> usize {
        let Some(start) = g.get(start).and_then(|r| r.xref.as_deref()) else { return 0 };
        let mut people = 0;
        seen.insert(start);
        let mut queue = VecDeque::from([start]);
        while let Some(x) = queue.pop_front() {
            if g.get(x).is_some_and(|r| r.tag == "INDI") {
                people += 1;
            }
            for &n in edges.get(x).into_iter().flatten() {
                if seen.insert(n) {
                    queue.push_back(n);
                }
            }
        }
        people
    }
    let mut seen: HashSet<&str> = HashSet::new();
    component(g, &edges, &root, &mut seen);
    for r in g.records_of(RecordKind::Individual) {
        let Some(x) = r.xref.as_deref() else { continue };
        if seen.contains(x) {
            continue;
        }
        let others = component(g, &edges, x, &mut seen).saturating_sub(1);
        let who = g.record_label(x);
        let message = match others {
            0 => format!("{who} n'a aucun lien avec l'individu racine"),
            n => format!("{who} et {n} autre{} forment un arbre séparé", if n > 1 { "s" } else { "" }),
        };
        out.push(issue(Severity::Info, "disconnected", x, vec![], message));
    }
}

// ---------------------------------------------------------------------------
// Corrections
// ---------------------------------------------------------------------------

impl Gedcom {
    /// Applique la correction d'une anomalie. Renvoie `false` si elle n'en a pas
    /// ou si le fichier a changé depuis le contrôle.
    pub fn fix_issue(&mut self, issue: &Issue) -> bool {
        self.fix_issues(std::slice::from_ref(issue)) > 0
    }

    /// Applique la correction de plusieurs anomalies. Renvoie le nombre corrigé.
    pub fn fix_issues(&mut self, issues: &[Issue]) -> usize {
        let mut fixable: Vec<&Issue> = issues.iter().filter(|i| fix_of(i.code).is_some()).collect();
        // Retirer d'abord les structures les plus loin dans chaque enregistrement,
        // pour que les positions des autres restent valables ; les suppressions
        // d'enregistrements à la fin.
        fixable.sort_by(|a, b| (a.path.is_empty(), &b.xref, &b.path).cmp(&(b.path.is_empty(), &a.xref, &a.path)));
        let mut done = 0;
        let mut touched: HashSet<String> = HashSet::new();
        for i in fixable {
            let ok = match i.code {
                "empty_fact" | "empty_note" if !i.path.is_empty() => self.remove_at(&i.xref, &i.path, &["NOTE"]),
                "empty_note" | "empty_family" | "unused_record" => self.delete(&i.xref).is_some(),
                "missing_backlink" => i.related.first().is_some_and(|f| self.add_backlink(&i.xref, f)),
                "broken_link" => i.related.first().is_some_and(|p| self.remove_pointers(&i.xref, p)),
                _ => false,
            };
            if ok {
                done += 1;
                touched.insert(i.xref.clone());
            }
        }
        for x in touched {
            if let Some(r) = self.get_mut(&x) {
                r.touch();
            }
        }
        done
    }

    /// Retire la sous-structure à `path` si c'est bien un événement ou une des balises `also`.
    fn remove_at(&mut self, xref: &str, path: &[usize], also: &[&str]) -> bool {
        let Some((last, parents)) = path.split_last() else { return false };
        let Some(mut n) = self.get_mut(xref) else { return false };
        for &i in parents {
            let Some(c) = n.children.get_mut(i) else { return false };
            n = c;
        }
        let ok = n.children.get(*last).is_some_and(|c| event_label(&c.tag).is_some() || also.contains(&c.tag.as_str()));
        if ok {
            n.children.remove(*last);
        }
        ok
    }

    /// Ajoute l'individu à la famille qu'il cite sans qu'elle le mentionne.
    pub fn add_backlink(&mut self, indi: &str, fam: &str) -> bool {
        let Some(ind) = self.get(indi) else { return false };
        let p = format!("@{indi}@");
        let as_child = ind.children_with("FAMC").any(|n| n.pointer() == Some(fam));
        let as_spouse = ind.children_with("FAMS").any(|n| n.pointer() == Some(fam));
        let sex = ind.child_val("SEX").unwrap_or("").to_string();
        let Some(f) = self.get_mut(fam) else { return false };
        if f.children.iter().any(|c| c.value.as_deref() == Some(p.as_str())) {
            return false;
        }
        let tag = if as_child {
            "CHIL"
        } else if as_spouse {
            match (sex.as_str(), f.child("HUSB").is_some(), f.child("WIFE").is_some()) {
                ("F", _, false) => "WIFE",
                ("M", false, _) => "HUSB",
                (_, false, _) => "HUSB",
                (_, _, false) => "WIFE",
                _ => return false,
            }
        } else {
            return false;
        };
        f.insert_child(Node::with_value(tag, p));
        true
    }

    /// Retire de l'enregistrement les structures qui pointent vers `target`.
    fn remove_pointers(&mut self, xref: &str, target: &str) -> bool {
        let p = format!("@{target}@");
        let Some(r) = self.get_mut(xref) else { return false };
        fn strip(n: &mut Node, p: &str) -> usize {
            let before = n.children.len();
            n.children.retain(|c| c.value.as_deref() != Some(p));
            before - n.children.len() + n.children.iter_mut().map(|c| strip(c, p)).sum::<usize>()
        }
        strip(r, &p) > 0
    }
}
