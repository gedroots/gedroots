//! Contrôles de l'assistant d'import : ce qu'un fichier venu d'un autre
//! logiciel contient de non standard, et sa conversion en GEDCOM 7.
//!
//! Chaque contrôle est indépendant : [`analyze`] liste ceux qui trouvent
//! quelque chose, [`apply`] applique ceux que l'utilisateur a gardés. Pour en
//! ajouter un, il suffit d'une entrée dans [`CHECKS`].

use crate::document::Gedcom;
use rust_i18n::t;
use std::borrow::Cow;

/// Un contrôle d'import.
pub struct Check {
    /// Identifiant stable (`"url"`).
    pub id: &'static str,
    /// Nombre d'éléments concernés dans le fichier.
    count: fn(&Gedcom) -> usize,
    /// Applique la conversion, renvoie le nombre d'éléments convertis.
    fix: fn(&mut Gedcom) -> usize,
}

/// Les contrôles, dans l'ordre où l'assistant les présente.
pub static CHECKS: &[Check] = &[
    Check { id: "url", count: Gedcom::legacy_url_count, fix: Gedcom::migrate_url_tags },
    Check { id: "prim", count: Gedcom::prim_count, fix: Gedcom::migrate_prim_tags },
    Check { id: "events", count: Gedcom::extension_event_count, fix: Gedcom::migrate_extension_events },
];

/// Ce qu'un contrôle a trouvé.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    pub id: &'static str,
    pub count: usize,
    /// « 213 liens _URL »
    pub label: String,
    pub into: Cow<'static, str>,
}

/// « 213 liens _URL » et ce qu'ils deviennent, pour le contrôle `id`.
fn texts(id: &str, count: usize) -> (Cow<'static, str>, Cow<'static, str>) {
    match id {
        "url" => (tn!("import.url", count), t!("import.url_into")),
        "prim" => (tn!("import.prim", count), t!("import.prim_into")),
        _ => (tn!("import.events", count), t!("import.events_into")),
    }
}

/// Contrôles qui trouvent quelque chose à normaliser dans ce fichier.
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

/// Applique les contrôles choisis. Renvoie, pour chacun, le nombre d'éléments convertis.
pub fn apply(g: &mut Gedcom, ids: &[&str]) -> Vec<(&'static str, usize)> {
    CHECKS.iter().filter(|c| ids.contains(&c.id)).map(|c| (c.id, (c.fix)(g))).collect()
}
