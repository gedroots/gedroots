//! Médias à plusieurs pages (acte sur deux feuillets, registre…).
//!
//! En GEDCOM 7, un enregistrement `OBJE` peut porter plusieurs `FILE`, chacun
//! avec son `FORM` et son `TITL`, dans l'ordre des pages :
//!
//! ```text
//! 0 @M1@ OBJE
//! 1 FILE Actes/acte_1.jpg
//! 2 FORM image/jpeg
//! 2 TITL Acte de mariage
//! 1 FILE Actes/acte_2.jpg
//! 2 FORM image/jpeg
//! ```
//!
//! Les individus et les sources pointent alors vers un seul média. La norme
//! applique le `CROP` d'un lien au premier fichier ; pour situer une personne
//! sur une autre page, GedRoots ajoute au lien l'extension `_GR_FILE` (numéro
//! de la page, à partir de 1, voir `docs/extensions.md`) :
//!
//! ```text
//! 1 OBJE @M1@
//! 2 CROP
//! 3 TOP 120
//! 3 LEFT 340
//! 2 _GR_FILE 2
//! ```
//!
//! Un lien sans `CROP` ni `_GR_FILE` concerne le document entier.
//!
//! Fusionner plusieurs médias en un seul, ou séparer une page en un nouveau
//! média, réécrit tous les pointeurs du fichier (y compris dans les citations).

use crate::document::{Gedcom, RecordKind};
use crate::model::Media;
use crate::node::Node;
use crate::relations::mime_for;

/// Balise de la page visée par un lien `OBJE`.
pub const PAGE_TAG: &str = "_GR_FILE";
pub const PAGE_URI: &str = "https://github.com/antoninguyot/gedroots/blob/main/docs/extensions.md#_gr_file";

/// Un fichier (une page) d'un média.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MediaFile<'a> {
    pub path: &'a str,
    pub form: Option<&'a str>,
    pub title: Option<&'a str>,
}

impl<'a> Media<'a> {
    /// Les fichiers du média, dans l'ordre des pages.
    pub fn files(&self) -> Vec<MediaFile<'a>> {
        self.node
            .children_with("FILE")
            .map(|f| MediaFile { path: f.val(), form: f.child_val("FORM"), title: f.child_val("TITL") })
            .collect()
    }

    pub fn page_count(&self) -> usize {
        self.node.children_with("FILE").count()
    }

    /// Fichier de la page `page` (à partir de 1).
    pub fn page_file(&self, page: usize) -> Option<&'a str> {
        self.node.children_with("FILE").nth(page.checked_sub(1)?).map(|f| f.val())
    }
}

/// Page visée par un lien `OBJE` (1 sans `_GR_FILE`).
pub fn page_of_link(link: &Node) -> usize {
    link.child_val(PAGE_TAG).and_then(|v| v.trim().parse::<usize>().ok()).filter(|p| *p >= 1).unwrap_or(1)
}

/// Le lien vise-t-il une page précise (zone recadrée ou `_GR_FILE`) plutôt que le document entier ?
fn is_page_link(link: &Node) -> bool {
    link.child("CROP").is_some() || link.child(PAGE_TAG).is_some()
}

/// Écrit la page d'un lien : rien pour la première (lecture standard du `CROP`).
fn set_page(link: &mut Node, page: usize) {
    link.children.retain(|c| c.tag != PAGE_TAG);
    if page > 1 {
        // Après CROP et TITL, l'ordre de la norme pour les sous-structures du lien.
        let at = link.children.iter().position(|c| c.tag != "CROP" && c.tag != "TITL").unwrap_or(link.children.len());
        link.children.insert(at, Node::with_value(PAGE_TAG, page.to_string()));
    }
}

/// Retire le « (1/2) », « (2) »… final d'un titre de page.
pub fn strip_page_suffix(title: &str) -> &str {
    let t = title.trim_end();
    if let Some(inner) = t.strip_suffix(')')
        && let Some(open) = inner.rfind('(')
    {
        let body = inner[open + 1..].trim();
        let numeric = !body.is_empty()
            && body.split('/').all(|p| !p.trim().is_empty() && p.trim().chars().all(|c| c.is_ascii_digit()));
        if numeric {
            return inner[..open].trim_end();
        }
    }
    t
}

