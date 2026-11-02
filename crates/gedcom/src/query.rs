//! Recherche par critères dans les quatre zones (individus, sources, lieux, médias).
//!
//! Une requête reste du texte : des mots libres et des critères `clé:valeur`.
//! Une valeur qui contient des espaces s'écrit entre guillemets
//! (`lieu:"Bar-sur-Aube"`, `dépôt:"AD de l'Aube"`). Dans l'interface, chaque
//! critère complet devient un jeton : [`chip_label`] donne son libellé et
//! [`Gedcom::suggest`] propose les critères et les valeurs du fichier.
//!
//! Les individus ont leurs propres critères ([`crate::search::IndividualQuery`]) ;
//! ce module ajoute ceux des autres zones :
//!
//! | Zone | Critères |
//! | --- | --- |
//! | Sources | `type:naissance` (type d'acte), `dépôt:…`, `sans:média`, `avec:média`, `sans:citation` |
//! | Lieux | `dans:aube` (juridiction), `sans:coordonnées`, `avec:coordonnées` |
//! | Médias | `événement:naissance`, `pour:marchand` (personne liée), `format:image\|pdf\|web`, `sans:lien` |
//!
//! Partout : `id:…` (identifiant GEDCOM, jamais suggéré) et les mots libres
//! (titre, nom, fichier…).
//!
//! Les médias sont aussi classés par **type** ([`MediaIndex::types`]) : les
//! sous-dossiers déclarés dans Réglages du fichier › Médias, sinon le support
//! GEDCOM 7 (`MEDI`). La barre des types de la zone Médias s'appuie dessus.

use std::collections::{HashMap, HashSet};
use std::ops::Range;

use crate::document::{Gedcom, RecordKind};
use crate::kinds::{SourceContent, medium, medium_label};
use crate::model::{Individual, Media, Place, Repository, Source, event_label, sort_key};
use crate::node::Node;
use crate::search::{IndividualQuery, YearRange};

/// Zone où l'on cherche.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Scope {
    Individuals,
    Sources,
    Places,
    Media,
}

/// Découpe une requête en mots, en gardant ensemble un `clé:"valeur avec espaces"`.
/// Renvoie la plage (octets) et le texte de chaque mot.
pub fn words(q: &str) -> Vec<(Range<usize>, &str)> {
    let mut out = Vec::new();
    let mut start: Option<usize> = None;
    let mut quoted = false;
    for (i, c) in q.char_indices() {
        if c == '"' {
            quoted = !quoted;
            start.get_or_insert(i);
        } else if c.is_whitespace() && !quoted {
            if let Some(s) = start.take() {
                out.push((s..i, &q[s..i]));
            }
        } else {
            start.get_or_insert(i);
        }
    }
    if let Some(s) = start {
        out.push((s..q.len(), &q[s..]));
    }
    out
}

/// Valeur d'un critère sans ses guillemets.
pub fn unquote(v: &str) -> &str {
    let v = v.trim();
    v.strip_prefix('"').map(|v| v.strip_suffix('"').unwrap_or(v)).unwrap_or(v)
}

/// Écrit `clé:valeur`, entre guillemets si la valeur contient un espace.
pub fn criterion(key: &str, value: &str) -> String {
    if value.chars().any(char::is_whitespace) {
        format!("{key}:\"{}\"", value.replace('"', ""))
    } else {
        format!("{key}:{value}")
    }
}

/// Clé normalisée d'un critère (`dépôt` → `depot`), s'il a la forme `clé:valeur`.
fn split(word: &str) -> Option<(String, &str)> {
    let (k, v) = word.split_once(':')?;
    if k.is_empty() || k.starts_with('"') {
        return None;
    }
    Some((sort_key(k), unquote(v)))
}

/// Critère des zones autres que les individus.
#[derive(Debug, Clone, PartialEq)]
enum Crit {
    Id(String),
    /// Source : type d'acte (balise ou libellé, normalisé).
    ActType(String),
    Repository(String),
    /// Source / média : `avec:` (true) ou `sans:` (false) média.
    HasMedia(bool),
    /// Source citée, média rattaché.
    Linked(bool),
    Within(String),
    HasCoordinates(bool),
    Event(String),
    Person(String),
    Format(Format),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Image,
    Pdf,
    Web,
}

impl Format {
    fn parse(v: &str) -> Option<Self> {
        Some(match sort_key(v).trim_end_matches('s') {
            "image" | "photo" | "jpg" | "png" => Format::Image,
            "pdf" => Format::Pdf,
            "web" | "lien" | "url" => Format::Web,
            _ => return None,
        })
    }
    fn label(self) -> &'static str {
        match self {
            Format::Image => "Images",
            Format::Pdf => "PDF",
            Format::Web => "Liens web",
        }
    }
}

fn present(v: &str, wanted: &[&str]) -> bool {
    let v = sort_key(v);
    let v = v.trim_end_matches('s');
    wanted.contains(&v)
}

