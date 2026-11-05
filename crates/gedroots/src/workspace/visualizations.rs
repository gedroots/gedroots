//! Visualizations page: the list of the visualizations on the left (the tree's figures first,
//! then those of the `gedroots-viz` crate, which family books also place on their pages), the
//! selected one in the centre with its settings above it (whose ancestors or descendants, over
//! how many generations) and a Chart / Table switch. The drawing is computed and rendered in the
//! background, in the colours of the app's theme.

use super::relatives::PickPurpose;
use super::*;
use gedroots_book::writing::Figure as Placed;
use gedroots_book::{Block, BookSpec};
use gedroots_viz::{Figure, Kind, Line, Palette, Scope, Tree};
use gpui_kit::component::menu::{DropdownMenu, PopupMenuItem};
use std::cell::RefCell;
use std::sync::Arc;

/// Key of the tree's figures, the first item of the list.
pub(super) const OVERVIEW: &str = "overview";
/// Widest a drawing is shown, and the pixels it is rendered with (sharp on a Retina screen).
const MAX_W: f32 = 880.;
const RENDER_W: u32 = 1760;

pub(super) struct VizState {
    /// Whose visualizations: `None`, the root person.
    subject: Option<String>,
    line: Line,
    generations: u32,
    /// Table rather than chart.
    table: bool,
    shown: Option<Rc<Shown>>,
    /// Drawing being computed, for this key and tree.
    busy: Option<(VizKey, Gedcom, Task<()>)>,
    /// Images no longer shown, released at the next frame (their GPU texture with them).
    dropped: Rc<RefCell<Vec<Arc<RenderImage>>>>,
}

impl Default for VizState {
    fn default() -> Self {
        VizState {
            subject: None,
            line: Line::Ancestors,
            generations: gedroots_viz::scope::DEFAULT_GENERATIONS,
            table: false,
            shown: None,
            busy: None,
            dropped: Rc::default(),
        }
    }
}

/// What a drawing depends on, besides the tree.
#[derive(Debug, Clone, PartialEq)]
struct VizKey {
    kind: Kind,
    scope: Scope,
    dark: bool,
    lang: String,
}

/// A visualization computed for a key and a tree.
struct Shown {
    key: VizKey,
    source: Gedcom,
    /// `None`: the tree has too little for it.
    figure: Option<Figure>,
    image: Option<Arc<RenderImage>>,
    /// Height of the drawing for a width of 1.
    ratio: f32,
}

/// A book in the Add to Book menu: its id, title and chapters (generation, title).
type BookEntry = (String, String, Vec<(u32, String)>);

fn icon_of(kind: Kind) -> IconName {
    match kind {
        Kind::Fan => IconName::ChartPie,
        Kind::Places => IconName::Route,
        Kind::Lives => IconName::ChartGantt,
        Kind::Generations => IconName::Layers,
        Kind::Longevity => IconName::Hourglass,
        Kind::Occupations => IconName::Briefcase,
    }
}

/// This year, which ends the lives still open.
fn this_year() -> i32 {
    use chrono::Datelike;
    chrono::Local::now().year()
}

impl Workspace {
    /// The rows of the page: the tree's figures, then each visualization.
    pub(super) fn viz_rows(&self) -> Vec<Row> {
        let row = |key: &str, title: String, meta: String| Row {
            key: key.to_string(),
            title,
            strong: String::new(),
            meta,
            dot: None,
            flagged: false,
        };
        let mut rows = vec![row(OVERVIEW, t!("viz.overview").into(), t!("viz.overview_summary").into())];
        rows.extend(Kind::ALL.into_iter().map(|k| row(k.slug(), k.title(), k.summary())));
        rows
    }

    fn viz_kind(&self) -> Option<Kind> {
        (self.view == View::Visualizations).then_some(())?;
        Kind::from_slug(self.selected.as_deref()?)
    }

    /// The scope the page shows: its person (the root person by default), line and generations.
    fn viz_scope(&self) -> Option<Scope> {
        let g = &self.doc.as_ref()?.ged;
        let subject = (self.viz.subject.clone().filter(|x| g.individual(x).is_some()))
            .or_else(|| g.root())
            .or_else(|| g.records_of(RecordKind::Individual).next().map(|r| Individual::new(r).xref().to_string()))?;
        Some(Scope::new(&subject, self.viz.line, self.viz.generations))
    }

