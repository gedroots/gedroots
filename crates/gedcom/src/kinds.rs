//! Types des sources et des médias, en GEDCOM 7 standard.
//!
//! - **Contenu d'une source** : `SOUR` / `DATA` / `EVEN <types>` (+ `DATE` période,
//!   `PLAC`), « ce registre contient des naissances et des décès, de 1793 à 1802 ».
//!   La saisie d'un acte le complète toute seule ([`record_in_source`]).
//! - **Support d'un média** : `OBJE` / `FILE` / `FORM` / `MEDI` (`PHOTO`,
//!   `MANUSCRIPT`…), une liste fermée définie par la norme.
//! - **Portrait** d'un individu : son premier lien vers une photo (voir
//!   [`Gedcom::set_portrait`]) ; `_PRIM Y` des autres logiciels est converti à
//!   l'import ([`Gedcom::migrate_prim_tags`]).

use crate::date::Date;
use crate::document::Gedcom;
use crate::model::event_label;
use crate::model::sort_key;
use crate::node::Node;
use crate::relations::mime_for;

/// Valeurs de `MEDI` (GEDCOM 7) et leur libellé, dans l'ordre de la liste « Support ».
pub const MEDIUMS: &[(&str, &str)] = &[
    ("PHOTO", "Photo"),
    ("MANUSCRIPT", "Manuscrit, acte"),
    ("BOOK", "Livre"),
    ("NEWSPAPER", "Journal"),
    ("MAGAZINE", "Revue"),
    ("TOMBSTONE", "Pierre tombale"),
    ("MAP", "Carte géographique"),
    ("CARD", "Carte postale, fiche"),
    ("FILM", "Microfilm"),
    ("FICHE", "Microfiche"),
    ("ELECTRONIC", "Document numérique"),
    ("AUDIO", "Enregistrement sonore"),
    ("VIDEO", "Vidéo"),
    ("OTHER", "Autre"),
];

pub fn medium_label(medi: &str) -> Option<&'static str> {
    MEDIUMS.iter().find(|(m, _)| *m == medi).map(|(_, l)| *l)
}

/// Support d'un média (`MEDI` de son premier fichier).
pub fn medium(media: &Node) -> Option<&str> {
    media.children_with("FILE").find_map(|f| f.child("FORM").and_then(|f| f.child_val("MEDI")))
}

/// Définit (ou retire si vide) le support de tous les fichiers du média.
/// `FORM` est obligatoire au-dessus de `MEDI` : déduit du fichier s'il manque.
pub fn set_medium(media: &mut Node, medi: &str) {
    let medi = medi.trim();
    for f in media.children.iter_mut().filter(|c| c.tag == "FILE") {
        if f.child("FORM").is_none() {
            if medi.is_empty() {
                continue;
            }
            let mime = mime_for(f.val()).to_string();
            f.set_child_val("FORM", &mime);
        }
        let form = f.child_mut("FORM").unwrap();
        form.set_child_val("MEDI", medi);
        if medi != "OTHER" {
            // PHRASE ne précise que « Autre ».
            if let Some(m) = form.child_mut("MEDI") {
                m.children.retain(|c| c.tag != "PHRASE");
            }
        }
    }
}

/// Types d'événements que la norme permet sous `SOUR` / `DATA` / `EVEN`.
fn data_event_tags() -> &'static [&'static str] {
    const FALLBACK: &[&str] = &[];
    crate::schema::record("SOUR")
        .and_then(|s| s.sub("DATA"))
        .and_then(|d| d.def().sub("EVEN"))
        .and_then(|e| match e.def().payload {
            crate::schema::Payload::Enum { values, .. } => Some(values),
            _ => None,
        })
        .unwrap_or(FALLBACK)
}

/// Ce qu'une source contient : types d'actes, période et lieu (`DATA` / `EVEN`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SourceContent {
    /// Balises (`BIRT`, `DEAT`…), sans doublon, dans l'ordre du fichier.
    pub events: Vec<String>,
    /// Période (`DATE` GEDCOM brut, ex. `FROM 1793 TO 1802`).
    pub period: String,
    pub place: String,
}

impl SourceContent {
    pub fn read(source: &Node) -> Self {
        let mut out = SourceContent::default();
        for ev in source.children_with("DATA").flat_map(|d| d.children_with("EVEN")) {
            for t in split_list(ev.val()) {
                if !out.events.contains(&t) {
                    out.events.push(t);
                }
            }
            if out.period.is_empty() {
                out.period = ev.child_val("DATE").unwrap_or("").to_string();
            }
            if out.place.is_empty() {
                out.place = ev.child_val("PLAC").unwrap_or("").to_string();
            }
        }
        out
    }

