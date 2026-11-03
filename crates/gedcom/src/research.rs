//! Research data for AI agents (GEDRoots' MCP server): summarized record of an individual, search,
//! Data Quality issues and hiding of living people. The records still to find are in
//! [`crate::leads`].
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
    /// The issue as the Quality page states it, and its detail when it has one (likelihood and
    /// clues of a possible duplicate).
    pub message: String,
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "String::is_empty"))]
    pub detail: String,
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
    /// Edits `propose_change` accepts for it (`set_event`, `set_sex`…); empty: fix or hide only.
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "Vec::is_empty"))]
    pub edits: Vec<String>,
    /// The user can hide it on the Quality page, when they checked it is right.
    pub can_hide: bool,
    /// Hidden by the user.
    pub hidden: bool,
    /// For a possible duplicate: `propose_change` can merge them. Facts that contradict each
    /// other, for which `choices` must say whose value to keep.
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "Option::is_none"))]
    pub merge: Option<MergeInfo>,
}

/// What merging a possible duplicate involves.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, schemars::JsonSchema))]
pub struct MergeInfo {
    /// Person kept (the root person, otherwise the one with the most links) and person merged
    /// into it.
    pub keep: String,
    pub other: String,
    pub conflicts: Vec<MergeConflict>,
}

/// A fact the two people both have, with different values.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, schemars::JsonSchema))]
pub struct MergeConflict {
    /// `NAME`, `SEX`, `BIRT`… (the key in `choices`).
    pub fact: String,
    /// Value of `keep`, value of `other`.
    pub keep: String,
    pub other: String,
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

/// Youngest age for a union or a child.
pub const ADULT: i32 = 15;
/// Oldest age for a parent at a child's birth.
pub const MAX_PARENT_AGE: i32 = 60;
/// Largest age gap between spouses.
pub const MAX_SPOUSE_GAP: i32 = 30;
/// Generations walked around a person to place their birth.
const GENERATIONS: usize = 4;

/// Events that prove a death.
const DEATH_TAGS: [&str; 3] = ["DEAT", "BURI", "CREM"];

impl Gedcom {
    /// Could the person be living? Cautious: without a death, or a sign that they were born more
    /// than [`LIVING_YEARS`] years ago ([`Gedcom::latest_birth_year`]), they are considered living.
    pub fn is_living(&self, xref: &str, this_year: i32) -> bool {
        let Some(ind) = self.individual(xref) else { return false };
        if ind.events().iter().any(|e| DEATH_TAGS.contains(&e.tag())) {
            return false;
        }
        let placed = |y: Option<i32>| y.is_some_and(|y| y <= this_year - LIVING_YEARS);
        if placed(self.latest_birth(xref, GENERATIONS, &mut std::collections::HashSet::new())) {
            return false;
        }
        // The acts where they are named (a whole-file search) only matter without a dated birth.
        let born = ind.events().iter().any(|e| matches!(e.tag(), "BIRT" | "BAPM" | "CHR") && e.date().is_some());
        born || !placed(self.named_in_acts(xref))
    }

    /// Latest year the person can have been born in, from what the tree says of them and their
    /// family, a few generations around: their birth; any dated event of theirs or citation of
    /// an act about them (an occupation [`ADULT`] years earlier); their unions and children
    /// ([`ADULT`] years earlier); the acts where they are a witness or godparent (an adult); their parents (dead, or born at most [`MAX_PARENT_AGE`]
    /// years earlier) and spouses ([`MAX_SPOUSE_GAP`]). A 19th-century ancestor known only
    /// through their grandchildren is placed too.
    pub fn latest_birth_year(&self, xref: &str) -> Option<i32> {
        let family = self.latest_birth(xref, GENERATIONS, &mut std::collections::HashSet::new());
        [family, self.named_in_acts(xref)].into_iter().flatten().min()
    }

    /// Earliest dated event where the person is associated: they were born by then, and
    /// [`ADULT`] years earlier for a witness or a godparent.
    fn named_in_acts(&self, xref: &str) -> Option<i32> {
        self.referrers(xref)
            .into_iter()
            .flat_map(|r| &r.children)
            .filter_map(|ev| {
                let asso = ev.children_with("ASSO").find(|a| a.pointer() == Some(xref))?;
                let year = ev.child_val("DATE").and_then(|d| crate::Date::parse(d).latest_year())?;
                // A witness or a godparent is an adult.
                let adult = matches!(asso.child_val("ROLE"), Some("WITN" | "GODP"));
                Some(if adult { year - ADULT } else { year })
            })
            .min()
    }

