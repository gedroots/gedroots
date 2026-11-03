//! The file's context for AI agents (GEDRoots' MCP server): what the user set in the file and how
//! its records are written, so that an agent writes new ones the same way.
//!
//! Nothing is inferred or classified here: the agent gets the settings, the description the user
//! wrote, the vocabulary in use and the latest records as raw GEDCOM, and draws its own
//! conclusions. Living people are never among the examples.

use crate::date::Date;
use crate::document::{Gedcom, RecordKind};
use crate::node::Node;
use crate::parse::node_to_string;
use std::collections::HashMap;

/// Lines kept from a record given as an example; longer records are cut.
const EXAMPLE_LINES: usize = 80;
/// Occupations listed, most frequent first.
const OCCUPATIONS: usize = 40;

/// What the user set in the file and how it is written.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, schemars::JsonSchema))]
pub struct FileContext {
    /// Description of the file written by the user (File Settings › General, `HEAD.NOTE`), word for
    /// word: the conventions they want followed, among other things.
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "String::is_empty"))]
    pub description: String,
    /// Language of the file (`HEAD.LANG`, BCP 47).
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "String::is_empty"))]
    pub language: String,
    /// Jurisdictions of a place, from the most precise to the broadest (`HEAD.PLAC.FORM`).
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "Vec::is_empty"))]
    pub place_format: Vec<String>,
    /// Number of each event and attribute tag in individuals and families (`BIRT`, `OCCU`…).
    pub event_tags: Vec<Count>,
    /// Custom event types (`EVEN` + `TYPE`): those used and those declared for later.
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "Vec::is_empty"))]
    pub event_types: Vec<EventTypeUse>,
    /// Occupations (`OCCU`) as written, most frequent first.
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "Vec::is_empty"))]
    pub occupations: Vec<Count>,
    /// Where GEDRoots files the media and how it names them (File Settings › Media).
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "Option::is_none"))]
    pub media_folders: Option<MediaFolders>,
    /// The latest records changed, as raw GEDCOM: how this file writes them.
    pub examples: Examples,
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, schemars::JsonSchema))]
pub struct Count {
    pub value: String,
    pub count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, schemars::JsonSchema))]
pub struct EventTypeUse {
    /// The events' `TYPE`.
    #[cfg_attr(feature = "serde", serde(rename = "type"))]
    pub ty: String,
    /// Label shown in GEDRoots, when it differs from the `TYPE`.
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "Option::is_none"))]
    pub label: Option<String>,
    /// Family event (under `FAM`) rather than individual.
    pub family: bool,
    pub count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, schemars::JsonSchema))]
pub struct MediaFolders {
    /// Media folder, relative to the `.ged`.
    pub root: String,
    pub subfolders: Vec<MediaSubfolder>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, schemars::JsonSchema))]
pub struct MediaSubfolder {
    pub dir: String,
    /// What it receives: event tags, custom types, `INDI` / `FAM`; empty for everything else.
    pub types: Vec<String>,
    /// File name template; empty to keep the original name.
    pub name_template: String,
}

/// Raw GEDCOM of the latest records changed, most recent first.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, schemars::JsonSchema))]
pub struct Examples {
    pub sources: Vec<String>,
    /// Never a person who may be living.
    pub people: Vec<String>,
    /// Never a family where a spouse may be living.
    pub families: Vec<String>,
    pub media: Vec<String>,
    pub repositories: Vec<String>,
}

