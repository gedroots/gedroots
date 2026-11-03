//! File › Mobile Devices…: shares the open tree with the user's phones and tablets, pairs a new
//! device with a QR code, and lists the paired ones (see `crate::mobile`).

use super::add::AddDialog;
use super::*;
use crate::mobile::{self, Device, Kind, Status};
use gpui_kit::component::switch::Switch;
use std::path::Path;
use std::sync::Arc;
use std::time::Instant;

/// “Mobile Devices” window.
pub(super) struct MobileDialog {
    /// The file's sharing key; `None`: the tree is not saved in a file of its own.
    share: Option<String>,
    /// Device shown on the right; `None`: “Add a Device”.
    selected: Option<String>,
    code: Code,
    copied: bool,
}

/// Pairing code of “Add a Device”.
enum Code {
    None,
    Waiting,
    Ready { text: String, qr: Arc<RenderImage>, expires: Instant },
    Expired,
    Failed,
}

/// Side of the QR code.
const QR_SIDE: f32 = 168.;

impl Workspace {
    /// Sharing key of the open file: a tree saved in a file of its own (not the sample tree, nor a
    /// tree imported and not saved yet).
    pub(super) fn mobile_share(&self) -> Option<String> {
        let doc = self.doc.as_ref().filter(|d| !d.sample && !d.imported)?;
        Some(mobile::share_key(doc.path.as_ref()?))
    }

