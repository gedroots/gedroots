//! AI agents: GEDRoots' local MCP server.
//!
//! When the setting is on (GEDRoots › Connect an AI Agent…), the app serves the MCP protocol over HTTP
//! on `127.0.0.1`: an agent such as Claude Code reads the open file (records, search, records to
//! find) without ever changing the tree: the records it finds become proposals, reviewed in the
//! “To Review” page before being saved. Only its research log (what it planned and searched, even in
//! vain) is written at once, with an undo point.
//!
//! - the server runs on its own thread (tokio runtime);
//! - each tool call asks the app's loop for a copy of the displayed document (a cheap copy: records
//!   are shared);
//! - a token is required in the `Authorization` header, and requests coming from a web page
//!   (`Origin` header) are refused.
//!
//! The setting and the token belong to the machine: they live in the platform's configuration
//! folder, never in the `.ged`.

pub mod book;
pub mod change;
pub mod clients;
mod prompts;
pub mod propose;
mod tools;
pub mod viz;

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

/// A change queued: its id, the proposals waiting, the issues it would create.
pub type ChangeQueued = (String, usize, Vec<String>);

/// What the app asks of the displayed document, from the server's thread.
pub enum Request {
    Snapshot(tokio::sync::oneshot::Sender<Option<Snapshot>>),
    /// Copy of the document with its Data Quality issues, as the Quality page computes them.
    Quality(tokio::sync::oneshot::Sender<Option<(Snapshot, Vec<gedcom::analysis::check::Issue>)>>),
    /// Change for a Data Quality issue: added to “To Review”, with its id, the number of proposals
    /// waiting and the issues it would create, or refused.
    ProposeChange(Box<change::ChangeRequest>, tokio::sync::oneshot::Sender<Result<ChangeQueued, String>>),
    /// Record proposal: added to the window's “To Review” page, or refused.
    Propose(Box<propose::Pending>, tokio::sync::oneshot::Sender<Result<propose::Queued, String>>),
    /// What the agent wrote for a chapter of a book: added to “To Review”, with its id and the
    /// number of proposals waiting.
    ProposeChapter(Box<book::ChapterRequest>, tokio::sync::oneshot::Sender<Result<(String, usize), String>>),
    /// The agent withdraws a waiting proposal: the number still waiting, or why it cannot.
    Withdraw(String, tokio::sync::oneshot::Sender<Result<usize, String>>),
    /// Change to the research log (plan, searches), never to the tree's data: applied at once,
    /// with an undo point, and announced in the top bar.
    Edit(Edit, tokio::sync::oneshot::Sender<Result<(), String>>),
    /// The trees open in GEDRoots, the one in front first.
    Trees(tokio::sync::oneshot::Sender<Vec<Tree>>),
    /// An agent has just opened a session: the “AI Agents” window updates.
    Connected,
    /// What an agent is doing, for the Research page (not kept: shown while it works).
    Activity(Activity),
}

/// A tool an agent just called, as the Research page tells it.
#[derive(Clone, Debug)]
pub struct Activity {
    /// The name the agent gave when it connected.
    pub agent: String,
    pub doing: Doing,
}

#[derive(Clone, Debug)]
pub enum Doing {
    /// Reads where the research stands.
    Resume,
    /// Reads a person's sheet (id).
    ReadsPerson(String),
    /// Searches the tree's sources.
    SearchesSources(String),
    /// Looks for who the people of an act are.
    MatchesPeople,
    /// Locates a place.
    LocatesPlace(String),
    /// Notes in a lead's log: its key (`owner|GOAL`), the kind of entry and its text.
    Notes {
        lead: String,
        kind: NoteKind,
        text: String,
    },
    /// Proposes an act, a correction or a chapter.
    ProposesAct,
    ProposesChange,
    ProposesChapter,
    /// Reads the Data Quality issues.
    ReadsQuality,
}

/// What an agent notes in a lead's log.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NoteKind {
    Plan,
    Search,
    Clue,
    Handoff,
}

/// A request for the tree named `tree` (as `list_trees` names it), or for the window in front.
pub struct Ask {
    pub tree: Option<String>,
    pub request: Request,
}

/// A tree open in a window, as `list_trees` shows it.
#[derive(Clone, Debug)]
pub struct Tree {
    /// Name shown in the window's title bar.
    pub name: String,
    /// File name, if the tree has one.
    pub file: Option<String>,
    /// `HEAD._GR_TREE`, once the tree was saved by GEDRoots.
    pub id: Option<String>,
    pub people: usize,
    pub families: usize,
    pub sources: usize,
    pub media: usize,
    /// Root person, start of the Sosa numbering.
    pub root: Option<gedcom::research::PersonRef>,
    /// The window in front: the tree used when a tool names none.
    pub in_front: bool,
}

impl Tree {
    /// Whether the agent means this tree by `name`: its id, its name or its file name, with or
    /// without the extension, ignoring case.
    pub fn is_named(&self, name: &str) -> bool {
        let name = name.trim().to_lowercase();
        let eq = |s: &str| s.to_lowercase() == name;
        self.id.as_deref().is_some_and(eq)
            || eq(&self.name)
            || self
                .file
                .as_deref()
                .is_some_and(|f| eq(f) || std::path::Path::new(f).file_stem().is_some_and(|s| eq(&s.to_string_lossy())))
    }
}

/// Error for a tool when the tree it asked for is not open.
pub fn no_tree(tree: Option<&str>) -> String {
    match tree {
        None => "no file is open in GEDRoots".to_string(),
        Some(t) => format!("no tree named “{t}” is open in GEDRoots: call list_trees for the open trees"),
    }
}

