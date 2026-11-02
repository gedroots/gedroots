//! GEDRoots' MCP tools and the HTTP server that serves them. They read the file; the only writing
//! tool, `propose_act`, adds a proposal to review in the app.

use super::propose::{self, Pending, ProposeParams};
use super::{Request, Snapshot};
use axum::extract::State;
use axum::http::{StatusCode, header};
use axum::middleware::Next;
use axum::response::Response;
use gedcom::RecordKind;
use gedcom::research::{Gap, PersonRef, PersonSheet, current_year};
use rmcp::handler::server::wrapper::{Json, Parameters};
use rmcp::transport::streamable_http_server::session::local::LocalSessionManager;
use rmcp::transport::streamable_http_server::{StreamableHttpServerConfig, StreamableHttpService};
use rmcp::{ServerHandler, prompt_handler, tool, tool_handler, tool_router};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use tokio_util::sync::CancellationToken;

#[derive(Clone)]
pub struct GedrootsMcp {
    bridge: async_channel::Sender<Request>,
    prompt_router: rmcp::handler::server::router::prompt::PromptRouter<Self>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SearchParams {
    /// Query as typed in GEDRoots' search box: words of the name or place.
    pub query: String,
    /// Maximum number of results (default 50).
    pub limit: Option<usize>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct PersonParams {
    /// Person id, as returned by another tool (e.g. `I12`).
    pub id: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct GapsParams {
    /// Last generation examined (1: the root person, 2: their parents…). Default 6, at most 12.
    pub generations: Option<u32>,
    /// Maximum number of gaps returned (default 100).
    pub limit: Option<usize>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SourceSearch {
    /// Words of the source's title, register, call number or repository name.
    pub query: String,
    /// Maximum number of results (default 30).
    pub limit: Option<usize>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct RepositoryRef {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct SourceRef {
    pub id: String,
    pub title: String,
    /// Register or collection (PUBL).
    pub register: String,
    pub repository: Option<RepositoryRef>,
    pub call_number: String,
    /// Acts recorded from this source: 1 for a source per act, more for a whole register.
    pub acts: usize,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct Sources {
    pub sources: Vec<SourceRef>,
    /// Repositories whose name matches, to cite in a new source.
    pub repositories: Vec<RepositoryRef>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct Proposed {
    /// Always "waiting for review": the user compares and saves it in GEDRoots.
    pub status: String,
    /// Proposals waiting for review in GEDRoots.
    pub waiting: usize,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct FileInfo {
    /// Name of the `.ged` file (empty for a file never saved).
    pub file: String,
    pub people: usize,
    pub families: usize,
    pub sources: usize,
    pub media: usize,
    /// Root person, start of the Sosa numbering.
    pub root: Option<PersonRef>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct People {
    pub people: Vec<PersonRef>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct Gaps {
    pub gaps: Vec<Gap>,
    /// Total number of gaps, before the limit.
    pub total: usize,
}

/// Error returned to the agent (tool result marked as an error, which it can read and correct).
type Res<T> = Result<Json<T>, String>;

#[tool_router]
impl GedrootsMcp {
    pub fn new(bridge: async_channel::Sender<Request>) -> Self {
        Self { bridge, prompt_router: Self::prompt_router() }
    }

    async fn snapshot(&self) -> Result<Snapshot, String> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.bridge.send(Request::Snapshot(tx)).await.map_err(|_| "GEDRoots is closing".to_string())?;
        rx.await.ok().flatten().ok_or_else(|| "no file is open in GEDRoots".to_string())
    }

    #[tool(
        name = "file_info",
        description = "File open in GEDRoots: name, number of people, families, sources and media, root person.",
        annotations(read_only_hint = true, open_world_hint = false)
    )]
    async fn file_info(&self) -> Res<FileInfo> {
        let s = self.snapshot().await?;
        let g = &s.ged;
        let year = current_year();
        Ok(Json(FileInfo {
            file: s
                .path
                .as_ref()
                .and_then(|p| p.file_name())
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default(),
            people: g.count(RecordKind::Individual),
            families: g.count(RecordKind::Family),
            sources: g.count(RecordKind::Source),
            media: g.count(RecordKind::Media),
            root: g.root().map(|r| g.person_ref(&r, year)),
        }))
    }

    #[tool(
        name = "search_people",
        description = "Searches the tree for people by name, given name or place. Living people are left out.",
        annotations(read_only_hint = true, open_world_hint = false)
    )]
    async fn search(&self, Parameters(p): Parameters<SearchParams>) -> Res<People> {
        let s = self.snapshot().await?;
        Ok(Json(People { people: s.ged.search_people(&p.query, p.limit.unwrap_or(50).min(500), current_year()) }))
    }

    #[tool(
        name = "person_sheet",
        description = "A person's sheet: events (date, place, cited sources with their reference, media), parents, unions and children, Sosa number.",
        annotations(read_only_hint = true, open_world_hint = false)
    )]
    async fn person(&self, Parameters(p): Parameters<PersonParams>) -> Res<PersonSheet> {
        let s = self.snapshot().await?;
        let id = p.id.trim().trim_matches('@');
        if s.ged.individual(id).is_none() {
            return Err(format!("no person with id {id}"));
        }
        s.ged.person_sheet(id, current_year()).map(Json).ok_or_else(|| "living person: sheet hidden".to_string())
    }

    #[tool(
        name = "research_gaps",
        description = "Records to look for among the root person's direct ancestors: births, marriages and deaths that are missing or have no source, by Sosa number, with each person's known places.",
        annotations(read_only_hint = true, open_world_hint = false)
    )]
    async fn gaps(&self, Parameters(p): Parameters<GapsParams>) -> Res<Gaps> {
        let s = self.snapshot().await?;
        if s.ged.root().is_none() {
            return Err("no root person: choose one in GEDRoots".into());
        }
        let all = s.ged.research_gaps(p.generations.unwrap_or(6).clamp(1, 12), current_year());
        let total = all.len();
        Ok(Json(Gaps { gaps: all.into_iter().take(p.limit.unwrap_or(100)).collect(), total }))
    }

    #[tool(
        name = "search_sources",
        description = "Searches the tree's sources (with their register, repository and number of acts) and repositories. Use it before proposing an act, to cite an existing source or repository.",
        annotations(read_only_hint = true, open_world_hint = false)
    )]
    async fn sources(&self, Parameters(p): Parameters<SourceSearch>) -> Res<Sources> {
        let s = self.snapshot().await?;
        let g = &s.ged;
        let words: Vec<String> = p.query.split_whitespace().map(str::to_lowercase).collect();
        let matches = |text: &str| {
            let t = text.to_lowercase();
            words.iter().all(|w| t.contains(w.as_str()))
        };
        let repo = |x: &str| {
            g.get(x)
                .filter(|n| n.tag == "REPO")
                .map(|n| RepositoryRef { id: x.to_string(), name: n.child_val("NAME").unwrap_or_default().to_string() })
        };
        let limit = p.limit.unwrap_or(30).min(200);
        let sources = g
            .records_of(RecordKind::Source)
            .filter_map(|n| {
                let src = gedcom::Source::new(n);
                let repository = src.repository().and_then(repo);
                let text = format!(
                    "{} {} {} {}",
                    src.title(),
                    src.publication().unwrap_or_default(),
                    src.call_number().unwrap_or_default(),
                    repository.as_ref().map(|r| r.name.as_str()).unwrap_or_default()
                );
                matches(&text).then(|| SourceRef {
                    id: src.xref().to_string(),
                    title: src.title(),
                    register: src.publication().unwrap_or_default(),
                    call_number: src.call_number().unwrap_or_default().to_string(),
                    acts: g.acts(src.xref()).len(),
                    repository,
                })
            })
            .take(limit)
            .collect();
        let repositories = g
            .records_of(RecordKind::Repository)
            .filter_map(|n| repo(n.xref.as_deref()?))
            .filter(|r| matches(&r.name))
            .take(limit)
            .collect();
        Ok(Json(Sources { sources, repositories }))
    }

