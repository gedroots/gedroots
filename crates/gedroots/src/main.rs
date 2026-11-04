//! GEDRoots, free and local genealogy software, with GEDCOM 7 as its data format.

// On Windows, no console window behind the app (except in debug, for the messages).
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod about;
mod act_editor;
mod agent;
mod commands;
mod completion;
mod date_field;
mod editor;
mod gazetteer;
mod gedzip;
mod i18n;
mod images;
mod map;
mod mobile;
mod recent;
mod report;
mod sample;
mod save_dialog;
mod theme;
mod updates;
mod viewer;
mod workspace;

use commands::{About, CheckForUpdates, Quit, ReleaseNotes, ToggleAutomaticUpdates};
use gpui_kit::*;
use std::borrow::Cow;
use std::path::PathBuf;
use workspace::Workspace;

// Interface strings: crates/gedroots/locales/*.yml (see i18n.rs).
rust_i18n::i18n!("locales", fallback = "en");

// Lucide icons used on top of gpui-kit's default set.
gpui_kit::assets::icon_assets!(
    ExtraIcons,
    [
        Users,
        Heart,
        BookOpen,
        MapPin,
        Globe,
        CornerDownRight,
        Locate,
        Image,
        Images,
        Landmark,
        StickyNote,
        FolderOpen,
        Undo2,
        Save,
        Plus,
        GitFork,
        Pencil,
        Code,
        Ellipsis,
        Copy,
        House,
        Star,
        Trash,
        Network,
        Library,
        Crop,
        UserPlus,
        Calendar,
        Upload,
        Link,
        AppWindow,
        Plug,
        Bot,
        Compass,
        Sprout,
        Import,
        UserSearch,
        Quote,
        Smartphone,
        Tablet,
        RotateCcwClock,
        Circle,
        CircleDashed,
        CircleDot,
        CircleSlash,
        CircleMinus,
    ]
);

struct AppAssets;

/// GEDRoots brand (SVG sources from the brand kit), displayed with `img("brand/…")`.
const BRAND: [(&str, &[u8]); 5] = [
    ("brand/logo-horizontal-light.svg", include_bytes!("../assets/brand/logo-horizontal-light.svg")),
    ("brand/logo-horizontal-dark.svg", include_bytes!("../assets/brand/logo-horizontal-dark.svg")),
    ("brand/icon-primary.svg", include_bytes!("../assets/brand/icon-primary.svg")),
    ("brand/icon-micro.svg", include_bytes!("../assets/brand/icon-micro.svg")),
    // The application icon (Dock, taskbar), for the About window.
    ("brand/app-icon.png", include_bytes!("../../../packaging/icon.png")),
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
    // First of all: Velopack may run the app for its install and update hooks, then exit.
    updates::startup();
    // Where the app crashed, for the next Help › Report an Issue.
    report::install_panic_hook();

    // On macOS, an app launched by the Finder can receive a `-psn_…` argument: ignore it.
    let path = std::env::args_os().skip(1).find(|a| !a.to_string_lossy().starts_with("-psn")).map(PathBuf::from);

    // Files opened from the Finder (double-click on a .ged, “Open With”, drop on the Dock icon):
    // macOS sends them to the app as `file://` URLs.
    let (opened_tx, opened_rx) = async_channel::unbounded::<PathBuf>();
    let app = gpui_kit::application().with_assets(AppAssets);
    app.on_open_urls(move |urls| {
        for path in urls.iter().filter_map(|u| file_url_path(u)) {
            let _ = opened_tx.try_send(path);
        }
    });
    // Click on the Dock icon while every window is closed: reopen one.
    app.on_reopen(|cx| {
        if cx.windows().is_empty() {
            open_workspace(None, cx);
        }
    });

    i18n::init();
    app.run(move |cx| {
        gpui_kit::init(cx);
        theme::install(cx);
        commands::bind_keys(cx);
        commands::install_menus(cx);
        agent::install(cx);
        mobile::install(cx);
        updates::install(cx);
        cx.on_action(|_: &Quit, cx| {
            updates::apply_on_quit();
            cx.quit()
        });
        cx.on_action(|_: &ReleaseNotes, cx| cx.open_url(&updates::release_notes_url()));
        cx.on_action(|_: &About, cx| about::open(cx));
        // Only declared for an installed copy: greyed out otherwise.
        if updates::available() {
            cx.on_action(|_: &CheckForUpdates, cx| updates::check_now(cx));
            cx.on_action(|_: &ToggleAutomaticUpdates, cx| updates::toggle_automatic(cx));
        }

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

/// Main window opened on `path` (or on the welcome screen).
/// Commands declared on the application rather than on a window (see `main`): they apply even
/// without a window.
pub(crate) fn is_global_action(action: &dyn Action) -> bool {
    let name = action.name();
    name == Quit.name()
        || name == ReleaseNotes.name()
        || name == About.name()
        || (updates::available() && (name == CheckForUpdates.name() || name == ToggleAutomaticUpdates.name()))
}

fn open_workspace(path: Option<PathBuf>, cx: &mut App) -> Option<(AnyWindowHandle, WeakEntity<Workspace>)> {
    let bounds = Bounds::centered(None, size(px(1440.), px(900.)), cx);
    let options = WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(bounds)),
        // Below this, the list and the record no longer fit side by side, even with the side
        // columns folded away.
        window_min_size: Some(size(px(720.), px(480.))),
        // macOS: title bar merged into the app's top bar, traffic lights included.
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
    let ws = ws.downgrade();
    cx.global_mut::<updates::Windows>().0.push(ws.clone());
    Some((window, ws))
}

/// Help › Open Sample Tree: the sample tree, in a window of its own.
pub(crate) fn open_sample(cx: &mut App) {
    open_sample_with_tour(Vec::new(), cx);
}

/// The sample tree in a window of its own, running the chapters of the guided tour that need it.
pub(crate) fn open_sample_with_tour(chapters: Vec<workspace::tour::Chapter>, cx: &mut App) {
    let Some((window, ws)) = open_workspace(None, cx) else { return };
    let _ = cx.update_window(window, |_, window, cx| {
        let _ = ws.update(cx, |ws, cx| {
            ws.open_sample(chapters.is_empty(), window, cx);
            ws.start_chapters(chapters, window, cx);
        });
    });
}

/// Opens a file received from the Finder: in the last window if it has no unsaved changes,
/// otherwise in a new window.
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

/// `file:///Users/a/G%C3%A9n%C3%A9alogie/tree.ged` → local path. Other URLs are ignored.
fn file_url_path(url: &str) -> Option<PathBuf> {
    let rest = url.strip_prefix("file://")?;
    // `file://localhost/…` is a valid form too.
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
            file_url_path("file:///Users/a/G%C3%A9n%C3%A9alogie%20Martin/arbre.ged"),
            Some(PathBuf::from("/Users/a/Généalogie Martin/arbre.ged"))
        );
        assert_eq!(file_url_path("file://localhost/tmp/a.ged"), Some(PathBuf::from("/tmp/a.ged")));
        assert_eq!(file_url_path("https://example.org/a.ged"), None);
    }
}
