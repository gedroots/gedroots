//! GEDRoots design system: color tokens (light / dark), typography, spacing and radii. Neutral
//! backgrounds (white and ochre in light mode, grey and black in dark mode); violet is only an
//! accent.
//!
//! The values come from the brand kit v1.0 (`design.md`). Views never use hard-coded colors: they
//! go through this module's functions, which read the palette of the current mode. The mode follows
//! the system's appearance (`sync`), and each change redraws the windows.

use gpui_kit::component::{Theme, ThemeMode};
use gpui_kit::{
    App, Div, ElementId, FontFeatures, Hsla, InteractiveElement, Pixels, SharedString, Stateful,
    StatefulInteractiveElement, Styled, Window, WindowAppearance, div, px, rgb,
};
use std::borrow::Cow;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

/// Color roles of the application. One set per mode.
struct Palette {
    /// Background of the main canvas.
    bg: u32,
    /// Bars and side panels.
    panel: u32,
    /// Cartes, champs.
    surface: u32,
    /// Hover / discreet selection.
    hover: u32,
    border: u32,
    border_strong: u32,
    /// Outline of the text fields.
    field_border: u32,
    text: u32,
    muted: u32,
    /// Primary violet: actions, links, selection.
    accent: u32,
    accent_hover: u32,
    accent_pressed: u32,
    /// Text on the primary violet.
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
    field_border: 0xD5CAB7,
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
    // 3:1 on `surface`, so an empty field stands out from a card.
    field_border: 0x6E6E6E,
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
/// Bars and panels.
pub fn panel() -> Hsla {
    c(p().panel)
}
/// Cartes, champs.
pub fn surface() -> Hsla {
    c(p().surface)
}
/// Hover / discreet selection.
pub fn hover() -> Hsla {
    c(p().hover)
}
pub fn border() -> Hsla {
    c(p().border)
}
pub fn border_strong() -> Hsla {
    c(p().border_strong)
}
/// Outline of the text fields.
pub fn field_border() -> Hsla {
    c(p().field_border)
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
/// Paternal line.
pub fn paternal() -> Hsla {
    c(p().paternal)
}
/// Maternal line.
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
/// Veil under modal windows.
pub fn scrim() -> Hsla {
    with_alpha(c(0x000000), if is_dark() { 0.55 } else { 0.35 })
}
/// Background of image viewers: dark in both modes, to judge the documents.
pub fn viewer() -> Hsla {
    c(0x0E0E0E)
}

/// On an image (frames and labels set on a photo or a record): dark mode colors, whatever the mode,
/// on a translucent black label background.
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

/// Line color of a Sosa number (1 = root person, then paternal/maternal).
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
/// Buttons, menus, drop-down lists.
pub const RADIUS_MD: Pixels = px(12.);
/// Cards and modal windows.
pub const RADIUS_LG: Pixels = px(20.);

// --- Scrolling areas -------------------------------------------------------------------------

/// Room left for the focus ring of fields (3 px, painted outside their border).
pub const FOCUS_ROOM: Pixels = px(4.);

/// Area that scrolls vertically without clipping the focus ring of the fields it contains.
///
/// A scrolling area clips everything that goes past its edges, including the ring of a field set
/// against them. So it takes `FOCUS_ROOM` of inner padding, offset by a negative margin: the
/// content stays aligned to the pixel with what surrounds it. Every scrolling area that contains
/// fields goes through here rather than through `overflow_y_scroll()`.
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

/// Tabular figures: dates and counts stay aligned in columns. Only on figures: Inter's `tnum` also
/// widens spaces, hyphens and punctuation, so running text would read “Saint - Lô”.
pub fn tabular_nums() -> FontFeatures {
    FontFeatures(Arc::new(vec![("tnum".into(), 1)]))
}

// --- Installation ---------------------------------------------------------------------------

/// Mode forced for screenshots and tests (`GEDROOTS_APPEARANCE=light|dark`).
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

/// Follows the window's appearance (called when the system switches to light or dark).
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
        t.input = field_border();
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
