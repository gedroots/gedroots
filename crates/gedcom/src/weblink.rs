//! Liens web (pages d'archives en ligne, fiches, sites…) stockés en GEDCOM 7 standard.
//!
//! GEDCOM 7 n'a pas de balise « URL » générique sur les individus, familles et
//! sources. La forme standard est un **lien multimédia** vers un enregistrement
//! `OBJE` dont le `FILE` est une URL (le type `FilePath` accepte les URL) :
//!
//! ```text
//! 0 @S1@ SOUR
//! 1 OBJE @M7@
//! 0 @M7@ OBJE
//! 1 FILE https://www.archives43.fr/ark:47539/s0051b032d062eee
//! 2 FORM text/html
//! 2 TITL Registre paroissial de Retournac
//! ```
//!
//! Les dépôts (`REPO`) n'acceptent pas de lien multimédia mais ont la balise
//! standard `WWW` (jusqu'à trois) : c'est elle qu'on utilise pour eux.
//!
//! La balise non standard `_URL` (écrite par plusieurs logiciels) est encore
//! lue, et [`Gedcom::migrate_url_tags`] la convertit (contrôle de l'assistant
//! d'import, voir [`crate::import`]).

use crate::document::{Gedcom, RecordKind};
use crate::model::Media;
use crate::node::Node;
use crate::schema;

/// Balise d'extension historique pour une adresse web.
pub const LEGACY_TAG: &str = "_URL";

/// Format (`FILE.FORM`) d'une page web.
pub const WEB_FORM: &str = "text/html";

/// Où est stocké un lien web d'un enregistrement.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WebLinkKind {
    /// Lien `OBJE` vers un média dont le `FILE` est une URL (forme standard).
    Media(String),
    /// `WWW` standard (dépôts) : rang parmi les `WWW` de l'enregistrement.
    Www(usize),
    /// `_URL` historique : rang parmi les `_URL` de l'enregistrement.
    Legacy(usize),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WebLink {
    pub kind: WebLinkKind,
    pub url: String,
    pub title: Option<String>,
}

impl WebLink {
    /// Adresse à ouvrir dans le navigateur (avec `https://` si le fichier l'omet).
    pub fn href(&self) -> String {
        normalize_url(&self.url).unwrap_or_else(|_| self.url.clone())
    }

    /// Titre, ou à défaut le nom du site (« www.archives43.fr »).
    pub fn label(&self) -> String {
        self.title.clone().filter(|t| !t.trim().is_empty()).unwrap_or_else(|| host(&self.url).to_string())
    }

    pub fn is_legacy(&self) -> bool {
        matches!(self.kind, WebLinkKind::Legacy(_))
    }
}

/// L'adresse est-elle une URL web (`http://`, `https://`) ?
pub fn is_web_url(s: &str) -> bool {
    let l = s.trim_start().get(..8).unwrap_or(s).to_ascii_lowercase();
    l.starts_with("http://") || l.starts_with("https://")
}

/// Nom du site : « https://www.archives43.fr/ark:… » → « www.archives43.fr ».
pub fn host(url: &str) -> &str {
    let u = url.trim();
    let rest = u.split_once("://").map(|(_, r)| r).unwrap_or(u);
    let end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let h = &rest[..end];
    h.rsplit_once('@').map(|(_, h)| h).unwrap_or(h)
}

/// Vérifie et complète une adresse saisie : « archives43.fr » → « https://archives43.fr ».
pub fn normalize_url(input: &str) -> Result<String, String> {
    let s = input.trim();
    if s.is_empty() {
        return Err(rust_i18n::t!("error.web.empty").into());
    }
    if s.chars().any(char::is_whitespace) {
        return Err(rust_i18n::t!("error.web.space").into());
    }
    let url = if is_web_url(s) {
        s.to_string()
    } else if s.contains("://") {
        return Err(rust_i18n::t!("error.web.scheme").into());
    } else {
        format!("https://{s}")
    };
    let h = host(&url);
    if h.is_empty() || !h.contains('.') && !h.starts_with("localhost") {
        return Err(rust_i18n::t!("error.web.invalid").into());
    }
    Ok(url)
}

