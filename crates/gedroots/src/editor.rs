//! Éditeurs d'un enregistrement : formulaire visuel + éditeur GEDCOM brut.

use gedcom::*;
use gpui_kit::component::input::{EditorState, InputState, TextareaState};
use gpui_kit::{App, AppContext, Context, Entity, SharedString, Window};

/// Ce qui est sélectionné dans la vue courante.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    Record { xref: String, kind: RecordKind },
    Place(String),
}

pub struct Field {
    pub label: &'static str,
    pub hint: &'static str,
    pub state: Entity<InputState>,
}

/// États des champs pour l'élément sélectionné.
pub struct RecordEditor {
    pub target: Target,
    pub fields: Vec<Field>,
    pub long_text: Option<(&'static str, Entity<TextareaState>)>,
    pub raw: Option<Entity<EditorState>>,
    pub error: Option<SharedString>,
}

fn input<T: 'static>(window: &mut Window, cx: &mut Context<T>, value: &str) -> Entity<InputState> {
    let value = value.to_string();
    cx.new(|cx| InputState::new(window, cx).default_value(value))
}

fn textarea<T: 'static>(window: &mut Window, cx: &mut Context<T>, value: &str) -> Entity<TextareaState> {
    let value = value.to_string();
    cx.new(|cx| TextareaState::new(window, cx).rows(5).default_value(value))
}

fn get(cx: &App, e: &Entity<InputState>) -> String {
    e.read(cx).value().to_string()
}

impl RecordEditor {
    pub fn new<T: 'static>(ged: &Gedcom, target: Target, window: &mut Window, cx: &mut Context<T>) -> Self {
        let mut fields = Vec::new();
        let mut long_text = None;
        let mut raw = None;
        macro_rules! field {
            ($label:expr, $hint:expr, $v:expr) => {
                fields.push(Field { label: $label, hint: $hint, state: input(window, cx, &$v) })
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
                    return Self { target, fields, long_text, raw, error: Some("Enregistrement introuvable".into()) };
                };
                match kind {
                    RecordKind::Individual => {
                        let f = IndividualForm::read(node);
                        field!("Prénoms", "", f.given);
                        field!("Nom", "", f.surname);
                        field!("Sexe", "M, F, X ou U", f.sex);
                        field!("Naissance — date", "ex. 14 MAR 1912, ABT 1850", f.birth.date);
                        field!("Naissance — lieu", "", f.birth.place);
                        field!("Décès — date", "", f.death.date);
                        field!("Décès — lieu", "", f.death.place);
                        field!("Profession", "", f.occupation);
                        long_text = Some(("Note", textarea(window, cx, &f.note)));
                    }
                    RecordKind::Family => {
                        let f = FamilyForm::read(node);
                        field!("Époux (identifiant)", "ex. I00012", f.husband);
                        field!("Épouse (identifiant)", "ex. I00013", f.wife);
                        field!("Mariage — date", "", f.marriage.date);
                        field!("Mariage — lieu", "", f.marriage.place);
                    }
                    RecordKind::Source => {
                        let f = SourceForm::read(node);
                        field!("Titre", "", f.title);
                        field!("Auteur", "", f.author);
                        field!("Publication", "", f.publication);
                        field!("Dépôt (identifiant)", "ex. R00001", f.repository);
                        field!("Cote", "", f.call_number);
                        long_text = Some(("Texte de la source", textarea(window, cx, &f.text)));
                    }
                    RecordKind::Repository => {
                        let f = RepositoryForm::read(node);
                        field!("Nom", "", f.name);
                        field!("Site web", "", f.www);
                    }
                    RecordKind::Media => {
                        let f = MediaForm::read(node);
                        field!("Fichier", "chemin relatif au fichier .ged", f.file);
                        field!("Format", "ex. image/jpeg", f.form);
                        field!("Titre", "", f.title);
                    }
                    RecordKind::SharedNote => {
                        long_text = Some(("Texte", textarea(window, cx, &node.text())));
                    }
                    RecordKind::Submitter => {
                        field!("Nom", "", node.child_val("NAME").unwrap_or(""));
                    }
                }
                let text = node_to_string(node);
                raw = Some(cx.new(|cx| EditorState::new(window, cx).line_number(true).default_value(text)));
            }
        }
        Self { target, fields, long_text, raw, error: None }
    }

    fn values(&self, cx: &App) -> Vec<String> {
        self.fields.iter().map(|f| get(cx, &f.state)).collect()
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
                if let (RecordKind::Family, Some(bad)) = (
                    kind,
                    [&v[0], &v[1]]
                        .into_iter()
                        .map(|x| x.trim().trim_matches('@'))
                        .find(|x| !x.is_empty() && ged.individual(x).is_none()),
                ) {
                    return Err(format!("@{bad}@ n'est pas un individu connu"));
                }
                let node = ged.get_mut(xref).ok_or("Enregistrement introuvable")?;
                match kind {
                    RecordKind::Individual => IndividualForm {
                        given: v[0].clone(),
                        surname: v[1].clone(),
                        sex: v[2].clone(),
                        birth: EventForm { date: v[3].clone(), place: v[4].clone() },
                        death: EventForm { date: v[5].clone(), place: v[6].clone() },
                        occupation: v[7].clone(),
                        note,
                    }
                    .apply(node),
                    RecordKind::Family => FamilyForm {
                        husband: v[0].clone(),
                        wife: v[1].clone(),
                        marriage: EventForm { date: v[2].clone(), place: v[3].clone() },
                    }
                    .apply(node),
                    RecordKind::Source => SourceForm {
                        title: v[0].clone(),
                        author: v[1].clone(),
                        publication: v[2].clone(),
                        repository: v[3].clone(),
                        call_number: v[4].clone(),
                        text: note,
                    }
                    .apply(node),
                    RecordKind::Repository => RepositoryForm { name: v[0].clone(), www: v[1].clone() }.apply(node),
                    RecordKind::Media => {
                        MediaForm { file: v[0].clone(), form: v[1].clone(), title: v[2].clone() }.apply(node)
                    }
                    RecordKind::SharedNote => node.set_text(&note),
                    RecordKind::Submitter => node.set_child_val("NAME", &v[0]),
                }
                if *kind == RecordKind::Family {
                    sync_family_links(ged, xref);
                }
                Ok(format!("@{xref}@ mis à jour"))
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
