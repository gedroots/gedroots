//! GEDRoots' MCP tools and the HTTP server that serves them. They read the file; the writing tools,
//! `propose_act` and `propose_change`, only add a proposal to review in the app.

use super::change::{self, ChangeParams, ChangeProposed};
use super::propose::{self, Decision, Hypothesis, MatchParams, Pending, ProposeParams};
use super::{Request, Snapshot};
use axum::extract::State;
use axum::http::{StatusCode, header};
use axum::middleware::Next;
use axum::response::Response;
use gedcom::RecordKind;
use gedcom::research::{Gap, PersonRef, PersonSheet, QualityFilter, QualityReport, current_year};
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
pub struct GapsParams {
    /// Last generation examined (1: the root person, 2: their parents…). Default 6, at most 12.
    pub generations: Option<u32>,
    /// Maximum number of gaps returned (default 100).
    pub limit: Option<usize>,
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
pub struct Gaps {
    pub gaps: Vec<Gap>,
    /// Total number of gaps, before the limit.
    pub total: usize,
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

    #[tool(
        name = "file_info",
        description = "File open in GEDRoots: name, number of people, families, sources and media, root person.",
        annotations(read_only_hint = true, open_world_hint = false)
    )]
    async fn file_info(&self) -> Res<FileInfo> {
        let s = self.snapshot().await?;
        let g = &s.ged;
        let year = current_year();
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
                root: g.root().map(|r| g.person_ref(&r, year)),
            },
        )
    }

    #[tool(
        name = "search_people",
        description = "Searches the tree for people by name, given name or place. A living person is given only their name and birth (`born`).",
        annotations(read_only_hint = true, open_world_hint = false)
    )]
    async fn search(&self, Parameters(p): Parameters<SearchParams>) -> Res<People> {
        let s = self.snapshot().await?;
        self.answer(
            &s,
            People { people: s.ged.search_people(&p.query, p.limit.unwrap_or(50).min(500), current_year()) },
        )
    }

    #[tool(
        name = "person_sheet",
        description = "A person's sheet: events (date, place, cited sources with their reference, media), parents, unions and children, Sosa number. A living person's sheet only gives their birth and close relatives.",
        annotations(read_only_hint = true, open_world_hint = false)
    )]
    async fn person(&self, Parameters(p): Parameters<PersonParams>) -> Res<PersonSheet> {
        let s = self.snapshot().await?;
        let id = p.id.trim().trim_matches('@');
        if s.ged.individual(id).is_none() {
            return Err(format!("no person with id {id}"));
        }
        let sheet = s.ged.person_sheet(id, current_year()).ok_or_else(|| format!("no person with id {id}"))?;
        self.answer(&s, sheet)
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
        self.answer(&s, Gaps { gaps: all.into_iter().take(p.limit.unwrap_or(100)).collect(), total })
    }

    #[tool(
        name = "quality_issues",
        description = "The Data Quality issues of the tree, as GEDRoots' Quality page lists them: inconsistent dates and ages, family links, GEDCOM structure, empty or unused items, media, places, events without a source, files to attach, places without coordinates. Each issue has its record, what is involved, whether the Quality page fixes it in one click, and whether the user can hide it. Filter by category, kind or record; `categories` counts every category. Records of people who may be living are marked `living`.",
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
        let report = s.ged.quality_report(&issues, &filter, p.offset.unwrap_or(0), limit, current_year())?;
        self.answer(&s, report)
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
        let year = current_year();
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
                        person: s.ged.person_ref(&c.xref, year),
                        reason: c.reason,
                        likely: c.score >= gedcom::proposal::PLAUSIBLE_MATCH,
                    })
                    .collect(),
            })
            .collect();
        self.answer(&s, Matches { people })
    }

    #[tool(
        name = "propose_act",
        description = "Proposes an act found in the archives (birth, marriage, death…), with its source, people and image. Nothing is written: the proposal waits in GEDRoots' review page, where the user compares it with the tree and saves or rejects it. Each person says who they are: identity link (in the tree, with id), create or mention; call match_people first. Give every other fact the act states about a person in their `facts` (birth date in a baptism, marginal mention of a death, occupations, residences), each with its own date or none; the answer lists everything that will be written, the proposal's id (to correct it, propose it again with `replaces`) and the temporary id of each new person (link it to name the same person in another proposal: never create them twice). It is refused, with the reason, if it does not fit the tree (unknown source or person, one source per act or per register as the file does, a parent other than the tree's, a person to create who resembles someone of the tree, impossible date): fix it and propose again. Give the place as the tree names it, the date in GEDCOM format, the downloaded file (a PDF register needs pdf_page) and the act's page in the archive's viewer (url): both are kept, the image and a web link.",
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
        description = "Proposes a change for a Data Quality issue (`quality_issues`). Nothing is written: it waits in GEDRoots' review page, where the user saves or rejects it. `fix` applies the Quality page's own fix (issues with `automatic_fix`). `edit` changes the records the issue is about, with the edits the issue lists in `edits` (a date read in the act, the sex of a spouse, a place's coordinates, a citation of an existing source…). `hide` keeps the issue as right, for instance a marriage really celebrated very young, and saves the reason with it. For a `possible_duplicate`: `merge` when the two records are the same person (read both with `read_record` first; `choices` says, for each fact of the issue's `merge.conflicts`, whose value is right), `hide` when they are two people. The user reviews a merge in GEDRoots' merge window. Give `reason` (what you read) and, when you can, `evidence`. Never hide an issue only to clear the list. It is refused, with the reason, if the edit is not one the issue allows, touches another record, or does not make the issue go away: GEDRoots applies it to a copy first. The answer lists the issues the change would create.",
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