impl<'a> Media<'a> {
    /// URL du média, si son fichier est une page ou ressource web.
    pub fn web_url(&self) -> Option<&'a str> {
        self.file().filter(|f| is_web_url(f))
    }

    pub fn is_web(&self) -> bool {
        self.web_url().is_some()
    }
}

/// `_URL` dont la valeur est une adresse web valide (avec ou sans `https://`).
fn is_convertible(n: &Node) -> bool {
    n.tag == LEGACY_TAG && normalize_url(n.val()).is_ok()
}

/// Le type d'enregistrement accepte-t-il des liens multimédia (`OBJE`) ?
fn accepts_media(tag: &str) -> bool {
    schema::record(tag).is_some_and(|d| d.sub("OBJE").is_some())
}

impl Gedcom {
    /// Liens web d'un enregistrement : médias web liés, `WWW`, puis `_URL` historiques.
    pub fn web_links(&self, record: &str) -> Vec<WebLink> {
        let Some(n) = self.get(record) else { return Vec::new() };
        let mut out = Vec::new();
        for p in n.children_with("OBJE").filter_map(|o| o.pointer()) {
            if let Some(m) = self.get(p).filter(|m| m.tag == "OBJE").map(Media::new)
                && let Some(url) = m.web_url()
                && !out.iter().any(|l: &WebLink| l.kind == WebLinkKind::Media(p.to_string()))
            {
                let title = m.node.child("FILE").and_then(|f| f.child_val("TITL")).or_else(|| m.node.child_val("TITL"));
                out.push(WebLink {
                    kind: WebLinkKind::Media(p.to_string()),
                    url: url.to_string(),
                    title: title.map(str::to_string),
                });
            }
        }
        for (i, w) in n.children_with("WWW").enumerate().filter(|(_, w)| !w.val().is_empty()) {
            out.push(WebLink { kind: WebLinkKind::Www(i), url: w.val().to_string(), title: None });
        }
        for (i, w) in n.children_with(LEGACY_TAG).enumerate().filter(|(_, w)| !w.val().is_empty()) {
            out.push(WebLink { kind: WebLinkKind::Legacy(i), url: w.val().to_string(), title: None });
        }
        out
    }

    /// Peut-on ajouter un lien web à cet enregistrement ?
    pub fn can_have_web_links(&self, record: &str) -> bool {
        self.get(record).is_some_and(|n| accepts_media(&n.tag) || n.tag == "REPO")
    }

    /// Média web existant pour cette URL, ou nouveau média `FILE <url>` / `FORM text/html`.
    pub fn web_media_for(&mut self, url: &str, title: &str) -> String {
        if let Some(x) = self.media_by_file(url) {
            let node = self.get_mut(&x).unwrap();
            let f = node.ensure_child("FILE");
            if f.child_val("TITL").is_none_or(str::is_empty) && !title.trim().is_empty() {
                f.set_child_val("TITL", title.trim());
                node.touch();
            }
            return x;
        }
        let x = self.create(RecordKind::Media);
        let f = self.get_mut(&x).unwrap().ensure_child("FILE");
        f.value = Some(url.to_string());
        f.set_child_val("FORM", WEB_FORM);
        f.set_child_val("TITL", title.trim());
        x
    }

    /// Ajoute un lien web : lien multimédia (`OBJE`), ou `WWW` pour un dépôt.
    pub fn add_web_link(&mut self, record: &str, url: &str, title: &str) -> Result<(), String> {
        let url = normalize_url(url)?;
        let tag = self.get(record).ok_or_else(|| rust_i18n::t!("error.record_not_found"))?.tag.clone();
        if tag == "REPO" {
            let n = self.get_mut(record).unwrap();
            if n.children_with("WWW").any(|w| w.val() == url) {
                return Ok(());
            }
            if n.children_with("WWW").count() >= 3 {
                return Err(rust_i18n::t!("error.web.repository_max").into());
            }
            n.insert_child(Node::with_value("WWW", url));
            n.touch();
            return Ok(());
        }
        if !accepts_media(&tag) {
            return Err(rust_i18n::t!("error.web.not_allowed").into());
        }
        let media = self.web_media_for(&url, title);
        let p = format!("@{media}@");
        let n = self.get_mut(record).unwrap();
        if !n.children.iter().any(|c| c.tag == "OBJE" && c.val() == p) {
            n.insert_child(Node::with_value("OBJE", p));
            n.touch();
        }
        Ok(())
    }

