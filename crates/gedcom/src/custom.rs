//! Réglages du fichier qui touchent aux données : format des lieux et types
//! d'événements personnalisés.
//!
//! - Format des lieux : `HEAD.PLAC.FORM` (standard), liste des juridictions de
//!   la plus précise à la plus large. Le changer peut réécrire tous les `PLAC`
//!   du fichier (réordonner, ajouter, retirer, renommer une juridiction).
//! - Types d'événements : un type personnalisé est un `EVEN` standard avec un
//!   `TYPE`. Ils sont déduits du fichier ; l'en-tête (`HEAD._GR_EVEN`, voir
//!   docs/extensions.md) ne déclare que ceux qu'on n'utilise pas encore et ceux
//!   qui ont un libellé affiché différent de leur `TYPE`.
//! - Import : les événements à balise d'extension d'autres logiciels (`_MILT`…)
//!   deviennent des `EVEN` + `TYPE` (voir [`Gedcom::migrate_extension_events`]).

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
    /// Valeur du `TYPE` des événements, telle qu'écrite dans le fichier.
    pub ty: String,
    /// Libellé affiché (`_GR_LABEL`), sinon le `TYPE`.
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
/// Sous `_GR_EVEN` : libellé affiché, quand il diffère du `TYPE`.
const LABEL_TAG: &str = "_GR_LABEL";

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
        let mut add = |ty: &str, family: bool, used: bool, label: Option<&str>| {
            let ty = ty.trim();
            if ty.is_empty() {
                return;
            }
            let i = match out.iter().position(|t| t.family == family && t.ty == ty) {
                Some(i) => i,
                None => {
                    out.push(EventType { ty: ty.into(), label: ty.into(), family, uses: 0, declared: false });
                    out.len() - 1
                }
            };
            let t = &mut out[i];
            t.uses += usize::from(used);
            if !used {
                t.declared = true;
                if let Some(l) = label.map(str::trim).filter(|l| !l.is_empty()) {
                    t.label = l.into();
                }
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
                    add(t, family, true, None);
                }
            }
        }
        for d in self.header.children_with(EVENT_TYPE_TAG) {
            add(d.val(), d.child_val(RECORD_TAG) == Some("FAM"), false, d.child_val(LABEL_TAG));
        }
        out.sort_by_cached_key(|t| (sort_key(&t.label), t.family));
        out
    }

    /// Libellés affichés des types qui en ont un (`(TYPE, libellé)`) : à passer à
    /// [`crate::set_event_labels`] pour que les événements s'affichent sous ce libellé.
    pub fn event_labels(&self) -> Vec<(String, String)> {
        self.event_types().into_iter().filter(|t| t.label != t.ty).map(|t| (t.ty, t.label)).collect()
    }

    /// Renomme un type d'événement partout (`TYPE` des événements et déclaration).
    /// Renommer vers un type existant les fusionne. Renvoie le nombre d'événements modifiés.
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
        self.rename_media_type(old, new);
        self.tidy_event_types();
        count
    }

    /// Change le libellé affiché d'un type (vide ou égal au `TYPE` : le `TYPE` est affiché).
    /// Les événements ne changent pas : le libellé est rangé dans `HEAD._GR_EVEN`.
    pub fn set_event_type_label(&mut self, family: bool, ty: &str, label: &str) {
        let label = label.trim();
        let label = if label == ty.trim() { "" } else { label };
        match self.declared_index(family, ty) {
            Some(i) => self.header.children[i].set_child_val(LABEL_TAG, label),
            None if !label.is_empty() => {
                self.push_declaration(family, ty);
                if let Some(i) = self.declared_index(family, ty) {
                    self.header.children[i].set_child_val(LABEL_TAG, label);
                }
            }
            None => {}
        }
        self.tidy_event_types();
    }

    fn declared_index(&self, family: bool, ty: &str) -> Option<usize> {
        self.header.children.iter().position(|c| {
            c.tag == EVENT_TYPE_TAG && c.val().trim() == ty && (c.child_val(RECORD_TAG) == Some("FAM")) == family
        })
    }

    fn push_declaration(&mut self, family: bool, ty: &str) {
        let mut n = Node::with_value(EVENT_TYPE_TAG, ty);
        if family {
            n.children.push(Node::with_value(RECORD_TAG, "FAM"));
        }
        self.header.insert_child(n);
    }

    /// Déclare un type d'événement pour pouvoir le choisir avant de s'en servir.
    /// Renvoie `false` s'il existait déjà (dans le fichier ou l'en-tête).
    pub fn declare_event_type(&mut self, family: bool, ty: &str) -> bool {
        let ty = ty.trim();
        if ty.is_empty() || self.event_types().iter().any(|t| t.family == family && t.ty == ty) {
            return false;
        }
        self.push_declaration(family, ty);
        self.tidy_event_types();
        true
    }

    /// Retire la déclaration d'un type (les événements du fichier ne changent pas).
    pub fn undeclare_event_type(&mut self, family: bool, ty: &str) {
        if let Some(i) = self.declared_index(family, ty) {
            self.header.children.remove(i);
        }
        self.tidy_event_types();
    }

    /// Retire les déclarations devenues inutiles (type utilisé, sans libellé propre)
    /// et ne garde dans `SCHMA` que les balises `_GR_EVEN` utilisées.
    fn tidy_event_types(&mut self) {
        let used: Vec<(bool, String)> =
            self.event_types().into_iter().filter(|t| t.uses > 0).map(|t| (t.family, t.ty)).collect();
        self.header.children.retain(|c| {
            !(c.tag == EVENT_TYPE_TAG
                && c.child(LABEL_TAG).is_none()
                && used.contains(&(c.child_val(RECORD_TAG) == Some("FAM"), c.val().trim().to_string())))
        });
        for sub in [EVENT_TYPE_TAG, RECORD_TAG, LABEL_TAG] {
            let used =
                self.header.children_with(EVENT_TYPE_TAG).any(|d| sub == EVENT_TYPE_TAG || d.child(sub).is_some());
            if used {
                self.declare_extension(sub, EVENT_TYPE_URI);
            } else {
                self.undeclare_extension(sub);
            }
        }
    }

    /// Événements à balise d'extension venus d'autres logiciels (`_MILT`, `_MILI`…),
    /// à convertir à l'import.
    pub fn extension_event_count(&self) -> usize {
        self.records
            .iter()
            .filter(|r| r.tag == "INDI" || r.tag == "FAM")
            .map(|r| r.children.iter().filter(|c| extension_event_type(c).is_some()).count())
            .sum()
    }

    /// Convertit les événements à balise d'extension en `EVEN` + `TYPE` standard :
    /// `1 _MILT` → `1 EVEN` / `2 TYPE Service militaire`. Un `TYPE` d'origine (la
    /// précision : « Infanterie ») passe dans une note. Renvoie le nombre d'événements convertis.
    pub fn migrate_extension_events(&mut self) -> usize {
        let mut count = 0;
        let has = |r: &Node| r.children.iter().any(|c| extension_event_type(c).is_some());
        for r in self.records_where_mut(|r| (r.tag == "INDI" || r.tag == "FAM") && has(r)) {
            for e in r.children.iter_mut() {
                let Some(ty) = extension_event_type(e) else { continue };
                e.tag = "EVEN".into();
                if let Some(i) = e.children.iter().position(|c| c.tag == "TYPE") {
                    let old = e.children.remove(i);
                    if !old.val().trim().is_empty() {
                        e.insert_child(Node::with_value("NOTE", old.val().trim()));
                    }
                }
                e.children.insert(0, Node::with_value("TYPE", &ty));
                if e.value.as_deref() == Some("Y") {
                    e.value = None;
                }
                count += 1;
            }
            r.touch();
        }
        count
    }
}

