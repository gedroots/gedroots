//! Free chart: move around by dragging or with the wheel, in every direction. Generations of
//! ancestors and descendants load as you get close to them, and unload once you have moved well away
//! from them; a minimap shows where you are.
//!
//! When a generation is added or removed, the compact layout tightens or spreads the boxes already
//! shown: they then slide to their new place instead of jumping there, and the person at the center
//! of the view does not move.

pub(super) mod layout;

use super::*;
use layout::{CARD_H, CARD_W, COL, Card, Layout, Slot};
use std::cell::Cell;
use std::collections::HashMap;
use std::time::{Duration, Instant};

/// Generations loaded when arriving on a person.
const START_UP: u32 = 2;
const START_DOWN: u32 = 1;
/// Beyond that, stop loading (inconsistent file).
const MAX_GENS: u32 = 200;
/// Size of the view before the first display.
const DEFAULT_VIEW: (f32, f32) = (1000., 700.);
const MINI_W: f32 = 200.;
const MINI_H: f32 = 130.;
/// Movement beyond which a press becomes a drag (and not a click).
const DRAG_SLOP: f32 = 4.;
/// Duration of the boxes' slide when a generation is added.
const GLIDE: Duration = Duration::from_millis(180);
/// Margin around the view where boxes are already built: their portraits load before they enter the
/// view.
const PRELOAD: f32 = COL;
/// Distance beyond the view, in addition to the loading threshold, at which a generation unloads:
/// moving back and forth near the edge of the view does not load and unload it in a loop.
const UNLOAD_MARGIN: f32 = COL;

/// Identity of a box from one layout to the next: what it shows, its column, and its rank among
/// identical boxes of the column (pedigree collapse).
type CardKey = (Slot, i32, usize);

fn card_keys(l: &Layout) -> Vec<CardKey> {
    let mut seen: HashMap<(&Slot, i32), usize> = HashMap::new();
    l.cards
        .iter()
        .map(|c| {
            let col = (c.x / COL).round() as i32;
            let n = seen.entry((&c.slot, col)).or_default();
            *n += 1;
            (c.slot.clone(), col, *n - 1)
        })
        .collect()
}

/// Deceleration of the slide (`t` from 0 to 1).
fn ease_out(t: f32) -> f32 {
    1. - (1. - t).powi(3)
}

/// State of the free chart.
pub(super) struct Chart {
    /// Person around whom the chart is built.
    anchor: Option<String>,
    /// Last selection taken into account.
    seen: Option<String>,
    up: u32,
    down: u32,
    /// Point of the chart shown at the center of the view, up to the sign: position on screen =
    /// position in the chart + `pan` + half view.
    pan: (f32, f32),
    /// Drag in progress: start position of the pointer and last position taken into account. The
    /// movement is added to `pan` step by step, to keep the corrections made in between (generation
    /// loaded, edge of the chart).
    drag: Option<(Point<Pixels>, Point<Pixels>)>,
    /// The last press moved the chart: the following click is ignored.
    dragged: bool,
    /// The minimap is being manipulated.
    mini_drag: bool,
    view: Rc<Cell<Option<Bounds<Pixels>>>>,
    mini: Rc<Cell<Option<Bounds<Pixels>>>>,
    /// From the minimap to the chart: origin of the chart, scale, margins.
    mini_map: Cell<[f32; 4]>,
    /// Y coordinate of each box in the last layout, and its anchor.
    shown: HashMap<CardKey, f32>,
    shown_anchor: Option<String>,
    /// Boxes sliding: start and starting y coordinates.
    glide: Option<(Instant, HashMap<CardKey, f32>)>,
}

impl Default for Chart {
    fn default() -> Self {
        Self {
            anchor: None,
            seen: None,
            up: START_UP,
            down: START_DOWN,
            pan: (-CARD_W / 2., -CARD_H / 2.),
            drag: None,
            dragged: false,
            mini_drag: false,
            view: Rc::default(),
            mini: Rc::default(),
            mini_map: Cell::new([0., 0., 1., 0.]),
            shown: HashMap::new(),
            shown_anchor: None,
            glide: None,
        }
    }
}

impl Chart {
    fn view_size(&self) -> (f32, f32) {
        self.view.get().map(|b| (f32::from(b.size.width), f32::from(b.size.height))).unwrap_or(DEFAULT_VIEW)
    }

