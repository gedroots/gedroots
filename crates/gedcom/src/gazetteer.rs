//! Creating places: splitting a place found in a gazetteer (OpenStreetMap) according to the file's
//! place format (`HEAD.PLAC.FORM`), localities attached to an existing place, and writing the
//! coordinates and the external identifier (`EXID`).
//!
//! Everything stays standard GEDCOM 7: the `PLAC` string follows the format, `MAP` holds the
//! coordinates, `EXID` + `TYPE` the place's identifier in the gazetteer.

use crate::document::Gedcom;
use crate::model::{Place, sort_key};
use crate::node::Node;

/// Role of a jurisdiction of the place format, recognized by its name.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Level {
    /// Lieu-dit, hameau, quartier.
    Locality,
    /// Commune, ville, paroisse.
    Municipality,
    PostCode,
    /// Department, county, province.
    County,
    /// Region, state.
    State,
    Country,
    /// Unknown jurisdiction (INSEE code…): left empty.
    Other,
}

impl Level {
    /// Role of a jurisdiction from its name (“Lieudit”, “Code_Postal”, “County”…).
    pub fn of(label: &str) -> Level {
        let k = sort_key(label).replace(['_', '-'], " ");
        let has = |words: &[&str]| words.iter().any(|w| k.contains(w));
        // English and French first, then German, Spanish, Italian, Dutch and Portuguese.
        if has(&[
            "lieudit",
            "lieu dit",
            "hameau",
            "hamlet",
            "quartier",
            "neighbourhood",
            "locality",
            "ortsteil",
            "weiler",
            "barrio",
            "aldea",
            "frazione",
            "localita",
            "buurtschap",
            "gehucht",
            "lugar",
        ]) {
            Level::Locality
        } else if has(&["postal", "postcode", "zip", "postleitzahl", "plz", "postale", "cap", "postcode"])
            || k.trim() == "cp"
        {
            Level::PostCode
        } else if has(&[
            "commune",
            "ville",
            "city",
            "town",
            "village",
            "paroisse",
            "parish",
            "municip",
            "localite",
            "gemeinde",
            "stadt",
            "ort",
            "pfarrei",
            "kirchspiel",
            "pueblo",
            "ciudad",
            "parroquia",
            "comune",
            "citta",
            "paese",
            "parrocchia",
            "gemeente",
            "plaats",
            "stad",
            "parochie",
            "concelho",
            "freguesia",
            "cidade",
        ]) {
            Level::Municipality
        } else if has(&[
            "departement",
            "county",
            "comte",
            "district",
            "province",
            "kreis",
            "landkreis",
            "bezirk",
            "amt",
            "provincia",
            "comarca",
            "partido",
            "distrito",
            "provincie",
        ]) {
            Level::County
        } else if has(&[
            "region",
            "state",
            "etat",
            "land",
            "bundesland",
            "kanton",
            "canton",
            "comunidad",
            "regione",
            "estado",
        ]) {
            Level::State
        } else if has(&["pays", "country", "nation", "staat", "pais", "nazione", "stato"]) {
            Level::Country
        } else {
            Level::Other
        }
    }
}

/// A place found in a gazetteer, with its split address.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Candidate {
    /// Name of the place itself (“Saint-Parres-aux-Tertres”).
    pub name: String,
    pub lat: f64,
    pub lon: f64,
    /// Address: gazetteer keys (`village`, `county`, `state`, `country`, `postcode`…).
    pub address: Vec<(String, String)>,
    /// External identifier: value and `TYPE` (URI) of the `EXID`.
    pub exid: Option<(String, String)>,
}

impl Candidate {
    fn addr(&self, keys: &[&str]) -> Option<&str> {
        keys.iter()
            .find_map(|k| self.address.iter().find(|(a, _)| a == k).map(|(_, v)| v.as_str()))
            .filter(|v| !v.trim().is_empty())
    }

    /// Value for a jurisdiction.
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

    /// Level of the place itself (its first address key): a village is a municipality, a
    /// département a county.
    pub fn own_level(&self) -> Level {
        match self.address.first().map(|(k, _)| k.as_str()) {
            Some("city" | "town" | "village" | "municipality") => Level::Municipality,
            Some("county" | "state_district") => Level::County,
            Some("state" | "region") => Level::State,
            Some("country") => Level::Country,
            _ => Level::Locality,
        }
    }

