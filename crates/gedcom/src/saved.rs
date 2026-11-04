//! Saved searches (`HEAD._GR_FIND`), shown in the sidebar.
//!
//! ```text
//! 0 HEAD
//! 1 _GR_FIND Born around 1850
//! 2 _GR_SCOPE INDI
//! 2 _GR_QUERY born:1850±5 sex:m
//! 1 _GR_FIND Unattached records
//! 2 _GR_SCOPE OBJE
//! 2 _GR_QUERY without:link
//! 2 _GR_TYPE Actes
//! ```

use crate::document::Gedcom;
use crate::node::Node;
use crate::query::{Scope, chip_label, words};
use rust_i18n::t;

pub const FIND_TAG: &str = "_GR_FIND";
const SCOPE_TAG: &str = "_GR_SCOPE";
const QUERY_TAG: &str = "_GR_QUERY";
const TYPE_TAG: &str = "_GR_TYPE";
const URI: &str = extension_uri!("_gr_find");

/// A saved search.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SavedSearch {
    pub name: String,
    pub scope: Scope,
    /// Query text (`born:1850±5 sex:m`).
    pub query: String,
    /// Media type chosen in the type bar (Media zone).
    pub media_type: Option<String>,
}

fn zone_tag(s: Scope) -> &'static str {
    match s {
        Scope::Individuals => "INDI",
        Scope::Sources => "SOUR",
        Scope::Places => "PLAC",
        Scope::Media => "OBJE",
    }
}

fn zone(tag: &str) -> Option<Scope> {
    Some(match tag.trim() {
        "INDI" => Scope::Individuals,
        "SOUR" => Scope::Sources,
        "PLAC" => Scope::Places,
        "OBJE" => Scope::Media,
        _ => return None,
    })
}

impl SavedSearch {
    fn read(n: &Node) -> Option<Self> {
        let scope = zone(n.child_val(SCOPE_TAG)?)?;
        Some(Self {
            name: n.val().trim().to_string(),
            scope,
            query: n.child_val(QUERY_TAG).map(|q| q.trim().to_string()).unwrap_or_default(),
            media_type: n.child_val(TYPE_TAG).map(str::trim).filter(|t| !t.is_empty()).map(str::to_string),
        })
    }

    fn node(&self) -> Node {
        let mut n = Node::with_value(FIND_TAG, self.name.trim());
        n.children.push(Node::with_value(SCOPE_TAG, zone_tag(self.scope)));
        if !self.query.trim().is_empty() {
            n.children.push(Node::with_value(QUERY_TAG, self.query.trim()));
        }
        if let Some(t) = &self.media_type {
            n.children.push(Node::with_value(TYPE_TAG, t.as_str()));
        }
        n
    }

    /// Same search (zone, criteria, type), whatever the name.
    pub fn same(&self, scope: Scope, query: &str, media_type: Option<&str>) -> bool {
        let norm = |q: &str| q.split_whitespace().collect::<Vec<_>>().join(" ");
        self.scope == scope && norm(&self.query) == norm(query) && self.media_type.as_deref() == media_type
    }
}

/// Name suggested for a search: its tokens (“Born about 1850 (± 5 years) · Men”), its free words in
/// quotes, preceded by the media type.
pub fn default_name(g: &Gedcom, scope: Scope, query: &str, media_type: Option<&str>) -> String {
    let mut parts: Vec<String> = media_type.map(str::to_string).into_iter().collect();
    let mut free: Vec<&str> = Vec::new();
    for (_, w) in words(query) {
        match chip_label(scope, w, Some(g)) {
            Some(l) => parts.push(l),
            None => free.push(w),
        }
    }
    if !free.is_empty() {
        parts.push(t!("search.saved.free_words", words = free.join(" ")).into_owned());
    }
    if parts.is_empty() {
        return t!("search.saved.default").into_owned();
    }
    parts.join(" · ")
}

impl Gedcom {
    pub fn saved_searches(&self) -> Vec<SavedSearch> {
        self.header.children_with(FIND_TAG).filter_map(SavedSearch::read).collect()
    }

