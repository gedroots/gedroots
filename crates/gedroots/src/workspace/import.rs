//! Import: File › Open… on a file from other software (`HEAD.SOUR` other than GEDRoots) or on a
//! GEDZIP archive opens it as a tree not saved yet, so that the export of the other software is
//! never overwritten. The wizard then
//! asks for the root person if the file declares none, lists what the file contains that is not
//! standard (see `gedcom::import`) and offers to convert it to GEDCOM 7. The conversion can be
//! undone like any change.

use super::add::AddDialog;
use super::*;
use crate::editor::{PickerState, picker};
use crate::t;
use gedcom::import::{self, Finding};
use gpui_kit::component::checkbox::Checkbox;
use std::path::Path;

pub(super) struct ImportDialog {
    file: String,
    /// Root person to declare, when the file declares none.
    root: Option<Entity<PickerState>>,
    findings: Vec<(Finding, bool)>,
    /// Added to the final status (where a GEDZIP was unzipped).
    note: String,
}

/// Is the file a GEDZIP archive (by its extension)?
pub(super) fn is_gedzip(path: &Path) -> bool {
    path.extension().is_some_and(|e| e.eq_ignore_ascii_case(gedcom::gedzip::EXTENSION))
}

impl Workspace {
    /// Imports `path`: a GEDZIP is unzipped into a folder next to it, whose tree is saved there.
    pub(super) fn import_path(&mut self, path: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        let file = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
        if !is_gedzip(&path) {
            match gedcom::load(&path) {
                Ok(ged) => self.open_imported(ged, path, file, String::new(), window, cx),
                Err(e) => self.set_error("ws.open_failed", t!("ws.open_failed", path = path.display(), error = e)),
            }
            return cx.notify();
        }
        let dest = crate::gedzip::extraction_folder(&path);
        self.set_status(t!("import.unzipping", file = file), false);
        cx.notify();
        let task = cx.background_spawn({
            let (path, dest) = (path.clone(), dest.clone());
            async move { crate::gedzip::extract(&path, &dest) }
        });
        cx.spawn_in(window, async move |this, cx| {
            let res = task.await.and_then(|(text, n)| Gedcom::parse(&text).map(|g| (g, n)).map_err(|e| e.to_string()));
            let _ = this.update_in(cx, |this, window, cx| match res {
                Ok((mut ged, n)) => {
                    gedcom::gedzip::unpack(&mut ged);
                    let stem = path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
                    let note = match n {
                        0 => String::new(),
                        n => crate::tn!("import.unzipped", n, folder = dest.file_name().unwrap_or_default().display())
                            .into_owned(),
                    };
                    this.open_imported(ged, dest.join(format!("{stem}.ged")), file, note, window, cx);
                    cx.notify();
                }
                Err(e) => {
                    this.set_error("ws.open_failed", t!("ws.open_failed", path = path.display(), error = e));
                    cx.notify();
                }
            });
        })
        .detach();
    }

    pub(super) fn open_imported(
        &mut self,
        ged: Gedcom,
        path: PathBuf,
        file: String,
        note: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.set_document(ged, Some(path), true, window, cx);
        self.status = None;
        let g = &self.doc.as_ref().unwrap().ged;
        let findings: Vec<(Finding, bool)> = import::analyze(g).into_iter().map(|f| (f, true)).collect();
        let needs_root = g.declared_root().is_none() && g.count(RecordKind::Individual) > 0;
        if findings.is_empty() && !needs_root {
            self.set_status(format!("{}{note}", t!("import.already_conform", file = file)), false);
            return self.offer_tour(window, cx);
        }
        let root = needs_root.then(|| picker(g, RecordKind::Individual, g.root().as_deref().unwrap_or(""), window, cx));
        self.add_dialog = Some(AddDialog::Import(Box::new(ImportDialog { file, root, findings, note })));
    }

    /// Closes the wizard: declares the chosen root person and, with `normalize`, converts the
    /// checked findings, in one change.
    fn apply_import(&mut self, normalize: bool, window: &mut Window, cx: &mut Context<Self>) {
        let Some(AddDialog::Import(d)) = self.add_dialog.take() else { return };
        let ids: Vec<&'static str> = match normalize {
            true => d.findings.iter().filter(|(_, on)| *on).map(|(f, _)| f.id).collect(),
            false => Vec::new(),
        };
        let root = d.root.as_ref().and_then(|r| r.read(cx).selected_value().cloned());
        if ids.is_empty() && root.is_none() {
            self.set_status(format!("{}{}", t!("import.imported", file = d.file), d.note), false);
        } else {
            let chosen = root.clone();
            if !ids.is_empty() {
                self.restore_point(t!("versions.before_import").into());
            }
            self.mutate(window, cx, move |g| {
                if let Some(r) = &root {
                    g.set_root(r);
                }
                let n: usize = import::apply(g, &ids).iter().map(|(_, n)| n).sum();
                let msg = match n {
                    0 => t!("import.imported", file = d.file),
                    n => crate::tn!("import.normalized", n, file = d.file),
                };
                Ok((format!("{msg}{}", d.note), None))
            });
            if let Some(r) = chosen {
                self.select(r, window, cx);
            }
        }
        window.focus(&self.focus, cx);
        self.offer_tour(window, cx);
        cx.notify();
    }

    pub(super) fn import_title(&self, d: &ImportDialog) -> String {
        t!("import.title", file = d.file).to_string()
    }

    pub(super) fn import_body(&self, d: &ImportDialog, cx: &mut Context<Self>) -> AnyElement {
        let any = d.findings.iter().any(|(_, on)| *on);
        let normalize = !d.findings.is_empty();
        v_flex()
            .gap_4()
            .when_some(d.root.as_ref(), |el, root| {
                el.child(
                    v_flex().gap_1().child(label_caps(t!("settings.root"))).child(
                        Select::new(root).small().search_placeholder(t!("event_dialog.search")).into_any_element(),
                    ),
                )
            })
            .when(normalize, |el| {
                el.child(v_flex().gap_2().child(label_caps(t!("import.convert"))).children(
                    d.findings.iter().enumerate().map(|(i, (f, on))| {
                        Checkbox::new(SharedString::from(format!("import-{}", f.id)))
                            .checked(*on)
                            .label(format!("{} → {}", f.label, f.into))
                            .on_click(cx.listener(move |this, checked: &bool, _, cx| {
                                if let Some(AddDialog::Import(d)) = &mut this.add_dialog {
                                    d.findings[i].1 = *checked;
                                }
                                cx.notify();
                            }))
                    }),
                ))
            })
            .child(
                h_flex()
                    .gap_2()
                    .justify_end()
                    .when(normalize, |el| {
                        el.child(
                            Button::new("import-skip")
                                .ghost()
                                .small()
                                .label(t!("import.keep"))
                                .on_click(cx.listener(|this, _, w, cx| this.apply_import(false, w, cx))),
                        )
                    })
                    .child(
                        Button::new("import-apply")
                            .primary()
                            .small()
                            .icon(IconName::Check)
                            .label(if normalize { t!("import.normalize") } else { t!("import.done") })
                            .disabled(normalize && !any)
                            .on_click(cx.listener(|this, _, w, cx| this.apply_import(true, w, cx))),
                    ),
            )
            .into_any_element()
    }
}
