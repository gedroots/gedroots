//! Research log, kept in the file: what was planned and searched for each lead
//! ([`crate::leads`]), including the searches that found nothing, so that nobody (the user or an
//! agent) searches the same register twice.
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
//! 3 _GR_AGENT
//! ```

use crate::document::Gedcom;
use crate::leads::{Goal, PastSearch, Years};
use crate::node::Node;

/// Extension structure of a lead's research log.
pub const TASK_TAG: &str = "_GR_TASK";
const STAT_TAG: &str = "_GR_STAT";
const LOG_TAG: &str = "_GR_LOG";
const RES_TAG: &str = "_GR_RES";
const SPAN_TAG: &str = "_GR_SPAN";
const AGENT_TAG: &str = "_GR_AGENT";
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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize, schemars::JsonSchema))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum SearchResult {
    /// The record was found.
    Found,
    /// Nothing for this person in what was searched.
    Nothing,
    /// A clue, not the record itself (a mention, an index entry without the act…).
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
    pub searches: Vec<PastSearch>,
}

/// A search to note.
#[derive(Debug, Clone, Default)]
pub struct NewSearch {
    pub searched: String,
    pub result: Option<SearchResult>,
    pub years: Option<Years>,
    pub place: String,
    pub url: String,
    pub note: String,
    pub by_agent: bool,
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
    let searches = n
        .children_with(LOG_TAG)
        .map(|l| PastSearch {
            date: l.child_val("DATE").unwrap_or("").to_string(),
            searched: l.child("TEXT").map(Node::text).unwrap_or_default(),
            result: SearchResult::parse(l.child_val(RES_TAG).unwrap_or("")),
            years: l.child_val(SPAN_TAG).and_then(parse_span),
            place: l.child_val("PLAC").unwrap_or("").to_string(),
            url: l.child_val("WWW").unwrap_or("").to_string(),
            note: l.child("NOTE").map(Node::text).unwrap_or_default(),
            by_agent: l.child(AGENT_TAG).is_some(),
        })
        .collect();
    Some(Task {
        goal,
        state: TaskState::parse(n.child_val(STAT_TAG).unwrap_or("")),
        note: n.child("NOTE").map(Node::text).unwrap_or_default(),
        searches,
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
        for tag in [TASK_TAG, STAT_TAG, LOG_TAG, RES_TAG, SPAN_TAG, AGENT_TAG] {
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

    /// Notes a search in a lead's log, dated today. A found record sets the lead to
    /// [`TaskState::Found`]; `finished`: nothing left to search for now ([`TaskState::NotFound`]);
    /// otherwise a lead still only suggested becomes planned.
    pub fn log_research(&mut self, owner: &str, goal: Goal, search: &NewSearch, finished: bool) -> bool {
        if self.get(owner).is_none() || search.searched.trim().is_empty() {
            return false;
        }
        self.declare_task_tags();
        let Some(task) = self.task_mut(owner, goal) else { return false };
        let mut log = Node::new(LOG_TAG).push(Node::with_value("DATE", crate::node::now_gedcom().0));
        let mut text = Node::new("TEXT");
        text.set_text(search.searched.trim());
        log.children.push(text);
        let result = search.result.unwrap_or(SearchResult::Nothing);
        log.children.push(Node::with_value(RES_TAG, result.value()));
        if let Some(y) = search.years {
            log.children.push(Node::with_value(SPAN_TAG, span_value(y)));
        }
        for (tag, v) in [("PLAC", &search.place), ("WWW", &search.url)] {
            if !v.trim().is_empty() {
                log.children.push(Node::with_value(tag, v.trim()));
            }
        }
        if !search.note.trim().is_empty() {
            let mut note = Node::new("NOTE");
            note.set_text(search.note.trim());
            log.children.push(note);
        }
        if search.by_agent {
            log.children.push(Node::new(AGENT_TAG));
        }
        task.children.push(log);
        let current = TaskState::parse(task.child_val(STAT_TAG).unwrap_or(""));
        let state = match (result, finished) {
            (SearchResult::Found, _) => TaskState::Found,
            (_, true) => TaskState::NotFound,
            _ if current == TaskState::Suggested => TaskState::Planned,
            _ => current,
        };
        task.set_child_val(STAT_TAG, state.value());
        // The state comes first, before the searches.
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
        let search = NewSearch {
            searched: "Decennial tables of Rennes, deaths".into(),
            result: Some(SearchResult::Nothing),
            years: Some(Years::new(1883, 1892)),
            place: "Rennes".into(),
            url: "https://example.org/ark:/1".into(),
            by_agent: true,
            ..Default::default()
        };
        assert!(g.log_research("I1", Goal::Death, &search, false));
        let t = g.research_task("I1", Goal::Death).unwrap();
        assert_eq!(t.state, TaskState::Planned);
        assert_eq!(t.note, "Last seen in 1880");
        assert_eq!(t.searches.len(), 1);
        let s = &t.searches[0];
        assert_eq!(s.result, SearchResult::Nothing);
        assert_eq!(s.years, Some(Years::new(1883, 1892)));
        assert!(s.by_agent && !s.date.is_empty());
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
        assert_eq!(again.research_task("I1", Goal::Death).unwrap().searches.len(), 3);
    }

    #[test]
    fn suggested_again_removes_an_empty_log() {
        let mut g = Gedcom::parse(FILE).unwrap();
        g.set_research_state("I1", Goal::Birth, TaskState::Ignored, "");
        assert_eq!(g.research_task("I1", Goal::Birth).unwrap().state, TaskState::Ignored);
        g.set_research_state("I1", Goal::Birth, TaskState::Suggested, "");
        assert!(g.research_task("I1", Goal::Birth).is_none());
        assert!(!g.log_research("I9", Goal::Birth, &NewSearch { searched: "x".into(), ..Default::default() }, false));
    }
}
