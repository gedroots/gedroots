//! Fenêtre « Analyses » : contrôle de cohérence, lien de parenté, chronologie
//! avec âges et statistiques. Elle lit une copie du fichier ouvert dans la
//! fenêtre principale et se met à jour quand on y revient.

use crate::theme;
use crate::workspace::Workspace;
use gedcom::analysis::kinship::{self, Kinship};
use gedcom::analysis::stats::{self, Stats};
use gedcom::analysis::timeline::{self, EntryKind};
use gedcom::*;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::{Sizable, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use std::cell::RefCell;

thread_local! {
    /// Fenêtre déjà ouverte (une seule à la fois).
    static OPEN: RefCell<Option<AnyWindowHandle>> = const { RefCell::new(None) };
}

/// Ouvre la fenêtre Analyses, ou la ramène au premier plan.
/// `ged` et `person` sont fournis par l'appelant : la fenêtre principale est
/// en cours de mise à jour et ne peut pas être relue ici.
pub fn open(ws: Entity<Workspace>, main: AnyWindowHandle, ged: Gedcom, person: Option<String>, cx: &mut App) {
    if let Some(h) = OPEN.with(|o| *o.borrow())
        && h.update(cx, |_, window, _| window.activate_window()).is_ok()
    {
        return;
    }
    let bounds = Bounds::centered(None, size(px(980.), px(760.)), cx);
    let options = WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(bounds)),
        titlebar: Some(TitlebarOptions { title: Some("GedRoots — Analyses".into()), ..Default::default() }),
        ..Default::default()
    };
    if let Ok((handle, _)) = gpui_kit::open_window(options, cx, move |window, cx| {
        cx.new(|cx| Analyses::new(ws, main, ged, person, window, cx))
    }) {
        OPEN.with(|o| *o.borrow_mut() = Some(handle));
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Tab {
    Kinship,
    Timeline,
    Stats,
}

impl Tab {
    const ALL: [Tab; 3] = [Tab::Kinship, Tab::Timeline, Tab::Stats];
    fn label(self) -> &'static str {
        match self {
            Tab::Kinship => "Lien de parenté",
            Tab::Timeline => "Chronologie",
            Tab::Stats => "Statistiques",
        }
    }
}

pub struct Analyses {
    ws: WeakEntity<Workspace>,
    main: AnyWindowHandle,
    ged: Gedcom,
    tab: Tab,
    stats: Stats,
    /// Personne sélectionnée dans la fenêtre principale.
    person: Option<String>,
    /// Seconde personne pour le lien de parenté.
    other: Option<String>,
    search: Entity<InputState>,
    matches: Vec<(String, String, String)>,
    _subs: Vec<Subscription>,
}

impl Analyses {
    fn new(
        ws: Entity<Workspace>,
        main: AnyWindowHandle,
        ged: Gedcom,
        person: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let search = cx.new(|cx| InputState::new(window, cx).placeholder("Rechercher une personne…"));
        let subs = vec![
            cx.subscribe_in(&search, window, |this, _, ev: &InputEvent, _, cx| {
                if matches!(ev, InputEvent::Change) {
                    this.update_matches(cx);
                    cx.notify();
                }
            }),
            // La sélection de la fenêtre principale pilote la personne étudiée.
            cx.observe(&ws, |this, ws, cx| {
                let sel = ws.read(cx).selected_individual();
                if sel.is_some() && sel != this.person {
                    this.person = sel;
                    this.stats = stats::stats(&this.ged, this.person.as_deref());
                    cx.notify();
                }
            }),
            // Revenir sur la fenêtre recharge le fichier (il a pu être modifié).
            cx.observe_window_activation(window, |this, window, cx| {
                if window.is_window_active() {
                    this.reload(cx);
                }
            }),
        ];
        let mut this = Self {
            ws: ws.downgrade(),
            main,
            ged,
            tab: Tab::Kinship,
            stats: Stats::default(),
            person,
            other: None,
            search,
            matches: Vec::new(),
            _subs: subs,
        };
        this.stats = stats::stats(&this.ged, this.person.as_deref());
        this
    }

