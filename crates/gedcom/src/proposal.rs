//! Propositions d'un agent IA : un acte à relire avant d'être écrit.
//!
//! Un agent (connecté par le serveur MCP de l'app) ne modifie jamais l'arbre : il
//! décrit un acte avec le même modèle que la saisie d'acte ([`Act`]), et la source
//! qui le contient (existante, ou nouvelle avec son registre et son dépôt). Avant
//! d'être montrée, une proposition est contrôlée ([`Proposal::check`]) : elle doit
//! s'insérer dans la structure de l'arbre (dépôt → source → fichier → événement →
//! personnes) et suivre les habitudes du fichier (une source par acte, ou par
//! registre). Une proposition incohérente est refusée avec la raison, que l'agent lit
//! pour la corriger.
//!
//! Pour la relecture, [`Proposal::compare`] met en regard ce que l'arbre contient et
//! ce qui est proposé, et [`Proposal::diff`] donne les lignes GEDCOM exactes qui
//! changeront, calculées sur une copie du fichier.

use crate::act::{Act, ActResult, ActRole, PersonChoice, PersonField};
use crate::document::{Gedcom, RecordKind};
use crate::node::Node;
use crate::parse::node_to_string;

/// Nouvelle source, créée à l'enregistrement de la proposition.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct NewSource {
    /// Titre ; vide : titre automatique de l'acte (« Acte de naissance de… »).
    pub title: String,
    /// Registre (`PUBL`) : « Rennes, naissances an X ».
    pub register: String,
    /// Dépôt existant (`REPO`).
    pub repository: Option<String>,
    /// Cote dans le dépôt (`CALN`).
    pub call_number: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SourceChoice {
    Existing(String),
    New(NewSource),
}

/// Un acte proposé, avec sa source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Proposal {
    pub source: SourceChoice,
    /// L'acte ; sa source (`act.source`) est remplacée par celle créée si elle est nouvelle.
    pub act: Act,
}

/// Ce qu'une ligne de la comparaison change.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Change {
    /// Déjà dans l'arbre.
    Same,
    /// Rien dans l'arbre : la valeur est ajoutée.
    Added,
    /// La valeur de l'arbre est précisée (date plus précise, compatible).
    Refined,
    /// La valeur proposée contredit celle de l'arbre.
    Differs,
    /// Personne créée.
    Create,
    /// Personne liée à un individu de l'arbre.
    Link,
    /// Personne seulement mentionnée dans la citation.
    Mention,
}

/// Champ d'une ligne de comparaison.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Field {
    /// Date de l'événement (valeurs GEDCOM).
    Date,
    /// Lieu de l'événement.
    Place,
    /// Personne de l'acte (indice dans `act.participants`) et son rôle affiché.
    Person { index: usize, role: String },
}

/// Ligne « Dans l'arbre » / « Proposé ».
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    pub field: Field,
    pub current: String,
    pub proposed: String,
    pub change: Change,
}

/// Maillon du rattachement : existe déjà, ou sera créé.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Link {
    pub value: String,
    pub detail: String,
    pub created: bool,
}

/// Rattachement de la proposition dans l'arbre.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Chain {
    pub repository: Option<Link>,
    pub source: Option<Link>,
    /// Sources du fichier tirées du même registre.
    pub same_register: usize,
}

/// Ligne du diff GEDCOM : `'+'` ajoutée, `'-'` retirée, `' '` inchangée.
pub type DiffLine = (char, String);

impl Proposal {
    /// Rattache la saisie de l'agent à l'arbre : un lieu nommé comme un lieu du fichier
    /// (« Agneaux ») devient ce lieu, et les personnes liées prennent leur nom de l'arbre
    /// (titres d'actes, noms de fichiers).
    pub fn complete(&mut self, ged: &Gedcom) {
        let act = &mut self.act;
        // Dates en syntaxe GEDCOM 7 (`FRENCH_R 30 GERM 4`, pas `@#DFRENCH R@ 30 GERM 4`).
        for d in [&mut act.date, &mut act.record_date] {
            let parsed = crate::Date::parse(d);
            if !d.trim().is_empty() && parsed.year().is_some() {
                *d = parsed.to_gedcom();
            }
        }
        act.place = ged.known_place(&act.place);
        for p in &mut act.participants {
            for f in [PersonField::Residence, PersonField::Birthplace] {
                let v = p.get(f).to_string();
                if !v.is_empty() {
                    p.set(f, ged.known_place(&v));
                }
            }
            if let PersonChoice::Link(x) = &p.choice
                && let Some(ind) = ged.individual(x)
            {
                if p.get(PersonField::Given).is_empty() && p.get(PersonField::Surname).is_empty() {
                    let (given, surname) = (ind.given(), ind.surname());
                    p.set(PersonField::Given, given);
                    p.set(PersonField::Surname, surname);
                }
                if p.get(PersonField::Sex).is_empty() && matches!(ind.sex(), "M" | "F") {
                    let sex = ind.sex().to_string();
                    p.set(PersonField::Sex, sex);
                }
            }
        }
    }

