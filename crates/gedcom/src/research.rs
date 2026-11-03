//! Research data for AI agents (GEDRoots' MCP server): summarized record of an individual, search,
//! gaps in the ancestry, Data Quality issues and hiding of living people.
//!
//! Everything is read only: these functions never modify the document.

use crate::analysis::check::{Issue, Severity, code_label};
use crate::document::{Gedcom, RecordKind};
use crate::model::{Event, Media};
use crate::quality::{self, Category};
use std::collections::HashMap;

/// A person named in a result. A living person is given only what identifies them: their name and
/// their birth.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, schemars::JsonSchema))]
pub struct PersonRef {
    pub id: String,
    pub name: String,
    pub years: String,
    pub living: bool,
    /// Living person: date and place of birth (“4 MAR 1990, Rennes”).
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "String::is_empty"))]
    pub born: String,
}

/// Source cited by an event, with the reference in the source (`PAGE`).
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, schemars::JsonSchema))]
pub struct Citation {
    pub source_id: String,
    pub source_title: String,
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "String::is_empty"))]
    pub page: String,
}

/// Linked media, with its files (paths relative to the `.ged`, or URLs).
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, schemars::JsonSchema))]
pub struct MediaRef {
    pub id: String,
    pub title: String,
    pub files: Vec<String>,
}

/// An event or attribute, as a researcher reads it.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, schemars::JsonSchema))]
pub struct EventInfo {
    /// Position of the event in its record (the person, or the family for a union event): it
    /// designates the event in a proposal.
    pub index: usize,
    /// GEDCOM tag (`BIRT`, `MARR`, `EVEN`…).
    pub tag: String,
    pub label: String,
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "String::is_empty"))]
    pub value: String,
    /// GEDCOM date (`14 MAR 1912`, `ABT 1850`…).
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "String::is_empty"))]
    pub date: String,
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "String::is_empty"))]
    pub place: String,
    pub citations: Vec<Citation>,
    pub media: Vec<MediaRef>,
}

/// A union: spouse, family events and children.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, schemars::JsonSchema))]
pub struct Union {
    pub family_id: String,
    pub spouse: Option<PersonRef>,
    pub events: Vec<EventInfo>,
    pub children: Vec<PersonRef>,
}

/// Record of an individual for an agent.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, schemars::JsonSchema))]
pub struct PersonSheet {
    pub person: PersonRef,
    /// `M`, `F`, `X` or `U`.
    pub sex: String,
    /// Sosa number from the root individual, for a direct ancestor.
    pub sosa: Option<u64>,
    pub events: Vec<EventInfo>,
    pub father: Option<PersonRef>,
    pub mother: Option<PersonRef>,
    pub unions: Vec<Union>,
    pub notes: Vec<String>,
}

/// Why an event is among the gaps.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, schemars::JsonSchema))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum GapKind {
    /// The event is not in the file.
    Missing,
    /// The event is there, without a source.
    Unsourced,
}

/// A record to look for, for a direct ancestor.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, schemars::JsonSchema))]
pub struct Gap {
    pub person: PersonRef,
    pub sosa: u64,
    /// Generation (1: the root individual, 2: their parents…).
    pub generation: u32,
    /// `BIRT`, `MARR` or `DEAT`.
    pub event: String,
    pub kind: GapKind,
    /// For a marriage: the family and the spouse.
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "Option::is_none"))]
    pub family_id: Option<String>,
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "Option::is_none"))]
    pub spouse: Option<PersonRef>,
    /// Date and place of the event already known (“without source” gap).
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "String::is_empty"))]
    pub date: String,
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "String::is_empty"))]
    pub place: String,
    /// Places of the person's other events, to know where to look.
    pub known_places: Vec<String>,
}

/// A record named by a quality issue.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, schemars::JsonSchema))]
pub struct RecordRef {
    pub id: String,
    /// `person`, `family`, `source`, `repository`, `media`, `note` or `submitter`.
    pub kind: String,
    pub label: String,
    /// A person who may be living, or a family where a spouse may be.
    pub living: bool,
}

