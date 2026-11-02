//! Schéma GEDCOM 7 : quelles sous-structures sont permises sous chaque
//! structure, avec quelle cardinalité, et quel type de valeur elles portent.
//!
//! Les tables viennent du registre officiel de FamilySearch
//! (`GEDCOM-registries`, fichier `generated_files/g7validation.json`) et sont
//! générées dans [`data`] par `docs/schema/gen.py`. Les libellés traduits
//! sont dans `locales/schema.yml`.

#[rustfmt::skip]
mod data;

use std::borrow::Cow;

/// Nombre d'occurrences permises d'une sous-structure (`{0:1}`, `{1:M}`…).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Card {
    Optional,
    Required,
    Many,
    AtLeastOne,
}

impl Card {
    /// Au plus une occurrence.
    pub fn single(self) -> bool {
        matches!(self, Card::Optional | Card::Required)
    }

    /// Au moins une occurrence.
    pub fn required(self) -> bool {
        matches!(self, Card::Required | Card::AtLeastOne)
    }

    pub fn notation(self) -> &'static str {
        match self {
            Card::Optional => "{0:1}",
            Card::Required => "{1:1}",
            Card::Many => "{0:M}",
            Card::AtLeastOne => "{1:M}",
        }
    }
}

/// Type de la valeur portée par une structure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Payload {
    /// Pas de valeur.
    None,
    /// `Y` ou rien (événements).
    YOrEmpty,
    /// Pointeur vers un enregistrement de ce type (`INDI`, `FAM`…).
    Pointer(&'static str),
    /// Valeur énumérée (ou liste de valeurs séparées par des virgules).
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

/// Définition d'une structure GEDCOM 7.
#[derive(Debug)]
pub struct StructDef {
    /// Identifiant dans le registre (`record-INDI`, `INDI-NAME`, `DATE`…).
    pub id: &'static str,
    /// Libellé anglais du registre.
    pub label: &'static str,
    pub payload: Payload,
    pub subs: &'static [Sub],
}

impl StructDef {
    /// Sous-structure standard `tag`, si elle est permise ici.
    pub fn sub(&self, tag: &str) -> Option<&'static Sub> {
        // `subs` est généré trié par balise.
        self.subs.binary_search_by(|s| s.tag.cmp(tag)).ok().map(|i| &self.subs[i])
    }
}

/// Le niveau 0 du fichier (enregistrements, `HEAD`, `TRLR`).
pub fn root() -> &'static StructDef {
    &data::STRUCTS[0]
}

/// Définition d'un enregistrement de niveau 0 (`INDI`, `FAM`, `SOUR`…).
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
    fn substructures_sont_triees() {
        for d in data::STRUCTS {
            assert!(d.subs.windows(2).all(|w| w[0].tag < w[1].tag), "{} non trié", d.id);
        }
    }

    #[test]
    fn individu() {
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
