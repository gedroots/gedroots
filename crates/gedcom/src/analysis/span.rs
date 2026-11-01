//! Intervalles de dates et calcul d'âges.
//!
//! Une date GEDCOM est souvent imprécise (`1850`, `ABT 1850`, `BEF 1809`…).
//! Pour comparer deux dates sans fausses alertes, on la convertit en un
//! intervalle de jours `[lo, hi]` (bornes éventuellement ouvertes) : « A est
//! avant B » n'est affirmé que si c'est vrai pour toutes les valeurs possibles.

use crate::date::calendar::UNIX_EPOCH_RD;
use crate::date::{Date, SimpleDate};

/// Marge appliquée aux dates « vers », « estimée » et « calculée ».
pub const FUZZ_YEARS: i64 = 5;
const DAYS_PER_YEAR: f64 = 365.2425;

/// Nombre de jours depuis le 1er janvier 1970 (calendrier grégorien proleptique).
pub fn days_from_civil(y: i64, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let m = m as i64;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d as i64 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// Intervalle de jours couvert par une date simple (conversions exactes entre calendriers).
pub fn simple_range(d: &SimpleDate) -> Option<(i64, i64)> {
    let (lo, hi) = d.rd_range()?;
    Some((lo - UNIX_EPOCH_RD, hi - UNIX_EPOCH_RD))
}

/// Intervalle de jours possible pour une date ; `None` = borne ouverte.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span {
    pub lo: Option<i64>,
    pub hi: Option<i64>,
}

impl Span {
    pub fn of(date: &Date) -> Option<Span> {
        let fuzz = (FUZZ_YEARS as f64 * DAYS_PER_YEAR) as i64;
        let r = |d: &SimpleDate| simple_range(d);
        Some(match date {
            Date::Exact(d) | Date::From(d) | Date::To(d) => {
                let (lo, hi) = r(d)?;
                Span { lo: Some(lo), hi: Some(hi) }
            }
            Date::About(d) | Date::Estimated(d) | Date::Calculated(d) => {
                let (lo, hi) = r(d)?;
                Span { lo: Some(lo - fuzz), hi: Some(hi + fuzz) }
            }
            Date::Before(d) => Span { lo: None, hi: Some(r(d)?.1) },
            Date::After(d) => Span { lo: Some(r(d)?.0), hi: None },
            Date::Between(a, b) | Date::FromTo(a, b) => Span { lo: Some(r(a)?.0), hi: Some(r(b)?.1) },
            Date::Other(_) => return None,
        })
    }

    pub fn parse(raw: &str) -> Option<Span> {
        Span::of(&Date::parse(raw))
    }

    /// Vrai si toute valeur possible de `self` précède strictement toute valeur de `other`.
    pub fn surely_before(&self, other: &Span) -> bool {
        matches!((self.hi, other.lo), (Some(a), Some(b)) if a < b)
    }

    /// Milieu de l'intervalle, si les deux bornes sont connues.
    pub fn mid(&self) -> Option<i64> {
        Some((self.lo? + self.hi?) / 2)
    }
}

/// Écart possible entre deux dates, en jours.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Age {
    pub min: Option<i64>,
    pub max: Option<i64>,
}

impl Age {
    /// Âge à la date `at` d'une personne née à `birth`.
    pub fn between(birth: &Span, at: &Span) -> Age {
        Age {
            min: match (birth.hi, at.lo) {
                (Some(b), Some(a)) => Some(a - b),
                _ => None,
            },
            max: match (birth.lo, at.hi) {
                (Some(b), Some(a)) => Some(a - b),
                _ => None,
            },
        }
    }

    pub fn min_years(&self) -> Option<f64> {
        self.min.map(|d| d as f64 / DAYS_PER_YEAR)
    }

    pub fn max_years(&self) -> Option<f64> {
        self.max.map(|d| d as f64 / DAYS_PER_YEAR)
    }

    /// Âge le plus probable, en années (milieu de l'intervalle).
    pub fn mid_years(&self) -> Option<f64> {
        Some((self.min? + self.max?) as f64 / 2. / DAYS_PER_YEAR)
    }

    /// « 34 ans », « ~34 ans », « 3 mois », « plus de 20 ans »…
    pub fn to_french(&self) -> Option<String> {
        fn fmt(days: i64) -> String {
            let days = days.max(0);
            if days < 31 {
                return if days <= 1 { "nouveau-né".into() } else { format!("{days} jours") };
            }
            if days < 730 {
                let m = (days as f64 / 30.44) as i64;
                return format!("{m} mois");
            }
            let y = (days as f64 / DAYS_PER_YEAR) as i64;
            format!("{y} ans")
        }
        match (self.min, self.max) {
            (Some(a), Some(b)) if b < 0 => (a < 0).then(|| "avant la naissance".into()),
            (Some(a), Some(b)) if b - a <= 31 => Some(fmt(a)),
            (Some(a), Some(b)) => Some(format!("~{}", fmt((a.max(0) + b) / 2))),
            (Some(a), None) if a > 0 => Some(format!("plus de {}", fmt(a))),
            (None, Some(b)) if b > 0 => Some(format!("moins de {}", fmt(b))),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn civil_days() {
        assert_eq!(days_from_civil(1970, 1, 1), 0);
        assert_eq!(days_from_civil(2000, 3, 1), 11_017);
        assert_eq!(days_from_civil(1792, 9, 22), days_from_civil(1792, 9, 21) + 1);
    }

    #[test]
    fn republican_dates() {
        // 30 germinal an IV = 19 avril 1796.
        let s = Span::parse("FRENCH_R 30 GERM 4").unwrap();
        assert_eq!(s.lo.unwrap(), days_from_civil(1796, 4, 19));
    }

    #[test]
    fn ordering_is_conservative() {
        let a = Span::parse("1850").unwrap();
        let b = Span::parse("ABT 1853").unwrap();
        assert!(!a.surely_before(&b));
        assert!(Span::parse("1850").unwrap().surely_before(&Span::parse("1851").unwrap()));
        assert!(!Span::parse("BEF 1900").unwrap().surely_before(&Span::parse("1850").unwrap()));
    }

    #[test]
    fn ages() {
        let b = Span::parse("14 MAR 1912").unwrap();
        let age = |d: &str| Age::between(&b, &Span::parse(d).unwrap()).to_french().unwrap();
        assert_eq!(age("2 JUN 1940"), "28 ans");
        assert_eq!(age("1987"), "~75 ans");
        assert_eq!(age("JUN 1912"), "2 mois");
        assert_eq!(age("AFT 1950"), "plus de 37 ans");
    }
}
