//! Création de lieux : découpage d'un lieu trouvé dans un gazetteer (OpenStreetMap)
//! selon le format des lieux du fichier (`HEAD.PLAC.FORM`), lieux-dits rattachés à un
//! lieu existant, et écriture des coordonnées et de l'identifiant externe (`EXID`).
//!
//! Tout reste en GEDCOM 7 standard : la chaîne `PLAC` suit le format, `MAP` porte les
//! coordonnées, `EXID` + `TYPE` l'identifiant du lieu dans le gazetteer.

use crate::document::Gedcom;
use crate::model::{Place, sort_key};
use crate::node::Node;

/// Rôle d'une juridiction du format des lieux, reconnu à son nom.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Level {
    /// Lieu-dit, hameau, quartier.
    Locality,
    /// Commune, ville, paroisse.
    Municipality,
    PostCode,
    /// Département, comté, province.
    County,
    /// Région, état.
    State,
    Country,
    /// Juridiction inconnue (code INSEE…) : laissée vide.
    Other,
}

impl Level {
    /// Rôle d'une juridiction d'après son nom (« Lieudit », « Code_Postal », « County »…).
    pub fn of(label: &str) -> Level {
        let k = sort_key(label).replace(['_', '-'], " ");
        let has = |words: &[&str]| words.iter().any(|w| k.contains(w));
        if has(&["lieudit", "lieu dit", "hameau", "hamlet", "quartier", "neighbourhood", "locality"]) {
            Level::Locality
        } else if has(&["postal", "postcode", "zip"]) || k.trim() == "cp" {
            Level::PostCode
        } else if has(&["commune", "ville", "city", "town", "village", "paroisse", "parish", "municip", "localite"]) {
            Level::Municipality
        } else if has(&["departement", "county", "comte", "district", "province"]) {
            Level::County
        } else if has(&["region", "state", "etat"]) {
            Level::State
        } else if has(&["pays", "country", "nation"]) {
            Level::Country
        } else {
            Level::Other
        }
    }
}

/// Un lieu trouvé dans un gazetteer, avec son adresse découpée.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Candidate {
    /// Nom du lieu lui-même (« Saint-Parres-aux-Tertres »).
    pub name: String,
    pub lat: f64,
    pub lon: f64,
    /// Adresse : clés du gazetteer (`village`, `county`, `state`, `country`, `postcode`…).
    pub address: Vec<(String, String)>,
    /// Identifiant externe : valeur et `TYPE` (URI) de l'`EXID`.
    pub exid: Option<(String, String)>,
}

impl Candidate {
    fn addr(&self, keys: &[&str]) -> Option<&str> {
        keys.iter()
            .find_map(|k| self.address.iter().find(|(a, _)| a == k).map(|(_, v)| v.as_str()))
            .filter(|v| !v.trim().is_empty())
    }

    /// Valeur pour une juridiction.
    pub fn level(&self, level: Level) -> Option<&str> {
        match level {
            Level::Locality => self.addr(&["hamlet", "isolated_dwelling", "locality", "neighbourhood", "quarter"]),
            Level::Municipality => self.addr(&["village", "town", "city", "municipality"]),
            Level::PostCode => self.addr(&["postcode"]).map(|p| p.split([';', ',']).next().unwrap_or(p).trim()),
            Level::County => self.addr(&["county", "state_district"]),
            Level::State => self.addr(&["state", "region"]),
            Level::Country => self.addr(&["country"]),
            Level::Other => None,
        }
    }

    /// Libellé du lieu dans une liste : son nom.
    pub fn title(&self) -> String {
        self.level(Level::Locality)
            .or_else(|| self.level(Level::Municipality))
            .map(str::to_string)
            .unwrap_or_else(|| self.name.clone())
    }

    /// Juridictions plus larges, pour situer le lieu : « Aube · Grand Est · France ».
    pub fn context(&self) -> String {
        let title = self.title();
        let mut out: Vec<&str> = Vec::new();
        if self.level(Level::Locality).is_some()
            && let Some(m) = self.level(Level::Municipality)
        {
            out.push(m);
        }
        for l in [Level::County, Level::State, Level::Country] {
            if let Some(v) = self.level(l).filter(|v| *v != title && !out.contains(v)) {
                out.push(v);
            }
        }
        out.join(" · ")
    }

    /// Juridictions du lieu selon le format des lieux. Sans format : commune,
    /// département, région, pays (les vides en moins), en un seul champ.
    pub fn parts(&self, form: &[String]) -> Vec<String> {
        if form.is_empty() {
            let v: Vec<&str> = [Level::Locality, Level::Municipality, Level::County, Level::State, Level::Country]
                .into_iter()
                .filter_map(|l| self.level(l))
                .collect();
            let joined = if v.is_empty() { self.name.clone() } else { v.join(", ") };
            return vec![joined];
        }
        let mut out: Vec<String> = form.iter().map(|f| self.level(Level::of(f)).unwrap_or("").to_string()).collect();
        // Aucune juridiction reconnue pour le nom du lieu : il va dans la plus précise.
        let title = self.title();
        if !out.contains(&title)
            && let Some(slot) = out.iter().position(|v| v.is_empty())
        {
            out[slot] = title;
        }
        out
    }
}

