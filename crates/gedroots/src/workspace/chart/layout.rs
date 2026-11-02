//! Disposition de l'arbre libre, en colonnes comme l'ancienne vue
//! d'ascendance : ascendants à droite de la personne centrale, descendants à
//! gauche, conjoints sous chaque personne, frères et sœurs au-dessus.
//!
//! Les coordonnées sont en pixels : la case de la personne centrale a son
//! coin haut gauche en (0, 0) ; la génération `g` des ascendants est dans la
//! colonne `g`, celle des descendants dans la colonne `-g`. Seules les
//! générations demandées (`up`, `down`) sont parcourues : les suivantes sont
//! chargées quand on s'en approche.

use gedcom::{Gedcom, Relation};
use std::collections::HashSet;

pub const CARD_W: f32 = 184.;
pub const CARD_H: f32 = 48.;
/// Écart vertical entre deux cases d'une même colonne.
pub const GAP: f32 = 10.;
/// Largeur d'une génération (case + liaisons).
pub const COL: f32 = CARD_W + 56.;
/// Générations au-dessus de la personne centrale où un parent inconnu
/// laisse un emplacement « + Père » / « + Mère ».
const SLOT_DEPTH: u32 = 3;

/// Ce qu'affiche une case.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Slot {
    /// Une personne. `side` : 1 pour la personne centrale, 2 côté paternel,
    /// 3 côté maternel, 0 hors ascendance (descendants, conjoints, fratrie).
    Person {
        xref: String,
        side: u8,
    },
    /// Parent inconnu : un clic le crée.
    AddParent {
        child: String,
        rel: Relation,
    },
    AddChild(String),
    AddSibling(String),
}

#[derive(Debug, Clone)]
pub struct Card {
    pub slot: Slot,
    /// Coin haut gauche.
    pub x: f32,
    pub y: f32,
}

impl Card {
    pub fn xref(&self) -> Option<&str> {
        match &self.slot {
            Slot::Person { xref, .. } => Some(xref),
            _ => None,
        }
    }

    pub fn center(&self) -> (f32, f32) {
        (self.x + CARD_W / 2., self.y + CARD_H / 2.)
    }
}

/// Extrémité d'une liaison. Les colonnes ne bougent pas quand une génération
/// s'ajoute, seules les ordonnées : l'abscisse est fixe et l'ordonnée suit une
/// case, pour que les liaisons accompagnent les cases quand elles glissent.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct End {
    pub x: f32,
    /// Indice de la case dans [`Layout::cards`].
    pub card: usize,
    /// Écart vertical depuis le haut de cette case.
    pub dy: f32,
}

#[derive(Debug, Default)]
pub struct Layout {
    pub cards: Vec<Card>,
    /// Segments de liaison, d'une extrémité à l'autre.
    pub lines: Vec<[End; 2]>,
    /// Des ascendants existent au-delà de la dernière génération chargée.
    pub more_up: bool,
    /// Des descendants existent au-delà de la dernière génération chargée.
    pub more_down: bool,
    /// Rectangle englobant (x0, y0, x1, y1).
    pub bounds: [f32; 4],
}

impl Layout {
    /// Première case de la personne (la personne centrale avant tout).
    pub fn find(&self, xref: &str) -> Option<&Card> {
        self.cards.iter().find(|c| c.xref() == Some(xref))
    }

    /// Segments de liaison (x0, y0, x1, y1), les cases étant aux ordonnées `ys`
    /// (une par case, dans l'ordre de [`Layout::cards`]).
    pub fn segments<'a>(&'a self, ys: &'a [f32]) -> impl Iterator<Item = [f32; 4]> + 'a {
        self.lines.iter().map(move |[a, b]| [a.x, ys[a.card] + a.dy, b.x, ys[b.card] + b.dy])
    }

    /// Case de personne la plus proche du point donné.
    pub fn nearest(&self, x: f32, y: f32) -> Option<&Card> {
        self.cards.iter().filter(|c| c.xref().is_some()).min_by(|a, b| {
            let d = |c: &Card| {
                let (cx, cy) = c.center();
                (cx - x).powi(2) + (cy - y).powi(2)
            };
            d(a).total_cmp(&d(b))
        })
    }
}

/// Nœud de la disposition « en arbre bien rangé » : une largeur et des enfants.
struct TNode {
    w: f32,
    kids: Vec<usize>,
}

