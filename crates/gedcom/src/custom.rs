//! Réglages du fichier qui touchent aux données : format des lieux et types
//! d'événements personnalisés.
//!
//! - Format des lieux : `HEAD.PLAC.FORM` (standard), liste des juridictions de
//!   la plus précise à la plus large. Le changer peut réécrire tous les `PLAC`
//!   du fichier (réordonner, ajouter, retirer, renommer une juridiction).
//! - Types d'événements : un type personnalisé est un `EVEN` standard avec un
//!   `TYPE`, déduit du fichier, ou une balise d'extension choisie (`_MILT`).
//!   L'en-tête (`HEAD._GR_EVEN`, voir docs/extensions.md) déclare ceux qu'on
//!   n'utilise pas encore et ceux à balise d'extension, avec leur libellé.

use crate::document::Gedcom;
use crate::model::sort_key;
use crate::node::Node;

/// Une juridiction du nouveau format, et celle de l'ancien format dont elle reprend les valeurs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Jurisdiction {
    pub name: String,
    /// Position dans l'ancien format (`None` : nouvelle juridiction, vide partout).
    pub from: Option<usize>,
}

/// Ce que la réécriture des lieux a fait.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PlaceRewrite {
    /// `PLAC` réécrits.
    pub rewritten: usize,
    /// `PLAC` laissés tels quels : nombre de juridictions différent de l'ancien format,
    /// ou format propre (`PLAC.FORM`).
    pub skipped: usize,
}

/// Un type d'événement personnalisé : un `EVEN` standard avec un `TYPE`, ou une
/// balise d'extension (`_MILT`) dont le libellé est déclaré dans l'en-tête.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EventType {
    pub label: String,
    /// Balise GEDCOM des événements : `EVEN` (le libellé est leur `TYPE`) ou une
    /// balise d'extension.
    pub tag: String,
    /// Événement familial (sous `FAM`) ou individuel (sous `INDI`).
    pub family: bool,
    /// Nombre d'événements de ce type dans le fichier.
    pub uses: usize,
    /// Déclaré dans l'en-tête (`HEAD._GR_EVEN`).
    pub declared: bool,
}

/// Déclaration d'un type d'événement (dans `HEAD`).
pub const EVENT_TYPE_TAG: &str = "_GR_EVEN";
pub const EVENT_TYPE_URI: &str = "https://github.com/antoninguyot/gedroots/blob/main/docs/extensions.md#_gr_even";
/// Sous `_GR_EVEN` : type réservé aux familles (`FAM`).
const RECORD_TAG: &str = "_GR_REC";
/// Sous `_GR_EVEN` : balise d'extension des événements de ce type (absente : `EVEN` + `TYPE`).
const TAG_TAG: &str = "_GR_TAG";

/// Événements standard familiaux, vers lesquels un type personnalisé peut être converti.
const FAMILY_STANDARD: &[&str] = &["MARR", "MARB", "MARC", "MARL", "ENGA", "DIV", "DIVF", "ANUL"];
/// Attributs : leur valeur est une précision, pas un type d'événement.
const ATTRIBUTES: &[&str] = &["RESI", "OCCU", "EDUC", "RELI", "TITL", "NATI", "FACT"];

fn record_tag(family: bool) -> &'static str {
    if family { "FAM" } else { "INDI" }
}

fn is_standard_event(family: bool, tag: &str) -> bool {
    tag != "EVEN"
        && !ATTRIBUTES.contains(&tag)
        && crate::model::EVENT_TAGS.iter().any(|(t, _)| *t == tag)
        && FAMILY_STANDARD.contains(&tag) == family
}

/// Balise saisie pour un type personnalisé, mise en forme : `EVEN` (ou vide), une
/// balise d'extension (`_MILT`) ou un événement standard du même enregistrement
/// (`MARC` pour un type familial…), vers lequel le type sera converti.
pub fn normalize_event_tag(family: bool, raw: &str) -> Result<String, String> {
    let t = raw.trim().to_uppercase();
    if t.is_empty() || t == "EVEN" {
        return Ok("EVEN".into());
    }
    let extension = t.len() > 1
        && t.starts_with('_')
        && t.chars().all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_');
    if extension || is_standard_event(family, &t) {
        return Ok(t);
    }
    Err(format!("« {} » n'est pas une balise d'événement (EVEN, _XXX ou une balise standard)", raw.trim()))
}

