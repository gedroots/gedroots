//! Application commands: actions, shortcuts and menus.
//!
//! Each command is described once, here, with its label and its shortcut. The same model
//! feeds:
//! - the native menu bar on macOS (`cx.set_menus`);
//! - the menu bar drawn inside the window on Linux and Windows;
//! - the command palette (⌘⇧P / Ctrl+Shift+P).
//!
//! Labels are translation keys (`crates/gedroots/locales/commands.yml`).

use gpui_kit::component::input;
use gpui_kit::*;
use rust_i18n::t;

actions!(
    gedroots,
    [
        // File
        NewFile,
        OpenFile,
        ClearRecent,
        ImportFile,
        OpenFileSettings,
        SaveFile,
        SaveFileAs,
        ExportFile,
        ConnectAgent,
        ReviewProposals,
        AcceptProposal,
        RejectProposal,
        Quit,
        // Edit
        Undo,
        Redo,
        EditRecord,
        DeleteRecord,
        CopyId,
        CopyName,
        FocusSearch,
        SaveSearch,
        RenameSearch,
        DeleteSearch,
        ToggleFlagged,
        AddWebLink,
        // Person
        NewIndividual,
        AddFather,
        AddMother,
        AddSpouse,
        AddChild,
        AddSibling,
        LinkFather,
        LinkMother,
        LinkSpouse,
        LinkChild,
        LinkSibling,
        GoToRoot,
        SearchAncestors,
        SearchDescendants,
        MediaGallery,
        AddEvent,
        // Source
        NewSource,
        NewRepository,
        NewNote,
        EnterRecord,
        // Media
        NewMedia,
        AttachMediaTo,
        TagPerson,
        ShowUnreferencedFiles,
        AddPages,
        PreviousPage,
        NextPage,
        MovePageLeft,
        MovePageRight,
        SplitPage,
        RemovePage,
        MergeWithMedia,
        MergeSelectedMedia,
        RenameMediaFiles,
        // View
        ToggleSidebar,
        ToggleInspector,
        GoBack,
        GoForward,
        ShowIndividuals,
        ShowSources,
        ShowPlaces,
        ShowMedia,
        ShowRepositories,
        ShowNotes,
        ShowQuality,
        ShowStats,
        KinshipWith,
        ToggleIgnoredIssues,
        ToggleRelativeEvents,
        ShowTree,
        ShowForm,
        ShowRaw,
        ToggleRecordPreview,
        TreeChart,
        TreeFan,
        CenterTree,
        MoreGenerations,
        FewerGenerations,
        ZoomIn,
        ZoomOut,
        ZoomToFit,
        ActualSize,
        FilterMen,
        FilterWomen,
        FilterAncestors,
        FilterUnsourced,
        FilterNoParents,
        FilterNoDate,
        FilterNoMarriage,
        FilterWithMedia,
        ClearSearch,
        // Help
        CommandPalette,
        TakeTour,
        OpenSampleTree,
        CheckForUpdates,
        ToggleAutomaticUpdates,
        ReleaseNotes,
        ReportIssue,
    ]
);

/// Opens a file from File › Open Recent.
#[derive(Clone, PartialEq, Action)]
#[action(namespace = gedroots, no_json)]
pub struct OpenRecent {
    pub path: std::path::PathBuf,
}

/// Opens the search for the selected person on a site of Person › Search On.
#[derive(Clone, PartialEq, Action)]
#[action(namespace = gedroots, no_json)]
pub struct SearchWeb {
    /// Position of the site in the file settings.
    pub index: usize,
}

/// Names of the search sites of the displayed file (File Settings › Search).
static SEARCH_SITES: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());

/// Records the search sites of the displayed file. Returns `true` if they changed: the menus then
/// need to be installed again.
pub fn set_search_sites(names: Vec<String>) -> bool {
    let mut sites = SEARCH_SITES.lock().unwrap_or_else(|e| e.into_inner());
    if *sites == names {
        return false;
    }
    *sites = names;
    SEARCH_SITES_GEN.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    true
}

static SEARCH_SITES_GEN: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// Version number of the search sites, so that each window reloads its menu bar.
pub fn search_sites_generation() -> u64 {
    SEARCH_SITES_GEN.load(std::sync::atomic::Ordering::Relaxed)
}

