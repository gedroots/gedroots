//! Arbre libre : on s'y déplace en glissant ou à la molette, dans toutes les
//! directions. Les générations d'ascendants et de descendants se chargent au
//! fur et à mesure qu'on s'en approche ; une mini-carte montre où l'on est.

pub(super) mod layout;

use super::*;
use layout::{CARD_H, CARD_W, COL, Card, Layout, Slot};
use std::cell::Cell;

/// Générations chargées en arrivant sur une personne.
const START_UP: u32 = 2;
const START_DOWN: u32 = 1;
/// Au-delà, on arrête de charger (fichier incohérent).
const MAX_GENS: u32 = 200;
/// Taille de la vue avant le premier affichage.
const DEFAULT_VIEW: (f32, f32) = (1000., 700.);
const MINI_W: f32 = 200.;
const MINI_H: f32 = 130.;
/// Déplacement au-delà duquel un appui devient un glissement (et non un clic).
const DRAG_SLOP: f32 = 4.;

/// État de l'arbre libre.
pub(super) struct Chart {
    /// Personne autour de laquelle l'arbre est construit.
    anchor: Option<String>,
    /// Dernière sélection prise en compte.
    seen: Option<String>,
    up: u32,
    down: u32,
    /// Point de l'arbre affiché au centre de la vue, au signe près :
    /// position à l'écran = position dans l'arbre + `pan` + demi-vue.
    pan: (f32, f32),
    /// Glissement en cours : position de départ du pointeur et `pan` d'alors.
    drag: Option<(Point<Pixels>, (f32, f32))>,
    /// Le dernier appui a déplacé l'arbre : le clic qui suit est ignoré.
    dragged: bool,
    /// La mini-carte est en cours de manipulation.
    mini_drag: bool,
    view: Rc<Cell<Option<Bounds<Pixels>>>>,
    mini: Rc<Cell<Option<Bounds<Pixels>>>>,
    /// Passage de la mini-carte à l'arbre : origine de l'arbre, échelle, marges.
    mini_map: Cell<[f32; 4]>,
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
        }
    }
}

impl Chart {
    fn view_size(&self) -> (f32, f32) {
        self.view.get().map(|b| (f32::from(b.size.width), f32::from(b.size.height))).unwrap_or(DEFAULT_VIEW)
    }

    /// Recentre l'arbre sur `xref`, avec les générations de départ.
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

    /// La case est-elle entièrement dans la vue ?
    fn visible(&self, c: &Card) -> bool {
        let (w, h) = self.view_size();
        let (sx, sy) = (c.x + self.pan.0 + w / 2., c.y + self.pan.1 + h / 2.);
        sx >= 0. && sy >= 0. && sx + CARD_W <= w && sy + CARD_H <= h
    }

    /// Construit l'arbre, en chargeant les générations dont la vue s'approche.
    /// Quand une génération s'ajoute, les cases peuvent se décaler : la
    /// personne la plus proche du centre de la vue reste à sa place à l'écran.
    fn layout(&mut self, g: &Gedcom, anchor: &str) -> Layout {
        let mut l = layout::build(g, anchor, self.up, self.down);
        let (w, _) = self.view_size();
        for _ in 0..MAX_GENS {
            // Dès que la dernière colonne chargée entre dans la vue, on charge la suivante.
            let left = -self.pan.0 - w / 2.;
            let right = -self.pan.0 + w / 2.;
            let need_up = l.more_up && right > self.up as f32 * COL && self.up < MAX_GENS;
            let need_down = l.more_down && left < -(self.down as f32) * COL + CARD_W && self.down < MAX_GENS;
            if !need_up && !need_down {
                break;
            }
            let focus = l.nearest(-self.pan.0, -self.pan.1).map(|c| (c.xref().unwrap_or("").to_string(), c.y));
            self.up += need_up as u32;
            self.down += need_down as u32;
            l = layout::build(g, anchor, self.up, self.down);
            if let Some((x, old)) = focus
                && let Some(c) = l.find(&x)
            {
                self.pan.1 += old - c.y;
            }
        }
        // Le centre de la vue reste sur l'arbre : on ne s'en éloigne pas dans le vide.
        let [x0, y0, x1, y1] = l.bounds;
        if x0 <= x1 && y0 <= y1 {
            self.pan = (self.pan.0.clamp(-x1, -x0), self.pan.1.clamp(-y1, -y0));
        }
        l
    }

    fn pan_by(&mut self, dx: f32, dy: f32) {
        self.pan = (self.pan.0 + dx, self.pan.1 + dy);
    }

    /// Point de la mini-carte → centre de la vue.
    fn mini_jump(&mut self, pos: Point<Pixels>) {
        let Some(b) = self.mini.get() else { return };
        let [x0, y0, s, _] = self.mini_map.get();
        let x = (f32::from(pos.x - b.origin.x)) / s + x0;
        let y = (f32::from(pos.y - b.origin.y)) / s + y0;
        self.pan = (-x, -y);
    }
}