/// A Data Quality issue, as the app's Quality page shows it.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, schemars::JsonSchema))]
pub struct QualityIssue {
    /// Identifier of the issue, to designate it to GEDRoots.
    pub id: String,
    /// Category of the Quality page (`dates`, `family`, `structure`, `empty`, `media`, `places`,
    /// `sources`, `files`, `coords`).
    pub category: String,
    /// Kind of issue (`death_before_birth`, `empty_fact`…) and its name in the app.
    pub code: String,
    pub kind: String,
    /// `error`, `warning` or `info`.
    pub severity: String,
    /// The issue as the Quality page states it.
    pub message: String,
    /// Main record (the one the Quality page opens); absent for an issue about places or files.
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "Option::is_none"))]
    pub record: Option<RecordRef>,
    /// Other records involved.
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "Vec::is_empty"))]
    pub related: Vec<RecordRef>,
    /// Places or files involved.
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "Vec::is_empty"))]
    pub values: Vec<String>,
    /// The Quality page fixes it in one click; `batch_fix`: for the whole list at once.
    pub automatic_fix: bool,
    pub batch_fix: bool,
    /// The user can hide it on the Quality page, when they checked it is right.
    pub can_hide: bool,
    /// Hidden by the user.
    pub hidden: bool,
}

/// Number of issues of a category.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, schemars::JsonSchema))]
pub struct QualityCount {
    pub category: String,
    pub label: String,
    /// `fix` (errors to fix) or `complete` (missing data to complete).
    pub group: String,
    pub count: usize,
}

/// Which issues to list.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct QualityFilter {
    pub category: Option<String>,
    pub code: Option<String>,
    /// Issues whose main or related record is this one.
    pub record: Option<String>,
    /// Also the issues the user hid.
    pub include_hidden: bool,
}

/// Issues of the file for an agent, a page at a time.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, schemars::JsonSchema))]
pub struct QualityReport {
    pub issues: Vec<QualityIssue>,
    /// Issues matching the filter, before `offset` and `limit`.
    pub total: usize,
    /// Every category with issues, without the filter (hidden issues only with `include_hidden`).
    pub categories: Vec<QualityCount>,
}

/// A person born less than this number of years ago, without a death, is living.
pub const LIVING_YEARS: i32 = 100;

/// Current year (Gregorian calendar, UTC).
pub fn current_year() -> i32 {
    let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    1970 + (secs as f64 / (365.2425 * 86_400.0)) as i32
}

/// Events that prove a death.
const DEATH_TAGS: [&str; 3] = ["DEAT", "BURI", "CREM"];

impl Gedcom {
    /// Could the person be living? Cautious: without a death or a date that places them more than
    /// [`LIVING_YEARS`] years ago, they are considered living.
    pub fn is_living(&self, xref: &str, this_year: i32) -> bool {
        let Some(ind) = self.individual(xref) else { return false };
        if ind.events().iter().any(|e| DEATH_TAGS.contains(&e.tag())) {
            return false;
        }
        match self.estimated_birth_year(xref) {
            Some(y) => y > this_year - LIVING_YEARS,
            None => true,
        }
    }

    /// Birth year known or inferred: birth or baptism, otherwise the oldest dated event of the
    /// person, their unions or their children (minus 15 years), otherwise the birth of a parent
    /// (plus 15 years).
    fn estimated_birth_year(&self, xref: &str) -> Option<i32> {
        let ind = self.individual(xref)?;
        let year = |e: &Event| e.date().and_then(|d| d.year());
        if let Some(y) =
            ind.events().iter().filter(|e| matches!(e.tag(), "BIRT" | "BAPM" | "CHR")).filter_map(year).min()
        {
            return Some(y);
        }
        let mut earliest: Option<i32> = ind.events().iter().filter_map(year).min().map(|y| y - 15);
        for fam in ind.fams().iter().filter_map(|f| self.family(f)) {
            let mut years: Vec<i32> = fam.events().iter().filter_map(year).collect();
            years.extend(
                fam.children()
                    .iter()
                    .filter_map(|c| self.individual(c))
                    .filter_map(|c| c.birth().as_ref().and_then(year)),
            );
            for y in years.into_iter().map(|y| y - 15) {
                earliest = Some(earliest.map_or(y, |e| e.min(y)));
            }
        }
        if earliest.is_some() {
            return earliest;
        }
        let (f, m) = self.parents(xref);
        [f, m].into_iter().flatten().filter_map(|p| p.birth().as_ref().and_then(year)).min().map(|y| y + 15)
    }

