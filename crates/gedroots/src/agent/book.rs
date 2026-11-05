//! The agent writes family books: `read_book` gives a book's chapters and who is in each,
//! `propose_chapter` sends what it wrote for one chapter (an introduction, people's narratives,
//! historical context, hypotheses). Nothing is written: the chapter waits in “To Review”, where the
//! user leaves out the context and hypotheses they do not want, then saves it into the book.

use gedcom::Gedcom;
use gedroots_book::content::Record;
use gedroots_book::writing::Figure;
use gedroots_book::{Block, BookSpec, Cite, Line, Writing};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct ReadBookParams {
    /// Tree to work on, as `list_trees` names it, when several are open (default: the one in
    /// front in GEDRoots).
    pub tree: Option<String>,
    /// Book id (`BK1`). Leave empty to list the tree's books.
    pub book: Option<String>,
}

/// A kind of block.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    /// Facts told as a narrative, citing people as `{I12}`.
    Text,
    /// Historical context (a war, a migration, a crisis, a law): `title` and `sources` required.
    Context,
    /// What the records do not say (why a family moved): `basis` required.
    Hypothesis,
    /// A visualization GEDRoots draws from the tree (`list_visualizations`): `visualization`
    /// required, `text` an optional comment shown under it.
    Visualization,
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct SourceRef {
    /// Author and title, as a bibliography gives them.
    pub title: String,
    /// Web page of the source, when it has one.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub url: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct BlockParams {
    pub kind: Kind,
    /// The text, in the book's language. Paragraphs separated by a line break. Name people by id
    /// in braces (`{I12}`): the book shows their name.
    pub text: String,
    /// Context: its title ("The phylloxera crisis").
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub title: String,
    /// Context: where the history comes from, at least one.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sources: Vec<SourceRef>,
    /// Hypothesis: the clues it rests on, at least one ("born on the coast near Alicante").
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub basis: Vec<String>,
    /// Visualization: its name (`places`, `fan`, `lives`…, from `list_visualizations`).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub visualization: String,
    /// Visualization: whose ancestors or descendants (`I12`). Default: the book's person.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub person: Option<String>,
    /// Visualization: `ancestors` or `descendants`. Default: the book's.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line: Option<String>,
    /// Visualization: how many generations, 2 to 12. Default: the book's.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub generations: Option<u32>,
}

impl BlockParams {
    fn new(kind: Kind, text: &str) -> BlockParams {
        BlockParams {
            kind,
            text: text.to_string(),
            title: String::new(),
            sources: vec![],
            basis: vec![],
            visualization: String::new(),
            person: None,
            line: None,
            generations: None,
        }
    }
}

