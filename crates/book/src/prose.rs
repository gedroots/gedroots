//! Simple sentences written from the facts of the tree (“He married Jeanne PRÉVOST on 9 January
//! 1898 in Fontaine-Denis-Nuisy.”), in the interface language. They only repeat what the tree says:
//! the narrative, the historical context and the hypotheses are left to the author or an agent.

use gedcom::analysis::span::{Age, Span};
use gedcom::date::Date;
use gedcom::document::Gedcom;
use gedcom::model::{Event, Individual, short_place};
use rust_i18n::t;

const MONTHS: [&str; 12] = ["JAN", "FEB", "MAR", "APR", "MAY", "JUN", "JUL", "AUG", "SEP", "OCT", "NOV", "DEC"];

/// Full month name.
fn month(m: usize) -> String {
    match m {
        1 => t!("book.month.1"),
        2 => t!("book.month.2"),
        3 => t!("book.month.3"),
        4 => t!("book.month.4"),
        5 => t!("book.month.5"),
        6 => t!("book.month.6"),
        7 => t!("book.month.7"),
        8 => t!("book.month.8"),
        9 => t!("book.month.9"),
        10 => t!("book.month.10"),
        11 => t!("book.month.11"),
        _ => t!("book.month.12"),
    }
    .into_owned()
}

/// A date inside a sentence: “on 5 September 1864”, « en 1864 », “about 1850”. Dates that are
/// not a plain Gregorian date (periods, other calendars…) keep their usual display.
pub fn date_phrase(raw: &str) -> Option<String> {
    let raw = raw.trim();
    if raw.is_empty() {
        return None;
    }
    let mut words: Vec<&str> = raw.split_whitespace().collect();
    let modifier = match words.first().copied() {
        Some("ABT" | "CAL" | "EST") => Some("about"),
        Some("BEF") => Some("before"),
        Some("AFT") => Some("after"),
        _ => None,
    };
    if modifier.is_some() {
        words.remove(0);
    }
    let plain = || -> Option<(Option<u32>, Option<usize>, i32)> {
        match words.as_slice() {
            [y] => Some((None, None, y.parse().ok()?)),
            [m, y] => Some((None, Some(MONTHS.iter().position(|x| x == m)? + 1), y.parse().ok()?)),
            [d, m, y] => Some((Some(d.parse().ok()?), Some(MONTHS.iter().position(|x| x == m)? + 1), y.parse().ok()?)),
            _ => None,
        }
    };
    let Some((day, mon, year)) = plain() else {
        return Some(Date::parse(raw).display());
    };
    let mut text = Vec::new();
    if let Some(d) = day {
        text.push(if d == 1 { t!("book.first_day").into_owned() } else { d.to_string() });
    }
    if let Some(m) = mon {
        text.push(month(m));
    }
    text.push(year.to_string());
    let date = text.join(" ");
    Some(
        match (modifier, day, mon) {
            (Some("about"), ..) => t!("book.date.about", date = date),
            (Some("before"), ..) => t!("book.date.before", date = date),
            (Some("after"), ..) => t!("book.date.after", date = date),
            (_, Some(_), _) => t!("book.date.day", date = date),
            (_, None, Some(_)) => t!("book.date.month", date = date),
            _ => t!("book.date.year", date = date),
        }
        .into_owned(),
    )
}

/// Town of a place, without its context: “Troyes”.
pub fn town(raw: &str) -> String {
    let short = short_place(raw);
    match short.find(" (") {
        Some(i) if short.ends_with(')') => short[..i].to_string(),
        _ => short,
    }
}

/// “on 5 September 1864 in Troyes”, from an event's date and place; `None` if it has neither.
pub fn when_where(e: &Event) -> Option<String> {
    let date = e.date_raw().and_then(date_phrase);
    let place = e.place().map(town).filter(|p| !p.is_empty()).map(|p| t!("book.place", place = p).into_owned());
    match (date, place) {
        (None, None) => None,
        (Some(d), None) => Some(d),
        (None, Some(p)) => Some(p),
        (Some(d), Some(p)) => Some(format!("{d} {p}")),
    }
}

