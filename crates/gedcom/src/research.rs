//! Données de recherche pour les agents IA (serveur MCP de GEDRoots) : fiche résumée
//! d'un individu, recherche, lacunes des ascendants et masquage des personnes vivantes.
//!
//! Tout est en lecture seule : ces fonctions ne modifient jamais le document.

use crate::document::{Gedcom, RecordKind};
use crate::model::{Event, Media};
use std::collections::HashMap;

/// Une personne citée dans un résultat. Une personne vivante n'a ni nom ni années.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, schemars::JsonSchema))]
pub struct PersonRef {
    pub id: String,
    pub name: String,
    pub years: String,
    pub living: bool,
}

/// Source citée par un événement, avec la référence dans la source (`PAGE`).
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, schemars::JsonSchema))]
pub struct Citation {
    pub source_id: String,
    pub source_title: String,
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "String::is_empty"))]
    pub page: String,
}

/// Média lié, avec ses fichiers (chemins relatifs au `.ged` ou URL).
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, schemars::JsonSchema))]
pub struct MediaRef {
    pub id: String,
    pub title: String,
    pub files: Vec<String>,
}

/// Un événement ou attribut, tel qu'un chercheur le lit.
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

/// Une union : conjoint, événements familiaux et enfants.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, schemars::JsonSchema))]
pub struct Union {
    pub family_id: String,
    pub spouse: Option<PersonRef>,
    pub events: Vec<EventInfo>,
    pub children: Vec<PersonRef>,
}

/// Fiche d'un individu pour un agent.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, schemars::JsonSchema))]
pub struct PersonSheet {
    pub person: PersonRef,
    /// `M`, `F`, `X` ou `U`.
    pub sex: String,
    /// Numéro Sosa depuis l'individu racine, pour un ascendant direct.
    pub sosa: Option<u64>,
    pub events: Vec<EventInfo>,
    pub father: Option<PersonRef>,
    pub mother: Option<PersonRef>,
    pub unions: Vec<Union>,
    pub notes: Vec<String>,
}

/// Pourquoi un événement figure dans les lacunes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, schemars::JsonSchema))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum GapKind {
    /// L'événement n'est pas dans le fichier.
    Missing,
    /// L'événement est là, sans source.
    Unsourced,
}

/// Un acte à chercher pour un ascendant direct.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, schemars::JsonSchema))]
pub struct Gap {
    pub person: PersonRef,
    pub sosa: u64,
    /// Génération (1 : l'individu racine, 2 : ses parents…).
    pub generation: u32,
    /// `BIRT`, `MARR` ou `DEAT`.
    pub event: String,
    pub kind: GapKind,
    /// Pour un mariage : la famille et le conjoint.
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "Option::is_none"))]
    pub family_id: Option<String>,
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "Option::is_none"))]
    pub spouse: Option<PersonRef>,
    /// Date et lieu déjà connus de l'événement (lacune « sans source »).
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "String::is_empty"))]
    pub date: String,
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "String::is_empty"))]
    pub place: String,
    /// Lieux des autres événements de la personne, pour savoir où chercher.
    pub known_places: Vec<String>,
}

/// Une personne née il y a moins de ce nombre d'années, sans décès, est vivante.
pub const LIVING_YEARS: i32 = 100;

/// Année en cours (calendrier grégorien, UTC).
pub fn current_year() -> i32 {
    let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    1970 + (secs as f64 / (365.2425 * 86_400.0)) as i32
}

/// Événements qui prouvent un décès.
const DEATH_TAGS: [&str; 3] = ["DEAT", "BURI", "CREM"];

impl Gedcom {
    /// La personne est-elle peut-être vivante ? Prudent : sans décès ni date qui permette
    /// de la situer à plus de [`LIVING_YEARS`] ans, elle est considérée comme vivante.
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

    /// Année de naissance connue ou déduite : naissance ou baptême, sinon plus ancien
    /// événement daté de la personne, de ses unions ou de ses enfants (moins 15 ans),
    /// sinon naissance d'un parent (plus 15 ans).
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

    /// Référence à une personne, masquée si elle est vivante.
    pub fn person_ref(&self, xref: &str, this_year: i32) -> PersonRef {
        let living = self.is_living(xref, this_year);
        let ind = self.individual(xref);
        PersonRef {
            id: xref.to_string(),
            name: if living { "Living person".to_string() } else { ind.map(|i| i.display_name()).unwrap_or_default() },
            years: if living { String::new() } else { ind.map(|i| i.years()).unwrap_or_default() },
            living,
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

    /// Fiche d'un individu. `None` s'il n'existe pas ou s'il est vivant.
    pub fn person_sheet(&self, xref: &str, this_year: i32) -> Option<PersonSheet> {
        let ind = self.individual(xref)?;
        if self.is_living(xref, this_year) {
            return None;
        }
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
                    events: fam.events().iter().map(|e| self.event_info(e)).collect(),
                    children: fam.children().iter().map(|c| self.person_ref(c, this_year)).collect(),
                }
            })
            .collect();
        Some(PersonSheet {
            person: self.person_ref(xref, this_year),
            sex: ind.sex().to_string(),
            sosa,
            events: ind.events().iter().map(|e| self.event_info(e)).collect(),
            father: father.map(|p| self.person_ref(p.xref(), this_year)),
            mother: mother.map(|p| self.person_ref(p.xref(), this_year)),
            unions,
            notes: ind.notes(),
        })
    }

    /// Individus correspondant à la requête (syntaxe de la recherche de l'app), vivants
    /// exclus, au plus `limit`.
    pub fn search_people(&self, query: &str, limit: usize, this_year: i32) -> Vec<PersonRef> {
        let ids: Vec<String> = if query.trim().is_empty() {
            self.records_of(RecordKind::Individual).filter_map(|n| n.xref.clone()).collect()
        } else {
            self.search_individuals(query)
        };
        ids.iter()
            .filter(|x| !self.is_living(x, this_year))
            .take(limit)
            .map(|x| self.person_ref(x, this_year))
            .collect()
    }

    /// Actes à chercher pour les ascendants directs jusqu'à la génération `max_generation` :
    /// naissance, mariage (porté par le père) et décès absents ou sans source. Les
    /// personnes vivantes sont ignorées. Triés par numéro Sosa.
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
            // Un baptême sourcé vaut acte de naissance ; une inhumation, acte de décès.
            let pick = |tags: &[&str]| {
                let all: Vec<&Event> = events.iter().filter(|e| tags.contains(&e.tag())).collect();
                all.iter().find(|e| e.is_sourced()).or(all.first()).copied()
            };
            out.extend(gap("BIRT", pick(&["BIRT", "BAPM", "CHR"])));
            // Mariage des parents : porté par le père (Sosa pair), avec la mère (Sosa + 1).
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
    fn living_people_are_hidden() {
        let g = ged();
        assert!(g.is_living("I1", 2026));
        assert!(!g.is_living("I2", 2026));
        // Née en 1855, sans décès : plus de 100 ans.
        assert!(!g.is_living("I3", 2026));
        // Sans date, mais père d'un enfant né en 1850.
        assert!(!g.is_living("I4", 2026));
        assert_eq!(g.person_ref("I1", 2026).name, "Living person");
        assert!(g.person_sheet("I1", 2026).is_none());
        assert!(g.search_people("MARCHAND", 10, 2026).iter().all(|p| p.id != "I1"));
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
        // Génération 2 seulement : les parents de la racine.
        assert!(g.research_gaps(2, 2026).iter().all(|g| g.generation == 2));
    }
}
