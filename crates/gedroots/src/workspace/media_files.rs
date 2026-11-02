//! Rangement des fichiers médias selon Réglages du fichier › Médias
//! (`gedcom::media_naming`) :
//! - un fichier rattaché depuis l'extérieur du dossier du .ged y est copié,
//!   sous le nom donné par le modèle ;
//! - les fichiers d'un nouvel acte (déposés ou choisis dans l'éditeur d'acte)
//!   prennent directement le nom du modèle : copiés s'ils viennent d'ailleurs,
//!   déplacés s'ils sont déjà dans le dossier et ne servent à aucun autre média ;
//! - Média › Renommer selon le modèle… déplace les fichiers d'un média.
//!
//! Le déplacement d'un fichier ne s'annule pas : l'historique d'annulation est
//! mis à jour avec le nouveau chemin pour ne jamais pointer vers l'ancien.

use super::add::AddDialog;
use super::*;
use std::path::{Path, PathBuf};

pub(super) struct RenameDialog {
    media: String,
    /// (chemin actuel, nouveau chemin)
    rows: Vec<(String, String)>,
}

/// Un fichier déplacé ou copié : (média, position du `FILE`, ancien chemin, nouveau chemin).
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

fn relative(base: &Path, p: &Path) -> String {
    match p.strip_prefix(base) {
        Ok(rel) => rel.components().map(|c| c.as_os_str().to_string_lossy()).collect::<Vec<_>>().join("/"),
        Err(_) => p.to_string_lossy().to_string(),
    }
}

/// Range les fichiers des médias selon le réglage. Les fichiers hors du dossier
/// du .ged sont copiés ; ceux du dossier ne sont déplacés que si `move_inside`
/// l'accepte (média, chemin actuel).
fn place_files(
    g: &mut Gedcom,
    base: &Path,
    media: &[String],
    move_inside: &dyn Fn(&Gedcom, &str, &str) -> bool,
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
                std::fs::create_dir_all(dir).map_err(|e| format!("dossier {} : {e}", dir.display()))?;
            }
            let done = if inside { std::fs::rename(&from, &to) } else { std::fs::copy(&from, &to).map(|_| ()) };
            done.map_err(|e| format!("{} : {e}", from.display()))?;
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

/// Après un rattachement : copie dans le dossier des médias les fichiers venus
/// d'ailleurs (`base` : dossier du .ged). Renvoie le complément du message d'état.
pub(super) fn store_new_media(g: &mut Gedcom, base: Option<&Path>, media: &[String]) -> String {
    let outside = media
        .iter()
        .any(|m| g.get(m).is_some_and(|n| n.children_with("FILE").any(|f| Path::new(f.val()).is_absolute())));
    if !outside {
        return String::new();
    }
    let Some(base) = base.filter(|_| !g.media_naming().is_empty()) else {
        return " — attention : fichier hors du dossier du .ged, chemin absolu enregistré".into();
    };
    match place_files(g, base, media, &|_, _, _| false) {
        Ok(m) if !m.is_empty() => {
            let n = m.len();
            format!(" · {n} copié{} dans le dossier du fichier", if n > 1 { "s" } else { "" })
        }
        Ok(_) => String::new(),
        Err(e) => format!(" — copie impossible : {e}"),
    }
}

/// Fichiers du média créé par un acte : nommés selon le modèle, copiés s'ils
/// viennent d'ailleurs, déplacés s'ils sont dans le dossier du .ged et
/// qu'aucun autre média ne les utilise. Renvoie le complément du message d'état.
pub(super) fn store_act_media(g: &mut Gedcom, base: Option<&Path>, media: &str) -> String {
    let Some(base) = base.filter(|_| !g.media_naming().is_empty()) else {
        return store_new_media(g, base, &[media.to_string()]);
    };
    let alone = |g: &Gedcom, m: &str, file: &str| {
        !g.records_of(RecordKind::Media)
            .any(|o| o.xref.as_deref() != Some(m) && o.children_with("FILE").any(|f| f.val() == file))
    };
    match place_files(g, base, &[media.to_string()], &alone) {
        Ok(m) if !m.is_empty() => {
            let n = m.len();
            format!(
                " · {n} fichier{} nommé{} selon le modèle",
                if n > 1 { "s" } else { "" },
                if n > 1 { "s" } else { "" }
            )
        }
        Ok(_) => String::new(),
        Err(e) => format!(" — rangement impossible : {e}"),
    }
}

