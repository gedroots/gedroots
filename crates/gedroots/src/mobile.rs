//! Mobile devices: the open tree, shared one way with the user's phones and tablets.
//!
//! When sharing is on for a file (File › Mobile Devices…), the app serves its last saved version,
//! media included, to the devices paired with it, peer to peer (`gedroots-sync`, over iroh): the
//! GEDRoots app on the device fetches what changed whenever it can reach the computer, directly or
//! through a relay, end-to-end encrypted. Every save publishes a new version.
//!
//! - the sync server runs on its own thread (tokio runtime), started the first time it is needed;
//! - a device pairs by scanning a single-use QR code that expires after 10 minutes, and is known
//!   from then on by its own key: removing it cuts it off at once;
//! - pairings are per file and belong to the machine: they live in the platform's configuration
//!   folder (`<config>/GEDRoots/mobile/`), never in the `.ged`, so they never leave with an export.

use gedroots_sync::{DeviceKind, Event, Server};
use gpui_kit::*;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// How long a pairing code stays valid.
pub const CODE_VALIDITY: Duration = Duration::from_secs(600);

/// Kind of device, for its icon and label.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Phone,
    Tablet,
}

/// A paired device.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Device {
    /// Its endpoint key.
    pub id: String,
    pub name: String,
    pub kind: Kind,
    /// When it was paired (seconds since 1970).
    pub paired: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_sync: Option<u64>,
    /// Was the last connection direct (not through a relay)?
    #[serde(default)]
    pub direct: bool,
    /// The version it got at its last sync.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<Version>,
}

/// Sharing of one file.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Share {
    pub enabled: bool,
    #[serde(default)]
    pub devices: Vec<Device>,
}

/// A published version of a tree.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Version {
    /// When it was published (seconds since 1970).
    pub at: u64,
    pub people: usize,
    /// Dataset and media.
    pub bytes: u64,
    /// Identifies the content (a version published again unchanged has the same).
    #[serde(default)]
    pub hash: String,
}

impl Version {
    /// Same content as `other`?
    pub fn same(&self, other: &Version) -> bool {
        !self.hash.is_empty() && self.hash == other.hash
    }
}

/// Saved pairings, by file (canonical path of the `.ged`).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
struct Saved {
    #[serde(default)]
    shares: BTreeMap<String, Share>,
}

/// State of the sync server.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Status {
    Off,
    Running,
    Failed(String),
}

/// What the app asks of the server's thread.
enum Cmd {
    Publish {
        share: String,
        dataset: String,
        files: Vec<(String, PathBuf)>,
        people: usize,
    },
    Unpublish(String),
    Revoke(String),
    PairCode {
        share: String,
        reply: async_channel::Sender<String>,
    },
    CancelPairing(String),
    /// A device's pairing moved to another key.
    Allow {
        device: String,
        share: String,
    },
}

/// What the server's thread tells the app's loop.
enum Msg {
    Sync(Event),
    Changed,
}

static SAVED: Mutex<Option<Saved>> = Mutex::new(None);
static STATUS: Mutex<Status> = Mutex::new(Status::Off);
static COMMANDS: Mutex<Option<tokio::sync::mpsc::UnboundedSender<Cmd>>> = Mutex::new(None);
/// Queue to the app's loop, created at startup.
static BRIDGE: Mutex<Option<async_channel::Sender<Msg>>> = Mutex::new(None);
/// Version served for each share since launch, or why publishing failed.
static PUBLISHED: Mutex<Option<HashMap<String, Result<Version, String>>>> = Mutex::new(None);

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

pub fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

/// Key of a tree's sharing: its identifier (`HEAD._GR_TREE`), so that renaming or moving the
/// file keeps its devices; its canonical path while it has none (never saved by GEDRoots).
/// Pairings saved under the path move to the identifier the first time it is known.
pub fn share_key(path: &Path, tree: Option<&str>) -> String {
    let by_path = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf()).to_string_lossy().into_owned();
    let Some(id) = tree else { return by_path };
    let key = format!("tree:{id}");
    let moves = lock_saved(|s| !s.shares.contains_key(&key) && s.shares.contains_key(&by_path));
    if moves {
        let mut devices = Vec::new();
        update(|s| {
            if let Some(share) = s.shares.remove(&by_path) {
                devices = share.devices.iter().map(|d| d.id.clone()).collect();
                s.shares.insert(key.clone(), share);
            }
        });
        if lock(&COMMANDS).is_some() {
            for device in devices {
                send(Cmd::Allow { device, share: key.clone() });
            }
        }
    }
    key
}

