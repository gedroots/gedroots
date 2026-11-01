//! Vues typées sur les enregistrements les plus courants, et leurs éditions.
//!
//! Les vues empruntent le [`Node`] sous-jacent ; les formulaires (`*Form`)
//! sont des copies modifiables qu'on réapplique sur le nœud avec `apply`,
//! en ne touchant qu'aux champs qu'ils représentent.

use crate::date::Date;
use crate::document::Gedcom;
use crate::node::Node;

/// Balises d'événements et d'attributs individuels/familiaux, avec leur libellé.
pub const EVENT_TAGS: &[(&str, &str)] = &[
    ("BIRT", "Naissance"),
    ("CHR", "Baptême"),
    ("BAPM", "Baptême"),
    ("DEAT", "Décès"),
    ("BURI", "Inhumation"),
    ("CREM", "Crémation"),
    ("ADOP", "Adoption"),
    ("CONF", "Confirmation"),
    ("FCOM", "Première communion"),
    ("NATU", "Naturalisation"),
    ("EMIG", "Émigration"),
    ("IMMI", "Immigration"),
    ("CENS", "Recensement"),
    ("PROB", "Succession"),
    ("WILL", "Testament"),
    ("GRAD", "Diplôme"),
    ("RETI", "Retraite"),
    ("RESI", "Résidence"),
    ("OCCU", "Profession"),
    ("EDUC", "Études"),
    ("RELI", "Religion"),
    ("TITL", "Titre"),
    ("NATI", "Nationalité"),
    ("FACT", "Fait"),
    ("EVEN", "Événement"),
    ("MARR", "Mariage"),
    ("MARB", "Publication des bans"),
    ("MARC", "Contrat de mariage"),
    ("MARL", "Licence de mariage"),
    ("ENGA", "Fiançailles"),
    ("DIV", "Divorce"),
    ("DIVF", "Dépôt de divorce"),
    ("ANUL", "Annulation"),
];

pub fn event_label(tag: &str) -> Option<&'static str> {
    EVENT_TAGS.iter().find(|(t, _)| *t == tag).map(|(_, l)| *l)
}

/// Un événement ou attribut (BIRT, DEAT, OCCU, RESI, MARR…).
#[derive(Debug, Clone)]
pub struct Event<'a> {
    pub node: &'a Node,
}

impl<'a> Event<'a> {
    pub fn tag(&self) -> &str {
        &self.node.tag
    }

    pub fn label(&self) -> String {
        let base = event_label(&self.node.tag).unwrap_or(&self.node.tag);
        match (self.node.tag.as_str(), self.node.child_val("TYPE")) {
            ("EVEN" | "FACT", Some(t)) => t.to_string(),
            _ => base.to_string(),
        }
    }

    /// Valeur de l'attribut (ex. la profession pour OCCU).
    pub fn value(&self) -> Option<&str> {
        self.node.value.as_deref().filter(|v| !v.is_empty() && *v != "Y")
    }

    pub fn date_raw(&self) -> Option<&str> {
        self.node.child_val("DATE")
    }

    pub fn date(&self) -> Option<Date> {
        self.date_raw().map(Date::parse)
    }

    pub fn place(&self) -> Option<&str> {
        self.node.child_val("PLAC")
    }

    pub fn source_xrefs(&self) -> Vec<&str> {
        self.node.children_with("SOUR").filter_map(|s| s.pointer()).collect()
    }

    pub fn is_sourced(&self) -> bool {
        self.node.child("SOUR").is_some()
    }
}

/// Vue sur un enregistrement `INDI`.
#[derive(Debug, Clone, Copy)]
pub struct Individual<'a> {
    pub node: &'a Node,
}

impl<'a> Individual<'a> {
    pub fn new(node: &'a Node) -> Self {
        Self { node }
    }

    pub fn xref(&self) -> &str {
        self.node.xref.as_deref().unwrap_or("")
    }

