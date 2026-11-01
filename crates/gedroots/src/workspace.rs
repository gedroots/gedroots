//! Fenêtre principale : barre d'activité, liste, zone centrale (arbre, fiche,
//! GEDCOM brut), panneau de détail et barre d'état — d'après la maquette.

use crate::commands::*;
use crate::editor::{FieldState, RecordEditor, Target};
use crate::entry_form::{EntryForm, EntryFormEvent};
use crate::map::{MapEvent, MapView};
use crate::theme;
use gedcom::*;
use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::input::{Editor, Input, InputEvent, InputState, Textarea};
use gpui_kit::component::menu::AppMenuBar;
use gpui_kit::component::select::Select;
use gpui_kit::component::{Disableable, Icon, Sizable, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use std::path::PathBuf;
use std::rc::Rc;

mod commands;
mod filters;
mod history;
mod photo;
mod relatives;
mod tree;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum View {
    Individuals,
    Families,
    Sources,
    Places,
    Media,
    Repositories,
    Notes,
}

impl View {
    const ALL: [View; 7] =
        [View::Individuals, View::Families, View::Sources, View::Places, View::Media, View::Repositories, View::Notes];

    fn label(self) -> &'static str {
        match self {
            View::Individuals => "Individus",
            View::Families => "Familles",
            View::Sources => "Sources",
            View::Places => "Lieux",
            View::Media => "Médias",
            View::Repositories => "Dépôts",
            View::Notes => "Notes",
        }
    }

    fn short(self) -> &'static str {
        match self {
            View::Individuals => "IND",
            View::Families => "FAM",
            View::Sources => "SRC",
            View::Places => "LIEU",
            View::Media => "MÉD",
            View::Repositories => "DÉP",
            View::Notes => "NOTE",
        }
    }

    fn icon(self) -> IconName {
        match self {
            View::Individuals => IconName::Users,
            View::Families => IconName::Heart,
            View::Sources => IconName::BookOpen,
            View::Places => IconName::MapPin,
            View::Media => IconName::Image,
            View::Repositories => IconName::Landmark,
            View::Notes => IconName::StickyNote,
        }
    }

    fn kind(self) -> Option<RecordKind> {
        Some(match self {
            View::Individuals => RecordKind::Individual,
            View::Families => RecordKind::Family,
            View::Sources => RecordKind::Source,
            View::Media => RecordKind::Media,
            View::Repositories => RecordKind::Repository,
            View::Notes => RecordKind::SharedNote,
            View::Places => return None,
        })
    }

    fn for_kind(kind: RecordKind) -> View {
        match kind {
            RecordKind::Individual => View::Individuals,
            RecordKind::Family => View::Families,
            RecordKind::Source => View::Sources,
            RecordKind::Media => View::Media,
            RecordKind::Repository => View::Repositories,
            RecordKind::SharedNote | RecordKind::Submitter => View::Notes,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CenterTab {
    Tree,
    Map,
    Form,
    Raw,
}

impl CenterTab {
    fn label(self) -> &'static str {
        match self {
            CenterTab::Tree => "Arbre",
            CenterTab::Map => "Carte",
            CenterTab::Form => "Fiche",
            CenterTab::Raw => "GEDCOM",
        }
    }

    fn icon(self) -> IconName {
        match self {
            CenterTab::Tree => IconName::GitFork,
            CenterTab::Map => IconName::Map,
            CenterTab::Form => IconName::Pencil,
            CenterTab::Raw => IconName::Code,
        }
    }
}

#[derive(Clone)]
struct Row {
    key: String,
    title: String,
    /// Partie mise en gras (nom de famille).
    strong: String,
    meta: String,
    dot: Option<Hsla>,
    flagged: bool,
}

struct Document {
    ged: Gedcom,
    path: Option<PathBuf>,
    dirty: bool,
    dirty_before_checkpoint: bool,
    history: Vec<Gedcom>,
}

pub struct Workspace {
    focus: FocusHandle,
    doc: Option<Document>,
    view: View,
    tab: CenterTab,
    filter: Entity<InputState>,
    only_flagged: bool,
    rows: Rc<Vec<Row>>,
    selected: Option<String>,
    editor: Option<RecordEditor>,
    status: Option<(SharedString, bool)>,
    map: Entity<MapView>,
    tree_mode: tree::TreeMode,
    tree_gens: u32,
    picker: Option<relatives::Picker>,
    /// Fichiers image/PDF du dossier du `.ged`, pour les suggestions de rattachement.
    media_files: Rc<Vec<String>>,
    /// Suppression en attente de confirmation (clé de l'élément).
    pending_delete: Option<String>,
    /// Aide de la recherche avancée dépliée.
    search_help: bool,
    /// Saisie guidée d'un acte en cours (remplace liste, fiche et détail).
    entry_form: Option<(Entity<EntryForm>, Subscription)>,
    /// Palette de commandes ouverte.
    palette: Option<commands::Palette>,
    /// Barre de menus dans la fenêtre (Linux, Windows) ; native sous macOS.
    menu_bar: Option<Entity<AppMenuBar>>,
    /// Titre de la fenêtre déjà appliqué.
    window_title: String,
    /// Cadre en cours de tracé sur une photo.
    photo_draw: Option<photo::PhotoDraw>,
    photo_bounds: photo::PhotoBounds,
    /// Personne survolée sur la photo ou dans la liste des liens.
    photo_hover: Option<String>,
    /// Précédent / Suivant, et dernier élément vu dans chaque liste.
    history: history::History,
    /// Fenêtre « Fichiers du dossier sans média » ouverte.
    orphans_open: bool,
    /// Boutons « Détacher » dépliés dans le panneau d'un individu.
    show_detach: bool,
    /// Défilement de la liste, pour garder l'élément affiché visible.
    list_scroll: UniformListScrollHandle,
    _subs: Vec<Subscription>,
}

impl Workspace {
    pub fn new(path: Option<PathBuf>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let filter = cx.new(|cx| InputState::new(window, cx).placeholder("Rechercher un nom, un lieu…"));
        let sub = cx.subscribe_in(&filter, window, |this, _, ev: &InputEvent, _, cx| {
            if matches!(ev, InputEvent::Change) {
                this.rebuild_rows(cx);
                cx.notify();
            }
        });
        let map = cx.new(|_| MapView::new());
        let map_sub = cx.subscribe_in(&map, window, Self::on_map_event);
        let mut this = Self {
            focus: cx.focus_handle(),
            doc: None,
            view: View::Individuals,
            tab: CenterTab::Tree,
            filter,
            only_flagged: false,
            rows: Rc::new(Vec::new()),
            selected: None,
            editor: None,
            status: None,
            map,
            tree_mode: tree::TreeMode::Ancestors,
            tree_gens: 4,
            picker: None,
            media_files: Rc::new(Vec::new()),
            pending_delete: None,
            search_help: false,
            entry_form: None,
            palette: None,
            menu_bar: (!cfg!(target_os = "macos")).then(|| AppMenuBar::new(cx)),
            window_title: String::new(),
            photo_draw: None,
            photo_bounds: Default::default(),
            photo_hover: None,
            history: Default::default(),
            orphans_open: false,
            show_detach: false,
            list_scroll: UniformListScrollHandle::new(),
            _subs: vec![sub, map_sub],
        };
        if let Some(p) = path {
            this.load(p, window, cx);
        }
        this
    }

    pub fn focus_handle(&self) -> &FocusHandle {
        &self.focus
    }

    // ------------------------------------------------------------------
    // Fichier
    // ------------------------------------------------------------------

    fn load(&mut self, path: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        match gedcom::load(&path) {
            Ok(ged) => {
                let first = ged.root();
                let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                self.doc = Some(Document {
                    ged,
                    path: Some(path),
                    dirty: false,
                    dirty_before_checkpoint: false,
                    history: Vec::new(),
                });
                self.view = View::Individuals;
                self.selected = None;
                self.reset_history();
                self.rescan_media();
                self.rebuild_rows(cx);
                if let Some(x) = first {
                    self.select(x, window, cx);
                }
                self.set_status(format!("{name} ouvert"), false);
            }
            Err(e) => self.set_status(format!("Impossible d'ouvrir {} : {e}", path.display()), true),
        }
        cx.notify();
    }

    fn new_file(&mut self, _: &NewFile, window: &mut Window, cx: &mut Context<Self>) {
        let mut ged = Gedcom::new();
        let x = ged.create(RecordKind::Individual);
        self.doc = Some(Document { ged, path: None, dirty: true, dirty_before_checkpoint: true, history: Vec::new() });
        self.view = View::Individuals;
        self.selected = None;
        self.reset_history();
        self.rebuild_rows(cx);
        self.select(x, window, cx);
        self.tab = CenterTab::Form;
        cx.notify();
    }

    fn open_file(&mut self, _: &OpenFile, window: &mut Window, cx: &mut Context<Self>) {
        let rx = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Ouvrir un fichier GEDCOM".into()),
        });
        cx.spawn_in(window, async move |this, cx| {
            if let Ok(Ok(Some(mut paths))) = rx.await
                && let Some(p) = paths.pop()
            {
                let _ = this.update_in(cx, |this, window, cx| this.load(p, window, cx));
            }
        })
        .detach();
    }

    fn save_file(&mut self, _: &SaveFile, window: &mut Window, cx: &mut Context<Self>) {
        let Some(doc) = &self.doc else { return };
        match doc.path.clone() {
            Some(p) => self.write_to(p, cx),
            None => {
                let dir = dirs_home();
                let rx = cx.prompt_for_new_path(&dir, Some("arbre.ged"));
                cx.spawn_in(window, async move |this, cx| {
                    if let Ok(Ok(Some(p))) = rx.await {
                        let _ = this.update_in(cx, |this, _, cx| this.write_to(p, cx));
                    }
                })
                .detach();
            }
        }
    }

    fn write_to(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        let Some(doc) = &mut self.doc else { return };
        match gedcom::save(&doc.ged, &path) {
            Ok(()) => {
                doc.path = Some(path.clone());
                doc.dirty = false;
                self.rescan_media();
                self.set_status(format!("Enregistré dans {}", path.display()), false);
            }
            Err(e) => self.set_status(format!("Échec de l'enregistrement : {e}"), true),
        }
        cx.notify();
    }

    fn undo(&mut self, _: &Undo, window: &mut Window, cx: &mut Context<Self>) {
        let Some(doc) = &mut self.doc else { return };
        if let Some(prev) = doc.history.pop() {
            doc.ged = prev;
            doc.dirty = true;
            self.rebuild_rows(cx);
            self.refresh_editor(window, cx);
            self.set_status("Modification annulée", false);
        }
        cx.notify();
    }

    fn checkpoint(&mut self) {
        if let Some(doc) = &mut self.doc {
            doc.history.push(doc.ged.clone());
            doc.dirty_before_checkpoint = doc.dirty;
            if doc.history.len() > 50 {
                doc.history.remove(0);
            }
            doc.dirty = true;
        }
    }

    /// Annule le dernier point de restauration (édition refusée ou sans effet).
    fn drop_checkpoint(&mut self) {
        if let Some(doc) = &mut self.doc {
            doc.history.pop();
            doc.dirty = doc.dirty_before_checkpoint;
        }
    }

    fn set_status(&mut self, msg: impl Into<SharedString>, error: bool) {
        self.status = Some((msg.into(), error));
    }

    // ------------------------------------------------------------------
    // Navigation
    // ------------------------------------------------------------------

    /// Passe dans une liste, sur le dernier élément vu (ou la racine pour les individus).
    fn set_view(&mut self, view: View, window: &mut Window, cx: &mut Context<Self>) {
        if self.view != view {
            self.open_location(view, None, window, cx);
        }
    }

    /// Affiche une liste et un élément (`None` : le dernier vu dans cette liste).
    fn open_location(&mut self, view: View, key: Option<String>, window: &mut Window, cx: &mut Context<Self>) {
        if self.view == view {
            if let Some(k) = key {
                self.select(k, window, cx);
            }
            return;
        }
        self.leave_location();
        self.view = view;
        self.only_flagged = false;
        self.selected = None;
        self.editor = None;
        self.filter.update(cx, |f, cx| f.set_value("", window, cx));
        self.tab = match view {
            View::Individuals => CenterTab::Tree,
            View::Places => CenterTab::Map,
            _ => CenterTab::Form,
        };
        self.rebuild_rows(cx);
        if let Some(k) = key.or_else(|| self.landing(view)) {
            self.select(k, window, cx);
        }
        cx.notify();
    }

    fn select(&mut self, key: String, window: &mut Window, cx: &mut Context<Self>) {
        if self.selected.as_deref() != Some(key.as_str()) {
            self.leave_location();
        }
        self.note_selected(&key);
        if self.view == View::Places {
            self.map.update(cx, |m, cx| m.set_selected(Some(key.clone()), cx));
        }
        if let Some(ix) = self.rows.iter().position(|r| r.key == key) {
            self.list_scroll.scroll_to_item(ix, ScrollStrategy::Center);
        }
        self.selected = Some(key);
        self.pending_delete = None;
        self.show_detach = false;
        self.photo_draw = None;
        self.photo_hover = None;
        self.refresh_editor(window, cx);
        cx.notify();
    }

    /// Ouvre un enregistrement quelconque (lien depuis une fiche).
    pub(crate) fn goto(&mut self, xref: String, window: &mut Window, cx: &mut Context<Self>) {
        let Some(kind) = self.doc.as_ref().and_then(|d| d.ged.get(&xref)).and_then(|n| RecordKind::from_tag(&n.tag))
        else {
            return;
        };
        self.open_location(View::for_kind(kind), Some(xref), window, cx);
    }

    fn goto_place(&mut self, name: String, window: &mut Window, cx: &mut Context<Self>) {
        self.open_location(View::Places, Some(name), window, cx);
    }

    /// Clic sur un repère, ou coordonnées trouvées / posées sur la carte.
    fn on_map_event(&mut self, _: &Entity<MapView>, ev: &MapEvent, window: &mut Window, cx: &mut Context<Self>) {
        match ev {
            MapEvent::Select(name) => self.select(name.clone(), window, cx),
            MapEvent::SetCoords { name, lat, lon, checkpoint } => {
                if *checkpoint {
                    self.checkpoint();
                }
                let Some(doc) = &mut self.doc else { return };
                doc.dirty = true;
                let (la, lo) = (geo::format_latitude(*lat), geo::format_longitude(*lon));
                let n = doc.ged.update_place(name, name, &la, &lo);
                self.rebuild_rows(cx);
                if self.selected.as_deref() == Some(name.as_str()) {
                    self.refresh_editor(window, cx);
                }
                self.set_status(format!("{} : {la} {lo} ({n} occurrence(s))", short_place(name)), false);
                cx.notify();
            }
        }
    }

    /// Revient à l'individu racine, dans l'arbre.
    fn goto_root(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(root) = self.doc.as_ref().and_then(|d| d.ged.root()) else { return };
        self.open_location(View::Individuals, Some(root), window, cx);
        self.tab = CenterTab::Tree;
    }

    /// Enregistre l'individu racine dans l'en-tête du fichier (extension `_GR_ROOT`).
    fn set_root(&mut self, xref: String, cx: &mut Context<Self>) {
        if self.doc.as_ref().and_then(|d| d.ged.root()).as_deref() == Some(xref.as_str()) {
            return;
        }
        self.checkpoint();
        let Some(doc) = &mut self.doc else { return };
        doc.ged.set_root(&xref);
        let name = doc.ged.record_label(&xref);
        self.set_status(format!("{name} est maintenant l'individu racine"), false);
        cx.notify();
    }

    fn copy_id(&mut self, xref: &str, cx: &mut Context<Self>) {
        cx.write_to_clipboard(ClipboardItem::new_string(xref.to_string()));
        self.set_status(format!("Identifiant {xref} copié"), false);
        cx.notify();
    }

    fn refresh_editor(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let (Some(doc), Some(key)) = (&self.doc, &self.selected) else {
            self.editor = None;
            return;
        };
        let target = match self.view.kind() {
            Some(_) => match doc.ged.get(key).and_then(|n| RecordKind::from_tag(&n.tag)) {
                Some(kind) => Target::Record { xref: key.clone(), kind },
                None => {
                    self.editor = None;
                    return;
                }
            },
            None => Target::Place(key.clone()),
        };
        self.editor = Some(RecordEditor::new(&doc.ged, target, window, cx));
        // Les anciens champs disparaissent : on rend le focus à l'espace de travail
        // pour que les raccourcis clavier restent actifs.
        window.focus(&self.focus, cx);
    }

    fn create_record(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(kind) = self.view.kind() else { return };
        self.checkpoint();
        let Some(doc) = &mut self.doc else { return };
        let x = doc.ged.create(kind);
        self.filter.update(cx, |f, cx| f.set_value("", window, cx));
        self.rebuild_rows(cx);
        self.select(x.clone(), window, cx);
        self.tab = CenterTab::Form;
        self.set_status(format!("{} créé(e)", kind.label_fr()), false);
    }

    /// Premier clic : demande confirmation ; second clic : supprime.
    fn delete_selected(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(key) = self.selected.clone() else { return };
        if self.pending_delete.as_ref() != Some(&key) {
            self.pending_delete = Some(key);
            cx.notify();
            return;
        }
        self.pending_delete = None;
        self.checkpoint();
        let Some(doc) = &mut self.doc else { return };
        let msg = match self.view.kind() {
            Some(_) => {
                let label = doc.ged.record_label(&key);
                doc.ged.delete_record(&key);
                format!("« {label} » supprimé (Annuler pour revenir en arrière)")
            }
            None => {
                let n = doc.ged.delete_place(&key);
                format!(
                    "Lieu retiré de {n} événement{} (Annuler pour revenir en arrière)",
                    if n > 1 { "s" } else { "" }
                )
            }
        };
        let pos = self.rows.iter().position(|r| r.key == key).unwrap_or(0);
        self.selected = None;
        self.editor = None;
        self.rebuild_rows(cx);
        // On sélectionne l'élément voisin pour enchaîner.
        if let Some(next) = self.rows.get(pos.min(self.rows.len().saturating_sub(1))).map(|r| r.key.clone()) {
            self.select(next, window, cx);
        }
        self.set_status(msg, false);
        cx.notify();
    }

    fn apply_form(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(ed) = self.editor.take() else { return };
        self.checkpoint();
        let doc = self.doc.as_mut().unwrap();
        let result = ed.apply_form(&mut doc.ged, cx);
        let renamed = match &ed.target {
            Target::Place(_) => Some(ed.fields[0].state.value(cx).trim().to_string()),
            _ => None,
        };
        self.editor = Some(ed);
        let unchanged =
            self.doc.as_ref().is_some_and(|d| d.history.last().is_some_and(|prev| prev.records == d.ged.records));
        if result.is_ok() && unchanged {
            let doc = self.doc.as_mut().unwrap();
            let _ = doc;
            self.drop_checkpoint();
            self.set_status("Aucune modification", false);
            cx.notify();
            return;
        }
        if let (Ok(_), Target::Record { xref, .. }) = (&result, &self.editor.as_ref().unwrap().target)
            && let Some(node) = self.doc.as_mut().unwrap().ged.get_mut(xref)
        {
            node.touch();
        }
        match result {
            Ok(msg) => {
                if let Some(n) = renamed {
                    self.selected = Some(n);
                }
                self.rebuild_rows(cx);
                self.refresh_editor(window, cx);
                self.set_status(msg, false);
            }
            Err(e) => {
                self.drop_checkpoint();
                self.set_status(e, true);
            }
        }
        cx.notify();
    }

    fn apply_raw(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(mut ed) = self.editor.take() else { return };
        self.checkpoint();
        let doc = self.doc.as_mut().unwrap();
        match ed.apply_raw(&mut doc.ged, cx) {
            Ok(new_xref) => {
                self.editor = Some(ed);
                self.selected = Some(new_xref.clone());
                self.rebuild_rows(cx);
                self.refresh_editor(window, cx);
                self.set_status("GEDCOM appliqué", false);
            }
            Err(e) => {
                self.drop_checkpoint();
                ed.error = Some(e.clone().into());
                self.editor = Some(ed);
                self.set_status(format!("GEDCOM invalide : {e}"), true);
            }
        }
        cx.notify();
    }

    // ------------------------------------------------------------------
    // Saisie guidée d'un acte
    // ------------------------------------------------------------------

    fn open_entry(&mut self, _: &EnterRecord, window: &mut Window, cx: &mut Context<Self>) {
        let Some(doc) = &self.doc else { return };
        if self.entry_form.is_some() {
            return;
        }
        // Depuis la vue Sources, on part de la source sélectionnée.
        let source = self.selected.clone().filter(|_| self.view == View::Sources);
        let (ged, path) = (doc.ged.clone(), doc.path.clone());
        let form = cx.new(|cx| EntryForm::new(ged, path, source, window, cx));
        let sub = cx.subscribe_in(&form, window, |this, form, ev: &EntryFormEvent, window, cx| match ev {
            EntryFormEvent::Close => {
                this.entry_form = None;
                window.focus(&this.focus, cx);
                cx.notify();
            }
            EntryFormEvent::Save { entry, next } => this.save_entry(form.clone(), entry, *next, window, cx),
        });
        self.entry_form = Some((form, sub));
        cx.notify();
    }

    fn save_entry(
        &mut self,
        form: Entity<EntryForm>,
        entry: &gedcom::Entry,
        next: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.checkpoint();
        let Some(doc) = &mut self.doc else { return };
        match entry.apply(&mut doc.ged) {
            Ok(res) => {
                let ged = doc.ged.clone();
                let principal = res.persons.iter().flatten().next().cloned();
                self.set_status(res.summary(), false);
                self.rebuild_rows(cx);
                form.update(cx, |f, cx| f.saved(ged, &res, next, window, cx));
                if !next {
                    self.entry_form = None;
                    if let Some(x) = principal {
                        self.goto(x, window, cx);
                    }
                }
            }
            Err(e) => {
                self.drop_checkpoint();
                form.update(cx, |f, cx| f.set_error(e, cx));
            }
        }
        cx.notify();
    }

    // ------------------------------------------------------------------
    // Liste
    // ------------------------------------------------------------------

    fn rebuild_rows(&mut self, cx: &mut Context<Self>) {
        let q = self.filter.read(cx).value().to_lowercase();
        let Some(doc) = &self.doc else {
            self.rows = Rc::new(Vec::new());
            return;
        };
        let g = &doc.ged;
        let mut rows: Vec<Row> = match self.view {
            View::Individuals => {
                let mut v: Vec<Row> = g
                    .records_of(RecordKind::Individual)
                    .map(|r| {
                        let i = Individual::new(r);
                        let (s, t) = i.sourced_ratio();
                        let place = i.birth().and_then(|e| e.place().map(short_place)).unwrap_or_default();
                        Row {
                            key: i.xref().to_string(),
                            strong: i.surname(),
                            title: i.given(),
                            meta: i.years(),
                            dot: i.sosa().and_then(|s| s.parse().ok()).map(theme::lineage),
                            flagged: t > 0 && s < t,
                            // le lieu sert au filtre
                        }
                        .with_search(place)
                    })
                    .collect();
                v.sort_by_cached_key(|r| (sort_key(&r.strong), sort_key(&r.title)));
                v
            }
            View::Families => g
                .records_of(RecordKind::Family)
                .map(|r| {
                    let f = Family::new(r);
                    let x = f.xref().to_string();
                    Row {
                        title: g.record_label(&x),
                        strong: String::new(),
                        meta: f.marriage().and_then(|m| m.date()).map(|d| d.short_year()).unwrap_or_default(),
                        dot: None,
                        flagged: f.marriage().is_none(),
                        key: x,
                    }
                })
                .collect(),
            View::Sources => g
                .records_of(RecordKind::Source)
                .map(|r| {
                    let s = Source::new(r);
                    Row {
                        key: s.xref().to_string(),
                        title: s.title(),
                        strong: String::new(),
                        meta: s.publication().unwrap_or_default(),
                        dot: None,
                        flagged: s.media_xrefs().is_empty(),
                    }
                })
                .collect(),
            View::Places => g
                .places()
                .into_iter()
                .map(|p| Row {
                    title: p.short(),
                    strong: String::new(),
                    meta: format!("{} ×", p.uses),
                    dot: None,
                    flagged: p.latitude.is_none(),
                    key: p.name,
                })
                .collect(),
            View::Media => g
                .records_of(RecordKind::Media)
                .map(|r| {
                    let m = Media::new(r);
                    let missing = !media_path(doc, &m).is_some_and(|p| p.exists());
                    Row {
                        key: m.xref().to_string(),
                        title: m.title(),
                        strong: String::new(),
                        meta: String::new(),
                        dot: missing.then(theme::warn),
                        flagged: missing,
                    }
                    .with_search(m.file().unwrap_or("").to_string())
                })
                .collect(),
            View::Repositories => g
                .records_of(RecordKind::Repository)
                .map(|r| {
                    let x = r.xref.clone().unwrap_or_default();
                    Row {
                        title: Repository::new(r).name().to_string(),
                        strong: String::new(),
                        meta: Repository::new(r).www().unwrap_or("").to_string(),
                        dot: None,
                        flagged: false,
                        key: x,
                    }
                })
                .collect(),
            View::Notes => g
                .records_of(RecordKind::SharedNote)
                .map(|r| {
                    let x = r.xref.clone().unwrap_or_default();
                    Row {
                        title: g.record_label(&x),
                        strong: String::new(),
                        meta: String::new(),
                        dot: None,
                        flagged: false,
                        key: x,
                    }
                })
                .collect(),
        };
        if self.view == View::Places {
            let places = g.places();
            self.map.update(cx, |m, cx| m.set_places(&places, cx));
        }
        if self.only_flagged {
            rows.retain(|r| r.flagged);
        }
        if self.view == View::Individuals {
            // Recherche avec critères (né:, lieu:, sexe:, sans:source…), voir gedcom::search.
            let query = IndividualQuery::parse(self.filter.read(cx).value().as_ref());
            if !query.is_empty() {
                let hits: std::collections::HashSet<String> = g
                    .records_of(RecordKind::Individual)
                    .map(Individual::new)
                    .filter(|i| query.matches(g, i))
                    .map(|i| i.xref().to_string())
                    .collect();
                rows.retain(|r| hits.contains(&r.key));
            }
        } else if !q.is_empty() {
            rows.retain(|r| {
                r.key.to_lowercase().contains(q.trim_matches('@'))
                    || r.title.to_lowercase().contains(&q)
                    || r.strong.to_lowercase().contains(&q)
                    || r.meta.to_lowercase().contains(&q)
                    || format!("{} {}", r.title, r.strong).to_lowercase().contains(&q)
            });
        }
        self.rows = Rc::new(rows);
    }

    fn flag_label(&self) -> &'static str {
        match self.view {
            View::Individuals => "",
            View::Families => "Sans mariage",
            View::Sources => "Sans média",
            View::Places => "Sans coordonnées",
            View::Media => "Fichier absent",
            _ => "",
        }
    }
}

