//! Application commands: actions, shortcuts and menus.
//!
//! Each command is described once, here, with its label and its shortcut. The same model
//! feeds:
//! - the native menu bar on macOS (`cx.set_menus`);
//! - the menu bar drawn inside the window on Linux and Windows;
//! - the command palette (⌘⇧P / Ctrl+Shift+P).
//!
//! Labels are translation keys (`crates/gedroots/locales/commands.yml`).

use crate::t;
use gpui_kit::component::input;
use gpui_kit::*;

actions!(
    gedroots,
    [
        // File
        NewWindow,
        NewFile,
        Create,
        OpenFile,
        ClearRecent,
        OpenFileSettings,
        SaveFile,
        SaveFileAs,
        SaveVersion,
        RevertToOpened,
        BrowseVersions,
        ExportFile,
        NewBook,
        ConnectAgent,
        MobileDevices,
        ReviewProposals,
        AcceptProposal,
        RejectProposal,
        Quit,
        // Edit
        Undo,
        Redo,
        EditRecord,
        RecordHistory,
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
        AddOtherName,
        AddNickname,
        AddNameTitle,
        AddNameParticle,
        AddNameSuffix,
        NoteSearch,
        IgnoreLead,
        ResumeLead,
        CopyAgentRequest,
        // Source
        NewSource,
        NewRepository,
        NewNote,
        EnterRecord,
        // Place
        LocatePlace,
        PlaceOnMap,
        MergeWithPlace,
        FindDuplicatePlaces,
        // Media
        NewMedia,
        AttachMediaTo,
        TagPerson,
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
        ShowResearch,
        ShowQuality,
        ShowStats,
        KinshipWith,
        MergeWithPerson,
        FindDuplicates,
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
        // Window
        CloseWindow,
        MinimizeWindow,
        ZoomWindow,
        BringAllToFront,
        // Help
        About,
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

/// Window › one of the open trees (the menu bar drawn in the window; macOS lists them itself).
#[derive(Clone, PartialEq, Action)]
#[action(namespace = gedroots, no_json)]
pub struct ActivateWindowAt {
    pub index: usize,
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
        // Like any document-based app: a new window, on the welcome screen (create or open a tree).
        // ⌘N creates what the page lists (Create button); a new window is ⌥⌘N, like Mail.
        KeyBinding::new(shortcut!("n"), Create, WS),
        KeyBinding::new(shortcut!("alt-n"), NewWindow, None),
        KeyBinding::new(shortcut!("w"), CloseWindow, WS),
        KeyBinding::new(shortcut!("m"), MinimizeWindow, WS),
        KeyBinding::new(shortcut!("o"), OpenFile, None),
        KeyBinding::new(shortcut!("s"), SaveFile, None),
        KeyBinding::new(shortcut!("shift-s"), SaveFileAs, WS),
        KeyBinding::new(shortcut!("q"), Quit, None),
        KeyBinding::new(shortcut!("z"), Undo, WS),
        // Redo: ⌘⇧Z (macOS), Ctrl+Y (Windows), Ctrl+Shift+Z (Linux).
        KeyBinding::new(if cfg!(target_os = "windows") { "ctrl-y" } else { shortcut!("shift-z") }, Redo, WS),
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
        KeyBinding::new(shortcut!("7"), ShowResearch, WS),
        KeyBinding::new(shortcut!("8"), ShowQuality, WS),
        KeyBinding::new(shortcut!("9"), ShowStats, WS),
        KeyBinding::new(shortcut!("shift-l"), NoteSearch, WS),
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
    use crate::t;
    use gpui_kit::SharedString;

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
    pub fn record_history() -> SharedString {
        t!("cmd.record_history").into()
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
    pub fn note_search() -> SharedString {
        t!("cmd.note_search").into()
    }
    pub fn ignore_lead() -> SharedString {
        t!("cmd.ignore_lead").into()
    }
    pub fn resume_lead() -> SharedString {
        t!("cmd.resume_lead").into()
    }
    pub fn copy_agent_request() -> SharedString {
        t!("cmd.copy_agent_request").into()
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
///
/// The menus only hold what no visible control does: a command with a button, a “+”, a link of
/// the sidebar or the top bar's Create button is left out of them, and kept in the palette with
/// its shortcut (see [`palette_only`]).
fn model_with(recents: &[std::path::PathBuf], sites: &[String]) -> Vec<(SharedString, Vec<Item>)> {
    use Item::{Separator as Sep, Sub};
    let mac = cfg!(target_os = "macos");
    let mut menus = Vec::new();
    // Updates. Once an update is downloaded, the command restarts on it.
    let check = if matches!(crate::updates::state(), crate::updates::State::Ready { .. }) {
        t!("cmd.restart_to_update")
    } else {
        t!("cmd.check_for_updates")
    };
    // The application menu, on every platform: what belongs to the app rather than to a tree
    // (updates, AI agents, mobile devices), then the settings and Quit, like macOS apps.
    menus.push((
        "GEDRoots".into(),
        vec![
            a(t!("cmd.about"), About),
            Sep,
            a(check, CheckForUpdates),
            Item::Check(
                t!("cmd.automatic_updates").into(),
                Box::new(ToggleAutomaticUpdates),
                crate::updates::load().automatic,
            ),
            Sep,
            a(t!("cmd.connect_agent"), ConnectAgent),
            a(t!("cmd.mobile_devices"), MobileDevices),
            Sep,
            a(t!("cmd.file_settings"), OpenFileSettings),
            Sep,
            a(if mac { t!("cmd.quit_app") } else { t!("cmd.quit") }, Quit),
        ],
    ));
    let mut file =
        vec![a(t!("cmd.new_window"), NewWindow), a(t!("cmd.new_file"), NewFile), a(t!("cmd.open"), OpenFile)];
    // Submenu hidden until a file has been opened.
    if !recents.is_empty() {
        let names = crate::recent::labels(recents);
        let mut items: Vec<Item> =
            recents.iter().zip(names).map(|(p, n)| Item::Recent(n, Box::new(OpenRecent { path: p.clone() }))).collect();
        items.extend([Sep, a(t!("cmd.clear_recent"), ClearRecent)]);
        file.push(Sub(t!("menu.open_recent").into(), items));
    }
    file.extend([
        Sep,
        a(t!("cmd.close_window"), CloseWindow),
        a(t!("cmd.save"), SaveFile),
        a(t!("cmd.save_as"), SaveFileAs),
        a(t!("cmd.save_version"), SaveVersion),
        // Like macOS documents: Revert To › Last Opened, Browse All Versions….
        Sub(
            t!("menu.revert_to").into(),
            vec![a(t!("cmd.revert_to_opened"), RevertToOpened), Sep, a(t!("cmd.browse_versions"), BrowseVersions)],
        ),
        Sep,
        a(t!("cmd.export"), ExportFile),
    ]);
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
            a(label::record_history(), RecordHistory),
            // Rarely used: one level down.
            Sub(t!("menu.copy_special").into(), vec![a(label::copy_name(), CopyName), a(label::copy_id(), CopyId)]),
            a(label::delete(), DeleteRecord),
            Sep,
            a(t!("cmd.find"), FocusSearch),
        ],
    ));
    let mut person = vec![
        a(label::go_to_root(), GoToRoot),
        Sep,
        a(t!("cmd.kinship_with"), KinshipWith),
        a(t!("cmd.search_ancestors"), SearchAncestors),
        a(t!("cmd.search_descendants"), SearchDescendants),
        Sep,
        a(t!("cmd.merge_with_person"), MergeWithPerson),
        a(t!("cmd.find_duplicates"), FindDuplicates),
        Sep,
        Sub(t!("menu.research").into(), vec![a(label::ignore_lead(), IgnoreLead), a(label::resume_lead(), ResumeLead)]),
    ];
    // Search sites of the file; submenu hidden if there are none.
    if !sites.is_empty() {
        let items =
            sites.iter().enumerate().map(|(index, name)| Item::Recent(name.clone(), Box::new(SearchWeb { index })));
        person.push(Sub(label::search_web(), items.collect()));
    }
    menus.push((t!("menu.person").into(), person));
    // Places are not records: their commands apply to the place shown on the Places page.
    menus.push((
        t!("menu.place").into(),
        vec![
            a(t!("cmd.locate_place"), LocatePlace),
            Sep,
            a(t!("cmd.merge_with_place"), MergeWithPlace),
            a(t!("cmd.find_duplicate_places"), FindDuplicatePlaces),
        ],
    ));
    menus.push((
        t!("menu.media").into(),
        vec![
            a(label::attach_media_to(), AttachMediaTo),
            a(label::tag_person(), TagPerson),
            Sep,
            Sub(
                t!("menu.pages").into(),
                vec![
                    a(t!("cmd.previous_page"), PreviousPage),
                    a(t!("cmd.next_page"), NextPage),
                    Sep,
                    a(t!("cmd.move_page_left"), MovePageLeft),
                    a(t!("cmd.move_page_right"), MovePageRight),
                    Sep,
                    a(t!("cmd.split_page"), SplitPage),
                    a(t!("cmd.remove_page"), RemovePage),
                ],
            ),
            a(label::merge_with_media(), MergeWithMedia),
            a(label::merge_selected_media(), MergeSelectedMedia),
            Sep,
            a(t!("cmd.rename_media_files"), RenameMediaFiles),
        ],
    ));
    menus.push((
        t!("menu.view").into(),
        vec![
            a(t!("cmd.show_tree"), ShowTree),
            a(t!("cmd.show_form"), ShowForm),
            a(t!("cmd.show_raw"), ShowRaw),
            Sep,
            a(t!("cmd.toggle_sidebar"), ToggleSidebar),
            a(t!("cmd.toggle_inspector"), ToggleInspector),
            a(t!("cmd.record_preview"), ToggleRecordPreview),
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
                    a(t!("cmd.flagged_only"), ToggleFlagged),
                    a(t!("cmd.ignored_issues"), ToggleIgnoredIssues),
                    a(label::clear_search(), ClearSearch),
                ],
            ),
            Sep,
            a(label::rename_search(), RenameSearch),
            a(label::delete_search(), DeleteSearch),
        ],
    ));
    // One window per tree. macOS fills this menu with the windows and their tabs by itself.
    let mut window = match mac {
        true => vec![a(t!("cmd.minimize_mac"), MinimizeWindow), a(t!("cmd.zoom_mac"), ZoomWindow)],
        false => vec![a(t!("cmd.minimize"), MinimizeWindow), a(t!("cmd.zoom"), ZoomWindow)],
    };
    if mac {
        window.extend([Sep, a(t!("cmd.bring_all_to_front"), BringAllToFront)]);
    } else {
        let listed = crate::windows::listed();
        if !listed.titles.is_empty() {
            window.push(Sep);
        }
        for (index, title) in listed.titles.into_iter().enumerate() {
            window.push(Item::Check(title.into(), Box::new(ActivateWindowAt { index }), listed.active == Some(index)));
        }
    }
    menus.push((t!("menu.window").into(), window));
    menus.push((
        t!("menu.help").into(),
        vec![
            a(t!("cmd.all_commands"), CommandPalette),
            a(t!("cmd.take_tour"), TakeTour),
            a(t!("cmd.open_sample"), OpenSampleTree),
            Sep,
            a(t!("cmd.release_notes"), ReleaseNotes),
            a(t!("cmd.report_issue"), ReportIssue),
        ],
    ));
    menus
}

