//! Actes d'une source : un éditeur générique, sans modèle.
//!
//! Un acte (naissance, mariage, recensement, n'importe quel événement GEDCOM)
//! se saisit dans la fiche de sa source : on choisit l'événement, puis on
//! ajoute les personnes citées avec leur rôle GEDCOM 7 standard (`CHIL`,
//! `FATH`, `MOTH`, `HUSB`, `WIFE`, `SPOU`, `WITN`, `GODP`, `OFFICIATOR`…). Les
//! champs proposés pour chaque personne et les liens familiaux créés découlent
//! du rôle. Chaque personne est **liée** à un individu existant, **créée**, ou
//! simplement **mentionnée** (dans la note de la citation).
//!
//! [`Act::save`] écrit tout de suite dans l'arbre, en GEDCOM 7 standard :
//! événement sur la personne principale (ou sur la famille des époux), citation
//! avec `PAGE`, `DATA`, `EVEN` / `ROLE` sur chaque personne, `ASSO` pour les
//! témoins. Aucune extension n'est écrite.
//!
//! La liste des actes d'une source ([`Gedcom::acts`]) est reconstituée à partir
//! des citations qui la désignent, regroupées par référence (`PAGE`) et date de
//! l'acte (`DATA.DATE`). Rouvrir un acte le modifie : l'ancienne version est
//! retirée ([`Gedcom::remove_act`]) puis la nouvelle écrite.

use crate::document::{Gedcom, RecordKind};
use crate::model::{Individual, sort_key};
use crate::node::Node;

// ----------------------------------------------------------------------
// Événements et rôles
// ----------------------------------------------------------------------

/// Événements familiaux (portés par l'enregistrement `FAM` des époux).
pub const FAMILY_ACT_EVENTS: &[&str] = &["MARR", "MARB", "MARC", "MARL", "MARS", "ENGA", "DIV", "DIVF", "ANUL"];

fn is_birth_like(tag: &str) -> bool {
    matches!(tag, "BIRT" | "CHR" | "BAPM")
}

fn is_death_like(tag: &str) -> bool {
    matches!(tag, "DEAT" | "BURI" | "CREM")
}

/// Données qu'on peut saisir pour une personne citée.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PersonField {
    Surname,
    Given,
    Sex,
    Age,
    Occupation,
    Residence,
    /// Lieu de naissance déclaré (recensements, mariages, décès).
    Birthplace,
    /// Lien déclaré (« fils », « épouse », « voisin »…).
    Kinship,
    /// Précisions d'une personne simplement mentionnée.
    Note,
}

impl PersonField {
    pub fn label(self) -> &'static str {
        match self {
            PersonField::Surname => "Nom",
            PersonField::Given => "Prénoms",
            PersonField::Sex => "Sexe",
            PersonField::Age => "Âge déclaré",
            PersonField::Occupation => "Profession",
            PersonField::Residence => "Domicile",
            PersonField::Birthplace => "Né(e) à",
            PersonField::Kinship => "Lien déclaré",
            PersonField::Note => "Précisions",
        }
    }
}

/// Rôle d'une personne dans l'acte. Chaque rôle correspond à un rôle GEDCOM 7.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ActRole {
    /// Porte l'événement : l'enfant, le défunt, les époux, chaque recensé…
    Principal,
    Father,
    Mother,
    Spouse,
    Witness,
    Godfather,
    Godmother,
    Declarant,
    Officiator,
    Clergy,
    Neighbor,
    Friend,
    Other,
}

use PersonField::*;

impl ActRole {
    /// Rôles proposés, dans l'ordre d'affichage.
    pub const ALL: [ActRole; 13] = [
        ActRole::Principal,
        ActRole::Father,
        ActRole::Mother,
        ActRole::Spouse,
        ActRole::Godfather,
        ActRole::Godmother,
        ActRole::Witness,
        ActRole::Declarant,
        ActRole::Officiator,
        ActRole::Clergy,
        ActRole::Neighbor,
        ActRole::Friend,
        ActRole::Other,
    ];

    /// Lié à une personne principale (père, mère ou conjoint de…).
    pub fn relative(self) -> bool {
        matches!(self, ActRole::Father | ActRole::Mother | ActRole::Spouse)
    }

    /// Participant associé à l'événement (`ASSO`) plutôt que lié par la famille.
    pub fn associate(self) -> bool {
        !matches!(self, ActRole::Principal | ActRole::Father | ActRole::Mother | ActRole::Spouse)
    }

    /// Libellé du rôle. `nth` : rang parmi les personnes principales.
    pub fn label(self, event: &str, family: bool, nth: usize) -> &'static str {
        match self {
            ActRole::Principal if family => {
                if nth == 0 {
                    "Époux"
                } else {
                    "Épouse"
                }
            }
            ActRole::Principal if is_birth_like(event) => "Enfant",
            ActRole::Principal if is_death_like(event) => "Défunt(e)",
            ActRole::Principal if event == "CENS" => {
                if nth == 0 {
                    "Chef de ménage"
                } else {
                    "Membre du ménage"
                }
            }
            ActRole::Principal => "Personne",
            ActRole::Father => "Père",
            ActRole::Mother => "Mère",
            ActRole::Spouse => "Conjoint",
            ActRole::Witness => "Témoin",
            ActRole::Godfather => "Parrain",
            ActRole::Godmother => "Marraine",
            ActRole::Declarant => "Déclarant",
            ActRole::Officiator => "Officiant",
            ActRole::Clergy => "Ecclésiastique",
            ActRole::Neighbor => "Voisin",
            ActRole::Friend => "Ami",
            ActRole::Other => "Autre",
        }
    }

    /// Rôle GEDCOM 7 écrit dans les citations (`EVEN` / `ROLE`).
    fn gedcom(self, event: &str, family: bool, nth: usize) -> Option<&'static str> {
        Some(match self {
            ActRole::Principal if family => {
                if nth == 0 {
                    "HUSB"
                } else {
                    "WIFE"
                }
            }
            ActRole::Principal if is_birth_like(event) => "CHIL",
            ActRole::Principal => return None,
            ActRole::Father => "FATH",
            ActRole::Mother => "MOTH",
            ActRole::Spouse => "SPOU",
            ActRole::Witness => "WITN",
            ActRole::Godfather | ActRole::Godmother => "GODP",
            ActRole::Declarant | ActRole::Other => "OTHER",
            ActRole::Officiator => "OFFICIATOR",
            ActRole::Clergy => "CLERGY",
            ActRole::Neighbor => "NGHBR",
            ActRole::Friend => "FRIEND",
        })
    }

    /// Précision fixe du rôle (`ROLE` / `PHRASE`).
    fn base_phrase(self) -> Option<&'static str> {
        match self {
            ActRole::Godfather => Some("Parrain"),
            ActRole::Godmother => Some("Marraine"),
            ActRole::Declarant => Some("Déclarant"),
            _ => None,
        }
    }

    /// Sexe imposé par le rôle.
    pub fn sex(self, family: bool, nth: usize) -> Option<&'static str> {
        match self {
            ActRole::Father | ActRole::Godfather => Some("M"),
            ActRole::Mother | ActRole::Godmother => Some("F"),
            ActRole::Principal if family => Some(if nth == 0 { "M" } else { "F" }),
            _ => None,
        }
    }

    /// Champs saisis pour ce rôle.
    pub fn fields(self, event: &str, family: bool, nth: usize) -> &'static [PersonField] {
        match self {
            ActRole::Principal if family => &[Surname, Given, Age, Occupation, Residence, Birthplace],
            ActRole::Principal if is_birth_like(event) => &[Surname, Given, Sex],
            ActRole::Principal if nth == 0 => &[Surname, Given, Sex, Age, Occupation, Residence, Birthplace],
            ActRole::Principal => &[Surname, Given, Sex, Age, Kinship, Occupation, Birthplace],
            ActRole::Father | ActRole::Mother | ActRole::Spouse => &[Surname, Given, Age, Occupation, Residence],
            ActRole::Officiator | ActRole::Clergy => &[Surname, Given, Occupation],
            ActRole::Godfather | ActRole::Godmother => &[Surname, Given, Age, Occupation, Residence, Kinship],
            _ => &[Surname, Given, Sex, Age, Occupation, Residence, Kinship],
        }
    }

    /// Rôle d'après la citation (`ROLE`, sa `PHRASE`) et le sexe de la personne.
    fn from_gedcom(role: &str, phrase: &str, sex: &str) -> ActRole {
        let p = sort_key(phrase);
        match role {
            "FATH" => ActRole::Father,
            "MOTH" => ActRole::Mother,
            "SPOU" => ActRole::Spouse,
            "WITN" => ActRole::Witness,
            "GODP" if p.starts_with("marraine") || (!p.starts_with("parrain") && sex == "F") => ActRole::Godmother,
            "GODP" => ActRole::Godfather,
            "OFFICIATOR" => ActRole::Officiator,
            "CLERGY" => ActRole::Clergy,
            "NGHBR" => ActRole::Neighbor,
            "FRIEND" => ActRole::Friend,
            "OTHER" if p.starts_with("declarant") => ActRole::Declarant,
            _ => ActRole::Other,
        }
    }
}