impl Row {
    /// Ajoute un texte cherchable sans l'afficher (glissé dans `meta` après un séparateur invisible).
    fn with_search(mut self, extra: String) -> Self {
        if !extra.is_empty() {
            self.meta = format!("{}\u{2063}{}", self.meta, extra);
        }
        self
    }

    fn meta_display(&self) -> &str {
        self.meta.split('\u{2063}').next().unwrap_or("")
    }
}

fn dirs_home() -> PathBuf {
    std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."))
}

/// Chemin absolu d'un média (`FILE` est relatif au dossier du .ged).
fn media_path(doc: &Document, m: &Media) -> Option<PathBuf> {
    let f = m.file()?;
    if f.is_empty() || f.contains("://") {
        return None;
    }
    let p = PathBuf::from(f.strip_prefix("file://").unwrap_or(f));
    if p.is_absolute() {
        return Some(p);
    }
    Some(doc.path.as_ref()?.parent()?.join(p))
}

// ======================================================================
// Rendu
// ======================================================================

fn label_caps(s: impl Into<SharedString>) -> Div {
    div().text_xs().font_weight(FontWeight::SEMIBOLD).text_color(theme::muted()).child(s.into())
}

fn mono(s: impl Into<SharedString>) -> Div {
    div().font_family("monospace").text_xs().text_color(theme::muted()).child(s.into())
}

