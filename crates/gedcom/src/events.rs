//! Modification et suppression des événements d'un individu ou d'une famille.
//!
//! Un événement est désigné par sa position parmi les sous-structures de
//! l'enregistrement (`index`), ce qui distingue deux résidences ou deux
//! professions. Seuls les champs de la fenêtre d'édition (type, précision,
//! date, lieu, première source) sont touchés : notes, médias, âge, adresse,
//! sources supplémentaires… restent en place.

use crate::document::Gedcom;
use crate::model::{Event, event_label};
use crate::node::Node;
use crate::relations::NewEvent;

/// Attributs dont la précision est la valeur de la ligne (`1 OCCU Meunier`).
/// Pour les autres balises, elle va dans `TYPE`.
fn detail_in_value(tag: &str) -> bool {
    matches!(tag, "OCCU" | "EDUC" | "RELI" | "TITL" | "NATI" | "FACT")
}

fn is_attribute(tag: &str) -> bool {
    detail_in_value(tag) || tag == "RESI"
}

impl Gedcom {
    /// Événements d'un enregistrement, avec leur position.
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

    /// Les champs modifiables d'un événement, pour pré-remplir la fenêtre d'édition.
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
        })
    }

    /// Libellé d'un événement (« Naissance », « Profession : Meunier »).
    pub fn event_title(&self, owner: &str, index: usize) -> Option<String> {
        let e = Event { node: self.event_node(owner, index)? };
        Some(match e.value() {
            Some(v) => format!("{} : {v}", e.label()),
            None => e.label(),
        })
    }

    /// Modifie un événement existant.
    pub fn update_event(&mut self, owner: &str, index: usize, ev: &NewEvent) -> Result<(), String> {
        let detail = ev.detail.trim();
        if ev.tag == "EVEN" && detail.is_empty() {
            return Err("précisez le type de l'événement".into());
        }
        if event_label(&ev.tag).is_none() {
            return Err("type d'événement inconnu".into());
        }
        let place = ev.place.trim();
        // Lieu déjà connu ailleurs : on reprend ses coordonnées.
        let known_map = self.place_map(place);
        let source = ev.source.as_deref().map(|s| s.trim_matches('@')).filter(|s| !s.is_empty());
        if let Some(s) = source
            && self.get(s).is_none()
        {
            return Err("source introuvable".into());
        }
        let node = self
            .get_mut(owner)
            .and_then(|n| n.children.get_mut(index))
            .filter(|c| event_label(&c.tag).is_some())
            .ok_or("événement introuvable")?;

        // Type et précision.
        let old_in_value = detail_in_value(&node.tag);
        if !old_in_value {
            node.children.retain(|c| c.tag != "TYPE");
        }
        node.tag = ev.tag.clone();
        if detail_in_value(&ev.tag) {
            node.value = (!detail.is_empty()).then(|| detail.to_string());
        } else {
            // La valeur d'origine (ex. « 1 RESI Paris ») est gardée, sauf si c'était la précision.
            if old_in_value || node.value.as_deref() == Some("Y") {
                node.value = None;
            }
            if !detail.is_empty() {
                // TYPE en tête, comme à l'ajout.
                node.children.insert(0, Node::with_value("TYPE", detail));
            }
        }

        // Date.
        crate::date::write_date(node, &ev.date.trim().to_uppercase(), &ev.date_phrase);

        // Lieu : si le nom change, les anciennes coordonnées ne valent plus.
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

        // Première source citée (les autres ne sont pas touchées).
        let first = node.children.iter().position(|c| c.tag == "SOUR" && c.pointer().is_some());
        match (first, source) {
            (Some(i), None) => {
                node.children.remove(i);
            }
            (Some(i), Some(s)) => {
                let cit = &mut node.children[i];
                if cit.pointer() != Some(s) {
                    // La page, la qualité… concernaient l'ancienne source.
                    *cit = Node::with_value("SOUR", format!("@{s}@"));
                }
            }
            (None, Some(s)) => node.insert_child(Node::with_value("SOUR", format!("@{s}@"))),
            (None, None) => {}
        }

        // Événement sans aucun détail : « 1 DEAT Y » (il a eu lieu).
        if !is_attribute(&ev.tag) && node.value.is_none() && node.children.is_empty() {
            node.value = Some("Y".into());
        }
        if let Some(r) = self.get_mut(owner) {
            r.touch();
        }
        Ok(())
    }

    /// Supprime un événement et renvoie son libellé.
    pub fn delete_event(&mut self, owner: &str, index: usize) -> Result<String, String> {
        let title = self.event_title(owner, index).ok_or("événement introuvable")?;
        let r = self.get_mut(owner).ok_or("enregistrement introuvable")?;
        r.children.remove(index);
        r.touch();
        Ok(title)
    }

    /// Coordonnées (`MAP`) d'un lieu, prises sur une de ses occurrences.
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
