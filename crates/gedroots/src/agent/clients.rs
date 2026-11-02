//! Known agents and the shortest way to connect each one to the MCP server: terminal command,
//! editor install link, block to paste into a configuration file, or address and header for another
//! client.

use base64::Engine;
use std::path::PathBuf;

/// Name of the server in the agents' configuration.
const NAME: &str = "gedroots";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Client {
    ClaudeCode,
    Codex,
    Gemini,
    ClaudeDesktop,
    Cursor,
    VsCode,
    Other,
}

/// Family of agents, to group the list.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Terminal,
    App,
    Editor,
    Other,
}

/// Comment brancher l'agent.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Setup {
    /// Command to run in a terminal.
    Command(String),
    /// Block to add to the `file` configuration file.
    Config { file: String, text: String },
    /// Link that opens the editor on its install, with the configuration as a fallback.
    Link { url: String, file: String, text: String },
    /// Address and header to enter in the client.
    Generic,
}

impl Client {
    pub const ALL: [Client; 7] = [
        Client::ClaudeCode,
        Client::Codex,
        Client::Gemini,
        Client::ClaudeDesktop,
        Client::Cursor,
        Client::VsCode,
        Client::Other,
    ];

    /// Identifier saved in the machine's setting.
    pub fn id(self) -> &'static str {
        match self {
            Client::ClaudeCode => "claude-code",
            Client::Codex => "codex",
            Client::Gemini => "gemini",
            Client::ClaudeDesktop => "claude-desktop",
            Client::Cursor => "cursor",
            Client::VsCode => "vscode",
            Client::Other => "other",
        }
    }

    pub fn from_id(id: &str) -> Option<Client> {
        Client::ALL.into_iter().find(|c| c.id() == id)
    }

    /// Product name (not translated), except “Other MCP Client”, translated by the caller.
    pub fn product(self) -> Option<&'static str> {
        Some(match self {
            Client::ClaudeCode => "Claude Code",
            Client::Codex => "Codex",
            Client::Gemini => "Gemini CLI",
            Client::ClaudeDesktop => "Claude Desktop",
            Client::Cursor => "Cursor",
            Client::VsCode => "VS Code",
            Client::Other => return None,
        })
    }

    pub fn kind(self) -> Kind {
        match self {
            Client::ClaudeCode | Client::Codex | Client::Gemini => Kind::Terminal,
            Client::ClaudeDesktop => Kind::App,
            Client::Cursor | Client::VsCode => Kind::Editor,
            Client::Other => Kind::Other,
        }
    }

    /// Does the agent show MCP prompts as `/mcp__gedroots__…` commands?
    pub fn shows_prompts(self) -> bool {
        self == Client::ClaudeCode
    }

    /// Agent from the name it gives when opening the MCP session (`clientInfo.name`).
    pub fn from_client_info(name: &str) -> Client {
        let n = name.to_lowercase();
        if n.contains("claude-code") || n.contains("claude code") {
            Client::ClaudeCode
        } else if n.contains("codex") {
            Client::Codex
        } else if n.contains("gemini") {
            Client::Gemini
        } else if n.contains("cursor") {
            Client::Cursor
        } else if n.contains("visual studio code") || n.contains("vscode") || n.contains("vs code") {
            Client::VsCode
        } else if n.contains("claude") || n.contains("mcp-remote") {
            Client::ClaudeDesktop
        } else {
            Client::Other
        }
    }

    /// Steps to follow, with the real token (the display masks it).
    pub fn setup(self, url: &str, token: &str) -> Setup {
        let auth = format!("Bearer {token}");
        let header = format!("Authorization: {auth}");
        match self {
            Client::ClaudeCode => {
                Setup::Command(format!("claude mcp add --transport http {NAME} {url} --header \"{header}\""))
            }
            Client::Gemini => {
                Setup::Command(format!("gemini mcp add --transport http {NAME} {url} --header \"{header}\""))
            }
            Client::Codex => Setup::Config {
                file: "~/.codex/config.toml".into(),
                text: format!(
                    "[mcp_servers.{NAME}]\nurl = \"{url}\"\nhttp_headers = {{ \"Authorization\" = \"{auth}\" }}"
                ),
            },
            Client::ClaudeDesktop => {
                let cfg = serde_json::json!({ "mcpServers": { NAME: {
                    "command": "npx",
                    "args": ["-y", "mcp-remote", url, "--header", header],
                }}});
                Setup::Config { file: "claude_desktop_config.json".into(), text: pretty(&cfg) }
            }
            Client::Cursor => {
                let server = serde_json::json!({ "url": url, "headers": { "Authorization": auth } });
                let config = base64::engine::general_purpose::STANDARD.encode(server.to_string());
                Setup::Link {
                    url: format!(
                        "cursor://anysphere.cursor-deeplink/mcp/install?name={NAME}&config={}",
                        percent(&config)
                    ),
                    file: "~/.cursor/mcp.json".into(),
                    text: pretty(&serde_json::json!({ "mcpServers": { NAME: server } })),
                }
            }
            Client::VsCode => {
                let server = serde_json::json!({ "type": "http", "url": url, "headers": { "Authorization": auth } });
                let mut named = server.clone();
                named["name"] = NAME.into();
                Setup::Link {
                    url: format!("vscode:mcp/install?{}", percent(&named.to_string())),
                    file: "mcp.json".into(),
                    text: pretty(&serde_json::json!({ "servers": { NAME: server } })),
                }
            }
            Client::Other => Setup::Generic,
        }
    }

    /// Configuration file to show in the file manager, if it exists (Claude Desktop: macOS and
    /// Windows).
    pub fn config_file(self) -> Option<PathBuf> {
        match self {
            Client::ClaudeDesktop => {
                let dir = dirs::config_dir()?.join("Claude");
                dir.is_dir().then(|| dir.join("claude_desktop_config.json"))
            }
            _ => None,
        }
    }
}

