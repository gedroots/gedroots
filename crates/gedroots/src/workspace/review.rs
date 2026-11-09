//! “To Review” page: the records proposed by an AI agent, compared with the tree before being
//! saved.
//!
//! The agent never writes: its proposals wait here. Each one is also kept in the file (`_GR_PROP`,
//! with the agent's request while it waits, then what became of it), so that it is queued again
//! when the file opens and every agent knows what was saved or rejected. For each one, the
//! page shows the record’s image, an “In the tree” / “Proposed” comparison (each person can be
//! unchecked), the attachment (repository, source, file, event) and, on request, the exact GEDCOM
//! lines that will change. “Save to Tree” writes everything at once (a single undo point) and
//! organizes the file according to File Settings › Media; “Reject…” removes it without writing
//! anything, with the user's reason, or asks the agent for changes: it proposes it again
//! (`replaces`), and the correction shows what the user asked. Changes for a Data Quality issue are
//! listed after the acts (`review_change.rs`).

use super::*;
use crate::agent::propose::{self, Outcome, Pending, Queued};
use crate::t;
use gedcom::Conflict;
use gedcom::agent_proposals::{ProposalKind, StoredProposal};
use gedcom::proposal::{Change, Field, Proposal, SourceChoice};
use gpui_kit::component::Selectable;
use gpui_kit::component::checkbox::Checkbox;
use gpui_kit::component::input::{InputEvent, Textarea, TextareaState};
use gpui_kit::component::menu::{DropdownMenu, PopupMenuItem};
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
    /// Chapters of books written by an agent, listed last.
    pub(super) chapters: Vec<super::review_book::ChapterItem>,
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
    pub(super) decided: Vec<propose::Decision>,
    /// Temporary id → person saved in the tree.
    saved: std::collections::HashMap<String, String>,
    /// “Reject…” open: the user's reason, or what the agent should change.
    pub(super) rejecting: Option<Entity<TextareaState>>,
    /// Its text changes (the button follows) and ⌘↩.
    reject_sub: Option<Subscription>,
}

impl Review {
    pub(super) fn is_empty(&self) -> bool {
        self.total() == 0
    }