fn parse_crit(scope: Scope, key: &str, val: &str) -> Option<Crit> {
    if key == "id" {
        return (!val.is_empty()).then(|| Crit::Id(val.trim_matches('@').to_string()));
    }
    let nonempty = |v: &str| (!v.is_empty()).then(|| sort_key(v));
    match (scope, key) {
        (Scope::Sources, "type" | "acte") => nonempty(val).map(Crit::ActType),
        (Scope::Sources, "depot" | "archive" | "archives") => nonempty(val).map(Crit::Repository),
        (Scope::Sources, "sans" | "avec") => {
            let on = key == "avec";
            if present(val, &["media", "image", "photo"]) {
                Some(Crit::HasMedia(on))
            } else if present(val, &["citation", "cite"]) {
                Some(Crit::Linked(on))
            } else {
                None
            }
        }
        (Scope::Places, "dans" | "en") => nonempty(val).map(Crit::Within),
        (Scope::Places, "sans" | "avec") => {
            present(val, &["coordonnee", "coord", "gp"]).then(|| Crit::HasCoordinates(key == "avec"))
        }
        (Scope::Media, "evenement" | "acte" | "fait") => nonempty(val).map(Crit::Event),
        (Scope::Media, "pour" | "personne" | "nom") => nonempty(val).map(Crit::Person),
        (Scope::Media, "format") => Format::parse(val).map(Crit::Format),
        (Scope::Media, "sans" | "avec") => present(val, &["lien", "rattachement"]).then(|| Crit::Linked(key == "avec")),
        _ => None,
    }
}

/// Requête analysée pour les sources, les lieux ou les médias.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct RecordQuery {
    /// Mots libres, normalisés.
    pub words: Vec<String>,
    crits: Vec<Crit>,
    /// Critères mal formés (`sans:truc`), pour les signaler.
    pub unknown: Vec<String>,
}

impl RecordQuery {
    pub fn parse(scope: Scope, q: &str) -> Self {
        let mut out = Self::default();
        for (_, w) in words(q) {
            match split(w) {
                Some((k, v)) => match parse_crit(scope, &k, v) {
                    Some(c) => out.crits.push(c),
                    None if is_key(scope, &k) => out.unknown.push(w.to_string()),
                    // « 10:30 » ou une clé inconnue : un mot comme un autre.
                    None => out.words.push(sort_key(w)),
                },
                None => out.words.push(sort_key(unquote(w))),
            }
        }
        out
    }

    pub fn is_empty(&self) -> bool {
        self.words.is_empty() && self.crits.is_empty()
    }

    fn words_in(&self, hay: &str) -> bool {
        let hay = sort_key(hay);
        self.words.iter().all(|w| hay.contains(w.as_str()))
    }

    fn id_ok(&self, xref: &str) -> bool {
        self.crits.iter().all(|c| match c {
            Crit::Id(x) => xref.eq_ignore_ascii_case(x) || sort_key(xref).contains(&sort_key(x)),
            _ => true,
        })
    }
}

/// Clés connues d'une zone, pour distinguer « critère mal écrit » et « mot avec deux-points ».
fn is_key(scope: Scope, key: &str) -> bool {
    let keys: &[&str] = match scope {
        Scope::Individuals => &[],
        Scope::Sources => &["type", "acte", "depot", "archive", "archives", "sans", "avec"],
        Scope::Places => &["dans", "en", "sans", "avec"],
        Scope::Media => &["evenement", "acte", "fait", "pour", "personne", "nom", "format", "sans", "avec"],
    };
    key == "id" || keys.contains(&key)
}

/// Ce que l'on sait d'un média pour la recherche (calculé une fois par version du fichier).
#[derive(Debug, Clone, Default)]
pub struct MediaInfo {
    /// Index dans [`MediaIndex::types`].
    pub kind: Option<usize>,
    /// Événement illustré (« Naissance »), normalisé.
    event: String,
    event_label: String,
    /// Personnes liées (noms normalisés).
    people: String,
    linked: bool,
    format: Option<Format>,
}

/// Types et contexte de tous les médias.
#[derive(Debug, Clone, Default)]
pub struct MediaIndex {
    /// Types dans l'ordre d'affichage (sans « Tous »). Le dernier peut être « Autres ».
    pub types: Vec<String>,
    pub media: HashMap<String, MediaInfo>,
}

pub const OTHER_TYPE: &str = "Autres";
pub const WEB_TYPE: &str = "Liens web";

impl MediaIndex {
    /// Type d'un média (libellé).
    pub fn type_of(&self, xref: &str) -> Option<&str> {
        self.media.get(xref).and_then(|m| m.kind).map(|k| self.types[k].as_str())
    }
}

fn format_of(m: &Media) -> Option<Format> {
    if m.web_url().is_some() {
        return Some(Format::Web);
    }
    if m.is_image() {
        return Some(Format::Image);
    }
    let pdf =
        m.form().is_some_and(|f| f.ends_with("pdf")) || m.file().is_some_and(|f| f.to_lowercase().ends_with(".pdf"));
    pdf.then_some(Format::Pdf)
}

/// Dernier élément d'un chemin de dossier (« Médias/Actes » → « Actes »).
fn dir_label(dir: &str) -> &str {
    dir.trim_matches('/').rsplit('/').next().unwrap_or(dir).trim()
}

