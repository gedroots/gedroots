//! Saved searches: “Searches” section of the sidebar.
//!
//! A search is saved with the section’s “+” or View › Save Search: it takes the name of its tokens,
//! which can be retyped right away (↵ confirms, Esc keeps the suggested name). It lives in the
//! `.ged` (`HEAD._GR_FIND`) and shows its number of results.

use super::*;
use gedcom::saved::{SavedSearch, default_name};
use rust_i18n::t;

/// Renaming in progress in the sidebar.
pub(super) struct SavedRename {
    index: usize,
    input: Entity<InputState>,
    _sub: Subscription,
}

fn view_of(scope: Scope) -> View {
    match scope {
        Scope::Individuals => View::Individuals,
        Scope::Sources => View::Sources,
        Scope::Places => View::Places,
        Scope::Media => View::Media,
    }
}

impl Workspace {
    /// Recomputes the saved searches and their number of results.
    pub(super) fn refresh_saved(&mut self) {
        let Some(doc) = &self.doc else {
            self.saved = Rc::new(Vec::new());
            return;
        };
        let list = doc.ged.saved_searches();
        if list.iter().any(|s| s.scope == Scope::Media) {
            self.ensure_media_index();
        }
        let Some(doc) = &self.doc else { return };
        let index = self.media_index.as_deref();
        self.saved = Rc::new(
            list.into_iter()
                .map(|s| {
                    let n = doc.ged.search(s.scope, &s.query, index, s.media_type.as_deref()).len();
                    (s, n)
                })
                .collect(),
        );
    }

    /// Displayed search: zone, confirmed criteria, media type. `None` without a search.
    fn current_search(&self, cx: &App) -> Option<(Scope, String, Option<String>)> {
        // The focus of the leads is not a saved search.
        let scope = self.view.scope().filter(|_| self.view != View::Research)?;
        let query = Self::committed_query(&self.filter.read(cx).value()).trim().to_string();
        let media_type = self.active_media_type();
        (!query.is_empty() || media_type.is_some()).then_some((scope, query, media_type))
    }

    /// Position of the displayed saved search, if there is one.
    pub(super) fn active_saved(&self, cx: &App) -> Option<usize> {
        let (scope, query, media_type) = self.current_search(cx)?;
        self.saved.iter().position(|(s, _)| s.same(scope, &query, media_type.as_deref()))
    }

    /// The displayed search can be saved (and is not already).
    pub(super) fn can_save_search(&self, cx: &App) -> bool {
        self.current_search(cx).is_some() && self.active_saved(cx).is_none()
    }

