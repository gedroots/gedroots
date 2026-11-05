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

/// A translation in the book's language (`gedcom::i18n::language`, which [`in_tree_language`]
/// sets while a book is written): `rust_i18n::t!` with its locale.
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

pub mod charts;
pub mod content;
pub mod prose;
pub mod render;
pub mod spec;
pub mod writing;

pub use content::Part;
pub use render::Typeset;
pub use spec::{BookSpec, Line, PageSize};
pub use writing::{Block, Cite, Writing};

use gedcom::document::Gedcom;
use std::path::Path;

/// The language a book of this tree is written in: the tree's (`HEAD.LANG`) when GEDRoots is
/// translated into it, otherwise the interface's.
pub fn language(g: &Gedcom) -> String {
    in_tree_language(g, || gedcom::i18n::language().into_owned())
}

/// Runs `f` with the text written in the tree's language (see [`language`]), on this thread.
fn in_tree_language<R>(g: &Gedcom, f: impl FnOnce() -> R) -> R {
    match g.header.child_val("LANG").and_then(gedcom::i18n::translated) {
        Some(lang) => gedcom::i18n::with_language(lang, f),
        None => f(),
    }
}

/// The title a book has when the user gave none ("The GUYOT Family"), in the tree's language.
pub fn default_title(g: &Gedcom, spec: &BookSpec) -> String {
    in_tree_language(g, || content::default_title(g, spec))
}

/// The chapters of a book and who is in each, in the tree's language.
pub fn outline(g: &Gedcom, spec: &BookSpec) -> Result<Vec<content::OutlineChapter>, String> {
    in_tree_language(g, || content::outline(g, spec))
}

/// Composes and typesets a whole book, as the app shows it, in the tree's language. `file_name`: the tree's file, named in
/// the colophon; `media_dir`: folder the media paths are relative to, for the portraits; `today`:
/// year, month, day.
pub fn compose(
    g: &Gedcom,
    spec: &BookSpec,
    file_name: &str,
    media_dir: Option<&Path>,
    today: (i32, u8, u8),
) -> Result<Typeset, String> {
    in_tree_language(g, || Ok(Composer { g, spec, file_name, media_dir, today }.part(&Part::whole())?.0))
}

/// Most pages in a volume: a longer book is cut into volumes between its chapters (printers bind
/// up to 800 pages, but a book of 500 is already heavy in the hand).
pub const VOLUME_PAGES: usize = 500;

/// From this many pages, the title can be printed on the spine (Lulu's rule).
pub const SPINE_TEXT_PAGES: usize = 80;

/// A file to export: a volume of the book (or the whole book), and its cover for a printer.
pub struct Volume {
    /// Its number, when the book has several volumes.
    pub number: Option<usize>,
    pub interior: Typeset,
    /// For a printer: back cover, spine and front cover on one spread.
    pub cover: Option<Typeset>,
}

/// Width of the spine of a paperback, in millimetres (Lulu's formula for their standard paper;
/// other printers give theirs with their own calculator).
pub fn spine_mm(pages: usize) -> f64 {
    pages as f64 / 17.48 + 1.524
}

/// Composes the book to export: in volumes when it is longer than [`VOLUME_PAGES`], and with its
/// covers when it is for a printer (`spec.print`).
pub fn compose_export(
    g: &Gedcom,
    spec: &BookSpec,
    file_name: &str,
    media_dir: Option<&Path>,
    today: (i32, u8, u8),
) -> Result<Vec<Volume>, String> {
    in_tree_language(g, || Composer { g, spec, file_name, media_dir, today }.export(VOLUME_PAGES))
}

impl Composer<'_> {
    /// The volumes, of at most `max` pages each.
    fn export(&self, max: usize) -> Result<Vec<Volume>, String> {
        let (c, spec, today) = (self, self.spec, self.today);
        let print = spec.print;
        let (interior, data, assets) = c.part(&Part { print, ..Part::whole() })?;
        if interior.page_count() <= max {
            let cover = if print { Some(cover(&data, &assets, interior.page_count(), today)?) } else { None };
            return Ok(vec![Volume { number: None, interior, cover }]);
        }
        drop(interior);
        let groups = c.volumes(max)?;
        let mut out = Vec::new();
        for (i, &(a, b)) in groups.iter().enumerate() {
            let part = Part {
                chapters: Some((a, b)),
                volume: Some((i + 1, groups.len())),
                opening: i == 0,
                figures: i + 1 == groups.len(),
                print,
            };
            let (interior, data, assets) = c.part(&part)?;
            let cover = if print { Some(cover(&data, &assets, interior.page_count(), today)?) } else { None };
            out.push(Volume { number: Some(i + 1), interior, cover });
        }
        Ok(out)
    }
}

