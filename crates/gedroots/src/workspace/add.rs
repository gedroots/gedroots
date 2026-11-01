//! Fenêtres d'ajout ouvertes par les « + » du panneau de détail :
//! nouvel événement, et proches (créer ou lier une personne).

use super::relatives::PickPurpose;
use super::*;
use crate::editor::{PickerState, choice, picker};

pub(super) enum AddDialog {
    Event(Box<EventDialog>),
    /// Proches d'un individu, ou membres d'une famille.
    Relatives {
        target: String,
    },
    WebLink(Box<super::weblinks::WebLinkDialog>),
}

/// Action d'une ligne de la fenêtre des proches.
type RowAction = Box<dyn Fn(&mut Workspace, &mut Window, &mut Context<Workspace>)>;

pub(super) struct EventDialog {
    owner: String,
    /// Événement modifié (position dans l'enregistrement) ; `None` : ajout.
    editing: Option<usize>,
    kind: Entity<PickerState>,
    detail: Entity<InputState>,
    date: Entity<InputState>,
    place: Entity<InputState>,
    source: Entity<PickerState>,
    error: Option<SharedString>,
    _subs: Vec<Subscription>,
}

impl Workspace {
    /// Ouvre la fenêtre « Nouvel événement » pour un individu ou une famille,
    /// ou la fenêtre de modification de l'événement `editing`.
    pub(super) fn open_event_dialog(
        &mut self,
        owner: String,
        editing: Option<usize>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(doc) = &self.doc else { return };
        let g = &doc.ged;
        let current = match editing {
            Some(ix) => match g.read_event(&owner, ix) {
                Some(ev) => Some(ev),
                None => return,
            },
            None => None,
        };
        let family = g.family(&owner).is_some();
        let tags = if family { FAMILY_EVENTS } else { INDIVIDUAL_EVENTS };
        let mut options: Vec<(&str, &str)> =
            tags.iter().map(|t| (*t, if *t == "EVEN" { "Autre…" } else { event_label(t).unwrap_or(t) })).collect();
        // Type absent de la liste proposée à l'ajout (CHR, ADOP…) : on le garde.
        let cur_tag = current.as_ref().map(|e| e.tag.clone()).unwrap_or_default();
        if !cur_tag.is_empty() && !tags.contains(&cur_tag.as_str()) {
            options.insert(0, (cur_tag.as_str(), event_label(&cur_tag).unwrap_or(cur_tag.as_str())));
        }
        let default_kind = match &current {
            Some(e) => e.tag.as_str(),
            None if family => "MARR",
            None => "",
        };
        let kind = choice(&options, default_kind, window, cx);
        let cur = current.clone().unwrap_or_default();
        let text = |ph: &'static str, value: String, window: &mut Window, cx: &mut Context<Self>| {
            cx.new(|cx| InputState::new(window, cx).placeholder(ph).default_value(value))
        };
        let detail = text("", cur.detail, window, cx);
        let date = text("ex. 14 MAR 1912, ABT 1850", cur.date, window, cx);
        let place = text("Commune…", cur.place, window, cx);
        let source = picker(g, RecordKind::Source, cur.source.as_deref().unwrap_or(""), window, cx);
        let sub = cx.subscribe_in(&place, window, |this, input, ev: &InputEvent, _, cx| {
            if matches!(ev, InputEvent::Focus | InputEvent::Change) {
                this.place_focus = Some(input.entity_id());
                cx.notify();
            }
        });
        let sub_kind = cx.observe(&kind, |_, _, cx| cx.notify());
        self.add_dialog = Some(AddDialog::Event(Box::new(EventDialog {
            owner,
            editing,
            kind,
            detail,
            date,
            place,
            source,
            error: None,
            _subs: vec![sub, sub_kind],
        })));
        cx.notify();
    }

    pub(super) fn open_relatives_dialog(&mut self, target: String, cx: &mut Context<Self>) {
        self.add_dialog = Some(AddDialog::Relatives { target });
        cx.notify();
    }

    fn close_add_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.add_dialog = None;
        window.focus(&self.focus, cx);
        cx.notify();
    }

    fn save_event(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(AddDialog::Event(d)) = &mut self.add_dialog else { return };
        let tag = d.kind.read(cx).selected_value().cloned().unwrap_or_default();
        if tag.is_empty() {
            d.error = Some("Choisissez le type d'événement".into());
            cx.notify();
            return;
        }
        let ev = NewEvent {
            tag,
            detail: d.detail.read(cx).value().to_string(),
            date: d.date.read(cx).value().to_string(),
            place: d.place.read(cx).value().to_string(),
            source: d.source.read(cx).selected_value().cloned(),
        };
        let (owner, editing) = (d.owner.clone(), d.editing);
        // Vérification avant la modification, pour garder la fenêtre ouverte en cas d'erreur.
        let mut probe = self.doc.as_ref().unwrap().ged.clone();
        let check = match editing {
            Some(ix) => probe.update_event(&owner, ix, &ev),
            None => probe.add_event(&owner, &ev),
        };
        if let Err(e) = check {
            d.error = Some(e.into());
            cx.notify();
            return;
        }
        self.add_dialog = None;
        let label = if ev.tag == "EVEN" { ev.detail.clone() } else { event_label(&ev.tag).unwrap_or("").to_string() };
        self.mutate(window, cx, move |g| {
            let who = g.record_label(&owner);
            match editing {
                Some(ix) => {
                    g.update_event(&owner, ix, &ev)?;
                    Ok((format!("{label} de {who} modifié(e)"), None))
                }
                None => {
                    g.add_event(&owner, &ev)?;
                    Ok((format!("{label} ajouté(e) à {who}"), None))
                }
            }
        });
        window.focus(&self.focus, cx);
    }

    /// Supprime un événement (Annuler pour revenir en arrière).
    pub(super) fn delete_event(&mut self, owner: String, ix: usize, window: &mut Window, cx: &mut Context<Self>) {
        if matches!(&self.add_dialog, Some(AddDialog::Event(d)) if d.owner == owner && d.editing == Some(ix)) {
            self.add_dialog = None;
        }
        self.mutate(window, cx, move |g| {
            let who = g.record_label(&owner);
            let label = g.delete_event(&owner, ix)?;
            Ok((format!("{label} retiré(e) de {who} (Annuler pour revenir en arrière)"), None))
        });
        window.focus(&self.focus, cx);
    }

    pub(super) fn render_add_dialog(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let g = &self.doc.as_ref()?.ged;
        let (title, body): (String, AnyElement) = match self.add_dialog.as_ref()? {
            AddDialog::Event(d) => {
                let what = match d.editing.and_then(|ix| g.event_title(&d.owner, ix)) {
                    Some(t) => t,
                    None => "Nouvel événement".into(),
                };
                (format!("{what} · {}", g.record_label(&d.owner)), self.event_body(d, cx))
            }
            AddDialog::Relatives { target } => match g.family(target) {
                Some(_) => (format!("Membres · {}", g.record_label(target)), self.family_body(target, cx)),
                None => (format!("Proches · {}", g.record_label(target)), self.relatives_body(target, cx)),
            },
            AddDialog::WebLink(d) => (self.weblink_dialog_title(d), self.weblink_body(d, cx)),
        };
        Some(
            div()
                .absolute()
                .inset_0()
                .flex()
                .justify_center()
                .items_start()
                .pt(px(90.))
                .bg(gpui_kit::hsla(0., 0., 0., 0.45))
                .child(
                    v_flex()
                        .id("add-dialog")
                        .occlude()
                        .w(px(520.))
                        .rounded_lg()
                        .border_1()
                        .border_color(theme::border_strong())
                        .bg(theme::panel())
                        .shadow_lg()
                        .on_key_down(cx.listener(|this, ev: &KeyDownEvent, w, cx| {
                            if ev.keystroke.key == "escape" {
                                this.close_add_dialog(w, cx);
                            }
                        }))
                        .child(
                            h_flex()
                                .p_3()
                                .gap_2()
                                .items_center()
                                .border_b_1()
                                .border_color(theme::border())
                                .child(
                                    div()
                                        .flex_1()
                                        .min_w_0()
                                        .overflow_hidden()
                                        .whitespace_nowrap()
                                        .text_ellipsis()
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .child(title),
                                )
                                .child(
                                    Button::new("add-close")
                                        .xsmall()
                                        .ghost()
                                        .icon(IconName::Close)
                                        .on_click(cx.listener(|this, _, w, cx| this.close_add_dialog(w, cx))),
                                ),
                        )
                        .child(div().p_3().child(body)),
                )
                .into_any_element(),
        )
    }

    fn event_body(&self, d: &EventDialog, cx: &mut Context<Self>) -> AnyElement {
        let tag = d.kind.read(cx).selected_value().cloned().unwrap_or_default();
        let detail_label = match tag.as_str() {
            "OCCU" => Some("Profession"),
            "EDUC" => Some("Établissement ou diplôme"),
            "EVEN" => Some("Type d'événement"),
            "RESI" | "" => None,
            _ => Some("Précision"),
        };
        let field = |label: &'static str, el: AnyElement| v_flex().gap_1().min_w_0().child(label_caps(label)).child(el);
        v_flex()
            .gap_3()
            .child(
                h_flex()
                    .gap_3()
                    .child(
                        field("Type", Select::new(&d.kind).small().placeholder("Choisir…").into_any_element()).flex_1(),
                    )
                    .when_some(detail_label, |row, label| {
                        row.child(field(label, Input::new(&d.detail).small().into_any_element()).flex_1())
                    }),
            )
            .child(
                h_flex()
                    .gap_3()
                    .items_start()
                    .child(field("Date", Input::new(&d.date).small().into_any_element()).w(px(180.)).flex_none())
                    .child(
                        field("Lieu", Input::new(&d.place).small().into_any_element())
                            .flex_1()
                            .children(self.place_suggestions(&d.place, cx)),
                    ),
            )
            .child(field(
                "Source",
                Select::new(&d.source)
                    .small()
                    .cleanable(true)
                    .placeholder("Aucune")
                    .search_placeholder("Rechercher…")
                    .into_any_element(),
            ))
            .when_some(d.error.clone(), |v, e| v.child(div().text_sm().text_color(theme::danger()).child(e)))
            .child(
                h_flex()
                    .gap_2()
                    .when_some(d.editing, |row, ix| {
                        let owner = d.owner.clone();
                        row.child(
                            Button::new("event-delete")
                                .ghost()
                                .small()
                                .icon(IconName::Delete)
                                .label("Supprimer")
                                .on_click(
                                    cx.listener(move |this, _, w, cx| this.delete_event(owner.clone(), ix, w, cx)),
                                ),
                        )
                    })
                    .child(div().flex_1())
                    .child(
                        Button::new("add-event-save")
                            .primary()
                            .small()
                            .icon(IconName::Check)
                            .label(if d.editing.is_some() { "Enregistrer" } else { "Ajouter" })
                            .on_click(cx.listener(|this, _, w, cx| this.save_event(w, cx))),
                    ),
            )
            .into_any_element()
    }

    /// Une ligne « Rôle : Nouveau · Existant… ».
    fn relative_row(
        &self,
        label: &'static str,
        new: Option<RowAction>,
        existing: RowAction,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let existing = Rc::new(existing);
        h_flex()
            .gap_2()
            .items_center()
            .py_1()
            .child(div().flex_1().child(label))
            .when_some(new, |d, new| {
                let new = Rc::new(new);
                d.child(
                    Button::new(SharedString::from(format!("rel-new-{label}")))
                        .xsmall()
                        .ghost()
                        .icon(IconName::Plus)
                        .label("Nouvelle personne")
                        .on_click(cx.listener(move |this, _, w, cx| {
                            this.add_dialog = None;
                            new(this, w, cx);
                        })),
                )
            })
            .child(
                Button::new(SharedString::from(format!("rel-link-{label}")))
                    .xsmall()
                    .ghost()
                    .label("Personne existante…")
                    .on_click(cx.listener(move |this, _, w, cx| {
                        this.add_dialog = None;
                        existing(this, w, cx);
                    })),
            )
            .into_any_element()
    }

    fn relatives_body(&self, indi: &str, cx: &mut Context<Self>) -> AnyElement {
        let g = &self.doc.as_ref().unwrap().ged;
        let (father, mother) = g.parents(indi);
        let rels = Relation::ALL.into_iter().filter(|r| match r {
            Relation::Father => father.is_none(),
            Relation::Mother => mother.is_none(),
            _ => true,
        });
        let mut rows = Vec::new();
        for rel in rels.collect::<Vec<_>>() {
            let (a, b) = (indi.to_string(), indi.to_string());
            rows.push(self.relative_row(
                rel.label_fr(),
                Some(Box::new(move |this, w, cx| this.add_relative(a.clone(), rel, w, cx))),
                Box::new(move |this, w, cx| this.open_picker(PickPurpose::Relative { indi: b.clone(), rel }, w, cx)),
                cx,
            ));
        }
        v_flex().children(rows).into_any_element()
    }

    fn family_body(&self, fam: &str, cx: &mut Context<Self>) -> AnyElement {
        let g = &self.doc.as_ref().unwrap().ged;
        let full = g.family(fam).is_some_and(|f| f.husband().is_some() && f.wife().is_some());
        let (a, b, c) = (fam.to_string(), fam.to_string(), fam.to_string());
        let child = self.relative_row(
            "Enfant",
            Some(Box::new(move |this, w, cx| this.add_family_child(a.clone(), w, cx))),
            Box::new(move |this, w, cx| this.open_picker(PickPurpose::FamilyChild { fam: b.clone() }, w, cx)),
            cx,
        );
        let parent = if full {
            None
        } else {
            Some(self.relative_row(
                "Parent",
                None,
                Box::new(move |this, w, cx| this.open_picker(PickPurpose::FamilySpouse { fam: c.clone() }, w, cx)),
                cx,
            ))
        };
        v_flex().child(child).children(parent).into_any_element()
    }
}

/// Bouton « + » discret à côté d'un titre de section.
pub(super) fn plus_button(id: &'static str) -> Button {
    Button::new(id).xsmall().ghost().icon(IconName::Plus)
}
