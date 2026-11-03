//! Complete GEDCOM 7 dates: qualifiers (`ABT`, `CAL`, `EST`), ranges (`BEF`, `AFT`, `BET … AND …`),
//! periods (`FROM … TO …`), Gregorian, Julian, French Republican and Hebrew calendars (exact
//! conversions), extension calendars (`_XXX`) kept as is, and phrases (`PHRASE`).
//!
//! - [`Date::parse`] reads the GEDCOM value (lenient, never rewriting it);
//! - [`parse_input`] understands everyday input (“vers 1850”, “15 brumaire an XII”, “between 1820
//!   and 1825 (from the age)”…);
//! - [`Date::to_input_text`] gives back a text that [`parse_input`] reads identically.

pub mod calendar;
mod input;

pub use input::{Parsed, parse_input};

use crate::node::Node;
use calendar as cal;
use rust_i18n::t;
use std::borrow::Cow;

#[derive(Debug, Clone, PartialEq, Eq, Default, Hash)]
pub enum Calendar {
    #[default]
    Gregorian,
    Julian,
    FrenchRepublican,
    Hebrew,
    /// Extension calendar (`_MAYA`…): kept, without conversion.
    Extension(String),
}

impl Calendar {
    /// Known calendars, in display order.
    pub const KNOWN: [Calendar; 4] =
        [Calendar::Gregorian, Calendar::Julian, Calendar::FrenchRepublican, Calendar::Hebrew];

    pub fn tag(&self) -> &str {
        match self {
            Calendar::Gregorian => "GREGORIAN",
            Calendar::Julian => "JULIAN",
            Calendar::FrenchRepublican => "FRENCH_R",
            Calendar::Hebrew => "HEBREW",
            Calendar::Extension(t) => t,
        }
    }

    pub fn from_tag(t: &str) -> Option<Calendar> {
        Some(match t {
            "GREGORIAN" => Calendar::Gregorian,
            "JULIAN" => Calendar::Julian,
            "FRENCH_R" => Calendar::FrenchRepublican,
            "HEBREW" => Calendar::Hebrew,
            t if t.len() > 1 && t.starts_with('_') => Calendar::Extension(t.to_string()),
            _ => return None,
        })
    }

    pub fn label(&self) -> Cow<'_, str> {
        match self {
            Calendar::Gregorian => t!("calendar.gregorian"),
            Calendar::Julian => t!("calendar.julian"),
            Calendar::FrenchRepublican => t!("calendar.republican"),
            Calendar::Hebrew => t!("calendar.hebrew"),
            Calendar::Extension(t) => Cow::Borrowed(t),
        }
    }

    fn months(&self) -> &'static [&'static str] {
        match self {
            Calendar::Gregorian | Calendar::Julian => &GREG_MONTHS,
            Calendar::FrenchRepublican => &REP_MONTHS,
            Calendar::Hebrew => &HEB_MONTHS,
            Calendar::Extension(_) => &[],
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SimpleDate {
    pub calendar: Calendar,
    pub day: Option<u8>,
    /// Month 1..=12 (1..=13 for the Republican and Hebrew calendars).
    pub month: Option<u8>,
    /// Extension month (`_XUL`…) or month of an extension calendar, kept as is.
    pub month_tag: Option<String>,
    pub year: Option<i32>,
    pub bce: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Date {
    Exact(SimpleDate),
    About(SimpleDate),
    Calculated(SimpleDate),
    Estimated(SimpleDate),
    Before(SimpleDate),
    After(SimpleDate),
    Between(SimpleDate, SimpleDate),
    From(SimpleDate),
    To(SimpleDate),
    FromTo(SimpleDate, SimpleDate),
    /// Unrecognized value, kept as is.
    Other(String),
}

/// Precision of a date, as the user picks it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DateKind {
    Exact,
    About,
    Calculated,
    Estimated,
    Before,
    After,
    Between,
    FromTo,
    From,
    To,
}

impl DateKind {
    pub const ALL: [DateKind; 10] = [
        DateKind::Exact,
        DateKind::About,
        DateKind::Estimated,
        DateKind::Calculated,
        DateKind::Before,
        DateKind::After,
        DateKind::Between,
        DateKind::FromTo,
        DateKind::From,
        DateKind::To,
    ];

    pub fn label(self) -> Cow<'static, str> {
        match self {
            DateKind::Exact => t!("date_kind.exact"),
            DateKind::About => t!("date_kind.about"),
            DateKind::Calculated => t!("date_kind.calculated"),
            DateKind::Estimated => t!("date_kind.estimated"),
            DateKind::Before => t!("date_kind.before"),
            DateKind::After => t!("date_kind.after"),
            DateKind::Between => t!("date_kind.between"),
            DateKind::FromTo => t!("date_kind.from_to"),
            DateKind::From => t!("date_kind.from"),
            DateKind::To => t!("date_kind.to"),
        }
    }

    pub fn two_dates(self) -> bool {
        matches!(self, DateKind::Between | DateKind::FromTo)
    }
}

const GREG_MONTHS: [&str; 12] = ["JAN", "FEB", "MAR", "APR", "MAY", "JUN", "JUL", "AUG", "SEP", "OCT", "NOV", "DEC"];
const REP_MONTHS: [&str; 13] =
    ["VEND", "BRUM", "FRIM", "NIVO", "PLUV", "VENT", "GERM", "FLOR", "PRAI", "MESS", "THER", "FRUC", "COMP"];
const HEB_MONTHS: [&str; 13] =
    ["TSH", "CSH", "KSL", "TVT", "SHV", "ADR", "ADS", "NSN", "IYR", "SVN", "TMZ", "AAV", "ELL"];

/// Short name of a Gregorian or Julian month (1..=12): “Mar”, “mars”.
fn month_name(m: u32, lang: &str) -> Cow<'static, str> {
    match GREG_MONTHS.get((m as usize).wrapping_sub(1)) {
        Some(tag) => crate::i18n::lookup_in(&format!("month.{tag}"), lang).unwrap_or(Cow::Borrowed("?")),
        None => Cow::Borrowed("?"),
    }
}

