//! Editors of a record: visual form + raw GEDCOM editor.

use crate::completion::GedcomCompletion;
use crate::date_field::DateField;
use crate::t;
use gedcom::complete::Catalog;
use gedcom::*;
use gpui_kit::component::input::{EditorState, InputState, TextareaState};
use gpui_kit::component::select::{SearchableVec, SelectItem, SelectState};
use gpui_kit::{App, AppContext, Context, Entity, SharedString, Window, px};
use std::rc::Rc;

/// What is selected in the current view.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    Record { xref: String, kind: RecordKind },
    Place(String),
}

pub struct Field {
    pub label: SharedString,
    pub state: FieldState,
    /// Place: suggests the places already in the file.
    pub place: bool,
    /// Saved value as read in the record (date in words, label of a choice); empty: hidden.
    pub shown: String,
    /// Where the value goes when reading the record.
    pub slot: Slot,
}

/// Place of a field's value on the record being read.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Slot {
    /// A row of the record's facts.
    #[default]
    Fact,
    /// Already in the record's title (name, title…): not repeated.
    Title,
    /// Under the title, after the repository: a source's call number.
    Subtitle,
}

/// Free input, choice from a list of fixed values (sex…), or date.
pub enum FieldState {
    Text(Entity<InputState>),
    Choice(Entity<PickerState>),
    Date(Entity<DateField>),
}

impl FieldState {
    pub fn value(&self, cx: &App) -> String {
        match self {
            FieldState::Text(e) => e.read(cx).value().to_string(),
            FieldState::Choice(e) => e.read(cx).selected_value().cloned().unwrap_or_default(),
            FieldState::Date(e) => e.read(cx).input().read(cx).value().to_string(),
        }
    }
}

/// `SEX` values (GEDCOM 7) and their label.
fn sexes() -> [(&'static str, String); 4] {
    [
        ("M", t!("sex.male").into()),
        ("F", t!("sex.female").into()),
        ("X", t!("sex.other").into()),
        ("U", t!("sex.unknown").into()),
    ]
}

/// Label of the value `v` among `options`, ignoring case; empty if it is not one of them.
fn choice_label(options: &[(&str, &str)], v: &str) -> String {
    options.iter().find(|(o, _)| o.eq_ignore_ascii_case(v.trim())).map(|(_, l)| l.to_string()).unwrap_or_default()
}

/// A GEDCOM date in words (“12 March 1854”), or its phrase when it has one.
fn date_words(date: &str, phrase: &str) -> String {
    if !phrase.trim().is_empty() {
        return phrase.trim().to_string();
    }
    if date.trim().is_empty() { String::new() } else { Date::parse(date).display() }
}

/// Drop-down list of fixed values (GEDCOM value, label).
pub(crate) fn choice<T: 'static>(
    options: &[(&str, &str)],
    selected: &str,
    window: &mut Window,
    cx: &mut Context<T>,
) -> Entity<PickerState> {
    choice_with_search(options, selected, false, window, cx)
}

/// Like [`choice`], with a search field for long lists.
pub(crate) fn choice_with_search<T: 'static>(
    options: &[(&str, &str)],
    selected: &str,
    searchable: bool,
    window: &mut Window,
    cx: &mut Context<T>,
) -> Entity<PickerState> {
    let items: Vec<Pick> = options
        .iter()
        .map(|(v, l)| Pick { xref: v.to_string(), label: SharedString::from(l.to_string()), search: sort_key(l) })
        .collect();
    // Value of the file, ignoring case (`m` → `M`, `FR` → `fr`).
    let selected =
        items.iter().find(|p| p.xref.eq_ignore_ascii_case(selected.trim())).map(|p| p.xref.clone()).unwrap_or_default();
    cx.new(|cx| {
        let mut s = SelectState::new(SearchableVec::new(items), None, window, cx).searchable(searchable);
        if !selected.is_empty() {
            s.set_selected_value(&selected, window, cx);
        }
        s
    })
}

/// A record offered in a picker (shown by its name, never by its identifier).
#[derive(Clone)]
pub struct Pick {
    xref: String,
    label: SharedString,
    search: String,
}

impl SelectItem for Pick {
    type Value = String;

