//! “New Tree” window (File › New Tree…): the root person, their parents and their grandparents,
//! laid out as a pedigree and entered in one go. Empty slots create nobody.

use super::*;
use gedcom::date::parse_input;
use gedcom::starter::{Slot, Starter, StarterPerson};

/// Width of an ancestor's box, of the root's box, and the space between two generations.
const BOX_W: f32 = 280.;
const ROOT_W: f32 = 300.;
const COL_GAP: f32 = 40.;
/// Heights of an ancestor's box (two rows of fields) and of the root's (four rows), and the gap
/// between two grandparents.
const BOX_H: f32 = 112.;
const ROOT_H: f32 = 184.;
const ROW_GAP: f32 = 12.;

/// Fields of a person of the window.
struct PersonFields {
    given: Entity<InputState>,
    surname: Entity<InputState>,
    birth: Entity<InputState>,
    /// Ancestors only: the root is usually alive.
    death: Option<Entity<InputState>>,
    /// Root only.
    place: Option<Entity<InputState>>,
}

pub(super) struct NewTreeDialog {
    people: Vec<PersonFields>,
    /// Root's sex (`F`, `M`, `U`).
    sex: &'static str,
    /// The root is someone else than the user (only changes the box's title).
    someone_else: bool,
    error: Option<SharedString>,
    _subs: Vec<Subscription>,
}

impl NewTreeDialog {
    fn entered(&self, cx: &App) -> Result<Starter, String> {
        let text = |e: &Entity<InputState>| e.read(cx).value().trim().to_string();
        let date = |e: &Entity<InputState>| -> Result<(String, String), String> {
            let t = text(e);
            let p = parse_input(&t).map_err(|err| t!("date_field.invalid", text = t, error = err).to_string())?;
            Ok((p.value(), p.phrase))
        };
        let mut s = Starter::default();
        for (slot, f) in Slot::ALL.iter().zip(&self.people) {
            s.people[*slot as usize] = StarterPerson {
                given: text(&f.given),
                surname: text(&f.surname),
                sex: if *slot == Slot::Root { self.sex.to_string() } else { String::new() },
                birth: date(&f.birth)?,
                birth_place: f.place.as_ref().map(text).unwrap_or_default(),
                death: match &f.death {
                    Some(d) => date(d)?,
                    None => Default::default(),
                },
            };
        }
        Ok(s)
    }
}

fn slot_title(slot: Slot, someone_else: bool) -> SharedString {
    match slot {
        Slot::Root if someone_else => t!("new_tree.someone_else"),
        Slot::Root => t!("new_tree.you"),
        Slot::Father => t!("new_tree.father"),
        Slot::Mother => t!("new_tree.mother"),
        Slot::PaternalGrandfather => t!("new_tree.paternal_grandfather"),
        Slot::PaternalGrandmother => t!("new_tree.paternal_grandmother"),
        Slot::MaternalGrandfather => t!("new_tree.maternal_grandfather"),
        Slot::MaternalGrandmother => t!("new_tree.maternal_grandmother"),
    }
    .into()
}

/// Top-left corner of each slot's box in the pedigree.
fn slot_origin(slot: Slot) -> (f32, f32) {
    let gp = |i: f32| i * (BOX_H + ROW_GAP);
    let center_of = |a: f32, b: f32| (gp(a) + gp(b)) / 2. + BOX_H / 2.;
    let parents_x = ROOT_W + COL_GAP;
    let gp_x = parents_x + BOX_W + COL_GAP;
    match slot {
        Slot::Root => (0., (center_of(0., 1.) + center_of(2., 3.)) / 2. - ROOT_H / 2.),
        Slot::Father => (parents_x, center_of(0., 1.) - BOX_H / 2.),
        Slot::Mother => (parents_x, center_of(2., 3.) - BOX_H / 2.),
        Slot::PaternalGrandfather => (gp_x, gp(0.)),
        Slot::PaternalGrandmother => (gp_x, gp(1.)),
        Slot::MaternalGrandfather => (gp_x, gp(2.)),
        Slot::MaternalGrandmother => (gp_x, gp(3.)),
    }
}

fn slot_size(slot: Slot) -> (f32, f32) {
    if slot == Slot::Root { (ROOT_W, ROOT_H) } else { (BOX_W, BOX_H) }
}

/// Elbow line from the right edge of `child` to the left edge of `parent`.
fn link(child: Slot, parent: Slot) -> Vec<Div> {
    let ((cx0, cy0), (cw, ch)) = (slot_origin(child), slot_size(child));
    let ((px0, py0), (_, ph)) = (slot_origin(parent), slot_size(parent));
    let (x1, y1, x2, y2) = (cx0 + cw, cy0 + ch / 2., px0, py0 + ph / 2.);
    let mid = (x1 + x2) / 2.;
    let line = |l: f32, t: f32, w: f32, h: f32| {
        div().absolute().left(px(l)).top(px(t)).w(px(w)).h(px(h)).bg(theme::border_strong())
    };
    vec![line(x1, y1, mid - x1, 1.), line(mid, y1.min(y2), 1., (y2 - y1).abs() + 1.), line(mid, y2, x2 - mid, 1.)]
}

