//! Champ de recherche des listes : critères en jetons, propositions, types de médias.
//!
//! La requête reste du texte (`né:1850 lieu:troyes guyot`, voir `gedcom::query`) ;
//! chaque critère complet s'affiche en jeton atomique dans le champ. Sous le
//! champ, des propositions tirées du fichier, avec le nombre de résultats que
//! donnerait chacune. Dans la zone Médias, une ligne de types (sous-dossiers des
//! réglages, sinon support GEDCOM) restreint la liste.

use super::*;
use gedcom::query::{Suggestion, words};
use gpui_kit::component::input::{InlineToken, InputContent};

/// Nombre maximal de propositions affichées.
const MAX_SUGGESTIONS: usize = 8;

impl Workspace {
    /// Le texte du champ a changé (frappe, effacement, jeton supprimé).
    pub(super) fn on_search_changed(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.suggest_closed = false;
        self.tokenize_search(window, cx);
        self.build_rows(cx);
        self.refresh_suggestions(cx);
        cx.notify();
    }

    /// Transforme chaque critère complet en jeton (le texte ne change pas).
    pub(super) fn tokenize_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(scope) = self.view.scope() else { return };
        let (text, spans, cursor) = {
            let f = self.filter.read(cx);
            let spans: Vec<_> = f.tokens().iter().map(|s| s.range()).collect();
            (f.value().to_string(), spans, f.cursor())
        };
        let chips = gedcom::chips(scope, &text);
        if chips.iter().map(|c| c.0.clone()).eq(spans.iter().cloned()) {
            return;
        }
        let mut content = InputContent::new(text);
        for (range, raw, label) in chips {
            match content.clone().with_token(range, InlineToken::new(raw.clone(), raw).with_label(label)) {
                Ok(c) => content = c,
                Err(_) => continue,
            }
        }
        self.filter.update(cx, |f, cx| {
            f.set_value(content, window, cx);
            f.set_selected_range(cursor..cursor, cx);
        });
    }

    /// Requête appliquée à la liste : sans le critère en cours de frappe
    /// (`né:18…`), qui ne compte qu'une fois suivi d'une espace ou de ↵.
    pub(super) fn committed_query(text: &str) -> String {
        let (range, partial) = Self::partial_word(text);
        if partial.contains(':') { text[..range.start].to_string() } else { text.to_string() }
    }

    /// Valide le critère en cours de frappe (↵ sans proposition).
    fn commit_partial(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let text = self.filter.read(cx).value().to_string();
        let (_, partial) = Self::partial_word(&text);
        if !partial.contains(':') {
            return;
        }
        let next = format!("{text} ");
        let end = next.len();
        self.filter.update(cx, |f, cx| {
            f.set_value(next, window, cx);
            f.set_selected_range(end..end, cx);
        });
        self.on_search_changed(window, cx);
    }

    /// Une recherche ou un filtre réduit la liste affichée.
    pub(super) fn is_filtered(&self, cx: &App) -> bool {
        self.only_flagged || !self.filter.read(cx).value().trim().is_empty() || self.active_media_type().is_some()
    }

    /// Type de média choisi, s'il existe dans le fichier ouvert.
    pub(super) fn active_media_type(&self) -> Option<String> {
        if self.view != View::Media {
            return None;
        }
        let t = self.media_type.clone()?;
        match &self.media_index {
            Some(i) if !i.types.contains(&t) => None,
            _ => Some(t),
        }
    }

    pub(super) fn ensure_media_index(&mut self) {
        if self.media_index.is_none()
            && let Some(doc) = &self.doc
        {
            self.media_index = Some(Rc::new(doc.ged.media_index()));
        }
    }

    /// Mot en cours de frappe (plage et texte) ; vide après une espace.
    fn partial_word(text: &str) -> (std::ops::Range<usize>, String) {
        if text.is_empty() || text.ends_with(char::is_whitespace) {
            return (text.len()..text.len(), String::new());
        }
        match words(text).pop() {
            Some((r, w)) if r.end == text.len() => (r, w.to_string()),
            _ => (text.len()..text.len(), String::new()),
        }
    }

    /// Recalcule les propositions et le nombre de résultats de chacune.
    pub(super) fn refresh_suggestions(&mut self, cx: &App) {
        self.suggestions.clear();
        self.suggest_sel = 0;
        let (Some(scope), Some(doc)) = (self.view.scope(), &self.doc) else { return };
        let text = self.filter.read(cx).value().to_string();
        let (range, partial) = Self::partial_word(&text);
        // Le dernier mot est déjà un critère complet : rien à proposer.
        if gedcom::chip_label(scope, &partial).is_some() {
            return;
        }
        let index = self.media_index.as_deref();
        let media_type = self.active_media_type();
        let present: Vec<&str> = words(&text).into_iter().map(|(_, w)| w).collect();
        for s in doc.ged.suggest(scope, &partial, index) {
            if present.iter().any(|w| w.eq_ignore_ascii_case(&s.insert)) {
                continue;
            }
            let query = format!("{}{} {}", &text[..range.start], s.insert, &text[range.end..]);
            let n = doc.ged.search(scope, &query, index, media_type.as_deref()).len();
            if n > 0 {
                self.suggestions.push((s, n));
            }
            if self.suggestions.len() == MAX_SUGGESTIONS {
                break;
            }
        }
    }

    /// Remplace le mot en cours par la proposition, qui devient un jeton.
    pub(super) fn accept_suggestion(&mut self, i: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some((s, _)) = self.suggestions.get(i).cloned() else { return };
        let text = self.filter.read(cx).value().to_string();
        let (range, _) = Self::partial_word(&text);
        let rest = text[range.end..].trim_start();
        let next = format!("{}{} {}", &text[..range.start], s.insert, rest);
        let end = range.start + s.insert.len() + 1;
        self.filter.update(cx, |f, cx| {
            f.set_value(next, window, cx);
            f.set_selected_range(end..end, cx);
            f.focus(window, cx);
        });
        self.on_search_changed(window, cx);
    }

    fn suggestions_visible(&self) -> bool {
        self.search_focused && !self.suggest_closed && !self.suggestions.is_empty() && self.view.scope().is_some()
    }

    /// Champ de recherche ; ↑ ↓ ↵ et Échap pilotent les propositions.
    pub(super) fn render_search_field(&self, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .px_3()
            .pt_3()
            .pb_2()
            .capture_key_down(cx.listener(|this, ev: &KeyDownEvent, window, cx| {
                if ev.keystroke.modifiers.modified() {
                    return;
                }
                if !this.suggestions_visible() {
                    if ev.keystroke.key == "enter" {
                        this.commit_partial(window, cx);
                    }
                    return;
                }
                let n = this.suggestions.len();
                match ev.keystroke.key.as_str() {
                    "down" => this.suggest_sel = (this.suggest_sel + 1) % n,
                    "up" => this.suggest_sel = (this.suggest_sel + n - 1) % n,
                    "enter" => this.accept_suggestion(this.suggest_sel, window, cx),
                    "escape" => this.suggest_closed = true,
                    _ => return,
                }
                cx.stop_propagation();
                cx.notify();
            }))
            .child(Input::new(&self.filter).small().cleanable(true))
    }

    /// Critères mal écrits (`sans:truc`), seulement quand il y en a.
    pub(super) fn render_search_notes(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let query = Self::committed_query(&self.filter.read(cx).value());
        let unknown = match self.view.scope() {
            Some(Scope::Individuals) => IndividualQuery::parse(&query).unknown,
            Some(scope) => RecordQuery::parse(scope, &query).unknown,
            None => Vec::new(),
        };
        div().when(!unknown.is_empty(), |d| {
            d.px_3()
                .pb_2()
                .text_xs()
                .text_color(theme::warn())
                .child(format!("Critère non compris : {}", unknown.join(", ")))
        })
    }

    /// Types de médias sur une ligne qui défile : Tous · Actes · Portraits…
    pub(super) fn render_media_types(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let types: Vec<String> = self.media_index.as_ref().map(|i| i.types.clone()).unwrap_or_default();
        let current = self.active_media_type();
        let pill = |id: usize, label: String, value: Option<String>, on: bool, cx: &mut Context<Self>| {
            div()
                .id(("media-type", id))
                .flex_none()
                .px_2()
                .py_0p5()
                .rounded(theme::RADIUS_SM)
                .border_1()
                .text_xs()
                .cursor_pointer()
                .when(on, |d| {
                    d.bg(theme::accent_soft())
                        .border_color(theme::accent())
                        .text_color(theme::text())
                        .font_weight(FontWeight::SEMIBOLD)
                })
                .when(!on, |d| {
                    d.border_color(transparent_black()).text_color(theme::muted()).hover(|d| d.bg(theme::hover()))
                })
                .child(label)
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.media_type = value.clone();
                    this.build_rows(cx);
                    this.refresh_suggestions(cx);
                    cx.notify();
                }))
        };
        h_flex().id("media-types").when(types.len() >= 2, |d| {
            let mut d = d.px_3().pb_2().gap_1().overflow_x_scroll();
            d = d.child(pill(0, "Tous".into(), None, current.is_none(), cx));
            for (i, t) in types.iter().enumerate() {
                let on = current.as_deref() == Some(t.as_str());
                d = d.child(pill(i + 1, t.clone(), Some(t.clone()), on, cx));
            }
            d
        })
    }

    /// Propositions sous le champ, groupées (Lieu, Nom, Critères…).
    pub(super) fn render_suggestions(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !self.suggestions_visible() {
            return None;
        }
        let mut list = v_flex()
            .id("search-suggestions")
            .absolute()
            .top(px(46.))
            .left_3()
            .right_3()
            .p_1p5()
            .gap_0p5()
            .bg(theme::bg())
            .border_1()
            .border_color(theme::border())
            .rounded(theme::RADIUS_MD)
            .shadow_lg()
            .occlude()
            .text_sm();
        let mut group = "";
        for (i, (s, n)) in self.suggestions.iter().enumerate() {
            if s.group != group {
                group = s.group;
                list = list.child(div().px_2().pt_1().child(label_caps(group)));
            }
            let on = i == self.suggest_sel;
            list = list.child(
                h_flex()
                    .id(("suggestion", i))
                    .px_2()
                    .py_1()
                    .gap_2()
                    .items_center()
                    .rounded(theme::RADIUS_SM)
                    .cursor_pointer()
                    .when(on, |d| d.bg(theme::hover()))
                    .when(!on, |d| d.hover(|d| d.bg(theme::hover())))
                    .child(Icon::new(group_icon(s)).size(px(14.)).text_color(if on {
                        theme::accent()
                    } else {
                        theme::muted()
                    }))
                    .child(div().flex_1().min_w_0().truncate().child(s.label.clone()))
                    .child(div().text_xs().text_color(theme::muted()).child(group_thousands(*n)))
                    // Au clic, le champ garde le focus.
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _, window, cx| {
                            window.prevent_default();
                            cx.stop_propagation();
                            this.accept_suggestion(i, window, cx);
                        }),
                    ),
            );
        }
        Some(list.into_any_element())
    }
}

fn group_icon(s: &Suggestion) -> IconName {
    match s.group {
        "Lieu" => IconName::MapPin,
        "Nom" | "Personne" => IconName::Users,
        "Profession" => IconName::FileText,
        "Type d'acte" => IconName::BookOpen,
        "Dépôt" => IconName::Landmark,
        "Juridiction" => IconName::Map,
        "Dates" | "Événement" => IconName::Calendar,
        "Support" => IconName::Image,
        _ => IconName::Search,
    }
}
