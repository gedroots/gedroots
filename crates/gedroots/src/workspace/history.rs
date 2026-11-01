//! Historique de navigation (Précédent / Suivant) et dernier élément vu dans
//! chaque liste.
//!
//! Chaque changement d'élément affiché (clic dans la liste, lien depuis une
//! fiche, lieu d'un événement…) mémorise l'endroit quitté. ⌘[ / ⌘] et les
//! boutons « précédent / suivant » de la souris y reviennent.

use super::*;
use std::collections::HashMap;

/// Taille maximale de l'historique.
const MAX: usize = 200;

/// Un endroit de l'interface : liste, élément affiché et onglet central.
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
    /// Dernier élément affiché dans chaque liste.
    last: HashMap<View, String>,
    /// Retour en arrière en cours : ne rien mémoriser.
    restoring: bool,
}

impl History {
    pub fn can_back(&self) -> bool {
        !self.back.is_empty()
    }

    pub fn can_forward(&self) -> bool {
        !self.forward.is_empty()
    }

    /// Mémorise l'endroit qu'on quitte ; une nouvelle navigation efface « Suivant ».
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

    /// À appeler avant de changer l'élément affiché.
    pub(super) fn leave_location(&mut self) {
        let here = self.here();
        self.history.leave(here);
    }

    /// Note l'élément affiché comme dernier vu de sa liste.
    pub(super) fn note_selected(&mut self, key: &str) {
        self.history.last.insert(self.view, key.to_string());
    }

    /// Oublie l'historique (autre fichier ouvert).
    pub(super) fn reset_history(&mut self) {
        self.history = History::default();
    }

    /// Élément à afficher en arrivant dans une liste : le dernier vu, sinon
    /// l'individu racine pour les individus, sinon le premier de la liste.
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

    /// L'endroit existe-t-il encore (élément supprimé, lieu renommé…) ?
    fn still_there(&self, loc: &Location) -> bool {
        let Some(doc) = &self.doc else { return false };
        match loc.view {
            View::Places => doc.ged.places().iter().any(|p| p.name == loc.key),
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
        self.entry_form = None;
        self.open_location(target.view, Some(target.key), window, cx);
        self.tab = target.tab;
        self.history.restoring = false;
        cx.notify();
    }
}
