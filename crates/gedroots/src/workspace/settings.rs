//! Réglages du fichier (Fichier › Réglages du fichier…) : auteur, droits,
//! description, langue, individu racine et anomalies masquées, rangés dans
//! l'en-tête du .ged (voir `gedcom::settings`). Annulable comme toute modification.

use super::add::AddDialog;
use super::*;
use crate::editor::{PickerState, choice, picker};
use gpui_kit::component::input::TextareaState;

pub(super) struct SettingsDialog {
    author: Entity<InputState>,
    email: Entity<InputState>,
    copyright: Entity<InputState>,
    description: Entity<TextareaState>,
    language: Entity<PickerState>,
    root: Entity<PickerState>,
}

impl Workspace {
    pub(super) fn open_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(doc) = &self.doc else { return };
        let g = &doc.ged;
        let s = g.file_settings();
        let text = |ph: &'static str, value: String, window: &mut Window, cx: &mut Context<Self>| {
            cx.new(|cx| InputState::new(window, cx).placeholder(ph).default_value(value))
        };
        let author = text("Nom", s.author, window, cx);
        let email = text("nom@exemple.fr", s.email, window, cx);
        let copyright = text("© 2026 …", s.copyright, window, cx);
        let description = cx.new(|cx| TextareaState::new(window, cx).rows(3).default_value(s.description));
        let mut langs: Vec<(&str, &str)> = gedcom::complete::LANGUAGES.to_vec();
        if !s.language.is_empty() && !langs.iter().any(|(c, _)| c.eq_ignore_ascii_case(&s.language)) {
            langs.push((s.language.as_str(), s.language.as_str()));
        }
        let language = choice(&langs, &s.language, window, cx);
        let root = picker(g, RecordKind::Individual, g.root().as_deref().unwrap_or(""), window, cx);
        self.add_dialog = Some(AddDialog::Settings(Box::new(SettingsDialog {
            author,
            email,
            copyright,
            description,
            language,
            root,
        })));
        cx.notify();
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
        self.add_dialog = None;
        let g = &self.doc.as_ref().unwrap().ged;
        let mut probe = g.clone();
        let changed = probe.set_file_settings(&s);
        let root_changed = root.as_ref().is_some_and(|r| g.root().as_ref() != Some(r));
        if changed || root_changed {
            self.mutate(window, cx, move |g| {
                g.set_file_settings(&s);
                if let Some(r) = root.filter(|_| root_changed) {
                    g.set_root(&r);
                }
                Ok(("Réglages du fichier enregistrés".into(), None))
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
        let g = &self.doc.as_ref().unwrap().ged;
        let info = g.file_info();
        let ignored = g.ignored_issues();
        let field = |label: &'static str, el: AnyElement| v_flex().gap_1().min_w_0().child(label_caps(label)).child(el);

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
                        .id("settings-ignored")
                        .max_h(px(140.))
                        .overflow_y_scroll()
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
}
