//! Multi-page media (record on two sheets, register…).
//!
//! In GEDCOM 7, an `OBJE` record can hold several `FILE`s, each with its `FORM` and `TITL`, in page
//! order:
//!
//! ```text
//! 0 @M1@ OBJE
//! 1 FILE Records/record_1.jpg
//! 2 FORM image/jpeg
//! 2 TITL Marriage record
//! 1 FILE Records/record_2.jpg
//! 2 FORM image/jpeg
//! ```
//!
//! Individuals and sources then point to a single media. The standard applies a link's `CROP` to
//! the first file; to place a person on another page, GEDRoots adds the `_GR_FILE` extension to the
//! link (page number, from 1, see `docs/extensions.md`):
//!
//! ```text
//! 1 OBJE @M1@
//! 2 CROP
//! 3 TOP 120
//! 3 LEFT 340
//! 2 _GR_FILE 2
//! ```
//!
//! A link without `CROP` or `_GR_FILE` is about the whole document.
//!
//! Merging several media into one, or splitting a page into a new media, rewrites every pointer of
//! the file (in citations too).

use crate::document::{Gedcom, RecordKind};
use crate::model::Media;
use crate::node::Node;
use crate::relations::mime_for;

/// Tag of the page a `OBJE` link points to.
pub const PAGE_TAG: &str = "_GR_FILE";
pub const PAGE_URI: &str = extension_uri!("_gr_file");

/// A file (a page) of a media.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MediaFile<'a> {
    pub path: &'a str,
    pub form: Option<&'a str>,
    pub title: Option<&'a str>,
}

impl<'a> Media<'a> {
    /// The files of the media, in page order.
    pub fn files(&self) -> Vec<MediaFile<'a>> {
        self.node
            .children_with("FILE")
            .map(|f| MediaFile { path: f.val(), form: f.child_val("FORM"), title: f.child_val("TITL") })
            .collect()
    }

    pub fn page_count(&self) -> usize {
        self.node.children_with("FILE").count()
    }

    /// File of page `page` (from 1).
    pub fn page_file(&self, page: usize) -> Option<&'a str> {
        self.node.children_with("FILE").nth(page.checked_sub(1)?).map(|f| f.val())
    }
}

/// Page an `OBJE` link points to (1 without `_GR_FILE`).
pub fn page_of_link(link: &Node) -> usize {
    link.child_val(PAGE_TAG).and_then(|v| v.trim().parse::<usize>().ok()).filter(|p| *p >= 1).unwrap_or(1)
}

/// Does the link point to a specific page (cropped area or `_GR_FILE`) rather than the whole
/// document?
fn is_page_link(link: &Node) -> bool {
    link.child("CROP").is_some() || link.child(PAGE_TAG).is_some()
}

/// Writes the page of a link: nothing for the first one (standard reading of the `CROP`).
fn set_page(link: &mut Node, page: usize) {
    link.children.retain(|c| c.tag != PAGE_TAG);
    if page > 1 {
        // After CROP and TITL, the standard's order for the link's substructures.
        let at = link.children.iter().position(|c| c.tag != "CROP" && c.tag != "TITL").unwrap_or(link.children.len());
        link.children.insert(at, Node::with_value(PAGE_TAG, page.to_string()));
    }
}

/// Removes the final “(1/2)”, “(2)”… of a page title.
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

/// Applies `f` to every `OBJE` link to `p` in the node and its descendants. `f` returns links to
/// insert right after (copies to another media).
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

