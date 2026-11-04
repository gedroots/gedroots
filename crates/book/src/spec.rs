//! What a book is made of, stored in the `.ged` as a `_GR_BOOK` record (see
//! `docs/extensions.md`): its title, its person, ancestors or descendants, the number of
//! generations and the page size. The content itself is computed from the tree each time.
//!
//! ```text
//! 0 @BK1@ _GR_BOOK The Marchand Family
//! 1 _GR_SUBJ @I1@
//! 1 _GR_LINE ANCESTORS
//! 1 _GR_GENS 8
//! 1 _GR_SIZE 170X240
//! ```

use gedcom::document::Gedcom;
use gedcom::node::Node;
use std::sync::Arc;

pub const BOOK_TAG: &str = "_GR_BOOK";
const SUBJECT_TAG: &str = "_GR_SUBJ";
const LINE_TAG: &str = "_GR_LINE";
const GENERATIONS_TAG: &str = "_GR_GENS";
const SIZE_TAG: &str = "_GR_SIZE";
/// Every tag of the record, all documented in the same section.
const TAGS: [&str; 5] = [BOOK_TAG, SUBJECT_TAG, LINE_TAG, GENERATIONS_TAG, SIZE_TAG];
const URI: &str = concat!(env!("CARGO_PKG_REPOSITORY"), "/blob/main/docs/extensions.md#_gr_book");
/// Prefix of the identifiers of the books (`@BK1@`).
const XREF_PREFIX: &str = "BK";

pub const MIN_GENERATIONS: u32 = 2;
pub const MAX_GENERATIONS: u32 = 12;
pub const DEFAULT_GENERATIONS: u32 = 8;

/// The person's ancestors (Sosa numbering) or descendants (d'Aboville numbering).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Line {
    #[default]
    Ancestors,
    Descendants,
}

impl Line {
    pub const ALL: [Line; 2] = [Line::Ancestors, Line::Descendants];

    fn tag(self) -> &'static str {
        match self {
            Line::Ancestors => "ANCESTORS",
            Line::Descendants => "DESCENDANTS",
        }
    }

    fn from_tag(s: &str) -> Line {
        if s.eq_ignore_ascii_case("DESCENDANTS") { Line::Descendants } else { Line::Ancestors }
    }
}

/// Trim size of the pages.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PageSize {
    /// 17 × 24 cm, the usual size of illustrated books in Europe.
    #[default]
    Book,
    A4,
    Letter,
}

impl PageSize {
    pub const ALL: [PageSize; 3] = [PageSize::Book, PageSize::A4, PageSize::Letter];