    /// Crée la source si elle est nouvelle et rend l'acte qui la désigne (pour le
    /// reprendre dans la saisie d'acte).
    pub fn materialize(&self, ged: &mut Gedcom) -> Result<Act, String> {
        let mut act = self.act.clone();
        match &self.source {
            SourceChoice::Existing(x) => act.source = x.clone(),
            SourceChoice::New(s) => {
                let x = ged.create(RecordKind::Source);
                let node = ged.get_mut(&x).ok_or("source not created")?;
                node.children.retain(|c| c.tag != "TITL");
                if !s.title.trim().is_empty() {
                    node.insert_child(Node::with_value("TITL", s.title.trim()));
                }
                if !s.register.trim().is_empty() {
                    node.insert_child(Node::with_value("PUBL", s.register.trim()));
                }
                if let Some(r) = &s.repository {
                    let mut repo = Node::with_value("REPO", format!("@{r}@"));
                    if !s.call_number.trim().is_empty() {
                        repo = repo.push(Node::with_value("CALN", s.call_number.trim()));
                    }
                    node.insert_child(repo);
                }
                act.source = x;
            }
        }
        Ok(act)
    }

    /// Écrit la proposition dans l'arbre (source si nouvelle, puis l'acte).
    pub fn apply(&self, ged: &mut Gedcom) -> Result<ActResult, String> {
        let mut copy = ged.clone();
        let act = self.materialize(&mut copy)?;
        let mut res = act.save(&mut copy)?;
        if matches!(self.source, SourceChoice::New(_)) && !res.touched.contains(&act.source) {
            res.touched.insert(0, act.source.clone());
        }
        *ged = copy;
        Ok(res)
    }

    /// La proposition s'insère-t-elle dans l'arbre ? L'erreur dit quoi corriger.
    pub fn check(&self, ged: &Gedcom, this_year: i32) -> Result<(), String> {
        self.check_source(ged)?;
        self.check_people(ged, this_year)?;
        self.apply(&mut ged.clone()).map(|_| ())
    }

    fn check_source(&self, ged: &Gedcom) -> Result<(), String> {
        match &self.source {
            SourceChoice::Existing(x) => {
                if ged.get(x).is_none_or(|n| n.tag != "SOUR") {
                    return Err(format!("no source with id {x}"));
                }
                // Source d'un seul acte (sans référence) : le fichier range un acte par source.
                let acts = ged.acts(x);
                if self.act.original.is_none() && acts.len() == 1 && acts[0].page.trim().is_empty() {
                    let s = crate::model::Source::new(ged.get(x).unwrap());
                    return Err(format!(
                        "source {x} (\"{}\") already holds one act: this file keeps one source per act; \
                         propose a new source with the same register and repository",
                        s.title()
                    ));
                }
            }
            SourceChoice::New(s) => {
                if let Some(r) = &s.repository
                    && ged.get(r).is_none_or(|n| n.tag != "REPO")
                {
                    return Err(format!("no repository with id {r}"));
                }
                if s.register.trim().is_empty() && s.title.trim().is_empty() {
                    return Err("give the new source's register (e.g. \"Rennes, births 1802\")".into());
                }
                // Registre déjà décrit par une source qui rassemble plusieurs actes.
                if let Some(x) = ged.register_sources(&s.register).into_iter().find(|x| ged.acts(x).len() > 1) {
                    return Err(format!(
                        "this file keeps one source per register: use source {x} with the act's reference as page"
                    ));
                }
            }
        }
        Ok(())
    }

