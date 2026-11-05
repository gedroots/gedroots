//! Research log, kept in the file: what was planned and searched for each lead
//! ([`crate::leads`]), including the searches that found nothing, the clues not proven yet and
//! where the work stands, so that nobody (the user or an agent) searches the same register twice
//! and any agent can take over another's work from the file alone.
//!
//! One `_GR_TASK` structure per lead, under the record it concerns (the individual, or the family
//! for a marriage), documented in `docs/extensions.md`:
//!
//! ```text
//! 0 @I4@ INDI
//! 1 _GR_TASK DEAT
//! 2 _GR_STAT PLANNED
//! 2 NOTE Last seen in 1852, his widow remarried in 1860
//! 2 _GR_LOG
//! 3 DATE 3 NOV 2026
//! 3 TEXT Decennial tables of Saint-Lô, deaths, MARCHAND and MARCHANT
//! 3 _GR_RES NONE
//! 3 _GR_SPAN FROM 1853 TO 1862
//! 3 PLAC Saint-Lô, Manche, France
//! 3 WWW https://archives.manche.fr/ark:/…
//! 3 _GR_AGENT Claude Code
//! 2 _GR_LOG
//! 3 DATE 3 NOV 2026
//! 4 TIME 21:47:03
//! 3 _GR_KIND CLUE
//! 3 TEXT Burial of Jean MARCHAND, 4 May 1858, Carentan
//! 3 WWW https://archives.manche.fr/ark:/…
//! 3 _GR_FOR Same name, widower of a Rose
//! 3 _GR_AGNST Aged 50, about ten years younger than our Jean
//! 3 _GR_AGENT Claude Code
//! 2 _GR_LOG
//! 3 DATE 3 NOV 2026
//! 3 _GR_KIND HANDOFF
//! 3 TEXT Saint-Lô tables searched 1853–1862; one clue in Carentan
//! 3 _GR_NEXT Carentan deaths 1858: age and spouse's name
//! 3 _GR_AGENT Claude Code
//! ```

use crate::document::Gedcom;
use crate::leads::{Goal, LogEntry, LogKind, Years};
use crate::node::Node;

/// Extension structure of a lead's research log.
pub const TASK_TAG: &str = "_GR_TASK";
const STAT_TAG: &str = "_GR_STAT";
const LOG_TAG: &str = "_GR_LOG";
const RES_TAG: &str = "_GR_RES";
const SPAN_TAG: &str = "_GR_SPAN";
const AGENT_TAG: &str = "_GR_AGENT";
const KIND_TAG: &str = "_GR_KIND";
const FOR_TAG: &str = "_GR_FOR";
const AGAINST_TAG: &str = "_GR_AGNST";
const NEXT_TAG: &str = "_GR_NEXT";
/// Name written when the agent did not say who it is.
pub const UNKNOWN_AGENT: &str = "AI agent";
const URI: &str = extension_uri!("_gr_task");

/// State of a lead.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize, schemars::JsonSchema))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum TaskState {
    /// Computed, nothing noted yet.
    Suggested,
    /// In a research plan.
    Planned,
    /// Searched where it could be, in vain for now.
    NotFound,
    /// The record was found (its act may still be waiting for review).
    Found,
    /// Not worth searching (emigrated, already known elsewhere…).
    Ignored,
}

impl TaskState {
    fn value(self) -> &'static str {
        match self {
            TaskState::Suggested => "",
            TaskState::Planned => "PLANNED",
            TaskState::NotFound => "NOT_FOUND",
            TaskState::Found => "FOUND",
            TaskState::Ignored => "IGNORED",
        }
    }

    fn parse(v: &str) -> TaskState {
        match v.trim() {
            "PLANNED" => TaskState::Planned,
            "NOT_FOUND" => TaskState::NotFound,
            "FOUND" => TaskState::Found,
            "IGNORED" => TaskState::Ignored,
            _ => TaskState::Suggested,
        }
    }
}

/// Result of a search.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize, schemars::JsonSchema))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum SearchResult {
    /// The record was found.
    Found,
    /// Nothing for this person in what was searched.
    #[default]
    Nothing,
    /// Part of what was looked for (an index entry without the act, a mention). A record that
    /// may be about this person without proof is a clue ([`LogKind::Clue`]).
    Partial,
}

impl SearchResult {
    fn value(self) -> &'static str {
        match self {
            SearchResult::Found => "FOUND",
            SearchResult::Nothing => "NONE",
            SearchResult::Partial => "PARTIAL",
        }
    }

    fn parse(v: &str) -> SearchResult {
        match v.trim() {
            "FOUND" => SearchResult::Found,
            "PARTIAL" => SearchResult::Partial,
            _ => SearchResult::Nothing,
        }
    }
}

