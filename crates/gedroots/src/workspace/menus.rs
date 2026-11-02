//! Menus contextuels (clic droit).
//!
//! Chaque menu agit sur l'élément cliqué, sans avoir à le sélectionner
//! d'abord, et appelle les mêmes fonctions que les commandes de la barre de
//! menus. Règles :
//! - le clic droit ne crée pas de commande : une action sur l'élément lui-même
//!   existe aussi dans la barre de menus, sous le même libellé (`commands::label`) ;
//! - il ne propose que ce qui s'applique à l'élément (pas de « Père » quand il
//!   est déjà connu…) ;
//! - exceptions : les actions sur une partie d'une fiche (un événement, un média
//!   rattaché, un membre de famille), que la barre de menus ne peut pas viser, et
//!   les liens pour s'y rendre (« Ouvrir la fiche », « Afficher le lieu »…).

use super::relatives::PickPurpose;
use super::*;
use crate::commands::label;
use gpui_kit::component::menu::{ContextMenu, ContextMenuExt, PopupMenu, PopupMenuItem};

/// Fonction de construction d'un menu, pour `.context_menu(…)`.
pub(super) type MenuBuilder = Rc<dyn Fn(PopupMenu, &mut Window, &mut Context<PopupMenu>) -> PopupMenu>;

/// D'où le menu est ouvert : quelques entrées en dépendent.
#[derive(Clone)]
pub(super) enum From {
    /// Une ligne de la liste de gauche (on peut y supprimer l'élément).
    List,
    /// Un lien du panneau de détail, une case de l'éventail…
    Link,
    /// Une case de l'arbre libre.
    Tree,
    /// Une image rattachée à l'enregistrement donné (vignettes du panneau).
    Attached(String),
    /// Un média d'un événement (enregistrement porteur, position de l'événement).
    OnEvent(String, usize),
    /// Un proche dans la section « Famille » d'un individu : le lien (individu,
    /// famille) que « Détacher » défait, et le libellé de la commande.
    Relative(String, String, &'static str),
}

type Ws = WeakEntity<Workspace>;

/// `.with_menu(builder)` : comme `.context_menu(…)`, avec un menu construit à l'avance.
pub(super) trait WithMenu: ContextMenuExt + Sized {
    fn with_menu(self, builder: MenuBuilder) -> ContextMenu<Self> {
        self.context_menu(move |m, w, cx| builder(m, w, cx))
    }
}

impl<E: ContextMenuExt> WithMenu for E {}

/// Une entrée qui agit sur l'espace de travail. L'action est différée : le
/// menu se ferme d'abord (et rend le focus), puis la fenêtre ouverte par
/// l'action (sélecteur, événement…) prend le focus.
pub(super) fn act(
    ws: &Ws,
    label: impl Into<SharedString>,
    f: impl Fn(&mut Workspace, &mut Window, &mut Context<Workspace>) + 'static,
) -> PopupMenuItem {
    let ws = ws.clone();
    let f = Rc::new(f);
    PopupMenuItem::new(label).on_click(move |_, window, cx| {
        let (ws, f) = (ws.clone(), f.clone());
        window.defer(cx, move |window, cx| {
            let _ = ws.update(cx, |this, cx| f(this, window, cx));
        });
    })
}

/// Lit l'état de l'espace de travail au moment d'ouvrir le menu.
fn read<R>(ws: &Ws, cx: &App, f: impl FnOnce(&Workspace, &Gedcom) -> R) -> Option<R> {
    let ws = ws.upgrade()?;
    let this = ws.read(cx);
    let g = &this.doc.as_ref()?.ged;
    Some(f(this, g))
}

impl Workspace {
    /// Ouvre un enregistrement dans l'onglet voulu.
    fn open_in(&mut self, xref: String, tab: CenterTab, window: &mut Window, cx: &mut Context<Self>) {
        self.act_editor = None;
        self.goto(xref, window, cx);
        self.tab = tab;
        cx.notify();
    }

    /// Supprimer depuis la liste : on affiche l'élément et sa confirmation.
    fn delete_from_list(&mut self, key: String, window: &mut Window, cx: &mut Context<Self>) {
        self.select(key, window, cx);
        self.tab = CenterTab::Form;
        self.delete_selected(window, cx);
    }

    fn ws(cx: &Context<Self>) -> Ws {
        cx.entity().downgrade()
    }

