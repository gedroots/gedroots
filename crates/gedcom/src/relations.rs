//! Création, liaison et suppression de personnes, familles, lieux et médias,
//! en gardant les pointeurs dans les deux sens cohérents
//! (`FAM.HUSB/WIFE/CHIL` ↔ `INDI.FAMS/FAMC`, `OBJE` ↔ enregistrements).

use crate::document::{Gedcom, RecordKind};
use crate::model::{Family, Individual};
use crate::node::Node;

/// Lien de parenté à créer depuis une personne.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Relation {
    Father,
    Mother,
    Spouse,
    Child,
    Sibling,
}

impl Relation {
    pub const ALL: [Relation; 5] =
        [Relation::Father, Relation::Mother, Relation::Spouse, Relation::Child, Relation::Sibling];

    pub fn label_fr(self) -> &'static str {
        match self {
            Relation::Father => "Père",
            Relation::Mother => "Mère",
            Relation::Spouse => "Conjoint",
            Relation::Child => "Enfant",
            Relation::Sibling => "Frère ou sœur",
        }
    }
}

fn ptr(x: &str) -> String {
    format!("@{x}@")
}

impl Gedcom {
    fn push_link(&mut self, holder: &str, tag: &str, target: &str) {
        let p = ptr(target);
        if let Some(n) = self.get_mut(holder)
            && !n.children.iter().any(|c| c.tag == tag && c.val() == p)
        {
            n.insert_child(Node::with_value(tag, p));
            n.touch();
        }
    }

    fn remove_link(&mut self, holder: &str, tag: &str, target: &str) {
        let p = ptr(target);
        if let Some(n) = self.get_mut(holder) {
            let before = n.children.len();
            n.children.retain(|c| !(c.tag == tag && c.val() == p));
            if n.children.len() != before {
                n.touch();
            }
        }
    }

    /// Nouvelle personne avec ce nom et ce sexe.
    pub fn create_individual(&mut self, given: &str, surname: &str, sex: &str) -> String {
        let x = self.create(RecordKind::Individual);
        let node = self.get_mut(&x).unwrap();
        let name = node.ensure_child("NAME");
        name.value = Some(format!("{given} /{surname}/").trim_start().to_string());
        name.set_child_val("GIVN", given);
        name.set_child_val("SURN", surname);
        node.set_child_val("SEX", sex);
        x
    }

    /// Nouvelle famille vide.
    pub fn create_family(&mut self) -> String {
        self.create(RecordKind::Family)
    }

    /// Place `indi` comme époux (HUSB) ou épouse (WIFE) de la famille selon son sexe
    /// (ou la place libre si le sexe est inconnu).
    pub fn set_spouse(&mut self, fam: &str, indi: &str) -> Result<(), String> {
        self.set_parent_slot(fam, indi, None)
    }

    /// Comme [`Gedcom::set_spouse`], en imposant la place (`HUSB` ou `WIFE`).
    /// Refuse une personne dont le sexe connu contredit la place demandée.
    pub fn set_parent_slot(&mut self, fam: &str, indi: &str, slot: Option<&str>) -> Result<(), String> {
        let f = self.family(fam).ok_or("famille introuvable")?;
        if f.husband() == Some(indi) || f.wife() == Some(indi) {
            return Ok(());
        }
        if f.children().contains(&indi) {
            return Err("cette personne est déjà un enfant de la famille".into());
        }
        let person = self.individual(indi).ok_or("personne introuvable")?;
        let sex = person.sex().to_string();
        let tag = match (slot, sex.as_str(), f.husband(), f.wife()) {
            (Some("HUSB"), "F", ..) => {
                return Err(format!("{} est une femme : elle ne peut pas être le père", person.display_name()));
            }
            (Some("WIFE"), "M", ..) => {
                return Err(format!("{} est un homme : il ne peut pas être la mère", person.display_name()));
            }
            (Some("HUSB"), _, Some(_), _) => return Err("la famille a déjà un père".into()),
            (Some("WIFE"), _, _, Some(_)) => return Err("la famille a déjà une mère".into()),
            (Some(t @ ("HUSB" | "WIFE")), ..) => t,
            (_, s, h, w) => match (s, h, w) {
                ("M", None, _) => "HUSB",
                ("F", _, None) => "WIFE",
                ("M", Some(_), _) => return Err("la famille a déjà un père".into()),
                ("F", _, Some(_)) => return Err("la famille a déjà une mère".into()),
                (_, None, _) => "HUSB",
                (_, _, None) => "WIFE",
                _ => return Err("la famille a déjà deux parents".into()),
            },
        };
        let node = self.get_mut(fam).unwrap();
        // HUSB et WIFE se placent en tête, avant les enfants et les événements.
        let at = node.children.iter().position(|c| c.tag != "HUSB" && c.tag != "WIFE").unwrap_or(node.children.len());
        node.children.insert(at, Node::with_value(tag, ptr(indi)));
        node.touch();
        self.push_link(indi, "FAMS", fam);
        Ok(())
    }

