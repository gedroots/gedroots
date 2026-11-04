//! Typed views on the most common records, and their editing.
//!
//! Views borrow the underlying [`Node`]; forms (`*Form`) are editable copies applied back to the
//! node with `apply`, touching only the fields they represent.

use crate::date::Date;
use crate::document::Gedcom;
use crate::node::Node;
use rust_i18n::t;
use std::borrow::Cow;

/// Tags of individual and family events and attributes, in display order.
pub const EVENT_TAGS: &[&str] = &[
    "BIRT", "CHR", "BAPM", "DEAT", "BURI", "CREM", "ADOP", "CONF", "FCOM", "NATU", "EMIG", "IMMI", "CENS", "PROB",
    "WILL", "GRAD", "RETI", "RESI", "OCCU", "EDUC", "RELI", "TITL", "NATI", "FACT", "EVEN", "MARR", "MARB", "MARC",
    "MARL", "ENGA", "DIV", "DIVF", "ANUL",
];

/// Name of an event or attribute (“Birth”), `None` if `tag` is not one.
pub fn event_label(tag: &str) -> Option<Cow<'static, str>> {
    Some(match tag {
        "BIRT" => t!("event.BIRT"),
        "CHR" => t!("event.CHR"),
        "BAPM" => t!("event.BAPM"),
        "DEAT" => t!("event.DEAT"),
        "BURI" => t!("event.BURI"),
        "CREM" => t!("event.CREM"),
        "ADOP" => t!("event.ADOP"),
        "CONF" => t!("event.CONF"),
        "FCOM" => t!("event.FCOM"),
        "NATU" => t!("event.NATU"),
        "EMIG" => t!("event.EMIG"),
        "IMMI" => t!("event.IMMI"),
        "CENS" => t!("event.CENS"),
        "PROB" => t!("event.PROB"),
        "WILL" => t!("event.WILL"),
        "GRAD" => t!("event.GRAD"),
        "RETI" => t!("event.RETI"),
        "RESI" => t!("event.RESI"),
        "OCCU" => t!("event.OCCU"),
        "EDUC" => t!("event.EDUC"),
        "RELI" => t!("event.RELI"),
        "TITL" => t!("event.TITL"),
        "NATI" => t!("event.NATI"),
        "FACT" => t!("event.FACT"),
        "EVEN" => t!("event.EVEN"),
        "MARR" => t!("event.MARR"),
        "MARB" => t!("event.MARB"),
        "MARC" => t!("event.MARC"),
        "MARL" => t!("event.MARL"),
        "ENGA" => t!("event.ENGA"),
        "DIV" => t!("event.DIV"),
        "DIVF" => t!("event.DIVF"),
        "ANUL" => t!("event.ANUL"),
        _ => return None,
    })
}

/// Names of an event in every language, to read what people type whatever the interface language
/// (“births”, “naissances”).
pub fn event_labels_everywhere(tag: &str) -> Vec<Cow<'static, str>> {
    crate::i18n::LANGUAGES.iter().filter_map(|lang| crate::i18n::lookup_in(&format!("event.{tag}"), lang)).collect()
}

/// Name of an event or attribute, or the tag itself if it is not one.
pub fn event_name(tag: &str) -> String {
    event_label(tag).map(Cow::into_owned).unwrap_or_else(|| tag.to_string())
}

/// Displayed labels of the custom event types of the open file (`TYPE` → label).
static EVENT_LABELS: std::sync::RwLock<Vec<(String, String)>> = std::sync::RwLock::new(Vec::new());

/// Sets the displayed labels of the open file's event types (see [`crate::Gedcom::event_labels`]).
pub fn set_event_labels(labels: Vec<(String, String)>) {
    if let Ok(mut l) = EVENT_LABELS.write() {
        *l = labels;
    }
}

/// Displayed label of a custom event `TYPE`: the one from the open file's settings, otherwise the
/// `TYPE` itself.
pub fn event_type_label(ty: &str) -> String {
    let ty = ty.trim();
    EVENT_LABELS
        .read()
        .ok()
        .and_then(|l| l.iter().find(|(t, _)| t == ty).map(|(_, l)| l.clone()))
        .unwrap_or_else(|| ty.to_string())
}

