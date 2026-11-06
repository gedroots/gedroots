//! GEDRoots' MCP tools and the HTTP server that serves them. They read the file; the writing tools,
//! `propose_act`, `propose_change` and `propose_chapter`, only add a proposal to review in the app
//! (kept in the file until decided), and `note_research` writes only the research log
//! (`_GR_TASK`), with an undo point. What the file keeps is the agents' shared memory:
//! `resume_research` gives any agent the state of the research where the last one left it.

use super::book::{self, Books, ProposeChapterParams, ReadBookParams};
use super::change::{self, ChangeParams, ChangeProposed};
use super::propose::{self, Decision, ProposeParams};
use super::viz::{self as visual, ReadVizParams, Visualizations};
use super::{Ask, Request, Snapshot};
use axum::extract::State;
use axum::http::{StatusCode, header};
use axum::middleware::Next;
use axum::response::Response;
use gedcom::RecordKind;
use gedcom::agent_proposals::{Outcome, ProposalKind};
use gedcom::context::FileContext;
use gedcom::leads::{Goal, Lead, LogKind, Years};
use gedcom::research::{PersonRef, PersonSheet, QualityFilter, QualityReport, current_year};
use gedcom::research_log::{NewEntry, SearchResult, TaskState, UNKNOWN_AGENT};
use rmcp::handler::server::wrapper::{Json, Parameters};
use rmcp::transport::streamable_http_server::session::local::LocalSessionManager;
use rmcp::transport::streamable_http_server::{StreamableHttpServerConfig, StreamableHttpService};
use rmcp::{ServerHandler, prompt_handler, tool, tool_handler, tool_router};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use tokio_util::sync::CancellationToken;

