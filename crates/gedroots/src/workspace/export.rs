//! File › Export…: one native save dialog, whose format list offers
//! - GEDZIP: a `.gdz` archive (GEDCOM 7, chapter 4) with `gedcom.ged` and the media files (see
//!   `gedcom::gedzip`);
//! - GEDCOM 5.5.1 and GEDCOM 5.5.1 for Geneanet: a 5.5.1 `.ged` for software and websites that do
//!   not read GEDCOM 7 (see `gedcom::gedcom551`). After the Geneanet export, its import page opens
//!   in the browser.
//!
//! The format is chosen in the dialog itself, like Preview's File › Export (see `save_dialog`).
//!
//! The export starts from the file as displayed, unsaved changes included; the `.ged` and the media
//! are not touched. Missing files do not block the export (their `FILE` stays as is): they are
//! listed at the end, each leading to its media.

use super::add::AddDialog;
use super::*;
use crate::gedzip::{self, Entry};
use crate::save_dialog;
use crate::t;
use gedcom::gedcom551::Flavor;
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};

/// Files missing from the archive: (media, `FILE` value).
pub(super) struct GedzipReport {
    file: String,
    missing: Vec<(String, String)>,
}

/// Formats of File › Export…, in the order of the dialog's list.
#[derive(Clone, Copy, PartialEq, Debug)]
enum ExportFormat {
    Gedzip,
    Gedcom551,
    Geneanet,
}

const FORMATS: [ExportFormat; 3] = [ExportFormat::Gedzip, ExportFormat::Gedcom551, ExportFormat::Geneanet];

/// Format chosen at the last export, selected at the next one (for the session).
static LAST_FORMAT: AtomicUsize = AtomicUsize::new(0);

impl ExportFormat {
    fn dialog_format(self) -> save_dialog::Format {
        match self {
            Self::Gedzip => save_dialog::Format {
                label: t!("export.format.gedzip").into(),
                extension: gedcom::gedzip::EXTENSION,
                suffix: "",
            },
            Self::Gedcom551 => save_dialog::Format {
                label: t!("export.format.gedcom551").into(),
                extension: "ged",
                suffix: " (5.5.1)",
            },
            Self::Geneanet => save_dialog::Format {
                label: t!("export.format.geneanet").into(),
                extension: "ged",
                suffix: " (Geneanet)",
            },
        }
    }
}

