//! Main window: activity bar, list, central area (chart, record, raw GEDCOM), detail panel and
//! status bar, after the mockup.

use crate::act_editor::{ActEditor, ActEditorEvent};
use crate::commands::*;
use crate::editor::{FieldState, NamePart, NamesEditor, RecordEditor, Slot, Target, name_type_label};
use crate::images::picture;
use crate::map::{MapEvent, MapView};
use crate::theme;
use gedcom::analysis::timeline::{self, EntryKind};
use gedcom::query::Scope;
use gedcom::*;
use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::input::{Editor, Input, InputEvent, InputState, Textarea};
use gpui_kit::component::menu::{AppMenuBar, DropdownMenu, PopupMenuItem};
use gpui_kit::component::select::Select;
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::component::{Disableable, Icon, Sizable, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use menus::{From as MenuFrom, WithMenu};
use rust_i18n::t;
use std::path::PathBuf;
use std::rc::Rc;

mod add;
mod agent;
mod books;
mod chart;
mod commands;
mod event_detail;
mod export;
mod filters;
mod gallery;
mod history;
mod import;
mod insights;
mod media_files;
mod menus;
mod merging;
mod mobile;
mod new_tree;
mod pages;
mod photo;
mod place_merging;
mod places;
mod quality;
mod record_history;
mod relatives;
mod report;
mod research;
mod review;
mod review_book;
mod review_change;
mod saved;
mod settings;
pub(crate) mod tour;
mod tree;
mod version_history;
mod weblinks;

/// Window widths below which a column folds away (like libadwaita's breakpoints and Mail's
/// columns): first the sidebar, then the details panel. A folded column comes back over the others
/// with its View menu command.
const FOLD_SIDEBAR: f32 = 1200.;
const FOLD_INSPECTOR: f32 = 1100.;

/// Columns folded away in a narrow window.
#[derive(Clone, Copy, Default, PartialEq)]
struct Compact {
    sidebar: bool,
    inspector: bool,
}

/// Folded column shown over the others.
#[derive(Clone, Copy, PartialEq)]
enum Drawer {
    Sidebar,
    Inspector,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum View {
    Individuals,
    Sources,
    Places,
    Media,
    Repositories,
    Notes,
    Research,
    Quality,
    Stats,
    Books,
}

impl View {
    /// The main sidebar zones.
    const SIDEBAR: [View; 4] = [View::Individuals, View::Sources, View::Places, View::Media];
    /// The secondary group of the sidebar, under a separator.
    const SECONDARY: [View; 6] =
        [View::Repositories, View::Notes, View::Research, View::Quality, View::Stats, View::Books];

    fn label(self) -> SharedString {
        match self {
            View::Individuals => t!("cmd.individuals"),
            View::Sources => t!("cmd.sources"),
            View::Places => t!("cmd.places"),
            View::Media => t!("cmd.media_list"),
            View::Repositories => t!("cmd.repositories"),
            View::Notes => t!("cmd.notes"),
            View::Research => t!("ws.research"),
            View::Quality => t!("ws.quality"),
            View::Stats => t!("ws.stats"),
            View::Books => t!("books.view"),
        }
        .into()
    }

    fn icon(self) -> IconName {
        match self {
            View::Individuals => IconName::Users,
            View::Sources => IconName::BookOpen,
            View::Places => IconName::MapPin,
            View::Media => IconName::Image,
            View::Repositories => IconName::Landmark,
            View::Notes => IconName::StickyNote,
            View::Research => IconName::Compass,
            View::Quality => IconName::CircleCheck,
            View::Stats => IconName::ChartPie,
            View::Books => IconName::BookMarked,
        }
    }

    /// Individuals and places: the centre shows the tree or the map, and the record is the right
    /// panel (read, and edited in place).
    fn has_panel(self) -> bool {
        matches!(self, View::Individuals | View::Places)
    }

    /// Pages without a record page (Research, Quality, Statistics, Books).
    fn page(self) -> Option<View> {
        matches!(self, View::Research | View::Quality | View::Stats | View::Books).then_some(self)
    }

    /// Zone with search by criteria (the other lists filter the text).
    fn scope(self) -> Option<Scope> {
        Some(match self {
            View::Individuals => Scope::Individuals,
            View::Sources => Scope::Sources,
            View::Places => Scope::Places,
            View::Media => Scope::Media,
            // The focus of the leads is a search of individuals (`sosa:1-63`).
            View::Research => Scope::Individuals,
            _ => return None,
        })
    }

    fn kind(self) -> Option<RecordKind> {
        Some(match self {
            View::Individuals => RecordKind::Individual,
            View::Sources => RecordKind::Source,
            View::Media => RecordKind::Media,
            View::Repositories => RecordKind::Repository,
            View::Notes => RecordKind::SharedNote,
            View::Places | View::Research | View::Quality | View::Stats | View::Books => return None,
        })
    }

    fn for_kind(kind: RecordKind) -> View {
        match kind {
            // Unions are shown on each spouse's page.
            RecordKind::Individual | RecordKind::Family => View::Individuals,
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
    fn label(self) -> SharedString {
        match self {
            CenterTab::Tree => t!("cmd.show_tree"),
            CenterTab::Map => t!("ws.map"),
            CenterTab::Form => t!("cmd.show_form"),
            CenterTab::Raw => t!("cmd.show_raw"),
        }
        .into()
    }

    fn icon(self) -> IconName {
        match self {
            CenterTab::Tree => IconName::GitFork,
            CenterTab::Map => IconName::Map,
            CenterTab::Form => IconName::FileText,
            CenterTab::Raw => IconName::Code,
        }
    }
}

#[derive(Clone)]
struct Row {
    key: String,
    title: String,
    /// Part in bold (surname).
    strong: String,
    meta: String,
    dot: Option<Hsla>,
    flagged: bool,
}

/// Record just created and opened on its fields (see `Workspace::edit_fresh`).
struct Fresh {
    xref: String,
    /// Record shown before, shown again if the creation is cancelled.
    back: Option<String>,
    /// `Document::checkpoints` right after the creation.
    checkpoints: u64,
}

struct Document {
    ged: Gedcom,
    /// File of the tree; media paths are relative to its folder.
    path: Option<PathBuf>,
    /// Opened by File › Import: `path` is the imported file (or, for a GEDZIP, where the unzipped
    /// tree would be saved), which Save never overwrites without asking where.
    imported: bool,
    /// The sample tree (Help › Open Sample Tree): never saved.
    sample: bool,
    dirty: bool,
    dirty_before_checkpoint: bool,
    /// The tree as it was last opened or saved: Save stamps the records changed since.
    saved: Gedcom,
    /// The tree as it was opened, for File › Revert To › Last Opened.
    opened: Gedcom,
    /// The file as GEDRoots last read or wrote it, to notice that something else wrote it.
    disk: Option<(std::time::SystemTime, u64)>,
    /// When the last version was recorded, and whether a save happened since.
    versioned_at: Option<std::time::Instant>,
    unversioned: bool,
    /// The tree at the previous automatic save check: saved when it stays the same for a check.
    settling: Option<Gedcom>,
    /// Previous states, for Undo (the last one is the most recent).
    history: Vec<Gedcom>,
    /// Undone states, for Redo; cleared by any new change.
    future: Vec<Gedcom>,
    /// `future` as it was before the last restore point, given back if it is abandoned.
    future_before_checkpoint: Vec<Gedcom>,
    /// Restore points made and kept, to tell whether anything changed since a given moment.
    checkpoints: u64,
}

impl Document {
    /// Changes that Save would keep (the sample tree has none: it is never saved).
    fn unsaved(&self) -> bool {
        self.dirty && !self.sample
    }

    /// The file automatic saves write: a tree opened from a `.ged` (not imported, not new, not the
    /// sample tree).
    fn own_file(&self) -> Option<&std::path::Path> {
        self.path.as_deref().filter(|_| !self.imported && !self.sample)
    }

    /// Writes the tree to `path` (dates of the changed records, header), and records a version
    /// as `versioning` says, on another thread unless `wait`. Returns whether a version was
    /// recorded.
    fn write(&mut self, path: &std::path::Path, versioning: Versioning, wait: bool) -> std::io::Result<bool> {
        self.ged.ensure_tree_id();
        self.ged.stamp_changes(&self.saved);
        self.ged.stamp_header();
        gedcom::save(&self.ged, path)?;
        self.saved = self.ged.clone();
        self.disk = disk_stamp(path);
        self.settling = None;
        let due = self.versioned_at.is_none_or(|t| t.elapsed() >= VERSION_EVERY);
        let versioned = versioning != Versioning::Throttled || due;
        if versioned {
            let (kind, label, author) = versioning.kind();
            let kind = Some(kind);
            match wait {
                true => record_version_now(path, &self.ged, kind, label, author),
                false => record_version(path, &self.ged, kind, label, author),
            }
            self.versioned_at = Some(std::time::Instant::now());
        }
        self.unversioned = !versioned;
        Ok(versioned)
    }

    /// When the window closes or the app quits: saves the changes and records the last version.
    fn flush(&mut self) {
        let Some(path) = self.own_file().map(std::path::Path::to_path_buf) else { return };
        if self.unsaved() {
            match self.write(&path, Versioning::Now, true) {
                Ok(_) => self.dirty = false,
                Err(e) => eprintln!("save before closing: {}: {e}", path.display()),
            }
        } else if self.unversioned {
            record_version_now(&path, &self.ged, Some(gedcom::versions::Kind::Saved), None, None);
            self.unversioned = false;
        }
    }

    /// Name shown in the title bar: the file name, without its extension for an imported tree
    /// (not saved yet, like an untitled one).
    fn title(&self) -> String {
        if self.sample {
            return t!("sample.title").into();
        }
        let name = match self.imported {
            true => self.path.as_ref().and_then(|p| p.file_stem()),
            false => self.path.as_ref().and_then(|p| p.file_name()),
        };
        name.map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| t!("ws.untitled").into())
    }
}

/// Records the tree in the history folder beside its file, on its own thread: as it was found
/// on opening (`kind` none: first version, or changes made outside GEDRoots), or as saved. The
/// old versions are pruned after a save.
fn record_version(
    path: &std::path::Path,
    ged: &Gedcom,
    kind: Option<gedcom::versions::Kind>,
    label: Option<String>,
    author: Option<String>,
) {
    let (path, ged) = (path.to_path_buf(), ged.clone());
    std::thread::spawn(move || record_version_now(&path, &ged, kind, label, author));
}

/// [`record_version`] on the calling thread: when the window closes or the app quits, or before
/// the version history is shown.
fn record_version_now(
    path: &std::path::Path,
    ged: &Gedcom,
    kind: Option<gedcom::versions::Kind>,
    label: Option<String>,
    author: Option<String>,
) {
    let store = gedcom::versions::Store::for_file(path, ged.tree_id());
    let result = match kind {
        None => store.record_opened(ged).map(|_| ()),
        Some(k) => store.record(ged, k, label.as_deref(), author.as_deref()).and_then(|_| store.prune().map(|_| ())),
    };
    if let Err(e) = result {
        eprintln!("version history: {}: {e}", store.dir().display());
    }
}

/// How often the window looks for changes to save and for a file changed outside GEDRoots.
const AUTOSAVE_CHECK: std::time::Duration = std::time::Duration::from_secs(1);

/// While editing, a version is recorded at most this often; the file itself is saved after each
/// change.
const VERSION_EVERY: std::time::Duration = std::time::Duration::from_secs(5 * 60);

/// Modification time and size of a file, to notice that something else wrote it.
fn disk_stamp(path: &std::path::Path) -> Option<(std::time::SystemTime, u64)> {
    let m = std::fs::metadata(path).ok()?;
    Some((m.modified().ok()?, m.len()))
}

/// How a save records a version.
#[derive(Clone, PartialEq, Eq)]
enum Versioning {
    /// File › Save, window closed, app quit: a version now.
    Now,
    /// File › Save Version…: a version with this name, kept until deleted.
    Named(String),
    /// An earlier version brought back: its id, `opened` for Revert To › Last Opened, or
    /// `undo <id>` for the changes of a version taken back.
    Restored(String),
    /// A proposal of an AI agent accepted in To Review: what it did, and the agent.
    Agent(String, Option<String>),
    /// Before an operation that changes many records (merge, fixes of a whole kind).
    RestorePoint(String),
    /// Automatic save: a version if the last one is old enough.
    Throttled,
}

impl Versioning {
    /// Kind, label and author of the version.
    fn kind(&self) -> (gedcom::versions::Kind, Option<String>, Option<String>) {
        use gedcom::versions::Kind;
        match self {
            Versioning::Now | Versioning::Throttled => (Kind::Saved, None, None),
            Versioning::Named(n) => (Kind::Named, Some(n.clone()), None),
            Versioning::Restored(from) => (Kind::Restored, Some(from.clone()), None),
            Versioning::Agent(what, who) => (Kind::Agent, Some(what.clone()), who.clone()),
            Versioning::RestorePoint(what) => (Kind::RestorePoint, Some(what.clone()), None),
        }
    }
}

/// Folder and name proposed by Save for a tree not saved yet: next to the imported file, under a
/// name that does not replace it (`export copy.ged`); otherwise in the home folder.
fn save_as_default(imported: Option<&std::path::Path>) -> (PathBuf, String) {
    let Some(p) = imported else { return (dirs_home(), format!("{}.ged", t!("file.default_name"))) };
    let dir = p.parent().map(std::path::Path::to_path_buf).unwrap_or_else(dirs_home);
    let stem = p.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| t!("file.default_name").into());
    let name = match p.exists() {
        true => t!("file.copy_name", name = stem).into_owned(),
        false => stem,
    };
    (dir, format!("{name}.ged"))
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
    /// Sidebar shown (View › Show or Hide Sidebar) when the window is wide enough.
    sidebar: bool,
    /// Details panel shown (View › Show or Hide Details Panel) when the window is wide enough.
    inspector: bool,
    /// Narrow window: which columns are folded away, as of the last frame.
    compact: Compact,
    /// Narrow window: folded column shown over the others on request.
    drawer: Option<Drawer>,
    /// Raw GEDCOM tab opened on demand (View › Raw GEDCOM), closed when switching lists.
    raw_open: bool,
    /// Free chart: central person, generations loaded, panning.
    chart: std::cell::RefCell<chart::Chart>,
    picker: Option<relatives::Picker>,
    /// Image/PDF files of the `.ged` folder, for the attachment suggestions.
    media_files: Rc<Vec<String>>,
    /// Deletion waiting for confirmation (key of the item).
    pending_delete: Option<String>,
    /// Search: types and context of the media, recomputed when the file changes.
    media_index: Option<Rc<MediaIndex>>,
    /// Type chosen in the type bar of the Media zone (`None`: all).
    media_type: Option<String>,
    /// Suggestions under the search field, with their number of results.
    suggestions: Vec<(gedcom::query::Suggestion, usize)>,
    suggest_sel: usize,
    /// Suggestions closed (Esc) until the next keystroke.
    suggest_closed: bool,
    /// The search field had the focus at the last render.
    search_focused: bool,
    /// Menu of a clicked search token (date margin, name variants).
    chip_menu: Option<filters::ChipMenu>,
    /// Saved searches (`HEAD._GR_FIND`) and their number of results.
    saved: Rc<Vec<(gedcom::saved::SavedSearch, usize)>>,
    /// Saved search being renamed.
    saved_rename: Option<saved::SavedRename>,
    /// Guided record entry in progress (replaces the list, record and detail).
    act_editor: Option<(Entity<ActEditor>, Subscription)>,
    /// Records of the displayed source (read again on each refresh of the page).
    source_acts: Vec<Act>,
    /// Palette de commandes ouverte.
    palette: Option<commands::Palette>,
    /// Menu bar in the window (Linux, Windows); native on macOS.
    menu_bar: Option<Entity<AppMenuBar>>,
    /// Window title already applied.
    window_title: String,
    /// Frame being drawn on a photo.
    photo_draw: Option<photo::PhotoDraw>,
    photo_bounds: photo::PhotoBounds,
    /// Zoom of the image of the displayed media.
    photo_zoom: crate::viewer::Zoom,
    /// Person hovered on the photo or in the list of links.
    photo_hover: Option<String>,
    /// Displayed page of a multi-page media (from 1).
    media_page: usize,
    /// Media selected together (⌘ / Shift + click), in order, to merge them.
    media_marked: Vec<String>,
    /// Back / Forward, and last item viewed in each list.
    history: history::History,
    /// “Detach” buttons unfolded in an individual's panel. Add window (event, relatives) opened by
    /// a “+”.
    add_dialog: Option<add::AddDialog>,
    /// “New Tree” window (File › New Tree…).
    new_tree: Option<Box<new_tree::NewTreeDialog>>,
    /// “Media” window (gallery of an individual, an event, a source).
    gallery: Option<gallery::Gallery>,
    /// “To Review” page: records proposed by an AI agent, waiting for review.
    review: review::Review,
    /// Place field being typed in the record page.
    place_focus: Option<EntityId>,
    /// Real places suggested while typing a place.
    place_search: places::PlaceSearch,
    /// Coordinates of created places, written when the place enters the file.
    pending_places: Vec<places::PendingPlace>,
    /// “New Place” window.
    new_place: Option<places::NewPlace>,
    /// Help › Report an Issue window.
    report: Option<report::ReportDialog>,
    /// “Locate” on a place's page.
    locate: Option<places::Locate>,
    editor_subs: Vec<Subscription>,
    /// Row of the list targeted by the last right-click.
    menu_row: Rc<std::cell::RefCell<Option<String>>>,
    /// Scrolling of the list, to keep the displayed item visible.
    list_scroll: UniformListScrollHandle,
    /// Quality page: problems of the file.
    quality: quality::QualityState,
    research: research::ResearchState,
    /// Books page: the selected book's preview, the New Book window.
    books: books::BooksState,
    /// Relationship shown in a modal window (A, B).
    kinship: Option<(String, String)>,
    /// Merge window (possible duplicates, or two people chosen by the user).
    merge: Option<merging::MergeDialog>,
    place_merge: Option<place_merging::PlaceMergeDialog>,
    /// File › Revert To › Browse All Versions… window.
    version_history: Option<version_history::VersionHistory>,
    /// Edit › Record History… window.
    record_history: Option<record_history::RecordHistory>,
    /// Guided tour in progress.
    tour: Option<tour::Tour>,
    /// Chapters of the tour offered (Help › Take the Tour).
    tour_chooser: bool,
    /// The step's action was done: the tour moves on at the next frame.
    tour_pending: bool,
    /// Event shown read-only (click in a timeline).
    event_detail: Option<event_detail::EventDetail>,
    /// The record's fields are being edited (Edit › Edit Record); otherwise it is shown as text.
    editing: bool,
    /// Record just created and opened on its fields: Cancel takes the creation back.
    fresh: Option<Fresh>,
    /// Record page: also show the relatives' events (View › Relatives' Events).
    show_relative_events: bool,
    /// Statistics of the file (Statistics page).
    stats: Option<Rc<gedcom::analysis::stats::Stats>>,
    /// Versions of the recent files and search sites known to the window's menu bar.
    recent_gen: (u64, u64),
    _subs: Vec<Subscription>,
    /// Checks every [`AUTOSAVE_CHECK`] for changes to save and for a file changed elsewhere.
    _autosave: Task<()>,
}

impl Workspace {
    pub fn new(path: Option<PathBuf>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let filter = cx.new(|cx| InputState::new(window, cx).placeholder(t!("ws.search_people")));
        let sub = cx.subscribe_in(&filter, window, |this, _, ev: &InputEvent, window, cx| {
            if matches!(ev, InputEvent::Change) {
                this.on_search_changed(window, cx);
            }
        });
        let map = cx.new(|_| MapView::new());
        let map_sub = cx.subscribe_in(&map, window, Self::on_map_event);
        // Light / dark: follows the system's appearance.
        let appearance_sub = cx.observe_window_appearance(window, |_, window, cx| theme::sync(window, cx));
        // AI agents: they read the file of the last window activated.
        crate::agent::set_target(cx.entity().downgrade(), cx);
        let activation_sub = cx.observe_window_activation(window, |this, window, cx| {
            if window.is_window_active() {
                crate::agent::set_target(cx.entity().downgrade(), cx);
                crate::windows::set_active(cx.entity_id(), cx);
            } else {
                // Leaving the window saves its changes at once.
                this.autosave(cx);
            }
        });
        // Quitting the app (menu, shortcut, logging out) saves the changes first.
        let quit_sub = cx.on_app_quit(|this: &mut Self, _| {
            if let Some(doc) = &mut this.doc {
                doc.flush();
            }
            async {}
        });
        // Automatic save after each change, and changes made to the file outside GEDRoots.
        let autosave = cx.spawn_in(window, async move |this, cx| {
            loop {
                cx.background_executor().timer(AUTOSAVE_CHECK).await;
                if this.update_in(cx, |this, window, cx| this.autosave_tick(window, cx)).is_err() {
                    break;
                }
            }
        });
        // Mobile devices: a file closed with its window is no longer served.
        let me = cx.entity_id();
        let release_sub = cx.on_release(move |this: &mut Self, cx| {
            if let Some(doc) = &mut this.doc {
                doc.flush();
            }
            if let Some(share) = this.mobile_share() {
                Self::mobile_unpublish(&share, me, cx);
            }
        });
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
            sidebar: true,
            inspector: true,
            compact: Compact::default(),
            drawer: None,
            raw_open: false,
            chart: Default::default(),
            picker: None,
            media_files: Rc::new(Vec::new()),
            pending_delete: None,
            media_index: None,
            media_type: None,
            suggestions: Vec::new(),
            suggest_sel: 0,
            suggest_closed: false,
            search_focused: false,
            chip_menu: None,
            saved: Rc::new(Vec::new()),
            saved_rename: None,
            act_editor: None,
            source_acts: Vec::new(),
            palette: None,
            menu_bar: (!cfg!(target_os = "macos")).then(|| AppMenuBar::new(cx)),
            window_title: String::new(),
            photo_draw: None,
            photo_bounds: Default::default(),
            photo_zoom: Default::default(),
            photo_hover: None,
            media_page: 1,
            media_marked: Vec::new(),
            history: Default::default(),
            list_scroll: UniformListScrollHandle::new(),
            quality: quality::QualityState { stale: true, ..Default::default() },
            research: research::ResearchState::default(),
            books: books::BooksState::default(),
            kinship: None,
            merge: None,
            place_merge: None,
            version_history: None,
            record_history: None,
            tour: None,
            tour_chooser: false,
            tour_pending: false,
            event_detail: None,
            editing: false,
            fresh: None,
            show_relative_events: false,
            stats: None,
            recent_gen: (crate::recent::generation(), crate::commands::search_sites_generation()),
            menu_row: Rc::default(),
            place_focus: None,
            place_search: Default::default(),
            pending_places: Vec::new(),
            new_place: None,
            report: None,
            locate: None,
            editor_subs: Vec::new(),
            add_dialog: None,
            new_tree: None,
            gallery: None,
            review: review::Review::default(),
            _subs: vec![sub, map_sub, appearance_sub, activation_sub, release_sub, quit_sub],
            _autosave: autosave,
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
    // File
    // ------------------------------------------------------------------

    /// Opens a file received from outside (Finder, Dock).
    pub fn open_path(&mut self, path: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        self.load(path, window, cx);
    }

    /// True if the open document has unsaved changes.
    pub fn has_unsaved_changes(&self) -> bool {
        self.doc.as_ref().is_some_and(Document::unsaved)
    }

    fn load(&mut self, path: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        if import::is_gedzip(&path) {
            return self.import_path(path, window, cx);
        }
        match gedcom::load(&path) {
            // A file from other software opens as a copy, with the import wizard: its file is
            // never overwritten, like a Word document opened in Pages.
            Ok(ged) if !ged.written_by_gedroots() => {
                let file = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                self.open_imported(ged, path, file, String::new(), window, cx);
            }
            Ok(ged) => {
                crate::recent::add(&path, cx);
                self.set_document(ged, Some(path), false, window, cx);
                self.status = None;
                self.offer_tour(window, cx);
            }
            Err(e) => self.set_error("ws.open_failed", t!("ws.open_failed", path = path.display(), error = e)),
        }
        cx.notify();
    }

    /// Shows `ged` in place of the open document. An imported or new tree (`path` none) starts with
    /// changes to save.
    fn set_document(
        &mut self,
        ged: Gedcom,
        path: Option<PathBuf>,
        imported: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(share) = self.mobile_share() {
            Self::mobile_unpublish(&share, cx.entity_id(), cx);
        }
        let first = ged.root();
        let dirty = imported || path.is_none();
        if let Some(p) = path.as_ref().filter(|_| !imported) {
            record_version(p, &ged, None, None, None);
        }
        let disk = path.as_deref().filter(|_| !imported).and_then(disk_stamp);
        self.doc = Some(Document {
            saved: ged.clone(),
            opened: ged.clone(),
            disk,
            versioned_at: Some(std::time::Instant::now()),
            unversioned: false,
            settling: None,
            ged,
            path,
            imported,
            sample: false,
            dirty,
            dirty_before_checkpoint: dirty,
            history: Vec::new(),
            future: Vec::new(),
            future_before_checkpoint: Vec::new(),
            checkpoints: 0,
        });
        self.view = View::Individuals;
        self.selected = None;
        self.reset_history();
        self.rescan_media();
        self.rebuild_rows(cx);
        if let Some(x) = first {
            self.select(x, window, cx);
        }
        self.mobile_publish(cx);
    }

    /// Shows the sample tree (in a window of its own, see `main.rs`).
    pub(crate) fn open_sample(&mut self, offer_tour: bool, window: &mut Window, cx: &mut Context<Self>) {
        match crate::sample::open() {
            Ok((ged, path)) => {
                self.set_document(ged, Some(path), false, window, cx);
                if let Some(d) = &mut self.doc {
                    d.sample = true;
                }
                self.tab = CenterTab::Tree;
                if offer_tour {
                    self.offer_tour(window, cx);
                }
            }
            Err(e) => self.set_error("ws.open_failed", t!("ws.open_failed", path = t!("sample.title"), error = e)),
        }
        cx.notify();
    }

    /// File › New Tree…: here while this window is empty, otherwise in a window of its own.
    fn new_file(&mut self, _: &NewFile, window: &mut Window, cx: &mut Context<Self>) {
        match self.is_empty() {
            true => self.open_new_tree(window, cx),
            false => crate::windows::open_then(cx, |ws, window, cx| ws.open_new_tree(window, cx)),
        }
    }

    /// No tree in this window: the welcome screen.
    pub fn is_empty(&self) -> bool {
        self.doc.is_none()
    }

    /// Shows the tree at `path`: in its window if it is open, here while this window is empty,
    /// otherwise in a new window.
    fn open_tree(&mut self, path: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(w) = crate::windows::showing(&path, cx) {
            return crate::windows::bring_to_front(w, cx);
        }
        match self.is_empty() {
            true => self.load(path, window, cx),
            false => {
                crate::windows::open(Some(path), cx);
            }
        }
    }

    fn open_file(&mut self, _: &OpenFile, window: &mut Window, cx: &mut Context<Self>) {
        let rx = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some(t!("ws.open_prompt").into()),
        });
        cx.spawn_in(window, async move |this, cx| {
            if let Ok(Ok(Some(mut paths))) = rx.await
                && let Some(p) = paths.pop()
            {
                let _ = this.update_in(cx, |this, window, cx| this.open_tree(p, window, cx));
            }
        })
        .detach();
    }

    fn save_file(&mut self, _: &SaveFile, window: &mut Window, cx: &mut Context<Self>) {
        let Some(doc) = &self.doc else { return };
        match doc.path.clone().filter(|_| !doc.imported) {
            Some(p) => self.write_to(p, Versioning::Now, cx),
            None => {
                let (dir, name) = save_as_default(doc.path.as_deref());
                let rx = cx.prompt_for_new_path(&dir, Some(&name));
                cx.spawn_in(window, async move |this, cx| {
                    if let Ok(Ok(Some(p))) = rx.await {
                        let _ = this.update_in(cx, |this, _, cx| this.write_to(p, Versioning::Now, cx));
                    }
                })
                .detach();
            }
        }
    }

    fn write_to(&mut self, path: PathBuf, versioning: Versioning, cx: &mut Context<Self>) {
        let before = self.mobile_share();
        let Some(doc) = &mut self.doc else { return };
        let explicit = versioning == Versioning::Now;
        match doc.write(&path, versioning, false) {
            Ok(versioned) => {
                if doc.imported || doc.path.as_ref() != Some(&path) {
                    crate::recent::add(&path, cx);
                }
                doc.path = Some(path.clone());
                doc.imported = false;
                doc.dirty = false;
                self.rescan_media();
                if explicit {
                    self.set_status(t!("ws.saved", path = path.display()), false);
                }
                // Mobile devices get the saved version (with each version while editing); after
                // Save As, the previous file is closed.
                if let Some(old) = before.filter(|b| self.mobile_share().as_ref() != Some(b)) {
                    Self::mobile_unpublish(&old, cx.entity_id(), cx);
                }
                if versioned {
                    self.mobile_publish(cx);
                }
            }
            Err(e) => self.set_error("ws.save_failed", t!("ws.save_failed", error = e)),
        }
        cx.notify();
    }

    /// Saves the changes of a tree that has its own file now, without waiting for them to settle.
    fn autosave(&mut self, cx: &mut Context<Self>) {
        let Some(doc) = &self.doc else { return };
        if let (true, Some(path)) = (doc.unsaved(), doc.own_file().map(std::path::Path::to_path_buf)) {
            self.write_to(path, Versioning::Throttled, cx);
        }
    }

    /// Every [`AUTOSAVE_CHECK`]: picks up a file changed outside GEDRoots, then saves the changes
    /// once they stayed the same for a whole check (not in the middle of a series of edits).
    fn autosave_tick(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.check_outside_change(window, cx);
        let Some(doc) = &mut self.doc else { return };
        if !doc.unsaved() || doc.own_file().is_none() {
            doc.settling = None;
            return;
        }
        let settled = doc.settling.as_ref().is_some_and(|before| {
            before.header == doc.ged.header
                && before.records.len() == doc.ged.records.len()
                && before.records.iter().zip(&doc.ged.records).all(|(a, b)| std::sync::Arc::ptr_eq(a, b))
        });
        if settled {
            self.autosave(cx);
        } else {
            doc.settling = Some(doc.ged.clone());
        }
    }

    /// The open file was written by something else (another software, a sync service bringing a
    /// newer copy). Without changes here, the tree is reloaded; with changes here, they are kept
    /// and the other copy goes into the version history.
    fn check_outside_change(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(doc) = &self.doc else { return };
        let Some(path) = doc.own_file().map(std::path::Path::to_path_buf) else { return };
        let now = disk_stamp(&path);
        if now.is_none() || now == doc.disk {
            return;
        }
        // Being written (a sync service downloading it): read again at the next check.
        let Ok(theirs) = gedcom::load(&path) else { return };
        let doc = self.doc.as_mut().unwrap();
        doc.disk = now;
        // Touched without a change (a sync service rewriting the same content).
        if theirs.header == doc.saved.header && theirs.records == doc.saved.records {
            return;
        }
        if doc.unsaved() {
            record_version(&path, &theirs, Some(gedcom::versions::Kind::Outside), None, None);
            self.set_status(t!("ws.outside_kept"), false);
        } else {
            // Undo goes back to the tree before the reload.
            doc.history.push(std::mem::replace(&mut doc.ged, theirs));
            doc.future.clear();
            doc.saved = doc.ged.clone();
            record_version(&path, &doc.ged, None, None, None);
            self.rebuild_rows(cx);
            self.refresh_editor(window, cx);
            self.set_status(t!("ws.outside_reloaded"), false);
            self.mobile_publish(cx);
        }
        cx.notify();
    }

    fn undo(&mut self, _: &Undo, window: &mut Window, cx: &mut Context<Self>) {
        let Some(doc) = &mut self.doc else { return };
        if let Some(prev) = doc.history.pop() {
            doc.future.push(std::mem::replace(&mut doc.ged, prev));
            doc.dirty = true;
            self.rebuild_rows(cx);
            self.refresh_editor(window, cx);
            self.set_status(t!("ws.undone"), false);
        }
        cx.notify();
    }

    fn redo(&mut self, _: &Redo, window: &mut Window, cx: &mut Context<Self>) {
        let Some(doc) = &mut self.doc else { return };
        if let Some(next) = doc.future.pop() {
            doc.history.push(std::mem::replace(&mut doc.ged, next));
            doc.dirty = true;
            self.rebuild_rows(cx);
            self.refresh_editor(window, cx);
            self.set_status(t!("ws.redone"), false);
        }
        cx.notify();
    }

    fn checkpoint(&mut self) {
        if let Some(doc) = &mut self.doc {
            doc.history.push(doc.ged.clone());
            doc.checkpoints += 1;
            doc.future_before_checkpoint = std::mem::take(&mut doc.future);
            doc.dirty_before_checkpoint = doc.dirty;
            if doc.history.len() > 50 {
                doc.history.remove(0);
            }
            doc.dirty = true;
        }
    }

    /// Cancels the last restore point (edit refused or without effect).
    fn drop_checkpoint(&mut self) {
        if let Some(doc) = &mut self.doc {
            doc.history.pop();
            doc.checkpoints -= 1;
            doc.future = std::mem::take(&mut doc.future_before_checkpoint);
            doc.dirty = doc.dirty_before_checkpoint;
        }
    }

    fn set_status(&mut self, msg: impl Into<SharedString>, error: bool) {
        self.status = Some((msg.into(), error));
    }

    /// Shows an error. `key` names it in the context of Help › Report an Issue, which never
    /// includes the message itself (it can quote a name or a path).
    fn set_error(&mut self, key: &'static str, msg: impl Into<SharedString>) {
        crate::report::record_error(key);
        self.set_status(msg, true);
    }

    // ------------------------------------------------------------------
    // Navigation
    // ------------------------------------------------------------------

    /// Switches to a list, on the last item viewed (or the root for individuals).
    fn set_view(&mut self, view: View, window: &mut Window, cx: &mut Context<Self>) {
        if self.view != view {
            self.open_location(view, None, window, cx);
        }
    }

    /// Shows a list and an item (`None`: the last one viewed in this list).
    fn open_location(&mut self, view: View, key: Option<String>, window: &mut Window, cx: &mut Context<Self>) {
        self.new_place = None;
        self.review.open = false;
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
        self.raw_open = false;
        self.selected = None;
        self.editor = None;
        let placeholder = match view {
            View::Sources => t!("ws.search_sources"),
            View::Places => t!("ws.search_places"),
            View::Media => t!("ws.search_media"),
            View::Repositories => t!("ws.search_repositories"),
            View::Notes => t!("ws.search_notes"),
            View::Quality => t!("ws.search_quality"),
            View::Research => t!("research.focus_placeholder"),
            View::Individuals | View::Stats | View::Books => t!("ws.search_people"),
        };
        // Research: the field holds the focus of the leads, kept while the file is open.
        // A trailing space makes its last criterion a token.
        let initial = match view {
            View::Research if !self.research.focus.is_empty() => format!("{} ", self.research.focus),
            _ => String::new(),
        };
        self.filter.update(cx, |f, cx| {
            f.set_value(initial, window, cx);
            f.set_placeholder(placeholder, window, cx);
        });
        self.suggestions.clear();
        if view == View::Research {
            self.tokenize_search(window, cx);
        }
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
            // A record in progress belongs to the source being left.
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
        if self.view == View::Books {
            self.typeset_book(cx);
        }
        cx.notify();
    }

    /// Opens any record (link from a record page).
    pub(crate) fn goto(&mut self, xref: String, window: &mut Window, cx: &mut Context<Self>) {
        let Some(g) = self.doc.as_ref().map(|d| &d.ged) else { return };
        let Some(kind) = g.get(&xref).and_then(|n| RecordKind::from_tag(&n.tag)) else { return };
        // A union opens on the page of one of the spouses (failing that, of a child).
        let member = g
            .family(&xref)
            .map(|f| [f.husband(), f.wife()].into_iter().flatten().chain(f.children()).next().map(str::to_string));
        let xref = match member {
            Some(Some(x)) => x,
            Some(None) => return self.set_error("ws.empty_family", t!("ws.empty_family")),
            None => xref,
        };
        self.open_location(View::for_kind(kind), Some(xref), window, cx);
    }

    pub(super) fn goto_place(&mut self, name: String, window: &mut Window, cx: &mut Context<Self>) {
        self.open_location(View::Places, Some(name), window, cx);
    }

    /// Click on a marker, or coordinates found / set on the map.
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
                self.set_status(
                    t!(
                        "places.located",
                        place = short_place(name),
                        lat = la,
                        lon = lo,
                        uses = crate::tn!("places.uses", n)
                    ),
                    false,
                );
                cx.notify();
            }
        }
    }

    /// Goes back to the root individual, in the chart.
    fn goto_root(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(root) = self.doc.as_ref().and_then(|d| d.ged.root()) else { return };
        self.open_location(View::Individuals, Some(root), window, cx);
        self.tab = CenterTab::Tree;
    }

    /// Copies the name of an individual, the title of a record or the name of a place.
    fn copy_name(&mut self, key: &str, cx: &mut Context<Self>) {
        let Some(doc) = &self.doc else { return };
        let g = &doc.ged;
        let name = match g.individual(key) {
            Some(i) => i.display_name(),
            None if g.get(key).is_some() => g.record_label(key),
            None => key.to_string(),
        };
        cx.write_to_clipboard(ClipboardItem::new_string(name));
        self.set_status(t!("ws.name_copied"), false);
        cx.notify();
    }

    fn copy_id(&mut self, xref: &str, cx: &mut Context<Self>) {
        cx.write_to_clipboard(ClipboardItem::new_string(xref.to_string()));
        self.set_status(t!("ws.id_copied", id = xref), false);
        cx.notify();
    }

    fn refresh_editor(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.editing = false;
        self.fresh = None;
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
        // Place fields: suggestions of existing places while typing.
        self.place_focus = None;
        self.editor_subs =
            ed.fields
                .iter()
                .filter(|f| f.place)
                .filter_map(|f| match &f.state {
                    FieldState::Text(e) => Some(cx.subscribe_in(e, window, |this, input, ev: &InputEvent, _, cx| {
                        this.on_place_input(input, ev, cx)
                    })),
                    _ => None,
                })
                .collect();
        // The raw GEDCOM's Apply button lights up once the text changes.
        if let Some(raw) = &ed.raw {
            self.editor_subs.push(cx.subscribe(raw, |_, _, ev: &InputEvent, cx| {
                if let InputEvent::Change = ev {
                    cx.notify();
                }
            }));
        }
        self.editor = Some(ed);
        // The old fields disappear: give the focus back to the workspace so that keyboard shortcuts
        // stay active.
        window.focus(&self.focus, cx);
    }

    fn create_record(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(kind) = self.view.kind() else { return };
        self.checkpoint();
        let Some(doc) = &mut self.doc else { return };
        let x = doc.ged.create(kind);
        let back = self.selected.clone();
        self.filter.update(cx, |f, cx| f.set_value("", window, cx));
        self.rebuild_rows(cx);
        self.select(x.clone(), window, cx);
        if !self.view.has_panel() {
            self.tab = CenterTab::Form;
        }
        // A new record opens on its fields.
        self.edit_fresh(x, back, window, cx);
        self.set_status(t!("ws.created", kind = crate::i18n::record_kind(kind)), false);
    }

    /// Premier clic : demande confirmation ; second clic : supprime.
    fn delete_selected(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.view == View::Books {
            return self.delete_book(window, cx);
        }
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
                t!("ws.deleted", label = label)
            }
            None => {
                let n = doc.ged.delete_place(&key);
                crate::tn!("ws.place_removed", n)
            }
        };
        let pos = self.rows.iter().position(|r| r.key == key).unwrap_or(0);
        self.selected = None;
        self.editor = None;
        self.rebuild_rows(cx);
        // Select the neighbouring item to carry on.
        if let Some(next) = self.rows.get(pos.min(self.rows.len().saturating_sub(1))).map(|r| r.key.clone()) {
            self.select(next, window, cx);
        }
        self.set_status(msg, false);
        cx.notify();
    }

    fn apply_form(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        // A place renamed to another place of the file: the two are compared and merged.
        if let Some(ed) = &self.editor
            && let Target::Place(old) = &ed.target
            && let Some(new) = ed.place_name(cx).filter(|n| n != old)
            && self.doc.as_ref().is_some_and(|d| d.ged.places().iter().any(|p| p.name == new))
        {
            let old = old.clone();
            self.cancel_edit(window, cx);
            self.open_place_merge(vec![(old, new)], 0, cx);
            return;
        }
        let Some(ed) = self.editor.take() else { return };
        self.checkpoint();
        let doc = self.doc.as_mut().unwrap();
        let created: Vec<String> = self.pending_places.iter().map(|p| p.name.clone()).collect();
        let result = ed.apply_form(&mut doc.ged, &created, cx);
        let renamed = ed.place_name(cx);
        self.editor = Some(ed);
        if result.is_ok() {
            self.apply_pending_places();
        }
        let unchanged =
            self.doc.as_ref().is_some_and(|d| d.history.last().is_some_and(|prev| prev.records == d.ged.records));
        if result.is_ok() && unchanged {
            let doc = self.doc.as_mut().unwrap();
            let _ = doc;
            self.drop_checkpoint();
            self.set_status(t!("ws.no_change"), false);
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
                self.set_error("ws.edit_failed", e);
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
                self.set_status(t!("ws.gedcom_applied"), false);
            }
            Err(e) => {
                self.drop_checkpoint();
                ed.error = Some(e.clone().into());
                self.editor = Some(ed);
                self.set_error("ws.gedcom_invalid", t!("ws.gedcom_invalid", error = e));
            }
        }
        cx.notify();
    }

    // ------------------------------------------------------------------
    // Records of a source
    // ------------------------------------------------------------------

    /// “Enter a Record…”: in the displayed source, otherwise after choosing (or creating) the
    /// source.
    pub(super) fn enter_act(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(doc) = &self.doc else { return };
        let source = self
            .selected
            .clone()
            .filter(|x| self.view == View::Sources && doc.ged.get(x).is_some_and(|n| n.tag == "SOUR"));
        match source {
            Some(s) => self.open_act(s, None, window, cx),
            None => match self.selected_lead().cloned() {
                // Research: the record of the chosen lead.
                Some(lead) => self.enter_lead_record(lead, window, cx),
                None => self.open_picker(relatives::PickPurpose::ActSource { prefill: None }, window, cx),
            },
        }
    }

    /// Opens a new record of `source` with its event and principal people already chosen.
    pub(super) fn open_new_act(
        &mut self,
        source: String,
        event: &str,
        people: &[String],
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.selected.as_deref() != Some(source.as_str()) || self.view != View::Sources {
            self.goto(source.clone(), window, cx);
        }
        self.tab = CenterTab::Form;
        let Some(doc) = &self.doc else { return };
        let g = &doc.ged;
        let mut act = Act::new(&source, event);
        // The empty principals the event offers (the deceased, the two spouses) are filled first.
        let mut free =
            act.participants.iter().enumerate().filter(|(_, p)| p.role == ActRole::Principal).map(|(i, _)| i);
        let slots: Vec<usize> = people.iter().map_while(|_| free.next()).collect();
        for (n, x) in people.iter().enumerate() {
            let Some(ind) = g.individual(x) else { continue };
            let mut p = Participant::new(ActRole::Principal);
            p.set(PersonField::Surname, ind.surname());
            p.set(PersonField::Given, ind.given());
            if matches!(ind.sex(), "M" | "F") {
                p.set(PersonField::Sex, ind.sex());
            }
            p.choice = PersonChoice::Link(x.clone());
            match slots.get(n) {
                Some(&i) => act.participants[i] = p,
                None => act.participants.push(p),
            }
        }
        self.show_act_editor(act, window, cx);
    }

    /// Opens a record of the source in its page (`key`: existing record; otherwise an empty
    /// record).
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
                // New record: the event that the source contains, otherwise that of its last
                // record.
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
        self.show_act_editor(act, window, cx);
    }

    /// Opens record entry on `act`, in the page of its displayed source.
    pub(super) fn show_act_editor(&mut self, act: Act, window: &mut Window, cx: &mut Context<Self>) {
        let Some(doc) = &self.doc else { return };
        let (ged, path) = (doc.ged.clone(), doc.path.clone());
        let editor = cx.new(|cx| ActEditor::new(ged, path, act, window, cx));
        let sub = cx.subscribe_in(&editor, window, |this, editor, ev: &ActEditorEvent, window, cx| match ev {
            ActEditorEvent::Close => {
                this.act_editor = None;
                window.focus(&this.focus, cx);
                cx.notify();
            }
            ActEditorEvent::Save { act, next } => this.save_act(editor.clone(), act, *next, window, cx),
            ActEditorEvent::PlaceInput { input, changed } => this.place_input(input, *changed, cx),
            ActEditorEvent::PickPerson { idx, suggestions } => this.open_picker(
                relatives::PickPurpose::ActPerson { idx: *idx, suggestions: suggestions.clone() },
                window,
                cx,
            ),
            ActEditorEvent::PickMedia => this.open_picker(relatives::PickPurpose::ActMedia, window, cx),
        });
        // Place fields of the record: the same menu as everywhere else.
        let ws = cx.entity().downgrade();
        editor.update(cx, |e, _| {
            e.set_place_menu(Rc::new(move |input, cx| {
                ws.update(cx, |ws, cx| ws.place_suggestions(input, cx)).ok().flatten()
            }))
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
        let base = doc.path.as_ref().and_then(|p| p.parent()).map(std::path::Path::to_path_buf);
        // Place fields: the displayed label becomes the full `PLAC` value again.
        let created: Vec<String> = self.pending_places.iter().map(|p| p.name.clone()).collect();
        let mut act = act.clone();
        act.place = doc.ged.place_value(&act.place, &created);
        for p in &mut act.participants {
            for f in [PersonField::Residence, PersonField::Birthplace] {
                let v = p.get(f).to_string();
                if !v.is_empty() {
                    p.set(f, doc.ged.place_value(&v, &created));
                }
            }
        }
        let act = &act;
        match act.save(&mut doc.ged) {
            Ok(res) => {
                // Images of the record: copied (or moved) and named according to File Settings ›
                // Media.
                let note = match (&act.media, &res.media) {
                    (gedcom::MediaChoice::NewFiles(_), Some(m)) => {
                        media_files::store_act_media(&mut doc.ged, base.as_deref(), m)
                    }
                    _ => String::new(),
                };
                // Places created in the record: their coordinates.
                self.apply_pending_places();
                let ged = self.doc.as_ref().map(|d| d.ged.clone()).unwrap_or_default();
                self.set_status(res.summary() + &note, false);
                self.tour_did(tour::Advance::SaveAct, cx);
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

    /// Deletes a record: its citations (events and people stay).
    pub(super) fn delete_act(&mut self, source: String, key: ActKey, window: &mut Window, cx: &mut Context<Self>) {
        self.mutate(window, cx, |g| {
            let title = g.act(&source, &key).map(|a| a.title()).unwrap_or_default();
            g.remove_act(&source, &key);
            Ok((t!("ws.act_deleted", title = title).into_owned(), None))
        });
    }

    /// “Records” section of a source's page, with its “+”.
    fn render_acts(&self, source: &str, cx: &mut Context<Self>) -> AnyElement {
        let s = source.to_string();
        v_flex()
            .gap_1()
            .child(
                h_flex()
                    .gap_1()
                    .items_center()
                    .child(label_caps(if self.source_acts.is_empty() {
                        t!("editor.acts").into_owned()
                    } else {
                        t!("ws.acts_count", n = self.source_acts.len()).into_owned()
                    }))
                    .child(div().flex_1())
                    .child(
                        Button::new("add-act")
                            .ghost()
                            .xsmall()
                            .icon(IconName::Plus)
                            .tooltip(t!("cmd.enter_act").trim_end_matches('…').to_string())
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
                    .rounded(theme::RADIUS_SM)
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
    // List
    // ------------------------------------------------------------------

    /// The file may have changed: runs the checks again and rebuilds the list.
    fn rebuild_rows(&mut self, cx: &mut Context<Self>) {
        self.quality.stale = true;
        self.media_index = None;
        self.refresh_quality();
        self.refresh_research();
        self.refresh_saved();
        self.build_rows(cx);
    }

    /// Rebuilds the list (search, filter or file changed).
    fn build_rows(&mut self, cx: &mut Context<Self>) {
        let q = self.filter.read(cx).value().to_lowercase();
        if self.view == View::Quality {
            self.rows = Rc::new(self.quality_rows(q.trim()));
            return;
        }
        if self.view == View::Research {
            let focus = Self::committed_query(&self.filter.read(cx).value()).trim().to_string();
            if focus != self.research.focus {
                self.research.focus = focus;
                self.refresh_research();
            }
            self.rows = Rc::new(self.research_rows());
            return;
        }
        if self.view == View::Books {
            self.rows = Rc::new(self.book_rows());
            self.typeset_book(cx);
            return;
        }
        if self.view == View::Stats {
            // A single “row”, never shown: it is used for the history (⌘[).
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
        if self.view == View::Media {
            self.ensure_media_index();
        }
        let Some(doc) = &self.doc else {
            self.rows = Rc::new(Vec::new());
            return;
        };
        // Short labels of places according to the file's format (File Settings › Places).
        gedcom::set_place_layout(&doc.ged.place_form());
        // Displayed labels of event types (File Settings › Events).
        gedcom::set_event_labels(doc.ged.event_labels());
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
                            // the place is used by the filter
                        }
                        .with_search(place)
                    })
                    .collect();
                v.sort_by_cached_key(|r| (sort_key(&r.strong), sort_key(&r.title)));
                v
            }
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
                    // Record types and period: “birth”, “1795”…
                    .with_search(SourceContent::read(r).summary())
                })
                .collect(),
            View::Places => g
                .places()
                .into_iter()
                .map(|p| Row {
                    title: p.short(),
                    strong: String::new(),
                    meta: p.uses.to_string(),
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
                            None if files.len() > 1 => t!("ws.pages_short", n = files.len()).into_owned(),
                            None => String::new(),
                        },
                        dot: missing.then(theme::warn),
                        flagged: missing,
                    }
                    .with_search(
                        files
                            .iter()
                            .map(|f| f.path.to_string())
                            .chain(medium(r).and_then(medium_label).map(|l| l.into_owned()))
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
            View::Research | View::Quality | View::Stats | View::Books => Vec::new(),
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
        if let Some(scope) = self.view.scope() {
            // Search with criteria (born:, place:, type:, in:…), see gedcom::query.
            let query = Self::committed_query(&self.filter.read(cx).value());
            let media_type = self.active_media_type();
            if !query.trim().is_empty() || media_type.is_some() {
                let hits = g.search(scope, &query, self.media_index.as_deref(), media_type.as_deref());
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

    fn flag_label(&self) -> SharedString {
        match self.view {
            View::Sources => t!("ws.flag_no_media").into(),
            View::Places => t!("places.no_coords").into(),
            View::Media => t!("ws.flag_no_file").into(),
            _ => "".into(),
        }
    }
}

impl Row {
    /// Adds searchable text without displaying it (slipped into `meta` after an invisible
    /// separator).
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

/// Section title with a button (“+”) on the right.
fn section_header(title: impl Into<SharedString>, button: impl IntoElement) -> Div {
    h_flex().items_center().justify_between().child(label_caps(title)).child(button)
}

/// Dashed “+ …” slot to add a person.
fn add_slot(id: impl Into<SharedString>, label: impl Into<SharedString>) -> Stateful<Div> {
    h_flex()
        .id(ElementId::Name(id.into()))
        .p_2()
        .gap_2()
        .items_center()
        .rounded(theme::RADIUS_SM)
        .border_1()
        .border_dashed()
        .border_color(theme::border_strong())
        .text_color(theme::muted())
        .text_xs()
        .cursor_pointer()
        .hover(|d| d.border_color(theme::accent()).text_color(theme::accent()))
        .child(icon(IconName::Plus))
        .child(label.into())
}

fn label_caps(s: impl Into<SharedString>) -> Div {
    div().text_xs().font_weight(FontWeight::SEMIBOLD).text_color(theme::muted()).child(s.into())
}

/// “1 284”: thousands separated by a thin non-breaking space, the French way.
fn group_thousands(n: usize) -> String {
    let digits = n.to_string();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push('\u{202f}');
        }
        out.push(c);
    }
    out
}

/// Small figures (dates, years, counts), with tabular figures so they line up in columns.
fn mono(s: impl Into<SharedString>) -> Div {
    div()
        .font_family("monospace")
        .font_features(theme::tabular_nums())
        .text_xs()
        .text_color(theme::muted())
        .child(s.into())
}

/// Date column of a timeline row. A long date (French Republican calendar with its Gregorian
/// equivalent) wraps within half the row instead of crushing the label.
fn event_date(date: impl Into<SharedString>) -> Div {
    div()
        .flex_shrink(1.)
        .max_w(relative(0.5))
        .pt(px(2.))
        .text_right()
        .text_xs()
        .text_color(theme::muted())
        .font_features(theme::tabular_nums())
        .child(date.into())
}

fn dot(color: Hsla) -> Div {
    div().size(px(7.)).flex_none().rounded_full().bg(color)
}

/// Row of a timeline: label and date, then a plain second line (with whom, where). A single click
/// target; the links live in the event window and the context menu. Discreet dot: no source.
fn event_row(
    id: SharedString,
    label: String,
    date: String,
    sub: String,
    unsourced: bool,
    media: usize,
    attended: bool,
) -> Stateful<Div> {
    v_flex()
        .id(id)
        .gap_0p5()
        .px_2()
        .py_1()
        .rounded(theme::RADIUS_SM)
        .cursor_pointer()
        .hover(|d| d.bg(theme::hover()))
        .child(
            h_flex()
                .justify_between()
                .gap_2()
                .items_start()
                .child(
                    h_flex()
                        .flex_1()
                        .gap_2()
                        .items_center()
                        .min_w_0()
                        .child(
                            div()
                                .min_w_0()
                                .font_weight(FontWeight::MEDIUM)
                                .when(attended, |d| d.italic())
                                .overflow_hidden()
                                .whitespace_nowrap()
                                .text_ellipsis()
                                .child(label),
                        )
                        .when(unsourced, |d| d.child(dot(theme::muted()))),
                )
                .child(event_date(date)),
        )
        .when(!sub.is_empty() || media > 0, |d| {
            d.child(
                h_flex()
                    .justify_between()
                    .gap_2()
                    .text_color(theme::muted())
                    .child(div().min_w_0().overflow_hidden().whitespace_nowrap().text_ellipsis().child(sub))
                    .when(media > 0, |d| {
                        d.child(
                            h_flex()
                                .flex_none()
                                .gap_1()
                                .items_center()
                                .text_xs()
                                .child(icon(IconName::Image).size_3p5())
                                .child(media.to_string()),
                        )
                    }),
            )
        })
}

fn icon(name: IconName) -> Icon {
    Icon::new(name).size(px(14.))
}

/// Portrait of the individual (first linked image media), or their initials.
fn avatar(doc: &Document, i: &Individual<'_>, size: f32, ring: Hsla) -> Div {
    let base = div()
        .size(px(size))
        .flex_none()
        .rounded_full()
        .border_2()
        .border_color(ring)
        .bg(theme::surface())
        .flex()
        .items_center()
        .justify_center();
    let portrait = doc.ged.portrait(i.xref());
    // Person placed on another page of a multi-page media: that page.
    let page = |m: &Media| pages::page_path(doc, m, doc.ged.link_page(i.xref(), m.xref()));
    // The initials stay underneath while the portrait loads: the box never appears empty.
    let initials: String = [i.given().chars().next(), i.surname().chars().next()].into_iter().flatten().collect();
    let base = base.font_weight(FontWeight::SEMIBOLD).when(size < 32., |d| d.text_xs()).child(initials);
    match portrait.and_then(|m| page(&m)).filter(|p| p.exists()) {
        Some(p) => {
            // The link's crop (CROP) frames the face on a group photo.
            let crop = portrait.and_then(|m| doc.ged.link_crop(i.xref(), m.xref()));
            // GPUI's mask is rectangular: it is the image itself that is rounded, inside the ring.
            let inner = size - 4.;
            base.relative()
                .child(photo::linked_img(p, crop, inner, inner, true).rounded_full().absolute().top_0().left_0())
        }
        None => base,
    }
}

impl Workspace {
    /// Top bar. On macOS it replaces the title bar: the traffic lights sit in it (see `main.rs`)
    /// and it is used to move the window. The file name only appears here, centered, followed by a
    /// dot while there are changes to save.
    fn render_header(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let mac = cfg!(target_os = "macos");
        let title = self.doc.as_ref().map(|d| {
            let name = d.title();
            if d.unsaved() { format!("{name} •") } else { name }
        });
        h_flex()
            .id("header")
            .h(px(40.))
            .flex_none()
            .px_3()
            .when(mac, |d| {
                d.pl(px(80.)).on_mouse_down(MouseButton::Left, |e, window, _| {
                    if e.click_count == 2 {
                        window.titlebar_double_click();
                    } else {
                        window.start_window_move();
                    }
                })
            })
            .items_center()
            .bg(theme::panel())
            .border_b_1()
            .border_color(theme::border())
            .child(
                h_flex()
                    .flex_none()
                    .gap_3()
                    .items_center()
                    .child(
                        h_flex()
                            .gap_2()
                            .items_center()
                            .font_weight(FontWeight::SEMIBOLD)
                            // Linux, Windows: the GEDRoots menu, first of the bar, names the app.
                            .child(img("brand/icon-micro.svg").size(px(16.))),
                    )
                    // Linux, Windows: menus in the window (native on macOS).
                    .children(self.menu_bar.clone().map(|m| div().h_full().child(m)))
                    // Arrows: on macOS after the traffic lights (Finder, Safari); elsewhere, next
                    // to the file name in the center (VS Code, Zed), with the menus on the left.
                    .when(mac && self.doc.is_some(), |d| d.child(self.render_history_buttons(cx))),
            )
            // The file name is centred in the room the menus leave, and shortened rather than
            // running under them in a narrow window.
            .child(h_flex().flex_1().min_w_0().px_3().justify_center().children(title.map(|t| {
                h_flex()
                    .min_w_0()
                    .gap_2()
                    .items_center()
                    .when(!mac, |d| d.child(self.render_history_buttons(cx)))
                    .child(div().min_w_0().truncate().text_sm().font_weight(FontWeight::MEDIUM).child(t))
            })))
            .child(
                h_flex()
                    .flex_none()
                    .max_w(px(320.))
                    .gap_3()
                    .items_center()
                    .justify_end()
                    .children(self.render_create_button(cx)),
            )
    }

    /// The top bar's Create button: creates what the page lists (`⌘N`), named after it.
    fn render_create_button(&self, cx: &mut Context<Self>) -> Option<impl IntoElement> {
        let label = self.create_label()?;
        Some(
            Button::new("create")
                .primary()
                .xsmall()
                .icon(IconName::Plus)
                .label(label.clone())
                .tooltip_with_action(label, &Create, Some("Workspace"))
                // A click does not move the window (macOS).
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .on_click(cx.listener(|this, _, w, cx| this.create_here(w, cx))),
        )
    }

    /// The status bar, at the bottom of the window: the last message (in `danger()` for an
    /// error) and where the update stands. It stays when empty, so the layout does not jump.
    fn render_status_bar(&self) -> impl IntoElement {
        h_flex()
            .flex_none()
            .h(px(24.))
            .px_3()
            .gap_3()
            .items_center()
            .border_t_1()
            .border_color(theme::border())
            .bg(theme::panel())
            .text_xs()
            .child(div().flex_1().min_w_0().children(self.status.clone().map(|(msg, err)| {
                div().truncate().text_color(if err { theme::danger() } else { theme::muted() }).child(msg)
            })))
            .children(self.render_update_notice())
    }

    /// Where the update stands, in the status bar: the answer to Check for Updates, and
    /// once a new version is downloaded, the button that installs it (like Zed and VS Code).
    fn render_update_notice(&self) -> Option<impl IntoElement> {
        use crate::updates::State;
        let (msg, err, restart): (SharedString, bool, bool) = match crate::updates::state() {
            State::Idle | State::Downloading { manual: false, .. } => return None,
            State::Checking => (t!("updates.checking").into(), false, false),
            State::UpToDate => (t!("updates.up_to_date", version = env!("CARGO_PKG_VERSION")).into(), false, false),
            State::Downloading { version, percent, .. } => {
                (t!("updates.downloading", version = version, percent = percent).into(), false, false)
            }
            State::Ready { version } => (t!("updates.ready", version = version).into(), false, true),
            State::Failed => (t!("updates.failed").into(), true, false),
            State::SaveFirst { .. } => (t!("updates.save_first").into(), true, true),
        };
        Some(
            h_flex()
                .flex_none()
                .gap_2()
                .items_center()
                .child(div().truncate().text_color(if err { theme::danger() } else { theme::muted() }).child(msg))
                .when(restart, |d| {
                    d.child(
                        Button::new("restart-to-update")
                            .primary()
                            .xsmall()
                            .label(t!("cmd.restart_to_update"))
                            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                            .on_click(|_, _, cx| crate::updates::check_now(cx)),
                    )
                }),
        )
    }

    /// “Back / Forward” arrows: go back to the place left (an individual after opening one of their
    /// places, for example). The tooltip names the destination.
    fn render_history_buttons(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let arrow = |back: bool| {
            let dest = self.next_label(back);
            let (id, icon, verb) = if back {
                ("history-back", IconName::ChevronLeft, t!("cmd.back"))
            } else {
                ("history-forward", IconName::ChevronRight, t!("cmd.forward"))
            };
            let action: &dyn Action = if back { &GoBack } else { &GoForward };
            Button::new(id)
                .ghost()
                .xsmall()
                .icon(icon)
                .disabled(dest.is_none())
                .when_some(dest, |b, dest| {
                    b.tooltip_with_action(t!("ws.history_tip", verb = verb, dest = dest), action, Some("Workspace"))
                })
                .on_click(cx.listener(
                    move |this, _, w, cx| {
                        if back { this.go_back(w, cx) } else { this.go_forward(w, cx) }
                    },
                ))
        };
        h_flex()
            .gap_0p5()
            .items_center()
            // A click on an arrow does not move the window (macOS).
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(arrow(true))
            .child(arrow(false))
    }

    /// Sidebar: the four zones, named, with their count, then a secondary group (repositories and
    /// notes when the file has some, quality, statistics), like Mail's or Finder's sections. It
    /// launches no action; the commands are in the menus.
    fn render_sidebar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let doc = self.doc.as_ref();
        // Active zone filtered: its counter gives the number of results.
        let filtered = self.is_filtered(cx);
        let count = |v: View| -> Option<usize> {
            let d = doc?;
            Some(match v {
                // Leads still to search in the focus, whatever the field shows.
                View::Research => return self.research_count(),
                _ if v == self.view && filtered => self.rows.len(),
                View::Places => d.ged.places().len(),
                // Only once the checks have run: a stale count would mislead.
                View::Quality if self.quality.stale => return None,
                View::Quality => self.quality.issues.iter().filter(|i| !i.ignored).count(),
                View::Stats => return None,
                View::Books => self.book_count(),
                v => d.ged.count(v.kind()?),
            })
        };
        let zone = |v: View, n: Option<usize>, cx: &mut Context<Self>| {
            let active = v == self.view;
            h_flex()
                .id(SharedString::from(format!("zone-{}", v.label())))
                .px_2()
                .py_1()
                .gap_2()
                .items_center()
                .rounded(theme::RADIUS_SM)
                .cursor_pointer()
                .when(active, |d| d.bg(theme::accent_soft()).font_weight(FontWeight::SEMIBOLD))
                .when(!active, |d| d.hover(|d| d.bg(theme::hover())))
                .child(Icon::new(v.icon()).size(px(16.)).text_color(if active {
                    theme::accent()
                } else {
                    theme::muted()
                }))
                .child(div().flex_1().min_w_0().truncate().child(v.label()))
                .children(n.map(|n| div().text_xs().text_color(theme::muted()).child(group_thousands(n))))
                .on_click(cx.listener(move |this, _, w, cx| this.show_view(v, w, cx)))
        };
        let primary: Vec<_> = View::SIDEBAR.into_iter().map(|v| zone(v, count(v), cx)).collect();
        // Repositories and notes only when the file has some (or one is open).
        let secondary: Vec<_> = View::SECONDARY
            .into_iter()
            .map(|v| (v, count(v)))
            .filter(|&(v, n)| {
                v == self.view || !matches!(v, View::Repositories | View::Notes | View::Books) || n.unwrap_or(0) > 0
            })
            .map(|(v, n)| zone(v, n, cx))
            .collect();
        v_flex()
            .id("sidebar")
            .relative()
            .w(px(184.))
            .flex_none()
            .overflow_y_scroll()
            .p_2()
            .pt_3()
            .gap_0p5()
            .bg(theme::panel())
            .border_r_1()
            .border_color(theme::border())
            .children(primary)
            .when(doc.is_some(), |d| d.child(div().h(px(1.)).my_1().bg(theme::border())).children(secondary))
            .children(self.render_review_zone(cx))
            .children(self.render_saved(cx))
            .children(self.tour_anchor(tour::Target::Sidebar, cx))
    }

    /// Whether the current view has a details panel (individuals and places, record selected).
    fn has_inspector(&self) -> bool {
        self.doc.is_some()
            && self.selected.is_some()
            && self.act_editor.is_none()
            && !self.review.open
            && matches!(self.view, View::Individuals | View::Places)
    }

    /// View › Show or Hide Sidebar / Details Panel: in a wide window the column is docked; in a
    /// narrow one it is folded away and the command shows it over the others.
    fn toggle_column(&mut self, which: Drawer, cx: &mut Context<Self>) {
        let folded = match which {
            Drawer::Sidebar => self.compact.sidebar,
            Drawer::Inspector => self.compact.inspector,
        };
        if folded {
            self.drawer = if self.drawer == Some(which) { None } else { Some(which) };
        } else {
            match which {
                Drawer::Sidebar => self.sidebar = !self.sidebar,
                Drawer::Inspector => self.inspector = !self.inspector,
            }
        }
        cx.notify();
    }

    /// Folded column shown over the others, with a shadow since it floats. A click outside, `Esc`
    /// or choosing a zone puts it away.
    fn render_drawer(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let which = self.drawer?;
        let column: AnyElement = match which {
            Drawer::Sidebar => div()
                .absolute()
                .top_0()
                .bottom_0()
                .left_0()
                .flex()
                .shadow_lg()
                .occlude()
                // A zone or a saved search chosen: the drawer has done its job.
                .on_mouse_up(
                    MouseButton::Left,
                    cx.listener(|this, _, _, cx| {
                        this.drawer = None;
                        cx.notify();
                    }),
                )
                .child(self.render_sidebar(cx))
                .into_any_element(),
            Drawer::Inspector => div()
                .absolute()
                .top_0()
                .bottom_0()
                .right_0()
                .flex()
                .shadow_lg()
                .child(self.render_detail(cx))
                .into_any_element(),
        };
        Some(
            div()
                .absolute()
                .size_full()
                .child(div().id("drawer-outside").absolute().size_full().on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _, _, cx| {
                        this.drawer = None;
                        cx.notify();
                    }),
                ))
                .child(column)
                .into_any_element(),
        )
    }

    fn render_list(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let rows = self.rows.clone();
        // A dot only when it means something (Sosa ancestor, missing file); the other rows keep its
        // room so names stay aligned.
        let dotted = rows.iter().any(|r| r.dot.is_some());
        let selected = self.selected.clone();
        let marked: Rc<Vec<String>> = Rc::new(self.media_marked.clone());
        let multi = self.view == View::Media;
        let entity = cx.entity().downgrade();
        let menu_row = self.menu_row.clone();
        let flag = self.flag_label();
        let only = self.only_flagged;
        v_flex()
            .w(px(300.))
            .flex_none()
            .relative()
            .bg(theme::bg())
            .border_r_1()
            .border_color(theme::border())
            .child(self.render_search_field(cx))
            .child(self.render_search_notes(cx))
            .when(self.view == View::Media, |d| d.child(self.render_media_types(cx)))
            // Active filter (View › Filter List): reminder, clickable to remove it.
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
                                        .when(dotted, |d| d.child(dot(r.dot.unwrap_or(transparent_black()))))
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
                                                        div()
                                                            .flex_none()
                                                            .font_weight(FontWeight::SEMIBOLD)
                                                            .child(r.strong.clone()),
                                                    )
                                                })
                                                .child(div().min_w_0().truncate().child(r.title.clone())),
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
                                                // Media: ⌘ / Ctrl + click or Shift + click to
                                                // select several.
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
                                        // Right-click: the list's menu (below) targets this row.
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
                    // A single menu for the whole list: the targeted row's, or below the rows,
                    // create an item.
                    .with_menu(self.list_menu(cx)),
            )
            // Search suggestions, over the list.
            .children(self.render_suggestions(cx))
            .children(self.render_chip_menu(cx))
    }

    fn render_center(&self, cx: &mut Context<Self>) -> AnyElement {
        if self.view == View::Quality {
            return v_flex().flex_1().min_w_0().bg(theme::bg()).child(self.render_quality(cx)).into_any_element();
        }
        if self.view == View::Research {
            return v_flex().flex_1().min_w_0().bg(theme::bg()).child(self.render_research(cx)).into_any_element();
        }
        if self.view == View::Stats {
            return self.render_stats(cx);
        }
        if self.view == View::Books {
            return self.render_books(cx);
        }
        // Raw GEDCOM: a tab only when it was asked for. With no other view than the record, no tab
        // bar.
        // Individuals and places: the record is the right panel, not a tab.
        let mut tabs: Vec<CenterTab> = match self.view {
            View::Individuals => vec![CenterTab::Tree],
            View::Places => vec![CenterTab::Map],
            _ => vec![CenterTab::Form],
        };
        if self.raw_open && self.view != View::Places {
            tabs.push(CenterTab::Raw);
        }
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
        let strip = tabs.len() > 1;
        v_flex()
            .flex_1()
            .min_w_0()
            .bg(theme::bg())
            .when(strip, |d| {
                d.child(
                    // Tabs in Zed's style: the active tab takes the content's background and opens
                    // onto it (no bottom line); the others stay on the bar.
                    h_flex()
                        .h(px(32.))
                        .flex_none()
                        .bg(theme::panel())
                        .children(tabs.into_iter().enumerate().map(|(i, t)| {
                            let active = t == tab;
                            div()
                                .id(SharedString::from(format!("tab-{}", t.label())))
                                .h_full()
                                .px_3()
                                .flex()
                                .items_center()
                                .gap_2()
                                .text_sm()
                                .cursor_pointer()
                                .border_color(theme::border())
                                .border_r_1()
                                .when(i > 0 && active, |d| d.border_l_1().ml(px(-1.)))
                                .map(|d| {
                                    if active {
                                        d.bg(theme::bg())
                                    } else {
                                        d.border_b_1().hover(|d| d.bg(theme::hover()))
                                    }
                                })
                                .text_color(if active { theme::text() } else { theme::muted() })
                                // Tooltip: the tab's shortcut.
                                .tooltip(move |w, cx| {
                                    let tip = Tooltip::new(t.label());
                                    match t {
                                        CenterTab::Tree => tip.action(&ShowTree, Some("Workspace")),
                                        CenterTab::Form => tip.action(&ShowForm, Some("Workspace")),
                                        CenterTab::Raw => tip.action(&ShowRaw, Some("Workspace")),
                                        CenterTab::Map => tip,
                                    }
                                    .build(w, cx)
                                })
                                .child(icon(t.icon()).size(px(14.)).text_color(if active {
                                    theme::text()
                                } else {
                                    theme::muted()
                                }))
                                // Same label as the View menu command that opens the tab.
                                .child(t.label())
                                // L'onglet GEDCOM brut se referme.
                                .when(t == CenterTab::Raw, |d| {
                                    d.child(
                                        div()
                                            .id("close-raw")
                                            .rounded(theme::RADIUS_SM)
                                            .hover(|d| d.bg(theme::hover()))
                                            .child(icon(IconName::Close).size(px(12.)).text_color(theme::muted()))
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                cx.stop_propagation();
                                                this.close_raw(cx);
                                            })),
                                    )
                                })
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.tab = t;
                                    cx.notify();
                                }))
                        }))
                        .child(div().flex_1().h_full().border_b_1().border_color(theme::border())),
                )
            })
            .child(body)
            .into_any_element()
    }

    fn render_empty_center(&self) -> impl IntoElement {
        div().flex_1()
    }

    fn render_form(&self, ed: &RecordEditor, cx: &mut Context<Self>) -> impl IntoElement {
        let preview = self.media_preview(ed, cx);
        let confirm = self.pending_delete.is_some() && self.pending_delete == self.selected;
        let editable = !ed.fields.is_empty() || !ed.pickers.is_empty() || ed.long_text.is_some();
        let editing = self.editing && editable;
        // Deletion: Edit › Delete Item, then confirmation here.
        let confirm_buttons = confirm.then(|| self.delete_buttons(matches!(ed.target, Target::Place(_)), cx));
        // Sources, media, repositories and notes are read whole in the centre, with no side panel:
        // their images, web links and links follow the record.
        let record = match &ed.target {
            Target::Record { xref, kind } if self.view != View::Individuals => Some((xref.clone(), *kind)),
            _ => None,
        };
        let sections = record.as_ref().map(|(x, k)| self.record_sections(x, *k, cx)).unwrap_or_default();
        // One readable column for every record, photo included (View › Zoom enlarges it).
        let column = v_flex()
            .w_full()
            .max_w(px(720.))
            .px_10()
            .py_8()
            .gap_6()
            .when(editing, |d| d.child(self.render_form_fields(ed, cx)))
            .when(!editing && editable, |d| d.child(self.render_form_read(ed, cx)))
            .when(editing, |d| d.child(self.edit_buttons(cx).justify_start()))
            .children(confirm_buttons)
            .when_some(
                match &ed.target {
                    Target::Record { xref, kind: RecordKind::Source } => Some(xref.clone()),
                    _ => None,
                },
                |d, s| d.child(self.render_acts(&s, cx)),
            )
            .when_some(preview, |d, p| d.child(p))
            .children(sections);
        // Dropping photos or documents on a source opens the import preview of the Media window
        // (on a media, the preview takes them as pages).
        let column = match &record {
            Some((x, RecordKind::Source)) => {
                let (record, entity) = (x.clone(), cx.entity().downgrade());
                column.drag_over::<ExternalPaths>(|d, _, _, _| d.bg(theme::accent_soft())).on_drop(
                    move |paths: &ExternalPaths, window, cx| {
                        let record = record.clone();
                        let _ = entity.update(cx, |this, cx| {
                            let files = paths.paths().to_vec();
                            this.open_gallery_with_files(GalleryTarget::Record(record), files, window, cx);
                        });
                    },
                )
            }
            _ => column,
        };
        div().id("form-scroll").flex_1().overflow_y_scroll().child(h_flex().w_full().justify_center().child(column))
    }

    /// What follows a record read in the centre: images, web links, the records it links to and
    /// those that use it.
    fn record_sections(&self, key: &str, kind: RecordKind, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let Some(doc) = self.doc.as_ref() else { return Vec::new() };
        let g = &doc.ged;
        let mut out = Vec::new();
        // The repository is already under a source's title.
        let repository = (kind == RecordKind::Source)
            .then(|| g.get(key).and_then(|n| n.child("REPO")).and_then(|r| r.pointer()).map(str::to_string))
            .flatten();
        if kind == RecordKind::Source {
            out.extend(self.render_gallery(key, true, true, cx).map(|d| d.into_any_element()));
        }
        out.extend(self.render_web_links(key, cx).map(|d| d.p_0().border_b_0().into_any_element()));
        let outgoing: Vec<String> = g
            .get(key)
            .map(|n| {
                let mut v: Vec<String> = Vec::new();
                for p in n.pointers() {
                    // Web pages are in “Web Links”, images above.
                    let skip = p == key
                        || repository.as_deref() == Some(p.as_str())
                        || weblinks::is_web_media(g, &p)
                        || (kind == RecordKind::Source && g.images_of(key).iter().any(|m| m.xref() == p));
                    if !skip && !v.contains(&p) {
                        v.push(p);
                    }
                }
                v
            })
            .unwrap_or_default();
        if !outgoing.is_empty() {
            out.push(self.render_links(t!("ws.links").into(), outgoing, cx).p_0().into_any_element());
        }
        let refs: Vec<String> = g.referrers(key).iter().filter_map(|r| r.xref.clone()).collect();
        // Photo: the “Used By” list is under the image, with the frames.
        let under_photo = kind == RecordKind::Media && self.photo_file(key).is_some();
        if !refs.is_empty() && !under_photo {
            out.push(self.render_links(t!("ws.used_by").into(), refs, cx).p_0().into_any_element());
        }
        out
    }

    /// Fields of the record being edited (Edit › Edit Record).
    fn render_form_fields(&self, ed: &RecordEditor, cx: &mut Context<Self>) -> impl IntoElement {
        v_flex()
            .gap_3()
            .when(!ed.pickers.is_empty(), |d| {
                d.child(div().grid().grid_cols(2).gap_3().children(ed.pickers.iter().map(|p| {
                    v_flex().gap_1().child(label_caps(p.label.clone())).child(
                        Select::new(&p.state)
                            .small()
                            .cleanable(true)
                            .placeholder(p.placeholder.clone())
                            .search_placeholder(t!("event_dialog.search")),
                    )
                })))
            })
            .child(div().grid().grid_cols(2).gap_3().children(ed.fields.iter().map(|f| self.render_field(f, cx))))
            .when_some(ed.long_text.as_ref(), |d, (label, state)| {
                d.child(v_flex().gap_1().child(label_caps(label.clone())).child(Textarea::new(state).h(px(120.))))
            })
    }

    /// One field of the record being edited: its label above its input.
    fn render_field(&self, f: &crate::editor::Field, cx: &mut Context<Self>) -> Div {
        v_flex()
            .gap_1()
            .min_w_0()
            .child(label_caps(f.label.clone()))
            .child(match &f.state {
                FieldState::Text(e) => Input::new(e).small().into_any_element(),
                FieldState::Choice(e) => {
                    Select::new(e).small().cleanable(true).placeholder(t!("event_dialog.choose")).into_any_element()
                }
                FieldState::Date(e) => e.clone().into_any_element(),
            })
            .children(match &f.state {
                FieldState::Text(e) if f.place => self.place_suggestions(e, cx),
                _ => None,
            })
    }

    /// Cancel and Done, under the fields of the record being edited.
    fn edit_buttons(&self, cx: &mut Context<Self>) -> Div {
        h_flex()
            .gap_2()
            .justify_end()
            .child(
                Button::new("cancel-form")
                    .ghost()
                    .small()
                    .label(t!("ws.cancel_edit"))
                    .tooltip(t!("ws.cancel_edit_tip"))
                    .on_click(cx.listener(|this, _, w, cx| this.cancel_edit(w, cx))),
            )
            .child(
                Button::new("apply-form")
                    .primary()
                    .small()
                    .icon(IconName::Check)
                    .label(t!("ws.done"))
                    .on_click(cx.listener(|this, _, w, cx| this.apply_form(w, cx))),
            )
    }

    /// Keep and Delete, after Edit › Delete Item.
    fn delete_buttons(&self, place: bool, cx: &mut Context<Self>) -> Div {
        h_flex()
            .gap_2()
            .justify_end()
            .child(Button::new("cancel-delete").ghost().small().label(t!("ws.keep")).on_click(cx.listener(
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
                    .label(if place { t!("ws.confirm_remove_place") } else { t!("ws.confirm_delete") })
                    .on_click(cx.listener(|this, _, w, cx| this.delete_selected(w, cx))),
            )
    }

    /// The editor of the record shown in the right panel, when it is this one (`key`).
    fn panel_editor(&self, key: &str) -> Option<&RecordEditor> {
        self.editor.as_ref().filter(|ed| match &ed.target {
            Target::Record { xref, .. } => xref == key,
            Target::Place(name) => name == key,
        })
    }

    /// The small pencil next to a record's title: Edit › Edit Record.
    fn edit_pencil(&self, cx: &mut Context<Self>) -> Button {
        Button::new("edit-record")
            .ghost()
            .small()
            .icon(IconName::Pencil)
            .tooltip_with_action(t!("cmd.edit_record"), &EditRecord, Some("Workspace"))
            .on_click(cx.listener(|this, _, w, cx| this.start_edit(w, cx)))
    }

    /// Edit › Edit Record on an individual: their panel's header becomes fields, in place: names
    /// next to the portrait (the sex beside the surname), note. Birth and death are events of the
    /// timeline below, edited there like the others.
    fn render_individual_fields(&self, ed: &RecordEditor, cx: &mut Context<Self>) -> Div {
        let f = &ed.fields;
        v_flex()
            .p_4()
            .gap_3()
            .border_b_1()
            .border_color(theme::border())
            // No portrait while editing: it cannot be changed here. The whole name sits on one
            // line, in reading order (given names, particle, surname, suffix); title, sex and
            // nickname go below. A part only gets a field once added.
            .child(
                h_flex()
                    .gap_2()
                    .items_start()
                    .child(self.render_field(&f[0], cx).flex_1())
                    .children(name_part_field(ed, NamePart::Particle).map(|d| d.w(px(56.)).flex_none()))
                    .child(self.render_field(&f[1], cx).flex_1())
                    .children(name_part_field(ed, NamePart::Suffix).map(|d| d.w(px(64.)).flex_none())),
            )
            .child(
                h_flex()
                    .gap_2()
                    .items_start()
                    .children(name_part_field(ed, NamePart::Title).map(|d| d.flex_1()))
                    .child(self.render_field(&f[2], cx).w(px(116.)).flex_none())
                    .children(name_part_field(ed, NamePart::Nickname).map(|d| d.flex_1())),
            )
            .when_some(ed.names.as_ref(), |d, names| d.children(self.render_name_extras(names, cx)))
            .when_some(ed.long_text.as_ref(), |d, (label, state)| {
                d.child(v_flex().gap_1().child(label_caps(label.clone())).child(Textarea::new(state).h(px(80.))))
            })
            .child(
                h_flex()
                    .gap_2()
                    .items_center()
                    .child(self.add_to_name_button(cx))
                    .child(div().flex_1())
                    .child(self.edit_buttons(cx)),
            )
    }

    /// The other names of the person: each one a line, unfolded into its fields when clicked.
    fn render_name_extras(&self, names: &NamesEditor, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let mut out = Vec::new();
        if names.others.is_empty() {
            return out;
        }
        let rows = names.others.iter().enumerate().map(|(ix, other)| {
            if names.open == Some(ix) {
                return v_flex()
                    .gap_2()
                    .p_2()
                    .rounded(theme::RADIUS_SM)
                    .border_1()
                    .border_color(theme::border_strong())
                    .bg(theme::bg())
                    .child(
                        v_flex()
                            .gap_1()
                            .child(label_caps(t!("editor.name_type")))
                            .child(Select::new(&other.kind).small().placeholder(t!("event_dialog.choose"))),
                    )
                    .child(
                        h_flex()
                            .gap_2()
                            .child(
                                v_flex()
                                    .flex_1()
                                    .min_w_0()
                                    .gap_1()
                                    .child(label_caps(t!("editor.given")))
                                    .child(Input::new(&other.given).small()),
                            )
                            .child(
                                v_flex()
                                    .flex_1()
                                    .min_w_0()
                                    .gap_1()
                                    .child(label_caps(t!("editor.name")))
                                    .child(Input::new(&other.surname).small()),
                            ),
                    )
                    .child(
                        h_flex()
                            .justify_between()
                            .child(
                                Button::new(("remove-name", ix))
                                    .ghost()
                                    .small()
                                    .label(t!("editor.remove_name"))
                                    .text_color(theme::danger())
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        if let Some(n) = this.editor.as_mut().and_then(|ed| ed.names.as_mut()) {
                                            n.remove_other(ix);
                                        }
                                        cx.notify();
                                    })),
                            )
                            .child(
                                Button::new(("fold-name", ix))
                                    .primary()
                                    .small()
                                    .label(t!("editor.fold_name"))
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        if let Some(n) = this.editor.as_mut().and_then(|ed| ed.names.as_mut()) {
                                            n.open = None;
                                        }
                                        cx.notify();
                                    })),
                            ),
                    )
                    .into_any_element();
            }
            let form = other.read(cx);
            let shown = join_words(&[&form.prefix, &form.given, &form.particle, &form.surname, &form.suffix]);
            h_flex()
                .id(("other-name", ix))
                .justify_between()
                .gap_2()
                .h(px(30.))
                .px_2()
                .rounded(theme::RADIUS_SM)
                .cursor_pointer()
                .hover(|d| d.bg(theme::hover()))
                .child(div().min_w_0().overflow_hidden().whitespace_nowrap().text_ellipsis().child(shown))
                .child(div().flex_none().text_xs().text_color(theme::muted()).child(name_type_label(&form.kind)))
                .on_click(cx.listener(move |this, _, _, cx| {
                    if let Some(n) = this.editor.as_mut().and_then(|ed| ed.names.as_mut()) {
                        n.open = Some(ix);
                    }
                    cx.notify();
                }))
                .into_any_element()
        });
        out.push(v_flex().gap_1().child(label_caps(t!("editor.other_names"))).children(rows).into_any_element());
        out
    }

    /// “+ Add ▾” under an individual's fields: the commands of Person › Add to Name.
    fn add_to_name_button(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let shown: Vec<NamePart> = self
            .editor
            .as_ref()
            .and_then(|ed| ed.names.as_ref())
            .map(|n| n.parts.iter().map(|(p, _)| *p).collect())
            .unwrap_or_default();
        let me = cx.entity().downgrade();
        Button::new("add-to-name")
            .ghost()
            .small()
            .icon(IconName::Plus)
            .label(t!("common.add"))
            .dropdown_caret(true)
            .dropdown_menu(move |menu, _, _| {
                let item = |label: SharedString, part: Option<NamePart>| {
                    let me = me.clone();
                    PopupMenuItem::new(label).on_click(move |_, w, cx| {
                        let _ = me.update(cx, |this, cx| this.add_to_name(part, w, cx));
                    })
                };
                let mut menu = menu.item(item(t!("cmd.other_name").into(), None)).separator();
                for part in NamePart::ALL {
                    menu = menu.item(item(part.label(), Some(part)).disabled(shown.contains(&part)));
                }
                menu
            })
    }

    /// Person › Add to Name: edits the record if needed, then shows the part's field (or a new
    /// other name, unfolded) and focuses it.
    pub(super) fn add_to_name(&mut self, part: Option<NamePart>, window: &mut Window, cx: &mut Context<Self>) {
        if self.editor.as_ref().and_then(|ed| ed.names.as_ref()).is_none() {
            return;
        }
        if !self.editing {
            self.start_edit(window, cx);
        }
        let Some(ed) = self.editor.as_mut() else { return };
        let given = ed.fields.first().map(|f| f.state.value(cx)).unwrap_or_default();
        let Some(names) = ed.names.as_mut() else { return };
        let input = match part {
            Some(p) => names.add_part(p, window, cx),
            None => names.add_other(&given, window, cx),
        };
        input.update(cx, |e, cx| e.focus(window, cx));
        cx.notify();
    }

    /// The record as it is saved, in text: its title (with a source's repository and call number
    /// under it), then its facts, empty values hidden, linked records clickable. The pencil (and
    /// Edit › Edit Record) switches to the fields, in place.
    fn render_form_read(&self, ed: &RecordEditor, cx: &mut Context<Self>) -> impl IntoElement {
        let g = self.doc.as_ref().map(|d| &d.ged);
        let row = |label: SharedString, value: AnyElement| {
            h_flex()
                .gap_4()
                .items_start()
                .child(div().w(px(120.)).flex_none().text_xs().pt_0p5().text_color(theme::muted()).child(label))
                .child(div().flex_1().min_w_0().child(value))
        };
        let title = match &ed.target {
            Target::Place(name) => Some(short_place(name)),
            // A note's title is its first words: the text is enough.
            Target::Record { kind: RecordKind::SharedNote, .. } => None,
            Target::Record { xref, .. } => g.map(|g| g.record_label(xref)),
        };
        let links = ed.pickers.iter().enumerate().filter_map(|(n, p)| {
            let x = p.shown.clone()?;
            let name = g?.record_label(&x);
            Some(
                div()
                    .id(("form-link", n))
                    .cursor_pointer()
                    .text_color(theme::accent())
                    .hover(|d| d.underline())
                    .child(name)
                    .on_click(cx.listener(move |this, _, w, cx| this.goto(x.clone(), w, cx)))
                    .into_any_element(),
            )
        });
        let filled = |slot: Slot| ed.fields.iter().filter(move |f| f.slot == slot && !f.shown.trim().is_empty());
        let subtitle: Vec<AnyElement> =
            links.chain(filled(Slot::Subtitle).map(|f| div().child(f.shown.clone()).into_any_element())).collect();
        let n = subtitle.len();
        let subtitle = (n > 0).then(|| {
            h_flex().flex_wrap().gap_1().text_color(theme::muted()).children(subtitle.into_iter().enumerate().flat_map(
                |(i, el)| [(i > 0).then(|| div().child("·").into_any_element()), Some(el)].into_iter().flatten(),
            ))
        });
        let facts = filled(Slot::Fact).map(|f| row(f.label.clone(), div().child(f.shown.clone()).into_any_element()));
        let long = ed.long_text.as_ref().and_then(|(label, state)| {
            let text = state.read(cx).value().to_string();
            (!text.trim().is_empty()).then(|| (label.clone(), div().whitespace_normal().child(text)))
        });
        // Without a title (a note), the text takes its place, next to the pencil.
        let (text, long) = match (&title, long) {
            (None, Some((_, text))) => (Some(text), None),
            (_, long) => (None, long.map(|(label, text)| row(label, text.into_any_element()))),
        };
        let edit = self.edit_pencil(cx);
        let head = v_flex()
            .flex_1()
            .min_w_0()
            .gap_1()
            .when_some(title.clone(), |d, t| {
                d.child(div().text_2xl().font_weight(FontWeight::SEMIBOLD).line_height(rems(1.9)).child(t))
            })
            .children(subtitle)
            .children(text);
        let facts: Vec<_> = facts.chain(long).collect();
        v_flex()
            .gap_6()
            .child(h_flex().gap_3().items_start().child(head).child(edit))
            .when(!facts.is_empty(), |d| d.child(v_flex().gap_2().children(facts)))
    }

    /// “Possible duplicate: X” under the person's name, for each likely duplicate; a click opens
    /// the merge window.
    fn duplicate_lines(&self, g: &Gedcom, xref: &str, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let pairs = self
            .quality
            .issues
            .iter()
            .filter(|i| {
                i.code == "possible_duplicate" && !i.ignored && i.severity == gedcom::analysis::check::Severity::Warning
            })
            .filter_map(|i| Some((i.xref.clone(), i.related.first()?.clone())))
            .filter(|(a, b)| a == xref || b == xref);
        pairs
            .enumerate()
            .map(|(n, (a, b))| {
                let other = if a == xref { &b } else { &a };
                let name = t!("merge.possible_duplicate", name = g.record_label(other)).to_string();
                h_flex()
                    .id(("card-duplicate", n))
                    .gap_1()
                    .items_center()
                    .text_xs()
                    .text_color(theme::warn())
                    .cursor_pointer()
                    .hover(|d| d.underline())
                    .child(icon(IconName::Users))
                    .child(name)
                    .on_click(cx.listener(move |this, _, _, cx| this.open_merge(vec![(a.clone(), b.clone())], 0, cx)))
                    .into_any_element()
            })
            .collect()
    }

    /// Edit › Edit Record: the record's fields, in place; a second time, saves (like Done).
    pub(super) fn start_edit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.editing {
            self.apply_form(window, cx);
            return;
        }
        if !self.view.has_panel() {
            self.tab = CenterTab::Form;
        }
        self.editing = true;
        if let Some(f) = self.editor.as_ref().and_then(|ed| ed.fields.first()) {
            match &f.state {
                FieldState::Text(e) => e.update(cx, |e, cx| e.focus(window, cx)),
                FieldState::Date(e) => {
                    let input = e.read(cx).input().clone();
                    input.update(cx, |e, cx| e.focus(window, cx))
                }
                FieldState::Choice(_) => {}
            }
        }
        cx.notify();
    }

    /// Opens the record `xref`, just created by the last restore point, on its fields. Until it
    /// is saved or left, Cancel takes the creation back and returns to `back` (like a new contact
    /// in Contacts): no empty record is left behind.
    pub(super) fn edit_fresh(
        &mut self,
        xref: String,
        back: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(checkpoints) = self.doc.as_ref().map(|d| d.checkpoints) else { return };
        self.start_edit(window, cx);
        self.fresh = Some(Fresh { xref, back, checkpoints });
    }

    /// Cancel (or Esc): the typed values are dropped, back to reading. On a record just created
    /// and not changed since, the creation is undone too.
    pub(super) fn cancel_edit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(f) = self.fresh.take()
            && self.editing
            && self.selected.as_deref() == Some(f.xref.as_str())
            && let Some(doc) = self.doc.as_mut()
            && doc.checkpoints == f.checkpoints
            && let Some(prev) = doc.history.pop()
        {
            doc.ged = prev;
            doc.checkpoints -= 1;
            doc.future = std::mem::take(&mut doc.future_before_checkpoint);
            doc.dirty = doc.dirty_before_checkpoint;
            let back = f.back.filter(|b| doc.ged.get(b).is_some());
            self.selected = None;
            self.editor = None;
            self.rebuild_rows(cx);
            match back {
                Some(b) => self.select(b, window, cx),
                None => self.refresh_editor(window, cx),
            }
            self.set_status(t!("ws.creation_cancelled"), false);
            cx.notify();
            return;
        }
        self.refresh_editor(window, cx);
        cx.notify();
    }

    /// Thumbnails of the linked images: in the detail panel, or under a record read in the centre
    /// (`inline`, larger tiles in a row). `add`: “+” next to the title (add photos, link or crop a
    /// media), and the title stays visible even without an image. For an individual, the images
    /// of their unions (wedding photo…) follow their own.
    fn render_gallery(&self, xref: &str, inline: bool, add: bool, cx: &mut Context<Self>) -> Option<Div> {
        let doc = self.doc.as_ref()?;
        // (record holding the link, image)
        let mut images: Vec<(String, Media)> =
            doc.ged.images_of(xref).into_iter().map(|m| (xref.to_string(), m)).collect();
        for fam in doc.ged.individual(xref).map(|i| i.fams()).unwrap_or_default() {
            for m in doc.ged.images_of(fam) {
                if !images.iter().any(|(_, x)| x.xref() == m.xref()) {
                    images.push((fam.to_string(), m));
                }
            }
        }
        if images.is_empty() && !add {
            return None;
        }
        let n = images.len();
        let add_button = add.then(|| self.add_image_button(xref, cx));
        let tiles = images.into_iter().map(|(owner, m)| {
            let owner = owner.as_str();
            let target = m.xref().to_string();
            let menu = self.record_menu(m.xref(), MenuFrom::Attached(owner.to_string()), cx);
            let page_count = m.page_count();
            let path = pages::page_path(doc, &m, doc.ged.link_page(owner, m.xref())).filter(|p| p.exists());
            let tile = div()
                .relative()
                .id(SharedString::from(format!("thumb-{target}")))
                .rounded(theme::RADIUS_SM)
                .overflow_hidden()
                .border_1()
                .border_color(theme::border())
                .bg(theme::surface())
                .cursor_pointer()
                .hover(|d| d.border_color(theme::accent()))
                .on_click(cx.listener(move |this, _, w, cx| this.goto(target.clone(), w, cx)));
            let tile = if inline { tile.w(px(120.)).h(px(150.)) } else { tile.h(px(96.)) };
            let crop = doc.ged.link_crop(owner, m.xref());
            // Number of pages of a multi-page media, in the corner.
            let badge = (page_count > 1).then(|| {
                div()
                    .absolute()
                    .right(px(4.))
                    .bottom(px(4.))
                    .px_1()
                    .rounded(theme::RADIUS_SM)
                    .bg(theme::image_label_bg())
                    .text_xs()
                    .text_color(theme::image_text())
                    .child(t!("ws.pages_short", n = page_count))
            });
            let tile = match path {
                Some(p) if crop.is_some() => {
                    // Cropped area of the link: face on a group photo, excerpt of a record…
                    let (w, h) = if inline { (118., 148.) } else { (104., 94.) };
                    tile.flex().justify_center().child(photo::linked_img(p, crop, w, h, true))
                }
                Some(p) => {
                    let (w, h) = if inline { (120., 150.) } else { (104., 96.) };
                    tile.child(picture(p, w, h, ObjectFit::Cover).size_full())
                }
                None => tile.flex().items_center().justify_center().child(
                    v_flex()
                        .items_center()
                        .gap_1()
                        .text_xs()
                        .text_color(theme::warn())
                        .child(icon(IconName::Image))
                        .child(t!("ws.not_found")),
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
                        .gap_2()
                        .items_center()
                        .when(!inline, |d| d.child(icon(IconName::Images)))
                        .child(label_caps(if n > 0 {
                            t!("ws.images_count", n = n).into_owned()
                        } else {
                            t!("ws.images").into_owned()
                        }))
                        .child(div().flex_1())
                        .children(add_button),
                )
                .when(n > 0, |d| {
                    d.child(if inline {
                        h_flex().flex_wrap().gap_2().children(tiles).into_any_element()
                    } else {
                        div().grid().grid_cols(3).gap_2().children(tiles).into_any_element()
                    })
                })
                .when(inline, |d| d.p_0().border_b_0()),
        )
    }

    /// “+” of the images: the Media window (view, link, import).
    fn add_image_button(&self, xref: &str, cx: &mut Context<Self>) -> AnyElement {
        let owner = xref.to_string();
        add::plus_button("add-image")
            .tooltip(t!("ws.media_tooltip"))
            .on_click(cx.listener(move |this, _, w, cx| this.open_gallery(GalleryTarget::Record(owner.clone()), w, cx)))
            .into_any_element()
    }

    fn media_preview(&self, ed: &RecordEditor, cx: &mut Context<Self>) -> Option<AnyElement> {
        let Target::Record { xref, kind: RecordKind::Media } = &ed.target else { return None };
        let doc = self.doc.as_ref()?;
        let m = Media::new(doc.ged.get(xref)?);
        if let Some(url) = m.web_url() {
            // Web page: button to open it in the browser.
            let href = url.to_string();
            return Some(
                h_flex()
                    .child(
                        Button::new("open-web-media")
                            .small()
                            .icon(IconName::ExternalLink)
                            .label(t!("ws.open_host", host = host(url)))
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
            // File present but without a preview (PDF…): nothing to report.
            Some(p) if p.exists() => None,
            Some(_) => {
                Some(div().text_color(theme::warn()).child(t!("act.file_missing", path = file)).into_any_element())
            }
            None => Some(div().text_color(theme::muted()).child(t!("ws.no_local_file")).into_any_element()),
        };
        if strip.is_none() && el.is_none() {
            return None;
        }
        // Dropping files on the preview adds them as pages.
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
        let changed = ed.raw_changed(cx);
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
                            .label(t!("ws.apply_gedcom"))
                            .disabled(!changed)
                            .on_click(cx.listener(|this, _, w, cx| this.apply_raw(w, cx))),
                    )
                    .child(Button::new("reset-raw").ghost().small().label(t!("ws.revert")).disabled(!changed).on_click(
                        cx.listener(|this, _, w, cx| {
                            this.refresh_editor(w, cx);
                            cx.notify();
                        }),
                    ))
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
        // Dropping photos or documents anywhere on a person's panel opens the import preview of the
        // Media window.
        let panel = if g.individual(key).is_some() {
            let (record, entity) = (key.clone(), cx.entity().downgrade());
            panel.drag_over::<ExternalPaths>(|d, _, _, _| d.bg(theme::accent_soft())).on_drop(
                move |paths: &ExternalPaths, window, cx| {
                    let record = record.clone();
                    let _ = entity.update(cx, |this, cx| {
                        let files = paths.paths().to_vec();
                        this.open_gallery_with_files(GalleryTarget::Record(record), files, window, cx);
                    });
                },
            )
        } else {
            panel
        };
        match self.view {
            View::Individuals => match g.individual(key) {
                Some(i) => panel.child(self.render_individual_card(g, i, cx)).into_any_element(),
                None => panel.into_any_element(),
            },
            View::Places => {
                let place = g.places().into_iter().find(|p| &p.name == key);
                let refs = place.as_ref().map(|p| p.records.clone()).unwrap_or_default();
                let confirm = self.pending_delete.as_ref() == Some(key);
                // Edit › Edit Record: one field per jurisdiction, then the coordinates, in place.
                let fields = self.panel_editor(key).filter(|_| self.editing).map(|ed| {
                    v_flex()
                        .p_4()
                        .gap_3()
                        .border_b_1()
                        .border_color(theme::border())
                        // The name (one field, or one per jurisdiction), then the coordinates.
                        .map(|d| {
                            let (names, coords) = ed.fields.split_at(ed.fields.len().saturating_sub(2));
                            let grid = || div().grid().grid_cols(2).gap_3();
                            match names {
                                [one] => d.child(self.render_field(one, cx)),
                                _ => d.child(grid().children(names.iter().map(|f| self.render_field(f, cx)))),
                            }
                            .child(grid().children(coords.iter().map(|f| self.render_field(f, cx))))
                        })
                        .child(self.edit_buttons(cx))
                });
                let editing = fields.is_some();
                let header = h_flex()
                    .p_4()
                    .gap_2()
                    .items_start()
                    .border_b_1()
                    .border_color(theme::border())
                    .child(
                        v_flex()
                            .flex_1()
                            .min_w_0()
                            .gap_1()
                            .child(div().text_xl().font_weight(FontWeight::SEMIBOLD).child(short_place(key)))
                            .children(place.iter().map(|p| {
                                let parts = p.parts().join(" › ");
                                div().text_color(theme::muted()).child(parts)
                            }))
                            .children(place.iter().map(|p| {
                                mono(match (&p.latitude, &p.longitude) {
                                    (Some(a), Some(b)) => t!(
                                        "ws.place_coords",
                                        lat = a,
                                        lon = b,
                                        uses = crate::tn!("places.uses", p.uses)
                                    )
                                    .into_owned(),
                                    _ => {
                                        t!("ws.place_no_coords", uses = crate::tn!("places.uses", p.uses)).into_owned()
                                    }
                                })
                            }))
                            .when(confirm, |d| d.child(self.delete_buttons(true, cx).pt_2())),
                    )
                    .child(self.edit_pencil(cx));
                panel
                    .map(|d| match fields {
                        Some(f) => d.child(f),
                        None => d.child(header),
                    })
                    .when(!editing, |d| d.children(place.as_ref().and_then(|p| self.render_locate(p, cx))))
                    .child(self.render_links(t!("ws.cited_by").into(), refs, cx))
                    .into_any_element()
            }
            // Sources, media, repositories and notes are read whole in the centre: no panel.
            _ => div().into_any_element(),
        }
    }

    fn render_links(&self, title: SharedString, xrefs: Vec<String>, cx: &mut Context<Self>) -> Div {
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
                    .rounded(theme::RADIUS_SM)
                    .cursor_pointer()
                    .hover(|d| d.bg(theme::hover()))
                    .children(kind.map(|k| icon(View::for_kind(k).icon()).text_color(theme::muted())))
                    .child(div().flex_1().overflow_hidden().whitespace_nowrap().text_ellipsis().child(label))
                    .on_click(cx.listener(move |this, _, w, cx| this.goto(target.clone(), w, cx)))
                    .with_menu(self.record_menu(&x, MenuFrom::Link, cx))
            }),
        )
    }

    /// “Events” section of an individual: their own and those of their unions (marriage, divorce…)
    /// in the same timeline. A click opens the event for editing, right-click for the other
    /// actions; dropping files on an event attaches them to it.
    fn render_events(&self, owner: &str, cx: &mut Context<Self>) -> AnyElement {
        let doc = self.doc.as_ref().unwrap();
        let g = &doc.ged;
        let events: Vec<PersonEvent> = g.person_events(owner);
        let person = g.individual(owner);
        // Relatives' events during the person's life (birth of a child, death of a parent…), shown
        // on demand.
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
                let label = match e.value() {
                    Some(v) => t!("ws.event_value", label = e.label(), value = v).into_owned(),
                    None => e.label(),
                };
                // Second line, plain text: with whom (event of a union), where (street address,
                // then the place).
                let place = e.place().filter(|p| !p.is_empty()).map(short_place);
                let where_ = match (e.address(), place) {
                    (Some(a), Some(p)) => Some(format!("{a}, {p}")),
                    (a, p) => a.or(p),
                };
                let sub: Vec<String> = spouse
                    .filter(|_| family)
                    .map(|s| t!("ws.with", name = g.record_label(&s)).into_owned())
                    .into_iter()
                    .chain(where_)
                    .collect();
                let date = e.date().map(|d| d.display()).unwrap_or_default();
                let (o1, o2, subject) = (ev_owner.clone(), ev_owner.clone(), x.clone());
                // Dropping files on the event: import preview of its media.
                let (drop_owner, entity) = (ev_owner.clone(), cx.entity().downgrade());
                let row = event_row(
                    SharedString::from(format!("ev-{ev_owner}-{ix}")),
                    label,
                    date,
                    sub.join(" · "),
                    !e.is_sourced(),
                    e.media_xrefs().len(),
                    false,
                )
                .drag_over::<ExternalPaths>(|d, _, _, _| d.bg(theme::accent_soft()))
                .on_drop(move |paths: &ExternalPaths, window, cx| {
                    cx.stop_propagation();
                    let owner = drop_owner.clone();
                    let _ = entity.update(cx, |this, cx| {
                        let files = paths.paths().to_vec();
                        let target = GalleryTarget::Event { owner, ix };
                        this.open_gallery_with_files(target, files, window, cx);
                    });
                })
                // Click: the event in read-only; double-click: straight to editing.
                .on_click(cx.listener(move |this, ev: &ClickEvent, w, cx| {
                    if ev.click_count() >= 2 {
                        this.event_detail = None;
                        this.open_event_dialog(o1.clone(), Some(ix), w, cx)
                    } else {
                        this.open_event_detail(o1.clone(), ix, subject.clone(), cx)
                    }
                }))
                .with_menu(self.event_menu(o2, ix, cx));
                (sort, row.into_any_element())
            })
            .collect();
        // Events of other people the person attended (witness, godparent…): `ASSO`.
        if person.is_some() {
            for a in g.attended_events(owner) {
                let sort = a.event.date().map(|d| d.sort_key()).unwrap_or((i32::MAX, 0, 0));
                let names: Vec<String> = a.principals.iter().map(|p| g.record_label(p)).collect();
                let who = match names.as_slice() {
                    [a, b] => t!("ws.couple", a = a, b = b).into_owned(),
                    _ => names.join(", "),
                };
                let sub: Vec<String> = Some(who)
                    .filter(|w| !w.is_empty())
                    .into_iter()
                    .chain(a.event.place().filter(|p| !p.is_empty()).map(short_place))
                    .collect();
                let (ev_owner, ix, subject) = (a.owner.clone(), a.index, x.clone());
                let row =
                    event_row(
                        SharedString::from(format!("att-{}-{}", a.owner, a.index)),
                        t!("ws.attended", role = a.role, event = a.event.label()).into_owned(),
                        a.event.date().map(|d| d.display()).unwrap_or_default(),
                        sub.join(" · "),
                        false,
                        0,
                        true,
                    )
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.open_event_detail(ev_owner.clone(), ix, subject.clone(), cx)
                    }))
                    .with_menu(self.record_menu(
                        a.principals.first().unwrap_or(&a.owner),
                        MenuFrom::Link,
                        cx,
                    ));
                rows.push((sort, row.into_any_element()));
            }
        }
        if show_relatives {
            for (n, r) in relatives.iter().enumerate() {
                let sort = r.date.as_ref().map(|d| d.sort_key()).unwrap_or((i32::MAX, 0, 0));
                rows.push((sort, self.render_relative_event(n, r, cx)));
            }
        }
        // Stable sort: on the same date, the person's event first.
        rows.sort_by_key(|r| r.0);
        let toggle = (!relatives.is_empty()).then(|| {
            Button::new("relative-events")
                .xsmall()
                .icon(IconName::Users)
                .when(show_relatives, |b| b.primary())
                .when(!show_relatives, |b| b.ghost())
                .tooltip(t!("cmd.relative_events"))
                .on_click(cx.listener(|this, _, _, cx| this.toggle_relative_events(cx)))
        });
        v_flex()
            .p_4()
            .gap_1()
            .border_b_1()
            .border_color(theme::border())
            .child(section_header(
                t!("settings.events"),
                h_flex().gap_1().children(toggle).child(
                    add::plus_button("add-event")
                        .tooltip(t!("cmd.add_event").trim_end_matches('…').to_string())
                        .on_click(cx.listener(move |this, _, w, cx| this.open_event_dialog(x.clone(), None, w, cx))),
                ),
            ))
            .children(rows.into_iter().map(|r| r.1))
            .into_any_element()
    }

    /// View › Relatives' Events.
    pub(super) fn toggle_relative_events(&mut self, cx: &mut Context<Self>) {
        self.show_relative_events = !self.show_relative_events;
        cx.notify();
    }

    /// Discreet row of a relative's event; a click opens that relative.
    fn render_relative_event(&self, n: usize, r: &timeline::Entry, cx: &mut Context<Self>) -> AnyElement {
        let target = r.related.clone();
        // Same layout as the person's own events: label and date, then the age on a second line.
        v_flex()
            .id(("rel-ev", n))
            .gap_0p5()
            .px_2()
            .py_1()
            .rounded(theme::RADIUS_SM)
            .text_color(theme::muted())
            .cursor_pointer()
            .hover(|d| d.bg(theme::hover()))
            .child(
                h_flex()
                    .justify_between()
                    .items_start()
                    .gap_2()
                    .child(div().flex_1().min_w_0().child(r.label.clone()))
                    .child(event_date(r.date.as_ref().map(|d| d.display()).unwrap_or_default())),
            )
            .when_some(r.age.clone(), |d, a| d.child(div().text_xs().font_features(theme::tabular_nums()).child(a)))
            .on_click(cx.listener(move |this, _, w, cx| {
                if let Some(x) = target.clone() {
                    this.goto(x, w, cx)
                }
            }))
            .into_any_element()
    }

    /// Media of an event: thumbnails of the images, title of the other documents.
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
                            .rounded(theme::RADIUS_SM)
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
                            .px_2()
                            .py_0p5()
                            .rounded(theme::RADIUS_SM)
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
        // Like the “Events” list: those of the unions count too.
        let timeline = g.person_events(i.xref());
        let (sourced, total) = (timeline.iter().filter(|e| e.event.is_sourced()).count(), timeline.len());
        let pct = if total == 0 { 0. } else { sourced as f32 / total as f32 };
        let sosa = i.sosa();
        let doc = self.doc.as_ref().unwrap();
        let is_root = g.root().as_deref() == Some(i.xref());
        let lineage =
            sosa.as_deref().and_then(|s| s.parse().ok()).map(theme::lineage).unwrap_or(theme::border_strong());
        let xref = i.xref().to_string();
        // Edit › Edit Record: the header becomes fields, in place.
        let fields = self
            .panel_editor(&xref)
            .filter(|ed| self.editing && ed.fields.len() >= 3)
            .map(|ed| self.render_individual_fields(ed, cx));
        let confirm = self.pending_delete.as_deref() == Some(xref.as_str());
        let names = i.names();
        let main_name = names.first().cloned().unwrap_or_default();
        let names_line = names_line(&names, i.sex() == "F");

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
                    .min_w_0()
                    .gap_1()
                    // Long names wrap rather than being cut.
                    .child(
                        h_flex()
                            .flex_wrap()
                            .gap_x_1()
                            .text_xl()
                            .child(
                                div()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child(join_words(&[&main_name.particle, &main_name.surname])),
                            )
                            .child(div().min_w_0().max_w_full().child(join_words(&[
                                &main_name.prefix,
                                &main_name.given,
                                &main_name.suffix,
                            ]))),
                    )
                    // Other names and nickname, only when there are some.
                    .when(!names_line.is_empty(), |d| d.child(div().text_color(theme::muted()).child(names_line)))
                    .child(div().text_color(theme::muted()).child(format!(
                        "{} · {}{}",
                        i.sex_label(),
                        if i.years().is_empty() { t!("ws.unknown_dates").into() } else { i.years() },
                        sosa.map(|s| format!(" · Sosa {s}")).unwrap_or_default(),
                    )))
                    .when(is_root, |d| {
                        d.child(
                            h_flex()
                                .gap_1()
                                .items_center()
                                .text_xs()
                                .text_color(theme::muted())
                                .child(icon(IconName::House))
                                .child(t!("settings.root")),
                        )
                    })
                    .children(self.duplicate_lines(g, &xref, cx))
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
                            .child(mono(t!("ws.sourced", sourced = sourced, total = total))),
                    )
                    .when(confirm, |d| d.child(self.delete_buttons(false, cx).pt_2())),
            )
            .child(self.edit_pencil(cx));

        let events_el = self.render_events(&xref, cx);

        // Family: parents, spouses, children. (role, relative, link to undo on right-click:
        // (individual, family, label)).
        type Unlink = (String, String, SharedString);
        // Fourth field: years of an ended union (“1932–1950”).
        let mut family: Vec<(SharedString, String, Unlink, Option<String>)> = Vec::new();
        if let Some(fam) = i.famc().first().and_then(|f| g.family(f)) {
            let link = (xref.clone(), fam.xref().to_string(), t!("ws.detach_parents").into());
            family.extend(fam.husband().map(|p| (label::father(), p.to_string(), link.clone(), None)));
            family.extend(fam.wife().map(|p| (label::mother(), p.to_string(), link, None)));
        }
        for fam in i.fams().into_iter().filter_map(|f| g.family(f)) {
            let f = fam.xref().to_string();
            let spouse = [fam.husband(), fam.wife()].into_iter().flatten().find(|x| *x != xref);
            // Ended union (divorce, annulment): its years instead of the spouse's.
            let end = fam.separation().map(|e| {
                let year = |e: Option<gedcom::Event>| e.and_then(|e| e.date()).and_then(|d| d.year());
                let end = year(Some(e)).map(|y| y.to_string()).unwrap_or_else(|| "?".into());
                match year(fam.marriage()) {
                    Some(start) => format!("{start}–{end}"),
                    None => format!("–{end}"),
                }
            });
            family.extend(spouse.map(|s| {
                (label::spouse(), s.to_string(), (xref.clone(), f.clone(), t!("ws.detach_union").into()), end)
            }));
            family.extend(fam.children().into_iter().map(|k| {
                (label::child(), k.to_string(), (k.to_string(), f.clone(), t!("ws.remove_child").into()), None)
            }));
        }
        family.retain(|(_, x, _, _)| g.individual(x).is_some());
        let family_el = v_flex()
            .p_4()
            .gap_1()
            .border_b_1()
            .border_color(theme::border())
            .child(section_header(t!("ws.family"), {
                let x = xref.clone();
                add::plus_button("add-relative")
                    .tooltip(t!("ws.add_relative"))
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
                    .rounded(theme::RADIUS_SM)
                    .items_center()
                    .cursor_pointer()
                    .hover(|d| d.bg(theme::hover()))
                    .child(div().w(px(64.)).flex_none().text_xs().text_color(theme::muted()).child(role))
                    .child(div().flex_1().overflow_hidden().whitespace_nowrap().text_ellipsis().child(p.display_name()))
                    // Ended union: its years rather than the spouse's life dates.
                    .child(mono(end.unwrap_or_else(|| p.years())))
                    .on_click(cx.listener(move |this, _, w, cx| this.goto(target.clone(), w, cx)))
                    .with_menu(self.person_menu(&x, MenuFrom::Relative(unlink.0, unlink.1, unlink.2), cx))
            }));

        // Media and notes of their unions too (attached to the couple).
        let unions: Vec<gedcom::Family> = i.fams().into_iter().filter_map(|f| g.family(f)).collect();
        let images: Vec<String> = std::iter::once(xref.as_str())
            .chain(unions.iter().map(|f| f.xref()))
            .flat_map(|x| g.images_of(x))
            .map(|m| m.xref().to_string())
            .collect();
        let mut media: Vec<String> = Vec::new();
        for x in i.media_xrefs().into_iter().chain(unions.iter().flat_map(|f| f.media_xrefs())) {
            if !images.iter().any(|m| m == x) && !weblinks::is_web_media(g, x) && !media.iter().any(|m| m == x) {
                media.push(x.to_string());
            }
        }
        let web = self.render_web_links(&xref, cx);
        let gallery = self.render_gallery(&xref, false, true, cx);
        let editing = fields.is_some();
        // While editing, the first note is in the fields.
        let mut notes: Vec<String> = i.notes().into_iter().skip(usize::from(editing)).collect();
        notes.extend(unions.iter().flat_map(|f| f.notes()));
        v_flex()
            .map(|d| match fields {
                Some(f) => d.child(f),
                None => d.child(header),
            })
            .when_some(gallery, |d, el| d.child(el))
            .child(events_el)
            .child(family_el)
            .when_some(web, |d, el| d.child(el))
            .when(!media.is_empty(), |d| d.child(self.render_links(t!("ws.other_media").into(), media, cx)))
            .when(!notes.is_empty(), |d| {
                d.child(
                    v_flex()
                        .p_4()
                        .gap_1()
                        .child(label_caps(t!("cmd.notes")))
                        .children(notes.into_iter().map(|n| div().text_color(theme::muted()).child(n))),
                )
            })
    }

    fn render_welcome(&self, window: &Window, cx: &mut Context<Self>) -> impl IntoElement {
        // One row per way to start, the creation first; the shortcut on the right.
        let row = |id: &'static str,
                   icon_name: IconName,
                   title: SharedString,
                   formats: Option<SharedString>,
                   action: &dyn Action,
                   primary: bool| {
            let kbd =
                gpui_kit::component::kbd::Kbd::binding_for_action(action, None, window).map(|k| k.appearance(false));
            h_flex()
                .id(id)
                .w_full()
                .px_4()
                .py_3()
                .gap_3()
                .items_center()
                .rounded(theme::RADIUS_MD)
                .border_1()
                .cursor_pointer()
                .border_color(if primary { theme::accent() } else { theme::border() })
                .bg(if primary { theme::accent_soft() } else { theme::bg() })
                .hover(|d| d.border_color(theme::accent()))
                .child(
                    div()
                        .flex_none()
                        .size(px(32.))
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded(theme::RADIUS_SM)
                        .bg(if primary { theme::accent() } else { theme::surface() })
                        .text_color(if primary { theme::on_accent() } else { theme::muted() })
                        .child(Icon::new(icon_name)),
                )
                .child(
                    v_flex()
                        .flex_1()
                        .min_w_0()
                        .child(div().font_weight(FontWeight::MEDIUM).child(title))
                        .children(formats.map(|f| div().text_xs().text_color(theme::muted()).child(f))),
                )
                .children(kbd.map(|k| div().text_color(theme::muted()).child(k)))
        };
        v_flex()
            .flex_1()
            .items_center()
            .justify_center()
            .gap_8()
            .child(
                img(if theme::is_dark() {
                    "brand/logo-horizontal-dark.svg"
                } else {
                    "brand/logo-horizontal-light.svg"
                })
                .w(px(272.))
                .h(px(78.)),
            )
            .child(
                v_flex()
                    .w(px(440.))
                    .gap_2()
                    .child(
                        row("welcome-new", IconName::Sprout, t!("welcome.new_tree").into(), None, &NewFile, true)
                            .on_click(cx.listener(|this, _, w, cx| this.new_file(&NewFile, w, cx))),
                    )
                    .child(
                        row(
                            "welcome-open",
                            IconName::FolderOpen,
                            t!("welcome.open").into(),
                            Some(t!("welcome.open_formats").into()),
                            &OpenFile,
                            false,
                        )
                        .on_click(cx.listener(|this, _, w, cx| this.open_file(&OpenFile, w, cx))),
                    ),
            )
            .children(self.render_recents(cx))
            .when_some(self.status.clone().filter(|s| s.1), |d, (msg, _)| {
                d.child(div().text_color(theme::danger()).child(msg))
            })
    }

    /// Recent files of the welcome page: name and folder, a click to reopen.
    fn render_recents(&self, cx: &mut Context<Self>) -> Option<impl IntoElement> {
        let paths = crate::recent::list();
        if paths.is_empty() {
            return None;
        }
        // The file name on the left, its folder's name on the right (“Généalogie Guyot”, the useful
        // end of the path); the whole path in the tooltip.
        Some(v_flex().w(px(440.)).gap_1().child(div().px_3().child(label_caps(t!("ws.recent")))).children(
            paths.into_iter().enumerate().map(|(i, path)| {
                let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                let full: SharedString = crate::recent::folder(&path).into();
                let folder = path
                    .parent()
                    .and_then(|d| d.file_name())
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| full.to_string());
                h_flex()
                    .id(("recent", i))
                    .px_3()
                    .py_2()
                    .gap_3()
                    .rounded(theme::RADIUS_SM)
                    .cursor_pointer()
                    .hover(|d| d.bg(theme::hover()))
                    .child(div().flex_none().child(name))
                    .child(div().flex_1().min_w_0().truncate().text_right().text_color(theme::muted()).child(folder))
                    .tooltip(move |w, cx| Tooltip::new(full.clone()).build(w, cx))
                    .on_click(cx.listener(move |this, _, w, cx| this.open_tree(path.clone(), w, cx)))
            }),
        ))
    }
}

