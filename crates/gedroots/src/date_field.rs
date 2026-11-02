//! Reusable date field.
//!
//! Inspired by the best date editors (Gramps, RootsMagic, Heredis):
//! - a single input in everyday language (“about 1850”, “between 1820 and 1825”, “15 brumaire an
//!   XII”, “3 mars 1700 julien”…), understood as you type;
//! - a discreet preview line when the interpretation adds something (normalized form, Gregorian
//!   equivalent, error);
//! - a calendar button that opens the details: precision, calendar (with the date converted into
//!   each, one click to convert) and original text.
//!
//! The stored value is only rewritten if the user edits the field.

use crate::theme;
use gedcom::date::{Calendar, Date, DateKind, Parsed, parse_input};
use gedcom::sort_key;
use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::popover::Popover;
use gpui_kit::component::{Sizable, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use rust_i18n::t;

pub struct DateField {
    input: Entity<InputState>,
    phrase: Entity<InputState>,
    /// Text displayed on opening, and the original value (`DATE`, `PHRASE`).
    original: (String, String, String),
    _subs: Vec<Subscription>,
}

/// Creates a date field for a GEDCOM value and its phrase.
pub fn date_field<T: 'static>(
    value: &str,
    phrase: &str,
    window: &mut Window,
    cx: &mut Context<T>,
) -> Entity<DateField> {
    cx.new(|cx| DateField::new(value, phrase, window, cx))
}

impl DateField {
    /// Replaces the greyed example of the empty field (e.g. a period).
    pub fn with_placeholder(self, hint: impl Into<SharedString>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        self.input.update(cx, |s, cx| s.set_placeholder(hint.into(), window, cx));
        self
    }

    pub fn new(value: &str, phrase: &str, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let parsed = Parsed::from_gedcom(value, phrase);
        let text = parsed.to_input_text();
        let input = {
            let text = text.clone();
            cx.new(|cx| InputState::new(window, cx).placeholder(t!("date_field.placeholder")).default_value(text))
        };
        let phrase_input = {
            let p = parsed.phrase.clone();
            cx.new(|cx| InputState::new(window, cx).placeholder(t!("date_field.phrase_placeholder")).default_value(p))
        };
        let main = cx.subscribe_in(&input, window, |this, _, ev: &InputEvent, window, cx| {
            if matches!(ev, InputEvent::Change) {
                // The original text follows the main input.
                if let Ok(p) = this.parsed(cx) {
                    let p = if p.date.is_none() { String::new() } else { p.phrase };
                    if this.phrase.read(cx).value().as_ref() != p {
                        this.phrase.update(cx, |s, cx| s.set_value(p, window, cx));
                    }
                }
                cx.notify();
            }
        });
        let side = cx.subscribe_in(&phrase_input, window, |this, input, ev: &InputEvent, window, cx| {
            if matches!(ev, InputEvent::Change) {
                let phrase = input.read(cx).value().trim().to_string();
                if let Ok(p) = this.parsed(cx)
                    && p.date.is_some()
                {
                    this.replace(Parsed { phrase, ..p }, window, cx);
                }
            }
        });
        Self {
            input,
            phrase: phrase_input,
            original: (text, value.trim().to_string(), phrase.trim().to_string()),
            _subs: vec![main, side],
        }
    }

    /// Input field (to focus it or listen to its changes).
    pub fn input(&self) -> &Entity<InputState> {
        &self.input
    }

    fn text(&self, cx: &App) -> String {
        self.input.read(cx).value().to_string()
    }

    pub fn parsed(&self, cx: &App) -> Result<Parsed, String> {
        parse_input(&self.text(cx))
    }

    /// `(DATE value, PHRASE)` to write; error if the date is impossible.
    pub fn value(&self, cx: &App) -> Result<(String, String), String> {
        let text = self.text(cx);
        if text.trim() == self.original.0.trim() {
            return Ok((self.original.1.clone(), self.original.2.clone()));
        }
        let p = parse_input(&text).map_err(|e| t!("date_field.invalid", text = text.trim(), error = e).to_string())?;
        Ok((p.value(), p.phrase))
    }

