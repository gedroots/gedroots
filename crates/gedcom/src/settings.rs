//! Réglages du fichier (menu Fichier › Réglages du fichier…) : ce qui décrit la
//! base elle-même, rangé dans l'en-tête `HEAD` en GEDCOM 7 standard.
//!
//! - auteur : `HEAD.SUBM` → enregistrement `SUBM` (`NAME`, `EMAIL`) ;
//! - droits : `HEAD.COPR` ; description : `HEAD.NOTE` ; langue : `HEAD.LANG` ;
//! - informations en lecture seule : version, logiciel et date du dernier
//!   enregistrement (`HEAD.SOUR`, `HEAD.DATE`, mis à jour par [`Gedcom::stamp_header`]),
//!   extensions déclarées (`HEAD.SCHMA`).
//!
//! Les anomalies masquées (`_GR_IGNORE`) se listent et se réaffichent ici aussi.

use crate::analysis::check::{IGNORE_TAG, code_label};
use crate::document::{Gedcom, RecordKind};
use crate::node::{Node, now_gedcom};

/// Champs modifiables du panneau.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FileSettings {
    pub author: String,
    pub email: String,
    pub copyright: String,
    pub description: String,
    /// Code de langue BCP 47 (`fr`), vide si absent.
    pub language: String,
}

/// Informations en lecture seule.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FileInfo {
    /// `7.0.13`
    pub version: String,
    /// « Ancestris 13 », « GEDRoots 0.1.0 »… (vide si inconnu).
    pub software: String,
    /// Date (et heure) du dernier enregistrement déclarée dans `HEAD.DATE`, au format GEDCOM.
    pub saved: Option<(String, Option<String>)>,
    /// Extensions déclarées dans `HEAD.SCHMA` (balises seulement).
    pub extensions: Vec<String>,
}

/// Une anomalie masquée par `_GR_IGNORE`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IgnoredIssue {
    pub xref: String,
    pub code: String,
    pub label: std::borrow::Cow<'static, str>,
}

impl Gedcom {
    /// Auteur déclaré dans l'en-tête (`HEAD.SUBM`), s'il existe encore.
    fn submitter(&self) -> Option<&Node> {
        self.header.child("SUBM").and_then(|n| n.pointer()).and_then(|x| self.get(x))
    }

    pub fn file_settings(&self) -> FileSettings {
        let subm = self.submitter();
        FileSettings {
            author: subm.and_then(|s| s.child_val("NAME")).unwrap_or("").to_string(),
            email: subm.and_then(|s| s.child_val("EMAIL")).unwrap_or("").to_string(),
            copyright: self.header.child_val("COPR").unwrap_or("").to_string(),
            description: self.header.child("NOTE").map(Node::text).unwrap_or_default(),
            language: self.header.child_val("LANG").unwrap_or("").to_string(),
        }
    }

    /// Enregistre les réglages. Renvoie `false` si rien n'a changé.
    pub fn set_file_settings(&mut self, s: &FileSettings) -> bool {
        let s = FileSettings {
            author: s.author.trim().to_string(),
            email: s.email.trim().to_string(),
            copyright: s.copyright.trim().to_string(),
            description: s.description.trim().to_string(),
            language: s.language.trim().to_string(),
        };
        if s == self.file_settings() {
            return false;
        }
        self.header.set_child_val("COPR", &s.copyright);
        self.header.set_child_text("NOTE", &s.description);
        self.header.set_child_val("LANG", &s.language);
        self.set_submitter(&s.author, &s.email);
        true
    }

    fn set_submitter(&mut self, name: &str, email: &str) {
        let current = self.submitter().and_then(|n| n.xref.clone());
        if name.is_empty() && email.is_empty() {
            // Plus d'auteur : on retire le lien, et l'enregistrement s'il ne sert qu'à l'en-tête.
            self.header.children.retain(|c| c.tag != "SUBM");
            if let Some(x) = current
                && self.referrers(&x).is_empty()
            {
                self.delete(&x);
            }
            return;
        }
        let x = match current {
            Some(x) => x,
            None => {
                let x = self.create(RecordKind::Submitter);
                self.header.set_child_val("SUBM", &format!("@{x}@"));
                x
            }
        };
        let Some(r) = self.get_mut(&x) else { return };
        // `NAME` est obligatoire sur un `SUBM` : on le garde, même vide.
        match r.child_mut("NAME") {
            Some(n) => n.value = (!name.is_empty()).then(|| name.to_string()),
            None => r.children.insert(0, Node::with_value("NAME", name)),
        }
        r.set_child_val("EMAIL", email);
        r.touch();
    }

    pub fn file_info(&self) -> FileInfo {
        let sour = self.header.child("SOUR");
        let software = sour
            .map(|s| {
                let name = s.child_val("NAME").or(s.value.as_deref()).unwrap_or("");
                match s.child_val("VERS") {
                    Some(v) if !name.is_empty() => format!("{name} {v}"),
                    _ => name.to_string(),
                }
            })
            .unwrap_or_default();
        let saved = self
            .header
            .child("DATE")
            .filter(|d| !d.val().is_empty())
            .map(|d| (d.val().to_string(), d.child_val("TIME").map(str::to_string)));
        let extensions = self
            .header
            .child("SCHMA")
            .map(|s| {
                s.children_with("TAG").filter_map(|t| t.val().split_whitespace().next()).map(String::from).collect()
            })
            .unwrap_or_default();
        FileInfo { version: self.version().to_string(), software, saved, extensions }
    }

