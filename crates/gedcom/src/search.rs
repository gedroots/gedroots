//! Recherche et filtres sur les individus.
//!
//! Une requête est du texte libre, éventuellement complété de critères
//! `clé:valeur`, par exemple :
//!
//! ```text
//! guyot born:1800-1850 place:troyes sex:f without:source
//! ```
//!
//! | Critère | Effet |
//! | --- | --- |
//! | texte libre | chaque mot doit apparaître dans un nom (prénoms, nom, noms alternatifs), sans tenir compte des accents ni de la casse |
//! | `born:1850`, `born:1800-1850`, `born:<1800`, `born:>1900`, `born:1850±5` | année de naissance (ou baptême) ; `±` : à quelques années près |
//! | `died:…` | année de décès (ou inhumation) |
//! | `alive:1870` | en vie cette année-là (d'après naissance/décès connus) |
//! | `place:troyes` | un événement de la personne a eu lieu à cet endroit |
//! | `occupation:meunier` | profession (`OCCU`) |
//! | `name:guyot` | nom de famille, variantes d'orthographe comprises (Guyau, Guillot) ; `name:=guyot` : orthographe exacte |
//! | `spouses:@I1@`, `children:@I1@`, `parents:@I1@`, `siblings:@I1@` | proches d'une personne |
//! | `ancestors:@I1@`, `descendants:@I1@` | ascendance ou descendance complète d'une personne |
//! | `sex:m` / `sex:f` / `sex:u` | sexe (m = homme, f = femme, u = inconnu) |
//! | `without:source`, `without:parents`, `without:date`, `without:spouse`, `without:media` | ce qui manque |
//! | `without:marriage` | a une union sans mariage (`MARR`) |
//! | `with:media`, `with:note`, `with:source`, `with:marriage` | ce qui est présent |
//! | `sosa`, `sosa:1-63` | porte un numéro Sosa (ascendant de la personne racine) |
//! | `id:I00012` ou `@I00012@` | identifiant GEDCOM (recherche avancée) |
//!
//! Les mots-clés sont en anglais quelle que soit la langue de l'interface ; seuls leurs
//! libellés sont traduits. Les recherches enregistrées en français par les versions
//! précédentes sont converties à la lecture ([`crate::keywords`]).

use std::cell::OnceCell;
use std::collections::HashSet;

use crate::document::Gedcom;
use crate::model::{Individual, sort_key};
use rust_i18n::t;

/// Intervalle d'années (bornes incluses).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct YearRange {
    pub from: Option<i32>,
    pub to: Option<i32>,
}

impl YearRange {
    pub(crate) fn parse(s: &str) -> Option<Self> {
        let s = s.trim();
        // « 1850±5 » (ou « 1850~5 ») : de 1845 à 1855.
        if let Some((y, m)) = s.split_once(['±', '~']) {
            let (y, m) = (y.trim().parse::<i32>().ok()?, m.trim().parse::<i32>().ok()?.abs());
            return Some(Self { from: Some(y - m), to: Some(y + m) });
        }
        if let Some(r) = s.strip_prefix('<') {
            return Some(Self { from: None, to: Some(r.trim().parse::<i32>().ok()? - 1) });
        }
        if let Some(r) = s.strip_prefix('>') {
            return Some(Self { from: Some(r.trim().parse::<i32>().ok()? + 1), to: None });
        }
        if let Some((a, b)) = s.split_once(['-', '–']) {
            let p = |x: &str| if x.trim().is_empty() { Ok(None) } else { x.trim().parse::<i32>().map(Some) };
            return Some(Self { from: p(a).ok()?, to: p(b).ok()? });
        }
        let y = s.parse::<i32>().ok()?;
        Some(Self { from: Some(y), to: Some(y) })
    }

    pub fn contains(&self, y: i32) -> bool {
        self.from.is_none_or(|f| y >= f) && self.to.is_none_or(|t| y <= t)
    }
}

/// Ce qui doit manquer (`without:`) ou être présent (`with:`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Feature {
    Source,
    Parents,
    Date,
    Family,
    Media,
    Note,
    /// Mariage d'une union. `without:marriage` : une union au moins n'a pas de mariage
    /// (une personne sans union ne compte pas).
    Marriage,
}

impl Feature {
    fn parse(s: &str) -> Option<Self> {
        Some(match sort_key(s).trim_end_matches('s') {
            "source" => Feature::Source,
            "parent" => Feature::Parents,
            "date" => Feature::Date,
            "spouse" | "family" => Feature::Family,
            "media" | "photo" | "image" => Feature::Media,
            "note" => Feature::Note,
            "marriage" => Feature::Marriage,
            _ => return None,
        })
    }

