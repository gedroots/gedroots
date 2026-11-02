//! Help › Report an Issue…: a description and, if the user agrees, the context of the window,
//! then GitHub's new issue page filled in (see `crate::report`).

use super::*;
use crate::report;
use gpui_kit::component::checkbox::Checkbox;
use gpui_kit::component::input::TextareaState;
use rust_i18n::t;

/// The “Report an Issue” window.
pub struct ReportDialog {
    description: Entity<TextareaState>,
    include: bool,
    /// Context shown under the checkbox.
    preview: bool,
    /// Context computed when the window opened: what is shown is what is sent.
    context: String,
    _sub: Subscription,
}

impl Workspace {
    pub(super) fn open_report(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.report.is_some() {
            return;
        }
        let description = cx.new(|cx| TextareaState::new(window, cx).rows(6).placeholder(t!("report.placeholder")));
        // After the palette, which gives the focus back to the window when it closes.
        let field = description.clone();
        cx.defer_in(window, move |_, window, cx| field.update(cx, |s, cx| s.focus(window, cx)));
        // The send button follows the description.
        let sub = cx.subscribe(&description, |_, _, _: &InputEvent, cx| cx.notify());
        let context = report::context(&self.report_screen(), &crate::i18n::language(), theme::is_dark());
        self.report = Some(ReportDialog { description, include: true, preview: false, context, _sub: sub });
        cx.notify();
    }

    fn close_report(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.report.take().is_some() {
            window.focus(&self.focus, cx);
            cx.notify();
        }
    }

    /// Opens GitHub's new issue page with the description and the context. A report too long for
    /// an address goes through the clipboard.
    fn send_report(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(d) = &self.report else { return };
        let description = d.description.read(cx).value().to_string();
        if description.trim().is_empty() {
            return;
        }
        let include = d.include;
        let body = report::body(&description, include.then_some(d.context.as_str()));
        let title = report::title(&description);
        match report::issue_url(&title, &body) {
            Some(url) => cx.open_url(&url),
            None => {
                cx.write_to_clipboard(ClipboardItem::new_string(body));
                cx.open_url(&report::issue_url_title_only(&title));
                self.set_status(t!("report.copied"), false);
            }
        }
        if include {
            report::clear_crash();
        }
        self.close_report(window, cx);
    }

    /// What the window shows, without anything from the file but counts and its format.
    fn report_screen(&self) -> Vec<(&'static str, String)> {
        let Some(doc) = &self.doc else { return vec![("Screen", "Welcome".into())] };
        let mut screen = format!("{:?}", self.view);
        if !matches!(self.view, View::Quality | View::Stats) {
            screen.push_str(&format!(" › {:?}", self.tab));
            if self.tab == CenterTab::Tree {
                screen.push_str(&format!(" ({:?}, {} generations)", self.tree_mode, self.tree_gens));
            }
        }
        let open: Vec<&str> = [
            (self.act_editor.is_some(), "record entry"),
            (self.review.open, "To Review"),
            (self.gallery.is_some(), "media gallery"),
            (self.add_dialog.is_some(), "add dialog"),
            (self.picker.is_some(), "person picker"),
            (self.new_place.is_some(), "new place"),
            (self.locate.is_some(), "locate place"),
            (self.kinship.is_some(), "kinship"),
            (self.photo_draw.is_some(), "photo framing"),
        ]
        .into_iter()
        .filter_map(|(on, name)| on.then_some(name))
        .collect();
        if !open.is_empty() {
            screen.push_str(&format!(", {}", open.join(", ")));
        }
        if !self.sidebar {
            screen.push_str(", sidebar hidden");
        }
        let g = &doc.ged;
        let selected = self.selected.as_ref().and_then(|x| g.get(x)).and_then(|n| RecordKind::from_tag(&n.tag));
        // Only how big the tree is: numbers of records, never their contents.
        let counts: Vec<String> = RecordKind::ALL
            .iter()
            .map(|&k| (k, g.count(k)))
            .filter(|&(_, n)| n > 0)
            .map(|(k, n)| {
                let (one, many) = match k {
                    RecordKind::Individual => ("person", "people"),
                    RecordKind::Family => ("family", "families"),
                    RecordKind::Source => ("source", "sources"),
                    RecordKind::Repository => ("repository", "repositories"),
                    RecordKind::Media => ("media", "media"),
                    RecordKind::SharedNote => ("note", "notes"),
                    RecordKind::Submitter => ("submitter", "submitters"),
                };
                format!("{n} {}", if n == 1 { one } else { many })
            })
            .collect();
        // `HEAD.SOUR`: the product identifier of the software that wrote the file.
        let product = |path: &[&str]| {
            g.header.path_val(path).map(|v| v.chars().filter(|c| !c.is_control()).take(40).collect::<String>())
        };
        let mut format = format!("GEDCOM {}", g.version());
        if let Some(sour) = product(&["SOUR"]) {
            format.push_str(&format!(" from {sour}"));
            if let Some(vers) = product(&["SOUR", "VERS"]) {
                format.push_str(&format!(" {vers}"));
            }
        }
        if doc.dirty {
            format.push_str(", unsaved changes");
        }
        if let Some(kind) = selected {
            screen.push_str(&format!(", {} selected", format!("{kind:?}").to_lowercase()));
        }
        let size = if counts.is_empty() { "empty".to_string() } else { counts.join(", ") };
        let out = vec![("Screen", screen), ("Tree size", size), ("Format", format)];
        out
    }

