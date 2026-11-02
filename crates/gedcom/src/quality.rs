//! Qualité des données : tous les contrôles du fichier, rangés par catégorie,
//! avec leur correction quand elle est sûre.
//!
//! Les contrôles de cohérence (dates, âges, liens familiaux) viennent de
//! [`crate::analysis::check`] ; ce module y ajoute la conformité au schéma
//! GEDCOM 7, les éléments vides, les enregistrements que rien n'utilise et,
//! quand l'appelant donne accès aux fichiers ([`Files`]), l'état des médias.
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
use crate::model::{Place, event_label, sort_key};
use crate::node::Node;
use crate::relations::mime_for;
use crate::schema::{self, Payload, StructDef};
use crate::weblink::is_web_url;
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
    Media,
    Places,
    Sources,
    Files,
    Coords,
}

impl Category {
    pub const ALL: [Category; 9] = [
        Category::Dates,
        Category::Family,
        Category::Structure,
        Category::Empty,
        Category::Media,
        Category::Places,
        Category::Sources,
        Category::Files,
        Category::Coords,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Category::Dates => "Dates et âges",
            Category::Family => "Liens familiaux",
            Category::Structure => "Structure GEDCOM",
            Category::Empty => "Éléments vides ou isolés",
            Category::Media => "Médias",
            Category::Places => "Lieux",
            Category::Coords => "Lieux sans coordonnées",
            Category::Sources => "Événements sans source",
            Category::Files => "Fichiers à rattacher",
        }
    }

    /// Identifiant stable (clé de la liste).
    pub fn id(self) -> &'static str {
        match self {
            Category::Dates => "dates",
            Category::Family => "family",
            Category::Structure => "structure",
            Category::Empty => "empty",
            Category::Media => "media",
            Category::Places => "places",
            Category::Coords => "coords",
            Category::Sources => "sources",
            Category::Files => "files",
        }
    }

    pub fn from_id(id: &str) -> Option<Category> {
        Self::ALL.into_iter().find(|c| c.id() == id)
    }

    pub fn group(self) -> Group {
        match self {
            Category::Sources | Category::Files | Category::Coords => Group::Complete,
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
        "missing_file" | "moved_file" | "duplicate_media" | "missing_form" => Category::Media,
        "unsourced" => Category::Sources,
        "unattached_file" => Category::Files,
        "place_variant" | "place_hierarchy" | "bad_coords" => Category::Places,
        "missing_coords" => Category::Coords,
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
        "moved_file" => Fix { label: "Corriger le chemin", batch: Some("Corriger tous les chemins") },
        "missing_form" => Fix { label: "Déduire le format", batch: Some("Déduire tous les formats") },
        "duplicate_media" => Fix { label: "Fusionner", batch: Some("Tout fusionner") },
        "unattached_file" => Fix { label: "Créer le média", batch: Some("Créer tous les médias") },
        // Deux lieux du même nom peuvent être distincts : à vérifier un par un.
        "place_variant" => Fix { label: "Fusionner", batch: None },
        "place_hierarchy" => Fix { label: "Harmoniser", batch: None },
        _ => return None,
    })
}

/// Accès aux fichiers des médias, fourni par l'application (la bibliothèque ne
/// lit pas le disque).
pub struct Files<'a> {
    /// Le fichier désigné par une valeur `FILE` existe-t-il ?
    pub exists: &'a dyn Fn(&str) -> bool,
    /// Fichiers de médias du dossier du `.ged`, en chemins relatifs (`Actes/x.jpg`).
    pub folder: &'a [String],
}

/// Toutes les anomalies du fichier, masquées comprises (champ `ignored`),
/// triées par catégorie puis gravité. Sans accès aux fichiers.
pub fn run(g: &Gedcom) -> Vec<Issue> {
    run_with(g, None)
}

