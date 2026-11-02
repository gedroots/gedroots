//! Réglages du fichier (Fichier › Réglages du fichier…), en cinq pages :
//! - Général : auteur, droits, description, langue et individu racine
//!   (voir `gedcom::settings`) ;
//! - Lieux : format des lieux (`HEAD.PLAC.FORM`), avec réécriture des lieux ;
//! - Événements : types d'événements personnalisés (`EVEN` + `TYPE`), avec
//!   leur libellé affiché ;
//! - Médias : dossier des médias, sous-dossiers par type et modèles de nom
//!   (`gedcom::media_naming`) ;
//! - Recherche : sites d'Individu › Rechercher sur, avec leur modèle d'URL
//!   (`gedcom::websearch`).
//!
//! Tout est rangé dans le .ged et s'annule comme toute modification.

use super::add::{AddDialog, plus_button};
use super::*;
use crate::editor::{PickerState, choice, picker};
use gpui_kit::component::input::TextareaState;
use gpui_kit::component::menu::{DropdownMenu, PopupMenuItem};
use rust_i18n::t;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum SettingsPage {
    General,
    Places,
    Events,
    Media,
    Search,
}

/// Un site de recherche en cours de modification.
struct SiteRow {
    name: Entity<InputState>,
    url: Entity<InputState>,
}

impl SiteRow {
    fn new(site: &SearchSite, window: &mut Window, cx: &mut Context<Workspace>) -> (Self, Vec<Subscription>) {
        let row = SiteRow {
            name: text_input(&t!("settings.site_name"), &site.name, window, cx),
            url: text_input(&t!("settings.site_url"), &site.url, window, cx),
        };
        let subs = vec![refresh_on_change(&row.url, cx)];
        (row, subs)
    }
}

/// Une juridiction du format des lieux en cours de modification.
struct PlaceRow {
    name: Entity<InputState>,
    /// Position dans le format actuel (`None` : ajoutée).
    from: Option<usize>,
}

/// Un type d'événement personnalisé en cours de modification.
struct EventRow {
    /// Libellé affiché (vide : le `TYPE`).
    label: Entity<InputState>,
    /// Valeur du `TYPE`.
    ty: Entity<InputState>,
    /// `TYPE` actuel (`None` : type ajouté).
    original: Option<String>,
    /// Libellé actuel (vide : le `TYPE`).
    original_label: String,
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
    media_root: Entity<InputState>,
    media: Vec<MediaRow>,
    /// Dossier dont le modèle reçoit les variables cliquées.
    media_active: usize,
    /// Contexte de chaque média du fichier (calculé à l'ouverture), pour les exemples.
    media_samples: Vec<(MediaContext, String)>,
    sites: Vec<SiteRow>,
    /// Site dont le modèle reçoit les variables cliquées.
    sites_active: usize,
    _subs: Vec<Subscription>,
}

/// Un sous-dossier des médias en cours de modification.
struct MediaRow {
    dir: Entity<InputState>,
    /// Ce qu'il reçoit (vide : tout le reste), choisi dans un menu.
    types: Vec<String>,
    template: Entity<InputState>,
}

impl MediaRow {
    fn new(f: &MediaFolder, window: &mut Window, cx: &mut Context<Workspace>) -> (Self, Vec<Subscription>) {
        let row = MediaRow {
            dir: text_input(&t!("settings.media_dir_placeholder"), &f.dir, window, cx),
            types: f.types.clone(),
            template: text_input("{year} {surname} {given}", &f.template, window, cx),
        };
        let subs = vec![refresh_on_change(&row.dir, cx), refresh_on_change(&row.template, cx)];
        (row, subs)
    }
}