/// Name of a month of the French Republican or Hebrew calendar (1..=13).
fn calendar_month_name(tags: &[&str], m: u8, lang: &str) -> Cow<'static, str> {
    match tags.get((m as usize).wrapping_sub(1)) {
        Some(tag) => crate::i18n::lookup_in(&format!("month.{tag}"), lang).unwrap_or(Cow::Borrowed("?")),
        None => Cow::Borrowed("?"),
    }
}

/// Readable Gregorian year (`1803`, `44 BC`) from an astronomical year.
fn year_text(y: i64, lang: &str) -> String {
    if y <= 0 { t!("date.bce_year", year = 1 - y, locale = lang).into_owned() } else { y.to_string() }
}

fn day_text(d: u32, lang: &str) -> String {
    if d == 1 { t!("date.first_day", locale = lang).into_owned() } else { d.to_string() }
}

pub(crate) fn roman(mut n: i32) -> String {
    if n <= 0 || n > 3999 {
        return n.to_string();
    }
    let table = [
        (1000, "M"),
        (900, "CM"),
        (500, "D"),
        (400, "CD"),
        (100, "C"),
        (90, "XC"),
        (50, "L"),
        (40, "XL"),
        (10, "X"),
        (9, "IX"),
        (5, "V"),
        (4, "IV"),
        (1, "I"),
    ];
    let mut s = String::new();
    for (v, r) in table {
        while n >= v {
            s.push_str(r);
            n -= v;
        }
    }
    s
}

impl SimpleDate {
    /// Complete Gregorian date.
    pub fn ymd(year: i32, month: u8, day: u8) -> SimpleDate {
        SimpleDate { day: Some(day), month: Some(month), year: Some(year), ..Default::default() }
    }

    pub(crate) fn parse(s: &str) -> Option<SimpleDate> {
        let mut toks: Vec<&str> = s.split_whitespace().collect();
        let mut d = SimpleDate::default();
        if toks.len() > 1
            && let Some(c) = Calendar::from_tag(toks[0])
        {
            d.calendar = c;
            toks.remove(0);
        }
        if toks.len() > 1 && toks.last() == Some(&"BCE") {
            d.bce = true;
            toks.pop();
        }
        let months = d.calendar.months();
        let month = |m: &str, d: &mut SimpleDate| -> Option<()> {
            match months.iter().position(|x| *x == m) {
                Some(i) => d.month = Some(i as u8 + 1),
                None if m.len() > 1 && m.starts_with('_') => d.month_tag = Some(m.to_string()),
                None => return None,
            }
            Some(())
        };
        let num = |x: &str| -> Option<i32> { x.bytes().all(|b| b.is_ascii_digit()).then(|| x.parse().ok())? };
        match toks.as_slice() {
            [y] => d.year = Some(num(y)?),
            [m, y] => {
                month(m, &mut d)?;
                d.year = Some(num(y)?);
            }
            [day, m, y] => {
                d.day = Some(u8::try_from(num(day)?).ok()?);
                month(m, &mut d)?;
                d.year = Some(num(y)?);
            }
            _ => return None,
        }
        Some(d)
    }

    fn has_month(&self) -> bool {
        self.month.is_some() || self.month_tag.is_some()
    }

    /// Complete date (day, month, year) of a known calendar: exactly convertible.
    pub fn is_complete(&self) -> bool {
        self.day.is_some() && self.month.is_some() && self.year.is_some() && self.month_tag.is_none()
    }

