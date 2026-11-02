//! “To Review” page: the records proposed by an AI agent, compared with the tree before being
//! saved.
//!
//! The agent never writes: its proposals wait here, in memory, for the session. For each one, the
//! page shows the record’s image, an “In the tree” / “Proposed” comparison (each person can be
//! unchecked), the attachment (repository, source, file, event) and, on request, the exact GEDCOM
//! lines that will change. “Save to Tree” writes everything at once (a single undo point) and
//! organizes the file according to File Settings › Media; “Reject” removes it without writing
//! anything.

use super::*;
use crate::agent::propose::Pending;
use gedcom::proposal::{Change, Field, Proposal, SourceChoice};
use gedcom::research::current_year;
use gpui_kit::component::checkbox::Checkbox;
use rust_i18n::t;
use std::collections::BTreeSet;

/// A proposal and the people the user unchecked.
pub(super) struct Item {
    pending: Pending,
    excluded: BTreeSet<usize>,
}

impl Item {
    /// The proposal, without the unchecked people.
    fn proposal(&self) -> Proposal {
        let mut p = self.pending.proposal.clone();
        let mut i = 0;
        p.act.participants.retain(|_| {
            let keep = !self.excluded.contains(&i);
            i += 1;
            keep
        });
        p
    }
}

#[derive(Default)]
pub(super) struct Review {
    items: Vec<Item>,
    /// Page displayed instead of the lists.
    pub(super) open: bool,
    sel: usize,
    /// GEDCOM section expanded.
    gedcom: bool,
}

