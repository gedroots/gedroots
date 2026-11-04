//! Entering a date in everyday language, in French or in English whatever the interface language.
//!
//! One field, as in Gramps or RootsMagic: type “vers 1850”, “14/03/1912”, “entre 1820 et 1825”, “du
//! 10 mai au 12 juin 1926”, “15 brumaire an XII”, “3 mars 1700 julien”, “44 av. J.-C.”, or in
//! English “about 1850”, “from 10 May to 12 Jun 1926”, “15 Brumaire year XII”, “44 BC”… Numbers are
//! read day first (14/03/1912). GEDCOM keywords (`ABT 1850`, `FRENCH_R 15 BRUM 12`) are understood
//! too.
//!
//! Text in parentheses at the end of the input becomes the date's `PHRASE` (“about 1850 (from the
//! age at death)”); a text without a recognizable date is kept as a phrase alone, which GEDCOM 7
//! allows.

use super::{Calendar, Date, SimpleDate};
use crate::model::sort_key;

/// Result of an input: a GEDCOM date and/or a phrase.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Parsed {
    pub date: Option<Date>,
    pub phrase: String,
}

impl Parsed {
    /// GEDCOM value of `DATE` (empty for a phrase alone).
    pub fn value(&self) -> String {
        self.date.as_ref().map(Date::to_gedcom).unwrap_or_default()
    }

    pub fn is_empty(&self) -> bool {
        self.date.is_none() && self.phrase.is_empty()
    }

    /// Equivalent input text (date in words, phrase in parentheses).
    pub fn to_input_text(&self) -> String {
        match &self.date {
            Some(d) if self.phrase.is_empty() => d.to_input_text(),
            Some(d) => format!("{} ({})", d.to_input_text(), self.phrase),
            None if self.phrase.is_empty() => String::new(),
            // A phrase that would look like a date is put in quotes.
            None if parse_input(&self.phrase).is_ok_and(|p| p.date.is_some()) => {
                t!("date.quoted", phrase = self.phrase).into_owned()
            }
            None => self.phrase.clone(),
        }
    }

    /// Reads the stored value (`DATE` + `PHRASE`).
    pub fn from_gedcom(value: &str, phrase: &str) -> Parsed {
        let value = value.trim();
        Parsed { date: (!value.is_empty()).then(|| Date::parse(value)), phrase: phrase.trim().to_string() }
    }
}

enum Fail {
    /// Not a date: the text will be a phrase.
    Unknown,
    /// A date, but impossible (30 February…).
    Invalid(String),
}

type R<T> = Result<T, Fail>;

/// Interprets an input. `Err`: date recognized but impossible (translated message).
pub fn parse_input(text: &str) -> Result<Parsed, String> {
    let t = text.trim();
    if t.is_empty() {
        return Ok(Parsed::default());
    }
    // Entre guillemets : phrase seule, volontairement.
    for (a, b) in [("«", "»"), ("\"", "\""), ("“", "”")] {
        if t.len() > a.len() + b.len() && t.starts_with(a) && t.ends_with(b) {
            let inner = t[a.len()..t.len() - b.len()].trim();
            return Ok(Parsed { date: None, phrase: inner.to_string() });
        }
    }
    // Raw GEDCOM value (calendars and extension months included).
    let raw = Date::parse(t);
    if !matches!(raw, Date::Other(_)) {
        raw.validate()?;
        return Ok(Parsed { date: Some(raw), phrase: String::new() });
    }
    let (body, mut phrase) = split_phrase(t);
    match parse_natural(&body) {
        Ok((date, auto_phrase)) => {
            if phrase.is_empty() && auto_phrase {
                phrase = t.to_string();
            }
            Ok(Parsed { date: Some(date), phrase })
        }
        Err(Fail::Invalid(e)) => Err(e),
        Err(Fail::Unknown) => Ok(Parsed { date: None, phrase: t.to_string() }),
    }
}

/// Splits off a final phrase in parentheses, unless it is a calendar (“(julien)”).
fn split_phrase(t: &str) -> (String, String) {
    if t.ends_with(')')
        && let Some(open) = t.rfind('(')
        && open > 0
    {
        let inner = &t[open + 1..t.len() - 1];
        if marker(&normalize_word(inner.trim())).is_none() {
            return (t[..open].trim().to_string(), inner.trim().to_string());
        }
    }
    (t.to_string(), String::new())
}

fn normalize_word(w: &str) -> String {
    sort_key(w).chars().filter(|c| !matches!(c, '.' | '-' | ' ')).collect()
}

