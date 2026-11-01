//! Modèles d'actes pour la saisie guidée des sources (« data driven entry »).
//!
//! Inspiré des modèles de source de Family Historian 7 et de son module
//! Ancestral Sources : un modèle décrit un type d'acte (naissance, mariage,
//! recensement…), les champs de la citation (n° d'acte, vue…) et les
//! **rôles** des personnes citées (enfant, père, témoin…), avec les liens
//! familiaux qu'ils impliquent.
//!
//! Les modèles intégrés sont définis ici. Un fichier peut en ajouter d'autres
//! sous forme d'enregistrements d'extension GEDCOM 7 (`0 @T1@ _GR_TPL`), décrits
//! dans `docs/extensions.md` : rien n'est stocké à côté du `.ged`.

use crate::document::Gedcom;
use crate::node::Node;

/// Préfixe des URI de documentation des extensions GedRoots (déclarées dans `HEAD.SCHMA`).
pub const EXT_DOC: &str = "https://github.com/antoninguyot/gedroots/blob/main/docs/extensions.md";

/// Balises d'extension GedRoots utilisées par la saisie guidée.
pub const EXT_TAGS: &[&str] =
    &["_GR_TPL", "_GR_FLD", "_GR_ROLE", "_GR_REL", "_GR_PF", "_GR_PAGE", "_GR_EVT", "_GR_MULT"];

/// Données qu'on peut saisir pour une personne citée.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PersonField {
    Surname,
    Given,
    Sex,
    Age,
    Occupation,
    Residence,
    /// Lieu de naissance déclaré (recensements, registres matricules).
    Birthplace,
    /// Lien déclaré avec la personne principale (« fils », « épouse », « bisaïeul »…).
    Kinship,
    Note,
}

impl PersonField {
    pub const ALL: [PersonField; 9] = [
        PersonField::Surname,
        PersonField::Given,
        PersonField::Sex,
        PersonField::Age,
        PersonField::Occupation,
        PersonField::Residence,
        PersonField::Birthplace,
        PersonField::Kinship,
        PersonField::Note,
    ];

    pub fn code(self) -> &'static str {
        match self {
            PersonField::Surname => "SURN",
            PersonField::Given => "GIVN",
            PersonField::Sex => "SEX",
            PersonField::Age => "AGE",
            PersonField::Occupation => "OCCU",
            PersonField::Residence => "RESI",
            PersonField::Birthplace => "BIRP",
            PersonField::Kinship => "RELA",
            PersonField::Note => "NOTE",
        }
    }

    pub fn from_code(code: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|f| f.code() == code)
    }

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
            PersonField::Note => "Note",
        }
    }
}

/// Lien d'un rôle avec un autre rôle du même acte.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Relation {
    /// Porte l'événement de l'acte (l'enfant, le défunt, les époux, chaque recensé…).
    Principal,
    /// Père du rôle indiqué.
    FatherOf(String),
    /// Mère du rôle indiqué.
    MotherOf(String),
    /// Conjoint du rôle indiqué.
    SpouseOf(String),
    /// Membre d'un ménage : le lien réel (conjoint, enfant) est déduit du champ « Lien déclaré ».
    HouseholdOf(String),
    /// Simple participant (témoin, déclarant, parrain…) : associé à l'événement.
    Witness,
}

impl Relation {
    fn encode(&self) -> String {
        match self {
            Relation::Principal => "PRIN".into(),
            Relation::FatherOf(r) => format!("FATH {r}"),
            Relation::MotherOf(r) => format!("MOTH {r}"),
            Relation::SpouseOf(r) => format!("SPOU {r}"),
            Relation::HouseholdOf(r) => format!("HOUS {r}"),
            Relation::Witness => "ASSO".into(),
        }
    }

    fn decode(s: &str) -> Option<Self> {
        let mut it = s.split_whitespace();
        let code = it.next()?;
        let arg = it.next().map(str::to_string);
        Some(match (code, arg) {
            ("PRIN", _) => Relation::Principal,
            ("ASSO", _) => Relation::Witness,
            ("FATH", Some(r)) => Relation::FatherOf(r),
            ("MOTH", Some(r)) => Relation::MotherOf(r),
            ("SPOU", Some(r)) => Relation::SpouseOf(r),
            ("HOUS", Some(r)) => Relation::HouseholdOf(r),
            _ => return None,
        })
    }
}