/// Commands left out of the menus because a visible control does them (a button, a “+”, the
/// sidebar, the top bar's arrows or Create button). The palette still lists them, with their
/// shortcut, grouped as before.
fn palette_only() -> Vec<(SharedString, Vec<Item>)> {
    use Item::Sub;
    vec![
        (t!("menu.file").into(), vec![a(t!("cmd.create"), Create)]),
        (t!("menu.edit").into(), vec![a(t!("cmd.edit_record"), EditRecord), a(t!("cmd.add_web_link"), AddWebLink)]),
        (
            t!("menu.person").into(),
            vec![
                a(label::new_individual(), NewIndividual),
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
                Sub(
                    t!("cmd.add_to_name").into(),
                    vec![
                        a(t!("cmd.other_name"), AddOtherName),
                        a(t!("cmd.nickname"), AddNickname),
                        a(t!("cmd.name_title"), AddNameTitle),
                        a(t!("cmd.name_particle"), AddNameParticle),
                        a(t!("cmd.name_suffix"), AddNameSuffix),
                    ],
                ),
                a(t!("cmd.relative_events"), ToggleRelativeEvents),
                Sub(
                    t!("menu.research").into(),
                    vec![a(label::note_search(), NoteSearch), a(label::copy_agent_request(), CopyAgentRequest)],
                ),
            ],
        ),
        (
            t!("menu.source").into(),
            vec![
                a(label::enter_act(), EnterRecord),
                a(label::new_source(), NewSource),
                a(label::new_repository(), NewRepository),
                a(label::new_note(), NewNote),
                a(t!("cmd.new_book"), NewBook),
                a(t!("cmd.review_proposals"), ReviewProposals),
                a(t!("cmd.accept_proposal"), AcceptProposal),
                a(t!("cmd.reject_proposal"), RejectProposal),
            ],
        ),
        (
            t!("menu.media").into(),
            vec![
                a(label::new_media(), NewMedia),
                a(label::media_gallery(), MediaGallery),
                a(label::add_pages(), AddPages),
            ],
        ),
        (t!("menu.place").into(), vec![a(t!("cmd.place_on_map"), PlaceOnMap)]),
        (
            t!("menu.view").into(),
            vec![
                a(label::save_search(), SaveSearch),
                // The search field suggests these criteria, with their number of results.
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
                    ],
                ),
            ],
        ),
        (
            t!("menu.go").into(),
            vec![
                a(t!("cmd.back"), GoBack),
                a(t!("cmd.forward"), GoForward),
                a(t!("cmd.individuals"), ShowIndividuals),
                a(t!("cmd.sources"), ShowSources),
                a(t!("cmd.places"), ShowPlaces),
                a(t!("cmd.media_list"), ShowMedia),
                a(t!("cmd.repositories"), ShowRepositories),
                a(t!("cmd.notes"), ShowNotes),
                a(t!("cmd.research"), ShowResearch),
                a(t!("cmd.quality"), ShowQuality),
                a(t!("cmd.stats"), ShowStats),
            ],
        ),
    ]
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
    #[cfg(target_os = "macos")]
    crate::windows::adopt_windows_menu(&t!("menu.window"));
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
    for (menu, items) in model().into_iter().chain(palette_only()) {
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
        // A command is either in the menus or in the palette only, never both.
        for (menu, items) in model_with(&recents, &sites).into_iter().chain(super::palette_only()) {
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
            // Window: the open trees fill it (the system, on macOS).
            if menu.as_ref() == crate::t!("menu.window").as_ref() {
                continue;
            }
            assert!((3..=12).contains(&n), "{menu} has {n} items");
        }
    }

    /// The texts that name a menu command (the app's English texts, the user documentation) name
    /// one of the menu bar: a command only in the palette has a visible control, which they name
    /// instead.
    #[test]
    fn texts_name_commands_of_the_menu_bar() {
        fn walk(path: &str, items: Vec<Item>, out: &mut Vec<String>) {
            for i in items {
                let name = match &i {
                    Item::Action(n, _) | Item::Os(n, ..) | Item::Check(n, ..) | Item::Sub(n, _) => n.to_string(),
                    Item::Recent(..) | Item::Separator => continue,
                };
                let here = format!("{path} › {name}");
                if let Item::Sub(_, items) = i {
                    walk(&here, items, out);
                }
                out.push(here);
            }
        }
        let mut paths = Vec::new();
        let mut menus = Vec::new();
        for (menu, items) in model_with(&[PathBuf::from("/a/Martin.ged")], &["Geneanet".to_string()]) {
            walk(&menu, items, &mut paths);
            menus.push(menu.to_string());
        }
        // The labels of the other platforms (Quit GEDRoots on macOS, Quit elsewhere).
        paths.extend(["GEDRoots › Quit".to_string(), "GEDRoots › Quit GEDRoots".to_string()]);
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let mut texts: Vec<(String, String)> = Vec::new();
        for dir in ["crates/gedroots/locales", "crates/gedcom/locales", "crates/book/locales"] {
            for e in std::fs::read_dir(root.join(dir)).unwrap().flatten() {
                let text = std::fs::read_to_string(e.path()).unwrap();
                let english = text.lines().filter(|l| l.trim_start().starts_with("en:"));
                texts.extend(english.map(|l| (e.path().display().to_string(), l.to_string())));
            }
        }
        fn mdx(dir: &std::path::Path, out: &mut Vec<(String, String)>) {
            for e in std::fs::read_dir(dir).unwrap().flatten() {
                let p = e.path();
                if p.is_dir() {
                    mdx(&p, out);
                } else if p.extension().is_some_and(|x| x == "mdx" || x == "md") {
                    let text = std::fs::read_to_string(&p).unwrap();
                    out.extend(text.lines().map(|l| (p.display().to_string(), l.to_string())));
                }
            }
        }
        mdx(&root.join("website/docs"), &mut texts);
        assert!(texts.len() > 100, "texts not found");
        for (file, line) in texts {
            for menu in &menus {
                let start = format!("{menu} › ");
                for (at, _) in line.match_indices(&start) {
                    // Inside a word (“Tree › Free Tree” is not the Tree menu): not a path.
                    if line[..at].chars().next_back().is_some_and(|c| c.is_alphanumeric()) {
                        continue;
                    }
                    let rest = &line[at..];
                    // The text names the menu and its command: the longest command that starts it.
                    assert!(
                        paths.iter().any(|p| rest.starts_with(p.as_str())),
                        "{file}: “{}” is not a command of the menu bar",
                        rest.split("**").next().unwrap_or(rest)
                    );
                }
            }
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