    pub(super) fn save_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let (Some((scope, query, media_type)), Some(doc)) = (self.current_search(cx), &self.doc) else { return };
        if self.active_saved(cx).is_some() {
            return;
        }
        let name = default_name(&doc.ged, scope, &query, media_type.as_deref());
        let s = SavedSearch { name, scope, query, media_type };
        let mut index = 0;
        self.mutate(window, cx, |g| {
            index = g.save_search(&s);
            Ok((t!("saved.saved").into(), None))
        });
        self.refresh_saved();
        self.start_rename_search(index, window, cx);
    }

    pub(super) fn open_saved(&mut self, i: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some((s, _)) = self.saved.get(i).cloned() else { return };
        self.show_view(view_of(s.scope), window, cx);
        self.media_type = s.media_type.clone();
        let q = if s.query.is_empty() { String::new() } else { format!("{} ", s.query) };
        self.set_query(q, window, cx);
        self.suggest_closed = true;
        cx.notify();
    }

    /// Rename field in place of the row, text selected.
    pub(super) fn start_rename_search(&mut self, i: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some((s, _)) = self.saved.get(i) else { return };
        let name = s.name.clone();
        let input = cx.new(|cx| InputState::new(window, cx).default_value(name));
        let sub = cx.subscribe_in(&input, window, |this, _, ev: &InputEvent, window, cx| match ev {
            InputEvent::PressEnter { .. } => this.commit_rename_search(window, cx),
            InputEvent::Blur => this.commit_rename_search(window, cx),
            _ => {}
        });
        input.update(cx, |f, cx| {
            f.focus(window, cx);
            f.select_all(window, cx);
        });
        self.saved_rename = Some(SavedRename { index: i, input, _sub: sub });
        cx.notify();
    }

    fn commit_rename_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(r) = self.saved_rename.take() else { return };
        let name = r.input.read(cx).value().trim().to_string();
        let unchanged = self.saved.get(r.index).is_none_or(|(s, _)| s.name == name);
        if !name.is_empty() && !unchanged {
            self.mutate(window, cx, |g| {
                g.rename_search(r.index, &name);
                Ok((t!("saved.renamed").into(), None))
            });
            self.refresh_saved();
        }
        cx.notify();
    }

    fn cancel_rename_search(&mut self, cx: &mut Context<Self>) {
        self.saved_rename = None;
        cx.notify();
    }

    pub(super) fn delete_saved(&mut self, i: usize, window: &mut Window, cx: &mut Context<Self>) {
        self.saved_rename = None;
        self.mutate(window, cx, |g| {
            let s = g.delete_search(i).ok_or_else(|| t!("saved.not_found").to_string())?;
            Ok((t!("saved.deleted", name = s.name).to_string(), None))
        });
        self.refresh_saved();
        cx.notify();
    }

    /// Menu of a saved search (right-click).
    fn saved_menu(&self, i: usize, cx: &Context<Self>) -> menus::MenuBuilder {
        let ws = cx.entity().downgrade();
        Rc::new(move |menu, _, _| {
            menu.item(menus::act(&ws, format!("{}…", label::rename_search()), move |this, w, cx| {
                this.start_rename_search(i, w, cx)
            }))
            .separator()
            .item(menus::act(&ws, label::delete_search(), move |this, w, cx| this.delete_saved(i, w, cx)))
        })
    }

    /// “Searches” section: hidden with no saved search and no search to save.
    pub(super) fn render_saved(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let can_save = self.can_save_search(cx);
        if self.saved.is_empty() && !can_save {
            return None;
        }
        let active = self.active_saved(cx);
        let header = h_flex()
            .mt_4()
            .px_2()
            .h(px(22.))
            .items_center()
            .justify_between()
            .child(label_caps(t!("saved.title")))
            .when(can_save, |d| {
                d.child(
                    add::plus_button("save-search")
                        .tooltip(label::save_search())
                        .on_click(cx.listener(|this, _, w, cx| this.save_search(w, cx))),
                )
            });
        let mut list = v_flex().gap_0p5().child(header);
        for (i, (s, n)) in self.saved.iter().enumerate() {
            let icon = view_of(s.scope).icon();
            if let Some(r) = self.saved_rename.as_ref().filter(|r| r.index == i) {
                list = list.child(
                    h_flex()
                        .px_1()
                        .gap_1()
                        .items_center()
                        .on_key_down(cx.listener(|this, ev: &KeyDownEvent, _, cx| {
                            if ev.keystroke.key == "escape" {
                                this.cancel_rename_search(cx);
                            }
                        }))
                        .child(Input::new(&r.input).small()),
                );
                continue;
            }
            let on = active == Some(i);
            list = list.child(
                h_flex()
                    .id(("saved-search", i))
                    .px_2()
                    .py_1()
                    .gap_2()
                    .items_center()
                    .rounded(theme::RADIUS_SM)
                    .cursor_pointer()
                    .when(on, |d| d.bg(theme::accent_soft()).font_weight(FontWeight::SEMIBOLD))
                    .when(!on, |d| d.hover(|d| d.bg(theme::hover())))
                    .child(Icon::new(icon).size(px(16.)).text_color(if on { theme::accent() } else { theme::muted() }))
                    .child(div().flex_1().min_w_0().truncate().child(s.name.clone()))
                    .child(div().text_xs().text_color(theme::muted()).child(group_thousands(*n)))
                    .on_click(cx.listener(move |this, ev: &ClickEvent, w, cx| {
                        if ev.click_count() >= 2 {
                            this.start_rename_search(i, w, cx);
                        } else {
                            this.open_saved(i, w, cx);
                        }
                    }))
                    .with_menu(self.saved_menu(i, cx)),
            );
        }
        Some(list.into_any_element())
    }
}
