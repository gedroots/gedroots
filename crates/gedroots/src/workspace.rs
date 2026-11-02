//! Fenêtre principale : barre d'activité, liste, zone centrale (arbre, fiche,
//! GEDCOM brut), panneau de détail et barre d'état — d'après la maquette.

use crate::act_editor::{ActEditor, ActEditorEvent};
use crate::commands::*;
use crate::editor::{FieldState, RecordEditor, Target};
use crate::images::picture;
use crate::map::{MapEvent, MapView};
use crate::theme;
use gedcom::analysis::timeline::{self, EntryKind};
use gedcom::*;
use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::input::{Editor, Input, InputEvent, InputState, Textarea};
use gpui_kit::component::menu::AppMenuBar;
use gpui_kit::component::select::Select;
use gpui_kit::component::{Disableable, Icon, Sizable, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use menus::{From as MenuFrom, WithMenu};
use std::path::PathBuf;
use std::rc::Rc;

mod add;
mod chart;
mod commands;
mod filters;
mod history;
mod import;
mod insights;
mod menus;
mod pages;
mod photo;
mod quality;
mod relatives;
mod settings;
mod tree;
mod weblinks;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum View {
    Individuals,
    Families,
    Sources,
    Places,
    Media,
    Repositories,
    Notes,
    Quality,
    Stats,
}

impl View {
    const ALL: [View; 9] = [
        View::Individuals,
        View::Families,
        View::Sources,
        View::Places,
        View::Media,
        View::Repositories,
        View::Notes,
        View::Quality,
        View::Stats,
    ];

    fn label(self) -> &'static str {
        match self {
            View::Individuals => "Individus",
            View::Families => "Familles",
            View::Sources => "Sources",
            View::Places => "Lieux",
            View::Media => "Médias",
            View::Repositories => "Dépôts",
            View::Notes => "Notes",
            View::Quality => "Qualité",
            View::Stats => "Stats",
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
            View::Quality => "QUAL",
            View::Stats => "STAT",
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
            View::Quality => IconName::CircleCheck,
            View::Stats => IconName::ChartPie,
        }
    }

    /// Pages sans fiche (Qualité, Statistiques).
    fn page(self) -> Option<View> {
        matches!(self, View::Quality | View::Stats).then_some(self)
    }

    fn kind(self) -> Option<RecordKind> {
        Some(match self {
            View::Individuals => RecordKind::Individual,
            View::Families => RecordKind::Family,
            View::Sources => RecordKind::Source,
            View::Media => RecordKind::Media,
            View::Repositories => RecordKind::Repository,
            View::Notes => RecordKind::SharedNote,
            View::Places | View::Quality | View::Stats => return None,
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
    /// Arbre libre : personne centrale, générations chargées, déplacement.
    chart: std::cell::RefCell<chart::Chart>,
    picker: Option<relatives::Picker>,
    /// Fichiers image/PDF du dossier du `.ged`, pour les suggestions de rattachement.
    media_files: Rc<Vec<String>>,
    /// Suppression en attente de confirmation (clé de l'élément).
    pending_delete: Option<String>,
    /// Aide de la recherche avancée dépliée.
    search_help: bool,
    /// Saisie guidée d'un acte en cours (remplace liste, fiche et détail).
    act_editor: Option<(Entity<ActEditor>, Subscription)>,
    /// Actes de la source affichée (relus à chaque rafraîchissement de la fiche).
    source_acts: Vec<Act>,
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
    /// Page affichée d'un média multipage (à partir de 1).
    media_page: usize,
    /// Médias sélectionnés ensemble (⌘ / Maj + clic), dans l'ordre, pour les fusionner.
    media_marked: Vec<String>,
    /// Précédent / Suivant, et dernier élément vu dans chaque liste.
    history: history::History,
    /// Boutons « Détacher » dépliés dans le panneau d'un individu.
    /// Fenêtre d'ajout (événement, proches) ouverte par un « + ».
    add_dialog: Option<add::AddDialog>,
    /// Champ de lieu en cours de saisie dans la fiche.
    place_focus: Option<EntityId>,
    editor_subs: Vec<Subscription>,
    /// Ligne de la liste visée par le dernier clic droit.
    menu_row: Rc<std::cell::RefCell<Option<String>>>,
    /// Défilement de la liste, pour garder l'élément affiché visible.
    list_scroll: UniformListScrollHandle,
    /// Page Qualité : anomalies du fichier.
    quality: quality::QualityState,
    /// Lien de parenté affiché dans le panneau de l'individu (A, B).
    kinship: Option<(String, String)>,
    /// Fiche : afficher aussi les événements des proches (Affichage › Événements des proches).
    show_relative_events: bool,
    /// Statistiques du fichier (page Statistiques).
    stats: Option<Rc<gedcom::analysis::stats::Stats>>,
    _subs: Vec<Subscription>,
}

impl Workspace {
    pub fn new(path: Option<PathBuf>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let filter = cx.new(|cx| InputState::new(window, cx).placeholder("Rechercher un nom, un lieu…"));
        let sub = cx.subscribe_in(&filter, window, |this, _, ev: &InputEvent, _, cx| {
            if matches!(ev, InputEvent::Change) {
                this.build_rows(cx);
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
            tree_mode: tree::TreeMode::Chart,
            tree_gens: 5,
            chart: Default::default(),
            picker: None,
            media_files: Rc::new(Vec::new()),
            pending_delete: None,
            search_help: false,
            act_editor: None,
            source_acts: Vec::new(),
            palette: None,
            menu_bar: (!cfg!(target_os = "macos")).then(|| AppMenuBar::new(cx)),
            window_title: String::new(),
            photo_draw: None,
            photo_bounds: Default::default(),
            photo_hover: None,
            media_page: 1,
            media_marked: Vec::new(),
            history: Default::default(),
            list_scroll: UniformListScrollHandle::new(),
            quality: quality::QualityState { stale: true, ..Default::default() },
            kinship: None,
            show_relative_events: false,
            stats: None,
            menu_row: Rc::default(),
            place_focus: None,
            editor_subs: Vec::new(),
            add_dialog: None,
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

    /// Ouvre un fichier reçu de l'extérieur (Finder, Dock).
    pub fn open_path(&mut self, path: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        self.load(path, window, cx);
    }

    /// Vrai si le document ouvert a des modifications non enregistrées.
    pub fn has_unsaved_changes(&self) -> bool {
        self.doc.as_ref().is_some_and(|d| d.dirty)
    }

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
        doc.ged.stamp_header();
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
        self.media_marked.clear();
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
            // Un acte en cours appartient à la source qu'on quitte.
            self.act_editor = None;
            self.media_page = 1;
        }
        self.media_marked.clear();
        self.note_selected(&key);
        if self.view == View::Places {
            self.map.update(cx, |m, cx| m.set_selected(Some(key.clone()), cx));
        }
        if let Some(ix) = self.rows.iter().position(|r| r.key == key) {
            self.list_scroll.scroll_to_item(ix, ScrollStrategy::Center);
        }
        self.selected = Some(key);
        self.pending_delete = None;
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
        let (Some(doc), Some(key), None) = (&self.doc, &self.selected, self.view.page()) else {
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
        self.source_acts = match &target {
            Target::Record { xref, kind: RecordKind::Source } => doc.ged.acts(xref),
            _ => Vec::new(),
        };
        let ed = RecordEditor::new(&doc.ged, target, window, cx);
        // Champs de lieu : suggestions des lieux existants pendant la saisie.
        self.place_focus = None;
        self.editor_subs = ed
            .fields
            .iter()
            .filter(|f| f.place)
            .filter_map(|f| match &f.state {
                FieldState::Text(e) => Some(cx.subscribe_in(e, window, |this, input, ev: &InputEvent, _, cx| {
                    if matches!(ev, InputEvent::Focus | InputEvent::Change) {
                        this.place_focus = Some(input.entity_id());
                        cx.notify();
                    }
                })),
                _ => None,
            })
            .collect();
        self.editor = Some(ed);
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
        let Some(key) = self.selected.clone().filter(|_| self.view.page().is_none()) else { return };
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
        let renamed = ed.place_name(cx);
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
    // Actes d'une source
    // ------------------------------------------------------------------

    /// « Saisir un acte… » : dans la source affichée, sinon après avoir choisi
    /// (ou créé) la source.
    pub(super) fn enter_act(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(doc) = &self.doc else { return };
        let source = self
            .selected
            .clone()
            .filter(|x| self.view == View::Sources && doc.ged.get(x).is_some_and(|n| n.tag == "SOUR"));
        match source {
            Some(s) => self.open_act(s, None, window, cx),
            None => self.open_picker(relatives::PickPurpose::ActSource, window, cx),
        }
    }

    /// Ouvre un acte de la source dans sa fiche (`key` : acte existant ; sinon un acte vide).
    pub(super) fn open_act(
        &mut self,
        source: String,
        key: Option<ActKey>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.selected.as_deref() != Some(source.as_str()) || self.view != View::Sources {
            self.goto(source.clone(), window, cx);
        }
        self.tab = CenterTab::Form;
        let Some(doc) = &self.doc else { return };
        let g = &doc.ged;
        let act = match key.and_then(|k| g.act(&source, &k)) {
            Some(a) => a,
            None => {
                // Nouvel acte : l'événement que la source contient, sinon celui de son dernier acte.
                let event = g
                    .get(&source)
                    .map(SourceContent::read)
                    .and_then(|c| c.events.first().cloned())
                    .or_else(|| self.source_acts.last().map(|a| a.event.clone()))
                    .filter(|e| !e.is_empty())
                    .unwrap_or_else(|| "BIRT".into());
                Act::new(&source, &event)
            }
        };
        let (ged, path) = (g.clone(), doc.path.clone());
        let editor = cx.new(|cx| ActEditor::new(ged, path, act, window, cx));
        let sub = cx.subscribe_in(&editor, window, |this, editor, ev: &ActEditorEvent, window, cx| match ev {
            ActEditorEvent::Close => {
                this.act_editor = None;
                window.focus(&this.focus, cx);
                cx.notify();
            }
            ActEditorEvent::Save { act, next } => this.save_act(editor.clone(), act, *next, window, cx),
        });
        self.act_editor = Some((editor, sub));
        cx.notify();
    }

    fn save_act(
        &mut self,
        editor: Entity<ActEditor>,
        act: &Act,
        next: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.checkpoint();
        let Some(doc) = &mut self.doc else { return };
        match act.save(&mut doc.ged) {
            Ok(res) => {
                let ged = doc.ged.clone();
                self.set_status(res.summary(), false);
                self.rebuild_rows(cx);
                self.refresh_editor(window, cx);
                if next {
                    editor.update(cx, |e, cx| e.saved(ged, &res, window, cx));
                } else {
                    self.act_editor = None;
                    window.focus(&self.focus, cx);
                }
            }
            Err(e) => {
                self.drop_checkpoint();
                editor.update(cx, |f, cx| f.set_error(e, cx));
            }
        }
        cx.notify();
    }

    /// Supprime un acte : ses citations (les événements et les personnes restent).
    pub(super) fn delete_act(&mut self, source: String, key: ActKey, window: &mut Window, cx: &mut Context<Self>) {
        self.mutate(window, cx, |g| {
            let title = g.act(&source, &key).map(|a| a.title()).unwrap_or_default();
            g.remove_act(&source, &key);
            Ok((format!("Acte supprimé : {title}"), None))
        });
    }

    /// Section « Actes » de la fiche d'une source, avec son « + ».
    fn render_acts(&self, source: &str, cx: &mut Context<Self>) -> AnyElement {
        let s = source.to_string();
        v_flex()
            .gap_1()
            .child(
                h_flex()
                    .gap_1()
                    .items_center()
                    .child(label_caps(if self.source_acts.is_empty() {
                        "Actes".to_string()
                    } else {
                        format!("Actes · {}", self.source_acts.len())
                    }))
                    .child(
                        Button::new("add-act")
                            .ghost()
                            .xsmall()
                            .icon(IconName::Plus)
                            .tooltip("Saisir un acte")
                            .on_click(cx.listener(move |this, _, w, cx| this.open_act(s.clone(), None, w, cx))),
                    ),
            )
            .children(self.source_acts.iter().enumerate().map(|(n, a)| {
                let key = a.original.clone().unwrap_or_default();
                let (s1, k1) = (source.to_string(), key.clone());
                h_flex()
                    .id(SharedString::from(format!("act-{n}")))
                    .gap_2()
                    .px_2()
                    .py_1()
                    .rounded_sm()
                    .cursor_pointer()
                    .hover(|d| d.bg(theme::hover()))
                    .child(
                        div().flex_1().min_w_0().overflow_hidden().whitespace_nowrap().text_ellipsis().child(a.title()),
                    )
                    .child(mono(a.subtitle()))
                    .on_click(cx.listener(move |this, _, w, cx| this.open_act(s1.clone(), Some(k1.clone()), w, cx)))
                    .with_menu(self.act_menu(source, key, cx))
            }))
            .into_any_element()
    }

    // ------------------------------------------------------------------
    // Liste
    // ------------------------------------------------------------------

    /// Le fichier a peut-être changé : relance les contrôles et reconstruit la liste.
    fn rebuild_rows(&mut self, cx: &mut Context<Self>) {
        self.quality.stale = true;
        self.refresh_quality();
        self.build_rows(cx);
    }

    /// Reconstruit la liste (recherche, filtre ou fichier modifié).
    fn build_rows(&mut self, cx: &mut Context<Self>) {
        let q = self.filter.read(cx).value().to_lowercase();
        if self.view == View::Quality {
            self.rows = Rc::new(self.quality_rows(q.trim()));
            return;
        }
        if self.view == View::Stats {
            // Une seule « ligne », jamais affichée : elle sert à l'historique (⌘[).
            self.refresh_stats();
            let row = Row {
                key: "stats".into(),
                title: String::new(),
                strong: String::new(),
                meta: String::new(),
                dot: None,
                flagged: false,
            };
            self.rows = Rc::new(vec![row]);
            return;
        }
        let Some(doc) = &self.doc else {
            self.rows = Rc::new(Vec::new());
            return;
        };
        // Libellés courts des lieux selon le format du fichier (Réglages du fichier › Lieux).
        gedcom::set_place_layout(&doc.ged.place_form());
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
                    // Types d'actes et période : « naissance », « 1795 »…
                    .with_search(SourceContent::read(r).summary())
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
                    let web = m.web_url();
                    let files = m.files();
                    let missing = web.is_none()
                        && (files.is_empty()
                            || files.iter().any(|f| !pages::file_path(doc, f.path).is_some_and(|p| p.exists())));
                    Row {
                        key: m.xref().to_string(),
                        title: m.title(),
                        strong: String::new(),
                        meta: match web {
                            Some(u) => host(u).to_string(),
                            None if files.len() > 1 => format!("{} p.", files.len()),
                            None => String::new(),
                        },
                        dot: missing.then(theme::warn),
                        flagged: missing,
                    }
                    .with_search(
                        files
                            .iter()
                            .map(|f| f.path)
                            .chain(medium(r).and_then(medium_label))
                            .collect::<Vec<_>>()
                            .join(" "),
                    )
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
            View::Quality | View::Stats => Vec::new(),
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

// ======================================================================
// Rendu
// ======================================================================

/// Titre de section avec un bouton (« + ») à droite.
fn section_header(title: &'static str, button: impl IntoElement) -> Div {
    h_flex().items_center().justify_between().child(label_caps(title)).child(button)
}

/// Emplacement en pointillés « + … » pour ajouter une personne.
fn add_slot(id: impl Into<SharedString>, label: &'static str) -> Stateful<Div> {
    h_flex()
        .id(ElementId::Name(id.into()))
        .p_2()
        .gap_1p5()
        .items_center()
        .rounded_md()
        .border_1()
        .border_dashed()
        .border_color(theme::border_strong())
        .text_color(theme::muted())
        .text_xs()
        .cursor_pointer()
        .hover(|d| d.border_color(theme::accent()).text_color(theme::accent()))
        .child(icon(IconName::Plus))
        .child(label)
}

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
    // Personne située sur une autre page d'un média multipage : cette page-là.
    let page = |m: &Media| pages::page_path(doc, m, doc.ged.link_page(i.xref(), m.xref()));
    match portrait.and_then(|m| page(&m)).filter(|p| p.exists()) {
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
                    .child("GEDRoots"),
            )
            // Linux, Windows : menus dans la fenêtre (natifs sous macOS).
            .children(self.menu_bar.clone().map(|m| div().h_full().child(m)))
            .child(div().text_color(theme::muted()).child(file))
            .child(div().flex_1())
    }

    fn render_activity_bar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        // Pas de nombre sous Statistiques.
        let counts: Vec<Option<usize>> = View::ALL
            .iter()
            .map(|v| match (&self.doc, v.kind()) {
                (_, None) if *v == View::Stats => None,
                (Some(_), None) if *v == View::Quality => Some(self.quality_count()),
                (Some(d), Some(k)) => Some(d.ged.count(k)),
                (Some(d), None) => Some(d.ged.places().len()),
                _ => Some(0),
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
                    .children(n.map(|n| mono(n.to_string())))
                    .on_click(cx.listener(move |this, _, w, cx| this.set_view(v, w, cx)))
            }))
    }

    fn render_list(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let rows = self.rows.clone();
        let selected = self.selected.clone();
        let marked: Rc<Vec<String>> = Rc::new(self.media_marked.clone());
        let multi = self.view == View::Media;
        let entity = cx.entity().downgrade();
        let menu_row = self.menu_row.clone();
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
                div()
                    .id("list-area")
                    .flex_1()
                    .min_h_0()
                    .child(
                        uniform_list("records", rows.len(), move |range, _window, _cx| {
                            range
                                .map(|i| {
                                    let r = &rows[i];
                                    let on = if marked.is_empty() {
                                        selected.as_deref() == Some(r.key.as_str())
                                    } else {
                                        marked.contains(&r.key)
                                    };
                                    let key = r.key.clone();
                                    let entity = entity.clone();
                                    let menu_row = menu_row.clone();
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
                                                    d.child(
                                                        div().font_weight(FontWeight::SEMIBOLD).child(r.strong.clone()),
                                                    )
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
                                        .on_click({
                                            let (key, entity) = (key.clone(), entity.clone());
                                            move |ev: &ClickEvent, window, cx| {
                                                let key = key.clone();
                                                // Médias : ⌘ / Ctrl + clic ou Maj + clic pour en sélectionner plusieurs.
                                                let m = ev.modifiers();
                                                let add = multi && (m.secondary(cx) || m.shift);
                                                let _ = entity.update(cx, |this, cx| {
                                                    if add {
                                                        this.mark_media(key, m.shift, cx)
                                                    } else {
                                                        this.select(key, window, cx)
                                                    }
                                                });
                                            }
                                        })
                                        // Clic droit : le menu de la liste (plus bas) vise cette ligne.
                                        .on_mouse_down(MouseButton::Right, move |_, _, _| {
                                            *menu_row.borrow_mut() = Some(key.clone());
                                        })
                                })
                                .collect()
                        })
                        .track_scroll(&self.list_scroll)
                        .size_full()
                        .text_sm(),
                    )
                    // Un seul menu pour toute la liste : celui de la ligne visée, ou
                    // sous les lignes, créer un élément.
                    .with_menu(self.list_menu(cx)),
            )
    }

    fn render_center(&self, cx: &mut Context<Self>) -> AnyElement {
        if self.view == View::Quality {
            return v_flex().flex_1().min_w_0().bg(theme::bg()).child(self.render_quality(cx)).into_any_element();
        }
        if self.view == View::Stats {
            return self.render_stats(cx);
        }
        let tabs: Vec<CenterTab> = match self.view {
            View::Individuals => vec![CenterTab::Tree, CenterTab::Form, CenterTab::Raw],
            View::Places => vec![CenterTab::Map, CenterTab::Form],
            _ => vec![CenterTab::Form, CenterTab::Raw],
        };
        let tab = if tabs.contains(&self.tab) { self.tab } else { tabs[0] };
        let merging = self.view == View::Media && self.media_marked.len() >= 2;
        let body: AnyElement = match (tab, &self.editor) {
            (CenterTab::Map, _) => self.map.clone().into_any_element(),
            (CenterTab::Form, _) if merging => self.render_merge_panel(cx),
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
                                CenterTab::Tree => match self.tree_mode {
                                    tree::TreeMode::Fan => {
                                        let r = self.tree_mode.generations();
                                        let gens = self.tree_gens.clamp(*r.start(), *r.end());
                                        format!("{} · {gens} gén.", self.tree_mode.label())
                                    }
                                    m => m.label().to_string(),
                                },
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
                    v_flex()
                        .gap_1()
                        .min_w_0()
                        .child(label_caps(f.label.clone()))
                        .child(match &f.state {
                            FieldState::Text(e) => Input::new(e).small().into_any_element(),
                            FieldState::Choice(e) => {
                                Select::new(e).small().cleanable(true).placeholder("Choisir…").into_any_element()
                            }
                            FieldState::Date(e) => e.clone().into_any_element(),
                        })
                        .children(match &f.state {
                            FieldState::Text(e) if f.place => self.place_suggestions(e, cx),
                            _ => None,
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
                .when_some(
                    match &ed.target {
                        Target::Record { xref, kind: RecordKind::Source } => Some(xref.clone()),
                        _ => None,
                    },
                    |d, s| d.child(self.render_acts(&s, cx)),
                )
                .when_some(preview, |d, p| d.child(p))
                .when_some(self.record_images(ed, cx), |d, p| d.child(p)),
        )
    }

    /// Lieux existants qui correspondent au champ de lieu en cours de saisie.
    fn place_suggestions(&self, input: &Entity<InputState>, cx: &mut Context<Self>) -> Option<AnyElement> {
        if self.place_focus != Some(input.entity_id()) {
            return None;
        }
        let value = input.read(cx).value().to_string();
        let q = sort_key(value.trim());
        if q.len() < 2 {
            return None;
        }
        let doc = self.doc.as_ref()?;
        let mut hits: Vec<(String, String)> = doc
            .ged
            .places()
            .into_iter()
            .filter(|p| p.name != value && sort_key(&p.name).contains(&q))
            .map(|p| (short_place(&p.name), p.name))
            .collect();
        // Les lieux qui commencent par la saisie d'abord.
        hits.sort_by_key(|(short, _)| (!sort_key(short).starts_with(&q), short.clone()));
        hits.truncate(6);
        if hits.is_empty() {
            return None;
        }
        Some(
            h_flex()
                .flex_wrap()
                .gap_1()
                .children(hits.into_iter().enumerate().map(|(n, (short, full))| {
                    let entity = input.clone();
                    div()
                        .id(SharedString::from(format!("pl-{}-{n}", input.entity_id())))
                        .px_2()
                        .py(px(3.))
                        .rounded_md()
                        .border_1()
                        .border_color(theme::border_strong())
                        .bg(theme::surface())
                        .text_xs()
                        .cursor_pointer()
                        .hover(|d| d.border_color(theme::accent()))
                        .child(short)
                        .on_click(cx.listener(move |this, _, window, cx| {
                            entity.update(cx, |s, cx| s.set_value(full.clone(), window, cx));
                            this.place_focus = None;
                            cx.notify();
                        }))
                }))
                .into_any_element(),
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
                    // Zone recadrée : sa page seulement ; sinon toutes les pages, l'une sous l'autre.
                    let shown: Vec<usize> = match crop {
                        Some(_) => vec![doc.ged.link_page(xref, m.xref())],
                        None => (1..=m.page_count().max(1)).collect(),
                    };
                    let body = v_flex().gap_2().children(shown.into_iter().map(|page| {
                        match pages::page_path(doc, &m, page).filter(|p| p.exists()) {
                            Some(p) if crop.is_some() => photo::fitted_crop(p, crop, 700., 640.),
                            Some(p) => picture(p, 700., 640., ObjectFit::Contain)
                                .max_w_full()
                                .max_h(px(640.))
                                .into_any_element(),
                            None => div()
                                .text_color(theme::warn())
                                .child(format!("Fichier introuvable : {}", m.page_file(page).unwrap_or("")))
                                .into_any_element(),
                        }
                    }));
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
                        .with_menu(self.record_menu(m.xref(), MenuFrom::Attached(xref.clone()), cx))
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
            let menu = self.record_menu(m.xref(), MenuFrom::Attached(xref.to_string()), cx);
            let page_count = m.page_count();
            let path = pages::page_path(doc, &m, doc.ged.link_page(xref, m.xref())).filter(|p| p.exists());
            let tile = div()
                .relative()
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
            // Nombre de pages d'un média multipage, en coin.
            let badge = (page_count > 1).then(|| {
                div()
                    .absolute()
                    .right(px(4.))
                    .bottom(px(4.))
                    .px_1()
                    .rounded_sm()
                    .bg(gpui_kit::hsla(0., 0., 0., 0.7))
                    .text_xs()
                    .text_color(theme::text())
                    .child(format!("{page_count} p."))
            });
            let tile = match path {
                Some(p) if crop.is_some() => {
                    // Zone recadrée du lien : visage sur une photo de groupe, extrait d'un acte…
                    let (w, h) = if large { (326., 218.) } else { (104., 94.) };
                    tile.flex().justify_center().child(photo::linked_img(p, crop, w, h, !large))
                }
                Some(p) => {
                    let (w, h, fit) =
                        if large { (326., 220., ObjectFit::Contain) } else { (104., 96., ObjectFit::Cover) };
                    tile.child(picture(p, w, h, fit).size_full())
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
            };
            tile.children(badge).with_menu(menu)
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
        if let Some(url) = m.web_url() {
            // Page web : bouton pour l'ouvrir dans le navigateur.
            let href = url.to_string();
            return Some(
                h_flex()
                    .child(
                        Button::new("open-web-media")
                            .small()
                            .icon(IconName::ExternalLink)
                            .label(format!("Ouvrir {}", host(url)))
                            .on_click(move |_, _, cx| cx.open_url(&href)),
                    )
                    .into_any_element(),
            );
        }
        let page = self.current_page(xref);
        let strip = self.render_page_strip(xref, cx);
        let file = m.page_file(page).unwrap_or("");
        let path = pages::page_path(doc, &m, page);
        let image = m
            .files()
            .get(page - 1)
            .is_some_and(|f| f.form.is_some_and(|t| t.starts_with("image/")) || mime_for(f.path).starts_with("image/"));
        let el: Option<AnyElement> = match path {
            Some(p) if p.exists() && image => Some(self.render_photo(xref, p, cx)),
            // Fichier présent mais sans aperçu (PDF…) : rien à signaler.
            Some(p) if p.exists() => None,
            Some(_) => {
                Some(div().text_color(theme::warn()).child(format!("Fichier introuvable : {file}")).into_any_element())
            }
            None => Some(div().text_color(theme::muted()).child("Aucun fichier local associé").into_any_element()),
        };
        if strip.is_none() && el.is_none() {
            return None;
        }
        // Déposer des fichiers sur l'aperçu les ajoute comme pages.
        let (media, entity) = (xref.clone(), cx.entity().downgrade());
        Some(
            v_flex()
                .gap_3()
                .drag_over::<ExternalPaths>(|d, _, _, _| d.bg(theme::accent_soft()))
                .on_drop(move |paths: &ExternalPaths, window, cx| {
                    let media = media.clone();
                    let _ = entity.update(cx, |this, cx| {
                        let files = paths.paths().iter().map(|p| this.file_value(p)).collect();
                        this.add_pages(media, files, window, cx);
                    });
                })
                .children(strip)
                .children(el)
                .into_any_element(),
        )
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
        let Some(doc) = &self.doc.as_ref().filter(|_| self.view.page().is_none()) else {
            return div().into_any_element();
        };
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
        // Sur un média, ils deviennent de nouvelles pages.
        let kind = g.get(key).and_then(|n| RecordKind::from_tag(&n.tag));
        let attachable = kind.is_some_and(|k| {
            matches!(k, RecordKind::Individual | RecordKind::Family | RecordKind::Source | RecordKind::Media)
        });
        let panel = if attachable {
            let (record, entity) = (key.clone(), cx.entity().downgrade());
            let as_pages = kind == Some(RecordKind::Media);
            panel.drag_over::<ExternalPaths>(|d, _, _, _| d.bg(theme::accent_soft())).on_drop(
                move |paths: &ExternalPaths, window, cx| {
                    let record = record.clone();
                    let _ = entity.update(cx, |this, cx| {
                        let files = paths.paths().iter().map(|p| this.file_value(p)).collect();
                        if as_pages {
                            this.add_pages(record, files, window, cx);
                        } else {
                            this.attach_files(Some(record), files, window, cx);
                        }
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
                    .children(self.render_kinship(key, cx))
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
                            // Les pages web sont dans « Liens web ».
                            if !v.contains(&p) && p != *key && !weblinks::is_web_media(g, &p) {
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
                let web = self.render_web_links(key, cx);
                // Rien d'autre que le titre (déjà dans la fiche) : pas de panneau, la
                // zone centrale prend la place.
                if actions.is_none() && gallery.is_none() && outgoing.is_empty() && !show_refs && web.is_none() {
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
                    .when(kind == Some(RecordKind::Family), |d| d.child(self.render_events(key, cx)))
                    .when_some(gallery, |d, el| d.child(el))
                    .when_some(web, |d, el| d.child(el))
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
                    .with_menu(self.record_menu(&x, MenuFrom::Link, cx))
            }),
        )
    }

    /// Section « Événements » d'un individu ou d'une famille. Pour un individu,
    /// les événements de ses unions (mariage, divorce…) sont dans la même
    /// chronologie. Un clic ouvre l'événement en modification, clic droit pour
    /// les autres actions ; déposer des fichiers sur un événement les y rattache.
    fn render_events(&self, owner: &str, cx: &mut Context<Self>) -> AnyElement {
        let doc = self.doc.as_ref().unwrap();
        let g = &doc.ged;
        let events: Vec<PersonEvent> = if g.individual(owner).is_some() {
            g.person_events(owner)
        } else {
            let mut v: Vec<PersonEvent> = g
                .events_of(owner)
                .into_iter()
                .map(|(index, event)| PersonEvent {
                    owner: owner.to_string(),
                    index,
                    event,
                    family: false,
                    spouse: None,
                })
                .collect();
            v.sort_by_key(|e| e.event.date().map(|d| d.sort_key()).unwrap_or((i32::MAX, 0, 0)));
            v
        };
        let person = g.individual(owner);
        // Événements des proches pendant la vie de la personne (naissance d'un
        // enfant, décès d'un parent…), affichés à la demande.
        let relatives: Vec<timeline::Entry> = match person {
            Some(_) => timeline::timeline(g, owner).into_iter().filter(|e| e.kind == EntryKind::Relative).collect(),
            None => Vec::new(),
        };
        let show_relatives = self.show_relative_events;
        let x = owner.to_string();
        let mut rows: Vec<((i32, u8, u8), AnyElement)> = events
            .into_iter()
            .map(|pe| {
                let PersonEvent { owner: ev_owner, index: ix, event: e, family, spouse } = pe;
                let sort = e.date().map(|d| d.sort_key()).unwrap_or((i32::MAX, 0, 0));
                // Âge de la personne à la date de l'événement (sauf sa naissance).
                let age = person
                    .as_ref()
                    .filter(|_| family || e.tag() != "BIRT")
                    .and_then(|p| e.date_raw().and_then(|d| timeline::age_at(p, d)));
                let place = e.place().map(|p| p.to_string());
                let sources: Vec<String> = e.source_xrefs().iter().map(|s| s.to_string()).collect();
                let label = match e.value() {
                    Some(v) => format!("{} : {v}", e.label()),
                    None => e.label(),
                };
                let key = format!("{ev_owner}-{ix}");
                let (o1, o2) = (ev_owner.clone(), ev_owner.clone());
                // Déposer des fichiers sur l'événement : médias de l'événement.
                let (drop_owner, entity) = (ev_owner.clone(), cx.entity().downgrade());
                let row = v_flex()
                    .id(SharedString::from(format!("ev-{key}")))
                    .gap_0p5()
                    .px_2()
                    .py_1()
                    .rounded_sm()
                    .cursor_pointer()
                    .hover(|d| d.bg(theme::hover()))
                    .drag_over::<ExternalPaths>(|d, _, _, _| d.bg(theme::accent_soft()))
                    .on_drop(move |paths: &ExternalPaths, window, cx| {
                        cx.stop_propagation();
                        let owner = drop_owner.clone();
                        let _ = entity.update(cx, |this, cx| {
                            let files = paths.paths().iter().map(|p| this.file_value(p)).collect();
                            this.attach_files_to_event(owner, ix, files, window, cx);
                        });
                    })
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
                            .child(
                                h_flex()
                                    .gap_2()
                                    .flex_none()
                                    .when_some(age, |d, a| d.child(div().text_xs().text_color(theme::muted()).child(a)))
                                    .child(mono(e.date().map(|d| d.to_french()).unwrap_or_default())),
                            ),
                    )
                    // Événement d'une union : avec qui.
                    .when_some(spouse.filter(|_| family), |d, s| {
                        let target = s.clone();
                        d.child(
                            div()
                                .id(SharedString::from(format!("ev-spouse-{key}")))
                                .text_color(theme::muted())
                                .hover(|d| d.text_color(theme::accent()))
                                .child(format!("avec {}", g.record_label(&s)))
                                .on_click(cx.listener(move |this, _, w, cx| {
                                    cx.stop_propagation();
                                    this.goto(target.clone(), w, cx)
                                })),
                        )
                    })
                    .when_some(place, |d, p| {
                        let target = p.clone();
                        d.child(
                            div()
                                .id(SharedString::from(format!("ev-place-{key}")))
                                .text_color(theme::muted())
                                .hover(|d| d.text_color(theme::accent()))
                                .child(short_place(&p))
                                .on_click(cx.listener(move |this, _, w, cx| {
                                    cx.stop_propagation();
                                    this.goto_place(target.clone(), w, cx)
                                })),
                        )
                    })
                    .children(sources.into_iter().map(|s| {
                        let title = g.record_label(&s);
                        let target = s.clone();
                        div()
                            .id(SharedString::from(format!("ev-src-{key}-{s}")))
                            .text_xs()
                            .text_color(theme::ok())
                            .hover(|d| d.text_color(theme::accent()))
                            .child(title)
                            .on_click(cx.listener(move |this, _, w, cx| {
                                cx.stop_propagation();
                                this.goto(target.clone(), w, cx)
                            }))
                    }))
                    .children(self.render_event_media(&ev_owner, ix, &e, cx))
                    .on_click(cx.listener(move |this, _, w, cx| this.open_event_dialog(o1.clone(), Some(ix), w, cx)))
                    .with_menu(self.event_menu(o2, ix, cx));
                (sort, row.into_any_element())
            })
            .collect();
        if show_relatives {
            for (n, r) in relatives.iter().enumerate() {
                let sort = r.date.as_ref().map(|d| d.sort_key()).unwrap_or((i32::MAX, 0, 0));
                rows.push((sort, self.render_relative_event(n, r, cx)));
            }
            // Tri stable : à date égale, l'événement de la personne d'abord.
            rows.sort_by_key(|r| r.0);
        }
        let toggle = (!relatives.is_empty()).then(|| {
            Button::new("relative-events")
                .xsmall()
                .icon(IconName::Users)
                .when(show_relatives, |b| b.primary())
                .when(!show_relatives, |b| b.ghost())
                .tooltip("Événements des proches")
                .on_click(cx.listener(|this, _, _, cx| this.toggle_relative_events(cx)))
        });
        v_flex()
            .p_4()
            .gap_1()
            .border_b_1()
            .border_color(theme::border())
            .child(section_header(
                "Événements",
                h_flex().gap_1().children(toggle).child(
                    add::plus_button("add-event")
                        .tooltip("Ajouter un événement")
                        .on_click(cx.listener(move |this, _, w, cx| this.open_event_dialog(x.clone(), None, w, cx))),
                ),
            ))
            .children(rows.into_iter().map(|r| r.1))
            .into_any_element()
    }

    /// Affichage › Événements des proches.
    pub(super) fn toggle_relative_events(&mut self, cx: &mut Context<Self>) {
        self.show_relative_events = !self.show_relative_events;
        cx.notify();
    }

    /// Ligne discrète d'un événement d'un proche ; un clic ouvre ce proche.
    fn render_relative_event(&self, n: usize, r: &timeline::Entry, cx: &mut Context<Self>) -> AnyElement {
        let target = r.related.clone();
        h_flex()
            .id(("rel-ev", n))
            .justify_between()
            .gap_2()
            .px_2()
            .py_1()
            .rounded_sm()
            .text_sm()
            .text_color(theme::muted())
            .cursor_pointer()
            .hover(|d| d.bg(theme::hover()))
            .child(div().min_w_0().child(r.label.clone()))
            .child(
                h_flex()
                    .gap_2()
                    .flex_none()
                    .when_some(r.age.clone(), |d, a| d.child(div().text_xs().child(a)))
                    .child(mono(r.date.as_ref().map(|d| d.to_french()).unwrap_or_default())),
            )
            .on_click(cx.listener(move |this, _, w, cx| {
                if let Some(x) = target.clone() {
                    this.goto(x, w, cx)
                }
            }))
            .into_any_element()
    }

    /// Médias d'un événement : vignettes des images, titre des autres documents.
    fn render_event_media(
        &self,
        owner: &str,
        ix: usize,
        e: &gedcom::Event<'_>,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let doc = self.doc.as_ref()?;
        let g = &doc.ged;
        let media: Vec<Media> =
            e.media_xrefs().into_iter().filter_map(|m| g.get(m).filter(|n| n.tag == "OBJE").map(Media::new)).collect();
        if media.is_empty() {
            return None;
        }
        let from = || MenuFrom::OnEvent(owner.to_string(), ix);
        Some(
            h_flex()
                .flex_wrap()
                .gap_1()
                .pt_1()
                .children(media.into_iter().map(|m| {
                    let target = m.xref().to_string();
                    let id = SharedString::from(format!("ev-media-{owner}-{ix}-{target}"));
                    let menu = self.record_menu(m.xref(), from(), cx);
                    let open = cx.listener(move |this, _, w, cx| {
                        cx.stop_propagation();
                        this.goto(target.clone(), w, cx)
                    });
                    let path = m.is_image().then(|| pages::page_path(doc, &m, 1)).flatten().filter(|p| p.exists());
                    match path {
                        Some(p) => div()
                            .id(id)
                            .size(px(56.))
                            .rounded_sm()
                            .overflow_hidden()
                            .border_1()
                            .border_color(theme::border())
                            .hover(|d| d.border_color(theme::accent()))
                            .child(picture(p, 56., 56., ObjectFit::Cover).size_full())
                            .on_click(open)
                            .with_menu(menu)
                            .into_any_element(),
                        None => h_flex()
                            .id(id)
                            .gap_1()
                            .items_center()
                            .max_w(px(220.))
                            .px_1p5()
                            .py_0p5()
                            .rounded_sm()
                            .border_1()
                            .border_color(theme::border())
                            .text_xs()
                            .text_color(if m.is_image() { theme::warn() } else { theme::muted() })
                            .hover(|d| d.border_color(theme::accent()))
                            .child(icon(if m.is_image() { IconName::Image } else { IconName::FileText }))
                            .child(div().overflow_hidden().whitespace_nowrap().text_ellipsis().child(m.title()))
                            .on_click(open)
                            .with_menu(menu)
                            .into_any_element(),
                    }
                }))
                .into_any_element(),
        )
    }

    fn render_individual_card(&self, g: &Gedcom, i: Individual<'_>, cx: &mut Context<Self>) -> impl IntoElement {
        // Comme la liste « Événements » : ceux des unions comptent aussi.
        let timeline = g.person_events(i.xref());
        let (sourced, total) = (timeline.iter().filter(|e| e.event.is_sourced()).count(), timeline.len());
        let pct = if total == 0 { 0. } else { sourced as f32 / total as f32 };
        let sosa = i.sosa();
        let doc = self.doc.as_ref().unwrap();
        let is_root = g.root().as_deref() == Some(i.xref());
        let lineage =
            sosa.as_deref().and_then(|s| s.parse().ok()).map(theme::lineage).unwrap_or(theme::border_strong());
        let xref = i.xref().to_string();

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

        let events_el = self.render_events(&xref, cx);

        // Famille : parents, conjoints, enfants.
        // (rôle, proche, lien à défaire au clic droit : (individu, famille, libellé)).
        type Unlink = (String, String, &'static str);
        // Quatrième champ : années d'une union terminée (« 1932–1950 »).
        let mut family: Vec<(&'static str, String, Unlink, Option<String>)> = Vec::new();
        if let Some(fam) = i.famc().first().and_then(|f| g.family(f)) {
            let link = (xref.clone(), fam.xref().to_string(), "Détacher de ses parents");
            family.extend(fam.husband().map(|p| ("Père", p.to_string(), link.clone(), None)));
            family.extend(fam.wife().map(|p| ("Mère", p.to_string(), link, None)));
        }
        for fam in i.fams().into_iter().filter_map(|f| g.family(f)) {
            let f = fam.xref().to_string();
            let spouse = [fam.husband(), fam.wife()].into_iter().flatten().find(|x| *x != xref);
            // Union terminée (divorce, annulation) : ses années à la place de celles du conjoint.
            let end = fam.separation().map(|e| {
                let year = |e: Option<gedcom::Event>| e.and_then(|e| e.date()).and_then(|d| d.year());
                let end = year(Some(e)).map(|y| y.to_string()).unwrap_or_else(|| "?".into());
                match year(fam.marriage()) {
                    Some(start) => format!("{start}–{end}"),
                    None => format!("–{end}"),
                }
            });
            family.extend(
                spouse.map(|s| ("Conjoint", s.to_string(), (xref.clone(), f.clone(), "Détacher de cette union"), end)),
            );
            family.extend(
                fam.children()
                    .into_iter()
                    .map(|k| ("Enfant", k.to_string(), (k.to_string(), f.clone(), "Retirer cet enfant"), None)),
            );
        }
        family.retain(|(_, x, _, _)| g.individual(x).is_some());
        let family_el = v_flex()
            .p_4()
            .gap_1()
            .border_b_1()
            .border_color(theme::border())
            .child(section_header("Famille", {
                let x = xref.clone();
                add::plus_button("add-relative")
                    .tooltip("Ajouter ou lier un proche")
                    .on_click(cx.listener(move |this, _, _, cx| this.open_relatives_dialog(x.clone(), cx)))
            }))
            .children(family.into_iter().map(|(role, x, unlink, end)| {
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
                    .child(div().w(px(64.)).flex_none().text_xs().text_color(theme::muted()).child(role))
                    .child(div().flex_1().overflow_hidden().whitespace_nowrap().text_ellipsis().child(p.display_name()))
                    // Union terminée : ses années plutôt que les dates de vie du conjoint.
                    .child(mono(end.unwrap_or_else(|| p.years())))
                    .on_click(cx.listener(move |this, _, w, cx| this.goto(target.clone(), w, cx)))
                    .with_menu(self.person_menu(&x, MenuFrom::Relative(unlink.0, unlink.1, unlink.2), cx))
            }));

        let images: Vec<String> = g.images_of(&xref).iter().map(|m| m.xref().to_string()).collect();
        let media: Vec<String> = i
            .media_xrefs()
            .iter()
            .map(|s| s.to_string())
            .filter(|x| !images.contains(x) && !weblinks::is_web_media(g, x))
            .collect();
        let web = self.render_web_links(&xref, cx);
        let gallery = self.render_gallery(&xref, false, cx);
        let notes = i.notes();
        v_flex()
            .child(header)
            .when_some(gallery, |d, el| d.child(el))
            .child(events_el)
            .child(family_el)
            .when_some(web, |d, el| d.child(el))
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
            .child(div().text_2xl().font_weight(FontWeight::SEMIBOLD).child("GEDRoots"))
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
                "{}{} — GEDRoots",
                d.path
                    .as_ref()
                    .and_then(|p| p.file_name())
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| "Nouveau fichier".into()),
                if d.dirty { " •" } else { "" }
            ),
            None => "GEDRoots".into(),
        };
        if title != self.window_title {
            window.set_window_title(&title);
            self.window_title = title;
        }
        // Les raccourcis (Enregistrer…) passent par ce nœud : on le garde dans la chaîne de focus.
        if window.focused(cx).is_none() {
            window.focus(&self.focus, cx);
        }
        let body: AnyElement = if let (Some(_), Some((editor, _))) = (&self.doc, &self.act_editor) {
            // Acte ouvert dans la fiche de sa source : il prend la place de la fiche et du panneau.
            h_flex()
                .flex_1()
                .min_h_0()
                .items_stretch()
                .child(self.render_activity_bar(cx))
                .child(self.render_list(cx))
                .child(editor.clone())
                .into_any_element()
        } else if self.doc.is_some() {
            h_flex()
                .flex_1()
                .min_h_0()
                .items_stretch()
                .child(self.render_activity_bar(cx))
                .when(self.view != View::Stats, |d| d.child(self.render_list(cx)))
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
            .map(|d| self.bind_page_actions(d, cx))
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
            .children(self.render_add_dialog(cx))
            .children(self.render_palette(cx))
    }
}

// ----------------------------------------------------------------------
// Accès pour les autres fenêtres.
// ----------------------------------------------------------------------

impl Workspace {
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
}
