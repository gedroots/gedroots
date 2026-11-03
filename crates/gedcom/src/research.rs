//! Research data for AI agents (GEDRoots' MCP server): summarized record of an individual, search
//! and hiding of living people. The records still to find are in [`crate::leads`].
//!
//! Everything is read only: these functions never modify the document.

use crate::document::{Gedcom, RecordKind};
use crate::model::{Event, Media};

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
    /// Balise GEDCOM (`BIRT`, `MARR`, `EVEN`…).
    pub tag: String,
    pub label: String,
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "String::is_empty"))]
    pub value: String,
    /// Date GEDCOM (`14 MAR 1912`, `ABT 1850`…).
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

    fn event_info(&self, e: &Event) -> EventInfo {
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
                    events: if living { Vec::new() } else { fam.events().iter().map(|e| self.event_info(e)).collect() },
                    children: fam.children().iter().map(|c| self.person_ref(c, this_year)).collect(),
                }
            })
            .collect();
        Some(PersonSheet {
            person: self.person_ref(xref, this_year),
            sex: ind.sex().to_string(),
            sosa,
            events: ind.events().iter().filter(|e| !living || e.tag() == "BIRT").map(|e| self.event_info(e)).collect(),
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
        assert_eq!(birth.citations[0].source_title, "État civil de Rennes");
        assert_eq!(birth.citations[0].page, "Acte n° 12");
        assert_eq!(s.father.as_ref().unwrap().id, "I4");
        assert_eq!(s.unions[0].spouse.as_ref().unwrap().name, "Marie DURAND");
        assert!(s.unions[0].children[0].living);
    }
}
