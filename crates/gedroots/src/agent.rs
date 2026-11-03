//! AI agents: GEDRoots' local MCP server.
//!
//! When the setting is on (Source › Connect an AI Agent…), the app serves the MCP protocol over HTTP
//! on `127.0.0.1`: an agent such as Claude Code reads the open file (records, search, records to
//! find) without ever changing the tree: the records it finds become proposals, reviewed in the
//! “To Review” page before being saved. Only its research log (what it planned and searched, even in
//! vain) is written at once, with an undo point. Living people are hidden.
//!
//! - the server runs on its own thread (tokio runtime);
//! - each tool call asks the app's loop for a copy of the displayed document (a cheap copy: records
//!   are shared);
//! - a token is required in the `Authorization` header, and requests coming from a web page
//!   (`Origin` header) are refused.
//!
//! The setting and the token belong to the machine: they live in the platform's configuration
//! folder, never in the `.ged`.

pub mod clients;
mod prompts;
pub mod propose;
mod tools;

use gedcom::Gedcom;
use gpui_kit::*;
use std::hash::{BuildHasher, Hasher};
use std::path::PathBuf;
use std::sync::Mutex;

/// Listening port, fixed so that the command copied into the agent stays valid.
pub const PORT: u16 = 47_280;

/// Address of the MCP server.
pub fn url() -> String {
    format!("http://127.0.0.1:{PORT}/mcp")
}

/// What the app asks of the displayed document, from the server's thread.
pub enum Request {
    Snapshot(tokio::sync::oneshot::Sender<Option<Snapshot>>),
    /// Record proposal: added to the window's “To Review” page, or refused.
    Propose(Box<propose::Pending>, tokio::sync::oneshot::Sender<Result<usize, String>>),
    /// Change to the research log (plan, searches), never to the tree's data: applied at once,
    /// with an undo point, and announced in the top bar.
    Edit(Edit, tokio::sync::oneshot::Sender<Result<(), String>>),
    /// An agent has just opened a session: the “AI Agents” window updates.
    Connected,
}

/// A change applied to the displayed document; an error leaves it untouched.
pub type Apply = Box<dyn FnOnce(&mut Gedcom) -> Result<(), String> + Send>;

/// A change of the research log by an agent, and the message shown once it is applied.
pub struct Edit {
    pub apply: Apply,
    pub message: String,
}

/// Copy of the active window's document.
pub struct Snapshot {
    pub path: Option<PathBuf>,
    pub ged: Gedcom,
}

/// Window whose file the agent reads: the last one activated.
#[derive(Default)]
pub struct Target(pub Option<WeakEntity<crate::workspace::Workspace>>);

impl Global for Target {}

/// Saved setting.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Settings {
    pub enabled: bool,
    pub token: String,
    /// Agent chosen in the “AI Agents” window (`clients::Client::id`).
    pub client: String,
}

/// State of the server, for the connection window.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Status {
    Off,
    Running,
    /// The port is taken or the server stopped.
    Failed(String),
}

struct Running {
    cancel: tokio_util::sync::CancellationToken,
}

static SERVER: Mutex<Option<Running>> = Mutex::new(None);
static STATUS: Mutex<Status> = Mutex::new(Status::Off);
/// Queue to the app's loop, created at startup.
static BRIDGE: Mutex<Option<async_channel::Sender<Request>>> = Mutex::new(None);
/// Last agent connected since launch: name given by the client, and when.
static LAST: Mutex<Option<(String, std::time::Instant)>> = Mutex::new(None);

/// Last agent connected (`clientInfo` of the MCP session) and how long ago.
pub fn last_client() -> Option<(String, std::time::Duration)> {
    LAST.lock().unwrap_or_else(|e| e.into_inner()).as_ref().map(|(n, t)| (n.clone(), t.elapsed()))
}

/// Records the agent that opens a session (server thread).
pub(crate) fn note_client(name: &str) {
    *LAST.lock().unwrap_or_else(|e| e.into_inner()) = Some((name.to_string(), std::time::Instant::now()));
    if let Some(tx) = BRIDGE.lock().unwrap_or_else(|e| e.into_inner()).as_ref() {
        let _ = tx.try_send(Request::Connected);
    }
}

pub fn status() -> Status {
    STATUS.lock().unwrap_or_else(|e| e.into_inner()).clone()
}

fn set_status(s: Status) {
    *STATUS.lock().unwrap_or_else(|e| e.into_inner()) = s;
}