    #[tool(
        name = "propose_act",
        description = "Proposes an act found in the archives (birth, marriage, death…), with its source, people and image. Nothing is written: the proposal waits in GEDRoots' review page, where the user compares it with the tree and saves or rejects it. It is refused, with the reason, if it does not fit the tree (unknown source or person, one source per act or per register as the file does, a parent other than the tree's, impossible date): fix it and propose again. Give the place as the tree names it, the date in GEDCOM format, the downloaded file (a PDF register needs pdf_page) and the act's page in the archive's viewer (url): both are kept, the image and a web link.",
        annotations(
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = false,
            open_world_hint = false
        )
    )]
    async fn propose(&self, Parameters(p): Parameters<ProposeParams>) -> Res<Proposed> {
        let s = self.snapshot().await?;
        let mut proposal = propose::to_proposal(&p)?;
        let year = current_year();
        proposal.complete(&s.ged);
        proposal.check(&s.ged, year)?;
        let original =
            p.file.as_ref().map(|f| std::path::PathBuf::from(f.trim())).filter(|f| !f.as_os_str().is_empty());
        // An act found online is kept twice: its image (downloaded file) and its web page.
        if original.is_none() && p.url.as_deref().is_some_and(|u| !u.trim().is_empty()) {
            return Err("give the downloaded image or PDF of the act in `file` too: GEDRoots keeps both \
                        the act's image and its web page (`url`)"
                .to_string());
        }
        let prepared = match &original {
            Some(path) => {
                let (path, page, images) = (path.clone(), p.pdf_page, propose::prefers_images(&s.ged));
                let prepared = tokio::task::spawn_blocking(move || {
                    propose::prepare_file(&path, page, images, &propose::cache_dir())
                })
                .await
                .map_err(|e| e.to_string())??;
                Some(prepared)
            }
            None => None,
        };
        let (image, preview) = match prepared {
            Some(p) => (Some(p.file), Some(p.preview)),
            None => (None, None),
        };
        propose::attach(&mut proposal, image.as_deref());
        let pending = Pending { proposal, original, pdf_page: p.pdf_page, image, preview };
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.bridge
            .send(Request::Propose(Box::new(pending), tx))
            .await
            .map_err(|_| "GEDRoots is closing".to_string())?;
        let waiting = rx.await.map_err(|_| "GEDRoots is closing".to_string())??;
        Ok(Json(Proposed { status: "waiting for review".into(), waiting }))
    }
}

