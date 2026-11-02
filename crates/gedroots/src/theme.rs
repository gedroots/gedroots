//! Design system GEDRoots : jetons de couleur (clair / sombre), typographie, espacements et rayons.
//! Fonds neutres (blanc et ocre en clair, gris et noir en sombre) ; le violet n'est qu'un accent.
//!
//! Les valeurs viennent du kit de marque v1.0 (`design.md`). Les vues n'emploient jamais
//! de couleur en dur : elles passent par les fonctions de ce module, qui lisent la palette du mode
//! courant. Le mode suit l'apparence du système (`sync`), et chaque changement redessine les fenêtres.

use gpui_kit::component::{Theme, ThemeMode};
use gpui_kit::{
    App, Div, ElementId, FontFeatures, Hsla, InteractiveElement, Pixels, SharedString, Stateful,
    StatefulInteractiveElement, Styled, Window, WindowAppearance, div, px, rgb,
};
use std::borrow::Cow;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

/// Rôles de couleur de l'application. Un jeu par mode.
struct Palette {
    /// Fond du canevas principal.
    bg: u32,
    /// Barres et panneaux latéraux.
    panel: u32,
    /// Cartes, champs.
    surface: u32,
    /// Survol / sélection discrète.
    hover: u32,
    border: u32,
    border_strong: u32,
    text: u32,
    muted: u32,
    /// Violet primaire : actions, liens, sélection.
    accent: u32,
    accent_hover: u32,
    accent_pressed: u32,
    /// Texte posé sur le violet primaire.
    on_accent: u32,
    paternal: u32,
    maternal: u32,
    ok: u32,
    warn: u32,
    danger: u32,
}

const LIGHT: Palette = Palette {
    bg: 0xFFFFFF,
    panel: 0xFAF7F1,
    surface: 0xF4EFE6,
    hover: 0xEEE7DA,
    border: 0xE6DED0,
    border_strong: 0xD5CAB7,
    text: 0x1F1B16,
    muted: 0x6B6257,
    accent: 0x6D3DF0,
    accent_hover: 0x5B2BDD,
    accent_pressed: 0x4C21B8,
    on_accent: 0xFFFFFF,
    paternal: 0x3F5BD8,
    maternal: 0x9C36B5,
    ok: 0x1A7346,
    warn: 0x9A5B00,
    danger: 0xC42B48,
};

const DARK: Palette = Palette {
    bg: 0x141414,
    panel: 0x1A1A1A,
    surface: 0x222222,
    hover: 0x2B2B2B,
    border: 0x2E2E2E,
    border_strong: 0x3D3D3D,
    text: 0xECECEC,
    muted: 0xA3A3A3,
    accent: 0xB997FF,
    accent_hover: 0xC8AAFF,
    accent_pressed: 0xA77CF6,
    on_accent: 0x1A1A1A,
    paternal: 0x8FA2FF,
    maternal: 0xE39BF0,
    ok: 0x6FD3A0,
    warn: 0xF0B35A,
    danger: 0xF2828E,
};

static DARK_MODE: AtomicBool = AtomicBool::new(false);

fn p() -> &'static Palette {
    if DARK_MODE.load(Ordering::Relaxed) { &DARK } else { &LIGHT }
}

pub fn is_dark() -> bool {
    DARK_MODE.load(Ordering::Relaxed)
}

pub fn c(hex: u32) -> Hsla {
    rgb(hex).into()
}

pub fn with_alpha(mut c: Hsla, a: f32) -> Hsla {
    c.a = a;
    c
}

// --- Couleurs -------------------------------------------------------------------------------

/// Fond principal.
pub fn bg() -> Hsla {
    c(p().bg)
}
/// Barres et panneaux.
pub fn panel() -> Hsla {
    c(p().panel)
}
/// Cartes, champs.
pub fn surface() -> Hsla {
    c(p().surface)
}
/// Survol / sélection discrète.
pub fn hover() -> Hsla {
    c(p().hover)
}
pub fn border() -> Hsla {
    c(p().border)
}
pub fn border_strong() -> Hsla {
    c(p().border_strong)
}
pub fn text() -> Hsla {
    c(p().text)
}
pub fn muted() -> Hsla {
    c(p().muted)
}
pub fn accent() -> Hsla {
    c(p().accent)
}
pub fn accent_soft() -> Hsla {
    with_alpha(accent(), if is_dark() { 0.16 } else { 0.10 })
}
pub fn on_accent() -> Hsla {
    c(p().on_accent)
}
/// Lignée paternelle.
pub fn paternal() -> Hsla {
    c(p().paternal)
}
/// Lignée maternelle.
pub fn maternal() -> Hsla {
    c(p().maternal)
}
pub fn ok() -> Hsla {
    c(p().ok)
}
pub fn warn() -> Hsla {
    c(p().warn)
}
pub fn danger() -> Hsla {
    c(p().danger)
}
/// Voile sous les fenêtres modales.
pub fn scrim() -> Hsla {
    with_alpha(c(0x000000), if is_dark() { 0.55 } else { 0.35 })
}
/// Fond des visionneuses d'images : sombre dans les deux modes, pour juger les documents.
pub fn viewer() -> Hsla {
    c(0x0E0E0E)
}

