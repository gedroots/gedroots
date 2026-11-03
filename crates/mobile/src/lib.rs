//! Rust core of the read-only mobile app, exposed to Swift (and later Kotlin) with UniFFI. The app
//! pairs with the desktop (File › Mobile Devices… › Add a Device), syncs the tree into its own
//! folder, then reads it: the list of people and their records.

use gedcom::{Gedcom, RecordKind};
use gedroots_sync::{DeviceKind, PairCode, Receiver, Refused};
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

#[derive(uniffi::Record)]
pub struct PersonRow {
    /// Internal identifier, never shown.
    pub id: String,
    pub name: String,
    pub surname: String,
    pub years: String,
}

#[derive(uniffi::Record)]
pub struct EventRow {
    pub label: String,
    pub date: String,
    pub place: String,
}

#[derive(uniffi::Record)]
pub struct UnionRow {
    pub spouse: Option<PersonRow>,
    pub children: Vec<PersonRow>,
}

#[derive(uniffi::Record)]
pub struct PersonDetail {
    pub row: PersonRow,
    pub sex: String,
    pub events: Vec<EventRow>,
    pub parents: Vec<PersonRow>,
    pub unions: Vec<UnionRow>,
    /// Full path of the portrait image, if any.
    pub portrait: Option<String>,
}

/// A received tree, read-only.
#[derive(uniffi::Object)]
pub struct Tree {
    ged: Gedcom,
    dir: PathBuf,
}

#[uniffi::export]
impl Tree {
    /// Opens the tree that `sync` put in `tree_dir`.
    #[uniffi::constructor]
    pub fn open(tree_dir: String) -> Result<Arc<Self>, MobileError> {
        let dir = PathBuf::from(tree_dir);
        let ged = gedcom::load(dir.join(gedcom::gedzip::DATASET)).map_err(MobileError::Failed)?;
        Ok(Arc::new(Self { ged, dir }))
    }

    /// Every individual, sorted by surname then given name.
    pub fn individuals(&self) -> Vec<PersonRow> {
        let mut rows: Vec<(String, PersonRow)> = self
            .ged
            .records_of(RecordKind::Individual)
            .map(gedcom::Individual::new)
            .map(|i| (gedcom::sort_key(&format!("{} {}", i.surname(), i.given())), row(&i)))
            .collect();
        rows.sort_by(|a, b| a.0.cmp(&b.0));
        rows.into_iter().map(|(_, r)| r).collect()
    }

    pub fn root(&self) -> Option<String> {
        self.ged.root()
    }

    pub fn person(&self, id: String) -> Option<PersonDetail> {
        let ind = self.ged.individual(&id)?;
        let events = ind
            .events()
            .iter()
            .map(|e| EventRow {
                label: e.label(),
                date: e.date().map(|d| d.display()).unwrap_or_default(),
                place: e.place().map(gedcom::short_place).unwrap_or_default(),
            })
            .collect();
        let (father, mother) = self.ged.parents(&id);
        let parents = [father, mother].into_iter().flatten().map(|p| row(&p)).collect();
        let unions = self
            .ged
            .spouses_and_children(&id)
            .into_iter()
            .map(|(spouse, children)| UnionRow {
                spouse: spouse.map(|s| row(&s)),
                children: children.iter().map(row).collect(),
            })
            .collect();
        let portrait = self
            .ged
            .portrait(&id)
            .and_then(|m| m.file())
            .map(|f| self.dir.join(gedcom::gedzip::unescape(f.trim())))
            .filter(|p| p.is_file())
            .map(|p| p.display().to_string());
        Some(PersonDetail { row: row(&ind), sex: ind.sex_label().into_owned(), events, parents, unions, portrait })
    }
}

fn row(i: &gedcom::Individual) -> PersonRow {
    PersonRow { id: i.xref().to_string(), name: i.display_name(), surname: i.surname(), years: i.years() }
}