/// Commands that do not apply right now, by action name, for the menu bar drawn in the window
/// (Linux, Windows). macOS asks the window itself each time a menu opens.
static UNAVAILABLE: std::sync::Mutex<Vec<&'static str>> = std::sync::Mutex::new(Vec::new());

/// Records the commands that do not apply. Returns `true` if the list changed: the menus then need
/// to be installed again.
pub fn set_unavailable(mut names: Vec<&'static str>) -> bool {
    names.sort_unstable();
    names.dedup();
    let mut current = UNAVAILABLE.lock().unwrap_or_else(|e| e.into_inner());
    if *current == names {
        return false;
    }
    *current = names;
    SEARCH_SITES_GEN.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    true
}

fn is_unavailable(action: &dyn Action) -> bool {
    UNAVAILABLE.lock().unwrap_or_else(|e| e.into_inner()).binary_search(&action.name()).is_ok()
}

/// Every command of the menus (text editing actions left out), to check which ones apply.
pub fn menu_actions() -> Vec<Box<dyn Action>> {
    fn walk(items: Vec<Item>, out: &mut Vec<Box<dyn Action>>) {
        for i in items {
            match i {
                Item::Sub(_, items) => walk(items, out),
                i => out.extend(i.into_command().map(|(_, a)| a)),
            }
        }
    }
    let mut out = Vec::new();
    for (_, items) in model() {
        walk(items, &mut out);
    }
    out
}

/// Shortcut with ⌘ on macOS, Ctrl elsewhere.
macro_rules! shortcut {
    ($k:literal) => {
        if cfg!(target_os = "macos") { concat!("cmd-", $k) } else { concat!("ctrl-", $k) }
    };
}

/// Key context of the main window.
const WS: Option<&str> = Some("Workspace");

pub fn bind_keys(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new(shortcut!("n"), NewFile, None),
        KeyBinding::new(shortcut!("o"), OpenFile, None),
        KeyBinding::new(shortcut!("s"), SaveFile, None),
        KeyBinding::new(shortcut!("shift-s"), SaveFileAs, WS),
        KeyBinding::new(shortcut!("q"), Quit, None),
        KeyBinding::new(shortcut!("z"), Undo, WS),
        // Redo: ⌘⇧Z (macOS), Ctrl+Y (Windows), Ctrl+Shift+Z (Linux).
        KeyBinding::new(if cfg!(target_os = "windows") { "ctrl-y" } else { shortcut!("shift-z") }, Redo, WS),
        KeyBinding::new(shortcut!("shift-n"), NewIndividual, WS),
        KeyBinding::new(shortcut!("f"), FocusSearch, WS),
        KeyBinding::new(shortcut!("e"), EnterRecord, WS),
        // Like Contacts: ⌘L edits the record, and saves it when pressed again.
        KeyBinding::new(shortcut!("l"), EditRecord, WS),
        KeyBinding::new(shortcut!("enter"), AcceptProposal, WS),
        KeyBinding::new(shortcut!("backspace"), RejectProposal, WS),
        KeyBinding::new(shortcut!("shift-m"), MediaGallery, WS),
        KeyBinding::new(shortcut!("shift-h"), GoToRoot, WS),
        KeyBinding::new(shortcut!("shift-p"), CommandPalette, WS),
        KeyBinding::new(shortcut!(","), OpenFileSettings, WS),
        // Back / forward: ⌘[ ⌘] (macOS), Alt+← Alt+→ (Windows, Linux).
        KeyBinding::new(if cfg!(target_os = "macos") { "cmd-[" } else { "alt-left" }, GoBack, WS),
        KeyBinding::new(if cfg!(target_os = "macos") { "cmd-]" } else { "alt-right" }, GoForward, WS),
        // The four sidebar areas, then the lists and pages of the View menu.
        KeyBinding::new(shortcut!("1"), ShowIndividuals, WS),
        KeyBinding::new(shortcut!("2"), ShowSources, WS),
        KeyBinding::new(shortcut!("3"), ShowPlaces, WS),
        KeyBinding::new(shortcut!("4"), ShowMedia, WS),
        KeyBinding::new(shortcut!("5"), ShowRepositories, WS),
        KeyBinding::new(shortcut!("6"), ShowNotes, WS),
        KeyBinding::new(shortcut!("7"), ShowQuality, WS),
        KeyBinding::new(shortcut!("8"), ShowStats, WS),
        // Like Finder and Mail: ⌃⌘S (Ctrl+Alt+S elsewhere).
        KeyBinding::new(if cfg!(target_os = "macos") { "ctrl-cmd-s" } else { "ctrl-alt-s" }, ToggleSidebar, WS),
        // Like the inspectors of Finder, Pages or Xcode: ⌥⌘I (Ctrl+Alt+I elsewhere).
        KeyBinding::new(shortcut!("alt-i"), ToggleInspector, WS),
        KeyBinding::new(shortcut!("alt-1"), ShowTree, WS),
        KeyBinding::new(shortcut!("alt-2"), ShowForm, WS),
        KeyBinding::new(shortcut!("alt-3"), ShowRaw, WS),
        KeyBinding::new(shortcut!("="), MoreGenerations, WS),
        KeyBinding::new(shortcut!("-"), FewerGenerations, WS),
        // Large images: like Preview, Photos and browsers. ⌘= ⌘- are shared with the fan chart's
        // generations: only one of them is available at a time, the key goes to that one.
        KeyBinding::new(shortcut!("="), ZoomIn, WS),
        KeyBinding::new(shortcut!("+"), ZoomIn, WS),
        KeyBinding::new(shortcut!("-"), ZoomOut, WS),
        KeyBinding::new(shortcut!("0"), ZoomToFit, WS),
        KeyBinding::new("pageup", PreviousPage, WS),
        KeyBinding::new("pagedown", NextPage, WS),
        // Raw GEDCOM editor suggestions on demand.
        KeyBinding::new("ctrl-space", input::ShowCompletions, Some("Input")),
    ]);
}

