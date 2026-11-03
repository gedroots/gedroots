//! Edit › Record History…: every change of one record, read from the version history, like the
//! change log of a person on FamilySearch or the history of a page in Notion.
//!
//! Newest first, one row per version that changed the record, with the facts it touched; the
//! selected change shows each field before and after. Restore puts the record back as it was after
//! that change, as one undo point; the rest of the tree is not touched.

use super::version_history::date_time_label;
use super::*;
use gedcom::record_diff::{self, FieldChange};
use gedcom::versions::{Kind, Store, Version};

pub(super) struct RecordHistory {
    xref: String,
    name: String,
    /// Newest first.
    entries: Vec<Entry>,
    loaded: bool,
    selected: usize,
    error: Option<String>,
}

struct Entry {
    version: Version,
    /// The record after this change (`None`: removed).
    after: Option<Node>,
    fields: Vec<FieldChange>,
    /// First version that holds the record.
    first: bool,
}

/// What made a change, in a few words.
fn what(e: &Entry) -> String {
    match (e.first, e.version.kind) {
        (true, Kind::Opened) => t!("versions.opened").into(),
        (true, _) => t!("record_history.added").into(),
        (_, Kind::Agent) => e.version.label.clone().unwrap_or_else(|| t!("versions.agent").into()),
        _ if e.after.is_none() => t!("record_history.removed").into(),
        _ => record_diff::summary(&e.fields),
    }
}

impl Workspace {
    /// Edit › Record History… applies: a record is open in a tree with a version history.
    pub(super) fn can_show_record_history(&self) -> bool {
        self.has_version_history()
            && self.selected.as_ref().is_some_and(|x| self.doc.as_ref().is_some_and(|d| d.ged.get(x).is_some()))
    }

    /// Edit › Record History…, on the selected record.
    pub(super) fn open_record_history_of_selection(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(x) = self.selected.clone() {
            self.open_record_history(x, window, cx);
        }
    }

    pub(super) fn open_record_history(&mut self, xref: String, window: &mut Window, cx: &mut Context<Self>) {
        let Some(doc) = &mut self.doc else { return };
        let Some(path) = doc.own_file().map(std::path::Path::to_path_buf) else { return };
        if doc.ged.get(&xref).is_none() {
            return;
        }
        // The last changes, not recorded yet, are part of it.
        doc.flush();
        let store = Store::for_file(&path, doc.ged.tree_id());
        let ged = doc.ged.clone();
        let name = ged.record_label(&xref);
        self.record_history = Some(RecordHistory {
            xref: xref.clone(),
            name,
            entries: Vec::new(),
            loaded: false,
            selected: 0,
            error: None,
        });
        cx.notify();
        cx.spawn_in(window, async move |this, cx| {
            let read = cx
                .background_spawn(async move {
                    let changes = store.record_history(&xref)?;
                    let label = |x: &str| match ged.get(x) {
                        Some(_) => ged.record_label(x),
                        None => t!("record_history.missing").into(),
                    };
                    let mut entries: Vec<Entry> = Vec::new();
                    for (i, c) in changes.into_iter().enumerate() {
                        let fields = record_diff::field_changes(c.before.as_ref(), c.after.as_ref(), &label);
                        // Only change dates moved (a save that touched the record).
                        if i > 0 && fields.is_empty() {
                            continue;
                        }
                        entries.push(Entry { version: c.version, after: c.after, fields, first: i == 0 });
                    }
                    entries.reverse();
                    Ok::<_, String>(entries)
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                let Some(h) = &mut this.record_history else { return };
                match read {
                    Ok(e) => h.entries = e,
                    Err(e) => h.error = Some(e),
                }
                h.loaded = true;
                cx.notify();
            });
        })
        .detach();
    }

    pub(super) fn close_record_history(&mut self, cx: &mut Context<Self>) -> bool {
        let open = self.record_history.take().is_some();
        cx.notify();
        open
    }

    pub(super) fn step_record_change(&mut self, delta: isize, cx: &mut Context<Self>) {
        let Some(h) = &mut self.record_history else { return };
        let n = h.entries.len() as isize;
        if n > 0 {
            h.selected = (h.selected as isize + delta).clamp(0, n - 1) as usize;
            cx.notify();
        }
    }