    /// Reference to a person; a living person's is reduced to their name and birth.
    pub fn person_ref(&self, xref: &str, this_year: i32) -> PersonRef {
        let living = self.is_living(xref, this_year);
        let ind = self.individual(xref);
        let born = match ind.as_ref().and_then(|i| i.birth()) {
            Some(b) if living => {
                [b.node.child_val("DATE").unwrap_or_default().to_string(), b.place().unwrap_or_default().to_string()]
                    .into_iter()
                    .filter(|s| !s.is_empty())
                    .collect::<Vec<_>>()
                    .join(", ")
            }
            _ => String::new(),
        };
        PersonRef {
            id: xref.to_string(),
            name: ind.as_ref().map(|i| i.display_name()).unwrap_or_default(),
            years: if living { String::new() } else { ind.map(|i| i.years()).unwrap_or_default() },
            living,
            born,
        }
    }

    fn event_info(&self, index: usize, e: &Event) -> EventInfo {
        let citations = e
            .node
            .children_with("SOUR")
            .filter_map(|s| {
                let x = s.pointer()?;
                Some(Citation {
                    source_id: x.to_string(),
                    source_title: self.record_label(x),
                    page: s.child_val("PAGE").unwrap_or("").to_string(),
                })
            })
            .collect();
        let mut media: Vec<String> = e.media_xrefs().into_iter().map(str::to_string).collect();
        for s in e.node.children_with("SOUR") {
            media.extend(s.children_with("OBJE").filter_map(|o| o.pointer()).map(str::to_string));
        }
        media.dedup();
        EventInfo {
            index,
            tag: e.tag().to_string(),
            label: e.label(),
            value: e.value().unwrap_or("").to_string(),
            date: e.date_raw().unwrap_or("").to_string(),
            place: e.place().unwrap_or("").to_string(),
            citations,
            media: media.iter().filter_map(|m| self.media_ref(m)).collect(),
        }
    }

    fn media_ref(&self, xref: &str) -> Option<MediaRef> {
        let m = Media::new(self.get(xref).filter(|n| n.tag == "OBJE")?);
        Some(MediaRef {
            id: xref.to_string(),
            title: m.title(),
            files: m.files().iter().map(|f| f.path.to_string()).collect(),
        })
    }

    /// Record of an individual. A living person's is a summary that identifies them: their birth,
    /// parents, spouses and children, without their other events or notes.
    pub fn person_sheet(&self, xref: &str, this_year: i32) -> Option<PersonSheet> {
        let ind = self.individual(xref)?;
        let living = self.is_living(xref, this_year);
        let sosa = self.sosa_numbers().get(xref).filter(|n| n.branch.is_empty()).map(|n| n.sosa);
        let (father, mother) = self.parents(xref);
        let unions = ind
            .fams()
            .iter()
            .filter_map(|f| self.family(f))
            .map(|fam| {
                let spouse = [fam.husband(), fam.wife()].into_iter().flatten().find(|s| *s != xref);
                Union {
                    family_id: fam.xref().to_string(),
                    spouse: spouse.map(|s| self.person_ref(s, this_year)),
                    events: if living {
                        Vec::new()
                    } else {
                        self.events_of(fam.xref()).iter().map(|(i, e)| self.event_info(*i, e)).collect()
                    },
                    children: fam.children().iter().map(|c| self.person_ref(c, this_year)).collect(),
                }
            })
            .collect();
        Some(PersonSheet {
            person: self.person_ref(xref, this_year),
            sex: ind.sex().to_string(),
            sosa,
            events: self
                .events_of(xref)
                .iter()
                .filter(|(_, e)| !living || e.tag() == "BIRT")
                .map(|(i, e)| self.event_info(*i, e))
                .collect(),
            father: father.map(|p| self.person_ref(p.xref(), this_year)),
            mother: mother.map(|p| self.person_ref(p.xref(), this_year)),
            unions,
            notes: if living { Vec::new() } else { ind.notes() },
        })
    }

