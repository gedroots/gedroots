//! Rangement et nommage des fichiers médias (Réglages du fichier › Médias).
//!
//! Le dossier et le modèle de nom sont propres au fichier, donc rangés dans
//! l'en-tête par l'extension `_GR_MEDIA` (voir docs/extensions.md) :
//!
//! ```text
//! 1 _GR_MEDIA Médias
//! 2 _GR_NAME {type}/{annee} {nom} {prenom}
//! ```
//!
//! Le modèle reçoit les données de ce que le média illustre : l'événement cité
//! (type, date, lieu), la personne, la source et la référence (`PAGE`).

use crate::date::Date;
use crate::document::Gedcom;
use crate::model::{Event, Family, Individual, event_label, short_place};
use crate::node::Node;

pub const MEDIA_TAG: &str = "_GR_MEDIA";
pub const MEDIA_URI: &str = "https://github.com/antoninguyot/gedroots/blob/main/docs/extensions.md#_gr_media";
/// Sous `_GR_MEDIA` : modèle de nom des fichiers.
const NAME_TAG: &str = "_GR_NAME";

/// Variables du modèle, dans l'ordre où le panneau les propose.
pub const MEDIA_TOKENS: &[&str] = &["type", "date", "annee", "lieu", "nom", "prenom", "source", "page"];

/// Dossier (relatif au .ged) et modèle de nom des fichiers médias.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MediaNaming {
    pub dir: String,
    pub template: String,
}

impl MediaNaming {
    /// Rien de réglé : GedRoots garde les fichiers là où ils sont.
    pub fn is_empty(&self) -> bool {
        self.dir.trim().is_empty() && self.template.trim().is_empty()
    }
}

/// Ce que le média illustre, pour remplir le modèle.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MediaContext {
    /// « Naissance », « Mariage », ou le `TYPE` d'un `EVEN`.
    pub kind: String,
    /// Date GEDCOM brute (`14 MAR 1912`).
    pub date: String,
    pub place: String,
    pub surname: String,
    pub given: String,
    pub source: String,
    pub page: String,
}

impl MediaContext {
    /// Complète les champs vides avec ceux de `other`.
    fn fill(&mut self, other: MediaContext) {
        let pairs = [
            (&mut self.kind, other.kind),
            (&mut self.date, other.date),
            (&mut self.place, other.place),
            (&mut self.surname, other.surname),
            (&mut self.given, other.given),
            (&mut self.source, other.source),
            (&mut self.page, other.page),
        ];
        for (a, b) in pairs {
            if a.trim().is_empty() {
                *a = b;
            }
        }
    }

    fn value(&self, token: &str) -> String {
        let date = Date::parse(&self.date);
        match token {
            "type" => self.kind.clone(),
            "date" => match date.primary() {
                Some(d) if d.calendar == crate::date::Calendar::Gregorian && d.year.is_some() => {
                    let mut s = format!("{:04}", d.year.unwrap_or(0));
                    if let Some(m) = d.month {
                        s.push_str(&format!("-{m:02}"));
                        if let Some(day) = d.day {
                            s.push_str(&format!("-{day:02}"));
                        }
                    }
                    s
                }
                _ => date.year().map(|y| y.to_string()).unwrap_or_default(),
            },
            "annee" => date.year().map(|y| y.to_string()).unwrap_or_default(),
            "lieu" => {
                let s = short_place(&self.place);
                s.split(" (").next().unwrap_or("").to_string()
            }
            "nom" => self.surname.clone(),
            "prenom" => self.given.clone(),
            "source" => self.source.clone(),
            "page" => self.page.clone(),
            _ => String::new(),
        }
    }
}