    /// Astronomical year (1 BC = 0).
    fn astro_year(&self) -> Option<i64> {
        let y = self.year? as i64;
        Some(if self.bce { 1 - y } else { y })
    }

    /// GEDCOM value; `with_calendar` forces the `GREGORIAN` prefix.
    pub fn to_gedcom(&self, with_calendar: bool) -> String {
        let mut parts: Vec<String> = Vec::new();
        if with_calendar || self.calendar != Calendar::Gregorian {
            parts.push(self.calendar.tag().to_string());
        }
        if let Some(d) = self.day {
            parts.push(d.to_string());
        }
        if let Some(t) = &self.month_tag {
            parts.push(t.clone());
        } else if let Some(m) = self.month {
            parts.push(self.calendar.months().get(m as usize - 1).copied().unwrap_or("?").to_string());
        }
        parts.push(self.year.map(|y| y.to_string()).unwrap_or_default());
        if self.bce {
            parts.push("BCE".into());
        }
        parts.join(" ")
    }

    /// Number of days in the month (known calendars).
    fn month_days(&self) -> Option<u32> {
        let y = self.astro_year()?;
        let m = self.month? as u32;
        Some(match self.calendar {
            Calendar::Gregorian => cal::western_month_days(y, m, false),
            Calendar::Julian => cal::western_month_days(y, m, true),
            Calendar::FrenchRepublican => cal::french_month_days(y, m),
            Calendar::Hebrew => cal::hebrew_month_days(y, m),
            Calendar::Extension(_) => return None,
        })
    }

    /// Checks that the date exists (day of the month, year 0, era…).
    pub fn validate(&self) -> Result<(), String> {
        if matches!(self.calendar, Calendar::Extension(_)) || self.month_tag.is_some() {
            return Ok(());
        }
        let y = self.year.ok_or_else(|| t!("date.error.no_year").into_owned())?;
        if y <= 0 {
            return Err(t!("date.error.year_zero").into());
        }
        if self.bce && !matches!(self.calendar, Calendar::Gregorian | Calendar::Julian) {
            return Err(t!("date.error.bce_calendar").into());
        }
        if let Some(m) = self.month
            && (m == 0 || m as usize > self.calendar.months().len())
        {
            return Err(t!("date.error.month").into());
        }
        if let Some(d) = self.day {
            if self.month.is_none() {
                return Err(t!("date.error.day_without_month").into());
            }
            let max = self.month_days().unwrap_or(31);
            if d == 0 || d as u32 > max {
                let mut m = self.clone();
                m.day = None;
                return Err(t!("date.error.too_many_days", month = m.core_text(), max = max).into());
            }
        }
        Ok(())
    }

    /// Fixed days covered: one day, a month or a whole year.
    pub fn rd_range(&self) -> Option<(i64, i64)> {
        let y = self.astro_year()?;
        let fixed = |y: i64, m: u32, d: u32| -> i64 {
            match self.calendar {
                Calendar::Julian => cal::fixed_from_julian(y, m, d),
                Calendar::FrenchRepublican => cal::fixed_from_french(y, m, d),
                Calendar::Hebrew => cal::fixed_from_hebrew(y, m, d),
                _ => cal::fixed_from_gregorian(y, m, d),
            }
        };
        if matches!(self.calendar, Calendar::Extension(_)) || self.month_tag.is_some() {
            return None;
        }
        let year_start = |y: i64| match self.calendar {
            Calendar::FrenchRepublican => cal::french_new_year(y),
            Calendar::Hebrew => cal::hebrew_new_year(y),
            _ => fixed(y, 1, 1),
        };
        match (self.month, self.day) {
            (Some(m), d) if (1..=self.calendar.months().len() as u8).contains(&m) => {
                let max = self.month_days()?;
                match d {
                    Some(d) => {
                        let x = fixed(y, m as u32, (d as u32).clamp(1, max));
                        Some((x, x))
                    }
                    None => {
                        let x = fixed(y, m as u32, 1);
                        Some((x, x + max as i64 - 1))
                    }
                }
            }
            _ => Some((year_start(y), year_start(y + 1) - 1)),
        }
    }

    /// Same day in another calendar (complete date only).
    pub fn convert(&self, to: &Calendar) -> Option<SimpleDate> {
        if &self.calendar == to {
            return Some(self.clone());
        }
        if !self.is_complete() {
            return None;
        }
        let (rd, _) = self.rd_range()?;
        let (y, m, d) = match to {
            Calendar::Gregorian => cal::gregorian_from_fixed(rd),
            Calendar::Julian => cal::julian_from_fixed(rd),
            Calendar::FrenchRepublican => {
                let r = cal::french_from_fixed(rd);
                if r.0 < 1 {
                    return None;
                }
                r
            }
            Calendar::Hebrew => cal::hebrew_from_fixed(rd),
            Calendar::Extension(_) => return None,
        };
        let bce = y <= 0;
        Some(SimpleDate {
            calendar: to.clone(),
            day: Some(d as u8),
            month: Some(m as u8),
            month_tag: None,
            year: Some(if bce { 1 - y } else { y } as i32),
            bce,
        })
    }