impl Gedcom {
    /// The file's context, with `examples` records of each kind.
    pub fn file_context(&self, examples: usize, this_year: i32) -> FileContext {
        let header = &self.header;
        let naming = self.media_naming();
        FileContext {
            description: header.child("NOTE").map(Node::text).unwrap_or_default().trim().to_string(),
            language: header.child_val("LANG").unwrap_or("").trim().to_string(),
            place_format: self.place_form().into_iter().filter(|j| !j.is_empty()).collect(),
            event_tags: self.event_tags(),
            event_types: self
                .event_types()
                .into_iter()
                .map(|t| EventTypeUse {
                    label: (t.label != t.ty).then_some(t.label),
                    ty: t.ty,
                    family: t.family,
                    count: t.uses,
                })
                .collect(),
            occupations: self.occupations(),
            media_folders: (!naming.is_empty()).then(|| MediaFolders {
                root: naming.root.clone(),
                subfolders: naming
                    .folders
                    .iter()
                    .map(|f| MediaSubfolder {
                        dir: f.dir.clone(),
                        types: f.types.clone(),
                        name_template: f.template.clone(),
                    })
                    .collect(),
            }),
            examples: Examples {
                sources: self.latest(RecordKind::Source, examples, |_| true),
                people: self.latest(RecordKind::Individual, examples, |n| {
                    n.xref.as_deref().is_some_and(|x| !self.is_living(x, this_year))
                }),
                families: self.latest(RecordKind::Family, examples, |n| {
                    ["HUSB", "WIFE"]
                        .iter()
                        .filter_map(|t| n.child(t).and_then(Node::pointer))
                        .all(|x| !self.is_living(x, this_year))
                }),
                media: self.latest(RecordKind::Media, examples, |_| true),
                repositories: self.latest(RecordKind::Repository, examples, |_| true),
            },
        }
    }

    /// Events and attributes of individuals and families, by tag, most frequent first.
    fn event_tags(&self) -> Vec<Count> {
        let mut counts: HashMap<&str, usize> = HashMap::new();
        for r in self.records.iter().filter(|r| r.tag == "INDI" || r.tag == "FAM") {
            for c in r.children.iter().filter(|c| is_event_tag(&c.tag)) {
                *counts.entry(c.tag.as_str()).or_default() += 1;
            }
        }
        sorted(counts.into_iter().map(|(v, n)| (v.to_string(), n)))
    }

    fn occupations(&self) -> Vec<Count> {
        let mut counts: HashMap<String, usize> = HashMap::new();
        for r in self.records_of(RecordKind::Individual) {
            for o in r.children_with("OCCU") {
                let v = o.val().trim();
                if !v.is_empty() {
                    *counts.entry(v.to_string()).or_default() += 1;
                }
            }
        }
        let mut all = sorted(counts.into_iter());
        all.truncate(OCCUPATIONS);
        all
    }

    /// Raw GEDCOM of the last `n` records of a kind that pass `keep`, by date of last change
    /// (`CHAN`); records without one count as older, in file order.
    fn latest(&self, kind: RecordKind, n: usize, keep: impl Fn(&Node) -> bool) -> Vec<String> {
        if n == 0 {
            return Vec::new();
        }
        let mut records: Vec<(usize, &Node)> = self.records_of(kind).enumerate().collect();
        records.sort_by_cached_key(|(i, r)| std::cmp::Reverse((changed(r), *i)));
        records.into_iter().filter(|(_, r)| keep(r)).take(n).map(|(_, r)| example(r)).collect()
    }
}

/// Date and time of the last change (`CHAN.DATE`), comparable.
fn changed(r: &Node) -> Option<((i32, u8, u8), String)> {
    let d = r.child("CHAN")?.child("DATE")?;
    let date = Date::parse(d.val());
    date.primary()?;
    Some((date.sort_key(), d.child_val("TIME").unwrap_or("").to_string()))
}

/// A record as GEDCOM text, cut after [`EXAMPLE_LINES`] lines.
fn example(r: &Node) -> String {
    let text = node_to_string(r);
    let lines: Vec<&str> = text.lines().collect();
    if lines.len() <= EXAMPLE_LINES {
        return text.trim_end().to_string();
    }
    format!("{}\n…", lines[..EXAMPLE_LINES].join("\n"))
}

