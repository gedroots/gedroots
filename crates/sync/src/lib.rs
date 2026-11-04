//! One-way sync of a tree from the desktop to the user's mobile devices, peer to peer (iroh).
//!
//! The desktop ([`Server`]) serves each shared tree as an iroh-blobs *collection*: `gedcom.ged`
//! (rewritten like a GEDZIP archive, see `gedcom::gedzip::pack`) plus every local media file, each
//! one a content-addressed blob. A device ([`Receiver`]) keeps its own blob store, so a later sync
//! only downloads the blobs it does not have yet: after an edit, the new dataset and new media.
//!
//! Pairing: the desktop shows a [`PairCode`] as a QR code, its address plus a random one-time token
//! that expires. The device presents the token on [`PAIR_ALPN`] with its name; the desktop then
//! knows the device by its endpoint key (iroh authenticates both ends) and the token is spent.
//! Every later request is accepted only from a known device, and removing it cuts it off at once.
//!
//! The desktop's endpoint key is kept between launches, so a paired device finds it again (iroh
//! publishes the endpoint's current addresses; a relay is used when no direct path exists). What
//! the desktop must remember about devices is the app's business: the server reports pairings and
//! syncs as [`Event`]s, and is told which devices to accept with [`Server::allow`].

use anyhow::{Context, Result, anyhow, bail};
use iroh::endpoint::{Connection, presets};
use iroh::protocol::{AcceptError, ProtocolHandler, Router};
use iroh::{Endpoint, EndpointAddr, EndpointId, SecretKey};
use iroh_blobs::api::blobs::{AddPathOptions, ImportMode};
use iroh_blobs::api::remote::GetProgressItem;
use iroh_blobs::format::collection::Collection;
use iroh_blobs::store::GcConfig;
use iroh_blobs::store::fs::FsStore;
use iroh_blobs::store::fs::options::Options;
use iroh_blobs::{BlobFormat, BlobsProtocol, Hash, HashAndFormat};
use iroh_tickets::endpoint::EndpointTicket;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime};

/// Pairing: a device presents a one-time token and its name.
pub const PAIR_ALPN: &[u8] = b"gedroots/pair/1";
/// The hash of the latest version of the tree a device is paired with.
pub const LATEST_ALPN: &[u8] = b"gedroots/latest/1";

/// Prefix of a pairing code.
const CODE_PREFIX: &str = "gedroots:";
/// Longest device name accepted.
const NAME_MAX: usize = 64;

/// Answers on [`LATEST_ALPN`] and [`PAIR_ALPN`] (first byte).
const OK: u8 = 0;
const UNKNOWN_DEVICE: u8 = 1;
const NOT_SHARED: u8 = 2;
const BAD_TOKEN: u8 = 3;

/// What a device needs to pair with a desktop: its address and a one-time token.
#[derive(Debug, Clone)]
pub struct PairCode {
    pub addr: EndpointAddr,
    pub token: [u8; 16],
}

impl PairCode {
    /// `gedroots:<endpoint ticket>.<token>`, short enough for a QR code.
    pub fn encode(&self) -> String {
        let ticket = EndpointTicket::new(self.addr.clone());
        let token = data_encoding::BASE32_NOPAD.encode(&self.token).to_ascii_lowercase();
        format!("{CODE_PREFIX}{ticket}.{token}")
    }

    pub fn decode(code: &str) -> Result<Self> {
        let rest = code.trim().strip_prefix(CODE_PREFIX).ok_or_else(|| anyhow!("not a GEDRoots pairing code"))?;
        let (ticket, token) = rest.rsplit_once('.').ok_or_else(|| anyhow!("pairing code without token"))?;
        let ticket: EndpointTicket = ticket.parse().context("pairing code: endpoint")?;
        let token = data_encoding::BASE32_NOPAD.decode(token.to_ascii_uppercase().as_bytes())?;
        let token: [u8; 16] = token.try_into().map_err(|_| anyhow!("pairing code: bad token"))?;
        Ok(Self { addr: ticket.into(), token })
    }
}