/// A lead's research log, as read from the file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Task {
    pub goal: Goal,
    pub state: TaskState,
    pub note: String,
    pub log: Vec<LogEntry>,
}

/// An entry to note: a search, a clue or a handoff ([`LogEntry`]).
#[derive(Debug, Clone, Default)]
pub struct NewEntry {
    pub kind: LogKind,
    pub text: String,
    pub result: Option<SearchResult>,
    pub years: Option<Years>,
    pub place: String,
    pub url: String,
    pub note: String,
    pub evidence_for: String,
    pub against: String,
    pub next: String,
    /// The AI agent noting it ([`UNKNOWN_AGENT`] when it does not say); empty: the user.
    pub agent: String,
}

impl LogKind {
    fn value(self) -> Option<&'static str> {
        match self {
            LogKind::Search => None,
            LogKind::Clue => Some("CLUE"),
            LogKind::Handoff => Some("HANDOFF"),
        }
    }

    fn parse(v: &str) -> LogKind {
        match v {
            "CLUE" => LogKind::Clue,
            "HANDOFF" => LogKind::Handoff,
            _ => LogKind::Search,
        }
    }
}

fn span_value(y: Years) -> String {
    match y.from == y.to {
        true => y.from.to_string(),
        false => format!("FROM {} TO {}", y.from, y.to),
    }
}

fn parse_span(v: &str) -> Option<Years> {
    let nums: Vec<i32> = v.split_whitespace().filter_map(|w| w.parse().ok()).collect();
    match nums.as_slice() {
        [y] => Some(Years::new(*y, *y)),
        [a, b] => Some(Years::new(*a, *b)),
        _ => None,
    }
}

fn read_task(n: &Node) -> Option<Task> {
    let goal = Goal::from_tag(n.val())?;
    let text = |l: &Node, tag: &str| l.child(tag).map(Node::text).unwrap_or_default();
    let log = n
        .children_with(LOG_TAG)
        .map(|l| LogEntry {
            kind: LogKind::parse(l.child_val(KIND_TAG).unwrap_or("")),
            date: l.child_val("DATE").unwrap_or("").to_string(),
            time: l.child("DATE").and_then(|d| d.child_val("TIME")).unwrap_or("").to_string(),
            text: text(l, "TEXT"),
            result: SearchResult::parse(l.child_val(RES_TAG).unwrap_or("")),
            years: l.child_val(SPAN_TAG).and_then(parse_span),
            place: l.child_val("PLAC").unwrap_or("").to_string(),
            url: l.child_val("WWW").unwrap_or("").to_string(),
            note: text(l, "NOTE"),
            evidence_for: text(l, FOR_TAG),
            against: text(l, AGAINST_TAG),
            next: text(l, NEXT_TAG),
            agent: l
                .child(AGENT_TAG)
                .map(|a| match a.val().trim() {
                    "" => UNKNOWN_AGENT.to_string(),
                    v => v.to_string(),
                })
                .unwrap_or_default(),
        })
        .collect();
    Some(Task {
        goal,
        state: TaskState::parse(n.child_val(STAT_TAG).unwrap_or("")),
        note: n.child("NOTE").map(Node::text).unwrap_or_default(),
        log,
    })
}

impl Gedcom {
    /// Research log of a lead: `owner` is the individual, or the family for a marriage.
    pub fn research_task(&self, owner: &str, goal: Goal) -> Option<Task> {
        self.get(owner)?.children_with(TASK_TAG).filter_map(read_task).find(|t| t.goal == goal)
    }

    /// Every research log of a record.
    pub fn research_tasks(&self, owner: &str) -> Vec<Task> {
        self.get(owner).map(|n| n.children_with(TASK_TAG).filter_map(read_task).collect()).unwrap_or_default()
    }

    fn declare_task_tags(&mut self) {
        for tag in [TASK_TAG, STAT_TAG, LOG_TAG, RES_TAG, SPAN_TAG, AGENT_TAG, KIND_TAG, FOR_TAG, AGAINST_TAG, NEXT_TAG]
        {
            self.declare_extension(tag, URI);
        }
    }

    /// The `_GR_TASK` structure of a lead, created if needed.
    fn task_mut(&mut self, owner: &str, goal: Goal) -> Option<&mut Node> {
        let rec = self.get_mut(owner)?;
        let i = match rec.children.iter().position(|c| c.tag == TASK_TAG && c.val() == goal.tag()) {
            Some(i) => i,
            None => {
                rec.insert_child(Node::with_value(TASK_TAG, goal.tag()));
                rec.children.iter().position(|c| c.tag == TASK_TAG && c.val() == goal.tag())?
            }
        };
        Some(&mut rec.children[i])
    }

