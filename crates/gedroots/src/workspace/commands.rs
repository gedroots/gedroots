//! Commandes des menus et de la palette, reliées à l'espace de travail.
//!
//! Une commande n'est déclarée que lorsqu'elle s'applique (individu
//! sélectionné, fichier ouvert…) : les menus macOS et la palette grisent ou
//! masquent alors d'eux-mêmes celles qui ne sont pas disponibles.

use super::relatives::PickPurpose;
use super::tree::TreeMode;
use super::*;
use gpui_kit::component::command::{Command, CommandItem, CommandState};

/// Déclare un gestionnaire d'action sur l'élément racine.
macro_rules! on {
    ($d:expr, $cx:expr, $action:ty, |$this:ident, $w:ident, $c:ident| $body:expr) => {
        $d.on_action($cx.listener(
            |$this: &mut Workspace, _: &$action, $w: &mut Window, $c: &mut Context<Workspace>| {
                #[allow(unused_variables)]
                let _ = (&$w, &$c);
                $body
            },
        ))
    };
}

/// Palette de commandes ouverte.
pub struct Palette {
    state: Entity<CommandState>,
    items: Vec<CommandItem>,
}

impl Workspace {
    /// Enregistre les gestionnaires des commandes applicables dans l'état courant.
    pub(super) fn bind_actions(&self, d: Div, _window: &mut Window, cx: &mut Context<Self>) -> Div {
        let doc = self.doc.as_ref();
        let g = doc.map(|d| &d.ged);
        let has_doc = doc.is_some();
        let sel = self.selected.clone();
        let sel_kind = sel.as_ref().and_then(|x| g?.get(x)).and_then(|n| RecordKind::from_tag(&n.tag));
        let indi = sel.clone().filter(|_| sel_kind == Some(RecordKind::Individual));
        let fam = sel.clone().filter(|_| sel_kind == Some(RecordKind::Family));
        let media = sel.clone().filter(|_| sel_kind == Some(RecordKind::Media));
        let parents = indi.as_ref().and_then(|x| Some(g?.parents(x))).unwrap_or((None, None));
        let fam_full =
            fam.as_ref().and_then(|f| g?.family(f)).is_some_and(|f| f.husband().is_some() && f.wife().is_some());
        let photo = media.as_ref().and_then(|x| self.photo_file(x)).filter(|_| self.photo_draw.is_none());
        let root = g.and_then(|g| g.root());
        let is_root = indi.is_some() && root == indi;
        let can_undo = doc.is_some_and(|d| !d.history.is_empty());
        let individuals = has_doc && self.view == View::Individuals && self.entry_form.is_none();
        let attachable = matches!(sel_kind, Some(RecordKind::Individual | RecordKind::Family | RecordKind::Source));

        let d = on!(d, cx, OpenFile, |this, w, cx| this.open_file(&OpenFile, w, cx));
        let d = on!(d, cx, NewFile, |this, w, cx| this.new_file(&NewFile, w, cx));
        let d = on!(d, cx, CommandPalette, |this, w, cx| this.toggle_palette(w, cx));
        if !has_doc {
            return d;
        }
        let d = on!(d, cx, SaveFile, |this, w, cx| this.save_file(&SaveFile, w, cx));
        let d = on!(d, cx, SaveFileAs, |this, w, cx| this.save_file_as(w, cx));
        let d = on!(d, cx, OpenAnalyses, |this, w, cx| this.open_analyses(&OpenAnalyses, w, cx));
        let d = on!(d, cx, EnterRecord, |this, w, cx| this.open_entry(&EnterRecord, w, cx));
        let d = on!(d, cx, FocusSearch, |this, w, cx| this.filter.update(cx, |f, cx| f.focus(w, cx)));
        let d = on!(d, cx, ClearSearch, |this, w, cx| this.set_query(String::new(), w, cx));
        let d = on!(d, cx, ToggleSearchHelp, |this, w, cx| {
            this.set_view(View::Individuals, w, cx);
            this.search_help = !this.search_help;
            cx.notify();
        });

        // Créations : on passe dans la bonne liste puis on crée.
        let d = on!(d, cx, NewIndividual, |this, w, cx| this.create_in(View::Individuals, w, cx));
        let d = on!(d, cx, NewFamily, |this, w, cx| this.create_in(View::Families, w, cx));
        let d = on!(d, cx, NewSource, |this, w, cx| this.create_in(View::Sources, w, cx));
        let d = on!(d, cx, NewMedia, |this, w, cx| this.create_in(View::Media, w, cx));

        // Vues.
        let d = on!(d, cx, ShowIndividuals, |this, w, cx| this.show_view(View::Individuals, w, cx));
        let d = on!(d, cx, ShowFamilies, |this, w, cx| this.show_view(View::Families, w, cx));
        let d = on!(d, cx, ShowSources, |this, w, cx| this.show_view(View::Sources, w, cx));
        let d = on!(d, cx, ShowPlaces, |this, w, cx| this.show_view(View::Places, w, cx));
        let d = on!(d, cx, ShowMedia, |this, w, cx| this.show_view(View::Media, w, cx));
        let d = on!(d, cx, ShowRepositories, |this, w, cx| this.show_view(View::Repositories, w, cx));
        let d = on!(d, cx, ShowNotes, |this, w, cx| this.show_view(View::Notes, w, cx));
        let d = on!(d, cx, ShowForm, |this, w, cx| this.show_tab(CenterTab::Form, cx));
        let d = on!(d, cx, ShowUnreferencedFiles, |this, w, cx| {
            this.orphans_open = true;
            cx.notify();
        });

        d.when(self.history.can_back(), |d| on!(d, cx, GoBack, |this, w, cx| this.go_back(w, cx)))
            .when(self.history.can_forward(), |d| on!(d, cx, GoForward, |this, w, cx| this.go_forward(w, cx)))
            .when(can_undo, |d| on!(d, cx, Undo, |this, w, cx| this.undo(&Undo, w, cx)))
            .when(self.view.kind().is_some(), |d| on!(d, cx, NewRecord, |this, w, cx| this.create_record(w, cx)))
            .when(sel.is_some(), |d| {
                on!(d, cx, DeleteRecord, |this, w, cx| {
                    this.tab = CenterTab::Form;
                    this.delete_selected(w, cx);
                })
            })
            .when(sel_kind.is_some(), |d| {
                on!(d, cx, CopyId, |this, w, cx| {
                    if let Some(x) = this.selected.clone() {
                        this.copy_id(&x, cx)
                    }
                })
            })
            .when(self.view != View::Places, |d| on!(d, cx, ShowRaw, |this, w, cx| this.show_tab(CenterTab::Raw, cx)))
            .when(!self.flag_label().is_empty(), |d| {
                on!(d, cx, ToggleFlagged, |this, w, cx| {
                    this.only_flagged = !this.only_flagged;
                    this.rebuild_rows(cx);
                    cx.notify();
                })
            })
            .when(root.is_some(), |d| on!(d, cx, GoToRoot, |this, w, cx| this.goto_root(w, cx)))
            // Arbre et filtres de la liste des individus.
            .when(individuals, |d| {
                let d = on!(d, cx, ShowTree, |this, w, cx| this.show_tab(CenterTab::Tree, cx));
                let d = on!(d, cx, TreeAncestors, |this, w, cx| this.set_tree_mode(TreeMode::Ancestors, cx));
                let d = on!(d, cx, TreeDescendants, |this, w, cx| this.set_tree_mode(TreeMode::Descendants, cx));
                let d = on!(d, cx, TreeFan, |this, w, cx| this.set_tree_mode(TreeMode::Fan, cx));
                let d = on!(d, cx, MoreGenerations, |this, w, cx| this.step_generations(1, cx));
                let d = on!(d, cx, FewerGenerations, |this, w, cx| this.step_generations(-1, cx));
                let d = on!(d, cx, FilterMen, |this, w, cx| this.toggle_filter("sexe:h", w, cx));
                let d = on!(d, cx, FilterWomen, |this, w, cx| this.toggle_filter("sexe:f", w, cx));
                let d = on!(d, cx, FilterAncestors, |this, w, cx| this.toggle_filter("sosa", w, cx));
                let d = on!(d, cx, FilterUnsourced, |this, w, cx| this.toggle_filter("sans:source", w, cx));
                let d = on!(d, cx, FilterNoParents, |this, w, cx| this.toggle_filter("sans:parents", w, cx));
                let d = on!(d, cx, FilterNoDate, |this, w, cx| this.toggle_filter("sans:date", w, cx));
                on!(d, cx, FilterWithMedia, |this, w, cx| this.toggle_filter("avec:média", w, cx))
            })
            // Individu sélectionné.
            .when(indi.is_some(), |d| {
                let d = d
                    .when(parents.0.is_none(), |d| {
                        let d = on!(d, cx, AddFather, |this, w, cx| this.relative(Relation::Father, false, w, cx));
                        on!(d, cx, LinkFather, |this, w, cx| this.relative(Relation::Father, true, w, cx))
                    })
                    .when(parents.1.is_none(), |d| {
                        let d = on!(d, cx, AddMother, |this, w, cx| this.relative(Relation::Mother, false, w, cx));
                        on!(d, cx, LinkMother, |this, w, cx| this.relative(Relation::Mother, true, w, cx))
                    });
                let d = on!(d, cx, AddSpouse, |this, w, cx| this.relative(Relation::Spouse, false, w, cx));
                let d = on!(d, cx, AddChild, |this, w, cx| this.relative(Relation::Child, false, w, cx));
                let d = on!(d, cx, AddSibling, |this, w, cx| this.relative(Relation::Sibling, false, w, cx));
                let d = on!(d, cx, LinkSpouse, |this, w, cx| this.relative(Relation::Spouse, true, w, cx));
                let d = on!(d, cx, LinkChild, |this, w, cx| this.relative(Relation::Child, true, w, cx));
                let d = on!(d, cx, LinkSibling, |this, w, cx| this.relative(Relation::Sibling, true, w, cx));
                d.when(!is_root, |d| {
                    on!(d, cx, SetRoot, |this, w, cx| {
                        if let Some(x) = this.selected.clone() {
                            this.set_root(x, cx)
                        }
                    })
                })
            })
            .when(indi.is_some() || fam.is_some(), |d| {
                on!(d, cx, AddEvent, |this, w, cx| {
                    if let Some(x) = this.selected.clone() {
                        this.open_event_dialog(x, None, w, cx)
                    }
                })
            })
            .when(attachable, |d| {
                on!(d, cx, AttachFiles, |this, w, cx| {
                    if let Some(x) = this.selected.clone() {
                        this.choose_files(x, w, cx)
                    }
                })
            })
            // Famille sélectionnée.
            .when(fam.is_some(), |d| {
                let d = on!(d, cx, NewFamilyChild, |this, w, cx| {
                    if let Some(f) = this.selected.clone() {
                        this.add_family_child(f, w, cx)
                    }
                });
                let d = on!(d, cx, LinkFamilyChild, |this, w, cx| {
                    if let Some(fam) = this.selected.clone() {
                        this.open_picker(PickPurpose::FamilyChild { fam }, w, cx)
                    }
                });
                d.when(!fam_full, |d| {
                    on!(d, cx, LinkFamilyParent, |this, w, cx| {
                        if let Some(fam) = this.selected.clone() {
                            this.open_picker(PickPurpose::FamilySpouse { fam }, w, cx)
                        }
                    })
                })
            })
            .when(photo.is_some(), |d| on!(d, cx, TagPerson, |this, w, cx| this.tag_person_on_photo(cx)))
            .when(media.is_some(), |d| {
                on!(d, cx, AttachMediaTo, |this, w, cx| {
                    if let Some(media) = this.selected.clone() {
                        this.open_picker(PickPurpose::MediaTo { media }, w, cx)
                    }
                })
            })
    }

