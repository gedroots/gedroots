//! Record editor, opened in a source's page: on the left the record's image, on the right the event
//! (any GEDCOM event), its reference and the people named with their role (link to an existing
//! individual, create, or only mention). There are no templates: each person's fields and the
//! family links follow from their role.
//!
//! The editor works on a copy of the document; the actual write is done by the workspace when it
//! receives [`ActEditorEvent::Save`].

use crate::date_field::DateField;
use crate::editor::{PickerState, choice_with_search, picker};
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
use rust_i18n::t;
use std::path::{Path, PathBuf};
use std::rc::Rc;

pub enum ActEditorEvent {
    /// Write the record; `next`: move on to the next record of the same register.
    Save {
        act: Box<Act>,
        next: bool,
    },
    Close,
    /// Choose an existing individual for the person at `idx`; `suggestions` (individual, why)
    /// come first.
    PickPerson {
        idx: usize,
        suggestions: Vec<(String, String)>,
    },
    /// Typing in a place field (`changed`: the text changed): the workspace holds the place menu,
    /// shared by every Place field.
    PlaceInput {
        input: Entity<InputState>,
        changed: bool,
    },
}

/// Place menu under a Place field, provided by the workspace.
pub type PlaceMenu = Rc<dyn Fn(&Entity<InputState>, &mut App) -> Option<AnyElement>>;

/// Role of an input field, to know what to recompute when it changes.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Plain,
    /// Place: suggests the file's places.
    Place,
    /// Surname, given name, age or date: refreshes the suggestions.
    Name,
}

struct PersonRow {
    role: ActRole,
    of: usize,
    inputs: Vec<(PersonField, Entity<InputState>)>,
    /// Sex chosen with buttons; `None`: set by the role or not applicable.
    sex: Option<String>,
    choice: PersonChoice,
    suggestions: Vec<Suggestion>,
    _subs: Vec<Subscription>,
}

pub struct ActEditor {
    ged: Gedcom,
    base_dir: Option<PathBuf>,
    /// The open record: source, original version, note kept as is.
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
    /// Displayed page of the record's image (from 0).
    media_page: usize,
    /// Any media of the file, to link a file already imported.
    media_picker: Entity<PickerState>,
    estimate_births: bool,
    rows: Vec<PersonRow>,
    /// Active place field (to show the suggested places under this field).
    place_focus: Option<EntityId>,
    place_menu: Option<PlaceMenu>,
    preview: Option<Result<String, String>>,
    /// Guided tour: the “Link to …” suggestion of this individual is highlighted.
    tour_link: Option<String>,
    error: Option<SharedString>,
    zoom: crate::viewer::Zoom,
    saved_count: usize,
    scroll: ScrollHandle,
    _subs: Vec<Subscription>,
}

impl EventEmitter<ActEditorEvent> for ActEditor {}

fn quality_labels() -> [std::borrow::Cow<'static, str>; 4] {
    [t!("act.quality_0"), t!("act.quality_1"), t!("act.quality_2"), t!("act.quality_3")]
}

/// Individual events offered (family events come from [`gedcom::act::FAMILY_ACT_EVENTS`]).
const INDIVIDUAL_ACT_EVENTS: &[&str] = &[
    "BIRT", "BAPM", "DEAT", "BURI", "CREM", "CENS", "CONF", "FCOM", "ADOP", "NATU", "EMIG", "IMMI", "EDUC", "GRAD",
    "RETI", "WILL", "PROB",
];

/// Value of the event list → (tag, type, family event).
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

