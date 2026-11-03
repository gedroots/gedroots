//! Tree views: free tree (see `chart`) and fan chart.

use super::*;
use gpui_kit::component::menu::ContextMenuExt;
use rust_i18n::t;
use std::cell::{Cell, RefCell};
use std::f32::consts::PI;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TreeMode {
    /// Ancestors and descendants around a person, to browse freely.
    Chart,
    Fan,
}

impl TreeMode {
    pub(super) fn generations(self) -> std::ops::RangeInclusive<u32> {
        match self {
            // Generations loaded as you move around.
            TreeMode::Chart => 0..=0,
            TreeMode::Fan => 2..=7,
        }
    }
}

/// Opening angle of the fan chart (degrees), on each side of the vertical.
const FAN_SPAN: f32 = 240.;
const FAN_LEFT: f32 = 90. + FAN_SPAN / 2.;

/// Outer radii of each generation (1 = central disc).
fn fan_radii(gens: u32) -> Vec<f32> {
    let widths = [64., 70., 70., 70., 62., 46., 38.];
    let mut r = 0.;
    widths
        .iter()
        .take(gens as usize)
        .map(|w| {
            r += w;
            r
        })
        .collect()
}

/// Point at angle `deg` (counterclockwise, 0° = right) and radius `r`.
fn polar(c: Point<Pixels>, r: f32, deg: f32) -> Point<Pixels> {
    let a = deg * PI / 180.;
    point(c.x + px(r * a.cos()), c.y - px(r * a.sin()))
}

/// Sector of Sosa `k`: (inner radius, outer radius, start angle, end angle).
fn sector(k: usize, radii: &[f32]) -> (f32, f32, f32, f32) {
    let g = (usize::BITS - k.leading_zeros()) as usize; // generation (1 = root)
    let first = 1usize << (g - 1);
    let n = first as f32;
    let i = (k - first) as f32;
    let step = FAN_SPAN / n;
    let inner = if g >= 2 { radii[g - 2] } else { 0. };
    (inner, radii[g - 1], FAN_LEFT - (i + 1.) * step, FAN_LEFT - i * step)
}

fn sector_path(c: Point<Pixels>, r0: f32, r1: f32, a0: f32, a1: f32, mut builder: PathBuilder) -> Option<Path<Pixels>> {
    let steps = (((a1 - a0) / 3.).ceil() as usize).max(2);
    builder.move_to(polar(c, r1, a0));
    for s in 1..=steps {
        builder.line_to(polar(c, r1, a0 + (a1 - a0) * s as f32 / steps as f32));
    }
    for s in (0..=steps).rev() {
        builder.line_to(polar(c, r0, a0 + (a1 - a0) * s as f32 / steps as f32));
    }
    builder.close();
    builder.build().ok()
}

fn circle_path(c: Point<Pixels>, r: f32, mut builder: PathBuilder) -> Option<Path<Pixels>> {
    builder.move_to(polar(c, r, 0.));
    for s in 1..=72 {
        builder.line_to(polar(c, r, s as f32 * 5.));
    }
    builder.close();
    builder.build().ok()
}

fn with_alpha(mut c: Hsla, a: f32) -> Hsla {
    c.a = a;
    c
}

impl Workspace {
    /// “Tree” tab: choice of view and number of generations.
    pub(super) fn render_tree_tab(&self, cx: &mut Context<Self>) -> AnyElement {
        let mode = self.tree_mode;
        let gens = self.tree_gens.clamp(*mode.generations().start(), *mode.generations().end());
        // View and generations: View › Tree menu (⌘= / ⌘-).
        let body: AnyElement = match mode {
            TreeMode::Chart => self.render_chart(cx),
            TreeMode::Fan => self.render_fan(gens, cx),
        };
        v_flex().flex_1().min_h_0().child(body).into_any_element()
    }

    // ------------------------------------------------------------------
    // Fan chart
    // ------------------------------------------------------------------