    pub fn clear(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.input.update(cx, |s, cx| s.set_value("", window, cx));
        self.phrase.update(cx, |s, cx| s.set_value("", window, cx));
        self.original = Default::default();
        cx.notify();
    }

    /// Replaces the input with the written form of `p` (undoable).
    fn replace(&mut self, p: Parsed, window: &mut Window, cx: &mut Context<Self>) {
        let text = p.to_input_text();
        self.input.update(cx, |s, cx| s.replace_all(text, window, cx));
        cx.notify();
    }

    fn set_kind(&mut self, kind: DateKind, window: &mut Window, cx: &mut Context<Self>) {
        if let Ok(p) = self.parsed(cx)
            && let Some(d) = p.date.as_ref().and_then(|d| d.with_kind(kind))
        {
            self.replace(Parsed { date: Some(d), ..p }, window, cx);
        }
    }

    fn set_calendar(&mut self, to: Calendar, window: &mut Window, cx: &mut Context<Self>) {
        if let Ok(p) = self.parsed(cx)
            && let Some(d) = p.date.as_ref().and_then(|d| switch_calendar(d, &to))
        {
            self.replace(Parsed { date: Some(d), ..p }, window, cx);
        }
    }

    /// Preview line, only if it tells something new.
    fn preview(&self, cx: &App) -> Option<(String, Hsla)> {
        let text = self.text(cx);
        let t = text.trim();
        if t.is_empty()
            || t == self.original.0.trim() && matches!(self.parsed(cx), Ok(Parsed { date: Some(Date::Other(_)), .. }))
        {
            return None;
        }
        Some(match parse_input(t) {
            Err(e) => (e, theme::danger()),
            Ok(Parsed { date: None, .. }) => (t!("date_field.text_only").into(), theme::warn()),
            Ok(p) => {
                let d = p.date.as_ref()?;
                let shown = d.display();
                let same = sort_key(&p.to_input_text()).replace([' ', '.'], "") == sort_key(t).replace([' ', '.'], "");
                if same && shown == d.to_input_text() && p.phrase.is_empty() {
                    return None;
                }
                // Input already normalized: only the Gregorian equivalent is useful.
                let eq: Vec<String> = d.dates().iter().filter_map(|x| x.gregorian_equivalent()).collect();
                let mut s = if same && !eq.is_empty() { format!("= {}", eq.join(" – ")) } else { shown };
                if !p.phrase.is_empty() {
                    s.push_str(&format!(" · « {} »", p.phrase));
                }
                (s, theme::muted())
            }
        })
    }
}

/// Converts complete dates; otherwise, Gregorian ↔ Julian only changes the label.
fn switch_calendar(d: &Date, to: &Calendar) -> Option<Date> {
    d.try_map(|s| {
        s.convert(to).or_else(|| {
            let western = |c: &Calendar| matches!(c, Calendar::Gregorian | Calendar::Julian);
            (western(&s.calendar) && western(to)).then(|| gedcom::SimpleDate { calendar: to.clone(), ..s.clone() })
        })
    })
}

fn chip(id: impl Into<SharedString>, label: impl Into<SharedString>, on: bool, enabled: bool) -> Stateful<Div> {
    div()
        .id(ElementId::Name(id.into()))
        .px_2()
        .py(px(3.))
        .rounded(theme::RADIUS_SM)
        .border_1()
        .text_xs()
        .border_color(if on { theme::accent() } else { theme::border_strong() })
        .bg(if on { theme::accent_soft() } else { theme::surface() })
        .text_color(if on { theme::text() } else { theme::muted() })
        .when(enabled, |d| d.cursor_pointer().hover(|d| d.border_color(theme::accent())))
        .when(!enabled, |d| d.opacity(0.4))
        .child(label.into())
}

