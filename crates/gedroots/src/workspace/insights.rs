//! “Statistics” page and relationship between two individuals (formerly the Analyses window).

use super::*;
use gedcom::analysis::kinship;
use gedcom::analysis::stats::{self, Stats};
use rust_i18n::t;

fn card() -> Div {
    v_flex().p_4().gap_2().rounded(theme::RADIUS_SM).border_1().border_color(theme::border()).bg(theme::panel())
}

fn pct(x: f64) -> String {
    t!("stats.percent", value = format!("{:.0}", x * 100.)).to_string()
}

/// A number with one decimal, with the language's separator (“1.5”, « 1,5 »).
fn decimal(x: f64) -> String {
    let s = format!("{x:.1}");
    match &*rust_i18n::locale() {
        "fr" => s.replace('.', ","),
        _ => s,
    }
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
                .child(div().h_full().rounded_full().bg(theme::muted()).w(relative(value.clamp(0., 1.) as f32))),
        )
        .child(div().w(px(130.)).flex_none().child(mono(right)))
}

impl Workspace {
    /// Clickable link to an individual.
    fn person_link(&self, id: impl Into<ElementId>, x: &str, cx: &mut Context<Self>) -> Stateful<Div> {
        let name = self.doc.as_ref().map(|d| d.ged.record_label(x)).unwrap_or_default();
        let target = x.to_string();
        div()
            .id(id.into())
            .cursor_pointer()
            .hover(|d| d.text_color(theme::accent()).underline())
            .child(name)
            .on_click(cx.listener(move |this, _, w, cx| this.goto(target.clone(), w, cx)))
    }

    // ------------------------------------------------------------------
    // Relationship
    // ------------------------------------------------------------------

    /// Individual › Relationship with…: choose the other person.
    pub(super) fn ask_kinship(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(indi) = self.selected_individual() {
            self.open_picker(relatives::PickPurpose::Kinship { indi }, window, cx);
        }
    }

    /// Link to a person in the relationship window: closes it and opens their record.
    fn kinship_link(&self, id: impl Into<ElementId>, x: &str, cx: &mut Context<Self>) -> Stateful<Div> {
        let name = self.doc.as_ref().map(|d| d.ged.record_label(x)).unwrap_or_default();
        let target = x.to_string();
        div().id(id.into()).cursor_pointer().text_color(theme::accent()).hover(|d| d.underline()).child(name).on_click(
            cx.listener(move |this, _, w, cx| {
                this.kinship = None;
                this.goto(target.clone(), w, cx);
            }),
        )
    }

    /// Modal window of the relationship between A and B (Esc, × or a click outside close it).
    pub(super) fn render_kinship(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let (a, b) = self.kinship.as_ref()?;
        let g = &self.doc.as_ref()?.ged;
        let close = Button::new("kinship-close").ghost().xsmall().icon(IconName::Close).on_click(cx.listener(
            |this, _, _, cx| {
                this.kinship = None;
                cx.notify();
            },
        ));
        let head = h_flex()
            .p_3()
            .gap_2()
            .items_center()
            .border_b_1()
            .border_color(theme::border())
            .child(div().flex_1().font_weight(FontWeight::SEMIBOLD).child(t!("stats.kinship_title")))
            .child(close);
        let body = match kinship::relationship(g, a, b) {
            Some(k) => v_flex()
                .gap_2()
                .child(
                    h_flex()
                        .gap_1()
                        .flex_wrap()
                        .child(self.kinship_link("k-b", b, cx))
                        .child(t!("stats.kinship_is"))
                        .child(div().font_weight(FontWeight::SEMIBOLD).child(k.label.clone()))
                        .child(t!("stats.kinship_of"))
                        .child(self.kinship_link("k-a", a, cx)),
                )
                .when_some(k.degree, |d, deg| d.child(mono(t!("stats.civil_degree", degree = deg).to_string())))
                .when(!k.common.is_empty(), |d| {
                    d.child(label_caps(t!("stats.common_ancestors"))).child(
                        h_flex()
                            .gap_3()
                            .flex_wrap()
                            .children(k.common.iter().enumerate().map(|(n, c)| self.kinship_link(("k-c", n), c, cx))),
                    )
                })
                .child(label_caps(t!("stats.path")))
                .children(k.path.iter().enumerate().map(|(n, x)| {
                    let years = g.individual(x).map(|i| i.years()).unwrap_or_default();
                    h_flex()
                        .gap_2()
                        .child(mono(if n == 0 { "  ".to_string() } else { "→ ".to_string() }))
                        .child(self.kinship_link(("k-p", n), x, cx))
                        .child(mono(years))
                })),
            None => v_flex().child(t!("stats.no_link", a = g.record_label(a), b = g.record_label(b)).to_string()),
        };
        Some(
            div()
                .absolute()
                .inset_0()
                .flex()
                .justify_center()
                .items_start()
                .pt(px(90.))
                .bg(theme::scrim())
                .child(
                    v_flex()
                        .id("kinship")
                        .occlude()
                        .w(px(520.))
                        .max_h(px(560.))
                        .rounded(theme::RADIUS_LG)
                        .border_1()
                        .border_color(theme::border_strong())
                        .bg(theme::panel())
                        .shadow_lg()
                        .on_mouse_down_out(cx.listener(|this, _, _, cx| {
                            this.kinship = None;
                            cx.notify();
                        }))
                        .child(head)
                        .child(div().id("kinship-body").flex_1().min_h_0().overflow_y_scroll().p_4().child(body)),
                )
                .into_any_element(),
        )
    }

