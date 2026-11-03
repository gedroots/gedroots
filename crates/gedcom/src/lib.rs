//! GEDRoots' GEDCOM 7 library.
//!
//! - [`node`]: lossless generic tree (every tag is kept);
//! - [`parse`]: reading/writing GEDCOM text;
//! - [`document`]: a loaded file, with an identifier index and editing operations;
//! - [`model`]: typed views (individuals, families, sources, places, media…) and edit forms;
//! - [`schema`]: GEDCOM 7 schema (allowed substructures, value types);
//! - [`complete`]: autocompletion of raw GEDCOM text;
//! - [`geo`]: place coordinates (`MAP`) and map projection;
//! - [`gazetteer`]: creating places (splitting by format, localities, `EXID`);
//! - [`search`]: search and filters on individuals;
//! - [`relations`]: adding/linking/removing relatives, places and media;
//! - [`research`]: records, search and gaps for AI agents (read only, living people hidden);
//! - [`events`]: editing and deleting events;
//! - [`duplicates`]: possible duplicate individuals, “not a duplicate” marks; [`merge`]: merging two individuals;
//! - [`quality`]: Data Quality page (consistency, GEDCOM 7 schema, empty or unused items);
//! - [`gedzip`]: preparing the GEDZIP export (`gedcom.ged` + media in an archive);
//! - [`import`]: checks of the import assistant (non-standard tags → GEDCOM 7);
//! - [`custom`]: place format (`HEAD.PLAC.FORM`) and custom event types;
//! - [`media_links`]: “Media” gallery (what is linked where, where to attach a new media);
//! - [`media_naming`]: folder and name template of media files (`_GR_MEDIA`);
//! - [`settings`]: file settings (author, copyright, language…) in the header;
//! - [`kinds`]: types of sources (`DATA` / `EVEN`) and media (`MEDI`), portrait;
//! - [`weblink`]: web links in standard GEDCOM 7 (`OBJE` whose `FILE` is a URL);
//! - [`crop`]: media cropping (`CROP`) and people identified in photos;
//! - [`multipage`]: multi-page media (several `FILE`s), merging and splitting;
//! - [`pedigree`]: descendants for the chart views;
//! - [`date`]: GEDCOM dates (French Republican calendar included), displayed in words;
//! - [`act`]: records of a source (entry, reading from citations, editing);
//! - [`associations`]: witnesses, godparents… of events (`ASSO`) and the events a person attended.

rust_i18n::i18n!("locales", fallback = "en");

/// URI of a GEDRoots extension tag, declared in `HEAD.SCHMA`: its section in
/// `docs/extensions.md`, under the `repository` of the workspace `Cargo.toml`.
macro_rules! extension_uri {
    ($anchor:literal) => {
        concat!(env!("CARGO_PKG_REPOSITORY"), "/blob/main/docs/extensions.md#", $anchor)
    };
}

#[macro_use]
pub mod i18n;

pub mod act;
pub mod analysis;
pub mod associations;
pub mod change;
pub mod complete;
pub mod crop;
pub mod custom;
pub mod date;
pub mod document;
pub mod duplicates;
pub mod events;
pub mod gazetteer;
pub mod gedcom551;
pub mod gedzip;
pub mod geo;
pub mod import;
pub mod kinds;
pub mod media_links;
pub mod media_naming;
pub mod merge;
pub mod model;
pub mod multipage;
pub mod node;
pub mod parse;
pub mod pedigree;
pub mod phonetic;
pub mod proposal;
pub mod quality;
pub mod query;
pub mod relations;
pub mod research;
pub mod saved;
pub mod schema;
pub mod search;
pub mod settings;
pub mod starter;
pub mod versions;
pub mod weblink;
pub mod websearch;

pub use act::{Act, ActKey, ActResult, ActRole, Fact, MediaChoice, Participant, PersonChoice, PersonField, Suggestion};
pub use associations::{Associate, AttendedEvent};
pub use crop::{Crop, MediaLink, Rect};
pub use custom::{EventType, Jurisdiction, PlaceRewrite};
pub use date::{Calendar, Date, DateKind, Parsed as DateInput, SimpleDate, parse_input as parse_date_input};
pub use document::{Gedcom, RecordKind};
pub use events::PersonEvent;
pub use kinds::{MEDIUMS, SourceContent, medium, medium_label, mediums, parse_event_list, set_medium};
pub use media_links::{GalleryGroup, GalleryTarget, MediaHost};
pub use media_naming::{
    MEDIA_TOKENS, MediaContext, MediaFolder, MediaNaming, Numbering, media_path, media_type_labels,
};
pub use model::*;
pub use multipage::{MediaFile, PAGE_TAG, page_of_link, strip_page_suffix};
pub use node::Node;
pub use parse::{ParseError, node_from_string, node_to_string};
pub use pedigree::{DescNode, DescUnion};
pub use query::{Group, MediaIndex, RecordQuery, Scope, chip_label, chips, criterion, other_type, web_type};
pub use relations::{FAMILY_EVENTS, INDIVIDUAL_EVENTS, NewEvent, Relation, is_media_file, mime_for, title_from_file};
pub use search::{IndividualQuery, Kin, has_token, toggle_token};
pub use settings::{FileInfo, FileSettings, IgnoredIssue};
pub use weblink::{WebLink, WebLinkKind, host, is_web_url, normalize_url};
pub use websearch::{SearchSite, WEB_TOKENS};

/// Loads a GEDCOM file from the disk.
pub fn load(path: impl AsRef<std::path::Path>) -> Result<Gedcom, String> {
    let bytes = std::fs::read(path.as_ref()).map_err(|e| e.to_string())?;
    let text = String::from_utf8(bytes).map_err(|_| rust_i18n::t!("error.not_utf8").into_owned())?;
    Gedcom::parse(&text).map_err(|e| e.to_string())
}

/// Saves the file, keeping a `.bak` copy of the previous version.
pub fn save(ged: &Gedcom, path: impl AsRef<std::path::Path>) -> std::io::Result<()> {
    let path = path.as_ref();
    if path.exists() {
        let bak = path.with_extension("ged.bak");
        std::fs::copy(path, bak)?;
    }
    let tmp = path.with_extension("ged.tmp");
    std::fs::write(&tmp, ged.to_gedcom_string())?;
    std::fs::rename(tmp, path)
}

#[cfg(test)]
mod tests {
    #[test]
    fn extension_uris_point_to_the_docs() {
        assert_eq!(
            crate::document::ROOT_URI,
            "https://github.com/gedroots/gedroots/blob/main/docs/extensions.md#_gr_root"
        );
    }
}
