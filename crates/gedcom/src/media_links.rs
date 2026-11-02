//! Rattachements des médias, pour la fenêtre « Médias » : ce que montre la
//! galerie d'un individu, d'une famille, d'un événement ou d'une source, et où
//! se joint un média importé ou lié depuis elle.
//!
//! Un média peut être lié :
//! - à un enregistrement (`1 OBJE` sous `INDI`, `FAM`, `SOUR`) : portrait,
//!   photo de famille, image d'une source qui ne contient qu'un acte ;
//! - à un événement (`2 OBJE` sous `BIRT`, `MARR`…) : faire-part, photo ;
//! - à un acte d'une source : `OBJE` sous chaque citation de l'acte (même
//!   `PAGE` et même date), et sur la source, comme le fait la saisie d'acte.
//!
//! Le nom du fichier (Réglages du fichier › Médias) se déduit ensuite de ces
//! liens, voir [`crate::media_naming`].

use std::collections::HashSet;

use crate::act::{ActKey, key_of};
use crate::document::Gedcom;
use crate::model::{Event, Media, event_label};
use crate::node::Node;

/// Où un média est lié.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum MediaHost {
    /// `OBJE` au niveau 1 d'un individu, d'une famille ou d'une source.
    Record(String),
    /// `OBJE` sous l'événement `ix` de `owner`.
    Event { owner: String, ix: usize },
    /// `OBJE` sous les citations d'un acte de la source.
    Act { source: String, key: ActKey },
}

/// Ce dont on ouvre la galerie.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GalleryTarget {
    Record(String),
    Event { owner: String, ix: usize },
}

/// Un groupe de la galerie : un événement, un acte, les portraits…
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GalleryGroup {
    pub title: String,
    pub subtitle: String,
    /// Où se joint un fichier déposé sur le groupe (`None` : groupe en lecture seule).
    pub host: Option<MediaHost>,
    /// Médias du groupe, avec le lien qu'on défait pour les détacher.
    pub items: Vec<(String, MediaHost)>,
}

fn ptrs(n: &Node) -> Vec<String> {
    n.children.iter().filter(|c| c.tag == "OBJE").filter_map(|c| c.pointer()).map(str::to_string).collect()
}

fn is_event(n: &Node) -> bool {
    event_label(&n.tag).is_some()
}

impl Gedcom {
    /// Média local (pas une page web).
    fn is_file_media(&self, m: &str) -> bool {
        self.get(m).is_some_and(|n| n.tag == "OBJE" && !Media::new(n).is_web())
    }

    /// Citations de la source dans les individus et familles (niveau 1 ou sous un
    /// événement), avec leur acte.
    fn source_citations(&self, source: &str) -> Vec<(&Node, ActKey)> {
        let p = format!("@{source}@");
        let mut out = Vec::new();
        for r in self.records.iter().filter(|r| r.tag == "INDI" || r.tag == "FAM") {
            for c in &r.children {
                if c.tag == "SOUR" && c.val() == p {
                    out.push((c, key_of(c)));
                } else {
                    for cc in c.children.iter().filter(|cc| cc.tag == "SOUR" && cc.val() == p) {
                        out.push((cc, key_of(cc)));
                    }
                }
            }
        }
        out
    }

    /// Une source « registre » contient plusieurs actes, distingués par leur
    /// référence (`PAGE`) : ses images se rangent par acte. Une source sans
    /// référence, ou avec une seule, décrit un acte : galerie simple, images
    /// sur la source (nommées d'après l'événement qui la cite).
    pub fn is_register(&self, source: &str) -> bool {
        let mut pages: Vec<&str> = Vec::new();
        for (c, _) in self.source_citations(source) {
            let p = c.child_val("PAGE").unwrap_or("").trim();
            if !p.is_empty() && !pages.contains(&p) {
                pages.push(p);
                if pages.len() > 1 {
                    return true;
                }
            }
        }
        false
    }

    /// Où joindre l'image de l'acte que cite `c` : ses citations dans un
    /// registre, la source elle-même sinon.
    fn cited_act_host(&self, source: &str, c: &Node) -> MediaHost {
        if self.is_register(source) {
            MediaHost::Act { source: source.to_string(), key: key_of(c) }
        } else {
            MediaHost::Record(source.to_string())
        }
    }