    /// Recenters the chart on `xref`, with the starting generations.
    fn reanchor(&mut self, xref: &str) {
        self.anchor = Some(xref.to_string());
        self.seen = Some(xref.to_string());
        self.up = START_UP;
        self.down = START_DOWN;
        self.pan = (-CARD_W / 2., -CARD_H / 2.);
    }

    fn center_on(&mut self, c: &Card) {
        let (x, y) = c.center();
        self.pan = (-x, -y);
    }

    /// Is the box entirely in the view?
    fn visible(&self, c: &Card) -> bool {
        let (w, h) = self.view_size();
        let (sx, sy) = (c.x + self.pan.0 + w / 2., c.y + self.pan.1 + h / 2.);
        sx >= 0. && sy >= 0. && sx + CARD_W <= w && sy + CARD_H <= h
    }

    /// Builds the chart, loading the generations the view gets close to and unloading those it has
    /// moved well away from (never below the starting generations). When a generation is added or
    /// removed, boxes can shift: the person closest to the center of the view stays in place on
    /// screen. Also returns the vertical shift of the view this required.
    fn layout(&mut self, g: &Gedcom, anchor: &str) -> (Layout, f32) {
        let before = self.pan.1;
        let mut l = layout::build(g, anchor, self.up, self.down);
        let (w, _) = self.view_size();
        for _ in 0..MAX_GENS {
            // As soon as the last loaded column enters the view, load the next one.
            let left = -self.pan.0 - w / 2.;
            let right = -self.pan.0 + w / 2.;
            let need_up = l.more_up && right > self.up as f32 * COL && self.up < MAX_GENS;
            let need_down = l.more_down && left < -(self.down as f32) * COL + CARD_W && self.down < MAX_GENS;
            // Mirror of the loading: the last column goes away once the view is past the point where
            // it would load again, plus a margin.
            let drop_up = !need_up && self.up > START_UP && right < (self.up - 1) as f32 * COL - UNLOAD_MARGIN;
            let drop_down =
                !need_down && self.down > START_DOWN && left > -((self.down - 1) as f32) * COL + CARD_W + UNLOAD_MARGIN;
            if !need_up && !need_down && !drop_up && !drop_down {
                break;
            }
            let focus = l.nearest(-self.pan.0, -self.pan.1).map(|c| (c.xref().unwrap_or("").to_string(), c.y));
            self.up = self.up + need_up as u32 - drop_up as u32;
            self.down = self.down + need_down as u32 - drop_down as u32;
            l = layout::build(g, anchor, self.up, self.down);
            if let Some((x, old)) = focus
                && let Some(c) = l.find(&x)
            {
                self.pan.1 += old - c.y;
            }
        }
        // The center of the view stays on the chart: it does not drift away into the void.
        let [x0, y0, x1, y1] = l.bounds;
        if x0 <= x1 && y0 <= y1 {
            self.pan = (self.pan.0.clamp(-x1, -x0), self.pan.1.clamp(-y1, -y0));
        }
        (l, self.pan.1 - before)
    }

    /// Displayed y coordinate of a box whose target place is `y`.
    fn glide_y(&self, key: &CardKey, y: f32, now: Instant) -> f32 {
        let Some((start, from)) = &self.glide else { return y };
        let t = now.duration_since(*start).as_secs_f32() / GLIDE.as_secs_f32();
        match from.get(key) {
            Some(f) if t < 1. => f + (y - f) * ease_out(t),
            _ => y,
        }
    }

    /// Y coordinates where the boxes of `l` are shown. If they moved since the previous frame
    /// (generation loaded, person added), they slide there from their previous place; `shift` is
    /// the shift of the view made in between to keep the center person still. Also returns whether
    /// the slide continues.
    fn glide(&mut self, l: &Layout, shift: f32, animate: bool) -> (Vec<f32>, bool) {
        let now = Instant::now();
        let keys = card_keys(l);
        let same_tree = animate && self.shown_anchor == self.anchor;
        let moved = keys.iter().zip(&l.cards).any(|(k, c)| self.shown.get(k).is_some_and(|y| (y - c.y).abs() > 0.5));
        if !same_tree {
            self.glide = None;
        } else if moved {
            // Start: where each box is shown now, view shift included.
            let from = self.shown.iter().map(|(k, y)| (k.clone(), self.glide_y(k, *y, now) - shift)).collect();
            self.glide = Some((now, from));
        }
        let ys = keys.iter().zip(&l.cards).map(|(k, c)| self.glide_y(k, c.y, now)).collect();
        let running = self.glide.as_ref().is_some_and(|(start, _)| now.duration_since(*start) < GLIDE);
        if !running {
            self.glide = None;
        }
        self.shown = keys.into_iter().zip(&l.cards).map(|(k, c)| (k, c.y)).collect();
        self.shown_anchor = self.anchor.clone();
        (ys, running)
    }