    /// Ajoute `child` aux enfants de la famille.
    pub fn add_child_to_family(&mut self, fam: &str, child: &str) -> Result<(), String> {
        let f = self.family(fam).ok_or("famille introuvable")?;
        if f.husband() == Some(child) || f.wife() == Some(child) {
            return Err("un parent ne peut pas être son propre enfant".into());
        }
        if self.is_ancestor_of(child, fam) {
            return Err("cette personne est un ascendant de la famille".into());
        }
        if !f.children().contains(&child) {
            let node = self.get_mut(fam).unwrap();
            // Après le dernier enfant (ou les parents), avant les événements.
            let at = node
                .children
                .iter()
                .rposition(|c| matches!(c.tag.as_str(), "HUSB" | "WIFE" | "CHIL"))
                .map(|i| i + 1)
                .unwrap_or(0);
            node.children.insert(at, Node::with_value("CHIL", ptr(child)));
            node.touch();
        }
        self.push_link(child, "FAMC", fam);
        Ok(())
    }

    /// `x` figure-t-il parmi les parents (ou leurs ascendants) de la famille ?
    fn is_ancestor_of(&self, x: &str, fam: &str) -> bool {
        let mut stack = vec![fam.to_string()];
        let mut seen = std::collections::HashSet::new();
        while let Some(f) = stack.pop() {
            if !seen.insert(f.clone()) {
                continue;
            }
            let Some(fam) = self.family(&f) else { continue };
            for p in [fam.husband(), fam.wife()].into_iter().flatten() {
                if p == x {
                    return true;
                }
                if let Some(i) = self.individual(p) {
                    stack.extend(i.famc().into_iter().map(str::to_string));
                }
            }
        }
        false
    }

    /// Famille d'origine de la personne, créée si besoin.
    fn ensure_parents_family(&mut self, indi: &str) -> Result<String, String> {
        let i = self.individual(indi).ok_or("personne introuvable")?;
        if let Some(f) = i.famc().first() {
            return Ok(f.to_string());
        }
        let fam = self.create_family();
        self.add_child_to_family(&fam, indi)?;
        Ok(fam)
    }

    /// Ajoute un événement (ou attribut) à un individu ou une famille.
    /// `detail` : la profession pour OCCU, le type pour un événement libre
    /// (EVEN), sinon un complément (`TYPE`). La source est citée si donnée.
    pub fn add_event(&mut self, owner: &str, ev: &NewEvent) -> Result<(), String> {
        let node = self.get_mut(owner).ok_or("enregistrement introuvable")?;
        let detail = ev.detail.trim();
        if ev.tag == "EVEN" && detail.is_empty() {
            return Err("précisez le type de l'événement".into());
        }
        let attribute = matches!(ev.tag.as_str(), "OCCU" | "EDUC" | "RELI" | "TITL" | "NATI" | "RESI" | "FACT");
        let mut e = Node::new(ev.tag.as_str());
        if attribute && ev.tag != "RESI" {
            e.value = Some(detail.to_string());
        } else if !detail.is_empty() {
            e.children.push(Node::with_value("TYPE", detail));
        }
        if let Some(d) = crate::date::date_node(&ev.date.to_uppercase(), &ev.date_phrase) {
            e.children.push(d);
        }
        if !ev.place.trim().is_empty() {
            e.children.push(Node::with_value("PLAC", ev.place.trim()));
        }
        if let Some(s) = ev.source.as_deref().filter(|s| !s.is_empty()) {
            e.children.push(Node::with_value("SOUR", format!("@{}@", s.trim_matches('@'))));
        }
        // Événement sans aucun détail : « 1 DEAT Y » (il a eu lieu).
        if !attribute && e.children.is_empty() {
            e.value = Some("Y".into());
        }
        node.insert_child(e);
        Ok(())
    }

