//! GEDRoots' MCP prompts: the agent's working methods, served by the app (Claude Code shows them as
//! `/mcp__gedroots__find_record`… commands). The text is in `prompts/*.md`.

use super::tools::GedrootsMcp;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{PromptMessage, Role};
use rmcp::{prompt, prompt_router};
use schemars::JsonSchema;
use serde::Deserialize;

const FIND_RECORD: &str = include_str!("prompts/find_record.md");
const READ_RECORD: &str = include_str!("prompts/read_record.md");

#[derive(Debug, Deserialize, JsonSchema)]
pub struct FindArgs {
    /// Person to search for (name, or nothing to start from the research gaps).
    pub person: Option<String>,
    /// Record to find: birth, marriage, death…
    pub event: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ReadArgs {
    /// Path of the downloaded image or PDF.
    pub file: Option<String>,
    /// Person the act is about.
    pub person: Option<String>,
}

fn text(template: &str, target: String) -> Vec<PromptMessage> {
    vec![PromptMessage::new_text(Role::User, template.replace("{target}", target.trim()))]
}

fn some(s: &Option<String>) -> Option<&str> {
    s.as_deref().map(str::trim).filter(|s| !s.is_empty())
}

#[prompt_router(vis = "pub(super)")]
impl GedrootsMcp {
    #[prompt(
        name = "find_record",
        description = "Find a birth, marriage or death record in the online archives for the tree open in GEDRoots."
    )]
    async fn find_record(&self, Parameters(a): Parameters<FindArgs>) -> Vec<PromptMessage> {
        let target = match (some(&a.person), some(&a.event)) {
            (Some(p), Some(e)) => format!("Record to find: the {e} of {p}."),
            (Some(p), None) => format!("Person: {p}. Look for the records missing on their sheet."),
            (None, Some(e)) => format!("Record to find: a missing {e} among my ancestors."),
            (None, None) => String::new(),
        };
        text(FIND_RECORD, target)
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
}