impl Render for Workspace {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.check_tour(window, cx);
        // Search suggestions: visible while the field has the focus.
        let focused = self.filter.read(cx).focus_handle(cx).is_focused(window);
        if focused && !self.search_focused {
            self.suggest_closed = false;
            self.refresh_suggestions(cx);
        }
        self.search_focused = focused;
        let title = match &self.doc {
            Some(d) => format!("{}{} — GEDRoots", d.title(), if d.unsaved() { " •" } else { "" }),
            None => "GEDRoots".into(),
        };
        if title != self.window_title {
            // The Window menu lists the trees by name.
            let (name, path) = match &self.doc {
                Some(d) => (d.title(), d.own_file().map(std::path::Path::to_path_buf)),
                None => ("GEDRoots".to_string(), None),
            };
            crate::windows::set_shown(cx.entity_id(), &name, path.as_deref(), cx);
            window.set_window_title(&title);
            // macOS: dot in the close button while the file is not saved.
            window.set_window_edited(self.doc.as_ref().is_some_and(Document::unsaved));
            self.window_title = title;
        }
        // Search sites of the active window's file (Person › Search On).
        if window.is_window_active() || cx.windows().len() == 1 {
            let names = self
                .doc
                .as_ref()
                .map(|d| d.ged.search_sites().into_iter().map(|s| s.name).collect())
                .unwrap_or_default();
            if crate::commands::set_search_sites(names) {
                crate::commands::install_menus(cx);
            }
        }
        // Menu bar drawn in the window (Linux, Windows): once this frame is drawn, grey out the
        // commands that do not apply to it (macOS asks the window itself when a menu opens).
        if self.menu_bar.is_some() && (window.is_window_active() || cx.windows().len() == 1) {
            let focus = self.focus.clone();
            window.on_next_frame(move |window, cx| {
                let unavailable = crate::commands::menu_actions()
                    .into_iter()
                    .filter(|a| {
                        !window.is_action_available_in(a.as_ref(), &focus) && !crate::is_global_action(a.as_ref())
                    })
                    .map(|a| a.name())
                    .collect();
                if crate::commands::set_unavailable(unavailable) {
                    crate::commands::install_menus(cx);
                    window.refresh();
                }
            });
        }
        // Recent files, search sites or available commands changed (here or in another window): the
        // menu bar picks them up.
        let menus_gen = (crate::recent::generation(), crate::commands::search_sites_generation());
        if self.recent_gen != menus_gen {
            self.recent_gen = menus_gen;
            if let Some(bar) = &self.menu_bar {
                bar.update(cx, |bar, cx| bar.reload(cx));
            }
        }
        // Narrow window: fold the sidebar, then the details panel (see `FOLD_SIDEBAR`).
        let width = f32::from(window.viewport_size().width);
        let compact = Compact { sidebar: width < FOLD_SIDEBAR, inspector: width < FOLD_INSPECTOR };
        if compact != self.compact {
            self.compact = compact;
            self.drawer = None;
        }
        if self.drawer == Some(Drawer::Inspector) && !self.has_inspector() {
            self.drawer = None;
        }
        self.release_book_pages(window);
        // Shortcuts (Save…) go through this node: keep it in the focus chain.
        if window.focused(cx).is_none() {
            window.focus(&self.focus, cx);
        }
        let body: AnyElement = if let (Some(_), Some((editor, _))) = (&self.doc, &self.act_editor) {
            // Record opened in its source's page: it takes the place of the record page and the
            // panel.
            h_flex()
                .flex_1()
                .min_h_0()
                .items_stretch()
                .when(self.sidebar && !self.compact.sidebar, |d| d.child(self.render_sidebar(cx)))
                .child(self.render_list(cx))
                .child(editor.clone())
                .into_any_element()
        } else if self.doc.is_some() && self.review.open {
            // Records proposed by an agent: the page takes the place of the lists.
            h_flex()
                .flex_1()
                .min_h_0()
                .items_stretch()
                .when(self.sidebar && !self.compact.sidebar, |d| d.child(self.render_sidebar(cx)))
                .child(self.render_review(cx))
                .into_any_element()
        } else if self.doc.is_some() {
            h_flex()
                .flex_1()
                .min_h_0()
                .items_stretch()
                .when(self.sidebar && !self.compact.sidebar, |d| d.child(self.render_sidebar(cx)))
                .when(self.view == View::Research, |d| d.child(self.render_research_list(cx)))
                .when(self.view == View::Books, |d| d.child(self.render_books_list(cx)))
                .when(!matches!(self.view, View::Stats | View::Research | View::Books), |d| {
                    d.child(self.render_list(cx))
                })
                .child(self.render_center(cx))
                .when(self.inspector && !self.compact.inspector, |d| d.child(self.render_detail(cx)))
                .into_any_element()
        } else {
            self.render_welcome(window, cx).into_any_element()
        };
        v_flex()
            .size_full()
            .relative()
            .track_focus(&self.focus)
            .key_context("Workspace")
            .map(|d| self.bind_actions(d, window, cx))
            .map(|d| self.bind_page_actions(d, cx))
            // Mouse “back / forward” buttons.
            .on_mouse_down(
                MouseButton::Navigate(NavigationDirection::Back),
                cx.listener(|this, _, w, cx| this.go_back(w, cx)),
            )
            .on_mouse_down(
                MouseButton::Navigate(NavigationDirection::Forward),
                cx.listener(|this, _, w, cx| this.go_forward(w, cx)),
            )
            .on_key_down(cx.listener(|this, ev: &KeyDownEvent, w, cx| {
                // Version history: ↑ ↓ move in the list.
                if this.version_history.is_some() && this.add_dialog.is_none() {
                    match ev.keystroke.key.as_str() {
                        "down" => return this.step_version(1, w, cx),
                        "up" => return this.step_version(-1, w, cx),
                        _ => {}
                    }
                }
                if this.record_history.is_some() {
                    match ev.keystroke.key.as_str() {
                        "down" => return this.step_record_change(1, cx),
                        "up" => return this.step_record_change(-1, cx),
                        _ => {}
                    }
                }
                if ev.keystroke.key == "escape" {
                    if this.tour_chooser {
                        this.tour_chooser = false;
                        return cx.notify();
                    }
                    if this.tour.is_some() {
                        return this.end_tour(cx);
                    }
                    // The focus may still be outside the window (e.g. right after it opened).
                    if this.add_dialog.is_some() {
                        return this.close_add_dialog(w, cx);
                    }
                    if this.books.dialog.is_some() {
                        return this.close_book_dialog(w, cx);
                    }
                    if this.gallery_escape(w, cx) {
                        return;
                    }
                    if this.drawer.take().is_some() {
                        return cx.notify();
                    }
                    if this.photo_draw.is_some() {
                        this.cancel_draw(cx);
                    } else if this.kinship.take().is_some()
                        || this.close_merge(cx)
                        || this.close_place_merge(cx)
                        || this.close_version_history(cx)
                        || this.close_record_history(cx)
                        || this.close_event_detail(cx)
                    {
                        cx.notify();
                    } else if this.editing {
                        this.cancel_edit(w, cx);
                    }
                }
            }))
            .bg(theme::bg())
            .text_color(theme::text())
            .text_sm()
            .child(self.render_header(cx))
            .child(div().flex_1().min_h_0().flex().relative().child(body).children(self.render_drawer(cx)))
            .when(self.doc.is_some(), |d| d.child(self.render_status_bar()))
            .children(self.render_kinship(cx))
            .children(self.render_merge(cx))
            .children(self.render_place_merge(cx))
            .children(self.render_version_history(cx))
            .children(self.render_record_history(cx))
            .children(self.render_event_detail(cx))
            .children(self.render_media_gallery(cx))
            .children(self.render_add_dialog(cx))
            .children(self.render_new_tree(cx))
            .children(self.render_book_dialog(cx))
            // Above the New Book window, which opens it to choose the book's person.
            .children(self.render_picker(cx))
            .children(self.render_new_place(cx))
            .children(self.render_report(cx))
            .children(self.render_palette(cx))
            .children(self.render_tour(window, cx))
    }
}

