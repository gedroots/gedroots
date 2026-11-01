//! Écran « Saisie d'acte » : saisie guidée d'une source, à la manière
//! d'Ancestral Sources (Family Historian). À gauche l'image de l'acte, à droite
//! le formulaire du modèle choisi : référence de l'acte, personnes citées (lier à
//! un individu existant, créer, ou mentionner seulement) et aperçu du GEDCOM.
//!
//! Le formulaire travaille sur une copie du document ; l'écriture réelle est
//! faite par l'espace de travail quand il reçoit [`EntryFormEvent::Save`].

use crate::theme;
use gedcom::*;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::checkbox::Checkbox;
use gpui_kit::component::input::{Input, InputEvent, InputState, Textarea, TextareaState};
use gpui_kit::component::{Disableable, Sizable, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use std::path::{Path, PathBuf};

pub enum EntryFormEvent {
    /// Écrire l'acte ; `next` : enchaîner sur l'acte suivant du même registre.
    Save {
        entry: Box<Entry>,
        next: bool,
    },
    Close,
}

/// Rôle d'un champ de saisie, pour savoir quoi recalculer quand il change.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Plain,
    /// Lieu : propose les lieux du fichier.
    Place,
    /// Nom ou prénom d'une personne : relance les suggestions.
    Name,
    /// Recherche de source.
    SourceSearch,
    /// Recherche de dépôt (nouvelle source).
    RepoSearch,
}

struct PersonRow {
    id: u64,
    role: String,
    inputs: Vec<(PersonField, Entity<InputState>)>,
    /// Sexe choisi par boutons (`Some("")` : à choisir) ; `None` : fixé par le rôle.
    sex: Option<String>,
    choice: PersonChoice,
    suggestions: Vec<Suggestion>,
}

pub struct EntryForm {
    ged: Gedcom,
    base_dir: Option<PathBuf>,
    templates: Vec<Template>,
    template: usize,
    source: Option<String>,
    new_source: bool,
    source_search: Entity<InputState>,
    repo_search: Entity<InputState>,
    repo: Option<String>,
    call_number: Entity<InputState>,
    publication: Entity<InputState>,
    date: Entity<InputState>,
    place: Entity<InputState>,
    record_date: Entity<InputState>,
    fields: Vec<(String, String, Entity<InputState>)>,
    transcription: Entity<TextareaState>,
    quality: Option<u8>,
    media: MediaChoice,
    estimate_births: bool,
    rows: Vec<PersonRow>,
    next_row_id: u64,
    /// Champ de lieu actif (pour afficher les lieux proposés sous ce champ).
    place_focus: Option<EntityId>,
    places: Vec<(String, String)>,
    preview: Option<Result<String, String>>,
    error: Option<SharedString>,
    zoom: f32,
    saved_count: usize,
    scroll: ScrollHandle,
    _subs: Vec<Subscription>,
}

impl EventEmitter<EntryFormEvent> for EntryForm {}

const QUALITY: [&str; 4] = ["Peu fiable", "Douteuse", "Indirecte", "Directe"];

impl EntryForm {
    /// `source` : source présélectionnée (son modèle est alors repris).
    pub fn new(
        ged: Gedcom,
        ged_path: Option<PathBuf>,
        source: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let templates = ged.templates();
        let template = source
            .as_deref()
            .and_then(|s| ged.source_template(s))
            .and_then(|t| templates.iter().position(|x| x.id == t.id))
            .unwrap_or(0);
        let places = ged.places().into_iter().map(|p| (p.short(), p.name)).collect();
        let mk = |cx: &mut Context<Self>, window: &mut Window, ph: &str| {
            let ph = ph.to_string();
            cx.new(|cx| InputState::new(window, cx).placeholder(ph))
        };
        let source_search = mk(cx, window, "Chercher une source ou saisir le titre d'une nouvelle…");
        let repo_search = mk(cx, window, "Dépôt (archives, mairie…)");
        let call_number = mk(cx, window, "Cote, ex. 3 E 502/112");
        let publication = mk(cx, window, "ex. Saint-Lô, Naissances (1893-1912)");
        let date = mk(cx, window, "ex. 14 MAR 1912, ABT 1850");
        let place = mk(cx, window, "Commune…");
        let record_date = mk(cx, window, "si différente de l'événement");
        let transcription = cx.new(|cx| TextareaState::new(window, cx).rows(4));
        let mut this = Self {
            ged,
            base_dir: ged_path.and_then(|p| p.parent().map(Path::to_path_buf)),
            templates,
            template,
            source: source.clone(),
            new_source: false,
            source_search,
            repo_search,
            repo: None,
            call_number,
            publication,
            date,
            place,
            record_date,
            fields: Vec::new(),
            transcription,
            quality: None,
            media: MediaChoice::None,
            estimate_births: true,
            rows: Vec::new(),
            next_row_id: 0,
            place_focus: None,
            places,
            preview: None,
            error: None,
            zoom: 1.0,
            saved_count: 0,
            scroll: ScrollHandle::new(),
            _subs: Vec::new(),
        };
        for (e, kind) in [
            (this.source_search.clone(), Kind::SourceSearch),
            (this.repo_search.clone(), Kind::RepoSearch),
            (this.call_number.clone(), Kind::Plain),
            (this.publication.clone(), Kind::Plain),
            (this.date.clone(), Kind::Name),
            (this.place.clone(), Kind::Place),
            (this.record_date.clone(), Kind::Plain),
        ] {
            this.watch(&e, kind, window, cx);
        }
        let t = this.transcription.clone();
        let sub = cx.subscribe_in(&t, window, |this, _, ev: &InputEvent, _, cx| {
            if matches!(ev, InputEvent::Change) {
                this.preview = None;
                cx.notify();
            }
        });
        this._subs.push(sub);
        this.media = this.default_media();
        this.reset_template(None, window, cx);
        this
    }

