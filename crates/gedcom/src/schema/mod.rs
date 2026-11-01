//! Schéma GEDCOM 7 : quelles sous-structures sont permises sous chaque
//! structure, avec quelle cardinalité, et quel type de valeur elles portent.
//!
//! Les tables viennent du registre officiel de FamilySearch
//! (`GEDCOM-registries`, fichier `generated_files/g7validation.json`) et sont
//! générées dans [`data`] par `docs/schema/gen.py`. Les libellés français
//! sont ajoutés ici à la main.

#[rustfmt::skip]
mod data;

/// Nombre d'occurrences permises d'une sous-structure (`{0:1}`, `{1:M}`…).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Card {
    Optional,
    Required,
    Many,
    AtLeastOne,
}

impl Card {
    /// Au plus une occurrence.
    pub fn single(self) -> bool {
        matches!(self, Card::Optional | Card::Required)
    }

    /// Au moins une occurrence.
    pub fn required(self) -> bool {
        matches!(self, Card::Required | Card::AtLeastOne)
    }

    pub fn notation(self) -> &'static str {
        match self {
            Card::Optional => "{0:1}",
            Card::Required => "{1:1}",
            Card::Many => "{0:M}",
            Card::AtLeastOne => "{1:M}",
        }
    }
}

/// Type de la valeur portée par une structure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Payload {
    /// Pas de valeur.
    None,
    /// `Y` ou rien (événements).
    YOrEmpty,
    /// Pointeur vers un enregistrement de ce type (`INDI`, `FAM`…).
    Pointer(&'static str),
    /// Valeur énumérée (ou liste de valeurs séparées par des virgules).
    Enum {
        list: bool,
        values: &'static [&'static str],
    },
    Date,
    Age,
    Time,
    MediaType,
    Language,
    Text,
}

#[derive(Debug, Clone, Copy)]
pub struct Sub {
    pub tag: &'static str,
    def: usize,
    pub card: Card,
}

impl Sub {
    pub fn def(&self) -> &'static StructDef {
        &data::STRUCTS[self.def]
    }
}

/// Définition d'une structure GEDCOM 7.
#[derive(Debug)]
pub struct StructDef {
    /// Identifiant dans le registre (`record-INDI`, `INDI-NAME`, `DATE`…).
    pub id: &'static str,
    /// Libellé anglais du registre.
    pub label: &'static str,
    pub payload: Payload,
    pub subs: &'static [Sub],
}

impl StructDef {
    /// Sous-structure standard `tag`, si elle est permise ici.
    pub fn sub(&self, tag: &str) -> Option<&'static Sub> {
        // `subs` est généré trié par balise.
        self.subs.binary_search_by(|s| s.tag.cmp(tag)).ok().map(|i| &self.subs[i])
    }
}

/// Le niveau 0 du fichier (enregistrements, `HEAD`, `TRLR`).
pub fn root() -> &'static StructDef {
    &data::STRUCTS[0]
}

/// Définition d'un enregistrement de niveau 0 (`INDI`, `FAM`, `SOUR`…).
pub fn record(tag: &str) -> Option<&'static StructDef> {
    root().sub(tag).map(Sub::def)
}