// ----------------------------------------------------------------------
// Access for the other windows.
// ----------------------------------------------------------------------

impl Workspace {
    /// Individual selected in the main window.
    pub(crate) fn selected_individual(&self) -> Option<String> {
        let doc = self.doc.as_ref()?;
        self.selected.clone().filter(|x| doc.ged.individual(x).is_some())
    }

    /// Applies a change to the file, with an undo point.
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

/// The field of a main-name part, when it has been added to the record being edited.
fn name_part_field(ed: &RecordEditor, part: NamePart) -> Option<Div> {
    let state = ed.names.as_ref()?.parts.iter().find(|(p, _)| *p == part)?.1.clone();
    Some(v_flex().gap_1().min_w_0().child(label_caps(part.label())).child(Input::new(&state).small()))
}

/// Words joined by spaces, empty ones left out.
fn join_words(words: &[&str]) -> String {
    words.iter().map(|w| w.trim()).filter(|w| !w.is_empty()).collect::<Vec<_>>().join(" ")
}

/// Under an individual's name: their nickname and their other names, by type, each showing only
/// what differs from the main name (“called Messaoud · also QUIRIEL, QUIRIELLE”). Empty if none.
fn names_line(names: &[NameForm], female: bool) -> String {
    let Some(main) = names.first() else { return String::new() };
    let surname = |n: &NameForm| join_words(&[&n.particle, &n.surname]);
    let mut groups: Vec<(&str, Vec<String>)> = Vec::new();
    for n in &names[1..] {
        let shown = if n.given.trim() == main.given.trim() || n.given.trim().is_empty() {
            surname(n)
        } else if surname(n) == surname(main) || n.surname.trim().is_empty() {
            n.given.trim().to_string()
        } else {
            join_words(&[&n.given, &surname(n)])
        };
        if shown.is_empty() {
            continue;
        }
        let kind = match n.kind.as_str() {
            k @ ("BIRTH" | "MAIDEN" | "MARRIED" | "IMMIGRANT" | "PROFESSIONAL") => k,
            _ => "AKA",
        };
        match groups.iter_mut().find(|(k, _)| *k == kind) {
            Some((_, list)) => list.push(shown),
            None => groups.push((kind, vec![shown])),
        }
    }
    let nickname = main.nickname.trim();
    let called = (!nickname.is_empty()).then(|| t!("names.called", name = nickname).into_owned());
    let others = groups.into_iter().map(|(kind, list)| {
        let names = list.join(", ");
        match kind {
            "BIRTH" if female => t!("names.born_f", names = names),
            "BIRTH" => t!("names.born", names = names),
            "MAIDEN" => t!("names.maiden", names = names),
            "MARRIED" => t!("names.married", names = names),
            "IMMIGRANT" => t!("names.immigrant", names = names),
            "PROFESSIONAL" => t!("names.professional", names = names),
            _ => t!("names.also", names = names),
        }
        .into_owned()
    });
    called.into_iter().chain(others).collect::<Vec<_>>().join(" · ")
}

#[cfg(test)]
mod autosave_tests {
    use super::*;
    use gedcom::versions::Store;