    /// Menu d'un enregistrement quelconque, selon son type.
    pub(super) fn record_menu(&self, xref: &str, from: From, cx: &Context<Self>) -> MenuBuilder {
        record_menu(Self::ws(cx), xref.to_string(), from)
    }

    pub(super) fn person_menu(&self, xref: &str, from: From, cx: &Context<Self>) -> MenuBuilder {
        person_menu(Self::ws(cx), xref.to_string(), from)
    }

    /// Menu d'un acte, dans la fiche de sa source.
    pub(super) fn act_menu(&self, source: &str, key: ActKey, cx: &Context<Self>) -> MenuBuilder {
        let ws = Self::ws(cx);
        let source = source.to_string();
        Rc::new(move |menu, _, _| {
            let (s1, k1, s2, k2) = (source.clone(), key.clone(), source.clone(), key.clone());
            menu.item(act(&ws, "Modifier l'acte", move |this, w, cx| {
                this.open_act(s1.clone(), Some(k1.clone()), w, cx)
            }))
            .separator()
            .item(act(&ws, "Supprimer l'acte", move |this, w, cx| this.delete_act(s2.clone(), k2.clone(), w, cx)))
        })
    }

    /// Menu d'un événement du panneau de détail.
    pub(super) fn event_menu(&self, owner: String, ix: usize, cx: &Context<Self>) -> MenuBuilder {
        let ws = Self::ws(cx);
        Rc::new(move |menu, _, cx| {
            let Some((place, sources)) = read(&ws, cx, |_, g| {
                let ev = g.read_event(&owner, ix);
                let place = ev.as_ref().map(|e| e.place.clone()).filter(|p| !p.is_empty());
                let sources: Vec<(String, String)> = g
                    .get(&owner)
                    .and_then(|n| n.children.get(ix))
                    .map(|e| {
                        e.children_with("SOUR")
                            .filter_map(|s| s.pointer())
                            .map(|s| (s.to_string(), g.record_label(s)))
                            .collect()
                    })
                    .unwrap_or_default();
                (place, sources)
            }) else {
                return menu;
            };
            let (o1, o2, o3, o4) = (owner.clone(), owner.clone(), owner.clone(), owner.clone());
            menu.item(act(&ws, "Modifier l'événement…", move |this, w, cx| {
                this.open_event_dialog(o1.clone(), Some(ix), w, cx)
            }))
            .separator()
            .item(act(&ws, label::attach_files(), move |this, w, cx| {
                this.choose_files_for_event(o3.clone(), ix, w, cx)
            }))
            .item(act(&ws, "Lier un média existant…", move |this, w, cx| {
                this.open_picker(PickPurpose::MediaToEvent { owner: o4.clone(), ix }, w, cx)
            }))
            .separator()
            .when_some(place, |m, p| {
                m.item(act(&ws, "Afficher le lieu", move |this, w, cx| this.goto_place(p.clone(), w, cx)))
            })
            .map(|m| {
                sources.into_iter().fold(m, |m, (s, title)| {
                    m.item(act(&ws, format!("Ouvrir la source « {} »", ellipsize(&title, 40)), move |this, w, cx| {
                        this.goto(s.clone(), w, cx)
                    }))
                })
            })
            .separator()
            .item(act(&ws, "Supprimer l'événement", move |this, w, cx| this.delete_event(o2.clone(), ix, w, cx)))
        })
    }

