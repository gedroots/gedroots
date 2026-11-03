//! Editing and deleting the events of an individual or a family.
//!
//! An event is designated by its position among the record's substructures (`index`), which tells
//! two residences or two occupations apart. Only the fields of the edit window (type, detail, date,
//! place, first source) are touched: notes, media, age, address, additional sources… stay in place.
//!
//! A person's record shows their events and those of their unions (marriage, divorce…) in a single
//! timeline: see [`Gedcom::person_events`]. Each event can have its media (`2 OBJE @M1@`, standard
//! GEDCOM 7).

use crate::document::Gedcom;
use crate::model::{Event, event_label};
use crate::node::Node;
use crate::relations::NewEvent;

/// Attributes whose detail is the line's value (`1 OCCU Miller`). For other tags, it goes into
/// `TYPE`.
pub(crate) fn detail_in_value(tag: &str) -> bool {
    matches!(tag, "OCCU" | "EDUC" | "RELI" | "TITL" | "NATI" | "FACT")
}

fn is_attribute(tag: &str) -> bool {
    detail_in_value(tag) || tag == "RESI"
}

/// An event in a person's timeline.
#[derive(Debug, Clone)]
pub struct PersonEvent<'a> {
    /// Record holding the event: the individual, or the family of a union.
    pub owner: String,
    /// Position of the event in `owner`.
    pub index: usize,
    pub event: Event<'a>,
    /// Event of a union (held by the family, not by the individual).
    pub family: bool,
    /// Event of a union: the spouse, if known.
    pub spouse: Option<String>,
}