/// Libellé français d'une structure, ou à défaut le libellé anglais du registre.
pub fn label_fr(tag: &str, def: &StructDef) -> &'static str {
    let by_id = match def.id {
        "record-INDI" => Some("Individu"),
        "record-FAM" => Some("Famille"),
        "record-SOUR" | "HEAD-SOUR" => Some("Source"),
        "record-REPO" => Some("Dépôt d'archives"),
        "record-OBJE" => Some("Média"),
        "record-SNOTE" => Some("Note partagée"),
        "record-SUBM" => Some("Auteur du fichier"),
        "FAM-HUSB" | "HUSB" => Some("Époux"),
        "FAM-WIFE" | "WIFE" => Some("Épouse"),
        "INDI-TITL" => Some("Titre de noblesse ou honorifique"),
        "TITL" => Some("Titre"),
        "NAME-TYPE" | "EXID-TYPE" | "TYPE" => Some("Type"),
        "FAMC-STAT" | "ord-STAT" => Some("Statut"),
        "FAMC-ADOP" => Some("Adopté par"),
        "SOUR-EVEN" | "DATA-EVEN" => Some("Événements enregistrés"),
        "SOUR-DATA" | "HEAD-SOUR-DATA" | "DATA" => Some("Données"),
        "PLAC-FORM" | "HEAD-PLAC-FORM" => Some("Format des lieux"),
        _ => None,
    };
    if let Some(l) = by_id {
        return l;
    }
    match tag {
        "ABBR" => "Abréviation",
        "ADDR" => "Adresse",
        "ADOP" => "Adoption",
        "ADR1" | "ADR2" | "ADR3" => "Ligne d'adresse (obsolète)",
        "AGE" => "Âge à l'événement",
        "AGNC" => "Organisme responsable",
        "ALIA" => "Alias (même personne)",
        "ANCI" => "Intérêt pour les ascendants",
        "ANUL" => "Annulation du mariage",
        "ASSO" => "Personne associée (témoin, parrain…)",
        "AUTH" => "Auteur",
        "BAPL" => "Baptême (SDJ)",
        "BAPM" => "Baptême",
        "BARM" => "Bar-mitsva",
        "BASM" => "Bat-mitsva",
        "BIRT" => "Naissance",
        "BLES" => "Bénédiction",
        "BURI" => "Inhumation",
        "CALN" => "Cote",
        "CAST" => "Caste",
        "CAUS" => "Cause",
        "CENS" => "Recensement",
        "CHAN" => "Dernière modification",
        "CHIL" => "Enfant",
        "CHR" => "Baptême (enfant)",
        "CHRA" => "Baptême (adulte)",
        "CITY" => "Ville",
        "CONF" => "Confirmation",
        "CONL" => "Confirmation (SDJ)",
        "CONT" => "Suite du texte",
        "COPR" => "Droits d'auteur",
        "CORP" => "Société",
        "CREA" => "Création",
        "CREM" => "Crémation",
        "CROP" => "Recadrage",
        "CTRY" => "Pays",
        "DATE" => "Date",
        "DEAT" => "Décès",
        "DESI" => "Intérêt pour les descendants",
        "DEST" => "Destinataire",
        "DIV" => "Divorce",
        "DIVF" => "Demande de divorce",
        "DSCR" => "Description physique",
        "EDUC" => "Études",
        "EMAIL" => "Courriel",
        "EMIG" => "Émigration",
        "ENDL" => "Dotation (SDJ)",
        "ENGA" => "Fiançailles",
        "EVEN" => "Événement",
        "EXID" => "Identifiant externe",
        "FACT" => "Fait",
        "FAM" => "Famille",
        "FAMC" => "Famille (en tant qu'enfant)",
        "FAMS" => "Famille (en tant que conjoint)",
        "FAX" => "Fax",
        "FCOM" => "Première communion",
        "FILE" => "Fichier",
        "FORM" => "Format",
        "GEDC" => "Version GEDCOM",
        "GIVN" => "Prénoms",
        "GRAD" => "Diplôme",
        "HEAD" => "En-tête",
        "HEIGHT" => "Hauteur (pixels)",
        "IDNO" => "Numéro d'identification",
        "IMMI" => "Immigration",
        "INDI" => "Individu",
        "INIL" => "Initiation (SDJ)",
        "LANG" => "Langue",
        "LATI" => "Latitude",
        "LEFT" => "Rognage à gauche",
        "LONG" => "Longitude",
        "MAP" => "Coordonnées",
        "MARB" => "Publication des bans",
        "MARC" => "Contrat de mariage",
        "MARL" => "Licence de mariage",
        "MARR" => "Mariage",
        "MARS" => "Convention matrimoniale",
        "MEDI" => "Support",
        "MIME" => "Type de média",
        "NAME" => "Nom",
        "NATI" => "Nationalité",
        "NATU" => "Naturalisation",
        "NCHI" => "Nombre d'enfants",
        "NICK" => "Surnom",
        "NMR" => "Nombre de mariages",
        "NO" => "N'a pas eu lieu",
        "NOTE" => "Note",
        "NPFX" => "Préfixe du nom (titre)",
        "NSFX" => "Suffixe du nom",
        "OBJE" => "Média",
        "OCCU" => "Profession",
        "ORDN" => "Ordination",
        "PAGE" => "Page / emplacement dans la source",
        "PEDI" => "Lien de filiation",
        "PHON" => "Téléphone",
        "PHRASE" => "Formulation libre",
        "PLAC" => "Lieu",
        "POST" => "Code postal",
        "PROB" => "Succession",
        "PROP" => "Propriété",
        "PUBL" => "Publication",
        "QUAY" => "Fiabilité",
        "REFN" => "Référence",
        "RELI" => "Religion",
        "REPO" => "Dépôt d'archives",
        "RESI" => "Résidence",
        "RESN" => "Restriction",
        "RETI" => "Retraite",
        "ROLE" => "Rôle",
        "SCHMA" => "Schéma des extensions",
        "SDATE" => "Date de tri",
        "SEX" => "Sexe",
        "SLGC" => "Scellement enfant (SDJ)",
        "SLGS" => "Scellement conjoints (SDJ)",
        "SNOTE" => "Note partagée",
        "SOUR" => "Source",
        "SPFX" => "Particule",
        "SSN" => "Numéro de sécurité sociale",
        "STAE" => "Département / région",
        "SUBM" => "Auteur du fichier",
        "SURN" => "Nom de famille",
        "TAG" => "Balise d'extension",
        "TEMP" => "Temple",
        "TEXT" => "Transcription",
        "TIME" => "Heure",
        "TOP" => "Rognage en haut",
        "TRAN" => "Traduction",
        "TRLR" => "Fin de fichier",
        "UID" => "Identifiant unique",
        "VERS" => "Version",
        "WIDTH" => "Largeur (pixels)",
        "WILL" => "Testament",
        "WWW" => "Site web",
        _ => def.label,
    }
}

