//! GEDRoots' MCP prompts: the agent's working methods, served by the app (Claude Code shows them as
//! `/mcp__gedroots__research`… commands). The text is in `prompts/*.md`.

use super::tools::GedrootsMcp;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{PromptMessage, Role};
use rmcp::{prompt, prompt_router};
use schemars::JsonSchema;
use serde::Deserialize;

/// How to read an act and propose it: in `read_record`, and in `research`, whose agent cannot read
/// another prompt.
const READ_ACT: &str = include_str!("prompts/read_act.md");
const READ_RECORD: &str = include_str!("prompts/read_record.md");
const RESEARCH: &str = include_str!("prompts/research.md");
const REVIEW_QUALITY: &str = include_str!("prompts/review_quality.md");
const WRITE_BOOK: &str = include_str!("prompts/write_book.md");

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ResearchArgs {
    /// Person to search for (name), or nothing to carry on where the research stands.
    pub person: Option<String>,
    /// Record to find: birth, marriage, death…
    pub event: Option<String>,
    /// People to research, in the syntax of GEDRoots' search (`sosa:1-15`, `place:manche`,
    /// `descendants:@I12@`…); nothing: the ancestors over six generations.
    pub focus: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ReadArgs {
    /// Path of the downloaded image or PDF.
    pub file: Option<String>,
    /// Person the act is about.
    pub person: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct QualityArgs {
    /// Category to work on (dates, family, structure, empty, media, places, sources, files, coords),
    /// or nothing for all.
    pub category: Option<String>,
    /// Person whose issues to settle (name), or nothing for the whole tree.
    pub person: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct BookArgs {
    /// Book to write (its title), or nothing for the tree's first book.
    pub book: Option<String>,
    /// Chapter to start with (generation number), or nothing for the first one.
    pub chapter: Option<String>,
}

fn text(template: &str, target: String) -> Vec<PromptMessage> {
    let text = template.replace("{read_act}", READ_ACT.trim()).replace("{target}", target.trim());
    vec![PromptMessage::new_text(Role::User, text)]
}

fn some(s: &Option<String>) -> Option<&str> {
    s.as_deref().map(str::trim).filter(|s| !s.is_empty())
}

#[prompt_router(vis = "pub(super)")]
impl GedrootsMcp {
    #[prompt(
        name = "research",
        description = "Find birth, marriage and death records in the online archives for the tree open in GEDRoots, carrying on from where the research stands and noting every search."
    )]
    async fn research(&self, Parameters(a): Parameters<ResearchArgs>) -> Vec<PromptMessage> {
        let target = [
            match (some(&a.person), some(&a.event)) {
                (Some(p), Some(e)) => Some(format!("Record to find: the {e} of {p}.")),
                (Some(p), None) => Some(format!("Person: {p}. Look for the records missing on their sheet.")),
                (None, Some(e)) => Some(format!("Record to find: a missing {e} among my ancestors.")),
                (None, None) => None,
            },
            some(&a.focus).map(|f| format!("Focus: `{f}`.")),
        ]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join("\n");
        text(RESEARCH, target)
    }

    #[prompt(
        name = "read_record",
        description = "Read a downloaded act (image or PDF), check it against the tree and propose it to GEDRoots for review."
    )]
    async fn read_record(&self, Parameters(a): Parameters<ReadArgs>) -> Vec<PromptMessage> {
        let target = [some(&a.file).map(|f| format!("File: {f}")), some(&a.person).map(|p| format!("Person: {p}"))]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>()
            .join("\n");
        text(READ_RECORD, target)
    }

    #[prompt(
        name = "review_quality",
        description = "Go through the Data Quality issues of the tree open in GEDRoots and propose a fix, an edit or a reasoned hide for each one."
    )]
    async fn review_quality(&self, Parameters(a): Parameters<QualityArgs>) -> Vec<PromptMessage> {
        let target = [
            some(&a.category).map(|c| format!("Category: {c}.")),
            some(&a.person).map(|p| format!("Only the issues about {p}.")),
        ]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join("\n");
        text(REVIEW_QUALITY, target)
    }

    #[prompt(
        name = "write_book",
        description = "Write the narratives, historical context and hypotheses of a family book of the tree open in GEDRoots, chapter by chapter, for review."
    )]
    async fn write_book(&self, Parameters(a): Parameters<BookArgs>) -> Vec<PromptMessage> {
        let target = [
            some(&a.book).map(|b| format!("Book: {b}.")),
            some(&a.chapter).map(|c| format!("Start with chapter {c}.")),
        ]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join("\n");
        text(WRITE_BOOK, target)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_research_prompt_carries_the_act_reading_method() {
        for template in [RESEARCH, READ_RECORD] {
            let message = text(template, "Person: Jean MARCHAND".into());
            let body = serde_json::to_string(&message).unwrap();
            assert!(!body.contains("{read_act}") && !body.contains("{target}"), "{body}");
            assert!(body.contains("faithful transcription") && body.contains("search_people"), "{body}");
        }
    }
}
