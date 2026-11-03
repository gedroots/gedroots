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
        let context = report::context(self.report_window(window, cx));
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

    /// What the window shows and the shape of the file, without anything from the file but
    /// counts, tag names and its format.
    fn report_window(&self, window: &Window, cx: &App) -> serde_json::Value {
        use serde_json::json;
        let size = window.viewport_size();
        let interface = json!({
            "language": &*crate::i18n::language(),
            "appearance": if theme::is_dark() { "dark" } else { "light" },
            "window": [size.width.as_f32().round(), size.height.as_f32().round()],
            "scale": window.scale_factor(),
            "windows": cx.windows().len(),
            "sidebar": self.sidebar,
        });
        let settings = json!({
            "installed": crate::updates::available(),
            "automatic_updates": crate::updates::load().automatic,
            "agents": match crate::agent::status() {
                crate::agent::Status::Off => "off",
                crate::agent::Status::Running => "running",
                crate::agent::Status::Failed(_) => "failed",
            },
        });
        let Some(doc) = &self.doc else {
            return json!({ "interface": interface, "settings": settings, "screen": { "view": "Welcome" } });
        };
        let g = &doc.ged;
        let open: Vec<&str> = [
            (self.act_editor.is_some(), "record_entry"),
            (self.review.open, "to_review"),
            (self.gallery.is_some(), "media_gallery"),
            (self.add_dialog.is_some(), "add_dialog"),
            (self.picker.is_some(), "person_picker"),
            (self.new_place.is_some(), "new_place"),
            (self.locate.is_some(), "locate_place"),
            (self.kinship.is_some(), "kinship"),
            (self.photo_draw.is_some(), "photo_framing"),
        ]
        .into_iter()
        .filter_map(|(on, name)| on.then_some(name))
        .collect();
        let selected = self.selected.as_ref().and_then(|x| g.get(x)).and_then(|n| RecordKind::from_tag(&n.tag));
        let pages = matches!(self.view, View::Quality | View::Stats);
        let screen = json!({
            "view": format!("{:?}", self.view),
            "tab": (!pages).then(|| format!("{:?}", self.tab)),
            "tree": (!pages && self.tab == CenterTab::Tree)
                .then(|| json!({ "mode": format!("{:?}", self.tree_mode), "generations": self.tree_gens })),
            "open": open,
            "selected": selected.map(|k| format!("{k:?}")),
            "search_active": !self.filter.read(cx).value().is_empty(),
            "flagged_only": self.only_flagged,
        });
        // How big the tree is: numbers of records, never their contents.
        let mut tree = serde_json::Map::new();
        for kind in RecordKind::ALL {
            let key = match kind {
                RecordKind::Individual => "individuals",
                RecordKind::Family => "families",
                RecordKind::Source => "sources",
                RecordKind::Repository => "repositories",
                RecordKind::Media => "media",
                RecordKind::SharedNote => "notes",
                RecordKind::Submitter => "submitters",
            };
            tree.insert(key.into(), g.count(kind).into());
        }
        tree.insert("places".into(), g.places().len().into());
        tree.insert("media_files_in_folder".into(), self.media_files.len().into());
        // Extension tags (`_UID`, `_GR_SCOPE`…) and how often they appear: tag names only.
        let mut tags = std::collections::BTreeMap::<String, usize>::new();
        let mut count = |n: &Node| {
            let tag_like =
                n.tag.len() <= 24 && n.tag.bytes().all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'_');
            if n.tag.starts_with('_') && tag_like {
                *tags.entry(n.tag.clone()).or_default() += 1;
            }
        };
        g.header.walk(&mut count);
        for r in &g.records {
            r.walk(&mut count);
        }
        let mut tags: Vec<(String, usize)> = tags.into_iter().collect();
        tags.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        tags.truncate(20);
        // `HEAD.SOUR`: the product identifier of the software that wrote the file.
        let product = |path: &[&str]| {
            g.header.path_val(path).map(|v| v.chars().filter(|c| !c.is_control()).take(40).collect::<String>())
        };
        let file = json!({
            "gedcom": g.version(),
            "written_by": product(&["SOUR"]),
            "written_by_version": product(&["SOUR", "VERS"]),
            "bom": g.format.bom,
            "crlf": g.format.crlf,
            "size_kb": doc.path.as_ref().and_then(|p| std::fs::metadata(p).ok()).map(|m| m.len() / 1024),
            "saved": doc.path.is_some() && !doc.imported,
            "unsaved_changes": doc.dirty,
            "undo_steps": doc.history.len(),
            "extension_tags": tags.into_iter().map(|(t, n)| json!([t, n])).collect::<Vec<_>>(),
        });
        let settings = {
            let mut s = settings;
            s["proposals_to_review"] = self.review.items.len().into();
            s
        };
        json!({ "interface": interface, "settings": settings, "screen": screen, "tree": tree, "file": file })
    }

    pub(super) fn render_report(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let d = self.report.as_ref()?;
        let empty = d.description.read(cx).value().trim().is_empty();
        let context = v_flex()
            .gap_1()
            .child(
                h_flex()
                    .gap_2()
                    .items_center()
                    .child(
                        Checkbox::new("report-include")
                            .checked(d.include)
                            .label(t!("report.include").into_owned())
                            .on_click(cx.listener(|this, checked: &bool, _, cx| {
                                if let Some(d) = &mut this.report {
                                    d.include = *checked;
                                    cx.notify();
                                }
                            })),
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