fn split_place(v: &str) -> Vec<&str> {
    v.split(',').map(str::trim).collect()
}

impl Gedcom {
    /// Réécrit le format des lieux. Sans changement de l'ordre ni des juridictions
    /// reprises, seul l'en-tête change.
    pub fn set_place_format(&mut self, new: &[Jurisdiction]) -> PlaceRewrite {
        let old_len = self.place_form().len();
        let names: Vec<String> = new.iter().map(|j| j.name.trim().to_string()).collect();
        if names.iter().all(String::is_empty) {
            self.header.children.retain(|c| c.tag != "PLAC");
        } else {
            let plac = self.header.ensure_child("PLAC");
            plac.set_child_val("FORM", &names.join(", "));
        }
        let identity = new.len() == old_len && new.iter().enumerate().all(|(i, j)| j.from == Some(i));
        let mut out = PlaceRewrite::default();
        if identity || old_len == 0 {
            return out;
        }
        let rewritable =
            |n: &Node| n.tag == "PLAC" && split_place(n.val()).len() == old_len && n.child("FORM").is_none();
        let filled = |n: &Node| n.tag == "PLAC" && !n.val().trim().is_empty();
        for r in &self.records {
            r.walk(&mut |n| out.skipped += usize::from(filled(n) && !rewritable(n)));
        }
        for r in self.records_where_mut(|r| r.any(&|n| filled(n) && rewritable(n))) {
            r.walk_mut(&mut |n| {
                if filled(n) && rewritable(n) {
                    let parts = split_place(n.val());
                    let v: Vec<&str> =
                        new.iter().map(|j| j.from.and_then(|k| parts.get(k).copied()).unwrap_or("")).collect();
                    n.value = Some(v.join(", "));
                    out.rewritten += 1;
                }
            });
            r.touch();
        }
        out
    }

    /// Types d'événements personnalisés : ceux du fichier et ceux déclarés, triés par libellé.
    pub fn event_types(&self) -> Vec<EventType> {
        let mut out: Vec<EventType> = Vec::new();
        for d in self.header.children_with(EVENT_TYPE_TAG) {
            let label = d.val().trim();
            let family = d.child_val(RECORD_TAG) == Some("FAM");
            if label.is_empty() || out.iter().any(|t| t.family == family && t.label == label) {
                continue;
            }
            let tag = d.child_val(TAG_TAG).map(str::trim).filter(|t| !t.is_empty()).unwrap_or("EVEN");
            out.push(EventType { label: label.into(), tag: tag.into(), family, uses: 0, declared: true });
        }
        for r in &self.records {
            let family = match r.tag.as_str() {
                "INDI" => false,
                "FAM" => true,
                _ => continue,
            };
            for e in &r.children {
                if e.tag == "EVEN" {
                    let Some(label) = e.child_val("TYPE").map(str::trim).filter(|l| !l.is_empty()) else { continue };
                    match out.iter_mut().find(|t| t.family == family && t.label == label) {
                        Some(t) => t.uses += 1,
                        None => out.push(EventType {
                            label: label.into(),
                            tag: "EVEN".into(),
                            family,
                            uses: 1,
                            declared: false,
                        }),
                    }
                } else if e.tag.starts_with('_')
                    && let Some(t) = out.iter_mut().find(|t| t.family == family && t.tag == e.tag)
                {
                    t.uses += 1;
                }
            }
        }
        out.sort_by_cached_key(|t| (sort_key(&t.label), t.family));
        out
    }

    /// Balises d'extension déclarées comme types d'événements, avec leur libellé :
    /// à passer à [`crate::set_custom_events`] pour qu'elles soient lues comme des événements.
    pub fn custom_event_tags(&self) -> Vec<(String, String)> {
        let mut out: Vec<(String, String)> = Vec::new();
        for d in self.header.children_with(EVENT_TYPE_TAG) {
            if let Some(tag) = d.child_val(TAG_TAG).map(str::trim).filter(|t| t.starts_with('_'))
                && !out.iter().any(|(t, _)| t == tag)
            {
                out.push((tag.to_string(), d.val().trim().to_string()));
            }
        }
        out
    }