/// An event or attribute (BIRT, DEAT, OCCU, RESI, MARR…).
#[derive(Debug, Clone)]
pub struct Event<'a> {
    pub node: &'a Node,
}

impl<'a> Event<'a> {
    pub fn tag(&self) -> &str {
        &self.node.tag
    }

    pub fn label(&self) -> String {
        match (self.node.tag.as_str(), self.node.child_val("TYPE")) {
            ("EVEN", Some(t)) => event_type_label(t),
            ("FACT", Some(t)) => t.to_string(),
            _ => event_name(&self.node.tag),
        }
    }

    /// Value of the attribute (e.g. the occupation for OCCU).
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

    /// Street address within the place (`ADDR`), on one line.
    pub fn address(&self) -> Option<String> {
        crate::address::address_of(self.node)
    }

    /// Media linked to the event (`2 OBJE @M1@`: marriage record, announcement…).
    pub fn media_xrefs(&self) -> Vec<&'a str> {
        self.node.children_with("OBJE").filter_map(|n| n.pointer()).collect()
    }
    pub fn is_sourced(&self) -> bool {
        self.node.child("SOUR").is_some()
    }
}

/// View on an `INDI` record.
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

    /// Surname: `SURN`, otherwise the part between `/…/`.
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

    /// Every `NAME`, the main one first.
    pub fn names(&self) -> Vec<NameForm> {
        self.node.children_with("NAME").map(NameForm::read).collect()
    }

    /// “Given name SURNAME”.
    pub fn display_name(&self) -> String {
        let (g, s) = (self.given(), self.surname());
        match (g.is_empty(), s.is_empty()) {
            (true, true) => t!("record.no_name").into(),
            (true, false) => s,
            (false, true) => g,
            _ => format!("{g} {s}"),
        }
    }

    pub fn sex(&self) -> &str {
        self.node.child_val("SEX").unwrap_or("U")
    }

    pub fn sex_label(&self) -> Cow<'static, str> {
        match self.sex() {
            "M" => t!("sex.male"),
            "F" => t!("sex.female"),
            "X" => t!("sex.other"),
            _ => t!("sex.unknown"),
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

    /// “1850–1912”, with “?” for unknown bounds.
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

    /// Sosa number computed by Ancestris (`_SOSADABOVILLE 12 G4`).
    pub fn sosa(&self) -> Option<String> {
        self.node.child_val("_SOSADABOVILLE").and_then(|v| v.split_whitespace().next()).map(str::to_string)
    }

    /// Number of sourced events / total.
    pub fn sourced_ratio(&self) -> (usize, usize) {
        let ev = self.events();
        (ev.iter().filter(|e| e.is_sourced()).count(), ev.len())
    }

    /// Every cited source (directly or in an event).
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

/// View on a `FAM` record.
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
    pub fn media_xrefs(&self) -> Vec<&'a str> {
        self.node.children_with("OBJE").filter_map(|n| n.pointer()).collect()
    }
    pub fn notes(&self) -> Vec<String> {
        self.node.children_with("NOTE").map(|n| n.text()).collect()
    }
    /// End of the union: divorce or annulment.
    pub fn separation(&self) -> Option<Event<'a>> {
        self.node.children.iter().find(|c| c.tag == "DIV" || c.tag == "ANUL").map(|node| Event { node })
    }
}

/// View on a `SOUR` record.
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
        self.node.child("TITL").map(|n| n.text()).unwrap_or_else(|| t!("record.no_title").into())
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

/// View on a `REPO` record.
#[derive(Debug, Clone, Copy)]
pub struct Repository<'a> {
    pub node: &'a Node,
}

impl<'a> Repository<'a> {
    pub fn new(node: &'a Node) -> Self {
        Self { node }
    }
    pub fn name(&self) -> Cow<'a, str> {
        self.node.child_val("NAME").map(Cow::Borrowed).unwrap_or_else(|| t!("record.no_name"))
    }
    pub fn www(&self) -> Option<&'a str> {
        self.node.child_val("WWW")
    }
}