    fn name_node(&self) -> Option<&'a Node> {
        self.node.child("NAME")
    }

    /// Nom de famille : `SURN` sinon la partie entre `/…/`.
    pub fn surname(&self) -> String {
        let Some(n) = self.name_node() else { return String::new() };
        if let Some(s) = n.child_val("SURN") {
            return s.to_string();
        }
        let v = n.val();
        match (v.find('/'), v.rfind('/')) {
            (Some(a), Some(b)) if b > a => v[a + 1..b].trim().to_string(),
            _ => String::new(),
        }
    }

    pub fn given(&self) -> String {
        let Some(n) = self.name_node() else { return String::new() };
        if let Some(s) = n.child_val("GIVN") {
            return s.to_string();
        }
        let v = n.val();
        v.split('/').next().unwrap_or("").trim().to_string()
    }

    /// « Prénom NOM ».
    pub fn display_name(&self) -> String {
        let (g, s) = (self.given(), self.surname());
        match (g.is_empty(), s.is_empty()) {
            (true, true) => "(sans nom)".into(),
            (true, false) => s,
            (false, true) => g,
            _ => format!("{g} {s}"),
        }
    }

    pub fn sex(&self) -> &str {
        self.node.child_val("SEX").unwrap_or("U")
    }

    pub fn sex_label(&self) -> &'static str {
        match self.sex() {
            "M" => "Homme",
            "F" => "Femme",
            "X" => "Autre",
            _ => "Inconnu",
        }
    }

    pub fn events(&self) -> Vec<Event<'a>> {
        self.node.children.iter().filter(|c| event_label(&c.tag).is_some()).map(|node| Event { node }).collect()
    }

    pub fn event(&self, tag: &str) -> Option<Event<'a>> {
        self.node.child(tag).map(|node| Event { node })
    }

    pub fn birth(&self) -> Option<Event<'a>> {
        self.event("BIRT").or_else(|| self.event("CHR")).or_else(|| self.event("BAPM"))
    }

    pub fn death(&self) -> Option<Event<'a>> {
        self.event("DEAT").or_else(|| self.event("BURI"))
    }

    /// « 1850–1912 », avec « ? » pour les bornes inconnues.
    pub fn years(&self) -> String {
        let y = |e: Option<Event>| e.and_then(|e| e.date()).map(|d| d.short_year());
        match (y(self.birth()), y(self.death())) {
            (None, None) => String::new(),
            (b, d) => format!("{}–{}", b.unwrap_or("?".into()), d.unwrap_or_default()),
        }
    }

    pub fn birth_year(&self) -> Option<i32> {
        self.birth().and_then(|e| e.date()).and_then(|d| d.year())
    }

    pub fn famc(&self) -> Vec<&'a str> {
        self.node.children_with("FAMC").filter_map(|n| n.pointer()).collect()
    }

    pub fn fams(&self) -> Vec<&'a str> {
        self.node.children_with("FAMS").filter_map(|n| n.pointer()).collect()
    }

    pub fn media_xrefs(&self) -> Vec<&'a str> {
        self.node.children_with("OBJE").filter_map(|n| n.pointer()).collect()
    }

    /// Numéro Sosa calculé par Ancestris (`_SOSADABOVILLE 12 G4`).
    pub fn sosa(&self) -> Option<String> {
        self.node.child_val("_SOSADABOVILLE").and_then(|v| v.split_whitespace().next()).map(str::to_string)
    }

    /// Nombre d'événements sourcés / total.
    pub fn sourced_ratio(&self) -> (usize, usize) {
        let ev = self.events();
        (ev.iter().filter(|e| e.is_sourced()).count(), ev.len())
    }

    /// Toutes les sources citées (directement ou dans un événement).
    pub fn cited_sources(&self) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        self.node.walk(&mut |n| {
            if n.tag == "SOUR"
                && let Some(p) = n.pointer()
                && !out.iter().any(|x| x == p)
            {
                out.push(p.to_string());
            }
        });
        out
    }

    pub fn notes(&self) -> Vec<String> {
        self.node.children_with("NOTE").map(|n| n.text()).collect()
    }
}

