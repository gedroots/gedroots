//! GEDRoots — logiciel de généalogie libre et local, avec GEDCOM 7 comme format de données.

// Sous Windows, pas de fenêtre de console derrière l'app (sauf en debug, pour les messages).
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod act_editor;
mod commands;
mod completion;
mod date_field;
mod editor;
mod images;
mod map;
mod recent;
mod theme;
mod workspace;

use commands::Quit;
use gpui_kit::*;
use std::borrow::Cow;
use std::path::PathBuf;
use workspace::Workspace;

// Icônes Lucide utilisées en plus du jeu par défaut de gpui-kit.
gpui_kit::assets::icon_assets!(
    ExtraIcons,
    [
        Users, Heart, BookOpen, MapPin, Image, Images, Landmark, StickyNote, FolderOpen, Undo2, Save, Plus, GitFork,
        Pencil, Code, Ellipsis, Copy, House, Star, Trash, Network, Library, Crop, UserPlus, Calendar,
    ]
);

struct AppAssets;

/// Marque GEDRoots (sources SVG du kit de marque), affichée avec `img("brand/…")`.
const BRAND: [(&str, &[u8]); 4] = [
    ("brand/logo-horizontal-light.svg", include_bytes!("../assets/brand/logo-horizontal-light.svg")),
    ("brand/logo-horizontal-dark.svg", include_bytes!("../assets/brand/logo-horizontal-dark.svg")),
    ("brand/icon-primary.svg", include_bytes!("../assets/brand/icon-primary.svg")),
    ("brand/icon-micro.svg", include_bytes!("../assets/brand/icon-micro.svg")),
];

impl AssetSource for AppAssets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        if let Some((_, bytes)) = BRAND.iter().find(|(p, _)| *p == path) {
            return Ok(Some(Cow::Borrowed(*bytes)));
        }
        if let Some(bytes) = ExtraIcons.load(path)? {
            return Ok(Some(bytes));
        }
        gpui_kit::assets::Assets.load(path)
    }

    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        let mut paths = gpui_kit::assets::Assets.list(path)?;
        paths.extend(ExtraIcons.list(path)?);
        paths.extend(BRAND.iter().filter(|(p, _)| p.starts_with(path)).map(|(p, _)| SharedString::from(*p)));
        paths.sort();
        paths.dedup();
        Ok(paths)
    }
}

fn main() {
    // Sous macOS, une app lancée par le Finder peut recevoir un argument `-psn_…` : on l'ignore.
    let path = std::env::args_os().skip(1).find(|a| !a.to_string_lossy().starts_with("-psn")).map(PathBuf::from);

    // Fichiers ouverts depuis le Finder (double-clic sur un .ged, « Ouvrir avec », glisser sur
    // l'icône du Dock) : macOS les envoie à l'app sous forme d'URL `file://`.
    let (opened_tx, opened_rx) = async_channel::unbounded::<PathBuf>();
    let app = gpui_kit::application().with_assets(AppAssets);
    app.on_open_urls(move |urls| {
        for path in urls.iter().filter_map(|u| file_url_path(u)) {
            let _ = opened_tx.try_send(path);
        }
    });
    // Clic sur l'icône du Dock alors que toutes les fenêtres sont fermées : on en rouvre une.
    app.on_reopen(|cx| {
        if cx.windows().is_empty() {
            open_workspace(None, cx);
        }
    });

    app.run(move |cx| {
        gpui_kit::init(cx);
        theme::install(cx);
        commands::bind_keys(cx);
        commands::install_menus(cx);
        cx.on_action(|_: &Quit, cx| cx.quit());

        let main = open_workspace(path, cx);
        cx.activate(true);

        cx.spawn(async move |cx| {
            let mut last = main;
            while let Ok(path) = opened_rx.recv().await {
                last = cx.update(|cx| open_in(last, path, cx));
            }
        })
        .detach();
    });
}

/// Fenêtre principale ouverte sur `path` (ou sur l'écran d'accueil).
fn open_workspace(path: Option<PathBuf>, cx: &mut App) -> Option<(AnyWindowHandle, WeakEntity<Workspace>)> {
    let bounds = Bounds::centered(None, size(px(1440.), px(900.)), cx);
    let options = WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(bounds)),
        // macOS : barre de titre fondue dans la barre du haut de l'app, feux tricolores compris.
        titlebar: Some(TitlebarOptions {
            title: Some("GEDRoots".into()),
            appears_transparent: cfg!(target_os = "macos"),
            traffic_light_position: cfg!(target_os = "macos").then(|| point(px(14.), px(13.))),
        }),
        ..Default::default()
    };
    let (window, ws) = gpui_kit::open_window(options, cx, move |window, cx| {
        let ws = cx.new(|cx| Workspace::new(path, window, cx));
        let focus = ws.read(cx).focus_handle().clone();
        window.focus(&focus, cx);
        ws
    })
    .ok()?;
    Some((window, ws.downgrade()))
}

/// Ouvre un fichier reçu du Finder : dans la dernière fenêtre si elle n'a pas de modifications
/// non enregistrées, sinon dans une nouvelle fenêtre.
fn open_in(
    last: Option<(AnyWindowHandle, WeakEntity<Workspace>)>,
    path: PathBuf,
    cx: &mut App,
) -> Option<(AnyWindowHandle, WeakEntity<Workspace>)> {
    if let Some((window, ws)) = last
        && let Some(ws_entity) = ws.upgrade()
        && !ws_entity.read(cx).has_unsaved_changes()
    {
        let loaded = cx.update_window(window, |_, window, cx| {
            ws_entity.update(cx, |ws, cx| ws.open_path(path.clone(), window, cx));
            window.activate_window();
        });
        if loaded.is_ok() {
            cx.activate(true);
            return Some((window, ws));
        }
    }
    let opened = open_workspace(Some(path), cx);
    cx.activate(true);
    opened
}

/// `file:///Users/a/G%C3%A9n%C3%A9alogie/arbre.ged` → chemin local. Les autres URL sont ignorées.
fn file_url_path(url: &str) -> Option<PathBuf> {
    let rest = url.strip_prefix("file://")?;
    // `file://localhost/…` est aussi une forme valide.
    let rest = rest.strip_prefix("localhost").unwrap_or(rest);
    let bytes = rest.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && i + 2 < bytes.len()
            && let Ok(b) = u8::from_str_radix(std::str::from_utf8(&bytes[i + 1..i + 3]).ok()?, 16)
        {
            out.push(b);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    Some(PathBuf::from(String::from_utf8(out).ok()?))
}

#[cfg(test)]
mod tests {
    use super::file_url_path;
    use std::path::PathBuf;

    #[test]
    fn file_urls() {
        assert_eq!(
            file_url_path("file:///Users/a/G%C3%A9n%C3%A9alogie%20Guyot/arbre.ged"),
            Some(PathBuf::from("/Users/a/Généalogie Guyot/arbre.ged"))
        );
        assert_eq!(file_url_path("file://localhost/tmp/a.ged"), Some(PathBuf::from("/tmp/a.ged")));
        assert_eq!(file_url_path("https://example.org/a.ged"), None);
    }
}
