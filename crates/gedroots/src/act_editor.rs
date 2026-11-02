//! Éditeur d'acte, ouvert dans la fiche d'une source : à gauche l'image de
//! l'acte, à droite l'événement (n'importe quel événement GEDCOM), sa référence
//! et les personnes citées avec leur rôle (lier à un individu existant, créer,
//! ou mentionner seulement). Il n'y a pas de modèles : les champs de chaque
//! personne et les liens familiaux découlent de son rôle.
//!
//! L'éditeur travaille sur une copie du document ; l'écriture réelle est faite
//! par l'espace de travail quand il reçoit [`ActEditorEvent::Save`].

use crate::date_field::DateField;
use crate::editor::{PickerState, choice_with_search};
use crate::images::picture;
use crate::theme;
use gedcom::*;
use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::checkbox::Checkbox;
use gpui_kit::component::input::{Input, InputEvent, InputState, Textarea, TextareaState};
use gpui_kit::component::menu::{DropdownMenu, PopupMenuItem};
use gpui_kit::component::select::{Select, SelectEvent};
use gpui_kit::component::{Disableable, Sizable, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use std::path::{Path, PathBuf};

pub enum ActEditorEvent {
    /// Écrire l'acte ; `next` : enchaîner sur l'acte suivant du même registre.
    Save {
        act: Box<Act>,
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
    /// Nom, prénom, âge ou date : relance les suggestions.
    Name,
}

struct PersonRow {
    role: ActRole,
    of: usize,
    inputs: Vec<(PersonField, Entity<InputState>)>,
    /// Sexe choisi par boutons ; `None` : fixé par le rôle ou sans objet.
    sex: Option<String>,
    choice: PersonChoice,
    suggestions: Vec<Suggestion>,
    _subs: Vec<Subscription>,
}

pub struct ActEditor {
    ged: Gedcom,
    base_dir: Option<PathBuf>,
    /// L'acte ouvert : source, version d'origine, note gardée telle quelle.
    base: Act,
    event: Entity<PickerState>,
    event_type: Entity<InputState>,
    date: Entity<DateField>,
    place: Entity<InputState>,
    page: Entity<InputState>,
    record_date: Entity<DateField>,
    transcription: Entity<TextareaState>,
    quality: Option<u8>,
    media: MediaChoice,
    estimate_births: bool,
    rows: Vec<PersonRow>,
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

impl EventEmitter<ActEditorEvent> for ActEditor {}

const QUALITY: [&str; 4] = ["Peu fiable", "Douteuse", "Indirecte", "Directe"];

/// Événements individuels proposés (les familiaux viennent de [`gedcom::act::FAMILY_ACT_EVENTS`]).
const INDIVIDUAL_ACT_EVENTS: &[&str] = &[
    "BIRT", "BAPM", "DEAT", "BURI", "CREM", "CENS", "CONF", "FCOM", "ADOP", "NATU", "EMIG", "IMMI", "EDUC", "GRAD",
    "RETI", "WILL", "PROB",
];

/// Valeur de la liste des événements → (balise, type, événement familial).
fn parse_event(v: &str) -> (String, String, bool) {
    let (family, rest) = match v.strip_prefix("FAM:") {
        Some(r) => (true, r),
        None => (false, v),
    };
    match rest.split_once(':') {
        Some((tag, ty)) => (tag.to_string(), ty.to_string(), family),
        None => (rest.to_string(), String::new(), family),
    }
}

/// Valeur de la liste des événements pour un acte.
fn event_value(act: &Act, custom: &[EventType]) -> String {
    if act.event == "EVEN" {
        let fam = if act.family_even { "FAM:" } else { "" };
        let known = custom.iter().any(|t| t.family == act.family_even && t.ty == act.event_type.trim());
        return if known { format!("{fam}EVEN:{}", act.event_type.trim()) } else { format!("{fam}EVEN") };
    }
    act.event.clone()
}

impl ActEditor {
    pub fn new(ged: Gedcom, ged_path: Option<PathBuf>, act: Act, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let places = ged.places().into_iter().map(|p| (p.short(), p.name)).collect();
        // Événements : individuels, familiaux, types personnalisés du fichier, puis « Autre… ».
        let custom = ged.event_types();
        let mut options: Vec<(String, String)> = INDIVIDUAL_ACT_EVENTS
            .iter()
            .chain(gedcom::act::FAMILY_ACT_EVENTS.iter().filter(|t| **t != "MARS"))
            .map(|t| (t.to_string(), event_label(t).unwrap_or(t).to_string()))
            .collect();
        for t in &custom {
            let fam = if t.family { "FAM:" } else { "" };
            options.push((format!("{fam}EVEN:{}", t.ty), t.label.clone()));
        }
        options.push(("EVEN".into(), "Autre événement…".into()));
        options.push(("FAM:EVEN".into(), "Autre événement familial…".into()));
        let current = event_value(&act, &custom);
        if !options.iter().any(|(v, _)| *v == current) {
            options.insert(0, (current.clone(), act.event_label()));
        }
        let opts: Vec<(&str, &str)> = options.iter().map(|(v, l)| (v.as_str(), l.as_str())).collect();
        let event = choice_with_search(&opts, &current, true, window, cx);
        let mk = |cx: &mut Context<Self>, window: &mut Window, ph: &str, v: &str| {
            let (ph, v) = (ph.to_string(), v.to_string());
            cx.new(|cx| InputState::new(window, cx).placeholder(ph).default_value(v))
        };
        let custom_type =
            if current == "EVEN" || current == "FAM:EVEN" { act.event_type.clone() } else { String::new() };
        let event_type = mk(cx, window, "ex. Registre matricule", &custom_type);
        let page = mk(cx, window, "Acte n° 27, vue 12", &act.page);
        let place = mk(cx, window, "Commune…", &act.place);
        // Colonnes étroites (grille à 4) : exemple sans « ex. » pour qu'il ne soit pas tronqué.
        let short = |value: &str, phrase: &str, window: &mut Window, cx: &mut Context<Self>| {
            cx.new(|cx| DateField::new(value, phrase, window, cx).with_placeholder("3 mars 1912", window, cx))
        };
        let date = short(&act.date, &act.date_phrase, window, cx);
        let record_date = short(&act.record_date, &act.record_date_phrase, window, cx);
        let transcription = {
            let t = act.transcription.clone();
            cx.new(|cx| TextareaState::new(window, cx).rows(4).default_value(t))
        };
        let mut this = Self {
            ged,
            base_dir: ged_path.and_then(|p| p.parent().map(Path::to_path_buf)),
            quality: act.quality,
            media: act.media.clone(),
            estimate_births: act.estimate_births,
            base: act.clone(),
            event,
            event_type,
            date,
            place,
            page,
            record_date,
            transcription,
            rows: Vec::new(),
            place_focus: None,
            places,
            preview: None,
            error: None,
            zoom: 1.0,
            saved_count: 0,
            scroll: ScrollHandle::new(),
            _subs: Vec::new(),
        };
        if this.base.original.is_none() && this.media == MediaChoice::None {
            this.media = this.default_media();
        }
        for (e, kind) in [
            (this.event_type.clone(), Kind::Plain),
            (this.page.clone(), Kind::Plain),
            (this.date.read(cx).input().clone(), Kind::Name),
            (this.place.clone(), Kind::Place),
            (this.record_date.read(cx).input().clone(), Kind::Plain),
        ] {
            let sub = this.watch(&e, kind, window, cx);
            this._subs.push(sub);
        }
        let t = this.transcription.clone();
        let sub = cx.subscribe_in(&t, window, |this, _, ev: &InputEvent, _, cx| {
            if matches!(ev, InputEvent::Change) {
                this.preview = None;
                cx.notify();
            }
        });
        this._subs.push(sub);
        let e = this.event.clone();
        let sub = cx.subscribe_in(&e, window, |this, _, ev: &SelectEvent<_>, window, cx| {
            if let SelectEvent::Confirm(Some(_)) = ev {
                this.event_changed(window, cx);
            }
        });
        this._subs.push(sub);
        this.set_rows(act.participants, window, cx);
        this
    }

    fn watch(
        &mut self,
        e: &Entity<InputState>,
        kind: Kind,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Subscription {
        cx.subscribe_in(e, window, move |this, input, ev: &InputEvent, _window, cx| {
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
                        Kind::Plain => {}
                    }
                }
                _ => return,
            }
            cx.notify();
        })
    }

    /// Événement choisi : (balise, type, familial).
    fn event(&self, cx: &App) -> (String, String, bool) {
        let v = self.event.read(cx).selected_value().cloned().unwrap_or_default();
        let (tag, ty, family) = parse_event(&v);
        if tag == "EVEN" && ty.is_empty() {
            return (tag, self.event_type.read(cx).value().trim().to_string(), family);
        }
        (tag, ty, family)
    }

    /// Recrée les lignes des personnes (les champs dépendent du rôle et de l'événement).
    fn set_rows(&mut self, people: Vec<Participant>, window: &mut Window, cx: &mut Context<Self>) {
        let mut act = self.collect(cx);
        act.participants = people;
        self.rows.clear();
        for i in 0..act.participants.len() {
            let p = act.participants[i].clone();
            let mut inputs = Vec::new();
            let mut subs = Vec::new();
            let mut sex = None;
            for f in act.fields_of(i) {
                if f == PersonField::Sex {
                    if act.imposed_sex(i).is_none() {
                        sex = Some(p.get(PersonField::Sex).to_string());
                    }
                    continue;
                }
                let kind = match f {
                    PersonField::Surname | PersonField::Given | PersonField::Age => Kind::Name,
                    PersonField::Residence | PersonField::Birthplace => Kind::Place,
                    _ => Kind::Plain,
                };
                let ph = match f {
                    PersonField::Age => "32 ans",
                    PersonField::Kinship => "fils, épouse, voisin",
                    _ => "",
                }
                .to_string();
                let v = p.get(f).to_string();
                let e = cx.new(|cx| InputState::new(window, cx).placeholder(ph).default_value(v));
                subs.push(self.watch(&e, kind, window, cx));
                inputs.push((f, e));
            }
            self.rows.push(PersonRow {
                role: p.role,
                of: p.of,
                inputs,
                sex,
                choice: p.choice.clone(),
                suggestions: Vec::new(),
                _subs: subs,
            });
        }
        self.preview = None;
        self.refresh_suggestions(cx);
    }

    fn default_media(&self) -> MediaChoice {
        let Some(s) = self.ged.get(&self.base.source) else { return MediaChoice::None };
        Source::new(s)
            .media_xrefs()
            .into_iter()
            .find(|m| self.ged.get(m).is_some_and(|n| Media::new(n).is_image()))
            .map(|m| MediaChoice::Existing(m.to_string()))
            .unwrap_or(MediaChoice::None)
    }

    /// Construit l'acte à partir des champs.
    fn collect(&self, cx: &App) -> Act {
        let v = |e: &Entity<InputState>| e.read(cx).value().trim().to_string();
        let mut a = self.base.clone();
        (a.event, a.event_type, a.family_even) = self.event(cx);
        // Date impossible : le texte saisi est gardé, `save` signale l'erreur.
        let date = |d: &Entity<DateField>| {
            let f = d.read(cx);
            f.value(cx).unwrap_or_else(|_| (f.input().read(cx).value().trim().to_string(), String::new()))
        };
        (a.date, a.date_phrase) = date(&self.date);
        a.place = v(&self.place);
        a.page = v(&self.page);
        (a.record_date, a.record_date_phrase) = date(&self.record_date);
        a.transcription = self.transcription.read(cx).value().to_string();
        a.quality = self.quality;
        a.media = self.media.clone();
        a.estimate_births = self.estimate_births;
        a.participants = self
            .rows
            .iter()
            .map(|r| Participant {
                role: r.role,
                of: r.of,
                values: r
                    .inputs
                    .iter()
                    .map(|(f, i)| (*f, v(i)))
                    .chain(r.sex.clone().map(|s| (PersonField::Sex, s)))
                    .collect(),
                choice: r.choice.clone(),
            })
            .collect();
        a
    }

    fn refresh_suggestions(&mut self, cx: &mut Context<Self>) {
        let act = self.collect(cx);
        for (i, r) in self.rows.iter_mut().enumerate() {
            r.suggestions = if r.choice == PersonChoice::Create { act.suggestions(&self.ged, i) } else { Vec::new() };
        }
    }

    fn event_changed(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let act = self.collect(cx);
        let mut people = act.participants.clone();
        if people.iter().all(|p| p.is_blank() && p.choice == PersonChoice::Create) {
            // Rien de saisi : les personnes attendues pour ce nouvel événement.
            people = act.default_participants();
        } else if act.is_family() {
            while people.iter().filter(|p| p.role == ActRole::Principal).count() < 2 {
                people.insert(1, Participant::new(ActRole::Principal));
            }
        }
        self.set_rows(people, window, cx);
        self.error = None;
        cx.notify();
    }

    fn add_person(&mut self, role: ActRole, window: &mut Window, cx: &mut Context<Self>) {
        let mut people = self.collect(cx).participants;
        // Une personne principale de plus va après les autres personnes principales.
        let at = match role {
            ActRole::Principal => people.iter().rposition(|p| p.role == ActRole::Principal).map_or(0, |i| i + 1),
            _ => people.len(),
        };
        people.insert(at, Participant::new(role));
        self.set_rows(people, window, cx);
        cx.notify();
    }

    fn remove_person(&mut self, idx: usize, window: &mut Window, cx: &mut Context<Self>) {
        let mut people = self.collect(cx).participants;
        if idx < people.len() {
            people.remove(idx);
        }
        self.set_rows(people, window, cx);
        cx.notify();
    }

    fn set_choice(&mut self, idx: usize, choice: PersonChoice, window: &mut Window, cx: &mut Context<Self>) {
        let mut people = self.collect(cx).participants;
        let Some(p) = people.get_mut(idx) else { return };
        // En liant, on complète les champs vides avec l'individu choisi.
        if let PersonChoice::Link(x) = &choice
            && let Some(ind) = self.ged.individual(x)
        {
            for (f, val) in [
                (PersonField::Surname, ind.surname()),
                (PersonField::Given, ind.given()),
                (PersonField::Sex, ind.sex().into()),
            ] {
                if p.get(f).is_empty() && val != "U" {
                    p.set(f, val);
                }
            }
        }
        p.choice = choice;
        self.set_rows(people, window, cx);
        self.error = None;
        cx.notify();
    }

    fn set_of(&mut self, idx: usize, of: usize, cx: &mut Context<Self>) {
        if let Some(r) = self.rows.get_mut(idx) {
            r.of = of;
        }
        self.preview = None;
        self.refresh_suggestions(cx);
        cx.notify();
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
        let act = self.collect(cx);
        let dates = [&self.date, &self.record_date].map(|d| d.read(cx).value(cx));
        let checked = dates.into_iter().try_for_each(|d| d.map(|_| ())).and_then(|_| act.validate(&self.ged));
        match checked {
            Ok(()) => cx.emit(ActEditorEvent::Save { act: Box::new(act), next }),
            Err(e) => {
                self.error = Some(e.into());
                cx.notify();
            }
        }
    }

    /// Appelé par l'espace de travail après un enregistrement réussi, pour enchaîner
    /// sur l'acte suivant du même registre.
    pub fn saved(&mut self, ged: Gedcom, result: &ActResult, window: &mut Window, cx: &mut Context<Self>) {
        let mut n = self.collect(cx).next();
        if let MediaChoice::NewFile(_) = n.media {
            n.media = result.media.clone().map(MediaChoice::Existing).unwrap_or_default();
        }
        self.ged = ged;
        self.saved_count += 1;
        self.places = self.ged.places().into_iter().map(|p| (p.short(), p.name)).collect();
        self.base = n.clone();
        self.media = n.media.clone();
        self.page.update(cx, |s, cx| s.set_value(n.page.clone(), window, cx));
        self.date.update(cx, |s, cx| s.clear(window, cx));
        self.record_date.update(cx, |s, cx| s.clear(window, cx));
        self.transcription.update(cx, |s, cx| s.set_value("", window, cx));
        self.set_rows(n.participants, window, cx);
        self.error = None;
        // Retour en haut du formulaire, curseur sur la première personne.
        self.scroll.set_offset(point(px(0.), px(0.)));
        if let Some((_, first)) = self.rows.first().and_then(|r| r.inputs.first()) {
            first.update(cx, |s, cx| s.focus(window, cx));
        }
        cx.notify();
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

/// Colonnes de la grille commune aux sections « Acte » et « Personnes ».
const GRID: u16 = 4;

/// Largeur (en colonnes) de chaque champ d'une personne : le nom et les prénoms
/// occupent chacun une demi-ligne, la note une ligne entière, les autres une colonne ;
/// le dernier champ d'une ligne incomplète s'étire jusqu'au bord pour ne pas laisser de trou.
fn grid_spans(fields: impl Iterator<Item = PersonField>) -> Vec<u16> {
    let mut spans: Vec<u16> = fields
        .map(|f| match f {
            PersonField::Surname | PersonField::Given => GRID / 2,
            PersonField::Note => GRID,
            _ => 1,
        })
        .collect();
    let mut used = 0;
    for i in 0..spans.len() {
        if used + spans[i] > GRID {
            if let Some(prev) = i.checked_sub(1) {
                spans[prev] += GRID - used;
            }
            used = 0;
        }
        used += spans[i];
    }
    if used > 0
        && used < GRID
        && let Some(last) = spans.last_mut()
    {
        *last += GRID - used;
    }
    spans
}

fn date_cell(label: impl Into<SharedString>, date: &Entity<DateField>) -> Div {
    v_flex().gap_1().min_w_0().child(caps(label)).child(date.clone())
}

fn field(label: impl Into<SharedString>, input: &Entity<InputState>) -> Div {
    v_flex().gap_1().min_w_0().child(caps(label)).child(Input::new(input).small())
}

fn chip(id: impl Into<SharedString>, label: impl Into<SharedString>, on: bool) -> Stateful<Div> {
    div()
        .id(ElementId::Name(id.into()))
        .px_2()
        .py(px(3.))
        .rounded(theme::RADIUS_SM)
        .border_1()
        .text_xs()
        .cursor_pointer()
        .border_color(if on { theme::accent() } else { theme::border_strong() })
        .bg(if on { theme::accent_soft() } else { theme::surface() })
        .text_color(if on { theme::text() } else { theme::muted() })
        .hover(|d| d.border_color(theme::accent()))
        .child(label.into())
}

fn section(title: impl Into<SharedString>) -> Div {
    v_flex()
        .gap_2()
        .p_3()
        .rounded(theme::RADIUS_SM)
        .border_1()
        .border_color(theme::border())
        .bg(theme::panel())
        .child(caps(title))
}

impl ActEditor {
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
            .ged
            .get(&self.base.source)
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
                .bg(theme::viewer())
                .child({
                    // À 100 % l'image tient dans le panneau ; au-delà on la grossit et on fait défiler.
                    let (w, h) =
                        imagesize::size(p).map(|s| (s.width as f32, s.height as f32)).unwrap_or((1000., 1400.));
                    let width = 460. * zoom;
                    // Décodée pour la largeur affichée (le panneau fait rarement plus de 900 px).
                    let shown = width.max(900.);
                    let i = picture(p.clone(), shown, shown * h / w.max(1.), ObjectFit::Contain);
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
                .p_4()
                .text_color(theme::muted())
                .child("Aucune image")
                .into_any_element(),
        };
        v_flex()
            .w(relative(0.42))
            .max_w(px(620.))
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
                        |d, f| d.child(chip("media-new", f, true)),
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

    fn render_act(&self, act: &Act, cx: &mut Context<Self>) -> Div {
        let place_hints = self.place_suggestions(&self.place, cx);
        let (tag, _, _) = parse_event(&self.event.read(cx).selected_value().cloned().unwrap_or_default());
        let free_type = tag == "EVEN" && act.event == "EVEN" && {
            let v = self.event.read(cx).selected_value().cloned().unwrap_or_default();
            v == "EVEN" || v == "FAM:EVEN"
        };
        section("Acte")
            .child(
                div()
                    .grid()
                    .grid_cols(GRID)
                    .gap_2()
                    .child(
                        v_flex()
                            .gap_1()
                            .min_w_0()
                            .col_span(if free_type { 1 } else { 2 })
                            .child(caps("Événement"))
                            .child(
                                Select::new(&self.event)
                                    .small()
                                    .placeholder("Choisir…")
                                    .search_placeholder("Rechercher…"),
                            ),
                    )
                    .when(free_type, |d| d.child(field("Type", &self.event_type)))
                    .child(date_cell("Date", &self.date))
                    .child(date_cell("Date de l'acte", &self.record_date))
                    .child(field("Lieu", &self.place).col_span(2))
                    .child(field("Référence", &self.page).col_span(2)),
            )
            .when_some(place_hints, |d, h| d.child(h))
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

    fn render_person(&self, act: &Act, idx: usize, row: &PersonRow, cx: &mut Context<Self>) -> impl IntoElement {
        let label = act.role_label(idx);
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
        // Père, mère ou conjoint de quelle personne principale ?
        let principals = act.principal_count();
        let of_chips: Vec<Stateful<Div>> = if row.role.relative() && principals > 1 {
            (0..principals)
                .map(|k| {
                    chip(format!("of-{idx}-{k}"), format!("de {}", act.principal_name(k)), row.of == k)
                        .on_click(cx.listener(move |this, _, _, cx| this.set_of(idx, k, cx)))
                })
                .collect()
        } else {
            Vec::new()
        };
        let place_hints: Vec<Div> = row.inputs.iter().filter_map(|(_, e)| self.place_suggestions(e, cx)).collect();
        v_flex()
            .gap_2()
            .p_3()
            .rounded(theme::RADIUS_SM)
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
                    .child(div().font_weight(FontWeight::SEMIBOLD).child(label))
                    .children(of_chips)
                    .child(status)
                    .child(div().flex_1())
                    .when(row.choice != PersonChoice::Create, |d| {
                        d.child(
                            Button::new(SharedString::from(format!("unlink-{idx}")))
                                .ghost()
                                .xsmall()
                                .label(if matches!(row.choice, PersonChoice::Link(_)) { "Délier" } else { "Annuler" })
                                .on_click(
                                    cx.listener(move |this, _, w, cx| {
                                        this.set_choice(idx, PersonChoice::Create, w, cx)
                                    }),
                                ),
                        )
                    })
                    .child(
                        Button::new(SharedString::from(format!("rm-{idx}")))
                            .ghost()
                            .xsmall()
                            .icon(IconName::Close)
                            .tooltip("Retirer de l'acte")
                            .on_click(cx.listener(move |this, _, w, cx| this.remove_person(idx, w, cx))),
                    ),
            )
            .child(
                div().grid().grid_cols(GRID).gap_2().children(
                    row.inputs
                        .iter()
                        .zip(grid_spans(row.inputs.iter().map(|(f, _)| *f)))
                        .map(|((f, e), span)| field(f.label(), e).col_span(span)),
                ),
            )
            .when_some(row.sex.clone(), |d, current| {
                d.child(h_flex().gap_1().items_center().child(caps("Sexe")).children(
                    [("M", "Homme"), ("F", "Femme")].into_iter().map(|(v, label)| {
                        chip(format!("sex-{idx}-{v}"), label, current == v).on_click(cx.listener(
                            move |this, _, _, cx| {
                                if let Some(r) = this.rows.get_mut(idx) {
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
                                format!("sug-{idx}-{}", s.xref),
                                if s.reason.is_empty() {
                                    format!("Lier à {}", s.label)
                                } else {
                                    format!("Lier à {} · {}", s.label, s.reason)
                                },
                                false,
                            )
                            .on_click(cx.listener(move |this, _, w, cx| {
                                this.set_choice(idx, PersonChoice::Link(x.clone()), w, cx)
                            }))
                        }))
                        .when(row.role != ActRole::Principal, |d| {
                            d.child(chip(format!("mention-{idx}"), "Mentionner sans créer", false).on_click(
                                cx.listener(move |this, _, w, cx| this.set_choice(idx, PersonChoice::Mention, w, cx)),
                            ))
                        }),
                )
            })
    }

    fn render_people(&self, act: &Act, cx: &mut Context<Self>) -> Div {
        let rows: Vec<AnyElement> =
            self.rows.iter().enumerate().map(|(i, r)| self.render_person(act, i, r, cx).into_any_element()).collect();
        // « + » : une personne de plus, avec son rôle.
        let family = act.is_family();
        let event = act.event.clone();
        let next_principal = act.principal_count();
        let me = cx.entity().downgrade();
        let add = Button::new("add-person")
            .ghost()
            .xsmall()
            .icon(IconName::Plus)
            .tooltip("Ajouter une personne")
            .dropdown_menu(move |mut menu, _, _| {
                for role in ActRole::ALL {
                    let label = role.label(&event, family, if role == ActRole::Principal { next_principal } else { 0 });
                    let me = me.clone();
                    menu = menu.item(PopupMenuItem::new(label).on_click(move |_, window, cx| {
                        let me = me.clone();
                        window.defer(cx, move |window, cx| {
                            let _ = me.update(cx, |this, cx| this.add_person(role, window, cx));
                        });
                    }));
                }
                menu
            });
        v_flex()
            .gap_2()
            .p_3()
            .rounded(theme::RADIUS_SM)
            .border_1()
            .border_color(theme::border())
            .bg(theme::panel())
            .child(h_flex().gap_1().items_center().child(caps("Personnes")).child(add))
            .children(rows)
            .when(self.rows.iter().any(|r| r.inputs.iter().any(|(f, _)| *f == PersonField::Age)), |d| {
                d.child(
                    Checkbox::new("estimate")
                        .checked(self.estimate_births)
                        .label("Naissance estimée d'après l'âge déclaré")
                        .on_click(cx.listener(|this, checked: &bool, _, cx| {
                            this.estimate_births = *checked;
                            this.preview = None;
                            cx.notify();
                        })),
                )
            })
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
            let n = self.saved_count;
            summary +=
                &format!(" · {n} acte{} enregistré{}", if n > 1 { "s" } else { "" }, if n > 1 { "s" } else { "" });
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
                Button::new("save-act")
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

impl Render for ActEditor {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let act = self.collect(cx);
        let title = if self.base.original.is_some() { act.title() } else { "Nouvel acte".to_string() };
        let header = h_flex()
            .px_3()
            .h(px(40.))
            .flex_none()
            .gap_2()
            .items_center()
            .border_b_1()
            .border_color(theme::border())
            .bg(theme::panel())
            .child(
                Button::new("close-act")
                    .ghost()
                    .small()
                    .icon(IconName::ChevronLeft)
                    .label(self.ged.record_label(&self.base.source))
                    .tooltip("Retour à la fiche de la source")
                    .on_click(cx.listener(|_, _, _, cx| cx.emit(ActEditorEvent::Close))),
            )
            .child(div().text_color(theme::muted()).child("›"))
            .child(div().font_weight(FontWeight::SEMIBOLD).child(title));

        let preview = self.preview.clone();
        let form = div().id("act-scroll").flex_1().min_w_0().overflow_y_scroll().track_scroll(&self.scroll).child(
            v_flex()
                .p_4()
                .gap_3()
                .max_w(px(980.))
                .child(self.render_act(&act, cx))
                .child(self.render_people(&act, cx))
                .child(
                    section("Transcription").child(Textarea::new(&self.transcription).h(px(110.))).child(
                        h_flex().gap_2().items_center().child(
                            Button::new("auto-text")
                                .ghost()
                                .xsmall()
                                .label("Générer un résumé")
                                .on_click(cx.listener(|this, _, w, cx| this.fill_transcription(w, cx))),
                        ),
                    ),
                )
                .when_some(preview, |d, p| {
                    d.child(
                        section("Aperçu GEDCOM").child(match p {
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
            .child(header)
            .child(h_flex().flex_1().min_h_0().items_stretch().child(self.render_image(cx)).child(form))
            .child(self.render_footer(cx))
    }
}