/// Labels shared by the menu bar and context menus (right click): a command has the
/// same name wherever it appears.
pub mod label {
    use gpui_kit::SharedString;
    use rust_i18n::t;

    pub fn new_individual() -> SharedString {
        t!("cmd.new_individual").into()
    }
    pub fn new_source() -> SharedString {
        t!("cmd.new_source").into()
    }
    pub fn new_media() -> SharedString {
        t!("cmd.new_media").into()
    }
    pub fn new_repository() -> SharedString {
        t!("cmd.new_repository").into()
    }
    pub fn new_note() -> SharedString {
        t!("cmd.new_note").into()
    }
    pub fn media_gallery() -> SharedString {
        t!("cmd.media_gallery").into()
    }
    pub fn copy_id() -> SharedString {
        t!("cmd.copy_id").into()
    }
    pub fn copy_name() -> SharedString {
        t!("cmd.copy_name").into()
    }
    pub fn delete() -> SharedString {
        t!("cmd.delete").into()
    }
    pub fn add_relative() -> SharedString {
        t!("cmd.add_relative").into()
    }
    pub fn link_person() -> SharedString {
        t!("cmd.link_person").into()
    }
    pub fn father() -> SharedString {
        t!("cmd.father").into()
    }
    pub fn mother() -> SharedString {
        t!("cmd.mother").into()
    }
    pub fn spouse() -> SharedString {
        t!("cmd.spouse").into()
    }
    pub fn child() -> SharedString {
        t!("cmd.child").into()
    }
    pub fn sibling() -> SharedString {
        t!("cmd.sibling").into()
    }
    pub fn father_link() -> SharedString {
        t!("cmd.father_link").into()
    }
    pub fn mother_link() -> SharedString {
        t!("cmd.mother_link").into()
    }
    pub fn spouse_link() -> SharedString {
        t!("cmd.spouse_link").into()
    }
    pub fn child_link() -> SharedString {
        t!("cmd.child_link").into()
    }
    pub fn sibling_link() -> SharedString {
        t!("cmd.sibling_link").into()
    }
    pub fn add_event() -> SharedString {
        t!("cmd.add_event").into()
    }
    pub fn go_to_root() -> SharedString {
        t!("cmd.go_to_root").into()
    }
    pub fn center_tree() -> SharedString {
        t!("cmd.center_tree").into()
    }
    pub fn enter_act() -> SharedString {
        t!("cmd.enter_act").into()
    }
    pub fn attach_media_to() -> SharedString {
        t!("cmd.attach_media_to").into()
    }
    pub fn tag_person() -> SharedString {
        t!("cmd.tag_person").into()
    }
    pub fn merge_with_media() -> SharedString {
        t!("cmd.merge_with_media").into()
    }
    pub fn merge_selected_media() -> SharedString {
        t!("cmd.merge_selected_media").into()
    }
    pub fn add_pages() -> SharedString {
        t!("cmd.add_pages").into()
    }
    pub fn clear_search() -> SharedString {
        t!("cmd.clear_search").into()
    }
    pub fn search_web() -> SharedString {
        t!("cmd.search_web").into()
    }
    pub fn save_search() -> SharedString {
        t!("cmd.save_search").into()
    }
    pub fn rename_search() -> SharedString {
        t!("cmd.rename_search").into()
    }
    pub fn delete_search() -> SharedString {
        t!("cmd.delete_search").into()
    }
}

