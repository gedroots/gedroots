//! Web links (pages of online archives, profiles, websites…) stored in standard GEDCOM 7.
//!
//! GEDCOM 7 has no generic “URL” tag on individuals, families and sources. The standard form is a
//! **multimedia link** to an `OBJE` record whose `FILE` is a URL (the `FilePath` type accepts
//! URLs):
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
//! Repositories (`REPO`) do not accept a multimedia link but have the standard `WWW` tag (up to
//! three): that is what is used for them.
//!
//! The non-standard `_URL` tag (written by several programs) is still read, and
//! [`Gedcom::migrate_url_tags`] converts it (a check of the import assistant, see
//! [`crate::import`]).

use crate::document::{Gedcom, RecordKind};
use crate::model::Media;
use crate::node::Node;
use crate::schema;

/// Historical extension tag for a web address.
pub const LEGACY_TAG: &str = "_URL";

/// Format (`FILE.FORM`) of a web page.
pub const WEB_FORM: &str = "text/html";

/// Where a web link of a record is stored.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WebLinkKind {
    /// `OBJE` link to a media whose `FILE` is a URL (standard form).
    Media(String),
    /// Standard `WWW` (repositories): rank among the record's `WWW`s.
    Www(usize),
    /// Historical `_URL`: rank among the record's `_URL`s.
    Legacy(usize),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WebLink {
    pub kind: WebLinkKind,
    pub url: String,
    pub title: Option<String>,
}

impl WebLink {
    /// Address to open in the browser (with `https://` if the file leaves it out).
    pub fn href(&self) -> String {
        normalize_url(&self.url).unwrap_or_else(|_| self.url.clone())
    }

    /// Title, or failing that the site's name (“www.archives43.fr”).
    pub fn label(&self) -> String {
        self.title.clone().filter(|t| !t.trim().is_empty()).unwrap_or_else(|| host(&self.url).to_string())
    }

    pub fn is_legacy(&self) -> bool {
        matches!(self.kind, WebLinkKind::Legacy(_))
    }
}

/// Is the address a web URL (`http://`, `https://`)?
pub fn is_web_url(s: &str) -> bool {
    let l = s.trim_start().get(..8).unwrap_or(s).to_ascii_lowercase();
    l.starts_with("http://") || l.starts_with("https://")
}

/// Name of the site: “https://www.archives43.fr/ark:…” → “www.archives43.fr”.
pub fn host(url: &str) -> &str {
    let u = url.trim();
    let rest = u.split_once("://").map(|(_, r)| r).unwrap_or(u);
    let end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let h = &rest[..end];
    h.rsplit_once('@').map(|(_, h)| h).unwrap_or(h)
}

/// Checks and completes a typed address: “archives43.fr” → “https://archives43.fr”.
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
    /// URL of the media, if its file is a web page or resource.
    pub fn web_url(&self) -> Option<&'a str> {
        self.file().filter(|f| is_web_url(f))
    }

    pub fn is_web(&self) -> bool {
        self.web_url().is_some()
    }
}

/// `_URL` whose value is a valid web address (with or without `https://`).
fn is_convertible(n: &Node) -> bool {
    n.tag == LEGACY_TAG && normalize_url(n.val()).is_ok()
}

/// Does the record type accept multimedia links (`OBJE`)?
fn accepts_media(tag: &str) -> bool {
    schema::record(tag).is_some_and(|d| d.sub("OBJE").is_some())
}

impl Gedcom {
    /// Web links of a record: linked web media, `WWW`, then historical `_URL`s.
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

    /// Can a web link be added to this record?
    pub fn can_have_web_links(&self, record: &str) -> bool {
        self.get(record).is_some_and(|n| accepts_media(&n.tag) || n.tag == "REPO")
    }

    /// Existing web media for this URL, or a new `FILE <url>` / `FORM text/html` media.
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

    /// Adds a web link: multimedia link (`OBJE`), or `WWW` for a repository.
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

    /// Edits a web link. An edited historical `_URL` becomes a multimedia link.
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

    /// Removes a web link. A web media no longer used anywhere is deleted.
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

    /// Number of `_URL`s that [`Gedcom::migrate_url_tags`] can convert.
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

    /// Locations of the convertible `_URL`s: (record, path of indices to the parent). Convertible =
    /// valid web address, under a structure that allows a multimedia link (`OBJE`) or, failing
    /// that, a `WWW` address.
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

    /// Converts `_URL`s to standard GEDCOM 7: multimedia link (`OBJE` → `FILE <url>`, `FORM
    /// text/html`) where it is allowed (individuals, families, sources, events, citations…),
    /// otherwise `WWW` (repositories). The media's title takes the source's, when there is one.
    /// Returns the number of links converted.
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
                // No multimedia link possible here: standard `WWW` address.
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
            // The link takes the place of the first `_URL`, the others are removed.
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
            // No `_URL` left: remove its extension declaration, if there is one.
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