    /// Approximate Gregorian year (for sorting and the timeline).
    pub fn gregorian_year(&self) -> Option<i32> {
        match self.calendar {
            Calendar::Gregorian | Calendar::Extension(_) => {
                let y = self.year?;
                Some(if self.bce { -y } else { y })
            }
            Calendar::Julian if self.month.is_none() => {
                let y = self.year?;
                Some(if self.bce { -y } else { y })
            }
            _ => {
                let (lo, hi) = self.rd_range()?;
                let (y, ..) = cal::gregorian_from_fixed((lo + hi) / 2);
                Some(if y <= 0 { y - 1 } else { y } as i32)
            }
        }
    }

    /// Approximate Gregorian sort key (year, month, day).
    pub fn sort_key(&self) -> (i32, u8, u8) {
        let y = self.gregorian_year().unwrap_or(i32::MIN);
        if self.calendar != Calendar::Gregorian
            && self.has_month()
            && let Some((lo, _)) = self.rd_range()
        {
            let (_, m, d) = cal::gregorian_from_fixed(lo);
            return (y, m as u8, if self.day.is_some() { d as u8 } else { 0 });
        }
        (y, self.month.unwrap_or(0), self.day.unwrap_or(0))
    }

    /// The date alone, in its calendar (`15 Brumaire year XII`, `3 Mar 1700 Julian`).
    pub fn core_text(&self) -> String {
        self.core_text_in(&crate::i18n::language())
    }

    fn core_text_in(&self, lang: &str) -> String {
        if let Calendar::Extension(_) = self.calendar {
            return self.to_gedcom(false);
        }
        if self.month_tag.is_some() {
            return self.to_gedcom(false);
        }
        let mut s = Vec::new();
        if let Some(day) = self.day {
            s.push(day_text(day as u32, lang));
        }
        if let Some(m) = self.month {
            s.push(match self.calendar {
                Calendar::FrenchRepublican => calendar_month_name(&REP_MONTHS, m, lang).into_owned(),
                Calendar::Hebrew => {
                    let leap = self.year.is_some_and(|y| cal::hebrew_leap(y as i64));
                    match m {
                        6 | 7 if !leap => t!("month.adar", locale = lang).into_owned(),
                        _ => calendar_month_name(&HEB_MONTHS, m, lang).into_owned(),
                    }
                }
                _ => month_name(m as u32, lang).into_owned(),
            });
        }
        if let Some(y) = self.year {
            s.push(match self.calendar {
                Calendar::FrenchRepublican => t!("date.republican_year", year = roman(y), locale = lang).into_owned(),
                _ => y.to_string(),
            });
        }
        if self.bce {
            s.push(t!("date.bce", locale = lang).into_owned());
        }
        match self.calendar {
            Calendar::Julian => s.push(t!("date.julian", locale = lang).into_owned()),
            Calendar::Hebrew => s.push(t!("date.hebrew", locale = lang).into_owned()),
            _ => {}
        }
        s.join(" ")
    }

    /// Gregorian equivalent (`7 Nov 1803`, `Sep–Oct 1803`, `1803–1804`).
    pub fn gregorian_equivalent(&self) -> Option<String> {
        self.gregorian_equivalent_in(&crate::i18n::language())
    }

    fn gregorian_equivalent_in(&self, lang: &str) -> Option<String> {
        let fr_day = |d| day_text(d, lang);
        let fr_month = |m| month_name(m, lang);
        let fr_year = |y| year_text(y, lang);
        if matches!(self.calendar, Calendar::Gregorian | Calendar::Extension(_)) {
            return None;
        }
        let (lo, hi) = self.rd_range()?;
        let (y1, m1, d1) = cal::gregorian_from_fixed(lo);
        let (y2, m2, d2) = cal::gregorian_from_fixed(hi);
        Some(if self.day.is_some() {
            format!("{} {} {}", fr_day(d1), fr_month(m1), fr_year(y1))
        } else if self.month.is_some() {
            match (y1 == y2, m1 == m2) {
                (true, true) if d1 == 1 && d2 == cal::western_month_days(y1, m1, false) => {
                    format!("{} {}", fr_month(m1), fr_year(y1))
                }
                (true, true) => format!("{}–{} {} {}", d1, d2, fr_month(m1), fr_year(y1)),
                (true, false) => format!("{}–{} {}", fr_month(m1), fr_month(m2), fr_year(y1)),
                _ => format!("{} {}–{} {}", fr_month(m1), fr_year(y1), fr_month(m2), fr_year(y2)),
            }
        } else if y1 == y2 {
            fr_year(y1)
        } else {
            format!("{}–{}", fr_year(y1), fr_year(y2))
        })
    }