fn caps(label: impl Into<SharedString>) -> Div {
    div().text_xs().font_weight(FontWeight::SEMIBOLD).text_color(theme::muted()).child(label.into().to_uppercase())
}

fn details(this: &Entity<DateField>, cx: &App) -> AnyElement {
    let f = this.read(cx);
    let parsed = f.parsed(cx).ok();
    let date = parsed.as_ref().and_then(|p| p.date.clone()).filter(|d| !matches!(d, Date::Other(_)));
    let kind = date.as_ref().and_then(Date::kind);
    let current = date.as_ref().and_then(|d| d.calendar().cloned());
    let id = this.entity_id();
    let kinds = DateKind::ALL.into_iter().map(|k| {
        let e = this.clone();
        chip(format!("dk-{id}-{k:?}"), k.label(), kind == Some(k), date.is_some())
            .when(date.is_some() && kind != Some(k), |c| {
                c.on_click(move |_, window, cx| e.update(cx, |f, cx| f.set_kind(k, window, cx)))
            })
    });
    let calendars = Calendar::KNOWN.into_iter().map(|c| {
        let converted = date.as_ref().and_then(|d| switch_calendar(d, &c));
        let on = current.as_ref() == Some(&c);
        // Only the converted dates, without the qualifier or the calendar name.
        let shown = converted
            .as_ref()
            .filter(|_| !on)
            .map(|d| {
                let parts: Vec<String> = d
                    .dates()
                    .iter()
                    .map(|x| x.core_text().trim_end_matches(" julien").trim_end_matches(" hébraïque").to_string())
                    .collect();
                parts.join(" – ")
            })
            .unwrap_or_default();
        let enabled = converted.is_some() && !on;
        let e = this.clone();
        let to = c.clone();
        h_flex()
            .id(ElementId::Name(format!("dc-{id}-{}", c.tag()).into()))
            .gap_2()
            .px_2()
            .py_1()
            .rounded(theme::RADIUS_SM)
            .text_sm()
            .when(on, |d| d.bg(theme::accent_soft()))
            .when(enabled, |d| d.cursor_pointer().hover(|d| d.bg(theme::hover())))
            .when(!enabled && !on, |d| d.opacity(0.45))
            .child(div().w(px(84.)).flex_none().child(c.label().to_string()))
            .child(div().flex_1().min_w_0().truncate().text_color(theme::muted()).child(shown))
            .when(enabled, |d| {
                d.on_click(move |_, window, cx| e.update(cx, |f, cx| f.set_calendar(to.clone(), window, cx)))
            })
    });
    v_flex()
        .w(px(320.))
        .gap_3()
        .child(
            v_flex()
                .gap_1()
                .child(caps(t!("date_field.precision")))
                .child(h_flex().flex_wrap().gap_1().children(kinds)),
        )
        .child(v_flex().gap_1().child(caps(t!("date_field.calendar"))).children(calendars))
        .when(date.is_some(), |d| {
            d.child(v_flex().gap_1().child(caps(t!("date_field.original_text"))).child(Input::new(&f.phrase).small()))
        })
        .into_any_element()
}

impl Render for DateField {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let me = cx.entity();
        let preview = self.preview(cx);
        let button = Popover::new(ElementId::Name(format!("date-pop-{}", me.entity_id()).into()))
            .anchor(Anchor::TopRight)
            .trigger(
                Button::new(ElementId::Name(format!("date-btn-{}", me.entity_id()).into()))
                    .ghost()
                    .xsmall()
                    .icon(IconName::Calendar),
            )
            .content(move |_, _, cx| details(&me, cx));
        v_flex()
            .gap(px(2.))
            .min_w_0()
            .child(Input::new(&self.input).small().suffix(button))
            .when_some(preview, |d, (text, color)| {
                d.child(div().text_xs().text_color(color).line_height(px(14.)).child(text))
            })
    }
}
