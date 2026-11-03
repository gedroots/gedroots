//! The Atlas: what genealogy records each country kept and from when, who may read them, and the
//! legal age at marriage. One YAML file per country in `atlas/` (format documented in
//! `docs/atlas.md`), embedded at build time; `generic` covers the other countries and the periods
//! that a country's file does not cover.

use crate::leads::{Goal, RecordType};
use crate::model::sort_key;
use serde::Deserialize;
use std::collections::BTreeMap;
use std::sync::LazyLock;

/// Version of the format that this library reads.
pub const FORMAT: u32 = 1;

/// Code of the rules used for countries without a file.
pub const GENERIC: &str = "generic";

/// The rules of a country.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Country {
    pub format: u32,
    /// ISO 3166-1 alpha-2 code, or `generic`.
    pub code: String,
    /// English name.
    pub name: String,
    /// Names of the country found in places, in any language.
    #[serde(default)]
    pub names: Vec<String>,
    /// Minimum age at marriage, by period.
    #[serde(default)]
    pub marriage_age: Vec<MarriageAge>,
    /// Kinds of records, best first.
    #[serde(default)]
    pub records: Vec<Record>,
}

/// Minimum age at marriage from `from` to `until` (years included).
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MarriageAge {
    pub from: Option<i32>,
    pub until: Option<i32>,
    pub male: i32,
    pub female: i32,
}

/// A kind of records that the country kept.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Record {
    #[serde(rename = "type")]
    pub kind: RecordType,
    /// Name in the country (“état civil”, “Standesamt”).
    #[serde(default)]
    pub local_name: String,
    /// What it helps to find.
    pub goals: Vec<Goal>,
    /// First and last years kept (included).
    pub from: Option<i32>,
    pub until: Option<i32>,
    /// Last year that archives usually put online.
    pub online_until: Option<i32>,
    /// An index of other records: worth it when the period is long.
    #[serde(default)]
    pub index: bool,
    /// Years after which a record becomes public, for every goal or by goal.
    pub public_after: Option<PublicAfter>,
    /// Years of the record when it was not kept every year (census years).
    #[serde(default)]
    pub years: Vec<i32>,
    /// Years to add after the event (a probate record is made after the death).
    #[serde(default)]
    pub extends_by: i32,
    /// Only for people of this sex.
    pub sex: Option<Sex>,
    /// Only for people born in these years; the record's year is the birth year + `offset`.
    pub born: Option<[i32; 2]>,
    #[serde(default)]
    pub offset: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Sex {
    Male,
    Female,
}

/// Access delay: a number of years for every goal, or by goal.
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum PublicAfter {
    All(i32),
    ByGoal(BTreeMap<Goal, i32>),
}

impl PublicAfter {
    /// Delay for a goal; the parents are found in birth records.
    pub fn years(&self, goal: Goal) -> Option<i32> {
        let goal = if goal == Goal::Parents { Goal::Birth } else { goal };
        match self {
            PublicAfter::All(n) => Some(*n),
            PublicAfter::ByGoal(m) => m.get(&goal).copied(),
        }
    }
}

impl Country {
    pub fn is_generic(&self) -> bool {
        self.code == GENERIC
    }

    /// Minimum age at marriage in `year`, otherwise that of the generic rules.
    pub fn marriage_age(&self, year: i32, male: bool) -> i32 {
        let find = |p: &Country| {
            p.marriage_age
                .iter()
                .find(|a| a.from.is_none_or(|f| f <= year) && a.until.is_none_or(|u| year < u))
                .or(p.marriage_age.last())
                .map(|a| if male { a.male } else { a.female })
        };
        find(self).or_else(|| find(generic())).unwrap_or(15)
    }
}

static SOURCES: &[(&str, &str)] = include!(concat!(env!("OUT_DIR"), "/atlas.rs"));

static COUNTRIES: LazyLock<Vec<Country>> = LazyLock::new(|| {
    SOURCES
        .iter()
        .map(|(file, text)| serde_saphyr::from_str::<Country>(text).unwrap_or_else(|e| panic!("atlas/{file}.yml: {e}")))
        .collect()
});

/// Names of the countries (normalized by [`sort_key`]) and their country.
static NAMES: LazyLock<Vec<(String, usize)>> = LazyLock::new(|| {
    COUNTRIES.iter().enumerate().flat_map(|(i, p)| p.names.iter().map(move |n| (sort_key(n), i))).collect()
});

/// Every country of the Atlas, the generic one included.
pub fn countries() -> &'static [Country] {
    &COUNTRIES
}

/// The rules of the countries without a file.
pub fn generic() -> &'static Country {
    COUNTRIES.iter().find(|p| p.is_generic()).expect("atlas/generic.yml")
}

/// Country of a country code (`FR`).
pub fn by_code(code: &str) -> Option<&'static Country> {
    COUNTRIES.iter().find(|p| p.code.eq_ignore_ascii_case(code))
}

/// Country of the country named in a place (“Agneaux, Manche, France”, “Agneaux (Manche, France)”):
/// the generic rules for a country without a file, `None` when no country is named.
pub fn in_place(place: &str) -> Option<&'static Country> {
    place
        .split([',', '(', ')'])
        .map(|s| sort_key(s.trim()))
        .filter(|s| !s.is_empty())
        .rev()
        .find_map(|s| NAMES.iter().find(|(n, _)| *n == s).map(|(_, i)| &COUNTRIES[*i]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_country_is_valid() {
        let mut codes = std::collections::HashSet::new();
        for (file, _) in SOURCES {
            let p = by_code(file).unwrap_or_else(|| panic!("{file}.yml: code differs from the file name"));
            assert_eq!(p.format, FORMAT, "{file}");
            assert!(codes.insert(p.code.clone()), "{file}: code used twice");
            assert!(p.is_generic() || (p.code.len() == 2 && p.code.chars().all(|c| c.is_ascii_uppercase())));
            for r in &p.records {
                assert!(!r.goals.is_empty(), "{file}: {:?} has no goal", r.kind);
                if let (Some(a), Some(b)) = (r.from, r.until) {
                    assert!(a <= b, "{file}: {:?} ends before it starts", r.kind);
                }
                assert!(r.years.windows(2).all(|w| w[0] < w[1]), "{file}: years out of order");
            }
        }
        // A name leads to one country only.
        let mut seen = std::collections::HashMap::new();
        for (n, i) in NAMES.iter() {
            if let Some(j) = seen.insert(n.clone(), *i) {
                assert_eq!(i, &j, "{n} names two countries");
            }
        }
        assert!(generic().records.iter().any(|r| r.kind == RecordType::CivilRegistration));
    }

    #[test]
    fn finds_the_country_of_a_place() {
        assert_eq!(in_place("Agneaux, Manche, France").unwrap().code, "FR");
        assert_eq!(in_place("Ghazaouet (Tlemcen, Algérie)").unwrap().code, "DZ");
        assert_eq!(in_place("Gent, Oost-Vlaanderen, België").unwrap().code, "BE");
        assert!(in_place("Montréal, Québec, Canada").unwrap().is_generic());
        assert!(in_place("Agneaux").is_none());
    }

    #[test]
    fn marriage_age_by_period() {
        let fr = by_code("FR").unwrap();
        assert_eq!(fr.marriage_age(1750, false), 12);
        assert_eq!(fr.marriage_age(1850, false), 15);
        assert_eq!(fr.marriage_age(1850, true), 18);
        assert_eq!(fr.marriage_age(2010, false), 18);
    }
}
