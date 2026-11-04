//! Chapters of a family book in “To Review”: what an AI agent wrote for a generation
//! (`propose_chapter`), read as it will be in the book. The context boxes and the hypotheses can be
//! unticked: only the ticked ones are saved into the book, and so into its PDF.

use super::*;
use crate::agent::book::ChapterRequest;
use gedroots_book::Block;
use gpui_kit::component::checkbox::Checkbox;
use rust_i18n::t;
use std::collections::BTreeSet;

/// A chapter waiting for review.
pub(super) struct ChapterItem {
    req: ChapterRequest,
    /// Blocks the user unticked, numbered in `Writing::blocks` order.
    excluded: BTreeSet<usize>,
}

impl Workspace {
    /// Chapter received from the agent (already checked against the book): queued, in place of a
    /// waiting proposal for the same chapter. Returns the number of proposals waiting.
    pub(crate) fn add_chapter(&mut self, req: ChapterRequest, cx: &mut Context<Self>) -> Result<usize, String> {
        let Some(doc) = &self.doc else { return Err("no file is open in GEDRoots".into()) };
        if gedroots_book::spec::book(&doc.ged, &req.book).is_none() {
            return Err(format!("no book `{}`: call read_book", req.book));
        }
        let item = ChapterItem { req, excluded: BTreeSet::new() };
        let same = |c: &ChapterItem| c.req.book == item.req.book && c.req.generation == item.req.generation;
        match self.review.chapters.iter_mut().find(|c| same(c)) {
            Some(slot) => *slot = item,
            None => self.review.chapters.push(item),
        }
        let n = self.review_count();
        self.set_status(crate::tn!("review.chapter_received", n), false);
        cx.notify();
        Ok(n)
    }

    /// The displayed chapter, when the selection is past the acts and the changes.
    pub(super) fn selected_chapter(&self) -> Option<usize> {
        let i = self.review.sel.checked_sub(self.review.items.len() + self.review.changes.len())?;
        (i < self.review.chapters.len()).then_some(i)
    }

    fn take_chapter(&mut self, i: usize) -> ChapterItem {
        let item = self.review.chapters.remove(i);
        let total = self.review.total();
        self.review.sel = self.review.sel.min(total.saturating_sub(1));
        if total == 0 {
            self.review.open = false;
        }
        item
    }

    pub(super) fn reject_chapter(&mut self, i: usize, cx: &mut Context<Self>) {
        self.take_chapter(i);
        self.set_status(t!("review.rejected"), false);
        cx.notify();
    }

    /// “Grandparents · The Marchand Family”.
    fn chapter_title(&self, item: &ChapterItem, g: &Gedcom) -> (String, String) {
        let spec = gedroots_book::spec::book(g, &item.req.book);
        let chapter = spec
            .as_ref()
            .and_then(|s| gedroots_book::outline(g, s).ok())
            .and_then(|o| o.into_iter().find(|c| c.generation == item.req.generation))
            .map(|c| c.title)
            .unwrap_or_else(|| item.req.generation.to_string());
        (chapter, spec.map(|s| self.book_title(&s)).unwrap_or_default())
    }

    /// Saves the ticked blocks into the book: one undo point, and a version named after it.
    pub(super) fn accept_chapter(&mut self, i: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(doc) = &self.doc else { return };
        let Some(item) = self.review.chapters.get(i) else { return };
        let Some(mut spec) = gedroots_book::spec::book(&doc.ged, &item.req.book) else {
            self.set_error("review.failed", t!("review.book_gone"));
            cx.notify();
            return;
        };
        let (chapter, book) = self.chapter_title(item, &doc.ged);
        let kept = item.req.writing.without(&item.excluded);
        let old = spec.chapters.get(&item.req.generation).cloned().unwrap_or_default();
        spec.chapters.insert(item.req.generation, kept.over(&old));
        self.take_chapter(i);
        let title = format!("{chapter} · {book}");
        let msg = t!("review.chapter_saved", title = title).into_owned();
        self.before_agent_change();
        self.edit_ged(
            move |g| {
                gedroots_book::spec::save(g, &spec);
                true
            },
            msg,
            window,
            cx,
        );
        self.record_agent_change(title, cx);
    }

    /// Row of the list.
    pub(super) fn render_chapter_row(
        &self,
        i: usize,
        item: &ChapterItem,
        g: &Gedcom,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let n = self.review.items.len() + self.review.changes.len() + i;
        let sel = self.review.sel == n;
        let (chapter, book) = self.chapter_title(item, g);
        let blocks = item.req.writing.blocks().count();
        let meta = t!("review.chapter_meta", book = book, blocks = crate::tn!("review.blocks", blocks)).into_owned();
        v_flex()
            .id(SharedString::from(format!("review-chapter-{i}")))
            .px_3()
            .py_2()
            .gap_0p5()
            .cursor_pointer()
            .border_b_1()
            .border_color(theme::border())
            .when(sel, |d| d.bg(theme::accent_soft()).border_l_2().border_color(theme::accent()))
            .when(!sel, |d| d.hover(|d| d.bg(theme::hover())))
            .child(div().font_weight(FontWeight::MEDIUM).child(chapter))
            .child(div().text_xs().text_color(theme::muted()).truncate().child(meta))
            .on_click(cx.listener(move |this, _, _, cx| {
                this.review.sel = n;
                cx.notify();
            }))
            .into_any_element()
    }

