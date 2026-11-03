//! A GEDCOM file loaded in memory.

use crate::node::Node;
use crate::parse::{self, ParseError, TextFormat};
use std::collections::HashMap;
use std::sync::Arc;

/// Level 0 record types handled by GEDCOM 7.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RecordKind {
    Individual,
    Family,
    Source,
    Repository,
    Media,
    SharedNote,
    Submitter,
}

impl RecordKind {
    pub const ALL: [RecordKind; 7] = [
        RecordKind::Individual,
        RecordKind::Family,
        RecordKind::Source,
        RecordKind::Repository,
        RecordKind::Media,
        RecordKind::SharedNote,
        RecordKind::Submitter,
    ];

    pub fn tag(self) -> &'static str {
        match self {
            RecordKind::Individual => "INDI",
            RecordKind::Family => "FAM",
            RecordKind::Source => "SOUR",
            RecordKind::Repository => "REPO",
            RecordKind::Media => "OBJE",
            RecordKind::SharedNote => "SNOTE",
            RecordKind::Submitter => "SUBM",
        }
    }

    pub fn from_tag(tag: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|k| k.tag() == tag)
    }

    /// Prefix used for new identifiers (Ancestris convention).
    pub fn xref_prefix(self) -> &'static str {
        match self {
            RecordKind::Individual => "I",
            RecordKind::Family => "F",
            RecordKind::Source => "S",
            RecordKind::Repository => "R",
            RecordKind::Media => "M",
            RecordKind::SharedNote => "N",
            RecordKind::Submitter => "B",
        }
    }

    /// Name of the kind, e.g. “Source”.
    pub fn label(self) -> std::borrow::Cow<'static, str> {
        match self {
            RecordKind::Individual => rust_i18n::t!("kind.individual"),
            RecordKind::Family => rust_i18n::t!("kind.family"),
            RecordKind::Source => rust_i18n::t!("kind.source"),
            RecordKind::Repository => rust_i18n::t!("kind.repository"),
            RecordKind::Media => rust_i18n::t!("kind.media"),
            RecordKind::SharedNote => rust_i18n::t!("kind.shared_note"),
            RecordKind::Submitter => rust_i18n::t!("kind.submitter"),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct Gedcom {
    pub header: Node,
    /// Records in file order (without `HEAD` and `TRLR`). Shared between copies (undo history): a
    /// change only copies the record changed (`Arc::make_mut`).
    pub records: Vec<Arc<Node>>,
    pub format: TextFormat,
    index: HashMap<String, usize>,
}

impl Gedcom {
    /// New empty GEDCOM 7 file.
    pub fn new() -> Self {
        let header = Node::new("HEAD")
            .push(Node::new("GEDC").push(Node::with_value("VERS", "7.0")))
            .push(Node::with_value("SOUR", "GEDROOTS").push(Node::with_value("NAME", "GEDRoots")))
            .push(Node::with_value("LANG", "fr"));
        let mut g = Gedcom { header, records: Vec::new(), format: TextFormat::default(), index: HashMap::new() };
        g.reindex();
        g
    }

    pub fn parse(text: &str) -> Result<Self, ParseError> {
        let (nodes, format) = parse::parse_nodes(text)?;
        let mut header = None;
        let mut records = Vec::new();
        for n in nodes {
            match n.tag.as_str() {
                "HEAD" if header.is_none() => header = Some(n),
                "TRLR" => {}
                _ => records.push(Arc::new(n)),
            }
        }
        let header = header.ok_or(ParseError { line: 1, message: rust_i18n::t!("error.parse.no_header").into() })?;
        let mut g = Gedcom { header, records, format, index: HashMap::new() };
        g.reindex();
        Ok(g)
    }

    pub fn to_gedcom_string(&self) -> String {
        let nl = if self.format.crlf { "\r\n" } else { "\n" };
        let mut s = String::with_capacity(self.records.len() * 256);
        if self.format.bom {
            s.push('\u{feff}');
        }
        parse::write_nodes(std::slice::from_ref(&self.header), 0, nl, &mut s);
        parse::write_nodes(self.records.iter().map(|r| &**r), 0, nl, &mut s);
        s.push_str("0 TRLR");
        if self.format.final_newline {
            s.push_str(nl);
        }
        s
    }

    /// Version declared in `HEAD.GEDC.VERS`.
    pub fn version(&self) -> &str {
        self.header.path_val(&["GEDC", "VERS"]).unwrap_or("?")
    }

    /// Place format declared in `HEAD.PLAC.FORM`.
    pub fn place_form(&self) -> Vec<String> {
        self.header
            .path_val(&["PLAC", "FORM"])
            .map(|f| f.split(',').map(|s| s.trim().to_string()).collect())
            .unwrap_or_default()
    }

    pub fn reindex(&mut self) {
        self.index.clear();
        for (i, r) in self.records.iter().enumerate() {
            if let Some(x) = &r.xref {
                self.index.insert(x.clone(), i);
            }
        }
    }

    pub fn get(&self, xref: &str) -> Option<&Node> {
        self.index.get(xref).map(|&i| &*self.records[i])
    }

    pub fn get_mut(&mut self, xref: &str) -> Option<&mut Node> {
        let i = *self.index.get(xref)?;
        Some(Arc::make_mut(&mut self.records[i]))
    }

    pub fn records_of(&self, kind: RecordKind) -> impl Iterator<Item = &Node> {
        self.records.iter().map(|r| &**r).filter(move |r| r.tag == kind.tag())
    }

    /// Records to modify: only those matching `touches` are copied, the others stay shared with the
    /// undo history.
    pub fn records_where_mut(&mut self, touches: impl Fn(&Node) -> bool) -> impl Iterator<Item = &mut Node> {
        self.records.iter_mut().filter(move |r| touches(r)).map(Arc::make_mut)
    }

    pub fn count(&self, kind: RecordKind) -> usize {
        self.records_of(kind).count()
    }

    /// Next free identifier, in the file's format (`I00042`, `S00249`…).
    pub fn next_xref(&self, kind: RecordKind) -> String {
        let prefix = kind.xref_prefix();
        let mut max = 0u64;
        let mut width = 5;
        for r in self.records_of(kind) {
            if let Some(num) = r.xref.as_deref().and_then(|x| x.strip_prefix(prefix))
                && let Ok(n) = num.parse::<u64>()
            {
                max = max.max(n);
                width = num.len();
            }
        }
        loop {
            max += 1;
            let candidate = format!("{prefix}{max:0width$}");
            if !self.index.contains_key(&candidate) {
                return candidate;
            }
        }
    }

    /// Adds an empty record of the given type and returns its identifier.
    pub fn create(&mut self, kind: RecordKind) -> String {
        let xref = self.next_xref(kind);
        let mut node = Node::new(kind.tag()).with_xref(xref.clone());
        match kind {
            RecordKind::Individual => {
                node.children.push(Node::with_value("NAME", "//"));
                node.children.push(Node::with_value("SEX", "U"));
            }
            RecordKind::Source => node.children.push(Node::with_value("TITL", rust_i18n::t!("record.new_source"))),
            RecordKind::Repository => {
                node.children.push(Node::with_value("NAME", rust_i18n::t!("record.new_repository")))
            }
            RecordKind::Media => {
                node.children.push(Node::with_value("FILE", "").push(Node::with_value("FORM", "image/jpeg")))
            }
            RecordKind::SharedNote => node.value = Some(rust_i18n::t!("record.new_note").into()),
            RecordKind::Submitter => node.children.push(Node::with_value("NAME", "")),
            RecordKind::Family => {}
        }
        node.touch();
        if let Some(chan) = node.child("CHAN").cloned() {
            node.insert_child(Node { tag: "CREA".into(), ..chan });
        }
        self.records.push(Arc::new(node));
        self.reindex();
        xref
    }

    /// Replaces a record (e.g. after raw editing). The identifier can change: pointers to the old
    /// identifier are then updated.
    pub fn replace(&mut self, xref: &str, node: Node) -> Result<(), String> {
        let i = *self.index.get(xref).ok_or_else(|| format!("@{xref}@ introuvable"))?;
        let new_xref = node.xref.clone().ok_or_else(|| rust_i18n::t!("error.keep_xref"))?;
        if new_xref != xref && self.index.contains_key(&new_xref) {
            return Err(rust_i18n::t!("error.xref_used", xref = format!("@{new_xref}@")).into());
        }
        self.records[i] = Arc::new(node);
        if new_xref != xref {
            self.rename_pointers(xref, &new_xref);
        }
        self.reindex();
        Ok(())
    }

    pub(crate) fn rename_pointers(&mut self, old: &str, new: &str) {
        let old_p = format!("@{old}@");
        let new_p = format!("@{new}@");
        let mentions = |n: &Node| n.any(&|n| n.value.as_deref() == Some(old_p.as_str()));
        let header = std::iter::once(&mut self.header);
        for r in self.records.iter_mut().filter(|r| mentions(r)).map(Arc::make_mut).chain(header) {
            r.walk_mut(&mut |n| {
                if n.value.as_deref() == Some(old_p.as_str()) {
                    n.value = Some(new_p.clone());
                }
            });
        }
    }

    /// Deletes a record and every structure that points to it.
    pub fn delete(&mut self, xref: &str) -> Option<Node> {
        let i = *self.index.get(xref)?;
        let removed = Arc::unwrap_or_clone(self.records.remove(i));
        let p = format!("@{xref}@");
        fn strip(n: &mut Node, p: &str) {
            n.children.retain(|c| c.value.as_deref() != Some(p));
            for c in &mut n.children {
                strip(c, p);
            }
        }
        for r in self.records_where_mut(|r| r.children.iter().any(|c| c.any(&|n| n.value.as_deref() == Some(&p)))) {
            strip(r, &p);
        }
        strip(&mut self.header, &p);
        self.reindex();
        Some(removed)
    }

    /// Records that point to `xref` (citations, family links…).
    pub fn referrers(&self, xref: &str) -> Vec<&Node> {
        self.records.iter().map(|r| &**r).filter(|r| r.pointers().iter().any(|p| p == xref)).collect()
    }

    // ------------------------------------------------------------------
    // GEDRoots extensions (documented in docs/extensions.md)
    // ------------------------------------------------------------------

    /// Root individual declared in `HEAD._GR_ROOT`, if it exists (see [`Gedcom::root`]).
    pub fn declared_root(&self) -> Option<String> {
        let x = self.header.child(ROOT_TAG).and_then(|n| n.pointer())?;
        self.get(x).is_some().then(|| x.to_string())
    }

    /// Root individual: `HEAD._GR_ROOT`, otherwise Ancestris' Sosa 1, otherwise the first
    /// individual.
    pub fn root(&self) -> Option<String> {
        if let Some(x) = self.header.child(ROOT_TAG).and_then(|n| n.pointer()).filter(|x| self.get(x).is_some()) {
            return Some(x.to_string());
        }
        self.records_of(RecordKind::Individual)
            .find(|r| crate::model::Individual::new(r).sosa().as_deref() == Some("1"))
            .or_else(|| self.records_of(RecordKind::Individual).next())
            .and_then(|r| r.xref.clone())
    }

    /// Sets the root individual in the header, and declares the extension in `HEAD.SCHMA`.
    pub fn set_root(&mut self, xref: &str) {
        self.declare_extension(ROOT_TAG, ROOT_URI);
        let v = format!("@{xref}@");
        match self.header.child_mut(ROOT_TAG) {
            Some(n) => n.value = Some(v),
            None => self.header.insert_child(Node::with_value(ROOT_TAG, v)),
        }
    }

    /// Declares an extension tag (`HEAD.SCHMA.TAG`), as GEDCOM 7 requires.
    pub fn declare_extension(&mut self, tag: &str, uri: &str) {
        let schma = self.header.ensure_child("SCHMA");
        let declared = schma.children_with("TAG").any(|t| t.val().split_whitespace().next() == Some(tag));
        if !declared {
            schma.children.push(Node::with_value("TAG", format!("{tag} {uri}")));
        }
    }
}

/// Tag of the root individual (in `HEAD`).
pub const ROOT_TAG: &str = "_GR_ROOT";
pub const ROOT_URI: &str = extension_uri!("_gr_root");
