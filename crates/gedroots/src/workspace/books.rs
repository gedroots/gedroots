//! Books (Source › New Book…): a PDF typeset from the tree by the `gedroots-book` crate, stored in
//! the `.ged` as a `_GR_BOOK` record (who it starts from, which line, how many generations, the
//! page size). The Books page lists them and shows the selected one page by page, typeset in the
//! background; File › Export…, on that page, writes it as PDF.

use super::*;
use crate::save_dialog;
use gedroots_book::{BookSpec, Line, PageSize, Typeset};
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::Arc;

/// Displayed width of a page of the preview, and the gap between two spreads.
const PAGE_W: f32 = 250.;
const SPREAD_GAP: f32 = 28.;
/// Pages kept decoded around the visible ones.
const KEPT_PAGES: usize = 40;

pub(super) struct BooksState {
    /// The selected book, typeset.
    preview: Option<Rc<Preview>>,
    /// Book being typeset: its spec and the tree it was taken from, and the task.
    typesetting: Option<(BookSpec, Gedcom, Task<()>)>,
    /// Why the selected book could not be typeset.
    error: Option<SharedString>,
    /// New Book window, or a book's settings (Edit › Edit Record).
    pub(super) dialog: Option<BookDialog>,
    scroll: UniformListScrollHandle,
    /// Page images no longer shown, released at the next frame (their GPU texture with them).
    dropped: Rc<RefCell<Vec<Arc<RenderImage>>>>,
}

impl Default for BooksState {
    fn default() -> Self {
        BooksState {
            preview: None,
            typesetting: None,
            error: None,
            dialog: None,
            scroll: UniformListScrollHandle::new(),
            dropped: Rc::default(),
        }
    }
}

/// A typeset book and the images of its pages, rendered when they come into view.
struct Preview {
    spec: BookSpec,
    /// The tree it was typeset from: typeset again once it changes.
    source: Gedcom,
    book: Arc<Typeset>,
    /// Height of a page for a width of 1.
    ratio: f32,
    pages: RefCell<HashMap<usize, Arc<RenderImage>>>,
    pending: RefCell<HashSet<usize>>,
}

impl Preview {
    /// Rows of the preview: the cover alone on the right, then the spreads.
    fn rows(&self) -> usize {
        1 + self.book.page_count().saturating_sub(1).div_ceil(2)
    }

    /// Pages of a row: (left, right).
    fn spread(&self, row: usize) -> (Option<usize>, Option<usize>) {
        let n = self.book.page_count();
        if row == 0 {
            return (None, (n > 0).then_some(0));
        }
        let left = 2 * row - 1;
        ((left < n).then_some(left), (left + 1 < n).then_some(left + 1))
    }
}

/// Fields of the New Book window.
pub(super) struct BookDialog {
    /// The book whose settings are changed; `None`: a new book.
    xref: Option<String>,
    subject: String,
    subject_name: String,
    line: Line,
    generations: u32,
    size: PageSize,
    title: Entity<InputState>,
    _sub: Subscription,
}

fn line_label(line: Line) -> SharedString {
    match line {
        Line::Ancestors => t!("books.ancestors"),
        Line::Descendants => t!("books.descendants"),
    }
    .into()
}

fn size_label(size: PageSize) -> SharedString {
    match size {
        PageSize::Book => t!("books.size_book"),
        PageSize::A4 => t!("books.size_a4"),
        PageSize::Letter => t!("books.size_letter"),
    }
    .into()
}

/// “Ancestors of Antonin GUYOT · 8 generations”.
fn book_meta(g: &Gedcom, spec: &BookSpec) -> String {
    let name = g.individual(&spec.subject).map(|i| i.display_name()).unwrap_or_else(|| t!("books.no_person").into());
    let line = match spec.line {
        Line::Ancestors => t!("books.ancestors_of", name = name),
        Line::Descendants => t!("books.descendants_of", name = name),
    };
    format!("{line} · {}", crate::tn!("books.generations", spec.generations as usize))
}