    /// Crée une nouvelle personne liée à `indi` et renvoie son identifiant.
    /// `family` choisit l'union pour un enfant quand il y en a plusieurs.
    pub fn add_relative(&mut self, indi: &str, rel: Relation, family: Option<&str>) -> Result<String, String> {
        let i = self.individual(indi).ok_or("personne introuvable")?;
        let surname = i.surname();
        let sex = i.sex().to_string();
        match rel {
            Relation::Father | Relation::Mother => {
                let (f, m) = self.parents(indi);
                if rel == Relation::Father && f.is_some() {
                    return Err("cette personne a déjà un père".into());
                }
                if rel == Relation::Mother && m.is_some() {
                    return Err("cette personne a déjà une mère".into());
                }
                let fam = self.ensure_parents_family(indi)?;
                let new = if rel == Relation::Father {
                    self.create_individual("", &surname, "M")
                } else {
                    self.create_individual("", "", "F")
                };
                self.set_spouse(&fam, &new)?;
                Ok(new)
            }
            Relation::Spouse => {
                let other = match sex.as_str() {
                    "M" => "F",
                    "F" => "M",
                    _ => "U",
                };
                let new = self.create_individual("", "", other);
                let fam = self.union_for_spouse(indi)?;
                self.set_spouse(&fam, &new)?;
                Ok(new)
            }
            Relation::Child => {
                let fams = i.fams().into_iter().map(str::to_string).collect::<Vec<_>>();
                let fam = match family {
                    Some(f) => f.to_string(),
                    None => match fams.as_slice() {
                        [] => {
                            let f = self.create_family();
                            self.set_spouse(&f, indi)?;
                            f
                        }
                        // La dernière union par défaut.
                        [.., last] => last.clone(),
                    },
                };
                let father_surname = self
                    .family(&fam)
                    .and_then(|f| f.husband())
                    .and_then(|h| self.individual(h))
                    .map(|h| h.surname())
                    .unwrap_or(if sex == "M" { surname } else { String::new() });
                let new = self.create_individual("", &father_surname, "U");
                self.add_child_to_family(&fam, &new)?;
                Ok(new)
            }
            Relation::Sibling => {
                let fam = self.ensure_parents_family(indi)?;
                let new = self.create_individual("", &surname, "U");
                self.add_child_to_family(&fam, &new)?;
                Ok(new)
            }
        }
    }

    /// Lie deux personnes existantes : `other` devient le `rel` de `indi`.
    pub fn link_relative(&mut self, indi: &str, rel: Relation, other: &str) -> Result<(), String> {
        if indi == other {
            return Err("on ne peut pas lier une personne à elle-même".into());
        }
        self.individual(other).ok_or("personne introuvable")?;
        match rel {
            Relation::Father | Relation::Mother => {
                let (f, m) = self.parents(indi);
                let slot = if rel == Relation::Father { f } else { m };
                if slot.is_some() {
                    return Err(format!(
                        "cette personne a déjà un{}",
                        if rel == Relation::Father { " père" } else { "e mère" }
                    ));
                }
                // Le parent rejoint la famille d'origine existante (celle de l'autre parent), sinon une nouvelle.
                let place = if rel == Relation::Father { "HUSB" } else { "WIFE" };
                let existing = self.individual(indi).and_then(|i| i.famc().first().map(|s| s.to_string()));
                match existing {
                    Some(fam) => self.set_parent_slot(&fam, other, Some(place)),
                    None => {
                        let fam = self.create_family();
                        if let Err(e) = self.set_parent_slot(&fam, other, Some(place)) {
                            self.delete(&fam);
                            return Err(e);
                        }
                        self.add_child_to_family(&fam, indi)
                    }
                }
            }
            Relation::Spouse => {
                let already = self
                    .individual(indi)
                    .unwrap()
                    .fams()
                    .iter()
                    .any(|f| self.family(f).is_some_and(|f| f.husband() == Some(other) || f.wife() == Some(other)));
                if already {
                    return Err("ces deux personnes forment déjà un couple".into());
                }
                let fam = self.union_for_spouse(indi)?;
                self.set_spouse(&fam, other)
            }
            Relation::Child => {
                let fam = match self.individual(indi).unwrap().fams().last() {
                    Some(f) => f.to_string(),
                    None => {
                        let f = self.create_family();
                        self.set_spouse(&f, indi)?;
                        f
                    }
                };
                self.add_child_to_family(&fam, other)
            }
            Relation::Sibling => {
                let fam = self.ensure_parents_family(indi)?;
                self.add_child_to_family(&fam, other)
            }
        }
    }