impl Workspace {
    /// File › Export…: one save dialog, the format chosen in it (see `save_dialog`).
    pub(super) fn export(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(doc) = &self.doc else { return };
        let directory = doc.path.as_ref().and_then(|p| p.parent()).map(Path::to_path_buf).unwrap_or_else(dirs_home);
        let stem = doc
            .path
            .as_ref()
            .and_then(|p| p.file_stem())
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| t!("export.default_name").into());
        let request = save_dialog::Request {
            title: t!("export.title").into(),
            format_label: t!("export.format").into(),
            directory,
            stem,
            formats: FORMATS.iter().map(|f| f.dialog_format()).collect(),
            selected: LAST_FORMAT.load(Ordering::Relaxed),
        };
        let answer = save_dialog::prompt(window, request);
        cx.spawn_in(window, async move |this, cx| {
            let answer = answer.await;
            let _ = this.update_in(cx, |this, window, cx| match answer {
                Ok(Some(choice)) => {
                    let Some(&format) = FORMATS.get(choice.format) else { return };
                    LAST_FORMAT.store(choice.format, Ordering::Relaxed);
                    match format {
                        ExportFormat::Gedzip => this.write_gedzip(choice.path, window, cx),
                        ExportFormat::Gedcom551 => this.write_gedcom(Flavor::Standard, choice.path, window, cx),
                        ExportFormat::Geneanet => this.write_gedcom(Flavor::Geneanet, choice.path, window, cx),
                    }
                }
                Ok(None) => {}
                Err(e) => {
                    this.set_error("export.dialog_failed", t!("export.dialog_failed", error = e));
                    cx.notify();
                }
            });
        })
        .detach();
    }

    fn write_gedcom(&mut self, flavor: Flavor, dest: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        let Some(doc) = &self.doc else { return };
        let same =
            |p: &Path| std::fs::canonicalize(p).ok().is_some_and(|p| std::fs::canonicalize(&dest).ok() == Some(p));
        if doc.path.as_deref().is_some_and(same) {
            self.set_error("export.replaces_open", t!("export.replaces_open"));
            cx.notify();
            return;
        }
        let text = gedcom::gedcom551::export(&doc.ged, flavor);
        let task = cx.background_spawn(async move {
            // Written next to it, then renamed: never a half-written file.
            let mut part = dest.as_os_str().to_owned();
            part.push(".part");
            let part = PathBuf::from(part);
            let res = std::fs::write(&part, text).and_then(|()| std::fs::rename(&part, &dest));
            if res.is_err() {
                let _ = std::fs::remove_file(&part);
            }
            res.map(|()| dest).map_err(|e| e.to_string())
        });
        cx.spawn_in(window, async move |this, cx| {
            let res = task.await;
            let _ = this.update(cx, |this, cx| {
                match res {
                    Ok(dest) => {
                        let mut msg = t!("export.exported", path = dest.display()).into_owned();
                        if flavor == Flavor::Geneanet {
                            cx.open_url(gedcom::gedcom551::GENEANET_IMPORT_URL);
                            msg.push_str(&t!("export.geneanet_upload"));
                        }
                        this.set_status(msg, false);
                    }
                    Err(e) => this.set_error("export.gedcom_failed", t!("export.gedcom_failed", error = e)),
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn write_gedzip(&mut self, dest: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        let Some(doc) = &self.doc else { return };
        let mut packed = gedcom::gedzip::pack(&doc.ged, &|f| find_file(doc, f).is_some());
        packed.ged.stamp_header();
        let entries: Vec<Entry> = packed
            .files
            .into_iter()
            // A file that disappeared in the meantime makes writing fail, with its path.
            .map(|f| Entry { source: find_file(doc, &f.original).unwrap_or_else(|| f.original.into()), name: f.name })
            .collect();
        let missing: Vec<(String, String)> =
            packed.missing.into_iter().map(|m| (m.media.into_iter().next().unwrap_or_default(), m.original)).collect();
        // The save dialog already asked whether to replace `dest`; never the open .ged nor a file
        // put in the archive.
        let same =
            |p: &Path| std::fs::canonicalize(p).ok().is_some_and(|p| std::fs::canonicalize(&dest).ok() == Some(p));
        if doc.path.as_deref().is_some_and(same) || entries.iter().any(|e| same(&e.source)) {
            self.set_error("export.name_taken", t!("export.name_taken"));
            cx.notify();
            return;
        }
        let text = packed.ged.to_gedcom_string();
        let count = entries.len();
        let file = dest.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
        self.set_status(t!("export.exporting", file = file), false);
        cx.notify();
        let task = cx.background_spawn(async move { gedzip::write(&dest, &text, &entries).map(|()| dest) });
        cx.spawn_in(window, async move |this, cx| {
            let res = task.await;
            let _ = this.update(cx, |this, cx| {
                match res {
                    Ok(dest) => {
                        let files = match count {
                            0 => t!("export.no_file"),
                            n => crate::tn!("export.files", n),
                        };
                        let mut msg = t!("export.done", path = dest.display(), files = files).to_string();
                        if !missing.is_empty() {
                            msg.push_str(&crate::tn!("export.missing", missing.len()));
                            this.add_dialog = Some(AddDialog::Gedzip(Box::new(GedzipReport { file, missing })));
                        }
                        this.set_status(msg, false);
                    }
                    Err(e) => this.set_error("export.failed", t!("export.failed", error = e)),
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub(super) fn gedzip_title(&self, d: &GedzipReport) -> String {
        t!("export.missing_title", file = d.file).to_string()
    }

    pub(super) fn gedzip_body(&self, d: &GedzipReport, cx: &mut Context<Self>) -> AnyElement {
        let g = self.doc.as_ref().map(|d| &d.ged);
        v_flex()
            .gap_3()
            .child(v_flex().id("gedzip-missing").max_h(px(360.)).overflow_y_scroll().gap_1().children(
                d.missing.iter().enumerate().map(|(i, (media, path))| {
                    let label = g.map(|g| g.record_label(media)).unwrap_or_default();
                    let media = media.clone();
                    h_flex()
                        .id(("gedzip-missing", i))
                        .px_2()
                        .py_1()
                        .gap_3()
                        .rounded(theme::RADIUS_SM)
                        .cursor_pointer()
                        .hover(|s| s.bg(theme::hover()))
                        .text_sm()
                        .child(div().flex_none().max_w(px(180.)).truncate().child(label))
                        .child(div().flex_1().min_w_0().truncate().text_color(theme::muted()).child(path.clone()))
                        .on_click(cx.listener(move |this, _, w, cx| {
                            this.add_dialog = None;
                            this.goto(media.clone(), w, cx);
                        }))
                }),
            ))
            .child(
                h_flex().justify_end().child(
                    Button::new("gedzip-ok")
                        .primary()
                        .small()
                        .label("OK")
                        .on_click(cx.listener(|this, _, w, cx| this.close_add_dialog(w, cx))),
                ),
            )
            .into_any_element()
    }
}

/// File of a `FILE` value on the disk (relative to the .ged folder).
fn find_file(doc: &Document, file: &str) -> Option<PathBuf> {
    find_file_in(doc.path.as_ref().and_then(|p| p.parent()), file)
}

/// File of a `FILE` value on the disk, relative to `dir` (the .ged folder).
pub(super) fn find_file_in(dir: Option<&Path>, file: &str) -> Option<PathBuf> {
    let unix = file.replace('\\', "/");
    [file, unix.as_str()].into_iter().filter_map(|f| super::pages::file_path_in(dir, f)).find(|p| p.is_file())
}
