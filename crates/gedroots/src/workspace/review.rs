//! “To Review” page: the records proposed by an AI agent, compared with the tree before being
//! saved.
//!
//! The agent never writes: its proposals wait here, in memory, for the session. For each one, the
//! page shows the record’s image, an “In the tree” / “Proposed” comparison (each person can be
//! unchecked), the attachment (repository, source, file, event) and, on request, the exact GEDCOM
//! lines that will change. “Save to Tree” writes everything at once (a single undo point) and
//! organizes the file according to File Settings › Media; “Reject” removes it without writing
//! anything. Changes for a Data Quality issue are listed after the acts (`review_change.rs`).

use super::*;
use crate::agent::propose::{self, Outcome, Pending, Queued};
use gedcom::Conflict;
use gedcom::proposal::{Change, Field, Proposal, SourceChoice};
use gpui_kit::component::checkbox::Checkbox;
use gpui_kit::component::menu::{DropdownMenu, PopupMenuItem};
use rust_i18n::t;
use std::collections::BTreeSet;

/// A person, or one of their facts, that the user can uncheck.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Keep {
    Person(usize),
    Fact(usize, usize),
}

/// A proposal and what the user unchecked.
pub(super) struct Item {
    /// Given to the agent (`P3`), to correct or withdraw it.
    id: String,
    pending: Pending,
    excluded: BTreeSet<Keep>,
    /// Choice for each value that contradicts the tree: the event (`None`) or a fact (person,
    /// fact), as numbered in the agent's proposal.
    conflicts: std::collections::BTreeMap<Option<(usize, usize)>, Conflict>,
    /// Prepared by the guided tour, not sent by an agent.
    demo: bool,
}

impl Item {
    /// Temporary ids of the people kept by [`Item::proposal`], in its order.
    fn kept_temps(&self) -> Vec<Option<String>> {
        let temps = &self.pending.temps;
        (0..self.pending.proposal.act.participants.len())
            .filter(|i| !self.excluded.contains(&Keep::Person(*i)))
            .map(|i| temps.get(i).cloned().flatten())
            .collect()
    }

    /// The proposal, without the unchecked people and facts.
    fn proposal(&self) -> Proposal {
        let mut p = self.pending.proposal.clone();
        p.conflicts.event = self.conflict(None);
        // Facts are numbered again once the unchecked people and facts are left out.
        let mut person = 0;
        for (i, x) in p.act.participants.iter_mut().enumerate() {
            if self.excluded.contains(&Keep::Person(i)) {
                continue;
            }
            let mut fact = 0;
            for k in 0..x.facts.len() {
                if self.excluded.contains(&Keep::Fact(i, k)) {
                    continue;
                }
                let c = self.conflict(Some((i, k)));
                if c != Conflict::KeepBoth {
                    p.conflicts.facts.insert((person, fact), c);
                }
                fact += 1;
            }
            person += 1;
        }
        for (i, person) in p.act.participants.iter_mut().enumerate() {
            let mut k = 0;
            person.facts.retain(|_| {
                let keep = !self.excluded.contains(&Keep::Fact(i, k));
                k += 1;
                keep
            });
        }
        let mut i = 0;
        p.act.participants.retain(|_| {
            let keep = !self.excluded.contains(&Keep::Person(i));
            i += 1;
            keep
        });
        p
    }

    fn conflict(&self, at: Option<(usize, usize)>) -> Conflict {
        self.conflicts.get(&at).copied().unwrap_or_default()
    }

    /// Unchecking a person also leaves out their facts.
    fn is_excluded(&self, k: Keep) -> bool {
        self.excluded.contains(&k) || matches!(k, Keep::Fact(i, _) if self.excluded.contains(&Keep::Person(i)))
    }

    fn keep(&mut self, k: Keep, keep: bool) {
        if keep {
            self.excluded.remove(&k);
        } else {
            self.excluded.insert(k);
        }
    }
}

#[derive(Default)]
pub(super) struct Review {
    pub(super) items: Vec<Item>,
    /// Changes for Data Quality issues, listed after the acts.
    pub(super) changes: Vec<super::review_change::ChangeGroup>,
    /// Page displayed instead of the lists.
    pub(super) open: bool,
    pub(super) sel: usize,
    /// GEDCOM section expanded.
    pub(super) gedcom: bool,
    /// Zoom of the act's image.
    pub(super) zoom: crate::viewer::Zoom,
    /// Last proposal id and temporary person id given.
    last_id: usize,
    last_temp: usize,
    /// What became of the proposals no longer waiting, for the agent.
    decided: Vec<propose::Decision>,
    /// Temporary id → person saved in the tree.
    saved: std::collections::HashMap<String, String>,
}

impl Review {
    pub(super) fn is_empty(&self) -> bool {
        self.items.is_empty() && self.changes.is_empty()
    }
}