    /// Retire une personne d'une famille (comme parent ou comme enfant).
    /// La famille est supprimée si elle devient vide.
    pub fn unlink_from_family(&mut self, indi: &str, fam: &str) {
        let p = ptr(indi);
        if let Some(n) = self.get_mut(fam) {
            n.children.retain(|c| !(matches!(c.tag.as_str(), "HUSB" | "WIFE" | "CHIL") && c.val() == p));
            n.touch();
        }
        self.remove_link(indi, "FAMS", fam);
        self.remove_link(indi, "FAMC", fam);
        self.prune_family(fam);
    }

    /// Union où placer un nouveau conjoint de `indi` : son union encore sans
    /// conjoint (parent seul avec ses enfants), sinon une nouvelle union.
    fn union_for_spouse(&mut self, indi: &str) -> Result<String, String> {
        let open = self.individual(indi).ok_or("personne introuvable")?.fams().into_iter().find(|f| {
            self.family(f).is_some_and(|f| {
                let spouses: Vec<&str> = [f.husband(), f.wife()].into_iter().flatten().collect();
                spouses == [indi]
            })
        });
        if let Some(f) = open {
            return Ok(f.to_string());
        }
        let fam = self.create_family();
        self.set_spouse(&fam, indi)?;
        Ok(fam)
    }

    /// Supprime la famille si elle ne relie plus personne (au plus un membre et aucun événement).
    fn prune_family(&mut self, fam: &str) -> bool {
        let Some(f) = self.family(fam) else { return false };
        let members = [f.husband(), f.wife()].into_iter().flatten().count() + f.children().len();
        if members == 0 || (members == 1 && f.events().is_empty()) {
            self.delete(fam);
            return true;
        }
        false
    }

    /// Supprime une personne, ses liens, et les familles devenues vides.
    pub fn delete_individual(&mut self, xref: &str) -> Option<Node> {
        let fams: Vec<String> = self
            .individual(xref)
            .map(|i| i.famc().into_iter().chain(i.fams()).map(str::to_string).collect())
            .unwrap_or_default();
        let removed = self.delete(xref)?;
        for f in fams {
            self.prune_family(&f);
        }
        Some(removed)
    }

    /// Supprime une famille et les liens `FAMS`/`FAMC` vers elle (les personnes restent).
    pub fn delete_family(&mut self, xref: &str) -> Option<Node> {
        self.delete(xref)
    }

    /// Supprime n'importe quel enregistrement, avec le nettoyage adapté à son type.
    pub fn delete_record(&mut self, xref: &str) -> Option<Node> {
        match self.get(xref).map(|n| n.tag.clone()).as_deref() {
            Some("INDI") => self.delete_individual(xref),
            Some(_) => self.delete(xref),
            None => None,
        }
    }

    /// Retire un lieu de tous les événements (les événements restent). Renvoie le nombre d'occurrences.
    pub fn delete_place(&mut self, name: &str) -> usize {
        let mut count = 0;
        let here = |n: &Node| n.tag == "PLAC" && n.val() == name;
        for r in self.records_where_mut(|r| r.any(&here)) {
            let mut touched = false;
            r.walk_mut(&mut |n| {
                let before = n.children.len();
                n.children.retain(|c| !(c.tag == "PLAC" && c.val() == name));
                let removed = before - n.children.len();
                count += removed;
                touched |= removed > 0;
            });
            if touched {
                r.touch();
            }
        }
        count
    }

    /// Média dont le fichier est `file` (chemin tel qu'écrit dans `FILE`).
    pub fn media_by_file(&self, file: &str) -> Option<String> {
        self.records_of(RecordKind::Media)
            .find(|m| m.children_with("FILE").any(|f| f.val() == file))
            .and_then(|m| m.xref.clone())
    }

    /// Crée (ou réutilise) le média pointant vers `file` et renvoie son identifiant.
    /// Le titre est tiré du nom de fichier.
    pub fn media_for_file(&mut self, file: &str) -> String {
        if let Some(x) = self.media_by_file(file) {
            return x;
        }
        let x = self.create(RecordKind::Media);
        let node = self.get_mut(&x).unwrap();
        let f = node.ensure_child("FILE");
        f.value = Some(file.to_string());
        f.set_child_val("FORM", mime_for(file));
        f.set_child_val("TITL", &title_from_file(file));
        x
    }