// Instructions given to the agent on connection.
#[tool_handler(
    name = "gedroots",
    instructions = "GEDRoots is a genealogy app. These tools read the GEDCOM 7 family tree open in \
the app (its active window). They never change it: `propose_act` only adds an act to the \
app's review page, where the user compares it with the tree before saving it. Living people are hidden. Ids (I12, F3, S4…) are \
only for passing between tools: never show them to the user. To look for records (birth, marriage, \
death acts), call `research_gaps`, then `person_sheet` on the person to learn their places, dates \
and the sources already cited. When you find an act, read it, check it matches the person, \
call `search_sources` to reuse the source or repository, then `propose_act`. The prompts \
`find_record` and `read_record` give the full method."
)]
#[prompt_handler(router = self.prompt_router)]
impl ServerHandler for GedrootsMcp {
    async fn initialize(
        &self,
        request: rmcp::model::InitializeRequestParams,
        context: rmcp::service::RequestContext<rmcp::RoleServer>,
    ) -> Result<rmcp::model::InitializeResult, rmcp::ErrorData> {
        // The “AI Agents” window shows the last agent connected.
        let info = &request.client_info;
        super::note_client(info.title.as_deref().filter(|t| !t.is_empty()).unwrap_or(&info.name));
        context.peer.set_peer_info(request.clone());
        self.negotiate_initialize(&request)
    }
}

/// Refuses any request without the right token.
async fn check_token(
    State(token): State<String>,
    req: axum::extract::Request,
    next: Next,
) -> Result<Response, StatusCode> {
    let expected = format!("Bearer {token}");
    match req.headers().get(header::AUTHORIZATION).and_then(|v| v.to_str().ok()) {
        Some(v) if v == expected => Ok(next.run(req).await),
        _ => Err(StatusCode::UNAUTHORIZED),
    }
}