/// “Birth · Jean BLANC”: linked people are named as in the tree.
fn title(ged: &Gedcom, p: &Proposal) -> String {
    let act = &p.act;
    let names: Vec<String> = act
        .participants
        .iter()
        .filter(|x| x.role == gedcom::ActRole::Principal)
        .filter_map(|x| match &x.choice {
            gedcom::PersonChoice::Link(id) => ged.individual(id).map(|i| i.display_name()),
            _ => Some(x.display_name()).filter(|n| !n.is_empty()),
        })
        .collect();
    let what = act.event_label();
    if names.is_empty() { what } else { format!("{what} · {}", names.join(" & ")) }
}

fn show_date(v: &str) -> String {
    if v.trim().is_empty() { "—".into() } else { gedcom::Date::parse(v).display() }
}

fn or_dash(v: &str) -> String {
    if v.trim().is_empty() { "—".into() } else { v.to_string() }
}

fn decision(id: String, outcome: Outcome) -> propose::Decision {
    propose::Decision { id, outcome, new_people: Vec::new() }
}

/// What became of a proposal, for the agent.
fn outcome_text(o: Outcome) -> &'static str {
    match o {
        Outcome::Saved => "the user saved it",
        Outcome::Rejected => "the user rejected it",
        Outcome::Edited => "the user opened it in record entry",
        Outcome::Withdrawn => "it was withdrawn",
        Outcome::Replaced => "it was replaced",
    }
}

fn change_tag(c: Change) -> (String, Hsla) {
    let (text, color) = match c {
        Change::Same => (t!("review.same"), theme::muted()),
        Change::Added => (t!("review.added"), theme::ok()),
        Change::Refined => (t!("review.refined"), theme::warn()),
        Change::Differs => (t!("review.differs"), theme::danger()),
        Change::Create => (t!("review.create"), theme::ok()),
        Change::Link => (t!("review.link"), theme::ok()),
        Change::Mention => (t!("review.mention"), theme::ok()),
    };
    (text.into_owned(), color)
}

fn conflict_label(c: Conflict) -> String {
    match c {
        Conflict::KeepBoth => t!("review.keep_both"),
        Conflict::Replace => t!("review.replace"),
        Conflict::Ignore => t!("review.ignore"),
    }
    .into_owned()
}

/// “≠ Keep Both ▾”: what saving does with a value that contradicts the tree.
fn conflict_menu(
    current: Conflict,
    at: Option<(usize, usize)>,
    color: Hsla,
    cx: &mut Context<Workspace>,
) -> impl IntoElement {
    let me = cx.entity().downgrade();
    let id = match at {
        None => "conflict-event".to_string(),
        Some((i, k)) => format!("conflict-{i}-{k}"),
    };
    Button::new(SharedString::from(id))
        .ghost()
        .xsmall()
        .dropdown_caret(true)
        .child(
            div()
                .text_xs()
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(color)
                .child(format!("≠ {}", conflict_label(current))),
        )
        .dropdown_menu(move |mut menu, _, _| {
            for c in [Conflict::KeepBoth, Conflict::Replace, Conflict::Ignore] {
                let me = me.clone();
                menu =
                    menu.item(PopupMenuItem::new(conflict_label(c)).checked(c == current).on_click(move |_, _, cx| {
                        let _ = me.update(cx, |ws, cx| {
                            let sel = ws.review.sel;
                            if let Some(it) = ws.review.items.get_mut(sel) {
                                it.conflicts.insert(at, c);
                            }
                            cx.notify();
                        });
                    }));
            }
            menu
        })
}

impl Workspace {
    /// Proposal received from the agent: checked against the displayed file, then queued.
    pub(crate) fn add_proposal(&mut self, pending: Pending, cx: &mut Context<Self>) -> Result<Queued, String> {
        self.queue_proposal(pending, false, cx)
    }