/// A menu entry.
enum Item {
    Action(SharedString, Box<dyn Action>),
    /// Computed label: recent file, search site.
    Recent(String, Box<dyn Action>),
    /// Standard system action (copy, paste…): macOS wires it to text fields.
    Os(SharedString, Box<dyn Action>, OsAction),
    /// Setting with a check mark.
    Check(SharedString, Box<dyn Action>, bool),
    Sub(SharedString, Vec<Item>),
    Separator,
}

impl Item {
    /// Label and action of a command (not a submenu, a separator or a text editing action).
    fn into_command(self) -> Option<(String, Box<dyn Action>)> {
        match self {
            Item::Action(name, action) => Some((name.to_string(), action)),
            Item::Recent(name, action) => Some((name, action)),
            Item::Check(name, action, _) => Some((name.to_string(), action)),
            Item::Os(..) | Item::Sub(..) | Item::Separator => None,
        }
    }
}

fn a(label: impl Into<SharedString>, action: impl Action) -> Item {
    Item::Action(label.into(), Box::new(action))
}

/// The menu model, shared by every platform.
fn model() -> Vec<(SharedString, Vec<Item>)> {
    let sites = SEARCH_SITES.lock().unwrap_or_else(|e| e.into_inner()).clone();
    model_with(&crate::recent::list(), &sites)
}