    fn latest_birth(&self, xref: &str, depth: usize, seen: &mut std::collections::HashSet<String>) -> Option<i32> {
        if !seen.insert(xref.to_string()) {
            return None;
        }
        let ind = self.individual(xref)?;
        let year = |e: &Event| e.date().and_then(|d| d.latest_year());
        if let Some(y) =
            ind.events().iter().filter(|e| matches!(e.tag(), "BIRT" | "BAPM" | "CHR")).filter_map(year).min()
        {
            return Some(y);
        }
        let mut best: Option<i32> = None;
        let mut take = |y: Option<i32>| {
            if let Some(y) = y {
                best = Some(best.map_or(y, |b| b.min(y)));
            }
        };
        // Their own events, dated or cited from a dated act; an occupation is an adult's.
        for e in ind.events() {
            let age = if e.tag() == "OCCU" { ADULT } else { 0 };
            take(year(&e).map(|y| y - age));
            for c in e.node.children_with("SOUR") {
                let cited = c.child("DATA").and_then(|d| d.child_val("DATE"));
                take(cited.and_then(|d| crate::Date::parse(d).latest_year()).map(|y| y - age));
            }
        }
        for fam in ind.fams().iter().filter_map(|f| self.family(f)) {
            for e in fam.events() {
                take(year(&e).map(|y| y - ADULT));
            }
            if depth > 0 {
                for c in fam.children() {
                    take(self.latest_birth(c, depth - 1, seen).map(|y| y - ADULT));
                }
                for sp in [fam.husband(), fam.wife()].into_iter().flatten().filter(|s| *s != xref) {
                    take(self.latest_birth(sp, depth - 1, seen).map(|y| y + MAX_SPOUSE_GAP));
                }
            }
        }
        for fam in ind.famc().iter().filter_map(|f| self.family(f)) {
            for p in [fam.husband(), fam.wife()].into_iter().flatten() {
                let Some(parent) = self.individual(p) else { continue };
                // Born before their parent's death (a year at most for a father).
                for e in parent.events().iter().filter(|e| DEATH_TAGS.contains(&e.tag())) {
                    take(year(e).map(|y| y + 1));
                }
                if depth > 0 {
                    take(self.latest_birth(p, depth - 1, seen).map(|y| y + MAX_PARENT_AGE));
                }
            }
        }
        best
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
            let found = self.search_individuals(query);
            if found.is_empty() { self.sounds_like(query) } else { found }
        };
        ids.iter().take(limit).map(|x| self.person_ref(x, this_year)).collect()
    }

