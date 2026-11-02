//! Structures for the chart views (descendants, fan chart).

use crate::document::Gedcom;

/// A union in a descendant chart: the spouse and the children.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DescUnion {
    pub family: String,
    pub spouse: Option<String>,
    pub children: Vec<DescNode>,
}

/// A person and their unions, down to the requested depth.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DescNode {
    pub xref: String,
    pub unions: Vec<DescUnion>,
    /// True if the person has descendants beyond the displayed depth.
    pub truncated: bool,
}

impl DescNode {
    /// Number of people in the tree (the root included, without the spouses).
    pub fn count(&self) -> usize {
        1 + self.unions.iter().flat_map(|u| &u.children).map(DescNode::count).sum::<usize>()
    }
}

impl Gedcom {
    /// Descendants of `xref` over `generations` generations (1 = the person alone).
    pub fn descendants(&self, xref: &str, generations: u32) -> Option<DescNode> {
        let mut seen = std::collections::HashSet::new();
        self.desc_rec(xref, generations, &mut seen)
    }

    fn desc_rec(&self, xref: &str, generations: u32, seen: &mut std::collections::HashSet<String>) -> Option<DescNode> {
        let i = self.individual(xref)?;
        // Guards against loops in an inconsistent file.
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

    /// Total number of known descendants (every generation).
    pub fn descendant_count(&self, xref: &str) -> usize {
        self.descendants(xref, u32::MAX).map(|d| d.count() - 1).unwrap_or(0)
    }
}
