//! Media cropping (`CROP`) and identifying people in photos.
//!
//! In GEDCOM 7, a link to a media (`n OBJE @M1@`) can carry a `CROP` substructure that delimits the
//! relevant part of the image:
//!
//! ```text
//! 1 OBJE @M1@
//! 2 CROP
//! 3 TOP 120
//! 3 LEFT 340
//! 3 HEIGHT 260
//! 3 WIDTH 200
//! ```
//!
//! The values are in pixels of the original image. `TOP` and `LEFT` default to 0; without
//! `HEIGHT`/`WIDTH`, the crop extends to the edge.
//!
//! Identifying a person in a group photo is therefore simply linking the media to the individual
//! with a `CROP` around their face: it is the standard representation, readable by other software,
//! and no extension is needed. A person's portrait uses this crop too.

use crate::document::Gedcom;
use crate::multipage::{PAGE_TAG, PAGE_URI, page_of_link};
use crate::node::Node;

/// Area of an image, in pixels, as written in `CROP`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Crop {
    pub top: u32,
    pub left: u32,
    pub height: Option<u32>,
    pub width: Option<u32>,
}

/// Rectangle resolved on an image of known size: `(x, y, width, height)`.
pub type Rect = (u32, u32, u32, u32);

impl Crop {
    /// Full crop from a rectangle.
    pub fn from_rect((x, y, w, h): Rect) -> Self {
        Self { top: y, left: x, height: Some(h), width: Some(w) }
    }

    /// Reads the `CROP` of an `OBJE` link (the link node itself). Unreadable values are ignored, as
    /// the standard prescribes for missing values.
    pub fn from_link(link: &Node) -> Option<Self> {
        let c = link.child("CROP")?;
        let num = |t: &str| c.child_val(t).and_then(|v| v.trim().parse::<u32>().ok());
        Some(Self {
            top: num("TOP").unwrap_or(0),
            left: num("LEFT").unwrap_or(0),
            height: num("HEIGHT"),
            width: num("WIDTH"),
        })
    }

    /// `CROP` structure to put under an `OBJE` link.
    pub fn to_node(&self) -> Node {
        let mut n = Node::new("CROP");
        // TOP and LEFT are always written: easier for a human to read, and equivalent to leaving
        // them out when they are 0.
        n.children.push(Node::with_value("TOP", self.top.to_string()));
        n.children.push(Node::with_value("LEFT", self.left.to_string()));
        if let Some(h) = self.height {
            n.children.push(Node::with_value("HEIGHT", h.to_string()));
        }
        if let Some(w) = self.width {
            n.children.push(Node::with_value("WIDTH", w.to_string()));
        }
        n
    }

    /// Effective rectangle on an `img_w × img_h` image, clamped to the image. `None` if the area is
    /// empty (crop outside the image).
    pub fn rect(&self, img_w: u32, img_h: u32) -> Option<Rect> {
        let x = self.left.min(img_w);
        let y = self.top.min(img_h);
        let w = self.width.unwrap_or(u32::MAX).min(img_w - x);
        let h = self.height.unwrap_or(u32::MAX).min(img_h - y);
        (w > 0 && h > 0).then_some((x, y, w, h))
    }

    /// Does the crop cover the whole image (and so have no effect)?
    pub fn is_full(&self, img_w: u32, img_h: u32) -> bool {
        self.rect(img_w, img_h) == Some((0, 0, img_w, img_h))
    }

    /// Short label, e.g. “200 × 260 px at (340, 120)”.
    pub fn describe(&self) -> String {
        let dim = |v: Option<u32>| v.map(|v| v.to_string()).unwrap_or_else(|| "…".into());
        t!("model.crop", w = dim(self.width), h = dim(self.height), x = self.left, y = self.top).into_owned()
    }
}

/// A link to a media from a record, with its crop if any.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MediaLink {
    /// Record holding the link (individual, family, source…).
    pub owner: String,
    /// Balise de cet enregistrement (`INDI`, `FAM`, `SOUR`…).
    pub owner_tag: String,
    pub crop: Option<Crop>,
    /// Page of the media the link points to (1 for the first one, see [`crate::multipage`]).
    pub page: usize,
}

fn ptr(x: &str) -> String {
    format!("@{x}@")
}

impl Gedcom {
    /// Every direct link (`1 OBJE @media@`) to a media, in file order.
    pub fn media_links(&self, media: &str) -> Vec<MediaLink> {
        let p = ptr(media);
        let mut out = Vec::new();
        for r in &self.records {
            let Some(owner) = r.xref.as_deref() else { continue };
            for l in r.children.iter().filter(|c| c.tag == "OBJE" && c.val() == p) {
                out.push(MediaLink {
                    owner: owner.to_string(),
                    owner_tag: r.tag.clone(),
                    crop: Crop::from_link(l),
                    page: page_of_link(l),
                });
            }
        }
        out
    }

    /// People identified in a media: individuals linked with a crop.
    pub fn people_on(&self, media: &str) -> Vec<MediaLink> {
        self.media_links(media).into_iter().filter(|l| l.owner_tag == "INDI" && l.crop.is_some()).collect()
    }

    /// Crop of the (first) link from `owner` to `media`.
    pub fn link_crop(&self, owner: &str, media: &str) -> Option<Crop> {
        let p = ptr(media);
        let r = self.get(owner)?;
        r.children.iter().find(|c| c.tag == "OBJE" && c.val() == p).and_then(Crop::from_link)
    }