impl Review {
    pub(super) fn is_empty(&self) -> bool {
        self.items.is_empty()
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

impl Workspace {
    /// Proposal received from the agent: checked against the displayed file, then queued.
    pub(crate) fn add_proposal(&mut self, mut pending: Pending, cx: &mut Context<Self>) -> Result<usize, String> {
        let Some(doc) = &self.doc else { return Err("no file is open in GEDRoots".into()) };
        let g = &doc.ged;
        pending.proposal.complete(g);
        pending.proposal.check(g, current_year())?;
        self.review.items.push(Item { pending, excluded: BTreeSet::new() });
        let n = self.review.items.len();
        self.set_status(crate::tn!("review.received", n), false);
        cx.notify();
        Ok(n)
    }

    pub(super) fn open_review(&mut self, cx: &mut Context<Self>) {
        if self.review.is_empty() {
            self.set_status(t!("review.none"), false);
        } else {
            self.act_editor = None;
            self.review.open = true;
        }
        cx.notify();
    }

    fn review_select(&mut self, i: usize, cx: &mut Context<Self>) {
        self.review.sel = i;
        cx.notify();
    }

    /// Removes the displayed proposal; closes the page if none is left.
    fn review_take(&mut self) -> Option<Item> {
        if self.review.sel >= self.review.items.len() {
            return None;
        }
        let item = self.review.items.remove(self.review.sel);
        self.review.sel = self.review.sel.min(self.review.items.len().saturating_sub(1));
        if self.review.items.is_empty() {
            self.review.open = false;
        }
        Some(item)
    }

    pub(super) fn reject_proposal(&mut self, cx: &mut Context<Self>) {
        if !self.review.open {
            return;
        }
        if self.review_take().is_some() {
            self.set_status(t!("review.rejected"), false);
        }
        cx.notify();
    }

    /// Saves the displayed proposal: an undo point, then the file organized.
    pub(super) fn accept_proposal(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.review.open || self.doc.is_none() {
            return;
        }
        let Some(item) = self.review.items.get(self.review.sel) else { return };
        let proposal = item.proposal();
        let base = self.ged_dir();
        // The extracted image lives in the app’s cache: it must join the .ged folder.
        if base.is_none() && item.pending.image.is_some() {
            self.set_status(t!("review.save_ged_first"), true);
            cx.notify();
            return;
        }
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
                self.review_take();
                self.set_status(res.summary() + &note, false);
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
                self.set_status(e, true);
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
        let act = match &proposal.source {
            SourceChoice::Existing(x) => gedcom::Act { source: x.clone(), ..proposal.act.clone() },
            SourceChoice::New(_) => {
                self.checkpoint();
                match proposal.materialize(&mut self.doc.as_mut().unwrap().ged) {
                    Ok(act) => act,
                    Err(e) => {
                        self.drop_checkpoint();
                        self.set_status(e, true);
                        cx.notify();
                        return;
                    }
                }
            }
        };
        self.review_take();
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
                                .child(self.review.items.len().to_string()),
                        )
                        .on_click(cx.listener(|this, _, _, cx| this.open_review(cx))),
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
            }));
        let Some(item) = self.review.items.get(self.review.sel) else {
            return list.into_any_element();
        };
        h_flex()
            .flex_1()
            .min_w_0()
            .items_stretch()
            .child(list)
            .child(self.review_viewer(item))
            .child(self.review_detail(item, g, cx))
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
            .flex()
            .items_center()
            .justify_center()
            .p_4()
            .map(|d| match pending.preview.as_ref().or(pending.image.as_ref()) {
                Some(path) => {
                    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                    d.child(
                        crate::images::picture(path.clone(), 1600., 1600., ObjectFit::Contain)
                            .size_full()
                            // Never a silent black viewer: say which file could not be shown.
                            .with_fallback(move || {
                                div()
                                    .size_full()
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .text_color(theme::image_muted())
                                    .child(t!("review.unreadable_image", file = name.clone()).into_owned())
                                    .into_any_element()
                            }),
                    )
                }
                None => d.child(div().text_color(theme::image_muted()).child(t!("review.no_image").into_owned())),
            })
            .when_some(caption, |d, c| {
                d.child(
                    div()
                        .absolute()
                        .bottom_3()
                        .right_3()
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

    fn review_detail(&self, item: &Item, g: &Gedcom, cx: &mut Context<Self>) -> AnyElement {
        let full = &item.pending.proposal;
        let p = item.proposal();
        // The proposal may no longer fit the tree (changed since): it stays visible, but is not
        // saved.
        let problem = p.check(g, current_year()).err();
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
        let sub =
            [sosa, Some(t!("review.by_agent").into_owned())].into_iter().flatten().collect::<Vec<_>>().join(" · ");

        // Comparison. Columns: check box, field, “In the tree”, “Proposed”.
        let line = || h_flex().items_start().gap_2().px_2().py_1p5().border_t_1().border_color(theme::border());
        let mut grid = v_flex().border_1().border_color(theme::border()).rounded(theme::RADIUS_SM).text_sm().child(
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
                .child(div().flex_1().min_w_0().child(t!("review.proposed").into_owned())),
        );
        for row in &rows {
            let (label, current, proposed, index) = match &row.field {
                Field::Date => {
                    (t!("review.date").into_owned(), show_date(&row.current), show_date(&row.proposed), None)
                }
                Field::Place => (
                    t!("review.place").into_owned(),
                    or_dash(&gedcom::short_place(&row.current)),
                    or_dash(&gedcom::short_place(&row.proposed)),
                    None,
                ),
                Field::Person { index, role } => {
                    (role.clone(), or_dash(&row.current), or_dash(&row.proposed), Some(*index))
                }
            };
            let principal = index.is_some_and(|i| full.act.participants[i].role == gedcom::ActRole::Principal);
            let on = index.is_none_or(|i| !item.excluded.contains(&i));
            let check: AnyElement = match index {
                Some(i) if !principal => Checkbox::new(SharedString::from(format!("review-keep-{i}")))
                    .checked(on)
                    .on_click(cx.listener(move |this, keep: &bool, _, cx| {
                        let sel = this.review.sel;
                        if let Some(it) = this.review.items.get_mut(sel) {
                            if *keep {
                                it.excluded.remove(&i);
                            } else {
                                it.excluded.insert(i);
                            }
                        }
                        cx.notify();
                    }))
                    .into_any_element(),
                _ => div().into_any_element(),
            };
            let (tag, color) = change_tag(row.change);
            let same = row.change == Change::Same;
            grid = grid.child(
                line()
                    .child(div().w(px(16.)).flex_none().child(check))
                    .child(div().w(px(76.)).flex_none().text_color(theme::muted()).child(label))
                    .child(div().flex_1().min_w_0().child(current))
                    .child(
                        v_flex()
                            .flex_1()
                            .min_w_0()
                            .when(!on, |d| d.opacity(0.45))
                            .child(div().text_xs().font_weight(FontWeight::SEMIBOLD).text_color(color).child(tag))
                            .when(!same, |d| d.child(div().text_color(color).child(proposed))),
                    ),
            );
        }

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
            .into_any_element()
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