/// Connects the request queue to the app's loop and starts the server if it is enabled.
pub fn install(cx: &mut App) {
    cx.set_global(Target::default());
    let (tx, rx) = async_channel::unbounded::<Request>();
    *BRIDGE.lock().unwrap_or_else(|e| e.into_inner()) = Some(tx);
    cx.spawn(async move |cx| {
        while let Ok(req) = rx.recv().await {
            match req {
                Request::Snapshot(reply) => {
                    let snap = cx.update(|cx| {
                        let ws = cx.global::<Target>().0.as_ref()?.upgrade()?;
                        ws.read(cx).agent_snapshot()
                    });
                    let _ = reply.send(snap);
                }
                Request::Propose(pending, reply) => {
                    let res = cx.update(|cx| {
                        let ws = cx.global::<Target>().0.as_ref().and_then(|w| w.upgrade());
                        match ws {
                            Some(ws) => ws.update(cx, |ws, cx| ws.add_proposal(*pending, cx)),
                            None => Err("no file is open in GEDRoots".to_string()),
                        }
                    });
                    let _ = reply.send(res);
                }
                Request::Edit(edit, reply) => {
                    let res = cx.update(|cx| {
                        let ws = cx.global::<Target>().0.as_ref().and_then(|w| w.upgrade());
                        match ws {
                            Some(ws) => ws.update(cx, |ws, cx| ws.agent_edit(edit, cx)),
                            None => Err("no file is open in GEDRoots".to_string()),
                        }
                    });
                    let _ = reply.send(res);
                }
                Request::Connected => cx.update(|cx| cx.refresh_windows()),
            }
        }
    })
    .detach();
    let settings = load();
    if settings.enabled {
        start(&settings.token);
    }
}

/// Records the window the agent must read.
pub fn set_target(ws: WeakEntity<crate::workspace::Workspace>, cx: &mut App) {
    cx.set_global(Target(Some(ws)));
}

/// Remembers the agent chosen in the window.
pub fn set_client(client: clients::Client) {
    let mut s = load();
    s.client = client.id().to_string();
    save(&s);
}

/// Turns the server on or off, and saves the choice.
pub fn set_enabled(enabled: bool) -> Settings {
    let mut s = load();
    s.enabled = enabled;
    save(&s);
    if enabled {
        start(&s.token);
    } else {
        stop();
    }
    s
}

fn start(token: &str) {
    stop();
    let Some(bridge) = BRIDGE.lock().unwrap_or_else(|e| e.into_inner()).clone() else { return };
    let cancel = tokio_util::sync::CancellationToken::new();
    *SERVER.lock().unwrap_or_else(|e| e.into_inner()) = Some(Running { cancel: cancel.clone() });
    set_status(Status::Running);
    let token = token.to_string();
    let spawned = std::thread::Builder::new().name("gedroots-mcp".into()).spawn(move || {
        let rt = match tokio::runtime::Builder::new_multi_thread().worker_threads(2).enable_all().build() {
            Ok(rt) => rt,
            Err(e) => return set_status(Status::Failed(e.to_string())),
        };
        if let Err(e) = rt.block_on(tools::serve(bridge, token, PORT, cancel.clone())) {
            // Stop requested: not an error.
            if !cancel.is_cancelled() {
                set_status(Status::Failed(e));
            }
        }
    });
    if let Err(e) = spawned {
        set_status(Status::Failed(e.to_string()));
    }
}

fn stop() {
    if let Some(r) = SERVER.lock().unwrap_or_else(|e| e.into_inner()).take() {
        r.cancel.cancel();
    }
    set_status(Status::Off);
}

// ----------------------------------------------------------------------
// Setting: `<config>/GEDRoots/agents`, `key=value` lines
// ----------------------------------------------------------------------

fn settings_path() -> Option<PathBuf> {
    Some(dirs::config_dir()?.join("GEDRoots").join("agents"))
}

/// Saved setting; a token is created (and saved) if there is none.
pub fn load() -> Settings {
    let text = settings_path().and_then(|p| std::fs::read_to_string(p).ok()).unwrap_or_default();
    let mut s = parse(&text);
    if s.token.is_empty() {
        s.token = new_token();
        save(&s);
    }
    s
}

fn parse(text: &str) -> Settings {
    let mut s = Settings { enabled: false, token: String::new(), client: String::new() };
    for line in text.lines() {
        match line.split_once('=') {
            Some(("enabled", v)) => s.enabled = v.trim() == "1",
            Some(("token", v)) => s.token = v.trim().to_string(),
            Some(("client", v)) => s.client = v.trim().to_string(),
            _ => {}
        }
    }
    s
}

fn save(s: &Settings) {
    let Some(path) = settings_path() else { return };
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let _ = std::fs::write(&path, format!("enabled={}\ntoken={}\nclient={}\n", u8::from(s.enabled), s.token, s.client));
}

/// Random 128-bit token (the system's random keys, through `RandomState`).
fn new_token() -> String {
    (0..2)
        .map(|i| {
            let mut h = std::collections::hash_map::RandomState::new().build_hasher();
            h.write_u64(i);
            format!("{:016x}", h.finish())
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{Settings, new_token, parse};

    #[test]
    fn settings_round_trip() {
        let s = parse("enabled=1\ntoken=abc\nclient=cursor\n");
        assert_eq!(s, Settings { enabled: true, token: "abc".into(), client: "cursor".into() });
        assert!(!parse("").enabled);
        assert_eq!(new_token().len(), 32);
        assert_ne!(new_token(), new_token());
    }
}