    /// The date in the interface language, with its Gregorian equivalent for other calendars.
    pub fn display(&self) -> String {
        self.display_in(&crate::i18n::language())
    }

    fn display_in(&self, lang: &str) -> String {
        let core = self.core_text_in(lang);
        match self.gregorian_equivalent_in(lang) {
            Some(g) => format!("{core} ({g})"),
            None => core,
        }
    }
}

impl Date {
    pub fn parse(raw: &str) -> Date {
        let s = raw.trim();
        // GEDCOM 5.5.1 calendar escapes (`@#DJULIAN@`…).
        let owned;
        let s = if s.contains("@#D") {
            owned = s
                .replace("@#DGREGORIAN@", "GREGORIAN")
                .replace("@#DJULIAN@", "JULIAN")
                .replace("@#DFRENCH R@", "FRENCH_R")
                .replace("@#DHEBREW@", "HEBREW");
            owned.as_str()
        } else {
            s
        };
        let other = || Date::Other(raw.trim().to_string());
        let simple = |x: &str| SimpleDate::parse(x);
        let (head, rest) = s.split_once(' ').unwrap_or((s, ""));
        let r = match head {
            "ABT" => simple(rest).map(Date::About),
            "CAL" => simple(rest).map(Date::Calculated),
            "EST" => simple(rest).map(Date::Estimated),
            "BEF" => simple(rest).map(Date::Before),
            "AFT" => simple(rest).map(Date::After),
            "BET" => rest.split_once(" AND ").and_then(|(a, b)| Some(Date::Between(simple(a)?, simple(b)?))),
            "FROM" => match rest.split_once(" TO ") {
                Some((a, b)) => simple(a).zip(simple(b)).map(|(a, b)| Date::FromTo(a, b)),
                None => simple(rest).map(Date::From),
            },
            "TO" => simple(rest).map(Date::To),
            _ => simple(s).map(Date::Exact),
        };
        r.unwrap_or_else(other)
    }

    pub fn kind(&self) -> Option<DateKind> {
        Some(match self {
            Date::Exact(_) => DateKind::Exact,
            Date::About(_) => DateKind::About,
            Date::Calculated(_) => DateKind::Calculated,
            Date::Estimated(_) => DateKind::Estimated,
            Date::Before(_) => DateKind::Before,
            Date::After(_) => DateKind::After,
            Date::Between(..) => DateKind::Between,
            Date::From(_) => DateKind::From,
            Date::To(_) => DateKind::To,
            Date::FromTo(..) => DateKind::FromTo,
            Date::Other(_) => return None,
        })
    }

    /// The simple dates that make up the value.
    pub fn dates(&self) -> Vec<&SimpleDate> {
        match self {
            Date::Between(a, b) | Date::FromTo(a, b) => vec![a, b],
            Date::Other(_) => vec![],
            _ => vec![self.primary().unwrap()],
        }
    }

    /// Changes the precision, keeping the (first) date; a missing second date takes the first one.
    pub fn with_kind(&self, kind: DateKind) -> Option<Date> {
        let ds = self.dates();
        let a = (*ds.first()?).clone();
        let b = ds.get(1).map(|d| (*d).clone()).unwrap_or_else(|| a.clone());
        Some(match kind {
            DateKind::Exact => Date::Exact(a),
            DateKind::About => Date::About(a),
            DateKind::Calculated => Date::Calculated(a),
            DateKind::Estimated => Date::Estimated(a),
            DateKind::Before => Date::Before(a),
            DateKind::After => Date::After(a),
            DateKind::Between => Date::Between(a, b),
            DateKind::FromTo => Date::FromTo(a, b),
            DateKind::From => Date::From(a),
            DateKind::To => Date::To(a),
        })
    }

    /// Applies `f` to each simple date; `None` if one fails.
    pub fn try_map(&self, f: impl Fn(&SimpleDate) -> Option<SimpleDate>) -> Option<Date> {
        Some(match self {
            Date::Exact(d) => Date::Exact(f(d)?),
            Date::About(d) => Date::About(f(d)?),
            Date::Calculated(d) => Date::Calculated(f(d)?),
            Date::Estimated(d) => Date::Estimated(f(d)?),
            Date::Before(d) => Date::Before(f(d)?),
            Date::After(d) => Date::After(f(d)?),
            Date::From(d) => Date::From(f(d)?),
            Date::To(d) => Date::To(f(d)?),
            Date::Between(a, b) => Date::Between(f(a)?, f(b)?),
            Date::FromTo(a, b) => Date::FromTo(f(a)?, f(b)?),
            Date::Other(_) => return None,
        })
    }

