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
        ImportFile,
        OpenFileSettings,
        SaveFile,
        SaveFileAs,
        Quit,
        // Édition
        Undo,
        NewRecord,
        DeleteRecord,
        CopyId,
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
        AddEvent,
        // Famille
        NewFamily,
        NewFamilyChild,
        LinkFamilyChild,
        LinkFamilyParent,
        // Source
        NewSource,
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
        GoBack,
        GoForward,
        ShowIndividuals,
        ShowFamilies,
        ShowSources,
        ShowPlaces,
        ShowMedia,
        ShowRepositories,
        ShowNotes,
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
        FilterWithMedia,
        ClearSearch,
        ToggleSearchHelp,
        // Analyses
        OpenAnalyses,
        // Aide
        CommandPalette,
    ]
);

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
        KeyBinding::new(shortcut!("shift-n"), NewRecord, WS),
        KeyBinding::new(shortcut!("f"), FocusSearch, WS),
        KeyBinding::new(shortcut!("e"), EnterRecord, WS),
        KeyBinding::new(shortcut!("shift-h"), GoToRoot, WS),
        KeyBinding::new(shortcut!("shift-a"), OpenAnalyses, WS),
        KeyBinding::new(shortcut!("shift-p"), CommandPalette, WS),
        KeyBinding::new(shortcut!("["), GoBack, WS),
        KeyBinding::new(shortcut!("]"), GoForward, WS),
        KeyBinding::new(shortcut!("1"), ShowIndividuals, WS),
        KeyBinding::new(shortcut!("2"), ShowFamilies, WS),
        KeyBinding::new(shortcut!("3"), ShowSources, WS),
        KeyBinding::new(shortcut!("4"), ShowPlaces, WS),
        KeyBinding::new(shortcut!("5"), ShowMedia, WS),
        KeyBinding::new(shortcut!("6"), ShowRepositories, WS),
        KeyBinding::new(shortcut!("7"), ShowNotes, WS),
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