/// Vue sur un enregistrement `FAM`.
#[derive(Debug, Clone, Copy)]
pub struct Family<'a> {
    pub node: &'a Node,
}

impl<'a> Family<'a> {
    pub fn new(node: &'a Node) -> Self {
        Self { node }
    }
    pub fn xref(&self) -> &str {
        self.node.xref.as_deref().unwrap_or("")
    }
    pub fn husband(&self) -> Option<&'a str> {
        self.node.child("HUSB").and_then(|n| n.pointer())
    }
    pub fn wife(&self) -> Option<&'a str> {
        self.node.child("WIFE").and_then(|n| n.pointer())
    }
    pub fn children(&self) -> Vec<&'a str> {
        self.node.children_with("CHIL").filter_map(|n| n.pointer()).collect()
    }
    pub fn events(&self) -> Vec<Event<'a>> {
        self.node.children.iter().filter(|c| event_label(&c.tag).is_some()).map(|node| Event { node }).collect()
    }
    pub fn marriage(&self) -> Option<Event<'a>> {
        self.node.child("MARR").map(|node| Event { node })
    }
}

/// Vue sur un enregistrement `SOUR`.
#[derive(Debug, Clone, Copy)]
pub struct Source<'a> {
    pub node: &'a Node,
}

impl<'a> Source<'a> {
    pub fn new(node: &'a Node) -> Self {
        Self { node }
    }
    pub fn xref(&self) -> &str {
        self.node.xref.as_deref().unwrap_or("")
    }
    pub fn title(&self) -> String {
        self.node.child("TITL").map(|n| n.text()).unwrap_or_else(|| "(sans titre)".into())
    }
    pub fn author(&self) -> Option<String> {
        self.node.child("AUTH").map(|n| n.text())
    }
    pub fn publication(&self) -> Option<String> {
        self.node.child("PUBL").map(|n| n.text())
    }
    pub fn repository(&self) -> Option<&'a str> {
        self.node.child("REPO").and_then(|n| n.pointer())
    }
    pub fn call_number(&self) -> Option<&'a str> {
        self.node.child("REPO").and_then(|n| n.child_val("CALN"))
    }
    pub fn url(&self) -> Option<&'a str> {
        self.node.child_val("_URL").or_else(|| self.node.child_val("WWW"))
    }
    pub fn media_xrefs(&self) -> Vec<&'a str> {
        self.node.children_with("OBJE").filter_map(|n| n.pointer()).collect()
    }
}

/// Vue sur un enregistrement `REPO`.
#[derive(Debug, Clone, Copy)]
pub struct Repository<'a> {
    pub node: &'a Node,
}

impl<'a> Repository<'a> {
    pub fn new(node: &'a Node) -> Self {
        Self { node }
    }
    pub fn name(&self) -> &'a str {
        self.node.child_val("NAME").unwrap_or("(sans nom)")
    }
    pub fn www(&self) -> Option<&'a str> {
        self.node.child_val("WWW")
    }
}

/// Vue sur un enregistrement `OBJE` (photo, scan d'acte…).
#[derive(Debug, Clone, Copy)]
pub struct Media<'a> {
    pub node: &'a Node,
}

impl<'a> Media<'a> {
    pub fn new(node: &'a Node) -> Self {
        Self { node }
    }
    pub fn xref(&self) -> &str {
        self.node.xref.as_deref().unwrap_or("")
    }
    pub fn file(&self) -> Option<&'a str> {
        self.node.child_val("FILE")
    }
    pub fn form(&self) -> Option<&'a str> {
        self.node.child("FILE").and_then(|f| f.child_val("FORM"))
    }
    pub fn title(&self) -> String {
        self.node
            .child("FILE")
            .and_then(|f| f.child_val("TITL"))
            .or_else(|| self.node.child_val("TITL"))
            .filter(|t| !t.trim().is_empty())
            .map(str::to_string)
            .or_else(|| self.web_url().map(|u| crate::weblink::host(u).to_string()))
            .or_else(|| self.file().map(|f| f.rsplit('/').next().unwrap_or(f).to_string()))
            .unwrap_or_else(|| "(sans titre)".into())
    }
    pub fn is_image(&self) -> bool {
        // Une image en ligne n'a pas de fichier local à afficher.
        !self.is_web()
            && (self.form().is_some_and(|f| f.starts_with("image/"))
                || self.file().is_some_and(|f| {
                    let l = f.to_ascii_lowercase();
                    [".jpg", ".jpeg", ".png", ".gif", ".webp", ".bmp", ".tif", ".tiff"].iter().any(|e| l.ends_with(e))
                }))
    }
}

