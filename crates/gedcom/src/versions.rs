//! Version history of a tree, kept in a hidden folder beside the `.ged` so that it follows the
//! file through Dropbox, iCloud Drive or OneDrive.
//!
//! The folder of `Guyot.ged` is `.Guyot.ged.history/`:
//!
//! - `versions/<id>.ver`: one file per version, the list of its records (hash and identifier) and
//!   what it is (time, kind, label, author), compressed;
//! - `packs/<id>.pack`: the records a version brought that no earlier pack holds. An unchanged
//!   record is stored once, whatever the number of versions.
//!
//! Every file is written once and never modified, under a name no other computer can choose: two
//! computers that write at the same time only add files, so a sync service never makes a
//! “conflicted copy”, and the union of both folders is a valid history. Only [`Store::prune`]
//! deletes files: old versions, then packs no version needs any more (a pack that is partly
//! needed is first copied into a new one).
//!
//! See `docs/version-history.md` for the file formats.

use crate::document::Gedcom;
use crate::node::Node;
use crate::parse;
use flate2::Compression;
use flate2::read::DeflateDecoder;
use flate2::write::DeflateEncoder;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

const VERSION_MAGIC: &str = "GEDROOTS VERSION 1";
const PACK_MAGIC: &str = "GEDROOTS PACK 1";
const DAY: u64 = 86_400;

/// What made a version.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Kind {
    /// The file as GEDRoots first found it.
    Opened,
    /// Saved in GEDRoots.
    Saved,
    /// Saved and named by the user (kept until deleted).
    Named,
    /// A proposal of an AI agent accepted in To Review.
    Agent,
    /// Recorded before an operation that changes many records (import fixes, merge…).
    RestorePoint,
    /// The file changed outside GEDRoots (another software, a sync service).
    Outside,
    /// An earlier version brought back.
    Restored,
}

impl Kind {
    fn as_str(self) -> &'static str {
        match self {
            Kind::Opened => "opened",
            Kind::Saved => "saved",
            Kind::Named => "named",
            Kind::Agent => "agent",
            Kind::RestorePoint => "restore-point",
            Kind::Outside => "outside",
            Kind::Restored => "restored",
        }
    }

    fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "opened" => Kind::Opened,
            "saved" => Kind::Saved,
            "named" => Kind::Named,
            "agent" => Kind::Agent,
            "restore-point" => Kind::RestorePoint,
            "outside" => Kind::Outside,
            "restored" => Kind::Restored,
            _ => return None,
        })
    }
}

/// Hash of a record's GEDCOM lines (BLAKE3, first 128 bits, in hexadecimal).
type Hash = String;

fn hash(text: &str) -> Hash {
    let h = blake3::hash(text.as_bytes());
    h.to_hex()[..32].to_string()
}

/// A version of the tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Version {
    /// Unique, and in the order of the versions: `<milliseconds>-<random>`.
    pub id: String,
    /// Seconds since 1970 (UTC).
    pub time: u64,
    pub kind: Kind,
    pub label: Option<String>,
    /// Who made it, when it was not the user (the name of an AI agent).
    pub author: Option<String>,
    /// `HEAD._GR_TREE` of the tree.
    pub tree: Option<String>,
    /// Records added, changed and removed since the previous version.
    pub added: usize,
    pub changed: usize,
    pub removed: usize,
    head: Hash,
    /// Records in file order: hash and identifier (empty for a record without one).
    records: Vec<(Hash, String)>,
}

/// Records added, changed and removed between two states of a tree, by identifier, in file order.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Changes {
    pub added: Vec<String>,
    pub changed: Vec<String>,
    pub removed: Vec<String>,
}

impl Changes {
    fn between(before: &[(Hash, String)], after: &[(Hash, String)]) -> Self {
        let index = |l: &[(Hash, String)]| -> HashMap<String, Hash> {
            l.iter().filter(|(_, x)| !x.is_empty()).map(|(h, x)| (x.clone(), h.clone())).collect()
        };
        let (old, new) = (index(before), index(after));
        let mut c = Changes::default();
        for (_, x) in after.iter().filter(|(_, x)| !x.is_empty()) {
            match old.get(x) {
                None => c.added.push(x.clone()),
                Some(h) if *h != new[x] => c.changed.push(x.clone()),
                Some(_) => {}
            }
        }
        c.removed =
            before.iter().filter(|(_, x)| !x.is_empty() && !new.contains_key(x)).map(|(_, x)| x.clone()).collect();
        c
    }

