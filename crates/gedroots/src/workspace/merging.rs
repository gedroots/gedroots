//! “Merge Two People” window: two individuals compared line by line, then merged.
//!
//! What is the same takes one line, what does not contradict is added (“+”), and only contradictory
//! facts (two births, two names) ask for a choice, which the user makes: nothing is preselected and
//! Merge stays greyed out until every conflict has a value. The window steps through the possible
//! duplicates: Merge (⌘↩) and Later go to the next pair, Not a Duplicate sets it aside for good
//! (`_GR_DISTINCT`). Each merge is one undo point (see `gedcom::merge`).

use super::*;
use gedcom::duplicates::{Duplicate, Exclusion, Level};
use gedcom::merge::{MergePlan, RowKind, Side};

/// The window and the pairs it steps through.
pub(super) struct MergeDialog {
    queue: Vec<(String, String)>,
    at: usize,
    /// Comparison of the current pair, or why it cannot be merged.
    compare: Result<Duplicate, Exclusion>,
    plan: Option<MergePlan>,
    /// Value chosen for each conflict of `plan`.
    choices: Vec<Option<Side>>,
}

impl MergeDialog {
    fn current(&self) -> Option<&(String, String)> {
        self.queue.get(self.at)
    }
}

impl Workspace {
    /// Opens the window on `queue[at]`; pairs that no longer exist are skipped.
    pub(super) fn open_merge(&mut self, queue: Vec<(String, String)>, at: usize, cx: &mut Context<Self>) {
        let mut d = MergeDialog { queue, at, compare: Err(Exclusion::SamePerson), plan: None, choices: vec![] };
        if self.load_pair(&mut d) {
            self.merge = Some(d);
        } else {
            self.merge = None;
            self.set_status(t!("merge.none_left"), false);
        }
        cx.notify();
    }

    /// Prepares the pair at `d.at`, or the next one that still exists. `false` when none is left.
    fn load_pair(&self, d: &mut MergeDialog) -> bool {
        let Some(g) = self.doc.as_ref().map(|d| &d.ged) else { return false };
        while let Some((a, b)) = d.current().cloned() {
            if g.individual(&a).is_some() && g.individual(&b).is_some() {
                d.compare = g.compare_individuals(&a, &b);
                d.plan = g.merge_plan(&a, &b).ok();
                d.choices = vec![None; d.plan.as_ref().map_or(0, |p| p.conflicts.len())];
                return true;
            }
            d.queue.remove(d.at);
        }
        false
    }

    /// Person › Merge With…: the selected person and the chosen one.
    pub(super) fn ask_merge_with(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(indi) = self.selected_individual() {
            self.open_picker(relatives::PickPurpose::MergeWith { indi }, window, cx);
        }
    }

    /// Person › Find Duplicates: the possible duplicates of the selected person.
    pub(super) fn find_duplicates(&mut self, cx: &mut Context<Self>) {
        if let Some(x) = self.selected_individual() {
            self.find_duplicates_of(&x, cx);
        }
    }

    /// The possible duplicates of `x`, in the merge window.
    pub(super) fn find_duplicates_of(&mut self, x: &str, cx: &mut Context<Self>) {
        let Some(doc) = &self.doc else { return };
        let queue: Vec<(String, String)> = doc.ged.duplicates_of(x).into_iter().map(|d| (d.a, d.b)).collect();
        if queue.is_empty() {
            self.set_status(t!("merge.no_duplicate", name = doc.ged.record_label(x)), false);
            cx.notify();
            return;
        }
        self.open_merge(queue, 0, cx);
    }

    /// The possible duplicates of the Quality page, from the clicked one.
    pub(super) fn merge_from_quality(&mut self, issue: &gedcom::analysis::check::Issue, cx: &mut Context<Self>) {
        let pairs: Vec<(String, String)> = self
            .quality
            .issues
            .iter()
            .filter(|i| i.code == "possible_duplicate" && !i.ignored)
            .filter_map(|i| Some((i.xref.clone(), i.related.first()?.clone())))
            .collect();
        let me = (issue.xref.clone(), issue.related.first().cloned().unwrap_or_default());
        let at = pairs.iter().position(|p| *p == me).unwrap_or(0);
        if issue.ignored || pairs.is_empty() {
            self.open_merge(vec![me], 0, cx);
        } else {
            self.open_merge(pairs, at, cx);
        }
    }