/// Value of the event list for a record.
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
        // Events: individual, family, custom types of the file, then “Other…”.
        let custom = ged.event_types();
        let mut options: Vec<(String, String)> = INDIVIDUAL_ACT_EVENTS
            .iter()
            .chain(gedcom::act::FAMILY_ACT_EVENTS.iter().filter(|t| **t != "MARS"))
            .map(|t| (t.to_string(), event_name(t)))
            .collect();
        for t in &custom {
            let fam = if t.family { "FAM:" } else { "" };
            options.push((format!("{fam}EVEN:{}", t.ty), t.label.clone()));
        }
        options.push(("EVEN".into(), t!("act.other_event").into()));
        options.push(("FAM:EVEN".into(), t!("act.other_family_event").into()));
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
        let event_type = mk(cx, window, &t!("act.type_placeholder"), &custom_type);
        let page = mk(cx, window, &t!("act.page_placeholder"), &act.page);
        let place = mk(cx, window, &t!("event_dialog.town"), &place_label(&act.place));
        // Narrow columns (4-column grid): example without “e.g.” so it is not truncated.
        let short = |value: &str, phrase: &str, window: &mut Window, cx: &mut Context<Self>| {
            cx.new(|cx| {
                DateField::new(value, phrase, window, cx).with_placeholder(t!("act.date_placeholder"), window, cx)
            })
        };
        let date = short(&act.date, &act.date_phrase, window, cx);
        let record_date = short(&act.record_date, &act.record_date_phrase, window, cx);
        let transcription = {
            let t = act.transcription.clone();
            cx.new(|cx| TextareaState::new(window, cx).rows(4).default_value(t))
        };
        let media_picker = picker(&ged, RecordKind::Media, "", window, cx);
        let mut this = Self {
            ged,
            base_dir: ged_path.and_then(|p| p.parent().map(Path::to_path_buf)),
            quality: act.quality,
            media: act.media.clone(),
            media_page: 0,
            media_picker,
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
            place_menu: None,
            preview: None,
            tour_link: None,
            error: None,
            zoom: Default::default(),
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
        let sub = this.watch_media_picker(window, cx);
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
                    if kind == Kind::Place {
                        cx.emit(ActEditorEvent::PlaceInput { input: input.clone(), changed: false });
                    }
                }
                InputEvent::Change => {
                    this.preview = None;
                    this.error = None;
                    match kind {
                        Kind::Place => {
                            this.place_focus = Some(input.entity_id());
                            cx.emit(ActEditorEvent::PlaceInput { input: input.clone(), changed: true });
                        }
                        Kind::Name => this.refresh_suggestions(cx),
                        Kind::Plain => {}
                    }
                }
                _ => return,
            }
            cx.notify();
        })
    }

    /// Chosen event: (tag, type, family).
    fn event(&self, cx: &App) -> (String, String, bool) {
        let v = self.event.read(cx).selected_value().cloned().unwrap_or_default();
        let (tag, ty, family) = parse_event(&v);
        if tag == "EVEN" && ty.is_empty() {
            return (tag, self.event_type.read(cx).value().trim().to_string(), family);
        }
        (tag, ty, family)
    }

    /// Rebuilds the people's rows (the fields depend on the role and the event).
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
                    PersonField::Age => t!("act.age_placeholder"),
                    PersonField::Kinship => t!("act.kinship_placeholder"),
                    _ => "".into(),
                }
                .into_owned();
                let v = match kind {
                    Kind::Place => place_label(p.get(f)),
                    _ => p.get(f).to_string(),
                };
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

    /// Builds the record from the fields.
    fn collect(&self, cx: &App) -> Act {
        let v = |e: &Entity<InputState>| e.read(cx).value().trim().to_string();
        let mut a = self.base.clone();
        (a.event, a.event_type, a.family_even) = self.event(cx);
        // Impossible date: the typed text is kept, `save` reports the error.
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
            // Nothing entered: the people expected for this new event.
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
        // One more principal person goes after the other principal people.
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
        // When linking, empty fields are filled from the chosen individual.
        if let PersonChoice::Link(x) = &choice
            && let Some(ind) = self.ged.individual(x)
        {
            // Surname and given names typed in each other's field: put them back in place.
            if gedcom::act::names_swapped(p, &ind) {
                let (s, g) = (p.get(PersonField::Surname).to_string(), p.get(PersonField::Given).to_string());
                p.set(PersonField::Surname, g);
                p.set(PersonField::Given, s);
            }
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

    /// The person at `idx` is the existing individual `xref` (chosen in the workspace's picker).
    pub fn link_person(&mut self, idx: usize, xref: String, window: &mut Window, cx: &mut Context<Self>) {
        self.set_choice(idx, PersonChoice::Link(xref), window, cx);
    }

    fn pick_person(&mut self, idx: usize, cx: &mut Context<Self>) {
        let suggestions = self
            .rows
            .get(idx)
            .map(|r| r.suggestions.iter().map(|s| (s.xref.clone(), s.reason.clone())).collect())
            .unwrap_or_default();
        cx.emit(ActEditorEvent::PickPerson { idx, suggestions });
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
            multiple: true,
            prompt: Some(t!("act.image").into()),
        });
        cx.spawn_in(window, async move |this, cx| {
            if let Ok(Ok(Some(paths))) = rx.await {
                let _ = this.update(cx, |this, cx| this.add_files(&paths, false, cx));
            }
        })
        .detach();
    }

    /// Path to save: relative to the .ged folder if the file is in it.
    fn file_value(&self, p: &Path) -> String {
        self.base_dir
            .as_ref()
            .and_then(|d| p.strip_prefix(d).ok())
            .map(|r| r.to_string_lossy().replace('\\', "/"))
            .unwrap_or_else(|| p.to_string_lossy().to_string())
    }

    /// Files chosen or dropped for the record (images and PDFs, one page each). `append`: added to
    /// the new files already chosen instead of replacing them. They are copied and named according
    /// to File Settings › Media on save.
    pub(crate) fn add_files(&mut self, paths: &[PathBuf], append: bool, cx: &mut Context<Self>) {
        let (ok, rejected): (Vec<&PathBuf>, Vec<&PathBuf>) = paths.iter().filter(|p| p.is_file()).partition(|p| {
            let mime = mime_for(&p.to_string_lossy());
            mime.starts_with("image/") || mime == "application/pdf"
        });
        self.error = (!rejected.is_empty()).then(|| t!("act.only_images").into());
        let files: Vec<String> = ok.into_iter().map(|p| self.file_value(p)).collect();
        if !files.is_empty() {
            let mut all = match (&self.media, append) {
                (MediaChoice::NewFiles(v), true) => v.clone(),
                _ => Vec::new(),
            };
            let first = all.len();
            for f in files {
                if !all.contains(&f) {
                    all.push(f);
                }
            }
            self.media_page = first.min(all.len().saturating_sub(1));
            // Files already imported (all in the same media): link that media, without duplicates.
            let known: Vec<Option<String>> = all.iter().map(|f| self.ged.media_by_file(f)).collect();
            self.media = match known.first() {
                Some(Some(x)) if known.iter().all(|k| k.as_ref() == Some(x)) => {
                    self.media_page = 0;
                    MediaChoice::Existing(x.clone())
                }
                _ => MediaChoice::NewFiles(all),
            };
            self.preview = None;
        }
        cx.notify();
    }

    fn watch_media_picker(&self, window: &mut Window, cx: &mut Context<Self>) -> Subscription {
        cx.subscribe_in(&self.media_picker, window, |this, picker, ev: &SelectEvent<_>, window, cx| {
            if let SelectEvent::Confirm(Some(x)) = ev {
                this.set_media(MediaChoice::Existing(x.clone()), cx);
                // The picker acts as an action: it goes back to “Existing Media…”.
                picker.update(cx, |p, cx| p.set_selected_index(None, window, cx));
            }
        })
    }

    fn set_media(&mut self, media: MediaChoice, cx: &mut Context<Self>) {
        self.media = media;
        self.media_page = 0;
        self.preview = None;
        cx.notify();
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

    /// Called by the workspace after a successful save, to move on to the next record of the same
    /// register.
    pub fn saved(&mut self, ged: Gedcom, result: &ActResult, window: &mut Window, cx: &mut Context<Self>) {
        let mut n = self.collect(cx).next();
        if let MediaChoice::NewFiles(_) = n.media {
            n.media = result.media.clone().map(MediaChoice::Existing).unwrap_or_default();
        }
        self.ged = ged;
        // The media created by this record can now be offered.
        self.media_picker = picker(&self.ged, RecordKind::Media, "", window, cx);
        let sub = self.watch_media_picker(window, cx);
        self._subs.push(sub);
        self.saved_count += 1;
        self.base = n.clone();
        self.media = n.media.clone();
        self.media_page = 0;
        self.page.update(cx, |s, cx| s.set_value(n.page.clone(), window, cx));
        self.date.update(cx, |s, cx| s.clear(window, cx));
        self.record_date.update(cx, |s, cx| s.clear(window, cx));
        self.transcription.update(cx, |s, cx| s.set_value("", window, cx));
        self.set_rows(n.participants, window, cx);
        self.error = None;
        // Back to the top of the form, cursor on the first person.
        self.scroll.set_offset(point(px(0.), px(0.)));
        if let Some((_, first)) = self.rows.first().and_then(|r| r.inputs.first()) {
            first.update(cx, |s, cx| s.focus(window, cx));
        }
        cx.notify();
    }

    /// Zoom of the act's image, for the View menu.
    /// Guided tour: highlights the suggestion to link `xref`.
    pub fn set_tour_link(&mut self, xref: Option<String>, cx: &mut Context<Self>) {
        self.tour_link = xref;
        cx.notify();
    }

    /// Is a person of the record linked to `xref`?
    pub fn links(&self, xref: &str) -> bool {
        self.rows.iter().any(|r| matches!(&r.choice, PersonChoice::Link(x) if x == xref))
    }

    pub fn zoom(&self) -> &crate::viewer::Zoom {
        &self.zoom
    }

    pub fn set_error(&mut self, e: String, cx: &mut Context<Self>) {
        self.error = Some(e.into());
        cx.notify();
    }

    /// Pages of the record's image: absolute paths, in order.
    fn media_pages(&self) -> Vec<PathBuf> {
        let files: Vec<String> = match &self.media {
            MediaChoice::None => Vec::new(),
            MediaChoice::Existing(x) => self
                .ged
                .get(x)
                .map(|n| Media::new(n).files().iter().map(|f| f.path.to_string()).collect())
                .unwrap_or_default(),
            MediaChoice::NewFiles(f) => f.clone(),
        };
        files
            .into_iter()
            .filter_map(|f| {
                let p = PathBuf::from(f.strip_prefix("file://").unwrap_or(&f));
                if p.is_absolute() { Some(p) } else { self.base_dir.as_ref().map(|d| d.join(p)) }
            })
            .collect()
    }
}