    /// Modifie un lien web. Un `_URL` historique modifié devient un lien multimédia.
    pub fn update_web_link(&mut self, record: &str, kind: &WebLinkKind, url: &str, title: &str) -> Result<(), String> {
        let url = normalize_url(url)?;
        match kind {
            WebLinkKind::Media(m) => {
                let node = self
                    .get_mut(m)
                    .filter(|n| n.tag == "OBJE")
                    .ok_or_else(|| rust_i18n::t!("error.media_not_found"))?;
                let f = node.ensure_child("FILE");
                f.value = Some(url);
                f.set_child_val("FORM", WEB_FORM);
                f.set_child_val("TITL", title.trim());
                node.touch();
                Ok(())
            }
            WebLinkKind::Www(i) => {
                let n = self.get_mut(record).ok_or_else(|| rust_i18n::t!("error.record_not_found"))?;
                let w = n
                    .children
                    .iter_mut()
                    .filter(|c| c.tag == "WWW")
                    .nth(*i)
                    .ok_or_else(|| rust_i18n::t!("error.address_not_found"))?;
                w.value = Some(url);
                n.touch();
                Ok(())
            }
            WebLinkKind::Legacy(_) => {
                self.remove_web_link(record, kind);
                self.add_web_link(record, &url, title)
            }
        }
    }

    /// Retire un lien web. Un média web qui n'est plus utilisé nulle part est supprimé.
    pub fn remove_web_link(&mut self, record: &str, kind: &WebLinkKind) {
        let nth_removed = |n: &mut Node, tag: &str, i: usize| {
            let mut k = 0;
            n.children.retain(|c| {
                if c.tag != tag {
                    return true;
                }
                k += 1;
                k - 1 != i
            });
            n.touch();
        };
        match kind {
            WebLinkKind::Media(m) => {
                let p = format!("@{m}@");
                if let Some(n) = self.get_mut(record) {
                    n.children.retain(|c| !(c.tag == "OBJE" && c.val() == p));
                    n.touch();
                }
                let orphan_web =
                    self.get(m).map(Media::new).is_some_and(|x| x.is_web()) && self.referrers(m).is_empty();
                if orphan_web {
                    self.delete(m);
                }
            }
            WebLinkKind::Www(i) => {
                if let Some(n) = self.get_mut(record) {
                    nth_removed(n, "WWW", *i);
                }
            }
            WebLinkKind::Legacy(i) => {
                if let Some(n) = self.get_mut(record) {
                    nth_removed(n, LEGACY_TAG, *i);
                }
            }
        }
    }

    /// Nombre de `_URL` que [`Gedcom::migrate_url_tags`] peut convertir.
    pub fn legacy_url_count(&self) -> usize {
        self.legacy_url_sites()
            .iter()
            .map(|(r, path)| {
                let mut n = self.get(r).unwrap();
                for &i in path {
                    n = &n.children[i];
                }
                n.children.iter().filter(|c| is_convertible(c)).count()
            })
            .sum()
    }

    /// Emplacements des `_URL` convertibles : (enregistrement, chemin d'indices jusqu'au parent).
    /// Convertible = adresse web valide, sous une structure qui permet un lien
    /// multimédia (`OBJE`) ou, à défaut, une adresse `WWW`.
    fn legacy_url_sites(&self) -> Vec<(String, Vec<usize>)> {
        fn walk(n: &Node, def: &schema::StructDef, path: &mut Vec<usize>, out: &mut Vec<Vec<usize>>) {
            if (def.sub("OBJE").is_some() || def.sub("WWW").is_some()) && n.children.iter().any(is_convertible) {
                out.push(path.clone());
            }
            for (i, c) in n.children.iter().enumerate() {
                if let Some(sub) = def.sub(&c.tag) {
                    path.push(i);
                    walk(c, sub.def(), path, out);
                    path.pop();
                }
            }
        }
        let mut sites = Vec::new();
        for r in &self.records {
            let (Some(x), Some(def)) = (r.xref.as_ref(), schema::record(&r.tag)) else { continue };
            let mut paths = Vec::new();
            walk(r, def, &mut Vec::new(), &mut paths);
            sites.extend(paths.into_iter().map(|p| (x.clone(), p)));
        }
        sites
    }