/// Calendar and era words, reduced to a marker.
fn marker(w: &str) -> Option<&'static str> {
    Some(match w {
        "julien" | "calendrierjulien" | "vieuxstyle" | "julian" | "oldstyle" | "os" => "§jul",
        "gregorien" | "calendriergregorien" | "nouveaustyle" | "gregorian" | "newstyle" | "ns" => "§greg",
        "republicain" | "revolutionnaire" | "calendrierrepublicain" | "republican" | "frenchrepublican" => "§rep",
        "hebreu" | "hebraique" | "juif" | "calendrierhebraique" | "hebrew" | "jewish" => "§heb",
        _ => return None,
    })
}

fn tokenize(s: &str) -> Vec<String> {
    let s = sort_key(s).replace(['’', 'ʼ'], "'").replace(['–', '—'], "-").replace(['(', ')', ','], " ");
    let mut out: Vec<String> = Vec::new();
    for raw in s.split_whitespace() {
        let mut w = raw;
        // Attached symbols: “<1850”, “~1850”, “1850?”.
        while let Some(c) = w.chars().next().filter(|c| matches!(c, '<' | '>' | '~')) {
            out.push(c.to_string());
            w = &w[1..];
        }
        let mut question = false;
        while w.ends_with('?') {
            question = true;
            w = &w[..w.len() - 1];
        }
        // Elisions: “jusqu'au” → “jusqu'” + “au”, “d'avril” → “d'” + “avril”.
        let mut rest = w;
        while let Some(i) = rest.find('\'') {
            out.push(rest[..=i].to_string());
            rest = &rest[i + 1..];
        }
        if !rest.is_empty() {
            let numeric = rest.chars().all(|c| c.is_ascii_digit() || matches!(c, '/' | '-' | '.'));
            if numeric {
                // “1820-1825”: a range of years.
                match rest.split_once('-') {
                    Some((a, b))
                        if a.len() >= 3
                            && !b.contains('-')
                            && a.bytes().all(|c| c.is_ascii_digit())
                            && b.bytes().all(|c| c.is_ascii_digit()) =>
                    {
                        out.extend([a.to_string(), "-".into(), b.to_string()]);
                    }
                    _ => out.push(rest.trim_end_matches('.').to_string()),
                }
            } else {
                let n = rest.replace(['.', '-'], "");
                if !n.is_empty() {
                    out.push(n);
                } else if rest.contains('-') {
                    out.push("-".into());
                }
            }
        }
        if question {
            out.push("?".into());
        }
    }
    // Eras and calendars over several words.
    let mut res: Vec<String> = Vec::new();
    let mut i = 0;
    while i < out.len() {
        let w = out[i].as_str();
        let next = out.get(i + 1).map(String::as_str);
        let next2 = out.get(i + 2).map(String::as_str);
        let av = matches!(w, "av" | "avant");
        let ap = matches!(w, "ap" | "apres");
        let christ = |x: Option<&str>| matches!(x, Some("jc" | "jesuschrist" | "j" | "jesus"));
        if (av && christ(next)) || (av && next == Some("notre") && next2 == Some("ere")) || (av && next == Some("ne")) {
            res.push("§bce".into());
            i += if next == Some("notre") { 3 } else { 2 };
            if out.get(i).is_some_and(|x| x == "c" || x == "christ") {
                i += 1;
            }
            continue;
        }
        if matches!(w, "bce" | "bc" | "avjc" | "aec" | "anejc") {
            res.push("§bce".into());
            i += 1;
            continue;
        }
        if ap && christ(next) {
            i += 2;
            continue;
        }
        if matches!(w, "calendrier" | "calendar") || (w == "french" && next.is_some_and(|x| marker(x) == Some("§rep")))
        {
            i += 1;
            continue;
        }
        if let Some(m) = marker(w) {
            res.push(m.into());
            i += 1;
            continue;
        }
        if matches!(w, "vieux" | "old" | "nouveau" | "new") && next == Some("style") {
            res.push(if matches!(w, "vieux" | "old") { "§jul" } else { "§greg" }.into());
            i += 2;
            continue;
        }
        res.push(out[i].clone());
        i += 1;
    }
    res
}

#[derive(Clone, Copy, PartialEq)]
enum Q {
    Exact,
    About,
    Cal,
    Est,
    Bef,
    Aft,
    Bet,
    FromTo,
    From,
    To,
}