    /// Adds a search at the end of the list; returns its position.
    pub fn save_search(&mut self, s: &SavedSearch) -> usize {
        for t in [FIND_TAG, SCOPE_TAG, QUERY_TAG, TYPE_TAG] {
            self.declare_extension(t, URI);
        }
        let n = self.saved_searches().len();
        // After the last search, otherwise at the end of the header.
        let at =
            self.header.children.iter().rposition(|c| c.tag == FIND_TAG).map_or(self.header.children.len(), |i| i + 1);
        self.header.children.insert(at, s.node());
        n
    }

    /// Readable `HEAD._GR_FIND`s, in order: position in the header.
    fn find_index(&self, i: usize) -> Option<usize> {
        self.header
            .children
            .iter()
            .enumerate()
            .filter(|(_, c)| c.tag == FIND_TAG && SavedSearch::read(c).is_some())
            .nth(i)
            .map(|(k, _)| k)
    }

    pub fn rename_search(&mut self, i: usize, name: &str) -> bool {
        let name = name.trim();
        match self.find_index(i) {
            Some(k) if !name.is_empty() => {
                self.header.children[k].value = Some(name.to_string());
                true
            }
            _ => false,
        }
    }

    pub fn delete_search(&mut self, i: usize) -> Option<SavedSearch> {
        let k = self.find_index(i)?;
        let n = self.header.children.remove(k);
        if !self.header.children.iter().any(|c| c.tag == FIND_TAG) {
            for t in [FIND_TAG, SCOPE_TAG, QUERY_TAG, TYPE_TAG] {
                self.undeclare_extension(t);
            }
        }
        SavedSearch::read(&n)
    }

    /// Moves a saved search (order in the sidebar).
    pub fn move_search(&mut self, from: usize, to: usize) -> bool {
        let (Some(a), Some(b)) = (self.find_index(from), self.find_index(to)) else { return false };
        let n = self.header.children.remove(a);
        self.header.children.insert(b, n);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const GED: &str = "0 HEAD\n1 GEDC\n2 VERS 7.0\n0 @I1@ INDI\n1 NAME Luc /GUYOT/\n1 BIRT\n2 DATE 1850\n0 TRLR\n";

    #[test]
    fn saved_searches_round_trip() {
        let mut g = Gedcom::parse(GED).unwrap();
        assert!(g.saved_searches().is_empty());
        let s = SavedSearch {
            name: default_name(&g, Scope::Individuals, "born:1850±5 guyot", None),
            scope: Scope::Individuals,
            query: "born:1850±5 guyot".into(),
            media_type: None,
        };
        assert_eq!(s.name, "Born about 1850 (± 5 years) · “guyot”");
        g.save_search(&s);
        let m = SavedSearch {
            name: "Actes".into(),
            scope: Scope::Media,
            query: "without:link".into(),
            media_type: Some("Actes".into()),
        };
        assert_eq!(g.save_search(&m), 1);
        let out = g.to_gedcom_string();
        assert!(out.contains(
            "1 _GR_FIND Born about 1850 (± 5 years) · “guyot”\n2 _GR_SCOPE INDI\n2 _GR_QUERY born:1850±5 guyot\n"
        ));
        assert!(out.contains("2 TAG _GR_FIND https://"));
        let g2 = Gedcom::parse(&out).unwrap();
        assert_eq!(g2.saved_searches(), vec![s.clone(), m.clone()]);
        assert!(g2.saved_searches()[1].same(Scope::Media, " without:link ", Some("Actes")));

        assert!(g.rename_search(0, "Nés vers 1850"));
        assert!(g.move_search(1, 0));
        assert_eq!(g.saved_searches()[1].name, "Nés vers 1850");
        assert_eq!(g.delete_search(0).map(|s| s.name).as_deref(), Some("Actes"));
        g.delete_search(0);
        assert!(!g.to_gedcom_string().contains("_GR_"));
    }
}