    fn viz_key(&self) -> Option<VizKey> {
        let mut scope = self.viz_scope()?;
        let kind = self.viz_kind()?;
        if !kind.supports(scope.line) {
            scope.line = Line::Ancestors;
        }
        Some(VizKey { kind, scope, dark: theme::is_dark(), lang: rust_i18n::locale().to_string() })
    }

    /// Computes the selected visualization in the background, unless the one shown is up to date.
    pub(super) fn refresh_viz(&mut self, cx: &mut Context<Self>) {
        let (Some(doc), Some(key)) = (&self.doc, self.viz_key()) else {
            self.viz.busy = None;
            return;
        };
        let current = |k: &VizKey, g: &Gedcom| *k == key && g.records == doc.ged.records;
        if self.viz.shown.as_ref().is_some_and(|s| current(&s.key, &s.source))
            || self.viz.busy.as_ref().is_some_and(|(k, g, _)| current(k, g))
        {
            return;
        }
        let g = doc.ged.clone();
        let (k, g2) = (key.clone(), g.clone());
        let job = cx.background_spawn(async move {
            let palette = if k.dark { Palette::DARK } else { Palette::LIGHT };
            let figure = Tree::new(&g2, k.scope.clone()).ok().and_then(|t| t.figure(k.kind, &palette, this_year()));
            let pixels = figure.as_ref().and_then(|f| gedroots_viz::raster::bgra(&f.svg, RENDER_W));
            (figure, pixels)
        });
        let task = cx.spawn(async move |this, cx| {
            let (figure, pixels) = job.await;
            let _ = this.update(cx, |this, cx| {
                let Some((key, source, _)) = this.viz.busy.take() else { return };
                let (image, ratio) =
                    match pixels.and_then(|(w, h, px)| Some((w, h, image::RgbaImage::from_raw(w, h, px)?))) {
                        Some((w, h, px)) => {
                            (Some(Arc::new(RenderImage::new(vec![image::Frame::new(px)]))), h as f32 / w.max(1) as f32)
                        }
                        None => (None, 1.),
                    };
                if let Some(old) = this.viz.shown.take().and_then(|s| s.image.clone()) {
                    this.viz.dropped.borrow_mut().push(old);
                }
                this.viz.shown = Some(Rc::new(Shown { key, source, figure, image, ratio }));
                cx.notify();
            });
        });
        self.viz.busy = Some((key, g, task));
    }

    /// Releases the images no longer shown (needs the window).
    pub(super) fn release_viz_images(&self, window: &mut Window) {
        for img in self.viz.dropped.borrow_mut().drain(..) {
            let _ = window.drop_image(img);
        }
    }

    /// The person chosen in the picker: the visualizations are now about them.
    pub(super) fn set_viz_person(&mut self, xref: String, cx: &mut Context<Self>) {
        self.viz.subject = Some(xref);
        self.refresh_viz(cx);
        cx.notify();
    }

    fn set_viz(&mut self, cx: &mut Context<Self>, f: impl FnOnce(&mut VizState)) {
        f(&mut self.viz);
        self.refresh_viz(cx);
        cx.notify();
    }

    /// Places the visualization shown at the end of a chapter's introduction (one undo point).
    /// What it shares with the book (person, line, generations) is left to the book, so that it
    /// follows the book's settings.
    fn add_viz_to_book(&mut self, book: String, generation: u32, window: &mut Window, cx: &mut Context<Self>) {
        let (Some(doc), Some(key)) = (&self.doc, self.viz_key()) else { return };
        let Some(mut spec) = gedroots_book::spec::book(&doc.ged, &book) else { return };
        let scope = key.scope;
        let figure = Placed {
            kind: key.kind,
            subject: (scope.subject != spec.subject).then(|| scope.subject.clone()),
            line: (scope.line != spec.line).then_some(scope.line),
            generations: (scope.generations != spec.generations).then_some(scope.generations),
            text: String::new(),
        };
        let chapter = gedroots_book::outline(&doc.ged, &spec)
            .ok()
            .and_then(|o| o.into_iter().find(|c| c.generation == generation))
            .map(|c| c.title)
            .unwrap_or_default();
        let msg = t!("viz.added", chapter = chapter, book = self.book_title(&spec)).into_owned();
        spec.chapters.entry(generation).or_default().intro.push(Block::Figure(figure));
        self.edit_ged(
            move |g| {
                gedroots_book::spec::save(g, &spec);
                true
            },
            msg,
            window,
            cx,
        );
    }

