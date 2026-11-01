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
}

/// Action d'une ligne de la fenêtre des proches.
type RowAction = Box<dyn Fn(&mut Workspace, &mut Window, &mut Context<Workspace>)>;

pub(super) struct EventDialog {
    owner: String,
    kind: Entity<PickerState>,
    detail: Entity<InputState>,
    date: Entity<InputState>,
    place: Entity<InputState>,
    source: Entity<PickerState>,
    error: Option<SharedString>,
    _subs: Vec<Subscription>,
}

impl Workspace {
    /// Ouvre la fenêtre « Nouvel événement » pour un individu ou une famille.
    pub(super) fn open_event_dialog(&mut self, owner: String, window: &mut Window, cx: &mut Context<Self>) {
        let Some(doc) = &self.doc else { return };
        let g = &doc.ged;
        let family = g.family(&owner).is_some();
        let tags = if family { FAMILY_EVENTS } else { INDIVIDUAL_EVENTS };
        let options: Vec<(&str, &str)> =
            tags.iter().map(|t| (*t, if *t == "EVEN" { "Autre…" } else { event_label(t).unwrap_or(t) })).collect();
        let kind = choice(&options, if family { "MARR" } else { "" }, window, cx);
        let text = |ph: &'static str, window: &mut Window, cx: &mut Context<Self>| {
            cx.new(|cx| InputState::new(window, cx).placeholder(ph))
        };
        let detail = text("", window, cx);
        let date = text("ex. 14 MAR 1912, ABT 1850", window, cx);
        let place = text("Commune…", window, cx);
        let source = picker(g, RecordKind::Source, "", window, cx);
        let sub = cx.subscribe_in(&place, window, |this, input, ev: &InputEvent, _, cx| {
            if matches!(ev, InputEvent::Focus | InputEvent::Change) {
                this.place_focus = Some(input.entity_id());
                cx.notify();
            }
        });
        let sub_kind = cx.observe(&kind, |_, _, cx| cx.notify());
        self.add_dialog = Some(AddDialog::Event(Box::new(EventDialog {
            owner,
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
        let owner = d.owner.clone();
        // Vérification avant la modification, pour garder la fenêtre ouverte en cas d'erreur.
        let mut probe = self.doc.as_ref().unwrap().ged.clone();
        if let Err(e) = probe.add_event(&owner, &ev) {
            d.error = Some(e.into());
            cx.notify();
            return;
        }
        self.add_dialog = None;
        let label = if ev.tag == "EVEN" { ev.detail.clone() } else { event_label(&ev.tag).unwrap_or("").to_string() };
        self.mutate(window, cx, move |g| {
            g.add_event(&owner, &ev)?;
            Ok((format!("{label} ajouté(e) à {}", g.record_label(&owner)), None))
        });
        window.focus(&self.focus, cx);
    }

    pub(super) fn render_add_dialog(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let g = &self.doc.as_ref()?.ged;
        let (title, body): (String, AnyElement) = match self.add_dialog.as_ref()? {
            AddDialog::Event(d) => (format!("Nouvel événement · {}", g.record_label(&d.owner)), self.event_body(d, cx)),
            AddDialog::Relatives { target } => match g.family(target) {
                Some(_) => (format!("Membres · {}", g.record_label(target)), self.family_body(target, cx)),
                None => (format!("Proches · {}", g.record_label(target)), self.relatives_body(target, cx)),
            },
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
                                .child(div().flex_1().font_weight(FontWeight::SEMIBOLD).child(title))
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
                h_flex().gap_2().justify_end().child(
                    Button::new("add-event-save")
                        .primary()
                        .small()
                        .icon(IconName::Check)
                        .label("Ajouter")
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
