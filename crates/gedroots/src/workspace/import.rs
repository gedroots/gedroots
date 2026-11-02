//! Assistant d'import (Fichier › Importer un GEDCOM…) : ouvre le fichier, liste
//! ce qu'il contient de non standard (voir `gedcom::import`) et propose de le
//! convertir en GEDCOM 7. La conversion s'annule comme toute modification.

use super::add::AddDialog;
use super::*;
use gedcom::import::{self, Finding};
use gpui_kit::component::checkbox::Checkbox;
use rust_i18n::t;

pub(super) struct ImportDialog {
    file: String,
    findings: Vec<(Finding, bool)>,
}

impl Workspace {
    pub(super) fn import_file(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let rx = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some(t!("import.prompt").into()),
        });
        cx.spawn_in(window, async move |this, cx| {
            if let Ok(Ok(Some(mut paths))) = rx.await
                && let Some(p) = paths.pop()
            {
                let _ = this.update_in(cx, |this, window, cx| {
                    this.load(p, window, cx);
                    this.open_import_wizard(cx);
                });
            }
        })
        .detach();
    }

    /// Analyse le fichier ouvert et affiche l'assistant s'il y a quelque chose à normaliser.
    fn open_import_wizard(&mut self, cx: &mut Context<Self>) {
        let Some(doc) = &self.doc else { return };
        let file = doc.path.as_ref().and_then(|p| p.file_name()).map(|n| n.to_string_lossy().to_string());
        let file = file.unwrap_or_default();
        let findings = import::analyze(&doc.ged);
        if findings.is_empty() {
            self.set_status(t!("import.already_conform", file = file), false);
        } else {
            let findings = findings.into_iter().map(|f| (f, true)).collect();
            self.add_dialog = Some(AddDialog::Import(Box::new(ImportDialog { file, findings })));
        }
        cx.notify();
    }

    fn apply_import(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(AddDialog::Import(d)) = self.add_dialog.take() else { return };
        let ids: Vec<&'static str> = d.findings.iter().filter(|(_, on)| *on).map(|(f, _)| f.id).collect();
        if !ids.is_empty() {
            self.mutate(window, cx, move |g| {
                let n: usize = import::apply(g, &ids).iter().map(|(_, n)| n).sum();
                Ok((t!("import.normalized", file = d.file, n = n).to_string(), None))
            });
        }
        window.focus(&self.focus, cx);
        cx.notify();
    }

    pub(super) fn import_title(&self, d: &ImportDialog) -> String {
        t!("import.title", file = d.file).to_string()
    }

    pub(super) fn import_body(&self, d: &ImportDialog, cx: &mut Context<Self>) -> AnyElement {
        let any = d.findings.iter().any(|(_, on)| *on);
        v_flex()
            .gap_3()
            .children(d.findings.iter().enumerate().map(|(i, (f, on))| {
                Checkbox::new(SharedString::from(format!("import-{}", f.id)))
                    .checked(*on)
                    .label(format!("{} → {}", f.label, f.into))
                    .on_click(cx.listener(move |this, checked: &bool, _, cx| {
                        if let Some(AddDialog::Import(d)) = &mut this.add_dialog {
                            d.findings[i].1 = *checked;
                        }
                        cx.notify();
                    }))
            }))
            .child(
                h_flex()
                    .gap_2()
                    .justify_end()
                    .child(Button::new("import-skip").ghost().small().label(t!("import.keep")).on_click(cx.listener(
                        |this, _, w, cx| {
                            this.add_dialog = None;
                            window_focus(this, w, cx);
                        },
                    )))
                    .child(
                        Button::new("import-apply")
                            .primary()
                            .small()
                            .icon(IconName::Check)
                            .label(t!("import.normalize"))
                            .disabled(!any)
                            .on_click(cx.listener(|this, _, w, cx| this.apply_import(w, cx))),
                    ),
            )
            .into_any_element()
    }
}

fn window_focus(this: &mut Workspace, window: &mut Window, cx: &mut Context<Workspace>) {
    window.focus(&this.focus, cx);
    cx.notify();
}