/// Kind of device, for its icon.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeviceKind {
    Phone,
    Tablet,
}

impl DeviceKind {
    fn byte(self) -> u8 {
        match self {
            Self::Phone => 0,
            Self::Tablet => 1,
        }
    }

    fn from_byte(b: u8) -> Self {
        if b == 1 { Self::Tablet } else { Self::Phone }
    }
}

/// What the server reports to the app.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    /// A device presented a valid token: it now belongs to `share`.
    Paired { device: String, share: String, name: String, kind: DeviceKind },
    /// A paired device asked for the latest version.
    Synced { device: String, direct: bool },
}

type OnEvent = Arc<dyn Fn(Event) + Send + Sync>;

#[derive(Debug, Clone, Copy)]
struct Latest {
    hash: Hash,
    size: u64,
}

struct Pending {
    token: [u8; 16],
    share: String,
    expires: Instant,
}

#[derive(Default)]
struct State {
    pending: Vec<Pending>,
    /// Device → the share it belongs to.
    devices: HashMap<EndpointId, String>,
    latest: HashMap<String, Latest>,
}

type Shared = Arc<Mutex<State>>;

fn lock(s: &Shared) -> std::sync::MutexGuard<'_, State> {
    s.lock().unwrap_or_else(|e| e.into_inner())
}

#[derive(Clone)]
struct PairProtocol {
    state: Shared,
    on_event: OnEvent,
}

impl std::fmt::Debug for PairProtocol {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("PairProtocol")
    }
}

impl ProtocolHandler for PairProtocol {
    async fn accept(&self, conn: Connection) -> Result<(), AcceptError> {
        let (mut send, mut recv) = conn.accept_bi().await?;
        let asked = recv.read_to_end(16 + 1 + NAME_MAX * 4).await.map_err(AcceptError::from_err)?;
        let answer = match parse_hello(&asked) {
            Some((token, kind, name)) => {
                let mut st = lock(&self.state);
                let now = Instant::now();
                st.pending.retain(|p| p.expires > now);
                match st.pending.iter().position(|p| p.token == token) {
                    Some(i) => {
                        let share = st.pending.remove(i).share;
                        st.devices.insert(conn.remote_id(), share.clone());
                        drop(st);
                        (self.on_event)(Event::Paired { device: conn.remote_id().to_string(), share, name, kind });
                        OK
                    }
                    None => BAD_TOKEN,
                }
            }
            None => BAD_TOKEN,
        };
        send.write_all(&[answer]).await.map_err(AcceptError::from_err)?;
        send.finish().map_err(AcceptError::from_err)?;
        conn.closed().await;
        Ok(())
    }
}

/// `token (16) · kind (1) · name (UTF-8)`.
fn parse_hello(b: &[u8]) -> Option<([u8; 16], DeviceKind, String)> {
    if b.len() < 17 {
        return None;
    }
    let token: [u8; 16] = b[..16].try_into().ok()?;
    let name: String = String::from_utf8_lossy(&b[17..]).chars().filter(|c| !c.is_control()).take(NAME_MAX).collect();
    Some((token, DeviceKind::from_byte(b[16]), name.trim().to_string()))
}

#[derive(Clone)]
struct LatestProtocol {
    state: Shared,
    on_event: OnEvent,
}

impl std::fmt::Debug for LatestProtocol {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("LatestProtocol")
    }
}

