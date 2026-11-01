//! Structures pour les vues en arbre (descendance, éventail).

use crate::document::Gedcom;

/// Une union dans un arbre de descendance : le conjoint et les enfants.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DescUnion {
    pub family: String,
    pub spouse: Option<String>,
    pub children: Vec<DescNode>,
}

/// Une personne et ses unions, jusqu'à la profondeur demandée.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DescNode {
    pub xref: String,
    pub unions: Vec<DescUnion>,
    /// Vrai si la personne a des descendants au-delà de la profondeur affichée.
    pub truncated: bool,
}

impl DescNode {
    /// Nombre de personnes dans l'arbre (la racine comprise, sans les conjoints).
    pub fn count(&self) -> usize {
        1 + self.unions.iter().flat_map(|u| &u.children).map(DescNode::count).sum::<usize>()
    }
}

impl Gedcom {
    /// Descendance de `xref` sur `generations` générations (1 = la personne seule).
    pub fn descendants(&self, xref: &str, generations: u32) -> Option<DescNode> {
        let mut seen = std::collections::HashSet::new();
        self.desc_rec(xref, generations, &mut seen)
    }

    fn desc_rec(&self, xref: &str, generations: u32, seen: &mut std::collections::HashSet<String>) -> Option<DescNode> {
        let i = self.individual(xref)?;
        // Protège contre les boucles dans un fichier incohérent.
        if !seen.insert(xref.to_string()) {
            return Some(DescNode { xref: xref.to_string(), unions: vec![], truncated: false });
        }
        let mut truncated = false;
        let unions = i
            .fams()
            .into_iter()
            .filter_map(|f| self.family(f))
            .map(|f| {
                let spouse = [f.husband(), f.wife()].into_iter().flatten().find(|x| *x != xref).map(str::to_string);
                let children = if generations > 1 {
                    f.children().into_iter().filter_map(|c| self.desc_rec(c, generations - 1, seen)).collect()
                } else {
                    truncated |= !f.children().is_empty();
                    vec![]
                };
                DescUnion { family: f.xref().to_string(), spouse, children }
            })
            .collect();
        Some(DescNode { xref: xref.to_string(), unions, truncated })
    }

    /// Nombre total de descendants connus (toutes générations).
    pub fn descendant_count(&self, xref: &str) -> usize {
        self.descendants(xref, u32::MAX).map(|d| d.count() - 1).unwrap_or(0)
    }
}
