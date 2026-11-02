//! Menus contextuels (clic droit).
//!
//! Chaque menu agit sur l'élément cliqué, sans avoir à le sélectionner
//! d'abord, et appelle les mêmes fonctions que les commandes de la barre de
//! menus : mêmes libellés, même comportement. Un menu ne propose que ce qui
//! s'applique à l'élément (pas de « Père » quand il est déjà connu…).

use super::relatives::PickPurpose;
use super::*;
use gpui_kit::component::menu::{ContextMenu, ContextMenuExt, PopupMenu, PopupMenuItem};

/// Fonction de construction d'un menu, pour `.context_menu(…)`.
pub(super) type MenuBuilder = Rc<dyn Fn(PopupMenu, &mut Window, &mut Context<PopupMenu>) -> PopupMenu>;

/// D'où le menu est ouvert : quelques entrées en dépendent.
#[derive(Clone)]
pub(super) enum From {
    /// Une ligne de la liste de gauche (on peut y supprimer l'élément).
    List,
    /// Une case de l'arbre, un lien du panneau de détail…
    Link,
    /// Un membre affiché dans la fiche d'une famille.
    Member(String),
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
fn act(
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
        self.entry_form = None;
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

    fn copy_text(&mut self, text: String, what: &str, cx: &mut Context<Self>) {
        cx.write_to_clipboard(ClipboardItem::new_string(text));
        self.set_status(format!("{what} copié"), false);
        cx.notify();
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
            // Événement d'une union vu depuis la fiche d'un individu : ouvrir la famille.
            let family = read(&ws, cx, |this, g| {
                (g.family(&owner).is_some() && this.selected.as_deref() != Some(owner.as_str())).then(|| owner.clone())
            })
            .flatten();
            menu.item(act(&ws, "Modifier l'événement…", move |this, w, cx| {
                this.open_event_dialog(o1.clone(), Some(ix), w, cx)
            }))
            .separator()
            .item(act(&ws, "Rattacher des photos ou documents…", move |this, w, cx| {
                this.choose_files_for_event(o3.clone(), ix, w, cx)
            }))
            .item(act(&ws, "Lier un média existant…", move |this, w, cx| {
                this.open_picker(PickPurpose::MediaToEvent { owner: o4.clone(), ix }, w, cx)
            }))
            .separator()
            .when_some(family, |m, f| {
                m.item(act(&ws, "Ouvrir la famille", move |this, w, cx| this.goto(f.clone(), w, cx)))
            })
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
                let builder = if view == View::Places {
                    place_menu(ws.clone(), key, From::List)
                } else {
                    record_menu(ws.clone(), key, From::List)
                };
                return builder(menu, w, cx);
            }
            let Some((filtered, root)) = read(&ws, cx, |this, g| {
                (!this.filter.read(cx).value().trim().is_empty() || this.only_flagged, g.root().is_some())
            }) else {
                return menu;
            };
            let new = match view.kind() {
                Some(RecordKind::Individual) => Some("Nouvel individu"),
                Some(RecordKind::Family) => Some("Nouvelle famille"),
                Some(RecordKind::Source) => Some("Nouvelle source"),
                Some(RecordKind::Media) => Some("Nouveau média"),
                Some(RecordKind::Repository) => Some("Nouveau dépôt"),
                Some(RecordKind::SharedNote) => Some("Nouvelle note"),
                _ => None,
            };
            menu.when_some(new, |m, label| m.item(act(&ws, label, |this, w, cx| this.create_record(w, cx))))
                .when(view == View::Individuals && root, |m| {
                    m.item(act(&ws, "Aller à l'individu racine", |this, w, cx| this.goto_root(w, cx)))
                })
                .when(view == View::Sources, |m| {
                    m.item(act(&ws, "Saisir un acte…", |this, w, cx| this.open_entry(&EnterRecord, w, cx)))
                })
                .when(filtered, |m| {
                    m.separator().item(act(&ws, "Effacer la recherche", |this, w, cx| {
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
            menu.item(act(&ws, "Nouvelle personne", move |this, w, cx| this.add_relative(a.clone(), rel, w, cx))).item(
                act(&ws, "Personne existante…", move |this, w, cx| {
                    this.open_picker(PickPurpose::Relative { indi: b.clone(), rel }, w, cx)
                }),
            )
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
        .item(act(&ws, "Copier le nom du lieu", move |this, _, cx| this.copy_text(n3.clone(), "Nom du lieu", cx)))
        .when(matches!(from, From::List), |m| {
            m.separator()
                .item(act(&ws, "Supprimer le lieu…", move |this, w, cx| this.delete_from_list(n4.clone(), w, cx)))
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

/// Entrées communes en bas de menu : copier l'identifiant, supprimer (depuis la liste).
fn tail(menu: PopupMenu, ws: &Ws, xref: &str, from: &From) -> PopupMenu {
    let (a, b) = (xref.to_string(), xref.to_string());
    menu.separator()
        .item(act(ws, "Copier l'ID", move |this, _, cx| this.copy_id(&a, cx)))
        .when(matches!(from, From::List), |m| {
            m.item(act(ws, "Supprimer l'élément…", move |this, w, cx| this.delete_from_list(b.clone(), w, cx)))
        })
}

pub(super) fn person_menu(ws: Ws, x: String, from: From) -> MenuBuilder {
    Rc::new(move |menu, window, cx| {
        let Some((father, mother, is_root, name)) = read(&ws, cx, |_, g| {
            let (f, m) = g.parents(&x);
            let name = g.individual(&x).map(|i| i.display_name()).unwrap_or_default();
            (f.is_some(), m.is_some(), g.root().as_deref() == Some(x.as_str()), name)
        }) else {
            return menu;
        };
        let rels: Vec<(Relation, &'static str, &'static str)> = [
            (Relation::Father, "Père", "Père…"),
            (Relation::Mother, "Mère", "Mère…"),
            (Relation::Spouse, "Conjoint", "Conjoint…"),
            (Relation::Child, "Enfant", "Enfant…"),
            (Relation::Sibling, "Frère ou sœur", "Frère ou sœur…"),
        ]
        .into_iter()
        .filter(|(r, _, _)| match r {
            Relation::Father => !father,
            Relation::Mother => !mother,
            _ => true,
        })
        .collect();
        let rels2 = rels.clone();
        let (x1, x2, x3, x4, x5, x6, x7) =
            (x.clone(), x.clone(), x.clone(), x.clone(), x.clone(), x.clone(), x.clone());
        let (wa, wb) = (ws.clone(), ws.clone());
        menu.item(act(&ws, "Afficher dans l'arbre", move |this, w, cx| {
            this.open_in(x1.clone(), CenterTab::Tree, w, cx)
        }))
        .item(act(&ws, "Ouvrir la fiche", move |this, w, cx| this.open_in(x2.clone(), CenterTab::Form, w, cx)))
        .separator()
        .submenu("Ajouter un proche", window, cx, move |m, _, _| {
            rels.iter().fold(m, |m, (rel, label, _)| {
                let (x, rel) = (x3.clone(), *rel);
                m.item(act(&wa, *label, move |this, w, cx| this.add_relative(x.clone(), rel, w, cx)))
            })
        })
        .submenu("Lier une personne existante", window, cx, move |m, _, _| {
            rels2.iter().fold(m, |m, (rel, _, label)| {
                let (indi, rel) = (x4.clone(), *rel);
                m.item(act(&wb, *label, move |this, w, cx| {
                    this.open_picker(PickPurpose::Relative { indi: indi.clone(), rel }, w, cx)
                }))
            })
        })
        .item(act(&ws, "Ajouter un événement…", move |this, w, cx| this.open_event_dialog(x5.clone(), None, w, cx)))
        .item(act(&ws, "Rattacher des photos ou documents…", move |this, w, cx| this.choose_files(x6.clone(), w, cx)))
        .separator()
        .when(!is_root, |m| {
            let x = x7.clone();
            m.item(act(&ws, "Définir comme individu racine", move |this, _, cx| this.set_root(x.clone(), cx)))
        })
        .when_some(
            match &from {
                From::Member(fam) => Some(fam.clone()),
                _ => None,
            },
            |m, fam| {
                let x = x.clone();
                m.item(act(&ws, "Retirer de la famille", move |this, w, cx| this.unlink(x.clone(), fam.clone(), w, cx)))
            },
        )
        .when_some(
            match &from {
                From::Relative(indi, fam, label) => Some((indi.clone(), fam.clone(), *label)),
                _ => None,
            },
            |m, (indi, fam, label)| {
                m.item(act(&ws, label, move |this, w, cx| this.unlink(indi.clone(), fam.clone(), w, cx)))
            },
        )
        .item(act(&ws, "Copier le nom", move |this, _, cx| this.copy_text(name.clone(), "Nom", cx)))
        .map(|m| tail(m, &ws, &x, &from))
    })
}

fn family_menu(ws: Ws, x: String, from: From) -> MenuBuilder {
    Rc::new(move |menu, _, cx| {
        let Some(full) =
            read(&ws, cx, |_, g| g.family(&x).is_some_and(|f| f.husband().is_some() && f.wife().is_some()))
        else {
            return menu;
        };
        let (x1, x2, x3, x4, x5, x6) = (x.clone(), x.clone(), x.clone(), x.clone(), x.clone(), x.clone());
        menu.item(act(&ws, "Ouvrir la fiche", move |this, w, cx| this.open_in(x1.clone(), CenterTab::Form, w, cx)))
            .separator()
            .item(act(&ws, "Ajouter un événement…", move |this, w, cx| {
                this.open_event_dialog(x2.clone(), None, w, cx)
            }))
            .item(act(&ws, "Ajouter un nouvel enfant", move |this, w, cx| this.add_family_child(x3.clone(), w, cx)))
            .item(act(&ws, "Lier un enfant existant…", move |this, w, cx| {
                this.open_picker(PickPurpose::FamilyChild { fam: x4.clone() }, w, cx)
            }))
            .when(!full, |m| {
                m.item(act(&ws, "Lier un parent existant…", move |this, w, cx| {
                    this.open_picker(PickPurpose::FamilySpouse { fam: x5.clone() }, w, cx)
                }))
            })
            .item(act(&ws, "Rattacher des photos ou documents…", move |this, w, cx| {
                this.choose_files(x6.clone(), w, cx)
            }))
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
                    .item(act(&ws, "Saisir un acte…", move |this, w, cx| {
                        this.goto(x2.clone(), w, cx);
                        this.open_entry(&EnterRecord, w, cx)
                    }))
                    .item(act(&ws, "Rattacher des photos ou documents…", move |this, w, cx| {
                        this.choose_files(x3.clone(), w, cx)
                    }))
            })
            .when(kind == RecordKind::Media, |m| {
                m.separator()
                    .item(act(&ws, "Rattacher ce média à…", move |this, w, cx| {
                        this.open_picker(PickPurpose::MediaTo { media: x4.clone() }, w, cx)
                    }))
                    .when(photo, |m| {
                        m.item(act(&ws, "Identifier une personne sur la photo", move |this, w, cx| {
                            this.open_in(x5.clone(), CenterTab::Form, w, cx);
                            this.tag_person_on_photo(cx)
                        }))
                    })
                    .separator()
                    .when(marked.len() >= 2, |m| {
                        let label = format!("Fusionner les {} médias en un média multipage", marked.len());
                        let list = marked.clone();
                        m.item(act(&ws, label, move |this, w, cx| this.merge_media(list.clone(), w, cx)))
                    })
                    .when(marked.is_empty(), |m| {
                        m.item(act(&ws, "Fusionner avec un autre média…", move |this, w, cx| {
                            this.open_picker(PickPurpose::MergeInto { media: x6.clone() }, w, cx)
                        }))
                    })
                    .item(act(&ws, "Ajouter des pages…", move |this, w, cx| this.choose_pages(x7.clone(), w, cx)))
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