impl Gedcom {
    /// Tous les pointeurs du fichier (pour « non citée », « non rattaché »).
    fn all_pointers(&self) -> HashSet<String> {
        let mut out = HashSet::new();
        for r in &self.records {
            for c in &r.children {
                c.walk(&mut |n| {
                    if let Some(p) = n.pointer() {
                        out.insert(p.to_string());
                    }
                });
            }
        }
        out
    }

    /// Classe les médias par type et calcule ce qu'ils illustrent.
    ///
    /// Le type d'un média est, dans l'ordre :
    /// 1. le sous-dossier (Réglages du fichier › Médias) qui contient son fichier,
    ///    sinon celui qui recevrait ce média d'après ce qu'il illustre ;
    /// 2. sans sous-dossiers réglés, le premier dossier de son fichier (« Actes »,
    ///    « Portraits »…), quand les fichiers sont rangés dans plusieurs dossiers ;
    /// 3. sinon son support GEDCOM (`MEDI`).
    ///
    /// Le reste va dans « Autres ».
    pub fn media_index(&self) -> MediaIndex {
        let naming = self.media_naming();
        let contexts = self.media_contexts(&HashMap::new(), Some(&naming));
        let pointers = self.all_pointers();
        let root = naming.root.trim().trim_matches('/').to_string();
        let mut people: HashMap<String, Vec<String>> = HashMap::new();
        for r in self.records.iter().filter(|r| r.tag == "INDI") {
            let i = Individual::new(r);
            for o in r.children.iter().filter(|c| c.tag == "OBJE") {
                if let Some(p) = o.pointer() {
                    people.entry(p.to_string()).or_default().push(format!("{} {}", i.surname(), i.given()));
                }
            }
        }
        // Chemin d'un fichier local, relatif au dossier des médias.
        let local = |f: &Node| -> Option<String> {
            let path = f.val().replace('\\', "/");
            if path.contains("://") || path.starts_with('/') || path.get(1..2) == Some(":") {
                return None;
            }
            let path = path.trim_start_matches("./").to_string();
            Some(match root.is_empty() {
                true => path,
                false => path.strip_prefix(&format!("{root}/")).map(str::to_string).unwrap_or(path),
            })
        };
        let first_dir = |r: &Node| -> Option<String> {
            r.children
                .iter()
                .filter(|c| c.tag == "FILE")
                .find_map(&local)
                .and_then(|p| p.split_once('/').map(|(d, _)| d.to_string()))
                .filter(|d| !d.is_empty() && d != "." && d != "..")
        };

        // Types : sous-dossiers réglés, sinon dossiers des fichiers, sinon supports.
        let mut types: Vec<String> = Vec::new();
        for f in &naming.folders {
            let l = dir_label(&f.dir);
            if !l.is_empty() && !types.iter().any(|t| t == l) {
                types.push(l.to_string());
            }
        }
        let mode_folders = !types.is_empty();
        let mut kind_of: HashMap<String, Option<String>> = HashMap::new();
        let medias: Vec<&Node> = self.records_of(RecordKind::Media).collect();
        if mode_folders {
            for r in &medias {
                let x = r.xref.clone().unwrap_or_default();
                let mut label = None;
                // 1. Le dossier où se trouve le fichier.
                for path in r.children.iter().filter(|c| c.tag == "FILE").filter_map(&local) {
                    if let Some(d) = naming.folders.iter().find(|d| {
                        let dir = d.dir.trim().trim_matches('/');
                        !dir.is_empty() && path.starts_with(&format!("{dir}/"))
                    }) {
                        label = Some(dir_label(&d.dir).to_string());
                        break;
                    }
                }
                // 2. Le dossier qui le recevrait.
                if label.is_none()
                    && let Some(ctx) = contexts.get(&x).filter(|c| !c.event.is_empty())
                {
                    label = naming.folder_for(ctx).map(|f| dir_label(&f.dir).to_string()).filter(|l| !l.is_empty());
                }
                kind_of.insert(x, label);
            }
        } else {
            let dirs: Vec<Option<String>> = medias.iter().map(|r| first_dir(r)).collect();
            let mut counts: HashMap<&str, usize> = HashMap::new();
            for d in dirs.iter().flatten() {
                *counts.entry(d.as_str()).or_default() += 1;
            }
            if counts.len() >= 2 {
                let mut v: Vec<(&str, usize)> = counts.into_iter().collect();
                v.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| sort_key(a.0).cmp(&sort_key(b.0))));
                types = v.into_iter().map(|(d, _)| d.to_string()).collect();
                for (r, d) in medias.iter().zip(dirs) {
                    kind_of.insert(r.xref.clone().unwrap_or_default(), d);
                }
            } else {
                let labels: Vec<Option<&'static str>> =
                    medias.iter().map(|r| medium(r).and_then(medium_label)).collect();
                types = crate::kinds::MEDIUMS
                    .iter()
                    .map(|(_, l)| *l)
                    .filter(|l| labels.contains(&Some(*l)))
                    .map(str::to_string)
                    .collect();
                for (r, l) in medias.iter().zip(labels) {
                    kind_of.insert(r.xref.clone().unwrap_or_default(), l.map(str::to_string));
                }
            }
        }
        // Les liens web n'ont pas de dossier : un type à part.
        for r in &medias {
            let x = r.xref.clone().unwrap_or_default();
            if Media::new(r).web_url().is_some() && kind_of.get(&x).cloned().flatten().is_none() {
                if !types.is_empty() && !types.iter().any(|t| t == WEB_TYPE) {
                    types.push(WEB_TYPE.to_string());
                }
                if !types.is_empty() {
                    kind_of.insert(x, Some(WEB_TYPE.to_string()));
                }
            }
        }
        let mut index = MediaIndex::default();
        let mut other = false;
        for r in &medias {
            let x = r.xref.clone().unwrap_or_default();
            let m = Media::new(r);
            let ctx = contexts.get(&x).cloned().unwrap_or_default();
            let kind = match kind_of.get(&x).cloned().flatten() {
                Some(l) => types.iter().position(|t| *t == l),
                None => {
                    other = true;
                    Some(usize::MAX)
                }
            };
            let info = MediaInfo {
                kind,
                event: sort_key(&ctx.kind),
                event_label: ctx.kind.clone(),
                people: sort_key(&format!(
                    "{} {} {}",
                    ctx.surname,
                    ctx.given,
                    people.get(&x).map(|v| v.join(" ")).unwrap_or_default()
                )),
                linked: pointers.contains(&x),
                format: format_of(&m),
            };
            index.media.insert(x, info);
        }
        // « Autres » pour ce qui n'entre dans aucun type, seulement s'il existe d'autres types.
        let n = types.len();
        for m in index.media.values_mut() {
            if m.kind == Some(usize::MAX) {
                m.kind = (n > 0).then_some(n);
            }
        }
        if other && n > 0 {
            types.push(OTHER_TYPE.to_string());
        }
        index.types = types;
        index
    }

    /// Clés (identifiants, ou nom complet pour un lieu) des enregistrements de la
    /// zone qui correspondent à la requête. `media_type` restreint les médias à
    /// un type de [`MediaIndex::types`].
    pub fn search(
        &self,
        scope: Scope,
        q: &str,
        media: Option<&MediaIndex>,
        media_type: Option<&str>,
    ) -> HashSet<String> {
        match scope {
            Scope::Individuals => {
                let query = IndividualQuery::parse(q);
                self.records_of(RecordKind::Individual)
                    .map(Individual::new)
                    .filter(|i| query.matches(self, i))
                    .map(|i| i.xref().to_string())
                    .collect()
            }
            Scope::Sources => {
                let query = RecordQuery::parse(scope, q);
                let pointers = query.crits.iter().any(|c| matches!(c, Crit::Linked(_))).then(|| self.all_pointers());
                self.records_of(RecordKind::Source)
                    .filter(|r| self.source_matches(&query, r, pointers.as_ref()))
                    .filter_map(|r| r.xref.clone())
                    .collect()
            }
            Scope::Places => {
                let query = RecordQuery::parse(scope, q);
                self.places().into_iter().filter(|p| place_matches(&query, p)).map(|p| p.name).collect()
            }
            Scope::Media => {
                let query = RecordQuery::parse(scope, q);
                let owned;
                let index = match media {
                    Some(i) => i,
                    None => {
                        owned = self.media_index();
                        &owned
                    }
                };
                self.records_of(RecordKind::Media)
                    .filter(|r| {
                        let x = r.xref.as_deref().unwrap_or("");
                        let info = index.media.get(x);
                        media_type.is_none_or(|t| index.type_of(x) == Some(t)) && media_matches(&query, r, info)
                    })
                    .filter_map(|r| r.xref.clone())
                    .collect()
            }
        }
    }

    fn source_matches(&self, q: &RecordQuery, r: &Node, pointers: Option<&HashSet<String>>) -> bool {
        let s = Source::new(r);
        if !q.id_ok(s.xref()) {
            return false;
        }
        let content = SourceContent::read(r);
        if !q.words.is_empty() {
            let hay = format!(
                "{} {} {} {} {}",
                s.title(),
                s.author().unwrap_or_default(),
                s.publication().unwrap_or_default(),
                content.summary(),
                s.call_number().unwrap_or_default()
            );
            if !q.words_in(&hay) {
                return false;
            }
        }
        q.crits.iter().all(|c| match c {
            Crit::ActType(t) => content.events.iter().any(|e| {
                sort_key(e) == *t || event_label(e).is_some_and(|l| sort_key(l).starts_with(t.trim_end_matches('s')))
            }),
            Crit::Repository(name) => s
                .repository()
                .and_then(|x| self.get(x))
                .is_some_and(|n| sort_key(Repository::new(n).name()).contains(name.as_str())),
            Crit::HasMedia(on) => s.media_xrefs().is_empty() != *on,
            Crit::Linked(on) => pointers.is_some_and(|p| p.contains(s.xref())) == *on,
            _ => true,
        })
    }
}