    pub(super) fn close_merge(&mut self, cx: &mut Context<Self>) -> bool {
        let open = self.merge.take().is_some();
        cx.notify();
        open
    }

    /// Next pair (Later, or after a merge); the window closes after the last one.
    fn merge_advance(&mut self, done: bool, cx: &mut Context<Self>) {
        let Some(mut d) = self.merge.take() else { return };
        if done {
            d.queue.remove(d.at);
        } else {
            d.at += 1;
        }
        if d.at >= d.queue.len() {
            d.at = 0;
        }
        if !done && d.at == 0 && d.queue.len() <= 1 {
            cx.notify();
            return;
        }
        if self.load_pair(&mut d) {
            self.merge = Some(d);
        }
        cx.notify();
    }

    fn merge_step(&mut self, delta: isize, cx: &mut Context<Self>) {
        let Some(mut d) = self.merge.take() else { return };
        let n = d.queue.len() as isize;
        d.at = ((d.at as isize + delta).rem_euclid(n.max(1))) as usize;
        if self.load_pair(&mut d) {
            self.merge = Some(d);
        }
        cx.notify();
    }

    fn can_merge(d: &MergeDialog) -> bool {
        d.compare.is_ok() && d.plan.is_some() && d.choices.iter().all(Option::is_some)
    }

    /// Merge (⌘↩): one undo point, then the next pair.
    pub(super) fn merge_current(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(d) = self.merge.as_ref().filter(|d| Self::can_merge(d)) else { return };
        let plan = d.plan.clone().unwrap();
        let choices: Vec<Side> = d.choices.iter().map(|c| c.unwrap()).collect();
        let name = self.doc.as_ref().map(|d| d.ged.record_label(&plan.keep)).unwrap_or_default();
        let mut kept = None;
        self.edit_ged(
            |g| {
                kept = g.merge_individuals(&plan, &choices).ok();
                kept.is_some()
            },
            t!("merge.merged", name = name).to_string(),
            window,
            cx,
        );
        // The open record was the merged-away person: show the kept one.
        if self.selected.as_deref() == Some(plan.other.as_str())
            && let Some(k) = kept
        {
            self.goto(k, window, cx);
        }
        self.merge_advance(true, cx);
    }

    /// Not a Duplicate: `_GR_DISTINCT`, then the next pair.
    fn merge_distinct(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some((a, b)) = self.merge.as_ref().and_then(|d| d.current().cloned()) else { return };
        let name = self.doc.as_ref().map(|d| d.ged.record_label(&a)).unwrap_or_default();
        self.edit_ged(|g| g.mark_distinct(&a, &b), t!("merge.marked_distinct", name = name).to_string(), window, cx);
        self.merge_advance(true, cx);
    }

    fn merge_choose(&mut self, conflict: usize, side: Side, cx: &mut Context<Self>) {
        if let Some(c) = self.merge.as_mut().and_then(|d| d.choices.get_mut(conflict)) {
            *c = Some(side);
        }
        cx.notify();
    }