    /// Change la balise des événements d'un type (`from_tag`, `from_label`) vers
    /// `to_tag` ; avec `EVEN`, le libellé `to_label` va dans `TYPE`. Renvoie le nombre
    /// d'événements modifiés.
    fn retag_events(&mut self, family: bool, from_tag: &str, from_label: &str, to_tag: &str, to_label: &str) -> usize {
        let rec = record_tag(family);
        let of_type = |e: &Node| {
            if from_tag == "EVEN" {
                e.tag == "EVEN" && e.child_val("TYPE").is_some_and(|t| t.trim() == from_label)
            } else {
                e.tag == from_tag
            }
        };
        let standard = to_tag != "EVEN" && !to_tag.starts_with('_');
        let mut count = 0;
        for r in self.records_where_mut(|r| r.tag == rec && r.children.iter().any(of_type)) {
            for e in r.children.iter_mut().filter(|e| of_type(e)) {
                match (from_tag == "EVEN", to_tag == "EVEN") {
                    (true, true) => e.set_child_val("TYPE", to_label),
                    (true, false) => {
                        if let Some(i) = e.children.iter().position(|c| c.tag == "TYPE") {
                            e.children.remove(i);
                        }
                    }
                    (false, true) => e.children.insert(0, Node::with_value("TYPE", to_label)),
                    (false, false) => {}
                }
                e.tag = to_tag.to_string();
                // « 1 MARC Y » : un événement standard sans détail a eu lieu.
                if standard && e.children.is_empty() && e.value.is_none() {
                    e.value = Some("Y".into());
                } else if !standard && e.value.as_deref() == Some("Y") {
                    e.value = None;
                }
                count += 1;
            }
            r.touch();
        }
        count
    }

    /// Renomme un type d'événement partout (événements et déclaration). Renommer vers
    /// un type existant les fusionne (ses événements prennent la balise de ce type).
    /// Renvoie le nombre d'événements modifiés.
    pub fn rename_event_type(&mut self, family: bool, old: &str, new: &str) -> usize {
        let new = new.trim();
        if new.is_empty() || new == old {
            return 0;
        }
        let types = self.event_types();
        let find = |l: &str| types.iter().find(|t| t.family == family && t.label == l);
        let Some(cur) = find(old) else { return 0 };
        let target = find(new);
        let to_tag = target.map_or(cur.tag.as_str(), |t| t.tag.as_str());
        let count = if cur.tag == "EVEN" || to_tag != cur.tag {
            self.retag_events(family, &cur.tag, old, to_tag, new)
        } else {
            0
        };
        let (old_key, new_key) =
            (if cur.tag == "EVEN" { old } else { cur.tag.as_str() }, if to_tag == "EVEN" { new } else { to_tag });
        if old_key != new_key {
            self.rename_media_type(old_key, new_key);
        }
        if let Some(i) = self.declared_index(family, old) {
            if target.is_some() {
                self.header.children.remove(i);
            } else {
                self.header.children[i].value = Some(new.to_string());
            }
        }
        self.tidy_event_extensions();
        count
    }

    /// Change la balise GEDCOM d'un type : `EVEN` + `TYPE`, une balise d'extension
    /// (`_MILT`, le libellé est alors gardé dans la déclaration), ou un événement
    /// standard (le type disparaît au profit de l'événement standard). Tous les
    /// événements du type sont réécrits. Renvoie leur nombre.
    pub fn set_event_type_tag(&mut self, family: bool, label: &str, tag: &str) -> Result<usize, String> {
        let new = normalize_event_tag(family, tag)?;
        let types = self.event_types();
        let cur =
            types.iter().find(|t| t.family == family && t.label == label).ok_or("type d'événement introuvable")?;
        if cur.tag == new {
            return Ok(0);
        }
        if new.starts_with('_')
            && let Some(other) = types.iter().find(|t| t.family == family && t.tag == new)
        {
            return Err(format!("la balise {new} est déjà celle de « {} »", other.label));
        }
        let count = self.retag_events(family, &cur.tag, label, &new, label);
        let old_key = if cur.tag == "EVEN" { label } else { cur.tag.as_str() };
        let new_key = if new == "EVEN" { label } else { new.as_str() };
        self.rename_media_type(old_key, new_key);
        let declared = self.declared_index(family, label);
        if new.starts_with('_') {
            match declared {
                Some(i) => self.header.children[i].set_child_val(TAG_TAG, &new),
                None => self.push_declaration(family, label, &new),
            }
        } else if let Some(i) = declared {
            // `EVEN` utilisé : déduit du fichier, plus besoin de déclaration. Événement
            // standard : ce n'est plus un type personnalisé.
            if new != "EVEN" || count > 0 {
                self.header.children.remove(i);
            } else {
                self.header.children[i].children.retain(|c| c.tag != TAG_TAG);
            }
        }
        self.tidy_event_extensions();
        Ok(count)
    }

