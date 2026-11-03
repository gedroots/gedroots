//! Rust core of the read-only mobile app, exposed to Swift (and later Kotlin) with UniFFI. The app
//! pairs with the desktop (File › Mobile Devices… › Add a Device), syncs the tree into its own
//! folder, then reads it: view models ready to display (people, records, events), computed here so
//! that the Swift (and later Kotlin) side only draws them.

use gedcom::analysis::{kinship, timeline};
use gedcom::{Gedcom, Media, RecordKind, Source};
use gedroots_sync::{DeviceKind, PairCode, Receiver, Refused};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

uniffi::setup_scaffolding!();

#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum MobileError {
    /// Not a GEDRoots pairing code.
    #[error("not a GEDRoots pairing code")]
    NotACode,
    /// The code was already used, has expired, or the computer could not be reached.
    #[error("pairing failed: {0}")]
    PairingFailed(String),
    /// The computer no longer knows this device (removed, or another computer).
    #[error("this device is no longer paired")]
    Removed,
    /// Sharing is off, or the file is not open on the computer.
    #[error("the tree is not shared right now")]
    NotShared,
    #[error("{0}")]
    Failed(String),
}

impl From<anyhow::Error> for MobileError {
    fn from(e: anyhow::Error) -> Self {
        match e.downcast_ref::<Refused>() {
            Some(Refused::UnknownDevice) => Self::Removed,
            Some(Refused::NotShared) => Self::NotShared,
            None => Self::Failed(format!("{e:#}")),
        }
    }
}

/// Language of the labels and dates (`"en"`, `"fr"`).
#[uniffi::export]
pub fn set_language(language: String) {
    rust_i18n::set_locale(&language);
}

/// Is `code` a GEDRoots pairing code?
#[uniffi::export]
pub fn is_pairing_code(code: String) -> bool {
    PairCode::decode(&code).is_ok()
}

/// Pairs this device (its key lives in `data_dir`) with the computer that shows `code`. Returns the
/// computer's address, to keep for [`sync`].
#[uniffi::export(async_runtime = "tokio")]
pub async fn pair(data_dir: String, code: String, name: String, tablet: bool) -> Result<String, MobileError> {
    let code = PairCode::decode(&code).map_err(|_| MobileError::NotACode)?;
    let receiver = Receiver::open(Path::new(&data_dir)).await?;
    let kind = if tablet { DeviceKind::Tablet } else { DeviceKind::Phone };
    let result = receiver.pair(&code, &name, kind).await;
    receiver.close().await;
    result.map_err(|e| MobileError::PairingFailed(format!("{e:#}")))
}

#[derive(uniffi::Record)]
pub struct SyncReport {
    pub total_bytes: u64,
    pub downloaded_bytes: u64,
    pub files: u64,
    pub elapsed_ms: u64,
    pub direct: bool,
    /// Folder of the received tree (`gedcom.ged` and the media).
    pub tree_dir: String,
}

/// Downloads what changed on the paired computer (`desktop`, as returned by [`pair`]) into
/// `data_dir` (the app's own folder).
#[uniffi::export(async_runtime = "tokio")]
pub async fn sync(data_dir: String, desktop: String) -> Result<SyncReport, MobileError> {
    let receiver = Receiver::open(Path::new(&data_dir)).await?;
    let result = receiver.sync(&desktop).await;
    receiver.close().await;
    let r = result?;
    Ok(SyncReport {
        total_bytes: r.total_bytes,
        downloaded_bytes: r.downloaded_bytes,
        files: r.files,
        elapsed_ms: r.elapsed.as_millis() as u64,
        direct: r.direct,
        tree_dir: r.tree.display().to_string(),
    })
}

/// Side of the root person's family an ancestor belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum Lineage {
    /// The root person (Sosa 1).
    Root,
    /// Father's side (Sosa 2 and their ancestors).
    Paternal,
    /// Mother's side (Sosa 3 and their ancestors).
    Maternal,
    /// Not a direct ancestor.
    Other,
}