    pub(super) fn render_merge(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let d = self.merge.as_ref()?;
        let g = &self.doc.as_ref()?.ged;
        let (a, b) = d.current()?.clone();
        let (keep, other) = d.plan.as_ref().map(|p| (p.keep.clone(), p.other.clone())).unwrap_or((a, b));

        // Title, level, position in the list.
        let n = d.queue.len();
        let level = d.compare.as_ref().ok().map(|c| {
            let color = if c.level == Level::Likely { theme::accent() } else { theme::muted() };
            div()
                .px_2()
                .rounded(px(6.))
                .border_1()
                .border_color(color)
                .text_color(color)
                .text_xs()
                .font_weight(FontWeight::SEMIBOLD)
                .child(c.level.label())
        });
        let pager = (n > 1).then(|| {
            h_flex()
                .gap_2()
                .items_center()
                .child(
                    Button::new("merge-prev")
                        .ghost()
                        .xsmall()
                        .icon(IconName::ChevronLeft)
                        .tooltip(t!("merge.previous"))
                        .on_click(cx.listener(|this, _, _, cx| this.merge_step(-1, cx))),
                )
                .child(mono(t!("merge.position", n = d.at + 1, total = n).to_string()))
                .child(
                    Button::new("merge-next")
                        .ghost()
                        .xsmall()
                        .icon(IconName::ChevronRight)
                        .tooltip(t!("merge.next"))
                        .on_click(cx.listener(|this, _, _, cx| this.merge_step(1, cx))),
                )
        });
        let head = h_flex()
            .px_6()
            .pt_4()
            .pb_2()
            .gap_3()
            .items_center()
            .child(div().text_xl().font_weight(FontWeight::SEMIBOLD).child(t!("merge.title")))
            .children(level)
            .child(div().flex_1())
            .children(pager);

        // For and against, or why the pair cannot be merged.
        let why = match &d.compare {
            Ok(c) => h_flex().px_6().pb_3().gap_2().flex_wrap().children(c.clues.iter().take(6).map(|clue| {
                let (sign, color) = if clue.is_for() { ("+", theme::ok()) } else { ("−", theme::danger()) };
                h_flex()
                    .gap_1()
                    .px_2()
                    .py_0p5()
                    .rounded(theme::RADIUS_SM)
                    .bg(theme::surface())
                    .text_xs()
                    .child(div().font_weight(FontWeight::BOLD).text_color(color).child(sign))
                    .child(clue.text.clone())
            })),
            Err(e) => h_flex().px_6().pb_3().text_color(theme::danger()).child(e.label()),
        };

        // One line per fact.
        let person = |x: &str| {
            let i = g.individual(x);
            let years = i.as_ref().map(|i| i.years()).unwrap_or_default();
            let (father, mother) = g.parents(x);
            let parents: Vec<String> = [father, mother].into_iter().flatten().map(|p| p.display_name()).collect();
            let sub = if parents.is_empty() {
                years
            } else {
                let p = t!("merge.child_of", parents = parents.join(t!("merge.and").as_ref())).to_string();
                if years.is_empty() { p } else { format!("{years} · {p}") }
            };
            v_flex()
                .flex_1()
                .min_w_0()
                .px_3()
                .py_2()
                .child(div().font_weight(FontWeight::SEMIBOLD).child(g.record_label(x)))
                .when(!sub.is_empty(), |d| d.child(div().text_xs().text_color(theme::muted()).child(sub)))
        };
        let label_cell = |s: String| div().w(px(120.)).flex_none().px_3().py_2().text_color(theme::muted()).child(s);
        let values = |vals: &[String], added: bool| {
            v_flex().flex_1().min_w_0().px_3().py_2().gap_0p5().children(vals.iter().map(move |v| {
                h_flex()
                    .gap_1()
                    .when(added, |d| {
                        d.child(div().text_color(theme::ok()).font_weight(FontWeight::SEMIBOLD).child("+"))
                    })
                    .child(v.clone())
            }))
        };
        let mut rows: Vec<AnyElement> = vec![
            h_flex()
                .bg(theme::panel())
                .border_b_1()
                .border_color(theme::border())
                .child(div().w(px(120.)).flex_none())
                .child(person(&keep))
                .child(person(&other))
                .into_any_element(),
        ];
        if let Some(plan) = &d.plan {
            for (r, row) in plan.rows.iter().enumerate() {
                let line = h_flex()
                    .items_start()
                    .border_b_1()
                    .border_color(theme::border())
                    .child(label_cell(row.label.clone()));
                let line = match &row.kind {
                    RowKind::Same => line.child(
                        h_flex()
                            .flex_1()
                            .px_3()
                            .py_2()
                            .gap_2()
                            .child(div().flex_1().children(row.keep.iter().map(|v| div().child(v.clone()))))
                            .child(div().text_xs().text_color(theme::muted()).child(t!("merge.same"))),
                    ),
                    RowKind::Combined => line
                        .child(values(&row.keep, false))
                        .child(values(&row.other, false))
                        .child(div().px_3().py_2().text_xs().text_color(theme::muted()).child(t!("merge.combined"))),
                    RowKind::Added => line
                        .child(values(&row.keep, !row.keep.is_empty() && row.keep != row.other))
                        .child(values(&row.other, !row.other.is_empty() && row.keep != row.other)),
                    RowKind::Conflict(c) => {
                        let c = *c;
                        let chosen = d.choices.get(c).copied().flatten();
                        let option = |side: Side, vals: &[String]| {
                            let on = chosen == Some(side);
                            let off = chosen.is_some() && !on;
                            h_flex()
                                .id(SharedString::from(format!("merge-{r}-{side:?}")))
                                .flex_1()
                                .min_w_0()
                                .px_3()
                                .py_2()
                                .gap_2()
                                .items_start()
                                .cursor_pointer()
                                .hover(|d| d.bg(theme::hover()))
                                .child(
                                    div()
                                        .mt(px(2.))
                                        .size(px(16.))
                                        .flex_none()
                                        .rounded_full()
                                        .border_1()
                                        .border_color(if on { theme::accent() } else { theme::field_border() })
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .when(on, |d| d.child(div().size(px(8.)).rounded_full().bg(theme::accent()))),
                                )
                                .child(
                                    div()
                                        .when(off, |d| d.text_color(theme::muted()).line_through())
                                        .children(vals.iter().map(|v| div().child(v.clone()))),
                                )
                                .on_click(cx.listener(move |this, _, _, cx| this.merge_choose(c, side, cx)))
                        };
                        line.bg(if chosen.is_none() { theme::accent_soft() } else { theme::bg() })
                            .child(option(Side::Keep, &row.keep))
                            .child(option(Side::Other, &row.other))
                    }
                };
                rows.push(line.into_any_element());
            }
        }
        let grid = v_flex()
            .mx_6()
            .rounded(theme::RADIUS_SM)
            .border_1()
            .border_color(theme::border())
            .overflow_hidden()
            .children(rows);

        // Buttons: Not a Duplicate apart; Later and Merge in the platform's order.
        let enabled = Self::can_merge(d);
        let merge = Button::new("merge-do")
            .primary()
            .label(t!("merge.merge"))
            .disabled(!enabled)
            .when(!enabled && d.compare.is_ok(), |b| b.tooltip(t!("merge.merge_tooltip")))
            .on_click(cx.listener(|this, _, w, cx| this.merge_current(w, cx)));
        let later = Button::new("merge-later")
            .label(if n > 1 { t!("merge.later") } else { t!("merge.cancel") })
            .on_click(cx.listener(move |this, _, _, cx| {
                if n > 1 {
                    this.merge_advance(false, cx)
                } else {
                    this.close_merge(cx);
                }
            }));
        let buttons = if cfg!(target_os = "windows") {
            [merge.into_any_element(), later.into_any_element()]
        } else {
            [later.into_any_element(), merge.into_any_element()]
        };
        let distinct = (d.compare.is_ok()).then(|| {
            Button::new("merge-distinct")
                .label(t!("merge.not_duplicate"))
                .on_click(cx.listener(|this, _, w, cx| this.merge_distinct(w, cx)))
        });
        let foot = h_flex()
            .px_6()
            .py_4()
            .gap_2()
            .items_center()
            .border_t_1()
            .border_color(theme::border())
            .children(distinct)
            .child(div().flex_1())
            .children(buttons);

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
                        .id("merge")
                        .occlude()
                        .w(px(820.))
                        .max_h(px(680.))
                        .rounded(theme::RADIUS_LG)
                        .border_1()
                        .border_color(theme::border_strong())
                        .bg(theme::bg())
                        .shadow_lg()
                        .on_key_down(cx.listener(|this, ev: &KeyDownEvent, w, cx| {
                            if ev.keystroke.key == "escape" {
                                this.close_merge(cx);
                            } else if ev.keystroke.key == "enter" && ev.keystroke.modifiers.secondary(cx) {
                                this.merge_current(w, cx);
                            }
                        }))
                        .child(head)
                        .child(why)
                        .child(div().id("merge-body").flex_1().min_h_0().overflow_y_scroll().pb_4().child(grid))
                        .child(foot),
                )
                .into_any_element(),
        )
    }
}
