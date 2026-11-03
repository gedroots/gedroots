//! Version history of the open tree: File › Revert To, File › Save Version….
//!
//! The window lists the versions of the history folder beside the file (`gedcom::versions`),
//! newest first and grouped by day, like the version browsers of Google Docs, Figma or Pages. The
//! selected version shows the records it added, changed and removed since the one before it.
//! Restoring never overwrites anything: the tree as it was becomes the current tree, as one undo
//! point, and is recorded as a new version; the current one stays in the history.

use super::add::AddDialog;
use super::*;
use chrono::{DateTime, Datelike, Local, TimeZone};
use gedcom::versions::{Changes, Kind, Store, Version};
use std::collections::HashMap;

/// Records listed per section; the others are counted.
const LIST_MAX: usize = 100;

pub(super) struct VersionHistory {
    store: Store,
    /// Newest first, with what each changed since the one before it (`None` for the oldest).
    versions: Vec<(Version, Option<Changes>)>,
    loaded: bool,
    selected: usize,
    detail: Option<Detail>,
    /// The latest version is the current tree: restoring it would change nothing.
    latest_is_current: bool,
}

/// Records of the selected version: names and kinds, read from the version (from the one before
/// it for the removed records).
struct Detail {
    id: String,
    labels: HashMap<String, (String, Option<RecordKind>)>,
    error: Option<String>,
}

/// Time of a version in the local time zone.
fn local(time: u64) -> DateTime<Local> {
    Local.timestamp_opt(time as i64, 0).single().unwrap_or_else(Local::now)
}

/// “3 November 2026” in the interface language.
fn date_label(t: &DateTime<Local>) -> String {
    const M: [&str; 12] = ["JAN", "FEB", "MAR", "APR", "MAY", "JUN", "JUL", "AUG", "SEP", "OCT", "NOV", "DEC"];
    let m = M[(t.month0() as usize).min(11)];
    gedcom::date::Date::parse(&format!("{} {m} {}", t.day(), t.year())).display()
}

/// Heading of a day in the list: Today, Yesterday, or the date.
fn day_label(t: &DateTime<Local>, today: chrono::NaiveDate) -> String {
    let d = t.date_naive();
    if d == today {
        t!("versions.today").into()
    } else if today.pred_opt() == Some(d) {
        t!("versions.yesterday").into()
    } else {
        date_label(t)
    }
}

/// “3 November 2026, 15:58”.
pub(super) fn date_time_label(time: u64) -> String {
    let t = local(time);
    t!("versions.date_time", date = date_label(&t), time = t.format("%H:%M").to_string()).into()
}

/// Name of a version: the name given by the user, otherwise what made it.
fn title(v: &Version) -> String {
    match (v.kind, v.label.as_deref()) {
        (Kind::Named | Kind::RestorePoint | Kind::Agent, Some(l)) => l.to_string(),
        (Kind::Restored, Some(from)) => restored_from(from).unwrap_or_else(|| t!("versions.restored").into()),
        (Kind::Opened, _) => t!("versions.opened").into(),
        (Kind::Saved | Kind::Named, _) => t!("versions.saved").into(),
        (Kind::Agent, _) => t!("versions.agent").into(),
        (Kind::RestorePoint, _) => t!("versions.restore_point").into(),
        (Kind::Outside, _) => t!("versions.outside").into(),
        (Kind::Restored, None) => t!("versions.restored").into(),
    }
}

/// Title in the window: the latest save is “Current Tree” when nothing changed since.
fn current_title(h: &VersionHistory, i: usize, v: &Version) -> String {
    match i == 0 && h.latest_is_current && v.kind == Kind::Saved {
        true => t!("versions.current").into(),
        false => title(v),
    }
}

/// Label of a restored version: the version it brought back, by its id, or the tree as opened.
const OPENED: &str = "opened";

/// Label of a version that took back the changes of another: `undo <id>`.
const UNDO: &str = "undo ";

/// Time of a version, from its id.
fn time_of(id: &str) -> Option<u64> {
    Some(id.split('-').next()?.parse::<u64>().ok()? / 1000)
}

/// What a restored version brought back, from its label.
fn restored_from(label: &str) -> Option<String> {
    if label == OPENED {
        return Some(t!("versions.last_opened").into());
    }
    if let Some(id) = label.strip_prefix(UNDO) {
        return Some(t!("versions.undone_from", from = date_time_label(time_of(id)?)).into());
    }
    Some(t!("versions.restored_from", from = date_time_label(time_of(label)?)).into())
}