    /// Rattache un média à un enregistrement (personne, famille, source…).
    pub fn attach_media(&mut self, record: &str, media: &str) -> Result<(), String> {
        let tag = self.get(record).ok_or("enregistrement introuvable")?.tag.clone();
        if !matches!(tag.as_str(), "INDI" | "FAM" | "SOUR" | "SUBM") {
            return Err("on ne peut rattacher un média qu'à une personne, une famille ou une source".into());
        }
        if self.get(media).is_none_or(|m| m.tag != "OBJE") {
            return Err("média introuvable".into());
        }
        self.push_link(record, "OBJE", media);
        Ok(())
    }

    /// Détache un média d'un enregistrement (le média reste dans le fichier).
    pub fn detach_media(&mut self, record: &str, media: &str) {
        self.remove_link(record, "OBJE", media);
    }

    /// Tous les chemins `FILE` déjà référencés.
    pub fn referenced_files(&self) -> std::collections::HashSet<String> {
        self.records_of(RecordKind::Media)
            .flat_map(|m| m.children_with("FILE").map(|f| f.val().to_string()).collect::<Vec<_>>())
            .collect()
    }
}

/// Type MIME d'après l'extension (`FILE.FORM` est obligatoire en GEDCOM 7).
pub fn mime_for(file: &str) -> &'static str {
    let web = crate::weblink::is_web_url(file);
    let file = if web { file.split(['?', '#']).next().unwrap_or(file) } else { file };
    let ext = file.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
    match ext.as_str() {
        "jpg" | "jpeg" => "image/jpeg",
        "png" => "image/png",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "bmp" => "image/bmp",
        "tif" | "tiff" => "image/tiff",
        "heic" => "image/heic",
        "pdf" => "application/pdf",
        "mp3" => "audio/mpeg",
        "wav" => "audio/wav",
        "mp4" => "video/mp4",
        "txt" => "text/plain",
        "htm" | "html" => "text/html",
        // Page web (« …/ark:47539/s0051b… », « …/fiche.php »).
        _ if web => crate::weblink::WEB_FORM,
        _ => "application/octet-stream",
    }
}

/// Fichiers qu'on propose de rattacher.
pub fn is_media_file(name: &str) -> bool {
    !name.starts_with('.') && mime_for(name) != "application/octet-stream" && !name.ends_with(".ged")
}

/// « Actes/Naissance_Jean-Guyot_1850.jpg » → « Naissance Jean-Guyot 1850 ».
pub fn title_from_file(file: &str) -> String {
    let name = file.rsplit(['/', '\\']).next().unwrap_or(file);
    let stem = name.rsplit_once('.').map(|(s, _)| s).unwrap_or(name);
    stem.replace('_', " ").trim().to_string()
}

impl<'a> Individual<'a> {
    /// Familles dont la personne est l'un des parents, avec le conjoint.
    pub fn unions(&self, g: &'a Gedcom) -> Vec<(String, Option<String>)> {
        self.fams()
            .into_iter()
            .filter_map(|f| g.family(f))
            .map(|f: Family| {
                let spouse = [f.husband(), f.wife()].into_iter().flatten().find(|x| *x != self.xref());
                (f.xref().to_string(), spouse.map(str::to_string))
            })
            .collect()
    }
}

/// Événement à ajouter avec [`Gedcom::add_event`].
#[derive(Debug, Clone, Default)]
pub struct NewEvent {
    pub tag: String,
    pub detail: String,
    /// Valeur GEDCOM de la date, et sa phrase éventuelle (`PHRASE`).
    pub date: String,
    pub date_phrase: String,
    pub place: String,
    pub source: Option<String>,
}

/// Événements proposés à l'ajout : individuels, puis familiaux.
pub const INDIVIDUAL_EVENTS: &[&str] = &[
    "BIRT", "BAPM", "DEAT", "BURI", "CREM", "RESI", "OCCU", "CENS", "EDUC", "GRAD", "RETI", "EMIG", "IMMI", "NATU",
    "CONF", "FCOM", "WILL", "PROB", "EVEN",
];
pub const FAMILY_EVENTS: &[&str] = &["MARR", "ENGA", "MARB", "MARC", "DIV", "EVEN"];