    pub fn is_empty(&self) -> bool {
        self.added.is_empty() && self.changed.is_empty() && self.removed.is_empty()
    }
}

impl Version {
    /// What this version changed since `earlier` (every record when there is none).
    pub fn changes_since(&self, earlier: Option<&Version>) -> Changes {
        Changes::between(earlier.map_or(&[][..], |e| &e.records), &self.records)
    }

    /// What `ged` changed since this version.
    pub fn changes_to(&self, ged: &Gedcom) -> Changes {
        Changes::between(&self.records, &Snapshot::of(ged).list())
    }

    /// Same records and header as `ged`.
    pub fn is_same_as(&self, ged: &Gedcom) -> bool {
        let snap = Snapshot::of(ged);
        self.same_content(&snap.head.0, &snap.list())
    }

    /// Identifiers of the records, in file order.
    pub fn xrefs(&self) -> impl Iterator<Item = &str> {
        self.records.iter().map(|(_, x)| x.as_str()).filter(|x| !x.is_empty())
    }

    fn same_content(&self, head: &str, records: &[(Hash, String)]) -> bool {
        self.head == head && self.records == records
    }

    fn write(&self) -> String {
        let mut s = format!("{VERSION_MAGIC}\nid {}\ntime {}\nkind {}\n", self.id, self.time, self.kind.as_str());
        for (key, v) in [("label", &self.label), ("author", &self.author), ("tree", &self.tree)] {
            if let Some(v) = v {
                s.push_str(&format!("{key} {}\n", one_line(v)));
            }
        }
        s.push_str(&format!(
            "added {}\nchanged {}\nremoved {}\nhead {}\n",
            self.added, self.changed, self.removed, self.head
        ));
        for (h, x) in &self.records {
            match x.is_empty() {
                true => s.push_str(&format!("rec {h}\n")),
                false => s.push_str(&format!("rec {h} {x}\n")),
            }
        }
        s
    }

    fn read(text: &str) -> Option<Self> {
        let mut lines = text.lines();
        if lines.next()? != VERSION_MAGIC {
            return None;
        }
        let mut v = Version {
            id: String::new(),
            time: 0,
            kind: Kind::Saved,
            label: None,
            author: None,
            tree: None,
            added: 0,
            changed: 0,
            removed: 0,
            head: String::new(),
            records: Vec::new(),
        };
        for line in lines {
            let (key, val) = line.split_once(' ').unwrap_or((line, ""));
            match key {
                "id" => v.id = val.to_string(),
                "time" => v.time = val.parse().ok()?,
                "kind" => v.kind = Kind::parse(val)?,
                "label" => v.label = Some(val.to_string()),
                "author" => v.author = Some(val.to_string()),
                "tree" => v.tree = Some(val.to_string()),
                "added" => v.added = val.parse().ok()?,
                "changed" => v.changed = val.parse().ok()?,
                "removed" => v.removed = val.parse().ok()?,
                "head" => v.head = val.to_string(),
                "rec" => {
                    let (h, x) = val.split_once(' ').unwrap_or((val, ""));
                    v.records.push((h.to_string(), x.to_string()));
                }
                // A later version of GEDRoots may add lines: they are kept by not failing.
                _ => {}
            }
        }
        (!v.id.is_empty() && !v.head.is_empty()).then_some(v)
    }
}

fn one_line(s: &str) -> String {
    s.replace(['\r', '\n'], " ")
}

/// A change of one record (see [`Store::record_history`]).
#[derive(Debug, Clone)]
pub struct RecordChange {
    pub version: Version,
    pub before: Option<Node>,
    pub after: Option<Node>,
}

/// GEDCOM lines of a record, as a version stores them.
fn record_text(n: &Node) -> String {
    let mut s = String::new();
    parse::write_nodes(std::iter::once(n), 0, "\n", &mut s);
    s
}

/// A record from its stored lines.
fn parse_record(text: &str) -> Option<Node> {
    let g = Gedcom::parse(&format!("0 HEAD\n1 GEDC\n2 VERS 7.0\n{text}0 TRLR\n")).ok()?;
    g.records.first().map(|r| (**r).clone())
}

