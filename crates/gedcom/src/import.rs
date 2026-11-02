//! Checks of the import assistant: what a file from other software contains that is not standard,
//! and its conversion to GEDCOM 7.
//!
//! Each check is independent: [`analyze`] lists those that find something, [`apply`] applies those
//! the user kept. Adding one only takes an entry in [`CHECKS`].

use crate::document::Gedcom;
use rust_i18n::t;
use std::borrow::Cow;

/// An import check.
pub struct Check {
    /// Identifiant stable (`"url"`).
    pub id: &'static str,
    /// Number of items concerned in the file.
    count: fn(&Gedcom) -> usize,
    /// Applies the conversion, returns the number of items converted.
    fix: fn(&mut Gedcom) -> usize,
}

/// The checks, in the order the assistant shows them.
pub static CHECKS: &[Check] = &[
    Check { id: "url", count: Gedcom::legacy_url_count, fix: Gedcom::migrate_url_tags },
    Check { id: "prim", count: Gedcom::prim_count, fix: Gedcom::migrate_prim_tags },
    Check { id: "events", count: Gedcom::extension_event_count, fix: Gedcom::migrate_extension_events },
];

/// What a check found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    pub id: &'static str,
    pub count: usize,
    /// “213 _URL links”
    pub label: String,
    pub into: Cow<'static, str>,
}

/// “213 _URL links” and what they become, for check `id`.
fn texts(id: &str, count: usize) -> (Cow<'static, str>, Cow<'static, str>) {
    match id {
        "url" => (tn!("import.url", count), t!("import.url_into")),
        "prim" => (tn!("import.prim", count), t!("import.prim_into")),
        _ => (tn!("import.events", count), t!("import.events_into")),
    }
}

/// Checks that find something to normalize in this file.
pub fn analyze(g: &Gedcom) -> Vec<Finding> {
    CHECKS
        .iter()
        .filter_map(|c| {
            let count = (c.count)(g);
            (count > 0).then(|| {
                let (label, into) = texts(c.id, count);
                Finding { id: c.id, count, label: label.into_owned(), into }
            })
        })
        .collect()
}

/// Applies the chosen checks. Returns, for each, the number of items converted.
pub fn apply(g: &mut Gedcom, ids: &[&str]) -> Vec<(&'static str, usize)> {
    CHECKS.iter().filter(|c| ids.contains(&c.id)).map(|c| (c.id, (c.fix)(g))).collect()
}