/// Today, for the colophon.
fn today() -> (i32, u8, u8) {
    use chrono::Datelike;
    let d = chrono::Local::now().date_naive();
    (d.year(), d.month() as u8, d.day() as u8)
}

impl Workspace {
    /// The rows of the Books page (used by the history and the landing item).
    pub(super) fn book_rows(&self) -> Vec<Row> {
        let Some(doc) = &self.doc else { return Vec::new() };
        gedroots_book::spec::books(&doc.ged)
            .into_iter()
            .filter_map(|b| {
                Some(Row {
                    title: self.book_title(&b),
                    strong: String::new(),
                    meta: book_meta(&doc.ged, &b),
                    dot: None,
                    flagged: false,
                    key: b.xref?,
                })
            })
            .collect()
    }

    pub(super) fn book_count(&self) -> usize {
        self.doc.as_ref().map(|d| gedroots_book::spec::books(&d.ged).len()).unwrap_or(0)
    }

    pub(super) fn book_title(&self, spec: &BookSpec) -> String {
        match spec.title.trim() {
            "" => self.doc.as_ref().map(|d| gedroots_book::default_title(&d.ged, spec)).unwrap_or_default(),
            t => t.to_string(),
        }
    }

    fn selected_book(&self) -> Option<BookSpec> {
        let doc = self.doc.as_ref()?;
        (self.view == View::Books).then_some(())?;
        gedroots_book::spec::book(&doc.ged, self.selected.as_deref()?)
    }

    /// Typesets the selected book in the background, unless its preview is up to date.
    pub(super) fn typeset_book(&mut self, cx: &mut Context<Self>) {
        let Some(doc) = &self.doc else { return };
        let Some(spec) = self.selected_book() else {
            self.drop_preview();
            self.books.typesetting = None;
            self.books.error = None;
            return;
        };
        let current = |s: &BookSpec, g: &Gedcom| *s == spec && g.records == doc.ged.records;
        if self.books.preview.as_ref().is_some_and(|p| current(&p.spec, &p.source))
            || self.books.typesetting.as_ref().is_some_and(|(s, g, _)| current(s, g))
        {
            return;
        }
        // Another book: its preview is not this one's.
        let g = doc.ged.clone();
        let path = doc.path.clone();
        if self.books.preview.as_ref().is_some_and(|p| p.spec.xref != spec.xref) {
            self.drop_preview();
        }
        self.books.error = None;
        let file = path.as_ref().and_then(|p| p.file_name()).map(|n| n.to_string_lossy().into_owned());
        let file = file.unwrap_or_else(|| t!("books.untitled_tree").into());
        let media_dir = path.as_ref().and_then(|p| p.parent()).map(Path::to_path_buf);
        let (s, g2) = (spec.clone(), g.clone());
        let job = cx.background_spawn(async move {
            gedroots_book::compose(&g2, &s, &file, media_dir.as_deref(), today()).map(Arc::new)
        });
        let task = cx.spawn(async move |this, cx| {
            let res = job.await;
            let _ = this.update(cx, |this, cx| {
                let Some((spec, source, _)) = this.books.typesetting.take() else { return };
                match res {
                    Ok(book) => {
                        let (w, h) = book.page_size();
                        let ratio = (h / w.max(1.)) as f32;
                        // Pages already shown stay until the new ones are rendered: no flash.
                        let same = this.books.preview.as_ref().is_some_and(|p| p.spec.xref == spec.xref);
                        if !same {
                            this.drop_preview();
                        }
                        let old = this.books.preview.take();
                        let preview = Rc::new(Preview {
                            spec,
                            source,
                            book,
                            ratio,
                            pages: RefCell::default(),
                            pending: RefCell::default(),
                        });
                        if let Some(old) = old {
                            // Shown until replaced, page by page.
                            *preview.pages.borrow_mut() = std::mem::take(&mut *old.pages.borrow_mut());
                            preview.pending.borrow_mut().extend(preview.pages.borrow().keys().copied());
                            Self::refresh_pages(&preview, this.books.dropped.clone(), cx);
                        }
                        this.books.preview = Some(preview);
                    }
                    Err(e) => this.books.error = Some(e.into()),
                }
                cx.notify();
            });
        });
        self.books.typesetting = Some((spec, g, task));
        cx.notify();
    }

