//! GedRoots — logiciel de généalogie libre et local, avec GEDCOM 7 comme format de données.

mod completion;
mod editor;
mod theme;
mod workspace;

use gpui_kit::*;
use std::borrow::Cow;
use std::path::PathBuf;
use workspace::{NewFile, OpenFile, Quit, SaveFile, Undo, Workspace};

// Icônes Lucide utilisées en plus du jeu par défaut de gpui-kit.
gpui_kit::assets::icon_assets!(ExtraIcons, [Users, MapPin, Image, Save, Code, Pencil, Library, Landmark, StickyNote]);

struct AppAssets;

impl AssetSource for AppAssets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        if let Some(bytes) = ExtraIcons.load(path)? {
            return Ok(Some(bytes));
        }
        gpui_kit::assets::Assets.load(path)
    }

    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        let mut paths = gpui_kit::assets::Assets.list(path)?;
        paths.extend(ExtraIcons.list(path)?);
        paths.sort();
        paths.dedup();
        Ok(paths)
    }
}

/// Raccourci avec ⌘ sur macOS, Ctrl ailleurs.
macro_rules! shortcut {
    ($k:literal) => {
        if cfg!(target_os = "macos") { concat!("cmd-", $k) } else { concat!("ctrl-", $k) }
    };
}

fn main() {
    let path = std::env::args_os().nth(1).map(PathBuf::from);

    gpui_kit::application().with_assets(AppAssets).run(move |cx| {
        gpui_kit::init(cx);
        theme::install(cx);
        cx.bind_keys([
            KeyBinding::new(shortcut!("o"), OpenFile, None),
            KeyBinding::new(shortcut!("s"), SaveFile, None),
            KeyBinding::new(shortcut!("n"), NewFile, None),
            KeyBinding::new(shortcut!("shift-z"), Undo, Some("Workspace")),
            KeyBinding::new(shortcut!("q"), Quit, None),
            // Suggestions de l'éditeur GEDCOM brut à la demande.
            KeyBinding::new("ctrl-space", component::input::ShowCompletions, Some("Input")),
        ]);
        cx.on_action(|_: &Quit, cx| cx.quit());

        let bounds = Bounds::centered(None, size(px(1440.), px(900.)), cx);
        let options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            titlebar: Some(TitlebarOptions { title: Some("GedRoots".into()), ..Default::default() }),
            ..Default::default()
        };
        gpui_kit::open_window(options, cx, move |window, cx| {
            let ws = cx.new(|cx| Workspace::new(path.clone(), window, cx));
            let focus = ws.read(cx).focus_handle().clone();
            window.focus(&focus, cx);
            ws
        })
        .expect("impossible d'ouvrir la fenêtre");
        cx.activate(true);
    });
}