    fn reload(&mut self, cx: &mut Context<Self>) {
        let Some(ws) = self.ws.upgrade() else { return };
        let (ged, sel) = {
            let ws = ws.read(cx);
            (ws.ged_snapshot(), ws.selected_individual())
        };
        let Some(ged) = ged else { return };
        self.ged = ged;
        if sel.is_some() {
            self.person = sel;
        }
        if self.person.as_ref().is_some_and(|p| self.ged.individual(p).is_none()) {
            self.person = None;
        }
        if self.other.as_ref().is_some_and(|p| self.ged.individual(p).is_none()) {
            self.other = None;
        }
        self.stats = stats::stats(&self.ged, self.person.as_deref());
        self.update_matches(cx);
        cx.notify();
    }

    fn update_matches(&mut self, cx: &mut Context<Self>) {
        let q = self.search.read(cx).value().trim().to_lowercase();
        self.matches.clear();
        if q.is_empty() {
            return;
        }
        let words: Vec<&str> = q.split_whitespace().collect();
        for r in self.ged.records_of(RecordKind::Individual) {
            let i = Individual::new(r);
            let name = i.display_name();
            let hay = format!("{} {}", name.to_lowercase(), i.xref().to_lowercase());
            if words.iter().all(|w| hay.contains(w)) {
                self.matches.push((i.xref().to_string(), name, i.years()));
                if self.matches.len() >= 40 {
                    break;
                }
            }
        }
    }

    /// Ouvre un enregistrement dans la fenêtre principale.
    fn goto(&self, xref: String, cx: &mut App) {
        goto_in(self.ws.clone(), self.main, xref, cx);
    }
}

fn goto_in(ws: WeakEntity<Workspace>, main: AnyWindowHandle, xref: String, cx: &mut App) {
    let Some(ws) = ws.upgrade() else { return };
    let _ = main.update(cx, |_, window, cx| {
        ws.update(cx, |ws, cx| ws.goto(xref, window, cx));
        window.activate_window();
    });
}

impl Analyses {
    fn name(&self, x: &str) -> String {
        self.ged.record_label(x)
    }
}

fn label_caps(s: impl Into<SharedString>) -> Div {
    div().text_xs().font_weight(FontWeight::SEMIBOLD).text_color(theme::muted()).child(s.into())
}

fn mono(s: impl Into<SharedString>) -> Div {
    div().text_xs().font_family("monospace").text_color(theme::muted()).child(s.into())
}

fn card() -> Div {
    v_flex().p_4().gap_2().rounded_md().border_1().border_color(theme::border()).bg(theme::panel())
}

fn pct(x: f64) -> String {
    format!("{:.0} %", x * 100.)
}

