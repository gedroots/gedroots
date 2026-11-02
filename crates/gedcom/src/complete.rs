//! Autocompletion of raw GEDCOM text.
//!
//! From the text of a record and the cursor position, the enclosing structure is found with the
//! GEDCOM 7 [`schema`](crate::schema), and depending on where the cursor is, the module suggests:
//!
//! - the tags allowed at this level (without those already present if they can only appear once),
//!   with their label;
//! - records of the right type for a pointer (`FAMC @…`);
//! - the values of an enumeration (`SEX`, `PEDI`, `QUAY`…);
//! - the keywords and months of a date (French Republican calendar included);
//! - the places already entered in the file for `PLAC`.
//!
//! The module does not depend on the interface: it returns byte ranges in the text and suggestions
//! that the editor displays as it wants.

use crate::document::{Gedcom, RecordKind};
use crate::model::{Individual, sort_key};
use crate::schema::{self, Card, Payload, StructDef};
use rust_i18n::t;
use std::borrow::Cow;
use std::ops::Range;

/// Maximum number of suggestions returned for long lists (individuals, places).
const MAX_ITEMS: usize = 80;

/// What the file contains that can be suggested: records (for pointers) and places. Built once when
/// the editor opens.
#[derive(Debug, Clone, Default)]
pub struct Catalog {
    pub records: Vec<RecordRef>,
    pub places: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct RecordRef {
    pub xref: String,
    /// Balise de l'enregistrement (`INDI`, `FAM`…).
    pub tag: &'static str,
    /// Readable label (“Jean MARCHAND (1850–1912)”).
    pub label: String,
    key: String,
}

impl Catalog {
    pub fn from_gedcom(ged: &Gedcom) -> Self {
        let mut records = Vec::new();
        for kind in RecordKind::ALL {
            for node in ged.records_of(kind) {
                let Some(xref) = node.xref.clone() else { continue };
                let mut label = ged.record_label(&xref);
                if kind == RecordKind::Individual {
                    let years = Individual::new(node).years();
                    if !years.is_empty() {
                        label = format!("{label} ({years})");
                    }
                }
                let key = sort_key(&format!("{xref} {label}"));
                records.push(RecordRef { xref, tag: kind.tag(), label, key });
            }
        }
        let places = ged.places().into_iter().map(|p| p.name).collect();
        Self { records, places }
    }
}

/// Kind of suggestion, to pick the icon in the editor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SuggestionKind {
    Tag,
    Pointer,
    Value,
    Keyword,
    Place,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Suggestion {
    /// Text shown in the list.
    pub label: String,
    /// Text inserted in place of [`Completion::replace`].
    pub insert: String,
    /// Detail shown on the right (label, identifier…).
    pub detail: String,
    /// Length (in bytes) of the start of `label` that matches the input, for highlighting; 0 if the
    /// match is elsewhere.
    pub matched: usize,
    pub kind: SuggestionKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Completion {
    /// Range (bytes) of the text replaced by the chosen suggestion.
    pub replace: Range<usize>,
    pub items: Vec<Suggestion>,
}

/// A split GEDCOM line: `level [@xref@] tag [value]`.
#[derive(Debug, Clone, Copy)]
struct Line<'a> {
    level: usize,
    tag: &'a str,
}

fn parse_line(line: &str) -> Option<Line<'_>> {
    let line = line.trim_start();
    let (level, rest) = line.split_once(' ')?;
    let level = level.parse().ok()?;
    let rest = rest.trim_start();
    let rest = if rest.starts_with('@') {
        // Identifiant de l'enregistrement.
        rest.split_once(' ').map(|(_, r)| r.trim_start())?
    } else {
        rest
    };
    let tag = rest.split(' ').next().filter(|t| !t.is_empty())?;
    Some(Line { level, tag })
}

