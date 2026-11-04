//! What an AI agent writes in a book, stored in its `_GR_BOOK` record (see `docs/extensions.md`):
//! for each chapter (a generation), an introduction and, for some people, a narrative that takes
//! the place of the sentences the engine writes from the facts.
//!
//! The text is of three kinds, told apart in the book at a glance:
//!
//! - **text**: facts, written as a narrative. People are cited by id in braces (`{I12}`): the book
//!   shows their name, and the id is checked to be in the tree;
//! - **context**: history that explains the facts (a war, a migration, a crisis), with at least one
//!   source;
//! - **hypothesis**: what the records do not say (why a family moved), with the clues it rests on.
//!
//! ```text
//! 1 _GR_CHAP 3
//! 2 _GR_TEXT The {I48} family left the Vega Baja…
//! 2 _GR_CTXT Spanish Oran
//! 3 _GR_TEXT From the 1840s, tens of thousands of…
//! 3 _GR_CITE J.-J. Jordi, Espagnol en Oranie
//! 4 _GR_URL https://books.openedition.org/pur/49159
//! 2 _GR_INDI @I48@
//! 3 _GR_TEXT Vincente is born in 1839…
//! 3 _GR_HYPO She probably followed her neighbours to Oran.
//! 4 _GR_BASE born on the coast near Alicante
//! ```

use gedcom::document::Gedcom;
use gedcom::node::Node;
use std::collections::BTreeMap;

pub(crate) const CHAPTER_TAG: &str = "_GR_CHAP";
const TEXT_TAG: &str = "_GR_TEXT";
const CONTEXT_TAG: &str = "_GR_CTXT";
const CITE_TAG: &str = "_GR_CITE";
const URL_TAG: &str = "_GR_URL";
const HYPOTHESIS_TAG: &str = "_GR_HYPO";
const BASIS_TAG: &str = "_GR_BASE";
const PERSON_TAG: &str = "_GR_INDI";
/// The tags of the agent's writing, declared with the book's.
pub(crate) const TAGS: [&str; 8] =
    [CHAPTER_TAG, TEXT_TAG, CONTEXT_TAG, CITE_TAG, URL_TAG, HYPOTHESIS_TAG, BASIS_TAG, PERSON_TAG];

/// A source of a context box: a book, an article, a web page.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cite {
    pub title: String,
    pub url: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Block {
    Text(String),
    Context { title: String, text: String, sources: Vec<Cite> },
    Hypothesis { text: String, basis: Vec<String> },
}

impl Block {
    /// Context and hypotheses can be left out when reviewing; the text cannot.
    pub fn optional(&self) -> bool {
        !matches!(self, Block::Text(_))
    }

    /// Why the block cannot go into a book, if it cannot.
    pub fn problem(&self, g: &Gedcom) -> Option<String> {
        let (text, what) = match self {
            Block::Text(t) => (t, "text"),
            Block::Context { text, .. } => (text, "context"),
            Block::Hypothesis { text, .. } => (text, "hypothesis"),
        };
        if text.trim().is_empty() {
            return Some(format!("a {what} block has no text"));
        }
        match self {
            Block::Context { title, .. } if title.trim().is_empty() => {
                return Some("a context block needs a `title`".into());
            }
            Block::Context { sources, .. } if !sources.iter().any(|s| !s.title.trim().is_empty()) => {
                return Some("a context block needs at least one source (`sources`, with its title)".into());
            }
            Block::Hypothesis { basis, .. } if !basis.iter().any(|b| !b.trim().is_empty()) => {
                return Some("a hypothesis needs the clues it rests on (`basis`)".into());
            }
            _ => {}
        }
        let unknown: Vec<String> = cited(text).into_iter().filter(|x| g.individual(x).is_none()).collect();
        (!unknown.is_empty()).then(|| format!("no person {} in the tree", unknown.join(", ")))
    }

