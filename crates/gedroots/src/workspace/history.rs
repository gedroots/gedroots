//! Navigation history (Back / Forward) and last item viewed in each list.
//!
//! Each change of displayed item (click in the list, link from a record, place of an event…)
//! remembers the place left, whatever its type: from a place opened from an individual, “Back”
//! returns to the individual. ⌘[ / ⌘], the arrows of the top bar and the mouse’s “back / forward”
//! buttons go back there.

use super::*;
use std::collections::HashMap;

/// Taille maximale de l'historique.
const MAX: usize = 200;

/// A place in the interface: list, displayed item and central tab.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Location {
    pub view: View,
    pub key: String,
    pub tab: CenterTab,
}

#[derive(Default)]
pub(super) struct History {
    back: Vec<Location>,
    forward: Vec<Location>,
    /// Last item displayed in each list.
    last: HashMap<View, String>,
    /// Going back in progress: remember nothing.
    restoring: bool,
}

impl History {
    pub fn can_back(&self) -> bool {
        !self.back.is_empty()
    }

    pub fn can_forward(&self) -> bool {
        !self.forward.is_empty()
    }

    /// Remembers the place being left; a new navigation clears “Forward”.
    fn leave(&mut self, here: Option<Location>) {
        if self.restoring {
            return;
        }
        let Some(here) = here else { return };
        if self.back.last() != Some(&here) {
            self.back.push(here);
            if self.back.len() > MAX {
                self.back.remove(0);
            }
        }
        self.forward.clear();
    }
}

impl Workspace {
    fn here(&self) -> Option<Location> {
        Some(Location { view: self.view, key: self.selected.clone()?, tab: self.tab })
    }

    /// To call before changing the displayed item.
    pub(super) fn leave_location(&mut self) {
        let here = self.here();
        self.history.leave(here);
    }

    /// Records the displayed item as the last viewed of its list.
    pub(super) fn note_selected(&mut self, key: &str) {
        self.history.last.insert(self.view, key.to_string());
    }

    /// Forgets the history (another file opened).
    pub(super) fn reset_history(&mut self) {
        self.history = History::default();
    }

    /// Item to display when arriving in a list: the last viewed, otherwise the root individual for
    /// individuals, otherwise the first of the list.
    pub(super) fn landing(&self, view: View) -> Option<String> {
        let exists = |k: &String| self.rows.iter().any(|r| &r.key == k);
        if let Some(k) = self.history.last.get(&view).filter(|k| exists(k)) {
            return Some(k.clone());
        }
        if view == View::Individuals
            && let Some(root) = self.doc.as_ref().and_then(|d| d.ged.root()).filter(|k| exists(k))
        {
            return Some(root);
        }
        self.rows.first().map(|r| r.key.clone())
    }

    /// Name of the place “Back” (or “Forward”) leads to, for the tooltip.
    pub(super) fn next_label(&self, back: bool) -> Option<String> {
        let stack = if back { &self.history.back } else { &self.history.forward };
        let loc = stack.iter().rev().find(|l| self.still_there(l))?;
        let doc = self.doc.as_ref()?;
        Some(match loc.view {
            View::Places => short_place(&loc.key),
            View::Quality | View::Stats => loc.view.label().to_string(),
            View::Research => self.lead_title(&loc.key).unwrap_or_else(|| loc.view.label().to_string()),
            View::Books => gedroots_book::spec::book(&doc.ged, &loc.key).map(|b| self.book_title(&b))?,
            _ => doc.ged.record_label(&loc.key),
        })
    }

    /// Does the place still exist (item deleted, place renamed…)?
    fn still_there(&self, loc: &Location) -> bool {
        let Some(doc) = &self.doc else { return false };
        match loc.view {
            View::Places => doc.ged.places().iter().any(|p| p.name == loc.key),
            View::Quality => gedcom::quality::Category::from_id(&loc.key).is_some(),
            View::Stats => true,
            View::Research => self.research.leads.iter().any(|l| research::lead_key(l) == loc.key),
            _ => doc.ged.get(&loc.key).is_some(),
        }
    }

    pub(super) fn go_back(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.step(true, window, cx);
    }

    pub(super) fn go_forward(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.step(false, window, cx);
    }

    fn step(&mut self, back: bool, window: &mut Window, cx: &mut Context<Self>) {
        let target = loop {
            let next = if back { self.history.back.pop() } else { self.history.forward.pop() };
            match next {
                Some(loc) if self.still_there(&loc) => break loc,
                Some(_) => continue,
                None => return,
            }
        };
        if let Some(here) = self.here() {
            let other = if back { &mut self.history.forward } else { &mut self.history.back };
            other.push(here);
        }
        self.history.restoring = true;
        self.act_editor = None;
        self.open_location(target.view, Some(target.key), window, cx);
        self.tab = target.tab;
        self.history.restoring = false;
        cx.notify();
    }
}