    /// Label of the place in a list: its name.
    pub fn title(&self) -> String {
        self.level(Level::Locality)
            .or_else(|| self.level(Level::Municipality))
            .map(str::to_string)
            .unwrap_or_else(|| self.name.clone())
    }

    /// Broader jurisdictions, to locate the place: “Aube · Grand Est · France”.
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

    /// Jurisdictions of the place according to the place format. Without a format: municipality,
    /// department, region, country (without the empty ones), in a single field.
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
        // No jurisdiction recognized for the place name: it goes into the most precise one.
        let title = self.title();
        if !out.contains(&title)
            && let Some(slot) = out.iter().position(|v| v.is_empty())
        {
            out[slot] = title;
        }
        out
    }
}

/// Name of a place from its jurisdictions: `", Troyes, 10000, Aube, Grand Est, France"`. Empty
/// jurisdictions keep their slot, to follow the format.
pub fn join_parts(parts: &[String]) -> String {
    if parts.iter().all(|p| p.trim().is_empty()) {
        return String::new();
    }
    parts.iter().map(|p| p.trim()).collect::<Vec<_>>().join(", ")
}

impl Gedcom {
    /// The place format as the gazetteer reads it: a jurisdiction whose name says nothing
    /// (“Code INSEE”, “Niveau 3”) is recognized by the tree's own values when they leave no doubt
    /// (postal codes, country names); otherwise it stays unknown and is left empty. The labels are
    /// for matching only, never shown.
    pub fn gazetteer_form(&self) -> Vec<String> {
        let form = self.place_form();
        if form.iter().all(|f| Level::of(f) != Level::Other) {
            return form;
        }
        let places: Vec<Vec<String>> = self
            .places()
            .into_iter()
            .map(|p| p.name.split(',').map(|s| s.trim().to_string()).collect::<Vec<_>>())
            .filter(|parts| parts.len() == form.len())
            .collect();
        let taken: Vec<Level> = form.iter().map(|f| Level::of(f)).collect();
        form.iter()
            .enumerate()
            .map(|(i, f)| {
                // A label naming another kind of value (an INSEE code, a FIPS number) stays unknown.
                let k = sort_key(f);
                let named = ["insee", "code", "id", "fips", "gnis", "number", "nummer", "numero"];
                if taken[i] != Level::Other
                    || named.iter().any(|w| k.split(|c: char| !c.is_alphanumeric()).any(|x| x == *w))
                {
                    return f.clone();
                }
                let values: Vec<&str> = places.iter().map(|p| p[i].as_str()).filter(|v| !v.is_empty()).collect();
                if values.len() < 3 {
                    return f.clone();
                }
                let all = |pred: &dyn Fn(&str) -> bool| values.iter().all(|v| pred(v));
                let postcode = |v: &str| (4..=6).contains(&v.len()) && v.chars().all(|c| c.is_ascii_digit());
                let country = |v: &str| crate::atlas::in_place(v).is_some();
                if all(&postcode) && !taken.contains(&Level::PostCode) {
                    "Postcode".to_string()
                } else if all(&country) && !taken.contains(&Level::Country) {
                    "Country".to_string()
                } else {
                    f.clone()
                }
            })
            .collect()
    }
}

/// Does a place name follow the file's place format: one value per jurisdiction, empty ones
/// kept (`", Troyes, 10000, Aube, Grand Est, France"`)? Always, without a format of several
/// jurisdictions.
pub fn follows_form(name: &str, form: &[String]) -> bool {
    form.len() < 2 || name.split(',').count() == form.len()
}

/// A place of the tree completed with a gazetteer result, in the file's format: the empty
/// jurisdictions take the gazetteer's value, the others keep the tree's spelling. A place that
/// does not follow the format (“Lalobbe, Ardennes, France” in a file of six jurisdictions) is
/// written again from the gazetteer, keeping a locality the gazetteer does not have.
pub fn complete(name: &str, candidate: &Candidate, form: &[String]) -> String {
    if form.len() < 2 {
        return name.to_string();
    }
    let theirs = candidate.parts(form);
    if follows_form(name, form) {
        let ours: Vec<String> = name.split(',').map(|s| s.trim().to_string()).collect();
        let merged: Vec<String> = ours.into_iter().zip(theirs).map(|(o, t)| if o.is_empty() { t } else { o }).collect();
        return join_parts(&merged);
    }
    let mut parts = theirs;
    // “La Besace, Lalobbe, Ardennes”: a first part that is none of the gazetteer's goes to the
    // locality, when the format has one and the gazetteer leaves it empty.
    let key = |s: &str| sort_key(s.trim());
    if let Some(first) = name.split(',').map(str::trim).find(|s| !s.is_empty())
        && !parts.iter().any(|p| key(p) == key(first))
        && let Some(i) = form.iter().position(|f| Level::of(f) == Level::Locality)
        && parts[i].is_empty()
    {
        parts[i] = first.to_string();
    }
    join_parts(&parts)
}