    fn render_fan(&self, gens: u32, cx: &mut Context<Self>) -> AnyElement {
        let (Some(doc), Some(root)) = (&self.doc, &self.selected) else { return div().into_any_element() };
        let g = &doc.ged;
        let mut anc = g.ancestry(root, gens);
        // Known generations plus one empty ring to fill in, rather than rings of empty boxes.
        let known =
            (2..anc.len()).filter(|k| anc[*k].is_some()).map(|k| usize::BITS - k.leading_zeros()).max().unwrap_or(1);
        let gens = (known + 1).clamp(2, gens);
        anc.truncate(1 << gens);
        let has_ancestors = known > 1;
        let radii = fan_radii(gens);
        let rmax = *radii.last().unwrap();
        let (w, h) = (2. * rmax + 24., rmax * (1. + (FAN_SPAN / 2. - 90.).to_radians().sin()) + 24.);
        let center_rel = point(px(w / 2.), px(rmax + 12.));

        // Colors of each sector, computed before drawing.
        let fills: Vec<(usize, Hsla)> = (1..anc.len())
            .map(|k| {
                let gn = (usize::BITS - k.leading_zeros()) as f32;
                let color = match &anc[k] {
                    Some(_) if k == 1 => theme::accent_soft(),
                    Some(_) => with_alpha(theme::lineage(k), (0.62 - 0.07 * gn).max(0.18)),
                    None => theme::surface(),
                };
                (k, color)
            })
            .collect();
        let bounds_cell: Rc<Cell<Option<Bounds<Pixels>>>> = Rc::new(Cell::new(None));
        let paint_radii = radii.clone();
        let cell = bounds_cell.clone();
        let canvas_el = canvas(
            move |bounds, _, _| cell.set(Some(bounds)),
            move |bounds, _, window, _| {
                let c = bounds.origin + center_rel;
                for (k, color) in &fills {
                    let (r0, r1, a0, a1) = sector(*k, &paint_radii);
                    let shape = |b| if *k == 1 { circle_path(c, r1, b) } else { sector_path(c, r0, r1, a0, a1, b) };
                    if let Some(p) = shape(PathBuilder::fill()) {
                        window.paint_path(p, *color);
                    }
                    if let Some(p) = shape(PathBuilder::stroke(px(1.5))) {
                        window.paint_path(p, theme::bg());
                    }
                }
            },
        )
        .absolute()
        .inset_0();

        // Labels, placed at the center of each sector.
        let labels = (1..anc.len()).filter_map(|k| {
            let i = g.individual(anc[k].as_deref()?)?;
            let gn = (usize::BITS - k.leading_zeros()) as usize;
            if gn > 5 {
                return None;
            }
            let (r0, r1, a0, a1) = sector(k, &radii);
            let (pos, bw) = if k == 1 {
                (center_rel, 2. * r1 - 16.)
            } else {
                let mid = (r0 + r1) / 2.;
                let arc = (a1 - a0).to_radians() * mid;
                (polar(center_rel, mid, (a0 + a1) / 2.), arc.min(r1 - r0 + 40.).max(40.) - 6.)
            };
            let text = match gn {
                1..=3 => vec![i.surname(), i.given(), i.years()],
                4 => vec![i.surname(), i.given().split_whitespace().next().unwrap_or("").to_string()],
                _ => vec![i.surname()],
            };
            Some(
                v_flex()
                    .absolute()
                    .left(pos.x - px(bw / 2.))
                    .top(pos.y - px(if gn <= 3 { 22. } else { 12. }))
                    .w(px(bw))
                    .items_center()
                    .text_size(px(if gn <= 2 {
                        12.
                    } else if gn <= 4 {
                        10.5
                    } else {
                        9.
                    }))
                    .line_height(px(if gn <= 4 { 14. } else { 11. }))
                    .children(text.into_iter().enumerate().filter(|(_, s)| !s.is_empty()).map(|(n, s)| {
                        div()
                            .max_w_full()
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .when(n == 0, |d| d.font_weight(FontWeight::SEMIBOLD))
                            .when(n == 2, |d| d.text_color(theme::muted()))
                            .child(s)
                    })),
            )
        });

        let hit_radii = radii.clone();
        // Person under the pointer (Sosa index in `anc`).
        let hit = Rc::new(move |pos: Point<Pixels>| -> Option<usize> {
            let b = bounds_cell.get()?;
            let c = b.origin + center_rel;
            let (dx, dy) = (f32::from(pos.x - c.x), f32::from(c.y - pos.y));
            let r = (dx * dx + dy * dy).sqrt();
            let gn = hit_radii.iter().position(|outer| r <= *outer).map(|i| i + 1)?;
            if gn == 1 {
                return Some(1);
            }
            let mut a = dy.atan2(dx).to_degrees();
            if a < FAN_LEFT - FAN_SPAN - 1e-3 {
                a += 360.;
            }
            if !(FAN_LEFT - FAN_SPAN..=FAN_LEFT).contains(&a) {
                return None;
            }
            let n = 1usize << (gn - 1);
            let i = (((FAN_LEFT - a) / (FAN_SPAN / n as f32)) as usize).min(n - 1);
            Some(n + i)
        });
        let (anc_click, hit_left) = (anc.clone(), hit.clone());
        // Right-click: the targeted person is remembered, the menu is built right after.
        let target: Rc<RefCell<Option<String>>> = Rc::default();
        let (t_set, t_get, anc_right) = (target.clone(), target, anc.clone());
        let ws = cx.entity().downgrade();
        let fan = div()
            .id("fan")
            .relative()
            .flex_none()
            .w(px(w))
            .h(px(h))
            .cursor_pointer()
            .child(canvas_el)
            .children(labels)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, ev: &MouseDownEvent, window, cx| {
                    if let Some(k) = hit_left(ev.position)
                        && k > 1
                        && let Some(Some(x)) = anc_click.get(k)
                    {
                        this.select(x.clone(), window, cx);
                    }
                }),
            )
            .on_mouse_down(MouseButton::Right, move |ev: &MouseDownEvent, _, _| {
                *t_set.borrow_mut() = hit(ev.position).and_then(|k| anc_right.get(k).cloned().flatten());
            })
            .context_menu(move |menu, w, cx| match t_get.borrow().clone() {
                Some(x) => menus::person_menu(ws.clone(), x, MenuFrom::Link)(menu, w, cx),
                None => menu,
            });
        div()
            .id("fan-scroll")
            .flex_1()
            .overflow_scroll()
            .p_4()
            .child(
                v_flex()
                    .gap_2()
                    .items_center()
                    // Lineage legend: only when there is a lineage to show.
                    .when(has_ancestors, |d| {
                        d.child(
                            h_flex()
                                .gap_4()
                                .text_xs()
                                .text_color(theme::muted())
                                .child(
                                    h_flex()
                                        .gap_2()
                                        .items_center()
                                        .child(dot(theme::paternal()))
                                        .child(t!("tree.paternal")),
                                )
                                .child(
                                    h_flex()
                                        .gap_2()
                                        .items_center()
                                        .child(dot(theme::maternal()))
                                        .child(t!("tree.maternal")),
                                ),
                        )
                    })
                    .child(fan),
            )
            .into_any_element()
    }
}