#[derive(Clone)]
pub struct GedrootsMcp {
    bridge: async_channel::Sender<Ask>,
    /// Decisions of each tree's review page already told to this session's agent.
    told: std::sync::Arc<std::sync::Mutex<std::collections::HashMap<String, usize>>>,
    /// Name the session's agent gave when it connected (`clientInfo`).
    agent: std::sync::Arc<std::sync::Mutex<String>>,
    prompt_router: rmcp::handler::server::router::prompt::PromptRouter<Self>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SearchParams {
    /// Tree to work on, as `file_context` names it in `trees`, when several are open (default: the one in
    /// front in GEDRoots).
    pub tree: Option<String>,
    /// Query as typed in GEDRoots' search box: words of a name or place, spelling variants found
    /// by sound (Carl finds Karl), and the advanced search keywords (`born:<1850`, `died:1900-1910`,
    /// `place:manche`, `sosa:1-15`, `descendants:@I12@`…).
    pub query: Option<String>,
    /// Instead of `query`, the people named in an act, before proposing it: for each one without
    /// `id`, the people of the tree who could be them, with the reason.
    pub act: Option<propose::ActMatch>,
    /// Maximum number of results (default 50).
    pub limit: Option<usize>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct PersonParams {
    /// Tree to work on, as `file_context` names it in `trees`, when several are open (default: the one in
    /// front in GEDRoots).
    pub tree: Option<String>,
    /// Person id, as returned by another tool (e.g. `I12`).
    pub id: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ContextParams {
    /// Tree to work on, as `file_context` names it in `trees`, when several are open (default: the one in
    /// front in GEDRoots).
    pub tree: Option<String>,
    /// Records of each kind given as examples (default 5, at most 20).
    pub examples: Option<usize>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ResumeParams {
    /// Tree to work on, as `file_context` names it in `trees`, when several are open (default: the one in
    /// front in GEDRoots).
    pub tree: Option<String>,
    /// One person (id): all their leads and their unions', whatever their state, with their log,
    /// and the proposals about them. Without it: where the research of the whole tree stands.
    pub person: Option<String>,
    /// People whose leads to search next, in the syntax of GEDRoots' search: `sosa:1-63` (the root
    /// person's ancestors over 6 generations, the default), `sosa:1-15 place:manche`,
    /// `name:guyot born:<1850`, `descendants:@I12@`…
    pub focus: Option<String>,
    /// Maximum number of leads to search next (default 20, at most 200).
    pub limit: Option<usize>,
}

/// What an entry of the research log is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum NoteKind {
    /// A lead you are about to search: it becomes `planned`, with `text` as the reason.
    Plan,
    /// A register, index or database searched, whatever its result.
    Search,
    /// A record that may be the one, not tied to the person yet.
    Clue,
    /// Where the lead stands as you stop, for whoever takes it up next.
    Handoff,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct NoteParams {
    /// Tree to work on, as `file_context` names it in `trees`, when several are open (default: the one in
    /// front in GEDRoots).
    pub tree: Option<String>,
    pub kind: NoteKind,
    /// Person id (for a marriage, either spouse).
    pub person: String,
    pub goal: Goal,
    /// For a marriage of someone with several unions: the spouse's id.
    pub spouse: Option<String>,
    /// plan: why this lead, in one sentence. search: what was searched, archive, register or index,
    /// place and the names tried with their spellings (“Decennial tables of Saint-Lô, deaths
    /// 1853–1862, MARCHAND, MARCHANT”). clue: the record (“Burial of Jean MARCHAND, 4 May 1858,
    /// Carentan”). handoff: where the lead stands.
    pub text: String,
    /// search: found, nothing or partial (a mention or an index entry without the act).
    pub result: Option<SearchResult>,
    /// First and last year searched.
    pub from_year: Option<i32>,
    pub to_year: Option<i32>,
    /// Place searched, as the tree names it.
    pub place: Option<String>,
    /// Page of the register or of the record in the archive's viewer.
    pub url: Option<String>,
    /// What else you saw: a person of the same name ruled out, a gap in the registers…
    pub note: Option<String>,
    /// clue: what speaks for it being this person (names, ages, places, relatives).
    #[serde(rename = "for")]
    pub evidence_for: Option<String>,
    /// clue: what speaks against it.
    pub against: Option<String>,
    /// handoff: the next search to make, precise enough to make it without asking.
    pub next: Option<String>,
    /// Nothing left to search for this lead for now: it becomes `not_found`.
    pub finished: Option<bool>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct QualityParams {
    /// Tree to work on, as `file_context` names it in `trees`, when several are open (default: the one in
    /// front in GEDRoots).
    pub tree: Option<String>,
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
    /// Tree to work on, as `file_context` names it in `trees`, when several are open (default: the one in
    /// front in GEDRoots).
    pub tree: Option<String>,
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
    /// Tree to work on, as `file_context` names it in `trees`, when several are open (default: the one in
    /// front in GEDRoots).
    pub tree: Option<String>,
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
    /// False when it does not split into the tree's place format (`format`).
    pub follows_format: bool,
    /// The place completed with the best candidate when it is `likely`: missing jurisdictions
    /// (postal code, département, region) filled, in the tree's format. When it is this place,
    /// propose_change `set_coordinates` then `rename_place` to this name.
    pub completed: Option<String>,
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
    /// `likely`, `possible` or `unlikely`, from what speaks for and against it in `why`.
    pub fit: String,
    /// The reasons: distance to the family's events, the jurisdictions the query names, its level
    /// (a département is not a municipality), its country.
    pub why: Vec<String>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct Located {
    /// Places of the tree with this name: when one is the act's place, use it (it needs no
    /// location, or, if not located, propose_change set_coordinates).
    pub in_tree: Vec<TreePlace>,
    /// Located places of the people's events and their relatives', closest in time first.
    pub nearby: Vec<NearbyPlace>,
    /// What the gazetteer (OpenStreetMap) finds, the best fit first.
    pub candidates: Vec<PlaceCandidate>,
    /// The tree's place format (`HEAD.PLAC.FORM`): a new place gives each jurisdiction in this
    /// order, empty ones kept. A candidate's `place` already follows it.
    pub format: Vec<String>,
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
    /// New places it adds to the tree without coordinates: locate each with `locate_place` and
    /// propose again with `replaces` and `locations`, unless no candidate fits.
    pub unlocated_places: Vec<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ProposalId {
    /// Tree to work on, as `file_context` names it in `trees`, when several are open (default: the one in
    /// front in GEDRoots).
    pub tree: Option<String>,
    /// Id of a waiting proposal (`P3`), from `propose_act` or `resume_research`.
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
    pub kind: ProposalKind,
    /// “Marriage · Johann SCHANDL & Maria RIEDAUER”.
    pub title: String,
    pub agent: String,
    /// When it was sent (GEDCOM date, UTC time).
    pub date: String,
    /// An act: its people, with the temporary id of those it creates (link it to name them).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub people: Vec<ProposalPerson>,
    /// An act: what saving it will write.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub will_write: Vec<String>,
}

/// A proposal no longer waiting, as the file keeps it.
#[derive(Debug, Serialize, JsonSchema)]
pub struct DecidedProposal {
    pub id: String,
    pub kind: ProposalKind,
    pub title: String,
    pub agent: String,
    pub date: String,
    pub outcome: Outcome,
    /// The user's reason, the proposal that replaced it, or why it could not be queued again.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub note: String,
    /// The people (or families) it is about; once saved, those it created too.
    pub about: Vec<PersonRef>,
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

/// `file_context`: the open trees, then how the file is written.
#[derive(Debug, Serialize, JsonSchema)]
pub struct Context {
    /// The trees open in GEDRoots, the one in front first.
    pub trees: Vec<TreeInfo>,
    #[serde(flatten)]
    pub file: FileContext,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct TreeInfo {
    /// Name to give as `tree` to the other tools.
    pub name: String,
    /// Name of the `.ged` file (none for a tree never saved).
    pub file: Option<String>,
    /// Stable identifier of the tree, to tell apart two trees of the same name.
    pub id: Option<String>,
    pub people: usize,
    pub families: usize,
    pub sources: usize,
    pub media: usize,
    /// Root person, start of the Sosa numbering.
    pub root: Option<PersonRef>,
    /// The tree the tools use when they name none.
    pub in_front: bool,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct Found {
    /// With `query`: the people found.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub people: Option<Vec<PersonRef>>,
    /// With `act`: for each person of the act without id, the candidates of the tree.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub matches: Option<Vec<PersonMatches>>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct Resume {
    /// Language of the tree (BCP 47, `HEAD.LANG`; empty when the file does not say: the one its
    /// records are written in). Write everything you note in the research log in it.
    pub language: String,
    /// Leads being worked on, by you or another agent or the user (planned, or with a clue or a
    /// handoff), last worked on first, each with its whole log: carry on from there.
    pub in_progress: Vec<Lead>,
    /// Other leads: for `focus`, those still to search, best first; for a person, all of them.
    pub other_leads: Vec<Lead>,
    /// Number of other leads before the limit.
    pub other_total: usize,
    /// Proposals waiting for the user in GEDRoots' review page, in its order.
    pub waiting: Vec<WaitingProposal>,
    /// What became of the others, most recent first.
    pub decided: Vec<DecidedProposal>,
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

/// The people of a waiting act, with the temporary id of those it creates.
fn proposal_people(w: &propose::Waiting) -> Vec<ProposalPerson> {
    let act = &w.proposal.act;
    act.participants
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
        .collect()
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
    pub fn new(bridge: async_channel::Sender<Ask>) -> Self {
        Self { bridge, told: Default::default(), agent: Default::default(), prompt_router: Self::prompt_router() }
    }

    /// Tells the window what the agent is doing, for its Research page; never waits.
    fn report(&self, tree: Option<&str>, doing: super::Doing) {
        let activity = super::Activity { agent: self.agent_name(), doing };
        let _ = self.bridge.try_send(Ask { tree: tree.map(str::to_string), request: Request::Activity(activity) });
    }

    /// The session's agent, as it named itself, written with what it notes and proposes.
    fn agent_name(&self) -> String {
        let name = self.agent.lock().unwrap_or_else(|e| e.into_inner()).clone();
        if name.trim().is_empty() { UNKNOWN_AGENT.to_string() } else { name }
    }

    /// The answer, with the decisions this session has not been told yet.
    fn answer<T>(&self, s: &Snapshot, result: T) -> Res<T> {
        let all = &s.queue.decided;
        let from = {
            let mut told = self.told.lock().unwrap_or_else(|e| e.into_inner());
            told.insert(s.key.clone(), all.len()).unwrap_or(0).min(all.len())
        };
        let news = all[from.max(all.len().saturating_sub(NEWS))..].to_vec();
        Ok(Json(Answer { result, news }))
    }

    /// Sends `request` to the window of `tree` (the one in front by default).
    async fn send(&self, tree: Option<&str>, request: Request) -> Result<(), async_channel::SendError<Ask>> {
        self.bridge.send(Ask { tree: tree.map(str::to_string), request }).await
    }

    async fn snapshot(&self, tree: Option<&str>) -> Result<Snapshot, String> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.send(tree, Request::Snapshot(tx)).await.map_err(|_| "GEDRoots is closing".to_string())?;
        rx.await.ok().flatten().ok_or_else(|| super::no_tree(tree))
    }

    /// Applies a change to the research log in the app.
    async fn edit(&self, tree: Option<&str>, apply: super::Apply, message: String) -> Result<(), String> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.send(tree, Request::Edit(super::Edit { apply, message }, tx))
            .await
            .map_err(|_| "GEDRoots is closing".to_string())?;
        rx.await.map_err(|_| "GEDRoots is closing".to_string())?
    }

    /// The trees open in GEDRoots, the one in front first.
    async fn trees(&self) -> Result<Vec<TreeInfo>, String> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.send(None, Request::Trees(tx)).await.map_err(|_| "GEDRoots is closing".to_string())?;
        let trees = rx.await.map_err(|_| "GEDRoots is closing".to_string())?;
        Ok(trees
            .into_iter()
            .map(|t| TreeInfo {
                name: t.name,
                file: t.file,
                id: t.id,
                people: t.people,
                families: t.families,
                sources: t.sources,
                media: t.media,
                root: t.root,
                in_front: t.in_front,
            })
            .collect())
    }

    #[tool(
        name = "file_context",
        description = "Start every session with it. `trees`: the trees open in GEDRoots, one per window, the one in front first, with their size and root person; every tool works on the tree in front unless given `tree` (when the user names another one). Then how that file is written, to write new records the same way: the description the user wrote for the file (their conventions, word for word), its language and place format, the events, custom event types and occupations in use with their counts, where media are filed and named, and the latest sources, people, families, media and repositories changed, as raw GEDCOM. Follow what you see: source titles, registers, one source per act or per register, name forms, event types.",
        annotations(read_only_hint = true, open_world_hint = false)
    )]
    async fn file_context(&self, Parameters(p): Parameters<ContextParams>) -> Res<Context> {
        let tree = p.tree.clone();
        let trees = self.trees().await?;
        let s = self.snapshot(tree.as_deref()).await?;
        let file = s.ged.file_context(p.examples.unwrap_or(5).min(20));
        self.answer(&s, Context { trees, file })
    }

    #[tool(
        name = "search_people",
        description = "Finds people in the tree. With `query`: by name, given name or place, with spelling variants found by sound and the advanced search keywords. With `act`, before proposing an act: give its event and date and everyone it names with their role, and the id of those already identified so that their relatives rank first; for each person without id, the candidates of the tree with the reason. Check `likely` candidates with person_sheet: `propose_act` refuses to create someone who resembles them unless they are linked or listed in `not`. An empty result is not proof that someone is absent: try other spellings.",
        annotations(read_only_hint = true, open_world_hint = false)
    )]
    async fn search(&self, Parameters(p): Parameters<SearchParams>) -> Res<Found> {
        let tree = p.tree.clone();
        let s = self.snapshot(tree.as_deref()).await?;
        let Some(a) = &p.act else {
            let query = p.query.as_deref().unwrap_or_default();
            let people = s.ged.search_people(query, p.limit.unwrap_or(50).min(500));
            return self.answer(&s, Found { people: Some(people), matches: None });
        };
        self.report(tree.as_deref(), super::Doing::MatchesPeople);
        let act = propose::match_act(a)?;
        let matches = act
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
        self.answer(&s, Found { people: None, matches: Some(matches) })
    }

    #[tool(
        name = "person_sheet",
        description = "A person's sheet: events (date, place, street address, age, cause, cited sources with their reference, media, witnesses and other associates, notes), parents (and `pedigree` when not by birth), unions and children, the person's own notes and media, Sosa number.",
        annotations(read_only_hint = true, open_world_hint = false)
    )]
    async fn person(&self, Parameters(p): Parameters<PersonParams>) -> Res<PersonSheet> {
        let tree = p.tree.clone();
        let s = self.snapshot(tree.as_deref()).await?;
        let id = &person_id(&s, &p.id)?;
        self.report(tree.as_deref(), super::Doing::ReadsPerson(id.clone()));
        if s.ged.individual(id).is_none() {
            return Err(format!("no person with id {id}"));
        }
        let sheet = s.ged.person_sheet(id).ok_or_else(|| format!("no person with id {id}"))?;
        self.answer(&s, sheet)
    }

    #[tool(
        name = "resume_research",
        description = "Where the research stands, as GEDRoots' file keeps it for every agent: start every session with it, so that you carry on from where you, another agent or the user stopped, without searching anything twice. `in_progress`: the leads being worked on, each with its whole log (registers searched and what they gave, clues with what speaks for and against, the last handoff and its `next` search). `other_leads`: records still to find for the people of `focus` (births, marriages and deaths missing or without a source, parents at the end of a line), each with the estimated years and why, the places to search first, where to look (kinds of registers for that country and period, with the registers of that place the file already cites, their archive and call number). `waiting`: your proposals and other agents' waiting for the user, with the temporary ids of the people they create. `decided`: what became of the others (saved, rejected with the user's reason, opened in record entry, withdrawn, replaced). With `person`: everything about that person instead.",
        annotations(read_only_hint = true, open_world_hint = false)
    )]
    async fn resume_research(&self, Parameters(p): Parameters<ResumeParams>) -> Res<Resume> {
        let tree = p.tree.clone();
        self.report(tree.as_deref(), super::Doing::Resume);
        let s = self.snapshot(tree.as_deref()).await?;
        let g = &s.ged;
        let year = current_year();
        let limit = p.limit.unwrap_or(20).min(200);
        let person = person_opt(&s, p.person.as_deref())?;
        let mut in_progress = g.leads_in_progress(year);
        let same = |a: &Lead, b: &Lead| a.goal == b.goal && a.owner() == b.owner();
        let (other, proposals) = match &person {
            Some(x) => {
                owner(g, x, Goal::Birth, None)?;
                let of = |l: &Lead| &l.person.id == x || l.spouse.as_ref().is_some_and(|sp| &sp.id == x);
                in_progress.retain(of);
                // The leads already closed keep their log.
                let mut all = g.leads_of(x, year);
                for t in g.research_tasks(x) {
                    if !all.iter().any(|l| l.goal == t.goal && l.family_id.is_none()) {
                        let mut l = gedcom::leads::closed_lead(g, x, t.goal, None);
                        (l.state, l.note, l.log) = (t.state, t.note, t.log);
                        all.push(l);
                    }
                }
                let mut about: Vec<String> = vec![x.clone()];
                about.extend(g.individual(x).map(|i| i.fams()).unwrap_or_default().into_iter().map(str::to_string));
                let proposals: Vec<_> =
                    g.agent_proposals().into_iter().filter(|pr| pr.about.iter().any(|a| about.contains(a))).collect();
                (all, proposals)
            }
            None => {
                let focus =
                    p.focus.as_deref().map(str::trim).filter(|f| !f.is_empty()).unwrap_or(gedcom::leads::DEFAULT_FOCUS);
                let leads = if focus.contains("sosa") && g.root().is_none() {
                    Vec::new()
                } else {
                    g.research_leads(focus, year)
                        .into_iter()
                        .filter(|l| matches!(l.state, TaskState::Suggested | TaskState::NotFound))
                        .collect()
                };
                in_progress.truncate(50);
                (leads, g.agent_proposals())
            }
        };
        let other: Vec<Lead> = other.into_iter().filter(|l| !in_progress.iter().any(|i| same(i, l))).collect();
        let other_total = other.len();
        let waiting = proposals
            .iter()
            .filter(|pr| pr.outcome == Outcome::Waiting)
            .map(|pr| {
                let act = s.queue.waiting.iter().find(|w| w.id == pr.id);
                WaitingProposal {
                    id: pr.id.clone(),
                    kind: pr.kind,
                    title: pr.title.clone(),
                    agent: pr.agent.clone(),
                    date: format!("{} {}", pr.date, pr.time).trim().to_string(),
                    people: act.map(proposal_people).unwrap_or_default(),
                    will_write: act.map(|w| propose::effects(&w.proposal, g)).unwrap_or_default(),
                }
            })
            .collect();
        let decided = proposals
            .iter()
            .rev()
            .filter(|pr| pr.outcome != Outcome::Waiting)
            .take(30)
            .map(|pr| DecidedProposal {
                id: pr.id.clone(),
                kind: pr.kind,
                title: pr.title.clone(),
                agent: pr.agent.clone(),
                date: format!("{} {}", pr.date, pr.time).trim().to_string(),
                outcome: pr.outcome,
                note: pr.note.clone(),
                about: pr.about.iter().filter(|a| g.individual(a).is_some()).map(|a| g.person_ref(a)).collect(),
            })
            .collect();
        self.answer(
            &s,
            Resume {
                language: g.header.child_val("LANG").unwrap_or("").trim().to_string(),
                in_progress,
                other_leads: other.into_iter().take(limit).collect(),
                other_total,
                waiting,
                decided,
            },
        )
    }

