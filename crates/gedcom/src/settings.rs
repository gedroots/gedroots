//! File settings (GEDRoots › File Settings… menu): what describes the database itself, stored in the
//! `HEAD` header in standard GEDCOM 7.
//!
//! - author: `HEAD.SUBM` → `SUBM` record (`NAME`, `EMAIL`);
//! - copyright: `HEAD.COPR`; description: `HEAD.NOTE`; language: `HEAD.LANG`;
//! - read-only information: version, software and date of the last save (`HEAD.SOUR`, `HEAD.DATE`,
//!   updated by [`Gedcom::stamp_header`]), declared extensions (`HEAD.SCHMA`).
//!
//! Hidden problems (`_GR_IGNORE`) are listed and shown again here too.

use crate::analysis::check::{IGNORE_TAG, code_label};
use crate::document::{Gedcom, RecordKind};
use crate::node::{Node, now_gedcom};

/// Editable fields of the panel.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FileSettings {
    pub author: String,
    pub email: String,
    pub copyright: String,
    pub description: String,
    /// BCP 47 language code (`fr`), empty if absent.
    pub language: String,
}

/// Read-only information.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FileInfo {
    /// `7.0.13`
    pub version: String,
    /// “Ancestris 13”, “GEDRoots 0.1.0”… (empty if unknown).
    pub software: String,
    /// Date (and time) of the last save declared in `HEAD.DATE`, in GEDCOM format.
    pub saved: Option<(String, Option<String>)>,
    /// Extensions declared in `HEAD.SCHMA` (tags only).
    pub extensions: Vec<String>,
}

/// A problem hidden by `_GR_IGNORE`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IgnoredIssue {
    pub xref: String,
    pub code: String,
    pub label: std::borrow::Cow<'static, str>,
    /// Why it was hidden (`NOTE` under the tag), if given.
    pub reason: String,
}

impl Gedcom {
    /// Author declared in the header (`HEAD.SUBM`), if it still exists.
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

    /// Saves the settings. Returns `false` if nothing changed.
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
            // No more author: remove the link, and the record if it only serves the header.
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
        // `NAME` is required on a `SUBM`: keep it, even empty.
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

    /// Was the file last saved by GEDRoots (`HEAD.SOUR GEDROOTS`)? Otherwise it comes from other
    /// software, and opens through the import wizard.
    pub fn written_by_gedroots(&self) -> bool {
        self.header.child_val("SOUR") == Some("GEDROOTS")
    }

    /// Before saving: GEDRoots becomes the software that produced the file (`HEAD.SOUR`), dated now
    /// (`HEAD.DATE`).
    pub fn stamp_header(&mut self) {
        let sour = self.header.ensure_child("SOUR");
        if sour.val() != "GEDROOTS" {
            // The previous software (and its `CORP`, `DATA`…) no longer describes this file.
            *sour = Node::with_value("SOUR", "GEDROOTS");
        }
        sour.set_child_val("VERS", env!("CARGO_PKG_VERSION"));
        sour.set_child_val("NAME", "GEDRoots");
        let (date, time) = now_gedcom();
        let d = self.header.ensure_child("DATE");
        d.value = Some(date);
        d.set_child_val("TIME", &time);
    }

    /// Hidden problems in the whole file, in file order.
    pub fn ignored_issues(&self) -> Vec<IgnoredIssue> {
        self.records
            .iter()
            .filter_map(|r| Some((r.xref.as_deref()?, r)))
            .flat_map(|(x, r)| {
                r.children_with(IGNORE_TAG).map(move |n| IgnoredIssue {
                    xref: x.to_string(),
                    code: n.val().to_string(),
                    label: code_label(n.val()),
                    reason: n.child("NOTE").map(|n| n.text()).unwrap_or_default(),
                })
            })
            .collect()
    }

    /// Shows a hidden problem again. Returns `false` if it was not hidden.
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

    /// Removes the declaration of an extension that is no longer used.
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

    const HEAD: &str = "0 HEAD\n1 GEDC\n2 VERS 7.0.13\n1 SOUR ANCESTRIS\n2 VERS 13\n2 NAME Ancestris\n2 CORP Ancestris\n1 DATE 1 NOV 2026\n2 TIME 10:00:00\n1 SUBM @B1@\n1 SCHMA\n2 TAG _GR_IGNORE https://x\n0 @B1@ SUBM\n1 NAME Jeanne\n0 @I1@ INDI\n1 NAME Jean /GUYOT/\n1 _GR_IGNORE too_old\n0 TRLR\n";

    #[test]
    fn reads_settings_and_info() {
        let g = Gedcom::parse(HEAD).unwrap();
        let s = g.file_settings();
        assert_eq!(s.author, "Jeanne");
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
            author: "Jeanne Martin".into(),
            email: "a@example.org".into(),
            copyright: "© 2026".into(),
            description: "Famille Martin\nBranche paternelle".into(),
            language: "fr".into(),
        };
        assert!(g.set_file_settings(&s));
        assert!(!g.set_file_settings(&s));
        let out = g.to_gedcom_string();
        assert!(out.contains("1 COPR © 2026\n"));
        assert!(out.contains("1 NOTE Famille Martin\n2 CONT Branche paternelle\n"));
        assert!(out.contains("1 LANG fr\n"));
        assert!(out.contains("0 @B1@ SUBM\n1 NAME Jeanne Martin\n1 EMAIL a@example.org\n"));
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
        // Last saved by Ancestris: opens through the import wizard, until GEDRoots saves it.
        assert!(!g.written_by_gedroots());
        g.stamp_header();
        assert!(g.written_by_gedroots());
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
