//! Éditeurs d'un enregistrement : formulaire visuel + éditeur GEDCOM brut.

use crate::completion::GedcomCompletion;
use gedcom::complete::Catalog;
use gedcom::*;
use gpui_kit::component::input::{EditorState, InputState, TextareaState};
use gpui_kit::component::select::{SearchableVec, SelectItem, SelectState};
use gpui_kit::{App, AppContext, Context, Entity, SharedString, Window, px};
use std::rc::Rc;

/// Ce qui est sélectionné dans la vue courante.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    Record { xref: String, kind: RecordKind },
    Place(String),
}

pub struct Field {
    pub label: &'static str,
    pub state: FieldState,
    /// Lieu : propose les lieux déjà présents dans le fichier.
    pub place: bool,
}

/// Saisie libre, ou choix dans une liste de valeurs fixes (sexe…).
pub enum FieldState {
    Text(Entity<InputState>),
    Choice(Entity<PickerState>),
}

impl FieldState {
    pub fn value(&self, cx: &App) -> String {
        match self {
            FieldState::Text(e) => e.read(cx).value().to_string(),
            FieldState::Choice(e) => e.read(cx).selected_value().cloned().unwrap_or_default(),
        }
    }
}

/// Valeurs de `SEX` (GEDCOM 7) et leur libellé.
const SEXES: &[(&str, &str)] = &[("M", "Homme"), ("F", "Femme"), ("X", "Autre"), ("U", "Inconnu")];

/// Liste déroulante de valeurs fixes (valeur GEDCOM, libellé).
fn choice<T: 'static>(
    options: &[(&str, &str)],
    selected: &str,
    window: &mut Window,
    cx: &mut Context<T>,
) -> Entity<PickerState> {
    let items: Vec<Pick> = options
        .iter()
        .map(|(v, l)| Pick { xref: v.to_string(), label: SharedString::from(l.to_string()), search: sort_key(l) })
        .collect();
    let selected = selected.trim().to_uppercase();
    cx.new(|cx| {
        let mut s = SelectState::new(SearchableVec::new(items), None, window, cx);
        if !selected.is_empty() {
            s.set_selected_value(&selected, window, cx);
        }
        s
    })
}

/// Un enregistrement proposé dans un sélecteur (affiché par son nom, jamais par son identifiant).
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

    /// Recherche sans accents ; `@I12@` permet la recherche avancée par identifiant.
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

