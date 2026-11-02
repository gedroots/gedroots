//! Autocomplétion du texte GEDCOM brut.
//!
//! À partir du texte d'un enregistrement et de la position du curseur, on
//! retrouve la structure englobante grâce au [`schema`](crate::schema) GEDCOM 7
//! et on propose, selon l'endroit où se trouve le curseur :
//!
//! - les balises permises à ce niveau (sans celles déjà présentes si elles
//!   ne peuvent apparaître qu'une fois), avec leur libellé français ;
//! - les enregistrements du bon type pour un pointeur (`FAMC @…`) ;
//! - les valeurs d'une énumération (`SEX`, `PEDI`, `QUAY`…) ;
//! - les mots-clés et mois d'une date (y compris le calendrier républicain) ;
//! - les lieux déjà saisis dans le fichier pour `PLAC`.
//!
//! Le module ne dépend pas de l'interface : il renvoie des plages en octets
//! dans le texte et des suggestions que l'éditeur affiche comme il veut.

use crate::document::{Gedcom, RecordKind};
use crate::model::{Individual, sort_key};
use crate::schema::{self, Card, Payload, StructDef};
use std::ops::Range;

/// Nombre maximal de suggestions renvoyées pour les listes longues
/// (individus, lieux).
const MAX_ITEMS: usize = 80;

/// Ce que le fichier contient et qu'on peut proposer : enregistrements
/// (pour les pointeurs) et lieux. Construit une fois à l'ouverture de l'éditeur.
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
    /// Libellé lisible (« Jean GUYOT (1850–1912) »).
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

/// Nature d'une suggestion, pour choisir l'icône dans l'éditeur.
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
    /// Texte affiché dans la liste.
    pub label: String,
    /// Texte inséré à la place de [`Completion::replace`].
    pub insert: String,
    /// Précision affichée à droite (libellé français, identifiant…).
    pub detail: String,
    /// Longueur (en octets) du début de `label` qui correspond à la saisie,
    /// pour la mise en évidence ; 0 si la correspondance est ailleurs.
    pub matched: usize,
    pub kind: SuggestionKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Completion {
    /// Plage (octets) du texte remplacé par la suggestion choisie.
    pub replace: Range<usize>,
    pub items: Vec<Suggestion>,
}

/// Une ligne GEDCOM découpée : `niveau [@xref@] balise [valeur]`.
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

/// Résout la définition de chaque ligne (None pour les extensions et les
/// balises inconnues, et pour tout ce qu'elles contiennent).
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

/// Un mot du libellé commence par la saisie (insensible à la casse et aux accents).
fn label_matches(label: &str, query: &str) -> bool {
    let q = sort_key(query.trim());
    !q.is_empty() && sort_key(label).split(|c: char| !c.is_alphanumeric()).any(|w| w.starts_with(&q))
}

/// Calcule les suggestions pour le curseur à `offset` (octets) dans `text`.
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

    // Découpe ce qui précède le curseur : niveau, identifiant, balise, valeur.
    let lead = before.len() - before.trim_start().len();
    let body = &before[lead..];
    let digits = body.bytes().take_while(u8::is_ascii_digit).count();
    if digits == 0 {
        return None;
    }
    let level: usize = body[..digits].parse().ok()?;
    let mut pos = line_start + lead + digits;
    let rest = &text[pos..offset];

    // Parent du niveau saisi : la dernière ligne précédente de niveau inférieur.
    let parent_idx = (0..cursor_line).rev().find(|&i| parsed[i].is_some_and(|l| l.level < level));
    let parent_def: Option<&'static StructDef> = if level == 0 {
        Some(schema::root())
    } else {
        match parent_idx {
            Some(i) if parsed[i].unwrap().level == level - 1 => defs[i],
            _ => None,
        }
    };

    // « 1 » seul : on propose déjà les balises, en complétant l'espace.
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

    // Identifiant d'enregistrement au niveau 0.
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
        // Curseur dans la balise.
        None => {
            if rest.starts_with('_') {
                return None;
            }
            let tag_end = offset + after.bytes().take_while(|b| !b.is_ascii_whitespace()).count();
            let items = tag_items(parent_def?, rest, &siblings(&parsed, cursor_line, parent_idx, level), level, false);
            (!items.is_empty()).then_some(Completion { replace: pos..tag_end, items })
        }
        // Curseur dans la valeur.
        Some(i) => {
            let tag = &rest[..i];
            let value_start = pos + i + 1;
            let value = &text[value_start..offset];
            let def = parent_def?.sub(tag)?.def();
            value_items(tag, def, value, value_start, offset, after, catalog)
        }
    }
}

