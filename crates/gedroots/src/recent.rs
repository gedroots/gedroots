//! Fichiers récemment ouverts, pour Fichier › Ouvrir récent et la page d'accueil.
//!
//! La liste est propre à la machine, pas au fichier : elle ne peut donc pas vivre dans le .ged.
//! - macOS : la liste du système (`NSDocumentController`), la même que celle du Dock et du
//!   Finder ;
//! - Linux et Windows : un petit fichier texte (un chemin par ligne) dans le dossier de
//!   configuration de la plateforme (`~/.config/GEDRoots/recents`, `%APPDATA%\GEDRoots\recents`).

use gpui_kit::App;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

/// Nombre de fichiers gardés (comme le réglage par défaut de macOS).
const MAX: usize = 10;

/// Liste lue une fois puis gardée en mémoire ; `None` = à relire.
static CACHE: Mutex<Option<Vec<PathBuf>>> = Mutex::new(None);
/// Incrémenté à chaque changement, pour que chaque fenêtre recharge sa barre de menus.
static GENERATION: AtomicU64 = AtomicU64::new(0);

/// Fichiers récents existants, du plus récent au plus ancien.
pub fn list() -> Vec<PathBuf> {
    let mut cache = CACHE.lock().unwrap_or_else(|e| e.into_inner());
    cache.get_or_insert_with(|| store::read().into_iter().filter(|p| p.is_file()).take(MAX).collect()).clone()
}

/// Numéro de version de la liste.
pub fn generation() -> u64 {
    GENERATION.load(Ordering::Relaxed)
}

/// Note `path` comme le fichier le plus récent.
pub fn add(path: &Path, cx: &mut App) {
    let path = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    // macOS : liste du système ; ailleurs, sans effet (on écrit notre propre fichier).
    cx.add_recent_document(&path);
    #[cfg(not(target_os = "macos"))]
    {
        let mut paths = store::read();
        store::write(&push_front(&mut paths, path));
    }
    changed(cx);
}

/// Vide la liste.
pub fn clear(cx: &mut App) {
    store::clear();
    changed(cx);
}

fn changed(cx: &mut App) {
    *CACHE.lock().unwrap_or_else(|e| e.into_inner()) = None;
    GENERATION.fetch_add(1, Ordering::Relaxed);
    crate::commands::install_menus(cx);
}

/// Place `path` en tête, sans doublon, en gardant au plus `MAX` entrées.
#[cfg_attr(target_os = "macos", allow(dead_code))]
fn push_front(paths: &mut Vec<PathBuf>, path: PathBuf) -> Vec<PathBuf> {
    paths.retain(|p| p != &path);
    paths.insert(0, path);
    paths.truncate(MAX);
    paths.clone()
}

/// Libellés des entrées : le nom du fichier, suivi de son dossier quand deux fichiers portent le
/// même nom.
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

/// Dossier d'un fichier pour l'affichage, avec `~` pour le dossier personnel.
pub fn folder(path: &Path) -> String {
    let dir = path.parent().unwrap_or(path);
    if let Some(home) = std::env::var_os("HOME").map(PathBuf::from)
        && let Ok(rest) = dir.strip_prefix(&home)
    {
        return if rest.as_os_str().is_empty() { "~".into() } else { format!("~/{}", rest.display()) };
    }
    dir.display().to_string()
}

#[cfg(target_os = "macos")]
mod store {
    use objc2::MainThreadMarker;
    use objc2_app_kit::NSDocumentController;
    use std::path::PathBuf;

    /// Liste du système. Appelée depuis le fil principal (menus, rendu).
    pub fn read() -> Vec<PathBuf> {
        let Some(mtm) = MainThreadMarker::new() else { return Vec::new() };
        let dc = NSDocumentController::sharedDocumentController(mtm);
        dc.recentDocumentURLs().iter().filter_map(|u| u.to_file_path()).collect()
    }

    pub fn clear() {
        let Some(mtm) = MainThreadMarker::new() else { return };
        // SAFETY : `clearRecentDocuments:` accepte un émetteur nul.
        unsafe { NSDocumentController::sharedDocumentController(mtm).clearRecentDocuments(None) };
    }
}

#[cfg(not(target_os = "macos"))]
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
    fn same_names_show_their_folder() {
        let paths =
            [PathBuf::from("/x/Guyot/arbre.ged"), PathBuf::from("/x/Martin/arbre.ged"), PathBuf::from("/x/Guyot.ged")];
        assert_eq!(labels(&paths), ["arbre.ged — Guyot", "arbre.ged — Martin", "Guyot.ged"]);
    }
}