fn pretty(v: &serde_json::Value) -> String {
    serde_json::to_string_pretty(v).unwrap_or_default()
}

/// URL encoding (RFC 3986): everything except unreserved characters.
fn percent(s: &str) -> String {
    let mut out = String::with_capacity(s.len() * 3);
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// Replaces the token with dots for display.
pub fn mask(text: &str, token: &str) -> String {
    if token.is_empty() { text.to_string() } else { text.replace(token, "••••••••") }
}

#[cfg(test)]
mod tests {
    use super::*;

    const URL: &str = "http://127.0.0.1:47280/mcp";

    #[test]
    fn every_client_has_a_setup_with_the_token() {
        for c in Client::ALL {
            assert_eq!(Client::from_id(c.id()), Some(c));
            match c.setup(URL, "tok") {
                Setup::Command(s) => assert!(s.contains(URL) && s.contains("Authorization: Bearer tok"), "{s}"),
                Setup::Config { text, .. } | Setup::Link { text, .. } => {
                    assert!(text.contains(URL) && text.contains("Bearer tok"), "{text}")
                }
                Setup::Generic => assert_eq!(c, Client::Other),
            }
        }
    }

    #[test]
    fn editor_links_carry_the_server() {
        let Setup::Link { url, .. } = Client::Cursor.setup(URL, "tok") else { panic!() };
        let config = url.split("config=").nth(1).unwrap().replace("%3D", "=").replace("%2B", "+").replace("%2F", "/");
        let json = String::from_utf8(base64::engine::general_purpose::STANDARD.decode(config).unwrap()).unwrap();
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["url"], URL);
        assert_eq!(v["headers"]["Authorization"], "Bearer tok");
        let Setup::Link { url, .. } = Client::VsCode.setup(URL, "tok") else { panic!() };
        assert!(url.starts_with("vscode:mcp/install?%7B"), "{url}");
        assert!(!url.contains(' ') && !url.contains('"'));
    }

    #[test]
    fn recognizes_clients_and_masks_token() {
        assert_eq!(Client::from_client_info("claude-code"), Client::ClaudeCode);
        assert_eq!(Client::from_client_info("codex-mcp-client"), Client::Codex);
        assert_eq!(Client::from_client_info("Visual Studio Code"), Client::VsCode);
        assert_eq!(Client::from_client_info("mcp-remote"), Client::ClaudeDesktop);
        assert_eq!(Client::from_client_info("my-agent"), Client::Other);
        assert_eq!(mask("Bearer abc", "abc"), "Bearer ••••••••");
    }
}
