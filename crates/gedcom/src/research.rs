//! Research data for AI agents (GEDRoots' MCP server): summarized record of an individual, search,
//! Data Quality issues. The records still to find are in
//! [`crate::leads`].
//!
//! Everything is read only: these functions never modify the document.

use crate::analysis::check::{Issue, Severity, code_label};
use crate::document::{Gedcom, RecordKind};
use crate::model::{Event, Media};
use crate::quality::{self, Category};
use std::collections::HashMap;

/// A person named in a result.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, schemars::JsonSchema))]
pub struct PersonRef {
    pub id: String,
    pub name: String,
    pub years: String,
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

/// Current year (Gregorian calendar, UTC).
pub fn current_year() -> i32 {
    let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    1970 + (secs as f64 / (365.2425 * 86_400.0)) as i32
}

impl Gedcom {
    /// Reference to a person.
    pub fn person_ref(&self, xref: &str) -> PersonRef {
        let ind = self.individual(xref);
        PersonRef {
            id: xref.to_string(),
            name: ind.as_ref().map(|i| i.display_name()).unwrap_or_default(),
            years: ind.map(|i| i.years()).unwrap_or_default(),
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

    /// Record of an individual.
    pub fn person_sheet(&self, xref: &str) -> Option<PersonSheet> {
        let ind = self.individual(xref)?;
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
                    spouse: spouse.map(|s| self.person_ref(s)),
                    events: self.events_of(fam.xref()).iter().map(|(i, e)| self.event_info(*i, e)).collect(),
                    children: fam.children().iter().map(|c| self.person_ref(c)).collect(),
                }
            })
            .collect();
        Some(PersonSheet {
            person: self.person_ref(xref),
            sex: ind.sex().to_string(),
            sosa,
            events: self.events_of(xref).iter().map(|(i, e)| self.event_info(*i, e)).collect(),
            father: father.map(|p| self.person_ref(p.xref())),
            mother: mother.map(|p| self.person_ref(p.xref())),
            unions,
            notes: ind.notes(),
        })
    }

    /// Individuals matching the query (the app's search syntax), at most `limit`.
    pub fn search_people(&self, query: &str, limit: usize) -> Vec<PersonRef> {
        let ids: Vec<String> = if query.trim().is_empty() {
            self.records_of(RecordKind::Individual).filter_map(|n| n.xref.clone()).collect()
        } else {
            let found = self.search_individuals(query);
            if found.is_empty() { self.sounds_like(query) } else { found }
        };
        ids.iter().take(limit).map(|x| self.person_ref(x)).collect()
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

    fn record_ref(&self, xref: &str) -> Option<RecordRef> {
        let kind = match RecordKind::from_tag(&self.get(xref)?.tag)? {
            RecordKind::Individual => "person",
            RecordKind::Family => "family",
            RecordKind::Source => "source",
            RecordKind::Repository => "repository",
            RecordKind::Media => "media",
            RecordKind::SharedNote => "note",
            RecordKind::Submitter => "submitter",
        };
        Some(RecordRef { id: xref.to_string(), kind: kind.to_string(), label: self.record_label(xref) })
    }

    /// The Data Quality issues (`issues`, from [`quality::run_with`]) for an agent: filtered, then
    /// `limit` from `offset`.
    pub fn quality_report(
        &self,
        issues: &[Issue],
        filter: &QualityFilter,
        offset: usize,
        limit: usize,
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
                    record: self.record_ref(&i.xref),
                    related: related.iter().filter_map(|x| self.record_ref(x)).collect(),
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
    fn a_name_spelled_otherwise_is_still_found() {
        let g = Gedcom::parse(
            "0 HEAD\n1 GEDC\n2 VERS 7.0\n0 @I1@ INDI\n1 NAME Karl /Götte/\n\
             0 @I2@ INDI\n1 NAME Annelise /Altenbucher/\n0 TRLR\n",
        )
        .unwrap();
        let ids = |q: &str| g.search_people(q, 10).into_iter().map(|p| p.id).collect::<Vec<_>>();
        assert_eq!(ids("Carl"), ["I1"]);
        assert_eq!(ids("Anneliese Altenbucher"), ["I2"]);
        assert!(ids("Durand").is_empty());
    }

    #[test]
    fn sheet_lists_events_citations_and_relatives() {
        let g = ged();
        let s = g.person_sheet("I2").unwrap();
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
    }

    #[test]
    fn quality_report_lists_issues_and_pages() {
        let text = FILE
            .replace("1 DEAT\n2 DATE 1920\n", "1 DEAT\n2 DATE 1820\n")
            .replace("0 @I1@ INDI\n1 NAME Paul /MARCHAND/\n", "0 @I1@ INDI\n1 NAME Paul /MARCHAND/\n1 RESI\n");
        let g = Gedcom::parse(&text).unwrap();
        let issues = quality::run(&g);
        let all = g.quality_report(&issues, &QualityFilter::default(), 0, 500).unwrap();
        // The empty residence of the root, born in 1990, is listed like any other.
        assert!(all.issues.iter().any(|i| i.record.as_ref().is_some_and(|r| r.id == "I1")));
        let death = all.issues.iter().find(|i| i.code == "death_before_birth").unwrap();
        assert_eq!(death.id, "death_before_birth:I2");
        assert_eq!(death.category, "dates");
        assert_eq!(death.severity, "error");
        assert_eq!(death.record.as_ref().unwrap().kind, "person");
        assert!(!death.automatic_fix && death.can_hide && !death.hidden);
        assert_eq!(death.edits, ["set_event", "delete_event"]);
        // Places without coordinates: no record, the place as value.
        let coords = all.issues.iter().find(|i| i.code == "missing_coords").unwrap();
        assert!(coords.record.is_none() && !coords.can_hide);
        assert_eq!(coords.values.len(), 1);
        assert_eq!(all.total, all.issues.len());
        assert_eq!(all.categories.iter().map(|c| c.count).sum::<usize>(), all.total);

        let filter = QualityFilter { record: Some("I2".into()), ..Default::default() };
        let mine = g.quality_report(&issues, &filter, 0, 500).unwrap();
        assert!(
            mine.issues.iter().all(|i| i.record.as_ref().unwrap().id == "I2" || i.related.iter().any(|r| r.id == "I2"))
        );
        let page = g.quality_report(&issues, &QualityFilter::default(), 1, 2).unwrap();
        assert_eq!(page.issues.len(), 2.min(all.total - 1));
        assert_eq!(page.issues[0].id, all.issues[1].id);
        let bad = QualityFilter { category: Some("nope".into()), ..Default::default() };
        assert!(g.quality_report(&issues, &bad, 0, 10).unwrap_err().contains("dates"));
    }

    #[test]
    fn hidden_issues_only_on_request() {
        let text = FILE.replace("1 DEAT\n2 DATE 1920\n", "1 DEAT\n2 DATE 1820\n1 _GR_IGNORE death_before_birth\n");
        let g = Gedcom::parse(&text).unwrap();
        let issues = quality::run(&g);
        let code = QualityFilter { code: Some("death_before_birth".into()), ..Default::default() };
        assert_eq!(g.quality_report(&issues, &code, 0, 10).unwrap().total, 0);
        let with = QualityFilter { include_hidden: true, ..code };
        let r = g.quality_report(&issues, &with, 0, 10).unwrap();
        assert!(r.issues[0].hidden);
    }
}