/// “A, B and C”.
pub fn list(items: &[String]) -> String {
    match items {
        [] => String::new(),
        [one] => one.clone(),
        [rest @ .., last] => t!("book.list_and", list = rest.join(", "), last = last).into_owned(),
    }
}

fn pronoun(ind: &Individual) -> Option<String> {
    match ind.sex() {
        "M" => Some(t!("book.he").into_owned()),
        "F" => Some(t!("book.she").into_owned()),
        _ => None,
    }
}

/// First given name, or the full name.
pub fn first_name(ind: &Individual) -> String {
    ind.given().split_whitespace().next().map(str::to_string).unwrap_or_else(|| ind.display_name())
}

/// Lowercase first letter (“He” → “he” inside a sentence).
fn lower_first(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(f) => f.to_lowercase().collect::<String>() + c.as_str(),
        None => String::new(),
    }
}

/// Uppercase first letter, at the start of a sentence.
fn upper_first(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
        None => String::new(),
    }
}

/// Age in whole years between two event dates.
pub fn age_years(birth: &Event, at: &Event) -> Option<u32> {
    let b = Span::parse(birth.date_raw()?)?;
    let a = Span::parse(at.date_raw()?)?;
    let age = Age::between(&b, &a);
    let (lo, hi) = (age.min_years()?, age.max_years()?);
    // Only when the age is known to the year.
    (hi - lo < 1.5 && lo >= 0.).then(|| ((lo + hi) / 2.).floor() as u32)
}

/// Events told in their own sentence, after the birth and the occupations.
fn told_alone(tag: &str) -> bool {
    !matches!(tag, "BIRT" | "CHR" | "BAPM" | "DEAT" | "BURI" | "OCCU" | "SEX" | "NAME")
}