    pub(super) fn open_mobile_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let share = self.mobile_share();
        let devices = share.as_deref().map(mobile::share).unwrap_or_default().devices;
        self.add_dialog = Some(AddDialog::Mobile(Box::new(MobileDialog {
            share,
            selected: devices.first().map(|d| d.id.clone()),
            code: Code::None,
            copied: false,
        })));
        self.mobile_request_code(window, cx);
        cx.notify();
    }

    fn mobile_dialog(&mut self) -> Option<&mut MobileDialog> {
        match &mut self.add_dialog {
            Some(AddDialog::Mobile(d)) => Some(d),
            _ => None,
        }
    }

    /// Window closed: its pairing code can no longer be used.
    pub(super) fn mobile_dialog_closed(d: &MobileDialog) {
        if let Some(share) = &d.share
            && matches!(d.code, Code::Waiting | Code::Ready { .. })
        {
            mobile::cancel_pairing(share);
        }
    }

    /// Asks for a pairing code when “Add a Device” is shown with sharing on.
    fn mobile_request_code(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(d) = self.mobile_dialog() else { return };
        let Some(share) = d.share.clone() else { return };
        if d.selected.is_some() || !mobile::share(&share).enabled || !matches!(d.code, Code::None | Code::Expired) {
            return;
        }
        d.code = Code::Waiting;
        d.copied = false;
        let rx = mobile::pair_code(&share);
        cx.spawn_in(window, async move |this, cx| {
            let text = rx.recv().await.ok();
            let qr = text.as_deref().and_then(qr_image);
            let _ = this.update(cx, |this, cx| {
                if let Some(d) = this.mobile_dialog()
                    && matches!(d.code, Code::Waiting)
                {
                    d.code = match (text, qr) {
                        (Some(text), Some(qr)) => {
                            Code::Ready { text, qr, expires: Instant::now() + mobile::CODE_VALIDITY }
                        }
                        _ => Code::Failed,
                    };
                }
                cx.notify();
            });
            cx.background_executor().timer(mobile::CODE_VALIDITY).await;
            let _ = this.update(cx, |this, cx| {
                if let Some(d) = this.mobile_dialog()
                    && matches!(&d.code, Code::Ready { expires, .. } if *expires <= Instant::now())
                {
                    d.code = Code::Expired;
                    cx.notify();
                }
            });
        })
        .detach();
    }

    /// A device has just paired: if this window was showing the code, it shows the device.
    pub(crate) fn mobile_paired(&mut self, share: &str, device: &str, name: &str, cx: &mut Context<Self>) {
        if let Some(d) = self.mobile_dialog()
            && d.share.as_deref() == Some(share)
        {
            d.selected = Some(device.to_string());
            d.code = Code::None;
            self.set_status(t!("mobile.paired_status", name = name), false);
            cx.notify();
        }
    }

    /// Serves the saved version of the open file to its devices, if it is shared.
    pub(super) fn mobile_publish(&self, cx: &mut Context<Self>) {
        let Some(share) = self.mobile_share().filter(|s| mobile::share(s).enabled) else { return };
        let Some(doc) = &self.doc else { return };
        let ged = doc.ged.clone();
        let dir = doc.path.as_ref().and_then(|p| p.parent()).map(Path::to_path_buf);
        cx.background_spawn(async move {
            let find = |f: &str| export::find_file_in(dir.as_deref(), f);
            let packed = gedcom::gedzip::pack(&ged, &|f| find(f).is_some());
            let files = packed.files.into_iter().filter_map(|f| Some((f.name, find(&f.original)?))).collect();
            let people = ged.records.iter().filter(|r| r.tag == "INDI").count();
            mobile::publish(&share, packed.ged.to_gedcom_string(), files, people);
        })
        .detach();
    }

    /// The file `share` is closed or replaced in window `me`: it is no longer served, unless
    /// another window shows it.
    pub(super) fn mobile_unpublish(share: &str, me: EntityId, cx: &App) {
        let elsewhere = cx
            .global::<crate::updates::Windows>()
            .0
            .iter()
            .filter_map(|w| w.upgrade())
            .any(|w| w.entity_id() != me && w.read(cx).mobile_share().as_deref() == Some(share));
        if !elsewhere {
            mobile::unpublish(share);
        }
    }

    pub(super) fn mobile_body(&self, d: &MobileDialog, cx: &mut Context<Self>) -> AnyElement {
        let Some(share) = d.share.clone() else {
            return v_flex()
                .px_4()
                .py_3()
                .gap_3()
                .child(
                    Switch::new("mobile-enabled")
                        .checked(false)
                        .disabled(true)
                        .label(SharedString::from(t!("mobile.share"))),
                )
                .child(div().text_sm().text_color(theme::muted()).child(t!("mobile.save_first").into_owned()))
                .into_any_element();
        };
        let settings = mobile::share(&share);
        let enabled = settings.enabled;
        let toggle = Switch::new("mobile-enabled")
            .checked(enabled)
            .label(SharedString::from(t!("mobile.share")))
            .on_click(cx.listener(move |this, on: &bool, w, cx| {
                mobile::set_enabled(&share, *on);
                if *on {
                    this.mobile_publish(cx);
                    this.mobile_request_code(w, cx);
                } else if let Some(d) = this.mobile_dialog() {
                    if matches!(d.code, Code::Waiting | Code::Ready { .. })
                        && let Some(s) = &d.share
                    {
                        mobile::cancel_pairing(s);
                    }
                    d.code = Code::None;
                }
                cx.notify();
            }));
        let failure = match mobile::status() {
            Status::Failed(e) if enabled => Some(e),
            _ => d.share.as_deref().and_then(mobile::published).and_then(Result::err).filter(|_| enabled),
        };
        let (dot, state) = if !enabled {
            (None, t!("mobile.off").into_owned())
        } else {
            let version = d.share.as_deref().and_then(mobile::published).and_then(Result::ok);
            let text = match version {
                Some(v) => t!("mobile.on_version", when = ago(v.at)).into_owned(),
                None => t!("mobile.on").into_owned(),
            };
            (Some(theme::ok()), text)
        };
        let head = h_flex().px_4().py_3().gap_3().items_center().child(div().flex_1().child(toggle)).child(
            h_flex()
                .gap_2()
                .items_center()
                .text_xs()
                .text_color(theme::muted())
                .when_some(dot, |h, c| h.child(div().size_2().rounded_full().bg(c)))
                .child(state),
        );
        let show = enabled || !settings.devices.is_empty();
        v_flex()
            .child(head)
            .when_some(failure, |v, e| v.child(div().px_4().pb_3().text_sm().text_color(theme::danger()).child(e)))
            .when(show, |v| {
                v.child(
                    h_flex()
                        .items_stretch()
                        .h(px(340.))
                        .border_t_1()
                        .border_color(theme::border())
                        .child(self.mobile_list(d, &settings.devices, enabled, cx))
                        .child(self.mobile_detail(d, &settings.devices, enabled, cx)),
                )
            })
            .into_any_element()
    }

    fn mobile_list(&self, d: &MobileDialog, devices: &[Device], enabled: bool, cx: &mut Context<Self>) -> Div {
        let current = d.share.as_deref().and_then(mobile::published).and_then(Result::ok);
        let mut list = v_flex().w(px(184.)).flex_none().p_2().gap_0p5().border_r_1().border_color(theme::border());
        if !devices.is_empty() {
            list = list.child(label_caps(t!("mobile.paired").into_owned()).px_2().pt_2().pb_1());
        }
        for dev in devices {
            let on = d.selected.as_deref() == Some(dev.id.as_str());
            let up_to_date = up_to_date(dev, current.as_ref());
            let id = dev.id.clone();
            list = list.child(
                h_flex()
                    .id(SharedString::from(format!("mobile-device-{}", dev.id)))
                    .gap_2()
                    .items_center()
                    .px_2()
                    .py_1()
                    .rounded(theme::RADIUS_SM)
                    .text_sm()
                    .cursor_pointer()
                    .when(on, |h| h.bg(theme::accent_soft()))
                    .when(!on, |h| h.hover(|s| s.bg(theme::hover())))
                    .child(Icon::new(icon(dev.kind)).size(px(16.)).text_color(if on {
                        theme::accent()
                    } else {
                        theme::muted()
                    }))
                    .child(div().flex_1().min_w_0().truncate().child(dev.name.clone()))
                    .when(up_to_date, |h| h.child(div().size_1p5().rounded_full().bg(theme::ok())))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if let Some(d) = this.mobile_dialog() {
                            d.selected = Some(id.clone());
                        }
                        cx.notify();
                    })),
            );
        }
        if !devices.is_empty() {
            list = list.child(div().h_px().mx_1().my_2().bg(theme::border()));
        }
        let on = d.selected.is_none();
        list.child(
            h_flex()
                .id("mobile-add")
                .gap_2()
                .items_center()
                .px_2()
                .py_1()
                .rounded(theme::RADIUS_SM)
                .text_sm()
                .when(!enabled, |h| h.text_color(theme::muted()).opacity(0.6))
                .when(enabled, |h| {
                    h.text_color(theme::accent())
                        .cursor_pointer()
                        .when(on, |h| h.bg(theme::accent_soft()))
                        .when(!on, |h| h.hover(|s| s.bg(theme::hover())))
                        .on_click(cx.listener(|this, _, w, cx| {
                            if let Some(d) = this.mobile_dialog() {
                                d.selected = None;
                            }
                            this.mobile_request_code(w, cx);
                            cx.notify();
                        }))
                })
                .child(Icon::new(IconName::Plus).size(px(16.)))
                .child(t!("mobile.add").into_owned()),
        )
    }

    fn mobile_detail(&self, d: &MobileDialog, devices: &[Device], enabled: bool, cx: &mut Context<Self>) -> AnyElement {
        let device = d.selected.as_deref().and_then(|id| devices.iter().find(|x| x.id == id));
        let body = match device {
            Some(dev) => self.mobile_device(d, dev, cx),
            None if enabled => self.mobile_code(d, cx),
            None => div(),
        };
        div()
            .id("mobile-detail")
            .flex_1()
            .min_w_0()
            .bg(theme::bg())
            .rounded_br(theme::RADIUS_LG)
            .overflow_y_scroll()
            .p_4()
            .child(body)
            .into_any_element()
    }

    fn mobile_code(&self, d: &MobileDialog, cx: &mut Context<Self>) -> Div {
        let muted = |s: String| div().text_sm().text_color(theme::muted()).child(s);
        let frame = || {
            div()
                .size(px(QR_SIDE + 16.))
                .flex()
                .items_center()
                .justify_center()
                .rounded(theme::RADIUS_LG)
                .border_1()
                .border_color(theme::border())
        };
        let center = v_flex().size_full().items_center().justify_center().gap_4();
        match &d.code {
            Code::Ready { text, qr, .. } => {
                let text = text.clone();
                let copy = Button::new("mobile-copy")
                    .ghost()
                    .xsmall()
                    .icon(if d.copied { IconName::Check } else { IconName::Copy })
                    .label(t!("mobile.copy_code").into_owned())
                    .on_click(cx.listener(move |this, _, _, cx| {
                        cx.write_to_clipboard(ClipboardItem::new_string(text.clone()));
                        if let Some(d) = this.mobile_dialog() {
                            d.copied = true;
                        }
                        cx.notify();
                    }));
                center
                    // The code stays dark on white in both appearances: cameras read it best.
                    .child(frame().bg(rgb(0xffffff)).child(img(qr.clone()).size(px(QR_SIDE))))
                    .child(
                        v_flex()
                            .items_center()
                            .gap_1()
                            .child(div().font_weight(FontWeight::SEMIBOLD).child(t!("mobile.scan").into_owned()))
                            .child(
                                h_flex()
                                    .gap_2()
                                    .items_center()
                                    .child(muted(
                                        t!("mobile.valid", n = mobile::CODE_VALIDITY.as_secs() / 60).into_owned(),
                                    ))
                                    .child(copy),
                            ),
                    )
            }
            Code::Expired => center.child(muted(t!("mobile.expired").into_owned())).child(new_code(cx)),
            Code::Failed => center.child(muted(t!("mobile.code_failed").into_owned())).child(new_code(cx)),
            Code::Waiting | Code::None => center.child(frame()),
        }
    }

    fn mobile_device(&self, d: &MobileDialog, dev: &Device, cx: &mut Context<Self>) -> Div {
        let kind = match dev.kind {
            Kind::Phone => t!("mobile.phone"),
            Kind::Tablet => t!("mobile.tablet"),
        };
        let header = h_flex()
            .gap_3()
            .items_center()
            .child(
                div()
                    .size_9()
                    .flex_none()
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(theme::RADIUS_SM)
                    .border_1()
                    .border_color(theme::border())
                    .bg(theme::surface())
                    .child(Icon::new(icon(dev.kind)).size(px(20.)).text_color(theme::accent())),
            )
            .child(
                v_flex()
                    .min_w_0()
                    .child(div().font_weight(FontWeight::SEMIBOLD).truncate().child(dev.name.clone()))
                    .child(
                        div()
                            .text_xs()
                            .text_color(theme::muted())
                            .child(t!("mobile.kind_paired", kind = kind, date = day(dev.paired)).into_owned()),
                    ),
            );
        let row = |label: String, value: Div| {
            h_flex()
                .gap_3()
                .items_center()
                .min_h(px(24.))
                .text_sm()
                .child(div().w(px(128.)).flex_none().text_color(theme::muted()).child(label))
                .child(value.flex_1().min_w_0())
        };
        let current = d.share.as_deref().and_then(mobile::published).and_then(Result::ok);
        let up_to_date = up_to_date(dev, current.as_ref());
        let sync = match dev.last_sync {
            None => h_flex().child(t!("mobile.never").into_owned()),
            Some(at) if up_to_date => h_flex()
                .gap_1p5()
                .items_center()
                .child(Icon::new(IconName::Check).size(px(14.)).text_color(theme::ok()))
                .child(t!("mobile.up_to_date", when = ago(at)).into_owned()),
            Some(at) => h_flex().child(ago(at)),
        };
        let mut rows = v_flex().gap_1p5().child(row(t!("mobile.last_sync").into_owned(), sync));
        if dev.last_sync.is_some() {
            let link = if dev.direct { t!("mobile.direct") } else { t!("mobile.relay") };
            rows = rows.child(row(t!("mobile.connection").into_owned(), h_flex().child(link.into_owned())));
        }
        if let Some(v) = &dev.version {
            let content = format!("{} · {}", crate::tn!("mobile.people", v.people), size(v.bytes));
            rows = rows.child(row(t!("mobile.on_device").into_owned(), h_flex().child(content)));
        }
        let share = d.share.clone();
        let id = dev.id.clone();
        v_flex().size_full().gap_4().child(header).child(rows).child(div().flex_1()).child(h_flex().child(
            Button::new("mobile-remove").danger().small().label(t!("mobile.remove").into_owned()).on_click(
                cx.listener(move |this, _, w, cx| {
                    if let Some(share) = &share {
                        mobile::remove(share, &id);
                    }
                    let next = share.as_deref().map(mobile::share).unwrap_or_default().devices;
                    if let Some(d) = this.mobile_dialog() {
                        d.selected = next.first().map(|x| x.id.clone());
                    }
                    this.mobile_request_code(w, cx);
                    cx.notify();
                }),
            ),
        ))
    }
}

