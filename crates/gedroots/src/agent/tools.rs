//! GEDRoots' MCP tools and the HTTP server that serves them. They read the file; the writing tools,
//! `propose_act` and `propose_change`, only add a proposal to review in the app, and `plan_research`
//! / `log_search` write only the research log (`_GR_TASK`), with an undo point.

use super::change::{self, ChangeParams, ChangeProposed};
use super::propose::{self, Decision, Hypothesis, MatchParams, Pending, ProposeParams};
use super::{Request, Snapshot};
use axum::extract::State;
use axum::http::{StatusCode, header};
use axum::middleware::Next;
use axum::response::Response;
use gedcom::RecordKind;
use gedcom::context::FileContext;
use gedcom::leads::{Goal, Lead, PastSearch, Years};
use gedcom::research::{PersonRef, PersonSheet, QualityFilter, QualityReport, current_year};
use gedcom::research_log::{NewSearch, SearchResult, TaskState};
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
    /// Decisions already told to this session's agent.
    told: std::sync::Arc<std::sync::atomic::AtomicUsize>,
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
pub struct ContextParams {
    /// Records of each kind given as examples (default 5, at most 20).
    pub examples: Option<usize>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct LeadsParams {
    /// People to examine, in the syntax of GEDRoots' search: `sosa:1-63` (the root person's
    /// ancestors over 6 generations, the default), `sosa:1-15 place:manche`, `name:guyot
    /// born:<1850`, `descendants:@I12@`…
    pub focus: Option<String>,
    /// Only this person (id), instead of `focus`.
    pub person: Option<String>,
    /// States to return (default: suggested, planned and not_found).
    pub states: Option<Vec<TaskState>>,
    /// Maximum number of leads returned (default 50).
    pub limit: Option<usize>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct PlanItem {
    /// Person id (for a marriage, either spouse).
    pub person: String,
    pub goal: Goal,
    /// For a marriage of someone with several unions: the spouse's id.
    pub spouse: Option<String>,
    /// Why this lead, in one sentence (shown to the user).
    pub reason: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct PlanParams {
    /// Leads to search, in the order you will search them.
    pub items: Vec<PlanItem>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct LogParams {
    /// Person id (for a marriage, either spouse).
    pub person: String,
    pub goal: Goal,
    /// For a marriage of someone with several unions: the spouse's id.
    pub spouse: Option<String>,
    /// What was searched: archive, register or index, place, and the names tried with their
    /// spellings (“Decennial tables of Saint-Lô, deaths 1853–1862, MARCHAND, MARCHANT”).
    pub searched: String,
    pub result: SearchResult,
    /// First and last year searched.
    pub from_year: Option<i32>,
    pub to_year: Option<i32>,
    /// Place searched, as the tree names it.
    pub place: Option<String>,
    /// Page of the register or of the act in the archive's viewer.
    pub url: Option<String>,
    /// What you saw: a clue, a person with the same name ruled out, a gap in the registers…
    pub note: Option<String>,
    /// Nothing left to search for this lead for now: it becomes `not_found`.
    pub finished: Option<bool>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct LogReadParams {
    /// Person id.
    pub person: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct QualityParams {
    /// Only this category: dates, family, structure, empty, media, places, sources, files, coords.
    pub category: Option<String>,
    /// Only this kind of issue (`death_before_birth`, `empty_fact`…).
    pub code: Option<String>,
    /// Only the issues about this record (person, family, source… id).
    pub record: Option<String>,
    /// Also the issues the user hid because they checked them (default false).
    pub include_hidden: Option<bool>,
    /// Issues to skip, to read the next page (default 0).
    pub offset: Option<usize>,
    /// Maximum number of issues (default 100, at most 500).
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

#[derive(Debug, Deserialize, JsonSchema)]
pub struct LocateParams {
    /// The place as the act or the tree writes it, with what tells it apart: "Saint-Martin,
    /// Manche", "Lebach, Saarland".
    pub place: String,
    /// Ids of the act's people already in the tree: their events and their relatives' are the
    /// context that picks the right homonym.
    #[serde(default)]
    pub people: Vec<String>,
    /// Date of the act (GEDCOM), to put the events closest in time first.
    pub date: Option<String>,
    /// Maximum number of candidates (default 8, at most 15).
    pub limit: Option<usize>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct TreePlace {
    /// As the tree writes it: give it as is in `propose_act`.
    pub place: String,
    pub located: bool,
    /// Events and records that name it.
    pub uses: usize,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct NearbyPlace {
    /// “Birth of Jean MARCHAND, 1802, Agneaux (Manche)”.
    pub event: String,
    /// self, father, mother, spouse, child or sibling of the people given.
    pub relation: String,
    pub latitude: f64,
    pub longitude: f64,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct PlaceCandidate {
    /// The place in the tree's place format: give it as the act's (or a fact's) `place`, and in
    /// `locations`.
    pub place: String,
    /// Name and broader jurisdictions, as the gazetteer gives them.
    pub name: String,
    pub context: String,
    pub latitude: f64,
    pub longitude: f64,
    /// OpenStreetMap identifier (`relation/415228`): give it in `locations`.
    pub osm: Option<String>,
    /// Distance to the closest place of `nearby`, in km.
    pub distance_km: Option<f64>,
    /// That closest event.
    pub nearest: Option<String>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct Located {
    /// Places of the tree with this name: when one is the act's place, use it (it needs no
    /// location, or, if not located, propose_change set_coordinates).
    pub in_tree: Vec<TreePlace>,
    /// Located places of the people's events and their relatives', closest in time first.
    pub nearby: Vec<NearbyPlace>,
    /// What the gazetteer (OpenStreetMap) finds, places near `nearby` ranked higher.
    pub candidates: Vec<PlaceCandidate>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct NewPerson {
    /// Temporary id: link it (`identity` link) to name the same new person in another proposal.
    pub id: String,
    pub role: String,
    pub name: String,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct Proposed {
    /// Id of the proposal (`P3`): give it in `replaces` to correct it, or to `withdraw_proposal`.
    pub id: String,
    /// Always "waiting for review": the user compares and saves it in GEDRoots.
    pub status: String,
    /// Proposals waiting for review in GEDRoots.
    pub waiting: usize,
    /// What saving it will write, one line per change the user reviews (deduced facts included):
    /// check it against the act.
    pub will_write: Vec<String>,
    /// The people it creates, with their temporary id.
    pub new_people: Vec<NewPerson>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ProposalId {
    /// Id of a waiting proposal (`P3`), from `propose_act` or `list_proposals`.
    pub id: String,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct ProposalPerson {
    pub role: String,
    pub name: String,
    /// link, create or mention.
    pub identity: String,
    /// Id in the tree (link), or temporary id (create).
    pub id: Option<String>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct WaitingProposal {
    pub id: String,
    /// What is proposed: "act" (a record found in the archives).
    pub kind: String,
    /// GEDCOM tag of the act's event, and its date.
    pub event: String,
    pub date: String,
    /// Reference of the act in its source.
    pub page: String,
    pub people: Vec<ProposalPerson>,
    /// What saving it will write, as `propose_act` answered.
    pub will_write: Vec<String>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct Proposals {
    /// Waiting for the user in the review page, in its order.
    pub waiting: Vec<WaitingProposal>,
    /// What became of the others since GEDRoots started, most recent first.
    pub decided: Vec<Decision>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct Withdrawn {
    /// Proposals still waiting for review.
    pub waiting: usize,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct Candidate {
    pub person: PersonRef,
    /// Why they could be this person (“father of Jean BLANC · born 4 APR 1770”).
    pub reason: String,
    /// Strong enough that `propose_act` refuses to create the person without linking them or
    /// listing them in `not`.
    pub likely: bool,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct PersonMatches {
    /// Index in `people`.
    pub index: usize,
    pub role: String,
    pub name: String,
    /// People of the tree who could be this person, most likely first.
    pub candidates: Vec<Candidate>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct Matches {
    /// The people of the act not yet identified (without `id`).
    pub people: Vec<PersonMatches>,
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
pub struct Leads {
    /// Focus used.
    pub focus: String,
    pub leads: Vec<Lead>,
    /// Total number of leads, before the limit.
    pub total: usize,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct Planned {
    /// The leads planned, in your order, as saved in the file.
    pub planned: Vec<Lead>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct TaskLog {
    /// “Death of Jean MARCHAND”.
    pub title: String,
    pub goal: Goal,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub spouse: Option<PersonRef>,
    pub state: TaskState,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub note: String,
    pub searches: Vec<PastSearch>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct ResearchLog {
    pub person: PersonRef,
    /// Everything noted for this person and their unions, records found included.
    pub log: Vec<TaskLog>,
}

/// A lead's owner: the person, or the family for a marriage.
fn owner(
    g: &gedcom::Gedcom,
    person: &str,
    goal: Goal,
    spouse: Option<&str>,
) -> Result<(String, Option<String>), String> {
    let ind = g.individual(person).ok_or_else(|| format!("no person with id {person}"))?;
    if goal != Goal::Marriage {
        return Ok((person.to_string(), None));
    }
    let fams: Vec<&str> = ind
        .fams()
        .into_iter()
        .filter(|f| {
            spouse.is_none_or(|s| g.family(f).is_some_and(|fam| fam.husband() == Some(s) || fam.wife() == Some(s)))
        })
        .collect();
    match fams.as_slice() {
        [f] => Ok((f.to_string(), Some(f.to_string()))),
        [] => Err("no union between these people".into()),
        _ => Err("this person has several unions: give the spouse".into()),
    }
}

fn clean_id(id: &str) -> &str {
    id.trim().trim_matches('@')
}

/// One gazetteer request at a time, at most one per [`crate::gazetteer::PAUSE`], whatever the
/// agent asks: places are located one by one, never in bulk.
fn pace() {
    static LAST: std::sync::Mutex<Option<std::time::Instant>> = std::sync::Mutex::new(None);
    let mut last = LAST.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(t) = *last {
        let wait = crate::gazetteer::PAUSE.saturating_sub(t.elapsed());
        std::thread::sleep(wait);
    }
    *last = Some(std::time::Instant::now());
}

/// A person's id in the tree. A temporary id (`new-3`) of a saved proposal becomes the id the
/// person got; one of a waiting proposal says so, since the person is not in the tree yet.
fn person_id(s: &Snapshot, id: &str) -> Result<String, String> {
    let x = clean_id(id);
    if !super::propose::is_temp(x) {
        return Ok(x.to_string());
    }
    if let Some(real) = s.queue.saved.get(x) {
        return Ok(real.clone());
    }
    match s.queue.waiting.iter().find(|w| w.temps.iter().flatten().any(|t| t == x)) {
        Some(w) => Err(format!(
            "{x} is a new person of proposal {}, still waiting for review: they are not in the tree yet. Once the user saves it, `news` gives their id (and {x} keeps working)",
            w.id
        )),
        None => Err(format!("no person with id {x}")),
    }
}

/// [`person_id`] of an optional id.
fn person_opt(s: &Snapshot, id: Option<&str>) -> Result<Option<String>, String> {
    id.map(|x| person_id(s, x)).transpose()
}

fn capitalize(s: &str) -> String {
    let mut c = s.chars();
    c.next().map(|f| f.to_uppercase().chain(c).collect()).unwrap_or_default()
}

/// A tool's answer, with what the user decided about the proposals since the agent's last call.
#[derive(Debug, Serialize, JsonSchema)]
pub struct Answer<T> {
    #[serde(flatten)]
    pub result: T,
    /// What the user did with the proposals since your last call: saved (with the ids the new
    /// people now have in the tree: link those), rejected, opened in record entry. Only present
    /// when something happened.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub news: Vec<Decision>,
}

/// Error returned to the agent (tool result marked as an error, which it can read and correct).
type Res<T> = Result<Json<Answer<T>>, String>;

/// Most news given at once (a new session gets the latest ones).
const NEWS: usize = 20;

#[tool_router]
impl GedrootsMcp {
    pub fn new(bridge: async_channel::Sender<Request>) -> Self {
        Self { bridge, told: Default::default(), prompt_router: Self::prompt_router() }
    }

    /// The answer, with the decisions this session has not been told yet.
    fn answer<T>(&self, s: &Snapshot, result: T) -> Res<T> {
        let all = &s.queue.decided;
        let from = self.told.swap(all.len(), std::sync::atomic::Ordering::Relaxed).min(all.len());
        let news = all[from.max(all.len().saturating_sub(NEWS))..].to_vec();
        Ok(Json(Answer { result, news }))
    }

    async fn snapshot(&self) -> Result<Snapshot, String> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.bridge.send(Request::Snapshot(tx)).await.map_err(|_| "GEDRoots is closing".to_string())?;
        rx.await.ok().flatten().ok_or_else(|| "no file is open in GEDRoots".to_string())
    }

    /// Applies a change to the research log in the app.
    async fn edit(&self, apply: super::Apply, message: String) -> Result<(), String> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.bridge
            .send(Request::Edit(super::Edit { apply, message }, tx))
            .await
            .map_err(|_| "GEDRoots is closing".to_string())?;
        rx.await.map_err(|_| "GEDRoots is closing".to_string())?
    }

    #[tool(
        name = "file_info",
        description = "File open in GEDRoots: name, number of people, families, sources and media, root person.",
        annotations(read_only_hint = true, open_world_hint = false)
    )]
    async fn file_info(&self) -> Res<FileInfo> {
        let s = self.snapshot().await?;
        let g = &s.ged;
        self.answer(
            &s,
            FileInfo {
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
                root: g.root().map(|r| g.person_ref(&r)),
            },
        )
    }

    #[tool(
        name = "file_context",
        description = "How this file is written, to write new records the same way: the description the user wrote for the file (their conventions, word for word), its language and place format, the events, custom event types and occupations in use with their counts, where media are filed and named, and the latest sources, people, families, media and repositories changed, as raw GEDCOM. Call it before proposing anything, and follow what you see: source titles, registers, one source per act or per register, name forms, event types.",
        annotations(read_only_hint = true, open_world_hint = false)
    )]
    async fn file_context(&self, Parameters(p): Parameters<ContextParams>) -> Res<FileContext> {
        let s = self.snapshot().await?;
        let context = s.ged.file_context(p.examples.unwrap_or(5).min(20));
        self.answer(&s, context)
    }

    #[tool(
        name = "search_people",
        description = "Searches the tree for people by name, given name or place.",
        annotations(read_only_hint = true, open_world_hint = false)
    )]
    async fn search(&self, Parameters(p): Parameters<SearchParams>) -> Res<People> {
        let s = self.snapshot().await?;
        self.answer(&s, People { people: s.ged.search_people(&p.query, p.limit.unwrap_or(50).min(500)) })
    }

    #[tool(
        name = "person_sheet",
        description = "A person's sheet: events (date, place, street address, cited sources with their reference, media), parents, unions and children, Sosa number.",
        annotations(read_only_hint = true, open_world_hint = false)
    )]
    async fn person(&self, Parameters(p): Parameters<PersonParams>) -> Res<PersonSheet> {
        let s = self.snapshot().await?;
        let id = &person_id(&s, &p.id)?;
        if s.ged.individual(id).is_none() {
            return Err(format!("no person with id {id}"));
        }
        let sheet = s.ged.person_sheet(id).ok_or_else(|| format!("no person with id {id}"))?;
        self.answer(&s, sheet)
    }

    #[tool(
        name = "research_leads",
        description = "Records still to find, for the people of a focus (default: the root person's ancestors over 6 generations): births, marriages and deaths missing or without a source, and the parents of ancestors at the end of a line. Each lead gives the estimated years and why, the places to search first, where to look (kinds of registers for that country and period, with the registers of that place the file already cites, their archive and call number), its state (suggested, planned, not_found, found, ignored) and the searches already noted, so that you never search the same register twice.",
        annotations(read_only_hint = true, open_world_hint = false)
    )]
    async fn leads(&self, Parameters(p): Parameters<LeadsParams>) -> Res<Leads> {
        let s = self.snapshot().await?;
        let year = current_year();
        let focus = p.focus.as_deref().map(str::trim).filter(|f| !f.is_empty()).unwrap_or(gedcom::leads::DEFAULT_FOCUS);
        let all = match person_opt(&s, p.person.as_deref())? {
            Some(x) => {
                owner(&s.ged, &x, Goal::Birth, None)?;
                s.ged.leads_of(&x, year)
            }
            None => {
                if focus.contains("sosa") && s.ged.root().is_none() {
                    return Err("no root person: choose one in GEDRoots, or give another focus".into());
                }
                s.ged.research_leads(focus, year)
            }
        };
        let states = p.states.unwrap_or_else(|| vec![TaskState::Suggested, TaskState::Planned, TaskState::NotFound]);
        let all: Vec<Lead> = all.into_iter().filter(|l| states.contains(&l.state)).collect();
        let total = all.len();
        self.answer(
            &s,
            Leads {
                focus: if p.person.is_some() { String::new() } else { focus.to_string() },
                leads: all.into_iter().take(p.limit.unwrap_or(50).min(500)).collect(),
                total,
            },
        )
    }

    #[tool(
        name = "research_log",
        description = "Everything noted in the research log of a person and of their unions: planned leads, searches made (found, nothing, partial), records found, ignored leads.",
        annotations(read_only_hint = true, open_world_hint = false)
    )]
    async fn research_log(&self, Parameters(p): Parameters<LogReadParams>) -> Res<ResearchLog> {
        let s = self.snapshot().await?;
        let g = &s.ged;
        let x = &person_id(&s, &p.person)?;
        owner(g, x, Goal::Birth, None)?;
        let person = g.person_ref(x);
        let mut log = Vec::new();
        let mut add = |owner: &str, spouse: Option<PersonRef>| {
            for t in g.research_tasks(owner) {
                let title = match (t.goal, &spouse) {
                    (Goal::Marriage, Some(sp)) => format!("Marriage of {} and {}", person.name, sp.name),
                    (goal, _) => format!("{} of {}", capitalize(goal.name()), person.name),
                };
                log.push(TaskLog {
                    title,
                    goal: t.goal,
                    spouse: spouse.clone(),
                    state: t.state,
                    note: t.note,
                    searches: t.searches,
                });
            }
        };
        add(x, None);
        for f in g.individual(x).map(|i| i.fams()).unwrap_or_default() {
            let spouse =
                g.family(f).and_then(|fam| [fam.husband(), fam.wife()].into_iter().flatten().find(|s| *s != x));
            add(f, spouse.map(|s| g.person_ref(s)));
        }
        self.answer(&s, ResearchLog { person, log })
    }

    #[tool(
        name = "plan_research",
        description = "Saves your research plan: the leads you are about to search, in order, each with its reason. They become `planned` in GEDRoots' research log (shown to the user, written at once with an undo point; the tree's data is not changed). Each item must be a current lead of `research_leads`. Then search them one by one and note each search with `log_search`.",
        annotations(read_only_hint = false, destructive_hint = false, idempotent_hint = true, open_world_hint = false)
    )]
    async fn plan_research(&self, Parameters(p): Parameters<PlanParams>) -> Res<Planned> {
        let s = self.snapshot().await?;
        let year = current_year();
        if p.items.is_empty() {
            return Err("give at least one lead".into());
        }
        let mut todo: Vec<(String, Goal, String)> = Vec::new();
        let mut planned = Vec::new();
        for (i, item) in p.items.iter().enumerate() {
            let at = |e: String| format!("item {i}: {e}");
            let x = &person_id(&s, &item.person).map_err(at)?;
            let spouse = person_opt(&s, item.spouse.as_deref()).map_err(at)?;
            let (_, family) = owner(&s.ged, x, item.goal, spouse.as_deref()).map_err(at)?;
            if item.reason.trim().is_empty() {
                return Err(format!("item {i}: give the reason of this lead"));
            }
            let lead = s.ged.lead(x, item.goal, family.as_deref(), year).map_err(|e| format!("item {i}: {e}"))?;
            if lead.state == TaskState::Ignored {
                return Err(format!("item {i}: the user chose to ignore this lead"));
            }
            todo.push((lead.owner().to_string(), item.goal, item.reason.trim().to_string()));
            planned.push((x.to_string(), item.goal, family));
        }
        let n = todo.len();
        self.edit(
            Box::new(move |g| {
                for (owner, goal, reason) in &todo {
                    g.set_research_state(owner, *goal, TaskState::Planned, reason);
                }
                Ok(())
            }),
            crate::tn!("agent.planned", n).into_owned(),
        )
        .await?;
        let s = self.snapshot().await?;
        let planned =
            planned.iter().filter_map(|(x, goal, family)| s.ged.lead(x, *goal, family.as_deref(), year).ok()).collect();
        self.answer(&s, Planned { planned })
    }

    #[tool(
        name = "log_search",
        description = "Notes a search you just made for a lead, whatever its result: what you searched (archive, register or index, years, names tried), `found`, `nothing` or `partial`, the years, place and viewer page. Note every register you looked at, even in vain: the next search (yours or the user's) starts from there, and the estimated years narrow. Written at once in GEDRoots' research log (with an undo point); the tree's data is not changed: a record found is still proposed with `propose_act`. Returns the lead as it is now.",
        annotations(
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = false,
            open_world_hint = false
        )
    )]
    async fn log_search(&self, Parameters(p): Parameters<LogParams>) -> Res<Lead> {
        let s = self.snapshot().await?;
        let year = current_year();
        let x = person_id(&s, &p.person)?;
        let spouse = person_opt(&s, p.spouse.as_deref())?;
        let (own, family) = owner(&s.ged, &x, p.goal, spouse.as_deref())?;
        let searched = p.searched.trim().to_string();
        if searched.is_empty() {
            return Err("say what you searched (register, place, years, names)".into());
        }
        let years = match (p.from_year, p.to_year) {
            (Some(a), Some(b)) => Some(Years::new(a, b)),
            (Some(a), None) | (None, Some(a)) => Some(Years::new(a, a)),
            (None, None) => None,
        };
        let search = NewSearch {
            searched: searched.clone(),
            result: Some(p.result),
            years,
            place: p.place.unwrap_or_default(),
            url: p.url.unwrap_or_default(),
            note: p.note.unwrap_or_default(),
            by_agent: true,
        };
        let (goal, finished) = (p.goal, p.finished.unwrap_or(false));
        self.edit(
            Box::new(move |g| match g.log_research(&own, goal, &search, finished) {
                true => Ok(()),
                false => Err("could not note this search".into()),
            }),
            rust_i18n::t!("agent.logged", what = searched).into_owned(),
        )
        .await?;
        let s = self.snapshot().await?;
        match s.ged.lead(&x, p.goal, family.as_deref(), year) {
            Ok(lead) => self.answer(&s, lead),
            // The record is already cited: the lead is closed, its log stays.
            Err(_) => {
                let mut lead = gedcom::leads::closed_lead(&s.ged, &x, p.goal, family.as_deref());
                if let Some(t) = s.ged.research_task(lead.owner(), p.goal) {
                    lead.state = t.state;
                    lead.note = t.note;
                    lead.searches = t.searches;
                }
                self.answer(&s, lead)
            }
        }
    }

    #[tool(
        name = "quality_issues",
        description = "The Data Quality issues of the tree, as GEDRoots' Quality page lists them: inconsistent dates and ages, family links, GEDCOM structure, empty or unused items, media, places, events without a source, files to attach, places without coordinates. Each issue has its record, what is involved, whether the Quality page fixes it in one click, and whether the user can hide it. Filter by category, kind or record; `categories` counts every category.",
        annotations(read_only_hint = true, open_world_hint = false)
    )]
    async fn quality(&self, Parameters(p): Parameters<QualityParams>) -> Res<QualityReport> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.bridge.send(Request::Quality(tx)).await.map_err(|_| "GEDRoots is closing".to_string())?;
        let (s, issues) = rx.await.ok().flatten().ok_or_else(|| "no file is open in GEDRoots".to_string())?;
        let trim = |v: Option<String>| v.map(|v| v.trim().trim_matches('@').to_string()).filter(|v| !v.is_empty());
        let filter = QualityFilter {
            category: trim(p.category),
            code: trim(p.code),
            record: trim(p.record),
            include_hidden: p.include_hidden.unwrap_or(false),
        };
        let limit = p.limit.unwrap_or(100).min(500);
        let report = s.ged.quality_report(&issues, &filter, p.offset.unwrap_or(0), limit)?;
        self.answer(&s, report)
    }

    #[tool(
        name = "search_sources",
        description = "Searches the tree's sources (with their register, repository and number of acts) and repositories. Use it before proposing an act, to cite an existing source or repository; when the archive is not among the repositories, propose_act creates it with the new source (`source.repository`).",
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
        self.answer(&s, Sources { sources, repositories })
    }

    #[tool(
        name = "match_people",
        description = "Finds who the people named in an act are in the tree, before proposing it. Give the act's event and date and everyone named, with their role; give the id of those already identified, so that their relatives rank first. Returns, for each person without id, the candidates of the tree with the reason. `likely` candidates must be checked (person_sheet): `propose_act` refuses to create someone who resembles them unless they are linked or listed in `not`.",
        annotations(read_only_hint = true, open_world_hint = false)
    )]
    async fn match_people(&self, Parameters(p): Parameters<MatchParams>) -> Res<Matches> {
        let s = self.snapshot().await?;
        let act = propose::match_act(&p)?;
        let people = act
            .participants
            .iter()
            .enumerate()
            .filter(|(_, x)| !matches!(x.choice, gedcom::PersonChoice::Link(_)))
            .map(|(i, x)| PersonMatches {
                index: i,
                role: act.role_label(i).into_owned(),
                name: x.display_name(),
                candidates: act
                    .suggestions(&s.ged, i)
                    .into_iter()
                    .map(|c| Candidate {
                        person: s.ged.person_ref(&c.xref),
                        reason: c.reason,
                        likely: c.score >= gedcom::proposal::PLAUSIBLE_MATCH,
                    })
                    .collect(),
            })
            .collect();
        self.answer(&s, Matches { people })
    }

    #[tool(
        name = "locate_place",
        description = "Finds where a place of an act is, one place at a time, before proposing it: the tree's places of that name, the located places of the people's events and of their relatives' (closest in time first), and the candidates of the gazetteer (OpenStreetMap) with their distance to those events. Homonyms are common (Saint-Martin, Neukirchen): choose the candidate that fits the act (its jurisdictions, the family's region, the distance), not merely the first, and give its `place` and coordinates in `propose_act` (`place` and `locations`). When none fits or you are unsure, propose the place without a location.",
        annotations(read_only_hint = true, open_world_hint = true)
    )]
    async fn locate_place(&self, Parameters(p): Parameters<LocateParams>) -> Res<Located> {
        let s = self.snapshot().await?;
        let query = p.place.trim().to_string();
        if query.is_empty() {
            return Err("give the `place` to locate".into());
        }
        let g = &s.ged;
        let in_tree = g
            .places_named(&query)
            .into_iter()
            .take(10)
            .map(|pl| TreePlace { located: pl.coords().is_some(), uses: pl.uses, place: pl.name })
            .collect();
        let people: Vec<String> = p.people.iter().filter_map(|x| person_id(&s, x).ok()).collect();
        let refs: Vec<&str> = people.iter().map(String::as_str).collect();
        let year = p.date.as_deref().and_then(|d| gedcom::Date::parse(d).year());
        let nearby = g.nearby_places(&refs, year, 15);
        if crate::gazetteer::offline() {
            return Err("place search is turned off on this computer (offline)".into());
        }
        let limit = p.limit.unwrap_or(8).clamp(1, 15);
        let near = gedcom::locate::center(&nearby);
        let q = query.clone();
        let found = tokio::task::spawn_blocking(move || {
            pace();
            crate::gazetteer::search_near(&q, limit, near)
        })
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| format!("the gazetteer did not answer: {e}"))?;
        let candidates = gedcom::locate::rank(found, &nearby, &g.place_form())
            .into_iter()
            .map(|r| PlaceCandidate {
                place: r.place,
                name: r.candidate.title(),
                context: r.candidate.context(),
                latitude: r.candidate.lat,
                longitude: r.candidate.lon,
                osm: r.candidate.exid.as_ref().and_then(|(id, ty)| {
                    let kind = ty.trim_end_matches('/').rsplit('/').next()?;
                    Some(format!("{kind}/{id}"))
                }),
                distance_km: r.nearest.map(|(km, _)| (km * 10.0).round() / 10.0),
                nearest: r.nearest.map(|(_, i)| gedcom::locate::describe(&nearby[i])),
            })
            .collect();
        let nearby = nearby
            .iter()
            .map(|n| NearbyPlace {
                event: gedcom::locate::describe(n),
                relation: n.relation.to_string(),
                latitude: n.latitude,
                longitude: n.longitude,
            })
            .collect();
        self.answer(&s, Located { in_tree, nearby, candidates })
    }

    #[tool(
        name = "propose_act",
        description = "Proposes an act found in the archives (birth, marriage, death…), with its source, people and image. Nothing is written: the proposal waits in GEDRoots' review page, where the user compares it with the tree and saves or rejects it. Each person says who they are: identity link (in the tree, with id), create or mention; call match_people first. Give every other fact the act states about a person in their `facts` (birth date in a baptism, marginal mention of a death, occupations, residences), each with its own date or none; the answer lists everything that will be written, the proposal's id (to correct it, propose it again with `replaces`) and the temporary id of each new person (link it to name the same person in another proposal: never create them twice). It is refused, with the reason, if it does not fit the tree (unknown source or person, a parent other than the tree's, a person to create who resembles someone of the tree, impossible date): fix it and propose again. Write facts in the tree's language and wording (an occupation the tree already has keeps the tree's text; only the transcription keeps the act's language and spelling); an occupation or residence at another date is another attestation, not a duplicate. Give the place as the tree names it (the municipality: a street, number or building goes in `address`, never in the place); a new place can carry its coordinates in `locations`, chosen with `locate_place`. A new source names its repository: an existing one (`repository_id`), or one to create (`repository`, when search_sources finds none). Give the date in GEDCOM format, the downloaded file (a PDF register needs pdf_page) and the act's page in the archive's viewer (url): both are kept, the image and a web link.",
        annotations(
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = false,
            open_world_hint = false
        )
    )]
    async fn propose(&self, Parameters(p): Parameters<ProposeParams>) -> Res<Proposed> {
        let s = self.snapshot().await?;
        let replaces = p.replaces.as_deref().map(str::trim).filter(|r| !r.is_empty()).map(str::to_string);
        if let Some(r) = &replaces
            && !s.queue.waiting.iter().any(|w| &w.id == r)
        {
            return Err(match s.queue.decided.iter().rev().find(|d| &d.id == r) {
                Some(_) => format!("proposal {r} is no longer waiting for review: call list_proposals"),
                None => format!("no proposal {r}: call list_proposals"),
            });
        }
        // Checked against the tree with the other waiting proposals saved.
        let h = Hypothesis::new(&s.ged, &s.queue, replaces.as_deref());
        let mut proposal = propose::to_proposal(&p)?;
        let temps = h.resolve(&mut proposal, &s.ged)?;
        proposal.complete(&h.ged);
        proposal.check(&h.ged)?;
        propose::check_identities(&proposal, &propose::ruled_out(&p), &h)?;
        // As the review page will show it: against the tree, a person another proposal creates is
        // created by this one too.
        h.unresolve(&mut proposal, &temps);
        let will_write = propose::effects(&proposal, &s.ged);
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
        let people: Vec<(String, String)> = (0..proposal.act.participants.len())
            .map(|i| (proposal.act.role_label(i).into_owned(), proposal.act.participants[i].display_name()))
            .collect();
        let pending = Pending { proposal, original, pdf_page: p.pdf_page, image, preview, temps, replaces };
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.bridge
            .send(Request::Propose(Box::new(pending), tx))
            .await
            .map_err(|_| "GEDRoots is closing".to_string())?;
        let queued = rx.await.map_err(|_| "GEDRoots is closing".to_string())??;
        let new_people = queued
            .temps
            .iter()
            .zip(people)
            .filter_map(|(t, (role, name))| Some(NewPerson { id: t.clone()?, role, name }))
            .collect();
        self.answer(
            &s,
            Proposed {
                id: queued.id,
                status: "waiting for review".into(),
                waiting: queued.waiting,
                will_write,
                new_people,
            },
        )
    }

    #[tool(
        name = "list_proposals",
        description = "The proposals waiting in GEDRoots' review page (id, act, people with their temporary ids, what each will write) and what became of the others: saved, rejected, opened in record entry by the user, withdrawn or replaced. Call it before proposing again, to correct a waiting proposal (propose_act with `replaces`) rather than send it twice.",
        annotations(read_only_hint = true, open_world_hint = false)
    )]
    async fn list_proposals(&self) -> Res<Proposals> {
        let s = self.snapshot().await?;
        let waiting = s
            .queue
            .waiting
            .iter()
            .map(|w| {
                let act = &w.proposal.act;
                let people = act
                    .participants
                    .iter()
                    .enumerate()
                    .map(|(i, x)| {
                        let (identity, id) = match &x.choice {
                            gedcom::PersonChoice::Link(id) => ("link", Some(id.clone())),
                            gedcom::PersonChoice::Create => ("create", w.temps.get(i).cloned().flatten()),
                            gedcom::PersonChoice::Mention => ("mention", None),
                        };
                        ProposalPerson {
                            role: act.role_label(i).into_owned(),
                            name: x.display_name(),
                            identity: identity.into(),
                            id,
                        }
                    })
                    .collect();
                WaitingProposal {
                    id: w.id.clone(),
                    kind: "act".into(),
                    event: act.event.clone(),
                    date: act.date.clone(),
                    page: act.page.clone(),
                    people,
                    will_write: propose::effects(&w.proposal, &s.ged),
                }
            })
            .collect();
        let decided = s.queue.decided.iter().rev().take(50).cloned().collect();
        self.answer(&s, Proposals { waiting, decided })
    }

    #[tool(
        name = "withdraw_proposal",
        description = "Takes back a proposal still waiting in GEDRoots' review page (wrong act, wrong person…). Nothing was written. To correct it instead, call propose_act with `replaces`.",
        annotations(read_only_hint = false, destructive_hint = false, idempotent_hint = true, open_world_hint = false)
    )]
    async fn withdraw(&self, Parameters(p): Parameters<ProposalId>) -> Res<Withdrawn> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.bridge.send(Request::Withdraw(p.id, tx)).await.map_err(|_| "GEDRoots is closing".to_string())?;
        let waiting = rx.await.map_err(|_| "GEDRoots is closing".to_string())??;
        let s = self.snapshot().await?;
        self.answer(&s, Withdrawn { waiting })
    }

    #[tool(
        name = "propose_change",
        description = "Proposes a change for a Data Quality issue (`quality_issues`), or a correction you read in a document. Nothing is written: it waits in GEDRoots' review page, where the user saves or rejects it. `fix` applies the Quality page's own fix (issues with `automatic_fix`). `edit` changes the records the issue is about, with the edits the issue lists in `edits` (a date read in the act, the sex of a spouse, a place's coordinates, a citation of an existing source…). `hide` keeps the issue as right, for instance a marriage really celebrated very young, and saves the reason with it. For a `possible_duplicate`: `merge` when the two records are the same person (read both with `read_record` first; `choices` says, for each fact of the issue's `merge.conflicts`, whose value is right), `hide` when they are two people. The user reviews a merge in GEDRoots' merge window. Give `reason` (what you read) and, when you can, `evidence`. Never hide an issue only to clear the list. It is refused, with the reason, if the edit is not one the issue allows, touches another record, or does not make the issue go away: GEDRoots applies it to a copy first. The answer lists the issues the change would create. Without `issue`, it is a correction of any record: a given name or surname misread in an earlier proposal or wrong in the tree (`set_name`; `add_name` keeps a variant spelling as another name), an occupation or other fact, its value, date, place or street address (`set_event` on an existing one, `add_event` for a new one), a place's name everywhere it is used or its coordinates (`rename_place`, `set_coordinates`, choosing with `locate_place`), the repository of a source (`set_repository`: an existing one, or one to create), a sex, a citation; `edit` only, with `reason` and `evidence` (the source, the viewer page or a quote of the document).",
        annotations(
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = false,
            open_world_hint = false
        )
    )]
    async fn propose_change(&self, Parameters(p): Parameters<ChangeParams>) -> Res<ChangeProposed> {
        let req = change::to_request(&p)?;
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.bridge
            .send(Request::ProposeChange(Box::new(req), tx))
            .await
            .map_err(|_| "GEDRoots is closing".to_string())?;
        let (waiting, new_issues) = rx.await.map_err(|_| "GEDRoots is closing".to_string())??;
        let s = self.snapshot().await?;
        self.answer(&s, ChangeProposed { status: "waiting for review".into(), waiting, new_issues })
    }
}

