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
    ("Unions sans mariage", "sans:mariage"),
    ("Avec média", "avec:média"),
];

const HELP: &str = "Critères : né:1850 · né:1800-1850 · né:<1800 · mort:… · vécu:1870 · lieu:troyes · \
                    prof:meunier · sexe:h/f · sans:source/parents/date/famille/mariage/média · avec:média/note/mariage · \
                    sosa:1-63 · id:I00012";

impl Workspace {
    /// Filtres actifs (choisis dans Affichage › Filtrer la liste), critères non
    /// compris et aide de la recherche (Aide › Aide de la recherche avancée).
    pub(super) fn render_quick_filters(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let query = self.filter.read(cx).value().to_string();
        let parsed = IndividualQuery::parse(&query);
        let active: Vec<_> = QUICK.iter().filter(|(_, token)| has_token(&query, token)).collect();
        let empty = active.is_empty() && parsed.unknown.is_empty() && !self.search_help;
        v_flex()
            .px_3()
            .gap_1()
            .when(!empty, |d| d.pb_2())
            .when(!active.is_empty(), |d| {
                d.child(h_flex().flex_wrap().gap_1().children(active.into_iter().map(|(label, token)| {
                    Button::new(SharedString::from(format!("quick-{token}")))
                        .xsmall()
                        .primary()
                        .icon(IconName::Close)
                        .label(*label)
                        .on_click(cx.listener(move |this, _, w, cx| {
                            let current = this.filter.read(cx).value().to_string();
                            let next = toggle_token(&current, token);
                            this.filter.update(cx, |f, cx| f.set_value(next, w, cx));
                            this.rebuild_rows(cx);
                            cx.notify();
                        }))
                })))
            })
            .when(!parsed.unknown.is_empty(), |d| {
                d.child(
                    div()
                        .text_xs()
                        .text_color(theme::warn())
                        .child(format!("Critère non compris : {}", parsed.unknown.join(", "))),
                )
            })
            .when(self.search_help, |d| {
                d.child(mono(HELP)).child(
                    div()
                        .id("search-help")
                        .text_xs()
                        .text_color(theme::accent())
                        .cursor_pointer()
                        .child("Masquer l'aide")
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.search_help = false;
                            cx.notify();
                        })),
                )
            })
    }
}