    /// Médias d'un acte : ceux de ses citations.
    fn act_media(&self, source: &str, key: &ActKey) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        for (c, k) in self.source_citations(source) {
            if &k == key {
                for m in ptrs(c) {
                    if !out.contains(&m) && self.is_file_media(&m) {
                        out.push(m);
                    }
                }
            }
        }
        out
    }

    /// Médias portés par la source qu'aucun acte ne revendique (couverture, tables…).
    fn loose_source_media(&self, source: &str) -> Vec<String> {
        let claimed: HashSet<String> = self.source_citations(source).into_iter().flat_map(|(c, _)| ptrs(c)).collect();
        let mut out: Vec<String> = Vec::new();
        for m in self.get(source).map(ptrs).unwrap_or_default() {
            if !claimed.contains(&m) && !out.contains(&m) && self.is_file_media(&m) {
                out.push(m);
            }
        }
        out
    }

    /// Médias d'un événement : les siens, ceux des actes qu'il cite, puis ceux que
    /// portent les sources citées (sauf ceux d'un autre acte d'un registre).
    fn event_groups(&self, owner: &str, ix: usize, e: &Node) -> Vec<GalleryGroup> {
        let mut groups = Vec::new();
        let mut seen: HashSet<String> = HashSet::new();
        for c in e.children.iter().filter(|c| c.tag == "SOUR") {
            let Some(s) = c.pointer() else { continue };
            if self.get(s).is_none_or(|n| n.tag != "SOUR") {
                continue;
            }
            let key = key_of(c);
            let act = MediaHost::Act { source: s.to_string(), key: key.clone() };
            let mut items: Vec<(String, MediaHost)> = Vec::new();
            for m in self.act_media(s, &key) {
                if seen.insert(m.clone()) {
                    items.push((m, act.clone()));
                }
            }
            let host = self.cited_act_host(s, c);
            let carried = if self.is_register(s) {
                self.loose_source_media(s)
            } else {
                self.get(s).map(ptrs).unwrap_or_default().into_iter().filter(|m| self.is_file_media(m)).collect()
            };
            for m in carried {
                if seen.insert(m.clone()) {
                    items.push((m, MediaHost::Record(s.to_string())));
                }
            }
            groups.push(GalleryGroup {
                title: self.record_label(s),
                subtitle: key.page.clone(),
                host: Some(host),
                items,
            });
        }
        let own: Vec<(String, MediaHost)> = ptrs(e)
            .into_iter()
            .filter(|m| self.is_file_media(m) && seen.insert(m.clone()))
            .map(|m| (m, MediaHost::Event { owner: owner.to_string(), ix }))
            .collect();
        // Un événement sourcé range ses médias dans l'acte : le groupe propre à
        // l'événement n'apparaît que s'il porte déjà des médias.
        if !own.is_empty() || groups.is_empty() {
            groups.insert(
                0,
                GalleryGroup {
                    title: "Événement".into(),
                    subtitle: String::new(),
                    host: Some(MediaHost::Event { owner: owner.to_string(), ix }),
                    items: own,
                },
            );
        }
        groups
    }

    /// Titre court d'un événement : « Naissance · 1er oct. 1902 ».
    pub fn event_heading(&self, owner: &str, ix: usize) -> String {
        let Some(e) = self.get(owner).and_then(|n| n.children.get(ix)) else { return String::new() };
        let title = self.event_title(owner, ix).unwrap_or_default();
        match (Event { node: e }).date() {
            Some(d) => format!("{title} · {}", d.display()),
            None => title,
        }
    }

    /// Groupes de la galerie, du plus direct au plus lointain. Les groupes vides
    /// sont gardés quand on peut y déposer des fichiers (événement, acte).
    pub fn gallery(&self, target: &GalleryTarget) -> Vec<GalleryGroup> {
        match target {
            GalleryTarget::Event { owner, ix } => {
                let Some(e) = self.get(owner).and_then(|n| n.children.get(*ix)).filter(|e| is_event(e)) else {
                    return Vec::new();
                };
                self.event_groups(owner, *ix, e)
            }
            GalleryTarget::Record(x) => {
                let Some(n) = self.get(x) else { return Vec::new() };
                match n.tag.as_str() {
                    "SOUR" => self.source_groups(x),
                    "INDI" | "FAM" => self.person_groups(x, n),
                    _ => Vec::new(),
                }
            }
        }
    }

    fn source_groups(&self, source: &str) -> Vec<GalleryGroup> {
        let host = MediaHost::Record(source.to_string());
        if !self.is_register(source) {
            let mut items: Vec<(String, MediaHost)> = Vec::new();
            let mut seen = HashSet::new();
            for (c, k) in self.source_citations(source) {
                for m in ptrs(c) {
                    if self.is_file_media(&m) && seen.insert(m.clone()) {
                        items.push((m, MediaHost::Act { source: source.to_string(), key: k.clone() }));
                    }
                }
            }
            for m in self.get(source).map(ptrs).unwrap_or_default() {
                if self.is_file_media(&m) && seen.insert(m.clone()) {
                    items.push((m, host.clone()));
                }
            }
            return vec![GalleryGroup { title: String::new(), subtitle: String::new(), host: Some(host), items }];
        }
        let mut groups = vec![GalleryGroup {
            title: rust_i18n::t!("media_links.whole_register").into(),
            subtitle: String::new(),
            host: Some(host.clone()),
            items: self.loose_source_media(source).into_iter().map(|m| (m, host.clone())).collect(),
        }];
        for act in self.acts(source) {
            let key = act.original.clone().unwrap_or_default();
            let host = MediaHost::Act { source: source.to_string(), key: key.clone() };
            groups.push(GalleryGroup {
                title: act.title(),
                subtitle: act.subtitle(),
                host: Some(host.clone()),
                items: self.act_media(source, &key).into_iter().map(|m| (m, host.clone())).collect(),
            });
        }
        groups
    }

    /// Individu ou famille : ses médias, ceux de ses unions (individu), puis
    /// événement par événement. Les groupes d'événements vides sont omis.
    fn person_groups(&self, x: &str, n: &Node) -> Vec<GalleryGroup> {
        let mut seen: HashSet<String> = HashSet::new();
        let host = MediaHost::Record(x.to_string());
        let own: Vec<(String, MediaHost)> = ptrs(n)
            .into_iter()
            .filter(|m| self.is_file_media(m) && seen.insert(m.clone()))
            .map(|m| (m, host.clone()))
            .collect();
        let mut groups = vec![GalleryGroup {
            title: if n.tag == "INDI" {
                rust_i18n::t!("media_links.portraits").into()
            } else {
                rust_i18n::t!("media_links.union_photos").into()
            },
            subtitle: String::new(),
            host: Some(host),
            items: own,
        }];
        if n.tag == "INDI" {
            for f in n.children.iter().filter(|c| c.tag == "FAMS").filter_map(|c| c.pointer()) {
                let Some(fam) = self.get(f) else { continue };
                let host = MediaHost::Record(f.to_string());
                let items: Vec<(String, MediaHost)> = ptrs(fam)
                    .into_iter()
                    .filter(|m| self.is_file_media(m) && seen.insert(m.clone()))
                    .map(|m| (m, host.clone()))
                    .collect();
                if !items.is_empty() {
                    groups.push(GalleryGroup {
                        title: self.record_label(f),
                        subtitle: String::new(),
                        host: Some(host),
                        items,
                    });
                }
            }
        }
        // Événements dans l'ordre chronologique, comme dans la fiche.
        let events: Vec<(String, usize)> = if n.tag == "INDI" {
            self.person_events(x).into_iter().map(|e| (e.owner, e.index)).collect()
        } else {
            let mut evs = self.events_of(x);
            evs.sort_by_key(|(_, e)| e.date().map(|d| d.sort_key()).unwrap_or((i32::MAX, 0, 0)));
            evs.into_iter().map(|(ix, _)| (x.to_string(), ix)).collect()
        };
        for (o, ix) in events {
            let Some(e) = self.get(&o).and_then(|r| r.children.get(ix)) else { continue };
            let mut items: Vec<(String, MediaHost)> = Vec::new();
            for g in self.event_groups(&o, ix, e) {
                for (m, h) in g.items {
                    if seen.insert(m.clone()) {
                        items.push((m, h));
                    }
                }
            }
            if !items.is_empty() {
                groups.push(GalleryGroup {
                    title: self.event_heading(&o, ix),
                    subtitle: String::new(),
                    host: Some(MediaHost::Event { owner: o.clone(), ix }),
                    items,
                });
            }
        }
        groups
    }

    /// Où joindre un fichier importé ou lié depuis la galerie, le choix par défaut
    /// en premier : pour un événement, l'acte qu'il cite, puis l'événement, puis
    /// la personne ou l'union.
    pub fn media_hosts(&self, target: &GalleryTarget) -> Vec<(String, MediaHost)> {
        match target {
            GalleryTarget::Event { owner, ix } => {
                let Some(e) = self.get(owner).and_then(|n| n.children.get(*ix)).filter(|e| is_event(e)) else {
                    return Vec::new();
                };
                let mut out: Vec<(String, MediaHost)> = Vec::new();
                for c in e.children.iter().filter(|c| c.tag == "SOUR") {
                    let Some(s) = c.pointer().filter(|s| self.get(s).is_some_and(|n| n.tag == "SOUR")) else {
                        continue;
                    };
                    let host = self.cited_act_host(s, c);
                    let label = match &host {
                        MediaHost::Act { key, .. } if !key.page.is_empty() => {
                            format!("{} · {}", self.record_label(s), key.page)
                        }
                        _ => self.record_label(s),
                    };
                    if !out.iter().any(|(_, h)| *h == host) {
                        out.push((label, host));
                    }
                }
                let what = self.event_title(owner, *ix).unwrap_or_default();
                out.push((
                    format!("L'événement {}", what.to_lowercase()),
                    MediaHost::Event { owner: owner.clone(), ix: *ix },
                ));
                out.push((self.record_label(owner), MediaHost::Record(owner.clone())));
                out
            }
            GalleryTarget::Record(x) => {
                let Some(n) = self.get(x) else { return Vec::new() };
                match n.tag.as_str() {
                    "SOUR" if self.is_register(x) => {
                        let mut out = vec![(
                            rust_i18n::t!("media_links.whole_register").into_owned(),
                            MediaHost::Record(x.clone()),
                        )];
                        for act in self.acts(x) {
                            let key = act.original.clone().unwrap_or_default();
                            out.push((act.title(), MediaHost::Act { source: x.clone(), key }));
                        }
                        out
                    }
                    "INDI" | "FAM" => {
                        // La personne, puis chacun de ses événements, par ordre
                        // chronologique. Un événement sourcé propose l'acte cité
                        // (là où va son média par défaut), une seule fois.
                        let mut out = vec![(self.record_label(x), MediaHost::Record(x.clone()))];
                        let events: Vec<(String, usize)> = if n.tag == "INDI" {
                            self.person_events(x).into_iter().map(|e| (e.owner, e.index)).collect()
                        } else {
                            self.events_of(x).into_iter().map(|(ix, _)| (x.clone(), ix)).collect()
                        };
                        for (owner, ix) in events {
                            let target = GalleryTarget::Event { owner: owner.clone(), ix };
                            if let Some((_, host)) = self.media_hosts(&target).into_iter().next()
                                && !out.iter().any(|(_, h)| *h == host)
                            {
                                let label = match &host {
                                    MediaHost::Event { .. } => self.event_heading(&owner, ix),
                                    MediaHost::Act { source, key } if !key.page.is_empty() => {
                                        format!("{} · {}", self.record_label(source), key.page)
                                    }
                                    MediaHost::Act { source: x, .. } | MediaHost::Record(x) => self.record_label(x),
                                };
                                out.push((label, host));
                            }
                        }
                        out
                    }
                    "SOUR" => vec![(self.record_label(x), MediaHost::Record(x.clone()))],
                    _ => Vec::new(),
                }
            }
        }
    }

    /// Lie un média là où l'indique `host` (sans doublon).
    pub fn link_media(&mut self, host: &MediaHost, media: &str) -> Result<(), String> {
        if self.get(media).is_none_or(|m| m.tag != "OBJE") {
            return Err(rust_i18n::t!("error.media_not_found").into());
        }
        match host {
            MediaHost::Record(x) => self.attach_media(x, media),
            MediaHost::Event { owner, ix } => self.attach_media_to_event(owner, *ix, media),
            MediaHost::Act { source, key } => {
                let p = format!("@{media}@");
                let sp = format!("@{source}@");
                let mut found = false;
                let cites = |c: &Node| c.tag == "SOUR" && c.val() == sp && key_of(c) == *key;
                let holds = |r: &Node| (r.tag == "INDI" || r.tag == "FAM") && r.any(&cites);
                for r in self.records_where_mut(holds) {
                    let mut touched = false;
                    for c in r.children.iter_mut() {
                        let mut targets: Vec<&mut Node> = Vec::new();
                        if cites(c) {
                            targets.push(c);
                        } else {
                            targets.extend(c.children.iter_mut().filter(|cc| cites(cc)));
                        }
                        for t in targets {
                            found = true;
                            if !t.children.iter().any(|o| o.tag == "OBJE" && o.val() == p) {
                                t.children.push(Node::with_value("OBJE", p.clone()));
                                touched = true;
                            }
                        }
                    }
                    if touched {
                        r.touch();
                    }
                }
                if !found {
                    return Err(rust_i18n::t!("error.act_not_found").into());
                }
                // Comme la saisie d'acte : la source porte aussi l'image.
                self.attach_media(source, media)
            }
        }
    }

    /// Défait le lien `host` → média (le média reste dans le fichier).
    pub fn unlink_media(&mut self, host: &MediaHost, media: &str) -> Result<(), String> {
        match host {
            MediaHost::Record(x) => {
                self.detach_media(x, media);
                Ok(())
            }
            MediaHost::Event { owner, ix } => self.detach_media_from_event(owner, *ix, media),
            MediaHost::Act { source, key } => {
                let p = format!("@{media}@");
                let sp = format!("@{source}@");
                let cites = |c: &Node| c.tag == "SOUR" && c.val() == sp && key_of(c) == *key;
                let holds = |r: &Node| (r.tag == "INDI" || r.tag == "FAM") && r.any(&cites);
                for r in self.records_where_mut(holds) {
                    let mut touched = false;
                    for c in r.children.iter_mut() {
                        let mut targets: Vec<&mut Node> = Vec::new();
                        if cites(c) {
                            targets.push(c);
                        } else {
                            targets.extend(c.children.iter_mut().filter(|cc| cites(cc)));
                        }
                        for t in targets {
                            let before = t.children.len();
                            t.children.retain(|o| !(o.tag == "OBJE" && o.val() == p));
                            touched |= t.children.len() != before;
                        }
                    }
                    if touched {
                        r.touch();
                    }
                }
                // La source garde l'image si un autre acte l'utilise encore.
                let still = self.source_citations(source).iter().any(|(c, _)| ptrs(c).iter().any(|m| m == media));
                if !still {
                    self.detach_media(source, media);
                }
                Ok(())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const G: &str = "0 HEAD\n1 GEDC\n2 VERS 7.0\n\
0 @I1@ INDI\n1 NAME Rémi /GUYOT/\n1 BIRT\n2 DATE 1 OCT 1902\n2 SOUR @S1@\n3 PAGE Acte n° 14\n1 OBJE @M1@\n\
0 @I2@ INDI\n1 NAME Marcel /HURAUX/\n1 BIRT\n2 DATE 3 OCT 1902\n2 SOUR @S1@\n3 PAGE Acte n° 15\n\
0 @I3@ INDI\n1 NAME Jeanne /DORME/\n1 DEAT\n2 SOUR @S2@\n2 OBJE @M4@\n\
0 @S1@ SOUR\n1 TITL Registre des naissances, 1902\n1 OBJE @M2@\n\
0 @S2@ SOUR\n1 TITL Acte de décès de Jeanne DORME\n1 OBJE @M3@\n\
0 @M1@ OBJE\n1 FILE Portraits/remi.jpg\n2 FORM image/jpeg\n\
0 @M2@ OBJE\n1 FILE couverture.jpg\n2 FORM image/jpeg\n\
0 @M3@ OBJE\n1 FILE deces.jpg\n2 FORM image/jpeg\n\
0 @M4@ OBJE\n1 FILE faire-part.jpg\n2 FORM image/jpeg\n\
0 @M5@ OBJE\n1 FILE nouveau.jpg\n2 FORM image/jpeg\n0 TRLR\n";

    fn ged() -> Gedcom {
        Gedcom::parse(G).unwrap()
    }

    fn key(page: &str) -> ActKey {
        ActKey { page: page.into(), date: String::new() }
    }

    #[test]
    fn register_media_go_to_the_act_citations() {
        let mut g = ged();
        assert!(g.is_register("S1"));
        assert!(!g.is_register("S2"));
        let host = MediaHost::Act { source: "S1".into(), key: key("Acte n° 15") };
        g.link_media(&host, "M5").unwrap();
        let groups = g.gallery(&GalleryTarget::Record("S1".into()));
        assert_eq!(groups[0].title, "Whole Register");
        assert_eq!(groups[0].items, vec![("M2".to_string(), MediaHost::Record("S1".into()))]);
        let act15 = groups.iter().find(|gr| gr.subtitle.contains("15") || gr.title.contains("15"));
        assert!(act15.is_some_and(|gr| gr.items == vec![("M5".to_string(), host.clone())]));
        // Galerie de l'événement de Rémi : pas l'image de l'acte n° 15.
        let ev = g.gallery(&GalleryTarget::Event { owner: "I1".into(), ix: 1 });
        assert!(!ev.iter().flat_map(|gr| &gr.items).any(|(m, _)| m == "M5"));
        g.unlink_media(&host, "M5").unwrap();
        assert!(!g.get("S1").unwrap().children.iter().any(|c| c.val() == "@M5@"));
    }

    #[test]
    fn event_gallery_and_default_host() {
        let g = ged();
        let t = GalleryTarget::Event { owner: "I3".into(), ix: 1 };
        let groups = g.gallery(&t);
        assert_eq!(groups[0].items, vec![("M4".to_string(), MediaHost::Event { owner: "I3".into(), ix: 1 })]);
        assert_eq!(groups[1].items, vec![("M3".to_string(), MediaHost::Record("S2".into()))]);
        // Source d'un seul acte : l'image va sur la source.
        let hosts = g.media_hosts(&t);
        assert_eq!(hosts[0].1, MediaHost::Record("S2".into()));
        assert_eq!(hosts.last().unwrap().1, MediaHost::Record("I3".into()));
        // Galerie de la personne : portrait, puis l'événement.
        let p = g.gallery(&GalleryTarget::Record("I1".into()));
        assert_eq!(p[0].items[0].0, "M1");
        let d = g.gallery(&GalleryTarget::Record("I3".into()));
        assert_eq!(d.iter().flat_map(|gr| gr.items.iter().map(|(m, _)| m.clone())).collect::<Vec<_>>(), ["M4", "M3"]);
    }

    #[test]
    fn person_hosts_offer_the_cited_acts() {
        let g = ged();
        let hosts: Vec<(String, MediaHost)> = g.media_hosts(&GalleryTarget::Record("I1".into()));
        assert_eq!(hosts[0].1, MediaHost::Record("I1".into()));
        assert_eq!(hosts[1].1, MediaHost::Act { source: "S1".into(), key: key("Acte n° 14") });
        assert_eq!(hosts[1].0, "Registre des naissances, 1902 · Acte n° 14");
        // Événement sourcé sans média propre : seul le groupe de l'acte.
        let ev = g.gallery(&GalleryTarget::Event { owner: "I1".into(), ix: 1 });
        assert!(ev.iter().all(|gr| gr.title != "Événement"));
    }
}