    // ------------------------------------------------------------------
    // Statistiques
    // ------------------------------------------------------------------

    /// Recomputes the statistics (ancestry of the root individual).
    pub(super) fn refresh_stats(&mut self) {
        self.stats = self.doc.as_ref().map(|d| Rc::new(stats::stats(&d.ged, d.ged.root().as_deref())));
    }

    pub(super) fn render_stats(&self, cx: &mut Context<Self>) -> AnyElement {
        let (Some(s), Some(doc)) = (self.stats.clone(), &self.doc) else { return div().into_any_element() };
        let s: &Stats = &s;
        let g = &doc.ged;
        let top_list = |title: std::borrow::Cow<'static, str>, items: &[(String, usize)]| {
            let max = items.first().map(|i| i.1).unwrap_or(1).max(1) as f64;
            card()
                .flex_1()
                .min_w(px(260.))
                .child(label_caps(title))
                .children(items.iter().map(|(k, n)| bar_row(k.clone(), *n as f64 / max, n.to_string())))
        };
        let figure = |label: std::borrow::Cow<'static, str>, value: String| {
            v_flex()
                .gap_0p5()
                .child(div().text_xl().font_weight(FontWeight::SEMIBOLD).child(value))
                .child(div().text_xs().text_color(theme::muted()).child(label))
        };
        let years = |x: Option<f64>| {
            x.map(|v| t!("stats.years", years = format!("{v:.0}")).to_string()).unwrap_or_else(|| "—".into())
        };
        let sourced = if s.events == 0 { 0. } else { s.sourced_events as f64 / s.events as f64 };
        let overview = card().child(label_caps(t!("stats.overview"))).child(
            h_flex()
                .gap_8()
                .flex_wrap()
                .child(figure(t!("stats.individuals"), s.individuals.to_string()))
                .child(figure(t!("stats.families"), s.families.to_string()))
                .child(figure(t!("stats.men_women"), format!("{} / {}", s.men, s.women)))
                .child(figure(t!("stats.sourced_events"), pct(sourced)))
                .child(figure(t!("stats.children_per_family"), s.mean_children.map(decimal).unwrap_or("—".into())))
                .child(figure(t!("stats.first_marriage_men"), years(s.mean_marriage_age_men)))
                .child(figure(t!("stats.first_marriage_women"), years(s.mean_marriage_age_women))),
        );
        let centuries = card()
            .child(label_caps(t!("stats.longevity")))
            .children(s.centuries.iter().map(|c| {
                let label = format!("{}–{}", c.start, c.start + 99);
                let right = t!("stats.births", years = years(c.mean_lifespan), births = c.births).to_string();
                bar_row(label, c.mean_lifespan.unwrap_or(0.) / 100., right)
            }))
            .when(!s.longest.is_empty(), |d| {
                d.child(label_caps(t!("stats.longest"))).children(s.longest.iter().enumerate().map(|(n, (x, a))| {
                    h_flex()
                        .justify_between()
                        .child(self.person_link(("long", n), x, cx))
                        .child(mono(t!("stats.years", years = format!("{a:.0}")).to_string()))
                }))
            });
        let ancestry = card()
            .child(label_caps(match &s.root {
                Some(r) => t!("stats.ancestry_of", name = g.record_label(r)).to_string(),
                None => t!("stats.ancestry").into(),
            }))
            .children(s.generations.iter().map(|level| {
                let dup = level.found - level.distinct;
                let right = format!(
                    "{}/{}{}",
                    level.found,
                    level.expected,
                    if dup > 0 { t!("stats.duplicates", n = dup).to_string() } else { String::new() }
                );
                bar_row(t!("stats.generation", n = level.number).to_string(), level.completeness(), right)
            }))
            .when(s.root.is_some(), |d| d.child(mono(t!("stats.implexe", value = pct(s.implexe)).to_string())));
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
                    .child(top_list(t!("stats.surnames"), &s.surnames[..s.surnames.len().min(12)]))
                    .child(top_list(t!("stats.given_men"), &s.given_men))
                    .child(top_list(t!("stats.given_women"), &s.given_women)),
            )
            .child(h_flex().gap_4().items_start().child(ancestry.flex_1()).child(centuries.flex_1()))
            .child(top_list(t!("stats.places"), &s.places))
            .into_any_element()
    }
}
