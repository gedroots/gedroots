//! Recherche et filtres sur les individus.
//!
//! Une requête est du texte libre, éventuellement complété de critères
//! `clé:valeur`, par exemple :
//!
//! ```text
//! guyot né:1800-1850 lieu:troyes sexe:f sans:source
//! ```
//!
//! | Critère | Effet |
//! | --- | --- |
//! | texte libre | chaque mot doit apparaître dans un nom (prénoms, nom, noms alternatifs), sans tenir compte des accents ni de la casse |
//! | `né:1850`, `né:1800-1850`, `né:<1800`, `né:>1900` | année de naissance (ou baptême) |
//! | `mort:…` / `décès:…` | année de décès (ou inhumation) |
//! | `vécu:1870` | en vie cette année-là (d'après naissance/décès connus) |
//! | `lieu:troyes` | un événement de la personne a eu lieu à cet endroit |
//! | `prof:meunier` | profession (`OCCU`) |
//! | `nom:guyot` | nom de famille |
//! | `sexe:h` / `sexe:f` / `sexe:?` | sexe (h/m = homme, f = femme, ? = inconnu) |
//! | `sans:source`, `sans:parents`, `sans:date`, `sans:famille`, `sans:média` | ce qui manque |
//! | `sans:mariage` | a une union sans mariage (`MARR`) |
//! | `avec:média`, `avec:note`, `avec:source`, `avec:mariage` | ce qui est présent |
//! | `sosa`, `sosa:1-63` | porte un numéro Sosa (ascendant du de cujus) |
//! | `id:I00012` ou `@I00012@` | identifiant GEDCOM (recherche avancée) |
//!
//! Les clés acceptent aussi leur forme sans accent (`ne:`, `deces:`, `media`).

use crate::document::Gedcom;
use crate::model::{Individual, sort_key};

/// Intervalle d'années (bornes incluses).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct YearRange {
    pub from: Option<i32>,
    pub to: Option<i32>,
}

impl YearRange {
    pub(crate) fn parse(s: &str) -> Option<Self> {
        let s = s.trim();
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

/// Ce qui doit manquer (`sans:`) ou être présent (`avec:`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Feature {
    Source,
    Parents,
    Date,
    Family,
    Media,
    Note,
    /// Mariage d'une union. `sans:mariage` : une union au moins n'a pas de mariage
    /// (une personne sans union ne compte pas).
    Marriage,
}

impl Feature {
    fn parse(s: &str) -> Option<Self> {
        Some(match sort_key(s).trim_end_matches('s') {
            "source" => Feature::Source,
            "parent" => Feature::Parents,
            "date" => Feature::Date,
            "famille" | "conjoint" => Feature::Family,
            "media" | "photo" | "image" => Feature::Media,
            "note" => Feature::Note,
            "mariage" => Feature::Marriage,
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

    /// Ce qui manque, pour `sans:`.
    fn missing(self, g: &Gedcom, i: &Individual) -> bool {
        match self {
            Feature::Marriage => i.fams().iter().any(|f| g.family(f).is_some_and(|f| f.marriage().is_none())),
            _ => !self.present(g, i),
        }
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
    /// Nom de famille (`nom:`), normalisé.
    pub surname: Option<String>,
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
                "ne" | "nee" | "naissance" | "n" => YearRange::parse(val).map(|r| out.born = Some(r)).is_some(),
                "mort" | "morte" | "deces" | "d" => YearRange::parse(val).map(|r| out.died = Some(r)).is_some(),
                "vecu" | "vivant" | "en" => val.parse().map(|y| out.alive_in = Some(y)).is_ok(),
                "lieu" | "a" => {
                    out.place = Some(sort_key(val));
                    !val.is_empty()
                }
                "nom" => {
                    out.surname = Some(sort_key(val));
                    !val.is_empty()
                }
                "prof" | "profession" | "metier" => {
                    out.occupation = Some(sort_key(val));
                    !val.is_empty()
                }
                "sexe" | "sex" => {
                    out.sex = match sort_key(val).as_str() {
                        "h" | "m" | "homme" => Some('M'),
                        "f" | "femme" => Some('F'),
                        "?" | "u" | "inconnu" => Some('U'),
                        _ => None,
                    };
                    out.sex.is_some()
                }
                "sans" => Feature::parse(val).map(|f| out.without.push(f)).is_some(),
                "avec" => Feature::parse(val).map(|f| out.with.push(f)).is_some(),
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
                // « 1 rue de la Paix » n'est pas un critère : on garde le mot tel quel.
                _ => {
                    out.words.push(sort_key(crate::query::unquote(tok)));
                    true
                }
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
        if let Some(n) = &self.surname
            && !sort_key(&i.surname()).contains(n.as_str())
        {
            return false;
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
    let key = token.split_once(':').map(|(k, _)| k);
    let mut present = false;
    let mut kept: Vec<&str> = Vec::new();
    for t in query.split_whitespace() {
        if t.eq_ignore_ascii_case(token) {
            present = true;
            continue;
        }
        // Un seul critère `sexe:` à la fois.
        if key == Some("sexe") && t.to_lowercase().starts_with("sexe:") {
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
    query.split_whitespace().any(|t| t.eq_ignore_ascii_case(token))
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