/// Redessine le panneau pendant la saisie (exemples).
fn refresh_on_change(e: &Entity<InputState>, cx: &mut Context<Workspace>) -> Subscription {
    let input = e.clone();
    cx.subscribe(e, move |this, _, ev: &InputEvent, cx| match ev {
        InputEvent::Change => cx.notify(),
        // Les variables cliquées vont dans le dernier modèle touché.
        InputEvent::Focus => {
            if let Some(d) = this.settings_dialog() {
                if let Some(i) = d.media.iter().position(|r| r.template == input) {
                    d.media_active = i;
                }
                if let Some(i) = d.sites.iter().position(|r| r.url == input) {
                    d.sites_active = i;
                }
            }
        }
        _ => {}
    })
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
        let author = text_input(&t!("editor.name"), &s.author, window, cx);
        let email = text_input(&t!("settings.email_placeholder"), &s.email, window, cx);
        let copyright = text_input("© 2026 …", &s.copyright, window, cx);
        let description = cx.new(|cx| TextareaState::new(window, cx).rows(3).default_value(s.description));
        let languages = gedcom::complete::languages();
        let mut langs: Vec<(&str, &str)> = languages.iter().map(|(c, l)| (*c, l.as_ref())).collect();
        if !s.language.is_empty() && !langs.iter().any(|(c, _)| c.eq_ignore_ascii_case(&s.language)) {
            langs.push((s.language.as_str(), s.language.as_str()));
        }
        let language = choice(&langs, &s.language, window, cx);
        let root = picker(&g, RecordKind::Individual, g.root().as_deref().unwrap_or(""), window, cx);
        let places = g
            .place_form()
            .iter()
            .enumerate()
            .map(|(i, name)| PlaceRow {
                name: text_input(&t!("settings.municipality"), name, window, cx),
                from: Some(i),
            })
            .collect();
        let events = g
            .event_types()
            .into_iter()
            .map(|t| {
                let label = if t.label == t.ty { String::new() } else { t.label };
                EventRow {
                    label: text_input(&t.ty, &label, window, cx),
                    ty: text_input("TYPE", &t.ty, window, cx),
                    original: Some(t.ty),
                    original_label: label,
                    family: t.family,
                    uses: t.uses,
                }
            })
            .collect();
        let naming = g.media_naming();
        let media_root = text_input(&t!("cmd.media_list"), &naming.root, window, cx);
        let mut subs = vec![refresh_on_change(&media_root, cx)];
        let media = naming
            .folders
            .iter()
            .map(|f| {
                let (row, s) = MediaRow::new(f, window, cx);
                subs.extend(s);
                row
            })
            .collect();
        let numbers = g.sosa_numbers();
        let media_samples = g
            .records_of(RecordKind::Media)
            .filter_map(|m| {
                let x = m.xref.as_deref()?;
                let file = m.children_with("FILE").map(|f| f.val()).find(|f| !gedcom::weblink::is_web_url(f))?;
                Some((g.media_context_with(x, &numbers, Some(&naming)), file.to_string()))
            })
            .collect();
        let sites = g
            .search_sites()
            .iter()
            .map(|site| {
                let (row, s) = SiteRow::new(site, window, cx);
                subs.extend(s);
                row
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
            media_root,
            media,
            media_active: 0,
            media_samples,
            sites,
            sites_active: 0,
            _subs: subs,
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
        // (famille, ancien TYPE, nouveau TYPE, libellé s'il change)
        let events: Vec<(bool, Option<String>, String, Option<String>)> = d
            .events
            .iter()
            .map(|r| {
                let label = r.label.read(cx).value().trim().to_string();
                let ty = r.ty.read(cx).value().trim().to_string();
                // Nouveau type sans TYPE : le libellé sert de TYPE.
                if r.original.is_none() && ty.is_empty() {
                    return (r.family, None, label, None);
                }
                let label = (label != r.original_label).then_some(label);
                (r.family, r.original.clone(), ty, label)
            })
            .collect();
        let removed = d.removed_events.clone();
        let naming = media_naming(d, cx);
        let sites = search_sites(d, cx);
        self.add_dialog = None;

        let g = &self.doc.as_ref().unwrap().ged;
        let mut probe = g.clone();
        let mut changed = probe.set_file_settings(&s);
        changed |= probe.set_media_naming(&naming);
        changed |= probe.set_search_sites(&sites);
        let root_changed = root.as_ref().is_some_and(|r| g.root().as_ref() != Some(r));
        let current = g.place_form();
        let format_changed = format.len() != current.len()
            || format.iter().enumerate().any(|(i, j)| j.from != Some(i) || j.name != current[i]);
        changed |= root_changed || format_changed || !removed.is_empty();
        changed |= events
            .iter()
            .any(|(_, old, new, label)| !new.is_empty() && (old.as_deref() != Some(new.as_str()) || label.is_some()));
        if changed {
            self.mutate(window, cx, move |g| {
                g.set_file_settings(&s);
                g.set_media_naming(&naming);
                g.set_search_sites(&sites);
                if let Some(r) = root.filter(|_| root_changed) {
                    g.set_root(&r);
                }
                let mut msg = t!("settings.saved").into_owned();
                if format_changed {
                    let r = g.set_place_format(&format);
                    if r.rewritten > 0 {
                        msg.push_str(&t!("settings.rewritten", places = places(r.rewritten)));
                    }
                    if r.skipped > 0 {
                        msg.push_str(&t!("settings.skipped", places = places(r.skipped)));
                    }
                }
                for (family, label) in &removed {
                    g.undeclare_event_type(*family, label);
                }
                for (family, old, new, label) in events.iter().filter(|e| !e.2.is_empty()) {
                    match old {
                        Some(old) => {
                            g.rename_event_type(*family, old, new);
                        }
                        None => {
                            g.declare_event_type(*family, new);
                        }
                    }
                    if let Some(label) = label {
                        g.set_event_type_label(*family, new, label);
                    }
                }
                Ok((msg, None))
            });
            // Rangement modifié depuis l'aperçu d'import : les noms prévus suivent.
            self.refresh_import_plan(cx);
        }
        window.focus(&self.focus, cx);
        cx.notify();
    }

    pub(super) fn settings_body(&self, d: &SettingsDialog, cx: &mut Context<Self>) -> AnyElement {
        let body = match d.page {
            SettingsPage::General => self.settings_general(d),
            SettingsPage::Places => self.settings_places(d, cx),
            SettingsPage::Events => self.settings_events(d, cx),
            SettingsPage::Media => self.settings_media(d, cx),
            SettingsPage::Search => self.settings_search(d, cx),
        };
        let tab = |id: &'static str, label: std::borrow::Cow<'static, str>, page: SettingsPage| {
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
                    .child(tab("settings-general", t!("settings.general"), SettingsPage::General))
                    .child(tab("settings-places", t!("cmd.places"), SettingsPage::Places))
                    .child(tab("settings-events", t!("settings.events"), SettingsPage::Events))
                    .child(tab("settings-media", t!("cmd.media_list"), SettingsPage::Media))
                    .child(tab("settings-search", t!("settings.search"), SettingsPage::Search)),
            )
            .child(theme::scroll_y("settings-body").max_h(px(460.)).child(body))
            .child(
                h_flex().gap_2().justify_end().child(
                    Button::new("settings-save")
                        .primary()
                        .small()
                        .icon(IconName::Check)
                        .label(t!("common.save"))
                        .on_click(cx.listener(|this, _, w, cx| this.save_settings(w, cx))),
                ),
            )
            .into_any_element()
    }

    fn settings_general(&self, d: &SettingsDialog) -> AnyElement {
        let g = &self.doc.as_ref().unwrap().ged;
        let info = g.file_info();

        // Informations : « GEDCOM 7.0.13 · Ancestris 13, le 1 novembre 2026 ».
        let mut about = format!("GEDCOM {}", info.version);
        if !info.software.is_empty() {
            about.push_str(&format!(" · {}", info.software));
        }
        if let Some((date, _)) = &info.saved {
            about.push_str(&t!("settings.saved_on", date = Date::parse(date).display()));
        }
        let counts = [RecordKind::Individual, RecordKind::Family, RecordKind::Source, RecordKind::Media]
            .iter()
            .map(|k| {
                let n = g.count(*k);
                match (k, n == 1) {
                    (RecordKind::Individual, true) => t!("settings.count_individuals.one", n = n),
                    (RecordKind::Individual, false) => t!("settings.count_individuals.other", n = n),
                    (RecordKind::Family, true) => t!("settings.count_families.one", n = n),
                    (RecordKind::Family, false) => t!("settings.count_families.other", n = n),
                    (RecordKind::Source, true) => t!("settings.count_sources.one", n = n),
                    (RecordKind::Source, false) => t!("settings.count_sources.other", n = n),
                    (_, true) => t!("settings.count_media.one", n = n),
                    (_, false) => t!("settings.count_media.other", n = n),
                }
            })
            .collect::<Vec<_>>()
            .join(" · ");

        v_flex()
            .gap_3()
            .child(v_flex().gap_0p5().text_sm().child(about).child(div().text_color(theme::muted()).child(counts)))
            .child(
                h_flex()
                    .gap_3()
                    .child(field(t!("settings.author"), Input::new(&d.author).small().into_any_element()).flex_1())
                    .child(field(t!("settings.email"), Input::new(&d.email).small().into_any_element()).flex_1()),
            )
            .child(
                h_flex()
                    .gap_3()
                    .child(
                        field(t!("settings.copyright"), Input::new(&d.copyright).small().into_any_element()).flex_1(),
                    )
                    .child(
                        field(
                            t!("settings.language"),
                            Select::new(&d.language).small().placeholder(t!("event_dialog.choose")).into_any_element(),
                        )
                        .w(px(160.))
                        .flex_none(),
                    ),
            )
            .child(field(t!("settings.description"), Textarea::new(&d.description).h(px(72.)).into_any_element()))
            .child(field(
                t!("settings.root"),
                Select::new(&d.root).small().search_placeholder(t!("event_dialog.search")).into_any_element(),
            ))
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
                t!("settings.place_format"),
                plus_button("plac-add").tooltip(t!("settings.add_jurisdiction")).on_click(cx.listener(
                    |this, _, w, cx| {
                        let name = text_input(&t!("settings.jurisdiction"), "", w, cx);
                        name.update(cx, |s, cx| s.focus(w, cx));
                        if let Some(d) = this.settings_dialog() {
                            d.places.push(PlaceRow { name, from: None });
                        }
                        cx.notify();
                    },
                )),
            ))
            .children(rows)
            .when_some(preview, |v, p| {
                v.child(div().pt_1().text_sm().text_color(theme::muted()).child(t!("settings.example", example = p)))
            })
            .into_any_element()
    }

    fn settings_events(&self, d: &SettingsDialog, cx: &mut Context<Self>) -> AnyElement {
        let section = |family: bool, title: std::borrow::Cow<'static, str>, cx: &mut Context<Self>| {
            let rows = d.events.iter().enumerate().filter(|(_, r)| r.family == family).map(|(i, r)| {
                let removable = r.uses == 0;
                h_flex()
                    .gap_2()
                    .items_center()
                    .child(div().flex_1().min_w_0().child(Input::new(&r.label).small()))
                    .child(div().flex_1().min_w_0().child(Input::new(&r.ty).small()))
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
                    plus_button(if family { "even-add-fam" } else { "even-add-indi" })
                        .tooltip(t!("settings.add_type"))
                        .on_click(cx.listener(move |this, _, w, cx| {
                            let label = text_input(&t!("settings.label"), "", w, cx);
                            let ty = text_input("TYPE", "", w, cx);
                            label.update(cx, |s, cx| s.focus(w, cx));
                            if let Some(d) = this.settings_dialog() {
                                d.events.push(EventRow {
                                    label,
                                    ty,
                                    original: None,
                                    original_label: String::new(),
                                    family,
                                    uses: 0,
                                });
                            }
                            cx.notify();
                        })),
                ))
                .children(rows)
        };
        v_flex()
            .gap_4()
            .child(section(false, t!("settings.individual_events"), cx))
            .child(section(true, t!("settings.family_events"), cx))
            .into_any_element()
    }
}