/// A tree split into the blobs a version stores.
struct Snapshot {
    head: (Hash, String),
    records: Vec<(Hash, String, String)>,
}

impl Snapshot {
    fn of(ged: &Gedcom) -> Self {
        let text = record_text;
        let head = text(&ged.header);
        let records = ged
            .records
            .iter()
            .map(|r| {
                let t = text(r);
                (hash(&t), r.xref.clone().unwrap_or_default(), t)
            })
            .collect();
        Snapshot { head: (hash(&head), head), records }
    }

    fn list(&self) -> Vec<(Hash, String)> {
        self.records.iter().map(|(h, x, _)| (h.clone(), x.clone())).collect()
    }
}

/// Index of a pack: offset and length of each blob in its uncompressed data.
struct Pack {
    path: PathBuf,
    blobs: Vec<(Hash, usize, usize)>,
    data_start: usize,
}

impl Pack {
    fn read_index(path: &Path) -> io::Result<Self> {
        let bytes = std::fs::read(path)?;
        let bad = || io::Error::new(io::ErrorKind::InvalidData, format!("{}: not a pack", path.display()));
        let mut pos = 0;
        let mut line = || -> Option<&str> {
            let end = bytes[pos..].iter().position(|&b| b == b'\n')? + pos;
            let l = std::str::from_utf8(&bytes[pos..end]).ok()?;
            pos = end + 1;
            Some(l)
        };
        if line() != Some(PACK_MAGIC) {
            return Err(bad());
        }
        let mut blobs = Vec::new();
        let mut offset = 0;
        loop {
            let l = line().ok_or_else(bad)?;
            if l.is_empty() {
                break;
            }
            let (h, len) = l.split_once(' ').ok_or_else(bad)?;
            let len: usize = len.parse().map_err(|_| bad())?;
            blobs.push((h.to_string(), offset, len));
            offset += len;
        }
        Ok(Pack { path: path.to_path_buf(), blobs, data_start: pos })
    }

    fn data(&self) -> io::Result<Vec<u8>> {
        let bytes = std::fs::read(&self.path)?;
        let mut out = Vec::new();
        DeflateDecoder::new(&bytes[self.data_start.min(bytes.len())..]).read_to_end(&mut out)?;
        Ok(out)
    }

    fn write(path: &Path, blobs: &[(&str, &str)]) -> io::Result<()> {
        let mut head = format!("{PACK_MAGIC}\n");
        for (h, t) in blobs {
            head.push_str(&format!("{h} {}\n", t.len()));
        }
        head.push('\n');
        // The index stays readable without inflating: only the data that follows is compressed.
        let mut enc = DeflateEncoder::new(head.into_bytes(), Compression::default());
        for (_, t) in blobs {
            enc.write_all(t.as_bytes())?;
        }
        write_once(path, &enc.finish()?)
    }
}

/// Writes a new file through a temporary one, so that no one ever sees it half written.
fn write_once(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, bytes)?;
    std::fs::rename(tmp, path)
}

fn compress(text: &str) -> io::Result<Vec<u8>> {
    let mut enc = DeflateEncoder::new(Vec::new(), Compression::default());
    enc.write_all(text.as_bytes())?;
    enc.finish()
}

fn decompress(bytes: &[u8]) -> io::Result<String> {
    let mut s = String::new();
    DeflateDecoder::new(bytes).read_to_string(&mut s)?;
    Ok(s)
}

fn now_millis() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

fn now() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

/// A new identifier: milliseconds since 1970, later than `after` (versions are ordered by it, even
/// when several are recorded in the same millisecond), then random digits.
fn new_id(millis: u64, after: Option<&str>) -> String {
    let last = after.and_then(|a| a.split('-').next()?.parse::<u64>().ok());
    let millis = last.map_or(millis, |l| millis.max(l + 1));
    format!("{millis:014}-{}", &uuid::Uuid::new_v4().simple().to_string()[..12])
}

/// History folder of a `.ged` file: `.Guyot.ged.history/` beside `Guyot.ged`.
pub fn folder_of(ged_path: &Path) -> PathBuf {
    let name = ged_path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    ged_path.with_file_name(format!(".{name}.history"))
}

/// The version history of one tree.
#[derive(Debug, Clone)]
pub struct Store {
    dir: PathBuf,
}

