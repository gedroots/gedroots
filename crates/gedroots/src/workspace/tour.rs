//! Guided tour (Help › Take the Tour): a few bubbles, each anchored on the part of the window it
//! talks about, one sentence each. The tour opens by itself the first time a tree opens on this
//! machine; that it was seen is a preference of the machine (a `tour` file in the configuration
//! folder), never stored in the `.ged`.
//!
//! A bubble is drawn by the element it points to (see [`Workspace::tour_anchor`]): it follows that
//! element wherever the layout puts it. A step whose element is not shown (hidden sidebar…) falls
//! back to a bubble at the top of the window. An action step has no Next button: it moves on when
//! the user does what it asks.

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
    /// No element: the bubble sits at the top of the window.
    Window,
}

/// What makes a step move on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Advance {
    Next,
    /// Opening the command palette.
    Palette,
}

struct Step {
    target: Target,
    title: fn() -> String,
    body: fn() -> String,
    advance: Advance,
}

/// The essentials: where things are, how the tree grows, how to find anything.
const ESSENTIALS: [Step; 4] = [
    Step {
        target: Target::Sidebar,
        title: || t!("tour.sidebar_title").into(),
        body: || t!("tour.sidebar_body").into(),
        advance: Advance::Next,
    },
    Step {
        target: Target::AddSlot,
        title: || t!("tour.grow_title").into(),
        body: || t!("tour.grow_body").into(),
        advance: Advance::Next,
    },
    Step {
        target: Target::Search,
        title: || t!("tour.search_title").into(),
        body: || t!("tour.search_body").into(),
        advance: Advance::Next,
    },
    Step {
        target: Target::Window,
        title: || t!("tour.commands_title").into(),
        body: || t!("tour.commands_body").into(),
        advance: Advance::Palette,
    },
];

pub(super) struct Tour {
    step: usize,
    /// The bubble of the step was drawn by its element during this frame.
    anchored: Cell<bool>,
}

/// File that records, on this machine, that the tour was offered.
fn seen_file() -> Option<PathBuf> {
    Some(dirs::config_dir()?.join("GEDRoots").join("tour"))
}

fn seen() -> bool {
    seen_file().is_none_or(|f| f.exists())
}

fn mark_seen() {
    if let Some(f) = seen_file() {
        if let Some(dir) = f.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let _ = std::fs::write(f, "seen\n");
    }
}

impl Workspace {
    /// First tree opened on this machine: the tour starts by itself, once.
    pub(super) fn offer_tour(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.tour.is_none() && !seen() {
            self.start_tour(window, cx);
        }
    }

    pub(super) fn start_tour(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.doc.is_none() {
            return;
        }
        mark_seen();
        self.tour = Some(Tour { step: 0, anchored: Cell::new(false) });
        self.enter_tour_step(window, cx);
    }

    /// Shows what the step talks about: the tree for its empty boxes.
    fn enter_tour_step(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(t) = &self.tour else { return };
        if ESSENTIALS[t.step].target == Target::AddSlot {
            self.close_overlays();
            if self.view != View::Individuals {
                self.show_view(View::Individuals, window, cx);
            }
            if self.selected.is_none()
                && let Some(root) = self.doc.as_ref().and_then(|d| d.ged.root())
            {
                self.select(root, window, cx);
            }
            self.tab = CenterTab::Tree;
        }
        cx.notify();
    }

    fn close_overlays(&mut self) {
        self.add_dialog = None;
        self.kinship = None;
    }

    pub(super) fn next_tour_step(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(t) = &mut self.tour else { return };
        if t.step + 1 >= ESSENTIALS.len() {
            return self.end_tour(cx);
        }
        t.step += 1;
        self.enter_tour_step(window, cx);
    }

    pub(super) fn end_tour(&mut self, cx: &mut Context<Self>) {
        if self.tour.take().is_some() {
            cx.notify();
        }
    }

    /// The user did `what`: the action step that waited for it moves on.
    pub(super) fn tour_did(&mut self, what: Advance, window: &mut Window, cx: &mut Context<Self>) {
        if self.tour.as_ref().is_some_and(|t| ESSENTIALS[t.step].advance == what) {
            self.next_tour_step(window, cx);
        }
    }

    /// Does the current step point to `target`?
    pub(super) fn tour_wants(&self, target: Target) -> bool {
        self.tour.as_ref().is_some_and(|t| ESSENTIALS[t.step].target == target)
    }

    /// Bubble and highlight of the current step, if it points to `target`: the element of
    /// `target` adds it as a child (it must be `relative`).
    pub(super) fn tour_anchor(&self, target: Target, cx: &mut Context<Self>) -> Option<AnyElement> {
        let t = self.tour.as_ref()?;
        let step = &ESSENTIALS[t.step];
        if step.target != target || t.anchored.get() {
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
        // Next to the element: to its right for the sidebar, above a box of the tree (below is its
        // person), below the search field.
        let (spot, corner) = match target {
            Target::Sidebar => (div().absolute().left_full().top(px(8.)).ml(px(12.)), Anchor::TopLeft),
            Target::AddSlot => (div().absolute().bottom_full().left_0().mb(px(10.)), Anchor::BottomLeft),
            _ => (div().absolute().top_full().left_0().mt(px(10.)), Anchor::TopLeft),
        };
        let bubble = self.tour_bubble(t.step, None, cx);
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

    /// Bubble of a step that no element drew this frame (no element, or element hidden): at the
    /// top of the window. Resets the frame's bookkeeping.
    pub(super) fn render_tour(&self, window: &mut Window, cx: &mut Context<Self>) -> Option<AnyElement> {
        let t = self.tour.as_ref()?;
        if t.anchored.replace(false) {
            return None;
        }
        let kbd = match ESSENTIALS[t.step].advance {
            Advance::Palette => Kbd::binding_for_action(&CommandPalette, None, window).map(|k| k.appearance(false)),
            Advance::Next => None,
        };
        Some(
            deferred(div().absolute().top(px(56.)).left_0().right_0().flex().justify_center().child(self.tour_bubble(
                t.step,
                kbd.map(IntoElement::into_any_element),
                cx,
            )))
            .with_priority(2)
            .into_any_element(),
        )
    }

    fn tour_bubble(&self, i: usize, kbd: Option<AnyElement>, cx: &mut Context<Self>) -> impl IntoElement {
        let step = &ESSENTIALS[i];
        let last = i + 1 == ESSENTIALS.len();
        let footer = h_flex()
            .gap_2()
            .items_center()
            .child(
                div()
                    .flex_1()
                    .text_xs()
                    .text_color(theme::muted())
                    .child(t!("tour.progress", n = i + 1, total = ESSENTIALS.len()).to_string()),
            )
            .map(|d| match step.advance {
                Advance::Next => d
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
                Advance::Palette => d.children(kbd).child(
                    Button::new("tour-leave")
                        .ghost()
                        .small()
                        .label(t!("tour.leave"))
                        .on_click(cx.listener(|this, _, _, cx| this.end_tour(cx))),
                ),
            });
        v_flex()
            .id("tour-bubble")
            .occlude()
            .w(px(300.))
            .p_4()
            .gap_1()
            .rounded(theme::RADIUS_MD)
            .border_1()
            .border_color(theme::border())
            .bg(theme::surface())
            .shadow_lg()
            .text_color(theme::text())
            .text_sm()
            .child(div().font_weight(FontWeight::SEMIBOLD).child((step.title)()))
            .child(div().text_color(theme::muted()).child((step.body)()))
            .child(div().pt_2().child(footer))
    }
}