/// Libellé français d'une valeur énumérée, quand il est utile.
pub fn enum_label_fr(value: &str) -> Option<&'static str> {
    Some(match value {
        "M" => "Masculin",
        "F" => "Féminin",
        "X" => "Autre",
        "U" => "Inconnu",
        "BIRTH" => "Naissance / biologique",
        "ADOPTED" => "Adopté",
        "FOSTER" => "Placé (famille d'accueil)",
        "SEALING" => "Scellement (SDJ)",
        "OTHER" => "Autre",
        "AKA" => "Aussi connu sous",
        "IMMIGRANT" => "Nom d'immigrant",
        "MAIDEN" => "Nom de jeune fille",
        "MARRIED" => "Nom d'épouse",
        "PROFESSIONAL" => "Nom d'usage professionnel",
        "PROVEN" => "Prouvé",
        "CHALLENGED" => "Contesté",
        "DISPROVEN" => "Réfuté",
        "HUSB" => "Époux",
        "WIFE" => "Épouse",
        "BOTH" => "Les deux",
        "CHIL" => "Enfant",
        "CLERGY" => "Clergé",
        "FATH" => "Père",
        "MOTH" => "Mère",
        "FRIEND" => "Ami",
        "GODP" => "Parrain / marraine",
        "MULTIPLE" => "Naissance multiple",
        "NGHBR" => "Voisin",
        "OFFICIATOR" => "Officiant",
        "PARENT" => "Parent",
        "SPOU" => "Conjoint",
        "WITN" => "Témoin",
        "CONFIDENTIAL" => "Confidentiel",
        "LOCKED" => "Verrouillé",
        "PRIVACY" => "Privé",
        "0" => "Non fiable / estimé",
        "1" => "Fiabilité douteuse",
        "2" => "Source secondaire",
        "3" => "Source directe et primaire",
        "AUDIO" => "Audio",
        "BOOK" => "Livre",
        "CARD" => "Fiche",
        "ELECTRONIC" => "Électronique",
        "FICHE" => "Microfiche",
        "FILM" => "Microfilm",
        "MAGAZINE" => "Revue",
        "MANUSCRIPT" => "Manuscrit",
        "MAP" => "Carte",
        "NEWSPAPER" => "Journal",
        "PHOTO" => "Photo",
        "TOMBSTONE" => "Pierre tombale",
        "VIDEO" => "Vidéo",
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn substructures_sont_triees() {
        for d in data::STRUCTS {
            assert!(d.subs.windows(2).all(|w| w[0].tag < w[1].tag), "{} non trié", d.id);
        }
    }

    #[test]
    fn individu() {
        let indi = record("INDI").unwrap();
        assert_eq!(indi.id, "record-INDI");
        let birt = indi.sub("BIRT").unwrap();
        assert_eq!(birt.card, Card::Many);
        assert_eq!(birt.def().payload, Payload::YOrEmpty);
        assert!(birt.def().sub("DATE").unwrap().card.single());
        assert_eq!(indi.sub("FAMC").unwrap().def().payload, Payload::Pointer("FAM"));
        assert!(
            matches!(indi.sub("SEX").unwrap().def().payload, Payload::Enum { values, .. } if values.contains(&"F"))
        );
        assert!(indi.sub("HUSB").is_none());
    }
}