    /// The pages carried over from the previous typesetting are rendered again.
    fn refresh_pages(preview: &Rc<Preview>, dropped: Rc<RefCell<Vec<Arc<RenderImage>>>>, cx: &mut Context<Self>) {
        let ws = cx.entity().downgrade();
        let indexes: Vec<usize> = preview.pending.borrow().iter().copied().collect();
        for i in indexes {
            render_page(preview.clone(), i, dropped.clone(), ws.clone(), cx);
        }
    }

    fn drop_preview(&mut self) {
        if let Some(p) = self.books.preview.take() {
            self.books.dropped.borrow_mut().extend(p.pages.borrow_mut().drain().map(|(_, img)| img));
        }
    }

    /// Releases the page images no longer shown (needs the window).
    pub(super) fn release_book_pages(&self, window: &mut Window) {
        for img in self.books.dropped.borrow_mut().drain(..) {
            let _ = window.drop_image(img);
        }
    }

    // ------------------------------------------------------------------
    // New Book window
    // ------------------------------------------------------------------

    /// Source › New Book…: starts from the selected person, or the root person.
    pub(super) fn new_book(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(doc) = &self.doc else { return };
        let g = &doc.ged;
        let selected = self.selected.clone().filter(|x| g.individual(x).is_some());
        let Some(subject) = selected.or_else(|| g.root()) else {
            self.set_error("books.no_subject", t!("books.choose_person"));
            cx.notify();
            return;
        };
        let spec = BookSpec::new(&subject, "");
        self.open_book_dialog(spec, window, cx);
    }

    /// Edit › Edit Record on the Books page: the selected book's settings.
    pub(super) fn edit_book(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(spec) = self.selected_book() {
            self.open_book_dialog(spec, window, cx);
        }
    }

    fn open_book_dialog(&mut self, spec: BookSpec, window: &mut Window, cx: &mut Context<Self>) {
        let Some(doc) = &self.doc else { return };
        let subject_name = doc.ged.individual(&spec.subject).map(|i| i.display_name()).unwrap_or_default();
        let placeholder = gedroots_book::default_title(&doc.ged, &spec);
        let title = cx.new(|cx| InputState::new(window, cx).placeholder(placeholder).default_value(spec.title.clone()));
        let sub = cx.subscribe_in(&title, window, |this, _, ev: &InputEvent, window, cx| {
            if matches!(ev, InputEvent::PressEnter { .. }) {
                this.save_book(window, cx);
            }
        });
        title.update(cx, |s, cx| s.focus(window, cx));
        self.books.dialog = Some(BookDialog {
            xref: spec.xref,
            subject: spec.subject,
            subject_name,
            line: spec.line,
            generations: spec.generations,
            size: spec.size,
            title,
            _sub: sub,
        });
        cx.notify();
    }

