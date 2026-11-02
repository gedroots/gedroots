//! Fenêtres d'ajout ouvertes par les « + » du panneau de détail :
//! nouvel événement, et proches (créer ou lier une personne).

use super::relatives::PickPurpose;
use super::*;
use crate::date_field::{DateField, date_field};
use crate::editor::{PickerState, choice, picker};

pub(super) enum AddDialog {
    Event(Box<EventDialog>),
    /// Proches d'un individu.
    Relatives {
        target: String,
    },
    WebLink(Box<super::weblinks::WebLinkDialog>),
    Import(Box<super::import::ImportDialog>),
    Settings(Box<super::settings::SettingsDialog>),
    RenameMedia(Box<super::media_files::RenameDialog>),
    Gedzip(Box<super::export::GedzipReport>),
}

/// Action d'une ligne de la fenêtre des proches.
type RowAction = Box<dyn Fn(&mut Workspace, &mut Window, &mut Context<Workspace>)>;

pub(super) struct EventDialog {
    owner: String,
    /// Événement modifié (position dans l'enregistrement) ; `None` : ajout.
    editing: Option<usize>,
    kind: Entity<PickerState>,
    detail: Entity<InputState>,
    date: Entity<DateField>,
    place: Entity<InputState>,
    source: Entity<PickerState>,
    /// Ajout depuis un individu marié : union qui portera un événement familial
    /// (types préfixés `FAM:`). `None` si l'individu n'a qu'une union, ou aucune.
    union: Option<Entity<PickerState>>,
    /// Unions de l'individu (famille, libellé).
    unions: Vec<(String, String)>,
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
        let mut options: Vec<(String, String)> = tags
            .iter()
            .map(|t| (t.to_string(), if *t == "EVEN" { "Autre…" } else { event_label(t).unwrap_or(t) }.to_string()))
            .collect();
        // Types d'événements personnalisés (Réglages du fichier › Événements), avant « Autre… ».
        let types = g.event_types();
        let custom = |fam: bool, prefix: &str| -> Vec<(String, String)> {
            types
                .iter()
                .filter(|t| t.family == fam)
                .map(|t| (format!("{prefix}EVEN:{}", t.ty), t.label.clone()))
                .collect()
        };
        let at = options.iter().position(|(t, _)| t == "EVEN").unwrap_or(options.len());
        options.splice(at..at, custom(family, ""));
        // Ajout depuis un individu : les événements de ses unions (mariage…) aussi.
        let unions: Vec<(String, String)> = match (editing, g.individual(&owner)) {
            (None, Some(i)) => i
                .unions(g)
                .into_iter()
                .map(|(fam, spouse)| {
                    let label = match spouse {
                        Some(s) => format!("Avec {}", g.record_label(&s)),
                        None => "Union sans conjoint connu".into(),
                    };
                    (fam, label)
                })
                .collect(),
            _ => Vec::new(),
        };
        if !unions.is_empty() {
            // Mariage et compagnie juste après la naissance et le baptême.
            let at = options.iter().position(|(t, _)| t == "BAPM").map_or(0, |i| i + 1);
            options.splice(
                at..at,
                FAMILY_EVENTS
                    .iter()
                    .filter(|t| **t != "EVEN")
                    .map(|t| (format!("FAM:{t}"), event_label(t).unwrap_or(t).to_string()))
                    .chain(custom(true, "FAM:")),
            );
        }
        let union = (unions.len() > 1).then(|| {
            let opts: Vec<(&str, &str)> = unions.iter().map(|(f, l)| (f.as_str(), l.as_str())).collect();
            choice(&opts, &unions[0].0, window, cx)
        });
        let mut options: Vec<(&str, &str)> = options.iter().map(|(v, l)| (v.as_str(), l.as_str())).collect();
        // Type absent de la liste proposée à l'ajout (CHR, ADOP…) : on le garde.
        let cur_tag = current.as_ref().map(|e| e.tag.clone()).unwrap_or_default();
        if !cur_tag.is_empty() && !tags.contains(&cur_tag.as_str()) {
            options.insert(0, (cur_tag.as_str(), event_label(&cur_tag).unwrap_or(cur_tag.as_str())));
        }
        let custom_kind = current
            .as_ref()
            .filter(|e| e.tag == "EVEN" && types.iter().any(|t| t.family == family && t.ty == e.detail.trim()))
            .map(|e| format!("EVEN:{}", e.detail.trim()));
        let default_kind = match &current {
            _ if custom_kind.is_some() => custom_kind.as_deref().unwrap_or_default(),
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
        let date = date_field(&cur.date, &cur.date_phrase, window, cx);
        let place = text("Commune…", place_label(&cur.place), window, cx);
        let source = picker(g, RecordKind::Source, cur.source.as_deref().unwrap_or(""), window, cx);
        let sub =
            cx.subscribe_in(&place, window, |this, input, ev: &InputEvent, _, cx| this.on_place_input(input, ev, cx));
        let sub_kind = cx.observe(&kind, |_, _, cx| cx.notify());
        self.add_dialog = Some(AddDialog::Event(Box::new(EventDialog {
            owner,
            editing,
            kind,
            detail,
            date,
            place,
            source,
            union,
            unions,
            error: None,
            _subs: vec![sub, sub_kind],
        })));
        cx.notify();
    }

    pub(super) fn open_relatives_dialog(&mut self, target: String, cx: &mut Context<Self>) {
        self.add_dialog = Some(AddDialog::Relatives { target });
        cx.notify();
    }