/// Serves the MCP protocol on `127.0.0.1:<port>/mcp` until cancelled.
pub async fn serve(
    bridge: async_channel::Sender<Request>,
    token: String,
    port: u16,
    cancel: CancellationToken,
) -> Result<(), String> {
    let mut config = StreamableHttpServerConfig::default().enforce_origin_validation();
    config.cancellation_token = cancel.child_token();
    let service = StreamableHttpService::new(
        move || Ok(GedrootsMcp::new(bridge.clone())),
        LocalSessionManager::default().into(),
        config,
    );
    let app = axum::Router::new()
        .nest_service("/mcp", service)
        .layer(axum::middleware::from_fn_with_state(token, check_token));
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", port))
        .await
        .map_err(|e| rust_i18n::t!("agent.port_unavailable", port = port, error = e.to_string()).into_owned())?;
    axum::serve(listener, app)
        .with_graceful_shutdown(async move { cancel.cancelled().await })
        .await
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    const FILE: &str = "0 HEAD\n1 GEDC\n2 VERS 7.0\n1 _GR_ROOT @I1@\n0 @I1@ INDI\n1 NAME Jean /MARCHAND/\n1 BIRT\n2 DATE 1850\n2 PLAC Rennes\n0 TRLR\n";

    /// Sends a JSON-RPC request; returns the JSON body (direct response or SSE stream).
    fn post(
        port: u16,
        token: &str,
        session: Option<&str>,
        body: serde_json::Value,
    ) -> (u16, Option<String>, serde_json::Value) {
        let mut req = ureq::post(&format!("http://127.0.0.1:{port}/mcp"))
            .set("Authorization", &format!("Bearer {token}"))
            .set("Accept", "application/json, text/event-stream")
            .set("Content-Type", "application/json");
        if let Some(s) = session {
            req = req.set("Mcp-Session-Id", s).set("MCP-Protocol-Version", "2025-11-25");
        }
        match req.send_string(&body.to_string()) {
            Ok(r) => {
                let status = r.status();
                let sid = r.header("mcp-session-id").map(str::to_string);
                let text = r.into_string().unwrap_or_default();
                let json = text
                    .lines()
                    .filter_map(|l| l.strip_prefix("data:"))
                    .map(str::trim)
                    .find(|l| !l.is_empty())
                    .map(str::to_string)
                    .unwrap_or(text);
                (status, sid, serde_json::from_str(&json).unwrap_or(serde_json::Value::Null))
            }
            Err(ureq::Error::Status(code, _)) => (code, None, serde_json::Value::Null),
            Err(e) => panic!("{e}"),
        }
    }

    #[test]
    fn serves_read_only_tools_with_token() {
        let (tx, rx) = async_channel::unbounded::<Request>();
        // Fake app loop: always returns the same document.
        std::thread::spawn(move || {
            while let Ok(Request::Snapshot(reply)) = rx.recv_blocking() {
                let _ = reply.send(Some(Snapshot { path: None, ged: gedcom::Gedcom::parse(FILE).unwrap() }));
            }
        });
        // Any free port: the app can run alongside on the usual port.
        let port = std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
        let cancel = CancellationToken::new();
        let c = cancel.clone();
        std::thread::spawn(move || {
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(serve(tx, "secret".into(), port, c)).unwrap();
        });
        let mut tries = 0;
        while std::net::TcpStream::connect(("127.0.0.1", port)).is_err() && tries < 50 {
            std::thread::sleep(std::time::Duration::from_millis(50));
            tries += 1;
        }
        let init = serde_json::json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{
            "protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"test","version":"1"}}});
        assert_eq!(post(port, "wrong", None, init.clone()).0, 401);
        let (status, sid, res) = post(port, "secret", None, init);
        assert_eq!(status, 200, "{res}");
        assert_eq!(res["result"]["serverInfo"]["name"], "gedroots");
        let sid = sid.expect("session");
        post(port, "secret", Some(&sid), serde_json::json!({"jsonrpc":"2.0","method":"notifications/initialized"}));
        let (_, _, list) =
            post(port, "secret", Some(&sid), serde_json::json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}));
        let tools = list["result"]["tools"].as_array().expect("tools");
        assert_eq!(tools.len(), 6);
        // Only `propose_act` is not read only, and it writes nothing.
        let writers: Vec<_> =
            tools.iter().filter(|t| t["annotations"]["readOnlyHint"] != true).map(|t| t["name"].clone()).collect();
        assert_eq!(writers, ["propose_act"]);
        let (_, _, call) = post(
            port,
            "secret",
            Some(&sid),
            serde_json::json!({"jsonrpc":"2.0","id":3,"method":"tools/call",
            "params":{"name":"person_sheet","arguments":{"id":"I1"}}}),
        );
        let sheet = &call["result"]["structuredContent"];
        assert_eq!(sheet["person"]["name"], "Jean MARCHAND", "{call}");
        assert_eq!(sheet["events"][0]["place"], "Rennes");
        // The working methods are served as prompts, with or without an argument.
        let (_, _, prompts) =
            post(port, "secret", Some(&sid), serde_json::json!({"jsonrpc":"2.0","id":4,"method":"prompts/list"}));
        let names: Vec<_> =
            prompts["result"]["prompts"].as_array().expect("prompts").iter().map(|p| p["name"].clone()).collect();
        assert_eq!(names.len(), 2, "{prompts}");
        assert!(names.contains(&"find_record".into()) && names.contains(&"read_record".into()));
        let (_, _, got) = post(
            port,
            "secret",
            Some(&sid),
            serde_json::json!({"jsonrpc":"2.0","id":5,"method":"prompts/get",
            "params":{"name":"find_record","arguments":{"person":"Jean MARCHAND","event":"birth"}}}),
        );
        let body = got["result"]["messages"][0]["content"]["text"].as_str().expect("text");
        assert!(body.contains("the birth of Jean MARCHAND") && !body.contains("{target}"), "{got}");
        let (_, _, bare) = post(
            port,
            "secret",
            Some(&sid),
            serde_json::json!({"jsonrpc":"2.0","id":6,"method":"prompts/get","params":{"name":"read_record"}}),
        );
        assert!(bare["result"]["messages"][0]["content"]["text"].is_string(), "{bare}");
        cancel.cancel();
    }
}