/// Disposition compacte (à la Reingold-Tilford) : chaque sous-arbre est
/// rapproché de son voisin autant que le permettent leurs contours, et chaque
/// nœud est centré au-dessus de ses enfants. Renvoie le centre de chaque nœud,
/// la racine étant en 0.
fn tidy(nodes: &[TNode], root: usize) -> Vec<f32> {
    fn shape(nodes: &[TNode], n: usize, rel: &mut [f32]) -> Vec<(f32, f32)> {
        let half = nodes[n].w / 2.;
        let mut acc: Vec<(f32, f32)> = Vec::new();
        let mut offs = Vec::with_capacity(nodes[n].kids.len());
        for &k in &nodes[n].kids {
            let c = shape(nodes, k, rel);
            let off = if offs.is_empty() {
                0.
            } else {
                acc.iter().zip(&c).map(|(a, b)| a.1 - b.0 + GAP).fold(f32::MIN, f32::max)
            };
            for (d, (l, r)) in c.iter().enumerate() {
                let (l, r) = (l + off, r + off);
                match acc.get_mut(d) {
                    Some(a) => *a = (a.0.min(l), a.1.max(r)),
                    None => acc.push((l, r)),
                }
            }
            offs.push(off);
        }
        let mut out = vec![(-half, half)];
        if let (Some(first), Some(last)) = (offs.first(), offs.last()) {
            let mid = (first + last) / 2.;
            for (&k, o) in nodes[n].kids.iter().zip(&offs) {
                rel[k] = o - mid;
            }
            out.extend(acc.iter().map(|(l, r)| (l - mid, r - mid)));
        }
        out
    }
    let mut rel = vec![0.; nodes.len()];
    shape(nodes, root, &mut rel);
    // Positions absolues, des parents vers les enfants (les enfants sont
    // toujours créés après leur parent).
    let mut abs = vec![0.; nodes.len()];
    let mut stack = vec![root];
    while let Some(n) = stack.pop() {
        for &k in &nodes[n].kids {
            abs[k] = abs[n] + rel[k];
            stack.push(k);
        }
    }
    abs
}

// ----------------------------------------------------------------------
// Ascendants
// ----------------------------------------------------------------------

struct Anc {
    slot: Slot,
    depth: u32,
    kids: Vec<usize>,
}

struct AncBuilder<'a> {
    g: &'a Gedcom,
    up: u32,
    nodes: Vec<Anc>,
    seen: HashSet<String>,
    more: bool,
}

impl AncBuilder<'_> {
    fn person(&mut self, xref: &str, depth: u32, side: u8) -> usize {
        let id = self.nodes.len();
        self.nodes.push(Anc { slot: Slot::Person { xref: xref.to_string(), side }, depth, kids: vec![] });
        // Implexe : un ancêtre déjà affiché n'est pas développé une seconde fois.
        if !self.seen.insert(xref.to_string()) {
            return id;
        }
        let (f, m) = self.g.parents(xref);
        if depth >= self.up {
            self.more |= f.is_some() || m.is_some();
            return id;
        }
        let mut kids = Vec::new();
        for (p, rel, s) in [(f, Relation::Father, 2), (m, Relation::Mother, 3)] {
            let side = if depth == 0 { s } else { side };
            match p {
                Some(p) => kids.push(self.person(p.xref(), depth + 1, side)),
                None if depth < SLOT_DEPTH => {
                    kids.push(self.nodes.len());
                    self.nodes.push(Anc {
                        slot: Slot::AddParent { child: xref.to_string(), rel },
                        depth: depth + 1,
                        kids: vec![],
                    });
                }
                None => {}
            }
        }
        self.nodes[id].kids = kids;
        id
    }
}

// ----------------------------------------------------------------------
// Descendants
// ----------------------------------------------------------------------

struct Desc {
    slot: Slot,
    depth: u32,
    /// Conjoints connus, dans l'ordre des unions (cases à droite de la personne).
    spouses: Vec<String>,
    /// Enfants de chaque union, avec l'indice du conjoint (s'il est connu).
    unions: Vec<(Option<usize>, Vec<usize>)>,
    /// Emplacement « + Enfant » éventuel.
    slot_kid: Option<usize>,
}

impl Desc {
    /// Hauteur du bloc : la personne, puis ses conjoints en dessous.
    fn height(&self) -> f32 {
        let n = self.spouses.len() as f32;
        CARD_H * (1. + n) + GAP * n
    }

    fn kids(&self) -> Vec<usize> {
        self.unions.iter().flat_map(|(_, k)| k.iter().copied()).chain(self.slot_kid).collect()
    }
}

struct DescBuilder<'a> {
    g: &'a Gedcom,
    down: u32,
    nodes: Vec<Desc>,
    seen: HashSet<String>,
    more: bool,
}

