//! Recently opened files, for File › Open Recent and the welcome page.
//!
//! The list belongs to the machine, not to the file: so it cannot live in the .ged. It is a small
//! text file (one path per line) in the platform's configuration folder
//! (`~/Library/Application Support/GEDRoots/recents`, `~/.config/GEDRoots/recents`,
//! `%APPDATA%\GEDRoots\recents`), which updates do not touch. On macOS each file is also given
//! to the system's list (`NSDocumentController`), for the Dock and the Finder; that list is not
//! read back, because macOS can reset it when an update changes the app's code signature.

use gpui_kit::App;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

/// Number of files kept (like macOS' default setting).
const MAX: usize = 10;

/// List read once, then kept in memory; `None` = to read again.
static CACHE: Mutex<Option<Vec<PathBuf>>> = Mutex::new(None);
/// Incremented on each change, so that each window reloads its menu bar.
static GENERATION: AtomicU64 = AtomicU64::new(0);

/// Existing recent files, most recent first.
pub fn list() -> Vec<PathBuf> {
    let mut cache = CACHE.lock().unwrap_or_else(|e| e.into_inner());
    cache.get_or_insert_with(|| store::read().into_iter().filter(|p| !is_gone(p)).take(MAX).collect()).clone()
}

/// The file no longer exists. A file that cannot be read for now (folder that macOS has not yet
/// authorized) stays in the list: opening it asks for the authorization again.
fn is_gone(path: &Path) -> bool {
    match std::fs::metadata(path) {
        Ok(m) => !m.is_file(),
        Err(e) => e.kind() == std::io::ErrorKind::NotFound,
    }
}

/// Version number of the list.
pub fn generation() -> u64 {
    GENERATION.load(Ordering::Relaxed)
}

/// Records `path` as the most recent file.
pub fn add(path: &Path, cx: &mut App) {
    let path = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    // macOS: also the system's list (Dock, Finder); elsewhere, no effect.
    cx.add_recent_document(&path);
    let mut paths = store::read();
    store::write(&push_front(&mut paths, path));
    changed(cx);
}

/// Clears the list.
pub fn clear(cx: &mut App) {
    store::clear();
    #[cfg(target_os = "macos")]
    clear_system_list();
    changed(cx);
}

fn changed(cx: &mut App) {
    *CACHE.lock().unwrap_or_else(|e| e.into_inner()) = None;
    GENERATION.fetch_add(1, Ordering::Relaxed);
    crate::commands::install_menus(cx);
}

/// Puts `path` first, without duplicates, keeping at most `MAX` entries.
fn push_front(paths: &mut Vec<PathBuf>, path: PathBuf) -> Vec<PathBuf> {
    paths.retain(|p| p != &path);
    paths.insert(0, path);
    paths.truncate(MAX);
    paths.clone()
}

/// Labels of the entries: the file name, followed by its folder when two files have the same name.
pub fn labels(paths: &[PathBuf]) -> Vec<String> {
    let name = |p: &PathBuf| p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    paths
        .iter()
        .map(|p| {
            let n = name(p);
            if paths.iter().filter(|o| name(o) == n).count() > 1
                && let Some(dir) = p.parent().and_then(|d| d.file_name())
            {
                format!("{n} — {}", dir.to_string_lossy())
            } else {
                n
            }
        })
        .collect()
}

/// Folder of a file for display, with `~` for the home folder.
pub fn folder(path: &Path) -> String {
    let dir = path.parent().unwrap_or(path);
    if let Some(home) = std::env::var_os("HOME").map(PathBuf::from)
        && let Ok(rest) = dir.strip_prefix(&home)
    {
        return if rest.as_os_str().is_empty() { "~".into() } else { format!("~/{}", rest.display()) };
    }
    dir.display().to_string()
}

/// Clears the system's list (Dock, Finder). Called from the main thread (menu command).
#[cfg(target_os = "macos")]
fn clear_system_list() {
    use objc2::MainThreadMarker;
    use objc2_app_kit::NSDocumentController;
    let Some(mtm) = MainThreadMarker::new() else { return };
    // SAFETY: `clearRecentDocuments:` accepts a null sender.
    unsafe { NSDocumentController::sharedDocumentController(mtm).clearRecentDocuments(None) };
}

mod store {
    use std::path::PathBuf;

    fn file() -> Option<PathBuf> {
        Some(dirs::config_dir()?.join("GEDRoots").join("recents"))
    }

    pub fn read() -> Vec<PathBuf> {
        let Some(text) = file().and_then(|f| std::fs::read_to_string(f).ok()) else { return Vec::new() };
        text.lines().filter(|l| !l.is_empty()).map(PathBuf::from).collect()
    }

    pub fn write(paths: &[PathBuf]) {
        let Some(f) = file() else { return };
        if let Some(dir) = f.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let text: String = paths.iter().map(|p| format!("{}\n", p.display())).collect();
        let _ = std::fs::write(f, text);
    }

    pub fn clear() {
        if let Some(f) = file() {
            let _ = std::fs::remove_file(f);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn most_recent_first_without_duplicates() {
        let mut v: Vec<PathBuf> = (0..MAX).map(|i| PathBuf::from(format!("/a/{i}.ged"))).collect();
        let v = push_front(&mut v, PathBuf::from("/a/3.ged"));
        assert_eq!(v[0], PathBuf::from("/a/3.ged"));
        assert_eq!(v.len(), MAX);
        assert_eq!(v.iter().filter(|p| p.ends_with("3.ged")).count(), 1);
        let mut v = v;
        let v = push_front(&mut v, PathBuf::from("/b/new.ged"));
        assert_eq!(v.len(), MAX);
        assert!(!v.contains(&PathBuf::from("/a/9.ged")));
    }

    #[test]
    fn only_missing_files_leave_the_list() {
        let dir = std::env::temp_dir().join(format!("gedroots-recent-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("tree.ged");
        std::fs::write(&file, "0 HEAD\n").unwrap();
        assert!(!is_gone(&file));
        assert!(is_gone(&dir));
        assert!(is_gone(&dir.join("missing.ged")));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn same_names_show_their_folder() {
        let paths =
            [PathBuf::from("/x/Guyot/arbre.ged"), PathBuf::from("/x/Martin/arbre.ged"), PathBuf::from("/x/Guyot.ged")];
        assert_eq!(labels(&paths), ["arbre.ged — Guyot", "arbre.ged — Martin", "Guyot.ged"]);
    }
}