/// Leading word(s) → qualifier and number of words consumed.
fn qualifier(t: &[String]) -> Option<(Q, usize)> {
    let w = |i: usize| t.get(i).map(String::as_str);
    let after = |n: usize, extra: &[&str]| if w(n).is_some_and(|x| extra.contains(&x)) { n + 1 } else { n };
    Some(match w(0)? {
        "vers" | "v" | "env" | "environ" | "~" | "ca" | "circa" | "c" | "approx" | "approximativement" | "abt"
        | "about" | "approximately" => (Q::About, 1),
        "autour" => (Q::About, after(1, &["de", "du", "d'"])),
        "around" => (Q::About, 1),
        "calcule" | "calculee" | "calc" | "cal" | "calculated" => (Q::Cal, 1),
        "estime" | "estimee" | "est" | "estimation" | "estimated" => (Q::Est, 1),
        "avant" | "av" | "<" | "bef" | "before" | "anterieur" | "anterieurement" => {
            (Q::Bef, after(1, &["a", "au", "le", "l'"]))
        }
        "apres" | "ap" | ">" | "aft" | "after" | "posterieur" | "posterieurement" => {
            (Q::Aft, after(1, &["a", "au", "le", "l'"]))
        }
        "entre" | "bet" | "between" => (Q::Bet, after(1, &["le", "l'"])),
        "de" | "du" | "d'" | "from" => (Q::FromTo, 1),
        "depuis" | "des" | "since" => (Q::From, after(1, &["le", "l'"])),
        "a" if w(1) == Some("partir") => (Q::From, after(2, &["de", "du", "d'"])),
        "jusqu'" => (Q::To, after(1, &["a", "au", "en", "l'"])),
        "jusque" | "to" | "until" | "till" | "jusqua" | "jusquau" | "jusquen" => (Q::To, after(1, &["a", "au", "en"])),
        "le" | "en" | "l'" | "int" | "on" | "in" => (Q::Exact, 1),
        _ => return None,
    })
}

fn find(t: &[String], seps: &[&str]) -> Option<usize> {
    t.iter().enumerate().skip(1).find(|(_, x)| seps.contains(&x.as_str())).map(|(i, _)| i)
}

fn has_marker(t: &[String]) -> bool {
    t.iter().any(|x| x.starts_with('§') && x != "§bce")
}

fn implies_calendar(t: &[String]) -> bool {
    t.iter().any(|x| x == "an" || month(x).is_some_and(|(c, _)| c != Calendar::Gregorian))
}

/// Two dates: the final calendar or era marker applies to both.
fn pair(a: &[String], b: &[String]) -> R<(SimpleDate, SimpleDate)> {
    let mut a = a.to_vec();
    if !has_marker(&a) && !implies_calendar(&a) {
        a.extend(b.iter().filter(|x| x.starts_with('§') && *x != "§bce").cloned());
    }
    if b.iter().any(|x| x == "§bce") && !a.iter().any(|x| x == "§bce") {
        a.push("§bce".into());
    }
    let (b, _) = simple(b)?;
    // Partial second element: “du 10 au 12 mai 1926”, “entre mars et mai 1850”.
    let lone_day = matches!(a.as_slice(), [x] if num(x).is_some_and(|n| n <= 36)) && b.day.is_some();
    let (a, _) = match simple(&a) {
        Ok(_) if lone_day => complete_from(&a, &b)?,
        Ok(x) => x,
        Err(Fail::Unknown) => complete_from(&a, &b)?,
        Err(e) => return Err(e),
    };
    Ok((a, b))
}

/// Incomplete first bound, completed by the second (month, year, calendar).
fn complete_from(a: &[String], b: &SimpleDate) -> R<(SimpleDate, bool)> {
    let mut t = a.to_vec();
    let has_month = t.iter().any(|x| month(x).is_some());
    if !has_month && let Some(m) = b.month {
        let tag = b.calendar.months()[m as usize - 1].to_lowercase();
        t.push(tag);
    }
    if let Some(y) = b.year {
        if b.calendar == Calendar::FrenchRepublican {
            t.extend(["an".to_string(), y.to_string()]);
        } else {
            t.push(y.to_string());
        }
    }
    simple(&t)
}