    fn present(self, g: &Gedcom, i: &Individual) -> bool {
        match self {
            // « sourcé » : tous les événements portent une source (comme le filtre « Sans source »).
            Feature::Source => {
                let (s, t) = i.sourced_ratio();
                s == t && (t > 0 || i.node.child("SOUR").is_some())
            }
            Feature::Parents => {
                let (f, m) = g.parents(i.xref());
                f.is_some() || m.is_some()
            }
            Feature::Date => i.birth().and_then(|e| e.date()).and_then(|d| d.year()).is_some(),
            Feature::Family => !i.fams().is_empty(),
            Feature::Media => i.node.child("OBJE").is_some(),
            Feature::Note => i.node.child("NOTE").is_some() || i.node.child("SNOTE").is_some(),
            Feature::Marriage => i.fams().iter().any(|f| g.family(f).is_some_and(|f| f.marriage().is_some())),
        }
    }

    /// Ce qui manque, pour `without:`.
    fn missing(self, g: &Gedcom, i: &Individual) -> bool {
        match self {
            Feature::Marriage => i.fams().iter().any(|f| g.family(f).is_some_and(|f| f.marriage().is_none())),
            _ => !self.present(g, i),
        }
    }
}

/// Proches d'une personne (`ascendants:@I1@`…).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kin {
    Spouses,
    Children,
    Parents,
    Siblings,
    Ancestors,
    Descendants,
}

impl Kin {
    pub const ALL: [Kin; 6] =
        [Kin::Ancestors, Kin::Descendants, Kin::Spouses, Kin::Children, Kin::Parents, Kin::Siblings];

    pub(crate) fn parse(key: &str) -> Option<Self> {
        Some(match key {
            "spouse" | "spouses" => Kin::Spouses,
            "child" | "children" => Kin::Children,
            "parent" | "parents" => Kin::Parents,
            "sibling" | "siblings" => Kin::Siblings,
            "ancestor" | "ancestors" => Kin::Ancestors,
            "descendant" | "descendants" => Kin::Descendants,
            _ => return None,
        })
    }

    /// Clé du critère.
    pub fn key(self) -> &'static str {
        match self {
            Kin::Spouses => "spouses",
            Kin::Children => "children",
            Kin::Parents => "parents",
            Kin::Siblings => "siblings",
            Kin::Ancestors => "ancestors",
            Kin::Descendants => "descendants",
        }
    }

    /// « Ascendants de Jean GUYOT ».
    pub fn label(self, name: &str) -> String {
        match self {
            Kin::Spouses => t!("search.kin.spouses", name = name),
            Kin::Children => t!("search.kin.children", name = name),
            Kin::Parents => t!("search.kin.parents", name = name),
            Kin::Siblings => t!("search.kin.siblings", name = name),
            Kin::Ancestors => t!("search.kin.ancestors", name = name),
            Kin::Descendants => t!("search.kin.descendants", name = name),
        }
        .into_owned()
    }
}

impl Gedcom {
    /// Proches d'une personne (sans elle-même).
    pub fn kin(&self, kin: Kin, xref: &str) -> HashSet<String> {
        let parents_of = |x: &str| -> Vec<String> {
            let Some(i) = self.individual(x) else { return vec![] };
            i.famc()
                .into_iter()
                .filter_map(|f| self.family(f))
                .flat_map(|f| [f.husband(), f.wife()])
                .flatten()
                .map(str::to_string)
                .collect()
        };
        let children_of = |x: &str| -> Vec<String> {
            let Some(i) = self.individual(x) else { return vec![] };
            i.fams().into_iter().filter_map(|f| self.family(f)).flat_map(|f| f.children()).map(str::to_string).collect()
        };
        let mut out: HashSet<String> = HashSet::new();
        match kin {
            Kin::Spouses => {
                if let Some(i) = self.individual(xref) {
                    for f in i.fams().into_iter().filter_map(|f| self.family(f)) {
                        out.extend([f.husband(), f.wife()].into_iter().flatten().map(str::to_string));
                    }
                }
            }
            Kin::Children => out.extend(children_of(xref)),
            Kin::Parents => out.extend(parents_of(xref)),
            Kin::Siblings => {
                if let Some(i) = self.individual(xref) {
                    for f in i.famc().into_iter().filter_map(|f| self.family(f)) {
                        out.extend(f.children().into_iter().map(str::to_string));
                    }
                }
            }
            Kin::Ancestors | Kin::Descendants => {
                let next = |x: &str| if kin == Kin::Ancestors { parents_of(x) } else { children_of(x) };
                let mut todo = next(xref);
                while let Some(x) = todo.pop() {
                    if out.insert(x.clone()) {
                        todo.extend(next(&x));
                    }
                }
            }
        }
        out.remove(xref);
        out
    }
}

