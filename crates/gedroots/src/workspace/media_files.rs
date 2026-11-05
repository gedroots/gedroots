//! Organization of media files according to File Settings › Media (`gedcom::media_naming`):
//! - a file attached from outside the .ged folder is copied there, under the name given by the
//!   template;
//! - the files of a new record (dropped or chosen in the record editor) directly take the
//!   template’s name: copied if they come from elsewhere, moved if they are already in the folder
//!   and used by no other media;
//! - Media › Rename Using Template… moves the files of a media.
//!
//! Moving a file cannot be undone: the undo history is updated with the new path so it never points
//! to the old one.

use super::add::AddDialog;
use super::*;
use crate::t;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

pub(super) struct RenameDialog {
    media: String,
    /// (chemin actuel, nouveau chemin)
    rows: Vec<(String, String)>,
}

/// A moved or copied file: (media, position of the `FILE`, old path, new path).
type Moved = (String, usize, String, String);

/// Chemin libre : `nom.jpg`, sinon `nom (2).jpg`, `nom (3).jpg`…
fn free_path(p: PathBuf, current: &Path) -> PathBuf {
    if !p.exists() || p == current {
        return p;
    }
    let stem = p.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
    let ext = p.extension().map(|e| format!(".{}", e.to_string_lossy())).unwrap_or_default();
    (2..).map(|n| p.with_file_name(format!("{stem} ({n}){ext}"))).find(|c| !c.exists()).unwrap()
}

/// Preview of [`free_path`]: relative name `new` will get in `base`, without overwriting an
/// existing file or an already planned name (`taken`).
pub(super) fn free_name(base: &Path, new: &str, taken: &mut HashSet<PathBuf>) -> String {
    let p = base.join(new);
    let free = |c: &PathBuf| !c.exists() && !taken.contains(c);
    let to = if free(&p) {
        p
    } else {
        let stem = p.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
        let ext = p.extension().map(|e| format!(".{}", e.to_string_lossy())).unwrap_or_default();
        (2..).map(|n| p.with_file_name(format!("{stem} ({n}){ext}"))).find(free).unwrap()
    };
    taken.insert(to.clone());
    relative(base, &to)
}

fn relative(base: &Path, p: &Path) -> String {
    match p.strip_prefix(base) {
        Ok(rel) => rel.components().map(|c| c.as_os_str().to_string_lossy()).collect::<Vec<_>>().join("/"),
        Err(_) => p.to_string_lossy().to_string(),
    }
}

/// Organizes the media files according to the setting. Files outside the .ged folder are copied
/// (moved if `move_outside`); those in the folder are only moved if `move_inside` accepts it
/// (media, current path).
fn place_files(
    g: &mut Gedcom,
    base: &Path,
    media: &[String],
    move_inside: &dyn Fn(&Gedcom, &str, &str) -> bool,
    move_outside: bool,
) -> Result<Vec<Moved>, String> {
    let mut moved = Vec::new();
    for m in media {
        for (i, old, new) in g.media_renames(m) {
            let inside = !Path::new(&old).is_absolute();
            if (inside && !move_inside(g, m, &old)) || old == new {
                continue;
            }
            let from = if inside { base.join(&old) } else { PathBuf::from(&old) };
            if !from.is_file() {
                continue;
            }
            let to = free_path(base.join(&new), &from);
            if to == from {
                continue;
            }
            if let Some(dir) = to.parent() {
                std::fs::create_dir_all(dir)
                    .map_err(|e| t!("media_files.folder_error", path = dir.display(), error = e).to_string())?;
            }
            let done = if inside || move_outside {
                // Autre disque : copie puis suppression de l'original.
                std::fs::rename(&from, &to)
                    .or_else(|_| std::fs::copy(&from, &to).and_then(|_| std::fs::remove_file(&from)))
            } else {
                std::fs::copy(&from, &to).map(|_| ())
            };
            done.map_err(|e| t!("media_files.file_error", path = from.display(), error = e).to_string())?;
            let rel = relative(base, &to);
            g.replace_media_file(m, i, &old, &rel);
            moved.push((m.clone(), i, old, rel));
        }
        if moved.iter().any(|(x, ..)| x == m)
            && let Some(r) = g.get_mut(m)
        {
            r.touch();
        }
    }
    Ok(moved)
}

