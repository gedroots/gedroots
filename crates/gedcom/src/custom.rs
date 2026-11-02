//! Réglages du fichier qui touchent aux données : format des lieux et types
//! d'événements personnalisés.
//!
//! - Format des lieux : `HEAD.PLAC.FORM` (standard), liste des juridictions de
//!   la plus précise à la plus large. Le changer peut réécrire tous les `PLAC`
//!   du fichier (réordonner, ajouter, retirer, renommer une juridiction).
//! - Types d'événements : un type personnalisé est un `EVEN` standard avec un
//!   `TYPE`. Ils sont déduits du fichier ; seuls ceux qu'on déclare avant de
//!   s'en servir sont écrits dans l'en-tête (`HEAD._GR_EVEN`, voir docs/extensions.md).

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

/// Un type d'événement personnalisé (`EVEN` + `TYPE`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EventType {
    pub label: String,
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
        let mut add = |label: &str, family: bool, used: bool, declared: bool| {
            let label = label.trim();
            if label.is_empty() {
                return;
            }
            match out.iter_mut().find(|t| t.family == family && t.label == label) {
                Some(t) => {
                    t.uses += usize::from(used);
                    t.declared |= declared;
                }
                None => out.push(EventType { label: label.into(), family, uses: usize::from(used), declared }),
            }
        };
        for r in &self.records {
            let family = match r.tag.as_str() {
                "INDI" => false,
                "FAM" => true,
                _ => continue,
            };
            for e in r.children_with("EVEN") {
                if let Some(t) = e.child_val("TYPE") {
                    add(t, family, true, false);
                }
            }
        }
        for d in self.header.children_with(EVENT_TYPE_TAG) {
            add(d.val(), d.child_val(RECORD_TAG) == Some("FAM"), false, true);
        }
        out.sort_by_cached_key(|t| (sort_key(&t.label), t.family));
        out
    }

    /// Renomme un type d'événement partout (événements et déclaration). Renommer vers
    /// un type existant les fusionne. Renvoie le nombre d'événements modifiés.
    pub fn rename_event_type(&mut self, family: bool, old: &str, new: &str) -> usize {
        let new = new.trim();
        if new.is_empty() || new == old {
            return 0;
        }
        let tag = if family { "FAM" } else { "INDI" };
        let mut count = 0;
        let typed = |e: &Node| e.tag == "EVEN" && e.child_val("TYPE").is_some_and(|t| t.trim() == old);
        for r in self.records_where_mut(|r| r.tag == tag && r.children.iter().any(typed)) {
            for e in r.children.iter_mut().filter(|e| typed(e)) {
                e.set_child_val("TYPE", new);
                count += 1;
            }
            r.touch();
        }
        let declared = self.declared_index(family, old);
        if let Some(i) = declared {
            if self.declared_index(family, new).is_some() {
                self.header.children.remove(i);
            } else {
                self.header.children[i].value = Some(new.to_string());
            }
        }
        count
    }

    fn declared_index(&self, family: bool, label: &str) -> Option<usize> {
        self.header.children.iter().position(|c| {
            c.tag == EVENT_TYPE_TAG && c.val().trim() == label && (c.child_val(RECORD_TAG) == Some("FAM")) == family
        })
    }

    /// Déclare un type d'événement pour pouvoir le choisir avant de s'en servir.
    /// Renvoie `false` s'il existait déjà (dans le fichier ou l'en-tête).
    pub fn declare_event_type(&mut self, family: bool, label: &str) -> bool {
        let label = label.trim();
        if label.is_empty() || self.event_types().iter().any(|t| t.family == family && t.label == label) {
            return false;
        }
        self.declare_extension(EVENT_TYPE_TAG, EVENT_TYPE_URI);
        if family {
            self.declare_extension(RECORD_TAG, EVENT_TYPE_URI);
        }
        let mut n = Node::with_value(EVENT_TYPE_TAG, label);
        if family {
            n.children.push(Node::with_value(RECORD_TAG, "FAM"));
        }
        self.header.insert_child(n);
        true
    }

    /// Retire la déclaration d'un type (les événements du fichier ne changent pas).
    pub fn undeclare_event_type(&mut self, family: bool, label: &str) {
        if let Some(i) = self.declared_index(family, label) {
            self.header.children.remove(i);
        }
        if self.header.child(EVENT_TYPE_TAG).is_none() {
            self.undeclare_extension(EVENT_TYPE_TAG);
        }
        if !self.header.children_with(EVENT_TYPE_TAG).any(|d| d.child(RECORD_TAG).is_some()) {
            self.undeclare_extension(RECORD_TAG);
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

        assert!(g.declare_event_type(true, "Bans"));
        assert!(!g.declare_event_type(true, "Bans"));
        assert!(!g.declare_event_type(true, "Contrat de mariage"));
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
}