/// Proches déjà calculés pour une requête (ignoré dans les comparaisons).
#[derive(Debug, Clone, Default)]
pub struct KinCache(OnceCell<Vec<HashSet<String>>>);

impl PartialEq for KinCache {
    fn eq(&self, _: &Self) -> bool {
        true
    }
}

/// Requête analysée.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct IndividualQuery {
    /// Mots à trouver dans les noms (déjà normalisés par [`sort_key`]).
    pub words: Vec<String>,
    pub born: Option<YearRange>,
    pub died: Option<YearRange>,
    pub alive_in: Option<i32>,
    pub place: Option<String>,
    pub occupation: Option<String>,
    /// Nom de famille (`name:`), normalisé.
    pub surname: Option<String>,
    /// `name:=…` : sans les variantes d'orthographe.
    pub surname_exact: bool,
    /// Proches d'une personne (`ascendants:@I1@`).
    pub kin: Vec<(Kin, String)>,
    kin_sets: KinCache,
    /// `M`, `F` ou `U`.
    pub sex: Option<char>,
    pub without: Vec<Feature>,
    pub with: Vec<Feature>,
    pub sosa: Option<(u64, u64)>,
    pub xref: Option<String>,
    /// Critères non compris, pour les signaler à l'utilisateur.
    pub unknown: Vec<String>,
}