// Instructions given to the agent on connection.
#[tool_handler(
    name = "gedroots",
    instructions = "GEDRoots is a genealogy app. These tools read the GEDCOM 7 family tree open in \
the app (its active window). They never change it: `propose_act` and `propose_change` only add \
a proposal to the app's review page, where the user compares it with the tree before saving it. Of a living person, only \
what identifies them is given (name, birth, close relatives). Ids (I12, F3, S4…) are \
only for passing between tools: never show them to the user. To look for records (birth, marriage, \
death acts), call `research_gaps`, then `person_sheet` on the person to learn their places, dates \
and the sources already cited. When you find an act, read it, check it matches the person, \
call `match_people` to know who each person named is in the tree (link them, create them or only \
mention them), `search_sources` to reuse the source or repository, then `propose_act`. A person \
that a waiting proposal creates has a temporary id (new-3): link it to name them again. To correct a \
waiting act, propose it again with `replaces`; `list_proposals` says which are still waiting. \
Every answer carries `news` when the user saved, rejected or edited a proposal since your last call: \
a saved proposal gives the ids its new people now have in the tree, to link from then on. \
The prompts `find_record` and `read_record` give the full method. To improve the data, `quality_issues` lists \
the tree's Data Quality issues and `propose_change` fixes, edits or hides one; the prompt \
`review_quality` gives the method."
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
            let ged = || gedcom::Gedcom::parse(FILE).unwrap();
            while let Ok(req) = rx.recv_blocking() {
                match req {
                    Request::Snapshot(reply) => {
                        let _ = reply.send(Some(Snapshot { path: None, ged: ged(), queue: Default::default() }));
                    }
                    Request::Quality(reply) => {
                        let issues = gedcom::quality::run(&ged());
                        let _ =
                            reply.send(Some((Snapshot { path: None, ged: ged(), queue: Default::default() }, issues)));
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
        assert_eq!(tools.len(), 11);
        // Only the proposals are not read only, and they write nothing.
        let writers: Vec<_> =
            tools.iter().filter(|t| t["annotations"]["readOnlyHint"] != true).map(|t| t["name"].clone()).collect();
        assert_eq!(writers, ["propose_act", "propose_change", "withdraw_proposal"]);
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
        // The working methods are served as prompts, with or without an argument.
        let (_, _, prompts) =
            post(port, "secret", Some(&sid), serde_json::json!({"jsonrpc":"2.0","id":4,"method":"prompts/list"}));
        let names: Vec<_> =
            prompts["result"]["prompts"].as_array().expect("prompts").iter().map(|p| p["name"].clone()).collect();
        assert_eq!(names.len(), 3, "{prompts}");
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
