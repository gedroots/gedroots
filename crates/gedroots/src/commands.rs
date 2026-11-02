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
        ExportGedzip,
        ExportGedcom551,
        ExportGeneanet,
        ConnectAgent,
        ReviewProposals,
        AcceptProposal,
        RejectProposal,
        Quit,
        // Edit
        Undo,
        Redo,
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
        TreeChart,
        TreeFan,
        CenterTree,
        MoreGenerations,
        FewerGenerations,
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
    ]
);

/// Opens a file from File › Open Recent.
#[derive(Clone, PartialEq, Action)]
#[action(namespace = gedroots, no_json)]
pub struct OpenRecent {
    pub path: std::path::PathBuf,
}

/// Ouvre la recherche de la personne sélectionnée sur un site d'Individu › Rechercher sur.
#[derive(Clone, PartialEq, Action)]
#[action(namespace = gedroots, no_json)]
pub struct SearchWeb {
    /// Position du site dans les réglages du fichier.
    pub index: usize,
}

/// Noms des sites de recherche du fichier affiché (Réglages du fichier › Recherche).
static SEARCH_SITES: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());

/// Note les sites de recherche du fichier affiché. Renvoie `true` s'ils ont
/// changé : les menus sont alors à réinstaller.
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

/// Numéro de version des sites de recherche, pour que chaque fenêtre recharge sa barre de menus.
pub fn search_sites_generation() -> u64 {
    SEARCH_SITES_GEN.load(std::sync::atomic::Ordering::Relaxed)
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
        KeyBinding::new(shortcut!("alt-1"), ShowTree, WS),
        KeyBinding::new(shortcut!("alt-2"), ShowForm, WS),
        KeyBinding::new(shortcut!("alt-3"), ShowRaw, WS),
        KeyBinding::new(shortcut!("="), MoreGenerations, WS),
        KeyBinding::new(shortcut!("-"), FewerGenerations, WS),
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
    Sub(SharedString, Vec<Item>),
    Separator,
}

impl Item {
    /// Label and action of a command (not a submenu, a separator or a text editing action).
    fn into_command(self) -> Option<(String, Box<dyn Action>)> {
        match self {
            Item::Action(name, action) => Some((name.to_string(), action)),
            Item::Recent(name, action) => Some((name, action)),
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
    if mac {
        // On macOS, settings go where they usually are: the application menu, ⌘,.
        menus.push((
            "GEDRoots".into(),
            vec![a(t!("cmd.file_settings"), OpenFileSettings), Sep, a(t!("cmd.quit_app"), Quit)],
        ));
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
        Sub(
            t!("menu.export").into(),
            vec![
                a(t!("cmd.export_gedzip"), ExportGedzip),
                a(t!("cmd.export_gedcom551"), ExportGedcom551),
                a(t!("cmd.export_geneanet"), ExportGeneanet),
            ],
        ),
        Sep,
        a(t!("cmd.connect_agent"), ConnectAgent),
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
            a(label::media_gallery(), MediaGallery),
            a(t!("cmd.add_web_link"), AddWebLink),
            Sep,
            a(label::copy_name(), CopyName),
            a(label::copy_id(), CopyId),
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
            a(label::go_to_root(), GoToRoot),
            a(t!("cmd.kinship_with"), KinshipWith),
            a(t!("cmd.search_ancestors"), SearchAncestors),
            a(t!("cmd.search_descendants"), SearchDescendants),
        ],
    ));
    // Sites de recherche du fichier ; sous-menu masqué s'il n'y en a aucun.
    if !sites.is_empty() {
        let items =
            sites.iter().enumerate().map(|(index, name)| Item::Recent(name.clone(), Box::new(SearchWeb { index })));
        if let Some((_, indi)) = menus.last_mut() {
            indi.push(Sub(label::search_web(), items.collect()));
        }
    }
    // Repositories and shared notes are not in the sidebar: they are created from the Source menu.
    menus.push((
        t!("menu.source").into(),
        vec![
            a(label::enter_act(), EnterRecord),
            Sep,
            a(t!("cmd.review_proposals"), ReviewProposals),
            a(t!("cmd.accept_proposal"), AcceptProposal),
            a(t!("cmd.reject_proposal"), RejectProposal),
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
            Sep,
            a(t!("cmd.unreferenced_files"), ShowUnreferencedFiles),
        ],
    ));
    menus.push((
        t!("menu.view").into(),
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
            a(t!("cmd.stats"), ShowStats),
            Sep,
            a(t!("cmd.show_tree"), ShowTree),
            a(t!("cmd.show_form"), ShowForm),
            a(t!("cmd.show_raw"), ShowRaw),
            Sep,
            a(t!("cmd.toggle_sidebar"), ToggleSidebar),
            a(t!("cmd.relative_events"), ToggleRelativeEvents),
            Sep,
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
    menus.push((t!("menu.help").into(), vec![a(t!("cmd.all_commands"), CommandPalette)]));
    menus
}

fn to_menu_items(items: Vec<Item>) -> Vec<MenuItem> {
    items
        .into_iter()
        .map(|i| match i {
            Item::Action(name, action) => {
                MenuItem::Action { name, action, os_action: None, checked: false, disabled: false }
            }
            Item::Recent(name, action) => {
                MenuItem::Action { name: name.into(), action, os_action: None, checked: false, disabled: false }
            }
            Item::Os(name, action, os) => {
                MenuItem::Action { name, action, os_action: Some(os), checked: false, disabled: false }
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
                i @ (Item::Action(..) | Item::Recent(..)) => {
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
                    i @ (Item::Action(..) | Item::Recent(..)) => {
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
        let recents = [PathBuf::from("/a/Guyot.ged"), PathBuf::from("/b/Guyot.ged")];
        let sites = ["Geneanet".to_string(), "FamilySearch".to_string()];
        for (menu, items) in model_with(&recents, &sites) {
            walk(&menu, items, &mut seen);
        }
    }

    #[test]
    fn palette_has_every_command_once() {
        super::set_search_sites(vec!["Geneanet".into()]);
        let entries = palette_entries();
        assert!(entries.iter().any(|e| e.path == "Person › Search On › Geneanet"));
        assert!(entries.iter().any(|e| e.path == "Person › Add Relative › Father"));
        assert!(entries.iter().any(|e| e.path == "Source › Enter Record"));
        assert!(entries.iter().any(|e| e.path == "File › Export › GEDCOM for Geneanet"));
        for (i, e) in entries.iter().enumerate() {
            assert!(
                !entries[i + 1..].iter().any(|o| o.action.partial_eq(e.action.as_ref())),
                "{} appears twice",
                e.path
            );
        }
    }
}