impl IndividualQuery {
    pub fn parse(q: &str) -> Self {
        let mut out = Self::default();
        for (_, tok) in crate::query::words(q) {
            if tok.len() > 2 && tok.starts_with('@') && tok.ends_with('@') {
                out.xref = Some(tok.trim_matches('@').to_string());
                continue;
            }
            let lower = tok.to_lowercase();
            if sort_key(&lower) == "sosa" {
                out.sosa = Some((1, u64::MAX));
                continue;
            }
            let Some((key, val)) = tok.split_once(':') else {
                out.words.push(sort_key(crate::query::unquote(tok)));
                continue;
            };
            let val = crate::query::unquote(val);
            let ok = match sort_key(key).as_str() {
                "born" | "b" => YearRange::parse(val).map(|r| out.born = Some(r)).is_some(),
                "died" | "d" => YearRange::parse(val).map(|r| out.died = Some(r)).is_some(),
                "alive" => val.parse().map(|y| out.alive_in = Some(y)).is_ok(),
                "place" => {
                    out.place = Some(sort_key(val));
                    !val.is_empty()
                }
                "name" => {
                    let exact = val.strip_prefix('=');
                    out.surname_exact = exact.is_some();
                    let v = exact.unwrap_or(val).trim();
                    out.surname = Some(sort_key(v));
                    !v.is_empty()
                }
                "occupation" => {
                    out.occupation = Some(sort_key(val));
                    !val.is_empty()
                }
                "sex" => {
                    out.sex = match sort_key(val).as_str() {
                        "m" | "male" => Some('M'),
                        "f" | "female" => Some('F'),
                        "u" | "?" | "unknown" => Some('U'),
                        _ => None,
                    };
                    out.sex.is_some()
                }
                "without" => Feature::parse(val).map(|f| out.without.push(f)).is_some(),
                "with" => Feature::parse(val).map(|f| out.with.push(f)).is_some(),
                "sosa" => {
                    let r = YearRange::parse(val);
                    out.sosa = r.map(|r| {
                        (r.from.unwrap_or(1).max(1) as u64, r.to.map(|t| t.max(0) as u64).unwrap_or(u64::MAX))
                    });
                    out.sosa.is_some()
                }
                "id" => {
                    out.xref = Some(val.trim_matches('@').to_string());
                    !val.is_empty()
                }
                k => match Kin::parse(k) {
                    Some(kin) => {
                        let x = val.trim_matches('@');
                        out.kin.push((kin, x.to_string()));
                        !x.is_empty()
                    }
                    // « 1 rue de la Paix » n'est pas un critère : on garde le mot tel quel.
                    None => {
                        out.words.push(sort_key(crate::query::unquote(tok)));
                        true
                    }
                },
            };
            if !ok {
                out.unknown.push(tok.to_string());
            }
        }
        out
    }

    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }

    pub fn matches(&self, g: &Gedcom, i: &Individual) -> bool {
        if let Some(x) = &self.xref
            && !i.xref().eq_ignore_ascii_case(x)
            && !sort_key(i.xref()).contains(&sort_key(x))
        {
            return false;
        }
        if !self.words.is_empty() {
            let mut names = String::new();
            for n in i.node.children_with("NAME") {
                names.push_str(&sort_key(&n.val().replace('/', " ")));
                names.push(' ');
                for part in ["GIVN", "SURN", "NICK", "_MARNM"] {
                    if let Some(v) = n.child_val(part) {
                        names.push_str(&sort_key(v));
                        names.push(' ');
                    }
                }
            }
            if !self.words.iter().all(|w| names.contains(w.as_str())) {
                return false;
            }
        }
        if let Some(n) = &self.surname {
            let s = sort_key(&i.surname());
            if !s.contains(n.as_str()) && (self.surname_exact || !crate::phonetic::similar(&s, n)) {
                return false;
            }
        }
        if !self.kin.is_empty() {
            let sets = self.kin_sets.0.get_or_init(|| self.kin.iter().map(|(k, x)| g.kin(*k, x)).collect());
            if !sets.iter().all(|s| s.contains(i.xref())) {
                return false;
            }
        }
        if let Some(s) = self.sex
            && i.sex().chars().next().unwrap_or('U') != s
        {
            return false;
        }
        let year = |e: Option<crate::model::Event>| e.and_then(|e| e.date()).and_then(|d| d.year());
        let (by, dy) = (year(i.birth()), year(i.death()));
        if let Some(r) = self.born
            && !by.is_some_and(|y| r.contains(y))
        {
            return false;
        }
        if let Some(r) = self.died
            && !dy.is_some_and(|y| r.contains(y))
        {
            return false;
        }
        if let Some(y) = self.alive_in {
            // Sans date de décès, on suppose une vie de 100 ans au plus.
            let start = by.or(dy.map(|d| d - 100));
            let end = dy.or(by.map(|b| b + 100));
            if !(start.is_some_and(|s| s <= y) && end.is_some_and(|e| e >= y)) {
                return false;
            }
        }
        if let Some(p) = &self.place {
            let mut found = false;
            i.node.walk(&mut |n| found |= n.tag == "PLAC" && sort_key(n.val()).contains(p.as_str()));
            if !found {
                return false;
            }
        }
        if let Some(o) = &self.occupation
            && !i.node.children_with("OCCU").any(|n| sort_key(n.val()).contains(o.as_str()))
        {
            return false;
        }
        if let Some((a, b)) = self.sosa {
            match i.sosa().and_then(|s| s.parse::<u64>().ok()) {
                Some(n) if n >= a && n <= b => {}
                _ => return false,
            }
        }
        self.without.iter().all(|f| f.missing(g, i)) && self.with.iter().all(|f| f.present(g, i))
    }
}

/// Bascule un critère dans une requête texte (pour les boutons de filtre rapide) :
/// l'ajoute s'il est absent, le retire sinon. Les critères de même clé sont remplacés.
pub fn toggle_token(query: &str, token: &str) -> String {
    let key = |t: &str| t.split_once(':').map(|(k, _)| sort_key(k));
    let sex = key(token).is_some_and(|k| k == "sex");
    let mut present = false;
    let mut kept: Vec<&str> = Vec::new();
    for t in query.split_whitespace() {
        if sort_key(t) == sort_key(token) {
            present = true;
            continue;
        }
        // Un seul critère `sex:` à la fois.
        if sex && key(t).is_some_and(|k| k == "sex") {
            continue;
        }
        kept.push(t);
    }
    if !present {
        kept.push(token);
    }
    let mut s = kept.join(" ");
    if !s.is_empty() {
        s.push(' ');
    }
    s
}

/// Indique si la requête contient déjà ce critère.
pub fn has_token(query: &str, token: &str) -> bool {
    query.split_whitespace().any(|t| sort_key(t) == sort_key(token))
}

impl Gedcom {
    /// Identifiants des individus correspondant à la requête.
    pub fn search_individuals(&self, query: &str) -> Vec<String> {
        let q = IndividualQuery::parse(query);
        self.records_of(crate::document::RecordKind::Individual)
            .map(Individual::new)
            .filter(|i| q.matches(self, i))
            .map(|i| i.xref().to_string())
            .collect()
    }
}
