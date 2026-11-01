//! Saisie guidée d'un acte : un seul formulaire crée la source, les individus,
//! les événements, les liens familiaux et les citations.
//!
//! C'est l'équivalent du module *Ancestral Sources* de Family Historian : on
//! choisit un modèle (naissance, mariage…), on renseigne la source, la date, le
//! lieu et les personnes citées ; pour chaque personne on choisit de la **lier**
//! à un individu existant (avec suggestions), d'en **créer** un nouveau, ou de
//! la **mentionner** seulement. [`Entry::apply`] écrit tout en GEDCOM 7 standard
//! (citations avec `PAGE`, `DATA`, `EVEN`/`ROLE`, `ASSO` pour les témoins…) ;
//! seules les valeurs structurées des champs de la citation passent par
//! l'extension `_GR_FLD`.

use crate::document::{Gedcom, RecordKind};
use crate::model::{Individual, sort_key};
use crate::node::Node;
use crate::template::{PersonField, Relation, RoleDef, Template};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SourceChoice {
    /// Source existante (identifiant).
    Existing(String),
    /// Nouvelle source (registre, recensement…).
    New { title: String, publication: String, repository: Option<String>, call_number: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum MediaChoice {
    #[default]
    None,
    /// Média existant (identifiant `OBJE`).
    Existing(String),
    /// Nouveau fichier, chemin relatif au dossier du `.ged` de préférence.
    NewFile(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum PersonChoice {
    /// Créer un nouvel individu.
    #[default]
    Create,
    /// Lier à un individu existant.
    Link(String),
    /// Mentionner dans la citation sans créer d'individu.
    Mention,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Participant {
    /// Clé du rôle dans le modèle.
    pub role: String,
    pub values: Vec<(PersonField, String)>,
    pub choice: PersonChoice,
}

impl Participant {
    pub fn new(role: &str) -> Self {
        Self { role: role.into(), values: Vec::new(), choice: PersonChoice::Create }
    }

    pub fn get(&self, f: PersonField) -> &str {
        self.values.iter().find(|(k, _)| *k == f).map(|(_, v)| v.trim()).unwrap_or("")
    }

    pub fn set(&mut self, f: PersonField, v: impl Into<String>) {
        let v = v.into();
        match self.values.iter_mut().find(|(k, _)| *k == f) {
            Some(slot) => slot.1 = v,
            None => self.values.push((f, v)),
        }
    }

    pub fn with(mut self, f: PersonField, v: &str) -> Self {
        self.set(f, v);
        self
    }

    pub fn is_blank(&self) -> bool {
        self.values.iter().all(|(_, v)| v.trim().is_empty())
    }

    /// « Prénoms NOM ».
    pub fn display_name(&self) -> String {
        let (g, s) = (self.get(PersonField::Given), self.get(PersonField::Surname));
        format!("{g} {s}").trim().to_string()
    }
}

/// Un acte en cours de saisie.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub template: Template,
    pub source: SourceChoice,
    /// Date de l'événement (format GEDCOM : `14 MAR 1912`, `ABT 1850`…).
    pub date: String,
    /// Phrase de la date (`DATE.PHRASE`), ex. le texte d'origine.
    pub date_phrase: String,
    /// Lieu de l'événement (au format des lieux du fichier).
    pub place: String,
    /// Date de rédaction de l'acte si elle diffère (`DATA.DATE` de la citation).
    pub record_date: String,
    pub record_date_phrase: String,
    /// Valeurs des champs de la citation (clé du modèle → valeur).
    pub fields: Vec<(String, String)>,
    /// Transcription (`DATA.TEXT`).
    pub transcription: String,
    /// Qualité de la preuve (`QUAY` 0 à 3).
    pub quality: Option<u8>,
    pub media: MediaChoice,
    pub participants: Vec<Participant>,
    /// Créer une naissance estimée (`EST`) à partir de l'âge déclaré quand il n'y en a pas.
    pub estimate_births: bool,
}

/// Ce que l'enregistrement d'un acte a produit.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EntryResult {
    pub source: String,
    pub media: Option<String>,
    /// Identifiant de chaque participant (`None` pour une simple mention), dans l'ordre de saisie.
    pub persons: Vec<Option<String>>,
    pub created: Vec<String>,
    pub families_created: Vec<String>,
    /// Tous les enregistrements créés ou modifiés.
    pub touched: Vec<String>,
}

impl EntryResult {
    pub fn summary(&self) -> String {
        let linked = self.persons.iter().flatten().count();
        let mut s = format!("Acte enregistré : {linked} personne{}", if linked > 1 { "s" } else { "" });
        let new = self.persons.iter().flatten().filter(|x| self.created.contains(x)).count();
        if new > 0 {
            s += &format!(" dont {new} créée{}", if new > 1 { "s" } else { "" });
        }
        if !self.families_created.is_empty() {
            s += &format!(
                ", {} famille{} créée{}",
                self.families_created.len(),
                if self.families_created.len() > 1 { "s" } else { "" },
                if self.families_created.len() > 1 { "s" } else { "" }
            );
        }
        s
    }
}

/// Un individu existant qui pourrait correspondre à une personne citée.
#[derive(Debug, Clone, PartialEq)]
pub struct Suggestion {
    pub xref: String,
    pub label: String,
    pub score: f32,
    /// Pourquoi on le propose (« père de Paul MARCHAND », « né vers 1880 »…).
    pub reason: String,
}

impl Entry {
    /// Nouvel acte vide avec les rôles par défaut du modèle.
    pub fn new(template: Template, source: SourceChoice) -> Self {
        let participants = template.roles.iter().filter(|r| r.default).map(|r| Participant::new(&r.key)).collect();
        Self {
            fields: template.fields.iter().map(|f| (f.key.clone(), String::new())).collect(),
            template,
            source,
            date: String::new(),
            date_phrase: String::new(),
            place: String::new(),
            record_date: String::new(),
            record_date_phrase: String::new(),
            transcription: String::new(),
            quality: None,
            media: MediaChoice::None,
            participants,
            estimate_births: true,
        }
    }

    pub fn field(&self, key: &str) -> &str {
        self.fields.iter().find(|(k, _)| k == key).map(|(_, v)| v.as_str()).unwrap_or("")
    }

    pub fn set_field(&mut self, key: &str, v: impl Into<String>) {
        let v = v.into();
        match self.fields.iter_mut().find(|(k, _)| k == key) {
            Some(slot) => slot.1 = v,
            None => self.fields.push((key.to_string(), v)),
        }
    }

    /// Référence de la citation (`PAGE`).
    pub fn page(&self) -> String {
        self.template.format_page(&self.fields)
    }

    fn role_of(&self, p: &Participant) -> Option<&RoleDef> {
        self.template.role(&p.role)
    }

    /// Année de l'événement, si la date en contient une.
    pub fn year(&self) -> Option<i32> {
        crate::Date::parse(&self.date).year()
    }

    /// Prépare l'acte suivant du même registre : garde la source, le lieu, le média
    /// et incrémente le n° d'acte / la vue.
    pub fn next(&self) -> Self {
        let mut e = Entry::new(self.template.clone(), self.source.clone());
        e.place = self.place.clone();
        e.media = self.media.clone();
        e.quality = self.quality;
        e.estimate_births = self.estimate_births;
        for (k, v) in &self.fields {
            let next = match k.as_str() {
                "numero" | "menage" | "matricule" => v.trim().parse::<u64>().map(|n| (n + 1).to_string()).ok(),
                "classe" | "folio" | "vue" | "page" | "rue" => Some(v.clone()),
                _ => None,
            };
            e.set_field(k, next.unwrap_or_default());
        }
        e
    }

    /// Titre d'une source propre à cet acte : « Acte de naissance de Rémi GUYOT ».
    pub fn auto_title(&self) -> String {
        let names: Vec<String> = self
            .effective()
            .iter()
            .filter(|p| self.role_of(p).is_some_and(|r| r.relation == Relation::Principal) && !p.is_blank())
            .map(|p| {
                let (g, s) = (p.get(PersonField::Given), p.get(PersonField::Surname).to_uppercase());
                format!("{g} {s}").trim().to_string()
            })
            .collect();
        let what = self.template.event_label().to_lowercase();
        let article = if what.starts_with(|c: char| "aeéèiouy".contains(c)) { "d'" } else { "de " };
        match names.len() {
            0 => format!("Acte {article}{what}"),
            _ => format!("Acte {article}{what} de {}", names.join(" et ")),
        }
    }

    /// Résumé lisible de l'acte, à la manière du bouton « Auto » d'Ancestral Sources.
    pub fn auto_text(&self) -> String {
        let mut lines = vec![format!(
            "{}{}{}",
            self.template.event_label(),
            if self.date.is_empty() {
                String::new()
            } else {
                format!(" le {}", crate::Date::parse(&self.date).to_french())
            },
            if self.place.is_empty() { String::new() } else { format!(" à {}", crate::short_place(&self.place)) }
        )];
        for p in &self.participants {
            if p.is_blank() {
                continue;
            }
            let label = self.role_of(p).map(|r| r.label.as_str()).unwrap_or(&p.role);
            let mut s = format!("{label} : {}", p.display_name());
            for f in [
                PersonField::Age,
                PersonField::Occupation,
                PersonField::Residence,
                PersonField::Birthplace,
                PersonField::Kinship,
            ] {
                let v = p.get(f);
                if !v.is_empty() {
                    s += &match f {
                        PersonField::Age => format!(", {}", age_text(v)),
                        PersonField::Residence => format!(", domicilié(e) à {}", crate::short_place(v)),
                        PersonField::Birthplace => format!(", né(e) à {}", crate::short_place(v)),
                        _ => format!(", {v}"),
                    };
                }
            }
            lines.push(s);
        }
        lines.join("\n")
    }

    /// Vérifie l'acte avant écriture. Renvoie la liste des problèmes bloquants.
    pub fn validate(&self, ged: &Gedcom) -> Result<(), String> {
        if let SourceChoice::Existing(x) = &self.source
            && ged.get(x).is_none_or(|n| n.tag != "SOUR")
        {
            return Err("la source choisie n'existe plus".into());
        }
        if let MediaChoice::Existing(x) = &self.media
            && ged.get(x).is_none_or(|n| n.tag != "OBJE")
        {
            return Err("le média choisi n'existe plus".into());
        }
        if !self.date.trim().is_empty() {
            let d = crate::Date::parse(&self.date);
            if d.year().is_none() {
                return Err(format!("date non reconnue : « {} »", self.date.trim()));
            }
            d.validate().map_err(|e| format!("date impossible : {e}"))?;
        }
        let mut principal = 0;
        for p in &self.participants {
            let role = self.role_of(p).ok_or_else(|| format!("rôle inconnu : {}", p.role))?;
            let is_principal = role.relation == Relation::Principal;
            match &p.choice {
                PersonChoice::Link(x) => {
                    if ged.individual(x).is_none() {
                        return Err(format!("{} : individu introuvable", role.label));
                    }
                    if is_principal {
                        principal += 1;
                    }
                }
                PersonChoice::Create => {
                    if p.is_blank() {
                        // Ligne vide : ignorée, sauf pour la personne principale.
                        continue;
                    }
                    if p.get(PersonField::Surname).is_empty() && p.get(PersonField::Given).is_empty() {
                        return Err(format!("{} : indiquez au moins un nom ou un prénom", role.label));
                    }
                    if is_principal {
                        principal += 1;
                    }
                }
                PersonChoice::Mention => {
                    if is_principal {
                        return Err(format!("{} : la personne principale doit être liée ou créée", role.label));
                    }
                }
            }
        }
        if principal == 0 {
            return Err("renseignez au moins la personne principale de l'acte".into());
        }
        if self.template.is_family_event() && principal < 2 {
            return Err("un événement familial demande les deux époux".into());
        }
        Ok(())
    }

    /// Valeurs effectives : un parent sans nom prend celui de l'enfant, et inversement.
    fn effective(&self) -> Vec<Participant> {
        let mut ps = self.participants.clone();
        let surname_of = |ps: &[Participant], key: &str| {
            ps.iter().find(|p| p.role == key).map(|p| p.get(PersonField::Surname).to_string()).unwrap_or_default()
        };
        for i in 0..ps.len() {
            if !ps[i].get(PersonField::Surname).is_empty() || ps[i].choice != PersonChoice::Create {
                continue;
            }
            let Some(role) = self.role_of(&ps[i]) else { continue };
            let inherited = match &role.relation {
                Relation::FatherOf(k) => surname_of(&ps, k),
                Relation::Principal => {
                    // Enfant : nom du père.
                    let father =
                        self.template.roles.iter().find(|r| r.relation == Relation::FatherOf(role.key.clone()));
                    father.map(|f| surname_of(&ps, &f.key)).unwrap_or_default()
                }
                _ => String::new(),
            };
            if !inherited.is_empty() {
                ps[i].set(PersonField::Surname, inherited);
            }
        }
        ps
    }

    /// Calcule le GEDCOM qui serait écrit, sans toucher au document : texte des
    /// enregistrements créés ou modifiés.
    pub fn preview(&self, ged: &Gedcom) -> Result<String, String> {
        let mut copy = ged.clone();
        let res = self.apply(&mut copy)?;
        let mut out = String::new();
        for x in &res.touched {
            if let Some(n) = copy.get(x) {
                out.push_str(&crate::parse::node_to_string(n));
                if !out.ends_with('\n') {
                    out.push('\n');
                }
            }
        }
        Ok(out)
    }

    /// Écrit l'acte dans le document.
    pub fn apply(&self, ged: &mut Gedcom) -> Result<EntryResult, String> {
        self.validate(ged)?;
        let mut w = Writer { ged, res: EntryResult::default() };
        let t = &self.template;

        // 1. Source.
        let source = match &self.source {
            SourceChoice::Existing(x) => {
                let x = x.clone();
                let tpl_ref = template_ref(t);
                let node = w.ged.get_mut(&x).unwrap();
                if node.child("_GR_TPL").is_none() {
                    node.insert_child(Node::with_value("_GR_TPL", tpl_ref));
                    w.touch(&x);
                }
                x
            }
            SourceChoice::New { title, publication, repository, call_number } => {
                let title = if title.trim().is_empty() { self.auto_title() } else { title.trim().to_string() };
                let x = w.ged.create(RecordKind::Source);
                let node = w.ged.get_mut(&x).unwrap();
                node.set_child_text("TITL", &title);
                node.set_child_text("PUBL", publication.trim());
                if let Some(r) = repository.as_deref().map(|r| r.trim().trim_matches('@')).filter(|r| !r.is_empty()) {
                    let mut repo = Node::with_value("REPO", format!("@{r}@"));
                    if !call_number.trim().is_empty() {
                        repo.children.push(Node::with_value("CALN", call_number.trim()));
                    }
                    node.insert_child(repo);
                }
                node.insert_child(Node::with_value("_GR_TPL", template_ref(t)));
                w.mark_created(&x);
                x
            }
        };
        w.ged.declare_extensions();
        w.res.source = source.clone();

        // 2. Média.
        let media = match &self.media {
            MediaChoice::None => None,
            MediaChoice::Existing(x) => Some(x.clone()),
            MediaChoice::NewFile(path) if !path.trim().is_empty() => {
                let x = w.ged.create(RecordKind::Media);
                let title = [w.ged.record_label(&source), self.page()]
                    .into_iter()
                    .filter(|s| !s.is_empty())
                    .collect::<Vec<_>>()
                    .join(", ");
                let node = w.ged.get_mut(&x).unwrap();
                let file = node.ensure_child("FILE");
                file.value = Some(path.trim().to_string());
                file.set_child_val("FORM", media_type(path));
                file.set_child_val("TITL", &title);
                w.mark_created(&x);
                Some(x)
            }
            MediaChoice::NewFile(_) => None,
        };
        if let Some(m) = &media {
            let ptr = format!("@{m}@");
            let src = w.ged.get_mut(&source).unwrap();
            if !src.children.iter().any(|c| c.tag == "OBJE" && c.val() == ptr) {
                src.insert_child(Node::with_value("OBJE", ptr));
                w.touch(&source);
            }
        }
        w.res.media = media.clone();

        // 3. Personnes : lier ou créer.
        let people = self.effective();
        let mut ids: Vec<Option<String>> = Vec::new();
        for p in &people {
            let role = self.role_of(p).unwrap();
            let id = match &p.choice {
                PersonChoice::Mention => None,
                PersonChoice::Create if p.is_blank() => None,
                PersonChoice::Link(x) => {
                    // Complète le sexe s'il était inconnu.
                    let sex = role.sex.clone().unwrap_or_else(|| p.get(PersonField::Sex).to_uppercase());
                    if matches!(sex.as_str(), "M" | "F")
                        && let Some(n) = w.ged.get_mut(x)
                        && n.child_val("SEX").is_none_or(|s| s == "U")
                    {
                        n.set_child_val("SEX", &sex);
                        w.touch(x);
                    }
                    Some(x.clone())
                }
                PersonChoice::Create => {
                    let x = w.ged.create(RecordKind::Individual);
                    let given = p.get(PersonField::Given);
                    let surname = p.get(PersonField::Surname);
                    let sex = role.sex.clone().unwrap_or_else(|| p.get(PersonField::Sex).to_uppercase());
                    let node = w.ged.get_mut(&x).unwrap();
                    let name = node.ensure_child("NAME");
                    name.value = Some(format!("{given} /{surname}/").trim_start().to_string());
                    name.set_child_val("GIVN", given);
                    name.set_child_val("SURN", surname);
                    node.set_child_val("SEX", if matches!(sex.as_str(), "M" | "F" | "X") { &sex } else { "U" });
                    w.mark_created(&x);
                    Some(x)
                }
            };
            ids.push(id);
        }
        w.res.persons = ids.clone();
        let id_of = |key: &str| -> Option<String> {
            people.iter().zip(&ids).find(|(p, id)| p.role == key && id.is_some()).and_then(|(_, id)| id.clone())
        };

        // 4. Liens familiaux.
        let mut parents: Vec<(String, Option<String>, Option<String>)> = Vec::new();
        for (p, id) in people.iter().zip(&ids) {
            let Some(id) = id else { continue };
            let role = self.role_of(p).unwrap();
            match &role.relation {
                Relation::FatherOf(k) | Relation::MotherOf(k) => {
                    let Some(child) = id_of(k) else { continue };
                    let slot = match parents.iter_mut().find(|(c, _, _)| *c == child) {
                        Some(s) => s,
                        None => {
                            parents.push((child.clone(), None, None));
                            parents.last_mut().unwrap()
                        }
                    };
                    if matches!(role.relation, Relation::FatherOf(_)) {
                        slot.1 = Some(id.clone());
                    } else {
                        slot.2 = Some(id.clone());
                    }
                }
                Relation::SpouseOf(k) => {
                    if let Some(other) = id_of(k) {
                        w.ensure_couple(&other, id);
                    }
                }
                _ => {}
            }
        }
        for (child, f, m) in parents {
            w.ensure_parents(&child, f.as_deref(), m.as_deref());
        }
        self.link_household(&mut w, &people, &ids);

        // 5. Citations et événements.
        let year = self.year();
        let family = if t.is_family_event() {
            let principals: Vec<String> = people
                .iter()
                .zip(&ids)
                .filter(|(p, id)| id.is_some() && self.role_of(p).is_some_and(|r| r.relation == Relation::Principal))
                .map(|(_, id)| id.clone().unwrap())
                .collect();
            Some(w.ensure_couple(&principals[0], &principals[1]))
        } else {
            None
        };
        let mut mentions = Vec::new();
        for p in &people {
            if p.choice == PersonChoice::Mention && !p.is_blank() {
                let role = self.role_of(p).unwrap();
                let mut s = format!("{} : {}", role.label, p.display_name());
                for f in [PersonField::Age, PersonField::Occupation, PersonField::Residence, PersonField::Kinship] {
                    if !p.get(f).is_empty() {
                        s += &format!(
                            ", {}",
                            if f == PersonField::Age { age_text(p.get(f)) } else { p.get(f).to_string() }
                        );
                    }
                }
                mentions.push(s);
            }
        }

        // Événement principal : sur la famille, ou sur chaque personne principale.
        // On retient où il se trouve pour y attacher les témoins (ASSO).
        let mut main_events: Vec<(String, usize)> = Vec::new();
        if let Some(fam) = &family {
            let mut ev = self.event_node(None);
            let mut cite = self.citation(&source, media.as_deref(), None, true);
            if !mentions.is_empty() {
                cite.children.push(Node::with_value("NOTE", mentions.join("; ")));
            }
            ev.children.push(cite);
            for (p, id) in people.iter().zip(&ids) {
                let Some(_) = id else { continue };
                let role = self.role_of(p).unwrap();
                if role.relation == Relation::Principal && !p.get(PersonField::Age).is_empty() {
                    let tag = if role.role == "WIFE" { "WIFE" } else { "HUSB" };
                    if let Some(age) = gedcom_age(p.get(PersonField::Age)) {
                        let at = ev.children.iter().position(|c| c.tag == "SOUR").unwrap_or(ev.children.len());
                        ev.children.insert(at, Node::new(tag).push(Node::with_value("AGE", age)));
                    }
                }
            }
            let node = w.ged.get_mut(fam).unwrap();
            node.insert_child(ev);
            let i = node.children.iter().rposition(|c| c.tag == t.event).unwrap();
            main_events.push((fam.clone(), i));
            w.touch(fam);
        }

        let mut first_principal = true;
        for (p, id) in people.iter().zip(&ids) {
            let Some(x) = id else { continue };
            let role = self.role_of(p).unwrap();
            let age = p.get(PersonField::Age);
            if role.relation == Relation::Principal && family.is_none() {
                let mut ev = self.event_node(gedcom_age(age));
                let mut cite = self.citation(&source, media.as_deref(), Some(role), first_principal);
                if first_principal && !mentions.is_empty() {
                    cite.children.push(Node::with_value("NOTE", mentions.join("; ")));
                }
                first_principal = false;
                if !p.get(PersonField::Note).is_empty() {
                    cite.children.push(Node::with_value("NOTE", p.get(PersonField::Note)));
                }
                ev.children.push(cite);
                let i = w.add_event(x, ev);
                main_events.push((x.clone(), i));
            } else {
                // Citation au niveau de l'individu, avec son rôle dans l'événement.
                let mut cite = self.citation(&source, media.as_deref(), Some(role), false);
                if !p.get(PersonField::Note).is_empty() {
                    cite.children.push(Node::with_value("NOTE", p.get(PersonField::Note)));
                }
                if !w.has_citation(x, &cite) {
                    let node = w.ged.get_mut(x).unwrap();
                    let before = node.children.len();
                    merge_citation(&mut node.children, cite);
                    // Nouvelle citation : on la range avant CREA/CHAN.
                    if node.children.len() > before {
                        let c = node.children.pop().unwrap();
                        node.insert_child(c);
                    }
                    w.touch(x);
                }
            }

            // Faits déduits : profession, domicile, naissance estimée.
            let occu = p.get(PersonField::Occupation);
            if !occu.is_empty() {
                let mut n = Node::with_value("OCCU", occu);
                if let Some(d) = self.date_node() {
                    n.children.push(d);
                }
                n.children.push(self.citation(&source, media.as_deref(), Some(role), false));
                w.add_fact(x, n);
            }
            let resi = p.get(PersonField::Residence);
            if !resi.is_empty() {
                let mut n = Node::new("RESI");
                if let Some(d) = self.date_node() {
                    n.children.push(d);
                }
                n.children.push(Node::with_value("PLAC", resi));
                n.children.push(self.citation(&source, media.as_deref(), Some(role), false));
                w.add_fact(x, n);
            }
            let is_birth_event =
                matches!(t.event.as_str(), "BIRT" | "CHR" | "BAPM") && role.relation == Relation::Principal;
            if !is_birth_event {
                let birthplace = p.get(PersonField::Birthplace);
                let est = match (self.estimate_births, year, parse_age_years(age)) {
                    (true, Some(y), Some(a)) => Some(format!("EST {}", y - a)),
                    _ => None,
                };
                if (est.is_some() || !birthplace.is_empty()) && !w.has_birth(x) {
                    let mut n = Node::new("BIRT");
                    if let Some(d) = est {
                        n.children.push(Node::with_value("DATE", d));
                    }
                    if !birthplace.is_empty() {
                        n.children.push(Node::with_value("PLAC", birthplace));
                    }
                    n.children.push(self.citation(&source, media.as_deref(), Some(role), false));
                    w.ged.get_mut(x).unwrap().insert_child(n);
                    w.touch(x);
                }
            }
        }

        // 6. Témoins et autres participants associés à l'événement principal.
        if let Some((holder, idx)) = main_events.first().cloned() {
            for (p, id) in people.iter().zip(&ids) {
                let Some(x) = id else { continue };
                let role = self.role_of(p).unwrap();
                if role.relation != Relation::Witness {
                    continue;
                }
                let mut r = Node::with_value("ROLE", &role.role);
                let phrase: Vec<&str> = [role.phrase.as_deref().unwrap_or(""), p.get(PersonField::Kinship)]
                    .into_iter()
                    .filter(|s| !s.is_empty())
                    .collect();
                if !phrase.is_empty() {
                    r.children.push(Node::with_value("PHRASE", phrase.join(", ")));
                }
                let asso = Node::with_value("ASSO", format!("@{x}@")).push(r);
                let ev = &mut w.ged.get_mut(&holder).unwrap().children[idx];
                let at = ev.children.iter().position(|c| c.tag == "SOUR").unwrap_or(ev.children.len());
                ev.children.insert(at, asso);
            }
        }

        let mut res = w.res;
        res.touched.dedup();
        Ok(res)
    }

    /// `DATE` (+ `PHRASE`) de l'événement.
    fn date_node(&self) -> Option<Node> {
        crate::date::date_node(&self.date, &self.date_phrase)
    }

    /// Nœud de l'événement de l'acte (sans citation).
    fn event_node(&self, age: Option<String>) -> Node {
        let t = &self.template;
        let mut ev = Node::new(&t.event);
        if let Some(ty) = &t.event_type {
            ev.children.push(Node::with_value("TYPE", ty));
        }
        if let Some(d) = self.date_node() {
            ev.children.push(d);
        }
        if !self.place.trim().is_empty() {
            ev.children.push(Node::with_value("PLAC", self.place.trim()));
        }
        if let Some(a) = age {
            ev.children.push(Node::with_value("AGE", a));
        }
        ev
    }

    /// Citation de la source. `full` ajoute la transcription et les champs structurés
    /// (on ne les écrit qu'une fois, sur l'événement principal).
    fn citation(&self, source: &str, media: Option<&str>, role: Option<&RoleDef>, full: bool) -> Node {
        let mut c = Node::with_value("SOUR", format!("@{source}@"));
        let page = self.page();
        if !page.is_empty() {
            c.children.push(Node::with_value("PAGE", page));
        }
        let own = !self.record_date.trim().is_empty() || !self.record_date_phrase.trim().is_empty();
        let date =
            if own { crate::date::date_node(&self.record_date, &self.record_date_phrase) } else { self.date_node() };
        let text = self.transcription.trim();
        if date.is_some() || (full && !text.is_empty()) {
            let mut data = Node::new("DATA");
            if let Some(d) = date {
                data.children.push(d);
            }
            if full && !text.is_empty() {
                let mut t = Node::new("TEXT");
                t.set_text(text);
                data.children.push(t);
            }
            c.children.push(data);
        }
        if let Some(r) = role {
            let mut ev = Node::with_value("EVEN", &self.template.event);
            let mut rn = Node::with_value("ROLE", &r.role);
            if let Some(p) = &r.phrase {
                rn.children.push(Node::with_value("PHRASE", p));
            }
            ev.children.push(rn);
            c.children.push(ev);
        }
        if let Some(q) = self.quality {
            c.children.push(Node::with_value("QUAY", q.min(3).to_string()));
        }
        if let Some(m) = media {
            c.children.push(Node::with_value("OBJE", format!("@{m}@")));
        }
        if full {
            for (k, v) in &self.fields {
                if !v.trim().is_empty() {
                    c.children.push(Node::with_value("_GR_FLD", format!("{k} {}", v.trim())));
                }
            }
        }
        c
    }

    /// Recensement : conjoint et enfants du chef de ménage d'après le « lien déclaré ».
    fn link_household(&self, w: &mut Writer, people: &[Participant], ids: &[Option<String>]) {
        for (head_role, members) in self.template.roles.iter().filter_map(|r| match &r.relation {
            Relation::HouseholdOf(h) => Some((h.clone(), r.key.clone())),
            _ => None,
        }) {
            let Some(head) = people.iter().zip(ids).find(|(p, id)| p.role == head_role && id.is_some()) else {
                continue;
            };
            let head = head.1.clone().unwrap();
            let members: Vec<(&Participant, String)> = people
                .iter()
                .zip(ids)
                .filter(|(p, id)| p.role == members && id.is_some())
                .map(|(p, id)| (p, id.clone().unwrap()))
                .collect();
            let kin = |p: &Participant| sort_key(p.get(PersonField::Kinship));
            let spouse = members.iter().find(|(p, _)| {
                let k = kin(p);
                ["epou", "femme", "mari", "conjoint"].iter().any(|s| k.contains(s))
            });
            if let Some((_, s)) = spouse {
                w.ensure_couple(&head, s);
            }
            for (p, x) in &members {
                let k = kin(p);
                if !["fils", "fille", "enfant"].iter().any(|s| k.contains(s))
                    || k.contains("petit")
                    || k.contains("beau")
                    || k.contains("belle")
                {
                    continue;
                }
                let couple: Vec<String> = std::iter::once(head.clone()).chain(spouse.map(|(_, s)| s.clone())).collect();
                let sex = |x: &str| w.ged.individual(x).map(|i| i.sex().to_string()).unwrap_or_default();
                let father = couple.iter().find(|c| sex(c) == "M").cloned();
                let mother = couple.iter().find(|c| sex(c) == "F").cloned();
                let (father, mother) = match (father, mother) {
                    (None, None) => (Some(head.clone()), None),
                    fm => fm,
                };
                w.ensure_parents(x, father.as_deref(), mother.as_deref());
            }
        }
    }
}

/// Valeur de `_GR_TPL` sur une source : identifiant intégré ou pointeur vers l'enregistrement.
fn template_ref(t: &Template) -> String {
    if t.custom { format!("@{}@", t.id) } else { t.id.clone() }
}

fn media_type(path: &str) -> &'static str {
    let l = path.to_ascii_lowercase();
    match l.rsplit('.').next().unwrap_or("") {
        "jpg" | "jpeg" => "image/jpeg",
        "png" => "image/png",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "tif" | "tiff" => "image/tiff",
        "bmp" => "image/bmp",
        "pdf" => "application/pdf",
        _ => "application/octet-stream",
    }
}

/// Âge en années à partir d'une saisie libre (« 32 ans », « 32 », « 6 mois » → 0).
pub fn parse_age_years(s: &str) -> Option<i32> {
    let s = sort_key(s.trim());
    if s.is_empty() {
        return None;
    }
    let digits: String = s.chars().take_while(|c| c.is_ascii_digit()).collect();
    let n: i32 = digits.parse().ok()?;
    let unit = s[digits.len()..].trim();
    if unit.starts_with('m')
        || unit.starts_with('j')
        || unit.starts_with('s')
        || unit.starts_with('d')
        || unit.starts_with('w')
    {
        Some(0)
    } else {
        Some(n)
    }
}

/// Âge au format GEDCOM 7 (`32y`, `6m`, `12d`).
pub fn gedcom_age(s: &str) -> Option<String> {
    let k = sort_key(s.trim());
    let digits: String = k.chars().take_while(|c| c.is_ascii_digit()).collect();
    let n: u32 = digits.parse().ok()?;
    let unit = k[digits.len()..].trim();
    Some(match unit.chars().next() {
        Some('m') => format!("{n}m"),
        Some('j') | Some('d') => format!("{n}d"),
        Some('s') | Some('w') => format!("{n}w"),
        _ => format!("{n}y"),
    })
}

fn age_text(s: &str) -> String {
    let s = s.trim();
    if s.chars().all(|c| c.is_ascii_digit()) { format!("{s} ans") } else { s.to_string() }
}

// ----------------------------------------------------------------------
// Écriture dans le document
// ----------------------------------------------------------------------

struct Writer<'a> {
    ged: &'a mut Gedcom,
    res: EntryResult,
}

impl Writer<'_> {
    fn touch(&mut self, x: &str) {
        if !self.res.touched.iter().any(|t| t == x) {
            self.res.touched.push(x.to_string());
            if !self.res.created.iter().any(|c| c == x)
                && let Some(n) = self.ged.get_mut(x)
            {
                n.touch();
            }
        }
    }

    fn mark_created(&mut self, x: &str) {
        self.res.created.push(x.to_string());
        if !self.res.touched.iter().any(|t| t == x) {
            self.res.touched.push(x.to_string());
        }
    }

    fn sex(&self, x: &str) -> String {
        self.ged.individual(x).map(|i| i.sex().to_string()).unwrap_or_default()
    }

    fn link(&mut self, indi: &str, tag: &str, fam: &str) {
        let ptr = format!("@{fam}@");
        let n = self.ged.get_mut(indi).unwrap();
        if !n.children.iter().any(|c| c.tag == tag && c.val() == ptr) {
            n.insert_child(Node::with_value(tag, ptr));
            self.touch(indi);
        }
    }

    /// Famille du couple, créée si besoin.
    fn ensure_couple(&mut self, a: &str, b: &str) -> String {
        let pa = format!("@{a}@");
        let pb = format!("@{b}@");
        let found = self.ged.records_of(RecordKind::Family).find(|f| {
            let spouses: Vec<&str> =
                f.children.iter().filter(|c| c.tag == "HUSB" || c.tag == "WIFE").map(|c| c.val()).collect();
            spouses.contains(&pa.as_str()) && spouses.contains(&pb.as_str())
        });
        if let Some(f) = found.and_then(|f| f.xref.clone()) {
            return f;
        }
        // Une famille où l'un est seul peut accueillir l'autre.
        let half = self.ged.records_of(RecordKind::Family).find_map(|f| {
            let h = f.child_val("HUSB");
            let w = f.child_val("WIFE");
            match (h, w) {
                (Some(h), None) if h == pa || h == pb => Some((f.xref.clone()?, if h == pa { b } else { a }, "WIFE")),
                (None, Some(w)) if w == pa || w == pb => Some((f.xref.clone()?, if w == pa { b } else { a }, "HUSB")),
                _ => None,
            }
        });
        if let Some((fam, other, slot)) = half
            && (self.sex(other) != if slot == "HUSB" { "F" } else { "M" })
        {
            self.ged.get_mut(&fam).unwrap().insert_child(Node::with_value(slot, format!("@{other}@")));
            self.touch(&fam);
            self.link(other, "FAMS", &fam);
            return fam;
        }
        let (h, w) = if self.sex(a) == "F" || self.sex(b) == "M" { (b, a) } else { (a, b) };
        let fam = self.ged.create(RecordKind::Family);
        let node = self.ged.get_mut(&fam).unwrap();
        node.insert_child(Node::with_value("HUSB", format!("@{h}@")));
        node.insert_child(Node::with_value("WIFE", format!("@{w}@")));
        self.mark_created(&fam);
        self.res.families_created.push(fam.clone());
        self.link(h, "FAMS", &fam);
        self.link(w, "FAMS", &fam);
        fam
    }

    /// Rattache l'enfant à la famille de ses parents (créée ou complétée si besoin).
    fn ensure_parents(&mut self, child: &str, father: Option<&str>, mother: Option<&str>) {
        if father.is_none() && mother.is_none() {
            return;
        }
        let pf = father.map(|f| format!("@{f}@"));
        let pm = mother.map(|m| format!("@{m}@"));
        // Famille d'origine existante compatible.
        let famc: Vec<String> =
            self.ged.individual(child).map(|i| i.famc().iter().map(|s| s.to_string()).collect()).unwrap_or_default();
        let compatible = famc.into_iter().find(|f| {
            let Some(fam) = self.ged.get(f) else { return false };
            let ok = |tag: &str, want: &Option<String>| match (fam.child_val(tag), want) {
                (_, None) | (None, _) => true,
                (Some(have), Some(w)) => have == w,
            };
            ok("HUSB", &pf) && ok("WIFE", &pm)
        });
        let fam = match compatible {
            Some(f) => {
                for (tag, p) in [("HUSB", father), ("WIFE", mother)] {
                    if let Some(p) = p
                        && self.ged.get(&f).unwrap().child(tag).is_none()
                    {
                        self.ged.get_mut(&f).unwrap().insert_child(Node::with_value(tag, format!("@{p}@")));
                        self.touch(&f);
                        self.link(p, "FAMS", &f);
                    }
                }
                f
            }
            None => match (father, mother) {
                (Some(f), Some(m)) => self.ensure_couple(f, m),
                (Some(p), None) | (None, Some(p)) if self.single_family(p).is_some() => {
                    // Un seul parent cité, qui n'a qu'une famille : l'enfant en fait partie.
                    self.single_family(p).unwrap()
                }
                (Some(p), None) | (None, Some(p)) => {
                    let tag = if father.is_some() { "HUSB" } else { "WIFE" };
                    let fam = self.ged.create(RecordKind::Family);
                    self.ged.get_mut(&fam).unwrap().insert_child(Node::with_value(tag, format!("@{p}@")));
                    self.mark_created(&fam);
                    self.res.families_created.push(fam.clone());
                    self.link(p, "FAMS", &fam);
                    fam
                }
                (None, None) => unreachable!(),
            },
        };
        let ptr = format!("@{child}@");
        let node = self.ged.get_mut(&fam).unwrap();
        if !node.children.iter().any(|c| c.tag == "CHIL" && c.val() == ptr) {
            // Les enfants vont après les époux et les enfants déjà présents.
            let at = node
                .children
                .iter()
                .rposition(|c| matches!(c.tag.as_str(), "HUSB" | "WIFE" | "CHIL"))
                .map(|i| i + 1)
                .unwrap_or(0);
            node.children.insert(at, Node::with_value("CHIL", ptr));
            self.touch(&fam);
        }
        self.link(child, "FAMC", &fam);
    }

    /// Unique famille où l'individu est conjoint.
    fn single_family(&self, x: &str) -> Option<String> {
        let fams = self.ged.individual(x)?.fams();
        (fams.len() == 1).then(|| fams[0].to_string())
    }

    /// Ajoute l'événement principal, ou le fusionne avec un événement unique existant
    /// (naissance, décès…) quand les dates sont compatibles. Renvoie son index.
    fn add_event(&mut self, x: &str, ev: Node) -> usize {
        let unique = matches!(ev.tag.as_str(), "BIRT" | "CHR" | "BAPM" | "DEAT" | "BURI" | "CREM");
        let node = self.ged.get_mut(x).unwrap();
        let existing = if unique { node.children.iter().position(|c| c.tag == ev.tag) } else { None };
        let idx = match existing {
            Some(i) if compatible_dates(node.children[i].child_val("DATE"), ev.child_val("DATE")) => {
                let target = &mut node.children[i];
                for c in ev.children {
                    match c.tag.as_str() {
                        "DATE" => {
                            let precise = target.child_val("DATE").is_none_or(|d| !starts_with_day_or_year(d));
                            if precise {
                                target.set_child_val("DATE", c.val());
                            }
                        }
                        "PLAC" => {
                            if target.child("PLAC").is_none() {
                                let at = target
                                    .children
                                    .iter()
                                    .position(|c| c.tag == "SOUR")
                                    .unwrap_or(target.children.len());
                                target.children.insert(at, c);
                            }
                        }
                        "AGE" | "TYPE" => {
                            if target.child(&c.tag).is_none() {
                                target.children.insert(0, c);
                            }
                        }
                        "SOUR" => merge_citation(&mut target.children, c),
                        _ => target.children.push(c),
                    }
                }
                i
            }
            _ => {
                node.insert_child(ev);
                node.children.iter().rposition(|c| c.tag != "CHAN" && c.tag != "CREA").unwrap()
            }
        };
        self.touch(x);
        idx
    }

    /// Ajoute un fait (profession, domicile), sauf s'il existe déjà avec la même valeur et la même date.
    fn add_fact(&mut self, x: &str, fact: Node) {
        let node = self.ged.get_mut(x).unwrap();
        let same = node.children.iter_mut().find(|c| {
            c.tag == fact.tag
                && c.val() == fact.val()
                && c.child_val("DATE") == fact.child_val("DATE")
                && c.child_val("PLAC") == fact.child_val("PLAC")
        });
        match same {
            Some(existing) => {
                let cite = fact.children.into_iter().find(|c| c.tag == "SOUR").unwrap();
                if !existing.children.contains(&cite) {
                    existing.children.push(cite);
                }
            }
            None => node.insert_child(fact),
        }
        self.touch(x);
    }

    fn has_citation(&self, x: &str, cite: &Node) -> bool {
        self.ged.get(x).is_some_and(|n| n.children.contains(cite))
    }

    fn has_birth(&self, x: &str) -> bool {
        self.ged.get(x).is_some_and(|n| n.children.iter().any(|c| matches!(c.tag.as_str(), "BIRT" | "CHR" | "BAPM")))
    }
}