impl Analyses {
    fn render_tabs(&self, cx: &mut Context<Self>) -> impl IntoElement {
        h_flex()
            .h(px(44.))
            .flex_none()
            .px_3()
            .gap_1()
            .items_center()
            .bg(theme::panel())
            .border_b_1()
            .border_color(theme::border())
            .children(Tab::ALL.into_iter().map(|t| {
                let active = t == self.tab;
                Button::new(SharedString::from(format!("tab-{}", t.label())))
                    .small()
                    .when(active, |b| b.primary())
                    .when(!active, |b| b.ghost())
                    .label(t.label())
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.tab = t;
                        cx.notify();
                    }))
            }))
            .child(div().flex_1())
            .when_some(self.person.clone(), |d, p| {
                d.child(div().text_color(theme::muted()).child(format!("Personne étudiée : {}", self.name(&p))))
            })
            .child(
                Button::new("reload")
                    .ghost()
                    .small()
                    .label("Actualiser")
                    .on_click(cx.listener(|this, _, _, cx| this.reload(cx))),
            )
    }

    fn person_link(&self, id: impl Into<ElementId>, x: &str, cx: &mut Context<Self>) -> Stateful<Div> {
        let target = x.to_string();
        div()
            .id(id.into())
            .cursor_pointer()
            .text_color(theme::accent())
            .hover(|d| d.underline())
            .child(self.name(x))
            .on_click(cx.listener(move |this, _, _, cx| this.goto(target.clone(), cx)))
    }

    fn render_kinship(&self, cx: &mut Context<Self>) -> AnyElement {
        let result: Option<Kinship> = match (&self.person, &self.other) {
            (Some(a), Some(b)) => kinship::relationship(&self.ged, a, b),
            _ => None,
        };
        let picker = v_flex()
            .w(px(320.))
            .flex_none()
            .p_4()
            .gap_2()
            .border_r_1()
            .border_color(theme::border())
            .child(label_caps("PERSONNE A"))
            .child(match &self.person {
                Some(p) => div().child(self.name(p)),
                None => div().text_color(theme::muted()).child("Sélectionnez une personne dans la fenêtre principale"),
            })
            .child(label_caps("PERSONNE B"))
            .child(Input::new(&self.search).small().cleanable(true))
            .child(v_flex().id("matches").flex_1().overflow_y_scroll().children(self.matches.iter().map(
                |(x, name, years)| {
                    let target = x.clone();
                    let on = self.other.as_deref() == Some(x.as_str());
                    h_flex()
                        .id(SharedString::from(format!("m-{x}")))
                        .px_2()
                        .py_1()
                        .gap_2()
                        .rounded_sm()
                        .cursor_pointer()
                        .when(on, |d| d.bg(theme::hover()))
                        .hover(|d| d.bg(theme::surface()))
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .overflow_hidden()
                                .whitespace_nowrap()
                                .text_ellipsis()
                                .child(name.clone()),
                        )
                        .child(mono(years.clone()).flex_none())
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.other = Some(target.clone());
                            cx.notify();
                        }))
                },
            )));
        let body = match (&self.person, &self.other, result) {
            (Some(a), Some(b), Some(k)) => {
                let path = k.path.clone();
                card()
                    .child(
                        h_flex()
                            .gap_1()
                            .flex_wrap()
                            .text_lg()
                            .child(self.person_link("k-b", b, cx))
                            .child("est")
                            .child(div().font_weight(FontWeight::SEMIBOLD).child(k.label.clone()))
                            .child("de")
                            .child(self.person_link("k-a", a, cx)),
                    )
                    .when_some(k.degree, |d, deg| d.child(mono(format!("Degré civil : {deg}"))))
                    .when(!k.common.is_empty(), |d| {
                        d.child(label_caps("ANCÊTRES COMMUNS LES PLUS PROCHES")).child(
                            h_flex().gap_3().flex_wrap().children(
                                k.common.iter().enumerate().map(|(n, c)| self.person_link(("k-c", n), c, cx)),
                            ),
                        )
                    })
                    .child(label_caps("CHEMIN"))
                    .child(v_flex().gap_1().children(path.iter().enumerate().map(|(n, x)| {
                        let years = self.ged.individual(x).map(|i| i.years()).unwrap_or_default();
                        h_flex()
                            .gap_2()
                            .child(mono(if n == 0 { "  ".into() } else { "→ ".to_string() }))
                            .child(self.person_link(("k-p", n), x, cx))
                            .child(mono(years))
                    })))
                    .into_any_element()
            }
            (Some(a), Some(b), None) => card()
                .child(format!("Aucun lien trouvé entre {} et {}.", self.name(a), self.name(b)))
                .child(
                    div().text_color(theme::muted()).child("Ni ancêtre commun, ni lien par mariage dans ce fichier."),
                )
                .into_any_element(),
            _ => div()
                .text_color(theme::muted())
                .child("Choisissez la personne B dans la liste pour calculer le lien.")
                .into_any_element(),
        };
        h_flex()
            .flex_1()
            .min_h_0()
            .items_stretch()
            .child(picker)
            .child(div().id("k-body").flex_1().p_4().overflow_y_scroll().child(body))
            .into_any_element()
    }

    fn render_timeline(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(p) = self.person.clone() else {
            return div()
                .p_4()
                .text_color(theme::muted())
                .child("Sélectionnez une personne dans la fenêtre principale.")
                .into_any_element();
        };
        let entries = timeline::timeline(&self.ged, &p);
        v_flex()
            .id("timeline")
            .flex_1()
            .p_4()
            .gap_1()
            .overflow_y_scroll()
            .child(div().text_lg().font_weight(FontWeight::SEMIBOLD).pb_2().child(self.name(&p)))
            .when(entries.is_empty(), |d| d.child(div().text_color(theme::muted()).child("Aucun événement daté")))
            .children(entries.into_iter().enumerate().map(|(n, e)| {
                let own = e.kind != EntryKind::Relative;
                let color = match e.kind {
                    EntryKind::Own => theme::accent(),
                    EntryKind::Family => theme::maternal(),
                    EntryKind::Relative => theme::border_strong(),
                };
                let related = e.related.clone();
                h_flex()
                    .id(("t", n))
                    .gap_3()
                    .py_1()
                    .px_2()
                    .rounded_sm()
                    .items_center()
                    .when(related.is_some(), |d| d.cursor_pointer().hover(|d| d.bg(theme::surface())))
                    .child(
                        div().w(px(150.)).flex_none().child(mono(
                            e.date.as_ref().map(|d| d.to_french()).unwrap_or_else(|| "date inconnue".into()),
                        )),
                    )
                    .child(
                        div()
                            .w(px(80.))
                            .flex_none()
                            .text_xs()
                            .text_color(theme::muted())
                            .child(e.age.unwrap_or_default()),
                    )
                    .child(div().size(px(8.)).flex_none().rounded_full().bg(color))
                    .child(
                        v_flex()
                            .flex_1()
                            .min_w_0()
                            .child(
                                div()
                                    .when(!own, |d| d.text_color(theme::muted()))
                                    .when(own, |d| d.font_weight(FontWeight::MEDIUM))
                                    .child(e.label),
                            )
                            .when_some(e.place.as_deref().map(short_place), |d, p| {
                                d.child(div().text_xs().text_color(theme::muted()).child(p))
                            }),
                    )
                    .when(own && !e.sourced, |d| {
                        d.child(div().text_xs().text_color(theme::warn()).child("sans source"))
                    })
                    .when_some(related, |d, x| d.on_click(cx.listener(move |this, _, _, cx| this.goto(x.clone(), cx))))
            }))
            .into_any_element()
    }

    fn bar_row(&self, label: String, value: f64, right: String) -> impl IntoElement {
        h_flex()
            .w_full()
            .gap_2()
            .items_center()
            .child(div().w(px(130.)).flex_none().overflow_hidden().whitespace_nowrap().text_ellipsis().child(label))
            .child(
                div()
                    .flex_1()
                    .min_w(px(60.))
                    .h(px(6.))
                    .rounded_full()
                    .bg(theme::border())
                    .child(div().h_full().rounded_full().bg(theme::accent()).w(relative(value.clamp(0., 1.) as f32))),
            )
            .child(div().w(px(130.)).flex_none().child(mono(right)))
    }

    fn render_stats(&self) -> AnyElement {
        let s = &self.stats;
        let top_list = |title: &'static str, items: &[(String, usize)]| {
            let max = items.first().map(|i| i.1).unwrap_or(1).max(1) as f64;
            card()
                .flex_1()
                .min_w(px(260.))
                .child(label_caps(title))
                .children(items.iter().map(|(k, n)| self.bar_row(k.clone(), *n as f64 / max, n.to_string())))
        };
        let figure = |label: &'static str, value: String| {
            v_flex()
                .gap_0p5()
                .child(div().text_xl().font_weight(FontWeight::SEMIBOLD).child(value))
                .child(div().text_xs().text_color(theme::muted()).child(label))
        };
        let years = |x: Option<f64>| x.map(|v| format!("{:.0} ans", v)).unwrap_or_else(|| "—".into());
        let sourced = if s.events == 0 { 0. } else { s.sourced_events as f64 / s.events as f64 };
        let overview = card().child(label_caps("VUE D'ENSEMBLE")).child(
            h_flex()
                .gap_8()
                .flex_wrap()
                .child(figure("individus", s.individuals.to_string()))
                .child(figure("familles", s.families.to_string()))
                .child(figure("hommes / femmes", format!("{} / {}", s.men, s.women)))
                .child(figure("événements sourcés", pct(sourced)))
                .child(figure("enfants par famille", s.mean_children.map(|x| format!("{x:.1}")).unwrap_or("—".into())))
                .child(figure("âge au 1er mariage (H)", years(s.mean_marriage_age_men)))
                .child(figure("âge au 1er mariage (F)", years(s.mean_marriage_age_women))),
        );
        let centuries =
            card()
                .child(label_caps("LONGÉVITÉ PAR SIÈCLE DE NAISSANCE"))
                .children(s.centuries.iter().map(|c| {
                    let label = format!("{}–{}", c.start, c.start + 99);
                    let right = format!("{} ({} n.)", years(c.mean_lifespan), c.births);
                    self.bar_row(label, c.mean_lifespan.unwrap_or(0.) / 100., right)
                }))
                .when(!s.longest.is_empty(), |d| {
                    d.child(label_caps("LES PLUS LONGUES VIES")).children(s.longest.iter().map(|(x, a)| {
                        h_flex().justify_between().child(self.name(x)).child(mono(format!("{a:.0} ans")))
                    }))
                });
        let ancestry = card()
            .child(label_caps(match &s.root {
                Some(r) => format!("ASCENDANCE DE {}", self.name(r).to_uppercase()),
                None => "ASCENDANCE (sélectionnez une personne)".into(),
            }))
            .children(s.generations.iter().map(|g| {
                let dup = g.found - g.distinct;
                let right = format!(
                    "{}/{}{}",
                    g.found,
                    g.expected,
                    if dup > 0 { format!(" ({dup} doubles)") } else { String::new() }
                );
                self.bar_row(format!("Génération {}", g.number), g.completeness(), right)
            }))
            .when(s.root.is_some(), |d| {
                d.child(mono(format!(
                    "Implexe : {} — part des ancêtres qui occupent plusieurs places dans l'arbre",
                    pct(s.implexe)
                )))
            });
        v_flex()
            .id("stats")
            .flex_1()
            .p_4()
            .gap_4()
            .overflow_y_scroll()
            .child(overview)
            .child(
                h_flex()
                    .gap_4()
                    .items_start()
                    .flex_wrap()
                    .child(top_list("PATRONYMES", &s.surnames[..s.surnames.len().min(12)]))
                    .child(top_list("PRÉNOMS MASCULINS", &s.given_men))
                    .child(top_list("PRÉNOMS FÉMININS", &s.given_women)),
            )
            .child(h_flex().gap_4().items_start().child(ancestry.flex_1()).child(centuries.flex_1()))
            .child(top_list("LIEUX LES PLUS CITÉS", &s.places))
            .into_any_element()
    }
}

impl Render for Analyses {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let body = match self.tab {
            Tab::Kinship => self.render_kinship(cx),
            Tab::Timeline => self.render_timeline(cx),
            Tab::Stats => self.render_stats(),
        };
        v_flex().size_full().bg(theme::bg()).text_color(theme::text()).text_sm().child(self.render_tabs(cx)).child(body)
    }
}
