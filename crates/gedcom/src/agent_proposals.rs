//! Proposals of AI agents, kept in the file: those waiting in To Review, so that they survive a
//! restart and another agent sees them, and what the user did with the others, so that any agent
//! knows what was saved, rejected or corrected without having to remember it.
//!
//! One `_GR_PROP` record per proposal, documented in `docs/extensions.md`:
//!
//! ```text
//! 0 @GRP3@ _GR_PROP P3
//! 1 _GR_KIND ACT
//! 1 DATE 5 NOV 2026
//! 2 TIME 21:47:03
//! 1 TEXT Marriage of Johann SCHANDL and Maria RIEDAUER, 22 April 1727, Fallbach
//! 1 _GR_ABOUT @I12@
//! 1 _GR_RES WAITING
//! 1 _GR_AGENT Claude Code
//! 1 _GR_DATA {"event":"MARR",…}
//! ```
//!
//! `_GR_DATA` is the agent's request, as it sent it, kept only while the proposal waits: GEDRoots
//! queues it again when the file opens. Once decided, the record keeps its summary and outcome.

use crate::document::Gedcom;
use crate::node::Node;
use std::sync::Arc;

pub const PROP_TAG: &str = "_GR_PROP";
const KIND_TAG: &str = "_GR_KIND";
const ABOUT_TAG: &str = "_GR_ABOUT";
const RES_TAG: &str = "_GR_RES";
const AGENT_TAG: &str = "_GR_AGENT";
const DATA_TAG: &str = "_GR_DATA";
const URI: &str = extension_uri!("_gr_prop");
/// `_GR_KIND`, `_GR_RES` and `_GR_AGENT` are shared with the research log.
const TASK_URI: &str = extension_uri!("_gr_task");
/// Prefix of the records' identifiers (`@GRP3@` holds proposal `P3`).
const XREF_PREFIX: &str = "GR";

/// What an agent proposed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize, schemars::JsonSchema))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum ProposalKind {
    /// A record found in the archives (`propose_act`).
    #[default]
    Act,
    /// A correction or a Data Quality change (`propose_change`).
    Change,
    /// A chapter of a family book (`propose_chapter`).
    Chapter,
}

impl ProposalKind {
    fn value(self) -> &'static str {
        match self {
            ProposalKind::Act => "ACT",
            ProposalKind::Change => "CHANGE",
            ProposalKind::Chapter => "CHAPTER",
        }
    }

    fn parse(v: &str) -> ProposalKind {
        match v {
            "CHANGE" => ProposalKind::Change,
            "CHAPTER" => ProposalKind::Chapter,
            _ => ProposalKind::Act,
        }
    }
}

/// What became of a proposal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize, schemars::JsonSchema))]
#[cfg_attr(feature = "serde", serde(rename_all = "lowercase"))]
pub enum Outcome {
    /// In To Review, waiting for the user.
    #[default]
    Waiting,
    /// Saved to the tree by the user.
    Saved,
    /// Rejected by the user: nothing was written.
    Rejected,
    /// Opened in record entry by the user, to correct it before saving.
    Edited,
    /// Withdrawn by the agent.
    Withdrawn,
    /// Replaced by a corrected proposal.
    Replaced,
}

impl Outcome {
    fn value(self) -> &'static str {
        match self {
            Outcome::Waiting => "WAITING",
            Outcome::Saved => "SAVED",
            Outcome::Rejected => "REJECTED",
            Outcome::Edited => "EDITED",
            Outcome::Withdrawn => "WITHDRAWN",
            Outcome::Replaced => "REPLACED",
        }
    }

    fn parse(v: &str) -> Outcome {
        match v {
            "SAVED" => Outcome::Saved,
            "REJECTED" => Outcome::Rejected,
            "EDITED" => Outcome::Edited,
            "WITHDRAWN" => Outcome::Withdrawn,
            "REPLACED" => Outcome::Replaced,
            _ => Outcome::Waiting,
        }
    }
}