/// Resolves the definition of each line (None for extensions and unknown tags, and for everything
/// they contain).
fn resolve<'a>(lines: &[Option<Line<'a>>]) -> Vec<Option<&'static StructDef>> {
    let mut stack: Vec<Option<&'static StructDef>> = Vec::new();
    let mut out = Vec::with_capacity(lines.len());
    for line in lines {
        let Some(line) = line else {
            out.push(None);
            continue;
        };
        let parent = if line.level == 0 { Some(schema::root()) } else { stack.get(line.level - 1).copied().flatten() };
        let def = if line.tag == "CONT" { None } else { parent.and_then(|p| p.sub(line.tag)).map(|s| s.def()) };
        stack.truncate(line.level);
        while stack.len() < line.level {
            stack.push(None);
        }
        stack.push(def);
        out.push(def);
    }
    out
}

fn upper_ascii(s: &str) -> String {
    s.to_ascii_uppercase()
}

/// A word of the label starts with the input (case and accent insensitive).
fn label_matches(label: &str, query: &str) -> bool {
    let q = sort_key(query.trim());
    !q.is_empty() && sort_key(label).split(|c: char| !c.is_alphanumeric()).any(|w| w.starts_with(&q))
}

/// Computes the suggestions for the cursor at `offset` (bytes) in `text`.
pub fn complete(text: &str, offset: usize, catalog: &Catalog) -> Option<Completion> {
    let offset = offset.min(text.len());
    if !text.is_char_boundary(offset) {
        return None;
    }
    let line_start = text[..offset].rfind('\n').map_or(0, |i| i + 1);
    let line_end = text[offset..].find('\n').map_or(text.len(), |i| offset + i);
    let raw_lines: Vec<&str> = text.split('\n').map(|l| l.trim_end_matches('\r')).collect();
    let cursor_line = text[..line_start].matches('\n').count();
    let parsed: Vec<Option<Line>> = raw_lines.iter().map(|l| parse_line(l)).collect();
    let defs = resolve(&parsed);

    let before = &text[line_start..offset];
    let after = &text[offset..line_end];

    // Splits what comes before the cursor: level, identifier, tag, value.
    let lead = before.len() - before.trim_start().len();
    let body = &before[lead..];
    let digits = body.bytes().take_while(u8::is_ascii_digit).count();
    if digits == 0 {
        return None;
    }
    let level: usize = body[..digits].parse().ok()?;
    let mut pos = line_start + lead + digits;
    let rest = &text[pos..offset];

    // Parent of the level being typed: the last previous line with a lower level.
    let parent_idx = (0..cursor_line).rev().find(|&i| parsed[i].is_some_and(|l| l.level < level));
    let parent_def: Option<&'static StructDef> = if level == 0 {
        Some(schema::root())
    } else {
        match parent_idx {
            Some(i) if parsed[i].unwrap().level == level - 1 => defs[i],
            _ => None,
        }
    };

    // “1” alone: already suggest the tags, completing the space.
    if rest.is_empty() {
        let word_end = line_start + lead + digits;
        let items = tag_items(parent_def?, "", &siblings(&parsed, cursor_line, parent_idx, level), level, true);
        return (!items.is_empty()).then(|| Completion { replace: line_start + lead..word_end, items });
    }
    if !rest.starts_with(' ') {
        return None;
    }
    pos += 1;
    let mut rest = &text[pos..offset];

    // Record identifier at level 0.
    if level == 0 && rest.starts_with('@') {
        match rest[1..].find('@') {
            Some(i) if rest[i + 2..].starts_with(' ') => {
                pos += i + 3;
                rest = &text[pos..offset];
            }
            _ => return None,
        }
    }

    match rest.find(' ') {
        // Cursor in the tag.
        None => {
            if rest.starts_with('_') {
                return None;
            }
            let tag_end = offset + after.bytes().take_while(|b| !b.is_ascii_whitespace()).count();
            let items = tag_items(parent_def?, rest, &siblings(&parsed, cursor_line, parent_idx, level), level, false);
            (!items.is_empty()).then_some(Completion { replace: pos..tag_end, items })
        }
        // Cursor in the value.
        Some(i) => {
            let tag = &rest[..i];
            let value_start = pos + i + 1;
            let value = &text[value_start..offset];
            let def = parent_def?.sub(tag)?.def();
            value_items(tag, def, value, value_start, offset, after, catalog)
        }
    }
}