    /// Queues a proposal for review; `demo`: prepared by the guided tour, and labelled so. A
    /// correction (`replaces`) takes the place of the proposal it corrects.
    pub(super) fn queue_proposal(
        &mut self,
        mut pending: Pending,
        demo: bool,
        cx: &mut Context<Self>,
    ) -> Result<Queued, String> {
        let Some(doc) = &self.doc else { return Err("no file is open in GEDRoots".into()) };
        let g = &doc.ged;
        let replaced = match &pending.replaces {
            Some(r) => Some(self.review.items.iter().position(|it| &it.id == r).ok_or_else(|| {
                match self.review.decided.iter().rev().find(|d| &d.id == r) {
                    Some(d) => format!("proposal {r} is no longer waiting: {}", outcome_text(d.outcome)),
                    None => format!("no proposal {r}"),
                }
            })?),
            None => None,
        };
        propose::link_known(&mut pending.proposal, &pending.temps, &self.review.saved);
        pending.proposal.complete(g);
        pending.proposal.check(g)?;
        // Every new person gets a temporary id, for the agent to name them in its next proposals.
        let n = pending.proposal.act.participants.len();
        pending.temps.resize(n, None);
        for (part, t) in pending.proposal.act.participants.iter().zip(pending.temps.iter_mut()) {
            match part.choice {
                gedcom::PersonChoice::Create if t.is_none() => {
                    self.review.last_temp += 1;
                    *t = Some(format!("{}{}", propose::TEMP, self.review.last_temp));
                }
                gedcom::PersonChoice::Create => {}
                _ => *t = None,
            }
        }
        self.review.last_id += 1;
        let id = format!("P{}", self.review.last_id);
        let temps = pending.temps.clone();
        let item = Item { id: id.clone(), pending, excluded: BTreeSet::new(), conflicts: Default::default(), demo };
        match replaced {
            Some(i) => {
                let old = std::mem::replace(&mut self.review.items[i], item);
                self.review.decided.push(decision(old.id, Outcome::Replaced));
                self.set_status(t!("review.replaced"), false);
            }
            None => {
                self.review.items.push(item);
                let n = self.review.items.len();
                match demo {
                    true => self.set_status(t!("review.demo"), false),
                    false => self.set_status(crate::tn!("review.received", n), false),
                }
            }
        }
        cx.notify();
        Ok(Queued { id, waiting: self.review.items.len(), temps })
    }

    /// The agent takes back a waiting proposal.
    pub(crate) fn withdraw_proposal(&mut self, id: &str, cx: &mut Context<Self>) -> Result<usize, String> {
        let id = id.trim();
        let Some(i) = self.review.items.iter().position(|it| it.id == id) else {
            return Err(match self.review.decided.iter().rev().find(|d| d.id == id) {
                Some(d) => format!("proposal {id} is no longer waiting: {}", outcome_text(d.outcome)),
                None => format!("no proposal {id}"),
            });
        };
        let sel = self.review.sel;
        self.review.sel = i;
        self.review_take(Outcome::Withdrawn);
        let total = self.review.items.len() + self.review.changes.len();
        self.review.sel = if i < sel { sel - 1 } else { sel.min(total.saturating_sub(1)) };
        self.set_status(t!("review.withdrawn"), false);
        cx.notify();
        Ok(self.review.items.len())
    }

    /// The review page as the agent sees it.
    pub(super) fn review_queue(&self) -> propose::Queue {
        propose::Queue {
            waiting: self
                .review
                .items
                .iter()
                .filter(|it| !it.demo)
                .map(|it| propose::Waiting {
                    id: it.id.clone(),
                    proposal: it.pending.proposal.clone(),
                    temps: it.pending.temps.clone(),
                })
                .collect(),
            decided: self.review.decided.clone(),
            saved: self.review.saved.clone(),
        }
    }

    pub(super) fn open_review(&mut self, cx: &mut Context<Self>) {
        if self.review.is_empty() {
            self.set_status(t!("review.none"), false);
        } else {
            self.act_editor = None;
            self.review.open = true;
            self.tour_did(super::tour::Advance::OpenReview, cx);
        }
        cx.notify();
    }

    fn review_select(&mut self, i: usize, cx: &mut Context<Self>) {
        self.review.sel = i;
        cx.notify();
    }

    /// Removes the displayed proposal, remembering what became of it; closes the page if none is
    /// left.
    fn review_take(&mut self, outcome: Outcome) -> Option<Item> {
        if self.review.sel >= self.review.items.len() {
            return None;
        }
        let item = self.review.items.remove(self.review.sel);
        if !item.demo {
            self.review.decided.push(decision(item.id.clone(), outcome));
        }
        let total = self.review.items.len() + self.review.changes.len();
        self.review.sel = self.review.sel.min(total.saturating_sub(1));
        if total == 0 {
            self.review.open = false;
        }
        Some(item)
    }

    pub(super) fn reject_proposal(&mut self, cx: &mut Context<Self>) {
        if !self.review.open {
            return;
        }
        if let Some(i) = self.selected_change() {
            self.reject_change(i, cx);
            return;
        }
        if self.review_take(Outcome::Rejected).is_some() {
            self.set_status(t!("review.rejected"), false);
            self.tour_did(super::tour::Advance::Decide, cx);
        }
        cx.notify();
    }

