//! The agent reads the tree's visualizations (`gedroots-viz`): `read_visualization` without a name
//! says what each one shows, with a name it gives its data, as the Visualizations page's Table view,
//! with its legend and caption. The drawings themselves stay in the app and the books.

use gedcom::Gedcom;
use gedroots_viz::{Kind, LegendItem, Line, Palette, Scope, Table, Tree};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ReadVizParams {
    /// Tree to work on, as `file_context` names it in `trees`, when several are open (default: the one in
    /// front in GEDRoots).
    pub tree: Option<String>,
    /// The visualization (`fan`, `places`, `lives`…). Leave it out for the list of them.
    pub visualization: Option<String>,
    /// Whose ancestors or descendants (`I12`). Default: the tree's root person.
    pub person: Option<String>,
    /// `ancestors` (default) or `descendants`.
    pub line: Option<String>,
    /// How many generations, 2 to 12 (default 8).
    pub generations: Option<u32>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct Visualizations {
    /// Without a name: every visualization GEDRoots draws.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub visualizations: Option<Vec<VizSummary>>,
    /// With a name: its data.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub visualization: Option<Visualization>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct VizSummary {
    /// Its name for `read_visualization`.
    pub name: String,
    /// Its title in the app.
    pub title: String,
    /// What it shows, and when it is worth placing in a book.
    pub purpose: String,
    /// The lines it is drawn for (`ancestors`, `descendants`).
    pub lines: Vec<String>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct Visualization {
    pub name: String,
    pub title: String,
    /// Whose (`I12`), and their name.
    pub person: String,
    pub person_name: String,
    pub line: String,
    pub generations: u32,
    /// How many things it shows: people, places, generations…
    pub count: usize,
    /// The line under the drawing (how to read it, what it covers), when it has one.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub caption: String,
    /// What its colours stand for (the fan's and the lives' regions), with how many people.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub legend: Vec<Legend>,
    /// Its data: one row per person, place, move, generation or occupation (`record`: the person's id).
    #[schemars(with = "serde_json::Value")]
    pub table: Table,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct Legend {
    pub label: String,
    pub people: usize,
    /// The people whose place is unknown.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub unknown: bool,
}

fn line_name(line: Line) -> &'static str {
    match line {
        Line::Ancestors => "ancestors",
        Line::Descendants => "descendants",
    }
}

fn names() -> String {
    Kind::ALL.map(|k| k.slug()).join(", ")
}

pub fn list() -> Vec<VizSummary> {
    Kind::ALL
        .into_iter()
        .map(|k| VizSummary {
            name: k.slug().into(),
            title: k.title(),
            purpose: k.purpose().into(),
            lines: Line::ALL.into_iter().filter(|l| k.supports(*l)).map(|l| line_name(l).into()).collect(),
        })
        .collect()
}

/// The data of a visualization, for a person (the root person by default).
pub fn read(g: &Gedcom, p: &ReadVizParams, year: i32) -> Result<Visualization, String> {
    let name = p.visualization.as_deref().unwrap_or_default().trim();
    let kind = Kind::from_slug(name).ok_or_else(|| format!("no visualization `{name}`: one of {}", names()))?;
    let line = match p.line.as_deref().map(|l| l.trim().to_ascii_lowercase()).as_deref() {
        None | Some("" | "ancestors") => Line::Ancestors,
        Some("descendants") => Line::Descendants,
        Some(other) => return Err(format!("no line `{other}`: ancestors or descendants")),
    };
    if !kind.supports(line) {
        return Err(format!("`{}` is drawn for ancestors only", kind.slug()));
    }
    let person = match p.person.as_deref().map(|x| x.trim().trim_matches('@')).filter(|x| !x.is_empty()) {
        Some(x) => x.to_string(),
        None => g.root().ok_or("the tree has no root person: give `person`")?,
    };
    let scope = Scope::new(&person, line, p.generations.unwrap_or(gedroots_viz::scope::DEFAULT_GENERATIONS));
    let generations = scope.generations;
    let tree = Tree::new(g, scope).map_err(|_| format!("no person `{person}` in the tree"))?;
    let figure = tree.figure(kind, &Palette::PRINT, year).ok_or_else(|| {
        format!("the tree has too little for `{}` here (too few dated lives, located places or ages)", kind.slug())
    })?;
    let legend = |l: LegendItem| Legend { label: l.label, people: l.count, unknown: l.unknown };
    Ok(Visualization {
        name: kind.slug().into(),
        title: kind.title(),
        person_name: g.individual(&person).map(|i| i.display_name()).unwrap_or_default(),
        person,
        line: line_name(line).into(),
        generations,
        count: figure.count,
        caption: figure.caption,
        legend: figure.legend.into_iter().map(legend).collect(),
        table: figure.table,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const TREE: &str = "0 HEAD\n1 GEDC\n2 VERS 7.0\n\
0 @I1@ INDI\n1 NAME Louis /MARCHAND/\n1 SEX M\n1 BIRT\n2 DATE 1864\n1 OCCU Vigneron\n1 FAMC @F1@\n\
0 @I2@ INDI\n1 NAME Jean /MARCHAND/\n1 SEX M\n1 OCCU Vigneron\n1 FAMS @F1@\n\
0 @F1@ FAM\n1 HUSB @I2@\n1 CHIL @I1@\n\
0 TRLR\n";

    fn params(viz: &str, line: Option<&str>) -> ReadVizParams {
        ReadVizParams {
            tree: None,
            visualization: Some(viz.into()),
            person: Some("@I1@".into()),
            line: line.map(Into::into),
            generations: Some(3),
        }
    }

    #[test]
    fn reads_a_visualization_as_a_table() {
        let g = Gedcom::parse(TREE).expect("tree");
        let v = read(&g, &params("occupations", None), 2026).expect("occupations");
        assert_eq!(v.person_name, "Louis MARCHAND");
        assert_eq!(v.table.rows[0].cells, ["Vigneron", "2"]);
        let fan = read(&g, &params("Fan", Some("ancestors")), 2026).expect("fan");
        assert_eq!(fan.table.rows.len(), 2);
        assert!(read(&g, &params("longevity", None), 2026).unwrap_err().contains("too little"));
        assert!(read(&g, &params("generations", Some("descendants")), 2026).unwrap_err().contains("ancestors only"));
        assert!(read(&g, &params("weather", None), 2026).unwrap_err().contains("fan, places"));
        assert_eq!(list().len(), Kind::ALL.len());
    }
}
