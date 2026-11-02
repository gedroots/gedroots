//! Outils MCP de GEDRoots, en lecture seule, et serveur HTTP qui les sert.

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
use rmcp::{ServerHandler, tool, tool_handler, tool_router};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use tokio_util::sync::CancellationToken;

#[derive(Clone)]
pub struct GedrootsMcp {
    bridge: async_channel::Sender<Request>,
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

/// Erreur rendue à l'agent (résultat d'outil en erreur, qu'il peut lire et corriger).
type Res<T> = Result<Json<T>, String>;

#[tool_router]
impl GedrootsMcp {
    pub fn new(bridge: async_channel::Sender<Request>) -> Self {
        Self { bridge }
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
}

// Consignes données à l'agent à la connexion.
#[tool_handler(
    name = "gedroots",
    instructions = "GEDRoots is a genealogy app. These tools read the GEDCOM 7 family tree open in \
the app (its active window) and never change it. Living people are hidden. Ids (I12, F3, S4…) are \
only for passing between tools: never show them to the user. To look for records (birth, marriage, \
death acts), call `research_gaps`, then `person_sheet` on the person to learn their places, dates \
and the sources already cited."
)]
impl ServerHandler for GedrootsMcp {}

/// Refuse toute requête sans le bon jeton.
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

/// Sert le protocole MCP sur `127.0.0.1:<port>/mcp` jusqu'à l'annulation.
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

    /// Envoie une requête JSON-RPC ; rend le corps JSON (réponse directe ou flux SSE).
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
        // Fausse boucle de l'app : rend toujours le même document.
        std::thread::spawn(move || {
            while let Ok(Request::Snapshot(reply)) = rx.recv_blocking() {
                let _ = reply.send(Some(Snapshot { path: None, ged: gedcom::Gedcom::parse(FILE).unwrap() }));
            }
        });
        // Port libre quelconque : l'app peut tourner à côté sur le port habituel.
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
        assert_eq!(tools.len(), 4);
        assert!(tools.iter().all(|t| t["annotations"]["readOnlyHint"] == true));
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
        cancel.cancel();
    }
}