/// The menu model, with the given recent files and search sites.
fn model_with(recents: &[std::path::PathBuf], sites: &[String]) -> Vec<(SharedString, Vec<Item>)> {
    use Item::{Separator as Sep, Sub};
    let mac = cfg!(target_os = "macos");
    let mut menus = Vec::new();
    // Updates: in the application menu on macOS (like Sparkle apps), in Help elsewhere (like
    // Windows apps, Zed, VS Code). Once an update is downloaded, the command restarts on it.
    let updates = || {
        let check = if matches!(crate::updates::state(), crate::updates::State::Ready { .. }) {
            t!("cmd.restart_to_update")
        } else {
            t!("cmd.check_for_updates")
        };
        vec![
            a(check, CheckForUpdates),
            Item::Check(
                t!("cmd.automatic_updates").into(),
                Box::new(ToggleAutomaticUpdates),
                crate::updates::load().automatic,
            ),
        ]
    };
    if mac {
        // On macOS, settings go where they usually are: the application menu, ⌘,.
        let mut app = updates();
        app.extend([Sep, a(t!("cmd.file_settings"), OpenFileSettings), Sep, a(t!("cmd.quit_app"), Quit)]);
        menus.push(("GEDRoots".into(), app));
    }
    let mut file = vec![a(t!("cmd.new_file"), NewFile), a(t!("cmd.open"), OpenFile)];
    // Submenu hidden until a file has been opened.
    if !recents.is_empty() {
        let names = crate::recent::labels(recents);
        let mut items: Vec<Item> =
            recents.iter().zip(names).map(|(p, n)| Item::Recent(n, Box::new(OpenRecent { path: p.clone() }))).collect();
        items.extend([Sep, a(t!("cmd.clear_recent"), ClearRecent)]);
        file.push(Sub(t!("menu.open_recent").into(), items));
    }
    file.extend([
        a(t!("cmd.import_gedcom"), ImportFile),
        Sep,
        a(t!("cmd.save"), SaveFile),
        a(t!("cmd.save_as"), SaveFileAs),
        Sep,
        a(t!("cmd.export"), ExportFile),
    ]);
    if !mac {
        file.extend([Sep, a(t!("cmd.file_settings"), OpenFileSettings), Sep, a(t!("cmd.quit"), Quit)]);
    }
    menus.push((t!("menu.file").into(), file));
    menus.push((
        t!("menu.edit").into(),
        vec![
            a(t!("cmd.undo"), Undo),
            a(t!("cmd.redo"), Redo),
            Sep,
            Item::Os(t!("cmd.cut").into(), Box::new(input::Cut), OsAction::Cut),
            Item::Os(t!("cmd.copy").into(), Box::new(input::Copy), OsAction::Copy),
            Item::Os(t!("cmd.paste").into(), Box::new(input::Paste), OsAction::Paste),
            Item::Os(t!("cmd.select_all").into(), Box::new(input::SelectAll), OsAction::SelectAll),
            Sep,
            a(t!("cmd.edit_record"), EditRecord),
            a(t!("cmd.add_web_link"), AddWebLink),
            Sep,
            // Rarely used: one level down, so that Edit stays within 12 items.
            Sub(t!("menu.copy_special").into(), vec![a(label::copy_name(), CopyName), a(label::copy_id(), CopyId)]),
            a(label::delete(), DeleteRecord),
            Sep,
            a(t!("cmd.find"), FocusSearch),
        ],
    ));
    menus.push((
        t!("menu.person").into(),
        vec![
            a(label::new_individual(), NewIndividual),
            Sep,
            Sub(
                label::add_relative(),
                vec![
                    a(label::father(), AddFather),
                    a(label::mother(), AddMother),
                    a(label::spouse(), AddSpouse),
                    a(label::child(), AddChild),
                    a(label::sibling(), AddSibling),
                ],
            ),
            Sub(
                label::link_person(),
                vec![
                    a(label::father_link(), LinkFather),
                    a(label::mother_link(), LinkMother),
                    a(label::spouse_link(), LinkSpouse),
                    a(label::child_link(), LinkChild),
                    a(label::sibling_link(), LinkSibling),
                ],
            ),
            a(label::add_event(), AddEvent),
            Sep,
            a(t!("cmd.kinship_with"), KinshipWith),
            a(t!("cmd.search_ancestors"), SearchAncestors),
            a(t!("cmd.search_descendants"), SearchDescendants),
        ],
    ));
    // Search sites of the file; submenu hidden if there are none.
    if !sites.is_empty() {
        let items =
            sites.iter().enumerate().map(|(index, name)| Item::Recent(name.clone(), Box::new(SearchWeb { index })));
        if let Some((_, indi)) = menus.last_mut() {
            indi.push(Sub(label::search_web(), items.collect()));
        }
    }
    // Agents propose records: connecting one and reviewing what it proposed go together.
    menus.push((
        t!("menu.source").into(),
        vec![
            a(label::enter_act(), EnterRecord),
            Sep,
            a(t!("cmd.review_proposals"), ReviewProposals),
            a(t!("cmd.accept_proposal"), AcceptProposal),
            a(t!("cmd.reject_proposal"), RejectProposal),
            a(t!("cmd.connect_agent"), ConnectAgent),
            Sep,
            a(label::new_source(), NewSource),
            a(label::new_repository(), NewRepository),
            a(label::new_note(), NewNote),
        ],
    ));
    menus.push((
        t!("menu.media").into(),
        vec![
            a(label::new_media(), NewMedia),
            a(label::attach_media_to(), AttachMediaTo),
            a(label::media_gallery(), MediaGallery),
            Sep,
            Sub(
                t!("menu.pages").into(),
                vec![
                    a(t!("cmd.previous_page"), PreviousPage),
                    a(t!("cmd.next_page"), NextPage),
                    Sep,
                    a(label::add_pages(), AddPages),
                    a(t!("cmd.move_page_left"), MovePageLeft),
                    a(t!("cmd.move_page_right"), MovePageRight),
                    Sep,
                    a(t!("cmd.split_page"), SplitPage),
                    a(t!("cmd.remove_page"), RemovePage),
                ],
            ),
            a(label::merge_with_media(), MergeWithMedia),
            a(label::merge_selected_media(), MergeSelectedMedia),
            a(t!("cmd.rename_media_files"), RenameMediaFiles),
            Sep,
            a(label::tag_person(), TagPerson),
        ],
    ));
    menus.push((
        t!("menu.view").into(),
        vec![
            a(t!("cmd.show_tree"), ShowTree),
            a(t!("cmd.show_form"), ShowForm),
            a(t!("cmd.show_raw"), ShowRaw),
            a(t!("cmd.record_preview"), ToggleRecordPreview),
            Sep,
            a(t!("cmd.toggle_sidebar"), ToggleSidebar),
            a(t!("cmd.toggle_inspector"), ToggleInspector),
            a(t!("cmd.relative_events"), ToggleRelativeEvents),
            Sep,
            Sub(
                t!("menu.zoom").into(),
                vec![
                    a(t!("cmd.zoom_in"), ZoomIn),
                    a(t!("cmd.zoom_out"), ZoomOut),
                    Sep,
                    a(t!("cmd.zoom_to_fit"), ZoomToFit),
                    a(t!("cmd.actual_size"), ActualSize),
                ],
            ),
            Sub(
                t!("menu.tree").into(),
                vec![
                    a(t!("cmd.tree_chart"), TreeChart),
                    a(t!("cmd.tree_fan"), TreeFan),
                    Sep,
                    a(label::center_tree(), CenterTree),
                    Sep,
                    a(t!("cmd.more_generations"), MoreGenerations),
                    a(t!("cmd.fewer_generations"), FewerGenerations),
                ],
            ),
            Sub(
                t!("menu.filter_list").into(),
                vec![
                    a(t!("cmd.filter_men"), FilterMen),
                    a(t!("cmd.filter_women"), FilterWomen),
                    a(t!("cmd.filter_ancestors"), FilterAncestors),
                    a(t!("cmd.filter_unsourced"), FilterUnsourced),
                    a(t!("cmd.filter_no_parents"), FilterNoParents),
                    a(t!("cmd.filter_no_date"), FilterNoDate),
                    a(t!("cmd.filter_no_marriage"), FilterNoMarriage),
                    a(t!("cmd.filter_with_media"), FilterWithMedia),
                    Sep,
                    a(t!("cmd.flagged_only"), ToggleFlagged),
                    a(t!("cmd.ignored_issues"), ToggleIgnoredIssues),
                    a(label::clear_search(), ClearSearch),
                ],
            ),
            Sep,
            a(label::save_search(), SaveSearch),
            a(label::rename_search(), RenameSearch),
            a(label::delete_search(), DeleteSearch),
        ],
    ));
    // Like Finder's Go menu: history, then the places of the file.
    menus.push((
        t!("menu.go").into(),
        vec![
            a(t!("cmd.back"), GoBack),
            a(t!("cmd.forward"), GoForward),
            Sep,
            a(t!("cmd.individuals"), ShowIndividuals),
            a(t!("cmd.sources"), ShowSources),
            a(t!("cmd.places"), ShowPlaces),
            a(t!("cmd.media_list"), ShowMedia),
            a(t!("cmd.repositories"), ShowRepositories),
            a(t!("cmd.notes"), ShowNotes),
            Sep,
            a(t!("cmd.quality"), ShowQuality),
            a(t!("cmd.unreferenced_files"), ShowUnreferencedFiles),
            a(t!("cmd.stats"), ShowStats),
            Sep,
            a(label::go_to_root(), GoToRoot),
        ],
    ));
    let mut help = vec![
        a(t!("cmd.all_commands"), CommandPalette),
        a(t!("cmd.take_tour"), TakeTour),
        a(t!("cmd.open_sample"), OpenSampleTree),
        Sep,
        a(t!("cmd.release_notes"), ReleaseNotes),
        a(t!("cmd.report_issue"), ReportIssue),
    ];
    if !mac {
        help.push(Sep);
        help.extend(updates());
    }
    menus.push((t!("menu.help").into(), help));
    menus
}