    fn title(&self) -> SharedString {
        self.label.clone()
    }

    fn value(&self) -> &String {
        &self.xref
    }

    /// Accent-insensitive search; `@I12@` allows advanced search by identifier.
    fn matches(&self, query: &str) -> bool {
        let q = query.trim();
        if q.starts_with('@') {
            return self.xref.eq_ignore_ascii_case(q.trim_matches('@'));
        }
        let q = sort_key(q);
        q.split_whitespace().all(|w| self.search.contains(w))
    }
}

pub type PickerState = SelectState<SearchableVec<Pick>>;

/// Picker of a linked record (spouse, repository…).
pub struct Picker {
    pub label: SharedString,
    pub placeholder: SharedString,
    pub state: Entity<PickerState>,
    /// Saved linked record, shown as a link when reading.
    pub shown: Option<String>,
}

fn picks(ged: &Gedcom, kind: RecordKind) -> Vec<Pick> {
    let mut v: Vec<Pick> = ged
        .records_of(kind)
        .filter_map(|r| {
            let xref = r.xref.clone()?;
            let label = match kind {
                RecordKind::Individual => {
                    let i = Individual::new(r);
                    let years = i.years();
                    if years.is_empty() { i.display_name() } else { format!("{} ({years})", i.display_name()) }
                }
                _ => ged.record_label(&xref),
            };
            Some(Pick { search: sort_key(&label), label: label.into(), xref })
        })
        .collect();
    v.sort_by(|a, b| a.search.cmp(&b.search));
    v
}

pub(crate) fn picker<T: 'static>(
    ged: &Gedcom,
    kind: RecordKind,
    selected: &str,
    window: &mut Window,
    cx: &mut Context<T>,
) -> Entity<PickerState> {
    let items = picks(ged, kind);
    let selected = selected.trim().trim_matches('@').to_string();
    cx.new(|cx| {
        let mut s = SelectState::new(SearchableVec::new(items), None, window, cx).searchable(true);
        if !selected.is_empty() {
            s.set_selected_value(&selected, window, cx);
        }
        s
    })
}

/// States of the fields for the selected item.
pub struct RecordEditor {
    pub target: Target,
    pub pickers: Vec<Picker>,
    pub fields: Vec<Field>,
    pub long_text: Option<(SharedString, Entity<TextareaState>)>,
    pub raw: Option<Entity<EditorState>>,
    /// Text of the raw GEDCOM editor when it opened, to know whether it was changed.
    pub raw_original: String,
    pub error: Option<SharedString>,
    /// An individual's names beyond the given name and surname of `fields`.
    pub names: Option<NamesEditor>,
}

/// Parts of a name that only show once filled, added from the “Add” menu.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NamePart {
    Nickname,
    Title,
    Particle,
    Suffix,
}

impl NamePart {
    pub const ALL: [NamePart; 4] = [NamePart::Nickname, NamePart::Title, NamePart::Particle, NamePart::Suffix];

    pub fn label(self) -> SharedString {
        match self {
            NamePart::Nickname => t!("editor.nickname"),
            NamePart::Title => t!("editor.name_title"),
            NamePart::Particle => t!("editor.particle"),
            NamePart::Suffix => t!("editor.suffix"),
        }
        .into()
    }

    fn get(self, f: &NameForm) -> &str {
        match self {
            NamePart::Nickname => &f.nickname,
            NamePart::Title => &f.prefix,
            NamePart::Particle => &f.particle,
            NamePart::Suffix => &f.suffix,
        }
    }

    fn set(self, f: &mut NameForm, v: String) {
        *match self {
            NamePart::Nickname => &mut f.nickname,
            NamePart::Title => &mut f.prefix,
            NamePart::Particle => &mut f.particle,
            NamePart::Suffix => &mut f.suffix,
        } = v;
    }
}

/// Label of a `NAME.TYPE` value (empty for none).
pub fn name_type_label(kind: &str) -> String {
    match kind {
        "BIRTH" => t!("name_type.birth"),
        "MARRIED" => t!("name_type.married"),
        "AKA" => t!("name_type.aka"),
        "MAIDEN" => t!("name_type.maiden"),
        "IMMIGRANT" => t!("name_type.immigrant"),
        "PROFESSIONAL" => t!("name_type.professional"),
        "OTHER" => t!("name_type.other"),
        _ => return String::new(),
    }
    .into_owned()
}