/// Ajoute une citation, ou complète une citation existante de la même source
/// (sans référence, ou avec la même) plutôt que de la dupliquer.
fn merge_citation(children: &mut Vec<Node>, cite: Node) {
    let page = cite.child_val("PAGE").map(str::to_string);
    let same = children.iter_mut().find(|c| {
        c.tag == "SOUR"
            && c.value == cite.value
            && match (c.child_val("PAGE"), page.as_deref()) {
                (None, _) => c.child("EVEN").is_none() || c.child("EVEN") == cite.child("EVEN"),
                (Some(a), Some(b)) => a == b && (c.child("EVEN").is_none() || c.child("EVEN") == cite.child("EVEN")),
                (Some(_), None) => false,
            }
    });
    match same {
        Some(existing) => {
            for sub in cite.children {
                if !existing.children.iter().any(|e| {
                    e.tag == sub.tag && (sub.tag != "OBJE" && sub.tag != "NOTE" && sub.tag != "_GR_FLD" || *e == sub)
                }) {
                    // PAGE en tête, comme l'écrivent les autres logiciels.
                    if sub.tag == "PAGE" {
                        existing.children.insert(0, sub);
                    } else {
                        existing.children.push(sub);
                    }
                }
            }
        }
        None => children.push(cite),
    }
}