    /// Page pointed to by the (first) link from `owner` to `media`.
    pub fn link_page(&self, owner: &str, media: &str) -> usize {
        let p = ptr(media);
        self.get(owner)
            .and_then(|r| r.children.iter().find(|c| c.tag == "OBJE" && c.val() == p))
            .map(page_of_link)
            .unwrap_or(1)
    }

    /// Sets (or removes, with `None`) the crop of the link from `owner` to `media`. The link is
    /// created if it does not exist yet.
    pub fn set_link_crop(&mut self, owner: &str, media: &str, crop: Option<Crop>) -> Result<(), String> {
        self.set_link_crop_on(owner, media, 1, crop)
    }

    /// Like [`Gedcom::set_link_crop`], for page `page` of a multi-page media.
    pub fn set_link_crop_on(
        &mut self,
        owner: &str,
        media: &str,
        page: usize,
        crop: Option<Crop>,
    ) -> Result<(), String> {
        if self.get(media).is_none_or(|m| m.tag != "OBJE") {
            return Err(t!("error.media_not_found").into());
        }
        let page = page.max(1);
        let p = ptr(media);
        let r = self.get_mut(owner).ok_or_else(|| t!("error.record_not_found"))?;
        let find = |r: &Node| {
            let link = |c: &Node| c.tag == "OBJE" && c.val() == p;
            r.children.iter().position(|c| link(c) && page_of_link(c) == page).or_else(|| {
                // An area on another page: the link to the whole document gets it.
                crop.and(r.children.iter().position(|c| link(c) && c.children.is_empty()))
            })
        };
        let i = match find(r) {
            Some(i) => i,
            None if crop.is_none() => return Ok(()),
            None => {
                r.insert_child(Node::with_value("OBJE", p.clone()));
                r.children.iter().rposition(|c| c.tag == "OBJE" && c.val() == p && c.children.is_empty()).unwrap()
            }
        };
        let link = &mut r.children[i];
        let before = link.clone();
        link.children.retain(|c| c.tag != "CROP" && c.tag != PAGE_TAG);
        if let Some(c) = crop {
            // CROP comes before TITL in the standard's order.
            link.children.insert(0, c.to_node());
            if page > 1 {
                let at = link
                    .children
                    .iter()
                    .position(|c| c.tag != "CROP" && c.tag != "TITL")
                    .unwrap_or(link.children.len());
                link.children.insert(at, Node::with_value(PAGE_TAG, page.to_string()));
            }
        }
        let changed = *link != before;
        if changed {
            // Without an area, the link becomes a link to the whole document again: no duplicate.
            if crop.is_none() {
                let dup = r
                    .children
                    .iter()
                    .enumerate()
                    .any(|(k, c)| k != i && c.tag == "OBJE" && c.val() == p && c.children.is_empty());
                if dup && r.children[i].children.is_empty() {
                    r.children.remove(i);
                }
            }
            r.touch();
            if crop.is_some() && page > 1 {
                self.declare_extension(PAGE_TAG, PAGE_URI);
            }
        }
        Ok(())
    }

    /// Identifies a person in a photo: links the media to the individual with this crop.
    pub fn tag_person(&mut self, indi: &str, media: &str, crop: Crop) -> Result<(), String> {
        if self.get(indi).is_none_or(|n| n.tag != "INDI") {
            return Err(t!("error.person_not_found").into());
        }
        self.set_link_crop(indi, media, Some(crop))
    }

    /// Identifies a person on page `page` of a media.
    pub fn tag_person_on(&mut self, indi: &str, media: &str, page: usize, crop: Crop) -> Result<(), String> {
        if self.get(indi).is_none_or(|n| n.tag != "INDI") {
            return Err(t!("error.person_not_found").into());
        }
        self.set_link_crop_on(indi, media, page, Some(crop))
    }

    /// Removes the identification of a person on a page (the links to the other pages stay).
    pub fn untag_person_on(&mut self, indi: &str, media: &str, page: usize) {
        let p = ptr(media);
        if let Some(r) = self.get_mut(indi) {
            let before = r.children.len();
            r.children.retain(|c| !(c.tag == "OBJE" && c.val() == p && page_of_link(c) == page));
            if r.children.len() != before {
                r.touch();
            }
        }
    }

    /// Removes the identification of a person in a photo (deletes the link).
    pub fn untag_person(&mut self, indi: &str, media: &str) {
        let p = ptr(media);
        if let Some(r) = self.get_mut(indi) {
            let before = r.children.len();
            r.children.retain(|c| !(c.tag == "OBJE" && c.val() == p));
            if r.children.len() != before {
                r.touch();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rect_defaults_and_clamping() {
        let c = Crop { top: 10, left: 20, height: None, width: None };
        assert_eq!(c.rect(100, 50), Some((20, 10, 80, 40)));
        let c = Crop { top: 40, left: 90, height: Some(30), width: Some(30) };
        assert_eq!(c.rect(100, 50), Some((90, 40, 10, 10)));
        assert_eq!(Crop { top: 60, ..Default::default() }.rect(100, 50), None);
        assert!(Crop::default().is_full(100, 50));
    }

    #[test]
    fn node_roundtrip() {
        let c = Crop { top: 1, left: 2, height: Some(3), width: None };
        let link = Node::with_value("OBJE", "@M1@").push(c.to_node());
        assert_eq!(Crop::from_link(&link), Some(c));
    }
}