/// Un lieu distinct du fichier, agrégé à partir de toutes les balises `PLAC`.
#[derive(Debug, Clone, PartialEq)]
pub struct Place {
    pub name: String,
    pub latitude: Option<String>,
    pub longitude: Option<String>,
    /// Nombre d'occurrences dans le fichier.
    pub uses: usize,
    /// Enregistrements qui le citent.
    pub records: Vec<String>,
}

impl Place {
    /// Juridictions non vides, de la plus précise à la plus large.
    pub fn parts(&self) -> Vec<&str> {
        self.name.split(',').map(str::trim).filter(|s| !s.is_empty()).collect()
    }

    /// Libellé court : « Commune (Département) » selon le format Ancestris.
    pub fn short(&self) -> String {
        short_place(&self.name)
    }
}

/// Clé de tri insensible à la casse et aux accents courants.
pub fn sort_key(s: &str) -> String {
    s.chars()
        .flat_map(char::to_lowercase)
        .map(|c| match c {
            'à' | 'â' | 'ä' | 'á' => 'a',
            'é' | 'è' | 'ê' | 'ë' => 'e',
            'î' | 'ï' | 'í' => 'i',
            'ô' | 'ö' | 'ó' => 'o',
            'ù' | 'û' | 'ü' | 'ú' => 'u',
            'ç' => 'c',
            'ÿ' => 'y',
            c => c,
        })
        .collect()
}

/// Libellé court d'un lieu au format Ancestris
/// (`Lieudit, Commune, Code_Postal, Département, Région, Pays`) :
/// « , Troyes, 10000, Aube, Grand Est, France » → « Troyes (Aube) »,
/// « La Gare, Amions, 42260, Loire, … » → « La Gare, Amions (Loire) ».
pub fn short_place(raw: &str) -> String {
    let parts: Vec<&str> = raw.split(',').map(str::trim).collect();
    if parts.len() < 4 {
        return parts.iter().copied().filter(|s| !s.is_empty()).collect::<Vec<_>>().join(", ");
    }
    let at = |i: usize| parts.get(i).copied().filter(|s| !s.is_empty());
    let country = at(parts.len() - 1);
    let name = match (at(0), at(1)) {
        (Some(l), Some(c)) => format!("{l}, {c}"),
        (None, Some(c)) => c.to_string(),
        (Some(l), None) => l.to_string(),
        (None, None) => match (at(3), at(4)) {
            (Some(d), _) => d.to_string(),
            (None, Some(r)) => r.to_string(),
            _ => return country.unwrap_or("").to_string(),
        },
    };
    let context = [at(3), at(4), country].into_iter().flatten().find(|c| !name.ends_with(c));
    match context {
        Some(c) => format!("{name} ({c})"),
        None => name,
    }
}

impl Gedcom {
    /// Index des lieux, trié par nom.
    pub fn places(&self) -> Vec<Place> {
        let mut map: std::collections::BTreeMap<String, Place> = Default::default();
        for r in &self.records {
            let xref = r.xref.clone().unwrap_or_default();
            r.walk(&mut |n| {
                if n.tag == "PLAC" && !n.val().trim().is_empty() {
                    let p = map.entry(n.val().to_string()).or_insert_with(|| Place {
                        name: n.val().to_string(),
                        latitude: None,
                        longitude: None,
                        uses: 0,
                        records: Vec::new(),
                    });
                    p.uses += 1;
                    if !p.records.contains(&xref) {
                        p.records.push(xref.clone());
                    }
                    if let Some(map) = n.child("MAP") {
                        p.latitude = p.latitude.take().or(map.child_val("LATI").map(str::to_string));
                        p.longitude = p.longitude.take().or(map.child_val("LONG").map(str::to_string));
                    }
                }
            });
        }
        let mut v: Vec<Place> = map.into_values().collect();
        v.sort_by_cached_key(|p| sort_key(&p.short()));
        v
    }