    /// Image (existante) de la page affichée d'un média.
    pub(super) fn photo_file(&self, media: &str) -> Option<PathBuf> {
        let doc = self.doc.as_ref()?;
        let m = Media::new(doc.ged.get(media)?);
        let page = self.current_page(media);
        let f = m.files().get(page - 1).copied()?;
        let image = f.form.is_some_and(|t| t.starts_with("image/")) || mime_for(f.path).starts_with("image/");
        super::pages::page_path(doc, &m, page).filter(|p| image && p.exists())
    }

    pub(super) fn tag_person_on_photo(&mut self, cx: &mut Context<Self>) {
        let Some(media) = self.selected.clone() else { return };
        let Some(path) = self.photo_file(&media) else { return };
        self.tab = CenterTab::Form;
        self.start_draw(media, None, &path, cx);
    }

    fn save_file_as(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(doc) = &self.doc else { return };
        let (dir, name) = match &doc.path {
            Some(p) => (
                p.parent().map(|p| p.to_path_buf()).unwrap_or_else(dirs_home),
                p.file_name().map(|n| n.to_string_lossy().to_string()),
            ),
            None => (dirs_home(), None),
        };
        let rx = cx.prompt_for_new_path(&dir, Some(name.as_deref().unwrap_or("arbre.ged")));
        cx.spawn_in(window, async move |this, cx| {
            if let Ok(Ok(Some(p))) = rx.await {
                let _ = this.update_in(cx, |this, _, cx| this.write_to(p, cx));
            }
        })
        .detach();
    }

