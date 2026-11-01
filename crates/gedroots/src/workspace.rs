//! Fenêtre principale : barre d'activité, liste, zone centrale (arbre, fiche,
//! GEDCOM brut), panneau de détail et barre d'état — d'après la maquette.

use crate::editor::{RecordEditor, Target};
use crate::map::{MapEvent, MapView};
use crate::theme;
use gedcom::*;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::input::{Editor, Input, InputEvent, InputState, Textarea};
use gpui_kit::component::{Disableable, Sizable, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use std::path::PathBuf;
use std::rc::Rc;

mod filters;
mod relatives;
mod tree;

actions!(gedroots, [OpenFile, SaveFile, Undo, NewFile, Quit, OpenAnalyses]);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
    _subs: Vec<Subscription>,
}

impl Workspace {
    pub fn new(path: Option<PathBuf>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let filter = cx.new(|cx| InputState::new(window, cx).placeholder("Nom, prénom, lieu, @I…@"));
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
                let first = ged
                    .records_of(RecordKind::Individual)
                    .find(|r| Individual::new(r).sosa().as_deref() == Some("1"))
                    .or_else(|| ged.records_of(RecordKind::Individual).next())
                    .and_then(|r| r.xref.clone());
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

    fn set_view(&mut self, view: View, window: &mut Window, cx: &mut Context<Self>) {
        if self.view == view {
            return;
        }
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
        if let Some(first) = self.rows.first().map(|r| r.key.clone()) {
            self.select(first, window, cx);
        }
        cx.notify();
    }

    fn select(&mut self, key: String, window: &mut Window, cx: &mut Context<Self>) {
        if self.view == View::Places {
            self.map.update(cx, |m, cx| m.set_selected(Some(key.clone()), cx));
        }
        self.selected = Some(key);
        self.pending_delete = None;
        self.refresh_editor(window, cx);
        cx.notify();
    }

    /// Ouvre un enregistrement quelconque (lien depuis une fiche).
    pub(crate) fn goto(&mut self, xref: String, window: &mut Window, cx: &mut Context<Self>) {
        let Some(kind) = self.doc.as_ref().and_then(|d| d.ged.get(&xref)).and_then(|n| RecordKind::from_tag(&n.tag))
        else {
            return;
        };
        let view = View::for_kind(kind);
        if view != self.view {
            self.set_view(view, window, cx);
        }
        self.select(xref, window, cx);
    }

    fn goto_place(&mut self, name: String, window: &mut Window, cx: &mut Context<Self>) {
        self.set_view(View::Places, window, cx);
        self.select(name, window, cx);
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
        self.set_status(format!("{} @{x}@ créé", kind.label_fr()), false);
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
            Target::Place(_) => Some(ed.fields[0].state.read(cx).value().trim().to_string()),
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
                self.set_status(format!("GEDCOM de @{new_xref}@ appliqué"), false);
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

impl Workspace {
    fn render_header(&self, cx: &mut Context<Self>) -> impl IntoElement {
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
        let version = self.doc.as_ref().map(|d| format!("GEDCOM {}", d.ged.version())).unwrap_or_default();
        let can_undo = self.doc.as_ref().is_some_and(|d| !d.history.is_empty());
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
                    .child(svg().path("icons/network.svg").size(px(16.)).text_color(theme::accent()))
                    .child("GedRoots"),
            )
            .child(div().text_color(theme::muted()).child(file))
            .child(div().flex_1())
            .when(!version.is_empty(), |d| {
                d.child(
                    div()
                        .px_2()
                        .py(px(1.))
                        .rounded_sm()
                        .border_1()
                        .border_color(theme::border_strong())
                        .child(mono(version)),
                )
            })
            .child(
                Button::new("open")
                    .ghost()
                    .small()
                    .label("Ouvrir…")
                    .on_click(cx.listener(|this, _, w, cx| this.open_file(&OpenFile, w, cx))),
            )
            .when(self.doc.is_some(), |d| {
                d.child(
                    Button::new("analyses")
                        .ghost()
                        .small()
                        .label("Analyses")
                        .on_click(cx.listener(|this, _, w, cx| this.open_analyses(&OpenAnalyses, w, cx))),
                )
                .child(
                    Button::new("undo")
                        .ghost()
                        .small()
                        .label("Annuler")
                        .disabled(!can_undo)
                        .on_click(cx.listener(|this, _, w, cx| this.undo(&Undo, w, cx))),
                )
                .child(
                    Button::new("save")
                        .primary()
                        .small()
                        .label("Enregistrer")
                        .on_click(cx.listener(|this, _, w, cx| this.save_file(&SaveFile, w, cx))),
                )
            })
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
        let can_create = self.view.kind().is_some() && self.doc.is_some();
        v_flex()
            .w(px(300.))
            .flex_none()
            .bg(theme::bg())
            .border_r_1()
            .border_color(theme::border())
            .child(
                h_flex()
                    .px_3()
                    .pt_3()
                    .pb_2()
                    .gap_2()
                    .items_center()
                    .child(div().font_weight(FontWeight::SEMIBOLD).child(self.view.label()))
                    .child(mono(rows.len().to_string()))
                    .child(div().flex_1())
                    .when(can_create, |d| {
                        d.child(
                            Button::new("create")
                                .ghost()
                                .xsmall()
                                .label("+ Nouveau")
                                .on_click(cx.listener(|this, _, w, cx| this.create_record(w, cx))),
                        )
                    }),
            )
            .child(div().px_3().pb_2().child(Input::new(&self.filter).small().cleanable(true)))
            .when(self.view == View::Individuals, |d| d.child(self.render_quick_filters(cx)))
            .when(!flag.is_empty(), |d| {
                d.child(
                    h_flex().px_3().pb_2().gap_1().child(
                        Button::new("flag")
                            .xsmall()
                            .when(only, |b| b.primary())
                            .when(!only, |b| b.ghost())
                            .label(flag)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.only_flagged = !this.only_flagged;
                                this.rebuild_rows(cx);
                                cx.notify();
                            })),
                    ),
                )
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
        let title = self.selected.as_ref().map(|k| match self.view {
            View::Places => short_place(k),
            _ => self.doc.as_ref().map(|d| d.ged.record_label(k)).unwrap_or_default(),
        });
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
                            .child(t.label())
                            .when(active && title.is_some(), |d| {
                                d.child(div().text_color(theme::muted()).child(format!("— {}", title.clone().unwrap())))
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
        let column = |range: std::ops::Range<usize>, cx: &mut Context<Self>| {
            v_flex().flex_1().min_w(px(140.)).justify_around().gap_2().children(range.map(|k| {
                match anc[k].as_ref().and_then(|x| g.individual(x)) {
                    Some(i) => {
                        let x = i.xref().to_string();
                        let on = k == 1;
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
                                    .gap_1p5()
                                    .items_center()
                                    .child(dot(theme::lineage(k)))
                                    .child(
                                        div()
                                            .flex_1()
                                            .overflow_hidden()
                                            .whitespace_nowrap()
                                            .text_ellipsis()
                                            .child(format!("{} {}", i.surname(), i.given())),
                                    )
                                    .child(mono(k.to_string())),
                            )
                            .child(mono(i.years()))
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
                        )
                        .child("Cliquer une personne pour recentrer l'arbre"),
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
        let preview = self.media_preview(ed);
        let can_delete = true;
        let confirm = self.pending_delete.is_some() && self.pending_delete == self.selected;
        div().id("form-scroll").flex_1().overflow_y_scroll().child(
            v_flex()
                .p_4()
                .gap_3()
                .max_w(px(760.))
                .child(div().grid().grid_cols(2).gap_3().children(ed.fields.iter().map(|f| {
                    v_flex()
                        .gap_1()
                        .child(label_caps(f.label))
                        .child(Input::new(&f.state).small())
                        .when(!f.hint.is_empty(), |d| d.child(mono(f.hint)))
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
                        .when(can_delete, |d| {
                            d.child(
                                Button::new("delete")
                                    .danger()
                                    .small()
                                    .label(match (confirm, &ed.target) {
                                        (true, _) => "Confirmer la suppression",
                                        (false, Target::Place(_)) => "Retirer ce lieu partout",
                                        (false, _) => "Supprimer",
                                    })
                                    .on_click(cx.listener(|this, _, w, cx| this.delete_selected(w, cx))),
                            )
                        }),
                )
                .child(mono(
                    "Seuls les champs ci-dessus sont modifiés : les autres structures (citations, \
                     balises propriétaires…) sont conservées. Onglet GEDCOM pour tout le reste.",
                ))
                .when_some(preview, |d, p| d.child(p)),
        )
    }

    fn media_preview(&self, ed: &RecordEditor) -> Option<AnyElement> {
        let Target::Record { xref, kind: RecordKind::Media } = &ed.target else { return None };
        let doc = self.doc.as_ref()?;
        let m = Media::new(doc.ged.get(xref)?);
        let path = media_path(doc, &m);
        let el = match path {
            Some(p) if p.exists() && m.is_image() => v_flex()
                .gap_1()
                .child(label_caps("Aperçu"))
                .child(
                    div()
                        .rounded_md()
                        .border_1()
                        .border_color(theme::border())
                        .bg(theme::surface())
                        .p_2()
                        .child(img(p.clone()).max_w_full().max_h(px(560.)).object_fit(ObjectFit::Contain)),
                )
                .child(mono(p.display().to_string())),
            Some(p) => v_flex()
                .gap_1()
                .child(label_caps("Aperçu"))
                .child(div().text_color(theme::warn()).child(format!("Fichier introuvable : {}", p.display()))),
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
                    .child(mono("Niveau 0 = l'enregistrement. Changer l'identifiant met à jour tous les liens."))
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
                panel
                    .child(
                        v_flex()
                            .p_4()
                            .gap_1()
                            .border_b_1()
                            .border_color(theme::border())
                            .child(div().text_lg().font_weight(FontWeight::SEMIBOLD).child(g.record_label(key)))
                            .child(mono(format!(
                                "@{key}@ · {}",
                                node.and_then(|n| RecordKind::from_tag(&n.tag)).map(|k| k.label_fr()).unwrap_or("")
                            ))),
                    )
                    .children(self.render_actions(cx))
                    .children(self.render_unreferenced_files(cx))
                    .when(!outgoing.is_empty(), |d| d.child(self.render_links("Liens", outgoing, cx)))
                    .child(self.render_links("Utilisé par", refs, cx))
                    .into_any_element()
            }
        }
    }

    fn render_links(&self, title: &'static str, xrefs: Vec<String>, cx: &mut Context<Self>) -> impl IntoElement {
        let g = &self.doc.as_ref().unwrap().ged;
        v_flex()
            .p_4()
            .gap_1()
            .child(label_caps(format!("{title} · {}", xrefs.len())))
            .when(xrefs.is_empty(), |d| d.child(div().text_color(theme::muted()).child("Aucun")))
            .children(xrefs.into_iter().take(300).map(|x| {
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
                    .child(mono(kind.map(|k| View::for_kind(k).short()).unwrap_or("?")).w(px(36.)))
                    .child(div().flex_1().overflow_hidden().whitespace_nowrap().text_ellipsis().child(label))
                    .on_click(cx.listener(move |this, _, w, cx| this.goto(target.clone(), w, cx)))
            }))
    }

    fn render_individual_card(&self, g: &Gedcom, i: Individual<'_>, cx: &mut Context<Self>) -> impl IntoElement {
        let (sourced, total) = i.sourced_ratio();
        let pct = if total == 0 { 0. } else { sourced as f32 / total as f32 };
        let sosa = i.sosa();
        let initials: String = [i.given().chars().next(), i.surname().chars().next()].into_iter().flatten().collect();
        let lineage =
            sosa.as_deref().and_then(|s| s.parse().ok()).map(theme::lineage).unwrap_or(theme::border_strong());
        let mut events: Vec<Event> = i.events();
        events.sort_by_key(|e| e.date().map(|d| d.sort_key()).unwrap_or((i32::MAX, 0, 0)));
        let xref = i.xref().to_string();

        let header = h_flex()
            .p_4()
            .gap_3()
            .items_start()
            .border_b_1()
            .border_color(theme::border())
            .child(
                div()
                    .size(px(44.))
                    .flex_none()
                    .rounded_full()
                    .border_2()
                    .border_color(lineage)
                    .bg(theme::surface())
                    .flex()
                    .items_center()
                    .justify_center()
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(initials),
            )
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
                        "{} · {}{} · @{}@",
                        i.sex_label(),
                        if i.years().is_empty() { "dates inconnues".into() } else { i.years() },
                        sosa.map(|s| format!(" · Sosa {s}")).unwrap_or_default(),
                        xref
                    )))
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

        let events_el = v_flex()
            .p_4()
            .gap_3()
            .border_b_1()
            .border_color(theme::border())
            .child(label_caps("Événements"))
            .when(events.is_empty(), |d| d.child(div().text_color(theme::muted()).child("Aucun événement")))
            .children(events.into_iter().enumerate().map(|(n, e)| {
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
                            .child(div().font_weight(FontWeight::MEDIUM).child(label))
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
                                .on_click(cx.listener(move |this, _, w, cx| this.goto_place(target.clone(), w, cx))),
                        )
                    })
                    .when(sources.is_empty(), |d| {
                        d.child(div().text_xs().text_color(theme::warn()).child("Aucune source citée"))
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
                            .child(format!("Source : {title}"))
                            .on_click(cx.listener(move |this, _, w, cx| this.goto(target.clone(), w, cx)))
                    }))
            }));

        // Famille : parents, conjoints, enfants.
        let mut family: Vec<(&'static str, String)> = Vec::new();
        let (f, m) = g.parents(&xref);
        family.extend(f.map(|p| ("Père", p.xref().to_string())));
        family.extend(m.map(|p| ("Mère", p.xref().to_string())));
        for (spouse, kids) in g.spouses_and_children(&xref) {
            family.extend(spouse.map(|s| ("Conjoint", s.xref().to_string())));
            family.extend(kids.into_iter().map(|k| ("Enfant", k.xref().to_string())));
        }
        let family_el = v_flex()
            .p_4()
            .gap_1()
            .border_b_1()
            .border_color(theme::border())
            .child(label_caps("Famille"))
            .when(family.is_empty(), |d| d.child(div().text_color(theme::muted()).child("Aucun lien familial")))
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

        let media: Vec<String> = i.media_xrefs().iter().map(|s| s.to_string()).collect();
        let notes = i.notes();
        v_flex()
            .child(header)
            .child(events_el)
            .child(family_el)
            .when(!media.is_empty(), |d| d.child(self.render_links("Médias", media, cx)))
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
        let (saved, counts, unsourced) = match &self.doc {
            Some(d) => {
                let unsourced: usize = d
                    .ged
                    .records_of(RecordKind::Individual)
                    .map(|r| Individual::new(r).events().iter().filter(|e| !e.is_sourced()).count())
                    .sum();
                (
                    !d.dirty,
                    format!(
                        "{} individus · {} familles · {} sources",
                        d.ged.count(RecordKind::Individual),
                        d.ged.count(RecordKind::Family),
                        d.ged.count(RecordKind::Source)
                    ),
                    unsourced,
                )
            }
            None => (true, String::new(), 0),
        };
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
                .child(counts)
                .when(unsourced > 0, |d| {
                    d.child(div().text_color(theme::warn()).child(format!("{unsourced} événements sans source")))
                })
            })
            .child(div().flex_1())
            .when_some(self.status.clone(), |d, (msg, err)| {
                d.child(div().text_color(if err { theme::danger() } else { theme::muted() }).child(msg))
            })
            .when_some(self.selected.clone().filter(|_| self.view != View::Places), |d, x| {
                d.child(mono(format!("@{x}@")))
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
            .child(mono("Astuce : gedroots chemin/vers/fichier.ged"))
    }
}

impl Render for Workspace {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // Les raccourcis (Enregistrer…) passent par ce nœud : on le garde dans la chaîne de focus.
        if window.focused(cx).is_none() {
            window.focus(&self.focus, cx);
        }
        let body: AnyElement = if self.doc.is_some() {
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
            .on_action(cx.listener(Self::open_file))
            .on_action(cx.listener(Self::save_file))
            .on_action(cx.listener(Self::undo))
            .on_action(cx.listener(Self::new_file))
            .on_action(cx.listener(Self::open_analyses))
            .bg(theme::bg())
            .text_color(theme::text())
            .text_sm()
            .child(self.render_header(cx))
            .child(body)
            .child(self.render_status())
            .children(self.render_picker(cx))
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