    fn watch(&mut self, e: &Entity<InputState>, kind: Kind, window: &mut Window, cx: &mut Context<Self>) {
        let sub = cx.subscribe_in(e, window, move |this, input, ev: &InputEvent, _window, cx| {
            match ev {
                InputEvent::Focus => {
                    this.place_focus = (kind == Kind::Place).then(|| input.entity_id());
                }
                InputEvent::Change => {
                    this.preview = None;
                    this.error = None;
                    match kind {
                        Kind::Place => this.place_focus = Some(input.entity_id()),
                        Kind::Name => this.refresh_suggestions(cx),
                        Kind::RepoSearch => this.repo = None,
                        _ => {}
                    }
                }
                _ => return,
            }
            cx.notify();
        });
        self._subs.push(sub);
    }

    fn input(
        &mut self,
        value: &str,
        ph: &str,
        kind: Kind,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<InputState> {
        let (value, ph) = (value.to_string(), ph.to_string());
        let e = cx.new(|cx| InputState::new(window, cx).placeholder(ph).default_value(value));
        self.watch(&e, kind, window, cx);
        e
    }

    fn template(&self) -> &Template {
        &self.templates[self.template]
    }

    /// Recrée les champs du modèle courant. `from` : acte précédent à enchaîner.
    fn reset_template(&mut self, from: Option<&Entry>, window: &mut Window, cx: &mut Context<Self>) {
        let t = self.template().clone();
        let old: Vec<(String, String)> =
            self.fields.iter().map(|(k, _, e)| (k.clone(), e.read(cx).value().to_string())).collect();
        self.fields = Vec::new();
        for f in &t.fields {
            let v = match from {
                Some(e) => e.field(&f.key).to_string(),
                None => old.iter().find(|(k, _)| *k == f.key).map(|(_, v)| v.clone()).unwrap_or_default(),
            };
            let e = self.input(&v, "", Kind::Plain, window, cx);
            self.fields.push((f.key.clone(), f.label.clone(), e));
        }
        self.rows.clear();
        for r in t.roles.iter().filter(|r| r.default) {
            self.add_row(&r.key.clone(), window, cx);
        }
        self.preview = None;
        self.refresh_suggestions(cx);
    }

    fn add_row(&mut self, role: &str, window: &mut Window, cx: &mut Context<Self>) {
        let Some(def) = self.template().role(role).cloned() else { return };
        let mut inputs = Vec::new();
        let mut sex = None;
        for f in &def.fields {
            if *f == PersonField::Sex {
                if def.sex.is_none() {
                    sex = Some(String::new());
                }
                continue;
            }
            let kind = match f {
                PersonField::Surname | PersonField::Given | PersonField::Age => Kind::Name,
                PersonField::Residence | PersonField::Birthplace => Kind::Place,
                _ => Kind::Plain,
            };
            let ph = match f {
                PersonField::Age => "ex. 32 ans",
                PersonField::Kinship => "ex. fils, épouse, voisin",
                _ => "",
            };
            let e = self.input("", ph, kind, window, cx);
            inputs.push((*f, e));
        }
        self.next_row_id += 1;
        self.rows.push(PersonRow {
            id: self.next_row_id,
            role: role.to_string(),
            inputs,
            sex,
            choice: PersonChoice::Create,
            suggestions: Vec::new(),
        });
    }

    fn default_media(&self) -> MediaChoice {
        let Some(s) = self.source.as_deref().and_then(|s| self.ged.get(s)) else { return MediaChoice::None };
        Source::new(s)
            .media_xrefs()
            .into_iter()
            .find(|m| self.ged.get(m).is_some_and(|n| Media::new(n).is_image()))
            .map(|m| MediaChoice::Existing(m.to_string()))
            .unwrap_or(MediaChoice::None)
    }

    /// Construit l'acte à partir des champs.
    fn collect(&self, cx: &App) -> Entry {
        let v = |e: &Entity<InputState>| e.read(cx).value().trim().to_string();
        let source = match (&self.source, self.new_source) {
            (Some(x), _) => SourceChoice::Existing(x.clone()),
            _ => SourceChoice::New {
                title: v(&self.source_search),
                publication: v(&self.publication),
                repository: self.repo.clone(),
                call_number: v(&self.call_number),
            },
        };
        let mut e = Entry::new(self.template().clone(), source);
        e.date = v(&self.date).to_uppercase();
        e.place = v(&self.place);
        e.record_date = v(&self.record_date).to_uppercase();
        for (k, _, i) in &self.fields {
            e.set_field(k, v(i));
        }
        e.transcription = self.transcription.read(cx).value().to_string();
        e.quality = self.quality;
        e.media = self.media.clone();
        e.estimate_births = self.estimate_births;
        e.participants = self
            .rows
            .iter()
            .map(|r| Participant {
                role: r.role.clone(),
                values: r
                    .inputs
                    .iter()
                    .map(|(f, i)| (*f, v(i)))
                    .chain(r.sex.clone().map(|s| (PersonField::Sex, s)))
                    .collect(),
                choice: r.choice.clone(),
            })
            .collect();
        e
    }

    fn refresh_suggestions(&mut self, cx: &mut Context<Self>) {
        let entry = self.collect(cx);
        for (i, r) in self.rows.iter_mut().enumerate() {
            r.suggestions = if r.choice == PersonChoice::Create { entry.suggestions(&self.ged, i) } else { Vec::new() };
        }
    }

    fn set_choice(&mut self, row: u64, choice: PersonChoice, window: &mut Window, cx: &mut Context<Self>) {
        let Some(i) = self.rows.iter().position(|r| r.id == row) else { return };
        // En liant, on complète les champs vides avec l'individu choisi.
        if let PersonChoice::Link(x) = &choice
            && let Some(ind) = self.ged.individual(x)
        {
            let (s, g, sex) = (ind.surname(), ind.given(), ind.sex().to_string());
            for (f, input) in &self.rows[i].inputs {
                let val = match f {
                    PersonField::Surname => s.clone(),
                    PersonField::Given => g.clone(),
                    PersonField::Sex if sex != "U" => sex.clone(),
                    _ => continue,
                };
                if input.read(cx).value().trim().is_empty() {
                    input.update(cx, |st, cx| st.set_value(val, window, cx));
                }
            }
            if let Some(s) = self.rows[i].sex.as_mut().filter(|s| s.is_empty() && sex != "U") {
                *s = sex;
            }
        }
        self.rows[i].choice = choice;
        self.preview = None;
        self.error = None;
        self.refresh_suggestions(cx);
        cx.notify();
    }

    fn pick_source(&mut self, xref: Option<String>, window: &mut Window, cx: &mut Context<Self>) {
        self.new_source = xref.is_none();
        self.source = xref;
        if let Some(t) = self.source.as_deref().and_then(|s| self.ged.source_template(s))
            && let Some(i) = self.templates.iter().position(|x| x.id == t.id)
            && i != self.template
        {
            self.template = i;
            self.reset_template(None, window, cx);
        }
        self.media = self.default_media();
        self.preview = None;
        self.error = None;
        cx.notify();
    }

    fn set_template(&mut self, i: usize, window: &mut Window, cx: &mut Context<Self>) {
        if i != self.template {
            self.template = i;
            self.reset_template(None, window, cx);
            cx.notify();
        }
    }

    fn pick_file(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let rx = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Image de l'acte".into()),
        });
        cx.spawn_in(window, async move |this, cx| {
            if let Ok(Ok(Some(mut paths))) = rx.await
                && let Some(p) = paths.pop()
            {
                let _ = this.update(cx, |this, cx| {
                    let rel = this
                        .base_dir
                        .as_ref()
                        .and_then(|d| p.strip_prefix(d).ok())
                        .map(|r| r.to_string_lossy().replace('\\', "/"))
                        .unwrap_or_else(|| p.to_string_lossy().to_string());
                    this.media = MediaChoice::NewFile(rel);
                    this.preview = None;
                    cx.notify();
                });
            }
        })
        .detach();
    }

    fn fill_transcription(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let text = self.collect(cx).auto_text();
        self.transcription.update(cx, |t, cx| t.set_value(text, window, cx));
    }

    fn toggle_preview(&mut self, cx: &mut Context<Self>) {
        self.preview = match self.preview {
            Some(_) => None,
            None => Some(self.collect(cx).preview(&self.ged)),
        };
        cx.notify();
    }

    fn save(&mut self, next: bool, cx: &mut Context<Self>) {
        let entry = self.collect(cx);
        match entry.validate(&self.ged) {
            Ok(()) => cx.emit(EntryFormEvent::Save { entry: Box::new(entry), next }),
            Err(e) => {
                self.error = Some(e.into());
                cx.notify();
            }
        }
    }

    /// Appelé par l'espace de travail après un enregistrement réussi.
    pub fn saved(
        &mut self,
        ged: Gedcom,
        result: &EntryResult,
        next: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let entry = self.collect(cx);
        // Source propre à chaque acte (titre automatique) : l'acte suivant en aura une nouvelle.
        let per_act = matches!(&entry.source, SourceChoice::New { title, .. } if title.is_empty());
        self.ged = ged;
        self.saved_count += 1;
        self.source = if per_act { None } else { Some(result.source.clone()) };
        self.new_source = per_act;
        self.places = self.ged.places().into_iter().map(|p| (p.short(), p.name)).collect();
        if next {
            let n = entry.next();
            self.media = match &n.media {
                _ if per_act => MediaChoice::None,
                MediaChoice::NewFile(_) => result.media.clone().map(MediaChoice::Existing).unwrap_or_default(),
                m => m.clone(),
            };
            if per_act {
                self.call_number.update(cx, |s, cx| s.set_value("", window, cx));
            }
            self.date.update(cx, |s, cx| s.set_value("", window, cx));
            self.record_date.update(cx, |s, cx| s.set_value("", window, cx));
            self.transcription.update(cx, |s, cx| s.set_value("", window, cx));
            self.reset_template(Some(&n), window, cx);
            self.error = None;
            // Retour en haut du formulaire, curseur sur la première personne.
            self.scroll.set_offset(point(px(0.), px(0.)));
            if let Some((_, first)) = self.rows.first().and_then(|r| r.inputs.first()) {
                first.update(cx, |s, cx| s.focus(window, cx));
            }
            cx.notify();
        }
    }

    pub fn set_error(&mut self, e: String, cx: &mut Context<Self>) {
        self.error = Some(e.into());
        cx.notify();
    }

    fn media_file(&self) -> Option<(PathBuf, String)> {
        let rel = match &self.media {
            MediaChoice::None => return None,
            MediaChoice::Existing(x) => {
                let m = Media::new(self.ged.get(x)?);
                (m.file()?.to_string(), m.title())
            }
            MediaChoice::NewFile(f) => (f.clone(), f.rsplit('/').next().unwrap_or(f).to_string()),
        };
        let p = PathBuf::from(rel.0.strip_prefix("file://").unwrap_or(&rel.0));
        let p = if p.is_absolute() { p } else { self.base_dir.as_ref()?.join(p) };
        Some((p, rel.1))
    }
}

