//! Date intervals and age calculation.
//!
//! A GEDCOM date is often imprecise (`1850`, `ABT 1850`, `BEF 1809`…). To compare two dates without
//! false alarms, it is turned into an interval of days `[lo, hi]` (bounds possibly open): “A is
//! before B” is only asserted if it holds for every possible value.

use crate::date::calendar::UNIX_EPOCH_RD;
use crate::date::{Date, SimpleDate};
use rust_i18n::t;

/// Margin applied to “about”, “estimated” and “calculated” dates.
pub const FUZZ_YEARS: i64 = 5;
const DAYS_PER_YEAR: f64 = 365.2425;

/// Number of days since 1 January 1970 (proleptic Gregorian calendar).
pub fn days_from_civil(y: i64, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let m = m as i64;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d as i64 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// Interval of days covered by a simple date (exact conversions between calendars).
pub fn simple_range(d: &SimpleDate) -> Option<(i64, i64)> {
    let (lo, hi) = d.rd_range()?;
    Some((lo - UNIX_EPOCH_RD, hi - UNIX_EPOCH_RD))
}

/// Possible interval of days for a date; `None` = open bound.
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

    /// True if every possible value of `self` is strictly before every value of `other`.
    pub fn surely_before(&self, other: &Span) -> bool {
        matches!((self.hi, other.lo), (Some(a), Some(b)) if a < b)
    }

    /// Middle of the interval, if both bounds are known.
    pub fn mid(&self) -> Option<i64> {
        Some((self.lo? + self.hi?) / 2)
    }
}

/// Possible gap between two dates, in days.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Age {
    pub min: Option<i64>,
    pub max: Option<i64>,
}

impl Age {
    /// Age at date `at` of a person born at `birth`.
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

    /// Most likely age, in years (middle of the interval).
    pub fn mid_years(&self) -> Option<f64> {
        Some((self.min? + self.max?) as f64 / 2. / DAYS_PER_YEAR)
    }

    /// “34 years”, “~34 years”, “3 months”, “over 20 years”…
    pub fn display(&self) -> Option<String> {
        fn fmt(days: i64) -> String {
            let days = days.max(0) as usize;
            if days < 31 {
                return if days <= 1 { t!("age.newborn").into() } else { tn!("age.days", days).into() };
            }
            if days < 730 {
                return tn!("age.months", (days as f64 / 30.44) as usize).into();
            }
            tn!("age.years", (days as f64 / DAYS_PER_YEAR) as usize).into()
        }
        match (self.min, self.max) {
            (Some(a), Some(b)) if b < 0 => (a < 0).then(|| t!("age.before_birth").into()),
            (Some(a), Some(b)) if b - a <= 31 => Some(fmt(a)),
            (Some(a), Some(b)) => Some(format!("~{}", fmt((a.max(0) + b) / 2))),
            (Some(a), None) if a > 0 => Some(t!("age.over", age = fmt(a)).into()),
            (None, Some(b)) if b > 0 => Some(t!("age.under", age = fmt(b)).into()),
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
        let age = |d: &str| Age::between(&b, &Span::parse(d).unwrap()).display().unwrap();
        assert_eq!(age("2 JUN 1940"), "28 years");
        assert_eq!(age("1987"), "~75 years");
        assert_eq!(age("JUN 1912"), "2 months");
        assert_eq!(age("AFT 1950"), "over 37 years");
    }
}