    /// Renomme un lieu partout et met à jour ses coordonnées. Renvoie le nombre d'occurrences modifiées.
    pub fn update_place(&mut self, old: &str, new: &str, lat: &str, long: &str) -> usize {
        let mut count = 0;
        for r in &mut self.records {
            r.walk_mut(&mut |n| {
                if n.tag == "PLAC" && n.val() == old {
                    count += 1;
                    n.value = Some(new.trim().to_string());
                    if lat.trim().is_empty() && long.trim().is_empty() {
                        n.children.retain(|c| c.tag != "MAP");
                    } else {
                        let map = n.ensure_child("MAP");
                        map.set_child_val("LATI", lat);
                        map.set_child_val("LONG", long);
                    }
                }
            });
        }
        count
    }

    pub fn individual(&self, xref: &str) -> Option<Individual<'_>> {
        self.get(xref).filter(|n| n.tag == "INDI").map(Individual::new)
    }

    pub fn family(&self, xref: &str) -> Option<Family<'_>> {
        self.get(xref).filter(|n| n.tag == "FAM").map(Family::new)
    }

    /// Père et mère de l'individu (première famille d'origine).
    pub fn parents(&self, xref: &str) -> (Option<Individual<'_>>, Option<Individual<'_>>) {
        let Some(fam) = self.individual(xref).and_then(|i| i.famc().first().and_then(|f| self.family(f))) else {
            return (None, None);
        };
        (fam.husband().and_then(|x| self.individual(x)), fam.wife().and_then(|x| self.individual(x)))
    }

    /// Conjoints et enfants, par famille.
    pub fn spouses_and_children(&self, xref: &str) -> Vec<(Option<Individual<'_>>, Vec<Individual<'_>>)> {
        let Some(ind) = self.individual(xref) else { return vec![] };
        ind.fams()
            .into_iter()
            .filter_map(|f| self.family(f))
            .map(|fam| {
                let spouse = [fam.husband(), fam.wife()]
                    .into_iter()
                    .flatten()
                    .find(|x| *x != xref)
                    .and_then(|x| self.individual(x));
                let kids = fam.children().into_iter().filter_map(|c| self.individual(c)).collect();
                (spouse, kids)
            })
            .collect()
    }

    /// Ascendance en numérotation Sosa (index 1 = la personne, 2k = père, 2k+1 = mère).
    pub fn ancestry(&self, xref: &str, generations: u32) -> Vec<Option<String>> {
        let size = 1usize << generations;
        let mut out = vec![None; size];
        if size > 1 {
            out[1] = self.individual(xref).map(|i| i.xref().to_string());
        }
        for k in 1..size / 2 {
            if let Some(x) = out[k].clone() {
                let (f, m) = self.parents(&x);
                out[2 * k] = f.map(|i| i.xref().to_string());
                out[2 * k + 1] = m.map(|i| i.xref().to_string());
            }
        }
        out
    }

    /// Portrait d'un individu : le premier média image qui lui est lié,
    /// en privilégiant ceux rangés dans un dossier « Portraits » ou titrés « Portrait ».
    pub fn portrait(&self, xref: &str) -> Option<Media<'_>> {
        let ind = self.individual(xref)?;
        let images: Vec<Media> = ind
            .media_xrefs()
            .into_iter()
            .filter_map(|m| self.get(m).filter(|n| n.tag == "OBJE").map(Media::new))
            .filter(|m| m.is_image())
            .collect();
        let is_portrait = |m: &Media| {
            m.file().is_some_and(|f| f.to_lowercase().contains("portrait"))
                || m.title().to_lowercase().contains("portrait")
        };
        images.iter().find(|m| is_portrait(m)).or(images.first()).copied()
    }

    /// Médias images liés à un enregistrement (source, individu…).
    pub fn images_of(&self, xref: &str) -> Vec<Media<'_>> {
        let Some(n) = self.get(xref) else { return vec![] };
        let mut out: Vec<Media> = Vec::new();
        for p in n.children_with("OBJE").filter_map(|o| o.pointer()) {
            if let Some(m) = self.get(p).filter(|m| m.tag == "OBJE").map(Media::new)
                && m.is_image()
                && !out.iter().any(|x| x.xref() == m.xref())
            {
                out.push(m);
            }
        }
        out
    }

    /// Libellé court d'un enregistrement quelconque, pour les listes et les liens.
    pub fn record_label(&self, xref: &str) -> String {
        let Some(n) = self.get(xref) else { return "(introuvable)".to_string() };
        match n.tag.as_str() {
            "INDI" => Individual::new(n).display_name(),
            "FAM" => {
                let f = Family::new(n);
                let name = |x: Option<&str>| x.and_then(|x| self.individual(x)).map(|i| i.display_name());
                match (name(f.husband()), name(f.wife())) {
                    (Some(h), Some(w)) => format!("{h} & {w}"),
                    (Some(a), None) | (None, Some(a)) => a,
                    _ => "(famille vide)".into(),
                }
            }
            "SOUR" => Source::new(n).title(),
            "REPO" => Repository::new(n).name().to_string(),
            "OBJE" => Media::new(n).title(),
            "SNOTE" => n.text().lines().next().unwrap_or("").to_string(),
            "SUBM" => n.child_val("NAME").unwrap_or("").to_string(),
            t => t.to_string(),
        }
    }
}