    /// The record as it was after the selected change, if it differs from now.
    fn record_to_restore(&self) -> Option<Node> {
        let h = self.record_history.as_ref()?;
        let after = h.entries.get(h.selected)?.after.clone()?;
        let now = self.doc.as_ref()?.ged.get(&h.xref)?;
        (!record_diff::field_changes(Some(now), Some(&after), &|x| x.to_string()).is_empty()).then_some(after)
    }

    /// Restore: the record as it was after the selected change (one undo point).
    fn restore_record(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(node) = self.record_to_restore() else { return };
        let Some(h) = self.record_history.take() else { return };
        let when = h.entries.get(h.selected).map(|e| date_time_label(e.version.time)).unwrap_or_default();
        let xref = h.xref.clone();
        let msg = t!("record_history.restored", name = h.name, from = when).into_owned();
        self.edit_ged(move |g| g.replace(&xref, node).is_ok(), msg, window, cx);
        cx.notify();
    }

    /// Show in Version History: the whole version of the selected change.
    fn show_change_in_versions(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(h) = self.record_history.take() else { return };
        let id = h.entries.get(h.selected).map(|e| e.version.id.clone());
        self.open_version_history_at(id, window, cx);
    }

    pub(super) fn render_record_history(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let h = self.record_history.as_ref()?;

        // List: one row per change, newest first.
        let rows = h.entries.iter().enumerate().map(|(i, e)| {
            let on = i == h.selected;
            let badge = match e.version.kind {
                Kind::Agent => e.version.author.clone().or_else(|| Some(t!("versions.agent_badge").into())),
                _ => None,
            };
            h_flex()
                .id(SharedString::from(format!("record-change-{i}")))
                .mx_2()
                .px_2()
                .py_1()
                .gap_3()
                .items_start()
                .rounded(theme::RADIUS_SM)
                .cursor_pointer()
                .when(on, |d| d.bg(theme::accent_soft()))
                .when(!on, |d| d.hover(|d| d.bg(theme::hover())))
                .child(
                    v_flex()
                        .flex_1()
                        .min_w_0()
                        .gap_0p5()
                        .child(
                            h_flex()
                                .gap_2()
                                .min_w_0()
                                .child(
                                    div()
                                        .min_w_0()
                                        .overflow_hidden()
                                        .whitespace_nowrap()
                                        .text_ellipsis()
                                        .font_weight(FontWeight::MEDIUM)
                                        .when(on, |d| d.text_color(theme::accent()))
                                        .child(what(e)),
                                )
                                .children(badge.map(|b| {
                                    div()
                                        .flex_none()
                                        .px_1()
                                        .rounded(px(4.))
                                        .border_1()
                                        .border_color(theme::border_strong())
                                        .text_xs()
                                        .text_color(theme::muted())
                                        .child(b)
                                })),
                        )
                        .child(mono(date_time_label(e.version.time))),
                )
                .on_click(cx.listener(move |this, _, _, cx| {
                    if let Some(h) = &mut this.record_history {
                        h.selected = i;
                        cx.notify();
                    }
                }))
        });
        let list = div()
            .id("record-changes")
            .w(px(300.))
            .flex_none()
            .h_full()
            .py_2()
            .overflow_y_scroll()
            .bg(theme::panel())
            .border_r_1()
            .border_color(theme::border())
            .children(rows);

        // The selected change: each field before and after.
        let detail = h.entries.get(h.selected).map(|e| {
            let head = v_flex()
                .gap_0p5()
                .child(div().text_xl().font_weight(FontWeight::SEMIBOLD).child(what(e)))
                .child(div().text_color(theme::muted()).child(match (&e.version.author, e.version.kind) {
                    (Some(a), Kind::Agent) => format!("{} · {a}", date_time_label(e.version.time)),
                    (_, Kind::Outside) => format!("{} · {}", date_time_label(e.version.time), t!("versions.outside")),
                    _ => date_time_label(e.version.time),
                }));
            let cell = |s: Option<&String>, muted: bool| {
                div()
                    .flex_1()
                    .min_w_0()
                    .px_3()
                    .py_2()
                    .when(muted, |d| d.text_color(theme::muted()))
                    .child(s.cloned().unwrap_or_else(|| "—".into()))
            };
            let header = h_flex()
                .bg(theme::panel())
                .border_b_1()
                .border_color(theme::border())
                .text_xs()
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(theme::muted())
                .child(div().w(px(180.)).flex_none().px_3().py_1().child(t!("record_history.field")))
                .when(!e.first, |d| d.child(div().flex_1().px_3().py_1().child(t!("record_history.before"))))
                .child(div().flex_1().px_3().py_1().child(match e.first {
                    // The record as it first appeared: its fields, nothing before.
                    true => t!("record_history.value"),
                    false => t!("record_history.after"),
                }));
            let lines = e.fields.iter().map(|f| {
                h_flex()
                    .items_start()
                    .border_b_1()
                    .border_color(theme::border())
                    .child(
                        div().w(px(180.)).flex_none().px_3().py_2().text_color(theme::muted()).child(f.label.clone()),
                    )
                    .when(!e.first, |d| d.child(cell(f.before.as_ref(), true)))
                    .child(cell(f.after.as_ref(), false))
            });
            let table = (!e.fields.is_empty()).then(|| {
                v_flex()
                    .rounded(theme::RADIUS_SM)
                    .border_1()
                    .border_color(theme::border())
                    .overflow_hidden()
                    .child(header)
                    .children(lines)
            });
            v_flex()
                .id("record-change")
                .flex_1()
                .min_w_0()
                .h_full()
                .px_6()
                .py_4()
                .gap_4()
                .overflow_y_scroll()
                .child(head)
                .children(table)
        });
        let empty = (h.loaded && h.entries.is_empty()).then(|| {
            v_flex()
                .flex_1()
                .items_center()
                .justify_center()
                .gap_2()
                .text_color(theme::muted())
                .child(icon(IconName::RotateCcwClock).size(px(24.)))
                .child(h.error.clone().unwrap_or_else(|| t!("record_history.empty").into()))
        });
        let main = h_flex().flex_1().min_h_0().child(list).children(detail).children(empty);

        // Buttons: Show in Version History apart; Done and Restore in the platform's order.
        let restore = Button::new("record-restore")
            .primary()
            .label(t!("record_history.restore"))
            .disabled(self.record_to_restore().is_none())
            .on_click(cx.listener(|this, _, w, cx| this.restore_record(w, cx)));
        let done =
            Button::new("record-history-done").label(t!("versions.done")).on_click(cx.listener(|this, _, _, cx| {
                this.close_record_history(cx);
            }));
        let buttons = if cfg!(target_os = "windows") {
            [restore.into_any_element(), done.into_any_element()]
        } else {
            [done.into_any_element(), restore.into_any_element()]
        };
        let show = Button::new("record-show-version")
            .label(t!("record_history.show_version"))
            .disabled(h.entries.is_empty())
            .on_click(cx.listener(|this, _, w, cx| this.show_change_in_versions(w, cx)));
        let foot = h_flex()
            .px_6()
            .py_4()
            .gap_2()
            .items_center()
            .border_t_1()
            .border_color(theme::border())
            .child(show)
            .child(div().flex_1())
            .children(buttons);
        let head =
            h_flex().px_6().py_3().border_b_1().border_color(theme::border()).child(
                div().font_weight(FontWeight::SEMIBOLD).child(t!("record_history.title", name = h.name.clone())),
            );

        Some(
            div()
                .absolute()
                .inset_0()
                .flex()
                .justify_center()
                .items_start()
                .pt(px(60.))
                .bg(theme::scrim())
                .child(
                    v_flex()
                        .id("record-history")
                        .occlude()
                        .w(px(920.))
                        .max_w(relative(0.95))
                        .h(px(620.))
                        .rounded(theme::RADIUS_LG)
                        .border_1()
                        .border_color(theme::border_strong())
                        .bg(theme::bg())
                        .shadow_lg()
                        .overflow_hidden()
                        .on_key_down(cx.listener(|this, ev: &KeyDownEvent, _, cx| {
                            if ev.keystroke.key == "escape" {
                                cx.stop_propagation();
                                this.close_record_history(cx);
                            }
                        }))
                        .child(head)
                        .child(main)
                        .child(foot),
                )
                .into_any_element(),
        )
    }
}