/// A proposal as kept in the file.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct StoredProposal {
    /// As given to the agent: `P3`.
    pub id: String,
    pub kind: ProposalKind,
    /// Day and time it was sent (GEDCOM date, UTC time).
    pub date: String,
    pub time: String,
    /// The agent that sent it ([`crate::research_log::UNKNOWN_AGENT`] when it did not say).
    pub agent: String,
    /// One line the user and agents recognize it by (“Marriage of Johann SCHANDL…”).
    pub title: String,
    /// The people (or families) it is about, in the tree.
    pub about: Vec<String>,
    pub outcome: Outcome,
    /// The user's reason, or the proposal that replaced it.
    pub note: String,
    /// The agent's request (JSON), kept while it waits.
    pub data: String,
}

impl StoredProposal {
    fn read(n: &Node) -> StoredProposal {
        let date = n.child("DATE");
        StoredProposal {
            id: n.val().to_string(),
            kind: ProposalKind::parse(n.child_val(KIND_TAG).unwrap_or("")),
            date: date.map(|d| d.val().to_string()).unwrap_or_default(),
            time: date.and_then(|d| d.child_val("TIME")).unwrap_or("").to_string(),
            agent: n.child_val(AGENT_TAG).unwrap_or("").to_string(),
            title: n.child("TEXT").map(Node::text).unwrap_or_default(),
            about: n.children_with(ABOUT_TAG).map(|a| a.val().trim_matches('@').to_string()).collect(),
            outcome: Outcome::parse(n.child_val(RES_TAG).unwrap_or("")),
            note: n.child("NOTE").map(Node::text).unwrap_or_default(),
            data: n.child(DATA_TAG).map(Node::text).unwrap_or_default(),
        }
    }

    fn write(&self) -> Node {
        let mut n = Node::with_value(PROP_TAG, self.id.as_str()).with_xref(xref(&self.id));
        let text = |tag: &str, v: &str| {
            let mut t = Node::new(tag);
            t.set_text(v);
            t
        };
        n.children.push(Node::with_value(KIND_TAG, self.kind.value()));
        let mut date = Node::with_value("DATE", self.date.as_str());
        if !self.time.is_empty() {
            date.children.push(Node::with_value("TIME", self.time.as_str()));
        }
        n.children.push(date);
        if !self.title.trim().is_empty() {
            n.children.push(text("TEXT", self.title.trim()));
        }
        for x in &self.about {
            n.children.push(Node::with_value(ABOUT_TAG, format!("@{}@", x.trim_matches('@'))));
        }
        n.children.push(Node::with_value(RES_TAG, self.outcome.value()));
        if !self.agent.trim().is_empty() {
            n.children.push(Node::with_value(AGENT_TAG, self.agent.trim()));
        }
        if !self.note.trim().is_empty() {
            n.children.push(text("NOTE", self.note.trim()));
        }
        if self.outcome == Outcome::Waiting && !self.data.is_empty() {
            n.children.push(text(DATA_TAG, &self.data));
        }
        n
    }
}

fn xref(id: &str) -> String {
    format!("{XREF_PREFIX}{id}")
}

impl Gedcom {
    /// Every proposal kept in the file, oldest first.
    pub fn agent_proposals(&self) -> Vec<StoredProposal> {
        self.records.iter().filter(|r| r.tag == PROP_TAG).map(|r| StoredProposal::read(r)).collect()
    }

    /// The proposal `id` (`P3`).
    pub fn agent_proposal(&self, id: &str) -> Option<StoredProposal> {
        self.get(&xref(id)).filter(|r| r.tag == PROP_TAG).map(StoredProposal::read)
    }

    /// Number of the next proposal: one more than the highest kept in the file.
    pub fn next_proposal_number(&self) -> u64 {
        self.records
            .iter()
            .filter(|r| r.tag == PROP_TAG)
            .filter_map(|r| r.val().strip_prefix('P')?.parse::<u64>().ok())
            .max()
            .unwrap_or(0)
            + 1
    }

    fn declare_proposal_tags(&mut self) {
        for tag in [PROP_TAG, ABOUT_TAG, DATA_TAG] {
            self.declare_extension(tag, URI);
        }
        for tag in [KIND_TAG, RES_TAG, AGENT_TAG] {
            self.declare_extension(tag, TASK_URI);
        }
    }