/// Nom d'un lieu à partir de ses juridictions : `", Troyes, 10000, Aube, Grand Est, France"`.
/// Les juridictions vides gardent leur place, pour suivre le format.
pub fn join_parts(parts: &[String]) -> String {
    if parts.iter().all(|p| p.trim().is_empty()) {
        return String::new();
    }
    parts.iter().map(|p| p.trim()).collect::<Vec<_>>().join(", ")
}

/// Lieu-dit d'un lieu existant : « La Borde, Troyes » → (juridictions du nouveau lieu, lieu parent).
///
/// Le parent est le lieu du fichier dont le nom (commune) correspond à ce qui suit la
/// première virgule ; le nouveau lieu reprend toutes ses juridictions et place le
/// lieu-dit dans la juridiction vide la plus précise, sinon en tête.
pub fn sub_place(input: &str, places: &[Place], form: &[String]) -> Option<(Vec<String>, String)> {
    let (name, rest) = input.split_once(',')?;
    let (name, rest) = (name.trim(), sort_key(rest.trim()));
    if name.is_empty() || rest.chars().count() < 2 {
        return None;
    }
    let main = |p: &Place| {
        let short = p.short();
        let base = short.split(" (").next().unwrap_or(&short).to_string();
        sort_key(&base)
    };
    // Une commune plutôt qu'un lieu-dit déjà rattaché, puis la plus citée.
    let parent = places
        .iter()
        .filter(|p| {
            let m = main(p);
            m == rest || (rest.chars().count() >= 3 && m.starts_with(&rest))
        })
        .min_by_key(|p| (main(p) != rest, main(p).contains(','), std::cmp::Reverse(p.uses)))?;
    let mut parts: Vec<String> = parent.name.split(',').map(|s| s.trim().to_string()).collect();
    let fits = form.len() >= 2 && parts.len() == form.len();
    let slot = if fits {
        form.iter().position(|f| Level::of(f) == Level::Locality).filter(|&i| parts[i].is_empty())
    } else {
        None
    }
    .or_else(|| parts.first().filter(|p| p.is_empty()).map(|_| 0));
    match slot {
        Some(i) => parts[i] = name.to_string(),
        None => parts.insert(0, name.to_string()),
    }
    if !(form.len() >= 2 && parts.len() == form.len()) {
        parts = vec![join_parts(&parts)];
    }
    Some((parts, parent.name.clone()))
}

