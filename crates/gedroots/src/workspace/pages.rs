//! Multi-page media: page-by-page viewer, adding, removing and reordering pages, merging media and
//! splitting off a page.
//!
//! The model is in `gedcom::multipage` (an `OBJE` with several `FILE`).

use super::relatives::PickPurpose;
use super::*;
use crate::t;

/// Size of the page thumbnails.
const THUMB_W: f32 = 64.;
const THUMB_H: f32 = 84.;

/// Page dragged in the thumbnail strip, to move it.
#[derive(Clone)]
pub struct DraggedPage {
    media: String,
    page: usize,
    path: Option<PathBuf>,
}

/// Preview under the pointer while dragging.
struct PagePreview(Option<PathBuf>);

impl Render for PagePreview {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let tile = div()
            .w(px(THUMB_W))
            .h(px(THUMB_H))
            .rounded(theme::RADIUS_SM)
            .overflow_hidden()
            .border_2()
            .border_color(theme::accent())
            .bg(theme::surface())
            .opacity(0.85);
        match self.0.clone() {
            Some(p) => tile.child(picture(p, THUMB_W, THUMB_H, ObjectFit::Cover).size_full()),
            None => tile,
        }
    }
}

/// Absolute path of a media file (`FILE` is relative to the .ged folder).
pub(super) fn file_path(doc: &Document, f: &str) -> Option<PathBuf> {
    file_path_in(doc.path.as_ref().and_then(|p| p.parent()), f)
}

/// Path of a `FILE` value relative to `dir` (the .ged folder).
pub(super) fn file_path_in(dir: Option<&std::path::Path>, f: &str) -> Option<PathBuf> {
    if f.is_empty() || (f.contains("://") && !f.starts_with("file://")) {
        return None;
    }
    let p = PathBuf::from(f.strip_prefix("file://").unwrap_or(f));
    if p.is_absolute() {
        return Some(p);
    }
    Some(dir?.join(p))
}

/// Path of page `page` (from 1) of a media.
pub(super) fn page_path(doc: &Document, m: &Media, page: usize) -> Option<PathBuf> {
    file_path(doc, m.page_file(page)?)
}

/// Is the file an image (according to `FORM` or the extension)?
fn is_image_file(path: &str, form: Option<&str>) -> bool {
    form.is_some_and(|f| f.starts_with("image/"))
        || mime_for(path).starts_with("image/")
        || path.to_ascii_lowercase().ends_with(".tif")
}

impl Workspace {
    /// Displayed page of the selected media (from 1), bounded by the number of pages.
    pub(super) fn current_page(&self, media: &str) -> usize {
        let n = self.doc.as_ref().and_then(|d| d.ged.get(media)).map(|n| Media::new(n).page_count()).unwrap_or(1);
        self.media_page.clamp(1, n.max(1))
    }

    pub(super) fn page_count(&self, media: &str) -> usize {
        self.doc.as_ref().and_then(|d| d.ged.get(media)).map(|n| Media::new(n).page_count()).unwrap_or(0)
    }

    pub(super) fn show_page(&mut self, page: usize, cx: &mut Context<Self>) {
        if self.media_page != page {
            self.media_page = page;
            self.photo_draw = None;
            self.photo_hover = None;
            cx.notify();
        }
    }

    pub(super) fn step_page(&mut self, delta: isize, cx: &mut Context<Self>) {
        let Some(m) = self.selected.clone() else { return };
        let (n, p) = (self.page_count(&m), self.current_page(&m));
        let next = (p as isize + delta).clamp(1, n.max(1) as isize) as usize;
        self.tab = CenterTab::Form;
        self.show_page(next, cx);
    }

    /// Adds files as pages of the media, then shows the first one added.
    pub(super) fn add_pages(&mut self, media: String, files: Vec<String>, window: &mut Window, cx: &mut Context<Self>) {
        if files.is_empty() {
            return;
        }
        let first_new = self.page_count(&media) + 1;
        let mut added = 0;
        self.mutate(window, cx, |g| {
            added = g.add_media_pages(&media, &files)?;
            if added == 0 {
                return Err(t!("pages.already").into());
            }
            Ok((crate::tn!("pages.added", added).to_string(), None))
        });
        if added > 0 {
            self.media_page = first_new;
        }
    }