/// Balises déjà présentes au même niveau sous le même parent (hors ligne du curseur).
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
        let label_fr = schema::label_fr(sub.tag, def);
        let prefix = sub.tag.starts_with(&q_upper);
        if !prefix && !label_matches(label_fr, query) {
            continue;
        }
        // Les balises obligatoires manquantes d'abord, puis les correspondances
        // sur la balise, puis celles trouvées par le libellé français.
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
            Card::Required | Card::AtLeastOne => " · obligatoire",
            _ => "",
        };
        items.push((
            rank,
            Suggestion {
                label: sub.tag.to_string(),
                insert,
                detail: format!("{label_fr}{card}"),
                matched: if prefix { q_upper.len() } else { 0 },
                kind: SuggestionKind::Tag,
            },
        ));
    }
    items.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.label.cmp(&b.1.label)));
    items.into_iter().map(|(_, s)| s).collect()
}

const DATE_KEYWORDS: &[(&str, &str)] = &[
    ("ABT", "environ"),
    ("CAL", "calculée"),
    ("EST", "estimée"),
    ("BEF", "avant"),
    ("AFT", "après"),
    ("BET", "entre… (BET … AND …)"),
    ("AND", "… et"),
    ("FROM", "depuis (FROM … TO …)"),
    ("TO", "jusqu'à"),
    ("GREGORIAN", "calendrier grégorien"),
    ("JULIAN", "calendrier julien"),
    ("FRENCH_R", "calendrier républicain"),
    ("HEBREW", "calendrier hébraïque"),
    ("BCE", "avant notre ère"),
];

const MONTHS: &[(&str, &str)] = &[
    ("JAN", "janvier"),
    ("FEB", "février"),
    ("MAR", "mars"),
    ("APR", "avril"),
    ("MAY", "mai"),
    ("JUN", "juin"),
    ("JUL", "juillet"),
    ("AUG", "août"),
    ("SEP", "septembre"),
    ("OCT", "octobre"),
    ("NOV", "novembre"),
    ("DEC", "décembre"),
];

const MONTHS_FRENCH_R: &[(&str, &str)] = &[
    ("VEND", "vendémiaire"),
    ("BRUM", "brumaire"),
    ("FRIM", "frimaire"),
    ("NIVO", "nivôse"),
    ("PLUV", "pluviôse"),
    ("VENT", "ventôse"),
    ("GERM", "germinal"),
    ("FLOR", "floréal"),
    ("PRAI", "prairial"),
    ("MESS", "messidor"),
    ("THER", "thermidor"),
    ("FRUC", "fructidor"),
    ("COMP", "jours complémentaires"),
];

const MEDIA_TYPES: &[(&str, &str)] = &[
    ("image/jpeg", "JPEG"),
    ("image/png", "PNG"),
    ("image/tiff", "TIFF"),
    ("image/gif", "GIF"),
    ("image/webp", "WebP"),
    ("application/pdf", "PDF"),
    ("text/plain", "texte"),
    ("text/html", "HTML"),
    ("audio/mpeg", "MP3"),
    ("video/mp4", "MP4"),
];