impl Gedcom {
    /// Écrit les coordonnées (et l'identifiant externe) d'un lieu sur toutes ses
    /// occurrences. Renvoie le nombre d'occurrences modifiées.
    pub fn locate_place(&mut self, name: &str, lat: &str, lon: &str, exid: Option<&(String, String)>) -> usize {
        let n = self.update_place(name, name, lat, lon);
        if let Some((value, ty)) = exid {
            for r in self.records_where_mut(|r| r.any(&|n| n.tag == "PLAC" && n.val() == name)) {
                r.walk_mut(&mut |n| {
                    if n.tag == "PLAC" && n.val() == name {
                        let same = |e: &Node| e.child_val("TYPE") == Some(ty.as_str());
                        n.children.retain(|e| !(e.tag == "EXID" && same(e)));
                        let at = n.children.iter().position(|c| c.tag != "FORM" && c.tag != "LANG" && c.tag != "MAP");
                        let node = Node::with_value("EXID", value.as_str()).push(Node::with_value("TYPE", ty.as_str()));
                        match at {
                            Some(i) => n.children.insert(i, node),
                            None => n.children.push(node),
                        }
                    }
                });
            }
        }
        n
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ancestris() -> Vec<String> {
        ["Lieudit", "Commune", "Code_Postal", "Département", "Région", "Pays"].map(String::from).to_vec()
    }

    fn parres() -> Candidate {
        Candidate {
            name: "Saint-Parres-aux-Tertres".into(),
            lat: 48.2978,
            lon: 4.1175,
            address: [
                ("village", "Saint-Parres-aux-Tertres"),
                ("county", "Aube"),
                ("state", "Grand Est"),
                ("postcode", "10410"),
                ("country", "France"),
            ]
            .map(|(a, b)| (a.to_string(), b.to_string()))
            .to_vec(),
            exid: Some(("415228".into(), "https://www.openstreetmap.org/relation/".into())),
        }
    }

    fn place(name: &str, uses: usize) -> Place {
        Place { name: name.into(), latitude: None, longitude: None, uses, records: vec![] }
    }

    #[test]
    fn levels() {
        let l: Vec<Level> = ancestris().iter().map(|f| Level::of(f)).collect();
        use Level::*;
        assert_eq!(l, vec![Locality, Municipality, PostCode, County, State, Country]);
        assert_eq!(Level::of("Code INSEE"), Other);
        assert_eq!(Level::of("City"), Municipality);
        assert_eq!(Level::of("Localité"), Municipality);
    }

    #[test]
    fn candidate_follows_form() {
        let c = parres();
        assert_eq!(c.parts(&ancestris()), vec!["", "Saint-Parres-aux-Tertres", "10410", "Aube", "Grand Est", "France"]);
        assert_eq!(join_parts(&c.parts(&ancestris())), ", Saint-Parres-aux-Tertres, 10410, Aube, Grand Est, France");
        let short = ["Commune", "Pays"].map(String::from).to_vec();
        assert_eq!(c.parts(&short), vec!["Saint-Parres-aux-Tertres", "France"]);
        assert_eq!(c.parts(&[]), vec!["Saint-Parres-aux-Tertres, Aube, Grand Est, France"]);
        assert_eq!(c.title(), "Saint-Parres-aux-Tertres");
        assert_eq!(c.context(), "Aube · Grand Est · France");
    }

    #[test]
    fn unknown_name_goes_to_most_precise() {
        let mut c = parres();
        c.name = "Abbaye de Montier-la-Celle".into();
        c.address.retain(|(k, _)| k != "village");
        let form = ["Commune", "Département", "Pays"].map(String::from).to_vec();
        assert_eq!(c.parts(&form), vec!["Abbaye de Montier-la-Celle", "Aube", "France"]);
    }

    #[test]
    fn sub_place_inherits_parent() {
        let places = vec![
            place(", Troyes, 10000, Aube, Grand Est, France", 41),
            place("Saint-Frobert, Troyes, 10000, Aube, Grand Est, France", 1),
        ];
        let (parts, parent) = sub_place("La Borde, Troyes", &places, &ancestris()).unwrap();
        assert_eq!(parent, ", Troyes, 10000, Aube, Grand Est, France");
        assert_eq!(parts, vec!["La Borde", "Troyes", "10000", "Aube", "Grand Est", "France"]);
        assert!(sub_place("La Borde, Lyon", &places, &ancestris()).is_none());
        assert!(sub_place("Troyes", &places, &ancestris()).is_none());
        // Format sans lieu-dit : en tête, un seul champ.
        let places = vec![place("Troyes, Aube, France", 3)];
        let form = ["Commune", "Département", "Pays"].map(String::from).to_vec();
        let (parts, _) = sub_place("La Borde, troyes", &places, &form).unwrap();
        assert_eq!(parts, vec!["La Borde, Troyes, Aube, France"]);
    }

    #[test]
    fn locate_writes_map_and_exid() {
        let mut g = Gedcom::parse(
            "0 HEAD\n1 GEDC\n2 VERS 7.0\n0 @I1@ INDI\n1 BIRT\n2 PLAC , Saint-Parres-aux-Tertres, 10410, Aube, Grand Est, France\n0 TRLR\n",
        )
        .unwrap();
        let c = parres();
        let name = join_parts(&c.parts(&ancestris()));
        assert_eq!(g.locate_place(&name, "N48.2978", "E4.1175", c.exid.as_ref()), 1);
        // Une seconde fois : pas de doublon.
        g.locate_place(&name, "N48.2978", "E4.1175", c.exid.as_ref());
        let text = g.to_gedcom_string();
        assert!(text.contains("3 MAP\n4 LATI N48.2978\n4 LONG E4.1175\n3 EXID 415228\n4 TYPE https://www.openstreetmap.org/relation/\n"), "{text}");
        assert_eq!(text.matches("EXID").count(), 1);
    }

    #[test]
    fn labels_drop_empty_jurisdictions() {
        use crate::model::place_label;
        assert_eq!(place_label(", Troyes, 10000, Aube, , France"), "Troyes, 10000, Aube, France");
        assert_eq!(place_label("Paris"), "Paris");
        let g = Gedcom::parse(
            "0 HEAD\n1 GEDC\n2 VERS 7.0\n0 @I1@ INDI\n1 BIRT\n2 PLAC , Troyes, 10000, Aube, Grand Est, France\n0 TRLR\n",
        )
        .unwrap();
        let full = ", Troyes, 10000, Aube, Grand Est, France";
        // Le libellé affiché redonne le lieu du fichier, juridictions vides comprises.
        assert_eq!(g.place_value("Troyes, 10000, Aube, Grand Est, France ", &[]), full);
        assert_eq!(g.place_value(full, &[]), full);
        let created = ", Bayeux, 14400, Calvados, Normandie, France".to_string();
        assert_eq!(g.place_value("Bayeux, 14400, Calvados, Normandie, France", &[created.clone()]), created);
        assert_eq!(g.place_value("Lyon, France", &[]), "Lyon, France");
    }
}