    pub(super) fn render_report(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let d = self.report.as_ref()?;
        let empty = d.description.read(cx).value().trim().is_empty();
        let context = v_flex()
            .gap_1()
            .child(
                Checkbox::new("report-include").checked(d.include).label(t!("report.include").into_owned()).on_click(
                    cx.listener(|this, checked: &bool, _, cx| {
                        if let Some(d) = &mut this.report {
                            d.include = *checked;
                            cx.notify();
                        }
                    }),
                ),
            )
            .child(
                h_flex()
                    .pl(px(24.))
                    .gap_2()
                    .items_center()
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_xs()
                            .text_color(theme::muted())
                            .child(t!("report.include_detail")),
                    )
                    .child(
                        Button::new("report-preview")
                            .xsmall()
                            .ghost()
                            .label(if d.preview { t!("report.hide") } else { t!("report.show") })
                            .on_click(cx.listener(|this, _, _, cx| {
                                if let Some(d) = &mut this.report {
                                    d.preview = !d.preview;
                                    cx.notify();
                                }
                            })),
                    ),
            )
            .when(d.preview, |c| {
                c.child(
                    div()
                        .id("report-context")
                        .ml(px(24.))
                        .max_h(px(180.))
                        .overflow_y_scroll()
                        .p_2()
                        .rounded(theme::RADIUS_SM)
                        .bg(theme::surface())
                        .child(mono(d.context.clone())),
                )
            });
        Some(
            div()
                .absolute()
                .inset_0()
                .flex()
                .justify_center()
                .items_start()
                .pt(px(70.))
                .bg(theme::scrim())
                .child(
                    v_flex()
                        .id("report")
                        .occlude()
                        .w(px(560.))
                        .rounded(theme::RADIUS_LG)
                        .border_1()
                        .border_color(theme::border_strong())
                        .bg(theme::panel())
                        .shadow_lg()
                        .on_key_down(cx.listener(|this, ev: &KeyDownEvent, w, cx| {
                            if ev.keystroke.key == "escape" {
                                this.close_report(w, cx);
                            }
                        }))
                        .child(
                            h_flex()
                                .p_3()
                                .gap_2()
                                .items_center()
                                .border_b_1()
                                .border_color(theme::border())
                                .child(div().flex_1().font_weight(FontWeight::SEMIBOLD).child(t!("report.title")))
                                .child(
                                    Button::new("report-close")
                                        .xsmall()
                                        .ghost()
                                        .icon(IconName::Close)
                                        .on_click(cx.listener(|this, _, w, cx| this.close_report(w, cx))),
                                ),
                        )
                        .child(v_flex().p_4().gap_4().child(Textarea::new(&d.description).h(px(140.))).child(context))
                        .child(
                            h_flex()
                                .p_3()
                                .gap_2()
                                .items_center()
                                .border_t_1()
                                .border_color(theme::border())
                                .child(div().flex_1())
                                .child(
                                    Button::new("report-cancel")
                                        .ghost()
                                        .small()
                                        .label(t!("common.cancel"))
                                        .on_click(cx.listener(|this, _, w, cx| this.close_report(w, cx))),
                                )
                                .child(
                                    Button::new("report-send")
                                        .primary()
                                        .small()
                                        .icon(IconName::ExternalLink)
                                        .label(t!("report.continue"))
                                        .disabled(empty)
                                        .on_click(cx.listener(|this, _, w, cx| this.send_report(w, cx))),
                                ),
                        ),
                )
                .into_any_element(),
        )
    }
}
