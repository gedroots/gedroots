//! “Merge Two Places” window: two spellings of a place compared level by level, then merged.
//!
//! Like the people's merge window: identical levels take one line, a level present on one side
//! only is added (“+”), and two different values ask for a choice that the user makes (nothing is
//! preselected, Merge stays greyed out until every conflict has a value). The localities of both
//! places follow them. The window steps through the possible duplicates: Merge (⌘↩) and Later go
//! to the next pair, Not a Duplicate sets it aside for good (`HEAD._GR_DISTINCT`). Each merge is
//! one undo point (see `gedcom::place_merge`).

use super::*;
use gedcom::duplicates::Level;
use gedcom::merge::Side;
use gedcom::place_merge::{Coords, PlaceDuplicate, PlacePlan, RowKind};

/// The window and the pairs it steps through.
pub(super) struct PlaceMergeDialog {
    queue: Vec<(String, String)>,
    at: usize,
    /// Why the pair is suggested (none for a pair chosen with Merge with Another Place…).
    suggestion: Option<PlaceDuplicate>,
    plan: Option<PlacePlan>,
    /// Value chosen for each conflict of `plan`.
    choices: Vec<Option<Side>>,
}

impl PlaceMergeDialog {
    fn current(&self) -> Option<&(String, String)> {
        self.queue.get(self.at)
    }
}

impl Workspace {
    /// Opens the window on `queue[at]`; pairs that no longer exist are skipped.
    pub(super) fn open_place_merge(&mut self, queue: Vec<(String, String)>, at: usize, cx: &mut Context<Self>) {
        let mut d = PlaceMergeDialog { queue, at, suggestion: None, plan: None, choices: vec![] };
        if self.load_place_pair(&mut d) {
            self.place_merge = Some(d);
        } else {
            self.place_merge = None;
            self.set_status(t!("merge.no_place_left"), false);
        }
        cx.notify();
    }

    /// Prepares the pair at `d.at`, or the next one that still exists. `false` when none is left.
    fn load_place_pair(&self, d: &mut PlaceMergeDialog) -> bool {
        let Some(g) = self.doc.as_ref().map(|d| &d.ged) else { return false };
        while let Some((a, b)) = d.current().cloned() {
            if let Some(plan) = g.place_merge_plan(&a, &b) {
                d.suggestion = g.place_duplicates_of(&a).into_iter().find(|s| s.a == b || s.b == b);
                d.choices = vec![None; plan.conflicts];
                d.plan = Some(plan);
                return true;
            }
            d.queue.remove(d.at);
            if d.at >= d.queue.len() {
                d.at = 0;
            }
        }
        false
    }

    /// The place shown on the Places page.
    pub(super) fn selected_place(&self) -> Option<String> {
        let g = &self.doc.as_ref()?.ged;
        let name = self.selected.clone().filter(|_| self.view == View::Places)?;
        g.places().iter().any(|p| p.name == name).then_some(name)
    }

    /// Place › Merge with Another Place…
    pub(super) fn ask_merge_place_with(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(place) = self.selected_place() {
            self.open_picker(relatives::PickPurpose::MergePlaceWith { place }, window, cx);
        }
    }

    /// Place › Find Duplicate Places: those of the place shown, otherwise all of them.
    pub(super) fn find_duplicate_places(&mut self, cx: &mut Context<Self>) {
        let Some(doc) = &self.doc else { return };
        let place = self.selected_place();
        let found = match &place {
            Some(p) => doc.ged.place_duplicates_of(p),
            None => doc.ged.place_duplicates(),
        };
        if found.is_empty() {
            let msg = match &place {
                Some(p) => t!("merge.no_place_duplicate", name = short_place(p)),
                None => t!("merge.no_place_left"),
            };
            self.set_status(msg, false);
            cx.notify();
            return;
        }
        self.open_place_merge(found.into_iter().map(|d| (d.a, d.b)).collect(), 0, cx);
    }

    /// The possible duplicate places of the Quality page, from the clicked one.
    pub(super) fn place_merge_from_quality(&mut self, issue: &gedcom::analysis::check::Issue, cx: &mut Context<Self>) {
        let pair = |i: &gedcom::analysis::check::Issue| match &i.related[..] {
            [a, b] => Some((a.clone(), b.clone())),
            _ => None,
        };
        let Some(me) = pair(issue) else { return };
        let pairs: Vec<(String, String)> = self
            .quality
            .issues
            .iter()
            .filter(|i| i.code == "possible_duplicate_place" && !i.ignored)
            .filter_map(pair)
            .collect();
        match pairs.iter().position(|p| *p == me) {
            Some(at) if !issue.ignored => self.open_place_merge(pairs, at, cx),
            _ => self.open_place_merge(vec![me], 0, cx),
        }
    }

