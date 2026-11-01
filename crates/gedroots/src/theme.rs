//! Palette sombre reprise de la maquette « Lignée » (canevas de design).

use gpui_kit::component::{Theme, ThemeMode};
use gpui_kit::{App, Hsla, rgb};

pub fn c(hex: u32) -> Hsla {
    rgb(hex).into()
}

/// Fond principal.
pub fn bg() -> Hsla {
    c(0x121418)
}
/// Barres et panneaux.
pub fn panel() -> Hsla {
    c(0x181b21)
}
/// Cartes, champs.
pub fn surface() -> Hsla {
    c(0x1f232b)
}
/// Survol / sélection discrète.
pub fn hover() -> Hsla {
    c(0x262b35)
}
pub fn border() -> Hsla {
    c(0x2b303a)
}
pub fn border_strong() -> Hsla {
    c(0x3a4150)
}
pub fn text() -> Hsla {
    c(0xdfe3ea)
}
pub fn muted() -> Hsla {
    c(0x98a1b0)
}
pub fn accent() -> Hsla {
    c(0x7aa9ff)
}
pub fn accent_soft() -> Hsla {
    let mut a = accent();
    a.a = 0.16;
    a
}
/// Lignée paternelle.
pub fn paternal() -> Hsla {
    c(0xe8a35d)
}
/// Lignée maternelle.
pub fn maternal() -> Hsla {
    c(0x5ec9b6)
}
pub fn ok() -> Hsla {
    c(0x79c88a)
}
pub fn warn() -> Hsla {
    c(0xf0b35a)
}
pub fn danger() -> Hsla {
    c(0xf07a6e)
}

/// Couleur de lignée d'un numéro Sosa (1 = de cujus, puis paternelle/maternelle).
pub fn lineage(sosa: usize) -> Hsla {
    if sosa <= 1 {
        return accent();
    }
    let mut k = sosa;
    while k > 3 {
        k /= 2;
    }
    if k == 2 { paternal() } else { maternal() }
}

pub fn install(cx: &mut App) {
    Theme::change(ThemeMode::Dark, None, cx);
    Theme::update(cx, |t| {
        t.background = bg();
        t.foreground = text();
        t.border = border();
        t.input = border_strong();
        t.muted = surface();
        t.muted_foreground = muted();
        t.primary = accent();
        t.primary_hover = c(0xa9c6ff);
        t.primary_active = c(0x5f93f0);
        t.primary_foreground = c(0x0d1420);
        t.accent = hover();
        t.accent_foreground = text();
        t.secondary = surface();
        t.secondary_hover = hover();
        t.secondary_foreground = text();
        t.popover = surface();
        t.popover_foreground = text();
        t.list_active = accent_soft();
        t.list_active_border = accent();
        t.list_hover = hover();
        t.ring = accent();
        t.caret = accent();
        t.selection = {
            let mut s = accent();
            s.a = 0.3;
            s
        };
        t.link = accent();
    });
}