    fn check_people(&self, ged: &Gedcom, this_year: i32) -> Result<(), String> {
        let act = &self.act;
        for (i, p) in act.participants.iter().enumerate() {
            if let PersonChoice::Link(x) = &p.choice {
                if ged.individual(x).is_none() {
                    return Err(format!("{}: no person with id {x}", act.role_label(i)));
                }
                if ged.is_living(x, this_year) {
                    return Err(format!("{}: living person, cannot be linked", act.role_label(i)));
                }
            }
        }
        // Parents : ceux de l'arbre ne sont pas remplacés en silence.
        for (i, p) in act.participants.iter().enumerate() {
            if !matches!(p.role, ActRole::Father | ActRole::Mother) || p.choice == PersonChoice::Mention || p.is_blank()
            {
                continue;
            }
            let Some(child) = self.linked_principal(p.of) else { continue };
            let (f, m) = ged.parents(&child);
            let current = if p.role == ActRole::Father { f } else { m };
            let Some(current) = current else { continue };
            if p.choice != PersonChoice::Link(current.xref().to_string()) {
                return Err(format!(
                    "{}: the tree already has {} ({}) here; link that person, or mention this one",
                    act.role_label(i),
                    current.display_name(),
                    current.xref()
                ));
            }
        }
        Ok(())
    }

    /// Individu lié comme `nth`-ième personne principale.
    fn linked_principal(&self, nth: usize) -> Option<String> {
        self.act.participants.iter().enumerate().find_map(|(i, p)| match &p.choice {
            PersonChoice::Link(x) if p.role == ActRole::Principal && self.act.nth_principal(i) == nth => {
                Some(x.clone())
            }
            _ => None,
        })
    }

    /// Événement de l'arbre que l'acte complète (sur la personne principale liée).
    fn current_event(&self, ged: &Gedcom) -> Option<(String, String)> {
        let x = self.linked_principal(0)?;
        let node = if self.act.is_family() {
            let other = self.linked_principal(1)?;
            let ind = ged.individual(&x)?;
            let fam = ind
                .fams()
                .into_iter()
                .find(|f| ged.family(f).is_some_and(|f| [f.husband(), f.wife()].contains(&Some(other.as_str()))))?;
            ged.get(fam)?
        } else {
            ged.get(&x)?
        };
        let ev = node.children.iter().find(|c| c.tag == self.act.event)?;
        Some((ev.child_val("DATE").unwrap_or("").to_string(), ev.child_val("PLAC").unwrap_or("").to_string()))
    }

    /// Lignes « Dans l'arbre » / « Proposé ».
    pub fn compare(&self, ged: &Gedcom) -> Vec<Row> {
        let act = &self.act;
        let (cur_date, cur_place) = self.current_event(ged).unwrap_or_default();
        let mut rows = Vec::new();
        if !act.date.trim().is_empty() || !cur_date.is_empty() {
            rows.push(Row {
                field: Field::Date,
                change: date_change(&cur_date, &act.date),
                current: cur_date,
                proposed: act.date.trim().to_string(),
            });
        }
        if !act.place.trim().is_empty() || !cur_place.is_empty() {
            let change = text_change(&cur_place, &act.place);
            rows.push(Row { field: Field::Place, change, current: cur_place, proposed: act.place.trim().to_string() });
        }
        for (i, p) in act.participants.iter().enumerate() {
            if p.is_blank() && p.choice == PersonChoice::Create {
                continue;
            }
            // Dans l'arbre : la personne liée elle-même, ou le parent déjà connu.
            let current = match (&p.choice, p.role) {
                (_, ActRole::Father | ActRole::Mother) => self.linked_principal(p.of).and_then(|c| {
                    let (f, m) = ged.parents(&c);
                    if p.role == ActRole::Father { f } else { m }.map(|i| i.display_name())
                }),
                (PersonChoice::Link(x), ActRole::Principal) => ged.individual(x).map(|i| i.display_name()),
                _ => None,
            }
            .unwrap_or_default();
            let (proposed, change) = match &p.choice {
                PersonChoice::Link(x) => {
                    let ind = ged.individual(x);
                    let name = ind.map(|i| format!("{} {}", i.display_name(), i.years()).trim().to_string());
                    let change = if !current.is_empty() { Change::Same } else { Change::Link };
                    (name.unwrap_or_default(), change)
                }
                PersonChoice::Create => (person_text(p), Change::Create),
                PersonChoice::Mention => (person_text(p), Change::Mention),
            };
            rows.push(Row {
                field: Field::Person { index: i, role: act.role_label(i).to_string() },
                current,
                proposed,
                change,
            });
        }
        rows
    }