    fn create_in(&mut self, view: View, window: &mut Window, cx: &mut Context<Self>) {
        self.show_view(view, window, cx);
        self.create_record(window, cx);
        cx.notify();
    }

    /// Change de liste, en quittant la saisie d'acte si besoin.
    fn show_view(&mut self, view: View, window: &mut Window, cx: &mut Context<Self>) {
        self.entry_form = None;
        self.set_view(view, window, cx);
        cx.notify();
    }

    fn show_tab(&mut self, tab: CenterTab, cx: &mut Context<Self>) {
        self.tab = tab;
        cx.notify();
    }

    fn set_tree_mode(&mut self, mode: TreeMode, cx: &mut Context<Self>) {
        self.tab = CenterTab::Tree;
        self.tree_mode = mode;
        if mode == TreeMode::Fan && self.tree_gens < 5 {
            self.tree_gens = 5;
        }
        cx.notify();
    }

    fn step_generations(&mut self, delta: i32, cx: &mut Context<Self>) {
        let range = self.tree_mode.generations();
        let gens = self.tree_gens.clamp(*range.start(), *range.end()) as i32 + delta;
        self.tree_gens = (gens.max(0) as u32).clamp(*range.start(), *range.end());
        self.tab = CenterTab::Tree;
        cx.notify();
    }

