//! GEDCOM 7 schema: which substructures are allowed under each structure, with which cardinality,
//! and which type of value they carry.
//!
//! The tables come from FamilySearch's official registry (`GEDCOM-registries`, file
//! `generated_files/g7validation.json`) and are generated into [`data`] by `docs/schema/gen.py`.
//! The translated labels are in `locales/schema.yml`.

#[rustfmt::skip]
mod data;

use std::borrow::Cow;

/// Number of occurrences allowed for a substructure (`{0:1}`, `{1:M}`…).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Card {
    Optional,
    Required,
    Many,
    AtLeastOne,
}

impl Card {
    /// At most one occurrence.
    pub fn single(self) -> bool {
        matches!(self, Card::Optional | Card::Required)
    }

    /// At least one occurrence.
    pub fn required(self) -> bool {
        matches!(self, Card::Required | Card::AtLeastOne)
    }
}

/// Type of the value carried by a structure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Payload {
    /// No value.
    None,
    /// `Y` or nothing (events).
    YOrEmpty,
    /// Pointer to a record of this type (`INDI`, `FAM`…).
    Pointer(&'static str),
    /// Enumerated value (or list of comma-separated values).
    Enum {
        list: bool,
        values: &'static [&'static str],
    },
    Date,
    Age,
    Time,
    MediaType,
    Language,
    Text,
}

#[derive(Debug, Clone, Copy)]
pub struct Sub {
    pub tag: &'static str,
    def: usize,
    pub card: Card,
}

impl Sub {
    pub fn def(&self) -> &'static StructDef {
        &data::STRUCTS[self.def]
    }
}

/// Definition of a GEDCOM 7 structure.
#[derive(Debug)]
pub struct StructDef {
    /// Identifier in the registry (`record-INDI`, `INDI-NAME`, `DATE`…).
    pub id: &'static str,
    /// English label from the registry.
    pub label: &'static str,
    pub payload: Payload,
    pub subs: &'static [Sub],
}

impl StructDef {
    /// Standard substructure `tag`, if it is allowed here.
    pub fn sub(&self, tag: &str) -> Option<&'static Sub> {
        // `subs` is generated sorted by tag.
        self.subs.binary_search_by(|s| s.tag.cmp(tag)).ok().map(|i| &self.subs[i])
    }
}

/// Level 0 of the file (records, `HEAD`, `TRLR`).
pub fn root() -> &'static StructDef {
    &data::STRUCTS[0]
}

/// Definition of a level 0 record (`INDI`, `FAM`, `SOUR`…).
pub fn record(tag: &str) -> Option<&'static StructDef> {
    root().sub(tag).map(Sub::def)
}

/// Label of a structure in the current language, or else the English label of the registry.
pub fn label(tag: &str, def: &StructDef) -> Cow<'static, str> {
    crate::i18n::lookup(&format!("struct.{}", def.id))
        .or_else(|| crate::i18n::lookup(&format!("tag.{tag}")))
        .unwrap_or(Cow::Borrowed(def.label))
}

/// Label of an enumerated value in the current language, when one is useful.
pub fn enum_label(value: &str) -> Option<Cow<'static, str>> {
    crate::i18n::lookup(&format!("enum.{value}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn substructures_are_sorted() {
        for d in data::STRUCTS {
            assert!(d.subs.windows(2).all(|w| w[0].tag < w[1].tag), "{} is not sorted", d.id);
        }
    }

    #[test]
    fn individual() {
        let indi = record("INDI").unwrap();
        assert_eq!(indi.id, "record-INDI");
        let birt = indi.sub("BIRT").unwrap();
        assert_eq!(birt.card, Card::Many);
        assert_eq!(birt.def().payload, Payload::YOrEmpty);
        assert!(birt.def().sub("DATE").unwrap().card.single());
        assert_eq!(indi.sub("FAMC").unwrap().def().payload, Payload::Pointer("FAM"));
        assert!(
            matches!(indi.sub("SEX").unwrap().def().payload, Payload::Enum { values, .. } if values.contains(&"F"))
        );
        assert!(indi.sub("HUSB").is_none());
    }
}