// ======================================================================
// Rendu
// ======================================================================

fn caps(s: impl Into<SharedString>) -> Div {
    div().text_xs().font_weight(FontWeight::SEMIBOLD).text_color(theme::muted()).child(s.into())
}

fn mono(s: impl Into<SharedString>) -> Div {
    div().font_family("monospace").text_xs().text_color(theme::muted()).child(s.into())
}

fn field(label: impl Into<SharedString>, input: &Entity<InputState>) -> Div {
    v_flex().gap_1().min_w_0().child(caps(label)).child(Input::new(input).small())
}

fn chip(id: impl Into<SharedString>, label: impl Into<SharedString>, on: bool) -> Stateful<Div> {
    div()
        .id(ElementId::Name(id.into()))
        .px_2()
        .py(px(3.))
        .rounded_md()
        .border_1()
        .text_xs()
        .cursor_pointer()
        .border_color(if on { theme::accent() } else { theme::border_strong() })
        .bg(if on { theme::accent_soft() } else { theme::surface() })
        .text_color(if on { theme::text() } else { theme::muted() })
        .hover(|d| d.border_color(theme::accent()))
        .child(label.into())
}

fn section(title: &'static str) -> Div {
    v_flex().gap_2().p_3().rounded_md().border_1().border_color(theme::border()).bg(theme::panel()).child(caps(title))
}