/// Un rôle de personne citée dans l'acte.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoleDef {
    /// Clé unique dans le modèle (`enfant`, `pere`…).
    pub key: String,
    pub label: String,
    /// Rôle GEDCOM 7 (`CHIL`, `FATH`, `MOTH`, `HUSB`, `WIFE`, `WITN`, `GODP`, `OTHER`…).
    pub role: String,
    /// Précision du rôle (`ROLE … / PHRASE`), ex. « Père de l'époux ».
    pub phrase: Option<String>,
    pub relation: Relation,
    /// Sexe imposé par le rôle (`M`, `F`).
    pub sex: Option<String>,
    pub fields: Vec<PersonField>,
    /// Le rôle peut apparaître plusieurs fois (témoins, membres du ménage).
    pub multiple: bool,
    /// Présent par défaut dans un nouvel acte.
    pub default: bool,
}

/// Un champ propre à la citation (n° d'acte, vue, cote…).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldDef {
    pub key: String,
    pub label: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Template {
    /// `NAISSANCE`… pour les modèles intégrés, identifiant de l'enregistrement `_GR_TPL` sinon.
    pub id: String,
    pub name: String,
    /// Balise de l'événement créé (`BIRT`, `MARR`, `CENS`, `EVEN`…).
    pub event: String,
    /// `TYPE` de l'événement pour `EVEN`/`FACT` (ex. « Registre matricule »).
    pub event_type: Option<String>,
    /// Formule de la référence de la citation (`PAGE`), avec `{clé}` remplacé par le champ.
    pub page: String,
    pub fields: Vec<FieldDef>,
    pub roles: Vec<RoleDef>,
    /// Le modèle est-il défini dans le fichier (et non intégré à GedRoots) ?
    pub custom: bool,
}

impl Template {
    pub fn role(&self, key: &str) -> Option<&RoleDef> {
        self.roles.iter().find(|r| r.key == key)
    }

    /// L'événement est-il familial (porté par l'enregistrement `FAM` des époux) ?
    pub fn is_family_event(&self) -> bool {
        matches!(self.event.as_str(), "MARR" | "MARB" | "MARC" | "MARL" | "ENGA" | "DIV" | "DIVF" | "ANUL")
    }

    /// Libellé de l'événement.
    pub fn event_label(&self) -> String {
        self.event_type
            .clone()
            .or_else(|| crate::model::event_label(&self.event).map(str::to_string))
            .unwrap_or_else(|| self.event.clone())
    }

    /// Calcule la référence (`PAGE`) à partir des champs de la citation.
    pub fn format_page(&self, values: &[(String, String)]) -> String {
        let get = |k: &str| values.iter().find(|(key, _)| key == k).map(|(_, v)| v.trim()).unwrap_or("");
        // Chaque segment séparé par une virgule n'est gardé que si tous ses champs sont remplis.
        let mut parts = Vec::new();
        for seg in self.page.split(',') {
            let mut out = String::new();
            let mut rest = seg;
            let mut complete = true;
            while let Some(a) = rest.find('{') {
                out.push_str(&rest[..a]);
                let Some(b) = rest[a..].find('}') else { break };
                let v = get(&rest[a + 1..a + b]);
                if v.is_empty() {
                    complete = false;
                }
                out.push_str(v);
                rest = &rest[a + b + 1..];
            }
            out.push_str(rest);
            if complete && !out.trim().is_empty() {
                parts.push(out.trim().to_string());
            }
        }
        parts.join(", ")
    }

    // ------------------------------------------------------------------
    // Stockage en extension GEDCOM
    // ------------------------------------------------------------------

    /// Enregistrement `0 @xref@ _GR_TPL` décrivant ce modèle.
    pub fn to_node(&self, xref: &str) -> Node {
        let mut n = Node::new("_GR_TPL").with_xref(xref);
        n.children.push(Node::with_value("NAME", &self.name));
        let mut ev = Node::with_value("_GR_EVT", &self.event);
        if let Some(t) = &self.event_type {
            ev.children.push(Node::with_value("TYPE", t));
        }
        n.children.push(ev);
        n.children.push(Node::with_value("_GR_PAGE", &self.page));
        for f in &self.fields {
            n.children.push(Node::with_value("_GR_FLD", &f.key).push(Node::with_value("NAME", &f.label)));
        }
        for r in &self.roles {
            let mut rn = Node::with_value("_GR_ROLE", &r.key).push(Node::with_value("NAME", &r.label));
            let mut role = Node::with_value("ROLE", &r.role);
            if let Some(p) = &r.phrase {
                role.children.push(Node::with_value("PHRASE", p));
            }
            rn.children.push(role);
            rn.children.push(Node::with_value("_GR_REL", r.relation.encode()));
            if let Some(s) = &r.sex {
                rn.children.push(Node::with_value("SEX", s));
            }
            let pf: Vec<&str> = r.fields.iter().map(|f| f.code()).collect();
            rn.children.push(Node::with_value("_GR_PF", pf.join(" ")));
            if r.multiple || r.default {
                let flags = [(r.multiple, "MULTIPLE"), (r.default, "DEFAULT")]
                    .into_iter()
                    .filter(|(on, _)| *on)
                    .map(|(_, f)| f)
                    .collect::<Vec<_>>()
                    .join(" ");
                rn.children.push(Node::with_value("_GR_MULT", flags));
            }
            n.children.push(rn);
        }
        n
    }