fn ptr(x: &str) -> String {
    format!("@{x}@")
}

/// Applique `f` à tous les liens `OBJE` vers `p` du nœud et de ses descendants.
/// `f` renvoie des liens à insérer juste après (copies vers un autre média).
fn for_links(n: &mut Node, p: &str, f: &mut impl FnMut(&mut Node) -> Vec<Node>) -> bool {
    let mut touched = false;
    let mut i = 0;
    while i < n.children.len() {
        if n.children[i].tag == "OBJE" && n.children[i].val() == p {
            let extra = f(&mut n.children[i]);
            touched = true;
            for (k, e) in extra.into_iter().enumerate() {
                n.children.insert(i + 1 + k, e);
                i += 1;
            }
        } else if for_links(&mut n.children[i], p, f) {
            touched = true;
        }
        i += 1;
    }
    touched
}

/// Supprime les liens `OBJE` en double (même cible, mêmes sous-structures) sous un même parent.
fn dedupe_links(n: &mut Node, p: &str) {
    let mut seen: Vec<Node> = Vec::new();
    n.children.retain(|c| {
        if c.tag == "OBJE" && c.val() == p {
            if seen.contains(c) {
                return false;
            }
            seen.push(c.clone());
        }
        true
    });
    for c in &mut n.children {
        dedupe_links(c, p);
    }
}

impl Gedcom {
    fn media_node(&self, media: &str) -> Result<&Node, String> {
        self.get(media).filter(|n| n.tag == "OBJE").ok_or_else(|| "média introuvable".to_string())
    }

    /// Réécrit les liens vers `media` dans tout le fichier ; touche les enregistrements modifiés.
    fn rewrite_links(&mut self, media: &str, mut f: impl FnMut(&mut Node) -> Vec<Node>, dedupe: &[&str]) {
        let p = ptr(media);
        let mut uses_pages = false;
        let mut wrap = |l: &mut Node| {
            let extra = f(l);
            uses_pages |= l.child(PAGE_TAG).is_some() || extra.iter().any(|e| e.child(PAGE_TAG).is_some());
            extra
        };
        for r in self.records.iter_mut() {
            if for_links(r, &p, &mut wrap) {
                for d in dedupe {
                    dedupe_links(r, &ptr(d));
                }
                r.touch();
            }
        }
        for_links(&mut self.header, &p, &mut wrap);
        if uses_pages {
            self.declare_extension(PAGE_TAG, PAGE_URI);
        }
    }

    /// Ajoute des pages (fichiers) à la fin d'un média. Les fichiers déjà présents
    /// dans ce média sont ignorés. Renvoie le nombre de pages ajoutées.
    pub fn add_media_pages(&mut self, media: &str, files: &[String]) -> Result<usize, String> {
        self.media_node(media)?;
        let node = self.get_mut(media).unwrap();
        let mut added = 0;
        for f in files {
            let f = f.trim();
            if f.is_empty() || node.children_with("FILE").any(|x| x.val() == f) {
                continue;
            }
            let file = Node::with_value("FILE", f).push(Node::with_value("FORM", mime_for(f)));
            let at = node.children.iter().rposition(|c| c.tag == "FILE").map(|i| i + 1);
            match at {
                Some(i) => node.children.insert(i, file),
                None => node.insert_child(file),
            }
            added += 1;
        }
        if added > 0 {
            node.touch();
        }
        Ok(added)
    }

    /// Retire une page d'un média (le fichier reste sur le disque). Les personnes
    /// situées sur cette page restent liées au média, sans zone.
    pub fn remove_media_page(&mut self, media: &str, page: usize) -> Result<(), String> {
        let n = Media::new(self.media_node(media)?).page_count();
        if n < 2 {
            return Err("un média garde au moins une page".into());
        }
        if page == 0 || page > n {
            return Err("page introuvable".into());
        }
        let base = base_title(self.get(media).unwrap());
        let node = self.get_mut(media).unwrap();
        let i = node.children.iter().enumerate().filter(|(_, c)| c.tag == "FILE").nth(page - 1).unwrap().0;
        node.children.remove(i);
        keep_title(node, base.as_deref());
        node.touch();
        self.rewrite_links(
            media,
            |l| {
                if is_page_link(l) {
                    let p = page_of_link(l);
                    if p == page {
                        l.children.retain(|c| c.tag != "CROP" && c.tag != PAGE_TAG);
                    } else if p > page {
                        set_page(l, p - 1);
                    }
                }
                vec![]
            },
            &[media],
        );
        Ok(())
    }