impl Store {
    /// History of the file at `ged_path`. When the file was renamed or moved within its folder,
    /// the history it left (same `tree` identifier, file gone) is renamed to follow it.
    pub fn for_file(ged_path: &Path, tree: Option<&str>) -> Self {
        let dir = folder_of(ged_path);
        if let (false, Some(tree), Some(parent)) = (dir.exists(), tree, ged_path.parent())
            && let Some(old) = Self::orphan_of(parent, tree)
        {
            let _ = std::fs::rename(old, &dir);
        }
        Store { dir }
    }

    /// A history folder of `parent` whose file no longer exists and whose tree is `tree`.
    fn orphan_of(parent: &Path, tree: &str) -> Option<PathBuf> {
        std::fs::read_dir(parent).ok()?.flatten().map(|e| e.path()).find(|p| {
            let Some(name) = p.file_name().and_then(|n| n.to_str()) else { return false };
            let Some(file) = name.strip_prefix('.').and_then(|n| n.strip_suffix(".history")) else { return false };
            p.is_dir()
                && !parent.join(file).exists()
                && Store { dir: p.clone() }
                    .versions()
                    .ok()
                    .and_then(|v| v.last().and_then(|v| v.tree.clone()))
                    .as_deref()
                    == Some(tree)
        })
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    fn versions_dir(&self) -> PathBuf {
        self.dir.join("versions")
    }

    fn packs_dir(&self) -> PathBuf {
        self.dir.join("packs")
    }

    fn files(dir: &Path, ext: &str) -> Vec<PathBuf> {
        let Ok(rd) = std::fs::read_dir(dir) else { return vec![] };
        let mut v: Vec<_> =
            rd.flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|e| e == ext)).collect();
        v.sort();
        v
    }

    /// Every version, oldest first. Unreadable files (a newer format, a download in progress) are
    /// skipped.
    pub fn versions(&self) -> io::Result<Vec<Version>> {
        let mut out: Vec<Version> = Self::files(&self.versions_dir(), "ver")
            .iter()
            .filter_map(|p| Version::read(&decompress(&std::fs::read(p).ok()?).ok()?))
            .collect();
        out.sort_by(|a, b| a.id.cmp(&b.id));
        Ok(out)
    }

    fn packs(&self) -> Vec<Pack> {
        Self::files(&self.packs_dir(), "pack").iter().filter_map(|p| Pack::read_index(p).ok()).collect()
    }

    /// Records the tree as a new version, unless it is the same as the latest one.
    pub fn record(
        &self,
        ged: &Gedcom,
        kind: Kind,
        label: Option<&str>,
        author: Option<&str>,
    ) -> io::Result<Option<Version>> {
        self.record_at(ged, kind, label, author, now_millis())
    }

    /// Records the file as GEDRoots opens it: its first version, or the changes made outside
    /// GEDRoots since the latest one. Nothing when it did not change.
    pub fn record_opened(&self, ged: &Gedcom) -> io::Result<Option<Version>> {
        let kind = if self.versions()?.is_empty() { Kind::Opened } else { Kind::Outside };
        self.record(ged, kind, None, None)
    }

    fn record_at(
        &self,
        ged: &Gedcom,
        kind: Kind,
        label: Option<&str>,
        author: Option<&str>,
        millis: u64,
    ) -> io::Result<Option<Version>> {
        let time = millis / 1000;
        let snap = Snapshot::of(ged);
        let list = snap.list();
        let latest = self.versions()?.pop();
        if let Some(l) = &latest
            && l.same_content(&snap.head.0, &list)
            && !matches!(kind, Kind::Named | Kind::RestorePoint)
        {
            return Ok(None);
        }
        let (added, changed, removed) = match &latest {
            Some(l) => {
                let before: HashMap<&str, &str> =
                    l.records.iter().filter(|(_, x)| !x.is_empty()).map(|(h, x)| (x.as_str(), h.as_str())).collect();
                let now: HashMap<&str, &str> =
                    list.iter().filter(|(_, x)| !x.is_empty()).map(|(h, x)| (x.as_str(), h.as_str())).collect();
                let added = now.keys().filter(|x| !before.contains_key(*x)).count();
                let removed = before.keys().filter(|x| !now.contains_key(*x)).count();
                let changed = now.iter().filter(|(x, h)| before.get(*x).is_some_and(|b| b != *h)).count();
                (added, changed, removed)
            }
            None => (list.len(), 0, 0),
        };
        self.create_dirs()?;
        let id = new_id(millis, latest.as_ref().map(|l| l.id.as_str()));
        let stored: HashSet<Hash> = self.packs().into_iter().flat_map(|p| p.blobs.into_iter().map(|b| b.0)).collect();
        let mut seen = HashSet::new();
        let new: Vec<(&str, &str)> = std::iter::once((&snap.head.0, &snap.head.1))
            .chain(snap.records.iter().map(|(h, _, t)| (h, t)))
            .filter(|(h, _)| !stored.contains(*h) && seen.insert(h.as_str()))
            .map(|(h, t)| (h.as_str(), t.as_str()))
            .collect();
        // The pack first: a version never names a record that no file holds.
        if !new.is_empty() {
            Pack::write(&self.packs_dir().join(format!("{id}.pack")), &new)?;
        }
        let v = Version {
            id: id.clone(),
            time,
            kind,
            label: label.map(str::to_string),
            author: author.map(str::to_string),
            tree: ged.tree_id().map(str::to_string),
            added,
            changed,
            removed,
            head: snap.head.0,
            records: list,
        };
        write_once(&self.versions_dir().join(format!("{id}.ver")), &compress(&v.write())?)?;
        Ok(Some(v))
    }

    fn create_dirs(&self) -> io::Result<()> {
        let new = !self.dir.exists();
        std::fs::create_dir_all(self.versions_dir())?;
        std::fs::create_dir_all(self.packs_dir())?;
        if new {
            hide(&self.dir);
        }
        Ok(())
    }

    /// The tree as it was in `version`.
    pub fn load(&self, version: &Version) -> Result<Gedcom, String> {
        let wanted: HashSet<&str> =
            std::iter::once(version.head.as_str()).chain(version.records.iter().map(|(h, _)| h.as_str())).collect();
        let texts = self.blobs(&wanted)?;
        let mut s = texts[&version.head].clone();
        for (h, _) in &version.records {
            s.push_str(&texts[h]);
        }
        s.push_str("0 TRLR\n");
        Gedcom::parse(&s).map_err(|e| e.to_string())
    }

    /// Every change of record `xref`, oldest first: the version that made it, and the record
    /// before and after (`None`: not in the tree). The oldest version that holds the record counts
    /// as a change from nothing.
    pub fn record_history(&self, xref: &str) -> Result<Vec<RecordChange>, String> {
        let versions = self.versions().map_err(|e| e.to_string())?;
        let mut steps: Vec<(usize, Option<String>, Option<String>)> = Vec::new();
        let mut prev: Option<String> = None;
        for (i, v) in versions.iter().enumerate() {
            let h = v.records.iter().find(|(_, x)| x == xref).map(|(h, _)| h.clone());
            if h != prev {
                steps.push((i, prev.clone(), h.clone()));
                prev = h;
            }
        }
        let wanted: HashSet<&str> = steps.iter().flat_map(|(_, a, b)| [a.as_deref(), b.as_deref()]).flatten().collect();
        let texts = self.blobs(&wanted)?;
        let node = |h: &Option<String>| h.as_ref().and_then(|h| parse_record(&texts[h]));
        Ok(steps
            .iter()
            .map(|(i, a, b)| RecordChange { version: versions[*i].clone(), before: node(a), after: node(b) })
            .collect())
    }

    /// `current` with the changes `version` made since `earlier` taken back: each record it
    /// changed, added or removed returns to its state in `earlier`, unless it changed again since
    /// (such records are left as they are and counted). Returns the tree and the numbers of records
    /// taken back and left.
    pub fn undo_changes(
        &self,
        version: &Version,
        earlier: &Version,
        current: &Gedcom,
    ) -> Result<(Gedcom, usize, usize), String> {
        let changes = version.changes_since(Some(earlier));
        let before = self.load(earlier)?;
        let hash_in = |v: &Version, x: &str| v.records.iter().find(|(_, r)| r == x).map(|(h, _)| h.clone());
        let unchanged_since = |x: &str| current.get(x).map(|n| hash(&record_text(n))) == hash_in(version, x);
        let mut g = current.clone();
        let (mut done, mut left) = (0, 0);
        for x in changes.changed.iter().chain(&changes.added) {
            if !unchanged_since(x) {
                left += 1;
                continue;
            }
            match before.get(x) {
                Some(old) => {
                    let i = g.records.iter().position(|r| r.xref.as_deref() == Some(x.as_str()));
                    if let Some(i) = i {
                        g.records[i] = std::sync::Arc::new(old.clone());
                    }
                }
                None => g.records.retain(|r| r.xref.as_deref() != Some(x.as_str())),
            }
            done += 1;
        }
        for x in &changes.removed {
            match (g.get(x), before.get(x)) {
                (None, Some(old)) => {
                    g.records.push(std::sync::Arc::new(old.clone()));
                    done += 1;
                }
                _ => left += 1,
            }
        }
        g.reindex();
        Ok((g, done, left))
    }

    /// Texts of the blobs with these hashes, read from the packs.
    fn blobs(&self, wanted: &HashSet<&str>) -> Result<HashMap<String, String>, String> {
        let mut texts: HashMap<String, String> = HashMap::new();
        for pack in self.packs() {
            if !pack.blobs.iter().any(|(h, _, _)| wanted.contains(h.as_str()) && !texts.contains_key(h)) {
                continue;
            }
            let data = pack.data().map_err(|e| e.to_string())?;
            for (h, off, len) in &pack.blobs {
                if wanted.contains(h.as_str())
                    && let Some(b) = data.get(*off..off + len)
                {
                    texts.insert(h.clone(), String::from_utf8_lossy(b).into_owned());
                }
            }
        }
        let missing = wanted.iter().filter(|h| !texts.contains_key(**h)).count();
        if missing > 0 {
            return Err(rust_i18n::t!("error.version_incomplete", count = missing).into());
        }
        Ok(texts)
    }

    /// Deletes the versions the retention no longer keeps, then the packs no version needs.
    ///
    /// Kept: every version of the last 24 hours, the last one of each day for 30 days, the last
    /// one of each week before; named versions always; restore points, agent and outside
    /// versions for 30 days; and the latest version.
    pub fn prune(&self) -> io::Result<usize> {
        self.prune_at(now())
    }

    fn prune_at(&self, now: u64) -> io::Result<usize> {
        let versions = self.versions()?;
        let Some(latest) = versions.last() else { return Ok(0) };
        let mut keep: HashSet<&str> = HashSet::from([latest.id.as_str()]);
        // Last version of each period (day for the last 30 days, week before).
        let mut last_of: BTreeMap<(bool, u64), &str> = BTreeMap::new();
        for v in &versions {
            let age = now.saturating_sub(v.time);
            let special = matches!(v.kind, Kind::RestorePoint | Kind::Agent | Kind::Outside);
            if v.kind == Kind::Named || age < DAY || (special && age < 30 * DAY) {
                keep.insert(&v.id);
            }
            let days = v.time / DAY;
            // 1 January 1970 was a Thursday: weeks start on Monday.
            let period = if age < 30 * DAY { (false, days) } else { (true, (days + 3) / 7) };
            last_of.insert(period, &v.id);
        }
        keep.extend(last_of.into_values());
        let mut removed = 0;
        for v in versions.iter().filter(|v| !keep.contains(v.id.as_str())) {
            std::fs::remove_file(self.versions_dir().join(format!("{}.ver", v.id)))?;
            removed += 1;
        }
        self.collect_packs(now)?;
        Ok(removed)
    }

    /// Deletes the packs no version needs; copies the needed part of a pack that is mostly unused
    /// into a new pack.
    fn collect_packs(&self, now: u64) -> io::Result<()> {
        let live: HashSet<Hash> = self
            .versions()?
            .into_iter()
            .flat_map(|v| std::iter::once(v.head).chain(v.records.into_iter().map(|(h, _)| h)))
            .collect();
        let mut carried = HashSet::new();
        for pack in self.packs() {
            let used: Vec<&(Hash, usize, usize)> = pack.blobs.iter().filter(|(h, _, _)| live.contains(h)).collect();
            let total: usize = pack.blobs.iter().map(|b| b.2).sum();
            let kept: usize = used.iter().map(|b| b.2).sum();
            if used.len() == pack.blobs.len() || kept * 2 > total {
                continue;
            }
            if !used.is_empty() {
                let data = pack.data()?;
                let blobs: Vec<(&str, &str)> = used
                    .iter()
                    .filter(|(h, _, _)| carried.insert(h.clone()))
                    .filter_map(|(h, off, len)| {
                        Some((h.as_str(), std::str::from_utf8(data.get(*off..off + len)?).ok()?))
                    })
                    .collect();
                if !blobs.is_empty() {
                    Pack::write(&self.packs_dir().join(format!("{}.pack", new_id(now * 1000, None))), &blobs)?;
                }
            }
            std::fs::remove_file(&pack.path)?;
        }
        Ok(())
    }
}