/// Une date existante (`ABT 1850`, `BEF 1900`…) est-elle compatible avec la nouvelle ?
fn compatible_dates(old: Option<&str>, new: Option<&str>) -> bool {
    match (old, new) {
        (None, _) | (_, None) => true,
        (Some(a), Some(b)) if a.trim() == b.trim() => true,
        (Some(a), Some(_)) => !starts_with_day_or_year(a),
    }
}

/// Date exacte (commence par un jour ou une année) plutôt qu'approximative (`ABT`, `EST`…).
fn starts_with_day_or_year(d: &str) -> bool {
    d.trim().chars().next().is_some_and(|c| c.is_ascii_digit())
}

// ----------------------------------------------------------------------
// Correspondances avec les individus existants
// ----------------------------------------------------------------------

impl Entry {
    /// Individus existants qui pourraient être la personne citée à l'index `idx`,
    /// du plus probable au moins probable.
    pub fn suggestions(&self, ged: &Gedcom, idx: usize) -> Vec<Suggestion> {
        let Some(p) = self.effective().get(idx).cloned() else { return vec![] };
        let Some(role) = self.role_of(&p).cloned() else { return vec![] };
        let surname = sort_key(p.get(PersonField::Surname));
        let given = sort_key(p.get(PersonField::Given));
        // Rien de saisi pour cette personne (le nom hérité ne suffit pas) : liens déduits seulement.
        if self.participants.get(idx).is_none_or(|p| p.is_blank()) || (surname.is_empty() && given.is_empty()) {
            return self.context_suggestions(ged, &role);
        }
        let first_given = given.split_whitespace().next().unwrap_or("").to_string();
        let year = self.year();
        let expected_birth = match (year, parse_age_years(p.get(PersonField::Age))) {
            (Some(y), Some(a)) => Some(y - a),
            (Some(y), None)
                if role.relation == Relation::Principal
                    && matches!(self.template.event.as_str(), "BIRT" | "BAPM" | "CHR") =>
            {
                Some(y)
            }
            _ => None,
        };
        let sex = role.sex.clone().unwrap_or_else(|| p.get(PersonField::Sex).to_uppercase());
        let context: Vec<(String, String)> =
            self.context_suggestions(ged, &role).into_iter().map(|s| (s.xref, s.reason)).collect();

        let mut out: Vec<Suggestion> = ged
            .records_of(RecordKind::Individual)
            .filter_map(|r| {
                let i = Individual::new(r);
                let s = sort_key(&i.surname());
                let g = sort_key(&i.given());
                let mut score = 0.0;
                let mut reasons: Vec<String> = Vec::new();
                if !surname.is_empty() {
                    if s == surname {
                        score += 3.0;
                    } else if !s.is_empty() && (s.contains(&surname) || surname.contains(&s)) {
                        score += 1.5;
                    } else {
                        return None;
                    }
                }
                if !first_given.is_empty() {
                    if g == given {
                        score += 3.0;
                    } else if g.split_whitespace().next() == Some(first_given.as_str()) {
                        score += 2.5;
                    } else if g.split_whitespace().any(|w| w == first_given) {
                        score += 1.5;
                    } else if g.starts_with(&first_given[..first_given.len().min(3)]) {
                        score += 0.5;
                    } else if !g.is_empty() {
                        return None;
                    }
                }
                if matches!(sex.as_str(), "M" | "F") && matches!(i.sex(), "M" | "F") && i.sex() != sex {
                    return None;
                }
                if let (Some(exp), Some(b)) = (expected_birth, i.birth_year()) {
                    let d = (exp - b).abs();
                    if d <= 2 {
                        score += 2.0;
                    } else if d <= 10 {
                        score += 0.5;
                    } else {
                        score -= 3.0;
                    }
                }
                // Mort avant l'acte (sauf pour l'acte de décès ou d'inhumation lui-même).
                if let (Some(y), Some(d)) = (year, i.death().and_then(|e| e.date()).and_then(|d| d.year()))
                    && d < y
                    && !(role.relation == Relation::Principal
                        && matches!(self.template.event.as_str(), "DEAT" | "BURI"))
                {
                    score -= 4.0;
                }
                if let Some(b) = i.birth_year()
                    && let Some(y) = year
                {
                    // Né après l'acte, ou trop jeune pour être parent / conjoint.
                    let min_age = match role.relation {
                        Relation::FatherOf(_) | Relation::MotherOf(_) => 14,
                        Relation::SpouseOf(_) => 12,
                        Relation::Principal if self.template.is_family_event() => 12,
                        _ => 0,
                    };
                    if b > y - min_age {
                        score -= 5.0;
                    }
                }
                if let Some((_, why)) = context.iter().find(|(x, _)| x == i.xref()) {
                    score += 5.0;
                    reasons.push(why.clone());
                }
                if let Some(b) = i.birth().and_then(|e| e.date()) {
                    reasons.push(format!("né(e) {}", b.to_french()));
                }
                (score >= 3.0).then(|| Suggestion {
                    xref: i.xref().to_string(),
                    label: format!(
                        "{}{}",
                        i.display_name(),
                        if i.years().is_empty() { String::new() } else { format!(" ({})", i.years()) }
                    ),
                    score,
                    reason: reasons.join(" · "),
                })
            })
            .collect();
        out.sort_by(|a, b| b.score.total_cmp(&a.score));
        out.truncate(5);
        out
    }

