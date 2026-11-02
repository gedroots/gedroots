//! File › Connect an AI Agent… (Fichier › Connecter un agent IA…) : active le serveur MCP local et donne la commande à
//! coller dans Claude Code.

use super::add::AddDialog;
use super::*;
use crate::agent::{self, Status};
use gpui_kit::component::switch::Switch;
use rust_i18n::t;

/// Fenêtre « Agents IA ».
pub(super) struct AgentDialog {
    enabled: bool,
    token: String,
}

impl Workspace {
    pub(super) fn open_agent_dialog(&mut self, cx: &mut Context<Self>) {
        let s = agent::load();
        self.add_dialog = Some(AddDialog::Agent(Box::new(AgentDialog { enabled: s.enabled, token: s.token })));
        cx.notify();
    }

    /// Copie du document, pour le serveur MCP.
    pub(crate) fn agent_snapshot(&self) -> Option<agent::Snapshot> {
        let doc = self.doc.as_ref()?;
        Some(agent::Snapshot { path: doc.path.clone(), ged: doc.ged.clone() })
    }

    pub(super) fn agent_body(&self, d: &AgentDialog, cx: &mut Context<Self>) -> AnyElement {
        let command = agent::claude_command(&d.token);
        let toggle = Switch::new("agent-enabled")
            .checked(d.enabled)
            .label(SharedString::from(t!("agent.allow")))
            .on_click(cx.listener(|this, on: &bool, _, cx| {
                let s = agent::set_enabled(*on);
                if let Some(AddDialog::Agent(d)) = &mut this.add_dialog {
                    d.enabled = s.enabled;
                }
                cx.notify();
            }));
        let failure = match agent::status() {
            Status::Failed(e) if d.enabled => Some(e),
            _ => None,
        };
        v_flex()
            .gap_3()
            .child(toggle)
            .when_some(failure, |v, e| v.child(div().text_sm().text_color(theme::danger()).child(e)))
            .when(d.enabled, |v| {
                v.child(
                    v_flex()
                        .gap_1()
                        .child(label_caps("CLAUDE CODE"))
                        .child(
                            div()
                                .p_2()
                                .rounded(theme::RADIUS_SM)
                                .border_1()
                                .border_color(theme::border())
                                .font_family("monospace")
                                .text_xs()
                                .child(command.clone()),
                        )
                        .child(h_flex().child(
                            Button::new("agent-copy").small().label(t!("agent.copy_command")).on_click(cx.listener(
                                move |this, _, _, cx| {
                                    cx.write_to_clipboard(ClipboardItem::new_string(command.clone()));
                                    this.set_status(t!("agent.command_copied"), false);
                                    cx.notify();
                                },
                            )),
                        )),
                )
            })
            .into_any_element()
    }
}