fn parse_natural(body: &str) -> R<(Date, bool)> {
    let mut t = tokenize(body);
    if t.is_empty() {
        return Err(Fail::Unknown);
    }
    let mut question = false;
    while t.last().is_some_and(|x| x == "?") {
        t.pop();
        question = true;
    }
    // “1850 env.”: qualifier at the end.
    let mut trailing_about = false;
    if t.len() > 1 && t.last().is_some_and(|x| matches!(x.as_str(), "env" | "environ" | "ca" | "circa")) {
        t.pop();
        trailing_about = true;
    }
    // “av 5785 hébraïque”: the month Av, not “avant” (before).
    let hebrew_av = t[0] == "av" && t.iter().any(|x| x == "§heb") && !t[1..].iter().any(|x| month(x).is_some());
    let (q, n) = if hebrew_av { (Q::Exact, 0) } else { qualifier(&t).unwrap_or((Q::Exact, 0)) };
    let rest = &t[n..];
    if rest.is_empty() {
        return Err(Fail::Unknown);
    }
    let mut auto_phrase = false;
    let date = match q {
        Q::Bet => {
            let i = find(rest, &["et", "and", "-", "a"]).ok_or(Fail::Unknown)?;
            let (a, b) = pair(&rest[..i], &rest[i + 1..])?;
            Date::Between(a, b)
        }
        Q::FromTo | Q::From => match find(rest, &["a", "au", "aux", "jusqu'", "jusque", "to", "until", "till", "-"]) {
            Some(i) => {
                let mut j = i + 1;
                if rest[i].starts_with("jusqu") && rest.get(j).is_some_and(|x| matches!(x.as_str(), "a" | "au" | "en"))
                {
                    j += 1;
                }
                let (a, b) = pair(&rest[..i], &rest[j..])?;
                Date::FromTo(a, b)
            }
            None => Date::From(simple(rest)?.0),
        },
        Q::Exact => match find(rest, &["-", "a", "au", "ou", "or", "to"]) {
            Some(i) => {
                let sep = rest[i].as_str();
                let (a, b) = pair(&rest[..i], &rest[i + 1..])?;
                match sep {
                    "-" => Date::Between(a, b),
                    "ou" | "or" => {
                        auto_phrase = true;
                        Date::Between(a, b)
                    }
                    _ => Date::FromTo(a, b),
                }
            }
            None => {
                let (d, dual) = simple(rest)?;
                auto_phrase |= dual;
                if question || trailing_about { Date::About(d) } else { Date::Exact(d) }
            }
        },
        _ => {
            let (d, dual) = simple(rest)?;
            auto_phrase |= dual;
            match q {
                Q::About => Date::About(d),
                Q::Cal => Date::Calculated(d),
                Q::Est => Date::Estimated(d),
                Q::Bef => Date::Before(d),
                Q::Aft => Date::After(d),
                _ => Date::To(d),
            }
        }
    };
    date.validate().map_err(Fail::Invalid)?;
    Ok((date, auto_phrase))
}

/// Month name → (calendar, number). Gregorian also covers Julian.
fn month(w: &str) -> Option<(Calendar, u8)> {
    use Calendar::*;
    let g = |m| Some((Gregorian, m));
    let r = |m| Some((FrenchRepublican, m));
    let h = |m| Some((Hebrew, m));
    match w {
        "janvier" | "janv" | "jan" | "january" => g(1),
        "fevrier" | "fevr" | "fev" | "feb" | "february" => g(2),
        "mars" | "mar" | "march" => g(3),
        "avril" | "avr" | "apr" | "april" => g(4),
        "mai" | "may" => g(5),
        "juin" | "jun" | "june" => g(6),
        "juillet" | "juil" | "jul" | "july" => g(7),
        "aout" | "aug" | "august" => g(8),
        "septembre" | "sept" | "sep" | "september" | "7bre" => g(9),
        "octobre" | "oct" | "october" | "8bre" => g(10),
        "novembre" | "nov" | "november" | "9bre" => g(11),
        "decembre" | "dec" | "december" | "xbre" | "10bre" => g(12),
        "vendemiaire" | "vend" => r(1),
        "brumaire" | "brum" => r(2),
        "frimaire" | "frim" => r(3),
        "nivose" | "nivo" | "niv" => r(4),
        "pluviose" | "pluv" => r(5),
        "ventose" | "vent" => r(6),
        "germinal" | "germ" => r(7),
        "floreal" | "flor" => r(8),
        "prairial" | "prai" | "prair" => r(9),
        "messidor" | "mess" => r(10),
        "thermidor" | "ther" | "therm" => r(11),
        "fructidor" | "fruc" | "fruct" => r(12),
        "complementaire" | "complementaires" | "complementary" | "sansculottide" | "sansculottides" | "comp" => r(13),
        "tishri" | "tichri" | "tishrei" | "tisri" | "tsh" => h(1),
        "heshvan" | "hechvan" | "cheshvan" | "marheshvan" | "marcheshvan" | "hesvan" | "csh" => h(2),
        "kislev" | "kislew" | "ksl" => h(3),
        "tevet" | "teveth" | "tebeth" | "tvt" => h(4),
        "shevat" | "chevat" | "shvat" | "chvat" | "shv" => h(5),
        "adr" => h(6),
        "adar" | "ads" => h(7),
        "nisan" | "nissan" | "nsn" => h(8),
        "iyar" | "iyyar" | "iyr" => h(9),
        "sivan" | "siwan" | "svn" => h(10),
        "tammuz" | "tamouz" | "tammouz" | "tamuz" | "tmz" => h(11),
        "av" | "ab" | "aav" => h(12),
        "elul" | "eloul" | "ell" => h(13),
        _ => None,
    }
}