    fn declared_index(&self, family: bool, label: &str) -> Option<usize> {
        self.header.children.iter().position(|c| {
            c.tag == EVENT_TYPE_TAG && c.val().trim() == label && (c.child_val(RECORD_TAG) == Some("FAM")) == family
        })
    }

    /// Déclare un type d'événement pour pouvoir le choisir avant de s'en servir,
    /// avec sa balise (`EVEN` ou une balise d'extension). Renvoie `Ok(false)` s'il
    /// existait déjà (dans le fichier ou l'en-tête).
    pub fn declare_event_type(&mut self, family: bool, label: &str, tag: &str) -> Result<bool, String> {
        let label = label.trim();
        let tag = normalize_event_tag(family, tag)?;
        let types = self.event_types();
        if label.is_empty() || types.iter().any(|t| t.family == family && t.label == label) {
            return Ok(false);
        }
        if tag != "EVEN" && !tag.starts_with('_') {
            return Err(format!("{tag} est déjà un événement standard"));
        }
        if let Some(other) = types.iter().find(|t| t.family == family && t.tag == tag && tag != "EVEN") {
            return Err(format!("la balise {tag} est déjà celle de « {} »", other.label));
        }
        self.push_declaration(family, label, &tag);
        self.tidy_event_extensions();
        Ok(true)
    }

    fn push_declaration(&mut self, family: bool, label: &str, tag: &str) {
        let mut n = Node::with_value(EVENT_TYPE_TAG, label);
        if family {
            n.children.push(Node::with_value(RECORD_TAG, "FAM"));
        }
        if tag != "EVEN" {
            n.children.push(Node::with_value(TAG_TAG, tag));
        }
        self.header.insert_child(n);
    }

    /// Retire la déclaration d'un type (les événements du fichier ne changent pas).
    pub fn undeclare_event_type(&mut self, family: bool, label: &str) {
        if let Some(i) = self.declared_index(family, label) {
            self.header.children.remove(i);
        }
        self.tidy_event_extensions();
    }

