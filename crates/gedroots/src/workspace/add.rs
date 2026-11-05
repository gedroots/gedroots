//! Add windows opened by the “+” of the detail panel: new event, and relatives (create or link a
//! person).

use super::relatives::PickPurpose;
use super::*;
use crate::date_field::{DateField, date_field};
use crate::editor::{PickerState, choice, choice_with_search, picker};
use crate::t;
use gpui_kit::component::input::TextareaState;

pub(super) enum AddDialog {
    Event(Box<EventDialog>),
    /// Relatives of an individual.
    Relatives {
        target: String,
    },
    WebLink(Box<super::weblinks::WebLinkDialog>),
    /// Research › Note a Search….
    Search(Box<super::research::SearchDialog>),
    Import(Box<super::import::ImportDialog>),
    Settings(Box<super::settings::SettingsDialog>),
    RenameMedia(Box<super::media_files::RenameDialog>),
    Gedzip(Box<super::export::GedzipReport>),
    Agent(Box<super::agent::AgentDialog>),
    Mobile(Box<super::mobile::MobileDialog>),
    /// File › Save Version…: the name of the version.
    SaveVersion(Entity<InputState>),
}

/// Action of a row of the relatives window.
type RowAction = Box<dyn Fn(&mut Workspace, &mut Window, &mut Context<Workspace>)>;

pub(super) struct EventDialog {
    owner: String,
    /// Edited event (position in the record); `None`: adding.
    editing: Option<usize>,
    kind: Entity<PickerState>,
    detail: Entity<InputState>,
    date: Entity<DateField>,
    place: Entity<InputState>,
    /// Street address within the place (`ADDR`).
    address: Entity<InputState>,
    source: Entity<PickerState>,
    /// First note of the event.
    note: Entity<TextareaState>,
    /// Adding from a married individual: union that will hold a family event (types prefixed with
    /// `FAM:`). `None` if the individual has only one union, or none.
    union: Option<Entity<PickerState>>,
    /// Unions of the individual (family, label).
    unions: Vec<(String, String)>,
    error: Option<SharedString>,
    _subs: Vec<Subscription>,
}

impl Workspace {
    /// Opens the “New Event” window for an individual or a family, or the edit window of event
    /// `editing`.
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
            .map(|t| {
                (
                    t.to_string(),
                    if *t == "EVEN" { t!("event_dialog.other").into_owned() } else { gedcom::event_name(t) },
                )
            })
            .collect();
        // Custom event types (File Settings › Events), before “Other…”.
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
        // Adding from an individual: the events of their unions (marriage…) too.
        let unions: Vec<(String, String)> = match (editing, g.individual(&owner)) {
            (None, Some(i)) => i
                .unions(g)
                .into_iter()
                .map(|(fam, spouse)| {
                    let label = match spouse {
                        Some(s) => t!("event_dialog.with", name = g.record_label(&s)).into_owned(),
                        None => t!("event_dialog.no_spouse").into_owned(),
                    };
                    (fam, label)
                })
                .collect(),
            _ => Vec::new(),
        };
        if !unions.is_empty() {
            // Marriage and co. right after birth and baptism.
            let at = options.iter().position(|(t, _)| t == "BAPM").map_or(0, |i| i + 1);
            options.splice(
                at..at,
                FAMILY_EVENTS
                    .iter()
                    .filter(|t| **t != "EVEN")
                    .map(|t| (format!("FAM:{t}"), gedcom::event_name(t)))
                    .chain(custom(true, "FAM:")),
            );
        }
        let union = (unions.len() > 1).then(|| {
            let opts: Vec<(&str, &str)> = unions.iter().map(|(f, l)| (f.as_str(), l.as_str())).collect();
            choice(&opts, &unions[0].0, window, cx)
        });
        let mut options: Vec<(&str, &str)> = options.iter().map(|(v, l)| (v.as_str(), l.as_str())).collect();
        // Type missing from the list offered when adding (CHR, ADOP…): keep it.
        let cur_tag = current.as_ref().map(|e| e.tag.clone()).unwrap_or_default();
        let cur_label = gedcom::event_name(&cur_tag);
        if !cur_tag.is_empty() && !tags.contains(&cur_tag.as_str()) {
            options.insert(0, (cur_tag.as_str(), cur_label.as_str()));
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
        let kind = choice_with_search(&options, default_kind, true, window, cx);
        let cur = current.clone().unwrap_or_default();
        let text = |ph: SharedString, value: String, window: &mut Window, cx: &mut Context<Self>| {
            cx.new(|cx| InputState::new(window, cx).placeholder(ph).default_value(value))
        };
        let detail = text("".into(), cur.detail, window, cx);
        let date = date_field(&cur.date, &cur.date_phrase, window, cx);
        let place = text(t!("event_dialog.town").into(), place_label(&cur.place), window, cx);
        let address = text(t!("event_dialog.address").into(), cur.address, window, cx);
        let source = picker(g, RecordKind::Source, cur.source.as_deref().unwrap_or(""), window, cx);
        let note = cx.new(|cx| TextareaState::new(window, cx).auto_grow(2, 8).default_value(cur.note));
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
            address,
            source,
            note,
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
        // Settings have no Save button: closing the window applies them.
        if matches!(self.add_dialog, Some(AddDialog::Settings(_))) {
            return self.save_settings(window, cx);
        }
        if let Some(AddDialog::Mobile(d)) = self.add_dialog.take() {
            Self::mobile_dialog_closed(&d);
        }
        self.new_place = None;
        window.focus(&self.focus, cx);
        cx.notify();
    }