/// View on an `OBJE` record (photo, scanned record…).
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
            .unwrap_or_else(|| t!("record.no_title").into())
    }
    pub fn is_image(&self) -> bool {
        // An online image has no local file to display.
        !self.is_web()
            && (self.form().is_some_and(|f| f.starts_with("image/"))
                || self.file().is_some_and(|f| {
                    let l = f.to_ascii_lowercase();
                    [".jpg", ".jpeg", ".png", ".gif", ".webp", ".bmp", ".tif", ".tiff"].iter().any(|e| l.ends_with(e))
                }))
    }
}

/// A distinct place of the file, aggregated from every `PLAC` tag.
#[derive(Debug, Clone, PartialEq)]
pub struct Place {
    pub name: String,
    pub latitude: Option<String>,
    pub longitude: Option<String>,
    /// Number of occurrences in the file.
    pub uses: usize,
    /// Records that cite it.
    pub records: Vec<String>,
}

impl Place {
    /// Non-empty jurisdictions, from the most precise to the broadest.
    pub fn parts(&self) -> Vec<&str> {
        self.name.split(',').map(str::trim).filter(|s| !s.is_empty()).collect()
    }

    /// Short label: “Municipality (Department)” in Ancestris' format.
    pub fn short(&self) -> String {
        short_place(&self.name)
    }
}

/// Sort key insensitive to case and accents: “Élisabeth” → “elisabeth”, “Œuvray” → “oeuvray”.
pub fn sort_key(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars().flat_map(char::to_lowercase) {
        match unaccent(c) {
            Some(plain) => out.push_str(plain),
            None => out.push(c),
        }
    }
    out
}

/// The letter without its accent, or its spelling in plain letters (“é” → “e”, “Œ” → “OE”),
/// case kept; `""` for a combining accent; `None` for a letter without accent. The one table of
/// accents: sorting, searching, the palette and file names all use it.
pub fn unaccent(c: char) -> Option<&'static str> {
    Some(match c {
        'à' | 'á' | 'â' | 'ã' | 'ä' | 'å' => "a",
        'À' | 'Á' | 'Â' | 'Ã' | 'Ä' | 'Å' => "A",
        'ç' => "c",
        'Ç' => "C",
        'è' | 'é' | 'ê' | 'ë' => "e",
        'È' | 'É' | 'Ê' | 'Ë' => "E",
        'ì' | 'í' | 'î' | 'ï' => "i",
        'Ì' | 'Í' | 'Î' | 'Ï' => "I",
        'ñ' => "n",
        'Ñ' => "N",
        'ò' | 'ó' | 'ô' | 'õ' | 'ö' | 'ø' => "o",
        'Ò' | 'Ó' | 'Ô' | 'Õ' | 'Ö' | 'Ø' => "O",
        'ù' | 'ú' | 'û' | 'ü' => "u",
        'Ù' | 'Ú' | 'Û' | 'Ü' => "U",
        'ý' | 'ÿ' => "y",
        'Ý' | 'Ÿ' => "Y",
        'œ' => "oe",
        'Œ' => "OE",
        'æ' => "ae",
        'Æ' => "AE",
        'ß' => "ss",
        '\u{300}'..='\u{36f}' => "",
        _ => return None,
    })
}

/// Short label of a place in Ancestris' format (`Lieudit, Commune, Code_Postal, Département,
/// Région, Pays`): “, Troyes, 10000, Aube, Grand Est, France” → “Troyes (Aube)”, “La Gare, Amions,
/// 42260, Loire, …” → “La Gare, Amions (Loire)”.
///
/// If a place format was set with [`set_place_layout`] (the open file's), the label follows that
/// format: see [`short_place_in`].
pub fn short_place(raw: &str) -> String {
    let layout = PLACE_LAYOUT.read().map(|l| l.clone()).unwrap_or_default();
    short_place_in(raw, &layout)
}

/// Place format of the open file, used by [`short_place`].
static PLACE_LAYOUT: std::sync::RwLock<Vec<String>> = std::sync::RwLock::new(Vec::new());