    /// “Add to Book”: each book, then its chapters.
    fn add_to_book_button(&self, books: Vec<BookSpec>, cx: &mut Context<Self>) -> impl IntoElement {
        let me = cx.entity().downgrade();
        Button::new("viz-add-to-book")
            .small()
            .ghost()
            .icon(IconName::BookMarked)
            .label(t!("viz.add_to_book"))
            .dropdown_menu(move |mut menu, window, cx| {
                let Some(ws) = me.upgrade() else { return menu };
                let entries: Vec<BookEntry> = {
                    let this = ws.read(cx);
                    let Some(doc) = &this.doc else { return menu };
                    books
                        .iter()
                        .map(|b| {
                            let chapters = gedroots_book::outline(&doc.ged, b)
                                .map(|o| o.into_iter().map(|c| (c.generation, c.title)).collect())
                                .unwrap_or_default();
                            (b.xref.clone().unwrap_or_default(), this.book_title(b), chapters)
                        })
                        .collect()
                };
                for (id, title, chapters) in entries {
                    let me = me.clone();
                    menu = menu.submenu(title, window, cx, move |mut sub, _, _| {
                        for (n, chapter) in &chapters {
                            let (me, id, n) = (me.clone(), id.clone(), *n);
                            sub = sub.item(PopupMenuItem::new(chapter.clone()).on_click(move |_, window, cx| {
                                let (me, id) = (me.clone(), id.clone());
                                window.defer(cx, move |window, cx| {
                                    let _ = me.update(cx, |this, cx| this.add_viz_to_book(id, n, window, cx));
                                });
                            }));
                        }
                        sub
                    });
                }
                menu
            })
    }

    // ------------------------------------------------------------------
    // Rendering
    // ------------------------------------------------------------------

    /// The list of the visualizations.
    pub(super) fn render_viz_list(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let selected = self.selected.clone();
        v_flex()
            .id("viz-list")
            .w(px(260.))
            .flex_none()
            .overflow_y_scroll()
            .bg(theme::bg())
            .border_r_1()
            .border_color(theme::border())
            .p_2()
            .gap_0p5()
            .children(self.rows.iter().map(|r| {
                let on = selected.as_deref() == Some(r.key.as_str());
                let key = r.key.clone();
                let icon_name = Kind::from_slug(&r.key).map(icon_of).unwrap_or(IconName::Gauge);
                h_flex()
                    .id(SharedString::from(format!("viz-{}", r.key)))
                    .w_full()
                    .px_2()
                    .py_2()
                    .gap_3()
                    .items_start()
                    .rounded(theme::RADIUS_SM)
                    .cursor_pointer()
                    .when(on, |d| d.bg(theme::accent_soft()))
                    .when(!on, |d| d.hover(|d| d.bg(theme::surface())))
                    .child(
                        div()
                            .pt_0p5()
                            .text_color(if on { theme::accent() } else { theme::muted() })
                            .child(Icon::new(icon_name).size(px(16.))),
                    )
                    .child(
                        v_flex()
                            .min_w_0()
                            .gap_0p5()
                            .child(div().text_sm().font_weight(FontWeight::MEDIUM).truncate().child(r.title.clone()))
                            .child(div().text_xs().text_color(theme::muted()).child(r.meta.clone())),
                    )
                    .on_click(cx.listener(move |this, _, w, cx| this.select(key.clone(), w, cx)))
            }))
    }