impl Workspace {
    fn settings_media(&self, d: &SettingsDialog, cx: &mut Context<Self>) -> AnyElement {
        let g = &self.doc.as_ref().unwrap().ged;
        let options = type_options(g);
        let naming = media_naming(d, cx);
        let n = d.media.len();
        let rows = d.media.iter().enumerate().map(|(i, r)| {
            let id = |what: &str| SharedString::from(format!("media-{what}-{i}"));
            // Exemple : le premier média du fichier rangé dans ce sous-dossier.
            let f = folder(r, cx);
            let example = d
                .media_samples
                .iter()
                .find(|(ctx, _)| naming.folder_for(ctx) == Some(&f))
                .map(|(ctx, file)| media_path(&naming, ctx, file, None));
            let ws = cx.entity().downgrade();
            let opts = options.clone();
            let types = Button::new(id("types"))
                .small()
                .outline()
                .w_full()
                .dropdown_caret(true)
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .truncate()
                        .when(r.types.is_empty(), |d| d.text_color(theme::muted()))
                        .child(if r.types.is_empty() {
                            t!("settings.everything_else").into_owned()
                        } else {
                            media_type_labels(&r.types)
                        }),
                )
                .dropdown_menu(move |menu, _, cx| {
                    let current = ws
                        .upgrade()
                        .and_then(|ws| match &ws.read(cx).add_dialog {
                            Some(AddDialog::Settings(d)) => d.media.get(i).map(|r| r.types.clone()),
                            _ => None,
                        })
                        .unwrap_or_default();
                    let mut menu = menu.scrollable(true).max_h(px(360.));
                    for (label, tags) in &opts {
                        let on = tags.iter().all(|t| current.contains(t));
                        let (ws, tags) = (ws.clone(), tags.clone());
                        menu = menu.item(PopupMenuItem::new(label.clone()).checked(on).on_click(move |_, _, cx| {
                            let _ = ws.update(cx, |this, cx| {
                                if let Some(d) = this.settings_dialog() {
                                    // Un type ne va que dans un sous-dossier : le cocher
                                    // ici le retire des autres.
                                    if !on {
                                        for (j, other) in d.media.iter_mut().enumerate() {
                                            if j != i {
                                                other.types.retain(|t| !tags.contains(t));
                                            }
                                        }
                                    }
                                    if let Some(r) = d.media.get_mut(i) {
                                        if on {
                                            r.types.retain(|t| !tags.contains(t));
                                        } else {
                                            for t in &tags {
                                                if !r.types.contains(t) {
                                                    r.types.push(t.clone());
                                                }
                                            }
                                        }
                                    }
                                }
                                cx.notify();
                            });
                        }));
                    }
                    menu
                });
            v_flex()
                .gap_1()
                .p_2()
                .rounded(theme::RADIUS_SM)
                .border_1()
                .border_color(if i == d.media_active { theme::border_strong() } else { theme::border() })
                .child(
                    h_flex()
                        .gap_1()
                        .items_center()
                        .child(div().w(px(150.)).child(Input::new(&r.dir).small()))
                        .child(div().flex_1().min_w_0().child(types))
                        .child(
                            Button::new(id("up")).xsmall().ghost().icon(IconName::ChevronUp).disabled(i == 0).on_click(
                                cx.listener(move |this, _, _, cx| {
                                    if let Some(d) = this.settings_dialog() {
                                        d.media.swap(i, i - 1);
                                    }
                                    cx.notify();
                                }),
                            ),
                        )
                        .child(
                            Button::new(id("down"))
                                .xsmall()
                                .ghost()
                                .icon(IconName::ChevronDown)
                                .disabled(i + 1 == n)
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    if let Some(d) = this.settings_dialog() {
                                        d.media.swap(i, i + 1);
                                    }
                                    cx.notify();
                                })),
                        )
                        .child(Button::new(id("del")).xsmall().ghost().icon(IconName::Close).on_click(cx.listener(
                            move |this, _, _, cx| {
                                if let Some(d) = this.settings_dialog() {
                                    d.media.remove(i);
                                    d.media_active = d.media_active.min(d.media.len().saturating_sub(1));
                                }
                                cx.notify();
                            },
                        ))),
                )
                .child(Input::new(&r.template).small())
                .when_some(example, |v, text| {
                    v.child(div().text_sm().truncate().text_color(theme::muted()).child(text))
                })
        });
        let active = d.media.get(d.media_active).map(|r| r.template.clone());
        let tokens = MEDIA_TOKENS.iter().map(|t| {
            let input = active.clone();
            Button::new(SharedString::from(format!("token-{t}"))).xsmall().outline().label(format!("{{{t}}}")).on_click(
                cx.listener(move |_, _, w, cx| {
                    let Some(input) = &input else { return };
                    input.update(cx, |s, cx| {
                        let v = s.value().to_string();
                        let sep = if v.is_empty() || v.ends_with(['/', ' ', '_', '-']) { "" } else { " " };
                        s.set_value(format!("{v}{sep}{{{t}}}"), w, cx);
                    });
                }),
            )
        });
        v_flex()
            .gap_3()
            .child(field(t!("settings.media_folder"), Input::new(&d.media_root).small().into_any_element()))
            .child(
                v_flex()
                    .gap_2()
                    .child(section_header(
                        t!("settings.subfolders"),
                        plus_button("media-add").tooltip(t!("settings.add_folder")).on_click(cx.listener(
                            |this, _, w, cx| {
                                let (row, subs) = MediaRow::new(&MediaFolder::default(), w, cx);
                                row.dir.update(cx, |s, cx| s.focus(w, cx));
                                if let Some(d) = this.settings_dialog() {
                                    d.media.push(row);
                                    d.media_active = d.media.len() - 1;
                                    d._subs.extend(subs);
                                }
                                cx.notify();
                            },
                        )),
                    ))
                    .children(rows)
                    .when(n > 0, |v| v.child(h_flex().flex_wrap().gap_1().children(tokens))),
            )
            .into_any_element()
    }
}