fn place_matches(q: &RecordQuery, p: &Place) -> bool {
    if !q.words.is_empty() && !q.words_in(&p.name) {
        return false;
    }
    q.crits.iter().all(|c| match c {
        Crit::Id(_) => false,
        Crit::Within(j) => p.parts().iter().skip(1).any(|part| sort_key(part).contains(j.as_str())),
        Crit::HasCoordinates(on) => p.latitude.is_some() == *on,
        _ => true,
    })
}

fn media_matches(q: &RecordQuery, r: &Node, info: Option<&MediaInfo>) -> bool {
    let m = Media::new(r);
    if !q.id_ok(m.xref()) {
        return false;
    }
    if !q.words.is_empty() {
        let mut hay = m.title();
        for f in r.children.iter().filter(|c| c.tag == "FILE") {
            hay.push(' ');
            hay.push_str(f.val());
        }
        if let Some(l) = medium(r).and_then(medium_label) {
            hay.push(' ');
            hay.push_str(l);
        }
        if !q.words_in(&hay) {
            return false;
        }
    }
    q.crits.iter().all(|c| match c {
        Crit::Event(e) => info.is_some_and(|i| i.event.starts_with(e.trim_end_matches('s'))),
        Crit::Person(p) => info.is_some_and(|i| p.split_whitespace().all(|w| i.people.contains(w))),
        Crit::Format(f) => info.is_some_and(|i| i.format == Some(*f)),
        Crit::Linked(on) => info.is_some_and(|i| i.linked == *on),
        _ => true,
    })
}