impl Workspace {
    /// Dossier du .ged, s'il est enregistré.
    pub(super) fn ged_dir(&self) -> Option<PathBuf> {
        self.doc.as_ref()?.path.as_ref()?.parent().map(Path::to_path_buf)
    }

    pub(super) fn open_rename_media(&mut self, media: String, cx: &mut Context<Self>) {
        let Some(doc) = &self.doc else { return };
        let rows: Vec<(String, String)> =
            doc.ged.media_renames(&media).into_iter().filter(|(_, a, b)| a != b).map(|(_, a, b)| (a, b)).collect();
        if rows.is_empty() {
            self.set_status("Les fichiers de ce média portent déjà le nom du modèle", false);
        } else {
            self.add_dialog = Some(AddDialog::RenameMedia(Box::new(RenameDialog { media, rows })));
        }
        cx.notify();
    }

    fn rename_media(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(AddDialog::RenameMedia(d)) = self.add_dialog.take() else { return };
        let Some(base) = self.ged_dir() else {
            self.set_status("Enregistrez d'abord le fichier .ged", true);
            cx.notify();
            return;
        };
        let Some(doc) = &mut self.doc else { return };
        match place_files(&mut doc.ged, &base, std::slice::from_ref(&d.media), &|_, _, _| true) {
            Ok(moved) => {
                // Les états précédents pointent aussi vers le nouveau nom : annuler une
                // modification antérieure ne doit pas casser le lien vers le fichier déplacé.
                for h in &mut doc.history {
                    for (m, i, old, new) in &moved {
                        h.replace_media_file(m, *i, old, new);
                    }
                }
                doc.dirty = true;
                let n = moved.len();
                self.set_status(
                    format!("{n} fichier{} renommé{}", if n > 1 { "s" } else { "" }, if n > 1 { "s" } else { "" }),
                    false,
                );
            }
            Err(e) => self.set_status(format!("Renommage interrompu : {e}"), true),
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
                        .label("Renommer")
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

    /// Fichier .ged de test (variable `GEDROOTS_TEST_GED` : une copie d'un vrai fichier).
    fn sample() -> (Gedcom, String) {
        match std::env::var("GEDROOTS_TEST_GED") {
            Ok(p) => {
                let g = Gedcom::parse(&std::fs::read_to_string(p).unwrap()).unwrap();
                let s = g.records_of(RecordKind::Source).next().and_then(|s| s.xref.clone()).unwrap();
                (g, s)
            }
            Err(_) => {
                (Gedcom::parse(include_str!("../../../gedcom/tests/fixtures/exemple.ged")).unwrap(), "S00001".into())
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
                template: "{type:1}_{nom:simple}_{prenom}".into(),
            }],
        });
        // Un fichier du dossier déjà utilisé par un autre média ne doit pas bouger.
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
        assert!(note.contains("2 fichiers nommés"), "{note}");

        let files: Vec<String> = g.get(&m).unwrap().children_with("FILE").map(|f| f.val().to_string()).collect();
        assert_eq!(files, ["Actes/N_MARCHAND_Louise p1.jpg", "Actes/N_MARCHAND_Louise p2.pdf", "partagé.jpg"]);
        assert!(base.join("Actes/N_MARCHAND_Louise p1.jpg").is_file());
        assert!(outside.is_file(), "un fichier venu d'ailleurs est copié, pas déplacé");
        assert!(base.join("Actes/N_MARCHAND_Louise p2.pdf").is_file());
        assert!(!base.join("scan.pdf").exists(), "un fichier du dossier est déplacé");
        assert!(base.join("partagé.jpg").is_file());
        assert_eq!(g.get(&other).unwrap().child("FILE").unwrap().val(), "partagé.jpg");
        let _ = std::fs::remove_dir_all(&root);
    }
}