/// “2 added · 3 changed”, or the number of records of the oldest version.
fn summary(v: &Version, changes: Option<&Changes>) -> String {
    let Some(c) = changes else { return crate::tn!("versions.records", v.xrefs().count()).into() };
    let mut parts: Vec<String> = Vec::new();
    if !c.added.is_empty() {
        parts.push(crate::tn!("versions.added", c.added.len()).into());
    }
    if !c.changed.is_empty() {
        parts.push(crate::tn!("versions.changed", c.changed.len()).into());
    }
    if !c.removed.is_empty() {
        parts.push(crate::tn!("versions.removed", c.removed.len()).into());
    }
    if parts.is_empty() {
        return t!("versions.no_change").into();
    }
    parts.join(" · ")
}

/// Kind badge of a version that was not made by a plain save.
fn badge(v: &Version) -> Option<String> {
    match v.kind {
        Kind::Named => Some(t!("versions.named").into()),
        Kind::Agent => Some(v.author.clone().unwrap_or_else(|| t!("versions.agent_badge").into())),
        Kind::RestorePoint if v.label.is_some() => Some(t!("versions.restore_point").into()),
        Kind::Outside | Kind::Restored | Kind::Opened | Kind::Saved | Kind::RestorePoint => None,
    }
}

impl Workspace {
    /// The open tree's file, when it has a version history (its own `.ged`).
    pub(super) fn has_version_history(&self) -> bool {
        self.doc.as_ref().is_some_and(|d| d.own_file().is_some())
    }

    /// Revert To › Last Opened applies: records changed since the tree was opened (saving alone
    /// changes the header's date).
    pub(super) fn changed_since_opened(&self) -> bool {
        self.doc.as_ref().is_some_and(|d| {
            d.own_file().is_some()
                && (d.ged.records.len() != d.opened.records.len()
                    || d.ged
                        .records
                        .iter()
                        .zip(&d.opened.records)
                        .any(|(a, b)| !std::sync::Arc::ptr_eq(a, b) && a != b))
        })
    }

    /// File › Revert To › Browse All Versions…. The changes not recorded yet become the latest
    /// version first, so that the list starts with the current tree.
    pub(super) fn open_version_history(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.open_version_history_at(None, window, cx);
    }

    /// The version history, on the version `id` (the latest one when `None`).
    pub(super) fn open_version_history_at(&mut self, id: Option<String>, window: &mut Window, cx: &mut Context<Self>) {
        let Some(doc) = &mut self.doc else { return };
        let Some(path) = doc.own_file().map(std::path::Path::to_path_buf) else { return };
        doc.flush();
        if matches!(self.add_dialog, Some(AddDialog::SaveVersion(_))) {
            self.add_dialog = None;
        }
        let Some(doc) = &self.doc else { return };
        let store = Store::for_file(&path, doc.ged.tree_id());
        let ged = doc.ged.clone();
        self.version_history = Some(VersionHistory {
            store: store.clone(),
            versions: Vec::new(),
            loaded: false,
            selected: 0,
            detail: None,
            latest_is_current: false,
        });
        cx.notify();
        cx.spawn_in(window, async move |this, cx| {
            let read = cx
                .background_spawn(async move {
                    let versions = store.versions().unwrap_or_default();
                    let latest_is_current = versions.last().is_some_and(|v| v.is_same_as(&ged));
                    let mut out: Vec<(Version, Option<Changes>)> = Vec::with_capacity(versions.len());
                    for (i, v) in versions.iter().enumerate() {
                        let changes = i.checked_sub(1).map(|p| v.changes_since(Some(&versions[p])));
                        out.push((v.clone(), changes));
                    }
                    out.reverse();
                    (out, latest_is_current)
                })
                .await;
            let _ = this.update_in(cx, |this, window, cx| {
                let Some(h) = &mut this.version_history else { return };
                (h.versions, h.latest_is_current) = read;
                h.loaded = true;
                let at = id.and_then(|id| h.versions.iter().position(|(v, _)| v.id == id)).unwrap_or(0);
                this.select_version(at, window, cx);
            });
        })
        .detach();
    }

    pub(super) fn close_version_history(&mut self, cx: &mut Context<Self>) -> bool {
        let open = self.version_history.take().is_some();
        cx.notify();
        open
    }