    pub fn is_empty(&self) -> bool {
        self.events.is_empty() && self.period.trim().is_empty() && self.place.trim().is_empty()
    }

    /// Réécrit `DATA` / `EVEN` (une seule ligne). Laisse `DATA` / `AGNC` et les notes.
    pub fn apply(&self, source: &mut Node) {
        if let Some(d) = source.child_mut("DATA") {
            d.children.retain(|c| c.tag != "EVEN");
        }
        if !self.events.is_empty() {
            let mut ev = Node::with_value("EVEN", self.events.join(", "));
            if !self.period.trim().is_empty() {
                ev.children.push(Node::with_value("DATE", self.period.trim()));
            }
            if !self.place.trim().is_empty() {
                ev.children.push(Node::with_value("PLAC", self.place.trim()));
            }
            let data = source.ensure_child("DATA");
            let at = data.children.iter().position(|c| c.tag != "EVEN").unwrap_or(data.children.len());
            data.children.insert(at.min(data.children.len()), ev);
        }
        if source.child("DATA").is_some_and(|d| d.children.is_empty() && d.value.is_none()) {
            source.children.retain(|c| c.tag != "DATA");
        }
    }

    /// Types d'actes en français : « Naissance, décès ».
    pub fn events_label(&self) -> String {
        let labels: Vec<String> = self.events.iter().map(|t| event_label(t).unwrap_or(t).to_string()).collect();
        let s = labels.join(", ");
        let mut c = s.chars();
        match c.next() {
            Some(f) => f.to_string() + &c.as_str().to_lowercase(),
            None => s,
        }
    }

    /// Résumé d'une ligne : « Naissance, décès · de 1793 à 1802 ».
    pub fn summary(&self) -> String {
        let mut parts = vec![];
        if !self.events.is_empty() {
            parts.push(self.events_label());
        }
        if !self.period.trim().is_empty() {
            parts.push(Date::parse(&self.period).to_french());
        }
        if !self.place.trim().is_empty() {
            parts.push(self.place.split(',').next().unwrap_or("").trim().to_string());
        }
        parts.join(" · ")
    }
}

fn split_list(v: &str) -> Vec<String> {
    v.split(',').map(|s| s.trim().to_uppercase()).filter(|s| !s.is_empty()).collect()
}

/// Lit une liste de types d'actes saisie en français (« naissances, décès »,
/// « BIRT, DEAT »). Renvoie l'élément non compris en cas d'erreur.
pub fn parse_event_list(text: &str) -> Result<Vec<String>, String> {
    let allowed = data_event_tags();
    let mut out: Vec<String> = Vec::new();
    for item in text.split([',', ';']).map(str::trim).filter(|s| !s.is_empty()) {
        let up = item.to_uppercase();
        let tag = if allowed.contains(&up.as_str()) {
            Some(up)
        } else {
            let k = sort_key(item);
            let k = k.trim_end_matches('s');
            let k = k.strip_prefix("acte de ").or_else(|| k.strip_prefix("actes de ")).unwrap_or(k);
            // « Baptême » désigne BAPM dans les modèles d'actes (CHR vient avant dans la liste).
            if k == "bapteme" {
                Some("BAPM".to_string())
            } else {
                allowed
                    .iter()
                    .find(|t| event_label(t).is_some_and(|l| sort_key(l).trim_end_matches('s') == k))
                    .map(|t| t.to_string())
            }
        };
        match tag {
            Some(t) if !out.contains(&t) => out.push(t),
            Some(_) => {}
            None => return Err(format!("type d'acte inconnu : « {item} »")),
        }
    }
    Ok(out)
}

/// Années grégoriennes couvertes par une date (`None` si elle n'en a pas).
fn year_bounds(raw: &str) -> Option<(i32, i32)> {
    let d = Date::parse(raw);
    let years: Vec<i32> = d.dates().iter().filter_map(|s| s.gregorian_year()).collect();
    Some((*years.iter().min()?, *years.iter().max()?))
}

