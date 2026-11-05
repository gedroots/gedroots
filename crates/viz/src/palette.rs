//! Colours of the visualizations: on paper for the books, and the app's light and dark themes
//! (the same roles as `crates/gedroots/src/theme.rs`, see `design.md`).

/// Groups (of places): muted colours that stay apart in greyscale, always named in a legend.
pub const GROUP_COLORS: [&str; 7] = ["#C9A35B", "#7E9C7A", "#8C86B8", "#C98A6B", "#5E8FA8", "#B8708A", "#D2B48C"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Palette {
    /// Background of the drawing.
    pub paper: &'static str,
    /// Text and strong lines.
    pub ink: &'static str,
    /// Secondary text.
    pub muted: &'static str,
    /// Grid lines, outlines of empty cells.
    pub rule: &'static str,
    /// Empty part of a bar.
    pub track: &'static str,
    pub accent: &'static str,
    pub paternal: &'static str,
    pub maternal: &'static str,
    /// Map: sea and land.
    pub sea: &'static str,
    pub land: &'static str,
    /// Text over a group colour (the cells of a fan).
    pub on_group: &'static str,
    /// What belongs to no named group.
    pub other: &'static str,
}

impl Palette {
    /// Dark ink on white paper, the violet of GEDRoots as the accent.
    pub const PRINT: Palette = Palette {
        paper: "#FFFFFF",
        ink: "#1F1B16",
        muted: "#6B6257",
        rule: "#D5CAB7",
        track: "#F4EFE6",
        accent: "#6D3DF0",
        paternal: "#3F5BD8",
        maternal: "#9C36B5",
        sea: "#EEF1F2",
        land: "#FFFFFF",
        on_group: "#1F1B16",
        other: "#A9A39A",
    };

    /// The app in light mode: the paper's colours on the app's white canvas.
    pub const LIGHT: Palette = Palette::PRINT;

    /// The app in dark mode.
    pub const DARK: Palette = Palette {
        paper: "#141414",
        ink: "#ECECEC",
        muted: "#A3A3A3",
        rule: "#3D3D3D",
        track: "#222222",
        accent: "#B997FF",
        paternal: "#8FA2FF",
        maternal: "#E39BF0",
        sea: "#1A1D20",
        land: "#222222",
        on_group: "#1F1B16",
        other: "#7A756D",
    };

    /// Colour of group `i` when `keep` groups are named: the others share [`Palette::other`].
    pub fn group(&self, i: usize, keep: usize) -> &'static str {
        if i < keep { GROUP_COLORS[i] } else { self.other }
    }
}