    #[tool(
        name = "note_research",
        description = "Writes in the research log of a lead, which GEDRoots keeps in the file for the user and every agent (shown in its Research page; written at once, with an undo point; the tree's data is not changed). `plan`: a lead you are about to search, with why in `text`. `search`: every register, index or database you looked at, whatever its result (`found`, `nothing`, `partial` for a mention or an index entry without the act), with the years, place and viewer page: the next search, yours or anyone's, starts from there and the estimated years narrow. `clue`: a record that may be the one but that you cannot tie to the person (an age that contradicts the tree, two people of the same name): give its viewer page and what speaks `for` and `against`; it waits here until another record settles it, and is never written to the tree. `handoff`: before you stop, where the lead stands and the `next` search to make, so that another agent carries on without asking. A record found is still proposed with `propose_act`. Write in the tree's language (`language` in resume_research), like the rest of the file, whatever language the archive or the conversation is in; names, places and quotes from a record keep their spelling. Returns the lead as it is now.",
        annotations(
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = false,
            open_world_hint = false
        )
    )]
    async fn note_research(&self, Parameters(p): Parameters<NoteParams>) -> Res<Lead> {
        let tree = p.tree.clone();
        let s = self.snapshot(tree.as_deref()).await?;
        let year = current_year();
        let x = person_id(&s, &p.person)?;
        let spouse = person_opt(&s, p.spouse.as_deref())?;
        let (own, family) = owner(&s.ged, &x, p.goal, spouse.as_deref())?;
        let text = p.text.trim().to_string();
        if text.is_empty() {
            return Err(match p.kind {
                NoteKind::Plan => "give the reason of this lead in `text`",
                NoteKind::Search => "say in `text` what you searched (register, place, years, names)",
                NoteKind::Clue => "describe the record in `text`",
                NoteKind::Handoff => "say in `text` where the lead stands",
            }
            .into());
        }
        let goal = p.goal;
        let lead_owner = own.clone();
        let (apply, message): (super::Apply, String) = match p.kind {
            NoteKind::Plan => {
                let lead = s.ged.lead(&x, goal, family.as_deref(), year)?;
                if lead.state == TaskState::Ignored {
                    return Err("the user chose to ignore this lead".into());
                }
                let own = lead.owner().to_string();
                (
                    Box::new(move |g| match g.set_research_state(&own, goal, TaskState::Planned, &text) {
                        true => Ok(()),
                        false => Err("could not plan this lead".into()),
                    }),
                    crate::tn!("agent.planned", 1).into_owned(),
                )
            }
            kind => {
                let result = match (kind, p.result) {
                    (NoteKind::Search, None) => {
                        return Err("give the search's `result`: found, nothing or partial".into());
                    }
                    (_, r) => r,
                };
                let years = match (p.from_year, p.to_year) {
                    (Some(a), Some(b)) => Some(Years::new(a, b)),
                    (Some(a), None) | (None, Some(a)) => Some(Years::new(a, a)),
                    (None, None) => None,
                };
                let entry = NewEntry {
                    kind: match kind {
                        NoteKind::Clue => LogKind::Clue,
                        NoteKind::Handoff => LogKind::Handoff,
                        _ => LogKind::Search,
                    },
                    text: text.clone(),
                    result,
                    years,
                    place: p.place.unwrap_or_default(),
                    url: p.url.unwrap_or_default(),
                    note: p.note.unwrap_or_default(),
                    evidence_for: p.evidence_for.unwrap_or_default(),
                    against: p.against.unwrap_or_default(),
                    next: p.next.unwrap_or_default(),
                    agent: self.agent_name(),
                };
                let finished = p.finished.unwrap_or(false);
                (
                    Box::new(move |g| match g.log_research(&own, goal, &entry, finished) {
                        true => Ok(()),
                        false => Err("could not note this".into()),
                    }),
                    match kind {
                        NoteKind::Clue => crate::t!("agent.clue", what = text),
                        NoteKind::Handoff => crate::t!("agent.handoff", what = text),
                        _ => crate::t!("agent.logged", what = text),
                    }
                    .into_owned(),
                )
            }
        };
        self.edit(tree.as_deref(), apply, message).await?;
        let kind = match p.kind {
            NoteKind::Plan => super::NoteKind::Plan,
            NoteKind::Search => super::NoteKind::Search,
            NoteKind::Clue => super::NoteKind::Clue,
            NoteKind::Handoff => super::NoteKind::Handoff,
        };
        self.report(
            tree.as_deref(),
            super::Doing::Notes { lead: format!("{lead_owner}|{}", goal.tag()), kind, text: p.text.trim().to_string() },
        );
        let s = self.snapshot(tree.as_deref()).await?;
        match s.ged.lead(&x, goal, family.as_deref(), year) {
            Ok(lead) => self.answer(&s, lead),
            // The record is already cited: the lead is closed, its log stays.
            Err(_) => {
                let mut lead = gedcom::leads::closed_lead(&s.ged, &x, goal, family.as_deref());
                if let Some(t) = s.ged.research_task(lead.owner(), goal) {
                    (lead.state, lead.note, lead.log) = (t.state, t.note, t.log);
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
        let tree = p.tree.clone();
        self.report(tree.as_deref(), super::Doing::ReadsQuality);
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.send(tree.as_deref(), Request::Quality(tx)).await.map_err(|_| "GEDRoots is closing".to_string())?;
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
        let tree = p.tree.clone();
        self.report(tree.as_deref(), super::Doing::SearchesSources(p.query.trim().to_string()));
        let s = self.snapshot(tree.as_deref()).await?;
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
        name = "locate_place",
        description = "Finds where a place is, one place at a time, before proposing it or to locate and complete a place of the tree: the tree's places of that name, the located places of the people's events and of their relatives' (closest in time first), and the gazetteer's candidates (OpenStreetMap) in the tree's place format, the best fit first, each with its `fit` (likely, possible, unlikely) and `why`: distance to the family's events, the jurisdictions you name (\"Saint-Martin, Manche\"), its level (a département is not a municipality), its country. Homonyms are common (Saint-Martin, Neukirchen): weigh `why` against the act (the parish or département it names, where the people lived), not merely the first. For a new place, give the chosen candidate's `place` as is in `propose_act` (`place`, a fact's `place`, a residence) and, in `locations`, its coordinates, `osm` and the `reason` of your choice; never write a new place by hand in another format. For a place of the tree without coordinates or incomplete (`follows_format` false, `completed`), propose_change `set_coordinates` then `rename_place` to the completed name. When no candidate fits, propose the place without a location, written in the tree's `format`.",
        annotations(read_only_hint = true, open_world_hint = true)
    )]
    async fn locate_place(&self, Parameters(p): Parameters<LocateParams>) -> Res<Located> {
        let s = self.snapshot(p.tree.as_deref()).await?;
        let query = p.place.trim().to_string();
        if query.is_empty() {
            return Err("give the `place` to locate".into());
        }
        self.report(p.tree.as_deref(), super::Doing::LocatesPlace(query.clone()));
        let g = &s.ged;
        let form = g.gazetteer_form();
        let in_tree: Vec<gedcom::Place> = g.places_named(&query).into_iter().take(10).collect();
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
        let ranked = gedcom::locate::rank(found, &nearby, &form, &query);
        let best = ranked.first().filter(|r| r.fit == gedcom::locate::Fit::Likely);
        let in_tree = in_tree
            .into_iter()
            .map(|pl| {
                let completed =
                    best.map(|b| gedcom::gazetteer::complete(&pl.name, &b.candidate, &form)).filter(|c| *c != pl.name);
                TreePlace {
                    located: pl.coords().is_some(),
                    uses: pl.uses,
                    follows_format: gedcom::gazetteer::follows_form(&pl.name, &form),
                    completed,
                    place: pl.name,
                }
            })
            .collect();
        let candidates = ranked
            .into_iter()
            .map(|r| PlaceCandidate {
                fit: r.fit.as_str().to_string(),
                why: r.why,
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
        self.answer(&s, Located { in_tree, nearby, candidates, format: g.place_form() })
    }

    #[tool(
        name = "propose_act",
        description = "Proposes an act found in the archives (birth, marriage, death…), with its source, people and image. Nothing is written: the proposal waits in GEDRoots' review page, where the user compares it with the tree and saves or rejects it. Each person says who they are: identity link (in the tree, with id), create or mention; call search_people with the `act` first. Give every other fact the act states about a person in their `facts` (birth date in a baptism, marginal mention of a death, occupations, residences), each with its own date or none; the answer lists everything that will be written, the proposal's id (to correct it, propose it again with `replaces`) and the temporary id of each new person (link it to name the same person in another proposal: never create them twice). It is refused, with the reason, if it does not fit the tree (unknown source or person, a parent other than the tree's, a person to create who resembles someone of the tree, impossible date): fix it and propose again. Write facts in the tree's language and wording (an occupation the tree already has keeps the tree's text; only the transcription keeps the act's language and spelling); an occupation or residence at another date is another attestation, not a duplicate. Give the place as the tree names it (the municipality: a street, number or building goes in `address`, never in the place); a new place can carry its coordinates in `locations`, chosen with `locate_place`. A new source names its repository: an existing one (`repository_id`), or one to create (`repository`, when search_sources finds none). Give the date in GEDCOM format, the downloaded file (a PDF register needs pdf_page) and the act's page in the archive's viewer (url): both are kept, the image and a web link.",
        annotations(
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = false,
            open_world_hint = false
        )
    )]
    async fn propose(&self, Parameters(p): Parameters<ProposeParams>) -> Res<Proposed> {
        let tree = p.tree.clone();
        self.report(tree.as_deref(), super::Doing::ProposesAct);
        let s = self.snapshot(tree.as_deref()).await?;
        let agent = self.agent_name();
        let (ged, queue) = (s.ged.clone(), s.queue.clone());
        let built = tokio::task::spawn_blocking(move || propose::build(&p, &ged, &queue, &agent))
            .await
            .map_err(|e| e.to_string())??;
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.send(tree.as_deref(), Request::Propose(Box::new(built.pending), tx))
            .await
            .map_err(|_| "GEDRoots is closing".to_string())?;
        let queued = rx.await.map_err(|_| "GEDRoots is closing".to_string())??;
        let new_people = queued
            .temps
            .iter()
            .zip(built.people)
            .filter_map(|(t, (role, name))| Some(NewPerson { id: t.clone()?, role, name }))
            .collect();
        self.answer(
            &s,
            Proposed {
                id: queued.id,
                status: "waiting for review".into(),
                waiting: queued.waiting,
                will_write: built.will_write,
                new_people,
                unlocated_places: built
                    .new_places
                    .into_iter()
                    .filter(|(_, located)| !located)
                    .map(|(p, _)| p)
                    .collect(),
            },
        )
    }

    #[tool(
        name = "withdraw_proposal",
        description = "Takes back a proposal still waiting in GEDRoots' review page (wrong act, wrong person…). Nothing was written. To correct it instead, propose it again with `replaces`.",
        annotations(read_only_hint = false, destructive_hint = false, idempotent_hint = true, open_world_hint = false)
    )]
    async fn withdraw(&self, Parameters(p): Parameters<ProposalId>) -> Res<Withdrawn> {
        let tree = p.tree.clone();
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.send(tree.as_deref(), Request::Withdraw(p.id, tx)).await.map_err(|_| "GEDRoots is closing".to_string())?;
        let waiting = rx.await.map_err(|_| "GEDRoots is closing".to_string())??;
        let s = self.snapshot(tree.as_deref()).await?;
        self.answer(&s, Withdrawn { waiting })
    }

    #[tool(
        name = "propose_change",
        description = "Proposes typed edits of the tree: a correction you read in a document (a name misread in an earlier proposal or wrong in the tree, a date, a place, a family link, a note, a media), a change for a Data Quality issue (`quality_issues`), or a merge. Nothing is written: it waits in GEDRoots' review page, where the user saves or rejects it. `edits` lists the changes, of any kind, on any record; their fields are in the schema. With `issue`, the answer says whether it `resolves` it; `apply_fix` applies the Quality page's own fix (`automatic_fix`), `hide_issue` keeps an issue that is right (a marriage really celebrated very young) with the reason, never only to clear the list. `merge_people` and `merge_places` make two records one (read both first; `choices` gives whose value is right for each fact or level that differs); the user reviews them in the merge window. A note comments on facts already in the tree; a research lead goes to `note_research`. To attach the image of a document, give the file you downloaded to `attach_media`: GEDRoots imports it like an act's image. Always give `reason` (what you read) and, without an issue, `evidence` (the source, the viewer page or a quote). GEDRoots applies it to a copy first: it is refused, with the reason, only when it cannot apply; the answer lists the issues it would create. To correct it while it waits, propose again with `replaces`.",
        annotations(
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = false,
            open_world_hint = false
        )
    )]
    async fn propose_change(&self, Parameters(p): Parameters<ChangeParams>) -> Res<ChangeProposed> {
        let tree = p.tree.clone();
        self.report(tree.as_deref(), super::Doing::ProposesChange);
        let s = self.snapshot(tree.as_deref()).await?;
        let ged = s.ged.clone();
        let mut req =
            tokio::task::spawn_blocking(move || change::to_request(&p, &ged)).await.map_err(|e| e.to_string())??;
        req.agent = self.agent_name();
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.send(tree.as_deref(), Request::ProposeChange(Box::new(req), tx))
            .await
            .map_err(|_| "GEDRoots is closing".to_string())?;
        let (id, waiting, resolves, new_issues) = rx.await.map_err(|_| "GEDRoots is closing".to_string())??;
        let s = self.snapshot(tree.as_deref()).await?;
        self.answer(&s, ChangeProposed { id, status: "waiting for review".into(), waiting, resolves, new_issues })
    }

    #[tool(
        name = "read_book",
        description = "The family books of the tree (without `book`), or the chapters of one: each chapter is a generation, with its people (id, name, years, number, how much of a record the book gives them, how many facts the tree knows) and what the book already has from you. Read it before `propose_chapter`. The book's dates, places, charts and index are computed by GEDRoots from the tree: you write the narrative, the context and the hypotheses, and choose where to place visualizations.",
        annotations(read_only_hint = true, open_world_hint = false)
    )]
    async fn read_book(&self, Parameters(p): Parameters<ReadBookParams>) -> Res<Books> {
        let s = self.snapshot(p.tree.as_deref()).await?;
        let books = book::read(&s.ged, p.book.as_deref())?;
        self.answer(&s, books)
    }

    #[tool(
        name = "propose_chapter",
        description = "Proposes what you wrote for one chapter of a family book (`read_book`): an introduction (`intro`) and the narratives of some of its people (`people`). Each block is `text` (the facts as a narrative, people named by id in braces, `{I12}`, checked to be in the tree), `context` (history that explains the facts: a war, a migration, a crisis, a law; `title` and at least one of `sources` required: search them, never cite from memory alone) or `hypothesis` (what the records do not say, such as why a family moved; the `basis` it rests on required, with what would check it), or `visualization` (a chart GEDRoots draws from the tree when it typesets the book, `read_visualization`: `visualization` required, with `person`, `line` and `generations` when it is not about the book's people, and an optional comment in `text`; choose the ones that tell the chapter's story, such as the migrations a generation made, and read them with `read_visualization` first; one about the book's own people is then shown there instead of in the book's opening pages or figures). Never present a hypothesis as a fact. Nothing is written: the chapter waits in GEDRoots' review page, where the user leaves out the context, hypotheses and visualizations they do not want, then saves it into the book. A new proposal for the same chapter takes the place of the waiting one. Write in the book's `language` (`read_book`). Work one chapter at a time.",
        annotations(read_only_hint = false, destructive_hint = false, idempotent_hint = true, open_world_hint = false)
    )]
    async fn propose_chapter(&self, Parameters(p): Parameters<ProposeChapterParams>) -> Res<ChapterProposed> {
        let tree = p.tree.clone();
        self.report(tree.as_deref(), super::Doing::ProposesChapter);
        let s = self.snapshot(tree.as_deref()).await?;
        let mut req = book::to_request(&p, &s.ged)?;
        req.agent = self.agent_name();
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.send(tree.as_deref(), Request::ProposeChapter(Box::new(req), tx))
            .await
            .map_err(|_| "GEDRoots is closing".to_string())?;
        let (id, waiting) = rx.await.map_err(|_| "GEDRoots is closing".to_string())??;
        let s = self.snapshot(tree.as_deref()).await?;
        self.answer(&s, ChapterProposed { id, status: "waiting for review".into(), waiting })
    }

    #[tool(
        name = "read_visualization",
        description = "The visualizations GEDRoots draws from the tree. Without `visualization`: the list of them (fan chart coloured by origin, map of the birthplaces, migrations from one generation to the next, distances each generation moved, lifespans, ancestors found per generation, longevity, occupations, births, marriages and deaths by month), what each one shows and when it is worth placing in a family book. With one: its data for a person's ancestors or descendants (default: the root person's ancestors over 8 generations), as the table the app shows beside the drawing, with the legend of its colours and its caption. The figures are computed by GEDRoots: quote them rather than counting yourself.",
        annotations(read_only_hint = true, open_world_hint = false)
    )]
    async fn read_visualization(&self, Parameters(p): Parameters<ReadVizParams>) -> Res<Visualizations> {
        let s = self.snapshot(p.tree.as_deref()).await?;
        if p.visualization.as_deref().is_none_or(|v| v.trim().is_empty()) {
            return self.answer(&s, Visualizations { visualizations: Some(visual::list()), visualization: None });
        }
        let v = visual::read(&s.ged, &p, current_year())?;
        self.answer(&s, Visualizations { visualizations: None, visualization: Some(v) })
    }
}