/// After attaching: copies the files coming from elsewhere into the media folder (`base`: the .ged
/// folder). Returns the complement of the status message.
pub(super) fn store_new_media(g: &mut Gedcom, base: Option<&Path>, media: &[String]) -> String {
    let outside = media
        .iter()
        .any(|m| g.get(m).is_some_and(|n| n.children_with("FILE").any(|f| Path::new(f.val()).is_absolute())));
    if !outside {
        return String::new();
    }
    let Some(base) = base.filter(|_| !g.media_naming().is_empty()) else {
        return t!("media_files.outside").into_owned();
    };
    match place_files(g, base, media, &|_, _, _| false, false) {
        Ok(m) if !m.is_empty() => {
            let n = m.len();
            crate::tn!("media_files.copied", n).into_owned()
        }
        Ok(_) => String::new(),
        Err(e) => t!("media_files.copy_failed", error = e).into_owned(),
    }
}

/// Files of the media created by a record: named according to the template, copied if they come
/// from elsewhere, moved if they are in the .ged folder and no other media uses them. Returns the
/// complement of the status message.
pub(super) fn store_act_media(g: &mut Gedcom, base: Option<&Path>, media: &str) -> String {
    let Some(base) = base.filter(|_| !g.media_naming().is_empty()) else {
        return store_new_media(g, base, &[media.to_string()]);
    };
    let alone = |g: &Gedcom, m: &str, file: &str| {
        !g.records_of(RecordKind::Media)
            .any(|o| o.xref.as_deref() != Some(m) && o.children_with("FILE").any(|f| f.val() == file))
    };
    match place_files(g, base, &[media.to_string()], &alone, false) {
        Ok(m) if !m.is_empty() => {
            let n = m.len();
            crate::tn!("media_files.named", n).into_owned()
        }
        Ok(_) => String::new(),
        Err(e) => t!("media_files.store_failed", error = e).into_owned(),
    }
}

/// Files imported from the Media window: named according to the template, moved (`move_files`) or
/// copied if they come from elsewhere, moved if they are already in the folder and no other media
/// uses them. Returns the complement of the status message.
pub(super) fn store_imported(g: &mut Gedcom, base: Option<&Path>, media: &[String], move_files: bool) -> String {
    let Some(base) = base.filter(|_| !g.media_naming().is_empty()) else {
        return store_new_media(g, base, media);
    };
    let alone = |g: &Gedcom, m: &str, file: &str| {
        !g.records_of(RecordKind::Media)
            .any(|o| o.xref.as_deref() != Some(m) && o.children_with("FILE").any(|f| f.val() == file))
    };
    match place_files(g, base, media, &alone, move_files) {
        Ok(m) if !m.is_empty() => {
            let n = m.len();
            if move_files {
                crate::tn!("media_files.placed_moved", n).into_owned()
            } else {
                crate::tn!("media_files.placed_copied", n).into_owned()
            }
        }
        Ok(_) => String::new(),
        Err(e) => t!("media_files.store_failed", error = e).into_owned(),
    }
}

/// Paths the files of a media would get according to the template (preview, before any move):
/// `(current path, new path)`.
pub(super) fn planned_paths(g: &Gedcom, media: &str) -> Vec<(String, String)> {
    if g.media_naming().is_empty() {
        return Vec::new();
    }
    g.media_renames(media).into_iter().map(|(_, a, b)| (a, b)).collect()
}

impl Workspace {
    /// Folder of the .ged, if it is saved.
    pub(super) fn ged_dir(&self) -> Option<PathBuf> {
        self.doc.as_ref()?.path.as_ref()?.parent().map(Path::to_path_buf)
    }

    pub(super) fn open_rename_media(&mut self, media: String, cx: &mut Context<Self>) {
        let Some(doc) = &self.doc else { return };
        let rows: Vec<(String, String)> =
            doc.ged.media_renames(&media).into_iter().filter(|(_, a, b)| a != b).map(|(_, a, b)| (a, b)).collect();
        if rows.is_empty() {
            self.set_status(t!("media_files.already_named"), false);
        } else {
            self.add_dialog = Some(AddDialog::RenameMedia(Box::new(RenameDialog { media, rows })));
        }
        cx.notify();
    }

    fn rename_media(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(AddDialog::RenameMedia(d)) = self.add_dialog.take() else { return };
        let Some(base) = self.ged_dir() else {
            self.set_error("media_files.save_first", t!("media_files.save_first"));
            cx.notify();
            return;
        };
        let Some(doc) = &mut self.doc else { return };
        match place_files(&mut doc.ged, &base, std::slice::from_ref(&d.media), &|_, _, _| true, false) {
            Ok(moved) => {
                // The previous states also point to the new name: undoing an earlier change must
                // not break the link to the moved file.
                for h in &mut doc.history {
                    for (m, i, old, new) in &moved {
                        h.replace_media_file(m, *i, old, new);
                    }
                }
                doc.dirty = true;
                let n = moved.len();
                self.set_status(crate::tn!("media_files.renamed", n), false);
            }
            Err(e) => self.set_error("media_files.rename_failed", t!("media_files.rename_failed", error = e)),
        }
        self.rescan_media();
        self.rebuild_rows(cx);
        self.refresh_editor(window, cx);
        window.focus(&self.focus, cx);
        cx.notify();
    }