// ----------------------------------------------------------------------
// Acte en cours de saisie
// ----------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum MediaChoice {
    #[default]
    None,
    /// Média existant (identifiant `OBJE`).
    Existing(String),
    /// Nouveaux fichiers, une page chacun, chemins relatifs au dossier du `.ged`
    /// de préférence.
    NewFiles(Vec<String>),
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
    pub role: ActRole,
    /// Pour un père, une mère ou un conjoint : rang de la personne principale concernée.
    pub of: usize,
    pub values: Vec<(PersonField, String)>,
    pub choice: PersonChoice,
}

impl Participant {
    pub fn new(role: ActRole) -> Self {
        Self { role, of: 0, values: Vec::new(), choice: PersonChoice::Create }
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

    pub fn of(mut self, nth: usize) -> Self {
        self.of = nth;
        self
    }

    pub fn is_blank(&self) -> bool {
        self.values.iter().all(|(_, v)| v.trim().is_empty())
    }

    /// « Prénoms NOM ».
    pub fn display_name(&self) -> String {
        let (g, s) = (self.get(Given), self.get(Surname).to_uppercase());
        format!("{g} {s}").trim().to_string()
    }
}

/// Identifie un acte parmi les citations d'une source : référence (`PAGE`) et
/// date de l'acte (`DATA.DATE`).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub struct ActKey {
    pub page: String,
    pub date: String,
}

/// Un acte d'une source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Act {
    pub source: String,
    /// Balise de l'événement (`BIRT`, `MARR`, `CENS`, `EVEN`…).
    pub event: String,
    /// `TYPE` de l'événement (types personnalisés `EVEN`).
    pub event_type: String,
    /// Pour `EVEN` : événement familial (sur la famille des époux).
    pub family_even: bool,
    /// Date de l'événement (format GEDCOM : `14 MAR 1912`, `ABT 1850`…).
    pub date: String,
    pub date_phrase: String,
    pub place: String,
    /// Référence de l'acte dans la source (`PAGE`) : « Acte n° 27, vue 12 ».
    pub page: String,
    /// Date de rédaction de l'acte si elle diffère (`DATA.DATE` de la citation).
    pub record_date: String,
    pub record_date_phrase: String,
    /// Transcription (`DATA.TEXT`).
    pub transcription: String,
    /// Qualité de la preuve (`QUAY` 0 à 3).
    pub quality: Option<u8>,
    pub media: MediaChoice,
    pub participants: Vec<Participant>,
    /// Créer une naissance estimée (`EST`) à partir de l'âge déclaré quand il n'y en a pas.
    pub estimate_births: bool,
    /// Note de la citation, hors personnes mentionnées (gardée telle quelle).
    pub note: String,
    /// Acte lu dans le fichier : il sera remplacé à l'enregistrement.
    pub original: Option<ActKey>,
}

/// Ce que l'enregistrement d'un acte a produit.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ActResult {
    pub source: String,
    pub media: Option<String>,
    /// Identifiant de chaque participant (`None` pour une simple mention), dans l'ordre de saisie.
    pub persons: Vec<Option<String>>,
    pub created: Vec<String>,
    pub families_created: Vec<String>,
    /// Tous les enregistrements créés ou modifiés.
    pub touched: Vec<String>,
}

impl ActResult {
    pub fn summary(&self) -> String {
        let linked = self.persons.iter().flatten().count();
        let mut s = format!("Acte enregistré : {linked} personne{}", plural(linked));
        let new = self.persons.iter().flatten().filter(|x| self.created.contains(x)).count();
        if new > 0 {
            s += &format!(" dont {new} créée{}", plural(new));
        }
        let fams = self.families_created.len();
        if fams > 0 {
            s += &format!(", {fams} famille{} créée{}", plural(fams), plural(fams));
        }
        s
    }
}

fn plural(n: usize) -> &'static str {
    if n > 1 { "s" } else { "" }
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

/// Titre d'une source tout juste créée (voir [`Gedcom::create`]).
const NEW_SOURCE_TITLE: &str = "Nouvelle source";

impl Act {
    /// Nouvel acte vide, avec les personnes attendues pour cet événement.
    pub fn new(source: &str, event: &str) -> Self {
        let mut a = Self {
            source: source.to_string(),
            event: event.to_string(),
            event_type: String::new(),
            family_even: false,
            date: String::new(),
            date_phrase: String::new(),
            place: String::new(),
            page: String::new(),
            record_date: String::new(),
            record_date_phrase: String::new(),
            transcription: String::new(),
            quality: None,
            media: MediaChoice::None,
            participants: Vec::new(),
            estimate_births: true,
            note: String::new(),
            original: None,
        };
        a.participants = a.default_participants();
        a
    }

    /// Personnes proposées dans un acte vide de cet événement.
    pub fn default_participants(&self) -> Vec<Participant> {
        use ActRole::*;
        let p = Participant::new;
        if self.is_family() {
            vec![p(Principal), p(Principal), p(Father), p(Mother), p(Father).of(1), p(Mother).of(1)]
        } else if is_birth_like(&self.event) {
            let mut v = vec![p(Principal), p(Father), p(Mother)];
            if self.event != "BIRT" {
                v.extend([p(Godfather), p(Godmother)]);
            }
            v
        } else {
            vec![p(Principal)]
        }
    }

    /// L'événement est-il familial (porté par l'enregistrement `FAM` des époux) ?
    pub fn is_family(&self) -> bool {
        FAMILY_ACT_EVENTS.contains(&self.event.as_str()) || (self.event == "EVEN" && self.family_even)
    }

    /// Libellé de l'événement.
    pub fn event_label(&self) -> String {
        if !self.event_type.trim().is_empty() {
            return crate::model::event_type_label(&self.event_type);
        }
        crate::model::event_label(&self.event).map(str::to_string).unwrap_or_else(|| self.event.clone())
    }

    /// Rang de la personne `idx` parmi les personnes principales.
    pub fn nth_principal(&self, idx: usize) -> usize {
        self.participants[..idx.min(self.participants.len())].iter().filter(|p| p.role == ActRole::Principal).count()
    }

    /// Nombre de personnes principales.
    pub fn principal_count(&self) -> usize {
        self.participants.iter().filter(|p| p.role == ActRole::Principal).count()
    }