/// Connects the server's events to the app's loop.
pub fn install(cx: &mut App) {
    let (tx, rx) = async_channel::unbounded::<Msg>();
    *lock(&BRIDGE) = Some(tx);
    cx.spawn(async move |cx| {
        while let Ok(msg) = rx.recv().await {
            cx.update(|cx| {
                if let Msg::Sync(event) = msg {
                    on_event(event, cx);
                }
                cx.refresh_windows();
            });
        }
    })
    .detach();
}

fn on_event(event: Event, cx: &mut App) {
    match event {
        Event::Paired { device, share, name, kind } => {
            let kind = match kind {
                DeviceKind::Phone => Kind::Phone,
                DeviceKind::Tablet => Kind::Tablet,
            };
            let name = if name.is_empty() { rust_i18n::t!("mobile.unnamed").into_owned() } else { name };
            update(|s| {
                let devices = &mut s.shares.entry(share.clone()).or_default().devices;
                let paired = Device {
                    id: device.clone(),
                    name: name.clone(),
                    kind,
                    paired: now(),
                    last_sync: None,
                    direct: false,
                    version: None,
                };
                // Paired again: same place in the list.
                match devices.iter_mut().find(|d| d.id == device) {
                    Some(d) => *d = paired,
                    None => devices.push(paired),
                }
            });
            for ws in cx.global::<crate::updates::Windows>().0.clone() {
                if let Some(ws) = ws.upgrade() {
                    ws.update(cx, |ws, cx| ws.mobile_paired(&share, &device, &name, cx));
                }
            }
        }
        Event::Synced { device, direct } => {
            let published = lock(&PUBLISHED).clone().unwrap_or_default();
            update(|s| {
                for (key, share) in s.shares.iter_mut() {
                    if let Some(d) = share.devices.iter_mut().find(|d| d.id == device) {
                        d.last_sync = Some(now());
                        d.direct = direct;
                        if let Some(Ok(v)) = published.get(key) {
                            d.version = Some(v.clone());
                        }
                    }
                }
            });
        }
    }
}

// ----------------------------------------------------------------------
// Sharing of a file
// ----------------------------------------------------------------------

pub fn share(key: &str) -> Share {
    lock_saved(|s| s.shares.get(key).cloned().unwrap_or_default())
}

pub fn status() -> Status {
    lock(&STATUS).clone()
}

/// The version served for `share` since launch, or why publishing it failed.
pub fn published(share: &str) -> Option<Result<Version, String>> {
    lock(&PUBLISHED).as_ref()?.get(share).cloned()
}

/// Turns sharing of a file on or off, and saves the choice. Turning it on is followed by a
/// [`publish`]; turning it off stops serving the file but keeps its devices.
pub fn set_enabled(share: &str, enabled: bool) {
    update(|s| s.shares.entry(share.to_string()).or_default().enabled = enabled);
    if !enabled {
        unpublish(share);
    }
}

/// Serves a new version of `share`: its dataset (as rewritten by `gedcom::gedzip::pack`) and the
/// media files (name in the version → file on disk).
pub fn publish(share: &str, dataset: String, files: Vec<(String, PathBuf)>, people: usize) {
    send(Cmd::Publish { share: share.to_string(), dataset, files, people });
}

/// Stops serving `share` (file closed, sharing turned off).
pub fn unpublish(share: &str) {
    if let Some(p) = lock(&PUBLISHED).as_mut() {
        p.remove(share);
    }
    // Nothing to stop if the server never started.
    if lock(&COMMANDS).is_some() {
        send(Cmd::Unpublish(share.to_string()));
    }
}

/// A new pairing code for `share` (the text of the QR code); the channel closes if the server is
/// not running.
pub fn pair_code(share: &str) -> async_channel::Receiver<String> {
    let (reply, rx) = async_channel::bounded(1);
    send(Cmd::PairCode { share: share.to_string(), reply });
    rx
}

/// Forgets the pairing code of `share` (window closed).
pub fn cancel_pairing(share: &str) {
    if lock(&COMMANDS).is_some() {
        send(Cmd::CancelPairing(share.to_string()));
    }
}

/// Removes a device: its next request is refused.
pub fn remove(share: &str, device: &str) {
    update(|s| {
        if let Some(sh) = s.shares.get_mut(share) {
            sh.devices.retain(|d| d.id != device);
        }
    });
    send(Cmd::Revoke(device.to_string()));
}

// ----------------------------------------------------------------------
// Server thread
// ----------------------------------------------------------------------

fn data_dir() -> Option<PathBuf> {
    Some(dirs::config_dir()?.join("GEDRoots").join("mobile"))
}

fn send(cmd: Cmd) {
    let mut commands = lock(&COMMANDS);
    if commands.is_none() {
        *commands = start();
    }
    if let Some(tx) = commands.as_ref()
        && tx.send(cmd).is_err()
    {
        *commands = None;
    }
}