    /// Selects a version and reads the names of the records it changed.
    fn select_version(&mut self, i: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(h) = &mut self.version_history else { return };
        let Some((v, changes)) = h.versions.get(i).cloned() else { return };
        h.selected = i;
        cx.notify();
        if h.detail.as_ref().is_some_and(|d| d.id == v.id) {
            return;
        }
        let Some(changes) = changes else {
            h.detail = Some(Detail { id: v.id.clone(), labels: HashMap::new(), error: None });
            return;
        };
        let earlier = h.versions.get(i + 1).map(|(e, _)| e.clone());
        let store = h.store.clone();
        cx.spawn_in(window, async move |this, cx| {
            let id = v.id.clone();
            let detail = cx
                .background_spawn(async move {
                    let mut labels = HashMap::new();
                    let mut error = None;
                    let mut read = |ver: &Version, xrefs: &mut dyn Iterator<Item = &String>| match store.load(ver) {
                        Ok(g) => {
                            for x in xrefs {
                                let kind = g.get(x).and_then(|n| RecordKind::from_tag(&n.tag));
                                labels.insert(x.clone(), (g.record_label(x), kind));
                            }
                        }
                        Err(e) => error = Some(e),
                    };
                    if !changes.added.is_empty() || !changes.changed.is_empty() {
                        let mut xs = changes.added.iter().take(LIST_MAX).chain(changes.changed.iter().take(LIST_MAX));
                        read(&v, &mut xs);
                    }
                    if let (Some(e), false) = (&earlier, changes.removed.is_empty()) {
                        read(e, &mut changes.removed.iter().take(LIST_MAX));
                    }
                    Detail { id, labels, error }
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                if let Some(h) = &mut this.version_history
                    && h.versions.get(h.selected).is_some_and(|(v, _)| v.id == detail.id)
                {
                    h.detail = Some(detail);
                    cx.notify();
                }
            });
        })
        .detach();
    }

    pub(super) fn step_version(&mut self, delta: isize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(h) = &self.version_history else { return };
        let n = h.versions.len() as isize;
        if n == 0 {
            return;
        }
        let i = (h.selected as isize + delta).clamp(0, n - 1) as usize;
        self.select_version(i, window, cx);
    }

    fn can_restore(h: &VersionHistory) -> bool {
        h.loaded && !h.versions.is_empty() && !(h.selected == 0 && h.latest_is_current)
    }

    /// Restore This Version.
    fn restore_selected_version(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(h) = self.version_history.as_ref().filter(|h| Self::can_restore(h)) else { return };
        let Some((v, _)) = h.versions.get(h.selected) else { return };
        match h.store.load(v) {
            Ok(ged) => {
                let status = t!("versions.restored_status", from = date_time_label(v.time)).into();
                let from = v.id.clone();
                self.version_history = None;
                self.restore_tree(ged, Versioning::Restored(from), status, window, cx);
            }
            Err(e) => self.set_error("versions.read_failed", t!("versions.read_failed", error = e)),
        }
        cx.notify();
    }

    /// Revert To › Last Opened.
    pub(super) fn revert_to_opened(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(doc) = &self.doc else { return };
        let ged = doc.opened.clone();
        self.restore_tree(ged, Versioning::Restored(OPENED.into()), t!("versions.reverted_opened").into(), window, cx);
    }

    /// The selected version has changes that Undo This Change can take back.
    fn can_undo_version(h: &VersionHistory) -> bool {
        h.loaded && h.versions.get(h.selected).is_some_and(|(_, c)| c.as_ref().is_some_and(|c| !c.is_empty()))
    }

    /// Undo This Change: the records the selected version changed go back to the version before
    /// it, unless they changed again since.
    fn undo_selected_version(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(h) = self.version_history.as_ref().filter(|h| Self::can_undo_version(h)) else { return };
        let (Some((v, _)), Some((earlier, _))) = (h.versions.get(h.selected), h.versions.get(h.selected + 1)) else {
            return;
        };
        let Some(current) = self.doc.as_ref().map(|d| d.ged.clone()) else { return };
        match h.store.undo_changes(v, earlier, &current) {
            Ok((_, 0, _)) => self.set_status(t!("versions.nothing_to_undo"), false),
            Ok((ged, done, left)) => {
                let mut status: String = crate::tn!("versions.undone", done).into();
                if left > 0 {
                    status = format!("{status} · {}", crate::tn!("versions.left", left));
                }
                let label = format!("{UNDO}{}", v.id);
                self.version_history = None;
                self.restore_tree(ged, Versioning::Restored(label), status, window, cx);
            }
            Err(e) => self.set_error("versions.read_failed", t!("versions.read_failed", error = e)),
        }
        cx.notify();
    }

