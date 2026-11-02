//! Réglages du fichier (Fichier › Réglages du fichier…), en trois pages :
//! - Général : auteur, droits, description, langue, individu racine et
//!   anomalies masquées (voir `gedcom::settings`) ;
//! - Lieux : format des lieux (`HEAD.PLAC.FORM`), avec réécriture des lieux ;
//! - Événements : types d'événements personnalisés (`EVEN` + `TYPE`).
//!
//! Tout est rangé dans le .ged et s'annule comme toute modification.

use super::add::{AddDialog, plus_button};
use super::*;
use crate::editor::{PickerState, choice, picker};
use gpui_kit::component::input::TextareaState;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum SettingsPage {
    General,
    Places,
    Events,
}

/// Une juridiction du format des lieux en cours de modification.
struct PlaceRow {
    name: Entity<InputState>,
    /// Position dans le format actuel (`None` : ajoutée).
    from: Option<usize>,
}

/// Un type d'événement personnalisé en cours de modification.
struct EventRow {
    label: Entity<InputState>,
    /// Libellé actuel (`None` : type ajouté).
    original: Option<String>,
    family: bool,
    uses: usize,
}

pub(super) struct SettingsDialog {
    page: SettingsPage,
    author: Entity<InputState>,
    email: Entity<InputState>,
    copyright: Entity<InputState>,
    description: Entity<TextareaState>,
    language: Entity<PickerState>,
    root: Entity<PickerState>,
    places: Vec<PlaceRow>,
    events: Vec<EventRow>,
    /// Types déclarés retirés de la liste.
    removed_events: Vec<(bool, String)>,
}

fn text_input(ph: &str, value: &str, window: &mut Window, cx: &mut Context<Workspace>) -> Entity<InputState> {
    let (ph, value) = (ph.to_string(), value.to_string());
    cx.new(|cx| InputState::new(window, cx).placeholder(ph).default_value(value))
}

