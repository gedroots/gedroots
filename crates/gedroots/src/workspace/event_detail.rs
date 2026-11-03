//! Read-only window of an event, opened by a click in a person's timeline: date and age, place,
//! spouse or principal people, associates (witnesses, godparents…) with their role, cited sources
//! with their page, media and notes. “Edit…” opens the edit window; Esc or a click outside closes
//! it. Every name in it opens its record.

use super::*;

/// Event shown in the window, seen from the record of `subject`.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct EventDetail {
    pub owner: String,
    pub ix: usize,
    pub subject: String,
}

impl Workspace {
    pub(super) fn open_event_detail(&mut self, owner: String, ix: usize, subject: String, cx: &mut Context<Self>) {
        self.event_detail = Some(EventDetail { owner, ix, subject });
        cx.notify();
    }

    pub(super) fn close_event_detail(&mut self, cx: &mut Context<Self>) -> bool {
        let open = self.event_detail.take().is_some();
        if open {
            cx.notify();
        }
        open
    }

    /// Link in the window: closes it, then opens the record.
    fn detail_link(&self, id: impl Into<ElementId>, x: &str, cx: &mut Context<Self>) -> Stateful<Div> {
        let name = self.doc.as_ref().map(|d| d.ged.record_label(x)).unwrap_or_default();
        let target = x.to_string();
        div().id(id.into()).cursor_pointer().text_color(theme::accent()).hover(|d| d.underline()).child(name).on_click(
            cx.listener(move |this, _, w, cx| {
                this.event_detail = None;
                this.goto(target.clone(), w, cx);
            }),
        )
    }

    pub(super) fn render_event_detail(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let d = self.event_detail.clone()?;
        let doc = self.doc.as_ref()?;
        let g = &doc.ged;
        let node = g.get(&d.owner)?.children.get(d.ix).filter(|n| event_label(&n.tag).is_some())?;
        let e = Event { node };
        let title = match e.value() {
            Some(v) => t!("ws.event_value", label = e.label(), value = v).into_owned(),
            None => e.label(),
        };
        let date = e.date().map(|d| d.display());
        let age = (d.owner != d.subject || e.tag() != "BIRT")
            .then(|| g.individual(&d.subject).zip(e.date_raw()).and_then(|(p, raw)| timeline::age_at(&p, raw)))
            .flatten();
        let subtitle = [date, age].into_iter().flatten().collect::<Vec<_>>().join(" · ");

        // Principal people other than the person whose record is open: the spouse of a union, or
        // the people of an event they attended.
        let others: Vec<String> = g.event_principals(&d.owner).into_iter().filter(|x| *x != d.subject).collect();
        let others_label = if g.family(&d.owner).is_some() && d.owner_is_union_of(g) {
            t!("ws.ev_with")
        } else if others.len() > 1 {
            t!("ws.ev_people")
        } else {
            t!("ws.ev_person")
        };
        let associates = g.event_associates(&d.owner, d.ix);
        let sources: Vec<(String, Option<String>)> = node
            .children_with("SOUR")
            .filter_map(|s| Some((s.pointer()?.to_string(), s.child_val("PAGE").map(str::to_string))))
            .collect();
        let notes: Vec<String> = node.children_with("NOTE").map(|n| n.text()).filter(|t| !t.is_empty()).collect();
        let place = e.place().filter(|p| !p.is_empty()).map(str::to_string);

        let row = |label: std::borrow::Cow<'static, str>, value: AnyElement| {
            h_flex()
                .gap_3()
                .items_start()
                .child(div().w(px(96.)).flex_none().text_xs().pt_0p5().text_color(theme::muted()).child(label))
                .child(div().flex_1().min_w_0().child(value))
        };
        let mut body = v_flex().gap_3();
        if !others.is_empty() {
            body = body.child(row(
                others_label,
                v_flex()
                    .gap_0p5()
                    .children(others.iter().enumerate().map(|(n, x)| self.detail_link(("ev-d-p", n), x, cx)))
                    .into_any_element(),
            ));
        }
        if let Some(p) = place {
            let target = p.clone();
            body = body.child(row(
                t!("ws.ev_place"),
                div()
                    .id("ev-d-place")
                    .cursor_pointer()
                    .text_color(theme::accent())
                    .hover(|d| d.underline())
                    .child(p)
                    .on_click(cx.listener(move |this, _, w, cx| {
                        this.event_detail = None;
                        this.goto_place(target.clone(), w, cx)
                    }))
                    .into_any_element(),
            ));
        }
        if let Some(a) = e.address() {
            body = body.child(row(t!("ws.ev_address"), div().child(a).into_any_element()));
        }
        if !associates.is_empty() {
            body = body.child(row(
                t!("ws.ev_associates"),
                v_flex()
                    .gap_0p5()
                    .children(associates.iter().enumerate().map(|(n, a)| {
                        h_flex().gap_1().flex_wrap().child(self.detail_link(("ev-d-a", n), &a.person, cx)).child(
                            div().text_color(theme::muted()).child(match &a.detail {
                                Some(d) => format!("· {}, {d}", a.role),
                                None => format!("· {}", a.role),
                            }),
                        )
                    }))
                    .into_any_element(),
            ));
        }
        if !sources.is_empty() {
            body = body.child(row(
                t!("ws.ev_sources"),
                v_flex()
                    .gap_1()
                    .children(sources.iter().enumerate().map(|(n, (s, page))| {
                        v_flex().child(self.detail_link(("ev-d-s", n), s, cx)).when_some(page.clone(), |d, p| {
                            d.child(div().text_xs().text_color(theme::muted()).child(p))
                        })
                    }))
                    .into_any_element(),
            ));
        }
        if let Some(media) = self.render_event_media(&d.owner, d.ix, &e, cx) {
            body = body.child(row(t!("ws.ev_media"), media));
        }
        if !notes.is_empty() {
            body = body.child(row(
                t!("ws.ev_notes"),
                v_flex().gap_1().children(notes.into_iter().map(|n| div().child(n))).into_any_element(),
            ));
        }