/// Langues proposées pour `LANG` (code BCP 47, libellé).
pub const LANGUAGES: &[(&str, &str)] = &[
    ("fr", "français"),
    ("en", "anglais"),
    ("de", "allemand"),
    ("it", "italien"),
    ("es", "espagnol"),
    ("la", "latin"),
    ("oc", "occitan"),
    ("br", "breton"),
    ("nl", "néerlandais"),
];

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
    // Le mot en cours de saisie (pour les valeurs à plusieurs mots), suite comprise.
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
                    detail: "pointeur vide".into(),
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
                .filter_map(|v| keyword(v, schema::enum_label_fr(v).unwrap_or(""), q, SuggestionKind::Value))
                .collect();
            let end = if list { word_end } else { value_end };
            (start..end, items)
        }
        Payload::YOrEmpty => (
            value_start..value_end,
            keyword("Y", "a eu lieu (sans date ni lieu)", value, SuggestionKind::Value).into_iter().collect(),
        ),
        Payload::Date => {
            if word.chars().next().is_some_and(|c| c.is_ascii_digit()) {
                return None;
            }
            let upper = upper_ascii(value);
            let months = if upper.contains("FRENCH_R") { MONTHS_FRENCH_R } else { MONTHS };
            // Les mois ne viennent qu'après un jour ou en début de date simple.
            let after_day = value[..word_start - value_start]
                .trim_end()
                .rsplit(' ')
                .next()
                .is_some_and(|w| w.parse::<u8>().is_ok());
            let mut items: Vec<Suggestion> = Vec::new();
            if after_day {
                items.extend(months.iter().filter_map(|(m, l)| keyword(m, l, word, SuggestionKind::Value)));
            } else {
                items.extend(DATE_KEYWORDS.iter().filter_map(|(k, l)| keyword(k, l, word, SuggestionKind::Keyword)));
                if !word.is_empty() {
                    items.extend(months.iter().filter_map(|(m, l)| keyword(m, l, word, SuggestionKind::Value)));
                }
            }
            (word_start..word_end, items)
        }
        Payload::MediaType => (
            value_start..value_end,
            MEDIA_TYPES.iter().filter_map(|(m, l)| keyword(m, l, value, SuggestionKind::Value)).collect(),
        ),
        Payload::Language => (
            value_start..value_end,
            LANGUAGES.iter().filter_map(|(m, l)| keyword(m, l, value, SuggestionKind::Value)).collect(),
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

    /// `|` marque le curseur.
    fn at(text_with_cursor: &str) -> Option<Completion> {
        let offset = text_with_cursor.find('|').unwrap();
        let text = text_with_cursor.replacen('|', "", 1);
        complete(&text, offset, &catalog())
    }

    fn labels(c: &Completion) -> Vec<&str> {
        c.items.iter().map(|s| s.label.as_str()).collect()
    }

    #[test]
    fn balises_d_un_individu() {
        let c = at("0 @I1@ INDI\n1 BI|").unwrap();
        assert_eq!(labels(&c), ["BIRT"]);
        assert_eq!(c.replace, 14..16);
        assert_eq!(c.items[0].insert, "BIRT");
        assert_eq!(c.items[0].matched, 2);
    }

    #[test]
    fn recherche_par_libelle_francais() {
        let c = at("0 @I1@ INDI\n1 nais|").unwrap();
        assert!(labels(&c).contains(&"BIRT"));
        let c = at("0 @I1@ INDI\n1 deces|").unwrap();
        assert!(labels(&c).contains(&"DEAT"));
    }

    #[test]
    fn balises_dans_un_evenement() {
        let c = at("0 @I1@ INDI\n1 BIRT\n2 |").unwrap();
        let l = labels(&c);
        assert!(l.contains(&"DATE") && l.contains(&"PLAC") && l.contains(&"SOUR"));
        assert!(!l.contains(&"BIRT"));
        // Ajoute l'espace après une balise qui attend une valeur.
        assert_eq!(c.items.iter().find(|s| s.label == "DATE").unwrap().insert, "DATE ");
    }

    #[test]
    fn niveau_seul() {
        let c = at("0 @I1@ INDI\n1 BIRT\n2|").unwrap();
        assert_eq!(c.items.iter().find(|s| s.label == "DATE").unwrap().insert, "2 DATE ");
        assert_eq!(c.replace, 19..20);
    }

    #[test]
    fn cardinalite_respectee() {
        let c = at("0 @I1@ INDI\n1 BIRT\n2 DATE 1850\n2 |\n2 PLAC Troyes").unwrap();
        let l = labels(&c);
        assert!(!l.contains(&"DATE") && !l.contains(&"PLAC"));
        assert!(l.contains(&"NOTE"));
        // SEX n'apparaît qu'une fois par individu.
        let c = at("0 @I1@ INDI\n1 SEX M\n1 S|").unwrap();
        assert!(!labels(&c).contains(&"SEX"));
    }

    #[test]
    fn pas_de_suggestion_sous_une_extension() {
        assert!(at("0 @I1@ INDI\n1 _MILT\n2 |").is_none());
        assert!(at("0 @I1@ INDI\n1 _MI|").is_none());
    }

    #[test]
    fn balises_obligatoires_d_abord() {
        let c = at("0 @I1@ INDI\n1 ASSO @I2@\n2 |").unwrap();
        assert_eq!(c.items[0].label, "ROLE");
    }

    #[test]
    fn enregistrements_au_niveau_zero() {
        let c = at("0 @X1@ |").unwrap();
        let l = labels(&c);
        assert!(l.contains(&"INDI") && l.contains(&"SOUR"));
    }

    #[test]
    fn pointeurs_du_bon_type() {
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
    fn valeurs_enumerees() {
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
    fn lieux_existants() {
        let c = at("0 @I1@ INDI\n1 DEAT\n2 PLAC troy|").unwrap();
        assert_eq!(labels(&c), [", Troyes, 10000, Aube, Grand Est, France"]);
    }

    #[test]
    fn hors_contexte() {
        assert!(at("|").is_none());
        assert!(at("0 @I1@ INDI\n1 NAME Jean /GU|").is_none());
        assert!(at("0 @I1@ INDI\n1 NOTE texte libre|").is_none());
    }
}
