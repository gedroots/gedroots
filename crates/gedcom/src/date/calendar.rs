//! Exact conversions between calendars, through a fixed day number (“rata die”: 1 = 1 January of
//! year 1, proleptic Gregorian).
//!
//! “Astronomical” years: 1 BC = 0, 2 BC = -1… Algorithms from Dershowitz & Reingold, *Calendrical
//! Calculations*.

/// Fixed day of 1 January 1970 (to convert to “Unix” days).
pub const UNIX_EPOCH_RD: i64 = 719_163;

fn div(a: i64, b: i64) -> i64 {
    a.div_euclid(b)
}

// --- Gregorian ---------------------------------------------------------------

pub fn gregorian_leap(y: i64) -> bool {
    y.rem_euclid(4) == 0 && (y.rem_euclid(100) != 0 || y.rem_euclid(400) == 0)
}

pub fn fixed_from_gregorian(y: i64, m: u32, d: u32) -> i64 {
    let m = m as i64;
    let adj = if m <= 2 {
        0
    } else if gregorian_leap(y) {
        -1
    } else {
        -2
    };
    365 * (y - 1) + div(y - 1, 4) - div(y - 1, 100) + div(y - 1, 400) + div(367 * m - 362, 12) + adj + d as i64
}

pub fn gregorian_from_fixed(rd: i64) -> (i64, u32, u32) {
    // H. Hinnant's algorithm on Unix days.
    let z = rd - UNIX_EPOCH_RD + 719_468;
    let era = div(z, 146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let y = yoe + era * 400 + if m <= 2 { 1 } else { 0 };
    (y, m, d)
}

// --- Julien ------------------------------------------------------------------

pub fn julian_leap(y: i64) -> bool {
    y.rem_euclid(4) == 0
}

const JULIAN_EPOCH: i64 = -1;

pub fn fixed_from_julian(y: i64, m: u32, d: u32) -> i64 {
    let m = m as i64;
    let adj = if m <= 2 {
        0
    } else if julian_leap(y) {
        -1
    } else {
        -2
    };
    JULIAN_EPOCH - 1 + 365 * (y - 1) + div(y - 1, 4) + div(367 * m - 362, 12) + adj + d as i64
}

pub fn julian_from_fixed(rd: i64) -> (i64, u32, u32) {
    let y = div(4 * (rd - JULIAN_EPOCH) + 1464, 1461);
    let prior = rd - fixed_from_julian(y, 1, 1);
    let corr = if rd < fixed_from_julian(y, 3, 1) {
        0
    } else if julian_leap(y) {
        1
    } else {
        2
    };
    let m = div(12 * (prior + corr) + 373, 367) as u32;
    let d = (rd - fixed_from_julian(y, m, 1) + 1) as u32;
    (y, m, d)
}

/// Number of days in a Gregorian month (`julian`: Julian leap year rule).
pub fn western_month_days(y: i64, m: u32, julian: bool) -> u32 {
    match m {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        _ if (julian && julian_leap(y)) || (!julian && gregorian_leap(y)) => 29,
        _ => 28,
    }
}

// --- French Republican -------------------------------------------------------

/// 1 Vendémiaire year I = 22 September 1792.
const FRENCH_EPOCH: i64 = 654_415;

/// Leap (sextile) years: III, VII, XI, XV… (arithmetic rule of the usual converters).
pub fn french_leap(y: i64) -> bool {
    y.rem_euclid(4) == 3
}

pub fn french_new_year(y: i64) -> i64 {
    FRENCH_EPOCH + 365 * (y - 1) + div(y, 4)
}

/// 12 months of 30 days, then 5 (or 6) complementary days.
pub fn french_month_days(y: i64, m: u32) -> u32 {
    if m == 13 { if french_leap(y) { 6 } else { 5 } } else { 30 }
}

pub fn fixed_from_french(y: i64, m: u32, d: u32) -> i64 {
    french_new_year(y) + 30 * (m as i64 - 1) + d as i64 - 1
}

pub fn french_from_fixed(rd: i64) -> (i64, u32, u32) {
    let mut y = div((rd - FRENCH_EPOCH) * 4, 1461) + 1;
    while french_new_year(y + 1) <= rd {
        y += 1;
    }
    while french_new_year(y) > rd {
        y -= 1;
    }
    let doy = rd - french_new_year(y);
    ((y), (doy / 30 + 1) as u32, (doy % 30 + 1) as u32)
}

// --- Hebrew ------------------------------------------------------------------

/// 1er tichri an 1 = 7 octobre 3761 av. J.-C. (julien).
const HEBREW_EPOCH: i64 = -1_373_427;

pub fn hebrew_leap(y: i64) -> bool {
    (7 * y + 1).rem_euclid(19) < 7
}

fn hebrew_elapsed_days(y: i64) -> i64 {
    let months = div(235 * y - 234, 19);
    let parts = 12_084 + 13_753 * months;
    let days = 29 * months + div(parts, 25_920);
    if (3 * (days + 1)).rem_euclid(7) < 3 { days + 1 } else { days }
}

fn hebrew_delay(y: i64) -> i64 {
    let (ny0, ny1, ny2) = (hebrew_elapsed_days(y - 1), hebrew_elapsed_days(y), hebrew_elapsed_days(y + 1));
    if ny2 - ny1 == 356 {
        2
    } else if ny1 - ny0 == 382 {
        1
    } else {
        0
    }
}

pub fn hebrew_new_year(y: i64) -> i64 {
    HEBREW_EPOCH + hebrew_elapsed_days(y) + hebrew_delay(y)
}

fn hebrew_year_days(y: i64) -> i64 {
    hebrew_new_year(y + 1) - hebrew_new_year(y)
}

/// Months in GEDCOM order (1 = Tishrei … 6 = Adar I, 7 = Adar / Adar II … 13 = Elul). In a common
/// year, Adar I does not exist: `ADR` is read as Adar (`ADS`).
pub fn hebrew_month_days(y: i64, m: u32) -> u32 {
    let len = hebrew_year_days(y);
    match m {
        1 | 5 | 8 | 10 | 12 => 30,
        2 => {
            if len % 10 == 5 {
                30
            } else {
                29
            }
        }
        3 => {
            if len % 10 == 3 {
                29
            } else {
                30
            }
        }
        6 if hebrew_leap(y) => 30,
        _ => 29,
    }
}

/// Months actually present in the year, in order.
fn hebrew_months(y: i64) -> impl Iterator<Item = u32> {
    let leap = hebrew_leap(y);
    (1..=13).filter(move |&m| m != 6 || leap)
}

pub fn fixed_from_hebrew(y: i64, m: u32, d: u32) -> i64 {
    let m = if m == 6 && !hebrew_leap(y) { 7 } else { m };
    let before: i64 = hebrew_months(y).take_while(|&x| x < m).map(|x| hebrew_month_days(y, x) as i64).sum();
    hebrew_new_year(y) + before + d as i64 - 1
}

pub fn hebrew_from_fixed(rd: i64) -> (i64, u32, u32) {
    let mut y = div((rd - HEBREW_EPOCH) * 98_496, 35_975_351) + 1;
    while hebrew_new_year(y + 1) <= rd {
        y += 1;
    }
    while hebrew_new_year(y) > rd {
        y -= 1;
    }
    let mut left = rd - hebrew_new_year(y);
    for m in hebrew_months(y) {
        let n = hebrew_month_days(y, m) as i64;
        if left < n {
            return (y, m, left as u32 + 1);
        }
        left -= n;
    }
    unreachable!("day outside the Hebrew year")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gregorian_roundtrip() {
        assert_eq!(fixed_from_gregorian(1, 1, 1), 1);
        assert_eq!(fixed_from_gregorian(1970, 1, 1), UNIX_EPOCH_RD);
        for rd in [-800_000, -1, 0, 1, 600_000, 730_000, 740_000] {
            let (y, m, d) = gregorian_from_fixed(rd);
            assert_eq!(fixed_from_gregorian(y, m, d), rd);
        }
    }

    #[test]
    fn julian_known() {
        // Gregorian reform: Thursday 4 Oct. 1582 (Julian) followed by Friday 15 Oct. 1582.
        assert_eq!(fixed_from_julian(1582, 10, 4) + 1, fixed_from_gregorian(1582, 10, 15));
        assert_eq!(julian_from_fixed(fixed_from_gregorian(1700, 3, 14)), (1700, 3, 3));
        for rd in [-500_000, 0, 1, 577_000, 700_000] {
            let (y, m, d) = julian_from_fixed(rd);
            assert_eq!(fixed_from_julian(y, m, d), rd);
        }
    }

    #[test]
    fn french_known() {
        assert_eq!(fixed_from_french(1, 1, 1), fixed_from_gregorian(1792, 9, 22));
        // 18 brumaire an VIII = 9 novembre 1799.
        assert_eq!(gregorian_from_fixed(fixed_from_french(8, 2, 18)), (1799, 11, 9));
        // 15 brumaire an XII = 7 novembre 1803 ; 30 germinal an IV = 19 avril 1796.
        assert_eq!(gregorian_from_fixed(fixed_from_french(12, 2, 15)), (1803, 11, 7));
        assert_eq!(gregorian_from_fixed(fixed_from_french(4, 7, 30)), (1796, 4, 19));
        // Last day of the calendar: 10 Nivôse year XIV = 31 December 1805.
        assert_eq!(gregorian_from_fixed(fixed_from_french(14, 4, 10)), (1805, 12, 31));
        assert_eq!(french_month_days(3, 13), 6);
        let rd = fixed_from_french(11, 13, 6);
        assert_eq!(french_from_fixed(rd), (11, 13, 6));
        assert_eq!(french_from_fixed(rd + 1), (12, 1, 1));
    }

    #[test]
    fn hebrew_known() {
        // Roch Hachana 5785 = 3 octobre 2024 ; Pessah 5784 (15 nissan) = 23 avril 2024.
        assert_eq!(gregorian_from_fixed(fixed_from_hebrew(5785, 1, 1)), (2024, 10, 3));
        assert_eq!(gregorian_from_fixed(fixed_from_hebrew(5784, 8, 15)), (2024, 4, 23));
        assert!(hebrew_leap(5784) && !hebrew_leap(5785));
        // Purim 5785 (14 Adar, common year) = 14 March 2025, even written ADR.
        assert_eq!(gregorian_from_fixed(fixed_from_hebrew(5785, 7, 14)), (2025, 3, 14));
        assert_eq!(fixed_from_hebrew(5785, 6, 14), fixed_from_hebrew(5785, 7, 14));
        for rd in [-1_373_427, 0, 650_000, 739_000, 739_500] {
            let (y, m, d) = hebrew_from_fixed(rd);
            assert_eq!(fixed_from_hebrew(y, m, d), rd);
        }
    }
}