/// Une entrée de menu.
enum Item {
    Action(&'static str, Box<dyn Action>),
    /// Action standard du système (copier, coller…) : macOS la relie aux champs de saisie.
    Os(&'static str, Box<dyn Action>, OsAction),
    Sub(&'static str, Vec<Item>),
    Separator,
}

fn a(label: &'static str, action: impl Action) -> Item {
    Item::Action(label, Box::new(action))
}

/// Le modèle de menus, commun à toutes les plateformes.
fn model() -> Vec<(&'static str, Vec<Item>)> {
    use Item::{Separator as Sep, Sub};
    let mac = cfg!(target_os = "macos");
    let mut menus = Vec::new();
    if mac {
        menus.push(("GedRoots", vec![a("Toutes les commandes…", CommandPalette), Sep, a("Quitter GedRoots", Quit)]));
    }
    let mut file = vec![
        a("Nouveau fichier", NewFile),
        a("Ouvrir…", OpenFile),
        a("Importer un GEDCOM…", ImportFile),
        Sep,
        a("Enregistrer", SaveFile),
        a("Enregistrer sous…", SaveFileAs),
        Sep,
        a("Réglages du fichier…", OpenFileSettings),
    ];
    if !mac {
        file.extend([Sep, a("Quitter", Quit)]);
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
            a("Nouvel élément dans cette liste", NewRecord),
            a("Supprimer l'élément…", DeleteRecord),
            a("Copier l'ID", CopyId),
            Sep,
            a("Ajouter un lien web…", AddWebLink),
            Sep,
            a("Rechercher", FocusSearch),
        ],
    ));
    menus.push((
        "Individu",
        vec![
            a("Nouvel individu", NewIndividual),
            Sep,
            Sub(
                "Ajouter un proche",
                vec![
                    a("Père", AddFather),
                    a("Mère", AddMother),
                    a("Conjoint", AddSpouse),
                    a("Enfant", AddChild),
                    a("Frère ou sœur", AddSibling),
                ],
            ),
            Sub(
                "Lier une personne existante",
                vec![
                    a("Père…", LinkFather),
                    a("Mère…", LinkMother),
                    a("Conjoint…", LinkSpouse),
                    a("Enfant…", LinkChild),
                    a("Frère ou sœur…", LinkSibling),
                ],
            ),
            a("Ajouter un événement…", AddEvent),
            a("Rattacher des photos ou documents…", AttachFiles),
            Sep,
            a("Définir comme individu racine", SetRoot),
            a("Aller à l'individu racine", GoToRoot),
        ],
    ));
    menus.push((
        "Famille",
        vec![
            a("Nouvelle famille", NewFamily),
            Sep,
            a("Ajouter un événement…", AddEvent),
            a("Ajouter un nouvel enfant", NewFamilyChild),
            a("Lier un enfant existant…", LinkFamilyChild),
            a("Lier un parent existant…", LinkFamilyParent),
        ],
    ));
    menus.push(("Source", vec![a("Saisir un acte…", EnterRecord), Sep, a("Nouvelle source", NewSource)]));
    menus.push((
        "Média",
        vec![
            a("Nouveau média", NewMedia),
            a("Rattacher des fichiers à l'élément…", AttachFiles),
            a("Rattacher ce média à…", AttachMediaTo),
            Sep,
            Sub(
                "Pages",
                vec![
                    a("Page précédente", PreviousPage),
                    a("Page suivante", NextPage),
                    Sep,
                    a("Ajouter des pages…", AddPages),
                    a("Déplacer la page vers la gauche", MovePageLeft),
                    a("Déplacer la page vers la droite", MovePageRight),
                    Sep,
                    a("Séparer cette page en un nouveau média", SplitPage),
                    a("Retirer cette page du média", RemovePage),
                ],
            ),
            a("Fusionner avec un autre média…", MergeWithMedia),
            a("Fusionner les médias sélectionnés", MergeSelectedMedia),
            a("Renommer selon le modèle…", RenameMediaFiles),
            Sep,
            a("Identifier une personne sur la photo", TagPerson),
            Sep,
            a("Fichiers du dossier sans média…", ShowUnreferencedFiles),
        ],
    ));
    menus.push((
        "Affichage",
        vec![
            a("Précédent", GoBack),
            a("Suivant", GoForward),
            Sep,
            a("Individus", ShowIndividuals),
            a("Familles", ShowFamilies),
            a("Sources", ShowSources),
            a("Lieux", ShowPlaces),
            a("Médias", ShowMedia),
            a("Dépôts", ShowRepositories),
            a("Notes", ShowNotes),
            Sep,
            a("Arbre", ShowTree),
            a("Fiche", ShowForm),
            a("GEDCOM brut", ShowRaw),
            Sep,
            Sub(
                "Arbre",
                vec![
                    a("Arbre libre", TreeChart),
                    a("Éventail", TreeFan),
                    Sep,
                    a("Centrer sur la personne", CenterTree),
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
                    a("Avec média", FilterWithMedia),
                    Sep,
                    a("Éléments signalés seulement", ToggleFlagged),
                    a("Effacer la recherche", ClearSearch),
                ],
            ),
            Sep,
            a("Toutes les commandes…", CommandPalette),
        ],
    ));
    menus.push(("Analyses", vec![a("Ouvrir les analyses", OpenAnalyses)]));
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
                Item::Action(name, action) => {
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
        if menu != "GedRoots" {
            walk(menu, items, &mut out);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::palette_entries;

    #[test]
    fn palette_has_every_command_once() {
        let entries = palette_entries();
        assert!(entries.iter().any(|e| e.path == "Individu › Ajouter un proche › Père"));
        assert!(entries.iter().any(|e| e.path == "Source › Saisir un acte"));
        for (i, e) in entries.iter().enumerate() {
            assert!(!entries[i + 1..].iter().any(|o| o.action.partial_eq(e.action.as_ref())), "{} en double", e.path);
        }
    }
}