impl ProtocolHandler for LatestProtocol {
    async fn accept(&self, conn: Connection) -> Result<(), AcceptError> {
        let (mut send, mut recv) = conn.accept_bi().await?;
        let _ = recv.read_to_end(0).await;
        let device = conn.remote_id();
        let answer = {
            let st = lock(&self.state);
            match st.devices.get(&device) {
                None => vec![UNKNOWN_DEVICE],
                Some(share) => match st.latest.get(share) {
                    None => vec![NOT_SHARED],
                    Some(l) => {
                        let mut a = vec![OK];
                        a.extend_from_slice(l.hash.as_bytes());
                        a.extend_from_slice(&l.size.to_be_bytes());
                        a
                    }
                },
            }
        };
        let known = answer[0] != UNKNOWN_DEVICE;
        send.write_all(&answer).await.map_err(AcceptError::from_err)?;
        send.finish().map_err(AcceptError::from_err)?;
        if known {
            (self.on_event)(Event::Synced { device: device.to_string(), direct: is_direct(&conn) });
        }
        conn.closed().await;
        Ok(())
    }
}

/// The blobs protocol, for known devices only.
#[derive(Debug, Clone)]
struct GatedBlobs {
    inner: BlobsProtocol,
    state: Shared,
}

impl ProtocolHandler for GatedBlobs {
    async fn accept(&self, conn: Connection) -> Result<(), AcceptError> {
        if !lock(&self.state).devices.contains_key(&conn.remote_id()) {
            conn.close(1u32.into(), b"unknown device");
            return Ok(());
        }
        self.inner.accept(conn).await
    }

    async fn shutdown(&self) {
        self.inner.shutdown().await
    }
}

impl std::fmt::Debug for State {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("State")
    }
}

/// Summary of a published version.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Published {
    pub files: usize,
    pub bytes: u64,
    /// Identifies the content: the same tree and media give the same hash.
    pub hash: String,
}

/// Desktop side: serves the shared trees to their paired devices.
pub struct Server {
    router: Router,
    store: FsStore,
    state: Shared,
    /// File → (modification time, length, hash): media are hashed once.
    hashes: Mutex<HashMap<PathBuf, (SystemTime, u64, Hash)>>,
}

impl Server {
    /// Starts the endpoint. `dir` keeps its key (stable address) and the blob store.
    pub async fn start(dir: &Path, on_event: impl Fn(Event) + Send + Sync + 'static) -> Result<Self> {
        std::fs::create_dir_all(dir)?;
        let key = SecretKey::from_bytes(&load_or_create_key(&dir.join("endpoint.key"))?);
        let endpoint = Endpoint::builder(presets::N0).secret_key(key).bind().await?;
        let blobs = dir.join("blobs");
        let mut options = Options::new(&blobs);
        options.gc = Some(GcConfig { interval: Duration::from_secs(600), add_protected: None });
        let store = FsStore::load_with_opts(blobs.join("blobs.db"), options).await?;
        let state: Shared = Arc::default();
        let on_event: OnEvent = Arc::new(on_event);
        let router = Router::builder(endpoint)
            .accept(iroh_blobs::ALPN, GatedBlobs { inner: BlobsProtocol::new(&store, None), state: state.clone() })
            .accept(PAIR_ALPN, PairProtocol { state: state.clone(), on_event: on_event.clone() })
            .accept(LATEST_ALPN, LatestProtocol { state: state.clone(), on_event })
            .spawn();
        Ok(Self { router, store, state, hashes: Mutex::default() })
    }

    /// Accepts `device` (as reported by [`Event::Paired`]) for `share`.
    pub fn allow(&self, device: &str, share: &str) -> Result<()> {
        let id: EndpointId = device.parse().map_err(|_| anyhow!("bad device id"))?;
        lock(&self.state).devices.insert(id, share.to_string());
        Ok(())
    }

    /// Forgets `device`: its next request is refused.
    pub fn revoke(&self, device: &str) {
        if let Ok(id) = device.parse::<EndpointId>() {
            lock(&self.state).devices.remove(&id);
        }
    }

    /// A new pairing code for `share`, valid once for `valid`. Waits (a few seconds at most) for
    /// the endpoint to be reachable, so that the code carries its relay address.
    pub async fn pair_code(&self, share: &str, valid: Duration) -> PairCode {
        let endpoint = self.router.endpoint();
        let _ = tokio::time::timeout(Duration::from_secs(5), endpoint.online()).await;
        let token: [u8; 16] = SecretKey::generate().to_bytes()[..16].try_into().unwrap();
        let mut st = lock(&self.state);
        let now = Instant::now();
        st.pending.retain(|p| p.expires > now && p.share != share);
        st.pending.push(Pending { token, share: share.to_string(), expires: now + valid });
        PairCode { addr: endpoint.addr(), token }
    }

