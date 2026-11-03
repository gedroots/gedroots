//! “Quality” page: the file’s issues by category, with their in-place fix (see `gedcom::quality`).
//!
//! The left list gives the categories, the central area the issues of the chosen category. A click
//! opens the item concerned (⌘[ comes back here), the row’s button applies the fix when it is safe,
//! the right-click allows hiding the issue (`_GR_IGNORE`).

use super::*;
use gedcom::analysis::check::{self, Issue, Severity};
use gedcom::quality::{self, Category};
use gpui::PathPromptOptions;
use gpui_kit::component::menu::PopupMenu;
use rust_i18n::t;
use std::cell::RefCell;

/// State of the page.
#[derive(Default)]
pub(super) struct QualityState {
    /// All issues (hidden ones included).
    pub issues: Rc<Vec<Issue>>,
    /// The file has changed since the last check.
    pub stale: bool,
    /// Also show the hidden issues (View › Hidden Issues).
    pub show_ignored: bool,
    /// Bulk fix waiting for confirmation (issue code).
    pub confirm: Option<&'static str>,
    /// Row targeted by the last right-click.
    pub menu_issue: Rc<RefCell<Option<Issue>>>,
}

impl Workspace {
    /// Reruns the checks if the file has changed.
    pub(super) fn refresh_quality(&mut self) {
        let Some(doc) = &self.doc else {
            self.quality.issues = Rc::default();
            return;
        };
        if self.quality.stale {
            // The folder may have changed since opening (files added, moved).
            let path = doc.path.clone();
            self.media_files = Rc::new(relatives::scan_media_files(path.as_deref()));
            let doc = self.doc.as_ref().unwrap();
            let exists = |f: &str| pages::file_path(doc, f).is_some_and(|p| p.exists());
            let files = quality::Files { exists: &exists, folder: &self.media_files };
            self.quality.issues = Rc::new(quality::run_with(&doc.ged, Some(&files)));
            self.quality.stale = false;
            self.quality.confirm = None;
        }
    }

    fn visible_issue(&self, i: &Issue) -> bool {
        self.quality.show_ignored || !i.ignored
    }

    /// Rows of the list: one per non-empty category.
    pub(super) fn quality_rows(&self, q: &str) -> Vec<Row> {
        Category::ALL
            .into_iter()
            .filter_map(|c| {
                let n = self
                    .quality
                    .issues
                    .iter()
                    .filter(|i| quality::category(i.code) == c && self.visible_issue(i))
                    .filter(|i| q.is_empty() || i.message.to_lowercase().contains(q))
                    .count();
                (n > 0).then(|| Row {
                    key: c.id().to_string(),
                    title: c.label().to_string(),
                    strong: String::new(),
                    meta: n.to_string(),
                    dot: None,
                    flagged: false,
                })
            })
            .collect()
    }

    fn quality_edit(
        &mut self,
        f: impl FnOnce(&mut Gedcom) -> bool,
        msg: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.edit_ged(f, msg, window, cx);
    }

    pub(super) fn fix_issue(&mut self, issue: Issue, window: &mut Window, cx: &mut Context<Self>) {
        let Some(fix) = quality::fix_of(issue.code) else { return };
        let msg = t!("quality.labelled", label = fix.label, message = issue.message).to_string();
        self.quality_edit(|g| g.fix_issue(&issue), msg, window, cx);
    }

    /// Fixes all visible issues of a code. Deletions and merges require a second click.
    fn fix_all(&mut self, code: &'static str, window: &mut Window, cx: &mut Context<Self>) {
        let harmless = matches!(code, "missing_backlink" | "moved_file" | "missing_form" | "unattached_file");
        if !harmless && self.quality.confirm != Some(code) {
            self.quality.confirm = Some(code);
            cx.notify();
            return;
        }
        self.quality.confirm = None;
        let issues: Vec<Issue> = self.quality.issues.iter().filter(|i| i.code == code && !i.ignored).cloned().collect();
        let n = issues.len();
        let msg = crate::tn!("quality.fixed", n, label = check::code_label(code)).to_string();
        self.quality_edit(|g| g.fix_issues(&issues) > 0, msg, window, cx);
    }

    /// Opens the item of an issue: its record, or the place concerned.
    fn open_issue(&mut self, issue: &Issue, window: &mut Window, cx: &mut Context<Self>) {
        if !issue.xref.is_empty() {
            self.goto(issue.xref.clone(), window, cx);
        } else if place_issue(issue)
            && let Some(p) = issue.related.first()
        {
            self.goto_place(p.clone(), window, cx);
        }
    }

