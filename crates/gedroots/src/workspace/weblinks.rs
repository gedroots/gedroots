//! Liens web des fiches : liste cliquable (ouverture dans le navigateur),
//! et fenêtre d'ajout / de modification.
//!
//! Stockage en GEDCOM 7 standard, voir `gedcom::weblink` : lien multimédia
//! vers un `OBJE` dont le `FILE` est l'URL (`WWW` pour les dépôts).

use super::add::{AddDialog, plus_button};
use super::*;
use gpui_kit::component::tooltip::Tooltip;

pub(super) struct WebLinkDialog {
    owner: String,
    /// Lien modifié (`None` : nouveau lien).
    editing: Option<WebLinkKind>,
    url: Entity<InputState>,
    title: Entity<InputState>,
    /// Les dépôts n'ont pas de titre de lien (`WWW` porte seulement l'adresse).
    titled: bool,
    error: Option<SharedString>,
}

impl Workspace {
    /// Ouvre la fenêtre d'ajout (ou de modification) d'un lien web.
    pub(super) fn open_weblink_dialog(
        &mut self,
        owner: String,
        editing: Option<WebLink>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(doc) = &self.doc else { return };
        if !doc.ged.can_have_web_links(&owner) {
            return;
        }
        let titled = doc.ged.get(&owner).is_some_and(|n| n.tag != "REPO");
        let (u, t) = editing.as_ref().map(|l| (l.url.clone(), l.title.clone().unwrap_or_default())).unwrap_or_default();
        let url = cx.new(|cx| InputState::new(window, cx).placeholder("https://…").default_value(u));
        let title =
            cx.new(|cx| InputState::new(window, cx).placeholder("ex. Registre paroissial, 1850").default_value(t));
        url.update(cx, |s, cx| s.focus(window, cx));
        self.add_dialog = Some(AddDialog::WebLink(Box::new(WebLinkDialog {
            owner,
            editing: editing.map(|l| l.kind),
            url,
            title,
            titled,
            error: None,
        })));
        cx.notify();
    }

    fn save_weblink(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(AddDialog::WebLink(d)) = &mut self.add_dialog else { return };
        let url = d.url.read(cx).value().to_string();
        let title = d.title.read(cx).value().to_string();
        let (owner, editing) = (d.owner.clone(), d.editing.clone());
        let apply = move |g: &mut Gedcom| match &editing {
            Some(k) => g.update_web_link(&owner, k, &url, &title),
            None => g.add_web_link(&owner, &url, &title),
        };
        // Vérification avant la modification, pour garder la fenêtre ouverte en cas d'erreur.
        let mut probe = self.doc.as_ref().unwrap().ged.clone();
        if let Err(e) = apply(&mut probe) {
            d.error = Some(e.into());
            cx.notify();
            return;
        }
        let msg = if d.editing.is_some() { "Lien web modifié" } else { "Lien web ajouté" };
        self.add_dialog = None;
        self.mutate(window, cx, move |g| apply(g).map(|_| (msg.to_string(), None)));
        window.focus(&self.focus, cx);
    }

    fn remove_weblink(&mut self, owner: String, kind: WebLinkKind, window: &mut Window, cx: &mut Context<Self>) {
        self.mutate(window, cx, move |g| {
            g.remove_web_link(&owner, &kind);
            Ok(("Lien web retiré".into(), None))
        });
    }

    pub(super) fn weblink_dialog_title(&self, d: &WebLinkDialog) -> String {
        let g = &self.doc.as_ref().unwrap().ged;
        let what = if d.editing.is_some() { "Modifier le lien web" } else { "Nouveau lien web" };
        format!("{what} · {}", g.record_label(&d.owner))
    }

    pub(super) fn weblink_body(&self, d: &WebLinkDialog, cx: &mut Context<Self>) -> AnyElement {
        let field = |label: &'static str, el: AnyElement| v_flex().gap_1().min_w_0().child(label_caps(label)).child(el);
        v_flex()
            .gap_3()
            .child(field("Adresse", Input::new(&d.url).small().into_any_element()))
            .when(d.titled, |v| v.child(field("Titre", Input::new(&d.title).small().into_any_element())))
            .when_some(d.error.clone(), |v, e| v.child(div().text_sm().text_color(theme::danger()).child(e)))
            .child(
                h_flex().gap_2().justify_end().child(
                    Button::new("weblink-save")
                        .primary()
                        .small()
                        .icon(IconName::Check)
                        .label(if d.editing.is_some() { "Enregistrer" } else { "Ajouter" })
                        .on_click(cx.listener(|this, _, w, cx| this.save_weblink(w, cx))),
                ),
            )
            .on_key_down(cx.listener(|this, ev: &KeyDownEvent, w, cx| {
                if ev.keystroke.key == "enter" {
                    this.save_weblink(w, cx);
                }
            }))
            .into_any_element()
    }