fn cover(
    data: &content::Data,
    assets: &[content::Asset],
    pages: usize,
    today: (i32, u8, u8),
) -> Result<Typeset, String> {
    let json = serde_json::json!({
        "title": data.title,
        "subtitle": data.subtitle,
        "summary": data.summary,
        "colophon": data.colophon,
        "lang": data.lang,
        "volume": data.volume,
        "width_mm": data.width_mm,
        "height_mm": data.height_mm,
        "bleed_mm": content::BLEED_MM,
        "spine_mm": spine_mm(pages),
        "spine_text": pages >= SPINE_TEXT_PAGES,
        "fan": data.fan,
    });
    let files =
        assets.iter().filter(|a| a.path == "fan-cover.svg").map(|a| (a.path.clone(), a.bytes.clone())).collect();
    render::typeset_cover(&json.to_string(), files, today)
}

/// What composing a part of the book needs.
struct Composer<'a> {
    g: &'a Gedcom,
    spec: &'a BookSpec,
    file_name: &'a str,
    media_dir: Option<&'a Path>,
    today: (i32, u8, u8),
}

impl Composer<'_> {
    fn part(&self, part: &Part) -> Result<(Typeset, content::Data, Vec<content::Asset>), String> {
        let today = self.today;
        let date =
            prose::date_phrase(&format!("{} {} {}", today.2, MONTHS[(today.1.clamp(1, 12) - 1) as usize], today.0))
                .map(|d| d.split_once(' ').map(|(_, rest)| rest.to_string()).unwrap_or(d))
                .unwrap_or_default();
        let (data, assets) = content::build(self.g, self.spec, self.file_name, self.media_dir, (today.0, date), part)?;
        let json = serde_json::to_string(&data).map_err(|e| e.to_string())?;
        let files = assets.iter().map(|a| (a.path.clone(), a.bytes.clone())).collect();
        Ok((render::typeset(&json, files, today)?, data, assets))
    }

    fn pages(&self, part: Part) -> Result<usize, String> {
        Ok(self.part(&part)?.0.page_count())
    }

    /// The chapters of each volume: consecutive chapters, as many as fit in [`VOLUME_PAGES`],
    /// each measured alone (with its people in the index).
    fn volumes(&self, max: usize) -> Result<Vec<(u32, u32)>, String> {
        let alone = |chapters: Option<(u32, u32)>, opening: bool, figures: bool| Part {
            chapters,
            volume: Some((1, 2)),
            opening,
            figures,
            print: self.spec.print,
        };
        let none = Some((0, 0));
        let empty = self.pages(alone(none, false, false))?;
        let opening = self.pages(alone(none, true, false))?.saturating_sub(empty);
        let figures = self.pages(alone(none, false, true))?.saturating_sub(empty);
        let chapters: Vec<u32> = content::outline(self.g, self.spec)?.iter().map(|c| c.generation).collect();
        let mut sizes = Vec::new();
        for &n in &chapters {
            sizes.push(self.pages(alone(Some((n, n)), false, false))?.saturating_sub(empty));
        }
        Ok(group(&chapters, &sizes, empty, opening, figures, max))
    }
}

