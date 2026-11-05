//! Context menus (right-click).
//!
//! Each menu acts on the clicked item, without having to select it first, and calls the same
//! functions as the menu bar commands. Rules:
//! - the right-click does not create commands: an action on the item itself also exists in the menu
//!   bar, under the same label (`commands::label`);
//! - it only offers what applies to the item (no “Father” when already known…);
//! - exceptions: actions on a part of a record (an event, an attached media, a family member),
//!   which the menu bar cannot target, and the links to go there (“Open Record”, “Show Place”…).

use super::relatives::PickPurpose;
use super::*;
use crate::commands::label;
use crate::t;
use gpui_kit::component::menu::{ContextMenu, ContextMenuExt, PopupMenu, PopupMenuItem};

/// Builder function of a menu, for `.context_menu(…)`.
pub(super) type MenuBuilder = Rc<dyn Fn(PopupMenu, &mut Window, &mut Context<PopupMenu>) -> PopupMenu>;

/// Where the menu is opened from: a few entries depend on it.
#[derive(Clone)]
pub(super) enum From {
    /// A row of the left list (the item can be deleted there).
    List,
    /// A link of the detail panel, a cell of the fan chart…
    Link,
    /// A cell of the free tree.
    Tree,
    /// An image attached to the given record (thumbnails of the panel).
    Attached(String),
    /// A media of an event (owner record, position of the event).
    OnEvent(String, usize),
    /// A relative in the “Family” section of an individual: the (individual, family) link that
    /// “Detach” undoes, and the command’s label.
    Relative(String, String, SharedString),
}

type Ws = WeakEntity<Workspace>;

/// `.with_menu(builder)`: like `.context_menu(…)`, with a menu built in advance.
pub(super) trait WithMenu: ContextMenuExt + Sized {
    fn with_menu(self, builder: MenuBuilder) -> ContextMenu<Self> {
        self.context_menu(move |m, w, cx| builder(m, w, cx))
    }
}

impl<E: ContextMenuExt> WithMenu for E {}

/// An entry acting on the workspace. The action is deferred: the menu closes first (and gives back
/// the focus), then the window opened by the action (picker, event…) takes the focus.
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

/// Reads the workspace state when the menu opens.
fn read<R>(ws: &Ws, cx: &App, f: impl FnOnce(&Workspace, &Gedcom) -> R) -> Option<R> {
    let ws = ws.upgrade()?;
    let this = ws.read(cx);
    let g = &this.doc.as_ref()?.ged;
    Some(f(this, g))
}

impl Workspace {
    /// Opens a record in the given tab.
    pub(super) fn open_in(&mut self, xref: String, tab: CenterTab, window: &mut Window, cx: &mut Context<Self>) {
        self.act_editor = None;
        self.goto(xref, window, cx);
        self.tab = tab;
        cx.notify();
    }

    /// Delete from the list: the item and its confirmation are shown.
    fn delete_from_list(&mut self, key: String, window: &mut Window, cx: &mut Context<Self>) {
        self.select(key, window, cx);
        self.tab = CenterTab::Form;
        self.delete_selected(window, cx);
    }

    fn ws(cx: &Context<Self>) -> Ws {
        cx.entity().downgrade()
    }

    /// Menu of any record, according to its type.
    pub(super) fn record_menu(&self, xref: &str, from: From, cx: &Context<Self>) -> MenuBuilder {
        record_menu(Self::ws(cx), xref.to_string(), from)
    }

    pub(super) fn person_menu(&self, xref: &str, from: From, cx: &Context<Self>) -> MenuBuilder {
        person_menu(Self::ws(cx), xref.to_string(), from)
    }