/// Instructions given to the agent on connection.
const INSTRUCTIONS: &str = "GEDRoots is a genealogy app. These tools read the GEDCOM 7 family tree open in \
the app (its active window). They never change its data: `propose_act` and `propose_change` only add \
a proposal to the app's review page, where the user compares it with the tree before saving it. Only \
the research log (`plan_research`, `log_search`: what you plan and search, even in vain) is written \
at once. \
Start with `file_context`: it shows how this file is written (the user's own conventions, source \
titles, names, event types), so that what you propose looks like the rest of the file: write in its language, even when the act \
is in another one, and reuse its wording (only the transcription keeps the act's language and spelling). \
As genealogists do, a woman is recorded under her maiden name, even when an act names her by her \
husband's: look for her under both, never create her again under her married name. Ids (I12, F3, S4…) are \
only for passing between tools: never show them to the user. To look for records (birth, marriage, \
death acts), call `research_leads`: each lead gives the estimated years, the places, where to look \
and the searches already made; `person_sheet` gives the person's dates, places and sources. Note \
every search with `log_search`, found or not. When you find an act, read it, check it matches the person, \
call `match_people` to know who each person named is in the tree (link them, create them or only \
mention them), `search_sources` to reuse the source or repository (a new source may create its \
repository when the tree has none for that archive), `locate_place` for a place the tree has not \
located (choose the homonym close to the family's other events), then `propose_act`. A person \
that a waiting proposal creates has a temporary id (new-3): link it to name them again. To correct a \
waiting act, propose it again with `replaces`; `list_proposals` says which are still waiting. \
Every answer carries `news` when the user saved, rejected or edited a proposal since your last call: \
a saved proposal gives the ids its new people now have in the tree, to link from then on. \
The prompts `research_session`, `find_record` and `read_record` give the full method. To improve the data, `quality_issues` lists \
the tree's Data Quality issues and `propose_change` fixes, edits or hides one; the prompt \
`review_quality` gives the method. A mistake in the tree that no issue reports (a given name you \
misread in a proposal the user has saved, a name the act spells otherwise, a wrong date) is \
corrected with `propose_change` without `issue`: never leave it for the user to fix by hand.";