// ----------------------------------------------------------------------
// Jetons et suggestions
// ----------------------------------------------------------------------

fn capitalize(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
        None => String::new(),
    }
}

fn years_label(r: &str) -> String {
    let r = r.trim();
    if let Some(y) = r.strip_prefix('<') {
        format!("avant {y}")
    } else if let Some(y) = r.strip_prefix('>') {
        format!("après {y}")
    } else {
        r.replace('-', "–")
    }
}

/// Libellé du jeton d'un critère complet, ou `None` si ce mot n'est pas un critère valide
/// de la zone (il reste alors du texte).
pub fn chip_label(scope: Scope, word: &str) -> Option<String> {
    if scope == Scope::Individuals {
        if sort_key(word) == "sosa" {
            return Some("Ascendants".into());
        }
        let (k, v) = split(word)?;
        let q = IndividualQuery::parse(word);
        if !q.unknown.is_empty() || !q.words.is_empty() {
            return None;
        }
        return Some(match k.as_str() {
            "ne" | "nee" | "naissance" | "n" => format!("Né {}", years_label(v)),
            "mort" | "morte" | "deces" | "d" => format!("Mort {}", years_label(v)),
            "vecu" | "vivant" | "en" => format!("Vivant en {v}"),
            "lieu" | "a" => format!("Lieu {v}"),
            "prof" | "profession" | "metier" => format!("Profession {v}"),
            "nom" => format!("Nom {v}"),
            "sexe" | "sex" => match q.sex {
                Some('M') => "Hommes".into(),
                Some('F') => "Femmes".into(),
                _ => "Sexe inconnu".into(),
            },
            "sans" => feature_label("Sans", v),
            "avec" => feature_label("Avec", v),
            "sosa" => format!("Sosa {}", years_label(v)),
            "id" => format!("ID {v}"),
            _ => return None,
        });
    }
    let (k, v) = split(word)?;
    let c = parse_crit(scope, &k, v)?;
    Some(match c {
        Crit::Id(x) => format!("ID {x}"),
        Crit::ActType(_) => format!("Actes de {}", v.to_lowercase()),
        Crit::Repository(_) => format!("Dépôt {v}"),
        Crit::HasMedia(true) => "Avec média".into(),
        Crit::HasMedia(false) => "Sans média".into(),
        Crit::Linked(true) if scope == Scope::Sources => "Citée".into(),
        Crit::Linked(false) if scope == Scope::Sources => "Non citée".into(),
        Crit::Linked(true) => "Rattaché".into(),
        Crit::Linked(false) => "Non rattaché".into(),
        Crit::Within(_) => format!("Dans {v}"),
        Crit::HasCoordinates(true) => "Avec coordonnées".into(),
        Crit::HasCoordinates(false) => "Sans coordonnées".into(),
        Crit::Event(_) => format!("Événement {}", capitalize(v)),
        Crit::Person(_) => format!("Lié à {v}"),
        Crit::Format(f) => f.label().into(),
    })
}

fn feature_label(prefix: &str, v: &str) -> String {
    let v = sort_key(v);
    let what = match v.trim_end_matches('s') {
        "source" => "source",
        "parent" => "parents",
        "date" => "date",
        "famille" | "conjoint" => "conjoint",
        "media" | "photo" | "image" => "média",
        "note" => "note",
        "mariage" => return if prefix == "Sans" { "Union sans mariage".into() } else { "Mariés".into() },
        other => return format!("{prefix} {other}"),
    };
    format!("{prefix} {what}")
}