    /// Rattachement : dépôt et source.
    pub fn chain(&self, ged: &Gedcom) -> Chain {
        let mut chain = Chain::default();
        let repo_link = |x: &str, created: bool, caln: &str| {
            ged.get(x).map(|r| Link {
                value: r.child_val("NAME").unwrap_or(x).to_string(),
                detail: caln.to_string(),
                created,
            })
        };
        match &self.source {
            SourceChoice::Existing(x) => {
                if let Some(n) = ged.get(x) {
                    let s = crate::model::Source::new(n);
                    chain.repository =
                        s.repository().and_then(|r| repo_link(r, false, s.call_number().unwrap_or_default()));
                    let register = s.publication().unwrap_or_default();
                    chain.same_register = ged.register_sources(&register).len().saturating_sub(1);
                    chain.source = Some(Link { value: s.title(), detail: register, created: false });
                }
            }
            SourceChoice::New(s) => {
                chain.repository = s.repository.as_deref().and_then(|r| repo_link(r, false, &s.call_number));
                chain.same_register = ged.register_sources(&s.register).len();
                let title = if s.title.trim().is_empty() { self.act.auto_title() } else { s.title.trim().to_string() };
                chain.source = Some(Link { value: title, detail: s.register.trim().to_string(), created: true });
            }
        }
        chain
    }

    /// Lignes GEDCOM qui changeront, enregistrement par enregistrement.
    pub fn diff(&self, ged: &Gedcom) -> Result<Vec<DiffLine>, String> {
        let mut copy = ged.clone();
        let res = self.apply(&mut copy)?;
        let mut out = Vec::new();
        for x in &res.touched {
            let before = ged.get(x).map(node_to_string).unwrap_or_default();
            let after = copy.get(x).map(node_to_string).unwrap_or_default();
            if !out.is_empty() {
                out.push((' ', String::new()));
            }
            out.extend(line_diff(&before, &after));
        }
        Ok(out)
    }
}

fn person_text(p: &crate::act::Participant) -> String {
    let mut s = p.display_name();
    for f in [PersonField::Age, PersonField::Occupation] {
        let v = p.get(f).trim();
        if !v.is_empty() {
            s = format!("{s}, {v}");
        }
    }
    s
}

fn norm(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ").to_lowercase()
}

fn text_change(current: &str, proposed: &str) -> Change {
    let (c, p) = (norm(current), norm(proposed));
    if c.is_empty() {
        Change::Added
    } else if p.is_empty() || c == p {
        Change::Same
    } else if p.starts_with(&c) || c.starts_with(&p) {
        Change::Refined
    } else {
        Change::Differs
    }
}

/// Date proposée face à celle de l'arbre : identique, plus précise (comprise dans
/// l'intervalle de celle de l'arbre, ou proche d'une date approchée) ou contraire.
fn date_change(current: &str, proposed: &str) -> Change {
    let (c, p) = (current.trim(), proposed.trim());
    if c.is_empty() {
        return Change::Added;
    }
    let (dc, dp) = (crate::Date::parse(c), crate::Date::parse(p));
    // Même date, écrite autrement (`FRENCH_R` / `@#DFRENCH R@`, casse).
    if p.is_empty() || c.eq_ignore_ascii_case(p) || dc.to_gedcom() == dp.to_gedcom() {
        return Change::Same;
    }
    match (dc.rd_range_any(), dp.rd_range_any()) {
        (Some((c0, c1)), Some((p0, p1))) if p0 >= c0 && p1 <= c1 => Change::Refined,
        _ => match (dc.year(), dp.year()) {
            // « vers 1802 », « 1802 » : une date du même ordre la précise.
            (Some(a), Some(b)) if dc.kind().is_some() && (a - b).abs() <= 2 => Change::Refined,
            _ => Change::Differs,
        },
    }
}

impl crate::Date {
    /// Intervalle (jours) couvert par la date simple principale.
    fn rd_range_any(&self) -> Option<(i64, i64)> {
        self.primary()?.rd_range()
    }
}