    fn read(node: &Node) -> Option<Block> {
        let sub = |n: &Node| n.child(TEXT_TAG).map(Node::text).unwrap_or_default();
        match node.tag.as_str() {
            TEXT_TAG => Some(Block::Text(node.text())),
            CONTEXT_TAG => Some(Block::Context {
                title: node.val().trim().to_string(),
                text: sub(node),
                sources: node
                    .children_with(CITE_TAG)
                    .map(|c| Cite {
                        title: c.val().trim().to_string(),
                        url: c.child_val(URL_TAG).unwrap_or_default().trim().to_string(),
                    })
                    .collect(),
            }),
            HYPOTHESIS_TAG => Some(Block::Hypothesis {
                text: node.text(),
                basis: node.children_with(BASIS_TAG).map(|b| b.val().trim().to_string()).collect(),
            }),
            _ => None,
        }
    }

    fn node(&self) -> Node {
        let text = |tag: &str, t: &str| {
            let mut n = Node::new(tag);
            n.set_text(t.trim());
            n
        };
        match self {
            Block::Text(t) => text(TEXT_TAG, t),
            Block::Context { title, text: t, sources } => {
                let mut n = Node::with_value(CONTEXT_TAG, title.trim()).push(text(TEXT_TAG, t));
                for s in sources.iter().filter(|s| !s.title.trim().is_empty()) {
                    let mut c = Node::with_value(CITE_TAG, s.title.trim());
                    c.set_child_val(URL_TAG, &s.url);
                    n.children.push(c);
                }
                n
            }
            Block::Hypothesis { text: t, basis } => {
                let mut n = text(HYPOTHESIS_TAG, t);
                n.children.extend(
                    basis.iter().filter(|b| !b.trim().is_empty()).map(|b| Node::with_value(BASIS_TAG, b.trim())),
                );
                n
            }
        }
    }
}

/// The ids cited in a text, in braces (`{I12}`).
pub fn cited(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = text;
    while let Some(start) = rest.find('{') {
        let after = &rest[start + 1..];
        let Some(end) = after.find('}') else { break };
        let id = after[..end].trim().trim_matches('@');
        if !id.is_empty() && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-') {
            out.push(id.to_string());
        }
        rest = &after[end + 1..];
    }
    out
}

/// The text with each `{I12}` replaced by `name(id)`.
pub fn expand(text: &str, name: impl Fn(&str) -> Option<String>) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find('{') {
        out.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        let Some(end) = after.find('}') else {
            out.push_str(&rest[start..]);
            return out;
        };
        let id = after[..end].trim().trim_matches('@');
        match name(id) {
            Some(n) => out.push_str(&n),
            None => out.push_str(&rest[start..start + end + 2]),
        }
        rest = &after[end + 1..];
    }
    out.push_str(rest);
    out
}

/// What the agent wrote for a chapter.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Writing {
    pub intro: Vec<Block>,
    /// Narrative of a person (id), in the order of the chapter.
    pub people: Vec<(String, Vec<Block>)>,
}

impl Writing {
    pub fn is_empty(&self) -> bool {
        self.intro.is_empty() && self.people.iter().all(|(_, b)| b.is_empty())
    }

    pub fn person(&self, xref: &str) -> Option<&[Block]> {
        self.people.iter().find(|(x, _)| x == xref).map(|(_, b)| b.as_slice()).filter(|b| !b.is_empty())
    }

    /// Every block, the introduction first: the order the review page numbers them in.
    pub fn blocks(&self) -> impl Iterator<Item = &Block> {
        self.intro.iter().chain(self.people.iter().flat_map(|(_, b)| b))
    }

    /// The writing without the blocks whose numbers (in [`Writing::blocks`] order) are in `left_out`.
    pub fn without(&self, left_out: &std::collections::BTreeSet<usize>) -> Writing {
        let mut n = 0;
        let mut keep = |b: &Vec<Block>| -> Vec<Block> {
            b.iter()
                .filter(|_| {
                    n += 1;
                    !left_out.contains(&(n - 1))
                })
                .cloned()
                .collect()
        };
        let intro = keep(&self.intro);
        let people = self.people.iter().map(|(x, b)| (x.clone(), keep(b))).filter(|(_, b)| !b.is_empty()).collect();
        Writing { intro, people }
    }