impl Workspace {
    pub(super) fn open_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.open_settings_page(SettingsPage::General, window, cx);
    }

    pub(super) fn open_settings_page(&mut self, page: SettingsPage, window: &mut Window, cx: &mut Context<Self>) {
        let Some(doc) = &self.doc else { return };
        let g = doc.ged.clone();
        let s = g.file_settings();
        let author = text_input("Nom", &s.author, window, cx);
        let email = text_input("nom@exemple.fr", &s.email, window, cx);
        let copyright = text_input("© 2026 …", &s.copyright, window, cx);
        let description = cx.new(|cx| TextareaState::new(window, cx).rows(3).default_value(s.description));
        let mut langs: Vec<(&str, &str)> = gedcom::complete::LANGUAGES.to_vec();
        if !s.language.is_empty() && !langs.iter().any(|(c, _)| c.eq_ignore_ascii_case(&s.language)) {
            langs.push((s.language.as_str(), s.language.as_str()));
        }
        let language = choice(&langs, &s.language, window, cx);
        let root = picker(&g, RecordKind::Individual, g.root().as_deref().unwrap_or(""), window, cx);
        let places = g
            .place_form()
            .iter()
            .enumerate()
            .map(|(i, name)| PlaceRow { name: text_input("Commune", name, window, cx), from: Some(i) })
            .collect();
        let events = g
            .event_types()
            .into_iter()
            .map(|t| EventRow {
                label: text_input("Libellé", &t.label, window, cx),
                original: Some(t.label),
                family: t.family,
                uses: t.uses,
            })
            .collect();
        self.add_dialog = Some(AddDialog::Settings(Box::new(SettingsDialog {
            page,
            author,
            email,
            copyright,
            description,
            language,
            root,
            places,
            events,
            removed_events: Vec::new(),
        })));
        cx.notify();
    }

    fn settings_dialog(&mut self) -> Option<&mut SettingsDialog> {
        match &mut self.add_dialog {
            Some(AddDialog::Settings(d)) => Some(d),
            _ => None,
        }
    }

    fn save_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(AddDialog::Settings(d)) = &self.add_dialog else { return };
        let s = FileSettings {
            author: d.author.read(cx).value().to_string(),
            email: d.email.read(cx).value().to_string(),
            copyright: d.copyright.read(cx).value().to_string(),
            description: d.description.read(cx).value().to_string(),
            language: d.language.read(cx).selected_value().cloned().unwrap_or_default(),
        };
        let root = d.root.read(cx).selected_value().cloned();
        let format: Vec<Jurisdiction> = d
            .places
            .iter()
            .map(|r| Jurisdiction { name: r.name.read(cx).value().trim().to_string(), from: r.from })
            .filter(|j| !j.name.is_empty() || j.from.is_some())
            .collect();
        // (famille, ancien libellé, nouveau libellé)
        let events: Vec<(bool, Option<String>, String)> = d
            .events
            .iter()
            .map(|r| (r.family, r.original.clone(), r.label.read(cx).value().trim().to_string()))
            .collect();
        let removed = d.removed_events.clone();
        self.add_dialog = None;

        let g = &self.doc.as_ref().unwrap().ged;
        let mut probe = g.clone();
        let mut changed = probe.set_file_settings(&s);
        let root_changed = root.as_ref().is_some_and(|r| g.root().as_ref() != Some(r));
        let current = g.place_form();
        let format_changed = format.len() != current.len()
            || format.iter().enumerate().any(|(i, j)| j.from != Some(i) || j.name != current[i]);
        changed |= root_changed || format_changed || !removed.is_empty();
        changed |= events.iter().any(|(_, old, new)| old.as_deref() != Some(new.as_str()) && !new.is_empty());
        if changed {
            self.mutate(window, cx, move |g| {
                g.set_file_settings(&s);
                if let Some(r) = root.filter(|_| root_changed) {
                    g.set_root(&r);
                }
                let mut msg = String::from("Réglages du fichier enregistrés");
                if format_changed {
                    let r = g.set_place_format(&format);
                    if r.rewritten > 0 {
                        msg.push_str(&format!(" · {} réécrit(s)", places(r.rewritten)));
                    }
                    if r.skipped > 0 {
                        msg.push_str(&format!(" · {} d'un autre format inchangé(s)", places(r.skipped)));
                    }
                }
                for (family, label) in &removed {
                    g.undeclare_event_type(*family, label);
                }
                for (family, old, new) in &events {
                    match old {
                        Some(old) => {
                            g.rename_event_type(*family, old, new);
                        }
                        None => {
                            g.declare_event_type(*family, new);
                        }
                    }
                }
                Ok((msg, None))
            });
        }
        window.focus(&self.focus, cx);
        cx.notify();
    }

    fn unignore(&mut self, xref: String, code: String, window: &mut Window, cx: &mut Context<Self>) {
        self.mutate(window, cx, move |g| {
            let who = g.record_label(&xref);
            g.unignore_issue(&xref, &code);
            Ok((format!("Anomalie réaffichée pour {who}"), None))
        });
    }

    pub(super) fn settings_body(&self, d: &SettingsDialog, cx: &mut Context<Self>) -> AnyElement {
        let body = match d.page {
            SettingsPage::General => self.settings_general(d, cx),
            SettingsPage::Places => self.settings_places(d, cx),
            SettingsPage::Events => self.settings_events(d, cx),
        };
        let tab = |id: &'static str, label: &'static str, page: SettingsPage| {
            let active = d.page == page;
            Button::new(id).small().label(label).when(active, |b| b.primary()).when(!active, |b| b.ghost()).on_click(
                cx.listener(move |this, _, _, cx| {
                    if let Some(d) = this.settings_dialog() {
                        d.page = page;
                    }
                    cx.notify();
                }),
            )
        };
        v_flex()
            .gap_3()
            .child(
                h_flex()
                    .gap_1()
                    .child(tab("settings-general", "Général", SettingsPage::General))
                    .child(tab("settings-places", "Lieux", SettingsPage::Places))
                    .child(tab("settings-events", "Événements", SettingsPage::Events)),
            )
            .child(div().id("settings-body").max_h(px(460.)).overflow_y_scroll().child(body))
            .child(
                h_flex().gap_2().justify_end().child(
                    Button::new("settings-save")
                        .primary()
                        .small()
                        .icon(IconName::Check)
                        .label("Enregistrer")
                        .on_click(cx.listener(|this, _, w, cx| this.save_settings(w, cx))),
                ),
            )
            .into_any_element()
    }

    fn settings_general(&self, d: &SettingsDialog, cx: &mut Context<Self>) -> AnyElement {
        let g = &self.doc.as_ref().unwrap().ged;
        let info = g.file_info();
        let ignored = g.ignored_issues();

        // Informations : « GEDCOM 7.0.13 · Ancestris 13, le 1 novembre 2026 ».
        let mut about = format!("GEDCOM {}", info.version);
        if !info.software.is_empty() {
            about.push_str(&format!(" · {}", info.software));
        }
        if let Some((date, _)) = &info.saved {
            about.push_str(&format!(", le {}", Date::parse(date).to_french()));
        }
        let counts = [
            (RecordKind::Individual, "individu", "individus"),
            (RecordKind::Family, "famille", "familles"),
            (RecordKind::Source, "source", "sources"),
            (RecordKind::Media, "média", "médias"),
        ]
        .iter()
        .map(|(k, one, many)| {
            let n = g.count(*k);
            format!("{n} {}", if n == 1 { one } else { many })
        })
        .collect::<Vec<_>>()
        .join(" · ");

        v_flex()
            .gap_3()
            .child(v_flex().gap_0p5().text_sm().child(about).child(div().text_color(theme::muted()).child(counts)))
            .child(
                h_flex()
                    .gap_3()
                    .child(field("Auteur", Input::new(&d.author).small().into_any_element()).flex_1())
                    .child(field("E-mail", Input::new(&d.email).small().into_any_element()).flex_1()),
            )
            .child(
                h_flex()
                    .gap_3()
                    .child(field("Droits", Input::new(&d.copyright).small().into_any_element()).flex_1())
                    .child(
                        field("Langue", Select::new(&d.language).small().placeholder("Choisir…").into_any_element())
                            .w(px(160.))
                            .flex_none(),
                    ),
            )
            .child(field("Description", Textarea::new(&d.description).h(px(72.)).into_any_element()))
            .child(field(
                "Individu racine",
                Select::new(&d.root).small().search_placeholder("Rechercher…").into_any_element(),
            ))
            .when(!ignored.is_empty(), |v| {
                v.child(field(
                    "Anomalies masquées",
                    v_flex()
                        .children(ignored.into_iter().enumerate().map(|(i, it)| {
                            let who = g.record_label(&it.xref);
                            h_flex()
                                .gap_2()
                                .items_center()
                                .text_sm()
                                .child(div().flex_1().min_w_0().truncate().child(format!("{} · {who}", it.label)))
                                .child(
                                    Button::new(SharedString::from(format!("unignore-{i}")))
                                        .xsmall()
                                        .ghost()
                                        .label("Réafficher")
                                        .on_click(cx.listener(move |this, _, w, cx| {
                                            this.unignore(it.xref.clone(), it.code.clone(), w, cx)
                                        })),
                                )
                        }))
                        .into_any_element(),
                ))
            })
            .into_any_element()
    }

    fn settings_places(&self, d: &SettingsDialog, cx: &mut Context<Self>) -> AnyElement {
        let g = &self.doc.as_ref().unwrap().ged;
        let n = d.places.len();
        // Aperçu : un lieu du fichier dans le nouveau format.
        let old_len = g.place_form().len();
        let sample = g.places().into_iter().map(|p| p.name).find(|name| {
            let parts: Vec<&str> = name.split(',').map(str::trim).collect();
            parts.len() == old_len && parts.iter().filter(|p| !p.is_empty()).count() >= 2
        });
        let preview = sample.map(|name| {
            let parts: Vec<&str> = name.split(',').map(str::trim).collect();
            d.places
                .iter()
                .filter_map(|r| r.from.and_then(|k| parts.get(k).copied()))
                .filter(|p| !p.is_empty())
                .collect::<Vec<_>>()
                .join(", ")
        });
        let rows = d.places.iter().enumerate().map(|(i, r)| {
            let id = |what: &str| SharedString::from(format!("plac-{what}-{i}"));
            h_flex()
                .gap_1()
                .items_center()
                .child(div().flex_1().child(Input::new(&r.name).small()))
                .child(Button::new(id("up")).xsmall().ghost().icon(IconName::ChevronUp).disabled(i == 0).on_click(
                    cx.listener(move |this, _, _, cx| {
                        if let Some(d) = this.settings_dialog() {
                            d.places.swap(i, i - 1);
                        }
                        cx.notify();
                    }),
                ))
                .child(
                    Button::new(id("down")).xsmall().ghost().icon(IconName::ChevronDown).disabled(i + 1 == n).on_click(
                        cx.listener(move |this, _, _, cx| {
                            if let Some(d) = this.settings_dialog() {
                                d.places.swap(i, i + 1);
                            }
                            cx.notify();
                        }),
                    ),
                )
                .child(Button::new(id("del")).xsmall().ghost().icon(IconName::Close).on_click(cx.listener(
                    move |this, _, _, cx| {
                        if let Some(d) = this.settings_dialog() {
                            d.places.remove(i);
                        }
                        cx.notify();
                    },
                )))
        });
        v_flex()
            .gap_2()
            .child(section_header(
                "Format des lieux",
                plus_button("plac-add").on_click(cx.listener(|this, _, w, cx| {
                    let name = text_input("Juridiction", "", w, cx);
                    name.update(cx, |s, cx| s.focus(w, cx));
                    if let Some(d) = this.settings_dialog() {
                        d.places.push(PlaceRow { name, from: None });
                    }
                    cx.notify();
                })),
            ))
            .children(rows)
            .when_some(preview, |v, p| {
                v.child(div().pt_1().text_sm().text_color(theme::muted()).child(format!("Exemple : {p}")))
            })
            .into_any_element()
    }

    fn settings_events(&self, d: &SettingsDialog, cx: &mut Context<Self>) -> AnyElement {
        let section = |family: bool, title: &'static str, cx: &mut Context<Self>| {
            let rows = d.events.iter().enumerate().filter(|(_, r)| r.family == family).map(|(i, r)| {
                let removable = r.uses == 0;
                h_flex()
                    .gap_2()
                    .items_center()
                    .child(div().flex_1().child(Input::new(&r.label).small()))
                    .when(r.uses > 0, |h| {
                        h.child(
                            div()
                                .w(px(28.))
                                .text_xs()
                                .text_color(theme::muted())
                                .text_right()
                                .child(r.uses.to_string()),
                        )
                    })
                    .when(removable, |h| {
                        h.child(
                            Button::new(SharedString::from(format!("even-del-{i}")))
                                .xsmall()
                                .ghost()
                                .icon(IconName::Close)
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    if let Some(d) = this.settings_dialog() {
                                        let r = d.events.remove(i);
                                        if let Some(label) = r.original {
                                            d.removed_events.push((r.family, label));
                                        }
                                    }
                                    cx.notify();
                                })),
                        )
                    })
            });
            let rows: Vec<_> = rows.collect();
            v_flex()
                .gap_2()
                .child(section_header(
                    title,
                    plus_button(if family { "even-add-fam" } else { "even-add-indi" }).on_click(cx.listener(
                        move |this, _, w, cx| {
                            let label = text_input("Libellé", "", w, cx);
                            label.update(cx, |s, cx| s.focus(w, cx));
                            if let Some(d) = this.settings_dialog() {
                                d.events.push(EventRow { label, original: None, family, uses: 0 });
                            }
                            cx.notify();
                        },
                    )),
                ))
                .children(rows)
        };
        v_flex()
            .gap_4()
            .child(section(false, "Événements individuels", cx))
            .child(section(true, "Événements familiaux", cx))
            .into_any_element()
    }
}

fn field(label: &'static str, el: AnyElement) -> Div {
    v_flex().gap_1().min_w_0().child(label_caps(label)).child(el)
}

fn places(n: usize) -> String {
    if n == 1 { "1 lieu".into() } else { format!("{n} lieux") }
}
