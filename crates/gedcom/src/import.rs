//! Contrôles de l'assistant d'import : ce qu'un fichier venu d'un autre
//! logiciel contient de non standard, et sa conversion en GEDCOM 7.
//!
//! Chaque contrôle est indépendant : [`analyze`] liste ceux qui trouvent
//! quelque chose, [`apply`] applique ceux que l'utilisateur a gardés. Pour en
//! ajouter un, il suffit d'une entrée dans [`CHECKS`].

use crate::document::Gedcom;

/// Un contrôle d'import.
pub struct Check {
    /// Identifiant stable (`"url"`).
    pub id: &'static str,
    /// Libellé court, au singulier et au pluriel (« lien `_URL` », « liens `_URL` »).
    pub what: (&'static str, &'static str),
    /// Ce que devient la donnée une fois normalisée.
    pub into: &'static str,
    /// Nombre d'éléments concernés dans le fichier.
    count: fn(&Gedcom) -> usize,
    /// Applique la conversion, renvoie le nombre d'éléments convertis.
    fix: fn(&mut Gedcom) -> usize,
}

/// Les contrôles, dans l'ordre où l'assistant les présente.
pub static CHECKS: &[Check] = &[
    Check {
        id: "url",
        what: ("lien _URL", "liens _URL"),
        into: "liens multimédia GEDCOM 7",
        count: Gedcom::legacy_url_count,
        fix: Gedcom::migrate_url_tags,
    },
    Check {
        id: "prim",
        what: ("photo principale _PRIM", "photos principales _PRIM"),
        into: "premier média de la fiche",
        count: Gedcom::prim_count,
        fix: Gedcom::migrate_prim_tags,
    },
];

/// Ce qu'un contrôle a trouvé.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    pub id: &'static str,
    pub count: usize,
    /// « 213 liens _URL »
    pub label: String,
    pub into: &'static str,
}

/// Contrôles qui trouvent quelque chose à normaliser dans ce fichier.
pub fn analyze(g: &Gedcom) -> Vec<Finding> {
    CHECKS
        .iter()
        .filter_map(|c| {
            let count = (c.count)(g);
            (count > 0).then(|| Finding {
                id: c.id,
                count,
                label: format!("{count} {}", if count == 1 { c.what.0 } else { c.what.1 }),
                into: c.into,
            })
        })
        .collect()
}

/// Applique les contrôles choisis. Renvoie, pour chacun, le nombre d'éléments convertis.
pub fn apply(g: &mut Gedcom, ids: &[&str]) -> Vec<(&'static str, usize)> {
    CHECKS.iter().filter(|c| ids.contains(&c.id)).map(|c| (c.id, (c.fix)(g))).collect()
}