    /// Menu de la liste : l'élément visé, ou sous les lignes, créer un élément.
    pub(super) fn list_menu(&self, cx: &Context<Self>) -> MenuBuilder {
        let ws = Self::ws(cx);
        let view = self.view;
        Rc::new(move |menu, w, cx| {
            // Clic droit sur une ligne : le menu de cet élément.
            if let Some(key) = read(&ws, cx, |this, _| this.menu_row.take()).flatten() {
                // Catégories de la page Qualité : pas de menu.
                if view == View::Quality {
                    return menu;
                }
                let builder = if view == View::Places {
                    place_menu(ws.clone(), key, From::List)
                } else {
                    record_menu(ws.clone(), key, From::List)
                };
                return builder(menu, w, cx);
            }
            let Some((filtered, root, can_save)) = read(&ws, cx, |this, g| {
                (
                    !this.filter.read(cx).value().trim().is_empty() || this.only_flagged,
                    g.root().is_some(),
                    this.can_save_search(cx),
                )
            }) else {
                return menu;
            };
            let new = match view.kind() {
                Some(RecordKind::Individual) => Some(label::new_individual()),
                Some(RecordKind::Source) => Some(label::new_source()),
                Some(RecordKind::Media) => Some(label::new_media()),
                Some(RecordKind::Repository) => Some(label::new_repository()),
                Some(RecordKind::SharedNote) => Some(label::new_note()),
                _ => None,
            };
            menu.when_some(new, |m, label| m.item(act(&ws, label, |this, w, cx| this.create_record(w, cx))))
                .when(view == View::Individuals && root, |m| {
                    m.item(act(&ws, label::go_to_root(), |this, w, cx| this.goto_root(w, cx)))
                })
                .when(view == View::Sources, |m| {
                    m.item(act(&ws, label::enter_act(), |this, w, cx| this.enter_act(w, cx)))
                })
                .when(can_save, |m| {
                    m.separator().item(act(&ws, label::save_search(), |this, w, cx| this.save_search(w, cx)))
                })
                .when(filtered, |m| {
                    m.separator().item(act(&ws, label::clear_search(), |this, w, cx| {
                        this.only_flagged = false;
                        this.set_query(String::new(), w, cx)
                    }))
                })
        })
    }

