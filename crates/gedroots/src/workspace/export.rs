//! Fichier › Exporter en GEDZIP… : une archive `.gdz` (GEDCOM 7, chapitre 4)
//! avec `gedcom.ged` et les fichiers des médias (voir `gedcom::gedzip`).
//!
//! L'export part du fichier tel qu'il est affiché, modifications non
//! enregistrées comprises ; le `.ged` et les médias ne sont pas touchés. Les
//! fichiers introuvables ne bloquent pas l'export (leur `FILE` reste tel quel) : ils sont listés à la fin,
//! chacun menant à son média.

use super::add::AddDialog;
use super::*;
use crate::gedzip::{self, Entry};
use std::path::Path;

/// Fichiers qui manquaient à l'archive : (média, valeur `FILE`).
pub(super) struct GedzipReport {
    file: String,
    missing: Vec<(String, String)>,
}

impl Workspace {
    pub(super) fn export_gedzip(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(doc) = &self.doc else { return };
        let dir = doc.path.as_ref().and_then(|p| p.parent()).map(Path::to_path_buf).unwrap_or_else(dirs_home);
        let stem = doc.path.as_ref().and_then(|p| p.file_stem()).map(|s| s.to_string_lossy().to_string());
        let name = format!("{}.{}", stem.as_deref().unwrap_or("arbre"), gedcom::gedzip::EXTENSION);
        let rx = cx.prompt_for_new_path(&dir, Some(&name));
        cx.spawn_in(window, async move |this, cx| {
            if let Ok(Ok(Some(p))) = rx.await {
                let _ = this.update_in(cx, |this, window, cx| this.write_gedzip(p, window, cx));
            }
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
            // Un fichier disparu entre-temps fait échouer l'écriture, avec son chemin.
            .map(|f| Entry { source: find_file(doc, &f.original).unwrap_or_else(|| f.original.into()), name: f.name })
            .collect();
        let missing: Vec<(String, String)> =
            packed.missing.into_iter().map(|m| (m.media.into_iter().next().unwrap_or_default(), m.original)).collect();
        // Le dialogue d'enregistrement a déjà demandé s'il fallait remplacer
        // `dest` ; jamais le .ged ouvert ni un fichier qu'on met dans l'archive.
        let same =
            |p: &Path| std::fs::canonicalize(p).ok().is_some_and(|p| std::fs::canonicalize(&dest).ok() == Some(p));
        if doc.path.as_deref().is_some_and(same) || entries.iter().any(|e| same(&e.source)) {
            self.set_status("Choisissez un autre nom pour l'archive : ce fichier fait partie de l'arbre", true);
            cx.notify();
            return;
        }
        let text = packed.ged.to_gedcom_string();
        let count = entries.len();
        let file = dest.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
        self.set_status(format!("Export de {file}…"), false);
        cx.notify();
        let task = cx.background_spawn(async move { gedzip::write(&dest, &text, &entries).map(|()| dest) });
        cx.spawn_in(window, async move |this, cx| {
            let res = task.await;
            let _ = this.update(cx, |this, cx| {
                match res {
                    Ok(dest) => {
                        let files = match count {
                            0 => "sans fichier".to_string(),
                            1 => "1 fichier".to_string(),
                            n => format!("{n} fichiers"),
                        };
                        let mut msg = format!("Exporté dans {} · {files}", dest.display());
                        if !missing.is_empty() {
                            msg.push_str(&format!(" · {} introuvable(s)", missing.len()));
                            this.add_dialog = Some(AddDialog::Gedzip(Box::new(GedzipReport { file, missing })));
                        }
                        this.set_status(msg, false);
                    }
                    Err(e) => this.set_status(format!("Échec de l'export GEDZIP : {e}"), true),
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub(super) fn gedzip_title(&self, d: &GedzipReport) -> String {
        format!("{} · fichiers introuvables", d.file)
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

/// Fichier d'une valeur `FILE` sur le disque (relative au dossier du .ged).
fn find_file(doc: &Document, file: &str) -> Option<PathBuf> {
    let unix = file.replace('\\', "/");
    [file, unix.as_str()].into_iter().filter_map(|f| super::pages::file_path(doc, f)).find(|p| p.is_file())
}
