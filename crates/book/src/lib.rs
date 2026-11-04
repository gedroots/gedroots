//! Family books: a PDF typeset from the tree, with its charts (fan, map, lifespans, figures), a
//! record for each person written from the facts, and an index.
//!
//! - [`spec`]: what a book is made of, stored in the `.ged` (`_GR_BOOK`);
//! - [`writing`]: what an AI agent wrote in it (narratives, context, hypotheses);
//! - [`content`]: the people, their records and the charts, computed from the tree;
//! - [`prose`]: the simple sentences written from the facts;
//! - [`charts`]: the charts, as SVG;
//! - [`render`]: typesetting with Typst, embedded, to PDF and page previews.

rust_i18n::i18n!("locales", fallback = "en");

pub mod charts;
pub mod content;
pub mod prose;
pub mod render;
pub mod spec;
pub mod writing;

pub use content::default_title;
pub use render::Typeset;
pub use spec::{BookSpec, Line, PageSize};
pub use writing::{Block, Cite, Writing};

use gedcom::document::Gedcom;
use std::path::Path;

/// Composes and typesets a book. `file_name`: the tree's file, named in the colophon;
/// `media_dir`: folder the media paths are relative to, for the portraits; `today`: year, month,
/// day.
pub fn compose(
    g: &Gedcom,
    spec: &BookSpec,
    file_name: &str,
    media_dir: Option<&Path>,
    today: (i32, u8, u8),
) -> Result<Typeset, String> {
    let date = prose::date_phrase(&format!("{} {} {}", today.2, MONTHS[(today.1.clamp(1, 12) - 1) as usize], today.0))
        .map(|d| d.split_once(' ').map(|(_, rest)| rest.to_string()).unwrap_or(d))
        .unwrap_or_default();
    let (data, assets) = content::build(g, spec, file_name, media_dir, (today.0, date))?;
    let json = serde_json::to_string(&data).map_err(|e| e.to_string())?;
    render::typeset(&json, assets.into_iter().map(|a| (a.path, a.bytes)).collect(), today)
}

const MONTHS: [&str; 12] = ["JAN", "FEB", "MAR", "APR", "MAY", "JUN", "JUL", "AUG", "SEP", "OCT", "NOV", "DEC"];

#[cfg(test)]
mod tests {
    use super::*;

    const TREE: &str = "0 HEAD\n1 GEDC\n2 VERS 7.0\n\
0 @I1@ INDI\n1 NAME Louis /MARCHAND/\n1 SEX M\n1 BIRT\n2 DATE 5 SEP 1864\n2 PLAC Troyes, Aube, France\n1 OCCU Vigneron\n1 DEAT\n2 DATE 2 MAR 1931\n2 PLAC Troyes, Aube, France\n1 FAMC @F1@\n\
0 @I2@ INDI\n1 NAME Jean /MARCHAND/\n1 SEX M\n1 BIRT\n2 DATE ABT 1830\n2 PLAC Bar-sur-Seine, Aube, France\n1 FAMS @F1@\n\
0 @I3@ INDI\n1 NAME Marie /PRÉVOST/\n1 SEX F\n1 BIRT\n2 DATE 1835\n2 PLAC Saint-Lô, Manche, France\n1 FAMS @F1@\n\
0 @F1@ FAM\n1 HUSB @I2@\n1 WIFE @I3@\n1 CHIL @I1@\n1 MARR\n2 DATE 9 JAN 1860\n2 PLAC Troyes, Aube, France\n\
0 TRLR\n";

    #[test]
    fn typesets_a_small_tree() {
        let g = Gedcom::parse(TREE).expect("valid tree");
        for line in [Line::Ancestors, Line::Descendants] {
            let mut spec = BookSpec::new("I1", "");
            spec.line = line;
            let book = compose(&g, &spec, "test.ged", None, (2026, 11, 4)).expect("typeset");
            assert!(book.page_count() >= 4, "{} pages", book.page_count());
            assert!(book.pdf().expect("pdf").starts_with(b"%PDF"));
            let (w, h, px) = book.page_bgra(0, 100).expect("cover");
            assert_eq!(px.len(), (w * h * 4) as usize);
        }
    }

    #[test]
    fn typesets_what_the_agent_wrote() {
        let g = Gedcom::parse(TREE).expect("valid tree");
        let mut spec = BookSpec::new("I1", "");
        spec.chapters.insert(
            2,
            Writing {
                intro: vec![Block::Context {
                    title: "Champagne".into(),
                    text: "Vineyards everywhere.".into(),
                    sources: vec![Cite { title: "A book".into(), url: String::new() }],
                }],
                people: vec![(
                    "I3".into(),
                    vec![
                        Block::Text("{I3} comes from Normandy.".into()),
                        Block::Hypothesis { text: "She came to work.".into(), basis: vec!["born far away".into()] },
                    ],
                )],
            },
        );
        let (data, _) = content::build(&g, &spec, "test.ged", None, (2026, String::new())).expect("data");
        assert!(data.levels);
        let ch = &data.chapters[1];
        assert_eq!(ch.intro[0].kind, "context");
        // Few facts, but written about: a short record instead of a line, the narrative instead of the sentences.
        let marie = ch.short.iter().find(|s| s.key == "p-I3").expect("short record");
        assert_eq!(marie.paragraphs, Vec::<String>::new());
        assert_eq!(marie.blocks[0].paragraphs, vec!["Marie PRÉVOST comes from Normandy."]);
        assert!(compose(&g, &spec, "test.ged", None, (2026, 11, 4)).is_ok());

        let outline = content::outline(&g, &spec).expect("outline");
        assert_eq!(outline.len(), 2);
        assert_eq!(outline[1].people.iter().map(|p| p.xref.as_str()).collect::<Vec<_>>(), ["I2", "I3"]);
    }

    #[test]
    fn a_book_without_its_person_is_an_error() {
        let g = Gedcom::parse(TREE).expect("valid tree");
        assert!(compose(&g, &BookSpec::new("I9", ""), "test.ged", None, (2026, 11, 4)).is_err());
    }

    #[test]
    fn typeset_books_move_between_threads() {
        fn send_sync<T: Send + Sync>() {}
        send_sync::<Typeset>();
    }
}