    /// Convertit les `_URL` en GEDCOM 7 standard : lien multimédia (`OBJE` →
    /// `FILE <url>`, `FORM text/html`) là où il est permis (individus, familles,
    /// sources, événements, citations…), sinon `WWW` (dépôts). Le titre du média
    /// reprend celui de la source, quand il y en a un. Renvoie le nombre de liens convertis.
    pub fn migrate_url_tags(&mut self) -> usize {
        let mut converted = 0;
        for (record, path) in self.legacy_url_sites() {
            let def = {
                let r = self.get(&record).unwrap();
                let mut def = schema::record(&r.tag).unwrap();
                let mut n = r;
                for &i in &path {
                    n = &n.children[i];
                    def = def.sub(&n.tag).unwrap().def();
                }
                def
            };
            if def.sub("OBJE").is_none() {
                // Pas de lien multimédia possible ici : adresse `WWW` standard.
                let mut n = self.get_mut(&record).unwrap();
                for &i in &path {
                    n = &mut n.children[i];
                }
                for c in n.children.iter_mut().filter(|c| is_convertible(c)) {
                    c.value = normalize_url(c.val()).ok();
                    c.tag = "WWW".into();
                    converted += 1;
                }
                self.get_mut(&record).unwrap().touch();
                continue;
            }
            let title = self
                .get(&record)
                .filter(|r| r.tag == "SOUR" && path.is_empty())
                .and_then(|r| r.child("TITL"))
                .map(|t| t.text())
                .unwrap_or_default();
            let urls: Vec<String> = {
                let mut n = self.get(&record).unwrap();
                for &i in &path {
                    n = &n.children[i];
                }
                n.children.iter().filter(|c| is_convertible(c)).filter_map(|c| normalize_url(c.val()).ok()).collect()
            };
            let medias: Vec<String> = urls.iter().map(|u| self.web_media_for(u, &title)).collect();
            let r = self.get_mut(&record).unwrap();
            let mut n = &mut *r;
            for &i in &path {
                n = &mut n.children[i];
            }
            // Le lien prend la place du premier `_URL`, les autres sont retirés.
            let at = n.children.iter().position(is_convertible).unwrap();
            n.children.retain(|c| !is_convertible(c));
            for (k, m) in medias.iter().enumerate() {
                let p = format!("@{m}@");
                if !n.children.iter().any(|c| c.tag == "OBJE" && c.val() == p) {
                    n.children.insert((at + k).min(n.children.len()), Node::with_value("OBJE", p));
                }
            }
            converted += urls.len();
            r.touch();
        }
        if converted > 0 && self.legacy_url_count() == 0 {
            // Plus aucun `_URL` : on retire sa déclaration d'extension, s'il y en a une.
            let still_used = self.records.iter().any(|r| {
                let mut used = false;
                r.walk(&mut |n| used |= n.tag == LEGACY_TAG);
                used
            });
            if !still_used && let Some(s) = self.header.child_mut("SCHMA") {
                s.children.retain(|t| !(t.tag == "TAG" && t.val().split_whitespace().next() == Some(LEGACY_TAG)));
            }
        }
        self.reindex();
        converted
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn urls() {
        assert_eq!(normalize_url("archives.cg54.fr").unwrap(), "https://archives.cg54.fr");
        assert_eq!(normalize_url(" http://x.fr/a b ").unwrap_err(), "The address must not contain spaces");
        assert!(normalize_url("ftp://x.fr").is_err());
        assert!(normalize_url("bonjour").is_err());
        assert_eq!(host("https://www.archives43.fr/ark:47539/s005?x=1"), "www.archives43.fr");
        assert_eq!(host("archives.cg54.fr"), "archives.cg54.fr");
    }
}