impl Workspace {
    /// Clic sur une case : sélection ; double clic : l'arbre se recentre dessus.
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

    /// Construit l'arbre autour de la personne et l'affiche.
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
        let mut l = st.layout(g, &anchor);
        // Nouvelle sélection (liste, Précédent…) : si la personne est déjà dans
        // l'arbre, on l'amène dans la vue ; sinon l'arbre se recentre sur elle.
        if st.seen.as_ref() != Some(sel) {
            st.seen = Some(sel.clone());
            match l.find(sel).cloned() {
                Some(c) if st.visible(&c) => {}
                Some(c) => st.center_on(&c),
                None => st.reanchor(sel),
            }
            let anchor = st.anchor.clone().unwrap_or_default();
            l = st.layout(g, &anchor);
        }

        let (vw, vh) = st.view_size();
        let pan = st.pan;
        let to_screen = move |x: f32, y: f32| (x + pan.0 + vw / 2., y + pan.1 + vh / 2.);
        let in_view = |c: &Card| {
            let (sx, sy) = to_screen(c.x, c.y);
            sx + CARD_W > -40. && sy + CARD_H > -40. && sx < vw + 40. && sy < vh + 40.
        };
        let root = g.root();
        let cards: Vec<AnyElement> = l
            .cards
            .iter()
            .filter(|c| in_view(c))
            .filter_map(|c| self.chart_card(doc, c, to_screen(c.x, c.y), sel, root.as_deref(), cx))
            .collect();

        // Liaisons, dessinées sous les cases.
        let lines: Vec<[f32; 4]> = l
            .lines
            .iter()
            .filter(|s| {
                let (x0, y0) = to_screen(s[0].min(s[2]), s[1].min(s[3]));
                let (x1, y1) = to_screen(s[0].max(s[2]), s[1].max(s[3]));
                x1 >= 0. && y1 >= 0. && x0 <= vw && y0 <= vh
            })
            .copied()
            .collect();
        let view_cell = st.view.clone();
        let lines_el = canvas(
            move |bounds, window, _| {
                if view_cell.get().map(|b| b.size) != Some(bounds.size) {
                    // Première mesure, ou fenêtre redimensionnée : on recompose.
                    window.refresh();
                }
                view_cell.set(Some(bounds));
            },
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
                    st.drag = Some((ev.position, st.pan));
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
                } else if let Some((start, from)) = st.drag {
                    let (dx, dy) = (f32::from(ev.position.x - start.x), f32::from(ev.position.y - start.y));
                    if dx.abs() + dy.abs() > DRAG_SLOP {
                        st.dragged = true;
                    }
                    if st.dragged {
                        st.pan = (from.0 + dx, from.1 + dy);
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
                // Molette classique avec Maj : défilement horizontal.
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
                // Couleur de lignée pour les ancêtres seulement (comme l'éventail) ;
                // descendants, conjoints et fratrie restent neutres.
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
                        .rounded_md()
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
                let label = if rel == Relation::Father { "Père" } else { "Mère" };
                place(add_slot(format!("chart-add-{child}-{label}"), label))
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
                    Slot::AddChild(_) => (Relation::Child, "Enfant"),
                    _ => (Relation::Sibling, "Frère ou sœur"),
                };
                let x = x.clone();
                let menu = self.slot_menu(&x, rel, cx);
                place(add_slot(format!("chart-add-{label}"), label))
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

    /// Mini-carte, dans le coin inférieur droit, quand l'arbre déborde de la vue.
    fn chart_minimap(&self, st: &Chart, l: &Layout, sel: &str, cx: &mut Context<Self>) -> Option<AnyElement> {
        let (vw, vh) = st.view_size();
        let [bx0, by0, bx1, by1] = l.bounds;
        // Ce qui est visible compte aussi : on voit où l'on est même hors de l'arbre.
        let (v0, v1) = ((-st.pan.0 - vw / 2., -st.pan.1 - vh / 2.), (-st.pan.0 + vw / 2., -st.pan.1 + vh / 2.));
        let fits = bx0 >= v0.0 && by0 >= v0.1 && bx1 <= v1.0 && by1 <= v1.1;
        if fits || st.view.get().is_none() || vw < 2. * MINI_W || vh < 2. * MINI_H {
            return None;
        }
        let (x0, y0, x1, y1) = (bx0.min(v0.0), by0.min(v0.1), bx1.max(v1.0), by1.max(v1.1));
        let pad = 6.;
        let s = ((MINI_W - 2. * pad) / (x1 - x0)).min((MINI_H - 2. * pad) / (y1 - y0));
        // Marges pour centrer le contenu dans la mini-carte.
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
                .rounded_md()
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