    /// Individuals matching the query (the app's search syntax), at most `limit`.
    pub fn search_people(&self, query: &str, limit: usize, this_year: i32) -> Vec<PersonRef> {
        let ids: Vec<String> = if query.trim().is_empty() {
            self.records_of(RecordKind::Individual).filter_map(|n| n.xref.clone()).collect()
        } else {
            self.search_individuals(query)
        };
        ids.iter().take(limit).map(|x| self.person_ref(x, this_year)).collect()
    }

    /// Does the record concern a living person (an individual, or a family whose spouse is)?
    fn concerns_living(&self, xref: &str, this_year: i32) -> bool {
        match self.get(xref).map(|n| n.tag.as_str()) {
            Some("INDI") => self.is_living(xref, this_year),
            Some("FAM") => self
                .family(xref)
                .is_some_and(|f| [f.husband(), f.wife()].into_iter().flatten().any(|s| self.is_living(s, this_year))),
            _ => false,
        }
    }

    fn record_ref(&self, xref: &str, this_year: i32) -> Option<RecordRef> {
        let kind = match RecordKind::from_tag(&self.get(xref)?.tag)? {
            RecordKind::Individual => "person",
            RecordKind::Family => "family",
            RecordKind::Source => "source",
            RecordKind::Repository => "repository",
            RecordKind::Media => "media",
            RecordKind::SharedNote => "note",
            RecordKind::Submitter => "submitter",
        };
        Some(RecordRef {
            id: xref.to_string(),
            kind: kind.to_string(),
            label: self.record_label(xref),
            living: self.concerns_living(xref, this_year),
        })
    }

    /// The Data Quality issues (`issues`, from [`quality::run_with`]) for an agent: filtered, then
    /// `limit` from `offset`. Issues about living people are included, their records marked so.
    pub fn quality_report(
        &self,
        issues: &[Issue],
        filter: &QualityFilter,
        offset: usize,
        limit: usize,
        this_year: i32,
    ) -> Result<QualityReport, String> {
        if let Some(c) = filter.category.as_deref().filter(|c| Category::from_id(c).is_none()) {
            let all: Vec<&str> = Category::ALL.iter().map(|c| c.id()).collect();
            return Err(format!("unknown category `{c}`: use one of {}", all.join(", ")));
        }
        let ids = quality::issue_ids(issues);
        let mut counts: HashMap<Category, usize> = HashMap::new();
        let mut matching = Vec::new();
        for (issue, id) in issues.iter().zip(ids) {
            if issue.ignored && !filter.include_hidden {
                continue;
            }
            let category = quality::category(issue.code);
            *counts.entry(category).or_default() += 1;
            let keep = filter.category.as_deref().is_none_or(|c| c == category.id())
                && filter.code.as_deref().is_none_or(|c| c == issue.code)
                && filter.record.as_deref().is_none_or(|r| issue.xref == r || issue.related.iter().any(|x| x == r));
            if keep {
                matching.push((issue, id, category));
            }
        }
        let total = matching.len();
        let issues = matching
            .into_iter()
            .skip(offset)
            .take(limit)
            .map(|(i, id, category)| {
                let fix = quality::fix_of(i.code);
                let (related, values): (Vec<&String>, Vec<&String>) =
                    i.related.iter().partition(|x| self.get(x).is_some());
                QualityIssue {
                    id,
                    category: category.id().to_string(),
                    code: i.code.to_string(),
                    kind: code_label(i.code).into_owned(),
                    severity: match i.severity {
                        Severity::Error => "error",
                        Severity::Warning => "warning",
                        Severity::Info => "info",
                    }
                    .to_string(),
                    message: i.message.clone(),
                    record: self.record_ref(&i.xref, this_year),
                    related: related.iter().filter_map(|x| self.record_ref(x, this_year)).collect(),
                    values: values.into_iter().cloned().collect(),
                    automatic_fix: fix.is_some(),
                    batch_fix: fix.is_some_and(|f| f.batch.is_some()),
                    can_hide: self.get(&i.xref).is_some(),
                    hidden: i.ignored,
                }
            })
            .collect();
        let mut categories: Vec<(Category, usize)> = counts.into_iter().collect();
        categories.sort();
        let categories = categories
            .into_iter()
            .map(|(c, count)| QualityCount {
                category: c.id().to_string(),
                label: c.label().into_owned(),
                group: match c.group() {
                    quality::Group::Fix => "fix",
                    quality::Group::Complete => "complete",
                }
                .to_string(),
                count,
            })
            .collect();
        Ok(QualityReport { issues, total, categories })
    }

