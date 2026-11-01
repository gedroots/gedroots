//! Un fichier GEDCOM chargé en mémoire.

use crate::node::Node;
use crate::parse::{self, ParseError, TextFormat};
use std::collections::HashMap;

/// Types d'enregistrements de niveau 0 gérés par GEDCOM 7.
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

    /// Préfixe utilisé pour les nouveaux identifiants (convention Ancestris).
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

    pub fn label_fr(self) -> &'static str {
        match self {
            RecordKind::Individual => "Individu",
            RecordKind::Family => "Famille",
            RecordKind::Source => "Source",
            RecordKind::Repository => "Dépôt",
            RecordKind::Media => "Média",
            RecordKind::SharedNote => "Note partagée",
            RecordKind::Submitter => "Auteur",
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct Gedcom {
    pub header: Node,
    /// Enregistrements dans l'ordre du fichier (hors `HEAD` et `TRLR`).
    pub records: Vec<Node>,
    pub format: TextFormat,
    index: HashMap<String, usize>,
}

impl Gedcom {
    /// Nouveau fichier GEDCOM 7 vide.
    pub fn new() -> Self {
        let header = Node::new("HEAD")
            .push(Node::new("GEDC").push(Node::with_value("VERS", "7.0")))
            .push(Node::with_value("SOUR", "GEDROOTS").push(Node::with_value("NAME", "GedRoots")))
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
                _ => records.push(n),
            }
        }
        let header = header.ok_or(ParseError { line: 1, message: "en-tête HEAD manquant".into() })?;
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
        parse::write_nodes(&self.records, 0, nl, &mut s);
        s.push_str("0 TRLR");
        if self.format.final_newline {
            s.push_str(nl);
        }
        s
    }

    /// Version déclarée dans `HEAD.GEDC.VERS`.
    pub fn version(&self) -> &str {
        self.header.path_val(&["GEDC", "VERS"]).unwrap_or("?")
    }

    /// Format des lieux déclaré dans `HEAD.PLAC.FORM`.
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
        self.index.get(xref).map(|&i| &self.records[i])
    }

    pub fn get_mut(&mut self, xref: &str) -> Option<&mut Node> {
        let i = *self.index.get(xref)?;
        Some(&mut self.records[i])
    }

    pub fn records_of(&self, kind: RecordKind) -> impl Iterator<Item = &Node> {
        self.records.iter().filter(move |r| r.tag == kind.tag())
    }

    pub fn count(&self, kind: RecordKind) -> usize {
        self.records_of(kind).count()
    }

    /// Prochain identifiant libre, au format du fichier (`I00042`, `S00249`…).
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

    /// Ajoute un enregistrement vide du type donné et renvoie son identifiant.
    pub fn create(&mut self, kind: RecordKind) -> String {
        let xref = self.next_xref(kind);
        let mut node = Node::new(kind.tag()).with_xref(xref.clone());
        match kind {
            RecordKind::Individual => {
                node.children.push(Node::with_value("NAME", "//"));
                node.children.push(Node::with_value("SEX", "U"));
            }
            RecordKind::Source => node.children.push(Node::with_value("TITL", "Nouvelle source")),
            RecordKind::Repository => node.children.push(Node::with_value("NAME", "Nouveau dépôt")),
            RecordKind::Media => {
                node.children.push(Node::with_value("FILE", "").push(Node::with_value("FORM", "image/jpeg")))
            }
            RecordKind::SharedNote => node.value = Some("Nouvelle note".into()),
            RecordKind::Submitter => node.children.push(Node::with_value("NAME", "")),
            RecordKind::Family => {}
        }
        node.touch();
        if let Some(chan) = node.child("CHAN").cloned() {
            node.insert_child(Node { tag: "CREA".into(), ..chan });
        }
        self.records.push(node);
        self.reindex();
        xref
    }

    /// Remplace un enregistrement (ex. après édition brute). L'identifiant peut changer :
    /// les pointeurs vers l'ancien identifiant sont alors mis à jour.
    pub fn replace(&mut self, xref: &str, node: Node) -> Result<(), String> {
        let i = *self.index.get(xref).ok_or_else(|| format!("@{xref}@ introuvable"))?;
        let new_xref = node.xref.clone().ok_or("l'enregistrement doit garder un identifiant @…@")?;
        if new_xref != xref && self.index.contains_key(&new_xref) {
            return Err(format!("l'identifiant @{new_xref}@ est déjà utilisé"));
        }
        self.records[i] = node;
        if new_xref != xref {
            self.rename_pointers(xref, &new_xref);
        }
        self.reindex();
        Ok(())
    }

    fn rename_pointers(&mut self, old: &str, new: &str) {
        let old_p = format!("@{old}@");
        let new_p = format!("@{new}@");
        for r in self.records.iter_mut().chain(std::iter::once(&mut self.header)) {
            r.walk_mut(&mut |n| {
                if n.value.as_deref() == Some(old_p.as_str()) {
                    n.value = Some(new_p.clone());
                }
            });
        }
    }

    /// Supprime un enregistrement et toutes les structures qui pointent vers lui.
    pub fn delete(&mut self, xref: &str) -> Option<Node> {
        let i = *self.index.get(xref)?;
        let removed = self.records.remove(i);
        let p = format!("@{xref}@");
        fn strip(n: &mut Node, p: &str) {
            n.children.retain(|c| c.value.as_deref() != Some(p));
            for c in &mut n.children {
                strip(c, p);
            }
        }
        for r in &mut self.records {
            strip(r, &p);
        }
        self.reindex();
        Some(removed)
    }

    /// Enregistrements qui pointent vers `xref` (citations, liens familiaux…).
    pub fn referrers(&self, xref: &str) -> Vec<&Node> {
        self.records.iter().filter(|r| r.pointers().iter().any(|p| p == xref)).collect()
    }
}
