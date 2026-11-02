//! Commandes de l'application : actions, raccourcis et menus.
//!
//! Chaque commande est décrite une seule fois ici, avec son libellé et son
//! raccourci. Le même modèle alimente :
//! - la barre de menus native sous macOS (`cx.set_menus`) ;
//! - la barre de menus dessinée dans la fenêtre sous Linux et Windows ;
//! - la palette de commandes (⌘⇧P / Ctrl+Maj+P).

use gpui_kit::component::input;
use gpui_kit::*;

actions!(
    gedroots,
    [
        // Fichier
        NewFile,
        OpenFile,
        ClearRecent,
        ImportFile,
        OpenFileSettings,
        SaveFile,
        SaveFileAs,
        ExportGedzip,
        Quit,
        // Édition
        Undo,
        DeleteRecord,
        CopyId,
        CopyName,
        FocusSearch,
        ToggleFlagged,
        AddWebLink,
        // Individu
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
        SetRoot,
        GoToRoot,
        AttachFiles,
        LinkMedia,
        CropMedia,
        AddEvent,
        // Source
        NewSource,
        NewRepository,
        NewNote,
        EnterRecord,
        // Média
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
        // Affichage
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
        ToggleSearchHelp,
        // Aide
        CommandPalette,
    ]
);

/// Ouvre un fichier de Fichier › Ouvrir récent.
#[derive(Clone, PartialEq, Action)]
#[action(namespace = gedroots, no_json)]
pub struct OpenRecent {
    pub path: std::path::PathBuf,
}

/// Raccourci avec ⌘ sur macOS, Ctrl ailleurs.
macro_rules! shortcut {
    ($k:literal) => {
        if cfg!(target_os = "macos") { concat!("cmd-", $k) } else { concat!("ctrl-", $k) }
    };
}

/// Contexte clavier de la fenêtre principale.
const WS: Option<&str> = Some("Workspace");

pub fn bind_keys(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new(shortcut!("n"), NewFile, None),
        KeyBinding::new(shortcut!("o"), OpenFile, None),
        KeyBinding::new(shortcut!("s"), SaveFile, None),
        KeyBinding::new(shortcut!("shift-s"), SaveFileAs, WS),
        KeyBinding::new(shortcut!("q"), Quit, None),
        KeyBinding::new(shortcut!("shift-z"), Undo, WS),
        KeyBinding::new(shortcut!("shift-n"), NewIndividual, WS),
        KeyBinding::new(shortcut!("f"), FocusSearch, WS),
        KeyBinding::new(shortcut!("e"), EnterRecord, WS),
        KeyBinding::new(shortcut!("shift-h"), GoToRoot, WS),
        KeyBinding::new(shortcut!("shift-p"), CommandPalette, WS),
        KeyBinding::new(shortcut!(","), OpenFileSettings, WS),
        KeyBinding::new(shortcut!("["), GoBack, WS),
        KeyBinding::new(shortcut!("]"), GoForward, WS),
        // Les quatre zones de la barre latérale, puis les listes et pages du menu Affichage.
        KeyBinding::new(shortcut!("1"), ShowIndividuals, WS),
        KeyBinding::new(shortcut!("2"), ShowSources, WS),
        KeyBinding::new(shortcut!("3"), ShowPlaces, WS),
        KeyBinding::new(shortcut!("4"), ShowMedia, WS),
        KeyBinding::new(shortcut!("5"), ShowRepositories, WS),
        KeyBinding::new(shortcut!("6"), ShowNotes, WS),
        KeyBinding::new(shortcut!("7"), ShowQuality, WS),
        KeyBinding::new(shortcut!("8"), ShowStats, WS),
        // Comme le Finder et Mail : ⌃⌘S (Ctrl+Alt+S ailleurs).
        KeyBinding::new(if cfg!(target_os = "macos") { "ctrl-cmd-s" } else { "ctrl-alt-s" }, ToggleSidebar, WS),
        KeyBinding::new(shortcut!("alt-1"), ShowTree, WS),
        KeyBinding::new(shortcut!("alt-2"), ShowForm, WS),
        KeyBinding::new(shortcut!("alt-3"), ShowRaw, WS),
        KeyBinding::new(shortcut!("="), MoreGenerations, WS),
        KeyBinding::new(shortcut!("-"), FewerGenerations, WS),
        KeyBinding::new("pageup", PreviousPage, WS),
        KeyBinding::new("pagedown", NextPage, WS),
        // Suggestions de l'éditeur GEDCOM brut à la demande.
        KeyBinding::new("ctrl-space", input::ShowCompletions, Some("Input")),
    ]);
}