    /// Saves the displayed proposal: an undo point, then the file organized.
    pub(super) fn accept_proposal(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        // ⌘↩ also merges in the merge window.
        if self.merge.is_some() {
            return self.merge_current(window, cx);
        }
        if !self.review.open || self.doc.is_none() {
            return;
        }
        if let Some(i) = self.selected_change() {
            self.accept_change(i, window, cx);
            return;
        }
        let Some(item) = self.review.items.get(self.review.sel) else { return };
        let proposal = item.proposal();
        let temps = item.kept_temps();
        let base = self.ged_dir();
        // The extracted image lives in the app’s cache: it must join the .ged folder.
        if base.is_none() && item.pending.image.is_some() {
            self.set_error("review.save_ged_first", t!("review.save_ged_first"));
            cx.notify();
            return;
        }
        self.before_agent_change();
        self.checkpoint();
        let doc = self.doc.as_mut().unwrap();
        match proposal.apply(&mut doc.ged) {
            Ok(res) => {
                let note = match (&proposal.act.media, &res.media) {
                    (gedcom::MediaChoice::NewFiles(_), Some(m)) => {
                        let note = media_files::store_act_media(&mut doc.ged, base.as_deref(), m);
                        match base.as_deref().map(|b| copy_out_of_cache(&mut doc.ged, b, m)) {
                            Some(Err(e)) => t!("media_files.copy_failed", error = e).into_owned(),
                            Some(Ok(true)) => String::new(),
                            _ => note,
                        }
                    }
                    _ => String::new(),
                };
                // The new people are linked in the other proposals that name them, and the agent
                // learns their ids.
                let mut saved = Vec::new();
                for (t, x) in temps.iter().zip(&res.persons) {
                    if let (Some(t), Some(x)) = (t, x) {
                        self.review.saved.insert(t.clone(), x.clone());
                        let name = doc.ged.individual(x).map(|i| i.display_name()).unwrap_or_default();
                        saved.push(propose::SavedPerson { temp: t.clone(), id: x.clone(), name });
                    }
                }
                for it in &mut self.review.items {
                    propose::link_known(&mut it.pending.proposal, &it.pending.temps, &self.review.saved);
                }
                self.review_take(Outcome::Saved);
                if let Some(d) = self.review.decided.last_mut().filter(|d| d.outcome == Outcome::Saved) {
                    d.new_people = saved;
                }
                self.set_status(res.summary() + &note, false);
                self.record_agent_change(res.summary(), cx);
                self.tour_did(super::tour::Advance::Decide, cx);
                self.rebuild_rows(cx);
                self.refresh_editor(window, cx);
                if !self.review.open
                    && let Some(Some(x)) = res.persons.first()
                {
                    self.open_location(View::Individuals, Some(x.clone()), window, cx);
                }
            }
            Err(e) => {
                self.drop_checkpoint();
                self.set_error("review.failed", e);
            }
        }
        cx.notify();
    }

    /// Opens the proposal in the record entry, to change anything. A new source is created first
    /// (⌘Z removes it).
    fn edit_proposal(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(item) = self.review.items.get(self.review.sel) else { return };
        let proposal = item.proposal();
        // New source: created first, with an undo point.
        let mut act = match &proposal.source {
            SourceChoice::Existing(x) => gedcom::Act { source: x.clone(), ..proposal.act.clone() },
            SourceChoice::New(_) => {
                self.checkpoint();
                match proposal.materialize(&mut self.doc.as_mut().unwrap().ged) {
                    Ok(act) => act,
                    Err(e) => {
                        self.drop_checkpoint();
                        self.set_error("review.failed", e);
                        cx.notify();
                        return;
                    }
                }
            }
        };
        // Record entry has its own fields for what the review lists as deduced facts.
        act.fold_deduced_facts();
        self.review_take(Outcome::Edited);
        self.review.open = false;
        self.rebuild_rows(cx);
        let source = act.source.clone();
        self.goto(source, window, cx);
        self.tab = CenterTab::Form;
        self.show_act_editor(act, window, cx);
    }

    /// “To Review” area of the sidebar, as long as there are proposals.
    pub(super) fn render_review_zone(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        if self.review.is_empty() {
            return None;
        }
        let active = self.review.open;
        Some(
            v_flex()
                .gap_0p5()
                .child(div().h(px(1.)).my_1().bg(theme::border()))
                .child(
                    h_flex()
                        .id("zone-review")
                        .px_2()
                        .py_1()
                        .gap_2()
                        .items_center()
                        .rounded(theme::RADIUS_SM)
                        .cursor_pointer()
                        .when(active, |d| d.bg(theme::accent_soft()).font_weight(FontWeight::SEMIBOLD))
                        .when(!active, |d| d.hover(|d| d.bg(theme::hover())))
                        .child(Icon::new(IconName::Inbox).size(px(16.)).text_color(if active {
                            theme::accent()
                        } else {
                            theme::muted()
                        }))
                        .child(div().flex_1().child(t!("review.title").into_owned()))
                        .child(
                            div()
                                .text_xs()
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(theme::accent())
                                .child(self.review_count().to_string()),
                        )
                        .on_click(cx.listener(|this, _, _, cx| this.open_review(cx)))
                        .children(self.tour_anchor(super::tour::Target::ReviewZone, cx)),
                )
                .into_any_element(),
        )
    }