// ======================================================================
// Rendu
// ======================================================================

fn caps(s: impl Into<SharedString>) -> Div {
    div().text_xs().font_weight(FontWeight::SEMIBOLD).text_color(theme::muted()).child(s.into())
}

fn mono(s: impl Into<SharedString>) -> Div {
    div()
        .font_family("monospace")
        .font_features(theme::tabular_nums())
        .text_xs()
        .text_color(theme::muted())
        .child(s.into())
}

/// Columns of the grid shared by the “Record” and “People” sections.
const GRID: u16 = 4;

/// Width (in columns) of each field of a person: the surname and given names each take half a row,
/// the note a full row, the others one column; the last field of an incomplete row stretches to the
/// edge so as not to leave a gap.
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
    /// Connects the workspace's place menu.
    pub fn set_place_menu(&mut self, menu: PlaceMenu) {
        self.place_menu = Some(menu);
    }

    /// Place menu under the active place field.
    fn place_suggestions(&self, input: &Entity<InputState>, cx: &mut Context<Self>) -> Option<AnyElement> {
        if self.place_focus != Some(input.entity_id()) {
            return None;
        }
        let menu = self.place_menu.clone()?;
        menu(input, cx)
    }

    fn render_image(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let candidates: Vec<(String, String)> = self
            .ged
            .get(&self.base.source)
            .map(|s| {
                Source::new(s)
                    .media_xrefs()
                    .into_iter()
                    // A web link of the source is not the act's image.
                    .filter_map(|m| {
                        self.ged
                            .get(m)
                            .filter(|n| !Media::new(n).is_web())
                            .map(|n| (m.to_string(), Media::new(n).title()))
                    })
                    .collect()
            })
            .unwrap_or_default();
        // Media linked with “Existing Media…” without being in the source yet: shown too.
        let mut candidates = candidates;
        if let MediaChoice::Existing(x) = &self.media
            && !candidates.iter().any(|(c, _)| c == x)
            && let Some(n) = self.ged.get(x)
        {
            candidates.push((x.clone(), Media::new(n).title()));
        }
        let pages = self.media_pages();
        let count = pages.len();
        let at = self.media_page.min(count.saturating_sub(1));
        let file = pages.get(at).cloned();
        let name = |p: &Path| p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
        let label = match (&self.media, &file) {
            (MediaChoice::Existing(x), _) => self.ged.get(x).map(|n| Media::new(n).title()).unwrap_or_default(),
            (_, Some(p)) => name(p),
            _ => t!("act.image").into(),
        };
        let is_image = |p: &Path| mime_for(&p.to_string_lossy()).starts_with("image/");
        if !file.as_deref().is_some_and(|p| p.exists() && is_image(p)) {
            self.zoom.clear();
        }
        let viewer: AnyElement = match &file {
            Some(p) if p.exists() && is_image(p) => {
                div().flex_1().min_h_0().child(self.zoom.view("act-image", p.clone())).into_any_element()
            }
            // PDF: no built-in preview, open it with the system's application.
            Some(p) if p.exists() => {
                let target = p.clone();
                v_flex()
                    .flex_1()
                    .items_center()
                    .justify_center()
                    .gap_2()
                    .p_4()
                    .bg(theme::viewer())
                    .child(div().text_color(theme::muted()).child(name(p)))
                    .child(
                        Button::new("open-doc")
                            .ghost()
                            .small()
                            .label(t!("quality.open"))
                            .on_click(move |_, _, cx| cx.open_with_system(&target)),
                    )
                    .into_any_element()
            }
            Some(p) => v_flex()
                .flex_1()
                .items_center()
                .justify_center()
                .p_4()
                .text_color(theme::warn())
                .child(t!("act.file_missing", path = p.display()))
                .into_any_element(),
            None => v_flex()
                .flex_1()
                .items_center()
                .justify_center()
                .p_4()
                .text_color(theme::muted())
                .child(t!("act.no_image"))
                .into_any_element(),
        };
        let new_files = match &self.media {
            MediaChoice::NewFiles(f) if !f.is_empty() => Some(f.clone()),
            _ => None,
        };
        // While dragging from the Finder / Explorer: the drop zone shows itself; it is emphasized
        // when the pointer hovers it.
        let dragging = cx.has_active_drag();
        v_flex()
            .relative()
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
                            .child(label),
                    )
                    .when(count > 1, |d| {
                        d.child(Button::new("page-prev").ghost().xsmall().label("‹").disabled(at == 0).on_click(
                            cx.listener(move |this, _, _, cx| {
                                this.media_page = at.saturating_sub(1);
                                cx.notify();
                            }),
                        ))
                        .child(mono(format!("{}/{count}", at + 1)))
                        .child(
                            Button::new("page-next").ghost().xsmall().label("›").disabled(at + 1 >= count).on_click(
                                cx.listener(move |this, _, _, cx| {
                                    this.media_page = at + 1;
                                    cx.notify();
                                }),
                            ),
                        )
                    }),
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
                        chip(format!("media-{x}"), title, on).max_w(px(240.)).overflow_hidden().on_click(
                            cx.listener(move |this, _, _, cx| this.set_media(MediaChoice::Existing(x.clone()), cx)),
                        )
                    }))
                    .when_some(new_files, |d, f| {
                        let first = f[0].rsplit(['/', '\\']).next().unwrap_or(&f[0]).to_string();
                        let label = if f.len() > 1 { format!("{first} +{}", f.len() - 1) } else { first };
                        d.child(chip("media-new", label, true).max_w(px(240.)).overflow_hidden())
                    })
                    .child(
                        div().w(px(170.)).child(
                            Select::new(&self.media_picker)
                                .xsmall()
                                .placeholder(t!("act.existing_media"))
                                .menu_width(px(360.))
                                .search_placeholder(t!("event_dialog.search")),
                        ),
                    )
                    .child(
                        Button::new("pick-file")
                            .ghost()
                            .xsmall()
                            .label(t!("act.other_file"))
                            .on_click(cx.listener(|this, _, w, cx| this.pick_file(w, cx))),
                    )
                    .when(self.media != MediaChoice::None, |d| {
                        d.child(
                            Button::new("no-media")
                                .ghost()
                                .xsmall()
                                .label(t!("act.without_image"))
                                .on_click(cx.listener(|this, _, _, cx| this.set_media(MediaChoice::None, cx))),
                        )
                    }),
            )
            .when(dragging, |d| {
                d.child(
                    v_flex()
                        .absolute()
                        .inset_0()
                        .m_2()
                        .items_center()
                        .justify_center()
                        .rounded(theme::RADIUS_MD)
                        .border_2()
                        .border_dashed()
                        .border_color(theme::accent())
                        .bg(theme::panel())
                        .drag_over::<ExternalPaths>(|d, _, _, _| d.bg(theme::accent_soft()))
                        .text_color(theme::accent())
                        .font_weight(FontWeight::MEDIUM)
                        .child(t!("act.drop")),
                )
            })
    }

    fn render_act(&self, act: &Act, cx: &mut Context<Self>) -> Div {
        let place_hints = self.place_suggestions(&self.place, cx);
        let (tag, _, _) = parse_event(&self.event.read(cx).selected_value().cloned().unwrap_or_default());
        let free_type = tag == "EVEN" && act.event == "EVEN" && {
            let v = self.event.read(cx).selected_value().cloned().unwrap_or_default();
            v == "EVEN" || v == "FAM:EVEN"
        };
        section(t!("act.section"))
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
                            .child(caps(t!("act.event")))
                            .child(
                                Select::new(&self.event)
                                    .small()
                                    .placeholder(t!("event_dialog.choose"))
                                    .search_placeholder(t!("event_dialog.search")),
                            ),
                    )
                    .when(free_type, |d| d.child(field(t!("event_dialog.type"), &self.event_type)))
                    // Dates take half a row: a full date (“23 sept. 1795”) is never cut.
                    .child(date_cell(t!("event_dialog.date"), &self.date).col_span(2))
                    .child(field(t!("editor.place"), &self.place).col_span(2))
                    .child(date_cell(t!("act.record_date"), &self.record_date).col_span(2))
                    .child(field(t!("act.reference"), &self.page).col_span(GRID)),
            )
            .when_some(place_hints, |d, h| d.child(h))
            .child(h_flex().gap_1().items_center().child(caps(t!("act.quality")).mr_1()).children(
                quality_labels().into_iter().enumerate().map(|(q, label)| {
                    let on = self.quality == Some(q as u8);
                    chip(format!("quay-{q}"), label, on).on_click(cx.listener(move |this, _, _, cx| {
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
                .child(t!("act.linked_to", name = self.ged.record_label(x)))
                .child(mono(self.ged.individual(x).map(|i| i.years()).unwrap_or_default()))
                .into_any_element(),
            PersonChoice::Mention => {
                div().text_xs().text_color(theme::muted()).child(t!("act.mentioned")).into_any_element()
            }
            PersonChoice::Create if row.suggestions.is_empty() => {
                div().text_xs().text_color(theme::muted()).child(t!("cmd.new_individual")).into_any_element()
            }
            PersonChoice::Create => div()
                .text_xs()
                .text_color(theme::warn())
                .child(crate::tn!("act.matches", row.suggestions.len()))
                .into_any_element(),
        };
        // Father, mother or spouse of which principal person?
        let principals = act.principal_count();
        let of_chips: Vec<Stateful<Div>> = if row.role.relative() && principals > 1 {
            (0..principals)
                .map(|k| {
                    chip(format!("of-{idx}-{k}"), t!("act.of", name = act.principal_name(k)), row.of == k)
                        .on_click(cx.listener(move |this, _, _, cx| this.set_of(idx, k, cx)))
                })
                .collect()
        } else {
            Vec::new()
        };
        let place_hints: Vec<AnyElement> =
            row.inputs.iter().filter_map(|(_, e)| self.place_suggestions(e, cx)).collect();
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
                    .when(row.choice == PersonChoice::Create, |d| {
                        // The tour points at the button that finds the individual it expects.
                        let ringed =
                            self.tour_link.as_deref().is_some_and(|x| row.suggestions.iter().any(|s| s.xref == x));
                        d.child(
                            div()
                                .rounded(theme::RADIUS_MD)
                                .when(ringed, |d| d.border_2().border_color(theme::accent()))
                                .child(
                                    Button::new(SharedString::from(format!("link-{idx}")))
                                        .ghost()
                                        .xsmall()
                                        .label(t!("act.link"))
                                        .on_click(cx.listener(move |this, _, _, cx| this.pick_person(idx, cx))),
                                ),
                        )
                        .when(row.role != ActRole::Principal, |d| {
                            d.child(
                                Button::new(SharedString::from(format!("mention-{idx}")))
                                    .ghost()
                                    .xsmall()
                                    .label(t!("act.mention"))
                                    .on_click(cx.listener(move |this, _, w, cx| {
                                        this.set_choice(idx, PersonChoice::Mention, w, cx)
                                    })),
                            )
                        })
                    })
                    .when(row.choice != PersonChoice::Create, |d| {
                        d.child(
                            Button::new(SharedString::from(format!("unlink-{idx}")))
                                .ghost()
                                .xsmall()
                                .label(if matches!(row.choice, PersonChoice::Link(_)) {
                                    t!("act.unlink")
                                } else {
                                    t!("common.cancel")
                                })
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
                            .tooltip(t!("act.remove_person"))
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
                d.child(h_flex().gap_1().items_center().child(caps(t!("editor.sex"))).children(
                    [("M", t!("sex.male")), ("F", t!("sex.female"))].into_iter().map(|(v, label)| {
                        chip(format!("sex-{idx}-{v}"), label, current == v).on_click(cx.listener(
                            move |this, _, _, cx| {
                                if let Some(r) = this.rows.get_mut(idx) {
                                    // Second click: deselect.
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
    }

    fn render_people(&self, act: &Act, cx: &mut Context<Self>) -> Div {
        let rows: Vec<AnyElement> =
            self.rows.iter().enumerate().map(|(i, r)| self.render_person(act, i, r, cx).into_any_element()).collect();
        // “+”: one more person, with their role.
        let family = act.is_family();
        let event = act.event.clone();
        let next_principal = act.principal_count();
        let me = cx.entity().downgrade();
        let add = Button::new("add-person")
            .ghost()
            .xsmall()
            .icon(IconName::Plus)
            .tooltip(t!("act.add_person"))
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
            .child(h_flex().gap_1().items_center().child(caps(t!("act.people"))).child(add))
            .children(rows)
            .when(self.rows.iter().any(|r| r.inputs.iter().any(|(f, _)| *f == PersonField::Age)), |d| {
                d.child(
                    Checkbox::new("estimate")
                        .checked(self.estimate_births)
                        .label(t!("act.estimate").into_owned())
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
        let mut summary = crate::tn!("act.linked", linked).into_owned();
        if mentions > 0 {
            summary += &crate::tn!("act.mentions", mentions);
        }
        if to_decide > 0 {
            summary += &t!("act.to_decide", n = to_decide);
        }
        if self.saved_count > 0 {
            summary += &crate::tn!("act.saved", self.saved_count);
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
                    .label(if self.preview.is_some() { t!("act.hide_preview") } else { t!("act.preview") })
                    .on_click(cx.listener(|this, _, _, cx| this.toggle_preview(cx))),
            )
            .child(
                Button::new("save-act")
                    .small()
                    .label(t!("common.save"))
                    .on_click(cx.listener(|this, _, _, cx| this.save(false, cx))),
            )
            .child(
                Button::new("save-next")
                    .primary()
                    .small()
                    .label(t!("act.save_next"))
                    .on_click(cx.listener(|this, _, _, cx| this.save(true, cx))),
            )
    }
}

impl Render for ActEditor {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let act = self.collect(cx);
        let title = if self.base.original.is_some() { act.title() } else { t!("act.new").into_owned() };
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
                    .tooltip(t!("act.back"))
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
                    section(t!("act.transcription")).child(Textarea::new(&self.transcription).h(px(110.))).child(
                        h_flex().gap_2().items_center().child(
                            Button::new("auto-text")
                                .ghost()
                                .xsmall()
                                .label(t!("act.summary"))
                                .on_click(cx.listener(|this, _, w, cx| this.fill_transcription(w, cx))),
                        ),
                    ),
                )
                .when_some(preview, |d, p| {
                    d.child(
                        section(t!("act.preview")).child(match p {
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

        // Images or PDFs dropped anywhere on the editor become the record's image (the next ones
        // are added as pages).
        v_flex()
            .id("act-editor")
            .flex_1()
            .min_w_0()
            .min_h_0()
            .bg(theme::bg())
            .on_drop(cx.listener(|this, paths: &ExternalPaths, _, cx| {
                cx.stop_propagation();
                this.add_files(paths.paths(), true, cx);
            }))
            .child(header)
            .child(h_flex().flex_1().min_h_0().items_stretch().child(self.render_image(cx)).child(form))
            .child(self.render_footer(cx))
    }
}