fn roman_value(s: &str) -> Option<i32> {
    let v = |c| match c {
        'i' => Some(1),
        'v' => Some(5),
        'x' => Some(10),
        'l' => Some(50),
        'c' => Some(100),
        'd' => Some(500),
        'm' => Some(1000),
        _ => None,
    };
    let vals: Vec<i32> = s.chars().map(v).collect::<Option<_>>()?;
    let mut n = 0;
    for (i, &x) in vals.iter().enumerate() {
        if vals.get(i + 1).is_some_and(|&y| y > x) { n -= x } else { n += x }
    }
    (n > 0 && super::roman(n).eq_ignore_ascii_case(s)).then_some(n)
}

/// An English ordinal day: `1st`, `22nd`, `3rd`, `14th`.
fn ordinal(s: &str) -> Option<i32> {
    let digits = s.strip_suffix("st").or_else(|| s.strip_suffix("nd")).or_else(|| s.strip_suffix("rd"));
    let digits = digits.or_else(|| s.strip_suffix("th"))?;
    num(digits).filter(|n| (1..=31).contains(n))
}

fn num(s: &str) -> Option<i32> {
    (!s.is_empty() && s.len() <= 6 && s.bytes().all(|b| b.is_ascii_digit())).then(|| s.parse().ok()).flatten()
}

const FILLERS: [&str; 15] =
    ["le", "la", "l'", "en", "de", "du", "d'", "a", "au", "mois", "annee", "the", "of", "in", "on"];