/// Balises d'extension d'événements connues (Legacy, RootsMagic, Family Tree Maker,
/// Ancestry, Heredis…) et le `TYPE` qu'elles deviennent.
const EXTENSION_EVENTS: &[(&str, &str)] = &[
    ("_MILT", "Service militaire"),
    ("_MILI", "Service militaire"),
    ("_MIL", "Service militaire"),
    ("_MILITARY", "Service militaire"),
    ("_MILITARY_SERVICE", "Service militaire"),
    ("_MDCL", "Informations médicales"),
    ("_MEDICAL", "Informations médicales"),
    ("_DEG", "Diplôme"),
    ("_DEGREE", "Diplôme"),
    ("_ELEC", "Élection"),
    ("_EMPLOY", "Emploi"),
    ("_EXCM", "Excommunication"),
    ("_FUN", "Funérailles"),
    ("_FUNERAL", "Funérailles"),
    ("_ORDI", "Ordination"),
    ("_ORDN", "Ordination"),
    ("_CIRC", "Circoncision"),
    ("_DEST", "Destination"),
    ("_SEPR", "Séparation"),
    ("_SEPARATED", "Séparation"),
    ("_HOSP", "Hospitalisation"),
    ("_ARRI", "Arrivée"),
    ("_DEPA", "Départ"),
];

/// Balises d'extension qui ne sont jamais des événements, même datées.
const NOT_EVENTS: &[&str] = &["_TODO", "_LINT", "_UID", "_PRIM", "_URL", "_PHOTO", "_VALID", "_UPD", "_SOSADABOVILLE"];