fn notify(msg: Msg) {
    if let Some(tx) = lock(&BRIDGE).as_ref() {
        let _ = tx.try_send(msg);
    }
}

fn set_status(s: Status) {
    *lock(&STATUS) = s;
    notify(Msg::Changed);
}

fn start() -> Option<tokio::sync::mpsc::UnboundedSender<Cmd>> {
    let Some(dir) = data_dir() else {
        set_status(Status::Failed(rust_i18n::t!("mobile.no_config").into_owned()));
        return None;
    };
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<Cmd>();
    // Every known device, whether its file is shared right now or not: the server answers
    // “not shared” to the latter.
    let known: Vec<(String, String)> = lock_saved(|s| {
        s.shares.iter().flat_map(|(k, sh)| sh.devices.iter().map(move |d| (d.id.clone(), k.clone()))).collect()
    });
    let spawned = std::thread::Builder::new().name("gedroots-mobile".into()).spawn(move || {
        let rt = match tokio::runtime::Builder::new_multi_thread().worker_threads(2).enable_all().build() {
            Ok(rt) => rt,
            Err(e) => return set_status(Status::Failed(e.to_string())),
        };
        rt.block_on(async move {
            let server = match Server::start(&dir, |e| notify(Msg::Sync(e))).await {
                Ok(s) => s,
                Err(e) => return set_status(Status::Failed(e.to_string())),
            };
            for (device, share) in known {
                let _ = server.allow(&device, &share);
            }
            set_status(Status::Running);
            while let Some(cmd) = rx.recv().await {
                match cmd {
                    Cmd::Publish { share, dataset, files, people } => {
                        let res = server
                            .publish(&share, dataset.into_bytes(), files)
                            .await
                            .map(|p| Version { at: now(), people, bytes: p.bytes, hash: p.hash })
                            .map_err(|e| e.to_string());
                        lock(&PUBLISHED).get_or_insert_default().insert(share, res);
                        notify(Msg::Changed);
                    }
                    Cmd::Unpublish(share) => server.unpublish(&share).await,
                    Cmd::Revoke(device) => server.revoke(&device),
                    Cmd::PairCode { share, reply } => {
                        let code = server.pair_code(&share, CODE_VALIDITY).await;
                        let _ = reply.send(code.encode()).await;
                    }
                    Cmd::CancelPairing(share) => server.cancel_pairing(&share),
                    Cmd::Allow { device, share } => {
                        let _ = server.allow(&device, &share);
                    }
                }
            }
            let _ = server.shutdown().await;
        });
    });
    match spawned {
        Ok(_) => Some(tx),
        Err(e) => {
            set_status(Status::Failed(e.to_string()));
            None
        }
    }
}

// ----------------------------------------------------------------------
// Pairings: `<config>/GEDRoots/mobile/devices.json`
// ----------------------------------------------------------------------

fn saved_path() -> Option<PathBuf> {
    Some(data_dir()?.join("devices.json"))
}

fn lock_saved<T>(f: impl FnOnce(&Saved) -> T) -> T {
    let mut saved = lock(&SAVED);
    let s = saved.get_or_insert_with(|| {
        saved_path().and_then(|p| std::fs::read_to_string(p).ok()).map(|t| parse(&t)).unwrap_or_default()
    });
    f(s)
}

fn update(f: impl FnOnce(&mut Saved)) {
    let mut s = lock_saved(Saved::clone);
    f(&mut s);
    // A file without sharing nor devices leaves no trace.
    s.shares.retain(|_, sh| sh.enabled || !sh.devices.is_empty());
    if let Some(path) = saved_path() {
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if let Ok(text) = serde_json::to_string_pretty(&s) {
            let _ = std::fs::write(path, text);
        }
    }
    *lock(&SAVED) = Some(s);
}

fn parse(text: &str) -> Saved {
    serde_json::from_str(text).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::{Device, Kind, Saved, Share, Version, parse};

    #[test]
    fn saved_round_trip() {
        let mut s = Saved::default();
        s.shares.insert(
            "/trees/a.ged".into(),
            Share {
                enabled: true,
                devices: vec![Device {
                    id: "abc".into(),
                    name: "Phone".into(),
                    kind: Kind::Tablet,
                    paired: 10,
                    last_sync: Some(20),
                    direct: true,
                    version: Some(Version { at: 15, people: 3, bytes: 99, hash: "h".into() }),
                }],
            },
        );
        let text = serde_json::to_string(&s).unwrap();
        assert!(text.contains("\"tablet\""));
        assert_eq!(parse(&text), s);
        assert_eq!(parse("not json"), Saved::default());
        assert_eq!(parse(r#"{"shares":{"/x.ged":{"enabled":false}}}"#).shares["/x.ged"].devices, vec![]);
    }
}