    /// Suggestions déduites des personnes déjà liées (père de l'enfant lié, conjoint…).
    fn context_suggestions(&self, ged: &Gedcom, role: &RoleDef) -> Vec<Suggestion> {
        let linked = |key: &str| {
            self.participants.iter().find(|p| p.role == key).and_then(|p| match &p.choice {
                PersonChoice::Link(x) => Some(x.clone()),
                _ => None,
            })
        };
        let mk = |x: &str, why: String| {
            ged.individual(x).map(|i| Suggestion {
                xref: x.to_string(),
                label: format!(
                    "{}{}",
                    i.display_name(),
                    if i.years().is_empty() { String::new() } else { format!(" ({})", i.years()) }
                ),
                score: 10.0,
                reason: why,
            })
        };
        let mut out = Vec::new();
        match &role.relation {
            Relation::FatherOf(k) | Relation::MotherOf(k) => {
                if let Some(c) = linked(k) {
                    let (f, m) = ged.parents(&c);
                    let p = if matches!(role.relation, Relation::FatherOf(_)) { f } else { m };
                    let child = ged.record_label(&c);
                    out.extend(p.and_then(|p| mk(p.xref(), format!("parent de {child}"))));
                }
                // L'autre parent est lié : on propose son conjoint.
                let other = self.template.roles.iter().find(|r| {
                    r.key != role.key && matches!(&r.relation, Relation::FatherOf(x) | Relation::MotherOf(x) if x == k)
                });
                if let Some(o) = other.and_then(|o| linked(&o.key)) {
                    let who = ged.record_label(&o);
                    for (s, _) in ged.spouses_and_children(&o) {
                        out.extend(s.and_then(|s| mk(s.xref(), format!("conjoint de {who}"))));
                    }
                }
            }
            Relation::SpouseOf(k) => {
                if let Some(c) = linked(k) {
                    let who = ged.record_label(&c);
                    for (s, _) in ged.spouses_and_children(&c) {
                        out.extend(s.and_then(|s| mk(s.xref(), format!("conjoint de {who}"))));
                    }
                }
            }
            Relation::Principal => {
                // Enfant d'un père ou d'une mère déjà lié(e) ; conjoint de l'autre époux.
                for r in &self.template.roles {
                    match &r.relation {
                        Relation::FatherOf(k) | Relation::MotherOf(k) if *k == role.key => {
                            if let Some(par) = linked(&r.key) {
                                let who = ged.record_label(&par);
                                for (_, kids) in ged.spouses_and_children(&par) {
                                    for kid in kids {
                                        out.extend(mk(kid.xref(), format!("enfant de {who}")));
                                    }
                                }
                            }
                        }
                        Relation::Principal if r.key != role.key && self.template.is_family_event() => {
                            if let Some(other) = linked(&r.key) {
                                let who = ged.record_label(&other);
                                for (s, _) in ged.spouses_and_children(&other) {
                                    out.extend(s.and_then(|s| mk(s.xref(), format!("conjoint de {who}"))));
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
            _ => {}
        }
        // Un même individu peut être proposé pour plusieurs raisons : on les regroupe.
        let mut merged: Vec<Suggestion> = Vec::new();
        for s in out {
            match merged.iter_mut().find(|m| m.xref == s.xref) {
                Some(m) => {
                    m.reason = format!("{} · {}", m.reason, s.reason);
                    m.score += 1.0;
                }
                None => merged.push(s),
            }
        }
        merged.sort_by(|a, b| b.score.total_cmp(&a.score));
        merged
    }
}
