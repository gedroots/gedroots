//! Dates GEDCOM 7 : analyse tolérante et affichage en français.
//!
//! On reconnaît les formes courantes (`19 SEP 2000`, `ABT 1850`, `BEF 1809`,
//! `FROM … TO …`, `BET … AND …`, calendrier républicain `FRENCH_R`…) pour
//! l'affichage et le tri. La valeur d'origine n'est jamais réécrite tant que
//! l'utilisateur ne la modifie pas.

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Calendar {
    #[default]
    Gregorian,
    Julian,
    FrenchRepublican,
    Hebrew,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SimpleDate {
    pub calendar: Calendar,
    pub day: Option<u8>,
    /// Mois 1..=12 (1..=13 pour le républicain).
    pub month: Option<u8>,
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
    /// Valeur non reconnue, conservée telle quelle.
    Other(String),
}

const GREG_MONTHS: [&str; 12] = ["JAN", "FEB", "MAR", "APR", "MAY", "JUN", "JUL", "AUG", "SEP", "OCT", "NOV", "DEC"];
const FR_MONTHS: [&str; 12] =
    ["janv.", "févr.", "mars", "avr.", "mai", "juin", "juil.", "août", "sept.", "oct.", "nov.", "déc."];
const REP_MONTHS: [&str; 13] =
    ["VEND", "BRUM", "FRIM", "NIVO", "PLUV", "VENT", "GERM", "FLOR", "PRAI", "MESS", "THER", "FRUC", "COMP"];
const REP_NAMES: [&str; 13] = [
    "vendémiaire",
    "brumaire",
    "frimaire",
    "nivôse",
    "pluviôse",
    "ventôse",
    "germinal",
    "floréal",
    "prairial",
    "messidor",
    "thermidor",
    "fructidor",
    "jours complémentaires",
];
const HEB_MONTHS: [&str; 13] =
    ["TSH", "CSH", "KSL", "TVT", "SHV", "ADR", "ADS", "NSN", "IYR", "SVN", "TMZ", "AAV", "ELL"];

impl SimpleDate {
    fn parse(s: &str) -> Option<SimpleDate> {
        let mut toks: Vec<&str> = s.split_whitespace().collect();
        let mut d = SimpleDate::default();
        if let Some(first) = toks.first() {
            let cal = match *first {
                "GREGORIAN" => Some(Calendar::Gregorian),
                "JULIAN" => Some(Calendar::Julian),
                "FRENCH_R" => Some(Calendar::FrenchRepublican),
                "HEBREW" => Some(Calendar::Hebrew),
                _ => None,
            };
            if let Some(c) = cal {
                d.calendar = c;
                toks.remove(0);
            }
        }
        if toks.last() == Some(&"BCE") {
            d.bce = true;
            toks.pop();
        }
        let months: &[&str] = match d.calendar {
            Calendar::FrenchRepublican => &REP_MONTHS,
            Calendar::Hebrew => &HEB_MONTHS,
            _ => &GREG_MONTHS,
        };
        match toks.as_slice() {
            [y] => d.year = Some(y.parse().ok()?),
            [m, y] => {
                d.month = Some(months.iter().position(|x| x == m)? as u8 + 1);
                d.year = Some(y.parse().ok()?);
            }
            [day, m, y] => {
                d.day = Some(day.parse().ok()?);
                d.month = Some(months.iter().position(|x| x == m)? as u8 + 1);
                d.year = Some(y.parse().ok()?);
            }
            _ => return None,
        }
        Some(d)
    }

    /// Année grégorienne approximative (pour le tri et la chronologie).
    pub fn gregorian_year(&self) -> Option<i32> {
        let y = self.year?;
        let y = match self.calendar {
            // An I = 22 sept. 1792 ; on se cale sur l'année civile majoritaire.
            Calendar::FrenchRepublican => {
                let m = self.month.unwrap_or(1);
                if m <= 4 { 1791 + y } else { 1792 + y }
            }
            Calendar::Hebrew => y - 3760,
            _ => y,
        };
        Some(if self.bce { -y } else { y })
    }

    /// Clé de tri (année, mois, jour) approximative.
    pub fn sort_key(&self) -> (i32, u8, u8) {
        (self.gregorian_year().unwrap_or(i32::MIN), self.month.unwrap_or(0), self.day.unwrap_or(0))
    }

    pub fn to_french(&self) -> String {
        let mut s = String::new();
        if let Some(day) = self.day {
            s.push_str(&if day == 1 { "1er".to_string() } else { day.to_string() });
            s.push(' ');
        }
        if let Some(m) = self.month {
            let name = match self.calendar {
                Calendar::FrenchRepublican => REP_NAMES.get(m as usize - 1).copied().unwrap_or("?"),
                Calendar::Hebrew => HEB_MONTHS.get(m as usize - 1).copied().unwrap_or("?"),
                _ => FR_MONTHS.get(m as usize - 1).copied().unwrap_or("?"),
            };
            s.push_str(name);
            s.push(' ');
        }
        if let Some(y) = self.year {
            match self.calendar {
                Calendar::FrenchRepublican => s.push_str(&format!("an {}", roman(y))),
                _ => s.push_str(&y.to_string()),
            }
        }
        if self.bce {
            s.push_str(" av. J.-C.");
        }
        if self.calendar == Calendar::FrenchRepublican
            && let Some(g) = self.gregorian_year()
        {
            s.push_str(&format!(" (≈ {g})"));
        }
        s.trim().to_string()
    }
}

fn roman(mut n: i32) -> String {
    if n <= 0 || n > 3999 {
        return n.to_string();
    }
    let table = [(10, "X"), (9, "IX"), (5, "V"), (4, "IV"), (1, "I")];
    let mut s = String::new();
    while n >= 40 {
        s.push_str("XL");
        n -= 40;
    }
    for (v, r) in table {
        while n >= v {
            s.push_str(r);
            n -= v;
        }
    }
    s
}

impl Date {
    pub fn parse(raw: &str) -> Date {
        let s = raw.trim();
        let other = || Date::Other(s.to_string());
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

    /// Date principale (la première borne pour une période).
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

    pub fn sort_key(&self) -> (i32, u8, u8) {
        self.primary().map(|d| d.sort_key()).unwrap_or((i32::MIN, 0, 0))
    }

    /// Une année courte pour les listes : `1850`, `~1850`, `<1809`.
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

    pub fn to_french(&self) -> String {
        match self {
            Date::Exact(d) => d.to_french(),
            Date::About(d) => format!("vers {}", d.to_french()),
            Date::Calculated(d) => format!("calculée {}", d.to_french()),
            Date::Estimated(d) => format!("estimée {}", d.to_french()),
            Date::Before(d) => format!("avant {}", d.to_french()),
            Date::After(d) => format!("après {}", d.to_french()),
            Date::Between(a, b) => format!("entre {} et {}", a.to_french(), b.to_french()),
            Date::From(d) => format!("depuis {}", d.to_french()),
            Date::To(d) => format!("jusqu'à {}", d.to_french()),
            Date::FromTo(a, b) => format!("du {} au {}", a.to_french(), b.to_french()),
            Date::Other(s) => s.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_and_qualified() {
        assert_eq!(Date::parse("19 SEP 2000").to_french(), "19 sept. 2000");
        assert_eq!(Date::parse("1 JAN 1900").to_french(), "1er janv. 1900");
        assert_eq!(Date::parse("ABT 1850").short_year(), "~1850");
        assert_eq!(Date::parse("BEF 23 MAY 1809").to_french(), "avant 23 mai 1809");
        assert_eq!(Date::parse("FROM 10 MAY 1926 TO 10 NOV 1926").year(), Some(1926));
        assert_eq!(Date::parse("BET 1850 AND 1860").to_french(), "entre 1850 et 1860");
    }

    #[test]
    fn french_republican() {
        let d = Date::parse("FRENCH_R 30 GERM 4");
        assert_eq!(d.year(), Some(1796));
        assert_eq!(d.to_french(), "30 germinal an IV (≈ 1796)");
        let d = Date::parse("BEF FRENCH_R 17 FLOR 13");
        assert_eq!(d.year(), Some(1805));
    }

    #[test]
    fn unknown_is_kept() {
        assert_eq!(Date::parse("pendant la guerre"), Date::Other("pendant la guerre".into()));
    }
}