/// Lineage of a Sosa number: its second most significant bit says through which parent of the
/// root it goes up.
fn lineage_of(sosa: Option<u64>) -> Lineage {
    match sosa {
        None | Some(0) => Lineage::Other,
        Some(1) => Lineage::Root,
        Some(k) => {
            let top = 63 - k.leading_zeros();
            if k >> (top - 1) & 1 == 0 { Lineage::Paternal } else { Lineage::Maternal }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct PersonRow {
    /// Internal identifier, never shown.
    pub id: String,
    pub name: String,
    pub surname: String,
    pub years: String,
    /// Short place of birth (or of the first event with a place).
    pub place: String,
    /// Sosa number, for the root person and their ancestors only.
    pub sosa: Option<u64>,
    pub lineage: Lineage,
}

/// Which people a list shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum Scope {
    All,
    /// The root person and their direct ancestors.
    Ancestors,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum TimelineKind {
    /// Event of the person.
    Own,
    /// Event of one of their unions.
    Family,
    /// Event of a relative during their life (birth of a child, death of a parent…).
    Relative,
}

/// An event that can be opened: the record holding it and its position there.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct EventRef {
    pub owner: String,
    pub index: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct TimelineRow {
    pub kind: TimelineKind,
    pub label: String,
    pub date: String,
    /// Year for the column at the left of the timeline, empty when undated.
    pub year: String,
    pub place: String,
    /// Age of the person then.
    pub age: Option<String>,
    pub sourced: bool,
    /// Person the event is about (spouse, child…).
    pub related: Option<String>,
    pub event: Option<EventRef>,
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct UnionRow {
    pub spouse: Option<PersonRow>,
    /// Date of the marriage, if any.
    pub married: String,
    pub children: Vec<PersonRow>,
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct SourceRow {
    pub id: String,
    pub title: String,
    /// Repository and call number (“Archives de la Manche · 5 Mi 1487”).
    pub detail: String,
}

/// Area of an image a link is about (`CROP`), in pixels of the image as displayed (EXIF
/// orientation applied). Without a width or height, the area extends to the edge.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Record)]
pub struct ImageCrop {
    pub left: u32,
    pub top: u32,
    pub width: Option<u32>,
    pub height: Option<u32>,
}

impl From<gedcom::Crop> for ImageCrop {
    fn from(c: gedcom::Crop) -> Self {
        Self { left: c.left, top: c.top, width: c.width, height: c.height }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct MediaRow {
    pub id: String,
    pub title: String,
    /// Full path of the file on the device, when it was received.
    pub path: Option<String>,
    pub is_image: bool,
    /// Area the record shows (a face in a group photo), for its thumbnail.
    pub crop: Option<ImageCrop>,
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct PersonDetail {
    pub row: PersonRow,
    pub sex: String,
    /// Latest occupation.
    pub occupation: Option<String>,
    /// Relationship to the root person (“great-grandfather”), when there is one.
    pub kinship: Option<String>,
    /// Name of the root person, for “… of Marcel Lemaire”.
    pub root_name: Option<String>,
    pub timeline: Vec<TimelineRow>,
    pub parents: Vec<PersonRow>,
    pub unions: Vec<UnionRow>,
    pub sources: Vec<SourceRow>,
    pub media: Vec<MediaRow>,
    pub notes: Vec<String>,
    /// Full path of the portrait image, if any.
    pub portrait: Option<String>,
    /// Area of the portrait image that shows the person.
    pub portrait_crop: Option<ImageCrop>,
}

/// Person present at an event, with their role (“Witness”, “Father of the bride”).
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct Participant {
    pub person: PersonRow,
    pub role: String,
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct Citation {
    pub source: SourceRow,
    /// Where in the source (“View 12 of 48”).
    pub page: String,
}

#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct PlaceRef {
    /// Full name, as in the file.
    pub name: String,
    pub short: String,
    pub latitude: Option<f64>,
    pub longitude: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct EventDetail {
    pub title: String,
    pub date: String,
    pub place: Option<PlaceRef>,
    /// The individual, or the spouses of a union.
    pub principals: Vec<PersonRow>,
    pub participants: Vec<Participant>,
    pub citations: Vec<Citation>,
    pub media: Vec<MediaRow>,
    pub note: Option<String>,
}

/// The Tree tab around one person: two generations of ancestors above, their unions and children
/// below.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct FamilyView {
    pub person: PersonRow,
    pub portrait: Option<String>,
    pub portrait_crop: Option<ImageCrop>,
    /// Father, mother (each may be unknown).
    pub parents: Vec<Option<PersonRow>>,
    /// Father's father, father's mother, mother's father, mother's mother.
    pub grandparents: Vec<Option<PersonRow>>,
    /// Is there anyone above the grandparents?
    pub more_ancestors: bool,
    pub unions: Vec<UnionRow>,
}

/// A place of the file that has coordinates, for the map.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct PlacePin {
    /// Full name, as in the file: the key for [`Tree::place_events`].
    pub name: String,
    pub short: String,
    pub latitude: f64,
    pub longitude: f64,
    /// Events that took place there.
    pub events: u32,
    /// Did the root person or one of their ancestors live an event there?
    pub ancestors: bool,
}

/// An event at a place, for the place's sheet.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct PlaceEvent {
    pub title: String,
    pub date: String,
    pub year: String,
    /// The individual, or the spouses of a union.
    pub people: Vec<PersonRow>,
    pub event: EventRef,
}

/// Events that cite a source at the same place in it (the same `PAGE`): one act.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct SourceAct {
    /// Where in the source (“View 112”), empty when the citations don't say.
    pub page: String,
    pub events: Vec<PlaceEvent>,
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct SourceDetail {
    pub row: SourceRow,
    pub author: Option<String>,
    pub publication: Option<String>,
    /// Web page of the source (the archives' viewer).
    pub url: Option<String>,
    pub media: Vec<MediaRow>,
    pub acts: Vec<SourceAct>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum MediaKind {
    All,
    Photos,
    /// Scans of records: media attached to a source or cited.
    Documents,
}

/// A record a media is linked to, for the viewer.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct MediaLink {
    pub title: String,
    /// Set when the record is an individual, to open their record.
    pub person: Option<String>,
    /// Set when the record is a source.
    pub source: Option<String>,
    /// Where the person is on the image, when they were identified on it.
    pub crop: Option<ImageCrop>,
}

/// A place's jurisdictions without the postal and INSEE codes (“Troyes, Aube, Grand Est,
/// France”), as the line under its short name.
fn place_without_codes(name: &str) -> String {
    let is_code = |s: &&str| s.chars().all(|c| c.is_ascii_digit() || c == ' ');
    name.split(',').map(str::trim).filter(|s| !s.is_empty() && !is_code(s)).collect::<Vec<_>>().join(", ")
}

/// Sort key of a date ([`gedcom::Date::sort_key`]).
type SortKey = (i32, u8, u8);

/// A received tree, read-only.
#[derive(uniffi::Object)]
pub struct Tree {
    ged: Gedcom,
    dir: PathBuf,
    sosa: HashMap<String, u64>,
    /// Media attached to a source record: scans of documents.
    source_media: std::collections::HashSet<String>,
    /// Coordinates of the places that have some, by full name.
    coords: HashMap<String, (f64, f64)>,
}

impl Tree {
    fn from_gedcom(ged: Gedcom, dir: PathBuf) -> Self {
        let sosa =
            ged.sosa_numbers().into_iter().filter(|(_, n)| n.branch.is_empty()).map(|(x, n)| (x, n.sosa)).collect();
        let source_media = ged
            .records_of(RecordKind::Source)
            .flat_map(|r| Source::new(r).media_xrefs().into_iter().map(str::to_string))
            .collect();
        let coords = ged
            .places()
            .into_iter()
            .filter_map(|p| {
                let lat = p.latitude.as_deref().and_then(gedcom::geo::parse_latitude)?;
                let lon = p.longitude.as_deref().and_then(gedcom::geo::parse_longitude)?;
                Some((p.name, (lat, lon)))
            })
            .collect();
        Self { ged, dir, sosa, source_media, coords }
    }

    fn row(&self, i: &gedcom::Individual) -> PersonRow {
        let sosa = self.sosa.get(i.xref()).copied();
        let events = i.events();
        let place = i
            .birth()
            .into_iter()
            .chain(events)
            .find_map(|e| e.place().filter(|p| !p.trim().is_empty()).map(gedcom::short_place))
            .unwrap_or_default();
        PersonRow {
            id: i.xref().to_string(),
            name: i.display_name(),
            surname: i.surname(),
            years: i.years(),
            place,
            sosa,
            lineage: lineage_of(sosa),
        }
    }

    fn row_of(&self, xref: &str) -> Option<PersonRow> {
        self.ged.individual(xref).map(|i| self.row(&i))
    }

    fn sorted_rows(&self, xrefs: impl Iterator<Item = String>) -> Vec<PersonRow> {
        let mut rows: Vec<(String, PersonRow)> = xrefs
            .filter_map(|x| self.ged.individual(&x))
            .map(|i| (gedcom::sort_key(&format!("{} {}", i.surname(), i.given())), self.row(&i)))
            .collect();
        rows.sort_by(|a, b| a.0.cmp(&b.0));
        rows.into_iter().map(|(_, r)| r).collect()
    }

    fn source_row(&self, xref: &str) -> Option<SourceRow> {
        let s = self.ged.get(xref).filter(|n| n.tag == "SOUR").map(Source::new)?;
        let repo = s.repository().map(|r| self.ged.record_label(r));
        let detail = [repo.as_deref(), s.call_number()].into_iter().flatten().collect::<Vec<_>>().join(" · ");
        Some(SourceRow { id: xref.to_string(), title: s.title(), detail })
    }

    /// Media rows, each with the crop of `owner`'s link to it.
    fn media_rows<S: AsRef<str>>(&self, xrefs: &[S], owner: Option<&str>) -> Vec<MediaRow> {
        let mut out: Vec<MediaRow> = Vec::new();
        for x in xrefs {
            let Some(m) = self.ged.get(x.as_ref()).filter(|n| n.tag == "OBJE").map(Media::new) else { continue };
            if out.iter().any(|r| r.id == m.xref()) {
                continue;
            }
            out.push(MediaRow {
                id: m.xref().to_string(),
                title: m.title(),
                path: self.file_path(m.file()),
                is_image: m.is_image(),
                crop: owner.and_then(|o| self.ged.link_crop(o, m.xref())).map(ImageCrop::from),
            });
        }
        out
    }

    /// Path on the device of a media file, if it was received.
    fn file_path(&self, file: Option<&str>) -> Option<String> {
        file.map(|f| self.dir.join(gedcom::gedzip::unescape(f.trim())))
            .filter(|p| p.is_file())
            .map(|p| p.display().to_string())
    }

    /// Every event of individuals and families, with its record and position.
    fn all_events(&self) -> impl Iterator<Item = (&str, usize, gedcom::Event<'_>)> {
        self.ged
            .records_of(RecordKind::Individual)
            .chain(self.ged.records_of(RecordKind::Family))
            .filter_map(|r| r.xref.as_deref().map(|x| (x, r)))
            .flat_map(|(x, r)| {
                r.children
                    .iter()
                    .enumerate()
                    .filter(|(_, n)| gedcom::event_label(&n.tag).is_some())
                    .map(move |(i, n)| (x, i, gedcom::Event { node: n }))
            })
    }

    fn place_event(&self, owner: &str, index: usize, e: &gedcom::Event) -> Option<PlaceEvent> {
        let date = e.date();
        Some(PlaceEvent {
            title: self.ged.event_title(owner, index)?,
            year: date.as_ref().and_then(|d| d.year()).map(|y| y.to_string()).unwrap_or_default(),
            date: date.map(|d| d.display()).unwrap_or_default(),
            people: self.ged.event_principals(owner).iter().filter_map(|x| self.row_of(x)).collect(),
            event: EventRef { owner: owner.to_string(), index: index as u32 },
        })
    }

    /// Is this media a scan of a record rather than a photo?
    fn is_document(&self, m: &Media) -> bool {
        match gedcom::medium(m.node) {
            Some("PHOTO") => false,
            Some(_) => true,
            None => self.source_media.contains(m.xref()),
        }
    }

    fn unions(&self, id: &str) -> Vec<UnionRow> {
        let Some(ind) = self.ged.individual(id) else { return Vec::new() };
        ind.fams()
            .into_iter()
            .filter_map(|f| self.ged.family(f))
            .map(|f| {
                let spouse = [f.husband(), f.wife()].into_iter().flatten().find(|x| *x != id);
                UnionRow {
                    spouse: spouse.and_then(|s| self.row_of(s)),
                    married: f.marriage().and_then(|m| m.date()).map(|d| d.display()).unwrap_or_default(),
                    children: f.children().into_iter().filter_map(|c| self.row_of(c)).collect(),
                }
            })
            .collect()
    }

    fn timeline_rows(&self, id: &str) -> Vec<TimelineRow> {
        timeline::timeline(&self.ged, id)
            .into_iter()
            .map(|e| TimelineRow {
                kind: match e.kind {
                    timeline::EntryKind::Own => TimelineKind::Own,
                    timeline::EntryKind::Family => TimelineKind::Family,
                    timeline::EntryKind::Relative => TimelineKind::Relative,
                },
                year: e.date.as_ref().and_then(|d| d.year()).map(|y| y.to_string()).unwrap_or_default(),
                date: e.date.as_ref().map(|d| d.display()).unwrap_or_default(),
                place: e.place.as_deref().map(gedcom::short_place).unwrap_or_default(),
                label: e.label,
                age: e.age,
                sourced: e.sourced,
                related: e.related,
                event: e.event.map(|(owner, index)| EventRef { owner, index: index as u32 }),
            })
            .collect()
    }
}

#[uniffi::export]
impl Tree {
    /// Opens the tree that `sync` put in `tree_dir`.
    #[uniffi::constructor]
    pub fn open(tree_dir: String) -> Result<Arc<Self>, MobileError> {
        let dir = PathBuf::from(tree_dir);
        let ged = gedcom::load(dir.join(gedcom::gedzip::DATASET)).map_err(MobileError::Failed)?;
        Ok(Arc::new(Self::from_gedcom(ged, dir)))
    }

    /// Every individual, sorted by surname then given name.
    pub fn individuals(&self) -> Vec<PersonRow> {
        self.search(String::new(), Scope::All)
    }

    /// People matching `query` (names, places and the advanced keywords: `born:1850-1900`,
    /// `place:troyes`…), sorted by surname then given name. An empty query lists everyone.
    pub fn search(&self, query: String, scope: Scope) -> Vec<PersonRow> {
        let xrefs: Vec<String> = if query.trim().is_empty() {
            self.ged.records_of(RecordKind::Individual).filter_map(|r| r.xref.clone()).collect()
        } else {
            self.ged.search_individuals(&query)
        };
        let rows = self.sorted_rows(xrefs.into_iter());
        match scope {
            Scope::All => rows,
            Scope::Ancestors => rows.into_iter().filter(|r| r.sosa.is_some()).collect(),
        }
    }

    pub fn root(&self) -> Option<String> {
        self.ged.root()
    }

    pub fn person(&self, id: String) -> Option<PersonDetail> {
        let ind = self.ged.individual(&id)?;
        let (father, mother) = self.ged.parents(&id);
        let parents = [father, mother].into_iter().flatten().map(|p| self.row(&p)).collect();
        let unions = self.unions(&id);
        let root = self.ged.root();
        let kinship = root
            .as_deref()
            .filter(|r| *r != id)
            .and_then(|r| kinship::relationship(&self.ged, r, &id))
            .map(|k| k.label);
        let root_name = root.as_deref().filter(|_| kinship.is_some()).map(|r| self.ged.record_label(r));
        let occupation =
            ind.events().iter().rev().find(|e| e.tag() == "OCCU").and_then(|e| e.value()).map(str::to_string);
        // Media of the person and of their events, the portrait first.
        let mut media_xrefs: Vec<String> = ind.media_xrefs().into_iter().map(str::to_string).collect();
        for e in ind.events() {
            media_xrefs.extend(e.media_xrefs().into_iter().map(str::to_string));
        }
        let portrait_media = self.ged.portrait(&id);
        if let Some(p) = portrait_media {
            let x = p.xref().to_string();
            media_xrefs.retain(|m| *m != x);
            media_xrefs.insert(0, x);
        }
        let portrait = portrait_media.and_then(|m| self.file_path(m.file()));
        let portrait_crop = portrait_media.and_then(|m| self.ged.link_crop(&id, m.xref())).map(ImageCrop::from);
        Some(PersonDetail {
            row: self.row(&ind),
            sex: ind.sex_label().into_owned(),
            occupation,
            kinship,
            root_name,
            timeline: self.timeline_rows(&id),
            parents,
            unions,
            sources: ind.cited_sources().iter().filter_map(|s| self.source_row(s)).collect(),
            media: self.media_rows(&media_xrefs, Some(&id)),
            notes: ind.notes(),
            portrait,
            portrait_crop,
        })
    }

    /// The family around `id` for the Tree tab.
    pub fn family(&self, id: String) -> Option<FamilyView> {
        let ind = self.ged.individual(&id)?;
        let up = self.ged.ancestry(&id, 4);
        let at = |k: usize| up.get(k).cloned().flatten().and_then(|x| self.row_of(&x));
        Some(FamilyView {
            person: self.row(&ind),
            portrait: self.ged.portrait(&id).and_then(|m| self.file_path(m.file())),
            portrait_crop: self.ged.portrait(&id).and_then(|m| self.ged.link_crop(&id, m.xref())).map(ImageCrop::from),
            parents: (2..4).map(at).collect(),
            grandparents: (4..8).map(at).collect(),
            more_ancestors: up[8..16].iter().any(Option::is_some),
            unions: self.unions(&id),
        })
    }

    /// Places that have coordinates, for the map.
    pub fn places(&self) -> Vec<PlacePin> {
        let mut counts: HashMap<String, (u32, bool)> = HashMap::new();
        for (owner, _, e) in self.all_events() {
            if let Some(p) = e.place() {
                let c = counts.entry(p.to_string()).or_default();
                c.0 += 1;
                c.1 |= self.ged.event_principals(owner).iter().any(|x| self.sosa.contains_key(x));
            }
        }
        self.ged
            .places()
            .into_iter()
            .filter_map(|p| {
                let (latitude, longitude) = *self.coords.get(&p.name)?;
                let (events, ancestors) = counts.get(&p.name).copied().unwrap_or_default();
                Some(PlacePin { short: p.short(), name: p.name, latitude, longitude, events, ancestors })
            })
            .collect()
    }

    /// Events at the place `name` (its full name), sorted by date.
    pub fn place_events(&self, name: String) -> Vec<PlaceEvent> {
        let mut out: Vec<((i32, u8, u8), PlaceEvent)> = self
            .all_events()
            .filter(|(_, _, e)| e.place() == Some(name.as_str()))
            .filter_map(|(owner, index, e)| {
                let key = e.date().map(|d| d.sort_key()).unwrap_or((i32::MAX, 0, 0));
                Some((key, self.place_event(owner, index, &e)?))
            })
            .collect();
        out.sort_by_key(|(k, _)| *k);
        out.into_iter().map(|(_, e)| e).collect()
    }

    /// Every source, sorted by title.
    pub fn sources(&self) -> Vec<SourceRow> {
        let mut rows: Vec<SourceRow> = self
            .ged
            .records_of(RecordKind::Source)
            .filter_map(|r| r.xref.as_deref())
            .filter_map(|x| self.source_row(x))
            .collect();
        rows.sort_by_cached_key(|r| gedcom::sort_key(&r.title));
        rows
    }

    /// A source with the acts read in it: the events citing it, grouped by page.
    pub fn source(&self, id: String) -> Option<SourceDetail> {
        let row = self.source_row(&id)?;
        let s = Source::new(self.ged.get(&id)?);
        // Page, earliest date (to sort the acts), events.
        let mut acts: Vec<(String, SortKey, Vec<PlaceEvent>)> = Vec::new();
        for (owner, index, e) in self.all_events() {
            for c in e.node.children_with("SOUR").filter(|c| c.pointer() == Some(id.as_str())) {
                let page = c.child_val("PAGE").unwrap_or_default().trim().to_string();
                let Some(ev) = self.place_event(owner, index, &e) else { continue };
                let key = e.date().map(|d| d.sort_key()).unwrap_or((i32::MAX, 0, 0));
                match acts.iter_mut().find(|a| !page.is_empty() && a.0 == page) {
                    Some(a) => {
                        a.1 = a.1.min(key);
                        a.2.push(ev);
                    }
                    None => acts.push((page, key, vec![ev])),
                }
            }
        }
        acts.sort_by_key(|a| a.1);
        Some(SourceDetail {
            row,
            author: s.author(),
            publication: s.publication(),
            url: s.url().map(str::to_string),
            media: self.media_rows(&s.media_xrefs(), Some(&id)),
            acts: acts.into_iter().map(|(page, _, events)| SourceAct { page, events }).collect(),
        })
    }

    /// Every media, photos or documents, in file order.
    pub fn media(&self, kind: MediaKind) -> Vec<MediaRow> {
        let xrefs: Vec<&str> = self
            .ged
            .records_of(RecordKind::Media)
            .filter(|r| match kind {
                MediaKind::All => true,
                MediaKind::Photos => !self.is_document(&Media::new(r)),
                MediaKind::Documents => self.is_document(&Media::new(r)),
            })
            .filter_map(|r| r.xref.as_deref())
            .collect();
        self.media_rows(&xrefs, None)
    }

    /// Records a media is linked to, for the viewer.
    pub fn media_links(&self, id: String) -> Vec<MediaLink> {
        // Areas on the first page only: the viewer shows the media's first file.
        let links = self.ged.media_links(&id);
        let crop_of = |owner: &str| {
            links.iter().find(|l| l.owner == owner && l.page == 1 && l.crop.is_some()).and_then(|l| l.crop)
        };
        self.ged
            .referrers(&id)
            .into_iter()
            .filter_map(|r| {
                let x = r.xref.as_deref()?;
                Some(MediaLink {
                    title: self.ged.record_label(x),
                    person: (r.tag == "INDI").then(|| x.to_string()),
                    source: (r.tag == "SOUR").then(|| x.to_string()),
                    crop: (r.tag == "INDI").then(|| crop_of(x)).flatten().map(ImageCrop::from),
                })
            })
            .collect()
    }

    /// An event, opened from a timeline row.
    pub fn event(&self, event: EventRef) -> Option<EventDetail> {
        let index = event.index as usize;
        let node = self.ged.get(&event.owner)?.children.get(index)?;
        let e = gedcom::Event { node };
        let title = self.ged.event_title(&event.owner, index)?;
        let place = e.place().filter(|p| !p.trim().is_empty()).map(|name| {
            let coords = self.coords.get(name);
            PlaceRef {
                name: place_without_codes(name),
                short: gedcom::short_place(name),
                latitude: coords.map(|c| c.0),
                longitude: coords.map(|c| c.1),
            }
        });
        let participants = self
            .ged
            .event_associates(&event.owner, index)
            .into_iter()
            .filter_map(|a| {
                let role = match a.detail {
                    Some(d) => format!("{} {d}", a.role),
                    None => a.role,
                };
                Some(Participant { person: self.row_of(&a.person)?, role })
            })
            .collect();
        let citations = node
            .children_with("SOUR")
            .filter_map(|c| {
                Some(Citation {
                    source: self.source_row(c.pointer()?)?,
                    page: c.child_val("PAGE").unwrap_or_default().to_string(),
                })
            })
            .collect();
        Some(EventDetail {
            title,
            date: e.date().map(|d| d.display()).unwrap_or_default(),
            place,
            principals: self.ged.event_principals(&event.owner).iter().filter_map(|x| self.row_of(x)).collect(),
            participants,
            citations,
            media: self.media_rows(&e.media_xrefs(), None),
            note: node.child("NOTE").map(|n| n.text()).filter(|t| !t.trim().is_empty()),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const GED: &str = "0 HEAD
1 GEDC
2 VERS 7.0
0 @I1@ INDI
1 NAME Marcel Henri /Lemaire/
1 SEX M
1 BIRT
2 DATE 3 MAY 1902
2 PLAC Saint-Lô, Manche, Normandie, France
1 FAMC @F1@
0 @I2@ INDI
1 NAME Louis Auguste /Lemaire/
1 SEX M
1 BIRT
2 DATE 12 MAR 1868
2 PLAC Saint-Lô, Manche, Normandie, France
3 MAP
4 LATI N49.1157
4 LONG W1.0906
1 OCCU Cultivateur
1 OBJE @M2@
2 CROP
3 TOP 10
3 LEFT 20
3 WIDTH 30
3 HEIGHT 40
1 FAMS @F1@
1 FAMC @F2@
0 @I3@ INDI
1 NAME Marie-Joséphine /Hébert/
1 SEX F
1 BIRT
2 DATE 1872
1 FAMS @F1@
0 @I4@ INDI
1 NAME Jean-Baptiste /Lemaire/
1 SEX M
1 DEAT
2 DATE 1897
1 FAMS @F2@
0 @I5@ INDI
1 NAME Auguste /Lecoq/
1 SEX M
0 @F1@ FAM
1 HUSB @I2@
1 WIFE @I3@
1 CHIL @I1@
1 MARR
2 DATE 4 FEB 1895
2 PLAC Agneaux, Manche, Normandie, France
2 ASSO @I5@
3 ROLE WITN
2 SOUR @S1@
3 PAGE View 12
2 NOTE Contrat chez Me Duval
0 @F2@ FAM
1 HUSB @I4@
1 CHIL @I2@
0 @S1@ SOUR
1 TITL Agneaux — Marriages 1893–1902
1 OBJE @M1@
0 @M1@ OBJE
1 FILE Actes/mariage.jpg
2 FORM image/jpeg
0 @M2@ OBJE
1 FILE Photos/louis.jpg
2 FORM image/jpeg
0 TRLR
";

    fn tree() -> Tree {
        Tree::from_gedcom(Gedcom::parse(GED).unwrap(), std::env::temp_dir())
    }

    #[test]
    fn places_lose_their_codes_and_titles_their_french_spacing() {
        assert_eq!(place_without_codes("Troyes, 10000, Aube, Grand Est, France"), "Troyes, Aube, Grand Est, France");
        assert_eq!(place_without_codes("Paris XII, 75112, Paris,, France"), "Paris XII, Paris, France");
        // English titles use English typography, whatever the file's language.
        assert_eq!(tree().ged.event_title("I2", 3).as_deref(), Some("Occupation: Cultivateur"));
    }

    #[test]
    fn lineage_follows_the_first_parent() {
        assert_eq!(lineage_of(Some(1)), Lineage::Root);
        assert_eq!(lineage_of(Some(2)), Lineage::Paternal);
        assert_eq!(lineage_of(Some(5)), Lineage::Paternal);
        assert_eq!(lineage_of(Some(3)), Lineage::Maternal);
        assert_eq!(lineage_of(Some(13)), Lineage::Maternal);
        assert_eq!(lineage_of(None), Lineage::Other);
    }

    #[test]
    fn rows_carry_sosa_lineage_and_place() {
        let t = tree();
        let rows = t.individuals();
        let names: Vec<&str> = rows.iter().map(|r| r.name.as_str()).collect();
        assert_eq!(names[0], "Marie-Joséphine Hébert");
        let louis = rows.iter().find(|r| r.id == "I2").unwrap();
        assert_eq!(louis.sosa, Some(2));
        assert_eq!(louis.lineage, Lineage::Paternal);
        assert!(louis.place.starts_with("Saint-Lô"), "{}", louis.place);
        let witness = rows.iter().find(|r| r.id == "I5").unwrap();
        assert_eq!((witness.sosa, witness.lineage), (None, Lineage::Other));
    }

    #[test]
    fn search_uses_keywords_and_scope() {
        let t = tree();
        let born: Vec<String> = t.search("born:1860-1870".into(), Scope::All).into_iter().map(|r| r.id).collect();
        assert_eq!(born, ["I2"]);
        let ancestors: Vec<String> = t.search(String::new(), Scope::Ancestors).into_iter().map(|r| r.id).collect();
        assert!(!ancestors.contains(&"I5".to_string()));
        assert_eq!(ancestors.len(), 4);
    }

    #[test]
    fn person_record_has_timeline_family_and_kinship() {
        let t = tree();
        let p = t.person("I2".into()).unwrap();
        assert_eq!(p.occupation.as_deref(), Some("Cultivateur"));
        assert!(p.kinship.is_some());
        assert_eq!(p.root_name.as_deref(), Some("Marcel Henri Lemaire"));
        assert_eq!(p.parents.iter().map(|r| r.id.as_str()).collect::<Vec<_>>(), ["I4"]);
        assert_eq!(p.unions.len(), 1);
        assert_eq!(p.unions[0].spouse.as_ref().map(|s| s.id.as_str()), Some("I3"));
        assert_eq!(p.unions[0].children.len(), 1);
        assert!(!p.unions[0].married.is_empty());
        let kinds: Vec<TimelineKind> = p.timeline.iter().map(|r| r.kind).collect();
        assert!(kinds.contains(&TimelineKind::Own));
        assert!(kinds.contains(&TimelineKind::Family));
        assert!(kinds.contains(&TimelineKind::Relative));
        assert_eq!(p.timeline[0].year, "1868");
        assert!(p.timeline.iter().all(|r| r.event.is_some()));
        assert_eq!(p.sources.iter().map(|s| s.id.as_str()).collect::<Vec<_>>(), Vec::<&str>::new());
        // The root person has no kinship to themselves.
        assert!(t.person("I1".into()).unwrap().kinship.is_none());
    }

    #[test]
    fn family_view_has_two_generations_up_and_the_unions() {
        let t = tree();
        let f = t.family("I1".into()).unwrap();
        let ids = |v: &[Option<PersonRow>]| v.iter().map(|r| r.as_ref().map(|r| r.id.clone())).collect::<Vec<_>>();
        assert_eq!(ids(&f.parents), [Some("I2".into()), Some("I3".into())]);
        assert_eq!(ids(&f.grandparents), [Some("I4".into()), None, None, None]);
        assert!(!f.more_ancestors);
        assert!(f.unions.is_empty());
        let louis = t.family("I2".into()).unwrap();
        assert_eq!(louis.unions[0].children[0].id, "I1");
    }

    #[test]
    fn places_with_coordinates_and_their_events() {
        let t = tree();
        let pins = t.places();
        assert_eq!(pins.len(), 1, "only Saint-Lô has coordinates");
        assert_eq!((pins[0].events, pins[0].ancestors), (2, true));
        let events = t.place_events(pins[0].name.clone());
        assert_eq!(events.iter().map(|e| e.year.as_str()).collect::<Vec<_>>(), ["1868", "1902"]);
        assert_eq!(events[0].people[0].id, "I2");
        assert!(t.event(events[1].event.clone()).is_some());
    }

    #[test]
    fn sources_acts_and_media() {
        let t = tree();
        assert_eq!(t.sources().len(), 1);
        let s = t.source("S1".into()).unwrap();
        assert_eq!(s.acts.len(), 1);
        assert_eq!(s.acts[0].page, "View 12");
        assert_eq!(s.acts[0].events[0].people.len(), 2);
        assert_eq!(s.media.iter().map(|m| m.id.as_str()).collect::<Vec<_>>(), ["M1"]);
        let ids = |k| t.media(k).into_iter().map(|m| m.id).collect::<Vec<_>>();
        assert_eq!(ids(MediaKind::Documents), ["M1"]);
        assert_eq!(ids(MediaKind::Photos), ["M2"]);
        assert_eq!(ids(MediaKind::All).len(), 2);
        let links = t.media_links("M2".into());
        assert_eq!(links[0].person.as_deref(), Some("I2"));
        let face = ImageCrop { left: 20, top: 10, width: Some(30), height: Some(40) };
        assert_eq!(links[0].crop, Some(face));
        assert_eq!(t.person("I2".into()).unwrap().media[0].crop, Some(face));
        assert_eq!(t.media_links("M1".into())[0].crop, None);
        assert_eq!(t.media_links("M1".into())[0].source.as_deref(), Some("S1"));
    }

    #[test]
    fn event_detail_opens_from_the_timeline() {
        let t = tree();
        let p = t.person("I2".into()).unwrap();
        let marriage = p.timeline.iter().find(|r| r.kind == TimelineKind::Family).unwrap();
        let e = t.event(marriage.event.clone().unwrap()).unwrap();
        assert_eq!(e.principals.iter().map(|r| r.id.as_str()).collect::<Vec<_>>(), ["I2", "I3"]);
        assert_eq!(e.participants.len(), 1);
        assert_eq!(e.participants[0].person.id, "I5");
        assert_eq!(e.citations.len(), 1);
        assert_eq!(e.citations[0].page, "View 12");
        assert_eq!(e.citations[0].source.title, "Agneaux — Marriages 1893–1902");
        assert_eq!(e.note.as_deref(), Some("Contrat chez Me Duval"));
        assert_eq!(e.place.as_ref().map(|p| p.short.as_str()).map(|s| s.starts_with("Agneaux")), Some(true));
        let birth = p.timeline.iter().find(|r| r.kind == TimelineKind::Own).unwrap();
        let b = t.event(birth.event.clone().unwrap()).unwrap();
        let place = b.place.unwrap();
        assert!((place.latitude.unwrap() - 49.1157).abs() < 1e-6);
        assert!((place.longitude.unwrap() + 1.0906).abs() < 1e-6);
    }
}