fn dot(color: Hsla) -> Div {
    div().size(px(7.)).flex_none().rounded_full().bg(color)
}

fn icon(name: IconName) -> Icon {
    Icon::new(name).size(px(14.))
}

/// Portrait de l'individu (premier média image lié), ou ses initiales.
fn avatar(doc: &Document, i: &Individual<'_>, size: f32, ring: Hsla) -> Div {
    let base = div()
        .size(px(size))
        .flex_none()
        .rounded_full()
        .border_2()
        .border_color(ring)
        .bg(theme::surface())
        .overflow_hidden()
        .flex()
        .items_center()
        .justify_center();
    let portrait = doc.ged.portrait(i.xref());
    match portrait.and_then(|m| media_path(doc, &m)).filter(|p| p.exists()) {
        Some(p) => {
            // Le recadrage du lien (CROP) cadre le visage sur une photo de groupe.
            let crop = portrait.and_then(|m| doc.ged.link_crop(i.xref(), m.xref()));
            base.child(photo::linked_img(p, crop, size, size, true))
        }
        None => {
            let initials: String =
                [i.given().chars().next(), i.surname().chars().next()].into_iter().flatten().collect();
            base.font_weight(FontWeight::SEMIBOLD).when(size < 32., |d| d.text_xs()).child(initials)
        }
    }
}

