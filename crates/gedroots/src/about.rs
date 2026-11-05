//! About GEDRoots: a small window of its own, like the standard About panel of macOS. It opens
//! from GEDRoots › About GEDRoots on macOS and Help › About GEDRoots elsewhere, even without a
//! window, and only once: the command brings an open one to the front.

use crate::t;
use crate::theme;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::{Sizable, v_flex};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use std::time::Duration;

/// The open About window, if any.
#[derive(Default)]
struct Open(Option<AnyWindowHandle>);

impl Global for Open {}

/// Opens the About window, or brings the open one to the front.
pub fn open(cx: &mut App) {
    if let Some(handle) = cx.try_global::<Open>().and_then(|o| o.0)
        && cx.update_window(handle, |_, window, _| window.activate_window()).is_ok()
    {
        return;
    }
    let mac = cfg!(target_os = "macos");
    let bounds = Bounds::centered(None, size(px(340.), px(if mac { 380. } else { 340. })), cx);
    let options = WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(bounds)),
        titlebar: Some(TitlebarOptions {
            title: Some(t!("about.title").into_owned().into()),
            // macOS: no visible title, the traffic lights over the window's background.
            appears_transparent: mac,
            traffic_light_position: mac.then(|| point(px(14.), px(13.))),
        }),
        is_resizable: false,
        is_minimizable: false,
        ..Default::default()
    };
    let opened = gpui_kit::open_window(options, cx, |window, cx| {
        let view = cx.new(|cx| About::new(window, cx));
        let focus = view.read(cx).focus.clone();
        window.focus(&focus, cx);
        view
    });
    if let Ok((window, _)) = opened {
        cx.set_global(Open(Some(window)));
    }
}

/// `GEDRoots 2026.11.3 (macOS 15.1, aarch64)`: what a bug report needs, copied by a click on the
/// version.
fn version_details() -> String {
    let os = sysinfo::System::long_os_version().unwrap_or_else(|| std::env::consts::OS.to_string());
    format!("GEDRoots {} ({os}, {})", env!("CARGO_PKG_VERSION"), std::env::consts::ARCH)
}

/// Year of the copyright: the first part of the CalVer version (`2026.11.3`).
fn year() -> &'static str {
    env!("CARGO_PKG_VERSION").split('.').next().unwrap_or_default()
}

struct About {
    focus: FocusHandle,
    /// “Copied” shows in place of the version for a moment after a click.
    copied: bool,
    _subs: Vec<Subscription>,
}

impl About {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let appearance = cx.observe_window_appearance(window, |_, window, cx| theme::sync(window, cx));
        Self { focus: cx.focus_handle(), copied: false, _subs: vec![appearance] }
    }

    fn copy_version(&mut self, cx: &mut Context<Self>) {
        cx.write_to_clipboard(ClipboardItem::new_string(version_details()));
        self.copied = true;
        cx.notify();
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(Duration::from_secs(2)).await;
            let _ = this.update(cx, |this, cx| {
                this.copied = false;
                cx.notify();
            });
        })
        .detach();
    }
}

impl Render for About {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let repository = env!("CARGO_PKG_REPOSITORY");
        let version = if self.copied {
            t!("about.copied").into_owned()
        } else {
            t!("about.version", version = env!("CARGO_PKG_VERSION")).into_owned()
        };
        v_flex()
            .id("about")
            .key_context("About")
            .track_focus(&self.focus)
            .size_full()
            .items_center()
            .justify_center()
            .px_6()
            .bg(theme::bg())
            .text_color(theme::text())
            // `Esc` and ⌘W / Ctrl+W close it, like the About windows of each platform.
            .on_key_down(cx.listener(|_, ev: &KeyDownEvent, window, cx| {
                let k = &ev.keystroke;
                if k.key == "escape" || (k.key == "w" && k.modifiers.secondary(cx) && !k.modifiers.shift) {
                    window.remove_window();
                }
            }))
            .when(cfg!(target_os = "macos"), |c| c.pt(px(16.)))
            .child(img("brand/app-icon.png").size(px(96.)))
            .child(div().mt_3().text_xl().font_weight(FontWeight::SEMIBOLD).child("GEDRoots"))
            .child(
                Button::new("about-version")
                    .ghost()
                    .small()
                    .mt_1()
                    .text_color(theme::muted())
                    .font_features(theme::tabular_nums())
                    .label(version)
                    .tooltip(t!("about.copy_details").into_owned())
                    .on_click(cx.listener(|this, _, _, cx| this.copy_version(cx))),
            )
            .child(
                div()
                    .mt_4()
                    .flex()
                    .gap_2()
                    .child(
                        Button::new("about-whats-new")
                            .small()
                            .label(t!("about.whats_new"))
                            .on_click(|_, _, cx| cx.open_url(&crate::updates::release_notes_url())),
                    )
                    .child(
                        Button::new("about-source")
                            .small()
                            .label(t!("about.source_code"))
                            .on_click(move |_, _, cx| cx.open_url(repository)),
                    ),
            )
            .child(
                v_flex()
                    .mt_6()
                    .items_center()
                    .gap_0p5()
                    .text_xs()
                    .text_color(theme::muted())
                    .child(t!("about.copyright", year = year()).into_owned())
                    .child(
                        div()
                            .id("about-license")
                            .cursor_pointer()
                            .text_color(theme::accent())
                            .hover(|s| s.underline())
                            .child(t!("about.license").into_owned())
                            .on_click(move |_, _, cx| cx.open_url(&format!("{repository}#license"))),
                    ),
            )
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn copyright_year_comes_from_the_version() {
        let year: u32 = super::year().parse().unwrap();
        assert!(year >= 2026);
    }

    #[test]
    fn version_details_name_the_app_and_the_machine() {
        let details = super::version_details();
        assert!(details.starts_with(&format!("GEDRoots {} (", env!("CARGO_PKG_VERSION"))));
        assert!(details.ends_with(&format!(", {})", std::env::consts::ARCH)));
    }
}