    /// Lit un enregistrement `_GR_TPL`.
    pub fn from_node(n: &Node) -> Result<Self, String> {
        if n.tag != "_GR_TPL" {
            return Err(format!("{} n'est pas un modèle _GR_TPL", n.tag));
        }
        let id = n.xref.clone().ok_or("un modèle _GR_TPL doit avoir un identifiant")?;
        let ev = n.child("_GR_EVT").ok_or("_GR_EVT manquant (balise de l'événement)")?;
        let mut t = Template {
            id,
            name: n.child_val("NAME").unwrap_or("Modèle sans nom").to_string(),
            event: ev.val().to_string(),
            event_type: ev.child_val("TYPE").map(str::to_string),
            page: n.child_val("_GR_PAGE").unwrap_or("").to_string(),
            fields: Vec::new(),
            roles: Vec::new(),
            custom: true,
        };
        for f in n.children_with("_GR_FLD") {
            t.fields
                .push(FieldDef { key: f.val().to_string(), label: f.child_val("NAME").unwrap_or(f.val()).to_string() });
        }
        for r in n.children_with("_GR_ROLE") {
            let role = r.child("ROLE");
            let flags = r.child_val("_GR_MULT").unwrap_or("");
            t.roles.push(RoleDef {
                key: r.val().to_string(),
                label: r.child_val("NAME").unwrap_or(r.val()).to_string(),
                role: role.map(|x| x.val()).unwrap_or("OTHER").to_string(),
                phrase: role.and_then(|x| x.child_val("PHRASE")).map(str::to_string),
                relation: r.child_val("_GR_REL").and_then(Relation::decode).unwrap_or(Relation::Witness),
                sex: r.child_val("SEX").map(str::to_string),
                fields: r
                    .child_val("_GR_PF")
                    .unwrap_or("SURN GIVN")
                    .split_whitespace()
                    .filter_map(PersonField::from_code)
                    .collect(),
                multiple: flags.contains("MULTIPLE"),
                default: flags.contains("DEFAULT"),
            });
        }
        if t.event.is_empty() {
            return Err("_GR_EVT vide".into());
        }
        Ok(t)
    }
}

// ----------------------------------------------------------------------
// Modèles intégrés
// ----------------------------------------------------------------------

use PersonField::*;

fn role(key: &str, label: &str, gedcom_role: &str, relation: Relation, fields: &[PersonField]) -> RoleDef {
    RoleDef {
        key: key.into(),
        label: label.into(),
        role: gedcom_role.into(),
        phrase: None,
        relation,
        sex: None,
        fields: fields.to_vec(),
        multiple: false,
        default: true,
    }
}

impl RoleDef {
    fn sex(mut self, s: &str) -> Self {
        self.sex = Some(s.into());
        self
    }
    fn phrase(mut self, p: &str) -> Self {
        self.phrase = Some(p.into());
        self
    }
    fn multiple(mut self) -> Self {
        self.multiple = true;
        self
    }
    fn optional(mut self) -> Self {
        self.default = false;
        self
    }
}

fn fields(list: &[(&str, &str)]) -> Vec<FieldDef> {
    list.iter().map(|(k, l)| FieldDef { key: k.to_string(), label: l.to_string() }).collect()
}

fn father(of: &str) -> Relation {
    Relation::FatherOf(of.into())
}
fn mother(of: &str) -> Relation {
    Relation::MotherOf(of.into())
}

const PARENT: &[PersonField] = &[Surname, Given, Age, Occupation, Residence];
const WITNESS: &[PersonField] = &[Surname, Given, Age, Occupation, Residence, Kinship];