/// A simple date. The boolean flags a dual year (“1648/49”).
fn simple(t: &[String]) -> R<(SimpleDate, bool)> {
    let mut cal_marker: Option<Calendar> = None;
    let mut bce = false;
    let mut republican_year = false;
    let mut nums_before: Vec<i32> = Vec::new();
    let mut nums_after: Vec<i32> = Vec::new();
    let mut mon: Option<(Calendar, u8)> = None;
    let mut year: Option<i32> = None;
    let mut dual = false;
    let mut i = 0;
    let push = |n: i32, mon: &Option<(Calendar, u8)>, before: &mut Vec<i32>, after: &mut Vec<i32>| {
        if mon.is_some() { after.push(n) } else { before.push(n) }
    };
    while i < t.len() {
        let w = t[i].as_str();
        i += 1;
        match w {
            "§bce" => bce = true,
            "§jul" | "§greg" | "§rep" | "§heb" => {
                let c = match w {
                    "§jul" => Calendar::Julian,
                    "§greg" => Calendar::Gregorian,
                    "§rep" => Calendar::FrenchRepublican,
                    _ => Calendar::Hebrew,
                };
                if cal_marker.as_ref().is_some_and(|x| *x != c) {
                    return Err(Fail::Invalid(t!("date.error.two_calendars").into()));
                }
                cal_marker = Some(c);
            }
            "an" | "year" => {
                let y = t.get(i).ok_or(Fail::Unknown)?;
                i += 1;
                year = Some(num(y).or_else(|| roman_value(y)).ok_or(Fail::Unknown)?);
                republican_year = true;
            }
            "1er" | "1re" | "premier" => push(1, &mon, &mut nums_before, &mut nums_after),
            // English ordinals: “1st”, “22nd”, “3rd”, “14th”.
            _ if ordinal(w).is_some() => push(ordinal(w).unwrap(), &mon, &mut nums_before, &mut nums_after),
            // “complementary days”: the month is already read.
            "day" | "days" if i >= 2 && t[i - 2] == "complementary" => {}
            "jour" | "jours" | "day" | "days"
                if t.get(i).is_some_and(|x| x.starts_with("complementaire") || x == "complementary") =>
            {
                i += 1;
                mon = Some((Calendar::FrenchRepublican, 13));
            }
            _ if FILLERS.contains(&w) => {}
            _ if w.contains(['/', '-', '.']) && w.bytes().any(|b| b.is_ascii_digit()) => {
                let parts: Vec<&str> = w.split(['/', '-', '.']).collect();
                let n: Vec<i32> = parts.iter().map(|p| num(p)).collect::<Option<_>>().ok_or(Fail::Unknown)?;
                match (parts.as_slice(), n.as_slice()) {
                    ([a, ..], [y, m, d]) if a.len() == 4 => {
                        mon = Some((Calendar::Gregorian, month_u8(*m)?));
                        nums_before.push(*d);
                        year = Some(*y);
                    }
                    (_, [d, m, y]) => {
                        mon = Some((Calendar::Gregorian, month_u8(*m)?));
                        nums_before.push(*d);
                        year = Some(*y);
                    }
                    ([a, b], [x, y]) if a.len() <= 2 && b.len() >= 3 => {
                        mon = Some((Calendar::Gregorian, month_u8(*x)?));
                        year = Some(*y);
                    }
                    // Dual dating “1648/49” or “1648/1649”: keep the second year.
                    ([a, b], [x, y]) if a.len() >= 3 && w.contains('/') => {
                        let full = if b.len() < a.len() {
                            let p = 10_i32.pow(b.len() as u32);
                            x - x % p + y + if *y < x % p { p } else { 0 }
                        } else {
                            *y
                        };
                        if full <= *x || full - x > 1 {
                            return Err(Fail::Unknown);
                        }
                        dual = true;
                        if mon.is_some() { nums_after.push(full) } else { year = Some(full) }
                    }
                    _ => return Err(Fail::Unknown),
                }
            }
            _ if num(w).is_some() => push(num(w).unwrap(), &mon, &mut nums_before, &mut nums_after),
            _ => {
                let mut m = month(w).ok_or(Fail::Unknown)?;
                if mon.is_some() {
                    return Err(Fail::Unknown);
                }
                // “adar I” / “adar II”.
                if w == "adar" {
                    match t.get(i).map(String::as_str) {
                        Some("i" | "rishon" | "aleph") => {
                            m.1 = 6;
                            i += 1;
                        }
                        Some("ii" | "sheni" | "bet") => i += 1,
                        _ => {}
                    }
                }
                mon = Some(m);
            }
        }
    }
    let mut d = SimpleDate { bce, ..Default::default() };
    match (&mon, nums_before.as_slice(), nums_after.as_slice(), year) {
        (None, [], [], Some(y)) => d.year = Some(y),
        (None, [y], [], None) => d.year = Some(*y),
        (Some((_, m)), before, after, y) => {
            d.month = Some(*m);
            d.day = match before {
                [] => None,
                [x] => Some(u8::try_from(*x).map_err(|_| Fail::Invalid(t!("date.error.day").into()))?),
                _ => return Err(Fail::Unknown),
            };
            d.year = match (after, y) {
                ([], Some(y)) => Some(y),
                ([y], None) => Some(*y),
                // Month first, as in English: “March 14, 1912”.
                ([day, y], None) if d.day.is_none() && (1..=31).contains(day) => {
                    d.day = Some(*day as u8);
                    Some(*y)
                }
                // Day and month without a year: GEDCOM requires a year, keep the text.
                _ => return Err(Fail::Unknown),
            };
        }
        _ => return Err(Fail::Unknown),
    }
    // Calendar: Republican or Hebrew month, “an XII”, or an explicit marker.
    let implied = match &mon {
        Some((c, _)) if *c != Calendar::Gregorian => Some(c.clone()),
        _ if republican_year => Some(Calendar::FrenchRepublican),
        _ => None,
    };
    if republican_year && implied.as_ref() != Some(&Calendar::FrenchRepublican) {
        return Err(Fail::Invalid(t!("date.error.year_word").into()));
    }
    d.calendar = match (implied, cal_marker) {
        (Some(a), Some(b)) if a != b => {
            let calendar = b.label().to_lowercase();
            return Err(Fail::Invalid(t!("date.error.month_calendar", calendar = calendar).into()));
        }
        (Some(a), _) => a,
        (None, Some(Calendar::FrenchRepublican)) if d.month.is_some() => {
            return Err(Fail::Invalid(t!("date.error.gregorian_month_republican").into()));
        }
        (None, Some(Calendar::Hebrew)) if d.month.is_some() => {
            return Err(Fail::Invalid(t!("date.error.gregorian_month_hebrew").into()));
        }
        (None, Some(b)) => b,
        (None, None) => Calendar::Gregorian,
    };
    if d.calendar == Calendar::FrenchRepublican && !republican_year && d.year.is_some_and(|y| y > 100) {
        return Err(Fail::Invalid(t!("date.error.republican_year").into()));
    }
    Ok((d, dual))
}

