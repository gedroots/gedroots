//! Guided tour (Help › Take the Tour): a few bubbles, each anchored on the part of the window it
//! talks about, one or two sentences each, in three chapters chosen in a small window:
//!
//! - **Essentials**, in the open tree: the sidebar, the tree's empty boxes, the search, the
//!   command palette;
//! - **Enter from sources** and **AI agents**, in the sample tree (see `sample.rs`), because they are
//!   learned by doing: the user links a person in a record and saves it, then reviews a record
//!   proposed as if by an agent (labelled as a demo: no agent runs).
//!
//! The chooser opens by itself the first time a tree opens on this machine. What was seen is a
//! preference of the machine (a `tour` file in the configuration folder: `seen`, then the finished
//! chapters), never stored in the `.ged`.
//!
//! A bubble is drawn by the element it points to (see [`Workspace::tour_anchor`]): it follows that
//! element wherever the layout puts it. A step whose element is not shown falls back to a bubble at
//! a fixed spot of the window. An action step has no Next button: it moves on when the user does
//! what it asks.

use super::*;
use gpui_kit::component::kbd::Kbd;
use std::cell::Cell;

/// Part of the window a step points to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Target {
    Sidebar,
    /// An empty box of the tree chart (add a parent, a child…).
    AddSlot,
    Search,
    /// “To Review” in the sidebar.
    ReviewZone,
    /// No element: at the top of the window.
    Top,
    /// No element: at the bottom left, over an image (record entry, review).
    Bottom,
}

/// What makes a step move on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Advance {
    Next,
    /// Opening the command palette.
    Palette,
    /// Linking Jacques in the record being entered.
    LinkFather,
    /// Saving the record being entered.
    SaveAct,
    /// Opening “To Review”.
    OpenReview,
    /// Saving or rejecting the proposal.
    Decide,
}

type Enter = fn(&mut Workspace, &mut Window, &mut Context<Workspace>);

struct Step {
    target: Target,
    title: fn() -> String,
    body: fn() -> String,
    advance: Advance,
    /// Prepares the window for the step (view to show, record to open…).
    enter: Option<Enter>,
}

const fn step(target: Target, title: fn() -> String, body: fn() -> String, advance: Advance) -> Step {
    Step { target, title, body, advance, enter: None }
}

const ESSENTIALS: [Step; 4] = [
    step(Target::Sidebar, || t!("tour.sidebar_title").into(), || t!("tour.sidebar_body").into(), Advance::Next),
    Step {
        enter: Some(|ws, w, cx| {
            ws.show_view(View::Individuals, w, cx);
            if let Some(root) = ws.doc.as_ref().and_then(|d| d.ged.root()) {
                ws.select(root, w, cx);
            }
            ws.tab = CenterTab::Tree;
        }),
        ..step(Target::AddSlot, || t!("tour.grow_title").into(), || t!("tour.grow_body").into(), Advance::Next)
    },
    step(Target::Search, || t!("tour.search_title").into(), || t!("tour.search_body").into(), Advance::Next),
    step(Target::Top, || t!("tour.commands_title").into(), || t!("tour.commands_body").into(), Advance::Palette),
];

const SOURCES: [Step; 5] = [
    Step {
        enter: Some(|ws, w, cx| {
            ws.act_editor = None;
            ws.open_location(View::Sources, Some(crate::sample::REGISTER.into()), w, cx);
        }),
        ..step(Target::Top, || t!("tour.register_title").into(), || t!("tour.register_body").into(), Advance::Next)
    },
    Step {
        enter: Some(|ws, w, cx| {
            ws.open_location(View::Sources, Some(crate::sample::REGISTER.into()), w, cx);
            ws.tab = CenterTab::Form;
            ws.show_act_editor(crate::sample::birth_act(), w, cx);
        }),
        ..step(Target::Bottom, || t!("tour.record_title").into(), || t!("tour.record_body").into(), Advance::Next)
    },
    Step {
        enter: Some(|ws, _, cx| {
            if let Some((e, _)) = &ws.act_editor {
                e.update(cx, |e, cx| e.set_tour_link(Some(crate::sample::JACQUES.into()), cx));
            }
        }),
        ..step(Target::Bottom, || t!("tour.link_title").into(), || t!("tour.link_body").into(), Advance::LinkFather)
    },
    Step {
        enter: Some(|ws, _, cx| {
            if let Some((e, _)) = &ws.act_editor {
                e.update(cx, |e, cx| e.set_tour_link(None, cx));
            }
        }),
        ..step(Target::Bottom, || t!("tour.save_title").into(), || t!("tour.save_body").into(), Advance::SaveAct)
    },
    Step {
        enter: Some(|ws, w, cx| {
            ws.open_location(View::Individuals, Some(crate::sample::EMILE.into()), w, cx);
            ws.tab = CenterTab::Form;
        }),
        ..step(Target::Top, || t!("tour.cited_title").into(), || t!("tour.cited_body").into(), Advance::Next)
    },
];

