//! Page « Statistiques » et lien de parenté entre deux individus
//! (anciennement la fenêtre Analyses).

use super::*;
use gedcom::analysis::kinship;
use gedcom::analysis::stats::{self, Stats};

fn card() -> Div {
    v_flex().p_4().gap_2().rounded(theme::RADIUS_SM).border_1().border_color(theme::border()).bg(theme::panel())
}

fn pct(x: f64) -> String {
    format!("{:.0} %", x * 100.)
}

fn bar_row(label: String, value: f64, right: String) -> impl IntoElement {
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

impl Workspace {
    /// Lien cliquable vers un individu.
    fn person_link(&self, id: impl Into<ElementId>, x: &str, cx: &mut Context<Self>) -> Stateful<Div> {
        let name = self.doc.as_ref().map(|d| d.ged.record_label(x)).unwrap_or_default();
        let target = x.to_string();
        div()
            .id(id.into())
            .cursor_pointer()
            .text_color(theme::accent())
            .hover(|d| d.underline())
            .child(name)
            .on_click(cx.listener(move |this, _, w, cx| this.goto(target.clone(), w, cx)))
    }

    // ------------------------------------------------------------------
    // Lien de parenté
    // ------------------------------------------------------------------

    /// Individu › Lien de parenté avec… : choisir l'autre personne.
    pub(super) fn ask_kinship(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(indi) = self.selected_individual() {
            self.open_picker(relatives::PickPurpose::Kinship { indi }, window, cx);
        }
    }

    /// Carte du lien de parenté, dans le panneau de l'individu `a`.
    pub(super) fn render_kinship(&self, a: &str, cx: &mut Context<Self>) -> Option<AnyElement> {
        let (ka, b) = self.kinship.as_ref()?;
        if ka != a {
            return None;
        }
        let g = &self.doc.as_ref()?.ged;
        let close = Button::new("kinship-close").ghost().xsmall().icon(IconName::Close).on_click(cx.listener(
            |this, _, _, cx| {
                this.kinship = None;
                cx.notify();
            },
        ));
        let head = h_flex().justify_between().child(label_caps("LIEN DE PARENTÉ")).child(close);
        let body = match kinship::relationship(g, a, b) {
            Some(k) => v_flex()
                .gap_2()
                .child(
                    h_flex()
                        .gap_1()
                        .flex_wrap()
                        .child(self.person_link("k-b", b, cx))
                        .child("est")
                        .child(div().font_weight(FontWeight::SEMIBOLD).child(k.label.clone()))
                        .child("de")
                        .child(self.person_link("k-a", a, cx)),
                )
                .when_some(k.degree, |d, deg| d.child(mono(format!("Degré civil : {deg}"))))
                .when(!k.common.is_empty(), |d| {
                    d.child(label_caps("ANCÊTRES COMMUNS")).child(
                        h_flex()
                            .gap_3()
                            .flex_wrap()
                            .children(k.common.iter().enumerate().map(|(n, c)| self.person_link(("k-c", n), c, cx))),
                    )
                })
                .child(label_caps("CHEMIN"))
                .children(k.path.iter().enumerate().map(|(n, x)| {
                    let years = g.individual(x).map(|i| i.years()).unwrap_or_default();
                    h_flex()
                        .gap_2()
                        .child(mono(if n == 0 { "  ".to_string() } else { "→ ".to_string() }))
                        .child(self.person_link(("k-p", n), x, cx))
                        .child(mono(years))
                })),
            None => v_flex().child(format!("Aucun lien trouvé avec {}.", g.record_label(b))),
        };
        Some(
            v_flex()
                .p_4()
                .gap_2()
                .border_b_1()
                .border_color(theme::border())
                .child(head)
                .child(body)
                .into_any_element(),
        )
    }

    // ------------------------------------------------------------------
    // Statistiques
    // ------------------------------------------------------------------

    /// Recalcule les statistiques (ascendance de l'individu racine).
    pub(super) fn refresh_stats(&mut self) {
        self.stats = self.doc.as_ref().map(|d| Rc::new(stats::stats(&d.ged, d.ged.root().as_deref())));
    }

    pub(super) fn render_stats(&self, cx: &mut Context<Self>) -> AnyElement {
        let (Some(s), Some(doc)) = (self.stats.clone(), &self.doc) else { return div().into_any_element() };
        let s: &Stats = &s;
        let g = &doc.ged;
        let top_list = |title: &'static str, items: &[(String, usize)]| {
            let max = items.first().map(|i| i.1).unwrap_or(1).max(1) as f64;
            card()
                .flex_1()
                .min_w(px(260.))
                .child(label_caps(title))
                .children(items.iter().map(|(k, n)| bar_row(k.clone(), *n as f64 / max, n.to_string())))
        };
        let figure = |label: &'static str, value: String| {
            v_flex()
                .gap_0p5()
                .child(div().text_xl().font_weight(FontWeight::SEMIBOLD).child(value))
                .child(div().text_xs().text_color(theme::muted()).child(label))
        };
        let years = |x: Option<f64>| x.map(|v| format!("{v:.0} ans")).unwrap_or_else(|| "—".into());
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
        let centuries = card()
            .child(label_caps("LONGÉVITÉ PAR SIÈCLE DE NAISSANCE"))
            .children(s.centuries.iter().map(|c| {
                let label = format!("{}–{}", c.start, c.start + 99);
                let right = format!("{} ({} n.)", years(c.mean_lifespan), c.births);
                bar_row(label, c.mean_lifespan.unwrap_or(0.) / 100., right)
            }))
            .when(!s.longest.is_empty(), |d| {
                d.child(label_caps("LES PLUS LONGUES VIES")).children(s.longest.iter().enumerate().map(
                    |(n, (x, a))| {
                        h_flex()
                            .justify_between()
                            .child(self.person_link(("long", n), x, cx))
                            .child(mono(format!("{a:.0} ans")))
                    },
                ))
            });
        let ancestry = card()
            .child(label_caps(match &s.root {
                Some(r) => format!("ASCENDANCE DE {}", g.record_label(r).to_uppercase()),
                None => "ASCENDANCE".into(),
            }))
            .children(s.generations.iter().map(|level| {
                let dup = level.found - level.distinct;
                let right = format!(
                    "{}/{}{}",
                    level.found,
                    level.expected,
                    if dup > 0 { format!(" ({dup} doubles)") } else { String::new() }
                );
                bar_row(format!("Génération {}", level.number), level.completeness(), right)
            }))
            .when(s.root.is_some(), |d| d.child(mono(format!("Implexe : {}", pct(s.implexe)))));
        v_flex()
            .id("stats")
            .flex_1()
            .min_w_0()
            .p_4()
            .gap_4()
            .overflow_y_scroll()
            .bg(theme::bg())
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