/// Tags already present at the same level under the same parent (except the cursor's line).
fn siblings<'a>(
    parsed: &[Option<Line<'a>>],
    cursor_line: usize,
    parent_idx: Option<usize>,
    level: usize,
) -> Vec<&'a str> {
    let start = parent_idx.map_or(0, |i| i + 1);
    let mut out = Vec::new();
    for (i, line) in parsed.iter().enumerate().skip(start) {
        let Some(line) = line else { continue };
        if i != cursor_line && line.level < level && i > cursor_line {
            break;
        }
        if i != cursor_line && line.level == level {
            out.push(line.tag);
        }
    }
    out
}

fn tag_items(parent: &StructDef, query: &str, present: &[&str], level: usize, with_level: bool) -> Vec<Suggestion> {
    let q_upper = upper_ascii(query);
    let mut items: Vec<(u8, Suggestion)> = Vec::new();
    for sub in parent.subs {
        if sub.card.single() && present.contains(&sub.tag) {
            continue;
        }
        let def = sub.def();
        let label = schema::label(sub.tag, def);
        let prefix = sub.tag.starts_with(&q_upper);
        if !prefix && !label_matches(&label, query) {
            continue;
        }
        // Missing required tags first, then matches on the tag, then those found through the label.
        let rank = if sub.card.required() && !present.contains(&sub.tag) {
            0
        } else if prefix {
            1
        } else {
            2
        };
        let needs_value = !matches!(def.payload, Payload::None | Payload::YOrEmpty);
        let mut insert = String::new();
        if with_level {
            insert.push_str(&level.to_string());
            insert.push(' ');
        }
        insert.push_str(sub.tag);
        if needs_value {
            insert.push(' ');
        }
        let card = match sub.card {
            Card::Required | Card::AtLeastOne => t!("complete.required"),
            _ => Cow::Borrowed(""),
        };
        items.push((
            rank,
            Suggestion {
                label: sub.tag.to_string(),
                insert,
                detail: format!("{label}{card}"),
                matched: if prefix { q_upper.len() } else { 0 },
                kind: SuggestionKind::Tag,
            },
        ));
    }
    items.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.label.cmp(&b.1.label)));
    items.into_iter().map(|(_, s)| s).collect()
}

const DATE_KEYWORDS: &[&str] = &[
    "ABT",
    "CAL",
    "EST",
    "BEF",
    "AFT",
    "BET",
    "AND",
    "FROM",
    "TO",
    "GREGORIAN",
    "JULIAN",
    "FRENCH_R",
    "HEBREW",
    "BCE",
];

const MONTHS: &[&str] = &["JAN", "FEB", "MAR", "APR", "MAY", "JUN", "JUL", "AUG", "SEP", "OCT", "NOV", "DEC"];

const MONTHS_FRENCH_R: &[&str] =
    &["VEND", "BRUM", "FRIM", "NIVO", "PLUV", "VENT", "GERM", "FLOR", "PRAI", "MESS", "THER", "FRUC", "COMP"];

const MEDIA_TYPES: &[(&str, &str)] = &[
    ("image/jpeg", "JPEG"),
    ("image/png", "PNG"),
    ("image/tiff", "TIFF"),
    ("image/gif", "GIF"),
    ("image/webp", "WebP"),
    ("application/pdf", "PDF"),
    ("text/plain", ""),
    ("text/html", "HTML"),
    ("audio/mpeg", "MP3"),
    ("video/mp4", "MP4"),
];

/// Codes of the languages offered for `LANG` (BCP 47).
const LANGUAGE_CODES: &[&str] = &["fr", "en", "de", "it", "es", "la", "oc", "br", "nl"];