const AGENTS: [Step; 5] = [
    Step {
        enter: Some(|ws, w, cx| {
            ws.act_editor = None;
            ws.review.open = false;
            if ws.review.is_empty() {
                let path = ws.doc.as_ref().and_then(|d| d.path.clone()).unwrap_or_default();
                let res = crate::sample::demo_proposal(&path).and_then(|p| ws.queue_proposal(p, true, cx));
                if let Err(e) = res {
                    ws.set_error("tour.demo_failed", t!("tour.demo_failed", error = e));
                }
            }
            ws.show_view(View::Individuals, w, cx);
        }),
        ..step(
            Target::ReviewZone,
            || t!("tour.proposal_title").into(),
            || t!("tour.proposal_body").into(),
            Advance::OpenReview,
        )
    },
    step(Target::Bottom, || t!("tour.compare_title").into(), || t!("tour.compare_body").into(), Advance::Next),
    step(Target::Bottom, || t!("tour.lines_title").into(), || t!("tour.lines_body").into(), Advance::Next),
    step(Target::Bottom, || t!("tour.decide_title").into(), || t!("tour.decide_body").into(), Advance::Decide),
    step(Target::Top, || t!("tour.connect_title").into(), || t!("tour.connect_body").into(), Advance::Next),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Chapter {
    Essentials,
    Sources,
    Agents,
}

impl Chapter {
    pub(crate) const ALL: [Chapter; 3] = [Chapter::Essentials, Chapter::Sources, Chapter::Agents];

    fn steps(self) -> &'static [Step] {
        match self {
            Chapter::Essentials => &ESSENTIALS,
            Chapter::Sources => &SOURCES,
            Chapter::Agents => &AGENTS,
        }
    }

    /// Runs in the sample tree.
    fn in_sample(self) -> bool {
        self != Chapter::Essentials
    }

    /// Name in the `tour` file.
    fn key(self) -> &'static str {
        match self {
            Chapter::Essentials => "essentials",
            Chapter::Sources => "sources",
            Chapter::Agents => "agents",
        }
    }

    fn title(self) -> String {
        match self {
            Chapter::Essentials => t!("tour.essentials"),
            Chapter::Sources => t!("tour.sources"),
            Chapter::Agents => t!("tour.agents"),
        }
        .into_owned()
    }

    fn summary(self) -> String {
        match self {
            Chapter::Essentials => t!("tour.essentials_summary"),
            Chapter::Sources => t!("tour.sources_summary"),
            Chapter::Agents => t!("tour.agents_summary"),
        }
        .into_owned()
    }

    fn icon(self) -> IconName {
        match self {
            Chapter::Essentials => IconName::Compass,
            Chapter::Sources => IconName::BookOpen,
            Chapter::Agents => IconName::Bot,
        }
    }

    fn minutes(self) -> usize {
        match self {
            Chapter::Essentials => 1,
            _ => 2,
        }
    }
}

pub(super) struct Tour {
    /// Chapters to go through, in order.
    chapters: Vec<Chapter>,
    chapter: usize,
    step: usize,
    /// The bubble of the step was drawn by its element during this frame.
    anchored: Cell<bool>,
}

impl Tour {
    fn current(&self) -> &'static Step {
        &self.chapters[self.chapter].steps()[self.step]
    }
}

