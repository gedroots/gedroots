//! Associations (`ASSO`): people who took part in an event without being its principals —
//! witnesses, godparents, informants, officiants…
//!
//! In GEDCOM 7 they are written on the event of the principal person (`1 MARR` / `2 ASSO @I5@` /
//! `3 ROLE WITN`). A person's timeline shows both sides: the associates of their own events, and
//! the events of other people they attended ([`Gedcom::attended_events`]).

use crate::act::association_role;
use crate::document::Gedcom;
use crate::model::{Event, event_label};
use crate::node::Node;

/// A person associated with an event, with their role in words.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Associate {
    pub person: String,
    /// “Witness”, “Godfather”, “neighbour”…
    pub role: String,
    /// Rest of the role's phrase: “uncle of the bride”.
    pub detail: Option<String>,
}

/// An event of another record that a person attended.
#[derive(Debug, Clone)]
pub struct AttendedEvent<'a> {
    /// Record holding the event: an individual or a family.
    pub owner: String,
    /// Position of the event in `owner`.
    pub index: usize,
    pub event: Event<'a>,
    /// Role of the person, in words.
    pub role: String,
    /// Rest of the role's phrase.
    pub detail: Option<String>,
    /// Principal people of the event: the individual, or the spouses of the family.
    pub principals: Vec<String>,
}

impl Gedcom {
    /// People associated with an event (`ASSO` pointing to an individual of the file).
    pub fn event_associates(&self, owner: &str, index: usize) -> Vec<Associate> {
        let Some(e) = self.get(owner).and_then(|n| n.children.get(index)) else { return Vec::new() };
        e.children_with("ASSO")
            .filter_map(|a| {
                let person = a.pointer()?;
                self.individual(person)?;
                let (role, detail) = self.role_of(a, person);
                Some(Associate { person: person.to_string(), role, detail })
            })
            .collect()
    }

    /// Events of other records where `indi` is associated (witness, godparent…), sorted by date.
    pub fn attended_events(&self, indi: &str) -> Vec<AttendedEvent<'_>> {
        let mut out = Vec::new();
        for record in self.referrers(indi) {
            let Some(owner) = record.xref.as_deref() else { continue };
            if owner == indi || !matches!(record.tag.as_str(), "INDI" | "FAM") {
                continue;
            }
            for (index, node) in record.children.iter().enumerate() {
                if event_label(&node.tag).is_none() {
                    continue;
                }
                for a in node.children_with("ASSO").filter(|a| a.pointer() == Some(indi)) {
                    let (role, detail) = self.role_of(a, indi);
                    out.push(AttendedEvent {
                        owner: owner.to_string(),
                        index,
                        event: Event { node },
                        role,
                        detail,
                        principals: self.event_principals(owner),
                    });
                }
            }
        }
        out.sort_by_key(|e| e.event.date().map(|d| d.sort_key()).unwrap_or((i32::MAX, 0, 0)));
        out
    }

    /// Principal people of the events of a record: the individual, or the spouses of a family.
    pub fn event_principals(&self, owner: &str) -> Vec<String> {
        if let Some(f) = self.family(owner) {
            return [f.husband(), f.wife()].into_iter().flatten().map(str::to_string).collect();
        }
        self.individual(owner).map(|i| vec![i.xref().to_string()]).unwrap_or_default()
    }

    fn role_of(&self, asso: &Node, person: &str) -> (String, Option<String>) {
        let role = asso.child("ROLE");
        let sex = self.individual(person).map(|i| i.sex().to_string()).unwrap_or_default();
        association_role(
            role.map(|r| r.val()).unwrap_or(""),
            role.and_then(|r| r.child_val("PHRASE")).unwrap_or(""),
            &sex,
        )
    }
}