    fn opened(path: &std::path::Path) -> Document {
        let ged = gedcom::load(path).unwrap();
        Document {
            saved: ged.clone(),
            opened: ged.clone(),
            disk: disk_stamp(path),
            versioned_at: Some(std::time::Instant::now()),
            unversioned: false,
            settling: None,
            ged,
            path: Some(path.to_path_buf()),
            imported: false,
            sample: false,
            dirty: false,
            dirty_before_checkpoint: false,
            history: Vec::new(),
            future: Vec::new(),
            future_before_checkpoint: Vec::new(),
            checkpoints: 0,
        }
    }

    fn temp_tree() -> (PathBuf, PathBuf) {
        let dir = std::env::temp_dir()
            .join(format!("gedroots-autosave-{}", std::process::id()))
            .join(format!("{:?}", std::thread::current().id()).replace(['(', ')'], ""));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("tree.ged");
        std::fs::write(&path, include_str!("../../gedcom/tests/fixtures/example.ged")).unwrap();
        (dir, path)
    }

    #[::core::prelude::v1::test]
    fn automatic_saves_record_a_version_only_when_due_and_at_closing() {
        let (dir, path) = temp_tree();
        let mut doc = opened(&path);
        let store = Store::for_file(&path, None);
        // An automatic save right after opening writes the file, not a version.
        doc.ged.get_mut("I00003").unwrap().set_child_val("SEX", "M");
        doc.dirty = true;
        assert!(!doc.write(&path, Versioning::Throttled, true).unwrap());
        doc.dirty = false;
        assert!(gedcom::load(&path).unwrap().get("I00003").unwrap().child_val("SEX") == Some("M"));
        assert!(doc.unversioned);
        assert!(store.versions().unwrap().is_empty());
        // Closing records the last version.
        doc.flush();
        assert!(!doc.unversioned);
        assert_eq!(store.versions().unwrap().len(), 1);
        // Once the last version is old enough, an automatic save records one.
        doc.versioned_at = Some(std::time::Instant::now() - VERSION_EVERY);
        doc.ged.get_mut("I00003").unwrap().set_child_val("SEX", "F");
        assert!(doc.write(&path, Versioning::Throttled, true).unwrap());
        assert_eq!(store.versions().unwrap().len(), 2);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[::core::prelude::v1::test]
    fn closing_saves_unsaved_changes() {
        let (dir, path) = temp_tree();
        let mut doc = opened(&path);
        doc.ged.get_mut("I00003").unwrap().set_child_val("SEX", "M");
        doc.dirty = true;
        doc.flush();
        assert!(!doc.dirty);
        assert_eq!(gedcom::load(&path).unwrap().get("I00003").unwrap().child_val("SEX"), Some("M"));
        assert_eq!(doc.disk, disk_stamp(&path));
        // An imported tree is never written without asking where.
        let mut imported = opened(&path);
        imported.imported = true;
        imported.dirty = true;
        imported.ged.get_mut("I00003").unwrap().set_child_val("SEX", "F");
        imported.flush();
        assert_eq!(gedcom::load(&path).unwrap().get("I00003").unwrap().child_val("SEX"), Some("M"));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[::core::prelude::v1::test]
    fn named_and_restored_versions_keep_their_label() {
        use gedcom::versions::Kind;
        let (dir, path) = temp_tree();
        let mut doc = opened(&path);
        let store = Store::for_file(&path, None);
        // A named version is recorded even without a change, and at once.
        doc.write(&path, Versioning::Named("Before the merge".into()), true).unwrap();
        doc.ged.get_mut("I00003").unwrap().set_child_val("SEX", "M");
        doc.write(&path, Versioning::Restored("01793724394512-3f2b8c1e9a4d".into()), true).unwrap();
        let v = store.versions().unwrap();
        assert_eq!(v.len(), 2);
        assert_eq!((v[0].kind, v[0].label.as_deref()), (Kind::Named, Some("Before the merge")));
        assert_eq!((v[1].kind, v[1].label.as_deref()), (Kind::Restored, Some("01793724394512-3f2b8c1e9a4d")));
        std::fs::remove_dir_all(dir).unwrap();
    }
}