impl EntryForm {
    /// Lieux du fichier qui correspondent au champ de lieu actif.
    fn place_suggestions(&self, input: &Entity<InputState>, cx: &mut Context<Self>) -> Option<Div> {
        if self.place_focus != Some(input.entity_id()) {
            return None;
        }
        let q = sort_key(input.read(cx).value().trim());
        if q.len() < 2 {
            return None;
        }
        let hits: Vec<(String, String)> = self
            .places
            .iter()
            .filter(|(short, full)| sort_key(short).contains(&q) && *full != input.read(cx).value().as_ref())
            .take(6)
            .cloned()
            .collect();
        if hits.is_empty() {
            return None;
        }
        let entity = input.clone();
        Some(h_flex().flex_wrap().gap_1().children(hits.into_iter().enumerate().map(|(n, (short, full))| {
            let entity = entity.clone();
            chip(format!("pl-{}-{n}", input.entity_id()), short, false).on_click(cx.listener(
                move |this, _, window, cx| {
                    entity.update(cx, |s, cx| s.set_value(full.clone(), window, cx));
                    this.place_focus = None;
                    cx.notify();
                },
            ))
        })))
    }

    fn render_image(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let candidates: Vec<(String, String)> = self
            .source
            .as_deref()
            .and_then(|s| self.ged.get(s))
            .map(|s| {
                Source::new(s)
                    .media_xrefs()
                    .into_iter()
                    .filter_map(|m| self.ged.get(m).map(|n| (m.to_string(), Media::new(n).title())))
                    .collect()
            })
            .unwrap_or_default();
        let file = self.media_file();
        let zoom = self.zoom;
        let viewer: AnyElement = match &file {
            Some((p, _)) if p.exists() => div()
                .id("acte-scroll")
                .flex_1()
                .min_h_0()
                .overflow_scroll()
                .bg(theme::c(0x0c0e11))
                .child({
                    // À 100 % l'image tient dans le panneau ; au-delà on la grossit et on fait défiler.
                    let (w, h) =
                        imagesize::size(p).map(|s| (s.width as f32, s.height as f32)).unwrap_or((1000., 1400.));
                    let width = 500. * zoom;
                    let i = img(p.clone()).object_fit(ObjectFit::Contain);
                    if zoom <= 1.0 { i.size_full() } else { i.w(px(width)).h(px(width * h / w.max(1.))) }
                })
                .into_any_element(),
            Some((p, _)) => v_flex()
                .flex_1()
                .items_center()
                .justify_center()
                .p_4()
                .text_color(theme::warn())
                .child(format!("Fichier introuvable : {}", p.display()))
                .into_any_element(),
            None => v_flex()
                .flex_1()
                .items_center()
                .justify_center()
                .gap_2()
                .p_4()
                .text_color(theme::muted())
                .child("Aucune image pour cet acte")
                .child(mono("Choisissez une image de la source ou un fichier"))
                .into_any_element(),
        };
        v_flex()
            .w(px(520.))
            .flex_none()
            .border_r_1()
            .border_color(theme::border())
            .bg(theme::panel())
            .child(
                h_flex()
                    .h(px(36.))
                    .px_3()
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
                            .text_xs()
                            .text_color(theme::muted())
                            .child(file.as_ref().map(|f| f.1.clone()).unwrap_or_else(|| "Image de l'acte".into())),
                    )
                    .child(Button::new("zoom-out").ghost().xsmall().label("−").disabled(zoom <= 1.0).on_click(
                        cx.listener(|this, _, _, cx| {
                            this.zoom = (this.zoom / 1.25).max(1.0);
                            cx.notify();
                        }),
                    ))
                    .child(mono(format!("{:.0} %", zoom * 100.)))
                    .child(Button::new("zoom-in").ghost().xsmall().label("+").on_click(cx.listener(
                        |this, _, _, cx| {
                            this.zoom = (this.zoom * 1.25).min(6.0);
                            cx.notify();
                        },
                    ))),
            )
            .child(viewer)
            .child(
                h_flex()
                    .p_2()
                    .gap_1()
                    .flex_wrap()
                    .border_t_1()
                    .border_color(theme::border())
                    .children(candidates.into_iter().map(|(x, title)| {
                        let on = self.media == MediaChoice::Existing(x.clone());
                        chip(format!("media-{x}"), title, on).max_w(px(240.)).overflow_hidden().on_click(cx.listener(
                            move |this, _, _, cx| {
                                this.media = MediaChoice::Existing(x.clone());
                                this.preview = None;
                                cx.notify();
                            },
                        ))
                    }))
                    .when_some(
                        match &self.media {
                            MediaChoice::NewFile(f) => Some(f.clone()),
                            _ => None,
                        },
                        |d, f| d.child(chip("media-new", format!("Nouveau : {f}"), true)),
                    )
                    .child(
                        Button::new("pick-file")
                            .ghost()
                            .xsmall()
                            .label("Autre fichier…")
                            .on_click(cx.listener(|this, _, w, cx| this.pick_file(w, cx))),
                    )
                    .when(self.media != MediaChoice::None, |d| {
                        d.child(Button::new("no-media").ghost().xsmall().label("Sans image").on_click(cx.listener(
                            |this, _, _, cx| {
                                this.media = MediaChoice::None;
                                this.preview = None;
                                cx.notify();
                            },
                        )))
                    }),
            )
    }

    fn render_source(&self, cx: &mut Context<Self>) -> Div {
        let mut sec = section("Source (registre)");
        if let Some(x) = &self.source {
            let title = self.ged.record_label(x);
            let repo = self.ged.get(x).and_then(|n| Source::new(n).repository()).map(|r| self.ged.record_label(r));
            return sec.child(
                h_flex()
                    .gap_2()
                    .items_center()
                    .child(
                        v_flex()
                            .flex_1()
                            .min_w_0()
                            .child(div().font_weight(FontWeight::MEDIUM).child(title))
                            .when_some(repo, |d, r| d.child(div().text_xs().text_color(theme::muted()).child(r))),
                    )
                    .child(Button::new("change-source").ghost().xsmall().label("Changer").on_click(cx.listener(
                        |this, _, _, cx| {
                            this.source = None;
                            this.new_source = false;
                            this.preview = None;
                            cx.notify();
                        },
                    ))),
            );
        }
        sec = sec.child(Input::new(&self.source_search).small().cleanable(true));
        let q = sort_key(self.source_search.read(cx).value().trim());
        if self.new_source {
            let rq = sort_key(self.repo_search.read(cx).value().trim());
            let repos: Vec<(String, String)> = if self.repo.is_none() && rq.len() >= 2 {
                self.ged
                    .records_of(RecordKind::Repository)
                    .filter_map(|r| {
                        let name = Repository::new(r).name().to_string();
                        sort_key(&name).contains(&rq).then(|| (r.xref.clone().unwrap_or_default(), name))
                    })
                    .take(6)
                    .collect()
            } else {
                Vec::new()
            };
            let title = if q.is_empty() {
                format!("Titre automatique : {}", self.collect(cx).auto_title())
            } else {
                "Nouvelle source : le titre est celui saisi ci-dessus.".to_string()
            };
            return sec
                .child(mono(title))
                .child(field("Registre / publication", &self.publication))
                .child(
                    h_flex()
                        .gap_2()
                        .child(div().flex_1().child(field("Dépôt", &self.repo_search)))
                        .child(div().w(px(180.)).child(field("Cote", &self.call_number))),
                )
                .when(self.repo.is_some(), |d| {
                    d.child(div().text_xs().text_color(theme::ok()).child("Dépôt existant lié"))
                })
                .child(h_flex().flex_wrap().gap_1().children(repos.into_iter().map(|(x, name)| {
                    chip(format!("repo-{x}"), name.clone(), false).on_click(cx.listener(move |this, _, window, cx| {
                        this.repo_search.update(cx, |s, cx| s.set_value(name.clone(), window, cx));
                        this.repo = Some(x.clone());
                        cx.notify();
                    }))
                })))
                .child(
                    Button::new("cancel-new-source").ghost().xsmall().label("Choisir une source existante").on_click(
                        cx.listener(|this, _, _, cx| {
                            this.new_source = false;
                            cx.notify();
                        }),
                    ),
                );
        }
        let hits: Vec<(String, String)> = self
            .ged
            .records_of(RecordKind::Source)
            .filter_map(|r| {
                let t = Source::new(r).title();
                (q.is_empty() || sort_key(&t).contains(&q)).then(|| (r.xref.clone().unwrap_or_default(), t))
            })
            .take(8)
            .collect();
        sec.child(v_flex().gap_0p5().children(hits.into_iter().map(|(x, title)| {
            div()
                .id(SharedString::from(format!("src-{x}")))
                .px_2()
                .py_1()
                .rounded_sm()
                .cursor_pointer()
                .overflow_hidden()
                .whitespace_nowrap()
                .text_ellipsis()
                .hover(|d| d.bg(theme::hover()))
                .child(title)
                .on_click(cx.listener(move |this, _, w, cx| this.pick_source(Some(x.clone()), w, cx)))
        })))
        .child(
            Button::new("new-source")
                .ghost()
                .xsmall()
                .label(if q.is_empty() {
                    "+ Nouvelle source pour cet acte (titre automatique)"
                } else {
                    "+ Nouvelle source avec ce titre"
                })
                .on_click(cx.listener(|this, _, w, cx| this.pick_source(None, w, cx))),
        )
    }

    fn render_reference(&self, cx: &mut Context<Self>) -> Div {
        let place_hints = self.place_suggestions(&self.place, cx);
        let page = self.template().format_page(
            &self.fields.iter().map(|(k, _, e)| (k.clone(), e.read(cx).value().to_string())).collect::<Vec<_>>(),
        );
        section("Référence de l'acte")
            .child(
                h_flex()
                    .gap_2()
                    .child(
                        div().w(px(180.)).child(field(format!("Date — {}", self.template().event_label()), &self.date)),
                    )
                    .child(div().flex_1().child(field("Lieu", &self.place))),
            )
            .when_some(place_hints, |d, h| d.child(h))
            .child(
                h_flex()
                    .gap_2()
                    .children(self.fields.iter().map(|(_, label, e)| div().flex_1().child(field(label.clone(), e))))
                    .child(div().w(px(170.)).child(field("Date de l'acte", &self.record_date))),
            )
            .child(h_flex().gap_2().items_center().child(caps("Référence")).child(mono(if page.is_empty() {
                "—".to_string()
            } else {
                page
            })))
            .child(h_flex().gap_1().items_center().child(caps("Qualité").mr_1()).children(
                QUALITY.iter().enumerate().map(|(q, label)| {
                    let on = self.quality == Some(q as u8);
                    chip(format!("quay-{q}"), *label, on).on_click(cx.listener(move |this, _, _, cx| {
                        this.quality = if this.quality == Some(q as u8) { None } else { Some(q as u8) };
                        this.preview = None;
                        cx.notify();
                    }))
                }),
            ))
    }

    fn render_person(&self, idx: usize, row: &PersonRow, cx: &mut Context<Self>) -> impl IntoElement {
        let t = self.template();
        let def = t.role(&row.role).cloned();
        let label = def.as_ref().map(|d| d.label.clone()).unwrap_or_else(|| row.role.clone());
        let removable = def.as_ref().is_some_and(|d| d.multiple || !d.default);
        let id = row.id;
        let status: AnyElement = match &row.choice {
            PersonChoice::Link(x) => h_flex()
                .gap_1()
                .text_xs()
                .text_color(theme::ok())
                .child(format!("Lié à {}", self.ged.record_label(x)))
                .child(mono(self.ged.individual(x).map(|i| i.years()).unwrap_or_default()))
                .into_any_element(),
            PersonChoice::Mention => {
                div().text_xs().text_color(theme::muted()).child("Mentionné sans créer d'individu").into_any_element()
            }
            PersonChoice::Create if row.suggestions.is_empty() => {
                div().text_xs().text_color(theme::muted()).child("Nouvel individu").into_any_element()
            }
            PersonChoice::Create => div()
                .text_xs()
                .text_color(theme::warn())
                .child(format!(
                    "{} correspondance{} possible{}",
                    row.suggestions.len(),
                    if row.suggestions.len() > 1 { "s" } else { "" },
                    if row.suggestions.len() > 1 { "s" } else { "" }
                ))
                .into_any_element(),
        };
        let place_hints: Vec<Div> = row.inputs.iter().filter_map(|(_, e)| self.place_suggestions(e, cx)).collect();
        let cols = row.inputs.len().clamp(2, 4) as u16;
        v_flex()
            .gap_2()
            .p_3()
            .rounded_md()
            .border_1()
            .border_color(match row.choice {
                PersonChoice::Link(_) => theme::ok(),
                PersonChoice::Create if !row.suggestions.is_empty() => theme::warn(),
                _ => theme::border(),
            })
            .bg(theme::surface())
            .child(
                h_flex()
                    .gap_2()
                    .items_center()
                    .child(
                        div()
                            .size(px(18.))
                            .flex_none()
                            .rounded_full()
                            .bg(theme::hover())
                            .flex()
                            .items_center()
                            .justify_center()
                            .text_xs()
                            .child((idx + 1).to_string()),
                    )
                    .child(div().font_weight(FontWeight::SEMIBOLD).child(label))
                    .child(status)
                    .child(div().flex_1())
                    .when(row.choice != PersonChoice::Create, |d| {
                        d.child(
                            Button::new(SharedString::from(format!("unlink-{id}")))
                                .ghost()
                                .xsmall()
                                .label(if matches!(row.choice, PersonChoice::Link(_)) { "Délier" } else { "Annuler" })
                                .on_click(
                                    cx.listener(move |this, _, w, cx| this.set_choice(id, PersonChoice::Create, w, cx)),
                                ),
                        )
                    })
                    .when(removable, |d| {
                        d.child(
                            Button::new(SharedString::from(format!("rm-{id}")))
                                .ghost()
                                .xsmall()
                                .label("Retirer")
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.rows.retain(|r| r.id != id);
                                    this.preview = None;
                                    this.refresh_suggestions(cx);
                                    cx.notify();
                                })),
                        )
                    }),
            )
            .child(div().grid().grid_cols(cols).gap_2().children(row.inputs.iter().map(|(f, e)| field(f.label(), e))))
            .when_some(row.sex.clone(), |d, current| {
                d.child(h_flex().gap_1().items_center().child(caps("Sexe")).children(
                    [("M", "Homme"), ("F", "Femme")].into_iter().map(|(v, label)| {
                        chip(format!("sex-{id}-{v}"), label, current == v).on_click(cx.listener(
                            move |this, _, _, cx| {
                                if let Some(r) = this.rows.iter_mut().find(|r| r.id == id) {
                                    // Second clic : on désélectionne.
                                    r.sex = Some(if r.sex.as_deref() == Some(v) { String::new() } else { v.into() });
                                }
                                this.preview = None;
                                this.refresh_suggestions(cx);
                                cx.notify();
                            },
                        ))
                    }),
                ))
            })
            .children(place_hints)
            .when(row.choice == PersonChoice::Create, |d| {
                d.child(
                    h_flex()
                        .flex_wrap()
                        .gap_1()
                        .children(row.suggestions.iter().map(|s| {
                            let x = s.xref.clone();
                            chip(
                                format!("sug-{id}-{}", s.xref),
                                if s.reason.is_empty() {
                                    format!("Lier à {}", s.label)
                                } else {
                                    format!("Lier à {} · {}", s.label, s.reason)
                                },
                                false,
                            )
                            .on_click(cx.listener(move |this, _, w, cx| {
                                this.set_choice(id, PersonChoice::Link(x.clone()), w, cx)
                            }))
                        }))
                        .when(def.as_ref().is_some_and(|d| d.relation != RoleRelation::Principal), |d| {
                            d.child(chip(format!("mention-{id}"), "Mentionner sans créer", false).on_click(
                                cx.listener(move |this, _, w, cx| this.set_choice(id, PersonChoice::Mention, w, cx)),
                            ))
                        }),
                )
            })
    }

    fn render_people(&self, cx: &mut Context<Self>) -> Div {
        let t = self.template();
        let addable: Vec<(String, String)> = t
            .roles
            .iter()
            .filter(|r| r.multiple || !self.rows.iter().any(|row| row.role == r.key))
            .map(|r| (r.key.clone(), r.label.clone()))
            .collect();
        let rows: Vec<AnyElement> =
            self.rows.iter().enumerate().map(|(i, r)| self.render_person(i, r, cx).into_any_element()).collect();
        section("Personnes citées")
            .children(rows)
            .child(h_flex().gap_1().flex_wrap().items_center().child(caps("Ajouter :").mr_1()).children(
                addable.into_iter().map(|(key, label)| {
                    chip(format!("add-{key}"), format!("+ {label}"), false).on_click(cx.listener(
                        move |this, _, w, cx| {
                            this.add_row(&key, w, cx);
                            this.preview = None;
                            cx.notify();
                        },
                    ))
                }),
            ))
            .child(
                Checkbox::new("estimate")
                    .checked(self.estimate_births)
                    .label("Créer une naissance estimée d'après l'âge déclaré (si absente)")
                    .on_click(cx.listener(|this, checked: &bool, _, cx| {
                        this.estimate_births = *checked;
                        this.preview = None;
                        cx.notify();
                    })),
            )
    }

    fn render_footer(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let linked = self.rows.iter().filter(|r| matches!(r.choice, PersonChoice::Link(_))).count();
        let to_decide =
            self.rows.iter().filter(|r| r.choice == PersonChoice::Create && !r.suggestions.is_empty()).count();
        let mentions = self.rows.iter().filter(|r| r.choice == PersonChoice::Mention).count();
        let mut summary = format!("{linked} lié{}", if linked > 1 { "s" } else { "" });
        if mentions > 0 {
            summary += &format!(" · {mentions} mention{}", if mentions > 1 { "s" } else { "" });
        }
        if to_decide > 0 {
            summary += &format!(" · {to_decide} à décider");
        }
        if self.saved_count > 0 {
            summary += &format!(
                " · {} acte{} enregistré{}",
                self.saved_count,
                if self.saved_count > 1 { "s" } else { "" },
                if self.saved_count > 1 { "s" } else { "" }
            );
        }
        h_flex()
            .h(px(48.))
            .flex_none()
            .px_4()
            .gap_2()
            .items_center()
            .bg(theme::panel())
            .border_t_1()
            .border_color(theme::border())
            .child(
                div().text_xs().text_color(if to_decide > 0 { theme::warn() } else { theme::muted() }).child(summary),
            )
            .when_some(self.error.clone(), |d, e| d.child(div().text_xs().text_color(theme::danger()).child(e)))
            .child(div().flex_1())
            .child(
                Button::new("preview")
                    .ghost()
                    .small()
                    .label(if self.preview.is_some() { "Masquer l'aperçu" } else { "Aperçu GEDCOM" })
                    .on_click(cx.listener(|this, _, _, cx| this.toggle_preview(cx))),
            )
            .child(
                Button::new("save-entry")
                    .small()
                    .label("Enregistrer")
                    .on_click(cx.listener(|this, _, _, cx| this.save(false, cx))),
            )
            .child(
                Button::new("save-next")
                    .primary()
                    .small()
                    .label("Enregistrer et acte suivant")
                    .on_click(cx.listener(|this, _, _, cx| this.save(true, cx))),
            )
    }
}

