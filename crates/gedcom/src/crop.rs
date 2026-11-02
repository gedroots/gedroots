//! Recadrage des médias (`CROP`) et identification de personnes sur les photos.
//!
//! En GEDCOM 7, un lien vers un média (`n OBJE @M1@`) peut porter une
//! sous-structure `CROP` qui délimite la partie de l'image concernée :
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
//! Les valeurs sont en pixels de l'image d'origine. `TOP` et `LEFT` valent 0
//! par défaut ; sans `HEIGHT`/`WIDTH`, le recadrage s'étend jusqu'au bord.
//!
//! Identifier une personne sur une photo de groupe revient donc simplement à
//! lier le média à l'individu avec un `CROP` autour de son visage : c'est la
//! représentation standard, lisible par les autres logiciels, et aucune
//! extension n'est nécessaire. Le portrait d'une personne utilise aussi ce
//! recadrage.

use crate::document::Gedcom;
use crate::multipage::{PAGE_TAG, PAGE_URI, page_of_link};
use crate::node::Node;

/// Zone d'une image, en pixels, telle qu'écrite dans `CROP`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Crop {
    pub top: u32,
    pub left: u32,
    pub height: Option<u32>,
    pub width: Option<u32>,
}

/// Rectangle résolu sur une image de taille connue : `(x, y, largeur, hauteur)`.
pub type Rect = (u32, u32, u32, u32);

impl Crop {
    /// Recadrage complet à partir d'un rectangle.
    pub fn from_rect((x, y, w, h): Rect) -> Self {
        Self { top: y, left: x, height: Some(h), width: Some(w) }
    }

    /// Lit le `CROP` d'un lien `OBJE` (le nœud du lien lui-même).
    /// Les valeurs illisibles sont ignorées, comme le prévoit la norme pour
    /// les valeurs absentes.
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

    /// Structure `CROP` à placer sous un lien `OBJE`.
    pub fn to_node(&self) -> Node {
        let mut n = Node::new("CROP");
        // On écrit toujours TOP et LEFT : plus lisible pour un humain, et
        // équivalent à leur absence quand ils valent 0.
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

    /// Rectangle effectif sur une image `img_w × img_h`, borné à l'image.
    /// `None` si la zone est vide (recadrage hors de l'image).
    pub fn rect(&self, img_w: u32, img_h: u32) -> Option<Rect> {
        let x = self.left.min(img_w);
        let y = self.top.min(img_h);
        let w = self.width.unwrap_or(u32::MAX).min(img_w - x);
        let h = self.height.unwrap_or(u32::MAX).min(img_h - y);
        (w > 0 && h > 0).then_some((x, y, w, h))
    }

    /// Le recadrage couvre-t-il toute l'image (donc sans effet) ?
    pub fn is_full(&self, img_w: u32, img_h: u32) -> bool {
        self.rect(img_w, img_h) == Some((0, 0, img_w, img_h))
    }

    /// Libellé court, ex. « 200 × 260 px à (340, 120) ».
    pub fn describe(&self) -> String {
        let dim = |v: Option<u32>| v.map(|v| v.to_string()).unwrap_or_else(|| "…".into());
        format!("{} × {} px à ({}, {})", dim(self.width), dim(self.height), self.left, self.top)
    }
}

/// Un lien vers un média depuis un enregistrement, avec son éventuel recadrage.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MediaLink {
    /// Enregistrement qui porte le lien (individu, famille, source…).
    pub owner: String,
    /// Balise de cet enregistrement (`INDI`, `FAM`, `SOUR`…).
    pub owner_tag: String,
    pub crop: Option<Crop>,
    /// Page du média visée par le lien (1 pour la première, voir [`crate::multipage`]).
    pub page: usize,
}

fn ptr(x: &str) -> String {
    format!("@{x}@")
}

impl Gedcom {
    /// Tous les liens directs (`1 OBJE @media@`) vers un média, dans l'ordre du fichier.
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

    /// Personnes identifiées sur un média : individus liés avec un recadrage.
    pub fn people_on(&self, media: &str) -> Vec<MediaLink> {
        self.media_links(media).into_iter().filter(|l| l.owner_tag == "INDI" && l.crop.is_some()).collect()
    }

    /// Recadrage du (premier) lien de `owner` vers `media`.
    pub fn link_crop(&self, owner: &str, media: &str) -> Option<Crop> {
        let p = ptr(media);
        let r = self.get(owner)?;
        r.children.iter().find(|c| c.tag == "OBJE" && c.val() == p).and_then(Crop::from_link)
    }

    /// Page visée par le (premier) lien de `owner` vers `media`.
    pub fn link_page(&self, owner: &str, media: &str) -> usize {
        let p = ptr(media);
        self.get(owner)
            .and_then(|r| r.children.iter().find(|c| c.tag == "OBJE" && c.val() == p))
            .map(page_of_link)
            .unwrap_or(1)
    }

    /// Définit (ou retire, avec `None`) le recadrage du lien de `owner` vers
    /// `media`. Le lien est créé s'il n'existe pas encore.
    pub fn set_link_crop(&mut self, owner: &str, media: &str, crop: Option<Crop>) -> Result<(), String> {
        self.set_link_crop_on(owner, media, 1, crop)
    }

    /// Comme [`Gedcom::set_link_crop`], pour la page `page` d'un média multipage.
    pub fn set_link_crop_on(
        &mut self,
        owner: &str,
        media: &str,
        page: usize,
        crop: Option<Crop>,
    ) -> Result<(), String> {
        if self.get(media).is_none_or(|m| m.tag != "OBJE") {
            return Err(rust_i18n::t!("error.media_not_found").into());
        }
        let page = page.max(1);
        let p = ptr(media);
        let r = self.get_mut(owner).ok_or_else(|| rust_i18n::t!("error.record_not_found"))?;
        let find = |r: &Node| {
            let link = |c: &Node| c.tag == "OBJE" && c.val() == p;
            r.children.iter().position(|c| link(c) && page_of_link(c) == page).or_else(|| {
                // Une zone sur une autre page : le lien vers le document entier la reçoit.
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
            // CROP précède TITL dans l'ordre de la norme.
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
            // Sans zone, le lien redevient un lien vers le document entier : pas de doublon.
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

    /// Identifie une personne sur une photo : lie le média à l'individu avec ce recadrage.
    pub fn tag_person(&mut self, indi: &str, media: &str, crop: Crop) -> Result<(), String> {
        if self.get(indi).is_none_or(|n| n.tag != "INDI") {
            return Err(rust_i18n::t!("error.person_not_found").into());
        }
        self.set_link_crop(indi, media, Some(crop))
    }

    /// Identifie une personne sur la page `page` d'un média.
    pub fn tag_person_on(&mut self, indi: &str, media: &str, page: usize, crop: Crop) -> Result<(), String> {
        if self.get(indi).is_none_or(|n| n.tag != "INDI") {
            return Err(rust_i18n::t!("error.person_not_found").into());
        }
        self.set_link_crop_on(indi, media, page, Some(crop))
    }

    /// Retire l'identification d'une personne sur une page (les liens vers les autres pages restent).
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

    /// Retire l'identification d'une personne sur une photo (supprime le lien).
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