/// Libellés partagés entre la barre de menus et les menus contextuels (clic
/// droit) : une commande porte le même nom partout où elle apparaît.
pub mod label {
    pub const NEW_INDIVIDUAL: &str = "Nouvel individu";
    pub const NEW_SOURCE: &str = "Nouvelle source";
    pub const NEW_MEDIA: &str = "Nouveau média";
    pub const NEW_REPOSITORY: &str = "Nouveau dépôt";
    pub const NEW_NOTE: &str = "Nouvelle note";
    pub const ATTACH_FILES: &str = "Rattacher des fichiers…";
    pub const COPY_ID: &str = "Copier l'ID";
    pub const COPY_NAME: &str = "Copier le nom";
    pub const DELETE: &str = "Supprimer l'élément…";
    pub const ADD_RELATIVE: &str = "Ajouter un proche";
    pub const LINK_PERSON: &str = "Lier une personne existante";
    pub const FATHER: &str = "Père";
    pub const MOTHER: &str = "Mère";
    pub const SPOUSE: &str = "Conjoint";
    pub const CHILD: &str = "Enfant";
    pub const SIBLING: &str = "Frère ou sœur";
    pub const FATHER_LINK: &str = "Père…";
    pub const MOTHER_LINK: &str = "Mère…";
    pub const SPOUSE_LINK: &str = "Conjoint…";
    pub const CHILD_LINK: &str = "Enfant…";
    pub const SIBLING_LINK: &str = "Frère ou sœur…";
    pub const ADD_EVENT: &str = "Ajouter un événement…";
    pub const SET_ROOT: &str = "Définir comme individu racine";
    pub const GO_TO_ROOT: &str = "Aller à l'individu racine";
    pub const CENTER_TREE: &str = "Centrer l'arbre sur la personne";
    pub const ENTER_ACT: &str = "Saisir un acte…";
    pub const ATTACH_MEDIA_TO: &str = "Rattacher ce média à…";
    pub const TAG_PERSON: &str = "Identifier une personne sur la photo";
    pub const MERGE_WITH_MEDIA: &str = "Fusionner avec un autre média…";
    pub const MERGE_SELECTED_MEDIA: &str = "Fusionner les médias sélectionnés";
    pub const ADD_PAGES: &str = "Ajouter des pages…";
    pub const CLEAR_SEARCH: &str = "Effacer la recherche";
}