impl Render for EntryForm {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let template_bar = h_flex()
            .px_4()
            .h(px(44.))
            .flex_none()
            .gap_1()
            .items_center()
            .border_b_1()
            .border_color(theme::border())
            .bg(theme::panel())
            .child(div().font_weight(FontWeight::SEMIBOLD).mr_3().child("Saisie d'acte"))
            .children(self.templates.iter().enumerate().map(|(i, t)| {
                chip(format!("tpl-{i}"), t.name.clone(), i == self.template)
                    .on_click(cx.listener(move |this, _, w, cx| this.set_template(i, w, cx)))
            }))
            .child(div().flex_1())
            .child(
                Button::new("close-entry")
                    .ghost()
                    .small()
                    .label("Fermer")
                    .on_click(cx.listener(|_, _, _, cx| cx.emit(EntryFormEvent::Close))),
            );

        let preview = self.preview.clone();
        let form = div().id("entry-scroll").flex_1().min_w_0().overflow_y_scroll().track_scroll(&self.scroll).child(
            v_flex()
                .p_4()
                .gap_3()
                .max_w(px(980.))
                .child(self.render_source(cx))
                .child(self.render_reference(cx))
                .child(self.render_people(cx))
                .child(
                    section("Transcription").child(Textarea::new(&self.transcription).h(px(110.))).child(
                        h_flex().gap_2().items_center().child(
                            Button::new("auto-text")
                                .ghost()
                                .xsmall()
                                .label("Générer un résumé depuis le formulaire")
                                .on_click(cx.listener(|this, _, w, cx| this.fill_transcription(w, cx))),
                        ),
                    ),
                )
                .when_some(preview, |d, p| {
                    d.child(
                        section("Aperçu GEDCOM 7 généré").child(match p {
                            Ok(text) => div()
                                .font_family("monospace")
                                .text_xs()
                                .whitespace_normal()
                                .children(text.lines().map(|l| div().child(l.to_string())).collect::<Vec<_>>())
                                .into_any_element(),
                            Err(e) => div().text_color(theme::danger()).child(e).into_any_element(),
                        }),
                    )
                }),
        );

        v_flex()
            .flex_1()
            .min_w_0()
            .min_h_0()
            .bg(theme::bg())
            .child(template_bar)
            .child(h_flex().flex_1().min_h_0().items_stretch().child(self.render_image(cx)).child(form))
            .child(self.render_footer(cx))
    }
}