/// The file's context, also offered as a resource for clients that let the user attach one.
const CONTEXT_URI: &str = "gedroots://file/context";

#[tool_handler]
#[prompt_handler(router = self.prompt_router)]
impl ServerHandler for GedrootsMcp {
    fn get_info(&self) -> rmcp::model::ServerConfig {
        rmcp::model::ServerConfig::new(
            rmcp::model::ServerCapabilities::builder().enable_tools().enable_prompts().enable_resources().build(),
        )
        .with_server_info(rmcp::model::Implementation::new("gedroots", env!("CARGO_PKG_VERSION")))
        .with_instructions(INSTRUCTIONS.to_string())
    }

    async fn list_resources(
        &self,
        _request: Option<rmcp::model::PaginatedRequestParams>,
        _context: rmcp::service::RequestContext<rmcp::RoleServer>,
    ) -> Result<rmcp::model::ListResourcesResult, rmcp::ErrorData> {
        let context = rmcp::model::Resource::new(CONTEXT_URI, "file_context")
            .with_title("File context")
            .with_description("How the open file is written: the user's conventions, vocabulary and latest records")
            .with_mime_type("application/json");
        Ok(rmcp::model::ListResourcesResult::with_all_items(vec![context]))
    }

    async fn read_resource(
        &self,
        request: rmcp::model::ReadResourceRequestParams,
        _context: rmcp::service::RequestContext<rmcp::RoleServer>,
    ) -> Result<rmcp::model::ReadResourceResponse, rmcp::ErrorData> {
        if request.uri != CONTEXT_URI {
            return Err(rmcp::ErrorData::resource_not_found(format!("no resource {}", request.uri), None));
        }
        let s = self.snapshot().await.map_err(|e| rmcp::ErrorData::internal_error(e, None))?;
        let json = serde_json::to_string_pretty(&s.ged.file_context(5))
            .map_err(|e| rmcp::ErrorData::internal_error(e.to_string(), None))?;
        let mut text = rmcp::model::ResourceContents::text(json, CONTEXT_URI);
        if let rmcp::model::ResourceContents::TextResourceContents { mime_type, .. } = &mut text {
            *mime_type = Some("application/json".into());
        }
        Ok(rmcp::model::ReadResourceResult::new(vec![text]).into())
    }

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
        // Fake app loop: one document, changed only by the research log edits.
        std::thread::spawn(move || {
            let mut ged = gedcom::Gedcom::parse(FILE).unwrap();
            while let Ok(req) = rx.recv_blocking() {
                match req {
                    Request::Snapshot(reply) => {
                        let _ = reply.send(Some(Snapshot { path: None, ged: ged.clone(), queue: Default::default() }));
                    }
                    Request::Quality(reply) => {
                        let issues = gedcom::quality::run(&ged);
                        let _ = reply
                            .send(Some((Snapshot { path: None, ged: ged.clone(), queue: Default::default() }, issues)));
                    }
                    Request::Edit(edit, reply) => {
                        let _ = reply.send((edit.apply)(&mut ged));
                    }
                    _ => break,
                }
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
        assert_eq!(tools.len(), 16);
        // The proposals only queue a proposal; the research tools write the research log only.
        let mut writers: Vec<String> = tools
            .iter()
            .filter(|t| t["annotations"]["readOnlyHint"] != true)
            .map(|t| t["name"].as_str().unwrap_or_default().to_string())
            .collect();
        writers.sort();
        assert_eq!(writers, ["log_search", "plan_research", "propose_act", "propose_change", "withdraw_proposal"]);
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
        // Data Quality: the place of the birth has no coordinates.
        let (_, _, call) = post(
            port,
            "secret",
            Some(&sid),
            serde_json::json!({"jsonrpc":"2.0","id":7,"method":"tools/call",
            "params":{"name":"quality_issues","arguments":{"category":"coords"}}}),
        );
        let report = &call["result"]["structuredContent"];
        assert_eq!(report["issues"][0]["code"], "missing_coords", "{call}");
        assert_eq!(report["issues"][0]["values"][0], "Rennes");
        assert!(report["categories"].as_array().is_some_and(|c| !c.is_empty()));
        // The research loop: leads, plan, a search in vain, the log.
        let call = |id: u32, name: &str, args: serde_json::Value| {
            post(
                port,
                "secret",
                Some(&sid),
                serde_json::json!({"jsonrpc":"2.0","id":id,"method":"tools/call",
                "params":{"name":name,"arguments":args}}),
            )
            .2
        };
        let leads = call(20, "research_leads", serde_json::json!({}));
        let goals: Vec<_> = leads["result"]["structuredContent"]["leads"]
            .as_array()
            .expect("leads")
            .iter()
            .map(|l| l["goal"].as_str().unwrap_or_default().to_string())
            .collect();
        assert_eq!(goals, ["birth", "death", "parents"], "{leads}");
        let plan = call(
            21,
            "plan_research",
            serde_json::json!({"items":[{"person":"I1","goal":"death","reason":"Born in 1850 in Rennes"}]}),
        );
        assert_eq!(plan["result"]["structuredContent"]["planned"][0]["state"], "planned", "{plan}");
        let logged = call(
            22,
            "log_search",
            serde_json::json!({"person":"I1","goal":"death",
            "searched":"Decennial tables of Rennes, deaths, MARCHAND","result":"nothing","from_year":1863,"to_year":1872}),
        );
        let lead = &logged["result"]["structuredContent"];
        assert_eq!(lead["searches"][0]["result"], "nothing", "{logged}");
        assert_eq!(lead["searches"][0]["by_agent"], true);
        let log = call(23, "research_log", serde_json::json!({"person":"I1"}));
        assert_eq!(log["result"]["structuredContent"]["log"][0]["title"], "Death of Jean MARCHAND", "{log}");
        // A sourced or unknown record cannot be planned.
        let refused =
            call(24, "plan_research", serde_json::json!({"items":[{"person":"I1","goal":"marriage","reason":"x"}]}));
        assert_eq!(refused["result"]["isError"], true, "{refused}");
        // The working methods are served as prompts, with or without an argument.
        let (_, _, prompts) =
            post(port, "secret", Some(&sid), serde_json::json!({"jsonrpc":"2.0","id":4,"method":"prompts/list"}));
        let names: Vec<_> =
            prompts["result"]["prompts"].as_array().expect("prompts").iter().map(|p| p["name"].clone()).collect();
        assert_eq!(names.len(), 4, "{prompts}");
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
        // The file's context: as a tool, and as a resource.
        let (_, _, call) = post(
            port,
            "secret",
            Some(&sid),
            serde_json::json!({"jsonrpc":"2.0","id":8,"method":"tools/call",
            "params":{"name":"file_context","arguments":{}}}),
        );
        let ctx = &call["result"]["structuredContent"];
        assert!(ctx["examples"]["people"][0].as_str().is_some_and(|p| p.starts_with("0 @I1@ INDI")), "{call}");
        assert_eq!(ctx["event_tags"][0]["value"], "BIRT");
        let (_, _, list) =
            post(port, "secret", Some(&sid), serde_json::json!({"jsonrpc":"2.0","id":9,"method":"resources/list"}));
        assert_eq!(list["result"]["resources"][0]["uri"], CONTEXT_URI, "{list}");
        let (_, _, read) = post(
            port,
            "secret",
            Some(&sid),
            serde_json::json!({"jsonrpc":"2.0","id":10,"method":"resources/read","params":{"uri":CONTEXT_URI}}),
        );
        let text = read["result"]["contents"][0]["text"].as_str().expect("text");
        assert!(text.contains("Jean /MARCHAND/"), "{read}");
        cancel.cancel();
    }
}