/// File that records, on this machine, that the tour was offered and the chapters finished.
fn prefs_file() -> Option<PathBuf> {
    Some(dirs::config_dir()?.join("GEDRoots").join("tour"))
}

fn prefs() -> Option<Vec<String>> {
    let text = std::fs::read_to_string(prefs_file()?).ok()?;
    Some(text.lines().map(str::to_string).collect())
}

fn write_prefs(lines: &[String]) {
    if let Some(f) = prefs_file() {
        if let Some(dir) = f.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let _ = std::fs::write(f, lines.iter().map(|l| format!("{l}\n")).collect::<String>());
    }
}

/// Marks `line` (`seen` or a chapter) as done on this machine.
fn remember(line: &str) {
    let mut lines = prefs().unwrap_or_default();
    if !lines.iter().any(|l| l == line) {
        lines.push(line.to_string());
        write_prefs(&lines);
    }
}

fn done(chapter: Chapter) -> bool {
    prefs().is_some_and(|l| l.iter().any(|l| l == chapter.key()))
}

impl Workspace {
    /// First tree opened on this machine: the chapters are offered, once.
    pub(super) fn offer_tour(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        if self.tour.is_none() && prefs_file().is_some() && prefs().is_none() {
            remember("seen");
            self.tour_chooser = true;
            cx.notify();
        }
    }

    /// Help › Take the Tour: the chapters.
    pub(super) fn open_tour_chooser(&mut self, cx: &mut Context<Self>) {
        self.tour = None;
        self.tour_chooser = true;
        cx.notify();
    }

    /// Runs `chapters`: here, or in a new sample tree window when one of them needs it.
    fn run_chapters(&mut self, chapters: Vec<Chapter>, window: &mut Window, cx: &mut Context<Self>) {
        self.tour_chooser = false;
        let sample = self.doc.as_ref().is_some_and(|d| d.sample);
        if !sample && chapters.iter().any(|c| c.in_sample()) {
            cx.defer(move |cx| crate::open_sample_with_tour(chapters, cx));
            return cx.notify();
        }
        self.start_chapters(chapters, window, cx);
    }

    pub(crate) fn start_chapters(&mut self, chapters: Vec<Chapter>, window: &mut Window, cx: &mut Context<Self>) {
        if self.doc.is_none() || chapters.is_empty() {
            return;
        }
        self.tour = Some(Tour { chapters, chapter: 0, step: 0, anchored: Cell::new(false) });
        self.enter_tour_step(window, cx);
    }

    fn enter_tour_step(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(t) = &self.tour else { return };
        if let Some(enter) = t.current().enter {
            self.add_dialog = None;
            self.kinship = None;
            enter(self, window, cx);
        }
        cx.notify();
    }

    pub(super) fn next_tour_step(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(t) = &mut self.tour else { return };
        if t.step + 1 < t.chapters[t.chapter].steps().len() {
            t.step += 1;
        } else {
            remember(t.chapters[t.chapter].key());
            if t.chapter + 1 >= t.chapters.len() {
                return self.end_tour(cx);
            }
            t.chapter += 1;
            t.step = 0;
        }
        self.enter_tour_step(window, cx);
    }

    pub(super) fn end_tour(&mut self, cx: &mut Context<Self>) {
        if let Some(t) = self.tour.take() {
            if let Some((e, _)) = &self.act_editor {
                e.update(cx, |e, cx| e.set_tour_link(None, cx));
            }
            let _ = t;
            cx.notify();
        }
    }

    /// The user did `what`: the step that waited for it, or an earlier step of the same chapter
    /// that it makes pointless (saving before linking), moves on past it. Applied on the next
    /// frame, which has the window.
    pub(super) fn tour_did(&mut self, what: Advance, cx: &mut Context<Self>) {
        let Some(t) = &mut self.tour else { return };
        let steps = t.chapters[t.chapter].steps();
        if let Some(i) = (t.step..steps.len()).find(|&i| steps[i].advance == what) {
            t.step = i;
            self.tour_pending = true;
            cx.notify();
        }
    }