/// Instructions given to the agent on connection.
const INSTRUCTIONS: &str = "GEDRoots is a genealogy app; these tools read the GEDCOM 7 family tree open in \
it. Start with `file_context`: the open trees (each tool works on the one in front, or on `tree`) and how \
the file is written; write like the rest of the file (its language, source titles, name forms, event \
types). The tools never change the tree's data: what you find becomes a proposal (`propose_act`, \
`propose_change`, `propose_chapter`) that the user reviews in GEDRoots before saving. Only the research \
log (`note_research`) is written at once. \
The file is the research's memory, shared by every agent and the user: you need no memory of your own. \
To search records, call `resume_research`: the leads in progress with everything searched, the clues \
and the last handoff, the records still to find, and what became of every proposal (saved, rejected \
with the user's reason). Carry on from there, never search a register twice, note every search with \
`note_research`, found or not, and before you stop, note a `handoff` on each lead you worked on, with \
the `next` search. A record you cannot tie to the person is a `clue` (with what speaks for and \
against), never a note in the tree. Propose each act as soon as you have read it, with your honest \
`quality` and your doubts in `remarks`: the user decides. \
Ids (I12, F3…) are only for passing between tools: never show them to the user. A person a waiting \
proposal creates has a temporary id (new-3): link it to name them again. Every answer carries `news` \
when the user decided on a proposal since your last call. The prompts give the method: `research` \
(find records), `read_record` (one record), `review_quality` (Data Quality issues), `write_book` \
(family books).";