/// Critères complets d'une requête : (plage, texte, libellé du jeton). Le dernier
/// mot n'en fait partie que s'il est suivi d'une espace (on est encore en train de le taper).
pub fn chips(scope: Scope, q: &str) -> Vec<(Range<usize>, String, String)> {
    let ended = q.ends_with(char::is_whitespace);
    let ws = words(q);
    let n = ws.len();
    ws.into_iter()
        .enumerate()
        .filter(|(i, _)| ended || *i + 1 < n)
        .filter_map(|(_, (r, w))| chip_label(scope, w).map(|l| (r, w.to_string(), l)))
        .collect()
}

/// Une proposition sous le champ de recherche.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Suggestion {
    /// Groupe affiché (« Lieu », « Nom », « Critères »…).
    pub group: &'static str,
    pub label: String,
    /// Critère à insérer à la place du mot en cours.
    pub insert: String,
}

fn sugg(group: &'static str, label: impl Into<String>, insert: impl Into<String>) -> Suggestion {
    Suggestion { group, label: label.into(), insert: insert.into() }
}

/// Critères sans valeur proposés dans chaque zone.
fn fixed(scope: Scope) -> Vec<Suggestion> {
    let list: &[(&str, &str)] = match scope {
        Scope::Individuals => &[
            ("Ascendants", "sosa"),
            ("Hommes", "sexe:h"),
            ("Femmes", "sexe:f"),
            ("Sans source", "sans:source"),
            ("Sans parents", "sans:parents"),
            ("Sans date", "sans:date"),
            ("Sans conjoint", "sans:famille"),
            ("Union sans mariage", "sans:mariage"),
            ("Avec média", "avec:média"),
            ("Sans média", "sans:média"),
            ("Avec note", "avec:note"),
        ],
        Scope::Sources => &[("Sans média", "sans:média"), ("Avec média", "avec:média"), ("Non citée", "sans:citation")],
        Scope::Places => &[("Sans coordonnées", "sans:coordonnées"), ("Avec coordonnées", "avec:coordonnées")],
        Scope::Media => &[
            ("Images", "format:image"),
            ("PDF", "format:pdf"),
            ("Liens web", "format:web"),
            ("Non rattaché", "sans:lien"),
        ],
    };
    list.iter().map(|(l, t)| sugg("Critères", *l, *t)).collect()
}

/// Ajoute `(valeur → nombre)` en gardant les plus fréquentes.
fn top(counts: HashMap<String, usize>, n: usize) -> Vec<String> {
    let mut v: Vec<(String, usize)> = counts.into_iter().collect();
    v.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| sort_key(&a.0).cmp(&sort_key(&b.0))));
    v.into_iter().take(n).map(|(k, _)| k).collect()
}

fn matches_word(value: &str, partial: &str) -> bool {
    let v = sort_key(value);
    v.starts_with(partial) || v.split([' ', '-', '\'']).any(|w| w.starts_with(partial))
}