/// Locality of an existing place: “La Borde, Troyes” → (jurisdictions of the new place, parent
/// place).
///
/// The parent is the file's place whose name (municipality) matches what follows the first comma;
/// the new place takes all its jurisdictions and puts the locality in the most precise empty
/// jurisdiction, otherwise first.
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
    // A municipality rather than an already attached locality, then the most cited.
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
    /// Writes the coordinates (and the external identifier) of a place on all its occurrences.
    /// Returns the number of occurrences changed.
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
        // Format without a locality: first, in a single field.
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
        // A second time: no duplicate.
        g.locate_place(&name, "N48.2978", "E4.1175", c.exid.as_ref());
        let text = g.to_gedcom_string();
        assert!(text.contains("3 MAP\n4 LATI N48.2978\n4 LONG E4.1175\n3 EXID 415228\n4 TYPE https://www.openstreetmap.org/relation/\n"), "{text}");
        assert_eq!(text.matches("EXID").count(), 1);
    }

    #[test]
    fn levels_in_other_languages_and_from_values() {
        assert_eq!(Level::of("Gemeinde"), Level::Municipality);
        assert_eq!(Level::of("Kreis"), Level::County);
        assert_eq!(Level::of("Provincia"), Level::County);
        assert_eq!(Level::of("Bundesland"), Level::State);
        let g = Gedcom::parse(
            "0 HEAD\n1 GEDC\n2 VERS 7.0\n1 PLAC\n2 FORM Commune, Code INSEE, Niveau 2, Niveau 3\n\
0 @I1@ INDI\n1 BIRT\n2 PLAC Troyes, 10387, 10000, France\n1 DEAT\n2 PLAC Agneaux, 50002, 50180, France\n\
1 RESI\n2 PLAC Lyon, 69123, 69001, France\n1 EVEN\n2 PLAC Rennes, , , France\n0 TRLR\n",
        )
        .unwrap();
        // Digits under a label that says nothing: postal codes; country names: the country. The
        // INSEE code looks the same but its label names it: left empty.
        assert_eq!(g.gazetteer_form(), vec!["Commune", "Code INSEE", "Postcode", "Country"]);
        assert_eq!(g.place_form(), vec!["Commune", "Code INSEE", "Niveau 2", "Niveau 3"]);
    }

    #[test]
    fn completes_places() {
        let form = ancestris();
        let c = parres();
        // Empty jurisdictions filled, the tree's spelling kept.
        assert_eq!(
            complete(", St-Parres-aux-Tertres, , Aube, , France", &c, &form),
            ", St-Parres-aux-Tertres, 10410, Aube, Grand Est, France"
        );
        // Not in the format: written again from the gazetteer.
        assert!(!follows_form("Saint-Parres-aux-Tertres, Aube, France", &form));
        assert_eq!(
            complete("Saint-Parres-aux-Tertres, Aube, France", &c, &form),
            ", Saint-Parres-aux-Tertres, 10410, Aube, Grand Est, France"
        );
        // A locality the gazetteer does not know is kept.
        assert_eq!(
            complete("Les Cortins, Saint-Parres-aux-Tertres (Aube)", &c, &form),
            "Les Cortins, Saint-Parres-aux-Tertres, 10410, Aube, Grand Est, France"
        );
        // No format: unchanged.
        assert_eq!(complete("Saint-Parres", &c, &[]), "Saint-Parres");
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
        // The displayed label gives back the file's place, empty jurisdictions included.
        assert_eq!(g.place_value("Troyes, 10000, Aube, Grand Est, France ", &[]), full);
        assert_eq!(g.place_value(full, &[]), full);
        let created = ", Bayeux, 14400, Calvados, Normandie, France".to_string();
        assert_eq!(
            g.place_value("Bayeux, 14400, Calvados, Normandie, France", std::slice::from_ref(&created)),
            created
        );
        assert_eq!(g.place_value("Lyon, France", &[]), "Lyon, France");
    }
}