    /// Case vide de l'arbre (parent inconnu…) : nouvelle personne ou personne existante.
    pub(super) fn slot_menu(&self, indi: &str, rel: Relation, cx: &Context<Self>) -> MenuBuilder {
        let (ws, indi) = (Self::ws(cx), indi.to_string());
        Rc::new(move |menu, _, _| {
            let (a, b) = (indi.clone(), indi.clone());
            menu.item(act(&ws, label::add_relative(), move |this, w, cx| this.add_relative(a.clone(), rel, w, cx)))
                .item(act(&ws, format!("{}…", label::link_person()), move |this, w, cx| {
                    this.open_picker(PickPurpose::Relative { indi: b.clone(), rel }, w, cx)
                }))
        })
    }
}

pub(super) fn place_menu(ws: Ws, name: String, from: From) -> MenuBuilder {
    Rc::new(move |menu, _, _| {
        let (n1, n2, n3, n4) = (name.clone(), name.clone(), name.clone(), name.clone());
        menu.item(act(&ws, "Afficher sur la carte", move |this, w, cx| {
            this.goto_place(n1.clone(), w, cx);
            this.tab = CenterTab::Map;
        }))
        .item(act(&ws, "Ouvrir la fiche du lieu", move |this, w, cx| {
            this.goto_place(n2.clone(), w, cx);
            this.tab = CenterTab::Form;
        }))
        .separator()
        .item(act(&ws, label::copy_name(), move |this, _, cx| this.copy_name(&n3, cx)))
        .when(matches!(from, From::List), |m| {
            m.item(act(&ws, label::delete(), move |this, w, cx| this.delete_from_list(n4.clone(), w, cx)))
        })
    })
}

/// Menu d'un enregistrement, son type lu à l'ouverture (lignes de la liste).
pub(super) fn record_menu(ws: Ws, xref: String, from: From) -> MenuBuilder {
    Rc::new(move |menu, w, cx| {
        let Some(kind) = read(&ws, cx, |_, g| g.get(&xref).and_then(|n| RecordKind::from_tag(&n.tag))).flatten() else {
            return menu;
        };
        let (ws, x, from) = (ws.clone(), xref.clone(), from.clone());
        let builder = match kind {
            RecordKind::Individual => person_menu(ws, x, from),
            RecordKind::Family => family_menu(ws, x, from),
            kind => other_menu(ws, x, kind, from),
        };
        builder(menu, w, cx)
    })
}

fn ellipsize(s: &str, n: usize) -> String {
    if s.chars().count() <= n { s.to_string() } else { format!("{}…", s.chars().take(n - 1).collect::<String>()) }
}

/// Entrées communes en bas de menu, dans l'ordre du menu Édition : copier le
/// nom, l'identifiant, supprimer (depuis la liste).
fn tail(menu: PopupMenu, ws: &Ws, xref: &str, from: &From) -> PopupMenu {
    let (a, b, c) = (xref.to_string(), xref.to_string(), xref.to_string());
    menu.separator()
        .item(act(ws, label::copy_name(), move |this, _, cx| this.copy_name(&c, cx)))
        .item(act(ws, label::copy_id(), move |this, _, cx| this.copy_id(&a, cx)))
        .when(matches!(from, From::List), |m| {
            m.item(act(ws, label::delete(), move |this, w, cx| this.delete_from_list(b.clone(), w, cx)))
        })
}

pub(super) fn person_menu(ws: Ws, x: String, from: From) -> MenuBuilder {
    Rc::new(move |menu, window, cx| {
        let Some((father, mother)) = read(&ws, cx, |_, g| {
            let (f, m) = g.parents(&x);
            (f.is_some(), m.is_some())
        }) else {
            return menu;
        };
        let rels: Vec<(Relation, SharedString, SharedString)> = [
            (Relation::Father, label::father(), label::father_link()),
            (Relation::Mother, label::mother(), label::mother_link()),
            (Relation::Spouse, label::spouse(), label::spouse_link()),
            (Relation::Child, label::child(), label::child_link()),
            (Relation::Sibling, label::sibling(), label::sibling_link()),
        ]
        .into_iter()
        .filter(|(r, _, _)| match r {
            Relation::Father => !father,
            Relation::Mother => !mother,
            _ => true,
        })
        .collect();
        let rels2 = rels.clone();
        let (x1, x2, x3, x4, x5, x6) = (x.clone(), x.clone(), x.clone(), x.clone(), x.clone(), x.clone());
        let (wa, wb) = (ws.clone(), ws.clone());
        menu.item(act(&ws, label::center_tree(), move |this, w, cx| {
            this.act_editor = None;
            this.goto(x1.clone(), w, cx);
            this.center_chart_on(x1.clone(), w, cx)
        }))
        .item(act(&ws, "Ouvrir la fiche", move |this, w, cx| this.open_in(x2.clone(), CenterTab::Form, w, cx)))
        .separator()
        .submenu(label::add_relative(), window, cx, move |m, _, _| {
            rels.iter().fold(m, |m, (rel, label, _)| {
                let (x, rel) = (x3.clone(), *rel);
                m.item(act(&wa, label.clone(), move |this, w, cx| this.add_relative(x.clone(), rel, w, cx)))
            })
        })
        .submenu(label::link_person(), window, cx, move |m, _, _| {
            rels2.iter().fold(m, |m, (rel, _, label)| {
                let (indi, rel) = (x4.clone(), *rel);
                m.item(act(&wb, label.clone(), move |this, w, cx| {
                    this.open_picker(PickPurpose::Relative { indi: indi.clone(), rel }, w, cx)
                }))
            })
        })
        .item(act(&ws, label::add_event(), move |this, w, cx| this.open_event_dialog(x5.clone(), None, w, cx)))
        .item(act(&ws, label::attach_files(), move |this, w, cx| this.choose_files(x6.clone(), w, cx)))
        .when_some(
            match &from {
                From::Relative(indi, fam, label) => Some((indi.clone(), fam.clone(), *label)),
                _ => None,
            },
            |m, (indi, fam, label)| {
                m.separator().item(act(&ws, label, move |this, w, cx| this.unlink(indi.clone(), fam.clone(), w, cx)))
            },
        )
        .map(|m| tail(m, &ws, &x, &from))
    })
}

/// Union citée dans un lien (source, média…) : elle s'ouvre sur la fiche d'un conjoint.
fn family_menu(ws: Ws, x: String, from: From) -> MenuBuilder {
    Rc::new(move |menu, _, _| {
        let x1 = x.clone();
        menu.item(act(&ws, "Ouvrir la fiche", move |this, w, cx| this.goto(x1.clone(), w, cx)))
            .map(|m| tail(m, &ws, &x, &from))
    })
}

/// Sources, médias, dépôts, notes.
fn other_menu(ws: Ws, x: String, kind: RecordKind, from: From) -> MenuBuilder {
    Rc::new(move |menu, _, cx| {
        let Some((photo, marked, portrait_of)) = read(&ws, cx, |this, g| {
            // Médias sélectionnés ensemble (⌘ / Maj + clic) dont celui-ci : on peut les fusionner.
            let marked = if this.media_marked.len() >= 2 && this.media_marked.contains(&x) {
                this.media_marked.clone()
            } else {
                vec![]
            };
            // Photo (ou image sans support) d'un individu dont elle n'est pas déjà le portrait.
            let portrait_of = match &from {
                From::Attached(owner)
                    if g.get(owner).is_some_and(|n| n.tag == "INDI")
                        && g.portrait(owner).is_none_or(|p| p.xref() != x)
                        && g.get(&x)
                            .is_some_and(|m| Media::new(m).is_image() && medium(m).is_none_or(|v| v == "PHOTO")) =>
                {
                    Some(owner.clone())
                }
                _ => None,
            };
            (this.photo_file(&x).is_some(), marked, portrait_of)
        }) else {
            return menu;
        };
        let (x6, x7) = (x.clone(), x.clone());
        let (x1, x2, x3, x4, x5) = (x.clone(), x.clone(), x.clone(), x.clone(), x.clone());
        menu.item(act(&ws, "Ouvrir la fiche", move |this, w, cx| this.open_in(x1.clone(), CenterTab::Form, w, cx)))
            .when(kind == RecordKind::Source, |m| {
                m.separator()
                    .item(act(&ws, label::enter_act(), move |this, w, cx| {
                        this.goto(x2.clone(), w, cx);
                        this.enter_act(w, cx)
                    }))
                    .item(act(&ws, label::attach_files(), move |this, w, cx| this.choose_files(x3.clone(), w, cx)))
            })
            .when(kind == RecordKind::Media, |m| {
                m.separator()
                    .item(act(&ws, label::attach_media_to(), move |this, w, cx| {
                        this.open_picker(PickPurpose::MediaTo { media: x4.clone() }, w, cx)
                    }))
                    .when(photo, |m| {
                        m.item(act(&ws, label::tag_person(), move |this, w, cx| {
                            this.open_in(x5.clone(), CenterTab::Form, w, cx);
                            this.tag_person_on_photo(cx)
                        }))
                    })
                    .separator()
                    .when(marked.len() >= 2, |m| {
                        let list = marked.clone();
                        m.item(act(&ws, label::merge_selected_media(), move |this, w, cx| {
                            this.merge_media(list.clone(), w, cx)
                        }))
                    })
                    .when(marked.is_empty(), |m| {
                        m.item(act(&ws, label::merge_with_media(), move |this, w, cx| {
                            this.open_picker(PickPurpose::MergeInto { media: x6.clone() }, w, cx)
                        }))
                    })
                    .item(act(&ws, label::add_pages(), move |this, w, cx| this.choose_pages(x7.clone(), w, cx)))
            })
            .when_some(portrait_of, |m, owner| {
                let media = x.clone();
                m.separator().item(act(&ws, "Définir comme portrait", move |this, w, cx| {
                    let (owner, media) = (owner.clone(), media.clone());
                    this.mutate(w, cx, move |g| {
                        g.set_portrait(&owner, &media);
                        Ok((format!("Portrait de {} défini", g.record_label(&owner)), None))
                    })
                }))
            })
            .when_some(
                match &from {
                    From::OnEvent(owner, ix) => Some((owner.clone(), *ix)),
                    _ => None,
                },
                |m, (owner, ix)| {
                    let media = x.clone();
                    m.separator().item(act(&ws, "Détacher de l'événement", move |this, w, cx| {
                        let (owner, media) = (owner.clone(), media.clone());
                        this.mutate(w, cx, move |g| {
                            let what = g.record_label(&media);
                            let from = g.event_title(&owner, ix).unwrap_or_default().to_lowercase();
                            g.detach_media_from_event(&owner, ix, &media)?;
                            Ok((format!("« {what} » détaché de l'événement {from}"), None))
                        })
                    }))
                },
            )
            .when_some(
                match &from {
                    From::Attached(owner) => Some(owner.clone()),
                    _ => None,
                },
                |m, owner| {
                    let media = x.clone();
                    m.separator().item(act(&ws, "Détacher de cet élément", move |this, w, cx| {
                        let (owner, media) = (owner.clone(), media.clone());
                        this.mutate(w, cx, move |g| {
                            let (what, from) = (g.record_label(&media), g.record_label(&owner));
                            g.detach_media(&owner, &media);
                            Ok((format!("« {what} » détaché de {from}"), None))
                        })
                    }))
                },
            )
            .map(|m| tail(m, &ws, &x, &from))
    })
}