fn to_menu_items(items: Vec<Item>) -> Vec<MenuItem> {
    items
        .into_iter()
        .map(|i| match i {
            Item::Action(name, action) => {
                let disabled = is_unavailable(action.as_ref());
                MenuItem::Action { name, action, os_action: None, checked: false, disabled }
            }
            Item::Recent(name, action) => {
                let disabled = is_unavailable(action.as_ref());
                MenuItem::Action { name: name.into(), action, os_action: None, checked: false, disabled }
            }
            Item::Os(name, action, os) => {
                MenuItem::Action { name, action, os_action: Some(os), checked: false, disabled: false }
            }
            Item::Check(name, action, checked) => {
                let disabled = is_unavailable(action.as_ref());
                MenuItem::Action { name, action, os_action: None, checked, disabled }
            }
            Item::Sub(name, items) => MenuItem::submenu(Menu::new(name).items(to_menu_items(items))),
            Item::Separator => MenuItem::separator(),
        })
        .collect()
}

pub fn menus() -> Vec<Menu> {
    model().into_iter().map(|(name, items)| Menu::new(name).items(to_menu_items(items))).collect()
}

/// Installs the menus: native on macOS, and for the bar drawn inside the window elsewhere
/// (see `AppMenuBar`).
pub fn install_menus(cx: &mut App) {
    cx.set_menus(menus());
    let owned = menus().into_iter().map(Menu::owned).collect();
    gpui_kit::base::GlobalState::global_mut(cx).set_app_menus(owned);
}