    /// Before an operation that changes many records: the tree as it is, saved and kept as a
    /// restore point (30 days), named after the operation.
    pub(super) fn restore_point(&mut self, what: String) {
        let Some(doc) = &mut self.doc else { return };
        let Some(path) = doc.own_file().map(std::path::Path::to_path_buf) else { return };
        match doc.write(&path, Versioning::RestorePoint(what), true) {
            Ok(_) => doc.dirty = false,
            Err(e) => eprintln!("restore point: {}: {e}", path.display()),
        }
    }

    /// Before an AI agent's proposal is saved: the changes not recorded yet become a version, so
    /// that the agent's version holds only its own changes.
    pub(super) fn before_agent_change(&mut self) {
        if let Some(doc) = &mut self.doc {
            doc.flush();
        }
    }

    /// After an AI agent's proposal is saved: saved at once, as a version of its own named after
    /// what it did and the agent.
    pub(super) fn record_agent_change(&mut self, what: String, cx: &mut Context<Self>) {
        let Some(path) = self.doc.as_ref().and_then(|d| d.own_file()).map(std::path::Path::to_path_buf) else { return };
        let status = self.status.clone();
        let author = crate::agent::last_client().map(|(name, _)| {
            let client = crate::agent::clients::Client::from_client_info(&name);
            client.product().map(str::to_string).unwrap_or(name)
        });
        self.write_to(path, Versioning::Agent(what, author), cx);
        // The proposal's own message stays.
        if status.is_some() && self.status.as_ref().is_none_or(|(_, error)| !error) {
            self.status = status;
        }
    }

    /// Makes `ged` the current tree: one undo point, saved at once and recorded as a version.
    pub(super) fn restore_tree(
        &mut self,
        ged: Gedcom,
        versioning: Versioning,
        status: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(path) = self.doc.as_ref().and_then(|d| d.own_file()).map(std::path::Path::to_path_buf) else { return };
        // The changes not recorded yet stay in the history.
        if let Some(doc) = &mut self.doc {
            doc.flush();
        }
        self.checkpoint();
        if let Some(doc) = &mut self.doc {
            doc.ged = ged;
        }
        self.write_to(path, versioning, cx);
        self.rebuild_rows(cx);
        self.refresh_editor(window, cx);
        if self.status.as_ref().is_none_or(|(_, error)| !error) {
            self.set_status(status, false);
        }
        window.focus(&self.focus, cx);
        cx.notify();
    }