    /// Forgets the pending pairing codes of `share`.
    pub fn cancel_pairing(&self, share: &str) {
        lock(&self.state).pending.retain(|p| p.share != share);
    }

    /// Publishes a version of `share`: the dataset and the files of its media (name in the
    /// collection → file on disk), as prepared by `gedcom::gedzip::pack`. Media are referenced in
    /// place, not copied, and only hashed again when they change.
    pub async fn publish(&self, share: &str, dataset: Vec<u8>, files: Vec<(String, PathBuf)>) -> Result<Published> {
        // Temporary tags of the batch keep every blob of this version from the garbage collector
        // until the named tag below protects the whole collection.
        let batch = self.store.blobs().batch().await?;
        let mut bytes = dataset.len() as u64;
        let tag = batch.add_bytes(dataset).temp_tag().await?;
        let mut items = vec![(gedcom::gedzip::DATASET.to_string(), tag.hash())];
        let mut guards = vec![tag];
        for (name, path) in &files {
            let path = std::path::absolute(path)?;
            let meta = std::fs::metadata(&path).with_context(|| path.display().to_string())?;
            bytes += meta.len();
            let stamp = meta.modified().unwrap_or(SystemTime::UNIX_EPOCH);
            let known = self.hashes.lock().unwrap_or_else(|e| e.into_inner()).get(&path).copied();
            let mut hash = None;
            if let Some((t, len, h)) = known
                && t == stamp
                && len == meta.len()
            {
                let guard = batch.temp_tag(HashAndFormat::raw(h)).await?;
                if self.has(h).await {
                    guards.push(guard);
                    hash = Some(h);
                }
            }
            let hash = match hash {
                Some(h) => h,
                None => {
                    let opts =
                        AddPathOptions { path: path.clone(), format: BlobFormat::Raw, mode: ImportMode::TryReference };
                    let tag = batch.add_path_with_opts(opts).temp_tag().await?;
                    let h = tag.hash();
                    guards.push(tag);
                    self.hashes.lock().unwrap_or_else(|e| e.into_inner()).insert(path.clone(), (stamp, meta.len(), h));
                    h
                }
            };
            items.push((name.clone(), hash));
        }
        let collection: Collection = items.into_iter().collect();
        let tt = collection.store(&self.store).await?;
        // A named tag keeps this version, and drops the previous one.
        self.store.tags().set(format!("share/{share}"), tt.hash_and_format()).await?;
        drop(guards);
        lock(&self.state).latest.insert(share.to_string(), Latest { hash: tt.hash(), size: bytes });
        Ok(Published { files: files.len(), bytes, hash: tt.hash().to_string() })
    }

    async fn has(&self, hash: Hash) -> bool {
        self.store.remote().local(HashAndFormat::raw(hash)).await.is_ok_and(|l| l.is_complete())
    }

    /// Stops serving `share` (sharing turned off, file closed); its blobs are collected later.
    pub async fn unpublish(&self, share: &str) {
        {
            let mut st = lock(&self.state);
            st.latest.remove(share);
            st.pending.retain(|p| p.share != share);
        }
        let _ = self.store.tags().delete(format!("share/{share}")).await;
    }

    pub async fn shutdown(self) -> Result<()> {
        self.router.shutdown().await?;
        Ok(())
    }
}

/// 32 random bytes kept in `path`, created on first use.
fn load_or_create_key(path: &Path) -> Result<[u8; 32]> {
    if let Ok(bytes) = std::fs::read(path)
        && let Ok(key) = <[u8; 32]>::try_from(bytes.as_slice())
    {
        return Ok(key);
    }
    let key = SecretKey::generate().to_bytes();
    std::fs::write(path, key)?;
    Ok(key)
}