    /// People whose names sound like every word of `query` (Carl finds Karl, Anneliese finds
    /// Annelise): the spelling of a name in a document is often not the tree's.
    fn sounds_like(&self, query: &str) -> Vec<String> {
        let words: Vec<&str> = query.split_whitespace().collect();
        self.records_of(RecordKind::Individual)
            .filter(|r| {
                let names: Vec<String> = crate::model::Individual::new(r)
                    .names()
                    .iter()
                    .flat_map(|n| [n.given.clone(), n.surname.clone()])
                    .collect();
                let parts: Vec<&str> = names.iter().flat_map(|n| n.split_whitespace()).collect();
                words.iter().all(|w| parts.iter().any(|p| crate::phonetic::similar(w, p)))
            })
            .filter_map(|r| r.xref.clone())
            .collect()
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
                    detail: i.detail.clone(),
                    record: self.record_ref(&i.xref, this_year),
                    related: related.iter().filter_map(|x| self.record_ref(x, this_year)).collect(),
                    values: values.into_iter().cloned().collect(),
                    automatic_fix: fix.is_some(),
                    batch_fix: fix.is_some_and(|f| f.batch.is_some()),
                    edits: crate::change::edits_for(i.code).iter().map(|e| e.to_string()).collect(),
                    can_hide: self.get(&i.xref).is_some(),
                    hidden: i.ignored,
                    merge: (i.code == "possible_duplicate")
                        .then(|| self.merge_plan(&i.xref, i.related.first()?).ok())
                        .flatten()
                        .map(|p| MergeInfo {
                            keep: p.keep,
                            other: p.other,
                            conflicts: p
                                .conflicts
                                .into_iter()
                                .map(|c| MergeConflict { fact: c.tag, keep: c.keep, other: c.other })
                                .collect(),
                        }),
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
    fn a_name_spelled_otherwise_is_still_found() {
        let g = Gedcom::parse(
            "0 HEAD\n1 GEDC\n2 VERS 7.0\n0 @I1@ INDI\n1 NAME Karl /Götte/\n\
             0 @I2@ INDI\n1 NAME Annelise /Altenbucher/\n0 TRLR\n",
        )
        .unwrap();
        let ids = |q: &str| g.search_people(q, 10, 2026).into_iter().map(|p| p.id).collect::<Vec<_>>();
        assert_eq!(ids("Carl"), ["I1"]);
        assert_eq!(ids("Anneliese Altenbucher"), ["I2"]);
        assert!(ids("Durand").is_empty());
    }

    #[test]
    fn ancestors_without_dates_are_placed_by_their_family_and_acts() {
        let g = Gedcom::parse(
            "0 HEAD\n1 GEDC\n2 VERS 7.0\n\
             0 @A@ INDI\n1 NAME Jean /BLANC/\n1 FAMS @F1@\n\
             0 @B@ INDI\n1 NAME Pierre /BLANC/\n1 FAMC @F1@\n1 FAMS @F2@\n\
             0 @C@ INDI\n1 NAME Louis /BLANC/\n1 FAMC @F2@\n1 BIRT\n2 DATE 1880\n\
             0 @F1@ FAM\n1 HUSB @A@\n1 CHIL @B@\n0 @F2@ FAM\n1 HUSB @B@\n1 CHIL @C@\n\
             0 @W@ INDI\n1 NAME Marc /ROUX/\n\
             0 @X@ INDI\n1 NAME Paul /ROUX/\n1 BAPM\n2 DATE 3 MAR 1850\n2 ASSO @W@\n3 ROLE GODP\n\
             0 @D@ INDI\n1 NAME Anne /ROUX/\n1 DEAT\n2 DATE 1890\n1 FAMS @F3@\n\
             0 @E@ INDI\n1 NAME Rose /ROUX/\n1 FAMC @F3@\n0 @F3@ FAM\n1 WIFE @D@\n1 CHIL @E@\n\
             0 @Y@ INDI\n1 NAME Lucie /ROUX/\n1 BIRT\n2 DATE 1950\n1 FAMS @F4@\n\
             0 @Z@ INDI\n1 NAME Léa /ROUX/\n1 FAMC @F4@\n0 @F4@ FAM\n1 WIFE @Y@\n1 CHIL @Z@\n\
             0 @O@ INDI\n1 NAME Max /ROUX/\n1 OCCU tisserand\n2 DATE AFT 1850\n\
             0 @J@ INDI\n1 NAME Jules /ROUX/\n1 OCCU chauffeur\n2 DATE 1929\n\
             0 @Q@ INDI\n1 NAME Ida /ROUX/\n1 RESI\n2 SOUR @S1@\n3 DATA\n4 DATE 2 JUN 1872\n\
             0 @S1@ SOUR\n1 TITL Recensement\n0 TRLR\n",
        )
        .unwrap();
        // Grandfather of a child born in 1880: placed through two generations.
        assert!(!g.is_living("A", 2026));
        assert_eq!(g.latest_birth_year("A"), Some(1880 - 2 * ADULT));
        // Godfather in an 1850 baptism.
        assert!(!g.is_living("W", 2026));
        // Daughter of a mother who died in 1890.
        assert!(!g.is_living("E", 2026));
        // Daughter of a mother born in 1950: may be living.
        assert!(g.is_living("Z", 2026));
        // An open date places nobody; a dated citation does.
        assert!(g.is_living("O", 2026));
        assert!(!g.is_living("Q", 2026));
        // A chauffeur in 1929 was an adult.
        assert!(!g.is_living("J", 2026));
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
        assert_eq!(death.edits, ["set_event", "delete_event"]);
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