    pub(super) fn choose_pages(&mut self, media: String, window: &mut Window, cx: &mut Context<Self>) {
        let rx = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: true,
            prompt: Some(t!("pages.add_prompt").into()),
        });
        cx.spawn_in(window, async move |this, cx| {
            if let Ok(Ok(Some(paths))) = rx.await {
                let _ = this.update_in(cx, |this, window, cx| {
                    let files = paths.iter().map(|p| this.file_value(p)).collect();
                    this.add_pages(media, files, window, cx);
                });
            }
        })
        .detach();
    }

    pub(super) fn move_page(
        &mut self,
        media: String,
        from: usize,
        to: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if from == to {
            return;
        }
        self.mutate(window, cx, |g| {
            g.move_media_page(&media, from, to)?;
            Ok((t!("pages.moved", from = from, to = to).to_string(), None))
        });
        self.media_page = to;
    }

    pub(super) fn remove_page(&mut self, media: String, page: usize, window: &mut Window, cx: &mut Context<Self>) {
        self.mutate(window, cx, |g| {
            g.remove_media_page(&media, page)?;
            Ok((t!("pages.removed", page = page).to_string(), None))
        });
    }

    /// Splits the page into a new media, and opens it.
    pub(super) fn split_page(&mut self, media: String, page: usize, window: &mut Window, cx: &mut Context<Self>) {
        self.mutate(window, cx, |g| {
            let new = g.split_media_page(&media, page)?;
            Ok((t!("pages.split", page = page, name = g.record_label(&new)).to_string(), Some(new)))
        });
    }

    /// Merges the media (in this order) into a multi-page media, and opens it.
    pub(super) fn merge_media(&mut self, list: Vec<String>, window: &mut Window, cx: &mut Context<Self>) {
        self.mutate(window, cx, |g| {
            let target = g.merge_media(&list)?;
            let n = Media::new(g.get(&target).unwrap()).page_count();
            Ok((t!("pages.merged", count = list.len(), n = n).to_string(), Some(target)))
        });
        self.media_marked.clear();
    }

    /// Click on a row of the media list with ⌘ / Ctrl (add to the selection) or Shift (select up to
    /// this row).
    pub(super) fn mark_media(&mut self, key: String, range: bool, cx: &mut Context<Self>) {
        if self.media_marked.is_empty()
            && let Some(cur) = self.selected.clone()
        {
            self.media_marked.push(cur);
        }
        if range {
            let anchor = self.media_marked.last().cloned().or_else(|| self.selected.clone());
            let pos = |k: &str| self.rows.iter().position(|r| r.key == k);
            if let (Some(a), Some(b)) = (anchor.as_deref().and_then(pos), pos(&key)) {
                let (lo, hi) = (a.min(b), a.max(b));
                let keys: Vec<String> = self.rows[lo..=hi].iter().map(|r| r.key.clone()).collect();
                let ordered: Vec<String> = if a <= b { keys } else { keys.into_iter().rev().collect() };
                for k in ordered {
                    if !self.media_marked.contains(&k) {
                        self.media_marked.push(k);
                    }
                }
            }
        } else if let Some(i) = self.media_marked.iter().position(|k| *k == key) {
            self.media_marked.remove(i);
        } else {
            self.media_marked.push(key);
        }
        if self.media_marked.len() < 2 {
            self.media_marked.clear();
        }
        self.tab = CenterTab::Form;
        cx.notify();
    }

    /// Order of the media to merge: a media is moved one step.
    fn shift_marked(&mut self, i: usize, delta: isize, cx: &mut Context<Self>) {
        let j = i as isize + delta;
        if j >= 0 && (j as usize) < self.media_marked.len() {
            self.media_marked.swap(i, j as usize);
            cx.notify();
        }
    }

    // ------------------------------------------------------------------
    // Rendu
    // ------------------------------------------------------------------

    /// Thumbnail of a page (or of a media), `w × h`.
    fn page_thumb(path: Option<PathBuf>, image: bool, w: f32, h: f32) -> Div {
        let tile = div()
            .w(px(w))
            .h(px(h))
            .flex_none()
            .rounded(theme::RADIUS_SM)
            .overflow_hidden()
            .border_1()
            .border_color(theme::border())
            .bg(theme::surface());
        match path.filter(|p| p.exists()) {
            Some(p) if image => tile.child(picture(p, w, h, ObjectFit::Cover).size_full()),
            Some(_) => {
                tile.flex().items_center().justify_center().child(icon(IconName::FileText).text_color(theme::muted()))
            }
            None => tile.flex().items_center().justify_center().child(icon(IconName::Image).text_color(theme::warn())),
        }
    }

    /// Strip of a media’s pages: previous / next and thumbnails, dragged to reorder them. Nothing
    /// for a single-page media.
    pub(super) fn render_page_strip(&self, media: &str, cx: &mut Context<Self>) -> Option<AnyElement> {
        let doc = self.doc.as_ref()?;
        let m = Media::new(doc.ged.get(media)?);
        let files = m.files();
        let n = files.len();
        if n < 2 {
            return None;
        }
        let cur = self.current_page(media);
        let thumbs = files.iter().enumerate().map(|(k, f)| {
            let page = k + 1;
            let path = file_path(doc, f.path);
            let drag = DraggedPage { media: media.to_string(), page, path: path.clone() };
            let target = media.to_string();
            let on = page == cur;
            v_flex()
                .id(SharedString::from(format!("page-{page}")))
                .gap_0p5()
                .items_center()
                .cursor_pointer()
                .child(
                    Self::page_thumb(path, is_image_file(f.path, f.form), THUMB_W, THUMB_H)
                        .when(on, |d| d.border_2().border_color(theme::accent())),
                )
                .child(mono(page.to_string()).when(on, |d| d.text_color(theme::accent())))
                .on_click(cx.listener(move |this, _, _, cx| this.show_page(page, cx)))
                .on_drag(drag, |d, _, _, cx| cx.new(|_| PagePreview(d.path.clone())))
                .drag_over::<DraggedPage>(|d, _, _, _| d.bg(theme::accent_soft()).rounded(theme::RADIUS_SM))
                .on_drop(cx.listener(move |this, d: &DraggedPage, w, cx| {
                    if d.media == target {
                        this.move_page(target.clone(), d.page, page, w, cx);
                    }
                }))
        });
        let id = media.to_string();
        Some(
            v_flex()
                .gap_2()
                .child(
                    h_flex()
                        .gap_2()
                        .items_center()
                        .child(label_caps(t!("pages.section", n = n).to_string()))
                        .child(
                            super::add::plus_button("add-page")
                                .tooltip(t!("pages.add_prompt"))
                                .on_click(cx.listener(move |this, _, w, cx| this.choose_pages(id.clone(), w, cx))),
                        )
                        .child(div().flex_1())
                        .child(
                            Button::new("page-prev")
                                .xsmall()
                                .ghost()
                                .icon(IconName::ChevronLeft)
                                .disabled(cur <= 1)
                                .on_click(cx.listener(|this, _, _, cx| this.step_page(-1, cx))),
                        )
                        .child(mono(format!("{cur} / {n}")))
                        .child(
                            Button::new("page-next")
                                .xsmall()
                                .ghost()
                                .icon(IconName::ChevronRight)
                                .disabled(cur >= n)
                                .on_click(cx.listener(|this, _, _, cx| this.step_page(1, cx))),
                        ),
                )
                .child(div().id("page-strip").overflow_x_scroll().child(h_flex().gap_2().pb_1().children(thumbs)))
                .into_any_element(),
        )
    }

    /// Central area when several media are selected: preview of the pages of the future media,
    /// editable order, and merge.
    pub(super) fn render_merge_panel(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(doc) = self.doc.as_ref() else { return div().into_any_element() };
        let g = &doc.ged;
        let marked = self.media_marked.clone();
        let total: usize = marked.iter().filter_map(|x| g.get(x)).map(|n| Media::new(n).page_count()).sum();
        let count = marked.len();
        let mut page = 0;
        let rows = marked.iter().enumerate().filter_map(|(i, x)| {
            let m = Media::new(g.get(x)?);
            let files = m.files();
            let first = page + 1;
            page += files.len();
            let pages = if files.len() > 1 {
                t!("pages.range", first = first, last = page)
            } else {
                t!("photo.page", n = first)
            };
            let target = x.clone();
            Some(
                h_flex()
                    .id(SharedString::from(format!("merge-{x}")))
                    .gap_3()
                    .p_2()
                    .items_center()
                    .rounded(theme::RADIUS_SM)
                    .border_1()
                    .border_color(theme::border())
                    .bg(theme::surface())
                    .child(Self::page_thumb(
                        files.first().and_then(|f| file_path(doc, f.path)),
                        files.first().is_some_and(|f| is_image_file(f.path, f.form)),
                        48.,
                        64.,
                    ))
                    .child(
                        v_flex()
                            .flex_1()
                            .min_w_0()
                            .gap_0p5()
                            .child(div().overflow_hidden().whitespace_nowrap().text_ellipsis().child(m.title()))
                            .child(mono(pages)),
                    )
                    .child(
                        Button::new(SharedString::from(format!("merge-up-{i}")))
                            .xsmall()
                            .ghost()
                            .icon(IconName::ChevronUp)
                            .disabled(i == 0)
                            .on_click(cx.listener(move |this, _, _, cx| this.shift_marked(i, -1, cx))),
                    )
                    .child(
                        Button::new(SharedString::from(format!("merge-down-{i}")))
                            .xsmall()
                            .ghost()
                            .icon(IconName::ChevronDown)
                            .disabled(i + 1 == count)
                            .on_click(cx.listener(move |this, _, _, cx| this.shift_marked(i, 1, cx))),
                    )
                    .child(
                        Button::new(SharedString::from(format!("merge-drop-{i}")))
                            .xsmall()
                            .ghost()
                            .icon(IconName::Close)
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.media_marked.retain(|k| *k != target);
                                if this.media_marked.len() < 2 {
                                    this.media_marked.clear();
                                }
                                cx.notify();
                            })),
                    ),
            )
        });
        div()
            .id("merge-scroll")
            .flex_1()
            .overflow_y_scroll()
            .child(
                v_flex()
                    .p_4()
                    .gap_3()
                    .max_w(px(720.))
                    .child(label_caps(t!("pages.selected", count = count, total = total).to_string()))
                    .children(rows)
                    .child(
                        h_flex()
                            .gap_2()
                            .child(
                                Button::new("merge-media")
                                    .primary()
                                    .small()
                                    .icon(IconName::Merge)
                                    .label(t!("pages.merge", total = total))
                                    .on_click(cx.listener(move |this, _, w, cx| {
                                        let list = this.media_marked.clone();
                                        this.merge_media(list, w, cx)
                                    })),
                            )
                            .child(Button::new("merge-cancel").ghost().small().label(t!("common.cancel")).on_click(
                                cx.listener(|this, _, _, cx| {
                                    this.media_marked.clear();
                                    cx.notify();
                                }),
                            )),
                    ),
            )
            .into_any_element()
    }

    /// Media menu commands related to pages, according to the selected media.
    pub(super) fn bind_page_actions(&self, d: Div, cx: &mut Context<Self>) -> Div {
        let media = self
            .selected
            .clone()
            .filter(|x| self.doc.as_ref().and_then(|d| d.ged.get(x)).is_some_and(|n| n.tag == "OBJE"));
        let merge_ready = self.view == View::Media && self.media_marked.len() >= 2;
        let d = d.when(merge_ready, |d| {
            d.on_action(cx.listener(|this, _: &MergeSelectedMedia, w, cx| {
                let list = this.media_marked.clone();
                this.merge_media(list, w, cx)
            }))
        });
        let Some(m) = media else { return d };
        let (n, p) = (self.page_count(&m), self.current_page(&m));
        d.on_action(cx.listener(|this, _: &AddPages, w, cx| {
            if let Some(m) = this.selected.clone() {
                this.choose_pages(m, w, cx)
            }
        }))
        .on_action(cx.listener(|this, _: &MergeWithMedia, w, cx| {
            if let Some(media) = this.selected.clone() {
                this.open_picker(PickPurpose::MergeInto { media }, w, cx)
            }
        }))
        .when(n > 1, |d| {
            d.when(p > 1, |d| {
                d.on_action(cx.listener(|this, _: &PreviousPage, _, cx| this.step_page(-1, cx))).on_action(cx.listener(
                    |this, _: &MovePageLeft, w, cx| {
                        if let Some(m) = this.selected.clone() {
                            let p = this.current_page(&m);
                            this.move_page(m, p, p - 1, w, cx)
                        }
                    },
                ))
            })
            .when(p < n, |d| {
                d.on_action(cx.listener(|this, _: &NextPage, _, cx| this.step_page(1, cx))).on_action(cx.listener(
                    |this, _: &MovePageRight, w, cx| {
                        if let Some(m) = this.selected.clone() {
                            let p = this.current_page(&m);
                            this.move_page(m, p, p + 1, w, cx)
                        }
                    },
                ))
            })
            .on_action(cx.listener(|this, _: &SplitPage, w, cx| {
                if let Some(m) = this.selected.clone() {
                    let p = this.current_page(&m);
                    this.split_page(m, p, w, cx)
                }
            }))
            .on_action(cx.listener(|this, _: &RemovePage, w, cx| {
                if let Some(m) = this.selected.clone() {
                    let p = this.current_page(&m);
                    this.remove_page(m, p, w, cx)
                }
            }))
        })
    }
}