    /// Calendar shared by every simple date, if there is one.
    pub fn calendar(&self) -> Option<&Calendar> {
        let ds = self.dates();
        let c = &ds.first()?.calendar;
        ds.iter().all(|d| &d.calendar == c).then_some(c)
    }

    /// GEDCOM 7 value. Every calendar is named as soon as one date is not Gregorian (the standard's
    /// recommendation).
    pub fn to_gedcom(&self) -> String {
        let named = self.dates().iter().any(|d| d.calendar != Calendar::Gregorian);
        let s = |d: &SimpleDate| d.to_gedcom(named);
        match self {
            Date::Exact(d) => s(d),
            Date::About(d) => format!("ABT {}", s(d)),
            Date::Calculated(d) => format!("CAL {}", s(d)),
            Date::Estimated(d) => format!("EST {}", s(d)),
            Date::Before(d) => format!("BEF {}", s(d)),
            Date::After(d) => format!("AFT {}", s(d)),
            Date::Between(a, b) => format!("BET {} AND {}", s(a), s(b)),
            Date::From(d) => format!("FROM {}", s(d)),
            Date::To(d) => format!("TO {}", s(d)),
            Date::FromTo(a, b) => format!("FROM {} TO {}", s(a), s(b)),
            Date::Other(x) => x.clone(),
        }
    }

    /// Checks that each date exists and that the bounds are in order.
    pub fn validate(&self) -> Result<(), String> {
        for d in self.dates() {
            d.validate()?;
        }
        if let Date::Between(a, b) | Date::FromTo(a, b) = self
            && let (Some((lo, _)), Some((_, hi))) = (a.rd_range(), b.rd_range())
            && lo > hi
        {
            return Err(t!("date.error.order").into());
        }
        Ok(())
    }

    /// Main date (the first bound for a period).
    pub fn primary(&self) -> Option<&SimpleDate> {
        match self {
            Date::Exact(d)
            | Date::About(d)
            | Date::Calculated(d)
            | Date::Estimated(d)
            | Date::Before(d)
            | Date::After(d)
            | Date::From(d)
            | Date::To(d)
            | Date::Between(d, _)
            | Date::FromTo(d, _) => Some(d),
            Date::Other(_) => None,
        }
    }

    pub fn year(&self) -> Option<i32> {
        self.primary().and_then(|d| d.gregorian_year())
    }

    /// Latest Gregorian year the date can be: the second date of a period, a few years after an
    /// approximate date, none for an open date (“after 1850”).
    pub fn latest_year(&self) -> Option<i32> {
        match self {
            Date::Exact(d) | Date::Before(d) | Date::To(d) => d.gregorian_year(),
            Date::About(d) | Date::Calculated(d) | Date::Estimated(d) => d.gregorian_year().map(|y| y + 10),
            Date::Between(_, d) | Date::FromTo(_, d) => d.gregorian_year(),
            Date::After(_) | Date::From(_) | Date::Other(_) => None,
        }
    }

    pub fn sort_key(&self) -> (i32, u8, u8) {
        self.primary().map(|d| d.sort_key()).unwrap_or((i32::MIN, 0, 0))
    }

    /// A short year for lists: `1850`, `~1850`, `<1809`.
    pub fn short_year(&self) -> String {
        let y = match self.year() {
            Some(y) => y.to_string(),
            None => return "?".into(),
        };
        match self {
            Date::About(_) | Date::Calculated(_) | Date::Estimated(_) | Date::Between(..) => format!("~{y}"),
            Date::Before(_) => format!("<{y}"),
            Date::After(_) => format!(">{y}"),
            _ => y,
        }
    }

    fn render(&self, lang: &str, f: impl Fn(&SimpleDate) -> String) -> String {
        // French changes the preposition with a day: “du 10 mai au 12 juin”, “de 1850 à 1860”.
        let until = |d: &SimpleDate| match d.day {
            Some(_) => t!("date.until_day", d = f(d), locale = lang),
            None => t!("date.until_year", d = f(d), locale = lang),
        };
        let from = |d: &SimpleDate| match d.day {
            Some(_) => t!("date.from_day", d = f(d), locale = lang),
            None => t!("date.from_year", d = f(d), locale = lang),
        };
        let to = |d: &SimpleDate| match d.day {
            Some(_) => t!("date.to_day", d = f(d), locale = lang),
            None => t!("date.to_year", d = f(d), locale = lang),
        };
        match self {
            Date::Exact(d) => f(d),
            Date::About(d) => t!("date.about", d = f(d), locale = lang).into_owned(),
            Date::Calculated(d) => t!("date.calculated", d = f(d), locale = lang).into_owned(),
            Date::Estimated(d) => t!("date.estimated", d = f(d), locale = lang).into_owned(),
            Date::Before(d) => t!("date.before", d = f(d), locale = lang).into_owned(),
            Date::After(d) => t!("date.after", d = f(d), locale = lang).into_owned(),
            Date::Between(a, b) => t!("date.between", a = f(a), b = f(b), locale = lang).into_owned(),
            Date::From(d) => t!("date.since", d = f(d), locale = lang).into_owned(),
            Date::To(d) => until(d).into_owned(),
            Date::FromTo(a, b) => t!("date.from_to", from = from(a), to = to(b), locale = lang).into_owned(),
            Date::Other(s) => s.clone(),
        }
    }