    pub(super) fn set_query(&mut self, query: String, window: &mut Window, cx: &mut Context<Self>) {
        self.filter.update(cx, |f, cx| f.set_value(query, window, cx));
        self.rebuild_rows(cx);
        cx.notify();
    }

    fn toggle_filter(&mut self, token: &str, window: &mut Window, cx: &mut Context<Self>) {
        let current = self.filter.read(cx).value().to_string();
        self.set_query(toggle_token(&current, token), window, cx);
    }

    /// Ajoute un nouveau proche, ou ouvre le sélecteur pour lier une personne existante.
    fn relative(&mut self, rel: Relation, existing: bool, window: &mut Window, cx: &mut Context<Self>) {
        let Some(indi) = self.selected.clone() else { return };
        if existing {
            self.open_picker(PickPurpose::Relative { indi, rel }, window, cx);
        } else {
            self.add_relative(indi, rel, window, cx);
        }
    }

    // ------------------------------------------------------------------
    // Palette de commandes
    // ------------------------------------------------------------------

    fn toggle_palette(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.palette.take().is_some() {
            window.focus(&self.focus, cx);
            cx.notify();
            return;
        }
        // Seules les commandes applicables maintenant (vérifié depuis l'espace de travail).
        let items = crate::commands::palette_entries()
            .into_iter()
            .filter(|e| window.is_action_available(e.action.as_ref(), cx))
            .map(|e| {
                // Mots-clés sans accents : « pere » trouve « Père ».
                let words: Vec<String> = e.path.split(" › ").map(fold_accents).collect();
                CommandItem::new().label(e.path).keywords(words).action(e.action)
            })
            .collect();
        let state = cx.new(|cx| CommandState::new(window, cx));
        state.update(cx, |s, cx| s.focus(window, cx));
        self.palette = Some(Palette { state, items });
        cx.notify();
    }

    fn close_palette(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.palette.take().is_some() {
            window.focus(&self.focus, cx);
            cx.notify();
        }
    }

    pub(super) fn render_palette(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let p = self.palette.as_ref()?;
        let (e1, e2) = (cx.entity().downgrade(), cx.entity().downgrade());
        let command = Command::new(&p.state)
            .items(p.items.iter().cloned())
            .placeholder("Rechercher une commande…")
            .empty(|_, _, _| div().p_4().text_color(theme::muted()).child("Aucune commande ne correspond."))
            .max_h(px(420.))
            .on_confirm(move |_, window, cx| {
                let _ = e1.update(cx, |this, cx| this.close_palette(window, cx));
            })
            .on_cancel(move |window, cx| {
                let _ = e2.update(cx, |this, cx| this.close_palette(window, cx));
            });
        Some(
            div()
                .id("palette-backdrop")
                .absolute()
                .inset_0()
                .flex()
                .justify_center()
                .items_start()
                .pt(px(80.))
                .bg(gpui_kit::hsla(0., 0., 0., 0.35))
                .on_mouse_down(MouseButton::Left, cx.listener(|this, _, w, cx| this.close_palette(w, cx)))
                .child(
                    div()
                        .id("palette")
                        .w(px(560.))
                        .rounded_lg()
                        .bg(theme::panel())
                        .border_1()
                        .border_color(theme::border_strong())
                        .shadow_lg()
                        .overflow_hidden()
                        // Un clic dans la palette ne la ferme pas.
                        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                        .child(command),
                )
                .into_any_element(),
        )
    }
}

/// Retire les accents (é → e, œ → oe…) pour la recherche dans la palette.
fn fold_accents(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            'à' | 'â' | 'ä' | 'À' | 'Â' | 'Ä' => out.push('a'),
            'é' | 'è' | 'ê' | 'ë' | 'É' | 'È' | 'Ê' | 'Ë' => out.push('e'),
            'î' | 'ï' | 'Î' | 'Ï' => out.push('i'),
            'ô' | 'ö' | 'Ô' | 'Ö' => out.push('o'),
            'ù' | 'û' | 'ü' | 'Ù' | 'Û' | 'Ü' => out.push('u'),
            'ç' | 'Ç' => out.push('c'),
            'œ' | 'Œ' => out.push_str("oe"),
            c => out.extend(c.to_lowercase()),
        }
    }
    out
}