    pub(super) fn close_book_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.books.dialog = None;
        window.focus(&self.focus, cx);
        cx.notify();
    }

    /// Creates the book (or saves its settings) and shows it.
    fn save_book(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(d) = self.books.dialog.take() else { return };
        let spec = BookSpec {
            xref: d.xref.clone(),
            title: d.title.read(cx).value().trim().to_string(),
            subject: d.subject,
            line: d.line,
            generations: d.generations,
            size: d.size,
        };
        self.checkpoint();
        let Some(doc) = &mut self.doc else { return };
        let xref = gedroots_book::spec::save(&mut doc.ged, &spec);
        doc.dirty = true;
        window.focus(&self.focus, cx);
        let msg = match d.xref {
            Some(_) => t!("books.saved"),
            None => t!("books.created"),
        };
        self.act_editor = None;
        self.review.open = false;
        if self.view == View::Books {
            self.rebuild_rows(cx);
        }
        self.open_location(View::Books, Some(xref), window, cx);
        self.set_status(msg, false);
        cx.notify();
    }

    /// Edit › Delete on the Books page: deletes the book, not the people (Undo brings it back).
    pub(super) fn delete_book(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(spec) = self.selected_book() else { return };
        let Some(key) = spec.xref.clone() else { return };
        let title = self.book_title(&spec);
        self.checkpoint();
        let Some(doc) = &mut self.doc else { return };
        gedroots_book::spec::delete(&mut doc.ged, &key);
        let pos = self.rows.iter().position(|r| r.key == key).unwrap_or(0);
        self.selected = None;
        self.rebuild_rows(cx);
        if let Some(next) = self.rows.get(pos.min(self.rows.len().saturating_sub(1))).map(|r| r.key.clone()) {
            self.select(next, window, cx);
        } else {
            self.typeset_book(cx);
        }
        self.set_status(t!("books.deleted", title = title), false);
        cx.notify();
    }

    // ------------------------------------------------------------------
    // Export
    // ------------------------------------------------------------------

    /// File › Export… on the Books page: the selected book, as PDF.
    pub(super) fn export_book(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(doc) = &self.doc else { return };
        let Some(spec) = self.selected_book() else { return };
        let directory = doc.path.as_ref().and_then(|p| p.parent()).map(Path::to_path_buf).unwrap_or_else(dirs_home);
        let request = save_dialog::Request {
            title: t!("books.export_title").into(),
            format_label: t!("export.format").into(),
            directory,
            stem: sanitize_file_name(&self.book_title(&spec)),
            formats: vec![save_dialog::Format { label: t!("books.pdf").into(), extension: "pdf", suffix: "" }],
            selected: 0,
        };
        let answer = save_dialog::prompt(window, request);
        cx.spawn_in(window, async move |this, cx| {
            let answer = answer.await;
            let _ = this.update_in(cx, |this, window, cx| match answer {
                Ok(Some(choice)) => this.write_book(spec, choice.path, window, cx),
                Ok(None) => {}
                Err(e) => {
                    this.set_error("export.dialog_failed", t!("export.dialog_failed", error = e));
                    cx.notify();
                }
            });
        })
        .detach();
    }

    /// Typesets the book (again, from the tree as it is now) and writes the PDF.
    fn write_book(&mut self, spec: BookSpec, dest: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        let Some(doc) = &self.doc else { return };
        let g = doc.ged.clone();
        let file = doc.path.as_ref().and_then(|p| p.file_name()).map(|n| n.to_string_lossy().into_owned());
        let file = file.unwrap_or_else(|| t!("books.untitled_tree").into());
        let media_dir = doc.path.as_ref().and_then(|p| p.parent()).map(Path::to_path_buf);
        // The preview, when it is the book as it is now.
        let ready = self
            .books
            .preview
            .as_ref()
            .filter(|p| p.spec == spec && p.source.records == g.records)
            .map(|p| p.book.clone());
        let name = dest.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
        self.set_status(t!("books.exporting", file = name), false);
        cx.notify();
        let task = cx.background_spawn(async move {
            let book = match ready {
                Some(b) => b,
                None => Arc::new(gedroots_book::compose(&g, &spec, &file, media_dir.as_deref(), today())?),
            };
            let pdf = book.pdf()?;
            let mut part = dest.as_os_str().to_owned();
            part.push(".part");
            let part = PathBuf::from(part);
            let res = std::fs::write(&part, pdf).and_then(|()| std::fs::rename(&part, &dest));
            if res.is_err() {
                let _ = std::fs::remove_file(&part);
            }
            res.map(|()| (dest, book.page_count())).map_err(|e| e.to_string())
        });
        cx.spawn_in(window, async move |this, cx| {
            let res = task.await;
            let _ = this.update(cx, |this, cx| {
                match res {
                    Ok((dest, pages)) => {
                        this.set_status(t!("books.exported", path = dest.display(), pages = pages), false)
                    }
                    Err(e) => this.set_error("books.export_failed", t!("books.export_failed", error = e)),
                }
                cx.notify();
            });
        })
        .detach();
    }

    // ------------------------------------------------------------------
    // Rendering
    // ------------------------------------------------------------------

    /// The list of the books.
    pub(super) fn render_books_list(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let selected = self.selected.clone();
        v_flex()
            .id("books-list")
            .w(px(300.))
            .flex_none()
            .overflow_y_scroll()
            .bg(theme::bg())
            .border_r_1()
            .border_color(theme::border())
            .py_1()
            .children(self.rows.iter().map(|r| {
                let on = selected.as_deref() == Some(r.key.as_str());
                let key = r.key.clone();
                v_flex()
                    .id(SharedString::from(format!("book-{}", r.key)))
                    .w_full()
                    .px_3()
                    .py_2()
                    .cursor_pointer()
                    .when(on, |d| d.bg(theme::hover()))
                    .hover(|d| d.bg(theme::surface()))
                    .child(div().text_sm().truncate().child(r.title.clone()))
                    .child(div().text_xs().text_color(theme::muted()).truncate().child(r.meta.clone()))
                    .on_click(cx.listener(move |this, _, w, cx| this.select(key.clone(), w, cx)))
            }))
    }

    /// The selected book, page by page.
    pub(super) fn render_books(&self, cx: &mut Context<Self>) -> AnyElement {
        let center = v_flex().flex_1().min_w_0().bg(theme::surface());
        let Some(spec) = self.selected_book() else {
            return center
                .items_center()
                .justify_center()
                .text_color(theme::muted())
                .child(t!("books.none"))
                .into_any_element();
        };
        let g = &self.doc.as_ref().expect("a file is open").ged;
        let preview = self.books.preview.clone().filter(|p| p.spec.xref == spec.xref);
        let busy = self.books.typesetting.is_some();
        let status: SharedString = match (&self.books.error, &preview, busy) {
            (Some(e), ..) => e.clone(),
            (None, _, true) => t!("books.typesetting").into(),
            (None, Some(p), false) => crate::tn!("books.pages", p.book.page_count()).into(),
            (None, None, false) => SharedString::default(),
        };
        let header = v_flex()
            .px_6()
            .pt_4()
            .pb_3()
            .gap_0p5()
            .border_b_1()
            .border_color(theme::border())
            .bg(theme::bg())
            .child(div().text_xl().font_weight(FontWeight::SEMIBOLD).child(self.book_title(&spec)))
            .child(div().text_sm().text_color(theme::muted()).child(format!(
                "{} · {} · {status}",
                book_meta(g, &spec),
                size_label(spec.size)
            )));
        let body: AnyElement = match preview {
            Some(p) => self.render_pages(p, cx),
            None => div().flex_1().into_any_element(),
        };
        center.child(header).child(body).into_any_element()
    }

    fn render_pages(&self, preview: Rc<Preview>, cx: &mut Context<Self>) -> AnyElement {
        let page_h = PAGE_W * preview.ratio;
        let ws = cx.entity().downgrade();
        let dropped = self.books.dropped.clone();
        let p = preview.clone();
        uniform_list("book-pages", preview.rows(), move |range, _window, cx| {
            // Pages around the visible ones stay decoded; the others are released.
            let (lo, hi) = (2 * range.start, 2 * range.end + 1);
            let far: Vec<usize> = p
                .pages
                .borrow()
                .keys()
                .copied()
                .filter(|&i| i + KEPT_PAGES / 2 < lo || i > hi + KEPT_PAGES / 2)
                .collect();
            for i in far {
                if let Some(img) = p.pages.borrow_mut().remove(&i) {
                    dropped.borrow_mut().push(img);
                }
            }
            range
                .map(|row| {
                    let (left, right) = p.spread(row);
                    let page = |i: Option<usize>, cx: &mut App| -> AnyElement {
                        let Some(i) = i else { return div().w(px(PAGE_W)).h(px(page_h)).into_any_element() };
                        let image = p.pages.borrow().get(&i).cloned();
                        if !p.pending.borrow().contains(&i) && image.is_none() {
                            p.pending.borrow_mut().insert(i);
                            render_page(p.clone(), i, dropped.clone(), ws.clone(), cx);
                        }
                        div()
                            .w(px(PAGE_W))
                            .h(px(page_h))
                            .bg(rgb(0xffffff))
                            .shadow_sm()
                            .children(image.map(|i| img(i).size_full()))
                            .into_any_element()
                    };
                    let number = match (left, right) {
                        (Some(l), Some(r)) => format!("{}–{}", l + 1, r + 1),
                        (Some(l), None) => (l + 1).to_string(),
                        (None, Some(r)) => (r + 1).to_string(),
                        (None, None) => String::new(),
                    };
                    v_flex()
                        .id(row)
                        .w_full()
                        .h(px(page_h + SPREAD_GAP + 16.))
                        .items_center()
                        .pt(px(SPREAD_GAP))
                        .gap_1()
                        .child(h_flex().child(page(left, cx)).child(page(right, cx)))
                        .child(div().text_xs().text_color(theme::muted()).child(number))
                })
                .collect()
        })
        .track_scroll(&self.books.scroll)
        .flex_1()
        .into_any_element()
    }

    /// The New Book window (or a book's settings).
    pub(super) fn render_book_dialog(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let d = self.books.dialog.as_ref()?;
        let row = |label: SharedString, control: AnyElement| {
            h_flex()
                .gap_4()
                .items_center()
                .child(div().w(px(110.)).flex_none().text_right().text_color(theme::muted()).child(label))
                .child(div().flex_1().min_w_0().child(control))
        };
        let segment = |id: &str, on: bool, label: SharedString| {
            div()
                .id(SharedString::from(id.to_string()))
                .px_3()
                .py_1()
                .rounded(theme::RADIUS_SM)
                .cursor_pointer()
                .when(on, |d| d.bg(theme::accent_soft()).text_color(theme::accent()))
                .when(!on, |d| d.hover(|d| d.bg(theme::hover())))
                .child(label)
        };
        let segmented = |items: Vec<AnyElement>| {
            h_flex()
                .p_0p5()
                .gap_0p5()
                .rounded(theme::RADIUS_SM)
                .border_1()
                .border_color(theme::border_strong())
                .children(items)
                .into_any_element()
        };
        let lines = [Line::Ancestors, Line::Descendants]
            .map(|l| {
                segment(&format!("book-line-{l:?}"), d.line == l, line_label(l))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if let Some(d) = &mut this.books.dialog {
                            d.line = l;
                        }
                        cx.notify();
                    }))
                    .into_any_element()
            })
            .into_iter()
            .collect::<Vec<_>>();
        let sizes = [PageSize::Book, PageSize::A4, PageSize::Letter]
            .map(|s| {
                segment(&format!("book-size-{s:?}"), d.size == s, size_label(s))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if let Some(d) = &mut this.books.dialog {
                            d.size = s;
                        }
                        cx.notify();
                    }))
                    .into_any_element()
            })
            .into_iter()
            .collect::<Vec<_>>();
        let step = |delta: i32| {
            move |this: &mut Self, _: &ClickEvent, _: &mut Window, cx: &mut Context<Self>| {
                if let Some(d) = &mut this.books.dialog {
                    d.generations = (d.generations as i32 + delta)
                        .clamp(gedroots_book::spec::MIN_GENERATIONS as i32, gedroots_book::spec::MAX_GENERATIONS as i32)
                        as u32;
                }
                cx.notify();
            }
        };
        let generations = h_flex()
            .gap_2()
            .items_center()
            .child(
                Button::new("book-fewer")
                    .small()
                    .icon(IconName::Minus)
                    .disabled(d.generations <= gedroots_book::spec::MIN_GENERATIONS)
                    .on_click(cx.listener(step(-1))),
            )
            .child(div().w(px(28.)).text_center().child(d.generations.to_string()))
            .child(
                Button::new("book-more")
                    .small()
                    .icon(IconName::Plus)
                    .disabled(d.generations >= gedroots_book::spec::MAX_GENERATIONS)
                    .on_click(cx.listener(step(1))),
            )
            .into_any_element();
        let editing = d.xref.is_some();
        let cancel = Button::new("book-cancel")
            .label(t!("new_tree.cancel"))
            .on_click(cx.listener(|this, _, w, cx| this.close_book_dialog(w, cx)));
        let create = Button::new("book-create")
            .primary()
            .label(if editing { t!("books.save") } else { t!("books.create") })
            .on_click(cx.listener(|this, _, w, cx| this.save_book(w, cx)));
        // Platform order: the action on the left on Windows, on the right elsewhere.
        let buttons = if cfg!(target_os = "windows") {
            [create.into_any_element(), cancel.into_any_element()]
        } else {
            [cancel.into_any_element(), create.into_any_element()]
        };
        Some(
            div()
                .absolute()
                .inset_0()
                .flex()
                .justify_center()
                .items_center()
                .bg(theme::scrim())
                .child(
                    v_flex()
                        .id("book-dialog")
                        .occlude()
                        .w(px(560.))
                        .rounded(theme::RADIUS_LG)
                        .border_1()
                        .border_color(theme::border_strong())
                        .bg(theme::panel())
                        .shadow_lg()
                        .on_key_down(cx.listener(|this, ev: &KeyDownEvent, w, cx| {
                            if ev.keystroke.key == "escape" {
                                this.close_book_dialog(w, cx);
                            }
                        }))
                        .child(
                            div()
                                .px_6()
                                .py_4()
                                .border_b_1()
                                .border_color(theme::border())
                                .text_xl()
                                .font_weight(FontWeight::SEMIBOLD)
                                .child(if editing { t!("books.settings_title") } else { t!("books.new_title") }),
                        )
                        .child(
                            v_flex()
                                .p_6()
                                .gap_3()
                                .child(row(
                                    t!("books.person").into(),
                                    div().child(d.subject_name.clone()).into_any_element(),
                                ))
                                .child(row(
                                    t!("books.line").into(),
                                    h_flex().child(segmented(lines)).into_any_element(),
                                ))
                                .child(row(t!("books.generations_label").into(), generations))
                                .child(row(
                                    t!("books.size").into(),
                                    h_flex().child(segmented(sizes)).into_any_element(),
                                ))
                                .child(row(t!("books.title").into(), Input::new(&d.title).into_any_element())),
                        )
                        .child(
                            h_flex()
                                .px_6()
                                .py_4()
                                .gap_2()
                                .justify_end()
                                .border_t_1()
                                .border_color(theme::border())
                                .children(buttons),
                        ),
                )
                .into_any_element(),
        )
    }
}