    pub(super) fn close_add_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.add_dialog = None;
        self.new_place = None;
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
        let (date, date_phrase) = match d.date.read(cx).value(cx) {
            Ok(v) => v,
            Err(e) => {
                d.error = Some(e.into());
                cx.notify();
                return;
            }
        };
        let ev = NewEvent {
            tag,
            detail: d.detail.read(cx).value().to_string(),
            date,
            date_phrase,
            place: places::place_value(
                self.doc.as_ref().map(|d| &d.ged),
                &self.pending_places,
                &d.place.read(cx).value(),
            ),
            source: d.source.read(cx).selected_value().cloned(),
        };
        let (mut owner, editing) = (d.owner.clone(), d.editing);
        // Événement d'une union : porté par la famille choisie.
        let mut ev = ev;
        if let Some(t) = ev.tag.strip_prefix("FAM:").map(str::to_string) {
            let fam = match &d.union {
                Some(u) => u.read(cx).selected_value().cloned(),
                None => d.unions.first().map(|(f, _)| f.clone()),
            };
            let Some(fam) = fam else {
                d.error = Some("Choisissez l'union".into());
                cx.notify();
                return;
            };
            ev.tag = t;
            owner = fam;
        }
        // Type personnalisé : un `EVEN` dont le `TYPE` est le libellé.
        if let Some(label) = ev.tag.strip_prefix("EVEN:").map(str::to_string) {
            ev.tag = "EVEN".into();
            ev.detail = label;
        }
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
        let label = if ev.tag == "EVEN" {
            gedcom::event_type_label(&ev.detail)
        } else {
            event_label(&ev.tag).unwrap_or("").to_string()
        };
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
            AddDialog::Relatives { target } => {
                (format!("Proches · {}", g.record_label(target)), self.relatives_body(target, cx))
            }
            AddDialog::WebLink(d) => (self.weblink_dialog_title(d), self.weblink_body(d, cx)),
            AddDialog::Import(d) => (self.import_title(d), self.import_body(d, cx)),
            AddDialog::Settings(d) => ("Réglages du fichier".into(), self.settings_body(d, cx)),
            AddDialog::RenameMedia(d) => ("Renommer selon le modèle".into(), self.rename_body(d, cx)),
            AddDialog::Gedzip(d) => (self.gedzip_title(d), self.gedzip_body(d, cx)),
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
                        .id("add-dialog")
                        .occlude()
                        .w(px(520.))
                        .rounded(theme::RADIUS_LG)
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
        let detail_label = match tag.trim_start_matches("FAM:") {
            t if t.starts_with("EVEN:") => None,
            "OCCU" => Some("Profession"),
            "EDUC" => Some("Établissement ou diplôme"),
            "EVEN" => Some("Type d'événement"),
            "RESI" | "" => None,
            // Le `TYPE` d'un événement standard ne se saisit pas ici (éditeur GEDCOM) ;
            // celui d'un événement modifié est gardé tel quel.
            _ => None,
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
            .when_some(d.union.as_ref().filter(|_| tag.starts_with("FAM:")), |v, u| {
                v.child(field("Union", Select::new(u).small().into_any_element()))
            })
            .child(
                h_flex()
                    .gap_3()
                    .items_start()
                    .child(field("Date", d.date.clone().into_any_element()).w(px(230.)).flex_none())
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
        label: String,
        new: Option<RowAction>,
        existing: RowAction,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let existing = Rc::new(existing);
        h_flex()
            .gap_2()
            .items_center()
            .py_1()
            .child(div().flex_1().min_w_0().overflow_hidden().whitespace_nowrap().text_ellipsis().child(label.clone()))
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
        // Plusieurs unions : on choisit celle de l'enfant (« Enfant avec Marie »).
        let unions = g.individual(indi).map(|i| i.unions(g)).unwrap_or_default();
        let mut rows = Vec::new();
        for rel in Relation::ALL {
            match rel {
                Relation::Father if father.is_some() => continue,
                Relation::Mother if mother.is_some() => continue,
                Relation::Child if unions.len() > 1 => {
                    for (fam, spouse) in &unions {
                        let label = match spouse {
                            Some(s) => format!("Enfant avec {}", g.record_label(s)),
                            None => "Enfant (conjoint inconnu)".into(),
                        };
                        let (a, b) = (fam.clone(), fam.clone());
                        rows.push(self.relative_row(
                            label,
                            Some(Box::new(move |this, w, cx| this.add_family_child(a.clone(), w, cx))),
                            Box::new(move |this, w, cx| {
                                this.open_picker(PickPurpose::FamilyChild { fam: b.clone() }, w, cx)
                            }),
                            cx,
                        ));
                    }
                    continue;
                }
                _ => {}
            }
            let (a, b) = (indi.to_string(), indi.to_string());
            rows.push(self.relative_row(
                rel.label_fr().to_string(),
                Some(Box::new(move |this, w, cx| this.add_relative(a.clone(), rel, w, cx))),
                Box::new(move |this, w, cx| this.open_picker(PickPurpose::Relative { indi: b.clone(), rel }, w, cx)),
                cx,
            ));
        }
        v_flex().children(rows).into_any_element()
    }
}

/// Bouton « + » discret à côté d'un titre de section.
pub(super) fn plus_button(id: &'static str) -> Button {
    Button::new(id).xsmall().ghost().icon(IconName::Plus)
}