    pub(super) fn rename_body(&self, d: &RenameDialog, cx: &mut Context<Self>) -> AnyElement {
        v_flex()
            .gap_3()
            .child(v_flex().gap_2().children(d.rows.iter().map(|(old, new)| {
                v_flex()
                    .gap_0p5()
                    .text_sm()
                    .child(div().text_color(theme::muted()).truncate().child(old.clone()))
                    .child(div().truncate().child(format!("→ {new}")))
            })))
            .child(
                h_flex().gap_2().justify_end().child(
                    Button::new("rename-media")
                        .primary()
                        .small()
                        .icon(IconName::Check)
                        .label(t!("media_files.rename"))
                        .on_click(cx.listener(|this, _, w, cx| this.rename_media(w, cx))),
                ),
            )
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::store_act_media;
    use gedcom::{Act, ActRole, Gedcom, MediaChoice, MediaFolder, MediaNaming, Participant, PersonField, RecordKind};

    /// Test .ged file (`GEDROOTS_TEST_GED` variable: a copy of a real file).
    fn sample() -> (Gedcom, String) {
        match std::env::var("GEDROOTS_TEST_GED") {
            Ok(p) => {
                let g = Gedcom::parse(&std::fs::read_to_string(p).unwrap()).unwrap();
                let s = g.records_of(RecordKind::Source).next().and_then(|s| s.xref.clone()).unwrap();
                (g, s)
            }
            Err(_) => {
                (Gedcom::parse(include_str!("../../../gedcom/tests/fixtures/example.ged")).unwrap(), "S00001".into())
            }
        }
    }

    #[test]
    fn act_files_are_copied_or_moved_under_the_template_name() {
        let root = std::env::temp_dir().join(format!("gedroots-act-media-{}", std::process::id()));
        let (base, elsewhere) = (root.join("arbre"), root.join("Téléchargements"));
        std::fs::create_dir_all(&base).unwrap();
        std::fs::create_dir_all(&elsewhere).unwrap();
        let outside = elsewhere.join("IMG_0412.JPG");
        std::fs::write(&outside, b"jpg").unwrap();
        std::fs::write(base.join("scan.pdf"), b"pdf").unwrap();
        std::fs::write(base.join("partagé.jpg"), b"jpg").unwrap();

        let (mut g, source) = sample();
        g.set_media_naming(&MediaNaming {
            root: String::new(),
            folders: vec![MediaFolder {
                dir: "Actes".into(),
                types: vec!["BIRT".into()],
                template: "{type:1}_{surname:simple}_{given}".into(),
            }],
        });
        // A file of the folder already used by another media must not move.
        let other = g.media_for_file("partagé.jpg");
        let mut act = Act::new(&source, "BIRT");
        act.date = "3 FEB 1914".into();
        act.participants = vec![
            Participant::new(ActRole::Principal)
                .with(PersonField::Surname, "MARCHAND")
                .with(PersonField::Given, "Louise")
                .with(PersonField::Sex, "F"),
        ];
        act.media =
            MediaChoice::NewFiles(vec![outside.to_string_lossy().to_string(), "scan.pdf".into(), "partagé.jpg".into()]);
        let m = act.save(&mut g).unwrap().media.unwrap();
        let note = store_act_media(&mut g, Some(&base), &m);
        assert!(note.contains("2 files named"), "{note}");

        let files: Vec<String> = g.get(&m).unwrap().children_with("FILE").map(|f| f.val().to_string()).collect();
        assert_eq!(files, ["Actes/B_MARCHAND_Louise p1.jpg", "Actes/B_MARCHAND_Louise p2.pdf", "partagé.jpg"]);
        assert!(base.join("Actes/B_MARCHAND_Louise p1.jpg").is_file());
        assert!(outside.is_file(), "un fichier venu d'ailleurs est copié, pas déplacé");
        assert!(base.join("Actes/B_MARCHAND_Louise p2.pdf").is_file());
        assert!(!base.join("scan.pdf").exists(), "un fichier du dossier est déplacé");
        assert!(base.join("partagé.jpg").is_file());
        assert_eq!(g.get(&other).unwrap().child("FILE").unwrap().val(), "partagé.jpg");
        let _ = std::fs::remove_dir_all(&root);
    }
}
