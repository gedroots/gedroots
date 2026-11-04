//! Main windows: one per tree, like the documents of a document-based app.
//!
//! - File › Open… (and Open Recent, Import, the Finder) shows the tree in the window it was asked
//!   from while that window is empty (welcome screen), otherwise in a new window; a tree that is
//!   already open brings its window to the front.
//! - File › New Window (⌥⌘N; ⌘N is Create) opens a window on the welcome screen.
//! - The Window menu lists the open trees: on macOS the system fills it (`NSApp.windowsMenu`),
//!   with its native tabs (Window › Merge All Windows); elsewhere the in-window menu bar shows it.

use crate::workspace::Workspace;
use gpui_kit::*;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// Tabs of the same app group together on macOS (System Settings › Prefer tabs, or Window ›
/// Merge All Windows).
pub const TABBING_ID: &str = "GEDRoots";

/// An open main window.
#[derive(Clone)]
struct Entry {
    window: AnyWindowHandle,
    ws: WeakEntity<Workspace>,
    id: EntityId,
}

/// The open main windows, in the order they were opened.
#[derive(Default)]
pub struct Registry(Vec<Entry>);

impl Global for Registry {}

impl Registry {
    /// The workspace of `window`.
    pub fn workspace(&self, window: AnyWindowHandle) -> Option<WeakEntity<Workspace>> {
        self.0.iter().find(|e| e.window == window).map(|e| e.ws.clone())
    }
}

/// What the menus show of each window: its title, and whether it is the front one.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Listed {
    pub titles: Vec<String>,
    pub active: Option<usize>,
}

/// What each window showed when last drawn, by workspace: its title and the tree's file.
struct Shown {
    id: EntityId,
    title: String,
    path: Option<PathBuf>,
}

static TITLES: Mutex<Vec<Shown>> = Mutex::new(Vec::new());
static ACTIVE: Mutex<Option<EntityId>> = Mutex::new(None);
static LISTED: Mutex<Option<Listed>> = Mutex::new(None);

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

pub fn install(cx: &mut App) {
    cx.set_global(Registry::default());
    cx.on_action(|_: &crate::commands::NewWindow, cx| {
        open(None, cx);
    });
    cx.on_action(|a: &crate::commands::ActivateWindowAt, cx| activate_at(a.index, cx));
}

/// Remembers a new main window, and forgets it once closed.
pub fn register(window: AnyWindowHandle, ws: &Entity<Workspace>, cx: &mut App) {
    let id = ws.entity_id();
    cx.global_mut::<Registry>().0.push(Entry { window, ws: ws.downgrade(), id });
    cx.observe_release(ws, move |_, cx| {
        cx.global_mut::<Registry>().0.retain(|e| e.id != id);
        lock(&TITLES).retain(|s| s.id != id);
        refresh(cx);
    })
    .detach();
}

/// A window's title or tree changed (or it was drawn for the first time).
pub fn set_shown(id: EntityId, title: &str, path: Option<&Path>, cx: &mut App) {
    {
        let mut titles = lock(&TITLES);
        let path = path.map(canonical);
        match titles.iter_mut().find(|s| s.id == id) {
            Some(s) => {
                s.title = title.to_string();
                s.path = path;
            }
            None => titles.push(Shown { id, title: title.to_string(), path }),
        }
    }
    refresh(cx);
}

/// A window came to the front.
pub fn set_active(id: EntityId, cx: &mut App) {
    *lock(&ACTIVE) = Some(id);
    refresh(cx);
}

/// What the Window menu lists now.
pub fn listed() -> Listed {
    lock(&LISTED).clone().unwrap_or_default()
}

/// Installs the menus again when the list of windows changed.
fn refresh(cx: &mut App) {
    let order: Vec<EntityId> = cx.global::<Registry>().0.iter().map(|e| e.id).collect();
    let titles = lock(&TITLES);
    let active = *lock(&ACTIVE);
    let shown: Vec<EntityId> = order.into_iter().filter(|id| titles.iter().any(|s| s.id == *id)).collect();
    let listed = Listed {
        titles: shown.iter().filter_map(|id| titles.iter().find(|s| s.id == *id).map(|s| s.title.clone())).collect(),
        active: active.and_then(|a| shown.iter().position(|id| *id == a)),
    };
    drop(titles);
    let changed = {
        let mut current = lock(&LISTED);
        let changed = current.as_ref() != Some(&listed);
        *current = Some(listed);
        changed
    };
    // The menu bar drawn in the window lists the trees; macOS lists the windows by itself.
    if changed && !cfg!(target_os = "macos") {
        crate::commands::install_menus(cx);
        cx.refresh_windows();
    }
}

/// Opens a main window on `path`, or on the welcome screen.
pub fn open(path: Option<PathBuf>, cx: &mut App) -> Option<(AnyWindowHandle, WeakEntity<Workspace>)> {
    crate::open_workspace(path, cx)
}

/// Opens a main window and runs `f` in it once it exists (to start a new tree, an import…).
pub fn open_then(cx: &mut App, f: impl FnOnce(&mut Workspace, &mut Window, &mut Context<Workspace>) + 'static) {
    let Some((window, ws)) = open(None, cx) else { return };
    let _ = cx.update_window(window, |_, window, cx| {
        let _ = ws.update(cx, |ws, cx| f(ws, window, cx));
    });
}

/// The window that shows the tree at `path`, if one does.
pub fn showing(path: &Path, cx: &App) -> Option<AnyWindowHandle> {
    let key = canonical(path);
    let id = lock(&TITLES).iter().find(|s| s.path.as_ref() == Some(&key)).map(|s| s.id)?;
    cx.global::<Registry>().0.iter().find(|e| e.id == id).map(|e| e.window)
}

/// Brings `window` to the front.
pub fn bring_to_front(window: AnyWindowHandle, cx: &mut App) {
    let _ = cx.update_window(window, |_, window, _| window.activate_window());
    cx.activate(true);
}

fn activate_at(index: usize, cx: &mut App) {
    let titles = lock(&TITLES).iter().map(|s| s.id).collect::<Vec<_>>();
    let shown: Vec<Entry> = cx.global::<Registry>().0.iter().filter(|e| titles.contains(&e.id)).cloned().collect();
    if let Some(e) = shown.get(index) {
        bring_to_front(e.window, cx);
    }
}

/// Every open workspace.
pub fn workspaces(cx: &App) -> Vec<WeakEntity<Workspace>> {
    cx.global::<Registry>().0.iter().map(|e| e.ws.clone()).collect()
}

fn canonical(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

/// macOS: makes the Window menu the system's windows menu, whatever its translated title (GPUI
/// only does so for a menu titled “Window”), so that macOS lists the windows and their tabs in it.
#[cfg(target_os = "macos")]
pub fn adopt_windows_menu(title: &str) {
    use objc2::MainThreadMarker;
    use objc2_app_kit::NSApplication;
    use objc2_foundation::NSString;
    let Some(mtm) = MainThreadMarker::new() else { return };
    let app = NSApplication::sharedApplication(mtm);
    let Some(main) = app.mainMenu() else { return };
    let item = main.itemWithTitle(&NSString::from_str(title));
    if let Some(menu) = item.and_then(|i| i.submenu()) {
        app.setWindowsMenu(Some(&menu));
    }
}