    /// Rows of the list: acts, changes, chapters.
    pub(super) fn total(&self) -> usize {
        self.items.len() + self.changes.len() + self.chapters.len()
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

/// What became of a proposal, for the agent.
pub(super) fn outcome_text(o: Outcome) -> &'static str {
    match o {
        Outcome::Saved => "the user saved it",
        Outcome::Rejected => "the user rejected it",
        Outcome::ChangesRequested => "the user asked for changes: propose it again with `replaces`",
        Outcome::Edited => "the user opened it in record entry",
        Outcome::Withdrawn => "it was withdrawn",
        Outcome::Replaced => "it was replaced",
        Outcome::Waiting => "it is waiting",
    }
}

/// The people of the tree an act names (linked, not temporary).
fn linked(p: &Proposal) -> Vec<String> {
    p.act
        .participants
        .iter()
        .filter_map(|x| match &x.choice {
            gedcom::PersonChoice::Link(id) if !propose::is_temp(id) => Some(id.clone()),
            _ => None,
        })
        .collect()
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
    /// Id for a new proposal (`P3`): one more than any given in this window or kept in the file.
    pub(super) fn next_proposal_id(&mut self) -> String {
        let kept = self.doc.as_ref().map(|d| d.ged.next_proposal_number()).unwrap_or(1) as usize;
        self.review.last_id = self.review.last_id.max(kept.saturating_sub(1)) + 1;
        format!("P{}", self.review.last_id)
    }

    /// Keeps a proposal in the file. It is not a change of the tree's data: no undo point, but
    /// the file has changes to save.
    pub(super) fn keep_proposal(&mut self, p: StoredProposal) {
        if let Some(doc) = &mut self.doc {
            doc.ged.store_agent_proposal(&p);
            doc.dirty = true;
        }
    }

    /// Records in the file what became of a proposal, and tells its agent (`news`).
    pub(super) fn decide_proposal(&mut self, id: &str, outcome: Outcome, note: &str) {
        if let Some(doc) = &mut self.doc
            && doc.ged.decide_agent_proposal(id, outcome, note)
        {
            doc.dirty = true;
        }
        let note = note.trim().to_string();
        self.review.decided.push(propose::Decision { id: id.to_string(), outcome, note, new_people: Vec::new() });
    }

    /// The proposal `id` corrects after the user asked for changes, and what they asked.
    pub(super) fn requested_changes(&self, id: &str) -> Option<String> {
        let g = &self.doc.as_ref()?.ged;
        let asked = g.agent_proposal(&g.agent_proposal(id)?.corrects?)?;
        Some(asked.note).filter(|n| !n.trim().is_empty())
    }

    /// Whether `r` is a proposal the user asked to correct, not answered yet.
    pub(super) fn awaits_correction(&self, r: &str) -> bool {
        self.doc.as_ref().is_some_and(|d| d.ged.agent_awaits_correction(r))
    }

    /// The proposal a new one corrects: given by `replaces` when the user asked for changes on it,
    /// or the one the waiting proposal it replaces corrected.
    pub(super) fn corrected_by(&self, replaces: Option<&str>) -> Option<String> {
        let r = replaces?;
        if self.awaits_correction(r) {
            return Some(r.to_string());
        }
        self.doc.as_ref()?.ged.agent_proposal(r)?.corrects
    }

    /// Queues again the proposals the file keeps as waiting, oldest first, as their agent sent
    /// them, under the same ids. One that no longer fits the tree is withdrawn, with the reason.
    pub(super) fn requeue_proposals(&mut self, cx: &mut Context<Self>) {
        let Some(doc) = &self.doc else { return };
        let waiting: Vec<StoredProposal> =
            doc.ged.agent_proposals().into_iter().filter(|p| p.outcome == Outcome::Waiting).collect();
        if waiting.is_empty() {
            return;
        }
        cx.spawn(async move |this, cx| {
            for p in waiting {
                let data: serde_json::Value = serde_json::from_str(&p.data).unwrap_or_default();
                let params = data["params"].clone();
                let result = match data["tool"].as_str().unwrap_or("") {
                    "propose_act" => {
                        let Ok(state) =
                            this.update(cx, |ws, _| ws.doc.as_ref().map(|d| (d.ged.clone(), ws.review_queue())))
                        else {
                            return;
                        };
                        let Some((ged, queue)) = state else { return };
                        let agent = p.agent.clone();
                        let built = cx
                            .background_spawn(async move {
                                let params: propose::ProposeParams =
                                    serde_json::from_value(params).map_err(|e| e.to_string())?;
                                propose::build(&params, &ged, &queue, &agent)
                            })
                            .await;
                        let temps: Vec<Option<String>> =
                            serde_json::from_value(data["temps"].clone()).unwrap_or_default();
                        this.update(cx, |ws, cx| {
                            let mut pending = built?.pending;
                            pending.id = Some(p.id.clone());
                            pending.temps.resize(temps.len().max(pending.temps.len()), None);
                            for (t, kept) in pending.temps.iter_mut().zip(&temps) {
                                if t.is_none() {
                                    *t = kept.clone();
                                }
                            }
                            ws.queue_proposal(pending, false, cx).map(|_| ())
                        })
                    }
                    "propose_change" => this.update(cx, |ws, cx| {
                        let params: crate::agent::change::ChangeParams =
                            serde_json::from_value(params).map_err(|e| e.to_string())?;
                        let ged = ws.doc.as_ref().map(|d| d.ged.clone()).ok_or("no file")?;
                        let mut req = crate::agent::change::to_request(&params, &ged)?;
                        (req.id, req.agent) = (Some(p.id.clone()), p.agent.clone());
                        ws.add_change(req, cx).map(|_| ())
                    }),
                    "propose_chapter" => this.update(cx, |ws, cx| {
                        let params: crate::agent::book::ProposeChapterParams =
                            serde_json::from_value(params).map_err(|e| e.to_string())?;
                        let ged = ws.doc.as_ref().map(|d| d.ged.clone()).ok_or("no file")?;
                        let mut req = crate::agent::book::to_request(&params, &ged)?;
                        (req.id, req.agent) = (Some(p.id.clone()), p.agent.clone());
                        ws.add_chapter(req, cx).map(|_| ())
                    }),
                    _ => Ok(Err("its request was not kept".to_string())),
                };
                match result {
                    Ok(Ok(())) => {}
                    Ok(Err(e)) => {
                        let note = format!("GEDRoots could not queue it again: {e}");
                        let _ = this.update(cx, |ws, cx| {
                            ws.decide_proposal(&p.id, Outcome::Withdrawn, &note);
                            cx.notify();
                        });
                    }
                    Err(_) => return,
                }
            }
        })
        .detach();
    }

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
            Some(r) => match self.review.items.iter().position(|it| &it.id == r) {
                Some(i) => Some(i),
                // Sent back by the user: the correction is a new proposal, which points to it.
                None if g.agent_awaits_correction(r) => None,
                None => return Err(g.agent_not_replaceable(r)),
            },
            None => None,
        };
        let corrects = self.corrected_by(pending.replaces.as_deref());
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
                // Queued again as the file opens: later ones follow.
                gedcom::PersonChoice::Create => {
                    let n = t.as_deref().and_then(|t| t.strip_prefix(propose::TEMP)?.parse().ok()).unwrap_or(0);
                    self.review.last_temp = self.review.last_temp.max(n);
                }
                _ => *t = None,
            }
        }
        let requeued = pending.id.is_some();
        let id = match pending.id.clone() {
            Some(id) => id,
            None => self.next_proposal_id(),
        };
        let temps = pending.temps.clone();
        if !demo {
            let data = serde_json::json!({
                "tool": "propose_act",
                "params": serde_json::from_str::<serde_json::Value>(&pending.request).unwrap_or_default(),
                "temps": temps,
            });
            let stored = StoredProposal {
                id: id.clone(),
                kind: ProposalKind::Act,
                title: self.doc.as_ref().map(|d| title(&d.ged, &pending.proposal)).unwrap_or_default(),
                about: linked(&pending.proposal),
                agent: pending.agent.clone(),
                data: data.to_string(),
                ..self.doc.as_ref().and_then(|d| d.ged.agent_proposal(&id)).unwrap_or_default()
            };
            let stored = StoredProposal { corrects: corrects.or(stored.corrects.clone()), ..stored };
            self.keep_proposal(stored);
        }
        let item = Item { id: id.clone(), pending, excluded: BTreeSet::new(), conflicts: Default::default(), demo };
        match replaced {
            Some(i) => {
                let old = std::mem::replace(&mut self.review.items[i], item);
                self.decide_proposal(&old.id, Outcome::Replaced, &id);
                self.set_status(t!("review.replaced"), false);
            }
            None => {
                self.review.items.push(item);
                let n = self.review.items.len();
                match demo {
                    true => self.set_status(t!("review.demo"), false),
                    // Queued again as the file opens: nothing new to announce.
                    false if requeued => {}
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
        // A change or a chapter: taken out of its list.
        let chapter = self.review.chapters.iter().position(|c| c.id == id);
        if self.take_change_by_id(id, Outcome::Withdrawn) || chapter.is_some_and(|c| self.withdraw_chapter(c)) {
            let total = self.review.total();
            self.review.sel = self.review.sel.min(total.saturating_sub(1));
            self.set_status(t!("review.withdrawn"), false);
            cx.notify();
            return Ok(self.review_count());
        }
        // Sent back by the user: the agent gives up on correcting it.
        if self.awaits_correction(id) {
            self.decide_proposal(id, Outcome::Withdrawn, "");
            cx.notify();
            return Ok(self.review_count());
        }
        let Some(i) = self.review.items.iter().position(|it| it.id == id) else {
            return Err(match self.review.decided.iter().rev().find(|d| d.id == id) {
                Some(d) => format!("proposal {id} is no longer waiting: {}", outcome_text(d.outcome)),
                None => format!("no proposal {id}"),
            });
        };
        let sel = self.review.sel;
        self.review.sel = i;
        self.review_take(Outcome::Withdrawn, "");
        let total = self.review.total();
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

    pub(super) fn review_select(&mut self, i: usize, cx: &mut Context<Self>) {
        self.review.sel = i;
        self.review.rejecting = None;
        cx.notify();
    }

    /// Removes the displayed proposal, remembering what became of it; closes the page if none is
    /// left.
    fn review_take(&mut self, outcome: Outcome, note: &str) -> Option<Item> {
        if self.review.sel >= self.review.items.len() {
            return None;
        }
        self.review.rejecting = None;
        let item = self.review.items.remove(self.review.sel);
        if !item.demo {
            self.decide_proposal(&item.id, outcome, note);
        }
        let total = self.review.total();
        self.review.sel = self.review.sel.min(total.saturating_sub(1));
        if total == 0 {
            self.review.open = false;
        }
        Some(item)
    }

    /// “Reject…”: asks why, in a popover above the button.
    pub(super) fn reject_proposal(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.review.open || self.review.is_empty() {
            return;
        }
        let state = match &self.review.rejecting {
            Some(s) => s.clone(),
            None => {
                let ph = t!("review.reject_placeholder");
                let s = cx.new(|cx| TextareaState::new(window, cx).auto_grow(3, 8).placeholder(ph));
                // ⌘↩ sends the request for changes; typing enables it.
                let sub = cx.subscribe_in(&s, window, |this, _, ev: &InputEvent, window, cx| match ev {
                    InputEvent::PressEnter { secondary: true, .. } => this.confirm_reject(true, window, cx),
                    InputEvent::Change => cx.notify(),
                    _ => {}
                });
                self.review.rejecting = Some(s.clone());
                self.review.reject_sub = Some(sub);
                s
            }
        };
        state.update(cx, |s, cx| s.focus(window, cx));
        cx.notify();
    }

    pub(super) fn close_reject(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.review.reject_sub = None;
        if self.review.rejecting.take().is_some() {
            window.focus(&self.focus, cx);
            cx.notify();
        }
    }

    /// Rejects the displayed proposal with the reason typed (if any); `changes`: asks its agent
    /// to correct it, which needs the reason.
    pub(super) fn confirm_reject(&mut self, changes: bool, window: &mut Window, cx: &mut Context<Self>) {
        let note = self.review.rejecting.as_ref().map(|s| s.read(cx).value().trim().to_string()).unwrap_or_default();
        if changes && note.is_empty() {
            return;
        }
        let outcome = if changes { Outcome::ChangesRequested } else { Outcome::Rejected };
        self.close_reject(window, cx);
        let status = if changes { t!("review.changes_requested") } else { t!("review.rejected") };
        if let Some(i) = self.selected_change() {
            self.take_change(i, outcome, &note);
        } else if let Some(i) = self.selected_chapter() {
            self.take_chapter(i, outcome, &note);
        } else if self.review_take(outcome, &note).is_some() {
            self.tour_did(super::tour::Advance::Decide, cx);
        } else {
            return;
        }
        self.set_status(status, false);
        cx.notify();
    }

    /// The popover of “Reject…”, above the button at the bottom left of the proposal.
    pub(super) fn reject_popover(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let state = self.review.rejecting.as_ref()?;
        let has_text = !state.read(cx).value().trim().is_empty();
        Some(
            v_flex()
                .id("reject-popover")
                .occlude()
                .absolute()
                .left_3()
                .bottom(px(52.))
                .w(px(440.))
                .max_w(relative(0.9))
                .p_3()
                .gap_2()
                .rounded(theme::RADIUS_LG)
                .border_1()
                .border_color(theme::border_strong())
                .bg(theme::panel())
                .shadow_lg()
                .on_mouse_down_out(cx.listener(|this, _, w, cx| this.close_reject(w, cx)))
                .capture_key_down(cx.listener(|this, ev: &KeyDownEvent, w, cx| {
                    if ev.keystroke.key == "escape" {
                        cx.stop_propagation();
                        this.close_reject(w, cx);
                    }
                }))
                .child(div().text_sm().font_weight(FontWeight::SEMIBOLD).child(t!("review.reject_why").into_owned()))
                .child(Textarea::new(state).small())
                .child(
                    h_flex()
                        .gap_2()
                        .items_center()
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .text_xs()
                                .text_color(theme::muted())
                                .child(t!("review.agent_reads").into_owned()),
                        )
                        .child(
                            Button::new("reject-confirm")
                                .small()
                                .label(t!("review.reject").into_owned())
                                .on_click(cx.listener(|this, _, w, cx| this.confirm_reject(false, w, cx))),
                        )
                        .child(
                            Button::new("reject-changes")
                                .primary()
                                .small()
                                .disabled(!has_text)
                                .label(t!("review.request_changes").into_owned())
                                .on_click(cx.listener(|this, _, w, cx| this.confirm_reject(true, w, cx))),
                        ),
                )
                .into_any_element(),
        )
    }

    /// “Your request” above a correction the agent sent after the user asked for changes.
    pub(super) fn your_request(&self, id: &str) -> Option<AnyElement> {
        let asked = self.requested_changes(id)?;
        Some(
            v_flex()
                .gap_1()
                .child(label_caps(t!("review.your_request").into_owned()))
                .child(
                    div()
                        .px_3()
                        .py_2()
                        .rounded(theme::RADIUS_SM)
                        .border_l_2()
                        .border_color(theme::accent())
                        .bg(theme::accent_soft())
                        .text_sm()
                        .child(asked),
                )
                .into_any_element(),
        )
    }

    /// Saves the displayed proposal: an undo point, then the file organized.
    pub(super) fn accept_proposal(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        // ⌘↩ also merges in the merge window.
        if self.merge.is_some() {
            return self.merge_current(window, cx);
        }
        if self.place_merge.is_some() {
            return self.merge_places_current(window, cx);
        }
        if !self.review.open || self.doc.is_none() {
            return;
        }
        // ⌘↩ in “Reject…” sends the request for changes.
        if self.review.rejecting.is_some() {
            return self.confirm_reject(true, window, cx);
        }
        if let Some(i) = self.selected_change() {
            self.accept_change(i, window, cx);
            return;
        }
        if let Some(i) = self.selected_chapter() {
            self.accept_chapter(i, window, cx);
            return;
        }
        let Some(item) = self.review.items.get(self.review.sel) else { return };
        let proposal = item.proposal();
        let temps = item.kept_temps();
        let item_id = item.id.clone();
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
                        doc.ged.relink_agent_proposals(t, x);
                        let name = doc.ged.individual(x).map(|i| i.display_name()).unwrap_or_default();
                        saved.push(propose::SavedPerson { temp: t.clone(), id: x.clone(), name });
                    }
                }
                // The proposal is now about the people it created too.
                if let Some(mut kept) = doc.ged.agent_proposal(&item_id) {
                    for x in res.persons.iter().flatten() {
                        if !kept.about.contains(x) {
                            kept.about.push(x.clone());
                        }
                    }
                    doc.ged.store_agent_proposal(&kept);
                }
                for it in &mut self.review.items {
                    propose::link_known(&mut it.pending.proposal, &it.pending.temps, &self.review.saved);
                }
                self.review_take(Outcome::Saved, "");
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
        self.review_take(Outcome::Edited, "");
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
                    .when(sel, |d| d.bg(theme::accent_soft()))
                    .when(!sel, |d| d.hover(|d| d.bg(theme::hover())))
                    .child(div().font_weight(FontWeight::MEDIUM).child(title(g, &p)))
                    .when(!meta.is_empty(), |d| d.child(div().text_xs().text_color(theme::muted()).child(meta)))
                    .when(self.requested_changes(&item.id).is_some(), |d| {
                        d.child(div().text_xs().text_color(theme::accent()).child(t!("review.corrected").into_owned()))
                    })
                    // A doubtful proposal stands out in the list.
                    .when(p.act.quality.is_some_and(|q| q <= 1), |d| {
                        d.child(
                            div().text_xs().text_color(theme::warn()).child(gedcom::act::quality_label(p.act.quality)),
                        )
                    })
                    .on_click(cx.listener(move |this, _, _, cx| this.review_select(i, cx)))
            }))
            .children(self.review.changes.iter().enumerate().map(|(i, group)| self.render_change_row(i, group, g, cx)))
            .children(self.review.chapters.iter().enumerate().map(|(i, item)| self.render_chapter_row(i, item, g, cx)));
        if let Some(i) = self.selected_chapter() {
            let (detail, bar) = self.render_chapter_detail(i, &self.review.chapters[i], g, cx);
            return review_page(list, detail, bar, self.reject_popover(cx));
        }
        if let Some(i) = self.selected_change() {
            // An image the agent attaches is shown like an act's, beside what changes.
            let image = self.review.changes[i]
                .items
                .first()
                .and_then(|c| super::review_change::change_files(&c.change).into_iter().next())
                .filter(|f| crate::images::image_size(f).is_some());
            let (detail, bar) = self.render_change_detail(i, &self.review.changes[i], g, cx);
            let content = h_flex()
                .size_full()
                .items_stretch()
                .when_some(image, |d, path| {
                    d.child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .relative()
                            .bg(theme::viewer())
                            .child(self.review.zoom.view("review-change-image", path).max_fit(f32::INFINITY)),
                    )
                })
                .child(detail);
            return review_page(list, content, bar, self.reject_popover(cx));
        }
        let Some(item) = self.review.items.get(self.review.sel) else {
            return list.into_any_element();
        };
        // No image: no empty viewer, the proposal takes a single centred column.
        let image = item.pending.preview.is_some() || item.pending.image.is_some();
        let (detail, bar) = self.review_detail(item, g, cx);
        let content = h_flex().size_full().items_stretch().map(|d| match image {
            true => d.child(self.review_viewer(item)).child(detail),
            false => d.child(
                v_flex().flex_1().min_w_0().items_center().child(detail.max_w(px(640.)).size_full().border_l_0()),
            ),
        });
        review_page(list, content, bar, self.reject_popover(cx))
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

    /// The proposal's detail and its action bar.
    fn review_detail(&self, item: &Item, g: &Gedcom, cx: &mut Context<Self>) -> (Div, AnyElement) {
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
        // The agent's confidence: the quality of the evidence written on the citation.
        let quality = Some(gedcom::act::quality_label(full.act.quality)).filter(|q| !q.is_empty());
        let sub = [sosa, Some(by.into_owned()), quality].into_iter().flatten().collect::<Vec<_>>().join(" · ");

        // Comparison. Columns: check box, field, “In the tree”, “Proposed”.
        let line = || h_flex().items_start().gap_2().px_2().py_2().border_t_1().border_color(theme::border());
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
        // A fact the record supports more or less than its event (a birth date in a death record).
        let fact_quality = |field: &Field| {
            let Field::Fact { person, fact, .. } = field else { return None };
            let q = full.act.participants.get(*person)?.facts.get(*fact)?.quality?;
            Some(gedcom::act::quality_label(Some(q)))
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
                        .when_some(fact_quality(&row.field), |d, q| {
                            d.child(div().text_xs().text_color(theme::muted()).child(q))
                        })
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
        let mut links = v_flex().gap_2().pl_3().border_l_1().border_color(theme::border_strong()).text_sm();
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
                gedcom::place_label(&place.value),
                place.detail.clone(),
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
        let remarks = item.pending.remarks.clone();
        let url = item.pending.proposal.url.trim().to_string();
        let can_save = problem.is_none();
        let detail = v_flex()
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
                    .child(div().text_xl().font_weight(FontWeight::SEMIBOLD).child(title(g, full)))
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
                    .children(self.your_request(&item.id))
                    .child(v_flex().gap_1().child(label_caps(t!("review.record").into_owned())).child(grid))
                    .when_some(facts, |d, facts| {
                        d.child(v_flex().gap_1().child(label_caps(t!("review.facts").into_owned())).child(facts))
                    })
                    .child(v_flex().gap_1().child(label_caps(t!("review.attachment").into_owned())).child(links))
                    .when(!remarks.is_empty(), |d| {
                        d.child(
                            v_flex()
                                .gap_1()
                                .child(label_caps(t!("review.remarks").into_owned()))
                                .child(paragraphs(&remarks)),
                        )
                    })
                    .when(!transcription.is_empty(), |d| {
                        d.child(
                            v_flex()
                                .gap_1()
                                .child(label_caps(t!("review.transcription").into_owned()))
                                .child(paragraphs(&transcription)),
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
            );
        let bar = action_bar(
            Button::new("review-reject")
                .ghost()
                .small()
                .selected(self.review.rejecting.is_some())
                .label(t!("review.reject_ellipsis").into_owned())
                .on_click(cx.listener(|this, _, w, cx| this.reject_proposal(w, cx))),
            Some(
                Button::new("review-edit")
                    .small()
                    .label(t!("review.edit").into_owned())
                    .on_click(cx.listener(|this, _, w, cx| this.edit_proposal(w, cx))),
            ),
            Button::new("review-save")
                .primary()
                .small()
                .disabled(!can_save)
                .label(t!("review.save").into_owned())
                .on_click(cx.listener(|this, _, w, cx| this.accept_proposal(w, cx))),
        );
        (detail, bar)
    }
}

/// The page right of the list: the selected proposal above, its action bar below. The bar spans
/// the whole page whatever the proposal shows (an image or not, an act, a change or a chapter),
/// so Save stays in the bottom right corner and a run of proposals can be saved without moving
/// the pointer, like GNOME's action bar or the commit buttons of a window.
/// The reject popover opens above the bar, over Reject.
fn review_page(
    list: impl IntoElement,
    content: impl IntoElement,
    bar: AnyElement,
    popover: Option<AnyElement>,
) -> AnyElement {
    h_flex()
        .flex_1()
        .min_w_0()
        .items_stretch()
        .child(list)
        .child(
            v_flex()
                .relative()
                .flex_1()
                .min_w_0()
                .child(div().flex_1().min_h_0().child(content))
                .child(bar)
                .children(popover),
        )
        .into_any_element()
}

/// Reject on the left, then the optional secondary action and Save on the right.
pub(super) fn action_bar(reject: Button, middle: Option<Button>, save: Button) -> AnyElement {
    h_flex()
        .flex_none()
        .px_4()
        .py_3()
        .gap_2()
        .border_t_1()
        .border_color(theme::border())
        .child(reject)
        .child(div().flex_1())
        .children(middle)
        .child(save)
        .into_any_element()
}

/// Name the record’s file will get according to File Settings › Media (without a template: its own
/// name, next to the .ged).
/// Text written by an agent, as it wrote it: one block per line, an empty line as a paragraph
/// break. Unusual spaces (no-break, thin, zero-width…) become ordinary ones for display: some
/// were drawn with no width, gluing words together (“13août1850”); the text itself is kept.
fn paragraphs(text: &str) -> Div {
    let mut out = v_flex().gap_2().text_sm();
    let mut block = v_flex().gap_0p5();
    let mut lines = 0;
    for line in text.lines() {
        let line = display_spaces(line.trim_end());
        if line.trim().is_empty() {
            if lines > 0 {
                out = out.child(block);
                block = v_flex().gap_0p5();
                lines = 0;
            }
            continue;
        }
        block = block.child(div().child(line));
        lines += 1;
    }
    if lines > 0 {
        out = out.child(block);
    }
    out
}

/// `s` with every space character drawn as an ordinary space and invisible joiners removed.
fn display_spaces(s: &str) -> String {
    s.chars()
        .filter(|c| !matches!(c, '\u{200b}' | '\u{2060}' | '\u{feff}' | '\u{ad}'))
        .map(|c| if c != ' ' && c != '\t' && c.is_whitespace() { ' ' } else { c })
        .collect()
}

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
pub(super) fn copy_out_of_cache(g: &mut Gedcom, base: &std::path::Path, media: &str) -> Result<bool, String> {
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

#[cfg(test)]
mod tests {
    use super::display_spaces;

    #[test]
    fn unusual_spaces_are_shown_as_spaces() {
        assert_eq!(display_spaces("13\u{a0}août\u{202f}1850, fiche\u{2009}I27"), "13 août 1850, fiche I27");
        assert_eq!(display_spaces("Hör\u{ad}mann\u{200b}\u{feff}"), "Hörmann");
        assert_eq!(display_spaces("a\tb"), "a\tb");
    }
}