    /// “Locate…”: choose the file of a missing media, or open the place on the map to locate it
    /// (one place at a time).
    fn locate_file(&mut self, issue: Issue, window: &mut Window, cx: &mut Context<Self>) {
        if place_issue(&issue) {
            self.open_issue(&issue, window, cx);
            if let Some(name) = issue.related.first().cloned() {
                self.start_locate(name, cx);
            }
            return;
        }
        let rx = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some(t!("quality.locate_prompt").into()),
        });
        cx.spawn_in(window, async move |this, cx| {
            if let Ok(Ok(Some(paths))) = rx.await
                && let Some(p) = paths.first()
            {
                let _ = this.update_in(cx, |this, window, cx| {
                    let value = this.file_value(p);
                    let msg = t!("quality.located", value = value).to_string();
                    this.quality_edit(|g| g.set_media_file(&issue.xref, &issue.path, &value), msg, window, cx);
                });
            }
        })
        .detach();
    }

    pub(super) fn set_issue_ignored(
        &mut self,
        issue: Issue,
        ignore: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let label = check::code_label(issue.code);
        let msg =
            if ignore { t!("quality.hidden", label = label) } else { t!("quality.shown", label = label) }.to_string();
        let (x, code) = (issue.xref.clone(), issue.code);
        self.quality_edit(
            move |g| if ignore { g.ignore_issue(&x, code) } else { g.unignore_issue(&x, code) },
            msg,
            window,
            cx,
        );
    }

    /// Opens the Quality page on a category (Go › Files to Attach).
    pub(super) fn open_quality(&mut self, cat: Category, window: &mut Window, cx: &mut Context<Self>) {
        self.open_location(View::Quality, Some(cat.id().to_string()), window, cx);
        cx.notify();
    }

    pub(super) fn toggle_ignored_issues(&mut self, cx: &mut Context<Self>) {
        self.quality.show_ignored = !self.quality.show_ignored;
        self.build_rows(cx);
        cx.notify();
    }

    /// Central area: the issues of the chosen category.
    pub(super) fn render_quality(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(cat) = self.selected.as_deref().and_then(Category::from_id) else {
            let empty = self.quality.issues.iter().all(|i| !self.visible_issue(i));
            return v_flex()
                .flex_1()
                .items_center()
                .justify_center()
                .text_color(if empty { theme::ok() } else { theme::muted() })
                .child(if empty { t!("quality.no_issue") } else { t!("quality.choose_category") })
                .into_any_element();
        };
        let q = self.filter.read(cx).value().trim().to_lowercase();
        let shown: Rc<Vec<Issue>> = Rc::new(
            self.quality
                .issues
                .iter()
                .filter(|i| quality::category(i.code) == cat && self.visible_issue(i))
                .filter(|i| q.is_empty() || i.message.to_lowercase().contains(&q))
                .cloned()
                .collect(),
        );
        // Bulk fixes: one per code that has one, with the number concerned.
        let mut batches: Vec<(&'static str, std::borrow::Cow<'static, str>, usize)> = Vec::new();
        for i in shown.iter().filter(|i| !i.ignored) {
            if let Some(label) = quality::fix_of(i.code).and_then(|f| f.batch) {
                match batches.iter_mut().find(|b| b.0 == i.code) {
                    Some(b) => b.2 += 1,
                    None => batches.push((i.code, label, 1)),
                }
            }
        }
        batches.retain(|b| b.2 > 1);
        let entity = cx.entity().downgrade();
        let menu_issue = self.quality.menu_issue.clone();
        // One readable column, like the records.
        let column = v_flex()
            .size_full()
            .max_w(px(720.))
            .child(
                h_flex()
                    .px_4()
                    .py_3()
                    .gap_2()
                    .items_center()
                    .border_b_1()
                    .border_color(theme::border())
                    .child(div().text_xl().font_weight(FontWeight::SEMIBOLD).child(cat.label()))
                    .child(mono(shown.len().to_string()))
                    .child(div().flex_1())
                    .children(batches.into_iter().map(|(code, label, n)| {
                        let confirming = self.quality.confirm == Some(code);
                        Button::new(SharedString::from(format!("batch-{code}")))
                            .xsmall()
                            .when(confirming, |b| b.danger())
                            .when(!confirming, |b| b.ghost())
                            .label(if confirming {
                                t!("quality.confirm", label = label, n = n).to_string()
                            } else {
                                format!("{label} ({n})")
                            })
                            .on_click(cx.listener(move |this, _, w, cx| this.fix_all(code, w, cx)))
                    })),
            )
            .child(
                div()
                    .id("quality-issues")
                    .flex_1()
                    .min_h_0()
                    .child(
                        uniform_list("issues", shown.len(), move |range, _, _| {
                            range
                                .map(|n| {
                                    let i = &shown[n];
                                    let (open, fix) = (entity.clone(), entity.clone());
                                    let (x, issue, for_menu) = (i.clone(), i.clone(), i.clone());
                                    let menu_issue = menu_issue.clone();
                                    h_flex()
                                        .id(n)
                                        .w_full()
                                        .h(px(46.))
                                        .px_4()
                                        .gap_3()
                                        .items_center()
                                        .border_b_1()
                                        .border_color(theme::border())
                                        .cursor_pointer()
                                        .when(i.ignored, |d| d.opacity(0.5))
                                        .hover(|d| d.bg(theme::surface()))
                                        .child(dot(severity_color(i.severity)))
                                        .child(
                                            v_flex()
                                                .flex_1()
                                                .min_w_0()
                                                .child(
                                                    div()
                                                        .overflow_hidden()
                                                        .whitespace_nowrap()
                                                        .text_ellipsis()
                                                        .child(i.message.clone()),
                                                )
                                                .child(
                                                    div()
                                                        .text_xs()
                                                        .text_color(theme::muted())
                                                        .child(check::code_label(i.code)),
                                                ),
                                        )
                                        .when(locatable(i.code) && !i.ignored, |d| {
                                            let (locate, issue) = (entity.clone(), i.clone());
                                            d.child(
                                                Button::new(("locate", n))
                                                    .ghost()
                                                    .xsmall()
                                                    .label(t!("quality.locate"))
                                                    .on_click(move |_, w, cx| {
                                                        cx.stop_propagation();
                                                        let issue = issue.clone();
                                                        let _ = locate
                                                            .update(cx, |this, cx| this.locate_file(issue, w, cx));
                                                    }),
                                            )
                                        })
                                        .when_some(quality::fix_of(i.code).filter(|_| !i.ignored), |d, f| {
                                            d.child(Button::new(("fix", n)).ghost().xsmall().label(f.label).on_click(
                                                move |_, w, cx| {
                                                    cx.stop_propagation();
                                                    let issue = issue.clone();
                                                    let _ = fix.update(cx, |this, cx| this.fix_issue(issue, w, cx));
                                                },
                                            ))
                                        })
                                        .on_click(move |_, w, cx| {
                                            let x = x.clone();
                                            let _ = open.update(cx, |this, cx| this.open_issue(&x, w, cx));
                                        })
                                        .on_mouse_down(MouseButton::Right, move |_, _, _| {
                                            *menu_issue.borrow_mut() = Some(for_menu.clone());
                                        })
                                })
                                .collect()
                        })
                        .size_full()
                        .text_sm(),
                    )
                    .with_menu(self.issue_menu(cx)),
            );
        h_flex().flex_1().min_h_0().justify_center().child(column).into_any_element()
    }

    /// Right-click menu of an issue.
    fn issue_menu(&self, cx: &mut Context<Self>) -> menus::MenuBuilder {
        let ws = cx.entity().downgrade();
        let target = self.quality.menu_issue.clone();
        Rc::new(move |menu: PopupMenu, _, _| {
            let Some(issue) = target.borrow_mut().take() else { return menu };
            let (a, b, c, d) = (issue.clone(), issue.clone(), issue.clone(), issue.clone());
            // File without media: nothing to open nor anywhere to write `_GR_IGNORE`.
            let record = !issue.xref.is_empty();
            let menu = if record || place_issue(&issue) {
                menu.item(menus::act(&ws, t!("quality.open"), move |this, w, cx| this.open_issue(&a, w, cx)))
            } else {
                menu
            };
            let menu = if locatable(issue.code) && !issue.ignored {
                menu.item(menus::act(&ws, t!("quality.locate"), move |this, w, cx| this.locate_file(d.clone(), w, cx)))
            } else {
                menu
            };
            let menu = match quality::fix_of(issue.code).filter(|_| !issue.ignored) {
                Some(f) => menu.item(menus::act(&ws, f.label, move |this, w, cx| this.fix_issue(b.clone(), w, cx))),
                None => menu,
            };
            let ignored = issue.ignored;
            if !record {
                return menu;
            }
            menu.separator().item(menus::act(
                &ws,
                if ignored { t!("quality.unhide") } else { t!("quality.hide") },
                move |this, w, cx| this.set_issue_ignored(c.clone(), !ignored, w, cx),
            ))
        })
    }
}

/// Issues about a place (without a record).
fn place_issue(i: &Issue) -> bool {
    i.xref.is_empty() && matches!(quality::category(i.code), Category::Places | Category::Coords)
}

/// Issues with a “Locate…” button.
fn locatable(code: &str) -> bool {
    matches!(code, "missing_file" | "missing_coords" | "bad_coords")
}

fn severity_color(s: Severity) -> Hsla {
    match s {
        Severity::Error => theme::danger(),
        Severity::Warning => theme::warn(),
        Severity::Info => theme::accent(),
    }
}