/// Languages offered for `LANG`: (BCP 47 code, name in the interface language).
pub fn languages() -> Vec<(&'static str, Cow<'static, str>)> {
    LANGUAGE_CODES
        .iter()
        .map(|c| (*c, crate::i18n::lookup(&format!("language.{c}")).unwrap_or(Cow::Borrowed(c))))
        .collect()
}

/// Detail of a date keyword (`ABT` → “about”) or month (`MAR` → “March”).
fn date_word(prefix: &str, code: &str) -> Cow<'static, str> {
    crate::i18n::lookup(&format!("{prefix}.{code}")).unwrap_or_default()
}

fn keyword(label: &str, detail: &str, query: &str, kind: SuggestionKind) -> Option<Suggestion> {
    let q = upper_ascii(query);
    let up = upper_ascii(label);
    let prefix = up.starts_with(&q);
    (prefix || label_matches(detail, query)).then(|| Suggestion {
        label: label.to_string(),
        insert: label.to_string(),
        detail: detail.to_string(),
        matched: if prefix { q.len() } else { 0 },
        kind,
    })
}

fn value_items(
    tag: &str,
    def: &StructDef,
    value: &str,
    value_start: usize,
    offset: usize,
    after: &str,
    catalog: &Catalog,
) -> Option<Completion> {
    let value_end = offset + after.len();
    // The word being typed (for values with several words), including what follows.
    let word_start = value.rfind([' ', ',']).map_or(value_start, |i| value_start + i + 1);
    let word = &value[word_start - value_start..];
    let word_end = offset + after.find([' ', ',']).unwrap_or(after.len());
    let (replace, items) = match def.payload {
        Payload::Pointer(kind) => {
            let q = sort_key(value.trim_start_matches('@').trim_end_matches('@'));
            let mut items: Vec<Suggestion> = catalog
                .records
                .iter()
                .filter(|r| r.tag == kind && (q.is_empty() || r.key.contains(&q)))
                .take(MAX_ITEMS)
                .map(|r| Suggestion {
                    label: r.label.clone(),
                    insert: format!("@{}@", r.xref),
                    detail: format!("@{}@", r.xref),
                    matched: 0,
                    kind: SuggestionKind::Pointer,
                })
                .collect();
            if "@VOID@".starts_with(&upper_ascii(value)) {
                items.push(Suggestion {
                    label: "@VOID@".into(),
                    insert: "@VOID@".into(),
                    detail: t!("complete.void").into(),
                    matched: value.len(),
                    kind: SuggestionKind::Keyword,
                });
            }
            (value_start..value_end, items)
        }
        Payload::Enum { list, values } => {
            let (start, q) = if list { (word_start, word.trim_start()) } else { (value_start, value) };
            let items = values
                .iter()
                .filter_map(|v| keyword(v, &schema::enum_label(v).unwrap_or_default(), q, SuggestionKind::Value))
                .collect();
            let end = if list { word_end } else { value_end };
            (start..end, items)
        }
        Payload::YOrEmpty => (
            value_start..value_end,
            keyword("Y", &t!("complete.happened"), value, SuggestionKind::Value).into_iter().collect(),
        ),
        Payload::Date => {
            if word.chars().next().is_some_and(|c| c.is_ascii_digit()) {
                return None;
            }
            let upper = upper_ascii(value);
            let (months, prefix) =
                if upper.contains("FRENCH_R") { (MONTHS_FRENCH_R, "month") } else { (MONTHS, "month_name") };
            // Months only come after a day or at the start of a simple date.
            let after_day = value[..word_start - value_start]
                .trim_end()
                .rsplit(' ')
                .next()
                .is_some_and(|w| w.parse::<u8>().is_ok());
            let mut items: Vec<Suggestion> = Vec::new();
            if after_day {
                items.extend(
                    months.iter().filter_map(|m| keyword(m, &date_word(prefix, m), word, SuggestionKind::Value)),
                );
            } else {
                items.extend(
                    DATE_KEYWORDS
                        .iter()
                        .filter_map(|k| keyword(k, &date_word("date_keyword", k), word, SuggestionKind::Keyword)),
                );
                if !word.is_empty() {
                    items.extend(
                        months.iter().filter_map(|m| keyword(m, &date_word(prefix, m), word, SuggestionKind::Value)),
                    );
                }
            }
            (word_start..word_end, items)
        }
        Payload::MediaType => (
            value_start..value_end,
            MEDIA_TYPES
                .iter()
                .filter_map(|(m, l)| {
                    let l = if l.is_empty() { t!("complete.plain_text") } else { Cow::Borrowed(*l) };
                    keyword(m, &l, value, SuggestionKind::Value)
                })
                .collect(),
        ),
        Payload::Language => (
            value_start..value_end,
            languages().iter().filter_map(|(m, l)| keyword(m, l, value, SuggestionKind::Value)).collect(),
        ),
        Payload::Text if tag == "PLAC" => {
            let q = sort_key(value);
            let items = catalog
                .places
                .iter()
                .filter(|p| p.as_str() != value && sort_key(p).contains(&q))
                .take(MAX_ITEMS)
                .map(|p| Suggestion {
                    label: p.clone(),
                    insert: p.clone(),
                    detail: String::new(),
                    matched: 0,
                    kind: SuggestionKind::Place,
                })
                .collect();
            (value_start..value_end, items)
        }
        _ => return None,
    };
    (!items.is_empty()).then_some(Completion { replace, items })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn catalog() -> Catalog {
        let ged = Gedcom::parse(
            "0 HEAD\n1 GEDC\n2 VERS 7.0\n0 @I1@ INDI\n1 NAME Jean /GUYOT/\n1 FAMS @F1@\n0 @I2@ INDI\n1 NAME Marie /DURAND/\n1 BIRT\n2 PLAC , Troyes, 10000, Aube, Grand Est, France\n0 @F1@ FAM\n1 HUSB @I1@\n0 TRLR\n",
        )
        .unwrap();
        Catalog::from_gedcom(&ged)
    }

    /// `|` marks the cursor.
    fn at(text_with_cursor: &str) -> Option<Completion> {
        let offset = text_with_cursor.find('|').unwrap();
        let text = text_with_cursor.replacen('|', "", 1);
        complete(&text, offset, &catalog())
    }

    fn labels(c: &Completion) -> Vec<&str> {
        c.items.iter().map(|s| s.label.as_str()).collect()
    }

    #[test]
    fn tags_of_an_individual() {
        let c = at("0 @I1@ INDI\n1 BI|").unwrap();
        assert_eq!(labels(&c), ["BIRT"]);
        assert_eq!(c.replace, 14..16);
        assert_eq!(c.items[0].insert, "BIRT");
        assert_eq!(c.items[0].matched, 2);
    }

    #[test]
    fn search_by_label() {
        let c = at("0 @I1@ INDI\n1 christening|").unwrap();
        assert!(labels(&c).contains(&"CHR"));
        let c = at("0 @I1@ INDI\n1 burial|").unwrap();
        assert!(labels(&c).contains(&"BURI"));
    }

    #[test]
    fn tags_in_an_event() {
        let c = at("0 @I1@ INDI\n1 BIRT\n2 |").unwrap();
        let l = labels(&c);
        assert!(l.contains(&"DATE") && l.contains(&"PLAC") && l.contains(&"SOUR"));
        assert!(!l.contains(&"BIRT"));
        // Adds the space after a tag that expects a value.
        assert_eq!(c.items.iter().find(|s| s.label == "DATE").unwrap().insert, "DATE ");
    }

    #[test]
    fn level_only() {
        let c = at("0 @I1@ INDI\n1 BIRT\n2|").unwrap();
        assert_eq!(c.items.iter().find(|s| s.label == "DATE").unwrap().insert, "2 DATE ");
        assert_eq!(c.replace, 19..20);
    }

    #[test]
    fn cardinality_is_respected() {
        let c = at("0 @I1@ INDI\n1 BIRT\n2 DATE 1850\n2 |\n2 PLAC Troyes").unwrap();
        let l = labels(&c);
        assert!(!l.contains(&"DATE") && !l.contains(&"PLAC"));
        assert!(l.contains(&"NOTE"));
        // SEX appears only once per individual.
        let c = at("0 @I1@ INDI\n1 SEX M\n1 S|").unwrap();
        assert!(!labels(&c).contains(&"SEX"));
    }

    #[test]
    fn no_suggestion_under_an_extension() {
        assert!(at("0 @I1@ INDI\n1 _MILT\n2 |").is_none());
        assert!(at("0 @I1@ INDI\n1 _MI|").is_none());
    }

    #[test]
    fn required_tags_first() {
        let c = at("0 @I1@ INDI\n1 ASSO @I2@\n2 |").unwrap();
        assert_eq!(c.items[0].label, "ROLE");
    }

    #[test]
    fn records_at_level_zero() {
        let c = at("0 @X1@ |").unwrap();
        let l = labels(&c);
        assert!(l.contains(&"INDI") && l.contains(&"SOUR"));
    }

    #[test]
    fn pointers_of_the_right_type() {
        let c = at("0 @F1@ FAM\n1 WIFE |").unwrap();
        let l = labels(&c);
        assert!(l.contains(&"Marie DURAND") && l.contains(&"Jean GUYOT"));
        assert!(c.items.iter().any(|s| s.insert == "@I2@"));
        let c = at("0 @F1@ FAM\n1 WIFE dur|").unwrap();
        assert_eq!(labels(&c), ["Marie DURAND"]);
        assert_eq!(c.replace, 18..21);
        let c = at("0 @I1@ INDI\n1 FAMC @|").unwrap();
        assert_eq!(c.items[0].insert, "@F1@");
        assert_eq!(c.items[0].label, "Jean GUYOT");
    }

    #[test]
    fn enumerated_values() {
        let c = at("0 @I1@ INDI\n1 SEX |").unwrap();
        assert_eq!(labels(&c), ["F", "M", "U", "X"]);
        let c = at("0 @I1@ INDI\n1 FAMC @F1@\n2 PEDI ad|").unwrap();
        assert_eq!(labels(&c), ["ADOPTED"]);
        let c = at("0 @I1@ INDI\n1 SEX fem|").unwrap();
        assert_eq!(labels(&c), ["F"]);
    }

    #[test]
    fn dates() {
        let c = at("0 @I1@ INDI\n1 BIRT\n2 DATE |").unwrap();
        assert!(labels(&c).contains(&"ABT"));
        let c = at("0 @I1@ INDI\n1 BIRT\n2 DATE 14 m|").unwrap();
        assert_eq!(labels(&c), ["MAR", "MAY"]);
        let c = at("0 @I1@ INDI\n1 BIRT\n2 DATE FRENCH_R 3 b|").unwrap();
        assert_eq!(labels(&c), ["BRUM"]);
        let c = at("0 @I1@ INDI\n1 BIRT\n2 DATE b|").unwrap();
        assert_eq!(labels(&c), ["BEF", "BET", "BCE"]);
        assert!(at("0 @I1@ INDI\n1 BIRT\n2 DATE 18|").is_none());
    }

    #[test]
    fn existing_places() {
        let c = at("0 @I1@ INDI\n1 DEAT\n2 PLAC troy|").unwrap();
        assert_eq!(labels(&c), [", Troyes, 10000, Aube, Grand Est, France"]);
    }

    #[test]
    fn out_of_context() {
        assert!(at("|").is_none());
        assert!(at("0 @I1@ INDI\n1 NAME Jean /GU|").is_none());
        assert!(at("0 @I1@ INDI\n1 NOTE texte libre|").is_none());
    }
}
