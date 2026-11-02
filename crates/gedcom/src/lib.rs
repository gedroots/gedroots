//! Bibliothèque GEDCOM 7 de GedRoots.
//!
//! - [`node`] : arbre générique sans perte (toutes les balises sont conservées) ;
//! - [`parse`] : lecture/écriture du texte GEDCOM ;
//! - [`document`] : un fichier chargé, avec index des identifiants et opérations d'édition ;
//! - [`model`] : vues typées (individus, familles, sources, lieux, médias…) et formulaires d'édition ;
//! - [`schema`] : schéma GEDCOM 7 (sous-structures permises, types de valeurs) ;
//! - [`complete`] : autocomplétion du texte GEDCOM brut ;
//! - [`geo`] : coordonnées des lieux (`MAP`) et projection de la carte ;
//! - [`search`] : recherche et filtres sur les individus ;
//! - [`relations`] : ajout/liaison/suppression de proches, lieux et médias ;
//! - [`events`] : modification et suppression des événements ;
//! - [`import`] : contrôles de l'assistant d'import (balises non standard → GEDCOM 7) ;
//! - [`custom`] : format des lieux (`HEAD.PLAC.FORM`) et types d'événements personnalisés ;
//! - [`settings`] : réglages du fichier (auteur, droits, langue…) dans l'en-tête ;
//! - [`weblink`] : liens web en GEDCOM 7 standard (`OBJE` dont le `FILE` est une URL) ;
//! - [`crop`] : recadrage des médias (`CROP`) et personnes identifiées sur les photos ;
//! - [`multipage`] : médias à plusieurs pages (plusieurs `FILE`), fusion et séparation ;
//! - [`pedigree`] : descendance pour les vues en arbre ;
//! - [`date`] : dates GEDCOM (y compris calendrier républicain) affichées en français ;
//! - [`template`] et [`entry`] : modèles d'actes et saisie guidée des sources.

pub mod analysis;
pub mod complete;
pub mod crop;
pub mod custom;
pub mod date;
pub mod document;
pub mod entry;
pub mod events;
pub mod geo;
pub mod import;
pub mod model;
pub mod multipage;
pub mod node;
pub mod parse;
pub mod pedigree;
pub mod relations;
pub mod schema;
pub mod search;
pub mod settings;
pub mod template;
pub mod weblink;

pub use crop::{Crop, MediaLink, Rect};
pub use custom::{EventType, Jurisdiction, PlaceRewrite};
pub use date::{Calendar, Date, DateKind, Parsed as DateInput, SimpleDate, parse_input as parse_date_input};
pub use document::{Gedcom, RecordKind};
pub use entry::{Entry, EntryResult, MediaChoice, Participant, PersonChoice, SourceChoice, Suggestion};
pub use events::PersonEvent;
pub use model::*;
pub use multipage::{MediaFile, PAGE_TAG, page_of_link, strip_page_suffix};
pub use node::Node;
pub use parse::{ParseError, node_from_string, node_to_string};
pub use pedigree::{DescNode, DescUnion};
pub use relations::{FAMILY_EVENTS, INDIVIDUAL_EVENTS, NewEvent, Relation, is_media_file, mime_for, title_from_file};
pub use search::{IndividualQuery, has_token, toggle_token};
pub use settings::{FileInfo, FileSettings, IgnoredIssue};
pub use template::{FieldDef, PersonField, Relation as RoleRelation, RoleDef, Template, builtin_templates};
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