impl Workspace {
    fn render_header(&self) -> impl IntoElement {
        let file = self
            .doc
            .as_ref()
            .map(|d| {
                d.path
                    .as_ref()
                    .and_then(|p| p.file_name())
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| "Nouveau fichier".into())
            })
            .unwrap_or_default();
        h_flex()
            .h(px(40.))
            .flex_none()
            .px_3()
            .gap_3()
            .items_center()
            .bg(theme::panel())
            .border_b_1()
            .border_color(theme::border())
            .child(
                h_flex()
                    .gap_2()
                    .items_center()
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(Icon::new(IconName::Network).size(px(16.)).text_color(theme::accent()))
                    .child("GedRoots"),
            )
            // Linux, Windows : menus dans la fenêtre (natifs sous macOS).
            .children(self.menu_bar.clone().map(|m| div().h_full().child(m)))
            .child(div().text_color(theme::muted()).child(file))
            .child(div().flex_1())
    }

    fn render_activity_bar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let counts: Vec<usize> = View::ALL
            .iter()
            .map(|v| match (&self.doc, v.kind()) {
                (Some(d), Some(k)) => d.ged.count(k),
                (Some(d), None) => d.ged.places().len(),
                _ => 0,
            })
            .collect();
        v_flex()
            .w(px(76.))
            .flex_none()
            .py_2()
            .gap_1()
            .items_center()
            .bg(theme::panel())
            .border_r_1()
            .border_color(theme::border())
            .children(View::ALL.into_iter().zip(counts).map(|(v, n)| {
                let active = v == self.view;
                div()
                    .id(SharedString::from(format!("view-{}", v.short())))
                    .w(px(68.))
                    .py_1p5()
                    .rounded_md()
                    .cursor_pointer()
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap_0p5()
                    .when(active, |d| d.bg(theme::hover()))
                    .hover(|d| d.bg(theme::hover()))
                    .child(Icon::new(v.icon()).size(px(18.)).text_color(if active {
                        theme::accent()
                    } else {
                        theme::muted()
                    }))
                    .child(
                        div()
                            .text_xs()
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(if active { theme::text() } else { theme::muted() })
                            .child(v.label()),
                    )
                    .child(mono(n.to_string()))
                    .on_click(cx.listener(move |this, _, w, cx| this.set_view(v, w, cx)))
            }))
    }

    fn render_list(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let rows = self.rows.clone();
        let selected = self.selected.clone();
        let entity = cx.entity().downgrade();
        let flag = self.flag_label();
        let only = self.only_flagged;
        // Le nom de la liste et son total sont dans la barre de gauche : ici, seulement
        // le nombre de résultats quand une recherche ou un filtre est actif.
        let filtered = only || !self.filter.read(cx).value().trim().is_empty();
        v_flex()
            .w(px(300.))
            .flex_none()
            .bg(theme::bg())
            .border_r_1()
            .border_color(theme::border())
            .child(div().px_3().pt_3().pb_2().child(Input::new(&self.filter).small().cleanable(true)))
            .when(filtered, |d| {
                let n = rows.len();
                d.child(mono(format!("{n} résultat{}", if n > 1 { "s" } else { "" })).px_3().pb_2())
            })
            .when(self.view == View::Individuals, |d| d.child(self.render_quick_filters(cx)))
            // Filtre actif (Affichage › Filtrer la liste) : rappel, cliquable pour le retirer.
            .when(only && !flag.is_empty(), |d| {
                d.child(h_flex().px_3().pb_2().child(
                    Button::new("flag").xsmall().primary().icon(IconName::Close).label(flag).on_click(cx.listener(
                        |this, _, _, cx| {
                            this.only_flagged = false;
                            this.rebuild_rows(cx);
                            cx.notify();
                        },
                    )),
                ))
            })
            .child(
                uniform_list("records", rows.len(), move |range, _window, _cx| {
                    range
                        .map(|i| {
                            let r = &rows[i];
                            let on = selected.as_deref() == Some(r.key.as_str());
                            let key = r.key.clone();
                            let entity = entity.clone();
                            h_flex()
                                .id(i)
                                .w_full()
                                .h(px(30.))
                                .px_3()
                                .gap_2()
                                .items_center()
                                .cursor_pointer()
                                .when(on, |d| d.bg(theme::hover()))
                                .hover(|d| d.bg(theme::surface()))
                                .child(dot(r.dot.unwrap_or(theme::border_strong())))
                                .child(
                                    h_flex()
                                        .flex_1()
                                        .min_w_0()
                                        .gap_1()
                                        .overflow_hidden()
                                        .whitespace_nowrap()
                                        .text_ellipsis()
                                        .when(!r.strong.is_empty(), |d| {
                                            d.child(div().font_weight(FontWeight::SEMIBOLD).child(r.strong.clone()))
                                        })
                                        .child(r.title.clone()),
                                )
                                .child(
                                    mono(r.meta_display().to_string())
                                        .flex_none()
                                        .max_w(px(96.))
                                        .overflow_hidden()
                                        .whitespace_nowrap()
                                        .text_ellipsis(),
                                )
                                .on_click(move |_, window, cx| {
                                    let key = key.clone();
                                    let _ = entity.update(cx, |this, cx| this.select(key, window, cx));
                                })
                        })
                        .collect()
                })
                .track_scroll(&self.list_scroll)
                .flex_1()
                .text_sm(),
            )
    }

    fn render_center(&self, cx: &mut Context<Self>) -> AnyElement {
        let tabs: Vec<CenterTab> = match self.view {
            View::Individuals => vec![CenterTab::Tree, CenterTab::Form, CenterTab::Raw],
            View::Places => vec![CenterTab::Map, CenterTab::Form],
            _ => vec![CenterTab::Form, CenterTab::Raw],
        };
        let tab = if tabs.contains(&self.tab) { self.tab } else { tabs[0] };
        let body: AnyElement = match (tab, &self.editor) {
            (CenterTab::Map, _) => self.map.clone().into_any_element(),
            (_, None) => self.render_empty_center().into_any_element(),
            (CenterTab::Tree, _) => self.render_tree_tab(cx),
            (CenterTab::Form, Some(ed)) => self.render_form(ed, cx).into_any_element(),
            (CenterTab::Raw, Some(ed)) => self.render_raw(ed, cx).into_any_element(),
        };
        v_flex()
            .flex_1()
            .min_w_0()
            .bg(theme::bg())
            .child(
                h_flex().h(px(34.)).flex_none().bg(theme::panel()).border_b_1().border_color(theme::border()).children(
                    tabs.into_iter().map(|t| {
                        let active = t == tab;
                        div()
                            .id(SharedString::from(format!("tab-{}", t.label())))
                            .h_full()
                            .px_4()
                            .flex()
                            .items_center()
                            .gap_2()
                            .cursor_pointer()
                            .border_r_1()
                            .border_color(theme::border())
                            .when(active, |d| d.bg(theme::bg()).border_t_2().border_color(theme::accent()))
                            .text_color(if active { theme::text() } else { theme::muted() })
                            .child(icon(t.icon()).text_color(if active { theme::accent() } else { theme::muted() }))
                            .child(match t {
                                // Vue d'arbre courante, réglable dans Affichage › Arbre.
                                CenterTab::Tree => {
                                    let r = self.tree_mode.generations();
                                    let gens = self.tree_gens.clamp(*r.start(), *r.end());
                                    format!("{} · {gens} gén.", self.tree_mode.label())
                                }
                                _ => t.label().to_string(),
                            })
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.tab = t;
                                cx.notify();
                            }))
                    }),
                ),
            )
            .child(body)
            .into_any_element()
    }

    fn render_empty_center(&self) -> impl IntoElement {
        v_flex()
            .flex_1()
            .items_center()
            .justify_center()
            .text_color(theme::muted())
            .child("Sélectionnez un élément dans la liste")
    }

    /// Arbre d'ascendance sur 4 générations (Sosa 1 à 15).
    fn render_tree(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let (Some(doc), Some(root)) = (&self.doc, &self.selected) else { return div() };
        let g = &doc.ged;
        let anc = g.ancestry(root, 4);
        let tree_root = g.root();
        let column = |range: std::ops::Range<usize>, cx: &mut Context<Self>| {
            v_flex().flex_1().min_w(px(140.)).justify_around().gap_2().children(range.map(|k| {
                match anc[k].as_ref().and_then(|x| g.individual(x)) {
                    Some(i) => {
                        let x = i.xref().to_string();
                        let on = k == 1;
                        let is_root = tree_root.as_deref() == Some(i.xref());
                        v_flex()
                            .id(SharedString::from(format!("anc-{k}")))
                            .p_2()
                            .gap_0p5()
                            .rounded_md()
                            .border_1()
                            .border_color(if on { theme::accent() } else { theme::border() })
                            .bg(if on { theme::accent_soft() } else { theme::surface() })
                            .cursor_pointer()
                            .hover(|d| d.border_color(theme::border_strong()))
                            .child(
                                h_flex()
                                    .gap_2()
                                    .items_center()
                                    .child(avatar(doc, &i, 28., theme::lineage(k)))
                                    .child(
                                        v_flex()
                                            .flex_1()
                                            .min_w_0()
                                            .child(
                                                div()
                                                    .overflow_hidden()
                                                    .whitespace_nowrap()
                                                    .text_ellipsis()
                                                    .child(format!("{} {}", i.surname(), i.given())),
                                            )
                                            .child(mono(i.years())),
                                    )
                                    .when(is_root, |d| d.child(icon(IconName::House).text_color(theme::accent()))),
                            )
                            .on_click(cx.listener(move |this, _, w, cx| this.select(x.clone(), w, cx)))
                            .into_any_element()
                    }
                    // Branche inconnue plus haut : on n'affiche pas ses ascendants.
                    None if k > 3 && anc[k / 2].is_none() => div().h(px(36.)).into_any_element(),
                    None => div()
                        .p_2()
                        .rounded_md()
                        .border_1()
                        .border_dashed()
                        .border_color(theme::border_strong())
                        .text_color(theme::muted())
                        .text_xs()
                        .child(format!("{} inconnu(e) · {k}", if k % 2 == 0 { "Père" } else { "Mère" }))
                        .into_any_element(),
                }
            }))
        };
        div().flex_1().p_4().child(
            v_flex()
                .size_full()
                .gap_3()
                .child(
                    h_flex()
                        .gap_4()
                        .text_xs()
                        .text_color(theme::muted())
                        .child(
                            h_flex().gap_1p5().items_center().child(dot(theme::paternal())).child("Lignée paternelle"),
                        )
                        .child(
                            h_flex().gap_1p5().items_center().child(dot(theme::maternal())).child("Lignée maternelle"),
                        ),
                )
                .child(
                    h_flex()
                        .flex_1()
                        .items_stretch()
                        .gap_3()
                        .text_sm()
                        .child(column(1..2, cx))
                        .child(column(2..4, cx))
                        .child(column(4..8, cx))
                        .child(column(8..16, cx)),
                ),
        )
    }

    fn render_form(&self, ed: &RecordEditor, cx: &mut Context<Self>) -> impl IntoElement {
        let preview = self.media_preview(ed, cx);
        let confirm = self.pending_delete.is_some() && self.pending_delete == self.selected;
        div().id("form-scroll").flex_1().overflow_y_scroll().child(
            v_flex()
                .p_4()
                .gap_3()
                .when(!ed.pickers.is_empty(), |d| {
                    d.child(div().grid().grid_cols(2).gap_3().children(ed.pickers.iter().map(|p| {
                        v_flex().gap_1().child(label_caps(p.label)).child(
                            Select::new(&p.state)
                                .small()
                                .cleanable(true)
                                .placeholder(p.placeholder)
                                .search_placeholder("Rechercher…"),
                        )
                    })))
                })
                .child(div().grid().grid_cols(2).gap_3().children(ed.fields.iter().map(|f| {
                    v_flex().gap_1().child(label_caps(f.label)).child(match &f.state {
                        FieldState::Text(e) => Input::new(e).small().into_any_element(),
                        FieldState::Choice(e) => Select::new(e).small().into_any_element(),
                    })
                })))
                .when_some(ed.long_text.as_ref(), |d, (label, state)| {
                    d.child(v_flex().gap_1().child(label_caps(*label)).child(Textarea::new(state).h(px(120.))))
                })
                .child(
                    h_flex()
                        .gap_2()
                        .child(
                            Button::new("apply-form")
                                .primary()
                                .small()
                                .icon(IconName::Check)
                                .label("Appliquer")
                                .on_click(cx.listener(|this, _, w, cx| this.apply_form(w, cx))),
                        )
                        .child(Button::new("reset-form").ghost().small().label("Rétablir").on_click(cx.listener(
                            |this, _, w, cx| {
                                this.refresh_editor(w, cx);
                                cx.notify();
                            },
                        )))
                        .child(div().flex_1())
                        // Suppression : Édition › Supprimer l'élément, puis confirmation ici.
                        .when(confirm, |d| {
                            d.child(Button::new("cancel-delete").ghost().small().label("Garder").on_click(cx.listener(
                                |this, _, _, cx| {
                                    this.pending_delete = None;
                                    cx.notify();
                                },
                            )))
                            .child(
                                Button::new("delete")
                                    .danger()
                                    .small()
                                    .icon(IconName::Trash)
                                    .label(match &ed.target {
                                        Target::Place(_) => "Confirmer : retirer ce lieu partout",
                                        _ => "Confirmer la suppression",
                                    })
                                    .on_click(cx.listener(|this, _, w, cx| this.delete_selected(w, cx))),
                            )
                        }),
                )
                .when_some(preview, |d, p| d.child(p))
                .when_some(self.record_images(ed, cx), |d, p| d.child(p)),
        )
    }

    /// Images liées à une source, une famille… affichées en grand sous la fiche.
    fn record_images(&self, ed: &RecordEditor, cx: &mut Context<Self>) -> Option<AnyElement> {
        let Target::Record { xref, kind } = &ed.target else { return None };
        if !matches!(kind, RecordKind::Source | RecordKind::Family) {
            return None;
        }
        let doc = self.doc.as_ref()?;
        let images = doc.ged.images_of(xref);
        if images.is_empty() {
            return None;
        }
        Some(
            v_flex()
                .gap_2()
                .child(label_caps(format!("Images · {}", images.len())))
                .children(images.into_iter().map(|m| {
                    let target = m.xref().to_string();
                    let title = m.title();
                    let crop = doc.ged.link_crop(xref, m.xref());
                    let body = match media_path(doc, &m).filter(|p| p.exists()) {
                        Some(p) if crop.is_some() => photo::fitted_crop(p, crop, 700., 640.),
                        Some(p) => {
                            img(p).max_w_full().max_h(px(640.)).object_fit(ObjectFit::Contain).into_any_element()
                        }
                        None => div()
                            .text_color(theme::warn())
                            .child(format!("Fichier introuvable : {}", m.file().unwrap_or("")))
                            .into_any_element(),
                    };
                    v_flex()
                        .id(SharedString::from(format!("img-{target}")))
                        .gap_1()
                        .p_2()
                        .rounded_md()
                        .border_1()
                        .border_color(theme::border())
                        .bg(theme::surface())
                        .cursor_pointer()
                        .hover(|d| d.border_color(theme::border_strong()))
                        .child(body)
                        .child(div().text_xs().text_color(theme::muted()).child(title))
                        .on_click(cx.listener(move |this, _, w, cx| this.goto(target.clone(), w, cx)))
                }))
                .into_any_element(),
        )
    }

    /// Vignettes des images liées, dans le panneau de détail.
    fn render_gallery(&self, xref: &str, large: bool, cx: &mut Context<Self>) -> Option<AnyElement> {
        let doc = self.doc.as_ref()?;
        let images = doc.ged.images_of(xref);
        if images.is_empty() {
            return None;
        }
        let n = images.len();
        let tiles = images.into_iter().map(|m| {
            let target = m.xref().to_string();
            let path = media_path(doc, &m).filter(|p| p.exists());
            let tile = div()
                .id(SharedString::from(format!("thumb-{target}")))
                .rounded_md()
                .overflow_hidden()
                .border_1()
                .border_color(theme::border())
                .bg(theme::surface())
                .cursor_pointer()
                .hover(|d| d.border_color(theme::accent()))
                .on_click(cx.listener(move |this, _, w, cx| this.goto(target.clone(), w, cx)));
            let tile = if large { tile.w_full().h(px(220.)) } else { tile.h(px(96.)) };
            let crop = doc.ged.link_crop(xref, m.xref());
            match path {
                Some(p) if crop.is_some() => {
                    // Zone recadrée du lien : visage sur une photo de groupe, extrait d'un acte…
                    let (w, h) = if large { (326., 218.) } else { (104., 94.) };
                    tile.flex().justify_center().child(photo::linked_img(p, crop, w, h, !large))
                }
                Some(p) => {
                    tile.child(img(p).size_full().object_fit(if large { ObjectFit::Contain } else { ObjectFit::Cover }))
                }
                None => tile.flex().items_center().justify_center().child(
                    v_flex()
                        .items_center()
                        .gap_1()
                        .text_xs()
                        .text_color(theme::warn())
                        .child(icon(IconName::Image))
                        .child("Introuvable"),
                ),
            }
        });
        Some(
            v_flex()
                .p_4()
                .gap_2()
                .border_b_1()
                .border_color(theme::border())
                .child(
                    h_flex()
                        .gap_1p5()
                        .items_center()
                        .child(icon(IconName::Images))
                        .child(label_caps(format!("Images · {n}"))),
                )
                .child(if large {
                    v_flex().gap_2().children(tiles).into_any_element()
                } else {
                    div().grid().grid_cols(3).gap_2().children(tiles).into_any_element()
                })
                .into_any_element(),
        )
    }

    fn media_preview(&self, ed: &RecordEditor, cx: &mut Context<Self>) -> Option<AnyElement> {
        let Target::Record { xref, kind: RecordKind::Media } = &ed.target else { return None };
        let doc = self.doc.as_ref()?;
        let m = Media::new(doc.ged.get(xref)?);
        let path = media_path(doc, &m);
        let el = match path {
            Some(p) if p.exists() && m.is_image() => return Some(self.render_photo(xref, p, cx)),
            // Fichier présent mais sans aperçu (PDF…) : rien à signaler.
            Some(p) if p.exists() => return None,
            Some(_) => {
                v_flex().child(div().text_color(theme::warn()).child("Fichier introuvable dans le dossier du .ged"))
            }
            None => v_flex().child(div().text_color(theme::muted()).child("Aucun fichier local associé")),
        };
        Some(el.into_any_element())
    }

    fn render_raw(&self, ed: &RecordEditor, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(raw) = &ed.raw else { return v_flex().flex_1() };
        v_flex()
            .flex_1()
            .min_h_0()
            .p_3()
            .gap_2()
            .child(
                h_flex()
                    .gap_2()
                    .items_center()
                    .child(
                        Button::new("apply-raw")
                            .primary()
                            .small()
                            .label("Appliquer le GEDCOM")
                            .on_click(cx.listener(|this, _, w, cx| this.apply_raw(w, cx))),
                    )
                    .child(Button::new("reset-raw").ghost().small().label("Rétablir").on_click(cx.listener(
                        |this, _, w, cx| {
                            this.refresh_editor(w, cx);
                            cx.notify();
                        },
                    )))
                    .when_some(ed.error.clone(), |d, e| d.child(div().text_sm().text_color(theme::danger()).child(e))),
            )
            .child(div().flex_1().min_h_0().font_family("monospace").text_sm().child(Editor::new(raw).h_full()))
    }

    fn render_detail(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(doc) = &self.doc else { return div().into_any_element() };
        let Some(key) = &self.selected else { return div().into_any_element() };
        let g = &doc.ged;
        let panel = v_flex()
            .id("detail")
            .w(px(360.))
            .flex_none()
            .overflow_y_scroll()
            .bg(theme::panel())
            .border_l_1()
            .border_color(theme::border())
            .text_sm();
        // Déposer des photos ou documents n'importe où sur le panneau les rattache.
        let attachable = g
            .get(key)
            .and_then(|n| RecordKind::from_tag(&n.tag))
            .is_some_and(|k| matches!(k, RecordKind::Individual | RecordKind::Family | RecordKind::Source));
        let panel = if attachable {
            let (record, entity) = (key.clone(), cx.entity().downgrade());
            panel.drag_over::<ExternalPaths>(|d, _, _, _| d.bg(theme::accent_soft())).on_drop(
                move |paths: &ExternalPaths, window, cx| {
                    let record = record.clone();
                    let _ = entity.update(cx, |this, cx| {
                        let files = paths.paths().iter().map(|p| this.file_value(p)).collect();
                        this.attach_files(Some(record), files, window, cx);
                    });
                },
            )
        } else {
            panel
        };
        match self.view {
            View::Individuals => match g.individual(key) {
                Some(i) => panel
                    .child(self.render_individual_card(g, i, cx))
                    .children(self.render_actions(cx))
                    .into_any_element(),
                None => panel.into_any_element(),
            },
            View::Places => {
                let place = g.places().into_iter().find(|p| &p.name == key);
                let refs = place.as_ref().map(|p| p.records.clone()).unwrap_or_default();
                panel
                    .child(
                        v_flex()
                            .p_4()
                            .gap_1()
                            .border_b_1()
                            .border_color(theme::border())
                            .child(div().text_lg().font_weight(FontWeight::SEMIBOLD).child(short_place(key)))
                            .children(place.iter().map(|p| {
                                mono(match (&p.latitude, &p.longitude) {
                                    (Some(a), Some(b)) => format!("{a} · {b} · {} occurrence(s)", p.uses),
                                    _ => format!("Sans coordonnées · {} occurrence(s)", p.uses),
                                })
                            }))
                            .children(place.iter().map(|p| {
                                let parts = p.parts().join(" › ");
                                div().text_color(theme::muted()).child(parts)
                            })),
                    )
                    .child(self.render_links("Cité par", refs, cx))
                    .into_any_element()
            }
            _ => {
                let refs: Vec<String> = g.referrers(key).iter().filter_map(|r| r.xref.clone()).collect();
                let node = g.get(key);
                let outgoing: Vec<String> = node
                    .map(|n| {
                        let mut v: Vec<String> = Vec::new();
                        for p in n.pointers() {
                            if !v.contains(&p) && p != *key {
                                v.push(p);
                            }
                        }
                        v
                    })
                    .unwrap_or_default();
                let kind = node.and_then(|n| RecordKind::from_tag(&n.tag));
                // Famille : les membres sont déjà listés (section « Membres »).
                let (outgoing, refs): (Vec<String>, Vec<String>) = if kind == Some(RecordKind::Family) {
                    let not_person = |x: &String| g.individual(x).is_none();
                    (outgoing.into_iter().filter(not_person).collect(), refs.into_iter().filter(not_person).collect())
                } else {
                    (outgoing, refs)
                };
                let gallery = self.render_gallery(key, kind == Some(RecordKind::Source), cx);
                let shown_under_photo = kind == Some(RecordKind::Media)
                    && self.tab == CenterTab::Form
                    && self.photo_file(key).is_some()
                    && !refs.is_empty();
                let actions = self.render_actions(cx);
                let show_refs = !shown_under_photo && !refs.is_empty();
                // Rien d'autre que le titre (déjà dans la fiche) : pas de panneau, la
                // zone centrale prend la place.
                if actions.is_none() && gallery.is_none() && outgoing.is_empty() && !show_refs {
                    return div().into_any_element();
                }
                panel
                    .child(
                        h_flex()
                            .p_4()
                            .gap_2()
                            .items_start()
                            .border_b_1()
                            .border_color(theme::border())
                            .when_some(kind, |d, k| {
                                d.child(icon(View::for_kind(k).icon()).text_color(theme::accent()).mt_1())
                            })
                            .child(
                                v_flex().flex_1().min_w_0().gap_1().child(
                                    div().text_lg().font_weight(FontWeight::SEMIBOLD).child(g.record_label(key)),
                                ),
                            ),
                    )
                    .children(actions)
                    .when_some(gallery, |d, el| d.child(el))
                    .when(!outgoing.is_empty(), |d| d.child(self.render_links("Liens", outgoing, cx)))
                    // Photo : la liste « Utilisée par » est sous l'image, avec les cadres.
                    .when(show_refs, |d| d.child(self.render_links("Utilisé par", refs, cx)))
                    .into_any_element()
            }
        }
    }

    fn render_links(&self, title: &'static str, xrefs: Vec<String>, cx: &mut Context<Self>) -> impl IntoElement {
        let g = &self.doc.as_ref().unwrap().ged;
        v_flex().p_4().gap_1().child(label_caps(format!("{title} · {}", xrefs.len()))).children(
            xrefs.into_iter().take(300).map(|x| {
                let kind = g.get(&x).and_then(|n| RecordKind::from_tag(&n.tag));
                let label = g.record_label(&x);
                let target = x.clone();
                h_flex()
                    .id(SharedString::from(format!("{title}-{x}")))
                    .gap_2()
                    .px_2()
                    .py_1()
                    .rounded_sm()
                    .cursor_pointer()
                    .hover(|d| d.bg(theme::hover()))
                    .children(kind.map(|k| icon(View::for_kind(k).icon()).text_color(theme::muted())))
                    .child(div().flex_1().overflow_hidden().whitespace_nowrap().text_ellipsis().child(label))
                    .on_click(cx.listener(move |this, _, w, cx| this.goto(target.clone(), w, cx)))
            }),
        )
    }

    fn render_individual_card(&self, g: &Gedcom, i: Individual<'_>, cx: &mut Context<Self>) -> impl IntoElement {
        let (sourced, total) = i.sourced_ratio();
        let pct = if total == 0 { 0. } else { sourced as f32 / total as f32 };
        let sosa = i.sosa();
        let doc = self.doc.as_ref().unwrap();
        let is_root = g.root().as_deref() == Some(i.xref());
        let lineage =
            sosa.as_deref().and_then(|s| s.parse().ok()).map(theme::lineage).unwrap_or(theme::border_strong());
        let mut events: Vec<Event> = i.events();
        events.sort_by_key(|e| e.date().map(|d| d.sort_key()).unwrap_or((i32::MAX, 0, 0)));
        let xref = i.xref().to_string();
        let has_events = !events.is_empty();

        let header = h_flex()
            .p_4()
            .gap_3()
            .items_start()
            .border_b_1()
            .border_color(theme::border())
            .child(avatar(doc, &i, 64., lineage))
            .child(
                v_flex()
                    .flex_1()
                    .gap_1()
                    .child(
                        h_flex()
                            .gap_1()
                            .text_lg()
                            .child(div().font_weight(FontWeight::SEMIBOLD).child(i.surname()))
                            .child(i.given()),
                    )
                    .child(div().text_color(theme::muted()).child(format!(
                        "{} · {}{}",
                        i.sex_label(),
                        if i.years().is_empty() { "dates inconnues".into() } else { i.years() },
                        sosa.map(|s| format!(" · Sosa {s}")).unwrap_or_default(),
                    )))
                    .when(is_root, |d| {
                        d.child(
                            h_flex()
                                .gap_1()
                                .items_center()
                                .text_xs()
                                .text_color(theme::accent())
                                .child(icon(IconName::House))
                                .child("Individu racine"),
                        )
                    })
                    .child(
                        h_flex()
                            .gap_2()
                            .items_center()
                            .child(
                                div()
                                    .w(px(120.))
                                    .h(px(4.))
                                    .rounded_full()
                                    .bg(theme::border())
                                    .child(div().h_full().rounded_full().bg(theme::ok()).w(px(120. * pct))),
                            )
                            .child(mono(format!("{sourced}/{total} sourcés"))),
                    ),
            );

        let events_el =
            v_flex().p_4().gap_3().border_b_1().border_color(theme::border()).child(label_caps("Événements")).children(
                events.into_iter().enumerate().map(|(n, e)| {
                    let place = e.place().map(|p| p.to_string());
                    let sources: Vec<String> = e.source_xrefs().iter().map(|s| s.to_string()).collect();
                    let label = match e.value() {
                        Some(v) => format!("{} : {v}", e.label()),
                        None => e.label(),
                    };
                    v_flex()
                        .gap_0p5()
                        .child(
                            h_flex()
                                .justify_between()
                                .gap_2()
                                .child(
                                    h_flex()
                                        .gap_1p5()
                                        .items_center()
                                        .child(div().font_weight(FontWeight::MEDIUM).child(label))
                                        // Point orange : événement sans source.
                                        .when(sources.is_empty(), |d| d.child(dot(theme::warn()))),
                                )
                                .child(mono(e.date().map(|d| d.to_french()).unwrap_or_default())),
                        )
                        .when_some(place, |d, p| {
                            let target = p.clone();
                            d.child(
                                div()
                                    .id(SharedString::from(format!("ev-place-{n}")))
                                    .text_color(theme::muted())
                                    .cursor_pointer()
                                    .hover(|d| d.text_color(theme::accent()))
                                    .child(short_place(&p))
                                    .on_click(
                                        cx.listener(move |this, _, w, cx| this.goto_place(target.clone(), w, cx)),
                                    ),
                            )
                        })
                        .children(sources.into_iter().map(|s| {
                            let title = g.record_label(&s);
                            let target = s.clone();
                            div()
                                .id(SharedString::from(format!("ev-src-{n}-{s}")))
                                .text_xs()
                                .text_color(theme::ok())
                                .cursor_pointer()
                                .hover(|d| d.text_color(theme::accent()))
                                .child(title)
                                .on_click(cx.listener(move |this, _, w, cx| this.goto(target.clone(), w, cx)))
                        }))
                }),
            );

        // Famille : parents, conjoints, enfants.
        let mut family: Vec<(&'static str, String)> = Vec::new();
        let (f, m) = g.parents(&xref);
        family.extend(f.map(|p| ("Père", p.xref().to_string())));
        family.extend(m.map(|p| ("Mère", p.xref().to_string())));
        for (spouse, kids) in g.spouses_and_children(&xref) {
            family.extend(spouse.map(|s| ("Conjoint", s.xref().to_string())));
            family.extend(kids.into_iter().map(|k| ("Enfant", k.xref().to_string())));
        }
        let family_empty = family.is_empty();
        let family_el = v_flex()
            .p_4()
            .gap_1()
            .border_b_1()
            .border_color(theme::border())
            .child(label_caps("Famille"))
            .children(family.into_iter().map(|(role, x)| {
                let p = g.individual(&x).unwrap();
                let target = x.clone();
                h_flex()
                    .id(SharedString::from(format!("fam-{role}-{x}")))
                    .gap_2()
                    .px_2()
                    .py_1()
                    .rounded_sm()
                    .items_center()
                    .cursor_pointer()
                    .hover(|d| d.bg(theme::hover()))
                    .child(div().w(px(64.)).text_xs().text_color(theme::muted()).child(role))
                    .child(div().flex_1().overflow_hidden().whitespace_nowrap().text_ellipsis().child(p.display_name()))
                    .child(mono(p.years()))
                    .on_click(cx.listener(move |this, _, w, cx| this.goto(target.clone(), w, cx)))
            }));

        let images: Vec<String> = g.images_of(&xref).iter().map(|m| m.xref().to_string()).collect();
        let media: Vec<String> =
            i.media_xrefs().iter().map(|s| s.to_string()).filter(|x| !images.contains(x)).collect();
        let gallery = self.render_gallery(&xref, false, cx);
        let notes = i.notes();
        let (has_events, has_family) = (has_events, !family_empty);
        v_flex()
            .child(header)
            .when_some(gallery, |d, el| d.child(el))
            .when(has_events, |d| d.child(events_el))
            .when(has_family, |d| d.child(family_el))
            .when(!media.is_empty(), |d| d.child(self.render_links("Autres médias", media, cx)))
            .when(!notes.is_empty(), |d| {
                d.child(
                    v_flex()
                        .p_4()
                        .gap_1()
                        .child(label_caps("Notes"))
                        .children(notes.into_iter().map(|n| div().text_color(theme::muted()).child(n))),
                )
            })
    }

    fn render_status(&self) -> impl IntoElement {
        let saved = self.doc.as_ref().is_none_or(|d| !d.dirty);
        h_flex()
            .h(px(26.))
            .flex_none()
            .px_3()
            .gap_4()
            .items_center()
            .text_xs()
            .bg(theme::panel())
            .border_t_1()
            .border_color(theme::border())
            .text_color(theme::muted())
            .when(self.doc.is_some(), |d| {
                d.child(
                    h_flex()
                        .gap_1p5()
                        .items_center()
                        .child(dot(if saved { theme::ok() } else { theme::warn() }))
                        .child(if saved { "Enregistré" } else { "Modifications non enregistrées" }),
                )
            })
            .child(div().flex_1())
            .when_some(self.status.clone(), |d, (msg, err)| {
                d.child(div().text_color(if err { theme::danger() } else { theme::muted() }).child(msg))
            })
    }

    fn render_welcome(&self, cx: &mut Context<Self>) -> impl IntoElement {
        v_flex()
            .flex_1()
            .items_center()
            .justify_center()
            .gap_4()
            .child(div().text_2xl().font_weight(FontWeight::SEMIBOLD).child("GedRoots"))
            .child(
                div()
                    .text_color(theme::muted())
                    .child("Logiciel de généalogie libre et local. Vos données restent dans un fichier GEDCOM 7."),
            )
            .child(
                h_flex()
                    .gap_2()
                    .child(
                        Button::new("welcome-open")
                            .primary()
                            .label("Ouvrir un fichier GEDCOM…")
                            .on_click(cx.listener(|this, _, w, cx| this.open_file(&OpenFile, w, cx))),
                    )
                    .child(
                        Button::new("welcome-new")
                            .ghost()
                            .label("Nouveau fichier")
                            .on_click(cx.listener(|this, _, w, cx| this.new_file(&NewFile, w, cx))),
                    ),
            )
            .when_some(self.status.clone().filter(|s| s.1), |d, (msg, _)| {
                d.child(div().text_color(theme::danger()).child(msg))
            })
    }
}