    /// Déplace une page (`from` → `to`, à partir de 1).
    pub fn move_media_page(&mut self, media: &str, from: usize, to: usize) -> Result<(), String> {
        let n = Media::new(self.media_node(media)?).page_count();
        if from == 0 || to == 0 || from > n || to > n {
            return Err("page introuvable".into());
        }
        if from == to {
            return Ok(());
        }
        let base = base_title(self.get(media).unwrap());
        let node = self.get_mut(media).unwrap();
        let idx: Vec<usize> =
            node.children.iter().enumerate().filter(|(_, c)| c.tag == "FILE").map(|(i, _)| i).collect();
        let mut files: Vec<Node> = idx.iter().map(|&i| node.children[i].clone()).collect();
        let moved = files.remove(from - 1);
        files.insert(to - 1, moved);
        for (slot, f) in idx.iter().zip(files) {
            node.children[*slot] = f;
        }
        keep_title(node, base.as_deref());
        node.touch();
        // Nouvelle position de chaque ancienne page.
        let mut order: Vec<usize> = (1..=n).collect();
        let m = order.remove(from - 1);
        order.insert(to - 1, m);
        let new_pos = |old: usize| order.iter().position(|&o| o == old).map(|i| i + 1).unwrap_or(old);
        self.rewrite_links(
            media,
            |l| {
                if is_page_link(l) {
                    set_page(l, new_pos(page_of_link(l)));
                }
                vec![]
            },
            &[],
        );
        Ok(())
    }

    /// Fusionne des médias en un seul média multipage, dans l'ordre donné : le
    /// premier reçoit les pages des suivants, qui sont supprimés. Tous les liens
    /// vers les médias fusionnés pointent ensuite vers le premier (les personnes
    /// situées sur une page gardent leur zone, avec le numéro de page).
    pub fn merge_media(&mut self, media: &[String]) -> Result<String, String> {
        let mut list: Vec<&str> = Vec::new();
        for m in media {
            self.media_node(m)?;
            if !list.contains(&m.as_str()) {
                list.push(m);
            }
        }
        if list.len() < 2 {
            return Err("choisissez au moins deux médias à fusionner".into());
        }
        let target = list[0].to_string();
        // Titre commun : « Acte de mariage (1/2) » et « (2/2) » → « Acte de mariage ».
        let titles: Vec<Option<String>> = list
            .iter()
            .flat_map(|m| {
                Media::new(self.get(m).unwrap())
                    .files()
                    .into_iter()
                    .map(|f| f.title.map(str::to_string))
                    .collect::<Vec<_>>()
            })
            .collect();
        let bases: Vec<&str> = titles.iter().flatten().map(|t| strip_page_suffix(t)).collect();
        let common = bases.first().filter(|b| bases.iter().all(|x| x == *b)).map(|b| b.to_string());

        let mut offset = Media::new(self.get(&target).unwrap()).page_count();
        for m in list[1..].iter() {
            let src = self.get(m).unwrap().clone();
            let pages = src.children_with("FILE").count();
            let node = self.get_mut(&target).unwrap();
            for c in src.children.iter().filter(|c| c.tag != "CREA" && c.tag != "CHAN") {
                if c.tag == "FILE" {
                    let at = node.children.iter().rposition(|x| x.tag == "FILE").map(|i| i + 1).unwrap_or(0);
                    node.children.insert(at, c.clone());
                } else if !node.children.contains(c) {
                    // Notes, sources, identifiants… du média fusionné : conservés.
                    node.insert_child(c.clone());
                }
            }
            let off = offset;
            self.rewrite_links(
                m,
                |l| {
                    if is_page_link(l) {
                        set_page(l, page_of_link(l) + off);
                    }
                    l.value = Some(ptr(&target));
                    vec![]
                },
                &[&target],
            );
            offset += pages;
            self.delete(m);
        }
        let node = self.get_mut(&target).unwrap();
        if let Some(base) = &common {
            for (k, f) in node.children.iter_mut().filter(|c| c.tag == "FILE").enumerate() {
                if k == 0 {
                    f.set_child_val("TITL", base);
                } else if f.child_val("TITL").is_some_and(|t| strip_page_suffix(t) == base) {
                    f.children.retain(|c| c.tag != "TITL");
                }
            }
        }
        node.touch();
        Ok(target)
    }