/// Sets the place format (`HEAD.PLAC.FORM`) to follow for short labels.
pub fn set_place_layout(form: &[String]) {
    if let Ok(mut l) = PLACE_LAYOUT.write() {
        *l = form.to_vec();
    }
}

/// Short label of a place according to a format (`["Commune", "Département", "Pays"]`):
/// municipality (preceded by the locality if any) and, in parentheses, the first broader
/// jurisdiction recognized (department, county, province…, otherwise region, otherwise country).
/// Without a format, or if the place does not follow it, Ancestris' format.
pub fn short_place_in(raw: &str, form: &[String]) -> String {
    let parts: Vec<&str> = raw.split(',').map(str::trim).collect();
    if form.len() >= 2 && parts.len() == form.len() && !is_ancestris_layout(form) {
        return short_by_layout(&parts, form);
    }
    short_ancestris(raw)
}

/// Ancestris' default format, which [`short_ancestris`] already knows.
fn is_ancestris_layout(form: &[String]) -> bool {
    let k: Vec<String> = form.iter().map(|f| sort_key(f)).collect();
    k.len() == 6 && k[1].contains("commune") && k[3].contains("departement")
}

fn short_by_layout(parts: &[&str], form: &[String]) -> String {
    let keys: Vec<String> = form.iter().map(|f| sort_key(f)).collect();
    let find = |words: &[&str]| keys.iter().position(|k| words.iter().any(|w| k.contains(w)));
    let at = |i: usize| parts.get(i).copied().filter(|s| !s.is_empty());
    let nonempty: Vec<usize> = (0..parts.len()).filter(|&i| at(i).is_some()).collect();
    let Some(&first) = nonempty.first() else { return String::new() };
    let commune = find(&["commune", "ville", "city", "town", "village", "paroisse", "parish", "municip", "localite"]);
    // Name: the municipality (or the most precise filled jurisdiction), with a locality before it.
    let main = commune.filter(|&c| at(c).is_some()).unwrap_or(first);
    let name = match (first < main).then(|| at(first)).flatten() {
        Some(l) => format!("{l}, {}", at(main).unwrap_or("")),
        None => at(main).unwrap_or("").to_string(),
    };
    let wider = |words: &[&str]| find(words).filter(|&i| i > main).and_then(at);
    let context = wider(&["departement", "county", "comte", "province", "state", "etat", "district"])
        .or_else(|| wider(&["region"]))
        .or_else(|| nonempty.last().filter(|&&i| i > main).and_then(|&i| at(i)));
    match context {
        Some(c) if c != name => format!("{name} ({c})"),
        _ => name,
    }
}