        let (owner, ix) = (d.owner.clone(), d.ix);
        let edit = Button::new("ev-d-edit").label(t!("ws.ev_edit")).on_click(cx.listener(move |this, _, w, cx| {
            this.event_detail = None;
            this.open_event_dialog(owner.clone(), Some(ix), w, cx)
        }));
        let close = Button::new("ev-d-close").ghost().xsmall().icon(IconName::Close).tooltip(t!("ws.close")).on_click(
            cx.listener(|this, _, _, cx| {
                this.close_event_detail(cx);
            }),
        );
        let head = h_flex()
            .p_4()
            .gap_2()
            .items_start()
            .border_b_1()
            .border_color(theme::border())
            .child(
                v_flex()
                    .flex_1()
                    .gap_0p5()
                    .child(div().text_base().font_weight(FontWeight::SEMIBOLD).child(title))
                    .when(!subtitle.is_empty(), |d| d.child(div().text_color(theme::muted()).child(subtitle))),
            )
            .child(close);
        Some(
            div()
                .absolute()
                .inset_0()
                .flex()
                .justify_center()
                .items_start()
                .pt(px(90.))
                .bg(theme::scrim())
                .child(
                    v_flex()
                        .id("event-detail")
                        .occlude()
                        .w(px(460.))
                        .max_h(px(600.))
                        .rounded(theme::RADIUS_LG)
                        .border_1()
                        .border_color(theme::border_strong())
                        .bg(theme::panel())
                        .shadow_lg()
                        .on_mouse_down_out(cx.listener(|this, _, _, cx| {
                            this.close_event_detail(cx);
                        }))
                        .child(head)
                        .child(div().id("event-detail-body").overflow_y_scroll().flex_1().min_h_0().p_4().child(body))
                        .child(h_flex().p_3().justify_end().border_t_1().border_color(theme::border()).child(edit)),
                )
                .into_any_element(),
        )
    }
}

impl EventDetail {
    /// The event belongs to a union of the person (their marriage, divorce…).
    fn owner_is_union_of(&self, g: &Gedcom) -> bool {
        g.event_principals(&self.owner).contains(&self.subject)
    }
}