/// The paragraphs of a person's record: birth and parents, occupations, each union with its
/// children, other events, death. `full`: every event; otherwise birth, unions and death only.
pub fn paragraphs(g: &Gedcom, ind: &Individual, full: bool) -> Vec<String> {
    let name = first_name(ind);
    let who = pronoun(ind).unwrap_or_else(|| name.clone());
    let mut out = Vec::new();

    // Birth and parents.
    let mut first = match ind.birth().and_then(|e| when_where(&e)) {
        Some(w) => t!("book.born", name = name, when = w).into_owned(),
        None => t!("book.born_unknown", name = name).into_owned(),
    };
    let (father, mother) = g.parents(ind.xref());
    let parents: Vec<String> = [father, mother].into_iter().flatten().map(|p| p.display_name()).collect();
    if !parents.is_empty() {
        let p = match parents.as_slice() {
            [a, b] => t!("book.and", a = a, b = b).into_owned(),
            _ => parents[0].clone(),
        };
        let of = match ind.sex() {
            "M" => t!("book.child_of.m", parents = p),
            "F" => t!("book.child_of.f", parents = p),
            _ => t!("book.child_of.u", parents = p),
        };
        first = format!("{}, {of}.", first.trim_end_matches('.'));
    }
    if full {
        let mut jobs: Vec<String> = Vec::new();
        for e in ind.events().iter().filter(|e| e.tag() == "OCCU") {
            if let Some(v) = e.value().map(|v| lower_first(v.trim())).filter(|v| !v.is_empty())
                && !jobs.contains(&v)
            {
                jobs.push(v);
            }
        }
        if !jobs.is_empty() {
            first.push(' ');
            first.push_str(&t!("book.occupation", who = who, what = list(&jobs)));
        }
    }
    out.push(first);

    // Unions.
    let birth = ind.birth();
    for (spouse, children) in g.spouses_and_children(ind.xref()) {
        let fam = ind.fams().into_iter().filter_map(|f| g.family(f)).find(|f| {
            spouse.as_ref().is_some_and(|s| f.husband() == Some(s.xref()) || f.wife() == Some(s.xref()))
                || (spouse.is_none()
                    && f.husband().is_none_or(|h| h == ind.xref())
                    && f.wife().is_none_or(|w| w == ind.xref()))
        });
        let marriage = fam.as_ref().and_then(|f| f.marriage());
        let mut p = String::new();
        if let Some(s) = &spouse {
            let subject = match ind.sex() {
                "M" => t!("book.married_who.he").into_owned(),
                "F" => t!("book.married_who.she").into_owned(),
                _ => name.clone(),
            };
            let when = marriage.as_ref().and_then(when_where);
            let (when, subject) = match when {
                Some(w) => (format!("{}, ", upper_first(&w)), subject),
                None => (String::new(), upper_first(&subject)),
            };
            let mut sentence = t!("book.married", when = when, who = subject, spouse = s.display_name()).into_owned();
            if let (Some(b), Some(m)) = (&birth, &marriage)
                && let Some(age) = age_years(b, m)
            {
                sentence = format!("{} ({}).", sentence.trim_end_matches('.'), t!("book.age_years", n = age));
            }
            p.push_str(&sentence);
        }
        if !children.is_empty() {
            let names: Vec<String> = children
                .iter()
                .map(|c| match c.birth_year() {
                    Some(y) => format!("{} ({y})", c.given().split_whitespace().collect::<Vec<_>>().join(" ")),
                    None => c.given(),
                })
                .filter(|n| !n.trim().is_empty())
                .collect();
            if !names.is_empty() {
                if !p.is_empty() {
                    p.push(' ');
                }
                p.push_str(&if names.len() == 1 {
                    t!("book.children.one", list = list(&names)).into_owned()
                } else {
                    t!("book.children.other", n = names.len(), list = list(&names)).into_owned()
                });
            }
        }
        if !p.is_empty() {
            out.push(p);
        }
    }

    // Other events, in date order.
    if full {
        let mut others: Vec<(i32, String)> = Vec::new();
        for e in ind.events().iter().filter(|e| told_alone(e.tag())) {
            let Some(w) = when_where(e) else { continue };
            let label = match e.value() {
                Some(v) if e.tag() != "EVEN" => format!("{} ({})", e.label(), v.trim()),
                _ => e.label(),
            };
            let key = e.date().map(|d| d.sort_key().0).unwrap_or(i32::MAX);
            others.push((key, t!("book.event", event = label, when = w).into_owned()));
        }
        others.sort_by_key(|(k, _)| *k);
        if !others.is_empty() {
            out.push(others.into_iter().map(|(_, s)| s).collect::<Vec<_>>().join(" "));
        }
    }

    // Death.
    if let Some(d) = ind.event("DEAT") {
        let w = when_where(&d);
        let age = birth.as_ref().and_then(|b| age_years(b, &d));
        let s = match (w, age) {
            (Some(w), Some(a)) => Some(t!("book.died_aged", name = name, when = w, age = a)),
            (Some(w), None) => Some(t!("book.died", name = name, when = w)),
            _ => None,
        };
        if let Some(s) = s {
            out.push(s.into_owned());
        }
    } else if let Some(b) = ind.event("BURI")
        && let Some(w) = when_where(&b)
    {
        out.push(
            match ind.sex() {
                "M" => t!("book.buried.m", name = name, when = w),
                "F" => t!("book.buried.f", name = name, when = w),
                _ => t!("book.buried.u", name = name, when = w),
            }
            .into_owned(),
        );
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dates_in_sentences() {
        assert_eq!(date_phrase("5 SEP 1864").as_deref(), Some("on 5 September 1864"));
        assert_eq!(date_phrase("SEP 1864").as_deref(), Some("in September 1864"));
        assert_eq!(date_phrase("ABT 1850").as_deref(), Some("about 1850"));
        assert_eq!(date_phrase("BEF 1 JAN 1900").as_deref(), Some("before 1 January 1900"));
        assert_eq!(date_phrase("1864").as_deref(), Some("in 1864"));
        assert_eq!(date_phrase(""), None);
    }

    #[test]
    fn lists() {
        let l = |v: &[&str]| list(&v.iter().map(|s| s.to_string()).collect::<Vec<_>>());
        assert_eq!(l(&["A"]), "A");
        assert_eq!(l(&["A", "B", "C"]), "A, B and C");
    }
}