impl Render for Workspace {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let title = match &self.doc {
            Some(d) => format!(
                "{}{} — GedRoots",
                d.path
                    .as_ref()
                    .and_then(|p| p.file_name())
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| "Nouveau fichier".into()),
                if d.dirty { " •" } else { "" }
            ),
            None => "GedRoots".into(),
        };
        if title != self.window_title {
            window.set_window_title(&title);
            self.window_title = title;
        }
        // Les raccourcis (Enregistrer…) passent par ce nœud : on le garde dans la chaîne de focus.
        if window.focused(cx).is_none() {
            window.focus(&self.focus, cx);
        }
        let body: AnyElement = if let (Some(_), Some((form, _))) = (&self.doc, &self.entry_form) {
            h_flex()
                .flex_1()
                .min_h_0()
                .items_stretch()
                .child(self.render_activity_bar(cx))
                .child(form.clone())
                .into_any_element()
        } else if self.doc.is_some() {
            h_flex()
                .flex_1()
                .min_h_0()
                .items_stretch()
                .child(self.render_activity_bar(cx))
                .child(self.render_list(cx))
                .child(self.render_center(cx))
                .child(self.render_detail(cx))
                .into_any_element()
        } else {
            self.render_welcome(cx).into_any_element()
        };
        v_flex()
            .size_full()
            .relative()
            .track_focus(&self.focus)
            .key_context("Workspace")
            .map(|d| self.bind_actions(d, window, cx))
            // Boutons « précédent / suivant » de la souris.
            .on_mouse_down(
                MouseButton::Navigate(NavigationDirection::Back),
                cx.listener(|this, _, w, cx| this.go_back(w, cx)),
            )
            .on_mouse_down(
                MouseButton::Navigate(NavigationDirection::Forward),
                cx.listener(|this, _, w, cx| this.go_forward(w, cx)),
            )
            .on_key_down(cx.listener(|this, ev: &KeyDownEvent, _, cx| {
                if ev.keystroke.key == "escape" && this.photo_draw.is_some() {
                    this.cancel_draw(cx);
                }
            }))
            .bg(theme::bg())
            .text_color(theme::text())
            .text_sm()
            // macOS : menus natifs, et le nom du fichier est dans le titre de la fenêtre.
            .when(self.menu_bar.is_some(), |d| d.child(self.render_header()))
            .child(body)
            .child(self.render_status())
            .children(self.render_picker(cx))
            .children(self.render_unreferenced_files(cx))
            .children(self.render_palette(cx))
    }
}