    /// Save a Copy…: the selected version as a `.ged` of its own, beside the file.
    fn save_version_copy(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(h) = &self.version_history else { return };
        let Some((v, _)) = h.versions.get(h.selected).cloned() else { return };
        let Some(path) = self.doc.as_ref().and_then(|d| d.own_file()).map(std::path::Path::to_path_buf) else { return };
        let store = h.store.clone();
        let dir = path.parent().map(std::path::Path::to_path_buf).unwrap_or_else(dirs_home);
        let stem = path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
        let stamp = local(v.time).format("%Y-%m-%d %H.%M").to_string();
        let name = format!("{}.ged", t!("versions.copy_name", name = stem, date = stamp));
        let rx = cx.prompt_for_new_path(&dir, Some(&name));
        cx.spawn_in(window, async move |this, cx| {
            let Ok(Ok(Some(dest))) = rx.await else { return };
            let result = store.load(&v).and_then(|g| gedcom::save(&g, &dest).map_err(|e| e.to_string()));
            let _ = this.update(cx, |this, cx| {
                match result {
                    Ok(()) => this.set_status(t!("versions.copy_saved", path = dest.display()), false),
                    Err(e) => this.set_error("versions.copy_failed", t!("versions.copy_failed", error = e)),
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// File › Save Version…: asks for a name.
    pub(super) fn open_save_version(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.has_version_history() {
            return;
        }
        let input = cx.new(|cx| InputState::new(window, cx).placeholder(t!("versions.name_placeholder")));
        // After the command palette, which gives the focus back to the window as it closes.
        let field = input.clone();
        cx.defer_in(window, move |_, window, cx| field.update(cx, |s, cx| s.focus(window, cx)));
        self.add_dialog = Some(AddDialog::SaveVersion(input));
        cx.notify();
    }

    fn save_named_version(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(AddDialog::SaveVersion(input)) = &self.add_dialog else { return };
        let name = input.read(cx).value().trim().to_string();
        if name.is_empty() {
            return;
        }
        let Some(path) = self.doc.as_ref().and_then(|d| d.own_file()).map(std::path::Path::to_path_buf) else { return };
        self.add_dialog = None;
        self.write_to(path, Versioning::Named(name.clone()), cx);
        if self.status.as_ref().is_none_or(|(_, error)| !error) {
            self.set_status(t!("versions.named_status", name = name), false);
        }
        window.focus(&self.focus, cx);
        cx.notify();
    }

    pub(super) fn save_version_body(&self, input: &Entity<InputState>, cx: &mut Context<Self>) -> AnyElement {
        v_flex()
            .gap_3()
            .child(Input::new(input).small())
            .child(
                h_flex().gap_2().justify_end().child(
                    Button::new("save-version")
                        .primary()
                        .small()
                        .label(t!("versions.save"))
                        .on_click(cx.listener(|this, _, w, cx| this.save_named_version(w, cx))),
                ),
            )
            .on_key_down(cx.listener(|this, ev: &KeyDownEvent, w, cx| {
                if ev.keystroke.key == "enter" {
                    this.save_named_version(w, cx);
                }
            }))
            .into_any_element()
    }

    pub(super) fn render_version_history(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let h = self.version_history.as_ref()?;
        let current = &self.doc.as_ref()?.ged;
        let today = Local::now().date_naive();

        // List: one heading per day, one row per version.
        let mut rows: Vec<AnyElement> = Vec::new();
        let mut last_day = None;
        for (i, (v, changes)) in h.versions.iter().enumerate() {
            let t = local(v.time);
            let day = t.date_naive();
            if last_day != Some(day) {
                last_day = Some(day);
                rows.push(
                    div()
                        .px_3()
                        .pt(px(if i == 0 { 4. } else { 12. }))
                        .pb_1()
                        .child(label_caps(day_label(&t, today)))
                        .into_any_element(),
                );
            }
            let on = i == h.selected;
            let name = current_title(h, i, v);
            rows.push(
                h_flex()
                    .id(SharedString::from(format!("version-{i}")))
                    .mx_2()
                    .px_2()
                    .py_1()
                    .gap_3()
                    .items_start()
                    .rounded(theme::RADIUS_SM)
                    .cursor_pointer()
                    .when(on, |d| d.bg(theme::accent_soft()))
                    .when(!on, |d| d.hover(|d| d.bg(theme::hover())))
                    .child(div().pt(px(1.)).w(px(40.)).flex_none().child(mono(t.format("%H:%M").to_string())))
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
                                            .child(name),
                                    )
                                    .children(badge(v).map(|b| {
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
                            .child(div().text_xs().text_color(theme::muted()).child(summary(v, changes.as_ref()))),
                    )
                    .on_click(cx.listener(move |this, _, w, cx| this.select_version(i, w, cx)))
                    .into_any_element(),
            );
        }
        let list = div()
            .id("versions-list")
            .w(px(320.))
            .flex_none()
            .h_full()
            .py_2()
            .overflow_y_scroll()
            .bg(theme::panel())
            .border_r_1()
            .border_color(theme::border())
            .children(rows);

        // The selected version: what it changed.
        let selected = h.versions.get(h.selected);
        let detail = selected.map(|(v, changes)| {
            let labels = h.detail.as_ref().filter(|d| d.id == v.id);
            let mut subtitle = date_time_label(v.time);
            if let (Kind::Agent, Some(a)) = (v.kind, &v.author) {
                subtitle = format!("{subtitle} · {a}");
            }
            let mut body = v_flex().gap_4();
            match changes {
                None => body = body.child(div().text_color(theme::muted()).child(summary(v, None))),
                Some(c) if c.is_empty() => {
                    body = body.child(div().text_color(theme::muted()).child(t!("versions.no_change")))
                }
                Some(c) => {
                    for (key, section, xrefs) in [
                        ("added", t!("versions.section_added"), &c.added),
                        ("changed", t!("versions.section_changed"), &c.changed),
                        ("removed", t!("versions.section_removed"), &c.removed),
                    ] {
                        if xrefs.is_empty() {
                            continue;
                        }
                        let heading = format!("{section} · {}", group_thousands(xrefs.len()));
                        let removed = key == "removed";
                        let items = xrefs.iter().take(LIST_MAX).map(|x| {
                            let (name, kind) = labels
                                .and_then(|d| d.labels.get(x).cloned())
                                .unwrap_or_else(|| (String::from("…"), None));
                            let here = !removed && current.get(x).is_some();
                            let target = x.clone();
                            h_flex()
                                .id(SharedString::from(format!("version-rec-{key}-{x}")))
                                .px_2()
                                .py_1()
                                .gap_2()
                                .items_center()
                                .rounded(theme::RADIUS_SM)
                                .children(kind.map(|k| icon(View::for_kind(k).icon()).text_color(theme::muted())))
                                .child(
                                    div()
                                        .min_w_0()
                                        .overflow_hidden()
                                        .whitespace_nowrap()
                                        .text_ellipsis()
                                        .when(removed, |d| d.text_color(theme::muted()))
                                        .child(name),
                                )
                                .when(here, |d| {
                                    d.cursor_pointer().hover(|d| d.bg(theme::hover())).on_click(cx.listener(
                                        move |this, _, w, cx| {
                                            this.version_history = None;
                                            this.goto(target.clone(), w, cx);
                                        },
                                    ))
                                })
                        });
                        let more = xrefs.len().saturating_sub(LIST_MAX);
                        body = body.child(
                            v_flex().gap_1().child(div().px_2().child(label_caps(heading))).children(items).when(
                                more > 0,
                                |d| {
                                    d.child(
                                        div()
                                            .px_2()
                                            .text_xs()
                                            .text_color(theme::muted())
                                            .child(crate::tn!("versions.more", more).to_string()),
                                    )
                                },
                            ),
                        );
                    }
                }
            }
            if let Some(e) = labels.and_then(|d| d.error.clone()) {
                body = body.child(div().text_color(theme::danger()).child(e));
            }
            v_flex()
                .id("versions-detail")
                .flex_1()
                .min_w_0()
                .h_full()
                .px_6()
                .py_4()
                .gap_4()
                .overflow_y_scroll()
                .child(
                    v_flex()
                        .gap_0p5()
                        .child(div().text_xl().font_weight(FontWeight::SEMIBOLD).child(current_title(h, h.selected, v)))
                        .child(div().text_color(theme::muted()).child(subtitle)),
                )
                .child(body)
        });
        let empty = (h.loaded && h.versions.is_empty()).then(|| {
            v_flex()
                .flex_1()
                .items_center()
                .justify_center()
                .gap_2()
                .text_color(theme::muted())
                .child(icon(IconName::RotateCcwClock).size(px(24.)))
                .child(t!("versions.empty"))
        });
        let main = h_flex().flex_1().min_h_0().child(list).children(detail).children(empty);

        // Buttons: Save a Copy… apart; Done and Restore in the platform's order.
        let can_restore = Self::can_restore(h);
        let restore = Button::new("versions-restore")
            .primary()
            .label(t!("versions.restore"))
            .disabled(!can_restore)
            .on_click(cx.listener(|this, _, w, cx| this.restore_selected_version(w, cx)));
        let done = Button::new("versions-done").label(t!("versions.done")).on_click(cx.listener(|this, _, _, cx| {
            this.close_version_history(cx);
        }));
        let buttons = if cfg!(target_os = "windows") {
            [restore.into_any_element(), done.into_any_element()]
        } else {
            [done.into_any_element(), restore.into_any_element()]
        };
        let copy = Button::new("versions-copy")
            .label(t!("versions.save_copy"))
            .disabled(selected.is_none())
            .on_click(cx.listener(|this, _, w, cx| this.save_version_copy(w, cx)));
        let undo = Button::new("versions-undo")
            .label(t!("versions.undo"))
            .disabled(!Self::can_undo_version(h))
            .on_click(cx.listener(|this, _, w, cx| this.undo_selected_version(w, cx)));
        let foot = h_flex()
            .px_6()
            .py_4()
            .gap_2()
            .items_center()
            .border_t_1()
            .border_color(theme::border())
            .child(copy)
            .child(undo)
            .child(div().flex_1())
            .children(buttons);

        let file = self.doc.as_ref().map(Document::title).unwrap_or_default();
        let head = h_flex()
            .px_6()
            .py_3()
            .gap_2()
            .items_center()
            .border_b_1()
            .border_color(theme::border())
            .child(div().font_weight(FontWeight::SEMIBOLD).child(t!("versions.title", file = file)));

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
                        .id("version-history")
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
                                this.close_version_history(cx);
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