/// Une entrée de menu.
enum Item {
    Action(&'static str, Box<dyn Action>),
    /// Fichier récent : libellé calculé.
    Recent(String, Box<dyn Action>),
    /// Action standard du système (copier, coller…) : macOS la relie aux champs de saisie.
    Os(&'static str, Box<dyn Action>, OsAction),
    Sub(&'static str, Vec<Item>),
    Separator,
}

impl Item {
    /// Libellé et action d'une commande (ni sous-menu, ni séparateur, ni action d'édition).
    fn into_command(self) -> Option<(String, Box<dyn Action>)> {
        match self {
            Item::Action(name, action) => Some((name.to_string(), action)),
            Item::Recent(name, action) => Some((name, action)),
            Item::Os(..) | Item::Sub(..) | Item::Separator => None,
        }
    }
}

fn a(label: &'static str, action: impl Action) -> Item {
    Item::Action(label, Box::new(action))
}

/// Le modèle de menus, commun à toutes les plateformes.
fn model() -> Vec<(&'static str, Vec<Item>)> {
    model_with(&crate::recent::list())
}

/// Le modèle de menus, avec les fichiers récents donnés.
fn model_with(recents: &[std::path::PathBuf]) -> Vec<(&'static str, Vec<Item>)> {
    use Item::{Separator as Sep, Sub};
    let mac = cfg!(target_os = "macos");
    let mut menus = Vec::new();
    if mac {
        // Sur macOS, les réglages vont à leur place habituelle : menu de l'application, ⌘,.
        menus.push(("GEDRoots", vec![a("Réglages du fichier…", OpenFileSettings), Sep, a("Quitter GEDRoots", Quit)]));
    }
    let mut file = vec![a("Nouveau fichier", NewFile), a("Ouvrir…", OpenFile)];
    // Sous-menu masqué tant qu'aucun fichier n'a été ouvert.
    if !recents.is_empty() {
        let names = crate::recent::labels(recents);
        let mut items: Vec<Item> =
            recents.iter().zip(names).map(|(p, n)| Item::Recent(n, Box::new(OpenRecent { path: p.clone() }))).collect();
        items.extend([Sep, a("Effacer le menu", ClearRecent)]);
        file.push(Sub("Ouvrir récent", items));
    }
    file.extend([
        a("Importer un GEDCOM…", ImportFile),
        Sep,
        a("Enregistrer", SaveFile),
        a("Enregistrer sous…", SaveFileAs),
        Sep,
        a("Exporter en GEDZIP…", ExportGedzip),
    ]);
    if !mac {
        file.extend([Sep, a("Réglages du fichier…", OpenFileSettings), Sep, a("Quitter", Quit)]);
    }
    menus.push(("Fichier", file));
    menus.push((
        "Édition",
        vec![
            a("Annuler la modification", Undo),
            Sep,
            Item::Os("Couper", Box::new(input::Cut), OsAction::Cut),
            Item::Os("Copier", Box::new(input::Copy), OsAction::Copy),
            Item::Os("Coller", Box::new(input::Paste), OsAction::Paste),
            Item::Os("Tout sélectionner", Box::new(input::SelectAll), OsAction::SelectAll),
            Sep,
            a(label::ATTACH_FILES, AttachFiles),
            a("Ajouter un lien web…", AddWebLink),
            Sep,
            a(label::COPY_NAME, CopyName),
            a(label::COPY_ID, CopyId),
            a(label::DELETE, DeleteRecord),
            Sep,
            a("Rechercher", FocusSearch),
        ],
    ));
    menus.push((
        "Individu",
        vec![
            a(label::NEW_INDIVIDUAL, NewIndividual),
            Sep,
            Sub(
                label::ADD_RELATIVE,
                vec![
                    a(label::FATHER, AddFather),
                    a(label::MOTHER, AddMother),
                    a(label::SPOUSE, AddSpouse),
                    a(label::CHILD, AddChild),
                    a(label::SIBLING, AddSibling),
                ],
            ),
            Sub(
                label::LINK_PERSON,
                vec![
                    a(label::FATHER_LINK, LinkFather),
                    a(label::MOTHER_LINK, LinkMother),
                    a(label::SPOUSE_LINK, LinkSpouse),
                    a(label::CHILD_LINK, LinkChild),
                    a(label::SIBLING_LINK, LinkSibling),
                ],
            ),
            a(label::ADD_EVENT, AddEvent),
            a("Lier un média existant…", LinkMedia),
            a("Recadrer un média existant…", CropMedia),
            Sep,
            a(label::SET_ROOT, SetRoot),
            a(label::GO_TO_ROOT, GoToRoot),
            a("Lien de parenté avec…", KinshipWith),
        ],
    ));
    // Dépôts et notes partagées hors de la barre latérale : on les crée depuis le menu Source.
    menus.push((
        "Source",
        vec![
            a(label::ENTER_ACT, EnterRecord),
            Sep,
            a(label::NEW_SOURCE, NewSource),
            a(label::NEW_REPOSITORY, NewRepository),
            a(label::NEW_NOTE, NewNote),
        ],
    ));
    menus.push((
        "Média",
        vec![
            a(label::NEW_MEDIA, NewMedia),
            a(label::ATTACH_MEDIA_TO, AttachMediaTo),
            Sep,
            Sub(
                "Pages",
                vec![
                    a("Page précédente", PreviousPage),
                    a("Page suivante", NextPage),
                    Sep,
                    a(label::ADD_PAGES, AddPages),
                    a("Déplacer la page vers la gauche", MovePageLeft),
                    a("Déplacer la page vers la droite", MovePageRight),
                    Sep,
                    a("Séparer cette page en un nouveau média", SplitPage),
                    a("Retirer cette page du média", RemovePage),
                ],
            ),
            a(label::MERGE_WITH_MEDIA, MergeWithMedia),
            a(label::MERGE_SELECTED_MEDIA, MergeSelectedMedia),
            a("Renommer selon le modèle…", RenameMediaFiles),
            Sep,
            a(label::TAG_PERSON, TagPerson),
            Sep,
            a("Fichiers à rattacher", ShowUnreferencedFiles),
        ],
    ));
    menus.push((
        "Affichage",
        vec![
            a("Précédent", GoBack),
            a("Suivant", GoForward),
            Sep,
            a("Individus", ShowIndividuals),
            a("Sources", ShowSources),
            a("Lieux", ShowPlaces),
            a("Médias", ShowMedia),
            a("Dépôts", ShowRepositories),
            a("Notes", ShowNotes),
            Sep,
            a("Qualité des données", ShowQuality),
            a("Statistiques", ShowStats),
            Sep,
            a("Arbre", ShowTree),
            a("Fiche", ShowForm),
            a("GEDCOM brut", ShowRaw),
            Sep,
            a("Masquer ou afficher la barre latérale", ToggleSidebar),
            a("Événements des proches", ToggleRelativeEvents),
            Sep,
            Sub(
                "Arbre",
                vec![
                    a("Arbre libre", TreeChart),
                    a("Éventail", TreeFan),
                    Sep,
                    a(label::CENTER_TREE, CenterTree),
                    Sep,
                    a("Plus de générations", MoreGenerations),
                    a("Moins de générations", FewerGenerations),
                ],
            ),
            Sub(
                "Filtrer la liste",
                vec![
                    a("Hommes", FilterMen),
                    a("Femmes", FilterWomen),
                    a("Ascendants de la racine", FilterAncestors),
                    a("Sans source", FilterUnsourced),
                    a("Sans parents", FilterNoParents),
                    a("Sans date", FilterNoDate),
                    a("Unions sans mariage", FilterNoMarriage),
                    a("Avec média", FilterWithMedia),
                    Sep,
                    a("Éléments signalés seulement", ToggleFlagged),
                    a("Anomalies masquées", ToggleIgnoredIssues),
                    a(label::CLEAR_SEARCH, ClearSearch),
                ],
            ),
        ],
    ));
    menus.push((
        "Aide",
        vec![a("Toutes les commandes…", CommandPalette), a("Aide de la recherche avancée", ToggleSearchHelp)],
    ));
    menus
}

fn to_menu_items(items: Vec<Item>) -> Vec<MenuItem> {
    items
        .into_iter()
        .map(|i| match i {
            Item::Action(name, action) => {
                MenuItem::Action { name: name.into(), action, os_action: None, checked: false, disabled: false }
            }
            Item::Recent(name, action) => {
                MenuItem::Action { name: name.into(), action, os_action: None, checked: false, disabled: false }
            }
            Item::Os(name, action, os) => {
                MenuItem::Action { name: name.into(), action, os_action: Some(os), checked: false, disabled: false }
            }
            Item::Sub(name, items) => MenuItem::submenu(Menu::new(name).items(to_menu_items(items))),
            Item::Separator => MenuItem::separator(),
        })
        .collect()
}

pub fn menus() -> Vec<Menu> {
    model().into_iter().map(|(name, items)| Menu::new(name).items(to_menu_items(items))).collect()
}

/// Installe les menus : natifs sous macOS, et pour la barre dessinée dans la
/// fenêtre ailleurs (voir `AppMenuBar`).
pub fn install_menus(cx: &mut App) {
    cx.set_menus(menus());
    let owned = menus().into_iter().map(Menu::owned).collect();
    gpui_kit::base::GlobalState::global_mut(cx).set_app_menus(owned);
}

/// Une commande pour la palette : chemin dans les menus (« Individu › Ajouter un proche › Père ») et action.
pub struct PaletteEntry {
    pub path: String,
    pub action: Box<dyn Action>,
}

/// Toutes les commandes des menus, à plat, pour la palette.
/// Les actions d'édition de texte (couper, coller…) n'y figurent pas.
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
        walk(menu, items, &mut out);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{Item, model_with, palette_entries};
    use gpui_kit::Action;
    use std::path::PathBuf;

    /// Une commande n'a qu'une place dans la barre de menus.
    #[test]
    fn menus_have_every_command_once() {
        fn walk(path: &str, items: Vec<Item>, seen: &mut Vec<(String, Box<dyn Action>)>) {
            for i in items {
                match i {
                    i @ (Item::Action(..) | Item::Recent(..)) => {
                        let Some((name, action)) = i.into_command() else { continue };
                        let here = format!("{path} › {name}");
                        if let Some((other, _)) = seen.iter().find(|(_, a)| a.partial_eq(action.as_ref())) {
                            panic!("« {here} » double « {other} »");
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
        for (menu, items) in model_with(&recents) {
            walk(menu, items, &mut seen);
        }
    }

    #[test]
    fn palette_has_every_command_once() {
        let entries = palette_entries();
        assert!(entries.iter().any(|e| e.path == "Individu › Ajouter un proche › Père"));
        assert!(entries.iter().any(|e| e.path == "Source › Saisir un acte"));
        assert!(entries.iter().any(|e| e.path == "Fichier › Exporter en GEDZIP"));
        for (i, e) in entries.iter().enumerate() {
            assert!(!entries[i + 1..].iter().any(|o| o.action.partial_eq(e.action.as_ref())), "{} en double", e.path);
        }
    }
}