    fn pan_by(&mut self, dx: f32, dy: f32) {
        self.pan = (self.pan.0 + dx, self.pan.1 + dy);
    }

    /// Point of the minimap → center of the view.
    fn mini_jump(&mut self, pos: Point<Pixels>) {
        let Some(b) = self.mini.get() else { return };
        let [x0, y0, s, _] = self.mini_map.get();
        let x = (f32::from(pos.x - b.origin.x)) / s + x0;
        let y = (f32::from(pos.y - b.origin.y)) / s + y0;
        self.pan = (-x, -y);
    }
}

impl Workspace {
    /// Click on a box: selection; double-click: the chart recenters on it.
    fn chart_click(&mut self, xref: String, count: usize, window: &mut Window, cx: &mut Context<Self>) {
        if self.chart.get_mut().dragged {
            return;
        }
        if count >= 2 {
            self.center_chart_on(xref, window, cx);
        } else {
            self.select(xref, window, cx);
        }
    }

    /// Builds the chart around the person and shows it.
    pub(super) fn center_chart_on(&mut self, xref: String, window: &mut Window, cx: &mut Context<Self>) {
        self.tab = CenterTab::Tree;
        self.select(xref.clone(), window, cx);
        self.chart.get_mut().reanchor(&xref);
        cx.notify();
    }