    /// The page: list of proposals, record image, comparison.
    pub(super) fn render_review(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(doc) = &self.doc else { return div().into_any_element() };
        let g = &doc.ged;
        let list = v_flex()
            .id("review-list")
            .w(px(260.))
            .flex_none()
            .overflow_y_scroll()
            .border_r_1()
            .border_color(theme::border())
            .children(self.review.items.iter().enumerate().map(|(i, item)| {
                let p = item.proposal();
                let sel = i == self.review.sel;
                let chain = p.chain(g);
                let meta = [chain.repository.map(|r| r.value).unwrap_or_default(), p.act.page.trim().to_string()]
                    .into_iter()
                    .filter(|s| !s.is_empty())
                    .collect::<Vec<_>>()
                    .join(" · ");
                v_flex()
                    .id(SharedString::from(format!("review-row-{i}")))
                    .px_3()
                    .py_2()
                    .gap_0p5()
                    .cursor_pointer()
                    .border_b_1()
                    .border_color(theme::border())
                    .when(sel, |d| d.bg(theme::accent_soft()).border_l_2().border_color(theme::accent()))
                    .when(!sel, |d| d.hover(|d| d.bg(theme::hover())))
                    .child(div().font_weight(FontWeight::MEDIUM).child(title(g, &p)))
                    .when(!meta.is_empty(), |d| d.child(div().text_xs().text_color(theme::muted()).child(meta)))
                    .on_click(cx.listener(move |this, _, _, cx| this.review_select(i, cx)))
            }))
            .children(self.review.changes.iter().enumerate().map(|(i, group)| self.render_change_row(i, group, g, cx)));
        if let Some(i) = self.selected_change() {
            return h_flex()
                .flex_1()
                .min_w_0()
                .items_stretch()
                .child(list)
                .child(self.render_change_detail(i, &self.review.changes[i], g, cx))
                .into_any_element();
        }
        let Some(item) = self.review.items.get(self.review.sel) else {
            return list.into_any_element();
        };
        // No image: no empty viewer, the proposal takes a single centred column.
        let image = item.pending.preview.is_some() || item.pending.image.is_some();
        h_flex()
            .flex_1()
            .min_w_0()
            .items_stretch()
            .child(list)
            .map(|d| match image {
                true => d.child(self.review_viewer(item)).child(self.review_detail(item, g, cx)),
                false => d.child(
                    v_flex()
                        .flex_1()
                        .min_w_0()
                        .items_center()
                        .child(self.review_detail(item, g, cx).max_w(px(640.)).size_full().border_l_0()),
                ),
            })
            .into_any_element()
    }

    fn review_viewer(&self, item: &Item) -> AnyElement {
        let pending = &item.pending;
        let caption = match (&pending.original, pending.pdf_page) {
            (Some(o), Some(p)) => {
                let name = o.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                Some(t!("review.pdf_page", page = p, file = name).into_owned())
            }
            _ => None,
        };
        div()
            .flex_1()
            .min_w_0()
            .relative()
            .bg(theme::viewer())
            .map(|d| match pending.preview.as_ref().or(pending.image.as_ref()) {
                // Never a silent black viewer: say which file could not be shown.
                Some(path) if crate::images::image_size(path).is_none() => {
                    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                    self.review.zoom.clear();
                    d.flex().items_center().justify_center().p_4().child(
                        div()
                            .text_color(theme::image_muted())
                            .child(t!("review.unreadable_image", file = name).into_owned()),
                    )
                }
                Some(path) => d.child(self.review.zoom.view("review-image", path.clone()).max_fit(f32::INFINITY)),
                None => {
                    self.review.zoom.clear();
                    d.flex()
                        .items_center()
                        .justify_center()
                        .p_4()
                        .child(div().text_color(theme::image_muted()).child(t!("review.no_image").into_owned()))
                }
            })
            .when_some(caption, |d, c| {
                d.child(
                    div()
                        .absolute()
                        .bottom_3()
                        // Bottom left: the minimap of a zoomed image takes the right corner.
                        .left_3()
                        .px_2()
                        .py_0p5()
                        .rounded(theme::RADIUS_SM)
                        .bg(theme::image_label_bg())
                        .text_xs()
                        .text_color(theme::image_text())
                        .child(c),
                )
            })
            .into_any_element()
    }