/// A name other than the main one, edited in place.
pub struct OtherName {
    pub kind: Entity<PickerState>,
    pub given: Entity<InputState>,
    pub surname: Entity<InputState>,
    /// As read: keeps the parts that are not edited here.
    pub form: NameForm,
}

/// An individual's names: parts of the main name shown, and the other names.
pub struct NamesEditor {
    /// Main name as read.
    pub main: NameForm,
    /// Parts of the main name shown (filled, or just added), with their field.
    pub parts: Vec<(NamePart, Entity<InputState>)>,
    pub others: Vec<OtherName>,
    /// Other name unfolded for editing.
    pub open: Option<usize>,
}

impl NamesEditor {
    fn new<T: 'static>(names: &[NameForm], window: &mut Window, cx: &mut Context<T>) -> Self {
        let main = names.first().cloned().unwrap_or_default();
        let parts = NamePart::ALL
            .into_iter()
            .filter(|p| !p.get(&main).trim().is_empty())
            .map(|p| (p, input(window, cx, p.get(&main), "")))
            .collect();
        let others = names.iter().skip(1).map(|f| OtherName::new(f.clone(), window, cx)).collect();
        Self { main, parts, others, open: None }
    }

    /// Shows the field of a part of the main name; returns it, to be focused.
    pub fn add_part<T: 'static>(
        &mut self,
        part: NamePart,
        window: &mut Window,
        cx: &mut Context<T>,
    ) -> Entity<InputState> {
        if let Some((_, e)) = self.parts.iter().find(|(p, _)| *p == part) {
            return e.clone();
        }
        let e = input(window, cx, "", "");
        self.parts.push((part, e.clone()));
        self.parts.sort_by_key(|(p, _)| NamePart::ALL.iter().position(|q| q == p));
        e
    }

    /// Adds an other name (“also known as”, same given names), unfolded; returns its surname
    /// field, to be focused.
    pub fn add_other<T: 'static>(
        &mut self,
        given: &str,
        window: &mut Window,
        cx: &mut Context<T>,
    ) -> Entity<InputState> {
        let form = NameForm { given: given.to_string(), kind: "AKA".into(), ..Default::default() };
        self.others.push(OtherName::new(form, window, cx));
        self.open = Some(self.others.len() - 1);
        self.others.last().unwrap().surname.clone()
    }

    pub fn remove_other(&mut self, ix: usize) {
        if ix < self.others.len() {
            self.others.remove(ix);
        }
        self.open = None;
    }

    /// The names to write, the main one first.
    fn forms(&self, given: &str, surname: &str, cx: &App) -> Vec<NameForm> {
        let mut main = self.main.clone();
        main.given = given.trim().to_string();
        main.surname = surname.trim().to_string();
        for (p, e) in &self.parts {
            p.set(&mut main, e.read(cx).value().trim().to_string());
        }
        let others = self.others.iter().map(|o| o.read(cx)).filter(|f| !(f.given.is_empty() && f.surname.is_empty()));
        std::iter::once(main).chain(others).collect()
    }
}

impl OtherName {
    fn new<T: 'static>(form: NameForm, window: &mut Window, cx: &mut Context<T>) -> Self {
        let types: Vec<(String, String)> = NAME_TYPES.iter().map(|k| (k.to_string(), name_type_label(k))).collect();
        let types: Vec<(&str, &str)> = types.iter().map(|(v, l)| (v.as_str(), l.as_str())).collect();
        Self {
            kind: choice(&types, &form.kind, window, cx),
            given: input(window, cx, &form.given, ""),
            surname: input(window, cx, &form.surname, ""),
            form,
        }
    }

    /// The name with the typed values.
    pub fn read(&self, cx: &App) -> NameForm {
        let mut f = self.form.clone();
        f.kind = self.kind.read(cx).selected_value().cloned().unwrap_or_default();
        f.given = self.given.read(cx).value().trim().to_string();
        f.surname = self.surname.read(cx).value().trim().to_string();
        f
    }
}

