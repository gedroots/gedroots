//! GedRoots — logiciel de généalogie libre et local, avec GEDCOM 7 comme format de données.

mod analyses;
mod commands;
mod completion;
mod editor;
mod entry_form;
mod map;
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
        Pencil, Code, Ellipsis, Copy, House, Star, Trash, Network, Library, Crop, UserPlus,
    ]
);

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

fn main() {
    let path = std::env::args_os().nth(1).map(PathBuf::from);

    gpui_kit::application().with_assets(AppAssets).run(move |cx| {
        gpui_kit::init(cx);
        theme::install(cx);
        commands::bind_keys(cx);
        commands::install_menus(cx);
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