/// Renders a page in the background and shows it once ready.
fn render_page(
    preview: Rc<Preview>,
    index: usize,
    dropped: Rc<RefCell<Vec<Arc<RenderImage>>>>,
    ws: WeakEntity<Workspace>,
    cx: &mut App,
) {
    let book = preview.book.clone();
    // Sharp on a Retina screen.
    let width = (PAGE_W * 2.) as u32;
    let job = cx.background_spawn(async move { book.page_bgra(index, width) });
    cx.spawn(async move |cx| {
        let pixels = job.await;
        preview.pending.borrow_mut().remove(&index);
        if let Some(image) = pixels.and_then(|(w, h, data)| image::RgbaImage::from_raw(w, h, data)) {
            let image = Arc::new(RenderImage::new(vec![image::Frame::new(image)]));
            if let Some(old) = preview.pages.borrow_mut().insert(index, image) {
                dropped.borrow_mut().push(old);
            }
        }
        let _ = ws.update(cx, |_, cx| cx.notify());
    })
    .detach();
}

/// A file name from a book's title: without the characters the systems refuse.
fn sanitize_file_name(title: &str) -> String {
    let name: String = title
        .chars()
        .map(|c| if matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|') { '-' } else { c })
        .collect();
    match name.trim() {
        "" => t!("books.default_file").into(),
        n => n.to_string(),
    }
}