/// Saisie d'un acte de type `tag` daté `date` (GEDCOM brut, peut être vide) :
/// ajoute le type au contenu de la source et élargit sa période à l'année de
/// l'acte. Renvoie `true` si la source a changé.
pub fn record_in_source(source: &mut Node, tag: &str, date: &str) -> bool {
    if !data_event_tags().contains(&tag) {
        return false;
    }
    let data = source.ensure_child("DATA");
    let i = data
        .children
        .iter()
        .position(|c| c.tag == "EVEN" && split_list(c.val()).iter().any(|t| t == tag))
        .or_else(|| data.children.iter().position(|c| c.tag == "EVEN"));
    let mut changed = false;
    let ev = match i {
        Some(i) => &mut data.children[i],
        None => {
            data.children.insert(0, Node::new("EVEN"));
            changed = true;
            &mut data.children[0]
        }
    };
    let mut list = split_list(ev.val());
    if !list.iter().any(|t| t == tag) {
        list.push(tag.to_string());
        ev.value = Some(list.join(", "));
        changed = true;
    }
    if let Some((lo, hi)) = year_bounds(date) {
        let cur = ev.child_val("DATE").and_then(year_bounds);
        let (nlo, nhi) = match cur {
            Some((a, b)) => (a.min(lo), b.max(hi)),
            None => (lo, hi),
        };
        if cur != Some((nlo, nhi)) {
            ev.set_child_val("DATE", &format!("FROM {nlo} TO {nhi}"));
            changed = true;
        }
    }
    changed
}

/// Années hors de la période déclarée par la source pour ce type d'acte :
/// `Some((période, date))` quand la date est certainement en dehors.
pub fn outside_period<'a>(source: &'a Node, tag: &str, date: &str) -> Option<&'a str> {
    use crate::analysis::span::Span;
    let ev = source
        .children_with("DATA")
        .flat_map(|d| d.children_with("EVEN"))
        .find(|e| split_list(e.val()).iter().any(|t| t == tag))?;
    let period_raw = ev.child_val("DATE")?;
    let period = Span::parse(period_raw)?;
    let at = Span::parse(date)?;
    (at.surely_before(&period) || period.surely_before(&at)).then_some(period_raw)
}

/// Balise des autres logiciels (Family Tree Maker, Legacy, MyHeritage…) qui
/// désigne la photo principale d'un individu.
pub const PRIM_TAG: &str = "_PRIM";

impl Gedcom {
    /// Fait de ce média le portrait de l'individu : son lien passe en tête des
    /// médias de l'individu (le lien est créé s'il n'existait pas) et le média,
    /// s'il n'a pas de support, devient une photo. Renvoie `false` si rien n'a changé.
    pub fn set_portrait(&mut self, indi: &str, media: &str) -> bool {
        if self.get(media).is_none_or(|m| m.tag != "OBJE") {
            return false;
        }
        let ptr = format!("@{media}@");
        let Some(ind) = self.get_mut(indi) else { return false };
        let first = ind.children.iter().position(|c| c.tag == "OBJE");
        let at = ind.children.iter().position(|c| c.tag == "OBJE" && c.val() == ptr);
        let mut changed = false;
        match (first, at) {
            (Some(f), Some(a)) if f == a => {}
            (Some(f), Some(a)) => {
                let link = ind.children.remove(a);
                ind.children.insert(f, link);
                changed = true;
            }
            (Some(f), None) => {
                ind.children.insert(f, Node::with_value("OBJE", &ptr));
                changed = true;
            }
            (None, _) => {
                ind.insert_child(Node::with_value("OBJE", &ptr));
                changed = true;
            }
        }
        if changed {
            ind.touch();
        }
        let m = self.get_mut(media).unwrap();
        if medium(m).is_none() && m.child("FILE").is_some() {
            set_medium(m, "PHOTO");
            m.touch();
            changed = true;
        }
        changed
    }

    /// Nombre de liens `_PRIM Y` (photo principale d'un autre logiciel).
    pub fn prim_count(&self) -> usize {
        let mut n = 0;
        for r in &self.records {
            for c in r.children.iter().filter(|c| c.tag == "OBJE") {
                n += c.children.iter().filter(|p| p.tag == PRIM_TAG).count();
            }
        }
        n
    }