impl Workspace {
    /// Opens the “New Tree” window.
    pub(super) fn open_new_tree(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let input = |ph: SharedString, window: &mut Window, cx: &mut Context<Self>| {
            cx.new(|cx| InputState::new(window, cx).placeholder(ph))
        };
        let mut people = Vec::new();
        let mut subs = Vec::new();
        for slot in Slot::ALL {
            let root = slot == Slot::Root;
            let f = PersonFields {
                given: input(t!("new_tree.given").into(), window, cx),
                surname: input(t!("new_tree.surname").into(), window, cx),
                birth: input(t!("new_tree.born").into(), window, cx),
                death: (!root).then(|| input(t!("new_tree.died").into(), window, cx)),
                place: root.then(|| input(t!("new_tree.birth_place").into(), window, cx)),
            };
            for e in [&f.given, &f.surname, &f.birth].into_iter().chain(f.death.iter()) {
                subs.push(cx.subscribe_in(e, window, |this, _, ev: &InputEvent, window, cx| match ev {
                    InputEvent::Change => this.refresh_new_tree(window, cx),
                    InputEvent::PressEnter { .. } => this.create_tree(window, cx),
                    _ => {}
                }));
            }
            if let Some(p) = &f.place {
                subs.push(cx.subscribe_in(p, window, |this, _, ev: &InputEvent, window, cx| {
                    if matches!(ev, InputEvent::PressEnter { .. }) {
                        this.create_tree(window, cx);
                    }
                }));
            }
            people.push(f);
        }
        let first = people[0].given.clone();
        self.new_tree =
            Some(Box::new(NewTreeDialog { people, sex: "U", someone_else: false, error: None, _subs: subs }));
        first.update(cx, |s, cx| s.focus(window, cx));
        cx.notify();
    }

    /// Typing: the empty surnames show the one they will inherit.
    fn refresh_new_tree(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(d) = &mut self.new_tree else { return };
        d.error = None;
        let Ok(s) = d.entered(cx) else {
            cx.notify();
            return;
        };
        for slot in [Slot::Father, Slot::PaternalGrandfather, Slot::MaternalGrandfather] {
            let ph = match s.suggested_surname(slot) {
                x if x.is_empty() => t!("new_tree.surname").into_owned(),
                x => x,
            };
            d.people[slot as usize].surname.update(cx, |st, cx| st.set_placeholder(ph, window, cx));
        }
        cx.notify();
    }

    pub(super) fn close_new_tree(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.new_tree = None;
        window.focus(&self.focus, cx);
        cx.notify();
    }

    /// Creates the people entered in a new, untitled file and shows the root in the tree.
    fn create_tree(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(d) = &mut self.new_tree else { return };
        let starter = match d.entered(cx) {
            Ok(s) => s,
            Err(e) => {
                d.error = Some(e.into());
                cx.notify();
                return;
            }
        };
        let mut ged = Gedcom::new();
        let root = match ged.seed(&starter) {
            Ok(x) => x,
            Err(e) => {
                d.error = Some(e.into());
                cx.notify();
                return;
            }
        };
        self.new_tree = None;
        self.set_document(ged, None, false, window, cx);
        self.select(root, window, cx);
        self.tab = CenterTab::Tree;
        window.focus(&self.focus, cx);
        cx.notify();
    }

