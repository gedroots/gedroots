//! Filtres rapides et aide de la recherche dans la liste des individus.

use super::*;

/// Filtres rapides : (libellé, critère inséré dans la recherche).
const QUICK: &[(&str, &str)] = &[
    ("Hommes", "sexe:h"),
    ("Femmes", "sexe:f"),
    ("Ascendants", "sosa"),
    ("Sans source", "sans:source"),
    ("Sans parents", "sans:parents"),
    ("Sans date", "sans:date"),
    ("Avec média", "avec:média"),
];

const HELP: &str = "Critères : né:1850 · né:1800-1850 · né:<1800 · mort:… · vécu:1870 · lieu:troyes · \
                    prof:meunier · sexe:h/f · sans:source/parents/date/famille/média · avec:média/note · \
                    sosa:1-63 · id:I00012";

impl Workspace {
    pub(super) fn render_quick_filters(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let query = self.filter.read(cx).value().to_string();
        let parsed = IndividualQuery::parse(&query);
        v_flex()
            .px_3()
            .pb_2()
            .gap_1()
            .child(h_flex().flex_wrap().gap_1().children(QUICK.iter().map(|(label, token)| {
                let on = has_token(&query, token);
                Button::new(SharedString::from(format!("quick-{token}")))
                    .xsmall()
                    .when(on, |b| b.primary())
                    .when(!on, |b| b.ghost())
                    .label(*label)
                    .on_click(cx.listener(move |this, _, w, cx| {
                        let current = this.filter.read(cx).value().to_string();
                        let next = toggle_token(&current, token);
                        this.filter.update(cx, |f, cx| f.set_value(next, w, cx));
                        this.rebuild_rows(cx);
                        cx.notify();
                    }))
            })))
            .when(!parsed.unknown.is_empty(), |d| {
                d.child(
                    div()
                        .text_xs()
                        .text_color(theme::warn())
                        .child(format!("Critère non compris : {}", parsed.unknown.join(", "))),
                )
            })
            .child(
                h_flex().gap_2().child(
                    div()
                        .id("search-help")
                        .text_xs()
                        .text_color(theme::accent())
                        .cursor_pointer()
                        .child(if self.search_help { "Masquer l'aide" } else { "Recherche avancée…" })
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.search_help = !this.search_help;
                            cx.notify();
                        })),
                ),
            )
            .when(self.search_help, |d| d.child(mono(HELP)))
    }
}
