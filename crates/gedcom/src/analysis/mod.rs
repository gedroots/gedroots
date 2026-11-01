//! Outils d'analyse de l'arbre : contrôle de cohérence, lien de parenté,
//! chronologie avec âges et statistiques. Ces outils ne font que lire le
//! fichier, sauf [`Gedcom::ignore_issue`] qui ajoute une extension `_GR_IGNORE`.

pub mod check;
pub mod kinship;
pub mod span;
pub mod stats;
pub mod timeline;

use crate::document::Gedcom;
use crate::node::Node;

/// Extensions GEDCOM propres à GedRoots, avec l'URI qui les documente.
pub const EXTENSIONS: &[(&str, &str)] =
    &[("_GR_IGNORE", "https://github.com/antoninguyot/gedroots/blob/main/docs/extensions.md#_gr_ignore")];

impl Gedcom {
    /// Déclare une extension GedRoots dans `HEAD.SCHMA` (exigé par GEDCOM 7).
    pub fn declare_extension(&mut self, tag: &str) {
        let Some((_, uri)) = EXTENSIONS.iter().find(|(t, _)| *t == tag) else { return };
        let value = format!("{tag} {uri}");
        let schma = self.header.ensure_child("SCHMA");
        if schma.children_with("TAG").any(|n| n.val().split_whitespace().next() == Some(tag)) {
            return;
        }
        schma.children.push(Node::with_value("TAG", value));
    }
}