/// Event or attribute of an individual or a family (GEDCOM 7), as opposed to names, links and
/// metadata.
fn is_event_tag(tag: &str) -> bool {
    const TAGS: &[&str] = &[
        // Individual events.
        "ADOP", "BAPM", "BARM", "BASM", "BIRT", "BLES", "BURI", "CENS", "CHR", "CHRA", "CONF", "CREM", "DEAT", "EMIG",
        "FCOM", "GRAD", "IMMI", "NATU", "ORDN", "PROB", "RETI", "WILL", "EVEN", // Individual attributes.
        "CAST", "DSCR", "EDUC", "IDNO", "NATI", "NCHI", "NMR", "OCCU", "PROP", "RELI", "RESI", "SSN", "TITL", "FACT",
        // Family events.
        "ANUL", "DIV", "DIVF", "ENGA", "MARB", "MARC", "MARL", "MARR", "MARS",
    ];
    TAGS.contains(&tag)
}

/// Most frequent first, then alphabetical.
fn sorted(counts: impl Iterator<Item = (String, usize)>) -> Vec<Count> {
    let mut all: Vec<Count> = counts.map(|(value, count)| Count { value, count }).collect();
    all.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.value.cmp(&b.value)));
    all
}

#[cfg(test)]
mod tests {
    use super::*;

    const FILE: &str = "0 HEAD\n1 GEDC\n2 VERS 7.0\n1 LANG fr\n1 NOTE Titles of sources: « Acte de naissance de Prénom NOM ».\n1 PLAC\n2 FORM Town, County, Country\n1 _GR_EVEN Publication des bans\n2 _GR_REC FAM\n\
0 @I1@ INDI\n1 NAME Jean /GUYOT/\n1 BIRT\n2 DATE 1 JAN 1850\n1 DEAT\n2 DATE 1920\n1 OCCU Cultivateur\n1 CHAN\n2 DATE 3 NOV 2026\n3 TIME 10:00:00\n\
0 @I2@ INDI\n1 NAME Marie /BOST/\n1 BIRT\n2 DATE 1 JAN 2001\n1 OCCU Cultivateur\n1 CHAN\n2 DATE 3 NOV 2026\n3 TIME 11:00:00\n\
0 @I3@ INDI\n1 NAME Pierre /GUYOT/\n1 DEAT\n2 DATE 1890\n1 OCCU Vigneron\n1 EVEN\n2 TYPE Service militaire\n1 CHAN\n2 DATE 1 NOV 2026\n\
0 @S1@ SOUR\n1 TITL Acte de naissance de Jean GUYOT\n0 @S2@ SOUR\n1 TITL Acte de décès de Pierre GUYOT\n1 CHAN\n2 DATE 2 NOV 2026\n0 TRLR\n";

    #[test]
    fn gives_the_settings_vocabulary_and_latest_records() {
        let g = Gedcom::parse(FILE).unwrap();
        let c = g.file_context(5, 2026);
        assert_eq!(c.description, "Titles of sources: « Acte de naissance de Prénom NOM ».");
        assert_eq!(c.language, "fr");
        assert_eq!(c.place_format, ["Town", "County", "Country"]);
        assert_eq!(c.occupations[0], Count { value: "Cultivateur".into(), count: 2 });
        assert!(c.event_tags.contains(&Count { value: "OCCU".into(), count: 3 }));
        assert!(c.event_types.iter().any(|t| t.ty == "Service militaire" && !t.family && t.count == 1));
        assert!(c.event_types.iter().any(|t| t.ty == "Publication des bans" && t.family && t.count == 0));
        // Latest change first, never a living person (I2, born 2001).
        assert_eq!(c.examples.people.len(), 2);
        assert!(c.examples.people[0].starts_with("0 @I1@ INDI"));
        assert!(c.examples.people[1].starts_with("0 @I3@ INDI"));
        // A record without a change date comes after those that have one.
        assert!(c.examples.sources[0].starts_with("0 @S2@ SOUR"));
        assert!(c.examples.sources[1].starts_with("0 @S1@ SOUR"));
        assert_eq!(g.file_context(1, 2026).examples.sources.len(), 1);
        assert!(g.file_context(0, 2026).examples.people.is_empty());
    }
}