    /// Records to look for, for the direct ancestors up to generation `max_generation`: birth,
    /// marriage (held by the father) and death missing or without a source. Living people are
    /// ignored. Sorted by Sosa number.
    pub fn research_gaps(&self, max_generation: u32, this_year: i32) -> Vec<Gap> {
        let numbers = self.sosa_numbers();
        let mut ancestors: Vec<(&String, u64)> =
            numbers.iter().filter(|(_, n)| n.branch.is_empty()).map(|(x, n)| (x, n.sosa)).collect();
        ancestors.sort_by_key(|(_, k)| *k);
        let by_sosa: HashMap<u64, &String> = ancestors.iter().map(|(x, k)| (*k, *x)).collect();
        let mut out = Vec::new();
        for (xref, sosa) in ancestors {
            let generation = 64 - sosa.leading_zeros();
            if generation > max_generation || self.is_living(xref, this_year) {
                continue;
            }
            let Some(ind) = self.individual(xref) else { continue };
            let events = ind.events();
            let known_places = {
                let mut p: Vec<String> = events.iter().filter_map(|e| e.place()).map(str::to_string).collect();
                p.dedup();
                p
            };
            let gap = |event: &str, found: Option<&Event>| {
                let kind = match found {
                    None => GapKind::Missing,
                    Some(e) if !e.is_sourced() => GapKind::Unsourced,
                    Some(_) => return None,
                };
                Some(Gap {
                    person: self.person_ref(xref, this_year),
                    sosa,
                    generation,
                    event: event.to_string(),
                    kind,
                    family_id: None,
                    spouse: None,
                    date: found.and_then(|e| e.date_raw()).unwrap_or("").to_string(),
                    place: found.and_then(|e| e.place()).unwrap_or("").to_string(),
                    known_places: known_places.clone(),
                })
            };
            // A sourced baptism counts as a birth record; a burial, as a death record.
            let pick = |tags: &[&str]| {
                let all: Vec<&Event> = events.iter().filter(|e| tags.contains(&e.tag())).collect();
                all.iter().find(|e| e.is_sourced()).or(all.first()).copied()
            };
            out.extend(gap("BIRT", pick(&["BIRT", "BAPM", "CHR"])));
            // Marriage of the parents: held by the father (even Sosa), with the mother (Sosa + 1).
            if sosa % 2 == 0
                && let Some(wife) = by_sosa.get(&(sosa + 1))
            {
                let fam =
                    ind.fams().into_iter().filter_map(|f| self.family(f)).find(|f| f.wife() == Some(wife.as_str()));
                let fam_events = fam.map(|f| f.events()).unwrap_or_default();
                let marr: Vec<&Event> = fam_events.iter().filter(|e| e.tag() == "MARR").collect();
                let found = marr.iter().find(|e| e.is_sourced()).or(marr.first()).copied();
                if let Some(mut g) = gap("MARR", found) {
                    g.family_id = fam.map(|f| f.xref().to_string());
                    g.spouse = Some(self.person_ref(wife, this_year));
                    out.push(g);
                }
            }
            out.extend(gap("DEAT", pick(&DEATH_TAGS)));
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FILE: &str = "0 HEAD
1 GEDC
2 VERS 7.0
1 _GR_ROOT @I1@
0 @I1@ INDI
1 NAME Paul /MARCHAND/
1 SEX M
1 BIRT
2 DATE 1 JAN 1990
1 FAMC @F1@
0 @I2@ INDI
1 NAME Jean /MARCHAND/
1 SEX M
1 BIRT
2 DATE 3 MAR 1850
2 PLAC Rennes
2 SOUR @S1@
3 PAGE Acte n° 12
1 DEAT
2 DATE 1920
1 FAMS @F1@
1 FAMC @F2@
0 @I3@ INDI
1 NAME Marie /DURAND/
1 SEX F
1 BIRT
2 DATE 1855
2 PLAC Vitré
1 FAMS @F1@
0 @I4@ INDI
1 NAME Pierre /MARCHAND/
1 SEX M
1 FAMS @F2@
0 @F1@ FAM
1 HUSB @I2@
1 WIFE @I3@
1 CHIL @I1@
1 MARR
2 DATE 1880
0 @F2@ FAM
1 HUSB @I4@
1 CHIL @I2@
0 @S1@ SOUR
1 TITL État civil de Rennes
0 TRLR
";

    fn ged() -> Gedcom {
        Gedcom::parse(FILE).unwrap()
    }

    #[test]
    fn living_people_are_reduced_to_what_identifies_them() {
        let g = ged();
        assert!(g.is_living("I1", 2026));
        assert!(!g.is_living("I2", 2026));
        // Born in 1855, no death: more than 100 years.
        assert!(!g.is_living("I3", 2026));
        // No date, but father of a child born in 1850.
        assert!(!g.is_living("I4", 2026));
        let r = g.person_ref("I1", 2026);
        assert_eq!((r.name.as_str(), r.born.as_str()), ("Paul MARCHAND", "1 JAN 1990"), "{r:?}");
        assert!(r.living && r.years.is_empty());
        let sheet = g.person_sheet("I1", 2026).unwrap();
        assert!(sheet.events.iter().all(|e| e.tag == "BIRT") && sheet.notes.is_empty());
        assert!(g.search_people("", 10, 2026).iter().any(|p| p.id == "I1"));
        assert!(g.person_ref("I2", 2026).born.is_empty());
    }

    #[test]
    fn sheet_lists_events_citations_and_relatives() {
        let g = ged();
        let s = g.person_sheet("I2", 2026).unwrap();
        assert_eq!(s.sosa, Some(2));
        let birth = s.events.iter().find(|e| e.tag == "BIRT").unwrap();
        assert_eq!(birth.place, "Rennes");
        // NAME, SEX, then BIRT: the position designates the event in the record.
        assert_eq!(birth.index, 2);
        assert_eq!(s.unions[0].events[0].index, 3);
        assert_eq!(birth.citations[0].source_title, "État civil de Rennes");
        assert_eq!(birth.citations[0].page, "Acte n° 12");
        assert_eq!(s.father.as_ref().unwrap().id, "I4");
        assert_eq!(s.unions[0].spouse.as_ref().unwrap().name, "Marie DURAND");
        assert!(s.unions[0].children[0].living);
    }

    #[test]
    fn gaps_skip_sourced_events_and_living_people() {
        let g = ged();
        let gaps = g.research_gaps(3, 2026);
        let keys: Vec<(u64, &str, GapKind)> = gaps.iter().map(|g| (g.sosa, g.event.as_str(), g.kind)).collect();
        assert_eq!(
            keys,
            vec![
                (2, "MARR", GapKind::Unsourced),
                (2, "DEAT", GapKind::Unsourced),
                (3, "BIRT", GapKind::Unsourced),
                (3, "DEAT", GapKind::Missing),
                (4, "BIRT", GapKind::Missing),
                (4, "DEAT", GapKind::Missing),
            ]
        );
        let marr = &gaps[0];
        assert_eq!(marr.family_id.as_deref(), Some("F1"));
        assert_eq!(marr.spouse.as_ref().unwrap().id, "I3");
        assert_eq!(gaps[3].known_places, vec!["Vitré".to_string()]);
        // Generation 2 only: the root's parents.
        assert!(g.research_gaps(2, 2026).iter().all(|g| g.generation == 2));
    }

    #[test]
    fn quality_report_marks_living_people_and_pages() {
        let text = FILE
            .replace("1 DEAT\n2 DATE 1920\n", "1 DEAT\n2 DATE 1820\n")
            .replace("0 @I1@ INDI\n1 NAME Paul /MARCHAND/\n", "0 @I1@ INDI\n1 NAME Paul /MARCHAND/\n1 RESI\n");
        let g = Gedcom::parse(&text).unwrap();
        let issues = quality::run(&g);
        let all = g.quality_report(&issues, &QualityFilter::default(), 0, 500, 2026).unwrap();
        // The empty residence of the living root is listed, its record marked living.
        let resi = all.issues.iter().find(|i| i.record.as_ref().is_some_and(|r| r.id == "I1")).unwrap();
        assert!(resi.record.as_ref().unwrap().living);
        let death = all.issues.iter().find(|i| i.code == "death_before_birth").unwrap();
        assert_eq!(death.id, "death_before_birth:I2");
        assert_eq!(death.category, "dates");
        assert_eq!(death.severity, "error");
        assert_eq!(death.record.as_ref().unwrap().kind, "person");
        assert!(!death.record.as_ref().unwrap().living);
        assert!(!death.automatic_fix && death.can_hide && !death.hidden);
        // Places without coordinates: no record, the place as value.
        let coords = all.issues.iter().find(|i| i.code == "missing_coords").unwrap();
        assert!(coords.record.is_none() && !coords.can_hide);
        assert_eq!(coords.values.len(), 1);
        assert_eq!(all.total, all.issues.len());
        assert_eq!(all.categories.iter().map(|c| c.count).sum::<usize>(), all.total);

        let filter = QualityFilter { record: Some("I2".into()), ..Default::default() };
        let mine = g.quality_report(&issues, &filter, 0, 500, 2026).unwrap();
        assert!(
            mine.issues.iter().all(|i| i.record.as_ref().unwrap().id == "I2" || i.related.iter().any(|r| r.id == "I2"))
        );
        let page = g.quality_report(&issues, &QualityFilter::default(), 1, 2, 2026).unwrap();
        assert_eq!(page.issues.len(), 2.min(all.total - 1));
        assert_eq!(page.issues[0].id, all.issues[1].id);
        let bad = QualityFilter { category: Some("nope".into()), ..Default::default() };
        assert!(g.quality_report(&issues, &bad, 0, 10, 2026).unwrap_err().contains("dates"));
    }

    #[test]
    fn hidden_issues_only_on_request() {
        let text = FILE.replace("1 DEAT\n2 DATE 1920\n", "1 DEAT\n2 DATE 1820\n1 _GR_IGNORE death_before_birth\n");
        let g = Gedcom::parse(&text).unwrap();
        let issues = quality::run(&g);
        let code = QualityFilter { code: Some("death_before_birth".into()), ..Default::default() };
        assert_eq!(g.quality_report(&issues, &code, 0, 10, 2026).unwrap().total, 0);
        let with = QualityFilter { include_hidden: true, ..code };
        let r = g.quality_report(&issues, &with, 0, 10, 2026).unwrap();
        assert!(r.issues[0].hidden);
    }
}
