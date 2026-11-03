//! Source › Connect an AI Agent…: turns on the local MCP server and gives, for each agent, the
//! shortest way to connect it.

use super::add::AddDialog;
use super::*;
use crate::agent::clients::{Client, Kind, Setup, mask};
use crate::agent::{self, Status};
use gpui_kit::component::switch::Switch;
use rust_i18n::t;

/// “AI Agents” window.
pub(super) struct AgentDialog {
    enabled: bool,
    token: String,
    client: Client,
    /// Token visible in “Other MCP Client”.
    reveal: bool,
    /// Manual configuration unfolded (editors).
    manual: bool,
    /// Last block copied, for the check mark.
    copied: Option<&'static str>,
}

/// Width of the window (wider than the others: list + detail).
pub(super) const WIDTH: f32 = 640.;

fn mono(s: impl Into<SharedString>) -> Div {
    div().font_family("monospace").text_xs().child(s.into())
}

fn ago(d: std::time::Duration) -> SharedString {
    let min = (d.as_secs() / 60) as usize;
    match min {
        0 => t!("agent.connected_now").into(),
        1..60 => crate::tn!("agent.connected_min", min).into(),
        _ => crate::tn!("agent.connected_hours", min / 60).into(),
    }
}

impl Workspace {
    pub(super) fn open_agent_dialog(&mut self, cx: &mut Context<Self>) {
        let s = agent::load();
        let client = Client::from_id(&s.client).unwrap_or(Client::ClaudeCode);
        self.add_dialog = Some(AddDialog::Agent(Box::new(AgentDialog {
            enabled: s.enabled,
            token: s.token,
            client,
            reveal: false,
            manual: false,
            copied: None,
        })));
        cx.notify();
    }

    /// Copy of the document, for the MCP server.
    pub(crate) fn agent_snapshot(&self) -> Option<agent::Snapshot> {
        let doc = self.doc.as_ref()?;
        Some(agent::Snapshot { path: doc.path.clone(), ged: doc.ged.clone(), queue: self.review_queue() })
    }

    fn agent_dialog(&mut self) -> Option<&mut AgentDialog> {
        match &mut self.add_dialog {
            Some(AddDialog::Agent(d)) => Some(d),
            _ => None,
        }
    }

    pub(super) fn agent_body(&self, d: &AgentDialog, cx: &mut Context<Self>) -> AnyElement {
        let toggle = Switch::new("agent-enabled")
            .checked(d.enabled)
            .label(SharedString::from(t!("agent.allow")))
            .on_click(cx.listener(|this, on: &bool, _, cx| {
                let s = agent::set_enabled(*on);
                if let Some(d) = this.agent_dialog() {
                    d.enabled = s.enabled;
                }
                cx.notify();
            }));
        let status = agent::status();
        let running = d.enabled && status == Status::Running;
        let failure = match status {
            Status::Failed(e) if d.enabled => Some(e),
            _ => None,
        };
        let address = agent::url().trim_start_matches("http://").trim_end_matches("/mcp").to_string();
        let head =
            h_flex().px_4().py_3().gap_3().items_center().child(div().flex_1().child(toggle)).when(running, |h| {
                h.child(
                    h_flex()
                        .gap_2()
                        .items_center()
                        .text_xs()
                        .text_color(theme::muted())
                        .child(div().size_2().rounded_full().bg(theme::ok()))
                        .child(t!("agent.running", address = address).into_owned()),
                )
            });
        v_flex()
            .child(head)
            .when_some(failure, |v, e| v.child(div().px_4().pb_3().text_sm().text_color(theme::danger()).child(e)))
            .when(d.enabled, |v| {
                v.child(
                    h_flex()
                        .items_stretch()
                        .h(px(400.))
                        .border_t_1()
                        .border_color(theme::border())
                        .child(self.agent_list(d, cx))
                        .child(self.agent_detail(d, cx)),
                )
            })
            .into_any_element()
    }