    fn tag(self) -> &'static str {
        match self {
            PageSize::Book => "170X240",
            PageSize::A4 => "A4",
            PageSize::Letter => "LETTER",
        }
    }

    fn from_tag(s: &str) -> PageSize {
        PageSize::ALL.into_iter().find(|p| p.tag().eq_ignore_ascii_case(s.trim())).unwrap_or_default()
    }

    /// Width and height in millimetres.
    pub fn mm(self) -> (f64, f64) {
        match self {
            PageSize::Book => (170., 240.),
            PageSize::A4 => (210., 297.),
            PageSize::Letter => (215.9, 279.4),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BookSpec {
    /// Identifier of the `_GR_BOOK` record, once stored.
    pub xref: Option<String>,
    pub title: String,
    /// The person the book starts from.
    pub subject: String,
    pub line: Line,
    pub generations: u32,
    pub size: PageSize,
}

impl BookSpec {
    /// A new book starting from `subject`, with the default settings.
    pub fn new(subject: &str, title: &str) -> Self {
        BookSpec {
            xref: None,
            title: title.to_string(),
            subject: subject.to_string(),
            line: Line::default(),
            generations: DEFAULT_GENERATIONS,
            size: PageSize::default(),
        }
    }

    fn read(node: &Node) -> Self {
        BookSpec {
            xref: node.xref.clone(),
            title: node.val().trim().to_string(),
            subject: node.child(SUBJECT_TAG).and_then(|n| n.pointer()).unwrap_or_default().to_string(),
            line: Line::from_tag(node.child_val(LINE_TAG).unwrap_or_default()),
            generations: node
                .child_val(GENERATIONS_TAG)
                .and_then(|v| v.trim().parse().ok())
                .unwrap_or(DEFAULT_GENERATIONS)
                .clamp(MIN_GENERATIONS, MAX_GENERATIONS),
            size: PageSize::from_tag(node.child_val(SIZE_TAG).unwrap_or_default()),
        }
    }

    fn write(&self, node: &mut Node) {
        node.value = Some(self.title.trim().to_string());
        node.set_child_val(SUBJECT_TAG, &format!("@{}@", self.subject));
        node.set_child_val(LINE_TAG, self.line.tag());
        node.set_child_val(GENERATIONS_TAG, &self.generations.clamp(MIN_GENERATIONS, MAX_GENERATIONS).to_string());
        node.set_child_val(SIZE_TAG, self.size.tag());
    }
}

/// The books of the file, in file order.
pub fn books(g: &Gedcom) -> Vec<BookSpec> {
    g.records.iter().filter(|r| r.tag == BOOK_TAG).map(|r| BookSpec::read(r)).collect()
}

/// The book stored as `xref`.
pub fn book(g: &Gedcom, xref: &str) -> Option<BookSpec> {
    g.get(xref).filter(|r| r.tag == BOOK_TAG).map(BookSpec::read)
}

/// Stores a book: a new `_GR_BOOK` record when it has no identifier yet, otherwise its record is
/// updated. Declares the extension tags and returns the identifier.
pub fn save(g: &mut Gedcom, spec: &BookSpec) -> String {
    for tag in TAGS {
        g.declare_extension(tag, URI);
    }
    if let Some(x) = spec.xref.as_deref()
        && let Some(node) = g.get_mut(x).filter(|n| n.tag == BOOK_TAG)
    {
        spec.write(node);
        node.touch();
        return x.to_string();
    }
    let xref = next_xref(g);
    let mut node = Node::new(BOOK_TAG).with_xref(xref.clone());
    spec.write(&mut node);
    node.touch();
    g.records.push(Arc::new(node));
    g.reindex();
    xref
}

/// Deletes a book; `false` if there is none with this identifier.
pub fn delete(g: &mut Gedcom, xref: &str) -> bool {
    g.get(xref).is_some_and(|n| n.tag == BOOK_TAG) && g.delete(xref).is_some()
}

fn next_xref(g: &Gedcom) -> String {
    let mut n = g
        .records
        .iter()
        .filter_map(|r| r.xref.as_deref()?.strip_prefix(XREF_PREFIX)?.parse::<u64>().ok())
        .max()
        .unwrap_or(0);
    loop {
        n += 1;
        let x = format!("{XREF_PREFIX}{n}");
        if g.get(&x).is_none() {
            return x;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FILE: &str = "0 HEAD\n1 GEDC\n2 VERS 7.0\n0 @I1@ INDI\n1 NAME Jean /MARCHAND/\n0 TRLR\n";

    #[test]
    fn saves_reads_and_deletes_a_book() {
        let mut g = Gedcom::parse(FILE).unwrap();
        let mut spec = BookSpec::new("I1", "The Marchand Family");
        spec.line = Line::Descendants;
        spec.generations = 5;
        spec.size = PageSize::A4;
        let x = save(&mut g, &spec);
        assert_eq!(x, "BK1");
        let text = g.to_gedcom_string();
        assert!(text.contains("0 @BK1@ _GR_BOOK The Marchand Family\n1 _GR_SUBJ @I1@\n1 _GR_LINE DESCENDANTS\n1 _GR_GENS 5\n1 _GR_SIZE A4\n"), "{text}");
        assert!(text.contains("2 TAG _GR_BOOK https://"), "{text}");

        let g = Gedcom::parse(&text).unwrap();
        let read = books(&g);
        assert_eq!(read.len(), 1);
        assert_eq!(read[0], BookSpec { xref: Some("BK1".into()), ..spec.clone() });

        let mut g = g;
        let mut renamed = read[0].clone();
        renamed.title = "Marchand".into();
        assert_eq!(save(&mut g, &renamed), "BK1");
        assert_eq!(book(&g, "BK1").unwrap().title, "Marchand");
        assert_eq!(save(&mut g, &BookSpec::new("I1", "Second")), "BK2");

        assert!(delete(&mut g, "BK1"));
        assert!(!delete(&mut g, "I1"));
        assert_eq!(books(&g).len(), 1);
    }

    #[test]
    fn deleting_the_person_leaves_a_book_without_subject() {
        let mut g = Gedcom::parse(FILE).unwrap();
        save(&mut g, &BookSpec::new("I1", "Book"));
        g.delete("I1");
        assert_eq!(books(&g)[0].subject, "");
    }

    #[test]
    fn out_of_range_values_fall_back() {
        let g =
            Gedcom::parse("0 HEAD\n0 @BK1@ _GR_BOOK\n1 _GR_GENS 99\n1 _GR_SIZE HUGE\n1 _GR_LINE X\n0 TRLR\n").unwrap();
        let b = &books(&g)[0];
        assert_eq!((b.generations, b.size, b.line), (MAX_GENERATIONS, PageSize::Book, Line::Ancestors));
    }
}