impl Gedcom {
    /// Propositions pour le mot en cours `partial` (vide : les critères de la zone).
    /// Les valeurs viennent du fichier (lieux, noms, professions, types d'actes…).
    /// L'appelant compte les résultats de chacune et écarte celles qui n'en ont pas.
    pub fn suggest(&self, scope: Scope, partial: &str, media: Option<&MediaIndex>) -> Vec<Suggestion> {
        let p = sort_key(unquote(partial).trim());
        if p.contains(':') {
            return Vec::new();
        }
        let mut out: Vec<Suggestion> = Vec::new();
        // Une année ou une période : naissance, décès, en vie.
        if scope == Scope::Individuals && !p.is_empty() && YearRange::parse(&p).is_some() {
            let label = years_label(&p);
            out.push(sugg("Dates", format!("Né {label}"), format!("né:{p}")));
            out.push(sugg("Dates", format!("Mort {label}"), format!("mort:{p}")));
            if p.chars().all(|c| c.is_ascii_digit()) {
                out.push(sugg("Dates", format!("Vivant en {p}"), format!("vécu:{p}")));
            }
            return out;
        }
        if p.chars().count() >= 2 {
            match scope {
                Scope::Individuals => {
                    let mut places: HashMap<String, usize> = HashMap::new();
                    for pl in self.places() {
                        if let Some(first) = pl.parts().first()
                            && matches_word(first, &p)
                        {
                            *places.entry(first.to_string()).or_default() += pl.records.len();
                        }
                    }
                    for v in top(places, 3) {
                        out.push(sugg("Lieu", v.clone(), criterion("lieu", &v)));
                    }
                    let mut names: HashMap<String, usize> = HashMap::new();
                    let mut jobs: HashMap<String, usize> = HashMap::new();
                    for r in self.records_of(RecordKind::Individual) {
                        let s = Individual::new(r).surname();
                        if !s.is_empty() && matches_word(&s, &p) {
                            *names.entry(s).or_default() += 1;
                        }
                        for o in r.children_with("OCCU") {
                            let v = o.val().trim();
                            if !v.is_empty() && matches_word(v, &p) {
                                *jobs.entry(v.to_lowercase()).or_default() += 1;
                            }
                        }
                    }
                    for v in top(names, 3) {
                        out.push(sugg("Nom", v.clone(), criterion("nom", &v)));
                    }
                    for v in top(jobs, 2) {
                        out.push(sugg("Profession", v.clone(), criterion("prof", &v)));
                    }
                }
                Scope::Sources => {
                    let mut kinds: HashMap<String, usize> = HashMap::new();
                    for r in self.records_of(RecordKind::Source) {
                        for e in SourceContent::read(r).events {
                            let l = event_label(&e).unwrap_or(&e).to_string();
                            if matches_word(&l, &p) {
                                *kinds.entry(l).or_default() += 1;
                            }
                        }
                    }
                    for v in top(kinds, 3) {
                        let lower = v.to_lowercase();
                        out.push(sugg("Type d'acte", format!("Actes de {lower}"), criterion("type", &lower)));
                    }
                    let mut repos: HashMap<String, usize> = HashMap::new();
                    for r in self.records_of(RecordKind::Repository) {
                        let n = Repository::new(r).name().trim().to_string();
                        if !n.is_empty() && matches_word(&n, &p) {
                            *repos.entry(n).or_default() += 1;
                        }
                    }
                    for v in top(repos, 3) {
                        out.push(sugg("Dépôt", v.clone(), criterion("dépôt", &v)));
                    }
                }
                Scope::Places => {
                    let mut within: HashMap<String, usize> = HashMap::new();
                    for pl in self.places() {
                        for part in pl.parts().into_iter().skip(1) {
                            if matches_word(part, &p) && part.chars().any(|c| !c.is_ascii_digit()) {
                                *within.entry(part.to_string()).or_default() += 1;
                            }
                        }
                    }
                    for v in top(within, 4) {
                        out.push(sugg("Juridiction", v.clone(), criterion("dans", &v)));
                    }
                }
                Scope::Media => {
                    if let Some(index) = media {
                        let mut events: HashMap<String, usize> = HashMap::new();
                        for m in index.media.values() {
                            if !m.event_label.is_empty() && matches_word(&m.event_label, &p) {
                                *events.entry(m.event_label.clone()).or_default() += 1;
                            }
                        }
                        for v in top(events, 3) {
                            out.push(sugg("Événement", v.clone(), criterion("événement", &v.to_lowercase())));
                        }
                    }
                    let mut names: HashMap<String, usize> = HashMap::new();
                    for r in self.records_of(RecordKind::Individual) {
                        let s = Individual::new(r).surname();
                        if !s.is_empty() && matches_word(&s, &p) {
                            *names.entry(s).or_default() += 1;
                        }
                    }
                    for v in top(names, 3) {
                        out.push(sugg("Personne", v.clone(), criterion("pour", &v)));
                    }
                }
            }
        }
        // Les groupes dont une valeur commence par le mot tapé passent devant
        // (« guy » : le nom GUYOT avant le lieu Villeneuve-la-Guyard).
        let starts = |g: &str| out.iter().any(|s| s.group == g && sort_key(&s.label).starts_with(&p));
        let mut groups: Vec<&'static str> = Vec::new();
        for s in &out {
            if !groups.contains(&s.group) {
                groups.push(s.group);
            }
        }
        groups.sort_by_key(|g| !starts(g));
        let mut ordered: Vec<Suggestion> = Vec::new();
        for g in groups {
            ordered.extend(out.iter().filter(|s| s.group == g).cloned());
        }
        ordered.extend(fixed(scope).into_iter().filter(|s| p.is_empty() || matches_word(&s.label, &p)));
        ordered
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const GED: &str = "0 HEAD\n1 GEDC\n2 VERS 7.0\n\
        0 @I1@ INDI\n1 NAME Paul /MARCHAND/\n1 BIRT\n2 DATE 1912\n2 PLAC Saint-Lô, Manche\n2 SOUR @S1@\n1 OBJE @M2@\n\
        0 @R1@ REPO\n1 NAME AD de la Manche\n\
        0 @S1@ SOUR\n1 TITL Registre des naissances\n1 DATA\n2 EVEN BIRT\n1 REPO @R1@\n1 OBJE @M1@\n\
        0 @S2@ SOUR\n1 TITL Recensement 1906\n1 DATA\n2 EVEN CENS\n\
        0 @M1@ OBJE\n1 FILE Actes/naissance-paul.jpg\n2 FORM image/jpeg\n\
        0 @M2@ OBJE\n1 FILE Portraits/paul.jpg\n2 FORM image/jpeg\n3 MEDI PHOTO\n\
        0 @M3@ OBJE\n1 FILE divers/lettre.pdf\n2 FORM application/pdf\n\
        0 @M4@ OBJE\n1 FILE https://example.org/fiche\n2 FORM text/html\n\
        0 TRLR\n";

    fn ged() -> Gedcom {
        Gedcom::parse(GED).unwrap()
    }

    #[test]
    fn words_keep_quoted_values() {
        let w: Vec<&str> = words(r#"guyot lieu:"Bar sur Aube"  sans:source"#).into_iter().map(|(_, w)| w).collect();
        assert_eq!(w, ["guyot", r#"lieu:"Bar sur Aube""#, "sans:source"]);
        assert_eq!(unquote(r#""Bar sur Aube""#), "Bar sur Aube");
        assert_eq!(criterion("lieu", "Bar sur Aube"), r#"lieu:"Bar sur Aube""#);
        assert_eq!(criterion("lieu", "Troyes"), "lieu:Troyes");
    }

    #[test]
    fn chips_only_for_finished_criteria() {
        let c = chips(Scope::Individuals, "né:1800-1850 sans:source guyot lieu:tro");
        let labels: Vec<&str> = c.iter().map(|c| c.2.as_str()).collect();
        assert_eq!(labels, ["Né 1800–1850", "Sans source"]);
        let c = chips(Scope::Individuals, "lieu:troyes ");
        assert_eq!(c[0].2, "Lieu troyes");
        assert!(chips(Scope::Individuals, "sans:truc ").is_empty());
        assert_eq!(chips(Scope::Media, "format:pdf ")[0].2, "PDF");
        assert!(chips(Scope::Places, "format:pdf ").is_empty());
    }

    #[test]
    fn sources_by_type_repository_media() {
        let g = ged();
        let s = |q: &str| {
            let mut v: Vec<String> = g.search(Scope::Sources, q, None, None).into_iter().collect();
            v.sort();
            v
        };
        assert_eq!(s("type:naissance"), ["S1"]);
        assert_eq!(s("type:recensement"), ["S2"]);
        assert_eq!(s(r#"dépôt:"AD de la Manche""#), ["S1"]);
        assert_eq!(s("sans:média"), ["S2"]);
        assert_eq!(s("sans:citation"), ["S2"]);
        assert_eq!(s("registre"), ["S1"]);
    }

    #[test]
    fn places_by_jurisdiction_and_coordinates() {
        let g = ged();
        assert_eq!(g.search(Scope::Places, "dans:manche", None, None).len(), 1);
        assert!(g.search(Scope::Places, "dans:saint", None, None).is_empty());
        assert_eq!(g.search(Scope::Places, "sans:coordonnées", None, None).len(), 1);
    }

    #[test]
    fn media_types_from_file_folders_then_medium() {
        let g = ged();
        let index = g.media_index();
        // Pas de sous-dossiers réglés : les dossiers des fichiers, les plus remplis d'abord.
        assert_eq!(index.types, ["Actes", "divers", "Portraits", WEB_TYPE]);
        assert_eq!(index.type_of("M4"), Some(WEB_TYPE));
        assert_eq!(index.type_of("M2"), Some("Portraits"));
        let actes = g.search(Scope::Media, "", Some(&index), Some("Actes"));
        assert_eq!(actes.len(), 1);
        // Fichiers tous à la racine : le support GEDCOM, puis « Autres ».
        let flat = Gedcom::parse(&GED.replace("Actes/", "").replace("Portraits/", "").replace("divers/", "")).unwrap();
        let index = flat.media_index();
        assert_eq!(index.types, ["Photo", WEB_TYPE, OTHER_TYPE]);
        assert_eq!(index.type_of("M2"), Some("Photo"));
        assert_eq!(index.type_of("M3"), Some(OTHER_TYPE));
        assert_eq!(g.search(Scope::Media, "format:pdf", Some(&index), None).len(), 1);
        assert_eq!(g.search(Scope::Media, "sans:lien", Some(&index), None).len(), 2);
        assert_eq!(g.search(Scope::Media, "pour:marchand", Some(&index), None).len(), 2);
        assert_eq!(g.search(Scope::Media, "événement:naissance", Some(&index), None).len(), 1);
    }

    #[test]
    fn media_types_from_folders() {
        let mut g = ged();
        let naming = crate::MediaNaming {
            root: String::new(),
            folders: vec![
                crate::MediaFolder { dir: "Actes".into(), types: vec!["BIRT".into()], template: String::new() },
                crate::MediaFolder { dir: "Portraits".into(), types: vec!["INDI".into()], template: String::new() },
            ],
        };
        g.set_media_naming(&naming);
        let index = g.media_index();
        assert_eq!(index.types, ["Actes", "Portraits", WEB_TYPE, OTHER_TYPE]);
        assert_eq!(index.type_of("M1"), Some("Actes"));
        assert_eq!(index.type_of("M2"), Some("Portraits"));
        assert_eq!(index.type_of("M3"), Some(OTHER_TYPE));
    }

    #[test]
    fn suggestions_come_from_the_file() {
        let g = ged();
        let s = g.suggest(Scope::Individuals, "sai", None);
        assert!(s.iter().any(|s| s.group == "Lieu" && s.insert == "lieu:Saint-Lô"));
        let s = g.suggest(Scope::Individuals, "march", None);
        assert_eq!(s[0].insert, "nom:MARCHAND");
        let s = g.suggest(Scope::Individuals, "1850", None);
        assert_eq!(s[0].insert, "né:1850");
        let s = g.suggest(Scope::Sources, "nais", None);
        assert!(s.iter().any(|s| s.insert == "type:naissance"));
        // Champ vide : les critères de la zone.
        assert!(g.suggest(Scope::Places, "", None).iter().all(|s| s.group == "Critères"));
    }
}