fn new_code(cx: &mut Context<Workspace>) -> Button {
    Button::new("mobile-new-code").small().label(t!("mobile.new_code").into_owned()).on_click(cx.listener(
        |this, _, w, cx| {
            if let Some(d) = this.mobile_dialog() {
                d.code = Code::None;
            }
            this.mobile_request_code(w, cx);
            cx.notify();
        },
    ))
}

/// Has `dev` got the version served now?
fn up_to_date(dev: &Device, current: Option<&mobile::Version>) -> bool {
    dev.version.as_ref().zip(current).is_some_and(|(a, b)| a.same(b))
}

fn icon(kind: Kind) -> IconName {
    match kind {
        Kind::Phone => IconName::Smartphone,
        Kind::Tablet => IconName::Tablet,
    }
}

/// QR code of `text`, dark on white, with its quiet zone.
fn qr_image(text: &str) -> Option<Arc<RenderImage>> {
    let code = qrcode::QrCode::new(text.as_bytes()).ok()?;
    let modules = code.width();
    let colors = code.to_colors();
    // Whole pixels per module, sharp at twice the displayed size.
    let quiet = 2;
    let scale = ((QR_SIDE as usize * 2) / (modules + 2 * quiet)).max(1) as u32;
    let side = (modules + 2 * quiet) as u32 * scale;
    let image = image::RgbaImage::from_fn(side, side, |x, y| {
        let (mx, my) = ((x / scale) as usize, (y / scale) as usize);
        let dark = (quiet..quiet + modules).contains(&mx)
            && (quiet..quiet + modules).contains(&my)
            && colors[(my - quiet) * modules + (mx - quiet)] == qrcode::Color::Dark;
        // BGRA, like the other images of the app.
        if dark { image::Rgba([0x14, 0x14, 0x14, 0xff]) } else { image::Rgba([0xff, 0xff, 0xff, 0xff]) }
    });
    Some(Arc::new(RenderImage::new(vec![image::Frame::new(image)])))
}

