//! Vues en arbre : ascendance, descendance et éventail.

use super::*;
use std::cell::Cell;
use std::f32::consts::PI;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TreeMode {
    Ancestors,
    Descendants,
    Fan,
}

impl TreeMode {
    pub(super) fn label(self) -> &'static str {
        match self {
            TreeMode::Ancestors => "Ascendance",
            TreeMode::Descendants => "Descendance",
            TreeMode::Fan => "Éventail",
        }
    }

    pub(super) fn generations(self) -> std::ops::RangeInclusive<u32> {
        match self {
            TreeMode::Ancestors => 4..=4,
            TreeMode::Descendants => 2..=8,
            TreeMode::Fan => 2..=7,
        }
    }
}

/// Angle d'ouverture de l'éventail (degrés), de part et d'autre de la verticale.
const FAN_SPAN: f32 = 240.;
const FAN_LEFT: f32 = 90. + FAN_SPAN / 2.;

/// Rayons extérieurs de chaque génération (1 = disque central).
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

/// Point à l'angle `deg` (sens trigonométrique, 0° = à droite) et au rayon `r`.
fn polar(c: Point<Pixels>, r: f32, deg: f32) -> Point<Pixels> {
    let a = deg * PI / 180.;
    point(c.x + px(r * a.cos()), c.y - px(r * a.sin()))
}