impl DescBuilder<'_> {
    fn person(&mut self, xref: &str, depth: u32) -> usize {
        let id = self.nodes.len();
        self.nodes.push(Desc {
            slot: Slot::Person { xref: xref.to_string(), side: 0 },
            depth,
            spouses: vec![],
            unions: vec![],
            slot_kid: None,
        });
        let Some(i) = self.g.individual(xref) else { return id };
        if !self.seen.insert(xref.to_string()) {
            return id;
        }
        let mut spouses = Vec::new();
        let mut unions = Vec::new();
        for f in i.fams().into_iter().filter_map(|f| self.g.family(f)) {
            let spouse = [f.husband(), f.wife()]
                .into_iter()
                .flatten()
                .find(|x| *x != xref && self.g.individual(x).is_some())
                .map(str::to_string);
            let children: Vec<String> =
                f.children().into_iter().filter(|c| self.g.individual(c).is_some()).map(str::to_string).collect();
            if spouse.is_none() && children.is_empty() {
                continue;
            }
            let sp = spouse.map(|s| {
                spouses.push(s);
                spouses.len() - 1
            });
            let kids = if depth < self.down {
                children.iter().map(|c| self.person(c, depth + 1)).collect()
            } else {
                self.more |= !children.is_empty();
                vec![]
            };
            unions.push((sp, kids));
        }
        let node = &mut self.nodes[id];
        node.spouses = spouses;
        node.unions = unions;
        id
    }
}

// ----------------------------------------------------------------------
// Assemblage
// ----------------------------------------------------------------------

/// Extrémité en `(x, y)`, rattachée à la case `card`.
fn end(cards: &[Card], card: usize, x: f32, y: f32) -> End {
    End { x, card, dy: y - cards[card].y }
}

/// Barre verticale couvrant toutes les ordonnées données, chacune rattachée à sa case.
fn bar(cards: &[Card], x: f32, ys: impl IntoIterator<Item = (usize, f32)>) -> [End; 2] {
    let mut ys = ys.into_iter();
    let first = ys.next().expect("au moins une extrémité");
    let (t, b) = ys.fold((first, first), |(t, b), p| (if p.1 < t.1 { p } else { t }, if p.1 > b.1 { p } else { b }));
    [end(cards, t.0, x, t.1), end(cards, b.0, x, b.1)]
}