fn is_direct(conn: &Connection) -> bool {
    conn.paths().iter().any(|p| p.is_selected() && p.is_ip())
}

/// Device side: its key, a blob store and the folder where the tree is exported.
pub struct Receiver {
    endpoint: Endpoint,
    store: FsStore,
    tree: PathBuf,
}

/// What a sync did.
#[derive(Debug, Clone)]
pub struct SyncReport {
    /// Size of the whole tree (dataset and media).
    pub total_bytes: u64,
    /// Bytes actually downloaded this time.
    pub downloaded_bytes: u64,
    pub files: u64,
    pub elapsed: Duration,
    /// Was the connection direct (not through a relay) when the sync ended?
    pub direct: bool,
    /// Folder of the received tree: `gedcom.ged` and the media, laid out like a GEDZIP archive.
    pub tree: PathBuf,
}

/// Why a sync did not happen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refused {
    /// The desktop does not know this device (removed, or another computer).
    UnknownDevice,
    /// Sharing is off, or the file is not open on the desktop.
    NotShared,
}

impl std::fmt::Display for Refused {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::UnknownDevice => "this device is no longer paired",
            Self::NotShared => "the tree is not shared right now",
        })
    }
}

impl std::error::Error for Refused {}

impl Receiver {
    /// Opens the device's state in `dir`; its key is kept there, so the desktop recognises it.
    pub async fn open(dir: &Path) -> Result<Self> {
        std::fs::create_dir_all(dir)?;
        let key = SecretKey::from_bytes(&load_or_create_key(&dir.join("endpoint.key"))?);
        let endpoint = Endpoint::builder(presets::N0).secret_key(key).bind().await?;
        let store = FsStore::load(dir.join("blobs")).await?;
        Ok(Self { endpoint, store, tree: dir.join("tree") })
    }

    /// Pairs with the desktop of `code`. Returns the desktop's address, to keep for [`Self::sync`].
    pub async fn pair(&self, code: &PairCode, name: &str, kind: DeviceKind) -> Result<String> {
        let conn = self.endpoint.connect(code.addr.clone(), PAIR_ALPN).await?;
        let (mut send, mut recv) = conn.open_bi().await?;
        let mut hello = code.token.to_vec();
        hello.push(kind.byte());
        hello.extend(name.chars().take(NAME_MAX).collect::<String>().bytes());
        send.write_all(&hello).await?;
        send.finish()?;
        let answer = recv.read_to_end(8).await?;
        conn.close(0u32.into(), b"");
        match answer.first() {
            Some(&OK) => Ok(EndpointTicket::new(code.addr.clone()).to_string()),
            _ => bail!("this pairing code was already used or has expired"),
        }
    }

    /// Asks the desktop for its latest version and downloads what is missing.
    pub async fn sync(&self, desktop: &str) -> Result<SyncReport> {
        self.sync_with(desktop, |_, _| {}).await
    }