/// Secteur du Sosa `k` : (rayon intérieur, extérieur, angle de début, de fin).
fn sector(k: usize, radii: &[f32]) -> (f32, f32, f32, f32) {
    let g = (usize::BITS - k.leading_zeros()) as usize; // génération (1 = racine)
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
    /// Onglet « Arbre » : choix de la vue et du nombre de générations.
    pub(super) fn render_tree_tab(&self, cx: &mut Context<Self>) -> AnyElement {
        let mode = self.tree_mode;
        let gens = self.tree_gens.clamp(*mode.generations().start(), *mode.generations().end());
        // Vue et générations : menu Affichage › Arbre (⌘= / ⌘-).
        let body: AnyElement = match mode {
            TreeMode::Ancestors => self.render_tree(cx).into_any_element(),
            TreeMode::Descendants => self.render_descendants(gens, cx),
            TreeMode::Fan => self.render_fan(gens, cx),
        };
        v_flex().flex_1().min_h_0().child(body).into_any_element()
    }

    // ------------------------------------------------------------------
    // Descendance
    // ------------------------------------------------------------------

    fn render_descendants(&self, gens: u32, cx: &mut Context<Self>) -> AnyElement {
        let (Some(doc), Some(root)) = (&self.doc, &self.selected) else { return div().into_any_element() };
        let g = &doc.ged;
        let Some(tree) = g.descendants(root, gens) else { return div().into_any_element() };
        let total = g.descendant_count(root);
        let shown = tree.count() - 1;
        div()
            .id("desc-scroll")
            .flex_1()
            .overflow_scroll()
            .p_4()
            .child(
                v_flex()
                    .gap_3()
                    .child(mono(if shown < total {
                        format!("{shown} descendants affichés sur {total} — augmentez le nombre de générations")
                    } else {
                        format!("{total} descendant{} connu{}", plural(total), plural(total))
                    }))
                    .child(self.desc_node(g, &tree, 0, cx)),
            )
            .into_any_element()
    }

    fn person_box(&self, g: &Gedcom, xref: &str, root: bool, cx: &mut Context<Self>) -> Stateful<Div> {
        let i = g.individual(xref);
        let (name, years, sex) = match i {
            Some(i) => (i.display_name(), i.years(), i.sex().to_string()),
            None => ("?".into(), String::new(), "U".into()),
        };
        let color = match sex.as_str() {
            "M" => theme::paternal(),
            "F" => theme::maternal(),
            _ => theme::border_strong(),
        };
        let target = xref.to_string();
        v_flex()
            .id(SharedString::from(format!("box-{xref}")))
            .w(px(190.))
            .flex_none()
            .px_2()
            .py_1()
            .rounded_md()
            .border_1()
            .border_l_4()
            .border_color(if root { theme::accent() } else { theme::border() })
            .bg(if root { theme::accent_soft() } else { theme::surface() })
            .cursor_pointer()
            .hover(|d| d.bg(theme::hover()))
            .child(
                h_flex()
                    .gap_1p5()
                    .items_center()
                    .child(dot(color))
                    .child(div().flex_1().overflow_hidden().whitespace_nowrap().text_ellipsis().child(name)),
            )
            .child(mono(years))
            .on_click(cx.listener(move |this, _, w, cx| this.select(target.clone(), w, cx)))
    }

    fn desc_node(&self, g: &Gedcom, node: &DescNode, depth: usize, cx: &mut Context<Self>) -> AnyElement {
        let me = self.person_box(g, &node.xref, depth == 0, cx).when(node.truncated, |d| {
            d.child(div().text_xs().text_color(theme::accent()).child("▸ descendants masqués"))
        });
        h_flex()
            .items_start()
            .gap_3()
            .text_sm()
            .child(me)
            .when(!node.unions.is_empty(), |d| {
                d.child(v_flex().gap_2().children(node.unions.iter().map(|u| {
                    v_flex()
                        .gap_1()
                        .child(
                            h_flex().gap_2().items_center().child(div().text_color(theme::muted()).child("∞")).child(
                                match &u.spouse {
                                    Some(s) => self.person_box(g, s, false, cx).into_any_element(),
                                    None => div()
                                        .text_xs()
                                        .text_color(theme::muted())
                                        .child("conjoint inconnu")
                                        .into_any_element(),
                                },
                            ),
                        )
                        .when(!u.children.is_empty(), |d| {
                            d.child(
                                v_flex()
                                    .ml(px(8.))
                                    .pl_3()
                                    .gap_1p5()
                                    .border_l_1()
                                    .border_color(theme::border_strong())
                                    .children(u.children.iter().map(|c| self.desc_node(g, c, depth + 1, cx))),
                            )
                        })
                })))
            })
            .into_any_element()
    }

    // ------------------------------------------------------------------
    // Éventail
    // ------------------------------------------------------------------

    fn render_fan(&self, gens: u32, cx: &mut Context<Self>) -> AnyElement {
        let (Some(doc), Some(root)) = (&self.doc, &self.selected) else { return div().into_any_element() };
        let g = &doc.ged;
        let anc = g.ancestry(root, gens);
        let radii = fan_radii(gens);
        let rmax = *radii.last().unwrap();
        let (w, h) = (2. * rmax + 24., rmax * (1. + (FAN_SPAN / 2. - 90.).to_radians().sin()) + 24.);
        let center_rel = point(px(w / 2.), px(rmax + 12.));

        // Couleurs de chaque secteur, calculées avant le dessin.
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

        // Libellés, posés au centre de chaque secteur.
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

        let anc_click = anc.clone();
        let hit_radii = radii.clone();
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
                    let Some(b) = bounds_cell.get() else { return };
                    let c = b.origin + center_rel;
                    let (dx, dy) = (f32::from(ev.position.x - c.x), f32::from(c.y - ev.position.y));
                    let r = (dx * dx + dy * dy).sqrt();
                    let Some(gn) = hit_radii.iter().position(|outer| r <= *outer).map(|i| i + 1) else { return };
                    let k = if gn == 1 {
                        1
                    } else {
                        let mut a = dy.atan2(dx).to_degrees();
                        if a < FAN_LEFT - FAN_SPAN - 1e-3 {
                            a += 360.;
                        }
                        if !(FAN_LEFT - FAN_SPAN..=FAN_LEFT).contains(&a) {
                            return;
                        }
                        let n = 1usize << (gn - 1);
                        let i = (((FAN_LEFT - a) / (FAN_SPAN / n as f32)) as usize).min(n - 1);
                        n + i
                    };
                    if k > 1
                        && let Some(Some(x)) = anc_click.get(k)
                    {
                        this.select(x.clone(), window, cx);
                    }
                }),
            );
        div()
            .id("fan-scroll")
            .flex_1()
            .overflow_scroll()
            .p_4()
            .child(
                v_flex()
                    .gap_2()
                    .items_center()
                    .child(
                        h_flex()
                            .gap_4()
                            .text_xs()
                            .text_color(theme::muted())
                            .child(
                                h_flex()
                                    .gap_1p5()
                                    .items_center()
                                    .child(dot(theme::paternal()))
                                    .child("Lignée paternelle"),
                            )
                            .child(
                                h_flex()
                                    .gap_1p5()
                                    .items_center()
                                    .child(dot(theme::maternal()))
                                    .child("Lignée maternelle"),
                            ),
                    )
                    .child(fan),
            )
            .into_any_element()
    }
}

fn plural(n: usize) -> &'static str {
    if n > 1 { "s" } else { "" }
}