/// Comme [`run`], avec en plus les contrôles qui regardent les fichiers des
/// médias (absents, déplacés, non rattachés).
pub fn run_with(g: &Gedcom, files: Option<&Files>) -> Vec<Issue> {
    let t = Thresholds { report_unsourced: true, ..Thresholds::default() };
    let mut issues = check::check_all(g, &t);
    let mut more = Vec::new();
    if g.version().starts_with('7') {
        structure(g, &mut more);
    }
    empty(g, &mut more);
    unused(g, &mut more);
    media(g, files, &mut more);
    places(g, &mut more);
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
        // `FILE` sans `FORM` : signalé (et corrigé) par `missing_form`.
        if n == 0 && sub.card.required() && !(sub.tag == "FORM" && node.tag == "FILE") {
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
// Médias et fichiers
// ---------------------------------------------------------------------------

/// Chemin `FILE` comparable à ceux du dossier (`./Actes\\x.jpg` → `Actes/x.jpg`).
fn normalize(file: &str) -> String {
    let f = file.trim().strip_prefix("file://").unwrap_or(file.trim()).replace('\\', "/");
    f.strip_prefix("./").unwrap_or(&f).to_string()
}

fn file_name(path: &str) -> String {
    path.rsplit('/').next().unwrap_or(path).to_lowercase()
}

fn media(g: &Gedcom, files: Option<&Files>, out: &mut Vec<Issue>) {
    // Fichiers du dossier par nom, pour retrouver un fichier déplacé.
    let mut by_name: HashMap<String, Vec<&str>> = HashMap::new();
    for f in files.map(|f| f.folder).unwrap_or_default() {
        by_name.entry(file_name(f)).or_default().push(f);
    }
    let mut seen: HashMap<Vec<String>, &str> = HashMap::new();
    let mut referenced: HashSet<String> = HashSet::new();
    for r in g.records.iter().filter(|r| r.tag == "OBJE") {
        let Some(x) = r.xref.as_deref() else { continue };
        let who = g.record_label(x);
        let mut paths = Vec::new();
        for (i, f) in r.children.iter().enumerate().filter(|(_, c)| c.tag == "FILE") {
            let v = f.val().trim();
            if v.is_empty() {
                continue;
            }
            paths.push(normalize(v));
            referenced.insert(normalize(v));
            if f.child("FORM").is_none() {
                out.push(issue(
                    Severity::Warning,
                    "missing_form",
                    x,
                    vec![i],
                    format!("{who} : format du fichier non indiqué"),
                ));
            }
            let Some(files) = files.filter(|_| !is_web_url(v)) else { continue };
            if (files.exists)(v) {
                continue;
            }
            match by_name.get(&file_name(&normalize(v))).map(Vec::as_slice) {
                Some([found]) => {
                    // Le fichier retrouvé n'est pas « à rattacher » : la correction du chemin s'en charge.
                    referenced.insert(found.to_string());
                    let mut i =
                        issue(Severity::Warning, "moved_file", x, vec![i], format!("{who} : retrouvé dans {found}"));
                    i.related = vec![found.to_string()];
                    out.push(i);
                }
                _ => out.push(issue(Severity::Error, "missing_file", x, vec![i], format!("{who} : {v}"))),
            }
        }
        if paths.is_empty() {
            continue;
        }
        match seen.get(&paths) {
            Some(first) => {
                let mut i = issue(
                    Severity::Warning,
                    "duplicate_media",
                    x,
                    vec![],
                    format!("« {who} » et « {} » désignent le même fichier", g.record_label(first)),
                );
                i.related = vec![first.to_string()];
                out.push(i);
            }
            None => {
                seen.insert(paths, x);
            }
        }
    }
    // Fichiers du dossier qu'aucun média ne désigne.
    for f in files.map(|f| f.folder).unwrap_or_default() {
        if !referenced.contains(f) {
            let mut i = issue(Severity::Info, "unattached_file", "", vec![], f.clone());
            i.related = vec![f.clone()];
            out.push(i);
        }
    }
}

// ---------------------------------------------------------------------------
// Lieux
// ---------------------------------------------------------------------------

/// Parties significatives d'un nom de lieu, pour comparer deux écritures :
/// sans casse, accents, tirets ni ponctuation, sans les codes (postal, INSEE),
/// « St » et « Ste » développés. Les parties vides sont retirées.
fn place_parts(name: &str) -> Vec<String> {
    name.split(',')
        .map(|part| {
            let clean: String = sort_key(part).chars().map(|c| if c.is_alphanumeric() { c } else { ' ' }).collect();
            clean
                .split_whitespace()
                .map(|w| match w {
                    "st" => "saint",
                    "ste" => "sainte",
                    w => w,
                })
                .collect::<Vec<_>>()
                .join(" ")
        })
        .filter(|p| !p.is_empty() && !p.chars().all(|c| c.is_ascii_digit() || c == ' '))
        .collect()
}

/// `short` est-il une écriture abrégée de `long` (mêmes parties dans le même
/// ordre, certaines en moins, la première identique) ?
fn abbreviates(short: &[String], long: &[String]) -> bool {
    if short.len() >= long.len() || short.first() != long.first() {
        return false;
    }
    let mut rest = long.iter();
    short.iter().all(|s| rest.any(|l| l == s))
}

fn place_issue(code: &'static str, severity: Severity, places: &[&str], message: String) -> Issue {
    let mut i = issue(severity, code, "", vec![], message);
    i.related = places.iter().map(|p| p.to_string()).collect();
    i
}

fn places(g: &Gedcom, out: &mut Vec<Issue>) {
    let all = g.places();
    for p in &all {
        match (&p.latitude, &p.longitude) {
            (None, None) => out.push(place_issue("missing_coords", Severity::Info, &[&p.name], p.name.clone())),
            _ if p.coords().is_none() => out.push(place_issue(
                "bad_coords",
                Severity::Warning,
                &[&p.name],
                format!(
                    "{} : coordonnées illisibles ({} {})",
                    p.name,
                    p.latitude.as_deref().unwrap_or("?"),
                    p.longitude.as_deref().unwrap_or("?")
                ),
            )),
            _ => {}
        }
    }

    // Variantes : même nom une fois normalisé, ou forme abrégée d'un seul autre lieu.
    // Chaque variante est rapprochée du lieu le plus utilisé de son groupe.
    let parts: Vec<Vec<String>> = all.iter().map(|p| place_parts(&p.name)).collect();
    let better = |a: &Place, b: &Place| {
        (a.uses, a.coords().is_some(), a.name.len()) > (b.uses, b.coords().is_some(), b.name.len())
    };
    let mut groups: HashMap<&[String], Vec<usize>> = HashMap::new();
    for (i, k) in parts.iter().enumerate().filter(|(_, k)| !k.is_empty()) {
        groups.entry(k.as_slice()).or_default().push(i);
    }
    let mut target: Vec<Option<usize>> = vec![None; all.len()];
    for members in groups.values().filter(|m| m.len() > 1) {
        let best = *members.iter().reduce(|a, b| if better(&all[*b], &all[*a]) { b } else { a }).unwrap();
        for &m in members.iter().filter(|&&m| m != best) {
            target[m] = Some(best);
        }
    }
    for i in 0..all.len() {
        if target[i].is_some() || parts[i].is_empty() {
            continue;
        }
        let longer: Vec<usize> =
            (0..all.len()).filter(|&j| target[j].is_none() && abbreviates(&parts[i], &parts[j])).collect();
        // Plusieurs lieux possibles (Saint-Martin, Isère / Saint-Martin, Gard) : on ne devine pas.
        if let [j] = longer[..] {
            target[i] = Some(j);
        }
    }
    hierarchy(&all, out);
    for (i, t) in target.iter().enumerate() {
        let Some(t) = *t else { continue };
        let (a, b) = (&all[i], &all[t]);
        let n = |p: &Place| format!("{} fois", p.uses);
        out.push(place_issue(
            "place_variant",
            Severity::Warning,
            &[&a.name, &b.name],
            format!("« {} » ({}) semble être « {} » ({})", a.name, n(a), b.name, n(b)),
        ));
    }
}

/// Niveaux d'un nom de lieu, à leur position (vides compris).
fn levels(name: &str) -> Vec<&str> {
    name.split(',').map(str::trim).collect()
}

/// Une même division rattachée à deux divisions supérieures différentes
/// (« Loire » tantôt en « Rhône-Alpes », tantôt en « Auvergne-Rhône-Alpes »).
/// Le niveau le plus bas d'un lieu (sa commune, son lieu-dit) n'est pas
/// comparé : deux communes homonymes sont souvent dans deux départements.
fn hierarchy(all: &[Place], out: &mut Vec<Issue>) {
    // (niveau du parent, parent) → valeur au niveau suivant → nombre de lieux.
    let mut seen: HashMap<(usize, String), HashMap<&str, usize>> = HashMap::new();
    for p in all {
        let l = levels(&p.name);
        let Some(lowest) = l.iter().position(|x| !x.is_empty()) else { continue };
        for m in (lowest + 2)..l.len() {
            let (parent, child) = (l[m - 1], l[m]);
            let code = |x: &str| x.chars().all(|c| c.is_ascii_digit() || c == ' ');
            // Codes (postal, INSEE) : deux communes homonymes en ont de différents.
            if parent.is_empty() || child.is_empty() || code(parent) || code(child) {
                continue;
            }
            *seen.entry((m - 1, sort_key(parent))).or_default().entry(child).or_default() += p.uses;
        }
    }
    let mut keys: Vec<_> = seen.keys().cloned().collect();
    keys.sort();
    for key in keys {
        let values = &seen[&key];
        if values.len() < 2 {
            continue;
        }
        let mut v: Vec<(&str, usize)> = values.iter().map(|(k, n)| (*k, *n)).collect();
        v.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(b.0)));
        let (to, _) = v[0];
        let parent = all
            .iter()
            .find_map(|p| levels(&p.name).get(key.0).filter(|x| sort_key(x) == key.1).map(|x| x.to_string()))
            .unwrap_or_default();
        for (from, n) in &v[1..] {
            let mut i = place_issue(
                "place_hierarchy",
                Severity::Warning,
                &[&parent, from, to],
                format!("{parent} : « {from} » ({n} fois) au lieu de « {to} »"),
            );
            i.path = vec![key.0];
            out.push(i);
        }
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
        // Les variantes de lieux d'abord : harmoniser renomme les lieux qu'elles visent.
        fixable.sort_by_key(|i| i.code == "place_hierarchy");
        let mut done = 0;
        let mut touched: HashSet<String> = HashSet::new();
        for i in fixable {
            let ok = match i.code {
                "empty_fact" | "empty_note" if !i.path.is_empty() => self.remove_at(&i.xref, &i.path, &["NOTE"]),
                "empty_note" | "empty_family" | "unused_record" => self.delete(&i.xref).is_some(),
                "missing_backlink" => i.related.first().is_some_and(|f| self.add_backlink(&i.xref, f)),
                "broken_link" => i.related.first().is_some_and(|p| self.remove_pointers(&i.xref, p)),
                "moved_file" => i.related.first().is_some_and(|f| self.set_media_file(&i.xref, &i.path, f)),
                "missing_form" => self.infer_form(&i.xref, &i.path),
                "duplicate_media" => i.related.first().is_some_and(|keep| self.merge_duplicate_media(keep, &i.xref)),
                "place_hierarchy" => match (&i.related[..], i.path.first()) {
                    ([parent, from, to], Some(&level)) => self.rename_jurisdiction(level, parent, from, to) > 0,
                    _ => false,
                },
                "place_variant" => match &i.related[..] {
                    [from, to] => self.merge_place(from, to),
                    _ => false,
                },
                "unattached_file" => i.related.first().is_some_and(|f| {
                    self.media_for_file(f);
                    true
                }),
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

    /// Le `FILE` à `path` dans le média (position parmi ses sous-structures).
    fn media_file_at(&mut self, media: &str, path: &[usize]) -> Option<&mut Node> {
        let [i] = path else { return None };
        let m = self.get_mut(media).filter(|m| m.tag == "OBJE")?;
        m.children.get_mut(*i).filter(|f| f.tag == "FILE")
    }

    /// Change le chemin d'un fichier de média (fichier déplacé ou retrouvé à la
    /// main) et son format.
    pub fn set_media_file(&mut self, media: &str, path: &[usize], value: &str) -> bool {
        let Some(f) = self.media_file_at(media, path) else { return false };
        f.value = Some(value.to_string());
        f.set_child_val("FORM", mime_for(value));
        true
    }

    /// Déduit `FORM` de l'extension du fichier.
    fn infer_form(&mut self, media: &str, path: &[usize]) -> bool {
        let Some(f) = self.media_file_at(media, path) else { return false };
        if f.child("FORM").is_some() {
            return false;
        }
        let form = mime_for(f.val());
        f.set_child_val("FORM", form);
        true
    }

    /// Fusionne le média `dup` dans `keep` (même fichier) : les liens vers `dup`
    /// pointent vers `keep`, ses notes et sources lui sont ajoutées, puis `dup`
    /// est supprimé.
    pub fn merge_duplicate_media(&mut self, keep: &str, dup: &str) -> bool {
        if keep == dup || self.get(keep).is_none_or(|n| n.tag != "OBJE") {
            return false;
        }
        let Some(src) = self.get(dup).filter(|n| n.tag == "OBJE").cloned() else { return false };
        let target = self.get_mut(keep).unwrap();
        for c in src.children.iter().filter(|c| !matches!(c.tag.as_str(), "FILE" | "CREA" | "CHAN")) {
            if !target.children.contains(c) {
                target.insert_child(c.clone());
            }
        }
        target.touch();
        let p = format!("@{keep}@");
        self.rewrite_links(
            dup,
            |l| {
                l.value = Some(p.clone());
                vec![]
            },
            &[keep],
        );
        self.delete(dup).is_some()
    }

    /// Remplace partout le lieu `from` par `to`, en gardant les coordonnées de
    /// `to` (ou celles de `from` si `to` n'en a pas).
    pub fn merge_place(&mut self, from: &str, to: &str) -> bool {
        let places = self.places();
        let find = |n: &str| places.iter().find(|p| p.name == n);
        let (Some(a), Some(b)) = (find(from), find(to)) else { return false };
        let (lat, long) = match (&b.latitude, &b.longitude, &a.latitude, &a.longitude) {
            (Some(la), Some(lo), _, _) | (_, _, Some(la), Some(lo)) => (la.clone(), lo.clone()),
            _ => (String::new(), String::new()),
        };
        self.update_place(from, to, &lat, &long);
        if !lat.is_empty() {
            self.update_place(to, to, &lat, &long);
        }
        true
    }

    /// Dans les lieux dont le niveau `level` vaut `parent`, remplace `from` par
    /// `to` au niveau suivant. Renvoie le nombre de lieux renommés.
    pub fn rename_jurisdiction(&mut self, level: usize, parent: &str, from: &str, to: &str) -> usize {
        let mut n = 0;
        for p in self.places() {
            let l = levels(&p.name);
            if l.get(level).is_some_and(|x| sort_key(x) == sort_key(parent)) && l.get(level + 1) == Some(&from) {
                let mut new: Vec<&str> = l.clone();
                new[level + 1] = to;
                let new = new.join(", ");
                let (lat, long) = (p.latitude.clone().unwrap_or_default(), p.longitude.clone().unwrap_or_default());
                self.update_place(&p.name, &new, &lat, &long);
                n += 1;
            }
        }
        n
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