/// `TYPE` d'un événement à balise d'extension : balise connue, ou balise inconnue
/// qui a une date ou un lieu (son nom, sans le `_`).
fn extension_event_type(n: &Node) -> Option<String> {
    if !n.tag.starts_with('_') || n.tag.starts_with("_GR_") || NOT_EVENTS.contains(&n.tag.as_str()) {
        return None;
    }
    if let Some((_, ty)) = EXTENSION_EVENTS.iter().find(|(t, _)| *t == n.tag) {
        return Some(ty.to_string());
    }
    let dated = n.children.iter().any(|c| c.tag == "DATE" || c.tag == "PLAC");
    dated.then(|| n.tag.trim_start_matches('_').replace('_', " ").to_lowercase())
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
            t.iter().map(|t| (t.ty.as_str(), t.family)).collect::<Vec<_>>(),
            vec![("Bapteme prot.", false), ("Baptême protestant", false), ("Contrat de mariage", true)]
        );
        // Fusion de deux types.
        assert_eq!(g.rename_event_type(false, "Bapteme prot.", "Baptême protestant"), 1);
        let t = g.event_types();
        assert_eq!(t[0].ty, "Baptême protestant");
        assert_eq!(t[0].uses, 2);
        assert_eq!(t.len(), 2);

        assert!(g.declare_event_type(true, "Bans"));
        assert!(!g.declare_event_type(true, "Bans"));
        assert!(!g.declare_event_type(true, "Contrat de mariage"));
        let out = g.to_gedcom_string();
        assert!(out.contains("1 _GR_EVEN Bans\n2 _GR_REC FAM\n"));
        assert!(out.contains("2 TAG _GR_EVEN https://"));
        let re = Gedcom::parse(&out).unwrap();
        assert!(re.event_types().iter().any(|t| t.ty == "Bans" && t.family && t.declared && t.uses == 0));

        g.rename_event_type(true, "Bans", "Publication des bans");
        assert!(g.event_types().iter().any(|t| t.ty == "Publication des bans" && t.declared));
        g.undeclare_event_type(true, "Publication des bans");
        assert!(g.header.child(EVENT_TYPE_TAG).is_none());
        assert!(g.header.child("SCHMA").is_none());
    }

    #[test]
    fn labels_are_kept_in_the_header() {
        let mut g = Gedcom::parse(G).unwrap();
        g.set_event_type_label(false, "Baptême protestant", "Baptême réformé");
        let out = g.to_gedcom_string();
        assert!(out.contains("1 _GR_EVEN Baptême protestant\n2 _GR_LABEL Baptême réformé\n"), "{out}");
        assert!(out.contains("2 TAG _GR_LABEL https://"));
        assert!(out.contains("2 TYPE Baptême protestant\n"));
        let t = g.event_types();
        let b = t.iter().find(|t| t.ty == "Baptême protestant").unwrap();
        assert_eq!((b.label.as_str(), b.uses), ("Baptême réformé", 1));
        assert_eq!(g.event_labels(), vec![("Baptême protestant".to_string(), "Baptême réformé".to_string())]);
        // Renommer le TYPE garde le libellé.
        g.rename_event_type(false, "Baptême protestant", "baptism (protestant)");
        assert!(g.to_gedcom_string().contains("1 _GR_EVEN baptism (protestant)\n2 _GR_LABEL Baptême réformé\n"));
        // Libellé effacé : le type est utilisé, la déclaration n'a plus lieu d'être.
        g.set_event_type_label(false, "baptism (protestant)", "");
        let out = g.to_gedcom_string();
        assert!(!out.contains("_GR_EVEN"), "{out}");
        assert!(!out.contains("_GR_LABEL"));
    }

    #[test]
    fn imports_extension_events_as_even_type() {
        let mut g = Gedcom::parse(
            "0 HEAD\n0 @I1@ INDI\n1 _MILT\n2 DATE 1914\n2 TYPE Infanterie\n1 _TODO Chercher\n2 DATE 2020\n1 _HOBBY\n2 PLAC Paris\n1 _FOO bar\n0 @F1@ FAM\n1 _SEPR Y\n0 TRLR\n",
        )
        .unwrap();
        assert_eq!(g.extension_event_count(), 3);
        assert_eq!(g.migrate_extension_events(), 3);
        let out = g.to_gedcom_string();
        assert!(out.contains("1 EVEN\n2 TYPE Service militaire\n2 DATE 1914\n2 NOTE Infanterie\n"), "{out}");
        assert!(out.contains("1 EVEN\n2 TYPE hobby\n2 PLAC Paris\n"), "{out}");
        assert!(out.contains("1 _TODO Chercher\n") && out.contains("1 _FOO bar\n"));
        assert!(out.contains("0 @F1@ FAM\n1 EVEN\n2 TYPE Séparation\n"), "{out}");
        assert_eq!(g.extension_event_count(), 0);
    }
}