    fn save_event(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(AddDialog::Event(d)) = &mut self.add_dialog else { return };
        let tag = d.kind.read(cx).selected_value().cloned().unwrap_or_default();
        if tag.is_empty() {
            d.error = Some(t!("event_dialog.choose_type").into());
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
            address: d.address.read(cx).value().to_string(),
            source: d.source.read(cx).selected_value().cloned(),
            note: d.note.read(cx).value().to_string(),
        };
        let (mut owner, editing) = (d.owner.clone(), d.editing);
        // Event of a union: held by the chosen family.
        let mut ev = ev;
        if let Some(t) = ev.tag.strip_prefix("FAM:").map(str::to_string) {
            let fam = match &d.union {
                Some(u) => u.read(cx).selected_value().cloned(),
                None => d.unions.first().map(|(f, _)| f.clone()),
            };
            let Some(fam) = fam else {
                d.error = Some(t!("event_dialog.choose_union").into());
                cx.notify();
                return;
            };
            ev.tag = t;
            owner = fam;
        }
        // Custom type: an `EVEN` whose `TYPE` is the label.
        if let Some(label) = ev.tag.strip_prefix("EVEN:").map(str::to_string) {
            ev.tag = "EVEN".into();
            ev.detail = label;
        }
        // Check before editing, to keep the window open on error.
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
            event_label(&ev.tag).unwrap_or_default().into_owned()
        };
        self.mutate(window, cx, move |g| {
            let who = g.record_label(&owner);
            match editing {
                Some(ix) => {
                    g.update_event(&owner, ix, &ev)?;
                    Ok((t!("event_dialog.edited", label = label, who = who).into_owned(), None))
                }
                None => {
                    g.add_event(&owner, &ev)?;
                    Ok((t!("event_dialog.added", label = label, who = who).into_owned(), None))
                }
            }
        });
        window.focus(&self.focus, cx);
    }

    /// Deletes an event (Undo to go back).
    pub(super) fn delete_event(&mut self, owner: String, ix: usize, window: &mut Window, cx: &mut Context<Self>) {
        if matches!(&self.add_dialog, Some(AddDialog::Event(d)) if d.owner == owner && d.editing == Some(ix)) {
            self.add_dialog = None;
        }
        self.mutate(window, cx, move |g| {
            let who = g.record_label(&owner);
            let label = g.delete_event(&owner, ix)?;
            Ok((t!("event_dialog.deleted", label = label, who = who).into_owned(), None))
        });
        window.focus(&self.focus, cx);
    }

    pub(super) fn render_add_dialog(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let g = &self.doc.as_ref()?.ged;
        let wide = matches!(self.add_dialog, Some(AddDialog::Agent(_) | AddDialog::Mobile(_)));
        let (title, body): (String, AnyElement) = match self.add_dialog.as_ref()? {
            AddDialog::Event(d) => {
                let what = match d.editing.and_then(|ix| g.event_title(&d.owner, ix)) {
                    Some(t) => t,
                    None => t!("event_dialog.new").into_owned(),
                };
                (format!("{what} · {}", g.record_label(&d.owner)), self.event_body(d, cx))
            }
            AddDialog::Relatives { target } => (
                t!("event_dialog.relatives_title", name = g.record_label(target)).into_owned(),
                self.relatives_body(target, cx),
            ),
            AddDialog::WebLink(d) => (self.weblink_dialog_title(d), self.weblink_body(d, cx)),
            AddDialog::Search(d) => (self.search_dialog_title(d), self.search_dialog_body(d, cx)),
            AddDialog::Import(d) => (self.import_title(d), self.import_body(d, cx)),
            AddDialog::Settings(d) => {
                (t!("cmd.file_settings").trim_end_matches('…').to_string(), self.settings_body(d, cx))
            }
            AddDialog::RenameMedia(d) => {
                (t!("cmd.rename_media_files").trim_end_matches('…').to_string(), self.rename_body(d, cx))
            }
            AddDialog::Gedzip(d) => (self.gedzip_title(d), self.gedzip_body(d, cx)),
            AddDialog::Agent(d) => (crate::t!("agent.title").into(), self.agent_body(d, cx)),
            AddDialog::Mobile(d) => (crate::t!("mobile.title").into(), self.mobile_body(d, cx)),
            AddDialog::SaveVersion(input) => {
                (t!("cmd.save_version").trim_end_matches('…').to_string(), self.save_version_body(input, cx))
            }
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
                        .w(px(if wide { super::agent::WIDTH } else { 520. }))
                        .rounded(theme::RADIUS_LG)
                        .border_1()
                        .border_color(theme::border_strong())
                        .bg(theme::panel())
                        .shadow_lg()
                        .on_key_down(cx.listener(|this, ev: &KeyDownEvent, w, cx| {
                            if ev.keystroke.key == "escape" {
                                cx.stop_propagation();
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
                        .child(div().when(!wide, |d| d.p_3()).child(body)),
                )
                .into_any_element(),
        )
    }

    fn event_body(&self, d: &EventDialog, cx: &mut Context<Self>) -> AnyElement {
        let tag = d.kind.read(cx).selected_value().cloned().unwrap_or_default();
        let detail_label = match tag.trim_start_matches("FAM:") {
            t if t.starts_with("EVEN:") => None,
            "OCCU" => Some(t!("editor.occupation")),
            "EDUC" => Some(t!("event_dialog.education")),
            "EVEN" => Some(t!("event_dialog.event_type")),
            "RESI" | "" => None,
            // The `TYPE` of a standard event is not entered here (GEDCOM editor); that of an edited
            // event is kept as is.
            _ => None,
        };
        let field = |label: std::borrow::Cow<'static, str>, el: AnyElement| {
            v_flex().gap_1().min_w_0().child(label_caps(label)).child(el)
        };
        v_flex()
            .gap_3()
            .child(
                h_flex()
                    .gap_3()
                    .child(
                        field(
                            t!("event_dialog.type"),
                            Select::new(&d.kind)
                                .small()
                                .placeholder(t!("event_dialog.choose"))
                                .search_placeholder(t!("event_dialog.search"))
                                .into_any_element(),
                        )
                        .flex_1(),
                    )
                    .when_some(detail_label, |row, label| {
                        row.child(field(label, Input::new(&d.detail).small().into_any_element()).flex_1())
                    }),
            )
            .when_some(d.union.as_ref().filter(|_| tag.starts_with("FAM:")), |v, u| {
                v.child(field(t!("event_dialog.union"), Select::new(u).small().into_any_element()))
            })
            .child(
                h_flex()
                    .gap_3()
                    .items_start()
                    .child(field(t!("event_dialog.date"), d.date.clone().into_any_element()).w(px(230.)).flex_none())
                    .child(
                        v_flex()
                            .flex_1()
                            .min_w_0()
                            .gap_3()
                            .child(
                                field(t!("editor.place"), Input::new(&d.place).small().into_any_element())
                                    .children(self.place_suggestions(&d.place, cx)),
                            )
                            .child(field(t!("editor.address"), Input::new(&d.address).small().into_any_element())),
                    ),
            )
            .child(field(
                gedcom::RecordKind::Source.label(),
                Select::new(&d.source)
                    .small()
                    .cleanable(true)
                    .placeholder(t!("event_dialog.none"))
                    .search_placeholder(t!("event_dialog.search"))
                    .into_any_element(),
            ))
            .child(field(t!("editor.note"), Textarea::new(&d.note).small().into_any_element()))
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
                                .label(t!("common.delete"))
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
                            .label(if d.editing.is_some() { t!("common.save") } else { t!("common.add") })
                            .on_click(cx.listener(|this, _, w, cx| this.save_event(w, cx))),
                    ),
            )
            .into_any_element()
    }

    /// A “Role: New · Existing…” row.
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
                        .label(t!("event_dialog.new_person"))
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
                    .label(t!("event_dialog.existing_person"))
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
        // Several unions: pick the child's (“Child with Marie”).
        let unions = g.individual(indi).map(|i| i.unions(g)).unwrap_or_default();
        let mut rows = Vec::new();
        for rel in Relation::ALL {
            match rel {
                Relation::Father if father.is_some() => continue,
                Relation::Mother if mother.is_some() => continue,
                Relation::Child if unions.len() > 1 => {
                    for (fam, spouse) in &unions {
                        let label = match spouse {
                            Some(s) => t!("event_dialog.child_with", name = g.record_label(s)).into_owned(),
                            None => t!("event_dialog.child_unknown").into_owned(),
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
                crate::i18n::relation(rel).to_string(),
                Some(Box::new(move |this, w, cx| this.add_relative(a.clone(), rel, w, cx))),
                Box::new(move |this, w, cx| this.open_picker(PickPurpose::Relative { indi: b.clone(), rel }, w, cx)),
                cx,
            ));
        }
        v_flex().children(rows).into_any_element()
    }
}

/// Discreet “+” button next to a section title.
pub(super) fn plus_button(id: &'static str) -> Button {
    Button::new(id).xsmall().ghost().icon(IconName::Plus)
}