    fn person_box(&self, d: &NewTreeDialog, slot: Slot, cx: &mut Context<Self>) -> AnyElement {
        let f = &d.people[slot as usize];
        let root = slot == Slot::Root;
        let read = |e: &Entity<InputState>| !e.read(cx).value().trim().is_empty();
        let empty = !root && ![&f.given, &f.surname, &f.birth].into_iter().chain(f.death.iter()).any(read);
        let (x, y) = slot_origin(slot);
        let (w, h) = slot_size(slot);
        let lineage = match slot {
            Slot::Father | Slot::PaternalGrandfather | Slot::PaternalGrandmother => Some(theme::paternal()),
            Slot::Mother | Slot::MaternalGrandfather | Slot::MaternalGrandmother => Some(theme::maternal()),
            Slot::Root => None,
        };
        let pair = |a: AnyElement, b: AnyElement| {
            h_flex().gap_2().child(div().flex_1().child(a)).child(div().flex_1().child(b))
        };
        let field = |e: &Entity<InputState>| Input::new(e).small().into_any_element();
        let title: AnyElement = if root {
            let ws = cx.entity().downgrade();
            let someone_else = d.someone_else;
            h_flex()
                .child(
                    Button::new("new-tree-who")
                        .xsmall()
                        .ghost()
                        .icon(IconName::House)
                        .label(slot_title(slot, someone_else))
                        .dropdown_caret(true)
                        .dropdown_menu(move |menu, _, _| {
                            let mut menu = menu;
                            for (other, label) in [(false, t!("new_tree.you")), (true, t!("new_tree.someone_else"))] {
                                let ws = ws.clone();
                                menu = menu.item(PopupMenuItem::new(label).checked(other == someone_else).on_click(
                                    move |_, _, cx| {
                                        if let Some(ws) = ws.upgrade() {
                                            ws.update(cx, |ws, cx| {
                                                if let Some(d) = &mut ws.new_tree {
                                                    d.someone_else = other;
                                                }
                                                cx.notify();
                                            });
                                        }
                                    },
                                ));
                            }
                            menu
                        }),
                )
                .into_any_element()
        } else {
            h_flex()
                .gap_2()
                .items_center()
                .text_xs()
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(theme::muted())
                .when_some(lineage, |d, c| d.child(div().size(px(8.)).rounded_full().bg(c)))
                .child(slot_title(slot, false))
                .into_any_element()
        };
        let sex = root.then(|| {
            h_flex().gap_1().children([("F", t!("sex.female")), ("M", t!("sex.male")), ("U", t!("sex.unknown"))].map(
                |(v, label)| {
                    let on = d.sex == v;
                    div()
                        .id(SharedString::from(format!("new-tree-sex-{v}")))
                        .flex_1()
                        .py_1()
                        .text_center()
                        .rounded(theme::RADIUS_SM)
                        .border_1()
                        .cursor_pointer()
                        .border_color(if on { theme::accent() } else { theme::border_strong() })
                        .bg(if on { theme::accent_soft() } else { theme::bg() })
                        .text_color(if on { theme::text() } else { theme::muted() })
                        .hover(|d| d.border_color(theme::accent()))
                        .child(label)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            if let Some(d) = &mut this.new_tree {
                                d.sex = v;
                            }
                            cx.notify();
                        }))
                },
            ))
        });
        v_flex()
            .absolute()
            .left(px(x))
            .top(px(y))
            .w(px(w))
            .h(px(h))
            .p_3()
            .gap_2()
            .rounded(theme::RADIUS_SM)
            .border_1()
            .when(root, |d| d.border_color(theme::accent()).bg(theme::accent_soft()))
            .when(!root, |d| d.border_color(theme::border_strong()).bg(theme::bg()))
            .when(empty, |d| d.border_dashed())
            .child(title)
            .child(pair(field(&f.given), field(&f.surname)))
            .children(sex)
            .when(root, |d| d.child(field(&f.birth)).children(f.place.as_ref().map(field)))
            .when(!root, |d| {
                d.child(pair(field(&f.birth), f.death.as_ref().map(field).unwrap_or_else(|| div().into_any_element())))
            })
            .into_any_element()
    }

    pub(super) fn render_new_tree(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let d = self.new_tree.as_ref()?;
        let (gx, _) = slot_origin(Slot::PaternalGrandfather);
        let width = gx + BOX_W;
        let height = 4. * BOX_H + 3. * ROW_GAP;
        let links = [
            (Slot::Root, Slot::Father),
            (Slot::Root, Slot::Mother),
            (Slot::Father, Slot::PaternalGrandfather),
            (Slot::Father, Slot::PaternalGrandmother),
            (Slot::Mother, Slot::MaternalGrandfather),
            (Slot::Mother, Slot::MaternalGrandmother),
        ];
        let cancel = Button::new("new-tree-cancel")
            .label(t!("new_tree.cancel"))
            .on_click(cx.listener(|this, _, w, cx| this.close_new_tree(w, cx)));
        let create = Button::new("new-tree-create")
            .primary()
            .label(t!("new_tree.create"))
            .on_click(cx.listener(|this, _, w, cx| this.create_tree(w, cx)));
        // Platform order: the action on the left on Windows, on the right elsewhere.
        let buttons = if cfg!(target_os = "windows") {
            [create.into_any_element(), cancel.into_any_element()]
        } else {
            [cancel.into_any_element(), create.into_any_element()]
        };
        Some(
            div().absolute().inset_0().flex().justify_center().items_center().bg(theme::scrim()).child(
                v_flex()
                    .id("new-tree")
                    .occlude()
                    .rounded(theme::RADIUS_LG)
                    .border_1()
                    .border_color(theme::border_strong())
                    .bg(theme::panel())
                    .shadow_lg()
                    .on_key_down(cx.listener(|this, ev: &KeyDownEvent, w, cx| {
                        if ev.keystroke.key == "escape" {
                            this.close_new_tree(w, cx);
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
                            .child(t!("new_tree.title")),
                    )
                    .child(
                        div().p_6().child(
                            div()
                                .relative()
                                .w(px(width))
                                .h(px(height))
                                .children(links.into_iter().flat_map(|(c, p)| link(c, p)))
                                .children(Slot::ALL.map(|s| self.person_box(d, s, cx))),
                        ),
                    )
                    .child(
                        h_flex()
                            .px_6()
                            .py_4()
                            .gap_2()
                            .items_center()
                            .border_t_1()
                            .border_color(theme::border())
                            .child(div().flex_1().min_w_0().text_color(theme::danger()).children(d.error.clone()))
                            .children(buttons),
                    ),
            ),
        )
        .map(|e| e.into_any_element())
    }
}