/// Modèles fournis avec GedRoots, pensés pour l'état civil et les registres français.
pub fn builtin_templates() -> Vec<Template> {
    let acte = fields(&[("numero", "N° d'acte"), ("vue", "Vue"), ("folio", "Folio")]);
    let page = "Acte n° {numero}, vue {vue}, f° {folio}".to_string();
    let tpl = |id: &str, name: &str, event: &str, roles: Vec<RoleDef>| Template {
        id: id.into(),
        name: name.into(),
        event: event.into(),
        event_type: None,
        page: page.clone(),
        fields: acte.clone(),
        roles,
        custom: false,
    };
    let witness = || role("temoin", "Témoin", "WITN", Relation::Witness, WITNESS).multiple().optional();
    vec![
        tpl(
            "NAISSANCE",
            "Naissance",
            "BIRT",
            vec![
                role("enfant", "Enfant", "CHIL", Relation::Principal, &[Surname, Given, Sex]),
                role("pere", "Père", "FATH", father("enfant"), PARENT).sex("M"),
                role("mere", "Mère", "MOTH", mother("enfant"), PARENT).sex("F"),
                role("declarant", "Déclarant", "OTHER", Relation::Witness, WITNESS).phrase("Déclarant").optional(),
                witness(),
            ],
        ),
        tpl(
            "BAPTEME",
            "Baptême",
            "BAPM",
            vec![
                role("enfant", "Baptisé(e)", "CHIL", Relation::Principal, &[Surname, Given, Sex]),
                role("pere", "Père", "FATH", father("enfant"), PARENT).sex("M"),
                role("mere", "Mère", "MOTH", mother("enfant"), PARENT).sex("F"),
                role("parrain", "Parrain", "GODP", Relation::Witness, WITNESS).sex("M").phrase("Parrain"),
                role("marraine", "Marraine", "GODP", Relation::Witness, WITNESS).sex("F").phrase("Marraine"),
                role("pretre", "Officiant", "OFFICIATOR", Relation::Witness, &[Surname, Given, Occupation]).optional(),
            ],
        ),
        tpl(
            "MARIAGE",
            "Mariage",
            "MARR",
            vec![
                role(
                    "epoux",
                    "Époux",
                    "HUSB",
                    Relation::Principal,
                    &[Surname, Given, Age, Occupation, Residence, Birthplace],
                )
                .sex("M"),
                role(
                    "epouse",
                    "Épouse",
                    "WIFE",
                    Relation::Principal,
                    &[Surname, Given, Age, Occupation, Residence, Birthplace],
                )
                .sex("F"),
                role("pere_epoux", "Père de l'époux", "FATH", father("epoux"), PARENT)
                    .sex("M")
                    .phrase("Père de l'époux"),
                role("mere_epoux", "Mère de l'époux", "MOTH", mother("epoux"), PARENT)
                    .sex("F")
                    .phrase("Mère de l'époux"),
                role("pere_epouse", "Père de l'épouse", "FATH", father("epouse"), PARENT)
                    .sex("M")
                    .phrase("Père de l'épouse"),
                role("mere_epouse", "Mère de l'épouse", "MOTH", mother("epouse"), PARENT)
                    .sex("F")
                    .phrase("Mère de l'épouse"),
                witness(),
            ],
        ),
        tpl(
            "DECES",
            "Décès",
            "DEAT",
            vec![
                role(
                    "defunt",
                    "Défunt(e)",
                    "PRIN",
                    Relation::Principal,
                    &[Surname, Given, Sex, Age, Occupation, Residence, Birthplace],
                )
                .phrase("Défunt"),
                role(
                    "conjoint",
                    "Conjoint",
                    "SPOU",
                    Relation::SpouseOf("defunt".into()),
                    &[Surname, Given, Occupation],
                )
                .optional(),
                role("pere", "Père", "FATH", father("defunt"), PARENT).sex("M").optional(),
                role("mere", "Mère", "MOTH", mother("defunt"), PARENT).sex("F").optional(),
                role("declarant", "Déclarant", "OTHER", Relation::Witness, WITNESS).phrase("Déclarant").multiple(),
            ],
        ),
        tpl(
            "INHUMATION",
            "Inhumation",
            "BURI",
            vec![
                role("defunt", "Défunt(e)", "PRIN", Relation::Principal, &[Surname, Given, Sex, Age, Residence])
                    .phrase("Défunt"),
                role("conjoint", "Conjoint", "SPOU", Relation::SpouseOf("defunt".into()), &[Surname, Given]).optional(),
                witness(),
            ],
        ),
        Template {
            id: "RECENSEMENT".into(),
            name: "Recensement".into(),
            event: "CENS".into(),
            event_type: None,
            page: "Vue {vue}, page {page}, ménage n° {menage}".into(),
            fields: fields(&[("vue", "Vue"), ("page", "Page"), ("menage", "N° de ménage"), ("rue", "Rue / quartier")]),
            roles: vec![
                role(
                    "chef",
                    "Chef de ménage",
                    "PRIN",
                    Relation::Principal,
                    &[Surname, Given, Sex, Age, Occupation, Birthplace],
                )
                .phrase("Chef de ménage"),
                role(
                    "membre",
                    "Membre du ménage",
                    "OTHER",
                    Relation::HouseholdOf("chef".into()),
                    &[Surname, Given, Sex, Age, Kinship, Occupation, Birthplace],
                )
                .multiple(),
            ],
            custom: false,
        },
        Template {
            id: "MATRICULE".into(),
            name: "Registre matricule".into(),
            event: "EVEN".into(),
            event_type: Some("Registre matricule".into()),
            page: "Classe {classe}, matricule n° {matricule}, vue {vue}".into(),
            fields: fields(&[("classe", "Classe"), ("matricule", "N° matricule"), ("vue", "Vue")]),
            roles: vec![
                role(
                    "conscrit",
                    "Conscrit",
                    "PRIN",
                    Relation::Principal,
                    &[Surname, Given, Occupation, Residence, Birthplace, Note],
                )
                .sex("M")
                .phrase("Conscrit"),
                role("pere", "Père", "FATH", father("conscrit"), &[Surname, Given, Residence]).sex("M"),
                role("mere", "Mère", "MOTH", mother("conscrit"), &[Surname, Given, Residence]).sex("F"),
            ],
            custom: false,
        },
    ]
}

