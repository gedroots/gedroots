//! Tree analysis tools: consistency check, relationship, timeline with ages and statistics. These
//! tools only read the file, except [`crate::Gedcom::ignore_issue`], which adds a `_GR_IGNORE`
//! extension.

pub mod check;
pub mod kinship;
pub mod span;
pub mod stats;
pub mod timeline;