/// The file's context, also offered as a resource for clients that let the user attach one.
const CONTEXT_URI: &str = "gedroots://file/context";

/// A chapter sent for review.
#[derive(Debug, Serialize, JsonSchema)]
pub struct ChapterProposed {
    /// Id of the proposal (`P3`), as `resume_research` lists it.
    pub id: String,
    pub status: String,
    /// Proposals waiting for review.
    pub waiting: usize,
}

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
        let s = self.snapshot(None).await.map_err(|e| rmcp::ErrorData::internal_error(e, None))?;
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
        let name = info.title.as_deref().filter(|t| !t.is_empty()).unwrap_or(&info.name);
        super::note_client(name);
        *self.agent.lock().unwrap_or_else(|e| e.into_inner()) = name.to_string();
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
    bridge: async_channel::Sender<Ask>,
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
        .map_err(|e| crate::t!("agent.port_unavailable", port = port, error = e.to_string()).into_owned())?;
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
        let (tx, rx) = async_channel::unbounded::<Ask>();
        // Fake app loop: one document, changed only by the research log edits.
        std::thread::spawn(move || {
            let mut ged = gedcom::Gedcom::parse(FILE).unwrap();
            while let Ok(ask) = rx.recv_blocking() {
                match ask.request {
                    Request::Snapshot(reply) => {
                        let _ = reply.send(Some(Snapshot {
                            key: String::new(),
                            ged: ged.clone(),
                            queue: Default::default(),
                        }));
                    }
                    Request::Quality(reply) => {
                        let issues = gedcom::quality::run(&ged);
                        let _ = reply.send(Some((
                            Snapshot { key: String::new(), ged: ged.clone(), queue: Default::default() },
                            issues,
                        )));
                    }
                    Request::Edit(edit, reply) => {
                        let _ = reply.send((edit.apply)(&mut ged));
                    }
                    Request::Trees(reply) => {
                        let _ = reply.send(vec![super::super::Tree {
                            name: "Marchand".into(),
                            file: Some("Marchand.ged".into()),
                            id: None,
                            people: ged.count(RecordKind::Individual),
                            families: ged.count(RecordKind::Family),
                            sources: ged.count(RecordKind::Source),
                            media: ged.count(RecordKind::Media),
                            root: ged.root().map(|r| ged.person_ref(&r)),
                            in_front: true,
                        }]);
                    }
                    Request::Activity(_) => {}
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
        assert_eq!(tools.len(), 14);
        // The proposals only queue a proposal; the research tools write the research log only.
        let mut writers: Vec<String> = tools
            .iter()
            .filter(|t| t["annotations"]["readOnlyHint"] != true)
            .map(|t| t["name"].as_str().unwrap_or_default().to_string())
            .collect();
        writers.sort();
        assert_eq!(writers, ["note_research", "propose_act", "propose_change", "propose_chapter", "withdraw_proposal"]);
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
        let (_, _, call) = post(
            port,
            "secret",
            Some(&sid),
            serde_json::json!({"jsonrpc":"2.0","id":6,"method":"tools/call","params":{"name":"file_context","arguments":{}}}),
        );
        let trees = &call["result"]["structuredContent"]["trees"];
        assert_eq!(trees[0]["name"], "Marchand", "{call}");
        assert_eq!(trees[0]["in_front"], true);
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
        let resumed = call(20, "resume_research", serde_json::json!({}));
        let goals: Vec<_> = resumed["result"]["structuredContent"]["other_leads"]
            .as_array()
            .expect("leads")
            .iter()
            .map(|l| l["goal"].as_str().unwrap_or_default().to_string())
            .collect();
        assert_eq!(goals, ["birth", "death", "parents"], "{resumed}");
        let plan = call(
            21,
            "note_research",
            serde_json::json!({"kind":"plan","person":"I1","goal":"death","text":"Born in 1850 in Rennes"}),
        );
        assert_eq!(plan["result"]["structuredContent"]["state"], "planned", "{plan}");
        let logged = call(
            22,
            "note_research",
            serde_json::json!({"kind":"search","person":"I1","goal":"death",
            "text":"Decennial tables of Rennes, deaths, MARCHAND","result":"nothing","from_year":1863,"to_year":1872}),
        );
        let lead = &logged["result"]["structuredContent"];
        assert_eq!(lead["log"][0]["result"], "nothing", "{logged}");
        assert_eq!(lead["log"][0]["agent"], "test");
        let handoff = call(
            23,
            "note_research",
            serde_json::json!({"kind":"handoff","person":"I1","goal":"death",
            "text":"Rennes tables searched in vain","next":"Saint-Malo deaths 1863-1872"}),
        );
        assert_eq!(handoff["result"]["structuredContent"]["log"][1]["kind"], "handoff", "{handoff}");
        // A search needs its result; a sourced or unknown record cannot be planned.
        let refused =
            call(24, "note_research", serde_json::json!({"kind":"search","person":"I1","goal":"death","text":"x"}));
        assert_eq!(refused["result"]["isError"], true, "{refused}");
        let refused =
            call(25, "note_research", serde_json::json!({"kind":"plan","person":"I1","goal":"marriage","text":"x"}));
        assert_eq!(refused["result"]["isError"], true, "{refused}");
        // Another agent, in a session of its own, carries on from the file alone.
        let init = serde_json::json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{
            "protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"codex","version":"1"}}});
        let (_, other, _) = post(port, "secret", None, init);
        let other = other.expect("session");
        post(port, "secret", Some(&other), serde_json::json!({"jsonrpc":"2.0","method":"notifications/initialized"}));
        let (_, _, resumed) = post(
            port,
            "secret",
            Some(&other),
            serde_json::json!({"jsonrpc":"2.0","id":26,"method":"tools/call",
            "params":{"name":"resume_research","arguments":{}}}),
        );
        let state = &resumed["result"]["structuredContent"];
        assert_eq!(state["in_progress"][0]["goal"], "death", "{resumed}");
        assert_eq!(state["in_progress"][0]["log"][1]["next"], "Saint-Malo deaths 1863-1872");
        assert!(state["other_leads"].as_array().is_some_and(|l| l.iter().all(|l| l["goal"] != "death")));
        let (_, _, clue) = post(
            port,
            "secret",
            Some(&other),
            serde_json::json!({"jsonrpc":"2.0","id":27,"method":"tools/call",
            "params":{"name":"note_research","arguments":{"kind":"clue","person":"I1","goal":"death",
            "text":"Burial of Jean MARCHAND, 1868, Saint-Malo","for":"Same name","against":"Aged 30"}}}),
        );
        let entry = &clue["result"]["structuredContent"]["log"][2];
        assert_eq!((entry["kind"].as_str(), entry["agent"].as_str()), (Some("clue"), Some("codex")), "{clue}");
        assert_eq!(entry["for"], "Same name");
        // The working methods are served as prompts, with or without an argument.
        let (_, _, prompts) =
            post(port, "secret", Some(&sid), serde_json::json!({"jsonrpc":"2.0","id":4,"method":"prompts/list"}));
        let names: Vec<_> =
            prompts["result"]["prompts"].as_array().expect("prompts").iter().map(|p| p["name"].clone()).collect();
        assert_eq!(names.len(), 4, "{prompts}");
        assert!(names.contains(&"research".into()) && names.contains(&"read_record".into()));
        let (_, _, got) = post(
            port,
            "secret",
            Some(&sid),
            serde_json::json!({"jsonrpc":"2.0","id":5,"method":"prompts/get",
            "params":{"name":"research","arguments":{"person":"Jean MARCHAND","event":"birth"}}}),
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