    /// Sépare une page d'un média multipage en un nouveau média. Les personnes
    /// situées sur cette page sont reliées au nouveau média ; les liens vers le
    /// document entier sont dupliqués. Renvoie l'identifiant du nouveau média.
    pub fn split_media_page(&mut self, media: &str, page: usize) -> Result<String, String> {
        let m = Media::new(self.media_node(media)?);
        let n = m.page_count();
        if n < 2 {
            return Err("ce média n'a qu'une page".into());
        }
        if page == 0 || page > n {
            return Err("page introuvable".into());
        }
        let base = base_title(m.node);
        let node = self.get_mut(media).unwrap();
        let i = node.children.iter().enumerate().filter(|(_, c)| c.tag == "FILE").nth(page - 1).unwrap().0;
        let mut file = node.children.remove(i);
        keep_title(node, base.as_deref());
        node.touch();
        // Titre de la page séparée : le sien s'il est propre à la page, sinon « titre (k/n) ».
        if let Some(b) = &base
            && file.child_val("TITL").is_none_or(|t| t.trim() == b)
        {
            file.set_child_val("TITL", &format!("{b} ({page}/{n})"));
        }
        let new = self.create(RecordKind::Media);
        let nn = self.get_mut(&new).unwrap();
        nn.children.retain(|c| c.tag != "FILE");
        nn.children.insert(0, file);
        let new_p = ptr(&new);
        self.rewrite_links(
            media,
            |l| {
                if !is_page_link(l) {
                    // Document entier : lié aussi à la page séparée.
                    let mut copy = l.clone();
                    copy.value = Some(new_p.clone());
                    return vec![copy];
                }
                let p = page_of_link(l);
                if p == page {
                    l.value = Some(new_p.clone());
                    set_page(l, 1);
                } else if p > page {
                    set_page(l, p - 1);
                }
                vec![]
            },
            &[&new],
        );
        Ok(new)
    }
}

/// Titre saisi du média (`TITL` de la première page), sans « (1/2) » final.
fn base_title(node: &Node) -> Option<String> {
    let t = node.child("FILE").and_then(|f| f.child_val("TITL")).or_else(|| node.child_val("TITL"))?;
    Some(strip_page_suffix(t).to_string()).filter(|t| !t.is_empty())
}

/// Après un retrait ou un déplacement, la première page porte le titre du média :
/// s'il était sur une page qui n'est plus la première, il y remonte.
fn keep_title(node: &mut Node, base: Option<&str>) {
    let Some(base) = base else { return };
    if node.child("FILE").is_none_or(|f| f.child_val("TITL").is_some()) {
        return;
    }
    for (k, f) in node.children.iter_mut().filter(|c| c.tag == "FILE").enumerate() {
        if k == 0 {
            f.set_child_val("TITL", base);
        } else if f.child_val("TITL").is_some_and(|t| t.trim() == base) {
            f.children.retain(|c| c.tag != "TITL");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn page_suffix() {
        assert_eq!(strip_page_suffix("Acte de mariage (1/2)"), "Acte de mariage");
        assert_eq!(strip_page_suffix("Recensement 1906 (3)"), "Recensement 1906");
        assert_eq!(strip_page_suffix("Michel (dit Mimi)"), "Michel (dit Mimi)");
        assert_eq!(strip_page_suffix("Plan"), "Plan");
    }
}