    /// Libellé du rôle de la personne `idx`.
    pub fn role_label(&self, idx: usize) -> &'static str {
        let p = &self.participants[idx];
        p.role.label(&self.event, self.is_family(), self.nth_principal(idx))
    }

    /// Champs à saisir pour la personne `idx`.
    pub fn fields_of(&self, idx: usize) -> Vec<PersonField> {
        let p = &self.participants[idx];
        let mut f = p.role.fields(&self.event, self.is_family(), self.nth_principal(idx)).to_vec();
        if p.choice == PersonChoice::Mention || !p.get(Note).is_empty() {
            f.push(Note);
        }
        f
    }

    /// Sexe imposé pour la personne `idx`.
    pub fn imposed_sex(&self, idx: usize) -> Option<&'static str> {
        self.participants[idx].role.sex(self.is_family(), self.nth_principal(idx))
    }

    /// Libellé court d'une personne principale (« l'époux », « Paul MARCHAND »).
    pub fn principal_name(&self, nth: usize) -> String {
        if self.is_family() {
            return if nth == 0 { "l'époux".into() } else { "l'épouse".into() };
        }
        self.participants
            .iter()
            .filter(|p| p.role == ActRole::Principal)
            .nth(nth)
            .map(|p| p.display_name())
            .filter(|n| !n.is_empty())
            .unwrap_or_else(|| format!("la personne {}", nth + 1))
    }

    /// Année de l'événement, si la date en contient une.
    pub fn year(&self) -> Option<i32> {
        crate::Date::parse(&self.date).year()
    }

    /// Prépare l'acte suivant du même registre : même événement, lieu et image ;
    /// le premier nombre de la référence est incrémenté (« Acte n° 27 » → « Acte n° 28 »).
    pub fn next(&self) -> Self {
        let mut e = Act::new(&self.source, &self.event);
        e.event_type = self.event_type.clone();
        e.family_even = self.family_even;
        e.participants = e.default_participants();
        e.place = self.place.clone();
        e.media = self.media.clone();
        e.quality = self.quality;
        e.estimate_births = self.estimate_births;
        e.page = increment_first_number(&self.page);
        e
    }

    /// Titre d'une source propre à cet acte : « Acte de naissance de Rémi GUYOT ».
    pub fn auto_title(&self) -> String {
        let names: Vec<String> = self
            .participants
            .iter()
            .filter(|p| p.role == ActRole::Principal && !p.is_blank())
            .map(|p| p.display_name())
            .collect();
        let what = self.event_label().to_lowercase();
        let article = if what.starts_with(|c: char| "aeéèêiouyh".contains(c)) { "d'" } else { "de " };
        match names.len() {
            0 => format!("Acte {article}{what}"),
            _ => format!("Acte {article}{what} de {}", names.join(" et ")),
        }
    }

    /// Libellé de l'acte dans la liste de sa source : « Naissance · Louise MARCHAND ».
    pub fn title(&self) -> String {
        let names: Vec<String> = self
            .participants
            .iter()
            .filter(|p| p.role == ActRole::Principal && !p.is_blank())
            .map(|p| p.display_name())
            .collect();
        let what = if self.event.is_empty() { "Acte".to_string() } else { self.event_label() };
        if names.is_empty() { what } else { format!("{what} · {}", names.join(" et ")) }
    }

    /// Référence et date, pour la liste des actes : « Acte n° 27 · 3 février 1914 ».
    pub fn subtitle(&self) -> String {
        let date = if self.date.trim().is_empty() { &self.record_date } else { &self.date };
        let date = if date.trim().is_empty() { String::new() } else { crate::Date::parse(date).to_french() };
        [self.page.trim().to_string(), date].into_iter().filter(|s| !s.is_empty()).collect::<Vec<_>>().join(" · ")
    }

    /// Résumé lisible de l'acte, pour amorcer la transcription.
    pub fn auto_text(&self) -> String {
        let mut lines = vec![format!(
            "{}{}{}",
            self.event_label(),
            if self.date.is_empty() {
                String::new()
            } else {
                format!(" le {}", crate::Date::parse(&self.date).to_french())
            },
            if self.place.is_empty() { String::new() } else { format!(" à {}", crate::short_place(&self.place)) }
        )];
        for (i, p) in self.participants.iter().enumerate() {
            if p.is_blank() {
                continue;
            }
            let mut s = format!("{} : {}", self.role_label(i), p.display_name());
            for f in [Age, Occupation, Residence, Birthplace, Kinship] {
                let v = p.get(f);
                if !v.is_empty() {
                    s += &match f {
                        Age => format!(", {}", age_text(v)),
                        Residence => format!(", domicilié(e) à {}", crate::short_place(v)),
                        Birthplace => format!(", né(e) à {}", crate::short_place(v)),
                        _ => format!(", {v}"),
                    };
                }
            }
            lines.push(s);
        }
        lines.join("\n")
    }

    /// Vérifie l'acte avant écriture.
    pub fn validate(&self, ged: &Gedcom) -> Result<(), String> {
        if ged.get(&self.source).is_none_or(|n| n.tag != "SOUR") {
            return Err("la source de l'acte n'existe plus".into());
        }
        if self.event.trim().is_empty() {
            return Err("choisissez l'événement de l'acte".into());
        }
        if self.event == "EVEN" && self.event_type.trim().is_empty() {
            return Err("précisez le type d'événement".into());
        }
        if let MediaChoice::Existing(x) = &self.media
            && ged.get(x).is_none_or(|n| n.tag != "OBJE")
        {
            return Err("le média choisi n'existe plus".into());
        }
        for (label, d) in [("date", &self.date), ("date de l'acte", &self.record_date)] {
            if !d.trim().is_empty() {
                let parsed = crate::Date::parse(d);
                if parsed.year().is_none() {
                    return Err(format!("{label} non reconnue : « {} »", d.trim()));
                }
                parsed.validate().map_err(|e| format!("{label} impossible : {e}"))?;
            }
        }
        let principals = self.principal_count();
        let mut kept = 0;
        for (i, p) in self.participants.iter().enumerate() {
            let label = self.role_label(i);
            if p.role.relative() && p.of >= principals.max(1) {
                return Err(format!("{label} : personne principale introuvable"));
            }
            let counts = match &p.choice {
                PersonChoice::Link(x) => {
                    if ged.individual(x).is_none() {
                        return Err(format!("{label} : individu introuvable"));
                    }
                    true
                }
                // Ligne vide : ignorée.
                PersonChoice::Create if p.is_blank() => false,
                PersonChoice::Create => {
                    if p.get(Surname).is_empty() && p.get(Given).is_empty() {
                        return Err(format!("{label} : indiquez au moins un nom ou un prénom"));
                    }
                    true
                }
                PersonChoice::Mention if p.role == ActRole::Principal => {
                    return Err(format!("{label} : la personne principale doit être liée ou créée"));
                }
                PersonChoice::Mention => false,
            };
            if counts && p.role == ActRole::Principal {
                kept += 1;
            }
        }
        if kept == 0 {
            return Err("renseignez au moins la personne principale de l'acte".into());
        }
        if self.is_family() && kept < 2 {
            return Err("un événement familial demande les deux époux".into());
        }
        Ok(())
    }

    /// Valeurs effectives : un enfant sans nom prend celui de son père, un père sans
    /// nom celui de l'enfant.
    fn effective(&self) -> Vec<Participant> {
        let mut ps = self.participants.clone();
        let nth: Vec<usize> = (0..ps.len()).map(|i| self.nth_principal(i)).collect();
        let surname_of = |ps: &[Participant], pred: &dyn Fn(usize, &Participant) -> bool| {
            ps.iter()
                .enumerate()
                .find(|(i, p)| pred(*i, p) && !p.get(Surname).is_empty())
                .map(|(_, p)| p.get(Surname).to_string())
                .unwrap_or_default()
        };
        for i in 0..ps.len() {
            if !ps[i].get(Surname).is_empty() || ps[i].choice != PersonChoice::Create {
                continue;
            }
            let inherited = match ps[i].role {
                ActRole::Father => {
                    let of = ps[i].of;
                    surname_of(&ps, &|j, p| p.role == ActRole::Principal && nth[j] == of)
                }
                ActRole::Principal if !self.is_family() => {
                    let me = nth[i];
                    surname_of(&ps, &|_, p| p.role == ActRole::Father && p.of == me)
                }
                _ => String::new(),
            };
            if !inherited.is_empty() {
                ps[i].set(Surname, inherited);
            }
        }
        ps
    }

    /// GEDCOM qui serait écrit, sans toucher au document : texte des enregistrements
    /// créés ou modifiés.
    pub fn preview(&self, ged: &Gedcom) -> Result<String, String> {
        let mut copy = ged.clone();
        let res = self.save(&mut copy)?;
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

    /// Écrit l'acte dans le document. Un acte rouvert remplace son ancienne version.
    /// Rien n'est modifié en cas d'erreur.
    pub fn save(&self, ged: &mut Gedcom) -> Result<ActResult, String> {
        self.validate(ged)?;
        let mut copy = ged.clone();
        let mut removed = Vec::new();
        if let Some(k) = &self.original {
            removed = copy.unlink_act(&self.source, k, true);
        }
        let mut res = self.apply(&mut copy)?;
        copy.drop_empty_events(&removed);
        for x in removed {
            if !res.touched.contains(&x) {
                res.touched.push(x);
            }
        }
        *ged = copy;
        Ok(res)
    }

    fn apply(&self, ged: &mut Gedcom) -> Result<ActResult, String> {
        self.validate(ged)?;
        let mut w = Writer { ged, res: ActResult::default() };
        let source = self.source.clone();
        let family = self.is_family();
        w.res.source = source.clone();

        // 1. Source : titre automatique d'une source toute neuve, contenu (`DATA` / `EVEN` + période).
        let src = w.ged.get_mut(&source).unwrap();
        if src.child_val("TITL").is_none_or(|t| t.trim().is_empty() || t.trim() == NEW_SOURCE_TITLE) {
            let title = self.auto_title();
            src.set_child_text("TITL", &title);
            w.touch(&source);
        }
        let when = self.citation_date().map(|d| d.val().to_string()).unwrap_or_default();
        if crate::kinds::record_in_source(w.ged.get_mut(&source).unwrap(), &self.event, &when) {
            w.touch(&source);
        }

        // 2. Image de l'acte.
        let media = match &self.media {
            MediaChoice::None => None,
            MediaChoice::Existing(x) => Some(x.clone()),
            MediaChoice::NewFiles(paths) if paths.iter().any(|p| !p.trim().is_empty()) => {
                let x = w.ged.create(RecordKind::Media);
                let title = [w.ged.record_label(&source), self.page.trim().to_string()]
                    .into_iter()
                    .filter(|s| !s.is_empty())
                    .collect::<Vec<_>>()
                    .join(", ");
                let node = w.ged.get_mut(&x).unwrap();
                node.children.retain(|c| c.tag != "FILE");
                // Une page par fichier, dans l'ordre ; le titre sur la première.
                for (i, path) in paths.iter().map(|p| p.trim()).filter(|p| !p.is_empty()).enumerate() {
                    let mut file = Node::with_value("FILE", path).push(Node::with_value("FORM", crate::mime_for(path)));
                    if i == 0 {
                        file.set_child_val("TITL", &title);
                    }
                    match node.children.iter().rposition(|c| c.tag == "FILE") {
                        Some(at) => node.children.insert(at + 1, file),
                        None => node.insert_child(file),
                    }
                }
                // Un acte est un document manuscrit (`FORM` / `MEDI MANUSCRIPT`).
                crate::kinds::set_medium(node, "MANUSCRIPT");
                w.mark_created(&x);
                Some(x)
            }
            MediaChoice::NewFiles(_) => None,
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
        let nth: Vec<usize> = (0..people.len()).map(|i| self.nth_principal(i)).collect();
        let mut ids: Vec<Option<String>> = Vec::new();
        for (i, p) in people.iter().enumerate() {
            let sex = p.role.sex(family, nth[i]).map(str::to_string).unwrap_or_else(|| p.get(Sex).to_uppercase());
            let id = match &p.choice {
                PersonChoice::Mention => None,
                PersonChoice::Create if p.is_blank() => None,
                PersonChoice::Link(x) => {
                    // Complète le sexe s'il était inconnu.
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
                    let (given, surname) = (p.get(Given), p.get(Surname));
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
        let principal_id = |k: usize| -> Option<String> {
            people
                .iter()
                .zip(&ids)
                .enumerate()
                .find(|(i, (p, id))| p.role == ActRole::Principal && nth[*i] == k && id.is_some())
                .and_then(|(_, (_, id))| id.clone())
        };

        // 4. Liens familiaux déduits des rôles.
        let mut parents: Vec<(String, Option<String>, Option<String>)> = Vec::new();
        for (p, id) in people.iter().zip(&ids) {
            let Some(id) = id else { continue };
            match p.role {
                ActRole::Father | ActRole::Mother => {
                    let Some(child) = principal_id(p.of) else { continue };
                    let slot = match parents.iter_mut().position(|(c, _, _)| *c == child) {
                        Some(i) => &mut parents[i],
                        None => {
                            parents.push((child.clone(), None, None));
                            parents.last_mut().unwrap()
                        }
                    };
                    if p.role == ActRole::Father {
                        slot.1 = Some(id.clone());
                    } else {
                        slot.2 = Some(id.clone());
                    }
                }
                ActRole::Spouse => {
                    if let Some(other) = principal_id(p.of) {
                        w.ensure_couple(&other, id);
                    }
                }
                _ => {}
            }
        }
        for (child, f, m) in parents {
            w.ensure_parents(&child, f.as_deref(), m.as_deref());
        }
        if !family {
            self.link_household(&mut w, &people, &ids, &nth);
        }

        // 5. Événement et citations.
        let year = self.year();
        let fam = if family {
            let spouses: Vec<String> = people
                .iter()
                .zip(&ids)
                .filter(|(p, id)| p.role == ActRole::Principal && id.is_some())
                .filter_map(|(_, id)| id.clone())
                .collect();
            Some(w.ensure_couple(&spouses[0], &spouses[1]))
        } else {
            None
        };
        let note = self.citation_note(&people, &nth);

        // Événement principal : sur la famille, ou sur chaque personne principale.
        // On retient où il se trouve pour y attacher les témoins (ASSO).
        let mut main_event: Option<(String, usize)> = None;
        if let Some(fam) = &fam {
            let mut ev = self.event_node(None);
            let mut cite = self.citation(media.as_deref(), None, true);
            if !note.is_empty() {
                cite.children.push(Node::with_value("NOTE", &note));
            }
            ev.children.push(cite);
            for (i, (p, id)) in people.iter().zip(&ids).enumerate() {
                if id.is_none() || p.role != ActRole::Principal {
                    continue;
                }
                if let Some(age) = gedcom_age(p.get(Age)) {
                    let tag = if nth[i] == 0 { "HUSB" } else { "WIFE" };
                    let at = ev.children.iter().position(|c| c.tag == "SOUR").unwrap_or(ev.children.len());
                    ev.children.insert(at, Node::new(tag).push(Node::with_value("AGE", age)));
                }
            }
            let i = w.add_event(fam, ev);
            main_event = Some((fam.clone(), i));
        }

        let mut first_principal = true;
        for (i, (p, id)) in people.iter().zip(&ids).enumerate() {
            let Some(x) = id else { continue };
            let age = p.get(Age);
            let role = self.citation_role(p, nth[i], &people, &nth);
            if p.role == ActRole::Principal && fam.is_none() {
                let mut ev = self.event_node(gedcom_age(age));
                let mut cite = self.citation(media.as_deref(), role.clone(), first_principal);
                if first_principal && !note.is_empty() {
                    cite.children.push(Node::with_value("NOTE", &note));
                }
                ev.children.push(cite);
                let at = w.add_event(x, ev);
                if first_principal {
                    main_event = Some((x.clone(), at));
                }
                first_principal = false;
            } else {
                // Citation au niveau de l'individu, avec son rôle dans l'événement.
                let cite = self.citation(media.as_deref(), role.clone(), false);
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
            let occu = p.get(Occupation);
            if !occu.is_empty() {
                let mut n = Node::with_value("OCCU", occu);
                if let Some(d) = self.date_node() {
                    n.children.push(d);
                }
                n.children.push(self.citation(media.as_deref(), role.clone(), false));
                w.add_fact(x, n);
            }
            let resi = p.get(Residence);
            if !resi.is_empty() {
                let mut n = Node::new("RESI");
                if let Some(d) = self.date_node() {
                    n.children.push(d);
                }
                n.children.push(Node::with_value("PLAC", resi));
                n.children.push(self.citation(media.as_deref(), role.clone(), false));
                w.add_fact(x, n);
            }
            let is_birth_event = is_birth_like(&self.event) && p.role == ActRole::Principal;
            if !is_birth_event {
                let birthplace = p.get(Birthplace);
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
                    n.children.push(self.citation(media.as_deref(), role.clone(), false));
                    w.ged.get_mut(x).unwrap().insert_child(n);
                    w.touch(x);
                }
            }
        }

        // 6. Témoins, parrains, déclarants… associés à l'événement principal.
        if let Some((holder, idx)) = main_event {
            for (i, (p, id)) in people.iter().zip(&ids).enumerate() {
                let Some(x) = id else { continue };
                if !p.role.associate() {
                    continue;
                }
                let Some((role, phrase)) = self.citation_role(p, nth[i], &people, &nth) else { continue };
                let mut r = Node::with_value("ROLE", role);
                if let Some(ph) = phrase {
                    r.children.push(Node::with_value("PHRASE", ph));
                }
                let asso = Node::with_value("ASSO", format!("@{x}@")).push(r);
                let ev = &mut w.ged.get_mut(&holder).unwrap().children[idx];
                if !ev.children.contains(&asso) {
                    let at = ev.children.iter().position(|c| c.tag == "SOUR").unwrap_or(ev.children.len());
                    ev.children.insert(at, asso);
                }
            }
        }

        let mut res = w.res;
        res.touched.dedup();
        Ok(res)
    }

    /// Rôle GEDCOM et précision écrits dans la citation d'une personne.
    fn citation_role(
        &self,
        p: &Participant,
        nth: usize,
        people: &[Participant],
        nths: &[usize],
    ) -> Option<(String, Option<String>)> {
        let family = self.is_family();
        let mut phrase: Vec<String> = Vec::new();
        if let Some(b) = p.role.base_phrase() {
            phrase.push(b.into());
        }
        if p.role.relative() && people.iter().zip(nths).filter(|(q, _)| q.role == ActRole::Principal).count() > 1 {
            phrase.push(format!("{} de {}", p.role.label(&self.event, family, 0), self.principal_name(p.of)));
        }
        let kin = p.get(Kinship);
        if !kin.is_empty() {
            phrase.push(kin.to_string());
        }
        let role = match p.role.gedcom(&self.event, family, nth) {
            Some(r) => r.to_string(),
            // Personne principale sans rôle standard : `OTHER` seulement pour porter son lien déclaré.
            None if !phrase.is_empty() => "OTHER".to_string(),
            None => return None,
        };
        Some((role, (!phrase.is_empty()).then(|| phrase.join(", "))))
    }

    /// Note de la citation principale : personnes mentionnées, puis la note de l'acte.
    fn citation_note(&self, people: &[Participant], nth: &[usize]) -> String {
        let family = self.is_family();
        let mut parts: Vec<String> = people
            .iter()
            .enumerate()
            .filter(|(_, p)| p.choice == PersonChoice::Mention && !p.is_blank())
            .map(|(i, p)| {
                let mut s = format!("{} : {}", p.role.label(&self.event, family, nth[i]), p.display_name());
                for f in [Age, Occupation, Residence, Kinship, Note] {
                    let v = p.get(f);
                    if !v.is_empty() {
                        s += &format!(", {}", if f == Age { age_text(v) } else { v.to_string() });
                    }
                }
                s
            })
            .collect();
        if !self.note.trim().is_empty() {
            parts.push(self.note.trim().to_string());
        }
        parts.join("; ")
    }

    /// `DATE` (+ `PHRASE`) de l'événement.
    fn date_node(&self) -> Option<Node> {
        crate::date::date_node(&self.date, &self.date_phrase)
    }

    /// Date de la citation (`DATA.DATE`) : date de rédaction, sinon date de l'événement.
    fn citation_date(&self) -> Option<Node> {
        let own = !self.record_date.trim().is_empty() || !self.record_date_phrase.trim().is_empty();
        if own { crate::date::date_node(&self.record_date, &self.record_date_phrase) } else { self.date_node() }
    }

    /// Nœud de l'événement de l'acte (sans citation).
    fn event_node(&self, age: Option<String>) -> Node {
        let mut ev = Node::new(&self.event);
        if !self.event_type.trim().is_empty() {
            ev.children.push(Node::with_value("TYPE", self.event_type.trim()));
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

    /// Citation de la source. `full` ajoute la transcription (on ne l'écrit qu'une
    /// fois, sur l'événement principal).
    fn citation(&self, media: Option<&str>, role: Option<(String, Option<String>)>, full: bool) -> Node {
        let mut c = Node::with_value("SOUR", format!("@{}@", self.source));
        if !self.page.trim().is_empty() {
            c.children.push(Node::with_value("PAGE", self.page.trim()));
        }
        let date = self.citation_date();
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
        let mut ev = Node::with_value("EVEN", &self.event);
        if let Some((r, phrase)) = role {
            let mut rn = Node::with_value("ROLE", r);
            if let Some(p) = phrase {
                rn.children.push(Node::with_value("PHRASE", p));
            }
            ev.children.push(rn);
        }
        c.children.push(ev);
        if let Some(q) = self.quality {
            c.children.push(Node::with_value("QUAY", q.min(3).to_string()));
        }
        if let Some(m) = media {
            c.children.push(Node::with_value("OBJE", format!("@{m}@")));
        }
        c
    }

    /// Ménage (recensement…) : conjoint et enfants de la première personne principale,
    /// d'après le « lien déclaré » des suivantes.
    fn link_household(&self, w: &mut Writer, people: &[Participant], ids: &[Option<String>], nth: &[usize]) {
        let principals: Vec<(&Participant, String, usize)> = people
            .iter()
            .zip(ids)
            .enumerate()
            .filter(|(_, (p, id))| p.role == ActRole::Principal && id.is_some())
            .map(|(i, (p, id))| (p, id.clone().unwrap(), nth[i]))
            .collect();
        let Some((_, head, 0)) = principals.first().cloned() else { return };
        let members = &principals[1..];
        let kin = |p: &Participant| sort_key(p.get(Kinship));
        let spouse = members.iter().find(|(p, _, _)| {
            let k = kin(p);
            ["epou", "femme", "mari", "conjoint"].iter().any(|s| k.contains(s))
        });
        if let Some((_, s, _)) = spouse {
            w.ensure_couple(&head, s);
        }
        for (p, x, _) in members {
            let k = kin(p);
            if !["fils", "fille", "enfant"].iter().any(|s| k.contains(s))
                || k.contains("petit")
                || k.contains("beau")
                || k.contains("belle")
            {
                continue;
            }
            let couple: Vec<String> = std::iter::once(head.clone()).chain(spouse.map(|(_, s, _)| s.clone())).collect();
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

/// Incrémente le premier nombre d'une référence (« Acte n° 27, vue 12 » → « Acte n° 28, vue 12 »).
pub fn increment_first_number(s: &str) -> String {
    let Some(start) = s.find(|c: char| c.is_ascii_digit()) else { return s.to_string() };
    let end = s[start..].find(|c: char| !c.is_ascii_digit()).map_or(s.len(), |e| start + e);
    match s[start..end].parse::<u64>() {
        Ok(n) => {
            let width = end - start;
            format!("{}{:0width$}{}", &s[..start], n + 1, &s[end..])
        }
        Err(_) => s.to_string(),
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

/// Âge GEDCOM (`32y`, `6m`) en français (« 32 ans », « 6 mois »).
fn age_from_gedcom(s: &str) -> String {
    let s = s.trim();
    let mut out = Vec::new();
    for part in s.split_whitespace() {
        let digits: String = part.chars().take_while(|c| c.is_ascii_digit()).collect();
        let unit = &part[digits.len()..];
        if digits.is_empty() {
            out.push(part.to_string());
            continue;
        }
        out.push(match unit {
            "y" => format!("{digits} ans"),
            "m" => format!("{digits} mois"),
            "w" => format!("{digits} semaines"),
            "d" => format!("{digits} jours"),
            _ => part.to_string(),
        });
    }
    out.join(" ")
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
    res: ActResult,
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

    /// Ajoute l'événement de l'acte, ou le fusionne avec un événement existant :
    /// événement unique (naissance, décès…) aux dates compatibles, ou événement du
    /// même type encore vide (sans date ni source). Renvoie son index.
    fn add_event(&mut self, x: &str, ev: Node) -> usize {
        let unique = matches!(ev.tag.as_str(), "BIRT" | "CHR" | "BAPM" | "DEAT" | "BURI" | "CREM");
        let node = self.ged.get_mut(x).unwrap();
        let existing = node.children.iter().position(|c| {
            c.tag == ev.tag
                && c.child_val("TYPE") == ev.child_val("TYPE")
                && (unique || (c.child("DATE").is_none() && c.child("SOUR").is_none()))
        });
        let idx = match existing {
            Some(i) if compatible_dates(node.children[i].child_val("DATE"), ev.child_val("DATE")) => {
                let target = &mut node.children[i];
                for c in ev.children {
                    match c.tag.as_str() {
                        "DATE" => {
                            let precise = target.child_val("DATE").is_none_or(|d| !starts_with_day_or_year(d));
                            if precise {
                                let at = target.children.iter().position(|n| n.tag == "DATE");
                                match at {
                                    Some(at) => target.children[at] = c,
                                    None => target.children.insert(0, c),
                                }
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
                        _ => {
                            let at =
                                target.children.iter().position(|c| c.tag == "SOUR").unwrap_or(target.children.len());
                            target.children.insert(at, c);
                        }
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
                if !existing
                    .children
                    .iter()
                    .any(|e| e.tag == sub.tag && (sub.tag != "OBJE" && sub.tag != "NOTE" || *e == sub))
                {
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
// Lecture des actes d'une source
// ----------------------------------------------------------------------

/// La citation désigne-t-elle cette source ?
fn cites(n: &Node, ptr: &str) -> bool {
    n.tag == "SOUR" && n.val() == ptr
}

/// Clé de l'acte auquel appartient une citation.
fn key_of(cite: &Node) -> ActKey {
    ActKey {
        page: cite.child_val("PAGE").unwrap_or("").trim().to_string(),
        date: cite.path_val(&["DATA", "DATE"]).unwrap_or("").trim().to_string(),
    }
}

/// Une citation d'une source dans un enregistrement : au niveau de l'enregistrement
/// (`event` vide) ou d'un de ses événements / faits.
struct Hit<'a> {
    record: &'a Node,
    event: Option<&'a Node>,
    cite: &'a Node,
}

/// Citations (niveaux 1 et 2) des individus et familles qui désignent la source.
fn hits<'a>(ged: &'a Gedcom, source: &str) -> Vec<Hit<'a>> {
    let ptr = format!("@{source}@");
    let mut out = Vec::new();
    for r in ged.records.iter().map(|r| &**r).filter(|r| r.tag == "INDI" || r.tag == "FAM") {
        for c in &r.children {
            if cites(c, &ptr) {
                out.push(Hit { record: r, event: None, cite: c });
            } else {
                for cc in c.children.iter().filter(|cc| cites(cc, &ptr)) {
                    out.push(Hit { record: r, event: Some(c), cite: cc });
                }
            }
        }
    }
    out
}

/// Rôle (`ROLE`) et précision (`PHRASE`) d'une citation.
fn cite_role(cite: &Node) -> (&str, &str) {
    let role = cite.child("EVEN").and_then(|e| e.child("ROLE"));
    (role.map(|r| r.val()).unwrap_or(""), role.and_then(|r| r.child_val("PHRASE")).unwrap_or(""))
}

/// Retire de la précision la partie déduite du rôle (« Parrain », « Père de l'époux »),
/// pour ne garder que le lien déclaré.
fn kinship_from_phrase(role: ActRole, phrase: &str) -> String {
    phrase
        .split(", ")
        .filter(|part| {
            let k = sort_key(part);
            let base = role.base_phrase().map(sort_key);
            !(base.as_deref() == Some(k.as_str())
                || (role.relative()
                    && (k.starts_with("pere de") || k.starts_with("mere de") || k.starts_with("conjoint de"))))
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// Sépare « Prénoms NOM » (nom en majuscules à la fin).
fn split_name(name: &str) -> (String, String) {
    let words: Vec<&str> = name.split_whitespace().collect();
    let upper = |w: &str| w.chars().any(char::is_alphabetic) && !w.chars().any(char::is_lowercase);
    let cut = words.iter().rposition(|w| !upper(w)).map_or(0, |i| i + 1);
    (words[..cut].join(" "), words[cut..].join(" "))
}

impl Gedcom {
    /// Actes d'une source, reconstitués à partir des citations qui la désignent,
    /// dans l'ordre chronologique.
    pub fn acts(&self, source: &str) -> Vec<Act> {
        let all = hits(self, source);
        let mut keys: Vec<ActKey> = Vec::new();
        for h in &all {
            let k = key_of(h.cite);
            if !keys.contains(&k) {
                keys.push(k);
            }
        }
        let mut acts: Vec<Act> = keys
            .into_iter()
            .map(|k| {
                let group: Vec<&Hit> = all.iter().filter(|h| key_of(h.cite) == k).collect();
                self.read_act(source, k, &group)
            })
            .collect();
        acts.sort_by_cached_key(|a| {
            let d = if a.date.is_empty() { &a.record_date } else { &a.date };
            let numbers: Vec<u64> =
                a.page.split(|c: char| !c.is_ascii_digit()).filter_map(|n| n.parse().ok()).collect();
            (d.trim().is_empty(), crate::Date::parse(d).sort_key(), numbers, a.page.clone())
        });
        acts
    }

    /// Acte de la source identifié par sa clé.
    pub fn act(&self, source: &str, key: &ActKey) -> Option<Act> {
        let all = hits(self, source);
        let group: Vec<&Hit> = all.iter().filter(|h| key_of(h.cite) == *key).collect();
        (!group.is_empty()).then(|| self.read_act(source, key.clone(), &group))
    }

    fn read_act(&self, source: &str, key: ActKey, group: &[&Hit]) -> Act {
        // Événement : celui que désignent les citations, sinon celui qui les contient.
        let event = group
            .iter()
            .find_map(|h| h.cite.child_val("EVEN").filter(|e| !e.is_empty()))
            .or_else(|| {
                group.iter().find_map(|h| {
                    h.event.map(|e| e.tag.as_str()).filter(|t| !matches!(*t, "OCCU" | "RESI" | "NAME" | "SEX"))
                })
            })
            .unwrap_or("")
            .to_string();
        let mut act = Act::new(source, &event);
        act.participants.clear();
        act.estimate_births = true;
        act.page = key.page.clone();
        act.original = Some(key.clone());

        let is_main = |h: &Hit| h.event.is_some_and(|e| e.tag == event && !event.is_empty());
        let main: Vec<&&Hit> = group.iter().filter(|h| is_main(h)).collect();
        act.family_even = main.first().is_some_and(|h| h.record.tag == "FAM");
        if let Some(e) = main.first().and_then(|h| h.event) {
            act.event_type = e.child_val("TYPE").unwrap_or("").to_string();
            if let Some(d) = e.child("DATE") {
                act.date = d.val().to_string();
                act.date_phrase = d.child_val("PHRASE").unwrap_or("").to_string();
            }
            act.place = e.child_val("PLAC").unwrap_or("").to_string();
        }
        // Données de la citation : la citation principale d'abord.
        let ordered: Vec<&&Hit> = main.iter().copied().chain(group.iter().filter(|h| !is_main(h))).collect();
        if let Some(d) = ordered.iter().find_map(|h| h.cite.child("DATA").and_then(|d| d.child("DATE"))) {
            let (v, p) = (d.val().to_string(), d.child_val("PHRASE").unwrap_or("").to_string());
            if v != act.date || p != act.date_phrase {
                act.record_date = v;
                act.record_date_phrase = p;
            }
        }
        act.transcription = ordered
            .iter()
            .find_map(|h| h.cite.child("DATA").and_then(|d| d.child("TEXT")))
            .map(|t| t.text())
            .unwrap_or_default();
        act.quality = ordered.iter().find_map(|h| h.cite.child_val("QUAY")).and_then(|q| q.trim().parse().ok());
        act.media = ordered
            .iter()
            .find_map(|h| h.cite.child("OBJE").and_then(|o| o.pointer()))
            .map(|m| MediaChoice::Existing(m.to_string()))
            .unwrap_or_default();

        // Personnes.
        let mut people: Vec<(String, Participant)> = Vec::new();
        let person = |x: &str, role: ActRole| -> Participant {
            let mut p = Participant::new(role);
            p.choice = PersonChoice::Link(x.to_string());
            if let Some(i) = self.individual(x) {
                p.set(Surname, i.surname());
                p.set(Given, i.given());
                if !matches!(i.sex(), "" | "U") {
                    p.set(Sex, i.sex());
                }
            }
            p
        };
        // Personnes principales : sur l'événement (ou les époux de la famille).
        for h in &main {
            let e = h.event.unwrap();
            if h.record.tag == "FAM" {
                for tag in ["HUSB", "WIFE"] {
                    let Some(x) = h.record.child(tag).and_then(|n| n.pointer()) else { continue };
                    if people.iter().any(|(y, _)| y == x) {
                        continue;
                    }
                    let mut p = person(x, ActRole::Principal);
                    if let Some(a) = e.child(tag).and_then(|n| n.child_val("AGE")) {
                        p.set(Age, age_from_gedcom(a));
                    }
                    people.push((x.to_string(), p));
                }
            } else if let Some(x) = h.record.xref.as_deref()
                && !people.iter().any(|(y, _)| y == x)
            {
                let mut p = person(x, ActRole::Principal);
                if let Some(a) = e.child_val("AGE") {
                    p.set(Age, age_from_gedcom(a));
                }
                let (role, phrase) = cite_role(h.cite);
                if !phrase.is_empty() && role != "PRIN" {
                    p.set(Kinship, phrase);
                }
                people.push((x.to_string(), p));
            }
        }
        // Les autres : rôle de la citation, et faits qu'elle prouve.
        for h in group.iter().filter(|h| !is_main(h) && h.record.tag == "INDI") {
            let Some(x) = h.record.xref.as_deref() else { continue };
            let at = match people.iter().position(|(y, _)| y == x) {
                Some(i) => i,
                None => {
                    let (role, phrase) = cite_role(h.cite);
                    let sex = self.individual(x).map(|i| i.sex().to_string()).unwrap_or_default();
                    let r = ActRole::from_gedcom(role, phrase, &sex);
                    let mut p = person(x, r);
                    let kin = kinship_from_phrase(r, phrase);
                    if !kin.is_empty() {
                        p.set(Kinship, kin);
                    }
                    people.push((x.to_string(), p));
                    people.len() - 1
                }
            };
            let p = &mut people[at].1;
            match h.event {
                Some(e) if e.tag == "OCCU" => p.set(Occupation, e.val()),
                Some(e) if e.tag == "RESI" => p.set(Residence, e.child_val("PLAC").unwrap_or("")),
                Some(e) if matches!(e.tag.as_str(), "BIRT" | "CHR" | "BAPM") => {
                    if let Some(pl) = e.child_val("PLAC") {
                        p.set(Birthplace, pl);
                    }
                    // Naissance estimée d'après l'âge déclaré.
                    if p.get(Age).is_empty()
                        && let Some(est) = e.child_val("DATE").and_then(|d| d.strip_prefix("EST "))
                        && let (Ok(b), Some(y)) = (est.trim().parse::<i32>(), act.year())
                    {
                        p.set(Age, format!("{} ans", y - b));
                    }
                }
                _ => {}
            }
        }
        // Père, mère, conjoint de quelle personne principale ?
        let principals: Vec<String> =
            people.iter().filter(|(_, p)| p.role == ActRole::Principal).map(|(x, _)| x.clone()).collect();
        if principals.len() > 1 {
            let family = act.is_family();
            for (x, p) in people.iter_mut().filter(|(_, p)| p.role.relative()) {
                let phrase = group
                    .iter()
                    .find(|h| h.record.xref.as_deref() == Some(x.as_str()))
                    .map(|h| sort_key(cite_role(h.cite).1))
                    .unwrap_or_default();
                let by_phrase = if family && phrase.contains("de l'epouse") {
                    Some(1)
                } else if family && phrase.contains("de l'epoux") {
                    Some(0)
                } else {
                    None
                };
                let by_links = principals.iter().position(|pr| {
                    let (f, m) = self.parents(pr);
                    match p.role {
                        ActRole::Spouse => {
                            self.spouses_and_children(pr).iter().any(|(s, _)| s.as_ref().is_some_and(|s| s.xref() == x))
                        }
                        _ => [f, m].into_iter().flatten().any(|i| i.xref() == x),
                    }
                });
                p.of = by_phrase.or(by_links).unwrap_or(0);
            }
        }
        // Personnes mentionnées : dans la note de la citation principale.
        let note_cite = main.first().map(|h| h.cite).or_else(|| group.first().map(|h| h.cite));
        let mut kept_notes: Vec<String> = Vec::new();
        let family = act.is_family();
        for n in note_cite.into_iter().flat_map(|c| c.children_with("NOTE")) {
            for part in n.text().split("; ") {
                let parsed = part.split_once(" : ").and_then(|(label, rest)| {
                    let role = ActRole::ALL
                        .into_iter()
                        .find(|r| *r != ActRole::Principal && r.label(&event, family, 0) == label.trim())?;
                    let mut fields = rest.split(", ");
                    let (given, surname) = split_name(fields.next().unwrap_or(""));
                    let mut p = Participant::new(role).with(Given, &given).with(Surname, &surname);
                    p.choice = PersonChoice::Mention;
                    let more: Vec<&str> = fields.collect();
                    if !more.is_empty() {
                        p.set(Note, more.join(", "));
                    }
                    Some(p)
                });
                match parsed {
                    Some(p) => people.push((String::new(), p)),
                    None if !part.trim().is_empty() => kept_notes.push(part.trim().to_string()),
                    None => {}
                }
            }
        }
        act.note = kept_notes.join("; ");

        // Ordre : personnes principales, puis par rôle.
        let mut list: Vec<Participant> = people.into_iter().map(|(_, p)| p).collect();
        list.sort_by_key(|p| (ActRole::ALL.iter().position(|r| *r == p.role).unwrap_or(99), p.of));
        act.participants = list;
        act
    }

    /// Supprime un acte : ses citations et les témoins qu'il associait aux événements.
    /// Les événements, les faits, les individus et les liens familiaux restent.
    /// Renvoie les enregistrements modifiés.
    pub fn remove_act(&mut self, source: &str, key: &ActKey) -> Vec<String> {
        self.unlink_act(source, key, false)
    }

    /// Retire les citations de l'acte. `strip` (modification de l'acte) : ce que l'acte
    /// était seul à établir est vidé aussi (date, lieu et âge de son événement ; profession,
    /// domicile, naissance estimée), pour que la nouvelle version le réécrive à la même place.
    fn unlink_act(&mut self, source: &str, key: &ActKey, strip: bool) -> Vec<String> {
        let ptr = format!("@{source}@");
        let act = self.act(source, key);
        let event = act.as_ref().map(|a| a.event.clone()).unwrap_or_default();
        let people: Vec<String> = act
            .map(|a| {
                a.participants
                    .iter()
                    .filter_map(|p| match &p.choice {
                        PersonChoice::Link(x) => Some(format!("@{x}@")),
                        _ => None,
                    })
                    .collect()
            })
            .unwrap_or_default();
        let matches = |n: &Node| cites(n, &ptr) && key_of(n) == *key;
        let in_record = |r: &Node| {
            (r.tag == "INDI" || r.tag == "FAM")
                && r.children.iter().any(|c| matches(c) || c.children.iter().any(&matches))
        };
        // Témoin associé par l'acte : `ASSO` vers une personne de l'acte, avec son seul rôle.
        let act_asso = |n: &Node| {
            n.tag == "ASSO" && people.iter().any(|p| p == n.val()) && n.children.iter().all(|c| c.tag == "ROLE")
        };
        let mut touched = Vec::new();
        for r in self.records_where_mut(in_record) {
            r.children.retain(|c| !matches(c));
            let mut drop = Vec::new();
            for (i, c) in r.children.iter_mut().enumerate() {
                let before = c.children.len();
                c.children.retain(|cc| !matches(cc));
                if c.children.len() == before {
                    continue;
                }
                c.children.retain(|n| !act_asso(n));
                if !strip || c.children.iter().any(|n| n.tag == "SOUR") {
                    continue;
                }
                if c.tag == event {
                    // L'événement de l'acte : la nouvelle version le complète à la même place.
                    c.children.retain(|n| {
                        !(matches!(n.tag.as_str(), "DATE" | "PLAC" | "AGE")
                            || (matches!(n.tag.as_str(), "HUSB" | "WIFE") && n.children.iter().all(|a| a.tag == "AGE")))
                    });
                } else if matches!(c.tag.as_str(), "OCCU" | "RESI" | "BIRT")
                    && c.children.iter().all(|n| matches!(n.tag.as_str(), "DATE" | "PLAC"))
                    && (c.tag != "BIRT" || c.child_val("DATE").is_none_or(|d| d.starts_with("EST ")))
                {
                    // Fait déduit par l'acte (profession, domicile, naissance estimée).
                    drop.push(i);
                }
            }
            for i in drop.into_iter().rev() {
                r.children.remove(i);
            }
            r.touch();
            if let Some(x) = r.xref.clone() {
                touched.push(x);
            }
        }
        touched
    }

    /// Retire les événements restés vides après la modification d'un acte.
    fn drop_empty_events(&mut self, records: &[String]) {
        for x in records {
            let Some(r) = self.get_mut(x) else { continue };
            r.children.retain(|c| {
                let empty = c.value.as_deref().is_none_or(|v| v.trim().is_empty())
                    && c.children.iter().all(|n| n.tag == "TYPE")
                    && crate::model::event_label(&c.tag).is_some();
                !empty
            });
        }
    }
}

// ----------------------------------------------------------------------
// Correspondances avec les individus existants
// ----------------------------------------------------------------------

impl Act {
    /// Individus existants qui pourraient être la personne citée à l'index `idx`,
    /// du plus probable au moins probable.
    pub fn suggestions(&self, ged: &Gedcom, idx: usize) -> Vec<Suggestion> {
        let Some(p) = self.effective().get(idx).cloned() else { return vec![] };
        let nth = self.nth_principal(idx);
        let surname = sort_key(p.get(Surname));
        let given = sort_key(p.get(Given));
        // Rien de saisi pour cette personne (le nom hérité ne suffit pas) : liens déduits seulement.
        if self.participants.get(idx).is_none_or(|p| p.is_blank()) || (surname.is_empty() && given.is_empty()) {
            return self.context_suggestions(ged, idx);
        }
        let first_given = given.split_whitespace().next().unwrap_or("").to_string();
        let year = self.year();
        let expected_birth = match (year, parse_age_years(p.get(Age))) {
            (Some(y), Some(a)) => Some(y - a),
            (Some(y), None) if p.role == ActRole::Principal && is_birth_like(&self.event) => Some(y),
            _ => None,
        };
        let sex = p.role.sex(self.is_family(), nth).map(str::to_string).unwrap_or_else(|| p.get(Sex).to_uppercase());
        let context: Vec<(String, String)> =
            self.context_suggestions(ged, idx).into_iter().map(|s| (s.xref, s.reason)).collect();

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
                    && !(p.role == ActRole::Principal && is_death_like(&self.event))
                {
                    score -= 4.0;
                }
                if let Some(b) = i.birth_year()
                    && let Some(y) = year
                {
                    // Né après l'acte, ou trop jeune pour être parent / conjoint.
                    let min_age = match p.role {
                        ActRole::Father | ActRole::Mother => 14,
                        ActRole::Spouse => 12,
                        ActRole::Principal if self.is_family() => 12,
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
                    label: suggestion_label(&i),
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
    fn context_suggestions(&self, ged: &Gedcom, idx: usize) -> Vec<Suggestion> {
        let Some(me) = self.participants.get(idx) else { return vec![] };
        let nths: Vec<usize> = (0..self.participants.len()).map(|i| self.nth_principal(i)).collect();
        let linked_where = |pred: &dyn Fn(usize, &Participant) -> bool| -> Vec<String> {
            self.participants
                .iter()
                .enumerate()
                .filter(|(i, p)| *i != idx && pred(*i, p))
                .filter_map(|(_, p)| match &p.choice {
                    PersonChoice::Link(x) => Some(x.clone()),
                    _ => None,
                })
                .collect()
        };
        let mk = |x: &str, why: String| {
            ged.individual(x).map(|i| Suggestion {
                xref: x.to_string(),
                label: suggestion_label(&i),
                score: 10.0,
                reason: why,
            })
        };
        let mut out = Vec::new();
        let spouses_of = |x: &str, out: &mut Vec<Suggestion>| {
            let who = ged.record_label(x);
            for (s, _) in ged.spouses_and_children(x) {
                out.extend(s.and_then(|s| mk(s.xref(), format!("conjoint de {who}"))));
            }
        };
        match me.role {
            ActRole::Father | ActRole::Mother => {
                let of = me.of;
                for c in linked_where(&|i, p| p.role == ActRole::Principal && nths[i] == of) {
                    let (f, m) = ged.parents(&c);
                    let p = if me.role == ActRole::Father { f } else { m };
                    let child = ged.record_label(&c);
                    out.extend(p.and_then(|p| mk(p.xref(), format!("parent de {child}"))));
                }
                // L'autre parent est lié : on propose son conjoint.
                let other = if me.role == ActRole::Father { ActRole::Mother } else { ActRole::Father };
                for o in linked_where(&|_, p| p.role == other && p.of == of) {
                    spouses_of(&o, &mut out);
                }
            }
            ActRole::Spouse => {
                let of = me.of;
                for c in linked_where(&|i, p| p.role == ActRole::Principal && nths[i] == of) {
                    spouses_of(&c, &mut out);
                }
            }
            ActRole::Principal => {
                let mine = nths[idx];
                // Enfant d'un père ou d'une mère déjà lié(e) ; conjoint de l'autre époux.
                for par in linked_where(&|_, p| matches!(p.role, ActRole::Father | ActRole::Mother) && p.of == mine) {
                    let who = ged.record_label(&par);
                    for (_, kids) in ged.spouses_and_children(&par) {
                        for kid in kids {
                            out.extend(mk(kid.xref(), format!("enfant de {who}")));
                        }
                    }
                }
                if self.is_family() {
                    for other in linked_where(&|_, p| p.role == ActRole::Principal) {
                        spouses_of(&other, &mut out);
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

fn suggestion_label(i: &Individual) -> String {
    let years = i.years();
    if years.is_empty() { i.display_name() } else { format!("{} ({years})", i.display_name()) }
}

// ----------------------------------------------------------------------
// Ancienne saisie guidée (modèles) : nettoyage par l'assistant d'import
// ----------------------------------------------------------------------

/// Balises des anciens modèles d'actes de GEDRoots (`_GR_TPL`…), qui ne sont plus écrites.
pub const LEGACY_TEMPLATE_TAGS: &[&str] =
    &["_GR_TPL", "_GR_FLD", "_GR_ROLE", "_GR_REL", "_GR_PF", "_GR_PAGE", "_GR_EVT", "_GR_MULT"];

impl Gedcom {
    /// Restes de l'ancienne saisie guidée : modèles `_GR_TPL`, champs `_GR_FLD` des
    /// citations (déjà repris dans `PAGE`) et rôle non standard `PRIN`.
    pub fn legacy_template_count(&self) -> usize {
        let mut n = 0;
        for r in &self.records {
            if r.tag == "_GR_TPL" {
                n += 1;
                continue;
            }
            r.walk(&mut |c| {
                if c.tag == "_GR_TPL" || c.tag == "_GR_FLD" || (c.tag == "ROLE" && c.val() == "PRIN") {
                    n += 1;
                }
            });
        }
        n
    }

    /// Retire les restes de l'ancienne saisie guidée. Les données de l'acte restent
    /// dans les structures standard (`PAGE`, `EVEN`, `DATA`). Renvoie le nombre d'éléments retirés.
    pub fn migrate_legacy_templates(&mut self) -> usize {
        let before = self.records.len();
        self.records.retain(|r| r.tag != "_GR_TPL");
        let mut n = before - self.records.len();
        if n > 0 {
            self.reindex();
        }
        let legacy = |c: &Node| c.tag == "_GR_TPL" || c.tag == "_GR_FLD" || (c.tag == "ROLE" && c.val() == "PRIN");
        for r in self.records_where_mut(|r| r.any(&legacy)) {
            r.walk_mut(&mut |c| {
                let k = c.children.len();
                c.children.retain(|x| !legacy(x));
                n += k - c.children.len();
            });
            r.touch();
        }
        for tag in LEGACY_TEMPLATE_TAGS {
            self.undeclare_extension(tag);
        }
        n
    }
}