impl Gedcom {
    /// Lieu du fichier désigné par `typed` : son libellé complet, ou sa commune s'il
    /// n'y en a qu'un de ce nom. Sinon, la saisie telle quelle.
    pub fn known_place(&self, typed: &str) -> String {
        let value = self.place_value(typed, &[]);
        let t = norm(typed);
        if value != typed.trim() || t.is_empty() || t.contains(',') {
            return value;
        }
        let first = |name: &str| name.split(',').map(str::trim).find(|s| !s.is_empty()).map(norm);
        let same: Vec<String> = self
            .places()
            .into_iter()
            .filter(|p| first(&p.name).as_deref() == Some(t.as_str()))
            .map(|p| p.name)
            .collect();
        match same.as_slice() {
            [one] => one.clone(),
            _ => value,
        }
    }

    /// Sources dont le registre (`PUBL`) est celui-ci.
    pub fn register_sources(&self, register: &str) -> Vec<String> {
        let r = norm(register);
        if r.is_empty() {
            return Vec::new();
        }
        self.records_of(RecordKind::Source)
            .filter(|n| n.child_val("PUBL").is_some_and(|p| norm(p) == r))
            .filter_map(|n| n.xref.clone())
            .collect()
    }
}

/// Diff ligne à ligne (plus longue sous-suite commune ; les enregistrements sont courts).
fn line_diff(before: &str, after: &str) -> Vec<DiffLine> {
    let a: Vec<&str> = before.lines().collect();
    let b: Vec<&str> = after.lines().collect();
    let (n, m) = (a.len(), b.len());
    let mut lcs = vec![vec![0u32; m + 1]; n + 1];
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            lcs[i][j] = if a[i] == b[j] { lcs[i + 1][j + 1] + 1 } else { lcs[i + 1][j].max(lcs[i][j + 1]) };
        }
    }
    let (mut i, mut j, mut out) = (0, 0, Vec::new());
    while i < n || j < m {
        if i < n && j < m && a[i] == b[j] {
            out.push((' ', a[i].to_string()));
            i += 1;
            j += 1;
        } else if j < m && (i == n || lcs[i][j + 1] >= lcs[i + 1][j]) {
            out.push(('+', b[j].to_string()));
            j += 1;
        } else {
            out.push(('-', a[i].to_string()));
            i += 1;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::act::{MediaChoice, Participant};

    const FILE: &str = "0 HEAD\n1 GEDC\n2 VERS 7.0\n\
0 @R1@ REPO\n1 NAME Archives d'Ille-et-Vilaine\n\
0 @S1@ SOUR\n1 TITL Acte de naissance de Louis BLANC\n1 PUBL Rennes, naissances an VIII\n1 REPO @R1@\n\
0 @I1@ INDI\n1 NAME Louis /BLANC/\n1 SEX M\n1 BIRT\n2 DATE 1800\n2 SOUR @S1@\n1 DEAT\n2 DATE 1870\n1 FAMC @F1@\n\
0 @I2@ INDI\n1 NAME Jean /BLANC/\n1 SEX M\n1 BIRT\n2 DATE ABT 1802\n2 PLAC Rennes\n1 DEAT\n2 DATE 1880\n1 FAMC @F1@\n\
0 @I3@ INDI\n1 NAME Pierre /BLANC/\n1 SEX M\n1 DEAT\n2 DATE 1840\n1 FAMS @F1@\n\
0 @F1@ FAM\n1 HUSB @I3@\n1 CHIL @I1@\n1 CHIL @I2@\n\
0 TRLR\n";

    fn ged() -> Gedcom {
        Gedcom::parse(FILE).unwrap()
    }

    fn birth(source: SourceChoice) -> Proposal {
        let mut act = Act::new("", "BIRT");
        act.date = "4 APR 1802".into();
        act.place = "Rennes".into();
        act.page = "vue 45".into();
        act.media = MediaChoice::None;
        act.participants = vec![
            Participant { choice: PersonChoice::Link("I2".into()), ..Participant::new(ActRole::Principal) },
            Participant { choice: PersonChoice::Link("I3".into()), ..Participant::new(ActRole::Father) },
            Participant::new(ActRole::Mother).with(PersonField::Given, "Marie").with(PersonField::Surname, "DURAND"),
        ];
        Proposal { source, act }
    }

    fn new_source() -> SourceChoice {
        SourceChoice::New(NewSource {
            register: "Rennes, naissances an X".into(),
            repository: Some("R1".into()),
            call_number: "10 NUM 35238".into(),
            ..Default::default()
        })
    }

    #[test]
    fn coherent_proposal_compares_and_applies() {
        let g = ged();
        let mut p = birth(new_source());
        p.act.place = "rennes".into();
        p.act.date = "@#DFRENCH R@ 14 GERM 10".into();
        p.complete(&g);
        assert_eq!(p.act.date, "FRENCH_R 14 GERM 10");
        p.act.date = "4 APR 1802".into();
        assert_eq!(p.act.place, "Rennes");
        assert_eq!(p.act.participants[0].get(PersonField::Given), "Jean");
        p.check(&g, 2026).unwrap();
        let rows = p.compare(&g);
        assert_eq!(rows[0].field, Field::Date);
        assert_eq!(rows[0].change, Change::Refined, "{rows:?}");
        assert_eq!(rows[1].change, Change::Same);
        let father = rows.iter().find(|r| matches!(&r.field, Field::Person { index: 1, .. })).unwrap();
        assert_eq!(father.change, Change::Same);
        let mother = rows.iter().find(|r| matches!(&r.field, Field::Person { index: 2, .. })).unwrap();
        assert_eq!(mother.change, Change::Create);

        let chain = p.chain(&g);
        assert!(chain.source.as_ref().unwrap().created);
        assert_eq!(chain.repository.as_ref().unwrap().value, "Archives d'Ille-et-Vilaine");

        let diff = p.diff(&g).unwrap();
        assert!(diff.iter().any(|(c, l)| *c == '+' && l.contains("PUBL Rennes, naissances an X")));
        assert!(diff.iter().any(|(c, l)| *c == '+' && l.contains("CALN 10 NUM 35238")));
        // Date de l'arbre remplacée : l'ancienne ligne retirée.
        assert!(diff.iter().any(|(c, l)| *c == '-' && l.contains("DATE ABT 1802")), "{diff:?}");

        let mut g2 = g.clone();
        let res = p.apply(&mut g2).unwrap();
        let src = g2.get(&res.source).unwrap();
        assert_eq!(src.child_val("PUBL"), Some("Rennes, naissances an X"));
        // Rien n'est écrit dans l'original.
        assert_eq!(g.count(RecordKind::Source), 1);
    }

    #[test]
    fn incoherent_proposals_are_refused_with_reason() {
        let g = ged();
        // Source d'un seul acte : le fichier range un acte par source.
        let e = birth(SourceChoice::Existing("S1".into())).check(&g, 2026).unwrap_err();
        assert!(e.contains("one source per act"), "{e}");
        // Dépôt inconnu.
        let mut s = NewSource { register: "X".into(), repository: Some("R9".into()), ..Default::default() };
        let e = birth(SourceChoice::New(s.clone())).check(&g, 2026).unwrap_err();
        assert!(e.contains("no repository"), "{e}");
        s.repository = None;
        // Un autre père que celui de l'arbre.
        let mut p = birth(SourceChoice::New(s));
        p.act.participants[1] = Participant::new(ActRole::Father).with(PersonField::Given, "Paul");
        let e = p.check(&g, 2026).unwrap_err();
        assert!(e.contains("Pierre BLANC"), "{e}");
        // Date impossible (contrôle de la saisie d'acte).
        let mut p = birth(new_source());
        p.act.date = "31 FEB 1802".into();
        assert!(p.check(&g, 2026).is_err());
    }

    #[test]
    fn diff_marks_added_and_removed_lines() {
        let d = line_diff("a\nb\nc\n", "a\nc\nd\n");
        assert_eq!(d, vec![(' ', "a".into()), ('-', "b".into()), (' ', "c".into()), ('+', "d".into())]);
        assert_eq!(date_change("ABT 1802", "4 APR 1802"), Change::Refined);
        assert_eq!(date_change("1802", "4 APR 1802"), Change::Refined);
        assert_eq!(date_change("1810", "4 APR 1802"), Change::Differs);
        assert_eq!(date_change("", "1802"), Change::Added);
        assert_eq!(date_change("FRENCH_R 30 GERM 4", "@#DFRENCH R@ 30 GERM 4"), Change::Same);
    }
}