    /// At each frame: moves on when the step's action is done (see [`Workspace::tour_did`]), or
    /// when the record being entered links Jacques.
    pub(super) fn check_tour(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(t) = &self.tour else { return };
        let linked = t.current().advance == Advance::LinkFather
            && self.act_editor.as_ref().is_some_and(|(e, _)| e.read(cx).links(crate::sample::JACQUES));
        if std::mem::take(&mut self.tour_pending) || linked {
            self.next_tour_step(window, cx);
        }
    }

    /// Does the current step point to `target`?
    pub(super) fn tour_wants(&self, target: Target) -> bool {
        self.tour.as_ref().is_some_and(|t| t.current().target == target)
    }

    /// Bubble and highlight of the current step, if it points to `target`: the element of
    /// `target` adds it as a child.
    pub(super) fn tour_anchor(&self, target: Target, cx: &mut Context<Self>) -> Option<AnyElement> {
        let t = self.tour.as_ref()?;
        if t.current().target != target || t.anchored.get() {
            return None;
        }
        t.anchored.set(true);
        // Around the element; inside it for the sidebar, which touches the edges of the window.
        let o = if target == Target::Sidebar { px(4.) } else { px(-3.) };
        let ring = div()
            .absolute()
            .top(o)
            .left(o)
            .right(o)
            .bottom(o)
            .border_2()
            .border_color(theme::accent())
            .rounded(theme::RADIUS_SM);
        // Next to the element: to its right in the sidebar, above a box of the tree (below is its
        // person), below the search field.
        let (spot, corner) = match target {
            Target::Sidebar | Target::ReviewZone => {
                (div().absolute().left_full().top(px(-8.)).ml(px(16.)), Anchor::TopLeft)
            }
            Target::AddSlot => (div().absolute().bottom_full().left_0().mb(px(10.)), Anchor::BottomLeft),
            _ => (div().absolute().top_full().left_0().mt(px(10.)), Anchor::TopLeft),
        };
        let bubble = self.tour_bubble(t, None, cx);
        Some(
            div()
                .absolute()
                .inset_0()
                .child(ring)
                .child(
                    spot.child(
                        deferred(anchored().anchor(corner).snap_to_window_with_margin(px(8.)).child(bubble))
                            .with_priority(2),
                    ),
                )
                .into_any_element(),
        )
    }

    /// Bubble of a step that no element drew this frame (no element, or element hidden), at its
    /// spot of the window, and the chapter chooser. Resets the frame's bookkeeping.
    pub(super) fn render_tour(&self, window: &mut Window, cx: &mut Context<Self>) -> Option<AnyElement> {
        if self.tour_chooser {
            return Some(self.render_tour_chooser(cx));
        }
        let t = self.tour.as_ref()?;
        if t.anchored.replace(false) {
            return None;
        }
        let kbd = match t.current().advance {
            Advance::Palette => Kbd::binding_for_action(&CommandPalette, None, window).map(|k| k.appearance(false)),
            _ => None,
        };
        let bubble = self.tour_bubble(t, kbd.map(IntoElement::into_any_element), cx);
        let spot = match t.current().target {
            Target::Bottom => div().absolute().bottom(px(24.)).left(px(204.)),
            _ => div().absolute().top(px(56.)).left_0().right_0().flex().justify_center(),
        };
        Some(deferred(spot.child(bubble)).with_priority(2).into_any_element())
    }