    /// Section « Liens web » du panneau de détail (masquée s'il n'y en a pas).
    pub(super) fn render_web_links(&self, record: &str, cx: &mut Context<Self>) -> Option<AnyElement> {
        let g = &self.doc.as_ref()?.ged;
        let links = g.web_links(record);
        if links.is_empty() {
            return None;
        }
        let owner = record.to_string();
        let header = section_header("Liens web", {
            let x = owner.clone();
            plus_button("add-weblink")
                .tooltip("Ajouter un lien web")
                .on_click(cx.listener(move |this, _, w, cx| this.open_weblink_dialog(x.clone(), None, w, cx)))
        });
        let rows = links.into_iter().enumerate().map(|(n, l)| {
            let group = SharedString::from(format!("weblink-{n}"));
            let (href, edit, remove) = (l.href(), l.clone(), l.kind.clone());
            let (o1, o2) = (owner.clone(), owner.clone());
            let host = host(&l.url).to_string();
            let label = l.label();
            h_flex()
                .id(SharedString::from(format!("weblink-row-{n}")))
                .group(group.clone())
                .gap_2()
                .px_2()
                .py_1()
                .rounded_sm()
                .items_center()
                .cursor_pointer()
                .hover(|d| d.bg(theme::hover()))
                .tooltip({
                    // Les adresses d'archives font parfois plusieurs centaines de caractères.
                    let href = l.href();
                    let tip = match href.char_indices().nth(90) {
                        Some((i, _)) => SharedString::from(format!("{}…", &href[..i])),
                        None => SharedString::from(href),
                    };
                    move |w, cx| Tooltip::new(tip.clone()).build(w, cx)
                })
                .child(icon(IconName::Globe).text_color(theme::accent()))
                .child(
                    v_flex()
                        .flex_1()
                        .min_w_0()
                        .child(
                            div()
                                .overflow_hidden()
                                .whitespace_nowrap()
                                .text_ellipsis()
                                .text_color(theme::accent())
                                .child(label.clone()),
                        )
                        .when(label != host, |d| {
                            d.child(
                                div()
                                    .overflow_hidden()
                                    .whitespace_nowrap()
                                    .text_ellipsis()
                                    .text_xs()
                                    .text_color(theme::muted())
                                    .child(host),
                            )
                        }),
                )
                .child(
                    h_flex()
                        .opacity(0.)
                        .group_hover(group, |s| s.opacity(1.))
                        .child(
                            Button::new(SharedString::from(format!("weblink-edit-{n}")))
                                .xsmall()
                                .ghost()
                                .icon(IconName::Pencil)
                                .tooltip("Modifier")
                                .on_click(cx.listener(move |this, _, w, cx| {
                                    cx.stop_propagation();
                                    this.open_weblink_dialog(o1.clone(), Some(edit.clone()), w, cx)
                                })),
                        )
                        .child(
                            Button::new(SharedString::from(format!("weblink-remove-{n}")))
                                .xsmall()
                                .ghost()
                                .icon(IconName::Close)
                                .tooltip("Retirer")
                                .on_click(cx.listener(move |this, _, w, cx| {
                                    cx.stop_propagation();
                                    this.remove_weblink(o2.clone(), remove.clone(), w, cx)
                                })),
                        ),
                )
                .on_click(move |_, _, cx| cx.open_url(&href))
        });
        Some(
            v_flex()
                .p_4()
                .gap_1()
                .border_b_1()
                .border_color(theme::border())
                .child(header)
                .children(rows)
                .into_any_element(),
        )
    }
}

/// Le média est-il une page web (affichée dans « Liens web » plutôt que dans les médias) ?
pub(super) fn is_web_media(g: &Gedcom, xref: &str) -> bool {
    g.get(xref).filter(|n| n.tag == "OBJE").map(Media::new).is_some_and(|m| m.is_web())
}