    /// Keeps a proposal, replacing the one with the same id; its date is now when it has none.
    pub fn store_agent_proposal(&mut self, p: &StoredProposal) {
        self.declare_proposal_tags();
        let mut p = p.clone();
        // Only records of the tree: a place (named, not a record) or a removed record would be a
        // broken link.
        let mut seen = std::collections::HashSet::new();
        p.about.retain(|x| {
            let x = x.trim_matches('@');
            self.get(x).is_some_and(|r| r.tag != PROP_TAG) && seen.insert(x.to_string())
        });
        if p.date.is_empty() {
            (p.date, p.time) = crate::node::now_gedcom();
        }
        let node = p.write();
        // Unchanged records stay shared, so that the tree still counts as settled.
        if self.get(&xref(&p.id)).is_some_and(|n| *n == node) {
            return;
        }
        match self.get_mut(&xref(&p.id)) {
            Some(existing) => *existing = node,
            None => {
                self.records.push(Arc::new(node));
                self.reindex();
            }
        }
    }

    /// Gives this tree the proposals of `from`, as they are there: they are not part of what Undo,
    /// Redo or a reload bring back, which only concern the tree's data.
    pub fn carry_agent_proposals(&mut self, from: &Gedcom) {
        let theirs: Vec<Arc<Node>> = from.records.iter().filter(|r| r.tag == PROP_TAG).cloned().collect();
        let ours = self.records.iter().filter(|r| r.tag == PROP_TAG);
        if ours.clone().count() == theirs.len() && ours.zip(&theirs).all(|(a, b)| Arc::ptr_eq(a, b)) {
            return;
        }
        self.records.retain(|r| r.tag != PROP_TAG);
        self.records.extend(theirs);
        self.reindex();
        if self.records.iter().any(|r| r.tag == PROP_TAG) {
            self.declare_proposal_tags();
        }
    }

    /// Replaces a temporary person id in the requests of the waiting proposals, once the person is
    /// saved to the tree: queued again when the file opens, they link the saved person.
    pub fn relink_agent_proposals(&mut self, temp: &str, xref: &str) {
        let quoted = format!("\"{temp}\"");
        for p in self.agent_proposals() {
            if p.outcome == Outcome::Waiting && p.data.contains(&quoted) {
                let data = p.data.replace(&quoted, &format!("\"{}\"", xref.trim_matches('@')));
                self.store_agent_proposal(&StoredProposal { data, ..p });
            }
        }
    }

    /// Records what became of a proposal (the request is dropped once decided); false when the
    /// file has no such proposal.
    pub fn decide_agent_proposal(&mut self, id: &str, outcome: Outcome, note: &str) -> bool {
        let Some(mut p) = self.agent_proposal(id) else { return false };
        p.outcome = outcome;
        if !note.trim().is_empty() {
            p.note = note.trim().to_string();
        }
        if outcome != Outcome::Waiting {
            p.data.clear();
        }
        self.store_agent_proposal(&p);
        true
    }