// ----------------------------------------------------------------------
// Accès pour la fenêtre Analyses (voir `analyses.rs`).
// ----------------------------------------------------------------------

impl Workspace {
    /// Copie du fichier courant, pour les outils d'analyse.
    pub(crate) fn ged_snapshot(&self) -> Option<Gedcom> {
        self.doc.as_ref().map(|d| d.ged.clone())
    }

    /// Individu sélectionné dans la fenêtre principale.
    pub(crate) fn selected_individual(&self) -> Option<String> {
        let doc = self.doc.as_ref()?;
        self.selected.clone().filter(|x| doc.ged.individual(x).is_some())
    }

    /// Applique une modification au fichier, avec point d'annulation.
    pub(crate) fn edit_ged(
        &mut self,
        f: impl FnOnce(&mut Gedcom) -> bool,
        msg: impl Into<SharedString>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.doc.is_none() {
            return;
        }
        self.checkpoint();
        if f(&mut self.doc.as_mut().unwrap().ged) {
            self.rebuild_rows(cx);
            self.refresh_editor(window, cx);
            self.set_status(msg, false);
        } else {
            self.drop_checkpoint();
        }
        cx.notify();
    }

    fn open_analyses(&mut self, _: &OpenAnalyses, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(ged) = self.ged_snapshot() {
            let person = self.selected_individual();
            crate::analyses::open(cx.entity(), window.window_handle(), ged, person, cx);
        }
    }
}