    /// Menu of a record entry, in its source’s record.
    pub(super) fn act_menu(&self, source: &str, key: ActKey, cx: &Context<Self>) -> MenuBuilder {
        let ws = Self::ws(cx);
        let source = source.to_string();
        Rc::new(move |menu, _, _| {
            let (s1, k1, s2, k2) = (source.clone(), key.clone(), source.clone(), key.clone());
            menu.item(act(&ws, t!("context.edit_act"), move |this, w, cx| {
                this.open_act(s1.clone(), Some(k1.clone()), w, cx)
            }))
            .separator()
            .item(act(&ws, t!("context.delete_act"), move |this, w, cx| this.delete_act(s2.clone(), k2.clone(), w, cx)))
        })
    }

    /// Menu of an event of the detail panel.
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
            let (o1, o2, o3) = (owner.clone(), owner.clone(), owner.clone());
            menu.item(act(&ws, t!("context.edit_event"), move |this, w, cx| {
                this.open_event_dialog(o1.clone(), Some(ix), w, cx)
            }))
            .separator()
            .item(act(&ws, label::media_gallery(), move |this, w, cx| {
                this.open_gallery(GalleryTarget::Event { owner: o3.clone(), ix }, w, cx)
            }))
            .separator()
            .when_some(place, |m, p| {
                m.item(act(&ws, t!("context.show_place"), move |this, w, cx| this.goto_place(p.clone(), w, cx)))
            })
            .map(|m| {
                sources.into_iter().fold(m, |m, (s, title)| {
                    m.item(act(&ws, t!("context.open_source", title = ellipsize(&title, 40)), move |this, w, cx| {
                        this.goto(s.clone(), w, cx)
                    }))
                })
            })
            .separator()
            .item(act(&ws, t!("context.delete_event"), move |this, w, cx| this.delete_event(o2.clone(), ix, w, cx)))
        })
    }

    /// Menu of the list: the targeted item, or below the rows, create an item.
    pub(super) fn list_menu(&self, cx: &Context<Self>) -> MenuBuilder {
        let ws = Self::ws(cx);
        let view = self.view;
        Rc::new(move |menu, w, cx| {
            // Right-click on a row: the menu of that item.
            if let Some(key) = read(&ws, cx, |this, _| this.menu_row.take()).flatten() {
                // Categories of the Quality page: no menu.
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

    /// Empty cell of the tree (unknown parent…): new person or existing person.
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
        menu.item(act(&ws, t!("context.show_on_map"), move |this, w, cx| {
            this.goto_place(n1.clone(), w, cx);
            this.tab = CenterTab::Map;
        }))
        .item(act(&ws, t!("context.open_place"), move |this, w, cx| {
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

/// Menu of a record, its type read on opening (list rows).
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

/// Common entries at the bottom of the menu, in the Edit menu’s order: copy the name, the
/// identifier, delete (from the list).
fn tail(menu: PopupMenu, ws: &Ws, xref: &str, from: &From, cx: &App) -> PopupMenu {
    let (a, b, c, d) = (xref.to_string(), xref.to_string(), xref.to_string(), xref.to_string());
    let history = read(ws, cx, |this, _| this.has_version_history()).unwrap_or(false);
    menu.separator()
        .when(history, |m| {
            m.item(act(ws, label::record_history(), move |this, w, cx| this.open_record_history(d.clone(), w, cx)))
        })
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
        let (x7, x8) = (x.clone(), x.clone());
        let (wa, wb) = (ws.clone(), ws.clone());
        let sites = read(&ws, cx, |_, g| g.search_sites()).unwrap_or_default();
        let (ws_sites, x_sites) = (ws.clone(), x.clone());
        let menu = menu
            .item(act(&ws, label::center_tree(), move |this, w, cx| {
                this.act_editor = None;
                this.goto(x1.clone(), w, cx);
                this.center_chart_on(x1.clone(), w, cx)
            }))
            .item(act(&ws, t!("context.open_details"), move |this, w, cx| {
                this.open_in(x2.clone(), CenterTab::Form, w, cx)
            }))
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
            .item(act(&ws, label::media_gallery(), move |this, w, cx| {
                this.open_gallery(GalleryTarget::Record(x6.clone()), w, cx)
            }));
        let menu = if sites.is_empty() {
            menu
        } else {
            menu.submenu(label::search_web(), window, cx, move |m, _, _| {
                sites.iter().enumerate().fold(m, |m, (index, site)| {
                    let x = x_sites.clone();
                    m.item(act(&ws_sites, site.name.clone(), move |this, _, cx| this.search_web(index, &x, cx)))
                })
            })
        };
        let menu = menu
            .separator()
            .item(act(&ws, t!("cmd.merge_with_person"), move |this, w, cx| {
                this.open_picker(PickPurpose::MergeWith { indi: x7.clone() }, w, cx)
            }))
            .item(act(&ws, t!("cmd.find_duplicates"), move |this, _, cx| this.find_duplicates_of(&x8, cx)));
        menu.when_some(
            match &from {
                From::Relative(indi, fam, label) => Some((indi.clone(), fam.clone(), label.clone())),
                _ => None,
            },
            |m, (indi, fam, label)| {
                m.separator().item(act(&ws, label, move |this, w, cx| this.unlink(indi.clone(), fam.clone(), w, cx)))
            },
        )
        .map(|m| tail(m, &ws, &x, &from, cx))
    })
}

/// Family cited in a link (source, media…): it opens on the record of a spouse.
fn family_menu(ws: Ws, x: String, from: From) -> MenuBuilder {
    Rc::new(move |menu, _, cx| {
        let x1 = x.clone();
        menu.item(act(&ws, t!("context.open_details"), move |this, w, cx| this.goto(x1.clone(), w, cx)))
            .map(|m| tail(m, &ws, &x, &from, cx))
    })
}

/// Sources, media, repositories, notes.
fn other_menu(ws: Ws, x: String, kind: RecordKind, from: From) -> MenuBuilder {
    Rc::new(move |menu, _, cx| {
        let Some((photo, marked, portrait_of)) = read(&ws, cx, |this, g| {
            // Media selected together (⌘ / Shift + click) including this one: they can be merged.
            let marked = if this.media_marked.len() >= 2 && this.media_marked.contains(&x) {
                this.media_marked.clone()
            } else {
                vec![]
            };
            // Photo (or image without medium) of an individual of whom it is not already the
            // portrait.
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
        menu.item(act(&ws, t!("context.open_details"), move |this, w, cx| {
            this.open_in(x1.clone(), CenterTab::Form, w, cx)
        }))
        .when(kind == RecordKind::Source, |m| {
            m.separator()
                .item(act(&ws, label::enter_act(), move |this, w, cx| {
                    this.goto(x2.clone(), w, cx);
                    this.enter_act(w, cx)
                }))
                .item(act(&ws, label::media_gallery(), move |this, w, cx| {
                    this.open_gallery(GalleryTarget::Record(x3.clone()), w, cx)
                }))
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
            m.separator().item(act(&ws, t!("context.set_portrait"), move |this, w, cx| {
                let (owner, media) = (owner.clone(), media.clone());
                this.mutate(w, cx, move |g| {
                    g.set_portrait(&owner, &media);
                    Ok((t!("context.portrait_set", name = g.record_label(&owner)).to_string(), None))
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
                m.separator().item(act(&ws, t!("context.detach_event"), move |this, w, cx| {
                    let (owner, media) = (owner.clone(), media.clone());
                    this.mutate(w, cx, move |g| {
                        let what = g.record_label(&media);
                        let from = g.event_title(&owner, ix).unwrap_or_default().to_lowercase();
                        g.detach_media_from_event(&owner, ix, &media)?;
                        Ok((t!("context.detached_event", what = what, from = from).to_string(), None))
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
                m.separator().item(act(&ws, t!("context.detach"), move |this, w, cx| {
                    let (owner, media) = (owner.clone(), media.clone());
                    this.mutate(w, cx, move |g| {
                        let (what, from) = (g.record_label(&media), g.record_label(&owner));
                        g.detach_media(&owner, &media);
                        Ok((t!("context.detached", what = what, from = from).to_string(), None))
                    })
                }))
            },
        )
        .map(|m| tail(m, &ws, &x, &from, cx))
    })
}