    /// [`Self::sync`], telling `progress` the bytes of the tree already here and its whole size
    /// as they arrive.
    pub async fn sync_with(&self, desktop: &str, progress: impl Fn(u64, u64)) -> Result<SyncReport> {
        let addr: EndpointAddr = desktop.parse::<EndpointTicket>().context("desktop address")?.into();
        let start = Instant::now();
        let conn = self.endpoint.connect(addr.clone(), LATEST_ALPN).await?;
        let (mut send, mut recv) = conn.open_bi().await?;
        send.finish()?;
        let answer = recv.read_to_end(64).await?;
        conn.close(0u32.into(), b"");
        match answer.first() {
            Some(&OK) if answer.len() == 41 => {}
            Some(&UNKNOWN_DEVICE) => return Err(Refused::UnknownDevice.into()),
            Some(&NOT_SHARED) => return Err(Refused::NotShared.into()),
            _ => bail!("unexpected answer from the desktop"),
        }
        let hash = Hash::from_bytes(answer[1..33].try_into().unwrap());
        let total_bytes = u64::from_be_bytes(answer[33..].try_into().unwrap());

        let conn = self.endpoint.connect(addr, iroh_blobs::ALPN).await?;
        // The list of children first: without it, the store cannot tell which children it already
        // has, and the whole collection would be requested again.
        let list = self.store.remote().fetch(conn.clone(), HashAndFormat::raw(hash)).await?;
        let here = self.store.remote().local(HashAndFormat::hash_seq(hash)).await?.local_bytes();
        progress(here.min(total_bytes), total_bytes);
        let mut items = self.store.remote().fetch(conn.clone(), HashAndFormat::hash_seq(hash)).stream();
        let stats = loop {
            use n0_future::StreamExt;
            match items.next().await {
                Some(GetProgressItem::Progress(read)) => progress((here + read).min(total_bytes), total_bytes),
                Some(GetProgressItem::Done(stats)) => break stats,
                Some(GetProgressItem::Error(e)) => return Err(e.into()),
                None => bail!("the download stopped"),
            }
        };
        progress(total_bytes, total_bytes);
        let direct = is_direct(&conn);
        conn.close(0u32.into(), b"");
        self.store.tags().set("latest", HashAndFormat::hash_seq(hash)).await?;

        let collection = Collection::load(hash, self.store.as_ref()).await?;
        let tmp = self.tree.with_extension("part");
        let _ = std::fs::remove_dir_all(&tmp);
        let mut files = 0;
        for (name, blob) in collection.iter() {
            let target = safe_join(&tmp, name)?;
            if let Some(parent) = target.parent() {
                std::fs::create_dir_all(parent)?;
            }
            self.store.blobs().export(*blob, &target).await?;
            files += 1;
        }
        let _ = std::fs::remove_dir_all(&self.tree);
        std::fs::rename(&tmp, &self.tree)?;
        Ok(SyncReport {
            total_bytes,
            downloaded_bytes: list.total_bytes_read() + stats.total_bytes_read(),
            files,
            elapsed: start.elapsed(),
            direct,
            tree: self.tree.clone(),
        })
    }

    pub async fn close(self) {
        self.endpoint.close().await;
    }
}

/// Joins an archive name to `root`, refusing names that leave it.
fn safe_join(root: &Path, name: &str) -> Result<PathBuf> {
    let mut path = root.to_path_buf();
    for part in name.split('/') {
        match part {
            "" | "." => {}
            ".." => bail!("unsafe name in the collection: {name}"),
            p if p.contains('\\') || p.contains(':') => bail!("unsafe name in the collection: {name}"),
            p => path.push(p),
        }
    }
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pair_code_round_trip() {
        let addr = EndpointAddr::new(SecretKey::generate().public());
        let code = PairCode { addr, token: [7; 16] };
        let back = PairCode::decode(&code.encode()).unwrap();
        assert_eq!(back.token, [7; 16]);
        assert_eq!(back.addr.id, code.addr.id);
        assert!(PairCode::decode("gedroots:nope").is_err());
        assert!(PairCode::decode("https://example.org").is_err());
    }

    #[test]
    fn hello_is_bounded_and_clean() {
        let mut b = vec![1u8; 16];
        b.push(1);
        b.extend("iPad\u{7} du salon".bytes());
        let (token, kind, name) = parse_hello(&b).unwrap();
        assert_eq!(token, [1; 16]);
        assert_eq!(kind, DeviceKind::Tablet);
        assert_eq!(name, "iPad du salon");
        assert!(parse_hello(&[0; 10]).is_none());
    }

    #[test]
    fn safe_join_refuses_escapes() {
        let root = Path::new("/r");
        assert_eq!(safe_join(root, "Records/a.jpg").unwrap(), root.join("Records").join("a.jpg"));
        assert!(safe_join(root, "../x").is_err());
        assert!(safe_join(root, "C:\\x").is_err());
    }
}