fn short_ancestris(raw: &str) -> String {
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

/// Place as displayed in an input field: empty jurisdictions left out (“, Troyes, 10000, Aube, ,
/// France” → “Troyes, 10000, Aube, France”).
pub fn place_label(name: &str) -> String {
    if !name.contains(',') {
        return name.trim().to_string();
    }
    name.split(',').map(str::trim).filter(|s| !s.is_empty()).collect::<Vec<_>>().join(", ")
}

impl Gedcom {
    /// `PLAC` value of a typed place: the place of the file (or of `extra`, created places not
    /// written yet) displayed this way, empty jurisdictions included; otherwise the input.
    pub fn place_value(&self, typed: &str, extra: &[String]) -> String {
        let typed = typed.trim();
        let places = self.places();
        if extra.iter().any(|n| n == typed) || places.iter().any(|p| p.name == typed) {
            return typed.to_string();
        }
        extra
            .iter()
            .find(|n| place_label(n) == typed)
            .cloned()
            .or_else(|| places.into_iter().map(|p| p.name).find(|n| place_label(n) == typed))
            .unwrap_or_else(|| typed.to_string())
    }

    /// Index of the places, sorted by name.
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

    /// Renames a place everywhere and updates its coordinates. Returns the number of occurrences
    /// changed.
    pub fn update_place(&mut self, old: &str, new: &str, lat: &str, long: &str) -> usize {
        let mut count = 0;
        for r in self.records_where_mut(|r| r.any(&|n| n.tag == "PLAC" && n.val() == old)) {
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

    /// Father and mother of the individual (first family of origin).
    pub fn parents(&self, xref: &str) -> (Option<Individual<'_>>, Option<Individual<'_>>) {
        let Some(fam) = self.individual(xref).and_then(|i| i.famc().first().and_then(|f| self.family(f))) else {
            return (None, None);
        };
        (fam.husband().and_then(|x| self.individual(x)), fam.wife().and_then(|x| self.individual(x)))
    }

    /// Spouses and children, by family.
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

    /// Ancestry in Sosa numbering (index 1 = the person, 2k = father, 2k+1 = mother).
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

    /// Portrait of an individual: their first photo (`MEDI PHOTO`), in the order of their links
    /// (“Set as Portrait” moves the link to the top). Failing that, an image without a declared
    /// medium filed in a “Portraits” folder or titled “Portrait”, then an image without a medium
    /// that is not a scan of a source; otherwise their first image.
    pub fn portrait(&self, xref: &str) -> Option<Media<'_>> {
        let ind = self.individual(xref)?;
        let images: Vec<Media> = ind
            .media_xrefs()
            .into_iter()
            .filter_map(|m| self.get(m).filter(|n| n.tag == "OBJE").map(Media::new))
            .filter(|m| m.is_image())
            .collect();
        if let Some(p) = images.iter().find(|m| crate::kinds::medium(m.node) == Some("PHOTO")) {
            return Some(*p);
        }
        let is_portrait = |m: &Media| {
            m.file().is_some_and(|f| f.to_lowercase().contains("portrait"))
                || m.title().to_lowercase().contains("portrait")
        };
        let cited = |m: &Media| {
            let ptr = format!("@{}@", m.xref());
            self.records_of(crate::document::RecordKind::Source)
                .any(|s| s.children.iter().any(|c| c.tag == "OBJE" && c.val() == ptr))
        };
        let untyped = || images.iter().filter(|m| crate::kinds::medium(m.node).is_none());
        untyped().find(|m| is_portrait(m)).or_else(|| untyped().find(|m| !cited(m))).or(images.first()).copied()
    }

    /// Image media linked to a record (source, individual…).
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

    /// Short label of any record, for lists and links.
    pub fn record_label(&self, xref: &str) -> String {
        let Some(n) = self.get(xref) else { return t!("record.missing").into() };
        match n.tag.as_str() {
            "INDI" => Individual::new(n).display_name(),
            "FAM" => {
                let f = Family::new(n);
                let name = |x: Option<&str>| x.and_then(|x| self.individual(x)).map(|i| i.display_name());
                match (name(f.husband()), name(f.wife())) {
                    (Some(h), Some(w)) => format!("{h} & {w}"),
                    (Some(a), None) | (None, Some(a)) => a,
                    _ => t!("record.empty_family").into(),
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
// Forms of the visual editors.
// ---------------------------------------------------------------------------

/// Date + place of an event, edited together.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EventForm {
    /// GEDCOM value of the date and its phrase (`DATE.PHRASE`).
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
            // The event is only deleted if it holds nothing else (sources, notes…).
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
        // The place keeps its coordinates if its label does not change.
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

/// Types of `NAME.TYPE` (GEDCOM 7 enumeration), in the order offered in the interface.
pub const NAME_TYPES: [&str; 7] = ["BIRTH", "MARRIED", "AKA", "MAIDEN", "IMMIGRANT", "PROFESSIONAL", "OTHER"];

/// One `NAME` of an individual: its parts and its type. What it does not hold (sources, notes,
/// translations…) stays as it is on the node.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NameForm {
    pub given: String,
    pub surname: String,
    /// `NAME.TYPE`, empty when absent.
    pub kind: String,
    /// `NICK`: “called …”.
    pub nickname: String,
    /// `NPFX`: title before the name (“Abbé”, “Dr”).
    pub prefix: String,
    /// `SPFX`: particle of the surname (“de”, “van”).
    pub particle: String,
    /// `NSFX`: after the name (“fils”, “Jr”).
    pub suffix: String,
}

impl NameForm {
    pub fn read(name: &Node) -> Self {
        let v = name.val();
        let slashes = (v.find('/'), v.rfind('/'));
        let surname = name.child_val("SURN").map(str::to_string).unwrap_or_else(|| match slashes {
            (Some(a), Some(b)) if b > a => v[a + 1..b].trim().to_string(),
            _ => String::new(),
        });
        let given = name
            .child_val("GIVN")
            .map(str::to_string)
            .unwrap_or_else(|| v.split('/').next().unwrap_or("").trim().to_string());
        let val = |tag: &str| name.child_val(tag).unwrap_or("").to_string();
        Self {
            given,
            surname,
            kind: val("TYPE"),
            nickname: val("NICK"),
            prefix: val("NPFX"),
            particle: val("SPFX"),
            suffix: val("NSFX"),
        }
    }

    /// The `NAME` payload: “[title] given /[particle] surname/ [suffix]”.
    pub fn value(&self) -> String {
        let join =
            |parts: &[&str]| parts.iter().map(|p| p.trim()).filter(|p| !p.is_empty()).collect::<Vec<_>>().join(" ");
        let given = join(&[&self.prefix, &self.given]);
        let surname = join(&[&self.particle, &self.surname]);
        join(&[&given, &format!("/{surname}/"), &self.suffix])
    }

    pub fn apply(&self, name: &mut Node) {
        name.value = Some(self.value());
        let kind = self.kind.trim();
        if name.child_val("TYPE") != Some(kind) {
            // A new type: its phrase described the old one.
            name.children.retain(|c| c.tag != "TYPE");
            if !kind.is_empty() {
                name.children.insert(0, Node::with_value("TYPE", kind));
            }
        }
        // Given name and surname the payload already says are not repeated in GIVN / SURN.
        let implied = NameForm::read(&Node::with_value("NAME", self.value()));
        for (tag, v) in [
            ("NPFX", &self.prefix),
            ("GIVN", &self.given),
            ("NICK", &self.nickname),
            ("SPFX", &self.particle),
            ("SURN", &self.surname),
            ("NSFX", &self.suffix),
        ] {
            let same = match tag {
                "GIVN" => *v == implied.given,
                "SURN" => *v == implied.surname,
                _ => false,
            };
            if !(same && name.child(tag).is_none()) {
                name.set_child_val(tag, v);
            }
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct IndividualForm {
    /// Every `NAME`, the first (main) one first; never empty.
    pub names: Vec<NameForm>,
    pub sex: String,
    pub occupation: String,
    pub note: String,
}

impl IndividualForm {
    pub fn read(node: &Node) -> Self {
        let i = Individual::new(node);
        let mut names: Vec<NameForm> = node.children_with("NAME").map(NameForm::read).collect();
        if names.is_empty() {
            names.push(NameForm::default());
        }
        Self {
            names,
            sex: i.sex().to_string(),
            occupation: node.child_val("OCCU").unwrap_or("").to_string(),
            note: node.child("NOTE").map(|n| n.text()).unwrap_or_default(),
        }
    }

    /// Writes the names in order onto the `NAME` substructures (keeping what the form does not
    /// hold), removes the extra ones and adds the new ones after the last.
    pub fn apply(&self, node: &mut Node) {
        node.ensure_child("NAME");
        let at: Vec<usize> =
            node.children.iter().enumerate().filter(|(_, c)| c.tag == "NAME").map(|(i, _)| i).collect();
        for (form, &i) in self.names.iter().zip(&at) {
            form.apply(&mut node.children[i]);
        }
        for &i in at.iter().skip(self.names.len().max(1)).rev() {
            node.children.remove(i);
        }
        let after = node.children.iter().rposition(|c| c.tag == "NAME").map_or(0, |i| i + 1);
        let added: Vec<Node> = self.names[at.len().min(self.names.len())..]
            .iter()
            .map(|form| {
                let mut name = Node::new("NAME");
                form.apply(&mut name);
                name
            })
            .collect();
        node.children.splice(after..after, added);
        node.set_child_val("SEX", if self.sex.trim().is_empty() { "U" } else { self.sex.trim() });
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