    fn tour_bubble(&self, t: &Tour, kbd: Option<AnyElement>, cx: &mut Context<Self>) -> impl IntoElement {
        let chapter = t.chapters[t.chapter];
        let step = t.current();
        let total = chapter.steps().len();
        let last = t.step + 1 == total && t.chapter + 1 == t.chapters.len();
        let progress = t!("tour.progress", chapter = chapter.title(), n = t.step + 1, total = total).into_owned();
        let leave = || {
            Button::new("tour-leave")
                .ghost()
                .small()
                .label(t!("tour.leave"))
                .on_click(cx.listener(|this, _, _, cx| this.end_tour(cx)))
        };
        let actions = match step.advance {
            Advance::Next => h_flex()
                .gap_2()
                .child(
                    Button::new("tour-skip")
                        .ghost()
                        .small()
                        .label(t!("tour.skip"))
                        .on_click(cx.listener(|this, _, _, cx| this.end_tour(cx))),
                )
                .child(
                    Button::new("tour-next")
                        .primary()
                        .small()
                        .label(if last { t!("tour.done") } else { t!("tour.next") })
                        .on_click(cx.listener(|this, _, w, cx| this.next_tour_step(w, cx))),
                ),
            // An action step: the action to do, and a way out.
            _ => h_flex().gap_2().items_center().children(kbd).child(leave()),
        };
        v_flex()
            .id("tour-bubble")
            .occlude()
            .w(px(320.))
            .p_4()
            .gap_1()
            .rounded(theme::RADIUS_MD)
            .border_1()
            .border_color(theme::border())
            .bg(theme::surface())
            .shadow_lg()
            .text_color(theme::text())
            .text_sm()
            .child(div().text_xs().text_color(theme::muted()).child(progress))
            .child(div().font_weight(FontWeight::SEMIBOLD).child((step.title)()))
            .child(div().text_color(theme::muted()).child((step.body)()))
            .child(h_flex().pt_2().justify_end().child(actions))
    }

    fn render_tour_chooser(&self, cx: &mut Context<Self>) -> AnyElement {
        let row = |c: Chapter| {
            h_flex()
                .id(SharedString::from(format!("tour-{}", c.key())))
                .gap_3()
                .p_3()
                .items_center()
                .rounded(theme::RADIUS_SM)
                .border_1()
                .border_color(theme::border())
                .cursor_pointer()
                .hover(|d| d.bg(theme::hover()))
                .child(
                    div()
                        .flex_none()
                        .size(px(32.))
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded(theme::RADIUS_SM)
                        .bg(theme::accent_soft())
                        .child(Icon::new(c.icon()).size(px(16.)).text_color(theme::accent())),
                )
                .child(
                    v_flex()
                        .flex_1()
                        .min_w_0()
                        .child(div().font_weight(FontWeight::SEMIBOLD).child(c.title()))
                        .child(div().text_xs().text_color(theme::muted()).child(c.summary()))
                        .when(c.in_sample(), |d| {
                            d.child(
                                div().text_xs().text_color(theme::accent()).child(t!("tour.in_sample").into_owned()),
                            )
                        }),
                )
                .child(match done(c) {
                    true => Icon::new(IconName::Check).size(px(16.)).text_color(theme::ok()).into_any_element(),
                    false => div()
                        .text_xs()
                        .text_color(theme::muted())
                        .child(t!("tour.minutes", n = c.minutes()).into_owned())
                        .into_any_element(),
                })
                .on_click(cx.listener(move |this, _, w, cx| this.run_chapters(vec![c], w, cx)))
        };
        let mac = cfg!(target_os = "macos");
        let later = Button::new("tour-later").ghost().small().label(t!("tour.later")).on_click(cx.listener(
            |this, _, _, cx| {
                this.tour_chooser = false;
                cx.notify();
            },
        ));
        let start = Button::new("tour-start")
            .primary()
            .small()
            .label(t!("tour.start"))
            .on_click(cx.listener(|this, _, w, cx| this.run_chapters(Chapter::ALL.to_vec(), w, cx)));
        div()
            .absolute()
            .inset_0()
            .flex()
            .items_center()
            .justify_center()
            .bg(theme::scrim())
            .occlude()
            .child(
                v_flex()
                    .id("tour-chooser")
                    .w(px(460.))
                    .p_5()
                    .gap_3()
                    .rounded(theme::RADIUS_LG)
                    .border_1()
                    .border_color(theme::border())
                    .bg(theme::surface())
                    .shadow_lg()
                    .child(div().text_xl().font_weight(FontWeight::SEMIBOLD).child(t!("tour.title").into_owned()))
                    .children(Chapter::ALL.map(row))
                    .child(h_flex().pt_2().gap_2().justify_end().map(|d| match mac {
                        true => d.child(later).child(start),
                        false => d.child(start).child(later),
                    })),
            )
            .into_any_element()
    }
}