/// Hides the folder on Windows (the leading dot already hides it on macOS and Linux).
#[cfg(windows)]
fn hide(dir: &Path) {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{FILE_ATTRIBUTE_HIDDEN, SetFileAttributesW};
    let wide: Vec<u16> = dir.as_os_str().encode_wide().chain(std::iter::once(0)).collect();
    unsafe {
        SetFileAttributesW(wide.as_ptr(), FILE_ATTRIBUTE_HIDDEN);
    }
}

#[cfg(not(windows))]
fn hide(_: &Path) {}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = include_str!("../tests/fixtures/example.ged");

    fn store() -> (Store, PathBuf) {
        let dir = std::env::temp_dir().join(format!("gedroots-versions-{}", uuid::Uuid::new_v4().simple()));
        std::fs::create_dir_all(&dir).unwrap();
        let ged = dir.join("tree.ged");
        (Store::for_file(&ged, None), dir)
    }

    #[test]
    fn a_version_round_trips() {
        let (s, dir) = store();
        let g = Gedcom::parse(SAMPLE).unwrap();
        let v = s.record_opened(&g).unwrap().unwrap();
        assert_eq!(v.kind, Kind::Opened);
        assert_eq!(v.added, g.records.len());
        assert!(s.dir().ends_with(".tree.ged.history"));
        let back = s.load(&s.versions().unwrap()[0]).unwrap();
        // Same lines (the text format, BOM or line endings, belongs to the file, not to versions).
        assert_eq!(back.header, g.header);
        assert_eq!(back.records, g.records);
        // Opening the same file again records nothing.
        assert!(s.record_opened(&g).unwrap().is_none());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn an_unchanged_record_is_stored_once() {
        let (s, dir) = store();
        let mut g = Gedcom::parse(SAMPLE).unwrap();
        s.record_at(&g, Kind::Saved, None, None, 1_000_000).unwrap();
        let first = g.clone();
        g.get_mut("I00003").unwrap().set_child_val("SEX", "M");
        let v = s.record_at(&g, Kind::Outside, None, None, 2_000_000).unwrap().unwrap();
        assert_eq!((v.added, v.changed, v.removed), (0, 1, 0));
        let packs = s.packs();
        assert_eq!(packs.len(), 2);
        assert_eq!(packs[1].blobs.len(), 1, "only the changed record");
        let versions = s.versions().unwrap();
        assert_eq!(s.load(&versions[0]).unwrap().get("I00003"), first.get("I00003"));
        assert_eq!(s.load(&versions[1]).unwrap().get("I00003"), g.get("I00003"));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn retention_keeps_recent_named_and_one_per_period() {
        let (s, dir) = store();
        let mut g = Gedcom::parse(SAMPLE).unwrap();
        let now = 400 * DAY;
        let mut ids = Vec::new();
        // Two versions a day, from 100 days ago until today.
        for i in 0..200u64 {
            g.get_mut("I00003").unwrap().set_child_val("NOTE", &i.to_string());
            let kind = if i == 3 { Kind::Named } else { Kind::Saved };
            let v = s.record_at(&g, kind, None, None, (now - 100 * DAY + i * DAY / 2) * 1000).unwrap().unwrap();
            ids.push(v.id);
        }
        s.prune_at(now).unwrap();
        let kept: Vec<String> = s.versions().unwrap().into_iter().map(|v| v.id).collect();
        assert!(kept.contains(&ids[3]), "named");
        assert!(kept.contains(ids.last().unwrap()), "latest");
        assert!(!kept.contains(&ids[196]), "only the last one of an older day");
        // 30 days of one version a day, plus about 10 weeks, plus the recent and named ones.
        assert!((40..=48).contains(&kept.len()), "{}", kept.len());
        // Every kept version can still be read back.
        for v in s.versions().unwrap() {
            s.load(&v).unwrap();
        }
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn changes_between_versions_and_the_tree() {
        let (s, dir) = store();
        let mut g = Gedcom::parse(SAMPLE).unwrap();
        let first = s.record_at(&g, Kind::Saved, None, None, 1_000_000).unwrap().unwrap();
        assert!(first.is_same_as(&g));
        assert_eq!(first.changes_since(None).added.len(), g.records.iter().filter(|r| r.xref.is_some()).count());
        g.get_mut("I00003").unwrap().set_child_val("SEX", "M");
        let gone = g.records.iter().position(|r| r.xref.as_deref() == Some("I00002")).unwrap();
        g.records.remove(gone);
        let second = s.record_at(&g, Kind::Saved, None, None, 2_000_000).unwrap().unwrap();
        let c = second.changes_since(Some(&first));
        assert_eq!((c.added, c.changed, c.removed), (vec![], vec!["I00003".to_string()], vec!["I00002".to_string()]));
        assert!(second.changes_to(&g).is_empty());
        assert_eq!(first.changes_to(&g).changed, vec!["I00003".to_string()]);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_record_history_and_undoing_one_version() {
        let (s, dir) = store();
        let mut g = Gedcom::parse(SAMPLE).unwrap();
        let v1 = s.record_at(&g, Kind::Saved, None, None, 1_000_000).unwrap().unwrap();
        g.get_mut("I00003").unwrap().set_child_val("SEX", "M");
        s.record_at(&g, Kind::Saved, None, None, 2_000_000).unwrap();
        // An agent changes I00003 again and I00002.
        let before_agent = s.versions().unwrap().pop().unwrap();
        g.get_mut("I00003").unwrap().set_child_val("SEX", "F");
        g.get_mut("I00002").unwrap().set_child_val("NOTE", "agent");
        let agent = s.record_at(&g, Kind::Agent, Some("Act saved"), Some("Claude"), 3_000_000).unwrap().unwrap();
        // Unchanged versions do not count; the first one is a change from nothing.
        let h = s.record_history("I00003").unwrap();
        assert_eq!(h.len(), 3);
        assert!(h[0].before.is_none() && h[0].version.id == v1.id);
        assert_eq!(h[2].after.as_ref().unwrap().child_val("SEX"), Some("F"));
        // The user changes I00002 again: undoing the agent's version leaves it as it is.
        g.get_mut("I00002").unwrap().set_child_val("NOTE", "user");
        let (back, done, left) = s.undo_changes(&agent, &before_agent, &g).unwrap();
        assert_eq!((done, left), (1, 1));
        assert_eq!(back.get("I00003").unwrap().child_val("SEX"), Some("M"));
        assert_eq!(back.get("I00002").unwrap().child_val("NOTE"), Some("user"));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_restore_point_is_recorded_even_without_a_change() {
        let (s, dir) = store();
        let g = Gedcom::parse(SAMPLE).unwrap();
        s.record_at(&g, Kind::Saved, None, None, 1_000_000).unwrap();
        assert!(s.record_at(&g, Kind::Saved, None, None, 2_000_000).unwrap().is_none());
        assert!(s.record_at(&g, Kind::RestorePoint, Some("Before merging"), None, 3_000_000).unwrap().is_some());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn the_history_follows_a_renamed_file() {
        let (_, dir) = store();
        let mut g = Gedcom::parse(SAMPLE).unwrap();
        let id = g.ensure_tree_id();
        let old = dir.join("old.ged");
        Store::for_file(&old, Some(&id)).record(&g, Kind::Saved, None, None).unwrap();
        // The file was renamed: the history goes with it.
        let new = dir.join("new.ged");
        let s = Store::for_file(&new, Some(&id));
        assert_eq!(s.versions().unwrap().len(), 1);
        assert!(!folder_of(&old).exists());
        // A copy of a file that still exists starts its own history.
        std::fs::write(&new, "").unwrap();
        assert!(Store::for_file(&dir.join("copy.ged"), Some(&id)).versions().unwrap().is_empty());
        std::fs::remove_dir_all(dir).unwrap();
    }
}
