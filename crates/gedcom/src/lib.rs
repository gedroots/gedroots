//! Bibliothèque GEDCOM 7 de GedRoots.
//!
//! - [`node`] : arbre générique sans perte (toutes les balises sont conservées) ;
//! - [`parse`] : lecture/écriture du texte GEDCOM ;
//! - [`document`] : un fichier chargé, avec index des identifiants et opérations d'édition ;
//! - [`model`] : vues typées (individus, familles, sources, lieux, médias…) et formulaires d'édition ;
//! - [`date`] : dates GEDCOM (y compris calendrier républicain) affichées en français ;
//! - [`template`] et [`entry`] : modèles d'actes et saisie guidée des sources.

pub mod date;
pub mod document;
pub mod entry;
pub mod model;
pub mod node;
pub mod parse;
pub mod template;

pub use date::Date;
pub use document::{Gedcom, RecordKind};
pub use entry::{Entry, EntryResult, MediaChoice, Participant, PersonChoice, SourceChoice, Suggestion};
pub use model::*;
pub use node::Node;
pub use parse::{ParseError, node_from_string, node_to_string};
pub use template::{FieldDef, PersonField, Relation as RoleRelation, RoleDef, Template, builtin_templates};

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
