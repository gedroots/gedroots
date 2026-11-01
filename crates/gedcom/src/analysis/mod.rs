//! Outils d'analyse de l'arbre : contrôle de cohérence, lien de parenté,
//! chronologie avec âges et statistiques. Ces outils ne font que lire le
//! fichier, sauf [`crate::Gedcom::ignore_issue`] qui ajoute une extension `_GR_IGNORE`.

pub mod check;
pub mod kinship;
pub mod span;
pub mod stats;
pub mod timeline;