    /// Avant l'enregistrement : GEDRoots devient le logiciel qui a produit le
    /// fichier (`HEAD.SOUR`), daté du moment présent (`HEAD.DATE`).
    pub fn stamp_header(&mut self) {
        let sour = self.header.ensure_child("SOUR");
        if sour.val() != "GEDROOTS" {
            // Le logiciel précédent (et ses `CORP`, `DATA`…) ne décrit plus ce fichier.
            *sour = Node::with_value("SOUR", "GEDROOTS");
        }
        sour.set_child_val("VERS", env!("CARGO_PKG_VERSION"));
        sour.set_child_val("NAME", "GEDRoots");
        let (date, time) = now_gedcom();
        let d = self.header.ensure_child("DATE");
        d.value = Some(date);
        d.set_child_val("TIME", &time);
    }

    /// Anomalies masquées dans tout le fichier, dans l'ordre du fichier.
    pub fn ignored_issues(&self) -> Vec<IgnoredIssue> {
        self.records
            .iter()
            .filter_map(|r| Some((r.xref.as_deref()?, r)))
            .flat_map(|(x, r)| {
                r.children_with(IGNORE_TAG).map(move |n| IgnoredIssue {
                    xref: x.to_string(),
                    code: n.val().to_string(),
                    label: code_label(n.val()),
                })
            })
            .collect()
    }

    /// Réaffiche une anomalie masquée. Renvoie `false` si elle ne l'était pas.
    pub fn unignore_issue(&mut self, xref: &str, code: &str) -> bool {
        let Some(r) = self.get_mut(xref) else { return false };
        let before = r.children.len();
        r.children.retain(|c| !(c.tag == IGNORE_TAG && c.val() == code));
        if r.children.len() == before {
            return false;
        }
        r.touch();
        if self.ignored_issues().is_empty() {
            self.undeclare_extension(IGNORE_TAG);
        }
        true
    }

    /// Retire la déclaration d'une extension qui n'est plus utilisée.
    pub fn undeclare_extension(&mut self, tag: &str) {
        if let Some(s) = self.header.child_mut("SCHMA") {
            s.children.retain(|t| !(t.tag == "TAG" && t.val().split_whitespace().next() == Some(tag)));
            if s.children.is_empty() {
                self.header.children.retain(|c| c.tag != "SCHMA");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const HEAD: &str = "0 HEAD\n1 GEDC\n2 VERS 7.0.13\n1 SOUR ANCESTRIS\n2 VERS 13\n2 NAME Ancestris\n2 CORP Ancestris\n1 DATE 1 NOV 2026\n2 TIME 10:00:00\n1 SUBM @B1@\n1 SCHMA\n2 TAG _GR_IGNORE https://x\n0 @B1@ SUBM\n1 NAME Antonin\n0 @I1@ INDI\n1 NAME Jean /GUYOT/\n1 _GR_IGNORE too_old\n0 TRLR\n";

    #[test]
    fn reads_settings_and_info() {
        let g = Gedcom::parse(HEAD).unwrap();
        let s = g.file_settings();
        assert_eq!(s.author, "Antonin");
        assert_eq!(s.language, "");
        let i = g.file_info();
        assert_eq!(i.version, "7.0.13");
        assert_eq!(i.software, "Ancestris 13");
        assert_eq!(i.saved, Some(("1 NOV 2026".into(), Some("10:00:00".into()))));
        assert_eq!(i.extensions, vec!["_GR_IGNORE"]);
    }

    #[test]
    fn writes_standard_header() {
        let mut g = Gedcom::parse(HEAD).unwrap();
        let s = FileSettings {
            author: "Antonin Guyot".into(),
            email: "a@example.org".into(),
            copyright: "© 2026".into(),
            description: "Famille Guyot\nBranche paternelle".into(),
            language: "fr".into(),
        };
        assert!(g.set_file_settings(&s));
        assert!(!g.set_file_settings(&s));
        let out = g.to_gedcom_string();
        assert!(out.contains("1 COPR © 2026\n"));
        assert!(out.contains("1 NOTE Famille Guyot\n2 CONT Branche paternelle\n"));
        assert!(out.contains("1 LANG fr\n"));
        assert!(out.contains("0 @B1@ SUBM\n1 NAME Antonin Guyot\n1 EMAIL a@example.org\n"));
        assert_eq!(Gedcom::parse(&out).unwrap().file_settings(), s);
    }

    #[test]
    fn creates_and_removes_submitter() {
        let mut g = Gedcom::new();
        g.set_file_settings(&FileSettings { author: "Moi".into(), ..g.file_settings() });
        let x = g.header.child("SUBM").and_then(|n| n.pointer()).unwrap().to_string();
        assert_eq!(g.get(&x).and_then(|r| r.child_val("NAME")), Some("Moi"));
        g.set_file_settings(&FileSettings { author: String::new(), ..g.file_settings() });
        assert!(g.header.child("SUBM").is_none());
        assert!(g.get(&x).is_none());
    }

    #[test]
    fn stamps_header() {
        let mut g = Gedcom::parse(HEAD).unwrap();
        g.stamp_header();
        let i = g.file_info();
        assert!(i.software.starts_with("GEDRoots "));
        assert!(g.header.child("SOUR").unwrap().child("CORP").is_none());
        assert!(i.saved.is_some_and(|(_, t)| t.is_some()));
    }

    #[test]
    fn unignores_issue_and_undeclares() {
        let mut g = Gedcom::parse(HEAD).unwrap();
        let ign = g.ignored_issues();
        assert_eq!(ign.len(), 1);
        assert_eq!(ign[0].label, "Implausible lifespan");
        assert!(g.unignore_issue("I1", "too_old"));
        assert!(g.ignored_issues().is_empty());
        assert!(g.header.child("SCHMA").is_none());
        assert!(!g.unignore_issue("I1", "too_old"));
    }
}