    fn review_detail(&self, item: &Item, g: &Gedcom, cx: &mut Context<Self>) -> Div {
        let full = &item.pending.proposal;
        let p = item.proposal();
        // The proposal may no longer fit the tree (changed since): it stays visible, but is not
        // saved.
        let problem = p.check(g).err();
        let rows = full.compare(g);
        let sosa = full
            .act
            .participants
            .iter()
            .find_map(|x| match &x.choice {
                gedcom::PersonChoice::Link(id) if x.role == gedcom::ActRole::Principal => {
                    g.individual(id).and_then(|i| i.sosa())
                }
                _ => None,
            })
            .map(|s| t!("review.sosa", n = s).into_owned());
        let by = if item.demo { t!("review.demo") } else { t!("review.by_agent") };
        let sub = [sosa, Some(by.into_owned())].into_iter().flatten().collect::<Vec<_>>().join(" · ");

        // Comparison. Columns: check box, field, “In the tree”, “Proposed”.
        let line = || h_flex().items_start().gap_2().px_2().py_1p5().border_t_1().border_color(theme::border());
        let header = || {
            h_flex()
                .gap_2()
                .px_2()
                .py_1()
                .bg(theme::panel())
                .text_xs()
                .text_color(theme::muted())
                .child(div().w(px(16.)).flex_none())
                .child(div().w(px(76.)).flex_none())
                .child(div().flex_1().min_w_0().child(t!("review.in_tree").into_owned()))
                .child(div().flex_1().min_w_0().child(t!("review.proposed").into_owned()))
        };
        let table = || v_flex().border_1().border_color(theme::border()).rounded(theme::RADIUS_SM).text_sm();
        // A new person that other waiting proposals name too: whichever is saved first creates them.
        let also_in = |field: &Field| {
            let Field::Person { index, .. } = field else { return None };
            let temp = item.pending.temps.get(*index)?.as_ref()?;
            let titles: Vec<String> = self
                .review
                .items
                .iter()
                .filter(|it| it.id != item.id && it.pending.temps.iter().flatten().any(|t| t == temp))
                .map(|it| t!("review.quoted", title = title(g, &it.pending.proposal)).into_owned())
                .collect();
            (!titles.is_empty()).then(|| titles.join(", "))
        };
        let (mut people_rows, mut fact_rows): (Vec<AnyElement>, Vec<AnyElement>) = (Vec::new(), Vec::new());
        for row in &rows {
            // What a check box keeps: a person, or one of their facts.
            let (label, detail, current, proposed, key) = match &row.field {
                Field::Date => {
                    (t!("review.date").into_owned(), None, show_date(&row.current), show_date(&row.proposed), None)
                }
                Field::Place => (
                    t!("review.place").into_owned(),
                    None,
                    or_dash(&gedcom::short_place(&row.current)),
                    or_dash(&gedcom::short_place(&row.proposed)),
                    None,
                ),
                Field::Address => {
                    (t!("review.address").into_owned(), None, or_dash(&row.current), or_dash(&row.proposed), None)
                }
                Field::Person { index, role } => {
                    let principal = full.act.participants[*index].role == gedcom::ActRole::Principal;
                    let key = (!principal).then_some(Keep::Person(*index));
                    (role.clone(), None, or_dash(&row.current), or_dash(&row.proposed), key)
                }
                Field::Fact { person, fact, label, name } => (
                    label.clone(),
                    Some(name.clone()),
                    or_dash(&row.current),
                    or_dash(&row.proposed),
                    Some(Keep::Fact(*person, *fact)),
                ),
            };
            let on = key.is_none_or(|k| !item.is_excluded(k));
            let check: AnyElement = match key {
                Some(k) => Checkbox::new(SharedString::from(format!("review-keep-{k:?}")))
                    .checked(on)
                    .on_click(cx.listener(move |this, keep: &bool, _, cx| {
                        let sel = this.review.sel;
                        if let Some(it) = this.review.items.get_mut(sel) {
                            it.keep(k, *keep);
                        }
                        cx.notify();
                    }))
                    .into_any_element(),
                None => div().into_any_element(),
            };
            let (tag, color) = change_tag(row.change);
            let same = row.change == Change::Same;
            // A contradiction: keep both (default), prefer the record's, or keep the tree's.
            let at = match &row.field {
                Field::Date | Field::Place | Field::Address => Some(None),
                Field::Fact { person, fact, .. } => Some(Some((*person, *fact))),
                Field::Person { .. } => None,
            }
            .filter(|_| row.change == Change::Differs);
            let tag: AnyElement = match at {
                Some(at) => {
                    h_flex().ml(px(-6.)).child(conflict_menu(item.conflict(at), at, color, cx)).into_any_element()
                }
                None => {
                    div().text_xs().font_weight(FontWeight::SEMIBOLD).text_color(color).child(tag).into_any_element()
                }
            };
            let el = line()
                .child(div().w(px(16.)).flex_none().child(check))
                .child(
                    v_flex()
                        .w(px(76.))
                        .flex_none()
                        .text_color(theme::muted())
                        .child(label)
                        .when_some(detail, |d, name| d.child(div().text_xs().child(name))),
                )
                .child(div().flex_1().min_w_0().child(current))
                .child(
                    v_flex()
                        .flex_1()
                        .min_w_0()
                        .when(!on, |d| d.opacity(0.45))
                        .child(tag)
                        .when(!same, |d| d.child(div().text_color(color).child(proposed)))
                        .when_some(also_in(&row.field), |d, titles| {
                            d.child(
                                div()
                                    .text_xs()
                                    .text_color(theme::muted())
                                    .child(t!("review.also_in", titles = titles).into_owned()),
                            )
                        })
                        .when(!row.close.is_empty(), |d| {
                            let values = row
                                .close
                                .iter()
                                .map(|c| t!("review.quoted", title = c).into_owned())
                                .collect::<Vec<_>>()
                                .join(", ");
                            d.child(
                                div()
                                    .text_xs()
                                    .text_color(theme::warn())
                                    .child(t!("review.close", values = values).into_owned()),
                            )
                        })
                        .when(!row.similar.is_empty(), |d| {
                            let names = row.similar.join(", ");
                            d.child(
                                div()
                                    .text_xs()
                                    .text_color(theme::warn())
                                    .child(t!("review.similar", names = names).into_owned()),
                            )
                        }),
                )
                .into_any_element();
            if matches!(row.field, Field::Fact { .. }) {
                fact_rows.push(el);
            } else {
                people_rows.push(el);
            }
        }
        let grid = table().child(header()).children(people_rows);
        // Hidden when the record gives no other fact.
        let facts = (!fact_rows.is_empty()).then(|| table().child(header()).children(fact_rows));

        // Attachment: repository → source → file → event.
        let chain = p.chain(g);
        let link_row =
            |k: String, v: String, detail: String, created: bool| {
                let (tag, color) = if created {
                    (t!("review.create").into_owned(), theme::ok())
                } else {
                    (t!("review.existing").into_owned(), theme::muted())
                };
                h_flex()
                    .gap_2()
                    .items_start()
                    .child(div().w(px(84.)).flex_none().text_color(theme::muted()).child(k))
                    .child(v_flex().flex_1().min_w_0().child(v).when(!detail.is_empty(), |d| {
                        d.child(div().text_xs().text_color(theme::muted()).child(detail))
                    }))
                    .child(div().flex_none().text_xs().font_weight(FontWeight::SEMIBOLD).text_color(color).child(tag))
            };
        let mut links = v_flex().gap_1p5().pl_3().border_l_2().border_color(theme::border_strong()).text_sm();
        if let Some(r) = &chain.repository {
            links = links.child(link_row(
                t!("review.repository").into_owned(),
                r.value.clone(),
                r.detail.clone(),
                r.created,
            ));
        }
        if let Some(s) = &chain.source {
            let mut detail = s.detail.clone();
            if chain.same_register > 0 {
                detail = t!("review.same_register", register = detail, n = chain.same_register).into_owned();
            }
            links = links.child(link_row(t!("review.source").into_owned(), s.value.clone(), detail, s.created));
        }
        for place in &chain.places {
            links = links.child(link_row(
                t!("review.place").into_owned(),
                place.value.clone(),
                t!("review.located", coords = place.detail).into_owned(),
                place.created,
            ));
        }
        if let Some(img) = &item.pending.image {
            let planned = planned_file(g, &p)
                .unwrap_or_else(|| img.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default());
            let detail = match (&item.pending.original, item.pending.pdf_page) {
                (Some(_), Some(page)) if img.extension().is_some_and(|e| e == "jpg") => {
                    t!("review.extracted", page = page).into_owned()
                }
                _ => String::new(),
            };
            links = links.child(link_row(t!("review.file").into_owned(), planned, detail, true));
        }
        if let Ok(url) = gedcom::weblink::normalize_url(&p.url) {
            let host = gedcom::weblink::host(&url).to_string();
            let created = g.media_by_file(&url).is_none();
            links = links.child(link_row(t!("review.web_link").into_owned(), host, String::new(), created));
        }
        links = links.child(link_row(
            t!("review.event").into_owned(),
            p.act.event_label(),
            p.act.page.trim().to_string(),
            false,
        ));

        // GEDCOM lines. The file appears under the name it will get, not in the app’s cache.
        let mut diff = p.diff(g).unwrap_or_default();
        if let Some(img) = &item.pending.image
            && let Some(name) = planned_file(g, &p)
        {
            let from = img.to_string_lossy().into_owned();
            for (_, line) in &mut diff {
                if line.contains(&from) {
                    *line = line.replace(&from, &name);
                }
            }
        }
        let open = self.review.gedcom;
        let gedcom = v_flex()
            .gap_1()
            .child(
                h_flex()
                    .id("review-gedcom")
                    .gap_1()
                    .items_center()
                    .cursor_pointer()
                    .text_color(theme::muted())
                    .child(icon(if open { IconName::ChevronDown } else { IconName::ChevronRight }))
                    .child(label_caps(t!("review.gedcom").into_owned()))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.review.gedcom = !this.review.gedcom;
                        cx.notify();
                    })),
            )
            .when(open, |d| {
                d.child(
                    v_flex()
                        .p_2()
                        .rounded(theme::RADIUS_SM)
                        .border_1()
                        .border_color(theme::border())
                        .font_family("monospace")
                        .text_xs()
                        .children(diff.into_iter().map(|(c, line)| {
                            let color = match c {
                                '+' => theme::ok(),
                                '-' => theme::danger(),
                                _ => theme::muted(),
                            };
                            div().text_color(color).child(format!("{c} {line}"))
                        })),
                )
            });

        let transcription = p.act.transcription.trim().to_string();
        let url = item.pending.proposal.url.trim().to_string();
        let can_save = problem.is_none();
        v_flex()
            .w(px(500.))
            .flex_none()
            .border_l_1()
            .border_color(theme::border())
            .child(
                v_flex()
                    .px_4()
                    .pt_4()
                    .pb_2()
                    .gap_0p5()
                    .child(div().text_lg().font_weight(FontWeight::SEMIBOLD).child(title(g, full)))
                    .child(div().text_sm().text_color(theme::muted()).child(sub)),
            )
            .child(
                v_flex()
                    .id("review-detail")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .px_4()
                    .py_2()
                    .gap_4()
                    .when_some(problem, |d, e| d.child(div().text_sm().text_color(theme::danger()).child(e)))
                    .child(v_flex().gap_1().child(label_caps(t!("review.record").into_owned())).child(grid))
                    .when_some(facts, |d, facts| {
                        d.child(v_flex().gap_1().child(label_caps(t!("review.facts").into_owned())).child(facts))
                    })
                    .child(v_flex().gap_1().child(label_caps(t!("review.attachment").into_owned())).child(links))
                    .when(!transcription.is_empty(), |d| {
                        d.child(
                            v_flex()
                                .gap_1()
                                .child(label_caps(t!("review.transcription").into_owned()))
                                .child(div().text_sm().italic().child(transcription)),
                        )
                    })
                    .when(!url.is_empty(), |d| {
                        d.child(
                            h_flex().child(
                                Button::new("review-url")
                                    .ghost()
                                    .small()
                                    .icon(IconName::Globe)
                                    .label(t!("review.open_page").into_owned())
                                    .on_click(move |_, _, cx| cx.open_url(&url)),
                            ),
                        )
                    })
                    .child(gedcom),
            )
            .child(
                h_flex()
                    .px_4()
                    .py_3()
                    .gap_2()
                    .border_t_1()
                    .border_color(theme::border())
                    .child(
                        Button::new("review-reject")
                            .ghost()
                            .small()
                            .label(t!("review.reject").into_owned())
                            .on_click(cx.listener(|this, _, _, cx| this.reject_proposal(cx))),
                    )
                    .child(div().flex_1())
                    .child(
                        Button::new("review-edit")
                            .small()
                            .label(t!("review.edit").into_owned())
                            .on_click(cx.listener(|this, _, w, cx| this.edit_proposal(w, cx))),
                    )
                    .child(
                        Button::new("review-save")
                            .primary()
                            .small()
                            .disabled(!can_save)
                            .label(t!("review.save").into_owned())
                            .on_click(cx.listener(|this, _, w, cx| this.accept_proposal(w, cx))),
                    ),
            )
    }
}