    /// Sets the state of a lead, with an optional note (why it is planned or ignored; empty keeps
    /// the current one). `Suggested` removes the log if no search was noted.
    pub fn set_research_state(&mut self, owner: &str, goal: Goal, state: TaskState, note: &str) -> bool {
        if self.get(owner).is_none() {
            return false;
        }
        self.declare_task_tags();
        let Some(task) = self.task_mut(owner, goal) else { return false };
        match state {
            TaskState::Suggested => task.children.retain(|c| c.tag != STAT_TAG),
            _ => task.set_child_val(STAT_TAG, state.value()),
        }
        if !note.trim().is_empty() {
            task.set_child_text("NOTE", note);
        }
        let empty = task.children.is_empty();
        if empty && let Some(rec) = self.get_mut(owner) {
            rec.children.retain(|c| !(c.tag == TASK_TAG && c.val() == goal.tag()));
        }
        true
    }

    /// Notes an entry in a lead's log, dated now. A search that found the record sets the lead to
    /// [`TaskState::Found`]; `finished`: nothing left to search for now ([`TaskState::NotFound`]);
    /// otherwise a lead still only suggested becomes planned. A handoff leaves the state as it is.
    pub fn log_research(&mut self, owner: &str, goal: Goal, entry: &NewEntry, finished: bool) -> bool {
        if self.get(owner).is_none() || entry.text.trim().is_empty() {
            return false;
        }
        self.declare_task_tags();
        let Some(task) = self.task_mut(owner, goal) else { return false };
        let (date, time) = crate::node::now_gedcom();
        let mut log = Node::new(LOG_TAG).push(Node::with_value("DATE", date).push(Node::with_value("TIME", time)));
        let push_text = |log: &mut Node, tag: &str, v: &str| {
            if !v.trim().is_empty() {
                let mut n = Node::new(tag);
                n.set_text(v.trim());
                log.children.push(n);
            }
        };
        if let Some(k) = entry.kind.value() {
            log.children.push(Node::with_value(KIND_TAG, k));
        }
        push_text(&mut log, "TEXT", &entry.text);
        let result = entry.result.unwrap_or(SearchResult::Nothing);
        if entry.kind == LogKind::Search {
            log.children.push(Node::with_value(RES_TAG, result.value()));
        }
        if let Some(y) = entry.years {
            log.children.push(Node::with_value(SPAN_TAG, span_value(y)));
        }
        for (tag, v) in [("PLAC", &entry.place), ("WWW", &entry.url)] {
            if !v.trim().is_empty() {
                log.children.push(Node::with_value(tag, v.trim()));
            }
        }
        push_text(&mut log, "NOTE", &entry.note);
        push_text(&mut log, FOR_TAG, &entry.evidence_for);
        push_text(&mut log, AGAINST_TAG, &entry.against);
        push_text(&mut log, NEXT_TAG, &entry.next);
        if !entry.agent.trim().is_empty() {
            log.children.push(Node::with_value(AGENT_TAG, entry.agent.trim()));
        }
        task.children.push(log);
        let current = TaskState::parse(task.child_val(STAT_TAG).unwrap_or(""));
        let state = match (entry.kind, result, finished) {
            (LogKind::Handoff, _, _) => current,
            (LogKind::Search, SearchResult::Found, _) => TaskState::Found,
            (_, _, true) => TaskState::NotFound,
            _ if current == TaskState::Suggested => TaskState::Planned,
            _ => current,
        };
        if state == TaskState::Suggested {
            task.children.retain(|c| c.tag != STAT_TAG);
        } else {
            task.set_child_val(STAT_TAG, state.value());
        }
        // The state comes first, before the entries.
        if let Some(i) = task.children.iter().position(|c| c.tag == STAT_TAG) {
            let stat = task.children.remove(i);
            task.children.insert(0, stat);
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FILE: &str = "0 HEAD\n1 GEDC\n2 VERS 7.0\n1 _GR_ROOT @I1@\n0 @I1@ INDI\n1 NAME Jean /MARCHAND/\n1 BIRT\n2 DATE 1850\n2 PLAC Rennes\n1 CHAN\n2 DATE 1 JAN 2026\n0 TRLR\n";

    #[test]
    fn plan_log_and_read_back() {
        let mut g = Gedcom::parse(FILE).unwrap();
        assert!(g.research_task("I1", Goal::Death).is_none());
        assert!(g.set_research_state("I1", Goal::Death, TaskState::Planned, "Last seen in 1880"));
        let search = NewEntry {
            text: "Decennial tables of Rennes, deaths".into(),
            result: Some(SearchResult::Nothing),
            years: Some(Years::new(1883, 1892)),
            place: "Rennes".into(),
            url: "https://example.org/ark:/1".into(),
            agent: "Claude Code".into(),
            ..Default::default()
        };
        assert!(g.log_research("I1", Goal::Death, &search, false));
        let t = g.research_task("I1", Goal::Death).unwrap();
        assert_eq!(t.state, TaskState::Planned);
        assert_eq!(t.note, "Last seen in 1880");
        assert_eq!(t.log.len(), 1);
        let s = &t.log[0];
        assert_eq!(s.result, SearchResult::Nothing);
        assert_eq!(s.years, Some(Years::new(1883, 1892)));
        assert!(s.by_agent() && !s.date.is_empty() && !s.time.is_empty());
        assert_eq!(s.agent, "Claude Code");
        // Nothing left to search: not found; then found.
        let mut more = search.clone();
        more.years = Some(Years::new(1893, 1902));
        g.log_research("I1", Goal::Death, &more, true);
        assert_eq!(g.research_task("I1", Goal::Death).unwrap().state, TaskState::NotFound);
        more.result = Some(SearchResult::Found);
        g.log_research("I1", Goal::Death, &more, false);
        assert_eq!(g.research_task("I1", Goal::Death).unwrap().state, TaskState::Found);
        // Written before the change date, declared in the header, and read back after a round trip.
        let text = g.to_gedcom_string();
        assert!(text.contains("1 _GR_TASK DEAT\n2 _GR_STAT FOUND\n2 NOTE Last seen in 1880\n2 _GR_LOG\n"), "{text}");
        assert!(text.contains("3 _GR_SPAN FROM 1883 TO 1892\n"));
        assert!(text.find("_GR_TASK").unwrap() < text.find("1 CHAN").unwrap());
        assert!(text.contains("2 TAG _GR_TASK "));
        let again = Gedcom::parse(&text).unwrap();
        assert_eq!(again.research_task("I1", Goal::Death).unwrap().log.len(), 3);
    }

    #[test]
    fn suggested_again_removes_an_empty_log() {
        let mut g = Gedcom::parse(FILE).unwrap();
        g.set_research_state("I1", Goal::Birth, TaskState::Ignored, "");
        assert_eq!(g.research_task("I1", Goal::Birth).unwrap().state, TaskState::Ignored);
        g.set_research_state("I1", Goal::Birth, TaskState::Suggested, "");
        assert!(g.research_task("I1", Goal::Birth).is_none());
        assert!(!g.log_research("I9", Goal::Birth, &NewEntry { text: "x".into(), ..Default::default() }, false));
    }

    #[test]
    fn clues_and_handoffs() {
        let mut g = Gedcom::parse(FILE).unwrap();
        let clue = NewEntry {
            kind: LogKind::Clue,
            text: "Burial of Jean MARCHAND, 4 May 1858, Carentan".into(),
            url: "https://example.org/ark:/2".into(),
            evidence_for: "Same name, widower of a Rose".into(),
            against: "Aged 50, ten years younger".into(),
            agent: "Codex".into(),
            ..Default::default()
        };
        assert!(g.log_research("I1", Goal::Death, &clue, false));
        // A clue plans the lead; a handoff leaves its state.
        assert_eq!(g.research_task("I1", Goal::Death).unwrap().state, TaskState::Planned);
        let handoff = NewEntry {
            kind: LogKind::Handoff,
            text: "One clue in Carentan".into(),
            next: "Carentan deaths 1858: age and spouse".into(),
            agent: "Codex".into(),
            ..Default::default()
        };
        g.set_research_state("I1", Goal::Death, TaskState::Ignored, "");
        assert!(g.log_research("I1", Goal::Death, &handoff, false));
        let t = g.research_task("I1", Goal::Death).unwrap();
        assert_eq!(t.state, TaskState::Ignored);
        let text = g.to_gedcom_string();
        assert!(text.contains("3 _GR_KIND CLUE\n3 TEXT Burial of Jean MARCHAND"), "{text}");
        assert!(!text.contains("_GR_RES NONE"), "a clue has no search result: {text}");
        let again = Gedcom::parse(&text).unwrap().research_task("I1", Goal::Death).unwrap();
        assert_eq!(again.log[0].kind, LogKind::Clue);
        assert_eq!(
            (again.log[0].evidence_for.as_str(), again.log[0].against.as_str()),
            ("Same name, widower of a Rose", "Aged 50, ten years younger")
        );
        assert_eq!(
            (again.log[1].kind, again.log[1].next.as_str()),
            (LogKind::Handoff, "Carentan deaths 1858: age and spouse")
        );
        assert_eq!(again.log[1].agent, "Codex");
    }
}
