//! Visualizations of a tree: computed from the data, drawn as SVG, and shown both in the app (the
//! Visualizations page) and in family books (`gedroots-book`), which place the same drawings on
//! their pages. Each one also gives its data as a [`Table`]: the app's Table view, and what an
//! AI agent reads to write about it.
//!
//! - [`scope`]: whose ancestors or descendants, over how many generations;
//! - [`places`]: where people come from, grouped for the colours;
//! - [`fan`], [`map`], [`lives`], [`figures`]: the visualizations;
//! - [`palette`]: their colours on paper and in the app's themes;
//! - [`raster`]: a drawing as pixels, for the app.

rust_i18n::i18n!("locales", fallback = "en");

/// A translation in the language of the text being written (`gedcom::i18n::language`: the tree's
/// while a book is written, otherwise the interface's).
macro_rules! t {
    ($key:expr) => {
        rust_i18n::t!($key, locale = &*gedcom::i18n::language())
    };
    ($key:expr, $($name:ident = $value:expr),+ $(,)?) => {{
        let lang = gedcom::i18n::language();
        gedcom::i18n::elide(
            rust_i18n::t!($key, locale = &*lang, $($name = gedcom::i18n::Marked(&($value))),+),
            &lang,
        )
    }};
}

pub mod fan;
pub mod figures;
pub mod lives;
pub mod map;
pub mod palette;
pub mod places;
pub mod raster;
pub mod scope;
pub mod svg;

pub use palette::Palette;
pub use scope::{Line, Member, Scope};

use gedcom::document::Gedcom;
use places::Origins;
use serde::Serialize;

/// The visualizations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Kind {
    Fan,
    Places,
    Lives,
    Generations,
    Longevity,
    Occupations,
}

impl Kind {
    pub const ALL: [Kind; 6] =
        [Kind::Fan, Kind::Places, Kind::Lives, Kind::Generations, Kind::Longevity, Kind::Occupations];

    /// Its name for agents and in the `.ged` (`fan`, `places`…).
    pub fn slug(self) -> &'static str {
        match self {
            Kind::Fan => "fan",
            Kind::Places => "places",
            Kind::Lives => "lives",
            Kind::Generations => "generations",
            Kind::Longevity => "longevity",
            Kind::Occupations => "occupations",
        }
    }

    pub fn from_slug(s: &str) -> Option<Kind> {
        Kind::ALL.into_iter().find(|k| k.slug().eq_ignore_ascii_case(s.trim()))
    }

    /// Its title, in the current language.
    pub fn title(self) -> String {
        match self {
            Kind::Fan => t!("viz.fan.title"),
            Kind::Places => t!("viz.places.title"),
            Kind::Lives => t!("viz.lives.title"),
            Kind::Generations => t!("viz.generations.title"),
            Kind::Longevity => t!("viz.longevity.title"),
            Kind::Occupations => t!("viz.occupations.title"),
        }
        .into_owned()
    }

    /// What it shows, in a few words.
    pub fn summary(self) -> String {
        match self {
            Kind::Fan => t!("viz.fan.summary"),
            Kind::Places => t!("viz.places.summary"),
            Kind::Lives => t!("viz.lives.summary"),
            Kind::Generations => t!("viz.generations.summary"),
            Kind::Longevity => t!("viz.longevity.summary"),
            Kind::Occupations => t!("viz.occupations.summary"),
        }
        .into_owned()
    }

    /// For agents: what it shows and when it is worth placing in a book (English, like the tools).
    pub fn purpose(self) -> &'static str {
        match self {
            Kind::Fan => {
                "Fan chart of the ancestors (or descendants), coloured by the region or country people come from: shows at a glance which branches come from where."
            }
            Kind::Places => {
                "Map of the places people come from, with a line from each child's birthplace to their parents': migrations, and how far a family moved."
            }
            Kind::Lives => {
                "Lifespans of the first generations, with their marriages: who lived at the same time, long and short lives."
            }
            Kind::Generations => {
                "Ancestors found in each generation, against the number each generation has (ancestors only): how complete the research is."
            }
            Kind::Longevity => {
                "Age at death by year of birth, men and women, with the median per half-century: how long people lived over the centuries."
            }
            Kind::Occupations => "The most frequent occupations: the family's trades and social world.",
        }
    }

    /// Whether it is drawn for this line (the ancestors found only exist for ancestors).
    pub fn supports(self, line: Line) -> bool {
        self != Kind::Generations || line == Line::Ancestors
    }
}