/// Input field; the format example is shown greyed out while it is empty.
fn input<T: 'static>(
    window: &mut Window,
    cx: &mut Context<T>,
    value: &str,
    hint: impl Into<SharedString>,
) -> Entity<InputState> {
    let hint = hint.into();
    let value = value.to_string();
    cx.new(|cx| InputState::new(window, cx).placeholder(hint).default_value(value))
}

fn textarea<T: 'static>(window: &mut Window, cx: &mut Context<T>, value: &str) -> Entity<TextareaState> {
    let value = value.to_string();
    cx.new(|cx| TextareaState::new(window, cx).rows(5).default_value(value))
}

impl RecordEditor {
    pub fn new<T: 'static>(ged: &Gedcom, target: Target, window: &mut Window, cx: &mut Context<T>) -> Self {
        let mut fields = Vec::new();
        let mut pickers = Vec::new();
        let mut long_text = None;
        let mut raw = None;
        let mut raw_original = String::new();
        let mut names = None;
        macro_rules! field {
            ($label:expr, $hint:expr, $v:expr) => {
                fields.push(Field {
                    label: $label.into(),
                    state: FieldState::Text(input(window, cx, &$v, $hint)),
                    place: false,
                    shown: $v.to_string(),
                    slot: Slot::Fact,
                })
            };
        }
        match &target {
            Target::Place(name) => {
                let place = ged.places().into_iter().find(|p| &p.name == name);
                // One field per jurisdiction of the place format (File Settings › Places), if the
                // place follows it; otherwise the full name.
                let form = ged.place_form();
                let parts: Vec<&str> = name.split(',').map(str::trim).collect();
                if form.len() >= 2 && parts.len() == form.len() {
                    for (j, part) in form.iter().zip(parts) {
                        fields.push(Field {
                            label: j.replace('_', " ").into(),
                            state: FieldState::Text(input(window, cx, part, "")),
                            place: false,
                            shown: part.to_string(),
                            slot: Slot::Fact,
                        });
                    }
                } else {
                    field!(t!("editor.place"), t!("editor.place_placeholder"), name.clone());
                }
                field!(
                    t!("editor.latitude"),
                    t!("editor.latitude_placeholder"),
                    place.as_ref().and_then(|p| p.latitude.clone()).unwrap_or_default()
                );
                field!(
                    t!("editor.longitude"),
                    t!("editor.longitude_placeholder"),
                    place.as_ref().and_then(|p| p.longitude.clone()).unwrap_or_default()
                );
            }
            Target::Record { xref, kind } => {
                let Some(node) = ged.get(xref) else {
                    return Self {
                        target,
                        pickers,
                        fields,
                        long_text,
                        raw,
                        raw_original,
                        error: Some(t!("editor.not_found").into()),
                        names,
                    };
                };
                match kind {
                    RecordKind::Individual => {
                        let f = IndividualForm::read(node);
                        field!(t!("editor.given"), "", f.names[0].given);
                        field!(t!("editor.name"), "", f.names[0].surname);
                        names = Some(NamesEditor::new(&f.names, window, cx));
                        fields[0].slot = Slot::Title;
                        fields[1].slot = Slot::Title;
                        let sexes = sexes();
                        let sexes: Vec<(&str, &str)> = sexes.iter().map(|(v, l)| (*v, l.as_str())).collect();
                        let sex = choice(&sexes, &f.sex, window, cx);
                        fields.push(Field {
                            label: t!("editor.sex").into(),
                            state: FieldState::Choice(sex),
                            place: false,
                            shown: choice_label(&sexes, &f.sex),
                            slot: Slot::Fact,
                        });
                        // Birth, death and occupation are events of the timeline, edited there:
                        // not in the form.
                        long_text = Some((t!("editor.note").into(), textarea(window, cx, &f.note)));
                    }
                    // Unions are managed from the spouses' pages.
                    RecordKind::Family => {}
                    RecordKind::Source => {
                        let f = SourceForm::read(node);
                        field!(t!("editor.title"), "", f.title);
                        field!(t!("editor.author"), "", f.author);
                        field!(t!("editor.publication"), "", f.publication);
                        field!(t!("editor.call_number"), "", f.call_number);
                        fields[0].slot = Slot::Title;
                        fields[3].slot = Slot::Subtitle;
                        // Contents (`DATA` / `EVEN`): record types and period.
                        let content = SourceContent::read(node);
                        field!(t!("editor.record_types"), t!("editor.acts_placeholder"), content.events_label());
                        fields.push(Field {
                            label: t!("editor.period").into(),
                            shown: date_words(&content.period, ""),
                            state: FieldState::Date(cx.new(|cx| {
                                DateField::new(&content.period, "", window, cx).with_placeholder(
                                    t!("editor.period_placeholder"),
                                    window,
                                    cx,
                                )
                            })),
                            place: false,
                            slot: Slot::Fact,
                        });
                        let state = picker(ged, RecordKind::Repository, &f.repository, window, cx);
                        pickers.push(Picker {
                            label: t!("editor.repository").into(),
                            placeholder: t!("editor.choose_repository").into(),
                            state,
                            shown: Some(f.repository.trim().trim_matches('@').to_string())
                                .filter(|x| ged.get(x).is_some()),
                        });
                        long_text = Some((t!("editor.source_text").into(), textarea(window, cx, &f.text)));
                    }
                    RecordKind::Repository => {
                        let f = RepositoryForm::read(node);
                        field!(t!("editor.name"), "", f.name);
                        field!(t!("editor.website"), "", f.www);
                        fields[0].slot = Slot::Title;
                    }
                    RecordKind::Media => {
                        let f = MediaForm::read(node);
                        // The format (FORM) is inferred from the file: no field.
                        field!(t!("editor.title"), "", f.title);
                        fields[0].slot = Slot::Title;
                        field!(t!("editor.file"), t!("editor.file_placeholder"), f.file);
                        let mediums = gedcom::mediums();
                        let options: Vec<(&str, &str)> = mediums.iter().map(|(m, l)| (*m, l.as_ref())).collect();
                        let medi = choice(&options, medium(node).unwrap_or(""), window, cx);
                        fields.push(Field {
                            label: t!("editor.medium").into(),
                            state: FieldState::Choice(medi),
                            place: false,
                            shown: choice_label(&options, medium(node).unwrap_or("")),
                            slot: Slot::Fact,
                        });
                    }
                    RecordKind::SharedNote => {
                        long_text = Some((t!("editor.text").into(), textarea(window, cx, &node.text())));
                    }
                    RecordKind::Submitter => {
                        field!(t!("editor.name"), "", node.child_val("NAME").unwrap_or(""));
                        fields[0].slot = Slot::Title;
                    }
                }
                let text = node_to_string(node);
                raw_original = text.clone();
                let provider = Rc::new(GedcomCompletion::new(Catalog::from_gedcom(ged)));
                raw = Some(cx.new(|cx| {
                    let mut state = EditorState::new(window, cx).line_number(true).default_value(text);
                    state.lsp_mut().completion_provider = Some(provider);
                    state.lsp_mut().completion_menu.max_width = px(520.);
                    state
                }));
            }
        }
        Self { target, pickers, fields, long_text, raw, raw_original, error: None, names }
    }

    fn picked(&self, n: usize, cx: &App) -> String {
        self.pickers.get(n).and_then(|p| p.state.read(cx).selected_value().cloned()).unwrap_or_default()
    }

    fn values(&self, cx: &App) -> Vec<String> {
        self.fields.iter().map(|f| f.state.value(cx)).collect()
    }

    /// Event (date + place) of fields `n` (date) and `n + 1` (place).
    fn long_value(&self, cx: &App) -> String {
        self.long_text.as_ref().map(|(_, s)| s.read(cx).value().to_string()).unwrap_or_default()
    }

    /// Name of the typed place: the “Place” field, or the jurisdictions joined with commas (the
    /// original name is kept as is if it did not change).
    pub fn place_name(&self, cx: &App) -> Option<String> {
        let Target::Place(old) = &self.target else { return None };
        let v = self.values(cx);
        let parts = &v[..v.len() - 2];
        Some(if parts.len() == 1 {
            parts[0].trim().to_string()
        } else if parts.iter().map(|p| p.trim()).eq(old.split(',').map(str::trim)) {
            old.clone()
        } else {
            parts.iter().map(|p| p.trim()).collect::<Vec<_>>().join(", ")
        })
    }

    /// Applies the visual form to the document. Returns a result message. `created`: places created
    /// but not written to the file yet.
    pub fn apply_form(&self, ged: &mut Gedcom, created: &[String], cx: &App) -> Result<String, String> {
        // Place fields: the displayed label becomes the full `PLAC` value again.
        let v: Vec<String> = self
            .fields
            .iter()
            .map(|f| if f.place { ged.place_value(&f.state.value(cx), created) } else { f.state.value(cx) })
            .collect();
        let note = self.long_value(cx);
        match &self.target {
            Target::Place(old) => {
                let new = self.place_name(cx).unwrap_or_else(|| old.clone());
                let coords = &v[v.len() - 2..];
                let n = ged.update_place(old, &new, &coords[0], &coords[1]);
                Ok(crate::tn!("editor.place_updated", n).into_owned())
            }
            Target::Record { xref, kind } => {
                let node = ged.get_mut(xref).ok_or_else(|| t!("editor.not_found").to_string())?;
                match kind {
                    RecordKind::Individual => IndividualForm {
                        names: match &self.names {
                            Some(n) => n.forms(&v[0], &v[1], cx),
                            None => vec![NameForm { given: v[0].clone(), surname: v[1].clone(), ..Default::default() }],
                        },
                        sex: v[2].clone(),
                        occupation: IndividualForm::read(node).occupation,
                        note,
                    }
                    .apply(node),
                    RecordKind::Family => {}
                    RecordKind::Source => {
                        let before = SourceContent::read(node);
                        let events = if v[4].trim() == before.events_label() {
                            before.events.clone()
                        } else {
                            parse_event_list(&v[4])?
                        };
                        let period = match &self.fields[5].state {
                            FieldState::Date(e) => e.read(cx).value(cx)?.0,
                            other => other.value(cx),
                        };
                        let content = SourceContent { events, period, place: before.place.clone() };
                        SourceForm {
                            title: v[0].clone(),
                            author: v[1].clone(),
                            publication: v[2].clone(),
                            repository: self.picked(0, cx),
                            call_number: v[3].clone(),
                            text: note,
                        }
                        .apply(node);
                        if content != before {
                            content.apply(node);
                        }
                    }
                    RecordKind::Repository => RepositoryForm { name: v[0].clone(), www: v[1].clone() }.apply(node),
                    RecordKind::Media => {
                        let before = MediaForm::read(node);
                        let file = v[1].trim().to_string();
                        let form = if file != before.file || before.form.is_empty() {
                            mime_for(&file).to_string()
                        } else {
                            before.form
                        };
                        MediaForm { file, form, title: v[0].clone() }.apply(node);
                        if medium(node).unwrap_or("") != v[2] {
                            set_medium(node, &v[2]);
                        }
                    }
                    RecordKind::SharedNote => node.set_text(&note),
                    RecordKind::Submitter => node.set_child_val("NAME", &v[0]),
                }
                Ok(t!("editor.applied", name = ged.record_label(xref)).into_owned())
            }
        }
    }

    /// Whether the text of the raw GEDCOM editor differs from the record.
    pub fn raw_changed(&self, cx: &App) -> bool {
        self.raw.as_ref().is_some_and(|r| r.read(cx).value().as_ref() != self.raw_original.as_str())
    }

    /// Applies the text of the raw GEDCOM editor. Returns the new identifier.
    pub fn apply_raw(&self, ged: &mut Gedcom, cx: &App) -> Result<String, String> {
        let Target::Record { xref, kind } = &self.target else { return Err(t!("editor.nothing").into()) };
        let raw = self.raw.as_ref().ok_or_else(|| t!("editor.no_raw").to_string())?;
        let text = raw.read(cx).value().to_string();
        let node = node_from_string(&text).map_err(|e| e.to_string())?;
        if node.tag != kind.tag() {
            return Err(t!("editor.wrong_type", expected = kind.tag(), found = node.tag).into_owned());
        }
        let new_xref = node.xref.clone().unwrap_or_default();
        ged.replace(xref, node)?;
        Ok(new_xref)
    }
}