/// Removes duplicate `OBJE` links (same target, same substructures) under the same parent.
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
        self.get(media).filter(|n| n.tag == "OBJE").ok_or_else(|| t!("error.media_not_found").into_owned())
    }

    /// Media that can have pages: not a web link (`FILE https://…`).
    fn paged_media(&self, media: &str) -> Result<&Node, String> {
        let n = self.media_node(media)?;
        if Media::new(n).is_web() {
            return Err(t!("error.web_no_pages").into());
        }
        Ok(n)
    }

    /// Rewrites the links to `media` in the whole file; touches the modified records.
    pub(crate) fn rewrite_links(&mut self, media: &str, mut f: impl FnMut(&mut Node) -> Vec<Node>, dedupe: &[&str]) {
        let p = ptr(media);
        let mut uses_pages = false;
        let mut wrap = |l: &mut Node| {
            let extra = f(l);
            uses_pages |= l.child(PAGE_TAG).is_some() || extra.iter().any(|e| e.child(PAGE_TAG).is_some());
            extra
        };
        let linked = |r: &Node| r.children.iter().any(|c| c.any(&|n| n.tag == "OBJE" && n.val() == p));
        for r in self.records_where_mut(linked) {
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

    /// Adds pages (files) at the end of a media. Files already in this media are ignored. Returns
    /// the number of pages added.
    pub fn add_media_pages(&mut self, media: &str, files: &[String]) -> Result<usize, String> {
        self.paged_media(media)?;
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

    /// Removes a page from a media (the file stays on the disk). The people placed on this page
    /// stay linked to the media, without an area.
    pub fn remove_media_page(&mut self, media: &str, page: usize) -> Result<(), String> {
        let n = Media::new(self.media_node(media)?).page_count();
        if n < 2 {
            return Err(t!("error.media_one_page").into());
        }
        if page == 0 || page > n {
            return Err(t!("error.page_not_found").into());
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

    /// Moves a page (`from` → `to`, from 1).
    pub fn move_media_page(&mut self, media: &str, from: usize, to: usize) -> Result<(), String> {
        let n = Media::new(self.media_node(media)?).page_count();
        if from == 0 || to == 0 || from > n || to > n {
            return Err(t!("error.page_not_found").into());
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
        // New position of each old page.
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

    /// Merges media into a single multi-page media, in the given order: the first one receives the
    /// pages of the next ones, which are deleted. Every link to the merged media then points to the
    /// first one (people placed on a page keep their area, with the page number).
    pub fn merge_media(&mut self, media: &[String]) -> Result<String, String> {
        let mut list: Vec<&str> = Vec::new();
        for m in media {
            self.paged_media(m)?;
            if !list.contains(&m.as_str()) {
                list.push(m);
            }
        }
        if list.len() < 2 {
            return Err(t!("error.merge_two").into());
        }
        let target = list[0].to_string();
        // Common title: “Marriage record (1/2)” and “(2/2)” → “Marriage record”.
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
                    // Notes, sources, identifiers… of the merged media: kept.
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

    /// Splits a page of a multi-page media into a new media. The people placed on this page are
    /// linked to the new media; links to the whole document are duplicated. Returns the identifier
    /// of the new media.
    pub fn split_media_page(&mut self, media: &str, page: usize) -> Result<String, String> {
        let m = Media::new(self.media_node(media)?);
        let n = m.page_count();
        if n < 2 {
            return Err(t!("error.single_page").into());
        }
        if page == 0 || page > n {
            return Err(t!("error.page_not_found").into());
        }
        let base = base_title(m.node);
        let node = self.get_mut(media).unwrap();
        let i = node.children.iter().enumerate().filter(|(_, c)| c.tag == "FILE").nth(page - 1).unwrap().0;
        let mut file = node.children.remove(i);
        keep_title(node, base.as_deref());
        node.touch();
        // Title of the split page: its own if it is specific to the page, otherwise “title (k/n)”.
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
                    // Whole document: linked to the split page too.
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

/// Typed title of the media (`TITL` of the first page), without the final “(1/2)”.
fn base_title(node: &Node) -> Option<String> {
    let t = node.child("FILE").and_then(|f| f.child_val("TITL")).or_else(|| node.child_val("TITL"))?;
    Some(strip_page_suffix(t).to_string()).filter(|t| !t.is_empty())
}

/// After a removal or a move, the first page carries the media's title: if it was on a page that is
/// no longer the first, it moves up.
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