    /// The selected visualization, or the tree's figures.
    pub(super) fn render_visualizations(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(kind) = self.viz_kind() else { return self.render_stats(cx) };
        let (Some(doc), Some(key)) = (&self.doc, self.viz_key()) else { return div().flex_1().into_any_element() };
        // Theme or language changed since the drawing: draw it again.
        if self.viz.shown.as_ref().is_some_and(|s| s.key.kind == key.kind && s.key != key) && self.viz.busy.is_none() {
            let this = cx.entity();
            cx.defer(move |cx| this.update(cx, |this, cx| this.refresh_viz(cx)));
        }
        let g = &doc.ged;
        let name = g.individual(&key.scope.subject).map(|i| i.display_name()).unwrap_or_default();
        let lines = Line::ALL
            .map(|l| {
                let label = match l {
                    Line::Ancestors => t!("books.ancestors"),
                    Line::Descendants => t!("books.descendants"),
                };
                let id = if l == Line::Ancestors { "viz-ancestors" } else { "viz-descendants" };
                gallery::segment(id, key.scope.line == l, label)
                    .on_click(cx.listener(move |this, _, _, cx| this.set_viz(cx, |v| v.line = l)))
                    .into_any_element()
            })
            .into_iter()
            .collect::<Vec<_>>();
        let gens = key.scope.generations;
        let step = |delta: i32| {
            move |this: &mut Self, _: &ClickEvent, _: &mut Window, cx: &mut Context<Self>| {
                let n = (this.viz.generations as i32 + delta)
                    .clamp(gedroots_viz::scope::MIN_GENERATIONS as i32, gedroots_viz::scope::MAX_GENERATIONS as i32)
                    as u32;
                this.set_viz(cx, |v| v.generations = n);
            }
        };
        let generations = h_flex()
            .gap_1()
            .items_center()
            .child(
                Button::new("viz-fewer")
                    .xsmall()
                    .ghost()
                    .icon(IconName::Minus)
                    .tooltip(t!("viz.fewer"))
                    .disabled(gens <= gedroots_viz::scope::MIN_GENERATIONS)
                    .on_click(cx.listener(step(-1))),
            )
            .child(div().text_sm().child(crate::tn!("books.generations", gens as usize)))
            .child(
                Button::new("viz-more")
                    .xsmall()
                    .ghost()
                    .icon(IconName::Plus)
                    .tooltip(t!("viz.more"))
                    .disabled(gens >= gedroots_viz::scope::MAX_GENERATIONS)
                    .on_click(cx.listener(step(1))),
            );
        let switch = |children: Vec<AnyElement>| {
            h_flex().p_0p5().gap_0p5().rounded(theme::RADIUS_SM).bg(theme::surface()).children(children)
        };
        let views = vec![
            gallery::segment("viz-chart", !self.viz.table, t!("viz.chart"))
                .on_click(cx.listener(|this, _, _, cx| {
                    this.viz.table = false;
                    cx.notify();
                }))
                .into_any_element(),
            gallery::segment("viz-table", self.viz.table, t!("viz.table"))
                .on_click(cx.listener(|this, _, _, cx| {
                    this.viz.table = true;
                    cx.notify();
                }))
                .into_any_element(),
        ];
        let controls = h_flex()
            .gap_3()
            .flex_wrap()
            .items_center()
            .child(
                Button::new("viz-person")
                    .small()
                    .label(name)
                    .icon(IconName::ChevronDown)
                    .tooltip(t!("viz.change_person"))
                    .on_click(cx.listener(|this, _, w, cx| this.open_picker(PickPurpose::VizPerson, w, cx))),
            )
            .when(kind.supports(Line::Descendants), |d| d.child(switch(lines)))
            .child(generations)
            .child(switch(views));
        let books = gedroots_book::spec::books(g);
        let title = h_flex()
            .gap_3()
            .items_center()
            .child(div().flex_1().min_w_0().text_xl().font_weight(FontWeight::SEMIBOLD).child(kind.title()))
            // Only when the tree has a book and the visualization something to show.
            .when(!books.is_empty() && self.viz.shown.as_ref().is_some_and(|s| s.figure.is_some()), |d| {
                d.child(self.add_to_book_button(books, cx))
            });
        let header = v_flex()
            .px_6()
            .pt_4()
            .pb_3()
            .gap_3()
            .border_b_1()
            .border_color(theme::border())
            .child(title)
            .child(controls);
        let shown = self.viz.shown.clone().filter(|s| s.key.kind == kind);
        let body: AnyElement = match shown {
            None => div().flex_1().into_any_element(),
            Some(s) if s.figure.is_none() => empty_state(icon_of(kind), t!("viz.empty")).into_any_element(),
            Some(s) if self.viz.table => self.render_viz_table(&s, cx),
            Some(s) => self.render_viz_chart(&s),
        };
        v_flex().flex_1().min_w_0().bg(theme::bg()).child(header).child(body).into_any_element()
    }

