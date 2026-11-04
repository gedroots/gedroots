//! Changes for a Data Quality issue, or corrections without one, in “To Review”: proposed by an AI
//! agent (`propose_change`), checked against the file, then saved or rejected by the user.
//!
//! The page stays sober: one row per change in the list, and a single column with what changes
//! (“In the tree” / “Proposed”), the agent's reason and its evidence. The automatic fixes of a
//! same kind are gathered into one row, approved at once.

use super::*;
use crate::agent::change::ChangeRequest;
use gedcom::analysis::check::Issue;
use gedcom::change::{Action, QualityChange, find_issue};
use gedcom::proposal::Change;
use gedcom::quality;
use gpui_kit::component::checkbox::Checkbox;
use rust_i18n::t;
use std::collections::BTreeSet;

/// A change and the issues it would create.
pub(super) struct ChangeItem {
    pub(super) change: QualityChange,
    new_issues: Vec<String>,
}

/// One row of the list: a change, or the automatic fixes of one kind.
pub(super) struct ChangeGroup {
    pub(super) items: Vec<ChangeItem>,
    /// Fixes of the group the user unchecked.
    excluded: BTreeSet<usize>,
}

impl ChangeGroup {
    /// Kind of issue whose fixes are gathered here (an automatic fix that applies to a list).
    fn batch(&self) -> Option<&'static str> {
        let c = &self.items.first()?.change;
        let code = c.issue.as_ref()?.code;
        (c.action == Action::Fix && quality::fix_of(code).is_some_and(|f| f.batch.is_some())).then_some(code)
    }

    fn kept(&self) -> impl Iterator<Item = &ChangeItem> {
        self.items.iter().enumerate().filter(|(i, _)| !self.excluded.contains(i)).map(|(_, c)| c)
    }

    fn title(&self, g: &Gedcom) -> String {
        match self.batch() {
            Some(code) => quality::fix_of(code)
                .and_then(|f| f.batch)
                .map(|b| format!("{b} ({})", self.items.len()))
                .unwrap_or_default(),
            None => self.items.first().map(|c| c.change.title(g)).unwrap_or_default(),
        }
    }
}

fn change_color(c: Change) -> Hsla {
    match c {
        Change::Same => theme::muted(),
        Change::Differs | Change::Refined => theme::warn(),
        _ => theme::ok(),
    }
}