impl Gedcom {
    /// Modèles disponibles : intégrés puis définis dans le fichier (`_GR_TPL`).
    pub fn templates(&self) -> Vec<Template> {
        let mut v = builtin_templates();
        v.extend(self.records.iter().filter(|r| r.tag == "_GR_TPL").filter_map(|r| Template::from_node(r).ok()));
        v
    }

    /// Modèle associé à une source (`1 _GR_TPL` sur l'enregistrement `SOUR`).
    pub fn source_template(&self, source: &str) -> Option<Template> {
        let id = self.get(source)?.child_val("_GR_TPL")?;
        let id = id.trim_matches('@');
        self.templates().into_iter().find(|t| t.id == id)
    }

    /// Copie un modèle dans le fichier pour le personnaliser. Renvoie son identifiant.
    pub fn store_template(&mut self, template: &Template) -> String {
        let xref = self.next_free_xref("T");
        let mut t = template.clone();
        t.name = if template.custom { template.name.clone() } else { format!("{} (personnalisé)", template.name) };
        self.records.push(t.to_node(&xref));
        self.declare_extensions();
        self.reindex();
        xref
    }

    /// Prochain identifiant libre avec ce préfixe (pour les enregistrements d'extension).
    pub(crate) fn next_free_xref(&self, prefix: &str) -> String {
        let mut n = 1u64;
        loop {
            let x = format!("{prefix}{n}");
            if self.get(&x).is_none() {
                return x;
            }
            n += 1;
        }
    }

    /// Déclare les balises d'extension GedRoots dans `HEAD.SCHMA` (exigé par GEDCOM 7).
    pub fn declare_extensions(&mut self) {
        let schma = self.header.ensure_child("SCHMA");
        for tag in EXT_TAGS {
            if !schma.children.iter().any(|c| c.tag == "TAG" && c.val().split_whitespace().next() == Some(tag)) {
                let anchor = tag.to_lowercase();
                schma.children.push(Node::with_value("TAG", format!("{tag} {EXT_DOC}#{anchor}")));
            }
        }
        // SCHMA doit suivre GEDC dans l'en-tête.
        if let (Some(gi), Some(si)) = (
            self.header.children.iter().position(|c| c.tag == "GEDC"),
            self.header.children.iter().position(|c| c.tag == "SCHMA"),
        ) && si != gi + 1
        {
            let s = self.header.children.remove(si);
            let gi = self.header.children.iter().position(|c| c.tag == "GEDC").unwrap();
            self.header.children.insert(gi + 1, s);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_roundtrip_through_gedcom() {
        for t in builtin_templates() {
            let node = t.to_node("T1");
            let text = crate::parse::node_to_string(&node);
            let back = Template::from_node(&crate::parse::node_from_string(&text).unwrap()).unwrap();
            assert_eq!(Template { id: t.id.clone(), custom: false, ..back }, t, "{}", t.name);
        }
    }

    #[test]
    fn page_formula_skips_empty_segments() {
        let t = &builtin_templates()[0];
        let v = |a: &str, b: &str| vec![("numero".to_string(), a.to_string()), ("vue".to_string(), b.to_string())];
        assert_eq!(t.format_page(&v("112", "37")), "Acte n° 112, vue 37");
        assert_eq!(t.format_page(&v("112", "")), "Acte n° 112");
        assert_eq!(t.format_page(&v("", "")), "");
    }
}