    fn render_viz_chart(&self, s: &Shown) -> AnyElement {
        let Some(figure) = &s.figure else { return div().into_any_element() };
        let legend = h_flex().gap_x_4().gap_y_2().flex_wrap().children(figure.legend.iter().map(|l| {
            let swatch = div()
                .size(px(10.))
                .flex_none()
                .rounded(px(2.))
                .bg(gpui::Rgba::try_from(l.color.as_str()).map(Hsla::from).unwrap_or(theme::muted()));
            let swatch =
                if l.unknown { swatch.border_1().border_dashed().border_color(theme::border_strong()) } else { swatch };
            h_flex()
                .gap_1()
                .items_center()
                .text_xs()
                .child(swatch)
                .child(l.label.clone())
                .child(div().text_color(theme::muted()).font_features(theme::tabular_nums()).child(l.count.to_string()))
        }));
        v_flex()
            .id("viz-chart")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .px_6()
            .py_4()
            .gap_4()
            .items_center()
            .child(
                v_flex()
                    .w_full()
                    .max_w(px(MAX_W))
                    .gap_3()
                    // The box keeps the drawing's proportions (an image's own height would be its pixels').
                    .children(
                        s.image.clone().map(|i| div().w_full().aspect_ratio(1. / s.ratio).child(img(i).size_full())),
                    )
                    .when(!figure.legend.is_empty(), |d| d.child(legend))
                    .when(!figure.caption.is_empty(), |d| {
                        d.child(div().text_xs().text_color(theme::muted()).child(figure.caption.clone()))
                    }),
            )
            .into_any_element()
    }

    fn render_viz_table(&self, s: &Shown, cx: &mut Context<Self>) -> AnyElement {
        let Some(figure) = &s.figure else { return div().into_any_element() };
        let table = &figure.table;
        // The first column is a number or a name: the person's column takes the room.
        let wide = |i: usize| table.columns.len() > 2 && i == if table.columns.len() == 4 { 1 } else { 0 };
        let cell = |i: usize, text: String| {
            let d = div().px_2().truncate();
            if wide(i) || (table.columns.len() <= 2 && i == 0) {
                d.flex_1().min_w_0().child(text)
            } else {
                // Tabular figures on years and counts only, never on names.
                let numeric = text.starts_with(|c: char| c.is_ascii_digit());
                d.w(px(120.)).flex_none().when(numeric, |d| d.font_features(theme::tabular_nums())).child(text)
            }
        };
        let head = h_flex()
            .py_1()
            .border_b_1()
            .border_color(theme::border())
            .text_xs()
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(theme::muted())
            .children(table.columns.iter().enumerate().map(|(i, c)| cell(i, c.clone())));
        let rows = table.rows.iter().enumerate().map(|(n, r)| {
            let record = r.record.clone();
            h_flex()
                .id(("viz-row", n))
                .py_1()
                .text_sm()
                .rounded(theme::RADIUS_SM)
                .children(r.cells.iter().enumerate().map(|(i, c)| cell(i, c.clone())))
                .when_some(record, |d, x| {
                    d.cursor_pointer()
                        .hover(|d| d.bg(theme::surface()))
                        .on_click(cx.listener(move |this, _, w, cx| this.goto(x.clone(), w, cx)))
                })
        });
        v_flex()
            .id("viz-table")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .px_6()
            .py_4()
            .items_center()
            .child(v_flex().w_full().max_w(px(MAX_W)).child(head).children(rows))
            .into_any_element()
    }
}