    /// Déclare dans `SCHMA` les balises `_GR_EVEN` utilisées, et seulement elles.
    fn tidy_event_extensions(&mut self) {
        for sub in [EVENT_TYPE_TAG, RECORD_TAG, TAG_TAG] {
            let used =
                self.header.children_with(EVENT_TYPE_TAG).any(|d| sub == EVENT_TYPE_TAG || d.child(sub).is_some());
            if used {
                self.declare_extension(sub, EVENT_TYPE_URI);
            } else {
                self.undeclare_extension(sub);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const G: &str = "0 HEAD\n1 GEDC\n2 VERS 7.0\n1 PLAC\n2 FORM Lieudit, Commune, Code_Postal, Département, Région, Pays\n\
0 @I1@ INDI\n1 BIRT\n2 PLAC , Troyes, 10000, Aube, Grand Est, France\n1 DEAT\n2 PLAC Paris\n\
1 EVEN\n2 TYPE Baptême protestant\n1 EVEN\n2 TYPE Bapteme prot.\n\
0 @F1@ FAM\n1 EVEN\n2 TYPE Contrat de mariage\n2 PLAC , Troyes, 10000, Aube, Grand Est, France\n3 FORM a, b, c, d, e, f\n0 TRLR\n";

    fn j(name: &str, from: Option<usize>) -> Jurisdiction {
        Jurisdiction { name: name.into(), from }
    }

    #[test]
    fn rewrites_places_when_format_changes() {
        let mut g = Gedcom::parse(G).unwrap();
        // Sans lieu-dit ni région, et le code postal après le département.
        let r = g.set_place_format(&[
            j("Commune", Some(1)),
            j("Département", Some(3)),
            j("Code postal", Some(2)),
            j("Pays", Some(5)),
        ]);
        assert_eq!(r, PlaceRewrite { rewritten: 1, skipped: 2 });
        let out = g.to_gedcom_string();
        assert!(out.contains("2 FORM Commune, Département, Code postal, Pays\n"));
        assert!(out.contains("2 PLAC Troyes, Aube, 10000, France\n"));
        assert!(out.contains("2 PLAC Paris\n"));
        assert!(out.contains("2 PLAC , Troyes, 10000, Aube, Grand Est, France\n3 FORM a"));
    }

    #[test]
    fn short_labels_follow_the_format() {
        use crate::model::short_place_in;
        let f = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        let form = f(&["Commune", "Département", "Code postal", "Pays"]);
        assert_eq!(short_place_in("Troyes, Aube, 10000, France", &form), "Troyes (Aube)");
        assert_eq!(short_place_in("Troyes, , 10000, France", &form), "Troyes (France)");
        let form = f(&["Lieu-dit", "Ville", "County", "Country"]);
        assert_eq!(short_place_in("Mill Farm, Leeds, Yorkshire, England", &form), "Mill Farm, Leeds (Yorkshire)");
        // Format Ancestris, ou lieu qui ne suit pas le format : comme avant.
        let ancestris = f(&["Lieudit", "Commune", "Code_Postal", "Département", "Région", "Pays"]);
        assert_eq!(short_place_in(", Troyes, 10000, Aube, Grand Est, France", &ancestris), "Troyes (Aube)");
        assert_eq!(short_place_in("Paris, France", &form), "Paris, France");
    }

    #[test]
    fn renaming_a_jurisdiction_only_touches_the_header() {
        let mut g = Gedcom::parse(G).unwrap();
        let names = ["Lieu-dit", "Commune", "Code postal", "Département", "Région", "Pays"];
        let new: Vec<_> = names.iter().enumerate().map(|(i, n)| j(n, Some(i))).collect();
        assert_eq!(g.set_place_format(&new).rewritten, 0);
        assert_eq!(g.place_form()[0], "Lieu-dit");
        g.set_place_format(&[]);
        assert!(g.header.child("PLAC").is_none());
    }

    #[test]
    fn lists_renames_and_declares_event_types() {
        let mut g = Gedcom::parse(G).unwrap();
        let t = g.event_types();
        assert_eq!(
            t.iter().map(|t| (t.label.as_str(), t.family)).collect::<Vec<_>>(),
            vec![("Bapteme prot.", false), ("Baptême protestant", false), ("Contrat de mariage", true)]
        );
        // Fusion de deux types.
        assert_eq!(g.rename_event_type(false, "Bapteme prot.", "Baptême protestant"), 1);
        let t = g.event_types();
        assert_eq!(t[0].label, "Baptême protestant");
        assert_eq!(t[0].uses, 2);
        assert_eq!(t.len(), 2);

        assert_eq!(g.declare_event_type(true, "Bans", ""), Ok(true));
        assert_eq!(g.declare_event_type(true, "Bans", "EVEN"), Ok(false));
        assert_eq!(g.declare_event_type(true, "Contrat de mariage", ""), Ok(false));
        let out = g.to_gedcom_string();
        assert!(out.contains("1 _GR_EVEN Bans\n2 _GR_REC FAM\n"));
        assert!(out.contains("2 TAG _GR_EVEN https://"));
        let re = Gedcom::parse(&out).unwrap();
        assert!(re.event_types().iter().any(|t| t.label == "Bans" && t.family && t.declared && t.uses == 0));

        g.rename_event_type(true, "Bans", "Publication des bans");
        assert!(g.event_types().iter().any(|t| t.label == "Publication des bans" && t.declared));
        g.undeclare_event_type(true, "Publication des bans");
        assert!(g.header.child(EVENT_TYPE_TAG).is_none());
        assert!(g.header.child("SCHMA").is_none());
    }

    #[test]
    fn normalizes_tags() {
        assert_eq!(normalize_event_tag(false, " _milt "), Ok("_MILT".into()));
        assert_eq!(normalize_event_tag(false, ""), Ok("EVEN".into()));
        assert_eq!(normalize_event_tag(true, "marc"), Ok("MARC".into()));
        assert!(normalize_event_tag(false, "MARC").is_err());
        assert!(normalize_event_tag(false, "OCCU").is_err());
        assert!(normalize_event_tag(false, "_MI LT").is_err());
        assert!(normalize_event_tag(false, "_").is_err());
    }

    #[test]
    fn moves_a_type_to_an_extension_tag_and_back() {
        let mut g = Gedcom::parse(G).unwrap();
        g.set_media_naming(&crate::MediaNaming {
            root: String::new(),
            folders: vec![crate::MediaFolder {
                dir: "Militaire".into(),
                types: vec!["Baptême protestant".into()],
                template: String::new(),
            }],
        });
        assert_eq!(g.set_event_type_tag(false, "Baptême protestant", "_bapp"), Ok(1));
        let out = g.to_gedcom_string();
        assert!(out.contains("1 _GR_EVEN Baptême protestant\n2 _GR_TAG _BAPP\n"), "{out}");
        assert!(out.contains("2 TAG _GR_TAG https://"));
        assert!(out.contains("0 @I1@ INDI\n1 BIRT"));
        assert!(out.contains("1 _BAPP\n1 EVEN\n2 TYPE Bapteme prot."), "{out}");
        assert_eq!(g.media_naming().folders[0].types, vec!["_BAPP".to_string()]);
        let t = g.event_types();
        let b = t.iter().find(|t| t.label == "Baptême protestant").unwrap();
        assert_eq!((b.tag.as_str(), b.uses, b.declared), ("_BAPP", 1, true));
        assert_eq!(g.custom_event_tags(), vec![("_BAPP".to_string(), "Baptême protestant".to_string())]);
        // Une balise ne sert qu'à un type.
        assert!(g.set_event_type_tag(false, "Bapteme prot.", "_BAPP").is_err());
        assert!(g.declare_event_type(false, "Autre", "_BAPP").is_err());

        // Renommer : seule la déclaration change.
        assert_eq!(g.rename_event_type(false, "Baptême protestant", "Baptême réformé"), 0);
        assert!(g.to_gedcom_string().contains("1 _GR_EVEN Baptême réformé\n2 _GR_TAG _BAPP\n"));
        // Fusion avec un type EVEN : les événements reprennent EVEN + TYPE.
        assert_eq!(g.rename_event_type(false, "Baptême réformé", "Bapteme prot."), 1);
        let out = g.to_gedcom_string();
        assert!(!out.contains("_BAPP"), "{out}");
        assert!(!out.contains("_GR_EVEN"), "{out}");
        assert_eq!(out.matches("2 TYPE Bapteme prot.").count(), 2);
        assert_eq!(g.media_naming().folders[0].types, vec!["Bapteme prot.".to_string()]);
    }

    #[test]
    fn moves_a_type_to_a_standard_event() {
        let mut g = Gedcom::parse(G).unwrap();
        assert_eq!(g.set_event_type_tag(true, "Contrat de mariage", "MARC"), Ok(1));
        let out = g.to_gedcom_string();
        assert!(out.contains("0 @F1@ FAM\n1 MARC\n2 PLAC"), "{out}");
        assert!(!g.event_types().iter().any(|t| t.family));
        // Retour à EVEN pour un type déclaré inutilisé : la déclaration reste, sans balise.
        g.declare_event_type(false, "Service militaire", "_MILT").unwrap();
        assert_eq!(g.set_event_type_tag(false, "Service militaire", "EVEN"), Ok(0));
        let out = g.to_gedcom_string();
        assert!(out.contains("1 _GR_EVEN Service militaire\n"));
        assert!(!out.contains("_GR_TAG"));
    }

    #[test]
    fn extension_tags_are_events_once_registered() {
        let mut g = Gedcom::parse(
            "0 HEAD\n1 _GR_EVEN Service\n2 _GR_TAG _ZSRV\n0 @I1@ INDI\n1 _ZSRV\n2 DATE 1914\n1 _ZOTHER\n2 DATE 1915\n0 TRLR\n",
        )
        .unwrap();
        crate::set_custom_events(&g.custom_event_tags());
        let ev = g.events_of("I1");
        assert_eq!(ev.len(), 1);
        assert_eq!(ev[0].1.label(), "Service");
        let new = crate::relations::NewEvent { tag: "_ZSRV".into(), place: "Verdun".into(), ..Default::default() };
        g.add_event("I1", &new).unwrap();
        assert!(g.to_gedcom_string().contains("1 _ZSRV\n2 PLAC Verdun\n"));
        assert_eq!(g.event_types()[0].uses, 2);
    }
}