/// Sélecteur d'un enregistrement lié (époux, dépôt…).
pub struct Picker {
    pub label: &'static str,
    pub placeholder: &'static str,
    pub state: Entity<PickerState>,
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

fn picker<T: 'static>(
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

/// États des champs pour l'élément sélectionné.
pub struct RecordEditor {
    pub target: Target,
    pub pickers: Vec<Picker>,
    pub fields: Vec<Field>,
    pub long_text: Option<(&'static str, Entity<TextareaState>)>,
    pub raw: Option<Entity<EditorState>>,
    pub error: Option<SharedString>,
}

/// Champ de saisie ; l'exemple de format s'affiche en grisé tant qu'il est vide.
fn input<T: 'static>(window: &mut Window, cx: &mut Context<T>, value: &str, hint: &'static str) -> Entity<InputState> {
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
        macro_rules! field {
            ($label:expr, $hint:expr, $v:expr) => {
                fields.push(Field {
                    label: $label,
                    state: FieldState::Text(input(window, cx, &$v, $hint)),
                    place: false,
                })
            };
        }
        macro_rules! place_field {
            ($label:expr, $v:expr) => {
                fields.push(Field { label: $label, state: FieldState::Text(input(window, cx, &$v, "")), place: true })
            };
        }
        match &target {
            Target::Place(name) => {
                let place = ged.places().into_iter().find(|p| &p.name == name);
                field!("Lieu", "Lieudit, Commune, Code postal, Département, Région, Pays", name.clone());
                field!("Latitude", "ex. N48.8463", place.as_ref().and_then(|p| p.latitude.clone()).unwrap_or_default());
                field!(
                    "Longitude",
                    "ex. E2.4018",
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
                        error: Some("Enregistrement introuvable".into()),
                    };
                };
                match kind {
                    RecordKind::Individual => {
                        let f = IndividualForm::read(node);
                        field!("Prénoms", "", f.given);
                        field!("Nom", "", f.surname);
                        let sex = choice(SEXES, &f.sex, window, cx);
                        fields.push(Field { label: "Sexe", state: FieldState::Choice(sex), place: false });
                        field!("Profession", "", f.occupation);
                        field!("Naissance — date", "ex. 14 MAR 1912, ABT 1850", f.birth.date);
                        place_field!("Naissance — lieu", f.birth.place);
                        field!("Décès — date", "", f.death.date);
                        place_field!("Décès — lieu", f.death.place);
                        long_text = Some(("Note", textarea(window, cx, &f.note)));
                    }
                    RecordKind::Family => {
                        let f = FamilyForm::read(node);
                        for (label, x) in [("Époux", &f.husband), ("Épouse", &f.wife)] {
                            let state = picker(ged, RecordKind::Individual, x, window, cx);
                            pickers.push(Picker { label, placeholder: "Choisir un individu…", state });
                        }
                        field!("Mariage — date", "", f.marriage.date);
                        place_field!("Mariage — lieu", f.marriage.place);
                    }
                    RecordKind::Source => {
                        let f = SourceForm::read(node);
                        field!("Titre", "", f.title);
                        field!("Auteur", "", f.author);
                        field!("Publication", "", f.publication);
                        field!("Cote", "", f.call_number);
                        let state = picker(ged, RecordKind::Repository, &f.repository, window, cx);
                        pickers.push(Picker { label: "Dépôt", placeholder: "Choisir un dépôt…", state });
                        long_text = Some(("Texte de la source", textarea(window, cx, &f.text)));
                    }
                    RecordKind::Repository => {
                        let f = RepositoryForm::read(node);
                        field!("Nom", "", f.name);
                        field!("Site web", "", f.www);
                    }
                    RecordKind::Media => {
                        let f = MediaForm::read(node);
                        // Le format (FORM) se déduit du fichier : pas de champ.
                        field!("Titre", "", f.title);
                        field!("Fichier", "Actes/acte.jpg", f.file);
                    }
                    RecordKind::SharedNote => {
                        long_text = Some(("Texte", textarea(window, cx, &node.text())));
                    }
                    RecordKind::Submitter => {
                        field!("Nom", "", node.child_val("NAME").unwrap_or(""));
                    }
                }
                let text = node_to_string(node);
                let provider = Rc::new(GedcomCompletion::new(Catalog::from_gedcom(ged)));
                raw = Some(cx.new(|cx| {
                    let mut state = EditorState::new(window, cx).line_number(true).default_value(text);
                    state.lsp_mut().completion_provider = Some(provider);
                    state.lsp_mut().completion_menu.max_width = px(520.);
                    state
                }));
            }
        }
        Self { target, pickers, fields, long_text, raw, error: None }
    }

    fn picked(&self, n: usize, cx: &App) -> String {
        self.pickers.get(n).and_then(|p| p.state.read(cx).selected_value().cloned()).unwrap_or_default()
    }

    fn values(&self, cx: &App) -> Vec<String> {
        self.fields.iter().map(|f| f.state.value(cx)).collect()
    }

    fn long_value(&self, cx: &App) -> String {
        self.long_text.as_ref().map(|(_, s)| s.read(cx).value().to_string()).unwrap_or_default()
    }

    /// Applique le formulaire visuel au document. Renvoie un message de résultat.
    pub fn apply_form(&self, ged: &mut Gedcom, cx: &App) -> Result<String, String> {
        let v = self.values(cx);
        let note = self.long_value(cx);
        match &self.target {
            Target::Place(old) => {
                let n = ged.update_place(old, &v[0], &v[1], &v[2]);
                Ok(format!("Lieu mis à jour ({n} occurrence{})", if n > 1 { "s" } else { "" }))
            }
            Target::Record { xref, kind } => {
                let node = ged.get_mut(xref).ok_or("Enregistrement introuvable")?;
                match kind {
                    RecordKind::Individual => IndividualForm {
                        given: v[0].clone(),
                        surname: v[1].clone(),
                        sex: v[2].clone(),
                        occupation: v[3].clone(),
                        birth: EventForm { date: v[4].clone(), place: v[5].clone() },
                        death: EventForm { date: v[6].clone(), place: v[7].clone() },
                        note,
                    }
                    .apply(node),
                    RecordKind::Family => FamilyForm {
                        husband: self.picked(0, cx),
                        wife: self.picked(1, cx),
                        marriage: EventForm { date: v[0].clone(), place: v[1].clone() },
                    }
                    .apply(node),
                    RecordKind::Source => SourceForm {
                        title: v[0].clone(),
                        author: v[1].clone(),
                        publication: v[2].clone(),
                        repository: self.picked(0, cx),
                        call_number: v[3].clone(),
                        text: note,
                    }
                    .apply(node),
                    RecordKind::Repository => RepositoryForm { name: v[0].clone(), www: v[1].clone() }.apply(node),
                    RecordKind::Media => {
                        let before = MediaForm::read(node);
                        let file = v[1].trim().to_string();
                        let form = if file != before.file || before.form.is_empty() {
                            mime_for(&file).to_string()
                        } else {
                            before.form
                        };
                        MediaForm { file, form, title: v[0].clone() }.apply(node)
                    }
                    RecordKind::SharedNote => node.set_text(&note),
                    RecordKind::Submitter => node.set_child_val("NAME", &v[0]),
                }
                if *kind == RecordKind::Family {
                    sync_family_links(ged, xref);
                }
                Ok(format!("{} : modifications appliquées", ged.record_label(xref)))
            }
        }
    }

    /// Applique le texte de l'éditeur GEDCOM brut. Renvoie le nouvel identifiant.
    pub fn apply_raw(&self, ged: &mut Gedcom, cx: &App) -> Result<String, String> {
        let Target::Record { xref, kind } = &self.target else { return Err("rien à appliquer".into()) };
        let raw = self.raw.as_ref().ok_or("pas d'éditeur brut")?;
        let text = raw.read(cx).value().to_string();
        let node = node_from_string(&text).map_err(|e| e.to_string())?;
        if node.tag != kind.tag() {
            return Err(format!("le type doit rester {} (trouvé {})", kind.tag(), node.tag));
        }
        let new_xref = node.xref.clone().unwrap_or_default();
        ged.replace(xref, node)?;
        Ok(new_xref)
    }
}

/// Après édition d'une famille, garde les `FAMS` des époux cohérents.
fn sync_family_links(ged: &mut Gedcom, fam: &str) {
    let Some(f) = ged.family(fam) else { return };
    let spouses: Vec<String> = [f.husband(), f.wife()].into_iter().flatten().map(str::to_string).collect();
    let ptr = format!("@{fam}@");
    let indis: Vec<String> = ged.records_of(RecordKind::Individual).filter_map(|r| r.xref.clone()).collect();
    for x in indis {
        let is_spouse = spouses.contains(&x);
        let Some(node) = ged.get_mut(&x) else { continue };
        let has = node.children.iter().any(|c| c.tag == "FAMS" && c.val() == ptr);
        if is_spouse && !has {
            node.children.push(Node::with_value("FAMS", ptr.clone()));
        } else if !is_spouse && has {
            node.children.retain(|c| !(c.tag == "FAMS" && c.val() == ptr));
        }
    }
}