    /// The date in the interface language, with the Gregorian equivalent of other calendars.
    pub fn display(&self) -> String {
        self.display_in(&crate::i18n::language())
    }

    fn display_in(&self, lang: &str) -> String {
        self.render(lang, |d| d.display_in(lang))
    }

    /// Text for the date field, read back as is by [`parse_input`].
    pub fn to_input_text(&self) -> String {
        self.to_input_text_in(&crate::i18n::language())
    }

    fn to_input_text_in(&self, lang: &str) -> String {
        if self.dates().iter().any(|d| matches!(d.calendar, Calendar::Extension(_)) || d.month_tag.is_some()) {
            return self.to_gedcom();
        }
        let ds = self.dates();
        let mixed = ds.iter().any(|d| d.calendar != ds[0].calendar);
        self.render(lang, |d| {
            let text = d.core_text_in(lang);
            if mixed && d.calendar == Calendar::Gregorian {
                format!("{text} {}", t!("date.gregorian", locale = lang))
            } else {
                text
            }
        })
    }
}

// ---------------------------------------------------------------------------
// Reading and writing `DATE` + `PHRASE` in the tree.
// ---------------------------------------------------------------------------

/// Value and phrase of a node's `DATE` substructure.
pub fn read_date(node: &Node) -> (String, String) {
    match node.child("DATE") {
        Some(d) => (d.val().to_string(), d.child_val("PHRASE").unwrap_or("").to_string()),
        None => (String::new(), String::new()),
    }
}

/// New `DATE` node (with `PHRASE` if any); `None` if both are empty.
pub fn date_node(value: &str, phrase: &str) -> Option<Node> {
    let (value, phrase) = (value.trim(), phrase.trim());
    if value.is_empty() && phrase.is_empty() {
        return None;
    }
    let mut n = if value.is_empty() { Node::new("DATE") } else { Node::with_value("DATE", value) };
    if !phrase.is_empty() {
        n.children.push(Node::with_value("PHRASE", phrase));
    }
    Some(n)
}