    pub(super) fn render_chart(&self, cx: &mut Context<Self>) -> AnyElement {
        let (Some(doc), Some(sel)) = (&self.doc, &self.selected) else { return div().into_any_element() };
        let g = &doc.ged;
        let mut st = self.chart.borrow_mut();
        if st.anchor.as_deref().is_none_or(|a| g.individual(a).is_none()) {
            st.reanchor(sel);
        }
        let anchor = st.anchor.clone().unwrap_or_default();
        let (mut l, mut shift) = st.layout(g, &anchor);
        let mut animate = !cx.reduce_motion();
        // New selection (list, Back…): if the person is already in the chart, bring them into view;
        // otherwise the chart recenters on them.
        if st.seen.as_ref() != Some(sel) {
            st.seen = Some(sel.clone());
            match l.find(sel).cloned() {
                Some(c) if st.visible(&c) => {}
                Some(c) => {
                    st.center_on(&c);
                    animate = false;
                }
                None => st.reanchor(sel),
            }
            let anchor = st.anchor.clone().unwrap_or_default();
            let (l2, s2) = st.layout(g, &anchor);
            (l, shift) = (l2, shift + s2);
        }

        let view_cell = st.view.clone();
        let measure = move |bounds: Bounds<Pixels>, window: &mut Window, running: bool| {
            if view_cell.get().map(|b| b.size) != Some(bounds.size) {
                // First measurement, or window resized: lay out again.
                window.refresh();
            } else if running {
                window.request_animation_frame();
            }
            view_cell.set(Some(bounds));
        };
        if st.view.get().is_none() {
            // Size of the view still unknown: nothing is placed before it is known, so as not to
            // show the chart shifted for a frame.
            return div()
                .flex_1()
                .size_full()
                .child(canvas(move |b, w, _| measure(b, w, false), |_, _, _, _| {}).size_full())
                .into_any_element();
        }
        let (ys, running) = st.glide(&l, shift, animate);

        let (vw, vh) = st.view_size();
        let pan = st.pan;
        let to_screen = move |x: f32, y: f32| (x + pan.0 + vw / 2., y + pan.1 + vh / 2.);
        let near_view = |x: f32, y: f32| {
            let (sx, sy) = to_screen(x, y);
            sx + CARD_W > -PRELOAD && sy + CARD_H > -PRELOAD && sx < vw + PRELOAD && sy < vh + PRELOAD
        };
        let root = g.root();
        let cards: Vec<AnyElement> = l
            .cards
            .iter()
            .zip(&ys)
            .filter(|(c, y)| near_view(c.x, **y))
            .filter_map(|(c, y)| self.chart_card(doc, c, to_screen(c.x, *y), sel, root.as_deref(), cx))
            .collect();

        // Connectors, drawn under the boxes.
        let lines: Vec<[f32; 4]> = l
            .segments(&ys)
            .filter(|s| {
                let (x0, y0) = to_screen(s[0].min(s[2]), s[1].min(s[3]));
                let (x1, y1) = to_screen(s[0].max(s[2]), s[1].max(s[3]));
                x1 >= 0. && y1 >= 0. && x0 <= vw && y0 <= vh
            })
            .collect();
        let lines_el = canvas(
            move |bounds, window, _| measure(bounds, window, running),
            move |bounds, _, window, _| {
                let mut b = PathBuilder::stroke(px(1.));
                for s in &lines {
                    let (x0, y0) = to_screen(s[0], s[1]);
                    let (x1, y1) = to_screen(s[2], s[3]);
                    b.move_to(bounds.origin + point(px(x0), px(y0)));
                    b.line_to(bounds.origin + point(px(x1), px(y1)));
                }
                if let Ok(p) = b.build() {
                    window.paint_path(p, theme::border_strong());
                }
            },
        )
        .absolute()
        .inset_0();

        let mini = self.chart_minimap(&st, &l, sel, cx);
        drop(st);
        div()
            .id("chart")
            .relative()
            .flex_1()
            .size_full()
            .min_h_0()
            .overflow_hidden()
            .cursor(CursorStyle::OpenHand)
            .child(lines_el)
            .children(cards)
            .children(mini)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, ev: &MouseDownEvent, _, _| {
                    let st = this.chart.get_mut();
                    st.drag = Some((ev.position, ev.position));
                    st.dragged = false;
                }),
            )
            .on_mouse_move(cx.listener(|this, ev: &MouseMoveEvent, _, cx| {
                let st = this.chart.get_mut();
                if ev.pressed_button != Some(MouseButton::Left) {
                    st.drag = None;
                    st.mini_drag = false;
                    return;
                }
                if st.mini_drag {
                    st.mini_jump(ev.position);
                    cx.notify();
                } else if let Some((start, last)) = st.drag {
                    let moved = ev.position - start;
                    if f32::from(moved.x).abs() + f32::from(moved.y).abs() > DRAG_SLOP {
                        st.dragged = true;
                    }
                    if st.dragged {
                        let d = ev.position - last;
                        st.pan_by(f32::from(d.x), f32::from(d.y));
                        st.drag = Some((start, ev.position));
                        cx.notify();
                    }
                }
            }))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _: &MouseUpEvent, _, _| {
                    let st = this.chart.get_mut();
                    st.drag = None;
                    st.mini_drag = false;
                }),
            )
            .on_mouse_up_out(
                MouseButton::Left,
                cx.listener(|this, _: &MouseUpEvent, _, _| {
                    let st = this.chart.get_mut();
                    st.drag = None;
                    st.mini_drag = false;
                }),
            )
            .on_scroll(cx.listener(|this, ev: &ScrollEvent, _, cx| {
                let d = ev.delta.pixel_delta(px(40.));
                let (mut dx, mut dy) = (f32::from(d.x), f32::from(d.y));
                // Classic wheel with Shift: horizontal scrolling.
                if !ev.delta.precise() && ev.modifiers.shift && dx == 0. {
                    (dx, dy) = (dy, 0.);
                }
                this.chart.get_mut().pan_by(dx, dy);
                cx.notify();
            }))
            .into_any_element()
    }

    fn chart_card(
        &self,
        doc: &Document,
        c: &Card,
        (x, y): (f32, f32),
        sel: &str,
        root: Option<&str>,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let place = |d: Stateful<Div>| d.absolute().left(px(x)).top(px(y)).w(px(CARD_W)).h(px(CARD_H));
        let el = match &c.slot {
            Slot::Person { xref, side } => {
                let i = doc.ged.individual(xref)?;
                let on = xref == sel;
                // Line color for ancestors only (like the fan chart); descendants, spouses and
                // siblings stay neutral.
                let ring = match side {
                    0 => theme::border_strong(),
                    s => theme::lineage(*s as usize),
                };
                let target = xref.clone();
                place(
                    h_flex()
                        .id(SharedString::from(format!("chart-{xref}-{}-{}", c.x as i32, c.y as i32)))
                        .px_2()
                        .gap_2()
                        .items_center()
                        .rounded(theme::RADIUS_SM)
                        .border_1()
                        .border_color(if on { theme::accent() } else { theme::border() })
                        .bg(if on { theme::accent_soft() } else { theme::surface() })
                        .text_sm()
                        .cursor_pointer()
                        .hover(|d| d.border_color(theme::border_strong())),
                )
                .child(avatar(doc, &i, 28., ring))
                .child(
                    v_flex()
                        .flex_1()
                        .min_w_0()
                        .child(div().overflow_hidden().whitespace_nowrap().text_ellipsis().child(format!(
                            "{} {}",
                            i.surname(),
                            i.given()
                        )))
                        .child(mono(i.years())),
                )
                .when(root == Some(xref.as_str()), |d| d.child(icon(IconName::House).text_color(theme::accent())))
                .on_click(cx.listener(move |this, ev: &ClickEvent, w, cx| {
                    this.chart_click(target.clone(), ev.click_count(), w, cx)
                }))
                .with_menu(self.person_menu(xref, MenuFrom::Tree, cx))
                .into_any_element()
            }
            Slot::AddParent { child, rel } => {
                let (child, rel) = (child.clone(), *rel);
                let menu = self.slot_menu(&child, rel, cx);
                let label = if rel == Relation::Father { label::father() } else { label::mother() };
                place(add_slot(format!("chart-add-{child}-{rel:?}"), label))
                    .on_click(cx.listener(move |this, _, w, cx| {
                        if !this.chart.get_mut().dragged {
                            this.add_relative(child.clone(), rel, w, cx)
                        }
                    }))
                    .with_menu(menu)
                    .into_any_element()
            }
            Slot::AddChild(x) | Slot::AddSibling(x) => {
                let (rel, label) = match c.slot {
                    Slot::AddChild(_) => (Relation::Child, label::child()),
                    _ => (Relation::Sibling, label::sibling()),
                };
                let x = x.clone();
                let menu = self.slot_menu(&x, rel, cx);
                place(add_slot(format!("chart-add-{rel:?}"), label))
                    .on_click(cx.listener(move |this, _, w, cx| {
                        if !this.chart.get_mut().dragged {
                            this.add_relative(x.clone(), rel, w, cx)
                        }
                    }))
                    .with_menu(menu)
                    .into_any_element()
            }
        };
        Some(el)
    }

    /// Minimap, in the bottom right corner, when the chart overflows the view.
    fn chart_minimap(&self, st: &Chart, l: &Layout, sel: &str, cx: &mut Context<Self>) -> Option<AnyElement> {
        let (vw, vh) = st.view_size();
        let [bx0, by0, bx1, by1] = l.bounds;
        // What is visible counts too: you can see where you are even off the chart.
        let (v0, v1) = ((-st.pan.0 - vw / 2., -st.pan.1 - vh / 2.), (-st.pan.0 + vw / 2., -st.pan.1 + vh / 2.));
        let fits = bx0 >= v0.0 && by0 >= v0.1 && bx1 <= v1.0 && by1 <= v1.1;
        if fits || st.view.get().is_none() || vw < 2. * MINI_W || vh < 2. * MINI_H {
            return None;
        }
        let (x0, y0, x1, y1) = (bx0.min(v0.0), by0.min(v0.1), bx1.max(v1.0), by1.max(v1.1));
        let pad = 6.;
        let s = ((MINI_W - 2. * pad) / (x1 - x0)).min((MINI_H - 2. * pad) / (y1 - y0));
        // Margins to center the content in the minimap.
        let (ox, oy) = ((MINI_W - (x1 - x0) * s) / 2., (MINI_H - (y1 - y0) * s) / 2.);
        st.mini_map.set([x0 - ox / s, y0 - oy / s, s, 0.]);
        let boxes: Vec<(f32, f32, f32, Hsla)> = l
            .cards
            .iter()
            .filter_map(|c| {
                let color = match &c.slot {
                    Slot::Person { xref, .. } if xref == sel => theme::accent(),
                    Slot::Person { side: 2, .. } => theme::paternal(),
                    Slot::Person { side: 3, .. } => theme::maternal(),
                    Slot::Person { .. } => theme::muted(),
                    _ => return None,
                };
                Some(((c.x - x0) * s + ox, (c.y - y0) * s + oy, (CARD_W * s).max(1.), color))
            })
            .collect();
        let card_h = (CARD_H * s).max(1.);
        let frame = ((v0.0 - x0) * s + ox, (v0.1 - y0) * s + oy, vw * s, vh * s);
        let mini_cell = st.mini.clone();
        let map = canvas(
            move |bounds, _, _| mini_cell.set(Some(bounds)),
            move |bounds, _, window, _| {
                let o = bounds.origin;
                for (x, y, w, color) in &boxes {
                    window.paint_quad(fill(Bounds::new(o + point(px(*x), px(*y)), size(px(*w), px(card_h))), *color));
                }
                let (x, y, w, h) = frame;
                window.paint_quad(outline(
                    Bounds::new(o + point(px(x), px(y)), size(px(w), px(h))),
                    theme::accent(),
                    BorderStyle::Solid,
                ));
            },
        )
        .size_full();
        Some(
            div()
                .id("chart-minimap")
                .absolute()
                .right(px(12.))
                .bottom(px(12.))
                .w(px(MINI_W))
                .h(px(MINI_H))
                .rounded(theme::RADIUS_SM)
                .border_1()
                .border_color(theme::border())
                .bg(theme::panel())
                .overflow_hidden()
                .cursor_pointer()
                .child(map)
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, ev: &MouseDownEvent, _, cx| {
                        cx.stop_propagation();
                        let st = this.chart.get_mut();
                        st.mini_drag = true;
                        st.drag = None;
                        st.mini_jump(ev.position);
                        cx.notify();
                    }),
                )
                .into_any_element(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::{COL, Chart, START_DOWN, START_UP, UNLOAD_MARGIN};
    use gedcom::Gedcom;

    /// A line of `n` generations of fathers above `@I0@`, and of sons below it.
    fn line(n: usize) -> Gedcom {
        let mut text = String::from("0 HEAD\n1 GEDC\n2 VERS 7.0\n");
        let n = n as i32;
        for i in -n..=n {
            text += &format!("0 @I{i}@ INDI\n1 NAME P{i} /L/\n1 SEX M\n");
            if i < n {
                text += &format!("1 FAMC @F{i}@\n");
            }
            if i > -n {
                text += &format!("1 FAMS @F{}@\n", i - 1);
            }
        }
        for i in -n..n {
            text += &format!("0 @F{i}@ FAM\n1 HUSB @I{}@\n1 CHIL @I{i}@\n", i + 1);
        }
        text += "0 TRLR\n";
        Gedcom::parse(&text).unwrap()
    }

    #[test]
    fn generations_unload_behind_the_view() {
        let g = line(12);
        // What a fresh chart loads around the person.
        let mut fresh = Chart::default();
        fresh.reanchor("I0");
        fresh.layout(&g, "I0");
        let mut c = Chart::default();
        c.reanchor("I0");
        // Far among the ancestors: their generations load.
        c.pan.0 = -8. * COL;
        c.layout(&g, "I0");
        assert!(c.up > 8, "{}", c.up);
        // Back to the person: they unload, up to one generation of margin.
        c.pan.0 = fresh.pan.0;
        c.layout(&g, "I0");
        assert!(c.up >= START_UP && c.up <= fresh.up + 1, "{} / {}", c.up, fresh.up);
        // Same on the descendants' side.
        c.pan.0 = 8. * COL;
        c.layout(&g, "I0");
        assert!(c.down > 8, "{}", c.down);
        c.pan.0 = fresh.pan.0;
        c.layout(&g, "I0");
        assert!(c.down >= START_DOWN && c.down <= fresh.down + 1, "{} / {}", c.down, fresh.down);
    }

    #[test]
    fn no_flicker_at_the_edge() {
        let g = line(12);
        let mut c = Chart::default();
        c.reanchor("I0");
        c.pan.0 = -6. * COL;
        c.layout(&g, "I0");
        let loaded = c.up;
        // Moving back by less than the margin keeps what is loaded.
        c.pan.0 += UNLOAD_MARGIN / 2.;
        c.layout(&g, "I0");
        assert_eq!(c.up, loaded);
        c.pan.0 -= UNLOAD_MARGIN / 2.;
        c.layout(&g, "I0");
        assert_eq!(c.up, loaded);
    }
}