impl Workspace {
    fn settings_search(&self, d: &SettingsDialog, cx: &mut Context<Self>) -> AnyElement {
        let g = &self.doc.as_ref().unwrap().ged;
        // Exemple : la personne affichée, sinon l'individu racine.
        let person = self.selected.clone().filter(|x| g.get(x).is_some_and(|n| n.tag == "INDI")).or_else(|| g.root());
        let n = d.sites.len();
        let rows = d.sites.iter().enumerate().map(|(i, r)| {
            let id = |what: &str| SharedString::from(format!("site-{what}-{i}"));
            let template = r.url.read(cx).value().trim().to_string();
            let example = person.as_ref().filter(|_| !template.is_empty()).map(|x| g.search_url(&template, x));
            v_flex()
                .gap_1()
                .p_2()
                .rounded(theme::RADIUS_SM)
                .border_1()
                .border_color(if i == d.sites_active { theme::border_strong() } else { theme::border() })
                .child(
                    h_flex()
                        .gap_1()
                        .items_center()
                        .child(div().flex_1().min_w_0().child(Input::new(&r.name).small()))
                        .child(
                            Button::new(id("up")).xsmall().ghost().icon(IconName::ChevronUp).disabled(i == 0).on_click(
                                cx.listener(move |this, _, _, cx| {
                                    if let Some(d) = this.settings_dialog() {
                                        d.sites.swap(i, i - 1);
                                    }
                                    cx.notify();
                                }),
                            ),
                        )
                        .child(
                            Button::new(id("down"))
                                .xsmall()
                                .ghost()
                                .icon(IconName::ChevronDown)
                                .disabled(i + 1 == n)
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    if let Some(d) = this.settings_dialog() {
                                        d.sites.swap(i, i + 1);
                                    }
                                    cx.notify();
                                })),
                        )
                        .child(Button::new(id("del")).xsmall().ghost().icon(IconName::Close).on_click(cx.listener(
                            move |this, _, _, cx| {
                                if let Some(d) = this.settings_dialog() {
                                    d.sites.remove(i);
                                    d.sites_active = d.sites_active.min(d.sites.len().saturating_sub(1));
                                }
                                cx.notify();
                            },
                        ))),
                )
                .child(Input::new(&r.url).small())
                .when_some(example, |v, text| {
                    v.child(div().text_sm().truncate().text_color(theme::muted()).child(text))
                })
        });
        let active = d.sites.get(d.sites_active).map(|r| r.url.clone());
        let tokens = WEB_TOKENS.iter().map(|t| {
            let input = active.clone();
            Button::new(SharedString::from(format!("web-token-{t}")))
                .xsmall()
                .outline()
                .label(format!("{{{t}}}"))
                .on_click(cx.listener(move |_, _, w, cx| {
                    let Some(input) = &input else { return };
                    input.update(cx, |s, cx| {
                        let v = s.value().to_string();
                        s.set_value(format!("{v}{{{t}}}"), w, cx);
                    });
                }))
        });
        v_flex()
            .gap_2()
            .child(section_header(
                t!("settings.sites"),
                plus_button("site-add").tooltip(t!("settings.add_site")).on_click(cx.listener(|this, _, w, cx| {
                    let empty = SearchSite { name: String::new(), url: String::new() };
                    let (row, subs) = SiteRow::new(&empty, w, cx);
                    row.name.update(cx, |s, cx| s.focus(w, cx));
                    if let Some(d) = this.settings_dialog() {
                        d.sites.push(row);
                        d.sites_active = d.sites.len() - 1;
                        d._subs.extend(subs);
                    }
                    cx.notify();
                })),
            ))
            .children(rows)
            .when(n > 0, |v| v.child(h_flex().flex_wrap().gap_1().children(tokens)))
            .into_any_element()
    }
}