    pub(super) fn close_place_merge(&mut self, cx: &mut Context<Self>) -> bool {
        let open = self.place_merge.take().is_some();
        cx.notify();
        open
    }

    /// Next pair (Later, or after a merge); the window closes after the last one.
    fn place_merge_advance(&mut self, done: bool, cx: &mut Context<Self>) {
        let Some(mut d) = self.place_merge.take() else { return };
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
        if self.load_place_pair(&mut d) {
            self.place_merge = Some(d);
        }
        cx.notify();
    }

    fn place_merge_step(&mut self, delta: isize, cx: &mut Context<Self>) {
        let Some(mut d) = self.place_merge.take() else { return };
        let n = d.queue.len() as isize;
        d.at = ((d.at as isize + delta).rem_euclid(n.max(1))) as usize;
        if self.load_place_pair(&mut d) {
            self.place_merge = Some(d);
        }
        cx.notify();
    }

    fn can_merge_places(d: &PlaceMergeDialog) -> bool {
        d.plan.is_some() && d.choices.iter().all(Option::is_some)
    }

    /// Merge (⌘↩): one undo point, then the next pair.
    pub(super) fn merge_places_current(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(d) = self.place_merge.as_ref().filter(|d| Self::can_merge_places(d)) else { return };
        let plan = d.plan.clone().unwrap();
        let choices: Vec<Side> = d.choices.iter().map(|c| c.unwrap()).collect();
        let (a, b) = (short_place(&plan.a), short_place(&plan.b));
        self.restore_point(t!("versions.before_merge", a = a, b = b).into());
        let mut result = None;
        self.edit_ged(
            |g| {
                result = g.merge_places(&plan, &choices);
                result.is_some()
            },
            t!("merge.places_merged", a = a, b = b).to_string(),
            window,
            cx,
        );
        // The place shown was one of the two: show the merged one.
        if let Some(r) = result
            && self.view == View::Places
            && (self.selected.as_deref() == Some(plan.a.as_str()) || self.selected.as_deref() == Some(plan.b.as_str()))
        {
            self.goto_place(r, window, cx);
        }
        self.place_merge_advance(true, cx);
    }

    /// Not a Duplicate: `HEAD._GR_DISTINCT`, then the next pair.
    fn place_merge_distinct(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some((a, b)) = self.place_merge.as_ref().and_then(|d| d.current().cloned()) else { return };
        let msg = t!("merge.marked_distinct", name = short_place(&a)).to_string();
        self.edit_ged(|g| g.mark_places_distinct(&a, &b), msg, window, cx);
        self.place_merge_advance(true, cx);
    }

    fn place_merge_choose(&mut self, conflict: usize, side: Side, cx: &mut Context<Self>) {
        if let Some(c) = self.place_merge.as_mut().and_then(|d| d.choices.get_mut(conflict)) {
            *c = Some(side);
        }
        cx.notify();
    }