    fn agent_list(&self, d: &AgentDialog, cx: &mut Context<Self>) -> Div {
        let seen = agent::last_client().map(|(n, _)| Client::from_client_info(&n));
        let mut list = v_flex().w(px(184.)).flex_none().p_2().gap_0p5().border_r_1().border_color(theme::border());
        let mut group = None;
        for c in Client::ALL {
            if group != Some(c.kind()) {
                group = Some(c.kind());
                let label = match c.kind() {
                    Kind::Terminal => t!("agent.group.terminal"),
                    Kind::App => t!("agent.group.app"),
                    Kind::Editor => t!("agent.group.editor"),
                    Kind::Other => t!("agent.group.other"),
                };
                list = list.child(label_caps(label.into_owned()).px_2().pt_2().pb_1());
            }
            let on = c == d.client;
            list = list.child(
                h_flex()
                    .id(SharedString::from(format!("agent-client-{}", c.id())))
                    .gap_2()
                    .items_center()
                    .px_2()
                    .py_1()
                    .rounded(theme::RADIUS_SM)
                    .text_sm()
                    .cursor_pointer()
                    .when(on, |h| h.bg(theme::accent_soft()))
                    .when(!on, |h| h.hover(|s| s.bg(theme::hover())))
                    .child(Icon::new(icon(c)).size(px(16.)).text_color(if on {
                        theme::accent()
                    } else {
                        theme::muted()
                    }))
                    .child(div().flex_1().min_w_0().truncate().child(name(c)))
                    .when(seen == Some(c), |h| h.child(div().size_1p5().rounded_full().bg(theme::ok())))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if let Some(d) = this.agent_dialog() {
                            d.client = c;
                            d.copied = None;
                            d.manual = false;
                        }
                        agent::set_client(c);
                        cx.notify();
                    })),
            );
        }
        list
    }

    fn agent_detail(&self, d: &AgentDialog, cx: &mut Context<Self>) -> AnyElement {
        let c = d.client;
        let kind = match c.kind() {
            Kind::Terminal => t!("agent.group.terminal"),
            Kind::App => t!("agent.group.app"),
            Kind::Editor => t!("agent.group.editor"),
            Kind::Other => t!("agent.streamable_http"),
        };
        let header = h_flex()
            .gap_3()
            .items_center()
            .child(
                div()
                    .size_9()
                    .flex_none()
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(theme::RADIUS_SM)
                    .border_1()
                    .border_color(theme::border())
                    .bg(theme::surface())
                    .child(Icon::new(icon(c)).size(px(20.)).text_color(theme::accent())),
            )
            .child(
                v_flex()
                    .child(div().font_weight(FontWeight::SEMIBOLD).child(name(c)))
                    .child(div().text_xs().text_color(theme::muted()).child(kind.into_owned())),
            );
        let mut body = v_flex().gap_4().child(header);
        match c.setup(&agent::url(), &d.token) {
            Setup::Command(cmd) => {
                body = body.child(field(t!("agent.in_terminal").into_owned(), None, self.code(d, "cmd", &cmd, cx)));
            }
            Setup::Config { file, text } => {
                // Claude Desktop: Node.js and the file first, the block is long.
                if c == Client::ClaudeDesktop {
                    let reveal = c.config_file();
                    body = body.child(
                        h_flex()
                            .gap_3()
                            .items_center()
                            .when_some(reveal, |h, path| {
                                h.child(
                                    Button::new("agent-reveal")
                                        .small()
                                        .icon(IconName::FolderOpen)
                                        .label(t!("agent.show_file").into_owned())
                                        .on_click(move |_, _, cx| {
                                            if path.exists() {
                                                cx.reveal_path(&path);
                                            } else if let Some(dir) = path.parent() {
                                                cx.open_with_system(dir);
                                            }
                                        }),
                                )
                            })
                            .child(
                                h_flex()
                                    .gap_1()
                                    .items_center()
                                    .text_xs()
                                    .text_color(theme::muted())
                                    .child(Icon::new(IconName::Info).size(px(14.)))
                                    .child(t!("agent.needs_node").into_owned()),
                            ),
                    );
                }
                body = body.child(field(t!("agent.add_to").into_owned(), Some(file), self.code(d, "cfg", &text, cx)));
            }
            Setup::Link { url, file, text } => {
                let product = name(c);
                body = body
                    .child(
                        h_flex().child(
                            Button::new("agent-install")
                                .primary()
                                .icon(IconName::ExternalLink)
                                .label(t!("agent.install_in", app = product).into_owned())
                                .on_click(move |_, _, cx| cx.open_url(&url)),
                        ),
                    )
                    .child(
                        v_flex()
                            .gap_2()
                            .child(
                                h_flex()
                                    .id("agent-manual")
                                    .gap_1()
                                    .items_center()
                                    .text_xs()
                                    .text_color(theme::muted())
                                    .cursor_pointer()
                                    .child(
                                        Icon::new(if d.manual {
                                            IconName::ChevronDown
                                        } else {
                                            IconName::ChevronRight
                                        })
                                        .size(px(14.)),
                                    )
                                    .child(t!("agent.manual").into_owned())
                                    .child(mono(file))
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        if let Some(d) = this.agent_dialog() {
                                            d.manual = !d.manual;
                                        }
                                        cx.notify();
                                    })),
                            )
                            .when(d.manual, |v| v.child(self.code(d, "cfg", &text, cx))),
                    );
            }
            Setup::Generic => body = body.child(self.generic(d, cx)),
        }
        if c.shows_prompts() {
            let chip = |s: &'static str| {
                div()
                    .px_2()
                    .py_0p5()
                    .rounded(theme::RADIUS_SM)
                    .border_1()
                    .border_color(theme::border())
                    .bg(theme::surface())
                    .child(mono(s))
            };
            body = body.child(
                v_flex().gap_1().child(label_caps(t!("agent.commands").into_owned())).child(
                    h_flex()
                        .gap_2()
                        .flex_wrap()
                        .child(chip("/mcp__gedroots__find_record"))
                        .child(chip("/mcp__gedroots__read_record")),
                ),
            );
        }
        if let Some((who, when)) = agent::last_client()
            && Client::from_client_info(&who) == c
        {
            let text = if c == Client::Other { format!("{who} · {}", ago(when)) } else { ago(when).to_string() };
            body = body.child(
                h_flex()
                    .gap_2()
                    .items_center()
                    .text_xs()
                    .text_color(theme::muted())
                    .child(Icon::new(IconName::Check).size(px(14.)).text_color(theme::ok()))
                    .child(text),
            );
        }
        div()
            .id("agent-detail")
            .flex_1()
            .min_w_0()
            .bg(theme::bg())
            .rounded_br(theme::RADIUS_LG)
            .overflow_y_scroll()
            .p_4()
            .child(body)
            .into_any_element()
    }

    /// Code block (token masked) with its copy button.
    fn code(&self, d: &AgentDialog, key: &'static str, text: &str, cx: &mut Context<Self>) -> Div {
        let shown = mask(text, &d.token);
        let text = text.to_string();
        div()
            .relative()
            .p_2()
            .pr_8()
            .rounded(theme::RADIUS_SM)
            .border_1()
            .border_color(theme::border())
            .bg(theme::surface())
            .child(mono(shown))
            .child(div().absolute().top_1().right_1().child(self.copy_button(d, key, text, cx)))
    }

    fn copy_button(&self, d: &AgentDialog, key: &'static str, text: String, cx: &mut Context<Self>) -> Button {
        let done = d.copied == Some(key);
        Button::new(SharedString::from(format!("agent-copy-{key}")))
            .ghost()
            .xsmall()
            .icon(if done { IconName::Check } else { IconName::Copy })
            .tooltip(t!("agent.copy").into_owned())
            .on_click(cx.listener(move |this, _, _, cx| {
                cx.write_to_clipboard(ClipboardItem::new_string(text.clone()));
                if let Some(d) = this.agent_dialog() {
                    d.copied = Some(key);
                }
                this.set_status(t!("agent.copied"), false);
                cx.notify();
            }))
    }

    /// “Other MCP Client”: address, transport and header.
    fn generic(&self, d: &AgentDialog, cx: &mut Context<Self>) -> Div {
        let url = agent::url();
        let header = format!("Authorization: Bearer {}", d.token);
        let shown = if d.reveal { header.clone() } else { mask(&header, &d.token) };
        let row = |label: String, value: String| {
            h_flex()
                .gap_3()
                .items_center()
                .min_h(px(28.))
                .child(div().w(px(72.)).flex_none().text_xs().text_color(theme::muted()).child(label))
                .child(div().flex_1().min_w_0().child(mono(value)))
        };
        v_flex()
            .gap_1()
            .py_1()
            .pl_3()
            .pr_1()
            .rounded(theme::RADIUS_SM)
            .border_1()
            .border_color(theme::border())
            .bg(theme::surface())
            .child(row(t!("agent.address").into_owned(), url.clone()).child(self.copy_button(d, "url", url, cx)))
            .child(row(t!("agent.transport").into_owned(), t!("agent.streamable_http").into_owned()))
            .child(
                row(t!("agent.header").into_owned(), shown)
                    .child(
                        Button::new("agent-reveal-token")
                            .ghost()
                            .xsmall()
                            .icon(if d.reveal { IconName::EyeOff } else { IconName::Eye })
                            .tooltip(
                                if d.reveal { t!("agent.hide_token") } else { t!("agent.show_token") }.into_owned(),
                            )
                            .on_click(cx.listener(|this, _, _, cx| {
                                if let Some(d) = this.agent_dialog() {
                                    d.reveal = !d.reveal;
                                }
                                cx.notify();
                            })),
                    )
                    .child(self.copy_button(d, "header", header, cx)),
            )
    }
}

fn icon(c: Client) -> IconName {
    match c.kind() {
        Kind::Terminal => IconName::SquareTerminal,
        Kind::App => IconName::AppWindow,
        Kind::Editor => IconName::Code,
        Kind::Other => IconName::Plug,
    }
}

fn name(c: Client) -> String {
    c.product().map(str::to_string).unwrap_or_else(|| t!("agent.other_client").into_owned())
}

/// Label (with a file path if any) above a block.
fn field(label: String, file: Option<String>, block: Div) -> Div {
    v_flex()
        .gap_1()
        .child(
            h_flex()
                .gap_2()
                .items_center()
                .child(label_caps(label))
                .when_some(file, |h, f| h.child(mono(f).text_color(theme::muted()))),
        )
        .child(block)
}