/// A command for the palette: its path in the menus (“Person › Add Relative › Father”) and action.
pub struct PaletteEntry {
    pub path: String,
    pub action: Box<dyn Action>,
}

/// Every menu command, flattened, for the palette.
/// Text editing actions (cut, paste…) are left out.
pub fn palette_entries() -> Vec<PaletteEntry> {
    fn walk(prefix: &str, items: Vec<Item>, out: &mut Vec<PaletteEntry>) {
        for i in items {
            match i {
                i @ (Item::Action(..) | Item::Recent(..) | Item::Check(..)) => {
                    let Some((name, action)) = i.into_command() else { continue };
                    let path = format!("{prefix} › {}", name.trim_end_matches('…'));
                    if !out.iter().any(|e| e.action.partial_eq(action.as_ref())) {
                        out.push(PaletteEntry { path, action });
                    }
                }
                Item::Sub(name, items) => walk(&format!("{prefix} › {name}"), items, out),
                Item::Os(..) | Item::Separator => {}
            }
        }
    }
    let mut out = Vec::new();
    for (menu, items) in model() {
        walk(&menu, items, &mut out);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{Item, model_with, palette_entries};
    use gpui_kit::Action;
    use std::path::PathBuf;

    /// A command has only one place in the menu bar.
    #[test]
    fn menus_have_every_command_once() {
        fn walk(path: &str, items: Vec<Item>, seen: &mut Vec<(String, Box<dyn Action>)>) {
            for i in items {
                match i {
                    i @ (Item::Action(..) | Item::Recent(..) | Item::Check(..)) => {
                        let Some((name, action)) = i.into_command() else { continue };
                        let here = format!("{path} › {name}");
                        if let Some((other, _)) = seen.iter().find(|(_, a)| a.partial_eq(action.as_ref())) {
                            panic!("“{here}” duplicates “{other}”");
                        }
                        seen.push((here, action));
                    }
                    Item::Sub(name, items) => walk(&format!("{path} › {name}"), items, seen),
                    Item::Os(..) | Item::Separator => {}
                }
            }
        }
        let mut seen = Vec::new();
        let recents = [PathBuf::from("/a/Martin.ged"), PathBuf::from("/b/Martin.ged")];
        let sites = ["Geneanet".to_string(), "FamilySearch".to_string()];
        for (menu, items) in model_with(&recents, &sites) {
            walk(&menu, items, &mut seen);
        }
    }

    /// design.md: menus of 3 to 12 items (separators left out, a submenu counts as one).
    #[test]
    fn menus_stay_short() {
        let recents = [PathBuf::from("/a/Martin.ged")];
        let sites = ["Geneanet".to_string()];
        for (menu, items) in model_with(&recents, &sites) {
            let n = items.iter().filter(|i| !matches!(i, Item::Separator)).count();
            assert!((3..=12).contains(&n), "{menu} has {n} items");
        }
    }

    #[test]
    fn palette_has_every_command_once() {
        super::set_search_sites(vec!["Geneanet".into()]);
        let entries = palette_entries();
        assert!(entries.iter().any(|e| e.path == "Person › Search On › Geneanet"));
        assert!(entries.iter().any(|e| e.path == "Person › Add Relative › Father"));
        assert!(entries.iter().any(|e| e.path == "Source › Enter Record"));
        assert!(entries.iter().any(|e| e.path == "File › Export"));
        for (i, e) in entries.iter().enumerate() {
            assert!(
                !entries[i + 1..].iter().any(|o| o.action.partial_eq(e.action.as_ref())),
                "{} appears twice",
                e.path
            );
        }
    }
}