/// Name the record’s file will get according to File Settings › Media (without a template: its own
/// name, next to the .ged).
fn planned_file(g: &Gedcom, p: &Proposal) -> Option<String> {
    let mut copy = g.clone();
    let res = p.apply(&mut copy).ok()?;
    let media = res.media?;
    let to = copy.media_renames(&media).into_iter().next().map(|(_, _, to)| to)?;
    let path = std::path::Path::new(&to);
    if path.is_absolute() { path.file_name().map(|n| n.to_string_lossy().into_owned()) } else { Some(to) }
}

/// Files of the media left in the app’s cache (no naming template): copied next to the .ged.
/// Returns `true` if a file was copied.
fn copy_out_of_cache(g: &mut Gedcom, base: &std::path::Path, media: &str) -> Result<bool, String> {
    let cache = crate::agent::propose::cache_dir();
    let Some(node) = g.get_mut(media) else { return Ok(false) };
    let mut copied = false;
    let mut taken = std::collections::HashSet::new();
    for f in node.children.iter_mut().filter(|c| c.tag == "FILE") {
        let from = std::path::PathBuf::from(f.val());
        if !from.starts_with(&cache) {
            continue;
        }
        let name = from.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        let to = media_files::free_name(base, &name, &mut taken);
        std::fs::copy(&from, base.join(&to)).map_err(|e| e.to_string())?;
        f.value = Some(to);
        copied = true;
    }
    Ok(copied)
}