/// Les sites saisis, dans l'ordre (ceux sans URL sont ignorés).
fn search_sites(d: &SettingsDialog, cx: &App) -> Vec<SearchSite> {
    d.sites
        .iter()
        .map(|r| SearchSite { name: r.name.read(cx).value().to_string(), url: r.url.read(cx).value().to_string() })
        .collect()
}

/// Choix du menu des types : libellé, balises. Les événements courants, les
/// types personnalisés du fichier, puis « Portrait » (média rattaché à une personne).
fn type_options(g: &Gedcom) -> Vec<(String, Vec<String>)> {
    const ATTRIBUTES: &[&str] = &["RESI", "OCCU", "EDUC", "RELI", "TITL", "NATI", "FACT", "EVEN", "CHR"];
    let mut out: Vec<(String, Vec<String>)> = gedcom::EVENT_TAGS
        .iter()
        .filter(|t| !ATTRIBUTES.contains(t))
        .map(|t| {
            let tags = if *t == "BAPM" { vec!["BAPM".into(), "CHR".into()] } else { vec![t.to_string()] };
            (gedcom::event_name(t), tags)
        })
        .collect();
    // Les actes d'état civil d'abord.
    const FIRST: &[&str] = &["BIRT", "BAPM", "MARR", "DEAT", "BURI", "CENS"];
    out.sort_by_key(|(_, tags)| FIRST.iter().position(|t| *t == tags[0]).unwrap_or(FIRST.len()));
    for t in g.event_types() {
        if !out.iter().any(|(l, _)| *l == t.label) {
            out.push((t.label, vec![t.ty]));
        }
    }
    out.push((t!("settings.portrait").into(), vec!["INDI".into()]));
    out
}

fn folder(r: &MediaRow, cx: &App) -> MediaFolder {
    MediaFolder {
        dir: r.dir.read(cx).value().trim().to_string(),
        types: r.types.clone(),
        template: r.template.read(cx).value().trim().to_string(),
    }
}

/// Le réglage saisi (les sous-dossiers laissés vides sont ignorés).
fn media_naming(d: &SettingsDialog, cx: &App) -> MediaNaming {
    let folders = d
        .media
        .iter()
        .map(|r| folder(r, cx))
        .filter(|f| !(f.dir.is_empty() && f.types.is_empty() && f.template.is_empty()))
        .collect();
    MediaNaming { root: d.media_root.read(cx).value().trim().to_string(), folders }
}

fn field(label: impl Into<SharedString>, el: AnyElement) -> Div {
    v_flex().gap_1().min_w_0().child(label_caps(label)).child(el)
}

fn places(n: usize) -> String {
    if n == 1 { t!("settings.places.one", n = n) } else { t!("settings.places.other", n = n) }.into_owned()
}