// ---------------------------------------------------------------------------
// Formulaires des éditeurs visuels.
// ---------------------------------------------------------------------------

/// Date + lieu d'un événement, édités ensemble.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EventForm {
    /// Valeur GEDCOM de la date et sa phrase (`DATE.PHRASE`).
    pub date: String,
    pub phrase: String,
    pub place: String,
}

impl EventForm {
    fn read(node: &Node, tag: &str) -> Self {
        let ev = node.child(tag);
        let (date, phrase) = ev.map(crate::date::read_date).unwrap_or_default();
        Self { date, phrase, place: ev.and_then(|e| e.child_val("PLAC")).unwrap_or("").to_string() }
    }

    fn apply(&self, node: &mut Node, tag: &str) {
        let empty = self.date.trim().is_empty() && self.phrase.trim().is_empty() && self.place.trim().is_empty();
        if empty {
            // On ne supprime l'événement que s'il ne porte rien d'autre (sources, notes…).
            if let Some(i) = node.children.iter().position(|c| c.tag == tag) {
                let ev = &mut node.children[i];
                ev.children.retain(|c| c.tag != "DATE" && c.tag != "PLAC");
                if ev.children.is_empty() {
                    node.children.remove(i);
                }
            }
            return;
        }
        let ev = node.ensure_child(tag);
        crate::date::write_date(ev, &self.date, &self.phrase);
        // Le lieu garde ses coordonnées si son libellé ne change pas.
        let place = self.place.trim();
        match ev.child_mut("PLAC") {
            Some(p) if p.val() == place => {}
            Some(p) if !place.is_empty() => {
                p.value = Some(place.to_string());
                p.children.retain(|c| c.tag != "MAP");
            }
            _ => ev.set_child_val("PLAC", place),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct IndividualForm {
    pub given: String,
    pub surname: String,
    pub sex: String,
    pub birth: EventForm,
    pub death: EventForm,
    pub occupation: String,
    pub note: String,
}

impl IndividualForm {
    pub fn read(node: &Node) -> Self {
        let i = Individual::new(node);
        Self {
            given: i.given(),
            surname: i.surname(),
            sex: i.sex().to_string(),
            birth: EventForm::read(node, "BIRT"),
            death: EventForm::read(node, "DEAT"),
            occupation: node.child_val("OCCU").unwrap_or("").to_string(),
            note: node.child("NOTE").map(|n| n.text()).unwrap_or_default(),
        }
    }

    pub fn apply(&self, node: &mut Node) {
        let (given, surname) = (self.given.trim(), self.surname.trim());
        let name = node.ensure_child("NAME");
        name.value = Some(format!("{given} /{surname}/").trim_start().to_string());
        name.set_child_val("GIVN", given);
        name.set_child_val("SURN", surname);
        node.set_child_val("SEX", if self.sex.trim().is_empty() { "U" } else { self.sex.trim() });
        self.birth.apply(node, "BIRT");
        self.death.apply(node, "DEAT");
        node.set_child_val("OCCU", &self.occupation);
        node.set_child_text("NOTE", &self.note);
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FamilyForm {
    pub husband: String,
    pub wife: String,
    pub marriage: EventForm,
}

impl FamilyForm {
    pub fn read(node: &Node) -> Self {
        let f = Family::new(node);
        Self {
            husband: f.husband().unwrap_or("").to_string(),
            wife: f.wife().unwrap_or("").to_string(),
            marriage: EventForm::read(node, "MARR"),
        }
    }

    pub fn apply(&self, node: &mut Node) {
        let ptr = |x: &str| {
            let x = x.trim().trim_matches('@');
            if x.is_empty() { String::new() } else { format!("@{x}@") }
        };
        node.set_child_val("HUSB", &ptr(&self.husband));
        node.set_child_val("WIFE", &ptr(&self.wife));
        self.marriage.apply(node, "MARR");
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SourceForm {
    pub title: String,
    pub author: String,
    pub publication: String,
    pub repository: String,
    pub call_number: String,
    pub text: String,
}

impl SourceForm {
    pub fn read(node: &Node) -> Self {
        let s = Source::new(node);
        Self {
            title: node.child("TITL").map(|n| n.text()).unwrap_or_default(),
            author: s.author().unwrap_or_default(),
            publication: s.publication().unwrap_or_default(),
            repository: s.repository().unwrap_or("").to_string(),
            call_number: s.call_number().unwrap_or("").to_string(),
            text: node.child("TEXT").map(|n| n.text()).unwrap_or_default(),
        }
    }

    pub fn apply(&self, node: &mut Node) {
        node.set_child_text("TITL", &self.title);
        node.set_child_text("AUTH", &self.author);
        node.set_child_text("PUBL", &self.publication);
        node.set_child_text("TEXT", &self.text);
        let repo = self.repository.trim().trim_matches('@');
        if repo.is_empty() {
            node.children.retain(|c| c.tag != "REPO");
        } else {
            let r = node.ensure_child("REPO");
            r.value = Some(format!("@{repo}@"));
            r.set_child_val("CALN", &self.call_number);
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RepositoryForm {
    pub name: String,
    pub www: String,
}

impl RepositoryForm {
    pub fn read(node: &Node) -> Self {
        Self {
            name: node.child_val("NAME").unwrap_or("").to_string(),
            www: node.child_val("WWW").unwrap_or("").to_string(),
        }
    }
    pub fn apply(&self, node: &mut Node) {
        node.set_child_val("NAME", &self.name);
        node.set_child_val("WWW", &self.www);
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MediaForm {
    pub file: String,
    pub form: String,
    pub title: String,
}

impl MediaForm {
    pub fn read(node: &Node) -> Self {
        let file = node.child("FILE");
        Self {
            file: file.map(|f| f.val().to_string()).unwrap_or_default(),
            form: file.and_then(|f| f.child_val("FORM")).unwrap_or("").to_string(),
            title: file.and_then(|f| f.child_val("TITL")).unwrap_or("").to_string(),
        }
    }
    pub fn apply(&self, node: &mut Node) {
        let f = node.ensure_child("FILE");
        f.value = Some(self.file.trim().to_string());
        f.set_child_val("FORM", &self.form);
        f.set_child_val("TITL", &self.title);
    }
}