    /// The proposals about a person or family, oldest first.
    pub fn agent_proposals_about(&self, xref: &str) -> Vec<StoredProposal> {
        let x = xref.trim_matches('@');
        self.agent_proposals().into_iter().filter(|p| p.about.iter().any(|a| a == x)).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FILE: &str = "0 HEAD\n1 GEDC\n2 VERS 7.0\n0 @I1@ INDI\n1 NAME Johann /SCHANDL/\n0 TRLR\n";

    #[test]
    fn waiting_then_decided() {
        let mut g = Gedcom::parse(FILE).unwrap();
        assert_eq!(g.next_proposal_number(), 1);
        let p = StoredProposal {
            id: "P1".into(),
            title: "Marriage of Johann SCHANDL, 22 April 1727, Fallbach".into(),
            about: vec!["I1".into()],
            agent: "Claude Code".into(),
            data: "{\"event\":\"MARR\",\n\"date\":\"22 APR 1727\"}".into(),
            ..Default::default()
        };
        g.store_agent_proposal(&p);
        assert_eq!(g.next_proposal_number(), 2);
        let text = g.to_gedcom_string();
        assert!(text.contains("0 @GRP1@ _GR_PROP P1\n1 _GR_KIND ACT\n1 DATE "), "{text}");
        assert!(text.contains("1 _GR_ABOUT @I1@\n1 _GR_RES WAITING\n1 _GR_AGENT Claude Code\n1 _GR_DATA {\"event\":\"MARR\",\n2 CONT \"date\""), "{text}");
        assert!(text.contains("2 TAG _GR_PROP https://"), "{text}");
        let again = Gedcom::parse(&text).unwrap();
        let read = again.agent_proposal("P1").unwrap();
        assert_eq!(
            (read.data.as_str(), read.outcome, read.about.clone()),
            (p.data.as_str(), Outcome::Waiting, vec!["I1".to_string()])
        );
        assert!(!read.time.is_empty());
        // Decided: the request goes, the summary and the user's reason stay.
        let mut g = again;
        assert!(g.decide_agent_proposal("P1", Outcome::Rejected, "Wrong Johann: he was born in 1726"));
        let read = g.agent_proposal("P1").unwrap();
        assert_eq!(
            (read.outcome, read.data.as_str(), read.note.as_str()),
            (Outcome::Rejected, "", "Wrong Johann: he was born in 1726")
        );
        assert_eq!(g.agent_proposals_about("@I1@").len(), 1);
        assert!(!g.decide_agent_proposal("P9", Outcome::Saved, ""));
    }

    #[test]
    fn about_only_records_of_the_tree() {
        // A proposal to merge two places names them, but places are not records: they must not
        // become links to a missing record.
        let mut g = Gedcom::parse(FILE).unwrap();
        let p = StoredProposal {
            id: "P1".into(),
            about: vec![
                ", Agde, Hérault, Occitanie, France".into(),
                "I1".into(),
                "@I1@".into(),
                "I9".into(),
                "".into(),
            ],
            ..Default::default()
        };
        g.store_agent_proposal(&p);
        assert_eq!(g.agent_proposal("P1").unwrap().about, vec!["I1".to_string()]);
        let issues = crate::analysis::check::check(&g, &Default::default());
        assert!(!issues.iter().any(|i| i.code == "broken_link"), "{issues:?}");
        // Files written before: the Quality page's fix removes these links and keeps the proposal.
        let old = "0 HEAD\n1 GEDC\n2 VERS 7.0\n0 @I1@ INDI\n0 @GRP1@ _GR_PROP P1\n1 _GR_KIND CHANGE\n\
                   1 _GR_ABOUT @, Agde, Hérault, Occitanie, France@\n1 _GR_ABOUT @I1@\n1 _GR_RES SAVED\n0 TRLR\n";
        let mut g = Gedcom::parse(old).unwrap();
        let broken: Vec<_> = crate::quality::run(&g).into_iter().filter(|i| i.code == "broken_link").collect();
        assert_eq!(broken.len(), 1);
        assert_eq!(g.fix_issues(&broken), 1);
        assert_eq!(g.agent_proposal("P1").unwrap().about, vec!["I1".to_string()]);
    }

    #[test]
    fn kept_through_undo_and_relinked() {
        let mut g = Gedcom::parse(FILE).unwrap();
        let before = g.clone();
        let p =
            StoredProposal { id: "P1".into(), data: "{\"people\":[{\"id\":\"new-2\"}]}".into(), ..Default::default() };
        g.store_agent_proposal(&p);
        // Read back at once, without parsing the file again.
        assert!(g.agent_proposal("P1").is_some());
        // Storing it again unchanged keeps the record shared.
        let stored = g.agent_proposal("P1").unwrap();
        let rec = g.records.last().unwrap().clone();
        g.store_agent_proposal(&stored);
        assert!(Arc::ptr_eq(&rec, g.records.last().unwrap()));
        // Undo brings back a tree without it: it is carried over.
        let mut undone = before.clone();
        undone.carry_agent_proposals(&g);
        assert_eq!(undone.agent_proposal("P1").map(|p| p.data), Some(p.data.clone()));
        assert!(undone.to_gedcom_string().contains("2 TAG _GR_PROP "));
        undone.relink_agent_proposals("new-2", "@I7@");
        assert_eq!(undone.agent_proposal("P1").unwrap().data, "{\"people\":[{\"id\":\"I7\"}]}");
    }
}