    /// This writing laid over `old`: a new introduction replaces the old one, a person's new
    /// narrative replaces theirs, the others stay.
    pub fn over(&self, old: &Writing) -> Writing {
        let intro = if self.intro.is_empty() { old.intro.clone() } else { self.intro.clone() };
        let mut people = old.people.clone();
        for (x, b) in &self.people {
            match people.iter_mut().find(|(y, _)| y == x) {
                Some(slot) => slot.1 = b.clone(),
                None => people.push((x.clone(), b.clone())),
            }
        }
        people.retain(|(_, b)| !b.is_empty());
        Writing { intro, people }
    }
}

/// The chapters' writing of a `_GR_BOOK` record, by generation.
pub(crate) fn read(node: &Node) -> BTreeMap<u32, Writing> {
    let mut out: BTreeMap<u32, Writing> = BTreeMap::new();
    for ch in node.children_with(CHAPTER_TAG) {
        let Ok(n) = ch.val().trim().parse::<u32>() else { continue };
        let w = out.entry(n).or_default();
        for c in &ch.children {
            if c.tag == PERSON_TAG {
                let Some(x) = c.pointer() else { continue };
                let blocks: Vec<Block> = c.children.iter().filter_map(Block::read).collect();
                if !blocks.is_empty() {
                    w.people.push((x.to_string(), blocks));
                }
            } else if let Some(b) = Block::read(c) {
                w.intro.push(b);
            }
        }
    }
    out.retain(|_, w| !w.is_empty());
    out
}

/// Replaces the chapters' writing of a `_GR_BOOK` record.
pub(crate) fn write(node: &mut Node, chapters: &BTreeMap<u32, Writing>) {
    node.children.retain(|c| c.tag != CHAPTER_TAG);
    for (n, w) in chapters.iter().filter(|(_, w)| !w.is_empty()) {
        let mut ch = Node::with_value(CHAPTER_TAG, n.to_string());
        ch.children.extend(w.intro.iter().map(Block::node));
        for (x, blocks) in w.people.iter().filter(|(_, b)| !b.is_empty()) {
            let mut p = Node::with_value(PERSON_TAG, format!("@{x}@"));
            p.children.extend(blocks.iter().map(Block::node));
            ch.children.push(p);
        }
        node.insert_child(ch);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_and_expands_the_cited_people() {
        let t = "{I1} marries {@I2@} in {a place}, {}.";
        assert_eq!(cited(t), vec!["I1", "I2"]);
        let named = expand(t, |x| (x == "I1").then(|| "Jean MARCHAND".to_string()));
        assert_eq!(named, "Jean MARCHAND marries {@I2@} in {a place}, {}.");
        assert_eq!(expand("open { brace", |_| None), "open { brace");
    }

    #[test]
    fn leaves_out_blocks_and_lays_writing_over() {
        let hyp = Block::Hypothesis { text: "Maybe".into(), basis: vec!["clue".into()] };
        let w = Writing {
            intro: vec![Block::Text("Intro".into()), hyp.clone()],
            people: vec![("I1".into(), vec![hyp.clone()]), ("I2".into(), vec![Block::Text("Two".into())])],
        };
        let kept = w.without(&[1, 2].into_iter().collect());
        assert_eq!(kept.intro, vec![Block::Text("Intro".into())]);
        assert_eq!(kept.people, vec![("I2".to_string(), vec![Block::Text("Two".into())])]);

        let new = Writing { intro: vec![], people: vec![("I1".into(), vec![Block::Text("One".into())])] };
        let both = new.over(&w);
        assert_eq!(both.intro, w.intro);
        assert_eq!(both.person("I1"), Some(&[Block::Text("One".into())][..]));
        assert!(both.person("I2").is_some());
    }
}