/// Arbre autour de `anchor` : `up` générations d'ascendants, `down` de descendants.
pub fn build(g: &Gedcom, anchor: &str, up: u32, down: u32) -> Layout {
    let mut out = Layout::default();
    if g.individual(anchor).is_none() {
        return out;
    }
    let mid_gap = (COL - CARD_W) / 2.;
    let step = CARD_H + GAP;

    // Ascendants, vers la droite : la racine est la personne centrale.
    let mut ab = AncBuilder { g, up, nodes: vec![], seen: HashSet::new(), more: false };
    ab.person(anchor, 0, 1);
    let tn: Vec<TNode> = ab.nodes.iter().map(|n| TNode { w: CARD_H, kids: n.kids.clone() }).collect();
    let ay = tidy(&tn, 0);
    // Les cases d'abord (le nœud `i` est la case `i`), puis leurs liaisons.
    for (n, a) in ab.nodes.iter().zip(&ay) {
        out.cards.push(Card { slot: n.slot.clone(), x: n.depth as f32 * COL, y: *a });
    }
    for (i, n) in ab.nodes.iter().enumerate() {
        if !n.kids.is_empty() {
            // De la case vers la droite, puis une barre jusqu'en face des parents.
            let x = out.cards[i].x;
            let cy = |k: usize| out.cards[k].y + CARD_H / 2.;
            let bx = x + CARD_W + mid_gap;
            out.lines.push([end(&out.cards, i, x + CARD_W, cy(i)), end(&out.cards, i, bx, cy(i))]);
            let b = bar(&out.cards, bx, std::iter::once(i).chain(n.kids.iter().copied()).map(|k| (k, cy(k))));
            out.lines.push(b);
            for &k in &n.kids {
                out.lines.push([end(&out.cards, k, bx, cy(k)), end(&out.cards, k, x + COL, cy(k))]);
            }
        }
    }
    out.more_up = ab.more;

    // Descendants, vers la gauche : la racine est la personne centrale et ses conjoints.
    let mut db = DescBuilder { g, down, nodes: vec![], seen: HashSet::new(), more: false };
    db.person(anchor, 0);
    if down > 0 {
        let id = db.nodes.len();
        db.nodes.push(Desc {
            slot: Slot::AddChild(anchor.to_string()),
            depth: 1,
            spouses: vec![],
            unions: vec![],
            slot_kid: None,
        });
        db.nodes[0].slot_kid = Some(id);
    }
    let tn: Vec<TNode> = db.nodes.iter().map(|n| TNode { w: n.height(), kids: n.kids() }).collect();
    let dy = tidy(&tn, 0);
    // La case de la personne centrale (en haut de son bloc) doit être en y = 0.
    let shift = db.nodes[0].height() / 2.;
    let top = |n: usize| dy[n] + shift - db.nodes[n].height() / 2.;
    // Les cases d'abord : `card[ix]` est la case du nœud `ix` (la personne
    // centrale est la case 0, déjà posée), ses conjoints suivent.
    let mut card = vec![0; db.nodes.len()];
    for (ix, n) in db.nodes.iter().enumerate() {
        let x = -(n.depth as f32) * COL;
        let y = top(ix);
        if ix != 0 {
            card[ix] = out.cards.len();
            out.cards.push(Card { slot: n.slot.clone(), x, y });
        }
        for (j, s) in n.spouses.iter().enumerate() {
            out.cards.push(Card { slot: Slot::Person { xref: s.clone(), side: 0 }, x, y: y + (j as f32 + 1.) * step });
        }
    }
    for (ix, n) in db.nodes.iter().enumerate() {
        let (x, y, me) = (-(n.depth as f32) * COL, top(ix), card[ix]);
        if !n.spouses.is_empty() {
            // Trait d'union, visible entre les cases.
            let ux = x + CARD_W / 2.;
            let last = y + n.spouses.len() as f32 * step;
            out.lines.push([end(&out.cards, me, ux, y + CARD_H), end(&out.cards, me, ux, last)]);
        }
        let bx = x - mid_gap;
        // Les enfants d'une union partent d'entre la personne et le conjoint.
        let from = |sp: Option<usize>| match sp {
            Some(j) => y + (j as f32 + 1.) * step - GAP / 2.,
            None => y + CARD_H / 2.,
        };
        let groups = n
            .unions
            .iter()
            .map(|(sp, kids)| (from(*sp), kids.clone()))
            .chain(n.slot_kid.map(|k| (y + CARD_H / 2., vec![k])));
        for (fy, kids) in groups.filter(|(_, k)| !k.is_empty()) {
            out.lines.push([end(&out.cards, me, x, fy), end(&out.cards, me, bx, fy)]);
            let kcs: Vec<(usize, f32)> = kids.iter().map(|k| (card[*k], top(*k) + CARD_H / 2.)).collect();
            out.lines.push(bar(&out.cards, bx, std::iter::once((me, fy)).chain(kcs.iter().copied())));
            for (k, c) in kcs {
                out.lines.push([end(&out.cards, k, bx, c), end(&out.cards, k, x - COL + CARD_W, c)]);
            }
        }
    }
    out.more_down = db.more;

    // Frères et sœurs, au-dessus de la personne centrale, reliés aux mêmes parents.
    let siblings: Vec<String> = g
        .individual(anchor)
        .and_then(|i| i.famc().first().and_then(|f| g.family(f)))
        .map(|f| {
            f.children().into_iter().filter(|c| *c != anchor && g.individual(c).is_some()).map(str::to_string).collect()
        })
        .unwrap_or_default();
    let n = siblings.len() as f32;
    let bx = CARD_W + mid_gap;
    let first = out.cards.len();
    for (j, s) in siblings.iter().enumerate() {
        let y = -(n - j as f32) * step;
        let c = out.cards.len();
        out.cards.push(Card { slot: Slot::Person { xref: s.clone(), side: 0 }, x: 0., y });
        let cy = y + CARD_H / 2.;
        out.lines.push([end(&out.cards, c, CARD_W, cy), end(&out.cards, c, bx, cy)]);
    }
    if !siblings.is_empty() {
        out.lines.push([end(&out.cards, first, bx, -n * step + CARD_H / 2.), end(&out.cards, 0, bx, CARD_H / 2.)]);
        // Sans parents connus, la barre de la fratrie part quand même de la personne centrale.
        out.lines.push([end(&out.cards, 0, CARD_W, CARD_H / 2.), end(&out.cards, 0, bx, CARD_H / 2.)]);
    }
    out.cards.push(Card { slot: Slot::AddSibling(anchor.to_string()), x: 0., y: -(n + 1.) * step });

    let mut b = [f32::MAX, f32::MAX, f32::MIN, f32::MIN];
    for c in &out.cards {
        b = [b[0].min(c.x), b[1].min(c.y), b[2].max(c.x + CARD_W), b[3].max(c.y + CARD_H)];
    }
    out.bounds = b;
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ged() -> Gedcom {
        // Grands-parents paternels, parents, deux enfants (dont @I1@), un petit-enfant.
        let text = "0 HEAD\n1 GEDC\n2 VERS 7.0\n\
            0 @GF@ INDI\n1 NAME Jean /A/\n1 SEX M\n1 FAMS @F0@\n\
            0 @GM@ INDI\n1 NAME Marie /B/\n1 SEX F\n1 FAMS @F0@\n\
            0 @P@ INDI\n1 NAME Paul /A/\n1 SEX M\n1 FAMC @F0@\n1 FAMS @F1@\n\
            0 @M@ INDI\n1 NAME Anne /C/\n1 SEX F\n1 FAMS @F1@\n\
            0 @I1@ INDI\n1 NAME Luc /A/\n1 SEX M\n1 FAMC @F1@\n1 FAMS @F2@\n\
            0 @I2@ INDI\n1 NAME Lea /A/\n1 SEX F\n1 FAMC @F1@\n\
            0 @W@ INDI\n1 NAME Eve /D/\n1 SEX F\n1 FAMS @F2@\n\
            0 @K@ INDI\n1 NAME Tom /A/\n1 SEX M\n1 FAMC @F2@\n\
            0 @F0@ FAM\n1 HUSB @GF@\n1 WIFE @GM@\n1 CHIL @P@\n\
            0 @F1@ FAM\n1 HUSB @P@\n1 WIFE @M@\n1 CHIL @I1@\n1 CHIL @I2@\n\
            0 @F2@ FAM\n1 HUSB @I1@\n1 WIFE @W@\n1 CHIL @K@\n\
            0 TRLR\n";
        Gedcom::parse(text).unwrap()
    }

    #[test]
    fn columns() {
        let g = ged();
        let l = build(&g, "I1", 3, 2);
        let row = |x: &str| l.find(x).map(|c| (c.x / COL).round() as i32);
        assert_eq!(l.find("I1").map(|c| (c.x, c.y)), Some((0., 0.)));
        assert_eq!(row("P"), Some(1));
        assert_eq!(row("GF"), Some(2));
        assert_eq!(row("K"), Some(-1));
        assert_eq!(row("W"), Some(0));
        assert_eq!(row("I2"), Some(0));
        // Conjointe en dessous, sœur au-dessus.
        assert!(l.find("W").unwrap().y > 0.);
        assert!(l.find("I2").unwrap().y < 0.);
        assert!(!l.more_up && !l.more_down);
    }

    #[test]
    fn lazy_generations() {
        let g = ged();
        let l = build(&g, "I1", 1, 0);
        assert!(l.find("GF").is_none());
        assert!(l.find("K").is_none());
        assert!(l.more_up && l.more_down);
        let l = build(&g, "GF", 1, 1);
        assert!(l.find("P").is_some() && l.find("I1").is_none());
        assert!(l.more_down);
    }

    #[test]
    fn links_follow_cards() {
        let g = ged();
        let l = build(&g, "I1", 3, 2);
        let mut ys: Vec<f32> = l.cards.iter().map(|c| c.y).collect();
        let ip = l.cards.iter().position(|c| c.xref() == Some("P")).unwrap();
        let (px, py) = (l.cards[ip].x, l.cards[ip].y + CARD_H / 2.);
        // Le père est relié à son fils par un trait qui arrive en son milieu…
        assert!(l.segments(&ys).any(|s| s[2] == px && s[3] == py));
        // … et ce trait le suit quand il glisse.
        ys[ip] += 100.;
        assert!(l.segments(&ys).any(|s| s[2] == px && s[3] == py + 100.));
        assert!(!l.segments(&ys).any(|s| s[2] == px && s[3] == py));
    }

    #[test]
    fn columns_stay_put() {
        // Charger une génération ne déplace les cases que verticalement.
        let g = ged();
        let small = build(&g, "I1", 1, 0);
        let big = build(&g, "I1", 3, 2);
        for c in small.cards.iter().filter(|c| c.xref().is_some()) {
            let x = c.xref().unwrap();
            assert_eq!(big.find(x).map(|b| b.x), Some(c.x), "{x}");
        }
    }

    #[test]
    fn no_overlap() {
        let g = ged();
        let l = build(&g, "I1", 4, 4);
        for (i, a) in l.cards.iter().enumerate() {
            for b in &l.cards[i + 1..] {
                let apart = (a.y - b.y).abs() >= CARD_H || (a.x - b.x).abs() >= CARD_W;
                assert!(apart, "{:?} / {:?}", a.slot, b.slot);
            }
        }
    }
}