fn line_name(line: Line) -> &'static str {
    match line {
        Line::Ancestors => "ancestors",
        Line::Descendants => "descendants",
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct PersonWriting {
    /// Person id, one of the chapter's people (`read_book`).
    pub id: String,
    /// Their narrative: it takes the place of the sentences the book writes from the facts.
    pub blocks: Vec<BlockParams>,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct ProposeChapterParams {
    /// Tree to work on, as `list_trees` names it, when several are open (default: the one in
    /// front in GEDRoots).
    pub tree: Option<String>,
    /// Book id (`BK1`, from `read_book`).
    pub book: String,
    /// Chapter: its generation number (1 = the book's person).
    pub generation: u32,
    /// Introduction of the chapter: what the generation lived through, where it came from. Empty:
    /// the current introduction stays.
    #[serde(default)]
    pub intro: Vec<BlockParams>,
    /// Narratives of people of the chapter. The people left out keep what they have.
    #[serde(default)]
    pub people: Vec<PersonWriting>,
    /// For the reviewer: what you could not check, what to look for. Never written in the book.
    pub remarks: Option<String>,
}

/// A chapter waiting for review.
#[derive(Debug, Clone)]
pub struct ChapterRequest {
    pub book: String,
    pub generation: u32,
    pub writing: Writing,
    pub remarks: String,
    /// Its id when it is queued again as the file opens (`P3`); a new one is given otherwise.
    pub id: Option<String>,
    /// The agent that sent it.
    pub agent: String,
    /// The parameters as the agent sent them (JSON), kept in the file while the proposal waits.
    pub request: String,
}

fn clean(id: &str) -> String {
    id.trim().trim_matches('@').to_string()
}

fn block(p: &BlockParams) -> Result<Block, String> {
    Ok(match p.kind {
        Kind::Visualization => {
            let kind = gedroots_viz::Kind::from_slug(&p.visualization).ok_or_else(|| {
                let names: Vec<&str> = gedroots_viz::Kind::ALL.iter().map(|k| k.slug()).collect();
                format!("no visualization `{}`: one of {}", p.visualization, names.join(", "))
            })?;
            let line = match p.line.as_deref().map(|l| l.trim().to_ascii_lowercase()).as_deref() {
                None | Some("") => None,
                Some("ancestors") => Some(Line::Ancestors),
                Some("descendants") => Some(Line::Descendants),
                Some(other) => return Err(format!("no line `{other}`: ancestors or descendants")),
            };
            Block::Figure(Figure {
                kind,
                subject: p.person.as_deref().map(clean).filter(|x| !x.is_empty()),
                line,
                generations: p.generations,
                text: p.text.clone(),
            })
        }
        Kind::Text => Block::Text(p.text.clone()),
        Kind::Context => Block::Context {
            title: p.title.trim().to_string(),
            text: p.text.clone(),
            sources: p
                .sources
                .iter()
                .map(|s| Cite { title: s.title.trim().into(), url: s.url.trim().into() })
                .collect(),
        },
        Kind::Hypothesis => Block::Hypothesis { text: p.text.clone(), basis: p.basis.clone() },
    })
}

fn params(b: &Block) -> BlockParams {
    match b {
        Block::Text(t) => BlockParams::new(Kind::Text, t),
        Block::Context { title, text, sources } => BlockParams {
            title: title.clone(),
            sources: sources.iter().map(|s| SourceRef { title: s.title.clone(), url: s.url.clone() }).collect(),
            ..BlockParams::new(Kind::Context, text)
        },
        Block::Hypothesis { text, basis } => {
            BlockParams { basis: basis.clone(), ..BlockParams::new(Kind::Hypothesis, text) }
        }
        Block::Figure(f) => BlockParams {
            visualization: f.kind.slug().into(),
            person: f.subject.clone(),
            line: f.line.map(|l| line_name(l).into()),
            generations: f.generations,
            ..BlockParams::new(Kind::Visualization, &f.text)
        },
    }
}

/// The chapter the agent sent, checked against the book: refused, with the reason, when the book
/// or the chapter does not exist, a person is not in it, or a block lacks what it needs.
pub fn to_request(p: &ProposeChapterParams, g: &Gedcom) -> Result<ChapterRequest, String> {
    let id = clean(&p.book);
    let spec = gedroots_book::spec::book(g, &id).ok_or_else(|| format!("no book `{id}`: call read_book"))?;
    let outline = gedroots_book::outline(g, &spec)?;
    let chapter = outline
        .iter()
        .find(|c| c.generation == p.generation)
        .ok_or_else(|| format!("the book has no chapter {}: its chapters are 1 to {}", p.generation, outline.len()))?;
    let writing = Writing {
        intro: p.intro.iter().map(block).collect::<Result<_, _>>()?,
        people: p
            .people
            .iter()
            .map(|w| Ok((clean(&w.id), w.blocks.iter().map(block).collect::<Result<_, String>>()?)))
            .collect::<Result<_, String>>()?,
    };
    if writing.is_empty() {
        return Err("nothing to propose: give `intro` or `people`".into());
    }
    for (x, _) in &writing.people {
        if !chapter.people.iter().any(|m| &m.xref == x) {
            return Err(format!("{x} is not in chapter {} (read_book lists its people)", p.generation));
        }
    }
    if let Some(problem) = writing.blocks().find_map(|b| b.problem(g)) {
        return Err(problem);
    }
    for b in writing.blocks() {
        if let Block::Figure(f) = b
            && !f.kind.supports(f.line.unwrap_or(spec.line))
        {
            return Err(format!("`{}` is drawn for ancestors only, and this book is of descendants", f.kind.slug()));
        }
    }
    Ok(ChapterRequest {
        book: id,
        generation: p.generation,
        writing,
        remarks: p.remarks.clone().unwrap_or_default().trim().to_string(),
        id: None,
        agent: String::new(),
        request: super::propose::stored_request(p).to_string(),
    })
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct BookSummary {
    pub id: String,
    pub title: String,
    /// The person the book starts from.
    pub person: String,
    /// ancestors or descendants.
    pub line: String,
    pub generations: u32,
    /// Language the book is written in, and the one to write in (BCP 47, `fr`).
    pub language: String,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct ChapterPerson {
    pub id: String,
    pub name: String,
    pub years: String,
    /// Sosa or d'Aboville number.
    pub number: String,
    /// full (a page with a narrative), short (a paragraph) or line (a row of the generation's
    /// table). A person you write about gets at least a short record.
    pub record: String,
    /// Dated or placed facts the tree knows: call `person_sheet` for them.
    pub facts: usize,
    /// What the book already has about them, from an earlier chapter you proposed.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub written: Vec<BlockParams>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct ChapterOutline {
    pub generation: u32,
    pub title: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub intro: Vec<BlockParams>,
    pub people: Vec<ChapterPerson>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct BookOutline {
    #[serde(flatten)]
    pub book: BookSummary,
    pub chapters: Vec<ChapterOutline>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct Books {
    /// The tree's books, when no book was asked for.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub books: Vec<BookSummary>,
    /// The book asked for.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub book: Option<BookOutline>,
}

fn summary(g: &Gedcom, spec: &BookSpec) -> BookSummary {
    BookSummary {
        id: spec.xref.clone().unwrap_or_default(),
        title: match spec.title.trim() {
            "" => gedroots_book::default_title(g, spec),
            t => t.to_string(),
        },
        person: g.individual(&spec.subject).map(|i| i.display_name()).unwrap_or_default(),
        line: line_name(spec.line).into(),
        generations: spec.generations,
        language: gedroots_book::language(g),
    }
}

/// The tree's books, or the chapters of one.
pub fn read(g: &Gedcom, book: Option<&str>) -> Result<Books, String> {
    let Some(id) = book.map(clean).filter(|b| !b.is_empty()) else {
        let books: Vec<BookSummary> = gedroots_book::spec::books(g).iter().map(|b| summary(g, b)).collect();
        if books.is_empty() {
            return Err("the tree has no book yet: ask the user to create one (Create on the Books page)".into());
        }
        return Ok(Books { books, book: None });
    };
    let spec =
        gedroots_book::spec::book(g, &id).ok_or_else(|| format!("no book `{id}`: call read_book without `book`"))?;
    let chapters = gedroots_book::outline(g, &spec)?
        .into_iter()
        .map(|c| {
            let w = spec.chapters.get(&c.generation);
            ChapterOutline {
                generation: c.generation,
                title: c.title,
                intro: w.map(|w| w.intro.iter().map(params).collect()).unwrap_or_default(),
                people: c
                    .people
                    .into_iter()
                    .map(|m| ChapterPerson {
                        written: w
                            .and_then(|w| w.person(&m.xref))
                            .map(|b| b.iter().map(params).collect())
                            .unwrap_or_default(),
                        id: m.xref,
                        name: m.name,
                        years: m.years,
                        number: m.number,
                        record: match m.record {
                            Record::Full => "full",
                            Record::Short => "short",
                            Record::Line => "line",
                        }
                        .into(),
                        facts: m.facts,
                    })
                    .collect(),
            }
        })
        .collect();
    Ok(Books { books: vec![], book: Some(BookOutline { book: summary(g, &spec), chapters }) })
}

#[cfg(test)]
mod tests {
    use super::*;

    const TREE: &str = "0 HEAD\n1 GEDC\n2 VERS 7.0\n\
0 @I1@ INDI\n1 NAME Louis /MARCHAND/\n1 BIRT\n2 DATE 1864\n1 FAMC @F1@\n\
0 @I2@ INDI\n1 NAME Jean /MARCHAND/\n1 FAMS @F1@\n\
0 @I9@ INDI\n1 NAME Paul /AUTRE/\n\
0 @F1@ FAM\n1 HUSB @I2@\n1 CHIL @I1@\n\
0 @BK1@ _GR_BOOK Marchand\n1 _GR_SUBJ @I1@\n\
0 TRLR\n";

    fn chapter(people: Vec<PersonWriting>, intro: Vec<BlockParams>) -> ProposeChapterParams {
        ProposeChapterParams { tree: None, book: "BK1".into(), generation: 2, intro, people, remarks: None }
    }

    fn text(t: &str) -> BlockParams {
        BlockParams::new(Kind::Text, t)
    }

    #[test]
    fn checks_the_chapter_against_the_book() {
        let g = Gedcom::parse(TREE).unwrap();
        let ok = to_request(
            &chapter(vec![PersonWriting { id: "@I2@".into(), blocks: vec![text("{I2} works the vines.")] }], vec![]),
            &g,
        )
        .expect("valid");
        assert_eq!(ok.writing.people[0].0, "I2");

        let not_here = chapter(vec![PersonWriting { id: "I9".into(), blocks: vec![text("Paul")] }], vec![]);
        assert!(to_request(&not_here, &g).unwrap_err().contains("not in chapter 2"));
        assert!(to_request(&chapter(vec![], vec![text("{I77} came")]), &g).unwrap_err().contains("I77"));
        let no_source = BlockParams { kind: Kind::Context, title: "War".into(), ..text("The war came.") };
        assert!(to_request(&chapter(vec![], vec![no_source]), &g).unwrap_err().contains("source"));
        let no_basis = BlockParams { kind: Kind::Hypothesis, ..text("Maybe.") };
        assert!(to_request(&chapter(vec![], vec![no_basis]), &g).unwrap_err().contains("basis"));
        let viz = |name: &str, line: Option<&str>| BlockParams {
            visualization: name.into(),
            line: line.map(Into::into),
            person: Some("@I2@".into()),
            ..BlockParams::new(Kind::Visualization, "")
        };
        let placed = to_request(&chapter(vec![], vec![viz("Places", None)]), &g).expect("visualization");
        assert_eq!(params(&placed.writing.intro[0]).person.as_deref(), Some("I2"));
        assert!(to_request(&chapter(vec![], vec![viz("weather", None)]), &g).unwrap_err().contains("one of fan"));
        let found = viz("generations", Some("descendants"));
        assert!(to_request(&chapter(vec![], vec![found]), &g).unwrap_err().contains("ancestors only"));
        let mut far = chapter(vec![], vec![text("x")]);
        far.generation = 9;
        assert!(to_request(&far, &g).unwrap_err().contains("no chapter 9"));
    }

    #[test]
    fn reads_the_books_and_their_chapters() {
        let g = Gedcom::parse(TREE).unwrap();
        assert_eq!(read(&g, None).unwrap().books[0].id, "BK1");
        let book = read(&g, Some("BK1")).unwrap().book.unwrap();
        assert_eq!(book.chapters.len(), 2);
        assert_eq!(book.chapters[1].people[0].id, "I2");
        assert_eq!(book.chapters[1].people[0].record, "line");
    }
}