/// Caractères interdits dans un nom de fichier (Windows compris).
fn clean(v: &str) -> String {
    v.chars().map(|c| if matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|') { '-' } else { c }).collect()
}

/// Remplace les `{variable}` du modèle ; le texte hors accolades est gardé, `/` crée des sous-dossiers.
fn fill_template(template: &str, ctx: &MediaContext) -> String {
    let mut out = String::new();
    let mut rest = template;
    while let Some(i) = rest.find('{') {
        out.push_str(&rest[..i]);
        match rest[i..].find('}') {
            Some(j) => {
                out.push_str(&clean(&ctx.value(rest[i + 1..i + j].trim())));
                rest = &rest[i + j + 1..];
            }
            None => {
                out.push_str(&rest[i..]);
                rest = "";
            }
        }
    }
    out.push_str(rest);
    out.split('/')
        .map(|seg| {
            let seg = seg.split_whitespace().collect::<Vec<_>>().join(" ");
            seg.trim_matches(|c: char| c == '-' || c == '_' || c == ',' || c == '.' || c.is_whitespace()).to_string()
        })
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("/")
}

/// Chemin (relatif au .ged) d'un fichier selon le réglage : dossier, modèle,
/// numéro de page pour un média à plusieurs fichiers, extension d'origine.
/// Sans modèle, ou si le modèle ne donne rien, le nom d'origine est gardé.
pub fn media_path(naming: &MediaNaming, ctx: &MediaContext, original: &str, page: Option<usize>) -> String {
    let file = original.rsplit(['/', '\\']).next().unwrap_or(original);
    let (stem, ext) = match file.rfind('.') {
        Some(i) if i > 0 => (&file[..i], file[i + 1..].to_ascii_lowercase()),
        _ => (file, String::new()),
    };
    let mut name = fill_template(&naming.template, ctx);
    if name.is_empty() {
        name = stem.to_string();
    }
    if let Some(n) = page {
        name.push_str(&format!(" p{n}"));
    }
    if !ext.is_empty() {
        name.push('.');
        name.push_str(&ext);
    }
    let dir = naming.dir.trim().trim_matches('/');
    if dir.is_empty() { name } else { format!("{dir}/{name}") }
}

impl Gedcom {
    pub fn media_naming(&self) -> MediaNaming {
        let n = self.header.child(MEDIA_TAG);
        MediaNaming {
            dir: n.map(|n| n.val().to_string()).unwrap_or_default(),
            template: n.and_then(|n| n.child_val(NAME_TAG)).unwrap_or("").to_string(),
        }
    }

    /// Enregistre le réglage. Renvoie `false` si rien n'a changé.
    pub fn set_media_naming(&mut self, m: &MediaNaming) -> bool {
        let m =
            MediaNaming { dir: m.dir.trim().trim_matches('/').to_string(), template: m.template.trim().to_string() };
        if m == self.media_naming() {
            return false;
        }
        self.header.children.retain(|c| c.tag != MEDIA_TAG);
        if m.is_empty() {
            self.undeclare_extension(MEDIA_TAG);
            self.undeclare_extension(NAME_TAG);
            return true;
        }
        let mut n = Node::new(MEDIA_TAG);
        n.value = (!m.dir.is_empty()).then(|| m.dir.clone());
        if !m.template.is_empty() {
            n.children.push(Node::with_value(NAME_TAG, &m.template));
            self.declare_extension(NAME_TAG, MEDIA_URI);
        } else {
            self.undeclare_extension(NAME_TAG);
        }
        self.declare_extension(MEDIA_TAG, MEDIA_URI);
        self.header.insert_child(n);
        true
    }

    /// Ce que le média illustre : d'abord un événement dont une citation porte le
    /// média (acte saisi), puis la source qui le porte, puis la personne ou la
    /// famille à laquelle il est rattaché.
    pub fn media_context(&self, media: &str) -> MediaContext {
        let target = format!("@{media}@");
        let has_media = |n: &Node| n.children.iter().any(|c| c.tag == "OBJE" && c.val() == target);
        let mut ctx = MediaContext::default();
        let names = |r: &Node| -> (String, String) {
            match r.tag.as_str() {
                "INDI" => {
                    let i = Individual::new(r);
                    (i.surname(), i.given())
                }
                "FAM" => {
                    let f = Family::new(r);
                    f.husband()
                        .or(f.wife())
                        .and_then(|x| self.get(x))
                        .map(|p| {
                            let i = Individual::new(p);
                            (i.surname(), i.given())
                        })
                        .unwrap_or_default()
                }
                _ => Default::default(),
            }
        };
        let source_title = |cit: &Node| {
            cit.pointer().and_then(|s| self.get(s)).and_then(|s| s.child_val("TITL")).unwrap_or("").to_string()
        };
        // 1. Citation d'un événement : qui porte le média, sinon qui cite une source qui le porte.
        let carriers: Vec<String> = self
            .records
            .iter()
            .filter(|r| r.tag == "SOUR" && has_media(r))
            .filter_map(|r| r.xref.as_ref().map(|x| format!("@{x}@")))
            .collect();
        let cites = |c: &Node, direct: bool| {
            c.tag == "SOUR" && if direct { has_media(c) } else { carriers.iter().any(|x| c.val() == x) }
        };
        'outer: for direct in [true, false] {
            for r in self.records.iter().filter(|r| r.tag == "INDI" || r.tag == "FAM") {
                for e in &r.children {
                    let Some(cit) = e.children.iter().find(|c| cites(c, direct)) else { continue };
                    let ev = Event { node: e };
                    let (surname, given) = names(r);
                    ctx.fill(MediaContext {
                        kind: ev.label(),
                        date: ev.date_raw().unwrap_or("").to_string(),
                        place: ev.place().unwrap_or("").to_string(),
                        surname,
                        given,
                        source: source_title(cit),
                        page: cit.child_val("PAGE").unwrap_or("").to_string(),
                    });
                    break 'outer;
                }
            }
        }
        // 2. Source qui porte le média : son contenu (`DATA.EVEN`) quand il désigne un
        //    seul type d'acte, une date précise plutôt qu'une période.
        if let Some(s) = self.records.iter().find(|r| r.tag == "SOUR" && has_media(r)) {
            let content = crate::kinds::SourceContent::read(s);
            let kind = match content.events.as_slice() {
                [t] => event_label(t).unwrap_or(t).to_string(),
                _ => String::new(),
            };
            let precise = !content.period.split_whitespace().any(|w| matches!(w, "FROM" | "TO" | "BET" | "AND"));
            ctx.fill(MediaContext {
                kind,
                date: if precise { content.period } else { String::new() },
                place: content.place,
                source: s.child_val("TITL").unwrap_or("").to_string(),
                ..Default::default()
            });
        }
        // 3. Personne ou famille liée directement.
        if let Some(r) = self.records.iter().find(|r| (r.tag == "INDI" || r.tag == "FAM") && has_media(r)) {
            let (surname, given) = names(r);
            ctx.fill(MediaContext { surname, given, ..Default::default() });
        }
        ctx
    }

    /// Chemins proposés pour les fichiers d'un média (un par `FILE`, liens web exclus) :
    /// `(position du FILE, chemin actuel, nouveau chemin)`.
    pub fn media_renames(&self, media: &str) -> Vec<(usize, String, String)> {
        let naming = self.media_naming();
        let Some(m) = self.get(media) else { return Vec::new() };
        let ctx = self.media_context(media);
        let files: Vec<&Node> = m.children_with("FILE").collect();
        let multi = files.len() > 1;
        files
            .iter()
            .enumerate()
            .filter(|(_, f)| !f.val().is_empty() && !crate::weblink::is_web_url(f.val()))
            .map(|(i, f)| (i, f.val().to_string(), media_path(&naming, &ctx, f.val(), multi.then_some(i + 1))))
            .collect()
    }

    /// Remplace le chemin du `FILE` n° `index` d'un média, s'il vaut encore `old`
    /// (sans toucher à la date de modification). Renvoie `true` s'il a changé.
    pub fn set_media_file(&mut self, media: &str, index: usize, old: &str, path: &str) -> bool {
        let matches =
            |g: &Gedcom| g.get(media).and_then(|m| m.children_with("FILE").nth(index)).is_some_and(|f| f.val() == old);
        if !matches(self) {
            return false;
        }
        if let Some(f) = self.get_mut(media).and_then(|m| m.children.iter_mut().filter(|c| c.tag == "FILE").nth(index))
        {
            f.value = Some(path.to_string());
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const G: &str = "0 HEAD\n1 GEDC\n2 VERS 7.0\n\
0 @I1@ INDI\n1 NAME Jean /GUYOT/\n1 BIRT\n2 DATE 14 MAR 1856\n2 PLAC , Troyes, 10000, Aube, Grand Est, France\n\
2 SOUR @S1@\n3 PAGE Acte n° 12\n3 OBJE @M1@\n1 OBJE @M2@\n\
0 @S1@ SOUR\n1 TITL État civil de Troyes\n1 OBJE @M1@\n\
0 @M1@ OBJE\n1 FILE /Users/x/Downloads/scan 001.JPG\n2 FORM image/jpeg\n1 FILE scan2.jpg\n2 FORM image/jpeg\n\
0 @M2@ OBJE\n1 FILE Portraits/jean.png\n2 FORM image/png\n0 TRLR\n";

    fn naming(dir: &str, t: &str) -> MediaNaming {
        MediaNaming { dir: dir.into(), template: t.into() }
    }

    #[test]
    fn stores_naming_in_header() {
        let mut g = Gedcom::parse(G).unwrap();
        assert!(g.media_naming().is_empty());
        assert!(g.set_media_naming(&naming("Médias/", "{type}/{annee} {nom} {prenom}")));
        let out = g.to_gedcom_string();
        assert!(out.contains("1 _GR_MEDIA Médias\n2 _GR_NAME {type}/{annee} {nom} {prenom}\n"));
        assert!(out.contains("2 TAG _GR_MEDIA https://"));
        assert_eq!(Gedcom::parse(&out).unwrap().media_naming(), naming("Médias", "{type}/{annee} {nom} {prenom}"));
        assert!(g.set_media_naming(&MediaNaming::default()));
        assert!(g.header.child(MEDIA_TAG).is_none());
        assert!(g.header.child("SCHMA").is_none());
    }

    #[test]
    fn context_from_citation_and_person() {
        let g = Gedcom::parse(G).unwrap();
        let c = g.media_context("M1");
        assert_eq!(
            (c.kind.as_str(), c.surname.as_str(), c.given.as_str(), c.source.as_str(), c.page.as_str()),
            ("Naissance", "GUYOT", "Jean", "État civil de Troyes", "Acte n° 12")
        );
        let c = g.media_context("M2");
        assert_eq!((c.kind.as_str(), c.surname.as_str()), ("", "GUYOT"));
    }

    #[test]
    fn renders_paths() {
        let mut g = Gedcom::parse(G).unwrap();
        g.set_media_naming(&naming("Médias", "{type}/{date} {nom} {prenom} - {lieu} - {page}"));
        let r = g.media_renames("M1");
        assert_eq!(r[0].2, "Médias/Naissance/1856-03-14 GUYOT Jean - Troyes - Acte n° 12 p1.jpg");
        assert_eq!(r[1].2, "Médias/Naissance/1856-03-14 GUYOT Jean - Troyes - Acte n° 12 p2.jpg");
        // Données manquantes : pas de séparateurs orphelins ; sans modèle, nom d'origine.
        g.set_media_naming(&naming("", "{type}/{annee} {nom} {prenom} - {page}"));
        assert_eq!(g.media_renames("M2")[0].2, "GUYOT Jean.png");
        g.set_media_naming(&naming("Photos", ""));
        assert_eq!(g.media_renames("M2")[0].2, "Photos/jean.png");
        // Caractères interdits remplacés.
        let ctx = MediaContext { source: "AD 10 : 4E/123".into(), ..Default::default() };
        assert_eq!(media_path(&naming("", "{source}"), &ctx, "a.pdf", None), "AD 10 - 4E-123.pdf");
    }
}