impl Gedcom {
    /// Timeline of an individual: their own events and those of their unions (marriage, divorce…),
    /// sorted by date; undated events come last, in file order.
    pub fn person_events(&self, indi: &str) -> Vec<PersonEvent<'_>> {
        let Some(i) = self.individual(indi) else { return Vec::new() };
        let mut out: Vec<PersonEvent> = self
            .events_of(indi)
            .into_iter()
            .map(|(index, event)| PersonEvent { owner: indi.to_string(), index, event, spouse: None, family: false })
            .collect();
        for (fam, spouse) in i.unions(self) {
            for (index, event) in self.events_of(&fam) {
                out.push(PersonEvent { owner: fam.clone(), index, event, spouse: spouse.clone(), family: true });
            }
        }
        out.sort_by_key(|e| e.event.date().map(|d| d.sort_key()).unwrap_or((i32::MAX, 0, 0)));
        out
    }

    /// Media linked to an event.
    pub fn event_media(&self, owner: &str, index: usize) -> Vec<&str> {
        self.event_node(owner, index)
            .map(|e| e.children_with("OBJE").filter_map(|n| n.pointer()).collect())
            .unwrap_or_default()
    }

    /// Links a media to an event (`2 OBJE @M1@`). No effect if it already is.
    pub fn attach_media_to_event(&mut self, owner: &str, index: usize, media: &str) -> Result<(), String> {
        if self.get(media).is_none_or(|m| m.tag != "OBJE") {
            return Err(rust_i18n::t!("error.media_not_found").into());
        }
        let p = format!("@{}@", media.trim_matches('@'));
        let e = self
            .get_mut(owner)
            .and_then(|n| n.children.get_mut(index))
            .filter(|c| event_label(&c.tag).is_some())
            .ok_or_else(|| rust_i18n::t!("error.event_not_found"))?;
        if e.children.iter().any(|c| c.tag == "OBJE" && c.val() == p) {
            return Ok(());
        }
        // After the existing citations and media, otherwise at the end.
        let at = e
            .children
            .iter()
            .rposition(|c| matches!(c.tag.as_str(), "OBJE" | "SOUR" | "NOTE" | "SNOTE"))
            .map(|i| i + 1)
            .unwrap_or(e.children.len());
        e.children.insert(at, Node::with_value("OBJE", p));
        // “1 MARR Y”: the event now has a detail.
        if e.value.as_deref() == Some("Y") {
            e.value = None;
        }
        if let Some(r) = self.get_mut(owner) {
            r.touch();
        }
        Ok(())
    }

    /// Detaches a media from an event (the media stays in the file).
    pub fn detach_media_from_event(&mut self, owner: &str, index: usize, media: &str) -> Result<(), String> {
        let p = format!("@{}@", media.trim_matches('@'));
        let e = self
            .get_mut(owner)
            .and_then(|n| n.children.get_mut(index))
            .filter(|c| event_label(&c.tag).is_some())
            .ok_or_else(|| rust_i18n::t!("error.event_not_found"))?;
        let before = e.children.len();
        e.children.retain(|c| !(c.tag == "OBJE" && c.val() == p));
        if e.children.len() == before {
            return Ok(());
        }
        if !is_attribute(&e.tag) && e.value.is_none() && e.children.is_empty() {
            e.value = Some("Y".into());
        }
        if let Some(r) = self.get_mut(owner) {
            r.touch();
        }
        Ok(())
    }

    /// Events of a record, with their position.
    pub fn events_of(&self, owner: &str) -> Vec<(usize, Event<'_>)> {
        let Some(node) = self.get(owner) else { return Vec::new() };
        node.children
            .iter()
            .enumerate()
            .filter(|(_, c)| event_label(&c.tag).is_some())
            .map(|(i, node)| (i, Event { node }))
            .collect()
    }

    fn event_node(&self, owner: &str, index: usize) -> Option<&Node> {
        self.get(owner)?.children.get(index).filter(|c| event_label(&c.tag).is_some())
    }

    /// The editable fields of an event, to prefill the edit window.
    pub fn read_event(&self, owner: &str, index: usize) -> Option<NewEvent> {
        let e = self.event_node(owner, index)?;
        let detail = if detail_in_value(&e.tag) {
            e.value.as_deref().filter(|v| *v != "Y").unwrap_or("").to_string()
        } else {
            e.child_val("TYPE").unwrap_or("").to_string()
        };
        Some(NewEvent {
            tag: e.tag.clone(),
            detail,
            date: e.child_val("DATE").unwrap_or("").to_string(),
            date_phrase: e.child("DATE").and_then(|d| d.child_val("PHRASE")).unwrap_or("").to_string(),
            place: e.child_val("PLAC").unwrap_or("").to_string(),
            source: e.children_with("SOUR").find_map(|s| s.pointer()).map(str::to_string),
            note: e.child("NOTE").map(|n| n.text()).unwrap_or_default(),
        })
    }

    /// Label of an event (“Birth”, “Occupation: Miller”).
    pub fn event_title(&self, owner: &str, index: usize) -> Option<String> {
        let e = Event { node: self.event_node(owner, index)? };
        Some(match e.value() {
            Some(v) => format!("{} : {v}", e.label()),
            None => e.label(),
        })
    }

    /// Edits an existing event.
    pub fn update_event(&mut self, owner: &str, index: usize, ev: &NewEvent) -> Result<(), String> {
        let detail = ev.detail.trim();
        if ev.tag == "EVEN" && detail.is_empty() {
            return Err(rust_i18n::t!("error.event_type_needed").into());
        }
        if event_label(&ev.tag).is_none() {
            return Err(rust_i18n::t!("error.unknown_event_type").into());
        }
        let place = ev.place.trim();
        // Place already known elsewhere: reuse its coordinates.
        let known_map = self.place_map(place);
        let source = ev.source.as_deref().map(|s| s.trim_matches('@')).filter(|s| !s.is_empty());
        if let Some(s) = source
            && self.get(s).is_none()
        {
            return Err(rust_i18n::t!("error.source_not_found").into());
        }
        let node = self
            .get_mut(owner)
            .and_then(|n| n.children.get_mut(index))
            .filter(|c| event_label(&c.tag).is_some())
            .ok_or_else(|| rust_i18n::t!("error.event_not_found"))?;

        // Type and detail.
        let old_in_value = detail_in_value(&node.tag);
        if !old_in_value {
            node.children.retain(|c| c.tag != "TYPE");
        }
        node.tag = ev.tag.clone();
        if detail_in_value(&ev.tag) {
            node.value = (!detail.is_empty()).then(|| detail.to_string());
        } else {
            // The original value (e.g. “1 RESI Paris”) is kept, unless it was the detail.
            if old_in_value || node.value.as_deref() == Some("Y") {
                node.value = None;
            }
            if !detail.is_empty() {
                // TYPE first, as when adding.
                node.children.insert(0, Node::with_value("TYPE", detail));
            }
        }

        // Date.
        crate::date::write_date(node, &ev.date.trim().to_uppercase(), &ev.date_phrase);

        // Place: if the name changes, the old coordinates no longer apply.
        if place.is_empty() {
            node.children.retain(|c| c.tag != "PLAC");
        } else if node.child_val("PLAC") != Some(place) {
            match node.child_mut("PLAC") {
                Some(p) => {
                    p.value = Some(place.to_string());
                    p.children.retain(|c| c.tag != "MAP");
                }
                None => node.insert_child(Node::with_value("PLAC", place)),
            }
            if let (Some(map), Some(p)) = (known_map, node.child_mut("PLAC")) {
                p.children.push(map);
            }
        }

        // First cited source (the others are not touched).
        let first = node.children.iter().position(|c| c.tag == "SOUR" && c.pointer().is_some());
        match (first, source) {
            (Some(i), None) => {
                node.children.remove(i);
            }
            (Some(i), Some(s)) => {
                let cit = &mut node.children[i];
                if cit.pointer() != Some(s) {
                    // The page, the quality… were about the old source.
                    *cit = Node::with_value("SOUR", format!("@{s}@"));
                }
            }
            (None, Some(s)) => node.insert_child(Node::with_value("SOUR", format!("@{s}@"))),
            (None, None) => {}
        }

        // First note (the others are not touched); rewritten only if its text changed.
        let note = ev.note.trim_end();
        let first = node.children.iter().position(|c| c.tag == "NOTE");
        match first {
            Some(i) if note.trim().is_empty() => {
                node.children.remove(i);
            }
            Some(i) if node.children[i].text() != note => node.children[i].set_text(note),
            Some(_) => {}
            None if !note.trim().is_empty() => {
                let mut n = Node::new("NOTE");
                n.set_text(note);
                node.insert_child(n);
            }
            None => {}
        }

        // Event without any detail: “1 DEAT Y” (it happened).
        if !is_attribute(&ev.tag) && node.value.is_none() && node.children.is_empty() {
            node.value = Some("Y".into());
        }
        if let Some(r) = self.get_mut(owner) {
            r.touch();
        }
        Ok(())
    }

    /// Deletes an event and returns its label.
    pub fn delete_event(&mut self, owner: &str, index: usize) -> Result<String, String> {
        let title = self.event_title(owner, index).ok_or_else(|| rust_i18n::t!("error.event_not_found"))?;
        let r = self.get_mut(owner).ok_or_else(|| rust_i18n::t!("error.record_not_found"))?;
        r.children.remove(index);
        r.touch();
        Ok(title)
    }

    /// Coordinates (`MAP`) of a place, taken from one of its occurrences.
    fn place_map(&self, name: &str) -> Option<Node> {
        if name.is_empty() {
            return None;
        }
        let mut found = None;
        for r in &self.records {
            r.walk(&mut |n| {
                if found.is_none()
                    && n.tag == "PLAC"
                    && n.val() == name
                    && let Some(m) = n.child("MAP")
                {
                    found = Some(m.clone());
                }
            });
            if found.is_some() {
                break;
            }
        }
        found
    }
}