/// Cuts the chapters (with their sizes in pages) into volumes of at most [`VOLUME_PAGES`]: each
/// volume has `empty` pages of its own (cover, contents), the first the `opening` charts, the last
/// the `figures`. A chapter longer than a volume makes a volume of its own. `max`: pages per volume.
fn group(
    chapters: &[u32],
    sizes: &[usize],
    empty: usize,
    opening: usize,
    figures: usize,
    max: usize,
) -> Vec<(u32, u32)> {
    let mut out: Vec<(u32, u32)> = Vec::new();
    let mut used = empty + opening;
    for (k, (&n, &size)) in chapters.iter().zip(sizes).enumerate() {
        let need = size + if k + 1 == chapters.len() { figures } else { 0 };
        let fits = used + need <= max;
        match out.last_mut() {
            Some(v) if fits => {
                v.1 = n;
                used += size;
            }
            _ => {
                if !out.is_empty() {
                    used = empty;
                }
                out.push((n, n));
                used += size;
            }
        }
    }
    out
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
    fn is_written_in_the_tree_language() {
        let french = Gedcom::parse(&TREE.replace("2 VERS 7.0\n", "2 VERS 7.0\n1 LANG fr\n")).expect("valid tree");
        let spec = BookSpec::new("I1", "");
        assert_eq!(language(&french), "fr");
        assert_eq!(default_title(&french, &spec), "La famille MARCHAND");
        let titles =
            |g: &Gedcom| -> Vec<String> { outline(g, &spec).expect("outline").into_iter().map(|c| c.title).collect() };
        // The interface's language is left as it was, and used when the tree has none.
        assert_eq!(&*gedcom::i18n::language(), "en");
        let english = Gedcom::parse(TREE).expect("valid tree");
        assert_eq!(default_title(&english, &spec), "The MARCHAND family");
        assert_ne!(titles(&french), titles(&english));
        let other = Gedcom::parse(&TREE.replace("2 VERS 7.0\n", "2 VERS 7.0\n1 LANG de\n")).expect("valid tree");
        assert_eq!(language(&other), "en");
    }

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
        let (data, _) =
            content::build(&g, &spec, "test.ged", None, (2026, String::new()), &Part::whole()).expect("data");
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
    fn exports_for_a_printer() {
        let g = Gedcom::parse(TREE).expect("valid tree");
        let mut spec = BookSpec::new("I1", "");
        spec.print = true;
        let volumes = compose_export(&g, &spec, "test.ged", None, (2026, 11, 4)).expect("export");
        assert_eq!(volumes.len(), 1);
        let v = &volumes[0];
        assert_eq!(v.number, None);
        // The pages and the cover carry the bleed: 170 mm + 2 × 3.175 mm wide.
        let mm = |pt: f64| pt / 72. * 25.4;
        let (w, h) = v.interior.page_size();
        assert!((mm(w) - 176.35).abs() < 0.1 && (mm(h) - 246.35).abs() < 0.1, "{} × {}", mm(w), mm(h));
        let cover = v.cover.as_ref().expect("cover");
        assert_eq!(cover.page_count(), 1);
        let spine = spine_mm(v.interior.page_count());
        assert!((mm(cover.page_size().0) - (2. * 170. + spine + 2. * 3.175)).abs() < 0.1);
    }

    #[test]
    fn cuts_long_books_into_volumes_between_chapters() {
        // 30 pages of its own per volume, 20 of charts in the first, 10 of figures in the last.
        let sizes = [100, 200, 150, 120, 300];
        assert_eq!(group(&[1, 2, 3, 4, 5], &sizes, 30, 20, 10, VOLUME_PAGES), vec![(1, 3), (4, 5)]);
        // A chapter longer than a volume stands alone.
        assert_eq!(group(&[1, 2], &[600, 10], 30, 0, 0, VOLUME_PAGES), vec![(1, 1), (2, 2)]);
        assert!((spine_mm(300) - 18.69).abs() < 0.01);

        // A book longer than its limit: one volume per chapter here, each with its cover.
        let g = Gedcom::parse(TREE).expect("valid tree");
        let mut spec = BookSpec::new("I1", "");
        spec.print = true;
        let c = Composer { g: &g, spec: &spec, file_name: "t.ged", media_dir: None, today: (2026, 11, 4) };
        let volumes = c.export(3).expect("volumes");
        assert_eq!(volumes.iter().map(|v| v.number).collect::<Vec<_>>(), [Some(1), Some(2)]);
        assert!(volumes.iter().all(|v| v.cover.is_some()));
    }

    #[test]
    fn draws_the_fan_of_descendants() {
        let g = Gedcom::parse(TREE).expect("valid tree");
        let mut spec = BookSpec::new("I2", "");
        spec.line = Line::Descendants;
        let (data, assets) =
            content::build(&g, &spec, "t.ged", None, (2026, String::new()), &Part::whole()).expect("data");
        assert!(data.fan && data.fan_page.is_some());
        let fan = assets.iter().find(|a| a.path == "fan.svg").expect("fan");
        assert!(String::from_utf8_lossy(&fan.bytes).contains("Louis"));
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
