//! Bibliothèque GEDCOM 7 de GEDRoots.
//!
//! - [`node`] : arbre générique sans perte (toutes les balises sont conservées) ;
//! - [`parse`] : lecture/écriture du texte GEDCOM ;
//! - [`document`] : un fichier chargé, avec index des identifiants et opérations d'édition ;
//! - [`model`] : vues typées (individus, familles, sources, lieux, médias…) et formulaires d'édition ;
//! - [`schema`] : schéma GEDCOM 7 (sous-structures permises, types de valeurs) ;
//! - [`complete`] : autocomplétion du texte GEDCOM brut ;
//! - [`geo`] : coordonnées des lieux (`MAP`) et projection de la carte ;
//! - [`gazetteer`] : création de lieux (découpage selon le format, lieux-dits, `EXID`) ;
//! - [`search`] : recherche et filtres sur les individus ;
//! - [`relations`] : ajout/liaison/suppression de proches, lieux et médias ;
//! - [`events`] : modification et suppression des événements ;
//! - [`quality`] : page Qualité des données (cohérence, schéma GEDCOM 7, éléments vides ou inutilisés) ;
//! - [`gedzip`] : préparation de l'export GEDZIP (`gedcom.ged` + médias dans une archive) ;
//! - [`import`] : contrôles de l'assistant d'import (balises non standard → GEDCOM 7) ;
//! - [`custom`] : format des lieux (`HEAD.PLAC.FORM`) et types d'événements personnalisés ;
//! - [`media_links`] : galerie « Médias » (ce qui est lié où, où joindre un nouveau média) ;
//! - [`media_naming`] : dossier et modèle de nom des fichiers médias (`_GR_MEDIA`) ;
//! - [`settings`] : réglages du fichier (auteur, droits, langue…) dans l'en-tête ;
//! - [`kinds`] : types des sources (`DATA` / `EVEN`) et des médias (`MEDI`), portrait ;
//! - [`weblink`] : liens web en GEDCOM 7 standard (`OBJE` dont le `FILE` est une URL) ;
//! - [`crop`] : recadrage des médias (`CROP`) et personnes identifiées sur les photos ;
//! - [`multipage`] : médias à plusieurs pages (plusieurs `FILE`), fusion et séparation ;
//! - [`pedigree`] : descendance pour les vues en arbre ;
//! - [`date`] : dates GEDCOM (y compris calendrier républicain) affichées en français ;
//! - [`act`] : actes d'une source (saisie, lecture depuis les citations, modification).

pub mod act;
pub mod analysis;
pub mod complete;
pub mod crop;
pub mod custom;
pub mod date;
pub mod document;
pub mod events;
pub mod gazetteer;
pub mod gedcom551;
pub mod gedzip;
pub mod geo;
pub mod import;
pub mod kinds;
pub mod media_links;
pub mod media_naming;
pub mod model;
pub mod multipage;
pub mod node;
pub mod parse;
pub mod pedigree;
pub mod phonetic;
pub mod quality;
pub mod query;
pub mod relations;
pub mod saved;
pub mod schema;
pub mod search;
pub mod settings;
pub mod weblink;

pub use act::{Act, ActKey, ActResult, ActRole, MediaChoice, Participant, PersonChoice, PersonField, Suggestion};
pub use crop::{Crop, MediaLink, Rect};
pub use custom::{EventType, Jurisdiction, PlaceRewrite};
pub use date::{Calendar, Date, DateKind, Parsed as DateInput, SimpleDate, parse_input as parse_date_input};
pub use document::{Gedcom, RecordKind};
pub use events::PersonEvent;
pub use kinds::{MEDIUMS, SourceContent, medium, medium_label, parse_event_list, set_medium};
pub use media_links::{GalleryGroup, GalleryTarget, MediaHost};
pub use media_naming::{
    MEDIA_TOKENS, MediaContext, MediaFolder, MediaNaming, Numbering, media_path, media_type_labels,
};
pub use model::*;
pub use multipage::{MediaFile, PAGE_TAG, page_of_link, strip_page_suffix};
pub use node::Node;
pub use parse::{ParseError, node_from_string, node_to_string};
pub use pedigree::{DescNode, DescUnion};
pub use query::{MediaIndex, OTHER_TYPE, RecordQuery, Scope, chip_label, chips, criterion};
pub use relations::{FAMILY_EVENTS, INDIVIDUAL_EVENTS, NewEvent, Relation, is_media_file, mime_for, title_from_file};
pub use search::{IndividualQuery, Kin, has_token, toggle_token};
pub use settings::{FileInfo, FileSettings, IgnoredIssue};
pub use weblink::{WebLink, WebLinkKind, host, is_web_url, normalize_url};

/// Charge un fichier GEDCOM depuis le disque.
pub fn load(path: impl AsRef<std::path::Path>) -> Result<Gedcom, String> {
    let bytes = std::fs::read(path.as_ref()).map_err(|e| e.to_string())?;
    let text =
        String::from_utf8(bytes).map_err(|_| "le fichier n'est pas en UTF-8 (GEDCOM 7 exige UTF-8)".to_string())?;
    Gedcom::parse(&text).map_err(|e| e.to_string())
}

/// Enregistre le fichier, en gardant une copie `.bak` de la version précédente.
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