/// A change applied to the displayed document; an error leaves it untouched.
pub type Apply = Box<dyn FnOnce(&mut Gedcom) -> Result<(), String> + Send>;

/// A change of the research log by an agent, and the message shown once it is applied.
pub struct Edit {
    pub apply: Apply,
    pub message: String,
}

/// Copy of a window's document.
pub struct Snapshot {
    /// Which tree it is, to tell each tree's news apart (its id, else its file).
    pub key: String,
    pub ged: Gedcom,
    /// The review page: waiting proposals and what became of the others.
    pub queue: propose::Queue,
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
static BRIDGE: Mutex<Option<async_channel::Sender<Ask>>> = Mutex::new(None);
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
        let _ = tx.try_send(Ask { tree: None, request: Request::Connected });
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
    let (tx, rx) = async_channel::unbounded::<Ask>();
    *BRIDGE.lock().unwrap_or_else(|e| e.into_inner()) = Some(tx);
    cx.spawn(async move |cx| {
        while let Ok(Ask { tree, request }) = rx.recv().await {
            let tree = tree.as_deref();
            match request {
                Request::Snapshot(reply) => {
                    let snap = cx.update(|cx| workspace(tree, cx).ok()?.read(cx).agent_snapshot());
                    let _ = reply.send(snap);
                }
                Request::Quality(reply) => {
                    let res = cx.update(|cx| workspace(tree, cx).ok()?.update(cx, |ws, _| ws.agent_quality()));
                    let _ = reply.send(res);
                }
                Request::ProposeChange(req, reply) => {
                    let res = cx.update(|cx| workspace(tree, cx)?.update(cx, |ws, cx| ws.add_change(*req, cx)));
                    let _ = reply.send(res);
                }
                Request::Propose(pending, reply) => {
                    let res = cx.update(|cx| workspace(tree, cx)?.update(cx, |ws, cx| ws.add_proposal(*pending, cx)));
                    let _ = reply.send(res);
                }
                Request::ProposeChapter(req, reply) => {
                    let res = cx.update(|cx| workspace(tree, cx)?.update(cx, |ws, cx| ws.add_chapter(*req, cx)));
                    let _ = reply.send(res);
                }
                Request::Edit(edit, reply) => {
                    let res = cx.update(|cx| workspace(tree, cx)?.update(cx, |ws, cx| ws.agent_edit(edit, cx)));
                    let _ = reply.send(res);
                }
                Request::Withdraw(id, reply) => {
                    let res = cx.update(|cx| workspace(tree, cx)?.update(cx, |ws, cx| ws.withdraw_proposal(&id, cx)));
                    let _ = reply.send(res);
                }
                Request::Trees(reply) => {
                    let trees = cx.update(|cx| trees(cx).into_iter().map(|(t, _)| t).collect());
                    let _ = reply.send(trees);
                }
                Request::Connected => cx.update(|cx| cx.refresh_windows()),
                Request::Activity(a) => {
                    let _ =
                        cx.update(|cx| workspace(tree, cx).map(|ws| ws.update(cx, |ws, cx| ws.agent_activity(a, cx))));
                }
            }
        }
    })
    .detach();
    let settings = load();
    if settings.enabled {
        start(&settings.token);
    }
}

/// The open trees, the one in front first, with their windows.
fn trees(cx: &App) -> Vec<(Tree, Entity<crate::workspace::Workspace>)> {
    let front = cx.global::<Target>().0.as_ref().and_then(|w| w.upgrade()).map(|w| w.entity_id());
    let mut trees: Vec<_> = crate::windows::workspaces(cx)
        .into_iter()
        .filter_map(|w| w.upgrade())
        .filter_map(|ws| {
            let mut tree = ws.read(cx).agent_tree()?;
            tree.in_front = Some(ws.entity_id()) == front;
            Some((tree, ws))
        })
        .collect();
    trees.sort_by_key(|(t, _)| !t.in_front);
    trees
}

/// The window of the tree named `tree`, or the one in front.
fn workspace(tree: Option<&str>, cx: &App) -> Result<Entity<crate::workspace::Workspace>, String> {
    let Some(name) = tree else {
        return trees(cx).into_iter().next().map(|(_, ws)| ws).ok_or_else(|| no_tree(None));
    };
    let mut found: Vec<_> = trees(cx).into_iter().filter(|(t, _)| t.is_named(name)).collect();
    match found.len() {
        0 => Err(no_tree(tree)),
        1 => Ok(found.remove(0).1),
        _ => Err(format!("several open trees are named “{name}”: name one by its id, as list_trees gives it")),
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
    use super::{Settings, Tree, new_token, parse};

    #[test]
    fn trees_are_named_by_name_file_or_id() {
        let t = Tree {
            name: "Guyot.ged".into(),
            file: Some("Guyot.ged".into()),
            id: Some("4b1d".into()),
            people: 1,
            families: 0,
            sources: 0,
            media: 0,
            root: None,
            in_front: false,
        };
        for name in ["Guyot", "guyot.ged", " GUYOT ", "4B1D"] {
            assert!(t.is_named(name), "{name}");
        }
        assert!(!t.is_named("Marchand"));
    }

    #[test]
    fn settings_round_trip() {
        let s = parse("enabled=1\ntoken=abc\nclient=cursor\n");
        assert_eq!(s, Settings { enabled: true, token: "abc".into(), client: "cursor".into() });
        assert!(!parse("").enabled);
        assert_eq!(new_token().len(), 32);
        assert_ne!(new_token(), new_token());
    }
}