    pub(super) fn render_place_merge(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let d = self.place_merge.as_ref()?;
        let plan = d.plan.as_ref()?;

        // Title, level, position in the list.
        let n = d.queue.len();
        let level = d.suggestion.as_ref().map(|s| {
            let color = if s.level == Level::Likely { theme::accent() } else { theme::muted() };
            div()
                .px_2()
                .rounded(px(6.))
                .border_1()
                .border_color(color)
                .text_color(color)
                .text_xs()
                .font_weight(FontWeight::SEMIBOLD)
                .child(s.level.label())
        });
        let pager = (n > 1).then(|| {
            h_flex()
                .gap_2()
                .items_center()
                .child(
                    Button::new("place-merge-prev")
                        .ghost()
                        .xsmall()
                        .icon(IconName::ChevronLeft)
                        .tooltip(t!("merge.previous"))
                        .on_click(cx.listener(|this, _, _, cx| this.place_merge_step(-1, cx))),
                )
                .child(mono(t!("merge.position", n = d.at + 1, total = n).to_string()))
                .child(
                    Button::new("place-merge-next")
                        .ghost()
                        .xsmall()
                        .icon(IconName::ChevronRight)
                        .tooltip(t!("merge.next"))
                        .on_click(cx.listener(|this, _, _, cx| this.place_merge_step(1, cx))),
                )
        });
        let head = h_flex()
            .px_6()
            .pt_4()
            .pb_2()
            .gap_3()
            .items_center()
            .child(div().text_xl().font_weight(FontWeight::SEMIBOLD).child(t!("merge.places_title")))
            .children(level)
            .child(div().flex_1())
            .children(pager);
        let why = d.suggestion.as_ref().map(|s| {
            h_flex().px_6().pb_3().gap_2().flex_wrap().children(s.clues.iter().map(|clue| {
                h_flex()
                    .gap_1()
                    .px_2()
                    .py_0p5()
                    .rounded(theme::RADIUS_SM)
                    .bg(theme::surface())
                    .text_xs()
                    .child(div().font_weight(FontWeight::BOLD).text_color(theme::ok()).child("+"))
                    .child(clue.clone())
            }))
        });

        // One line per level; identical ones gathered on one line.
        let label_cell =
            |s: SharedString| div().w(px(130.)).flex_none().px_3().py_2().text_color(theme::muted()).child(s);
        let line = || h_flex().items_start().border_b_1().border_color(theme::border());
        let value = |v: String, added: bool| {
            h_flex()
                .flex_1()
                .min_w_0()
                .px_3()
                .py_2()
                .gap_1()
                .when(added && !v.is_empty(), |d| {
                    d.child(div().text_color(theme::ok()).font_weight(FontWeight::SEMIBOLD).child("+"))
                })
                .child(v)
        };
        let same = |v: String| {
            h_flex()
                .flex_1()
                .px_3()
                .py_2()
                .gap_2()
                .child(div().flex_1().child(v))
                .child(div().text_xs().text_color(theme::muted()).child(t!("merge.same")))
        };
        let option = |id: String, c: usize, side: Side, v: String| {
            let chosen = d.choices.get(c).copied().flatten();
            let on = chosen == Some(side);
            let off = chosen.is_some() && !on;
            h_flex()
                .id(SharedString::from(id))
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
                .child(div().when(off, |d| d.text_color(theme::muted()).line_through()).child(v))
                .on_click(cx.listener(move |this, _, _, cx| this.place_merge_choose(c, side, cx)))
        };
        let conflict_bg =
            |c: usize| if d.choices.get(c).copied().flatten().is_none() { theme::accent_soft() } else { theme::bg() };

        let header = |name: &str, uses: usize, records: usize| {
            v_flex()
                .flex_1()
                .min_w_0()
                .px_3()
                .py_2()
                .child(div().font_weight(FontWeight::SEMIBOLD).child(short_place(name)))
                .child(div().text_xs().text_color(theme::muted()).child(t!(
                    "merge.place_uses",
                    uses = crate::tn!("places.uses", uses),
                    records = crate::tn!("merge.records", records)
                )))
        };
        let mut rows: Vec<AnyElement> = vec![
            h_flex()
                .bg(theme::panel())
                .border_b_1()
                .border_color(theme::border())
                .child(div().w(px(130.)).flex_none())
                .child(header(&plan.a, plan.uses.0, plan.records.0))
                .child(header(&plan.b, plan.uses.1, plan.records.1))
                .into_any_element(),
        ];
        let identical: Vec<&gedcom::place_merge::LevelRow> =
            plan.rows.iter().filter(|r| r.kind == RowKind::Same && !r.a.is_empty()).collect();
        for (i, r) in plan.rows.iter().enumerate() {
            match r.kind {
                RowKind::Same => {}
                RowKind::Added => rows.push(
                    line()
                        .child(label_cell(r.label.clone().into()))
                        .child(value(r.a.clone(), true))
                        .child(value(r.b.clone(), true))
                        .into_any_element(),
                ),
                RowKind::Conflict(c) => rows.push(
                    line()
                        .bg(conflict_bg(c))
                        .child(label_cell(r.label.clone().into()))
                        .child(option(format!("pm-{i}-a"), c, Side::Keep, r.a.clone()))
                        .child(option(format!("pm-{i}-b"), c, Side::Other, r.b.clone()))
                        .into_any_element(),
                ),
            }
        }
        if !identical.is_empty() {
            let names: Vec<&str> = identical.iter().map(|r| r.label.as_str()).collect();
            let values: Vec<&str> = identical.iter().map(|r| r.a.as_str()).collect();
            rows.push(
                line()
                    .px_3()
                    .py_2()
                    .text_xs()
                    .text_color(theme::muted())
                    .child(t!("merge.same_levels", levels = names.join(", "), values = values.join(" · ")).to_string())
                    .into_any_element(),
            );
        }
        let coords_label: SharedString = t!("merge.coordinates").into();
        let text = |side| plan.coords_text(side).unwrap_or_default();
        match plan.coords {
            Coords::None => {}
            Coords::Same => {
                rows.push(line().child(label_cell(coords_label)).child(same(text(Side::Other))).into_any_element())
            }
            Coords::FromA | Coords::FromB => rows.push(
                line()
                    .child(label_cell(coords_label))
                    .child(value(if plan.coords == Coords::FromA { text(Side::Keep) } else { String::new() }, true))
                    .child(value(if plan.coords == Coords::FromB { text(Side::Other) } else { String::new() }, true))
                    .into_any_element(),
            ),
            Coords::Conflict(c, km) => rows.push(
                line()
                    .bg(conflict_bg(c))
                    .child(
                        v_flex()
                            .w(px(130.))
                            .flex_none()
                            .px_3()
                            .py_2()
                            .text_color(theme::muted())
                            .child(coords_label)
                            .child(div().text_xs().child(t!(
                                "merge.apart",
                                km = format!("{km:.1}").replace('.', t!("merge.decimal").as_ref())
                            ))),
                    )
                    .child(option("pm-coords-a".into(), c, Side::Keep, text(Side::Keep)))
                    .child(option("pm-coords-b".into(), c, Side::Other, text(Side::Other)))
                    .into_any_element(),
            ),
        }
        let localities = |l: &[(String, usize)]| {
            v_flex().flex_1().min_w_0().px_3().py_2().gap_0p5().children(l.iter().map(|(name, uses)| {
                let own = gedcom::place_merge::levels(name).into_iter().find(|x| !x.is_empty()).unwrap_or_default();
                h_flex()
                    .gap_1()
                    .child(div().text_color(theme::ok()).font_weight(FontWeight::SEMIBOLD).child("+"))
                    .child(format!("{own} · {}", crate::tn!("places.uses", *uses)))
            }))
        };
        if !plan.localities.0.is_empty() || !plan.localities.1.is_empty() {
            rows.push(
                line()
                    .child(label_cell(t!("merge.localities").into()))
                    .child(localities(&plan.localities.0))
                    .child(localities(&plan.localities.1))
                    .into_any_element(),
            );
        }
        let grid = v_flex()
            .mx_6()
            .rounded(theme::RADIUS_SM)
            .border_1()
            .border_color(theme::border())
            .overflow_hidden()
            .children(rows);

        // What will be written: the full place, empty levels included.
        let preview = plan.preview_with(&d.choices, "…");
        let result = h_flex()
            .mx_6()
            .mt_3()
            .px_3()
            .py_2()
            .gap_2()
            .flex_wrap()
            .rounded(theme::RADIUS_SM)
            .bg(theme::panel())
            .child(div().text_color(theme::muted()).child(t!("merge.result")))
            .child(div().font_weight(FontWeight::SEMIBOLD).child(preview))
            .child(div().text_color(theme::muted()).child(crate::tn!("places.uses", plan.uses.0 + plan.uses.1)));

        // Buttons: Not a Duplicate apart; Later and Merge in the platform's order.
        let enabled = Self::can_merge_places(d);
        let merge = Button::new("place-merge-do")
            .primary()
            .label(t!("merge.merge"))
            .disabled(!enabled)
            .when(!enabled, |b| b.tooltip(t!("merge.merge_tooltip")))
            .on_click(cx.listener(|this, _, w, cx| this.merge_places_current(w, cx)));
        let later = Button::new("place-merge-later")
            .label(if n > 1 { t!("merge.later") } else { t!("merge.cancel") })
            .on_click(cx.listener(move |this, _, _, cx| {
                if n > 1 {
                    this.place_merge_advance(false, cx)
                } else {
                    this.close_place_merge(cx);
                }
            }));
        let buttons = if cfg!(target_os = "windows") {
            [merge.into_any_element(), later.into_any_element()]
        } else {
            [later.into_any_element(), merge.into_any_element()]
        };
        let distinct = d.suggestion.is_some().then(|| {
            Button::new("place-merge-distinct")
                .label(t!("merge.not_duplicate"))
                .on_click(cx.listener(|this, _, w, cx| this.place_merge_distinct(w, cx)))
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
                        .id("place-merge")
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
                                this.close_place_merge(cx);
                            } else if ev.keystroke.key == "enter" && ev.keystroke.modifiers.secondary(cx) {
                                this.merge_places_current(w, cx);
                            }
                        }))
                        .child(head)
                        .children(why)
                        .child(
                            div()
                                .id("place-merge-body")
                                .flex_1()
                                .min_h_0()
                                .overflow_y_scroll()
                                .pb_4()
                                .child(grid)
                                .child(result),
                        )
                        .child(foot),
                )
                .into_any_element(),
        )
    }
}