/// An entry of a legend: a colour, what it stands for, how many people.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct LegendItem {
    pub label: String,
    pub color: String,
    pub count: usize,
    /// The entry for people whose place is unknown (drawn as an empty, dashed swatch).
    pub unknown: bool,
}

/// The data of a visualization, as a table.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct Table {
    pub columns: Vec<String>,
    pub rows: Vec<TableRow>,
}

impl Table {
    fn new<S: AsRef<str>>(columns: &[S]) -> Table {
        Table { columns: columns.iter().map(|c| c.as_ref().to_string()).collect(), rows: Vec::new() }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TableRow {
    pub cells: Vec<String>,
    /// The person the row is about, when it is one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub record: Option<String>,
}

/// A visualization, drawn.
#[derive(Debug, Clone, PartialEq)]
pub struct Figure {
    pub svg: String,
    pub legend: Vec<LegendItem>,
    /// A line under the drawing (how to read it, what it covers), or empty.
    pub caption: String,
    /// How many things it shows: people, places, generations… (the book's titles count them).
    pub count: usize,
    pub table: Table,
}

/// The people of a scope and where they come from, from which every visualization is drawn.
pub struct Tree<'a> {
    pub g: &'a Gedcom,
    pub scope: Scope,
    pub members: Vec<Member>,
    pub origins: Origins,
}

impl<'a> Tree<'a> {
    /// The scope's people; an error when its person is not in the tree.
    pub fn new(g: &'a Gedcom, scope: Scope) -> Result<Tree<'a>, String> {
        if g.individual(&scope.subject).is_none() {
            return Err(t!("viz.no_subject").into_owned());
        }
        let members = scope::members(g, &scope);
        let origins = Origins::new(g, &members);
        Ok(Tree { g, scope, members, origins })
    }

    /// A visualization, `None` when the tree has too little for it (fewer than two located
    /// places, three dated lives…). `now`: this year, which ends the lives still open.
    pub fn figure(&self, kind: Kind, p: &Palette, now: i32) -> Option<Figure> {
        match kind {
            Kind::Fan => Some(self.fan(p, true)),
            Kind::Places => self.places(p),
            Kind::Lives => self.lives(p, now),
            Kind::Generations => self.generations(p),
            Kind::Longevity => self.longevity(p),
            Kind::Occupations => self.occupations(p),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::{BTreeMap, BTreeSet};

    const TREE: &str = "0 HEAD\n1 GEDC\n2 VERS 7.0\n\
0 @I1@ INDI\n1 NAME Louis /MARCHAND/\n1 SEX M\n1 BIRT\n2 DATE 5 SEP 1864\n2 PLAC Troyes, Aube, France\n1 OCCU Vigneron\n1 DEAT\n2 DATE 2 MAR 1931\n2 PLAC Troyes, Aube, France\n1 FAMC @F1@\n\
0 @I2@ INDI\n1 NAME Jean /MARCHAND/\n1 SEX M\n1 BIRT\n2 DATE ABT 1830\n2 PLAC Bar-sur-Seine, Aube, France\n1 OCCU Vigneron\n1 FAMS @F1@\n\
0 @I3@ INDI\n1 NAME Marie /PRÉVOST/\n1 SEX F\n1 BIRT\n2 DATE 1835\n2 PLAC Saint-Lô, Manche, France\n1 FAMS @F1@\n\
0 @F1@ FAM\n1 HUSB @I2@\n1 WIFE @I3@\n1 CHIL @I1@\n1 MARR\n2 DATE 9 JAN 1860\n2 PLAC Troyes, Aube, France\n\
0 TRLR\n";

    fn tree() -> Gedcom {
        Gedcom::parse(TREE).expect("valid tree")
    }

    #[test]
    fn draws_every_visualization_it_can() {
        let g = tree();
        let t = Tree::new(&g, Scope::new("I1", Line::Ancestors, 4)).expect("tree");
        assert_eq!(t.members.len(), 3);
        let fan = t.figure(Kind::Fan, &Palette::PRINT, 2026).expect("fan");
        assert!(fan.svg.starts_with("<svg") && fan.svg.ends_with("</svg>"));
        assert!(fan.svg.contains("Louis") && fan.svg.contains("MARCHAND"));
        assert_eq!(fan.table.rows.len(), 3);
        assert_eq!(fan.table.rows[0].record.as_deref(), Some("I1"));
        // The places have no coordinates, and only one life is fully dated.
        assert!(t.figure(Kind::Places, &Palette::PRINT, 2026).is_none());
        let gens = t.figure(Kind::Generations, &Palette::PRINT, 2026).expect("generations");
        assert!(gens.svg.contains("100 %"));
        assert_eq!(gens.table.rows[1].cells, ["2", "2", "2", "100%"]);
        let jobs = t.figure(Kind::Occupations, &Palette::DARK, 2026).expect("occupations");
        assert_eq!(jobs.table.rows[0].cells, ["Vigneron", "2"]);
        assert!(jobs.svg.contains(Palette::DARK.accent));

        let down = Tree::new(&g, Scope::new("I2", Line::Descendants, 3)).expect("tree");
        assert!(down.figure(Kind::Generations, &Palette::PRINT, 2026).is_none());
        assert!(down.figure(Kind::Fan, &Palette::PRINT, 2026).expect("fan").svg.contains("Louis"));
        assert!(Tree::new(&g, Scope::new("I9", Line::Ancestors, 4)).is_err());
    }

    #[test]
    fn kinds_have_a_name_for_agents() {
        for k in Kind::ALL {
            assert_eq!(Kind::from_slug(k.slug()), Some(k));
            assert!(!k.title().is_empty() && !k.summary().is_empty());
        }
        assert_eq!(Kind::from_slug(" Places "), Some(Kind::Places));
        assert_eq!(Kind::from_slug("seasons"), None);
    }

    #[test]
    fn rasters_a_drawing_with_its_text() {
        let g = tree();
        let t = Tree::new(&g, Scope::new("I1", Line::Ancestors, 3)).expect("tree");
        let fan = t.fan(&Palette::PRINT, true);
        let (w, h, px) = raster::bgra(&fan.svg, 400).expect("pixels");
        assert_eq!(w, 400);
        assert_eq!(h, 224);
        assert_eq!(px.len(), (w * h * 4) as usize);
        // Something was drawn: not every pixel is transparent.
        assert!(px.as_chunks::<4>().0.iter().any(|p| p[3] > 0));
    }

    fn placeholders(text: &str) -> BTreeSet<&str> {
        text.split("%{").skip(1).filter_map(|s| s.split_once('}')).map(|(name, _)| name).collect()
    }

    /// Each string exists in every language, with the same `%{…}` placeholders.
    #[test]
    fn every_string_is_translated() {
        let backend = crate::_rust_i18n_backend();
        let mut catalog: BTreeMap<String, BTreeMap<&str, String>> = BTreeMap::new();
        for lang in gedcom::i18n::LANGUAGES {
            for (key, text) in backend.messages_for_locale(lang).unwrap_or_default() {
                catalog.entry(key.into_owned()).or_default().insert(lang, text.into_owned());
            }
        }
        assert!(!catalog.is_empty());
        for (key, texts) in &catalog {
            for lang in gedcom::i18n::LANGUAGES {
                let Some(text) = texts.get(lang) else { panic!("`{key}` has no {lang} text") };
                assert_eq!(placeholders(text), placeholders(&texts["en"]), "`{key}`: placeholders differ in {lang}");
            }
        }
    }
}