/// Sur une image (cadres et étiquettes posés sur une photo ou un acte) : couleurs du mode sombre,
/// quel que soit le mode, sur un fond d'étiquette noir translucide.
pub fn image_label_bg() -> Hsla {
    with_alpha(c(0x000000), 0.72)
}
pub fn image_text() -> Hsla {
    c(DARK.text)
}
pub fn image_muted() -> Hsla {
    c(DARK.muted)
}
pub fn image_accent() -> Hsla {
    c(DARK.accent)
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

// --- Rayons ---------------------------------------------------------------------------------

/// Champs, puces, vignettes, cases de l'arbre.
pub const RADIUS_SM: Pixels = px(8.);
/// Boutons, menus, listes déroulantes.
pub const RADIUS_MD: Pixels = px(12.);
/// Cartes et fenêtres modales.
pub const RADIUS_LG: Pixels = px(20.);

// --- Zones qui défilent ----------------------------------------------------------------------

/// Place laissée à l'anneau de focus des champs (3 px, peint hors de leur bordure).
pub const FOCUS_ROOM: Pixels = px(4.);

/// Zone qui défile verticalement sans rogner l'anneau de focus des champs qu'elle contient.
///
/// Une zone qui défile coupe tout ce qui dépasse de ses bords, y compris l'anneau d'un champ posé
/// contre eux. Elle prend donc `FOCUS_ROOM` de retrait intérieur, compensé par une marge négative :
/// le contenu reste aligné au pixel près sur ce qui l'entoure. Toute zone qui défile et contient
/// des champs passe par ici plutôt que par `overflow_y_scroll()`.
pub fn scroll_y(id: impl Into<ElementId>) -> Stateful<Div> {
    div().id(id).overflow_y_scroll().p(FOCUS_ROOM).m(-FOCUS_ROOM)
}

// --- Typographie ----------------------------------------------------------------------------

pub const FONT_FAMILY: &str = "Inter";

const FONTS: [&[u8]; 4] = [
    include_bytes!("../assets/fonts/Inter-Regular.ttf"),
    include_bytes!("../assets/fonts/Inter-Medium.ttf"),
    include_bytes!("../assets/fonts/Inter-SemiBold.ttf"),
    include_bytes!("../assets/fonts/Inter-Bold.ttf"),
];

/// Chiffres tabulaires : dates et effectifs restent alignés en colonne.
pub fn tabular_nums() -> FontFeatures {
    FontFeatures(Arc::new(vec![("tnum".into(), 1)]))
}

// --- Installation ---------------------------------------------------------------------------

/// Mode imposé pour les captures et les tests (`GEDROOTS_APPEARANCE=light|dark`).
fn forced_dark() -> Option<bool> {
    match std::env::var("GEDROOTS_APPEARANCE").ok()?.as_str() {
        "light" => Some(false),
        "dark" => Some(true),
        _ => None,
    }
}

fn appearance_is_dark(a: WindowAppearance) -> bool {
    matches!(a, WindowAppearance::Dark | WindowAppearance::VibrantDark)
}

pub fn install(cx: &mut App) {
    let fonts = FONTS.iter().map(|b| Cow::Borrowed(*b)).collect();
    if let Err(e) = cx.text_system().add_fonts(fonts) {
        eprintln!("Inter font unavailable: {e}");
    }
    apply(forced_dark().unwrap_or_else(|| appearance_is_dark(cx.window_appearance())), cx);
}

/// Suit l'apparence de la fenêtre (appelé quand le système passe en clair ou en sombre).
pub fn sync(window: &Window, cx: &mut App) {
    let dark = forced_dark().unwrap_or_else(|| appearance_is_dark(window.appearance()));
    if dark != is_dark() {
        apply(dark, cx);
    }
}

fn apply(dark: bool, cx: &mut App) {
    DARK_MODE.store(dark, Ordering::Relaxed);
    Theme::change(if dark { ThemeMode::Dark } else { ThemeMode::Light }, None, cx);
    let pal = p();
    Theme::update(cx, |t| {
        t.font_family = SharedString::from(FONT_FAMILY);
        t.radius = RADIUS_SM;
        t.radius_lg = RADIUS_MD;
        t.background = bg();
        t.foreground = text();
        t.border = border();
        t.input = border_strong();
        t.muted = surface();
        t.muted_foreground = muted();
        t.primary = accent();
        t.primary_hover = c(pal.accent_hover);
        t.primary_active = c(pal.accent_pressed);
        t.primary_foreground = on_accent();
        t.button_primary = accent();
        t.button_primary_hover = c(pal.accent_hover);
        t.button_primary_active = c(pal.accent_pressed);
        t.button_primary_foreground = on_accent();
        t.accent = hover();
        t.accent_foreground = text();
        t.secondary = surface();
        t.secondary_hover = hover();
        t.secondary_active = hover();
        t.secondary_foreground = text();
        t.popover = if dark { surface() } else { bg() };
        t.popover_foreground = text();
        t.sidebar = panel();
        t.sidebar_foreground = text();
        t.sidebar_border = border();
        t.title_bar = panel();
        t.title_bar_border = border();
        t.list_active = accent_soft();
        t.list_active_border = accent();
        t.list_hover = hover();
        t.table = bg();
        t.table_hover = hover();
        t.table_active = accent_soft();
        t.table_active_border = accent();
        t.table_head = panel();
        t.table_row_border = border();
        t.tab_bar = panel();
        t.tab_active = bg();
        t.ring = accent();
        t.caret = accent();
        t.selection = with_alpha(accent(), 0.3);
        t.link = accent();
        t.link_hover = c(pal.accent_hover);
        t.link_active = c(pal.accent_pressed);
        t.success = ok();
        t.warning = warn();
        t.danger = danger();
        t.drop_target = accent_soft();
        t.drag_border = accent();
        t.progress_bar = accent();
        t.slider_bar = accent();
        t.switch = border_strong();
        t.chart_grid = border();
    });
}