    /// One block, as the book shows it; context and hypotheses with their box to tick.
    fn render_block(&self, i: usize, k: usize, b: &Block, on: bool, g: &Gedcom, cx: &mut Context<Self>) -> AnyElement {
        let name = |x: &str| g.individual(x).map(|p| p.display_name());
        let paragraphs = |t: &str| {
            gedroots_book::writing::expand(t, name)
                .split('\n')
                .map(str::trim)
                .filter(|p| !p.is_empty())
                .map(|p| div().child(p.to_string()))
                .collect::<Vec<_>>()
        };
        let tick = || {
            Checkbox::new(SharedString::from(format!("chapter-keep-{k}"))).checked(on).on_click(cx.listener(
                move |this, keep: &bool, _, cx| {
                    if let Some(c) = this.review.chapters.get_mut(i) {
                        if *keep {
                            c.excluded.remove(&k);
                        } else {
                            c.excluded.insert(k);
                        }
                    }
                    cx.notify();
                },
            ))
        };
        match b {
            Block::Text(t) => v_flex().gap_2().text_sm().children(paragraphs(t)).into_any_element(),
            Block::Context { title, text, sources } => h_flex()
                .gap_2()
                .items_start()
                .p_3()
                .bg(theme::panel())
                .border_l_2()
                .border_color(theme::warn())
                .rounded(theme::RADIUS_SM)
                .child(tick())
                .child(
                    v_flex()
                        .flex_1()
                        .min_w_0()
                        .gap_1()
                        .text_sm()
                        .when(!on, |d| d.opacity(0.45))
                        .child(label_caps(t!("review.context").into_owned()))
                        .child(div().font_weight(FontWeight::SEMIBOLD).child(title.clone()))
                        .children(paragraphs(text))
                        .children(sources.iter().enumerate().map(|(s, src)| {
                            let url = src.url.clone();
                            h_flex()
                                .id(SharedString::from(format!("chapter-src-{k}-{s}")))
                                .gap_1()
                                .text_xs()
                                .text_color(theme::muted())
                                .child(src.title.clone())
                                .when(!url.is_empty(), |d| {
                                    d.cursor_pointer()
                                        .text_color(theme::accent())
                                        .child(icon(IconName::ExternalLink))
                                        .on_click(move |_, _, cx| cx.open_url(&url))
                                })
                        })),
                )
                .into_any_element(),
            Block::Hypothesis { text, basis } => h_flex()
                .gap_2()
                .items_start()
                .px_3()
                .py_2()
                .border_l_2()
                .border_color(theme::accent())
                .child(tick())
                .child(
                    v_flex()
                        .flex_1()
                        .min_w_0()
                        .gap_1()
                        .text_sm()
                        .when(!on, |d| d.opacity(0.45))
                        .child(label_caps(t!("review.hypothesis").into_owned()).text_color(theme::accent()))
                        .child(v_flex().gap_2().italic().children(paragraphs(text)))
                        .child(
                            div()
                                .text_xs()
                                .text_color(theme::muted())
                                .child(t!("review.basis", list = basis.join(" · ")).into_owned()),
                        ),
                )
                .into_any_element(),
        }
    }

    /// Detail: the chapter as the book will read, in one column.
    pub(super) fn render_chapter_detail(
        &self,
        i: usize,
        item: &ChapterItem,
        g: &Gedcom,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let (chapter, book) = self.chapter_title(item, g);
        let w = &item.req.writing;
        let mut k = 0;
        let mut body = v_flex().gap_4();
        let mut section = |heading: String, blocks: &[Block], body: Div| {
            let mut s = v_flex().gap_3().child(label_caps(heading));
            for b in blocks {
                s = s.child(self.render_block(i, k, b, !item.excluded.contains(&k), g, cx));
                k += 1;
            }
            body.child(s)
        };
        if !w.intro.is_empty() {
            body = section(t!("review.intro").into_owned(), &w.intro, body);
        }
        for (x, blocks) in &w.people {
            let name = g.individual(x).map(|p| p.display_name()).unwrap_or_else(|| x.clone());
            body = section(name, blocks, body);
        }
        if !item.req.remarks.is_empty() {
            body = body.child(
                v_flex()
                    .gap_1()
                    .child(label_caps(t!("review.remarks").into_owned()))
                    .child(div().text_sm().child(item.req.remarks.clone())),
            );
        }
        let optional = w.blocks().any(Block::optional);
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
                            .child(div().text_lg().font_weight(FontWeight::SEMIBOLD).child(chapter))
                            .child(
                                div()
                                    .text_sm()
                                    .text_color(theme::muted())
                                    .child(format!("{book} · {}", t!("review.by_agent"))),
                            )
                            .when(optional, |d| {
                                d.child(
                                    div()
                                        .pt_1()
                                        .text_xs()
                                        .text_color(theme::muted())
                                        .child(t!("review.levels_note").into_owned()),
                                )
                            }),
                    )
                    .child(
                        v_flex().id("chapter-detail").flex_1().min_h_0().overflow_y_scroll().px_4().py_2().child(body),
                    )
                    .child(
                        h_flex()
                            .px_4()
                            .py_3()
                            .gap_2()
                            .border_t_1()
                            .border_color(theme::border())
                            .child(
                                Button::new("chapter-reject")
                                    .ghost()
                                    .small()
                                    .label(t!("review.reject").into_owned())
                                    .on_click(cx.listener(move |this, _, _, cx| this.reject_chapter(i, cx))),
                            )
                            .child(div().flex_1())
                            .child(
                                Button::new("chapter-save")
                                    .primary()
                                    .small()
                                    .label(t!("review.save_chapter").into_owned())
                                    .on_click(cx.listener(move |this, _, w, cx| this.accept_chapter(i, w, cx))),
                            ),
                    ),
            )
            .into_any_element()
    }
}