impl Workspace {
    /// Issues of a file, like the Quality page (media files checked too).
    fn rerun_quality(&self) -> impl Fn(&Gedcom) -> Vec<Issue> + '_ {
        move |g: &Gedcom| {
            let Some(doc) = &self.doc else { return Vec::new() };
            let exists = |f: &str| pages::file_path(doc, f).is_some_and(|p| p.exists());
            let files = quality::Files { exists: &exists, folder: &self.media_files };
            quality::run_with(g, Some(&files))
        }
    }

    /// Proposals waiting: acts and changes.
    pub(super) fn review_count(&self) -> usize {
        self.review.items.len()
            + self.review.changes.iter().map(|g| g.items.len()).sum::<usize>()
            + self.review.chapters.len()
    }

    /// Is a change waiting for this issue?
    pub(super) fn change_pending(&self, issue: &Issue) -> bool {
        let key = quality::issue_key(issue);
        self.review
            .changes
            .iter()
            .flat_map(|g| &g.items)
            .any(|c| c.change.issue.as_ref().is_some_and(|i| quality::issue_key(i) == key))
    }

    /// Change received from the agent: checked against the displayed file, then queued. Returns
    /// the number of proposals waiting and the issues the change would create.
    pub(crate) fn add_change(
        &mut self,
        req: ChangeRequest,
        cx: &mut Context<Self>,
    ) -> Result<(usize, Vec<String>), String> {
        if self.doc.is_none() {
            return Err("no file is open in GEDRoots".into());
        }
        self.refresh_quality();
        let issues = self.quality.issues.clone();
        // Without an issue: a correction the agent read in a document.
        let issue = match &req.issue_id {
            None => None,
            Some(id) => {
                let issue = find_issue(&issues, id)
                    .ok_or_else(|| format!("no issue `{id}` any more: call `quality_issues` again"))?;
                if issue.ignored && req.action == Action::Hide {
                    return Err("this issue is already hidden".into());
                }
                if self.change_pending(issue) {
                    return Err("a change for this issue is already waiting for review".into());
                }
                Some(issue.clone())
            }
        };
        let change = QualityChange {
            issue,
            issue_id: req.issue_id,
            action: req.action,
            reason: req.reason,
            evidence: req.evidence,
        };
        let new = {
            let rerun = self.rerun_quality();
            change.check(&self.doc.as_ref().unwrap().ged, &rerun)?
        };
        let new_issues: Vec<String> = new.into_iter().map(|i| i.message).collect();
        let item = ChangeItem { change, new_issues: new_issues.clone() };
        let single = ChangeGroup { items: vec![item], excluded: BTreeSet::new() };
        match single.batch().and_then(|code| self.review.changes.iter_mut().find(|g| g.batch() == Some(code))) {
            Some(group) => group.items.extend(single.items),
            None => self.review.changes.push(single),
        }
        let n = self.review_count();
        self.set_status(crate::tn!("review.change_received", n), false);
        self.quality.stale = true;
        cx.notify();
        Ok((n, new_issues))
    }

    /// The displayed change group, when the selection is past the acts.
    pub(super) fn selected_change(&self) -> Option<usize> {
        let i = self.review.sel.checked_sub(self.review.items.len())?;
        (i < self.review.changes.len()).then_some(i)
    }

    pub(super) fn take_change(&mut self, i: usize) -> ChangeGroup {
        let group = self.review.changes.remove(i);
        let total = self.review.total();
        self.review.sel = self.review.sel.min(total.saturating_sub(1));
        if total == 0 {
            self.review.open = false;
        }
        self.quality.stale = true;
        group
    }

    /// After the merge window merged two people or set them apart: their proposed merge is done.
    pub(super) fn drop_merge_changes(&mut self, a: &str, b: &str) {
        let same =
            |c: &ChangeItem| c.change.merge_pair().is_some_and(|(x, y)| (x == a && y == b) || (x == b && y == a));
        while let Some(i) = self.review.changes.iter().position(|g| g.items.iter().any(same)) {
            self.take_change(i);
        }
    }

    pub(super) fn reject_change(&mut self, i: usize, cx: &mut Context<Self>) {
        self.take_change(i);
        self.set_status(t!("review.rejected"), false);
        cx.notify();
    }

    /// Saves the displayed change (or the checked fixes of the group): one undo point.
    pub(super) fn accept_change(&mut self, i: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(doc) = &self.doc else { return };
        let Some(group) = self.review.changes.get(i) else { return };
        let kept: Vec<&ChangeItem> = group.kept().collect();
        if kept.is_empty() {
            return;
        }
        // A merge is decided in the merge window, the agent's choices already ticked.
        if let (Action::Merge(choices), Some(pair)) = (&kept[0].change.action, kept[0].change.merge_pair()) {
            let choices = choices.clone();
            return self.open_merge_prefilled(pair, &choices, cx);
        }
        let title = group.title(&doc.ged);
        // Checked on a copy first: an error leaves the tree as it is, with its reason.
        let mut copy = doc.ged.clone();
        let result = if group.batch().is_some() {
            let issues: Vec<Issue> = kept.iter().filter_map(|c| c.change.issue.clone()).collect();
            if copy.fix_issues(&issues) == 0 { Err(t!("review.outdated").into_owned()) } else { Ok(()) }
        } else {
            kept[0].change.apply(&mut copy)
        };
        if let Err(e) = result {
            self.set_error("review.failed", e);
            cx.notify();
            return;
        }
        self.take_change(i);
        let msg = t!("review.change_saved", title = title).into_owned();
        self.before_agent_change();
        self.edit_ged(
            move |g| {
                *g = copy;
                true
            },
            msg,
            window,
            cx,
        );
        self.record_agent_change(title, cx);
    }

    /// Row of the list.
    pub(super) fn render_change_row(
        &self,
        i: usize,
        group: &ChangeGroup,
        g: &Gedcom,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let n = self.review.items.len() + i;
        let sel = self.review.sel == n;
        // What the change does, in a few words: the title already names the issue.
        let c = &group.items[0].change;
        let meta = match (&c.action, group.batch()) {
            (_, Some(_)) => t!("review.automatic").into_owned(),
            (Action::Hide, _) => t!("review.hide_issue").into_owned(),
            (Action::Merge(_), _) => c
                .rows(g)
                .into_iter()
                .filter(|r| r.change == Change::Differs)
                .map(|r| format!("{} → {}", r.label, r.proposed))
                .chain(Some(t!("review.merge_people").into_owned()))
                .collect::<Vec<_>>()
                .join(" · "),
            _ => c.rows(g).into_iter().map(|r| format!("{} → {}", r.label, r.proposed)).collect::<Vec<_>>().join(" · "),
        };
        v_flex()
            .id(SharedString::from(format!("review-change-{i}")))
            .px_3()
            .py_2()
            .gap_0p5()
            .cursor_pointer()
            .border_b_1()
            .border_color(theme::border())
            .when(sel, |d| d.bg(theme::accent_soft()).border_l_2().border_color(theme::accent()))
            .when(!sel, |d| d.hover(|d| d.bg(theme::hover())))
            .child(div().font_weight(FontWeight::MEDIUM).child(group.title(g)))
            .child(div().text_xs().text_color(theme::muted()).truncate().child(meta))
            .on_click(cx.listener(move |this, _, _, cx| {
                this.review.sel = n;
                cx.notify();
            }))
            .into_any_element()
    }

    /// Detail: what changes, why, and the evidence, in one column.
    pub(super) fn render_change_detail(
        &self,
        i: usize,
        group: &ChangeGroup,
        g: &Gedcom,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let batch = group.batch().is_some();
        let first = &group.items[0];
        let c = &first.change;
        let mut body = v_flex().gap_4();

        if batch {
            // One line per fix, each can be unchecked.
            let rows = group.items.iter().enumerate().map(|(k, item)| {
                let on = !group.excluded.contains(&k);
                h_flex()
                    .gap_2()
                    .items_start()
                    .px_2()
                    .py_1p5()
                    .border_t_1()
                    .border_color(theme::border())
                    .child(Checkbox::new(SharedString::from(format!("change-keep-{k}"))).checked(on).on_click(
                        cx.listener(move |this, keep: &bool, _, cx| {
                            if let Some(gr) = this.review.changes.get_mut(i) {
                                if *keep {
                                    gr.excluded.remove(&k);
                                } else {
                                    gr.excluded.insert(k);
                                }
                            }
                            cx.notify();
                        }),
                    ))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .when(!on, |d| d.opacity(0.45))
                            .child(item.change.subject().to_string()),
                    )
            });
            body = body.child(
                v_flex().border_1().border_color(theme::border()).rounded(theme::RADIUS_SM).text_sm().children(rows),
            );
        } else if c.action != Action::Hide {
            // Hiding changes no value: the issue (under the title) and the reason say it all.
            let header = h_flex()
                .gap_2()
                .px_2()
                .py_1()
                .bg(theme::panel())
                .text_xs()
                .text_color(theme::muted())
                .child(div().w(px(110.)).flex_none())
                .child(div().flex_1().min_w_0().child(t!("review.in_tree").into_owned()))
                .child(div().flex_1().min_w_0().child(t!("review.proposed").into_owned()));
            let rows = c.rows(g).into_iter().map(|r| {
                let color = change_color(r.change);
                h_flex()
                    .gap_2()
                    .items_start()
                    .px_2()
                    .py_1p5()
                    .border_t_1()
                    .border_color(theme::border())
                    .child(div().w(px(110.)).flex_none().text_color(theme::muted()).child(r.label))
                    .child(div().flex_1().min_w_0().child(r.current))
                    .child(div().flex_1().min_w_0().text_color(color).child(r.proposed))
            });
            body = body.child(
                v_flex()
                    .border_1()
                    .border_color(theme::border())
                    .rounded(theme::RADIUS_SM)
                    .text_sm()
                    .child(header)
                    .children(rows),
            );
        }

        if !c.reason.is_empty() && !batch {
            body = body.child(
                v_flex()
                    .gap_1()
                    .child(label_caps(t!("review.why").into_owned()))
                    .child(div().text_sm().child(c.reason.clone())),
            );
        }
        let ev = &c.evidence;
        if !ev.is_empty() && !batch {
            let cited = ev
                .source
                .as_deref()
                .map(|s| g.record_label(s))
                .into_iter()
                .chain(Some(ev.page.clone()).filter(|p| !p.is_empty()))
                .collect::<Vec<_>>()
                .join(" · ");
            let url = ev.url.clone();
            body = body.child(
                v_flex()
                    .gap_1()
                    .child(label_caps(t!("review.evidence").into_owned()))
                    .when(!cited.is_empty(), |d| d.child(div().text_sm().child(cited)))
                    .when(!ev.quote.is_empty(), |d| {
                        d.child(div().text_sm().italic().child(format!("« {} »", ev.quote)))
                    })
                    .when(!url.is_empty(), |d| {
                        d.child(
                            h_flex().child(
                                Button::new("change-url")
                                    .ghost()
                                    .small()
                                    .icon(IconName::Globe)
                                    .label(t!("review.open_page").into_owned())
                                    .on_click(move |_, _, cx| cx.open_url(&url)),
                            ),
                        )
                    }),
            );
        }
        let new: Vec<String> = group.kept().flat_map(|c| c.new_issues.iter().cloned()).collect();
        if !new.is_empty() {
            body = body.child(
                div()
                    .text_sm()
                    .text_color(theme::warn())
                    .child(t!("review.new_issues", list = new.join(" ; ")).into_owned()),
            );
        }
        // Exact GEDCOM lines, on request (not for a list of fixes: the lines say little).
        if !batch {
            let open = self.review.gedcom;
            let diff = c.diff(g).unwrap_or_default();
            body = body.child(
                v_flex()
                    .gap_1()
                    .child(
                        h_flex()
                            .id("change-gedcom")
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
                                .children(diff.into_iter().map(|(k, line)| {
                                    let color = match k {
                                        '+' => theme::ok(),
                                        '-' => theme::danger(),
                                        _ => theme::muted(),
                                    };
                                    div().text_color(color).child(format!("{k} {line}"))
                                })),
                        )
                    }),
            );
        }

        let kept = group.kept().count();
        let save = if batch {
            t!("review.apply", n = kept).into_owned()
        } else if c.action == Action::Hide {
            t!("review.hide_issue").into_owned()
        } else if matches!(c.action, Action::Merge(_)) {
            t!("review.compare_merge").into_owned()
        } else {
            t!("review.save").into_owned()
        };
        // A correction has no issue: its reason is shown below, once.
        let by = t!("review.by_agent");
        let sub = match &c.issue {
            _ if batch => format!("{} · {by}", t!("review.automatic")),
            Some(i) => format!("{} · {by}", i.message),
            None => by.into_owned(),
        };
        v_flex()
            .flex_1()
            .min_w_0()
            .items_center()
            .child(
                v_flex()
                    .size_full()
                    .max_w(px(640.))
                    .child(
                        v_flex()
                            .px_4()
                            .pt_4()
                            .pb_2()
                            .gap_0p5()
                            .child(div().text_lg().font_weight(FontWeight::SEMIBOLD).child(group.title(g)))
                            .child(div().text_sm().text_color(theme::muted()).child(sub)),
                    )
                    .child(
                        v_flex().id("change-detail").flex_1().min_h_0().overflow_y_scroll().px_4().py_2().child(body),
                    )
                    .child(
                        h_flex()
                            .px_4()
                            .py_3()
                            .gap_2()
                            .border_t_1()
                            .border_color(theme::border())
                            .child(
                                Button::new("change-reject")
                                    .ghost()
                                    .small()
                                    .label(t!("review.reject").into_owned())
                                    .on_click(cx.listener(move |this, _, _, cx| this.reject_change(i, cx))),
                            )
                            .child(div().flex_1())
                            .child(
                                Button::new("change-save")
                                    .primary()
                                    .small()
                                    .disabled(kept == 0)
                                    .label(save)
                                    .on_click(cx.listener(move |this, _, w, cx| this.accept_change(i, w, cx))),
                            ),
                    ),
            )
            .into_any_element()
    }
}