    /// Convertit `_PRIM Y` en GEDCOM 7 : le lien marqué passe en tête des
    /// médias de l'enregistrement, et la balise est retirée. Renvoie le nombre
    /// de balises retirées.
    pub fn migrate_prim_tags(&mut self) -> usize {
        let mut n = 0;
        let has_prim = |r: &Node| r.children.iter().any(|c| c.tag == "OBJE" && c.child(PRIM_TAG).is_some());
        for r in self.records_where_mut(has_prim) {
            let links: Vec<usize> =
                r.children.iter().enumerate().filter(|(_, c)| c.tag == "OBJE").map(|(i, _)| i).collect();
            let Some(&first) = links.first() else { continue };
            let primary = links.iter().copied().find(|&i| {
                r.children[i].children.iter().any(|p| p.tag == PRIM_TAG && p.val().eq_ignore_ascii_case("Y"))
            });
            let mut touched = false;
            for &i in &links {
                let before = r.children[i].children.len();
                r.children[i].children.retain(|p| p.tag != PRIM_TAG);
                let removed = before - r.children[i].children.len();
                n += removed;
                touched |= removed > 0;
            }
            if let Some(p) = primary.filter(|&p| p != first) {
                let link = r.children.remove(p);
                r.children.insert(first, link);
            }
            if touched {
                r.touch();
            }
        }
        n
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_french_event_lists() {
        assert_eq!(parse_event_list("Naissances, décès").unwrap(), vec!["BIRT", "DEAT"]);
        assert_eq!(parse_event_list("actes de mariage; baptême").unwrap(), vec!["MARR", "BAPM"]);
        assert_eq!(parse_event_list("BIRT, birt").unwrap(), vec!["BIRT"]);
        assert_eq!(parse_event_list("recensement").unwrap(), vec!["CENS"]);
        assert!(parse_event_list("naissances, gâteaux").is_err());
    }

    #[test]
    fn record_widens_period() {
        let mut s = Node::new("SOUR");
        assert!(record_in_source(&mut s, "BIRT", "12 MAR 1795"));
        assert_eq!(s.path_val(&["DATA", "EVEN"]), Some("BIRT"));
        assert_eq!(s.path_val(&["DATA", "EVEN", "DATE"]), Some("FROM 1795 TO 1795"));
        assert!(record_in_source(&mut s, "DEAT", "1802"));
        assert!(!record_in_source(&mut s, "BIRT", "1799"));
        assert_eq!(s.path_val(&["DATA", "EVEN"]), Some("BIRT, DEAT"));
        assert_eq!(s.path_val(&["DATA", "EVEN", "DATE"]), Some("FROM 1795 TO 1802"));
        let c = SourceContent::read(&s);
        assert_eq!(c.events_label(), "Naissance, décès");
        assert!(!record_in_source(&mut s, "NOPE", "1800"));
    }

    #[test]
    fn content_roundtrip_keeps_agency() {
        let mut s = Node::new("SOUR");
        s.ensure_child("DATA").children.push(Node::with_value("AGNC", "Mairie"));
        let c = SourceContent { events: vec!["MARR".into()], period: "FROM 1800 TO 1810".into(), place: String::new() };
        c.apply(&mut s);
        assert_eq!(SourceContent::read(&s), c);
        assert_eq!(s.path_val(&["DATA", "AGNC"]), Some("Mairie"));
        SourceContent::default().apply(&mut s);
        assert!(s.path_val(&["DATA", "EVEN"]).is_none());
        assert_eq!(s.path_val(&["DATA", "AGNC"]), Some("Mairie"));
        SourceContent::default().apply(&mut Node::new("SOUR"));
    }

    #[test]
    fn outside_period_is_certain_only() {
        let mut s = Node::new("SOUR");
        record_in_source(&mut s, "BIRT", "1795");
        record_in_source(&mut s, "BIRT", "1802");
        assert!(outside_period(&s, "BIRT", "1850").is_some());
        assert!(outside_period(&s, "BIRT", "1800").is_none());
        assert!(outside_period(&s, "BIRT", "ABT 1804").is_none());
        assert!(outside_period(&s, "DEAT", "1850").is_none());
    }

    #[test]
    fn medium_needs_form() {
        let mut m = Node::new("OBJE");
        m.children.push(Node::with_value("FILE", "Portraits/a.jpg"));
        set_medium(&mut m, "PHOTO");
        assert_eq!(m.path_val(&["FILE", "FORM"]), Some("image/jpeg"));
        assert_eq!(medium(&m), Some("PHOTO"));
        set_medium(&mut m, "");
        assert_eq!(medium(&m), None);
        assert_eq!(m.path_val(&["FILE", "FORM"]), Some("image/jpeg"));
    }
}