/// How long ago `at` was: “just now”, “5 min ago”, “3 h ago”, then the day.
fn ago(at: u64) -> String {
    let secs = mobile::now().saturating_sub(at);
    match secs / 60 {
        0 => t!("mobile.just_now").into_owned(),
        m @ 1..60 => crate::tn!("mobile.minutes_ago", m as usize).into_owned(),
        m if m < 24 * 60 => crate::tn!("mobile.hours_ago", (m / 60) as usize).into_owned(),
        _ => t!("mobile.on_day", date = day(at)).into_owned(),
    }
}

/// The day of `at`, in the interface language.
fn day(at: u64) -> String {
    use gedcom::date::calendar::{UNIX_EPOCH_RD, gregorian_from_fixed};
    const M: [&str; 12] = ["JAN", "FEB", "MAR", "APR", "MAY", "JUN", "JUL", "AUG", "SEP", "OCT", "NOV", "DEC"];
    let (y, m, d) = gregorian_from_fixed((at / 86_400) as i64 + UNIX_EPOCH_RD);
    gedcom::date::Date::parse(&format!("{d} {} {y}", M[(m as usize).clamp(1, 12) - 1])).display()
}

/// Size of a tree, in MB (or KB below 1 MB).
fn size(bytes: u64) -> String {
    if bytes < 1_000_000 {
        t!("mobile.kb", n = bytes.div_ceil(1000)).into_owned()
    } else {
        t!("mobile.mb", n = (bytes + 500_000) / 1_000_000).into_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::{QR_SIDE, day, qr_image, size};

    #[test]
    fn qr_code_is_square_and_sharp() {
        let qr = qr_image("gedroots:endpointabc.token").unwrap();
        let s = qr.size(0);
        assert_eq!(s.width, s.height);
        assert!(s.width.0 >= QR_SIDE as i32);
    }

    #[test]
    fn sizes_and_days() {
        assert_eq!(size(412_300_000), "412 MB");
        assert_eq!(size(12_345), "13 KB");
        assert!(day(1_793_750_400).contains("2026"));
    }
}