/// Writes `DATE` + `PHRASE` under `parent`, keeping the other substructures (`TIME`…).
pub fn write_date(parent: &mut Node, value: &str, phrase: &str) {
    let (value, phrase) = (value.trim(), phrase.trim());
    if value.is_empty() && phrase.is_empty() {
        parent.children.retain(|c| c.tag != "DATE");
        return;
    }
    let d = parent.ensure_child("DATE");
    d.value = (!value.is_empty()).then(|| value.to_string());
    d.set_child_val("PHRASE", phrase);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn english() {
        let en = |raw: &str| Date::parse(raw).display_in("en");
        assert_eq!(en("19 SEP 2000"), "19 Sep 2000");
        assert_eq!(en("1 JAN 1900"), "1 Jan 1900");
        assert_eq!(en("ABT 1850"), "about 1850");
        assert_eq!(en("FROM 10 MAY 1926 TO 10 NOV 1926"), "from 10 May 1926 to 10 Nov 1926");
        assert_eq!(en("TO 1860"), "until 1860");
        assert_eq!(en("BET 1850 AND 1860"), "between 1850 and 1860");
        assert_eq!(en("FRENCH_R 30 GERM 4"), "30 Germinal year IV (19 Apr 1796)");
        assert_eq!(en("JULIAN 3 MAR 1700"), "3 Mar 1700 Julian (14 Mar 1700)");
        assert_eq!(en("HEBREW 14 ADS 5785"), "14 Adar 5785 Hebrew (14 Mar 2025)");
        assert_eq!(en("44 BCE"), "44 BC");
    }

    #[test]
    fn exact_and_qualified() {
        assert_eq!(Date::parse("19 SEP 2000").display_in("fr"), "19 sept. 2000");
        assert_eq!(Date::parse("1 JAN 1900").display_in("fr"), "1er janv. 1900");
        assert_eq!(Date::parse("ABT 1850").short_year(), "~1850");
        assert_eq!(Date::parse("BEF 23 MAY 1809").display_in("fr"), "avant 23 mai 1809");
        assert_eq!(Date::parse("FROM 10 MAY 1926 TO 10 NOV 1926").year(), Some(1926));
        assert_eq!(Date::parse("FROM 10 MAY 1926 TO 10 NOV 1926").display_in("fr"), "du 10 mai 1926 au 10 nov. 1926");
        assert_eq!(Date::parse("FROM 1850 TO 1860").display_in("fr"), "de 1850 à 1860");
        assert_eq!(Date::parse("TO 1860").display_in("fr"), "jusqu'en 1860");
        assert_eq!(Date::parse("BET 1850 AND 1860").display_in("fr"), "entre 1850 et 1860");
    }

    #[test]
    fn french_republican() {
        let d = Date::parse("FRENCH_R 30 GERM 4");
        assert_eq!(d.year(), Some(1796));
        assert_eq!(d.display_in("fr"), "30 germinal an IV (19 avr. 1796)");
        assert_eq!(Date::parse("FRENCH_R BRUM 12").display_in("fr"), "brumaire an XII (oct.–nov. 1803)");
        assert_eq!(Date::parse("FRENCH_R 12").display_in("fr"), "an XII (1803–1804)");
        let d = Date::parse("BEF FRENCH_R 17 FLOR 13");
        assert_eq!(d.year(), Some(1805));
    }

    #[test]
    fn julian_and_hebrew() {
        assert_eq!(Date::parse("JULIAN 3 MAR 1700").display_in("fr"), "3 mars 1700 julien (14 mars 1700)");
        assert_eq!(Date::parse("HEBREW 15 NSN 5784").display_in("fr"), "15 nissan 5784 hébraïque (23 avr. 2024)");
        assert_eq!(Date::parse("HEBREW 14 ADS 5785").to_input_text_in("fr"), "14 adar 5785 hébraïque");
        assert_eq!(Date::parse("44 BCE").display_in("fr"), "44 av. J.-C.");
        assert_eq!(Date::parse("@#DJULIAN@ 1650").to_gedcom(), "JULIAN 1650");
    }

    #[test]
    fn conversions() {
        let g = SimpleDate::ymd(1803, 11, 7);
        let r = g.convert(&Calendar::FrenchRepublican).unwrap();
        assert_eq!(r.to_gedcom(false), "FRENCH_R 15 BRUM 12");
        assert_eq!(r.convert(&Calendar::Gregorian).unwrap(), g);
        assert_eq!(g.convert(&Calendar::Julian).unwrap().to_gedcom(false), "JULIAN 26 OCT 1803");
        // Before year I: no Republican equivalent.
        assert!(SimpleDate::ymd(1700, 1, 1).convert(&Calendar::FrenchRepublican).is_none());
    }

    #[test]
    fn gedcom_output() {
        for s in [
            "19 SEP 2000",
            "ABT 1850",
            "BET 1850 AND 1860",
            "FROM GREGORIAN 1670 TO JULIAN 1800",
            "FRENCH_R 15 BRUM 12",
            "_MAYA 5 _XUL 1000",
            "JAN 1700 BCE",
            "TO 1900",
        ] {
            assert_eq!(Date::parse(s).to_gedcom(), s);
        }
        assert_eq!(Date::parse("FROM 1670 TO JULIAN 1800").to_gedcom(), "FROM GREGORIAN 1670 TO JULIAN 1800");
    }

    #[test]
    fn validation() {
        assert!(Date::parse("29 FEB 1900").validate().is_err());
        assert!(Date::parse("29 FEB 1600").validate().is_ok());
        assert!(Date::parse("JULIAN 29 FEB 1700").validate().is_ok());
        assert!(Date::parse("FRENCH_R 6 COMP 3").validate().is_ok());
        assert!(Date::parse("FRENCH_R 6 COMP 4").validate().is_err());
        assert!(Date::parse("BET 1860 AND 1850").validate().is_err());
        assert!(Date::parse("FRENCH_R 1 VEND 2 BCE").validate().is_err());
    }

    #[test]
    fn unknown_is_kept() {
        assert_eq!(Date::parse("pendant la guerre"), Date::Other("pendant la guerre".into()));
        assert_eq!(Date::parse("30 JAN 1648/49").to_gedcom(), "30 JAN 1648/49");
    }

    #[test]
    fn date_nodes() {
        let mut ev = Node::new("BIRT");
        write_date(&mut ev, "", "5 janvier (année inconnue)");
        assert_eq!(read_date(&ev), ("".into(), "5 janvier (année inconnue)".into()));
        write_date(&mut ev, "ABT 1850", "");
        assert_eq!(read_date(&ev), ("ABT 1850".into(), "".into()));
        write_date(&mut ev, "", "");
        assert!(ev.child("DATE").is_none());
    }
}