fn month_u8(m: i32) -> R<u8> {
    if (1..=12).contains(&m) {
        Ok(m as u8)
    } else {
        Err(Fail::Invalid(t!("date.error.month_number", month = m).into()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn g(s: &str) -> String {
        match parse_input(s) {
            Ok(p) if p.phrase.is_empty() => p.value(),
            Ok(p) => format!("{} + {}", p.value(), p.phrase),
            Err(e) => format!("ERR {e}"),
        }
    }

    #[test]
    fn simple_french() {
        assert_eq!(g("14 mars 1912"), "14 MAR 1912");
        assert_eq!(g("le 1er janvier 1900"), "1 JAN 1900");
        assert_eq!(g("14/03/1912"), "14 MAR 1912");
        assert_eq!(g("14.3.1912"), "14 MAR 1912");
        assert_eq!(g("1912-03-14"), "14 MAR 1912");
        assert_eq!(g("03/1912"), "MAR 1912");
        assert_eq!(g("en mars 1850"), "MAR 1850");
        assert_eq!(g("Août 1914"), "AUG 1914");
        assert_eq!(g("7bre 1750"), "SEP 1750");
        assert_eq!(g("1850"), "1850");
        assert_eq!(g("19 SEP 2000"), "19 SEP 2000");
    }

    #[test]
    fn qualifiers() {
        assert_eq!(g("vers 1850"), "ABT 1850");
        assert_eq!(g("v. 1850"), "ABT 1850");
        assert_eq!(g("env. 1850"), "ABT 1850");
        assert_eq!(g("~1850"), "ABT 1850");
        assert_eq!(g("1850 ?"), "ABT 1850");
        assert_eq!(g("1850?"), "ABT 1850");
        assert_eq!(g("estimée 1850"), "EST 1850");
        assert_eq!(g("calculé 1850"), "CAL 1850");
        assert_eq!(g("avant le 23 mai 1809"), "BEF 23 MAY 1809");
        assert_eq!(g("<1809"), "BEF 1809");
        assert_eq!(g("après 1900"), "AFT 1900");
        assert_eq!(g("abt 1850"), "ABT 1850");
    }

    #[test]
    fn ranges_and_periods() {
        assert_eq!(g("entre 1820 et 1825"), "BET 1820 AND 1825");
        assert_eq!(g("1820-1825"), "BET 1820 AND 1825");
        assert_eq!(g("1820 - 1825"), "BET 1820 AND 1825");
        assert_eq!(g("entre mars et mai 1850"), "BET MAR 1850 AND MAY 1850");
        assert_eq!(g("de 1850 à 1860"), "FROM 1850 TO 1860");
        assert_eq!(g("du 10 mai 1926 au 10 nov. 1926"), "FROM 10 MAY 1926 TO 10 NOV 1926");
        assert_eq!(g("du 10 au 12 mai 1926"), "FROM 10 MAY 1926 TO 12 MAY 1926");
        assert_eq!(g("depuis 1850"), "FROM 1850");
        assert_eq!(g("à partir de mai 1850"), "FROM MAY 1850");
        assert_eq!(g("jusqu'en 1860"), "TO 1860");
        assert_eq!(g("jusqu’au 10 mai 1926"), "TO 10 MAY 1926");
        assert_eq!(g("1820 ou 1821"), "BET 1820 AND 1821 + 1820 ou 1821");
        assert_eq!(g("entre 1860 et 1850"), "ERR the first date must come before the second");
    }

    #[test]
    fn calendars() {
        assert_eq!(g("15 brumaire an XII"), "FRENCH_R 15 BRUM 12");
        assert_eq!(g("15 brum. an 12"), "FRENCH_R 15 BRUM 12");
        assert_eq!(g("an XII"), "FRENCH_R 12");
        assert_eq!(g("3 jours complémentaires an III"), "FRENCH_R 3 COMP 3");
        assert_eq!(g("3 mars 1700 julien"), "JULIAN 3 MAR 1700");
        assert_eq!(g("3 mars 1700 (julien)"), "JULIAN 3 MAR 1700");
        assert_eq!(g("entre 1650 et 1660 (julien)"), "BET JULIAN 1650 AND JULIAN 1660");
        assert_eq!(g("15 nissan 5784 hébraïque"), "HEBREW 15 NSN 5784");
        assert_eq!(g("15 nissan 5784"), "HEBREW 15 NSN 5784");
        assert_eq!(g("14 adar I 5784"), "HEBREW 14 ADR 5784");
        assert_eq!(g("av 5785 hébraïque"), "HEBREW AAV 5785");
        assert_eq!(g("5785 hébreu"), "HEBREW 5785");
        assert_eq!(g("44 av. J.-C."), "44 BCE");
        assert_eq!(g("entre 50 et 44 av. J.-C."), "BET 50 BCE AND 44 BCE");
        assert_eq!(g("15 brumaire 1803"), "ERR French Republican year expected, e.g. “year XII”");
        assert_eq!(g("_MAYA 5 _XUL 1000"), "_MAYA 5 _XUL 1000");
    }

    #[test]
    fn invalid_and_phrases() {
        assert_eq!(g("30 février 1900"), "ERR Feb 1900 only has 28 days");
        assert_eq!(g("29 février 1900 julien"), "JULIAN 29 FEB 1900");
        assert_eq!(g("vers 1850 (d'après l'âge au décès)"), "ABT 1850 + d'après l'âge au décès");
        assert_eq!(g("30 janvier 1648/49"), "30 JAN 1649 + 30 janvier 1648/49");
        assert_eq!(g("5 janvier"), " + 5 janvier");
        assert_eq!(g("pendant la guerre"), " + pendant la guerre");
        assert_eq!(g("« 1850 »"), " + 1850");
        assert_eq!(g(""), "");
    }

    #[test]
    fn english() {
        assert_eq!(g("14 March 1912"), "14 MAR 1912");
        assert_eq!(g("March 14th, 1912"), "14 MAR 1912");
        assert_eq!(g("on 1st Jan 1900"), "1 JAN 1900");
        assert_eq!(g("about 1850"), "ABT 1850");
        assert_eq!(g("circa 1850"), "ABT 1850");
        assert_eq!(g("calculated 1850"), "CAL 1850");
        assert_eq!(g("estimated Mar 1850"), "EST MAR 1850");
        assert_eq!(g("before 23 May 1809"), "BEF 23 MAY 1809");
        assert_eq!(g("after 1900"), "AFT 1900");
        assert_eq!(g("between 1820 and 1825"), "BET 1820 AND 1825");
        assert_eq!(g("from 10 May to 12 Jun 1926"), "FROM 10 MAY 1926 TO 12 JUN 1926");
        assert_eq!(g("from 1850 to 1860"), "FROM 1850 TO 1860");
        assert_eq!(g("since 1850"), "FROM 1850");
        assert_eq!(g("until 1860"), "TO 1860");
        assert_eq!(g("1820 or 1821"), "BET 1820 AND 1821 + 1820 or 1821");
        assert_eq!(g("15 Brumaire year XII"), "FRENCH_R 15 BRUM 12");
        assert_eq!(g("3 complementary days year III"), "FRENCH_R 3 COMP 3");
        assert_eq!(g("3 Mar 1700 Julian"), "JULIAN 3 MAR 1700");
        assert_eq!(g("3 Mar 1700 old style"), "JULIAN 3 MAR 1700");
        assert_eq!(g("1700 French Republican"), "ERR French Republican year expected, e.g. “year XII”");
        assert_eq!(g("15 Nisan 5784 Hebrew"), "HEBREW 15 NSN 5784");
        assert_eq!(g("44 BC"), "44 BCE");
        assert_eq!(g("“1850”"), " + 1850");
    }

    #[test]
    fn input_text_roundtrip() {
        for raw in [
            "19 SEP 2000",
            "1 JAN 1900",
            "ABT 1850",
            "CAL 1850",
            "EST MAR 1850",
            "BEF 23 MAY 1809",
            "AFT 1900",
            "BET 1820 AND 1825",
            "BET MAR 1850 AND 12 MAY 1851",
            "FROM 10 MAY 1926 TO 10 NOV 1926",
            "FROM 1850 TO 1860",
            "FROM 1850",
            "TO 10 MAY 1926",
            "TO 1860",
            "FRENCH_R 15 BRUM 12",
            "FRENCH_R 3 COMP 3",
            "FRENCH_R VEND 2",
            "FRENCH_R 14",
            "ABT FRENCH_R 30 GERM 4",
            "JULIAN 3 MAR 1700",
            "BET JULIAN 1650 AND JULIAN 1660",
            "FROM GREGORIAN 1670 TO JULIAN 1800",
            "HEBREW 15 NSN 5784",
            "HEBREW 14 ADR 5784",
            "HEBREW 14 ADS 5784",
            "HEBREW 14 ADS 5785",
            "HEBREW AAV 5785",
            "HEBREW 5785",
            "44 BCE",
            "JULIAN 15 MAR 44 BCE",
            "_MAYA 5 _XUL 1000",
        ] {
            let d = Date::parse(raw);
            for lang in crate::i18n::LANGUAGES {
                let text = d.to_input_text_in(lang);
                let back = parse_input(&text).unwrap_or_else(|e| panic!("{raw} → {text} : {e}"));
                assert_eq!(back.value(), raw, "{raw} → “{text}” ({lang})");
            }
        }
        let p = Parsed::from_gedcom("ABT 1850", "d'après l'âge");
        assert_eq!(parse_input(&p.to_input_text()).unwrap(), p);
        let p = Parsed::from_gedcom("", "1850");
        assert_eq!(parse_input(&p.to_input_text()).unwrap(), p);
    }
}
