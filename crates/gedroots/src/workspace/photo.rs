//! Photos: cropped display (`CROP`) and identification of people.
//!
//! - thumbnails and portraits only show the link’s `CROP` area;
//! - on a media’s record, identified people are framed on the image; more are added by drawing a
//!   frame then choosing the person, which creates an `INDI → OBJE` link with `CROP` (standard
//!   GEDCOM 7).

use super::relatives::PickPurpose;
use super::*;
pub(super) use crate::images::image_size;
use crate::images::{picture, picture_region};
use rust_i18n::t;
use std::cell::Cell;
use std::path::Path;

/// Maximum size of the image in a media’s record.
const MAX_W: f32 = 1600.;
const MAX_H: f32 = 900.;

/// Image of a link, cropped if the link has a `CROP` (box `w × h`). `cover`: fills the box
/// (cropping), otherwise the image or area fits whole. Only the area is decoded: the element is
/// laid out and rounded like a whole image.
pub(super) fn linked_img(path: PathBuf, crop: Option<Crop>, w: f32, h: f32, cover: bool) -> Img {
    let fit = if cover { ObjectFit::Cover } else { ObjectFit::Contain };
    let region = crop.zip(image_size(&path)).and_then(|(c, size)| c.rect(size.0, size.1).map(|r| (r, size)));
    match region {
        Some((r, size)) if !Crop::from_rect(r).is_full(size.0, size.1) => picture_region(path, r, w, h, fit),
        _ => picture(path, w, h, fit),
    }
    .w(px(w))
    .h(px(h))
}

/// Frame being drawn on a photo.
pub struct PhotoDraw {
    pub media: String,
    /// `Some(x)`: (re)defining the area of the link of `x`; `None`: identifying a new person.
    pub owner: Option<String>,
    /// Page of the media (from 1).
    pub page: usize,
    /// Starting corner and current position, in image pixels.
    pub start: Option<(f32, f32)>,
    pub current: Option<(f32, f32)>,
    /// Taille de l'image.
    pub size: (u32, u32),
    /// Drawing started from the record of `owner`: return there (in this tab) once the area is
    /// saved.
    pub back: Option<CenterTab>,
}

impl PhotoDraw {
    fn rect(&self) -> Option<Rect> {
        let ((ax, ay), (bx, by)) = (self.start?, self.current?);
        let (w, h) = (self.size.0 as f32, self.size.1 as f32);
        let clamp = |v: f32, m: f32| v.clamp(0., m);
        let (x0, x1) = (clamp(ax.min(bx), w), clamp(ax.max(bx), w));
        let (y0, y1) = (clamp(ay.min(by), h), clamp(ay.max(by), h));
        Some((x0.round() as u32, y0.round() as u32, (x1 - x0).round() as u32, (y1 - y0).round() as u32))
    }
}

/// State shared with rendering: position of the image on screen.
#[derive(Default, Clone)]
pub struct PhotoBounds {
    /// Position of the image on screen (at the last render).
    bounds: Rc<Cell<Option<Bounds<Pixels>>>>,
    /// Width available for the image, measured at the previous render.
    avail: Rc<Cell<f32>>,
    /// Display scale: screen pixels per image pixel.
    scale: Rc<Cell<f32>>,
}

impl Workspace {
    fn photo_point(&self, pos: Point<Pixels>) -> Option<(f32, f32)> {
        let b = self.photo_bounds.bounds.get()?;
        let scale = self.photo_bounds.scale.get();
        if scale <= 0. {
            return None;
        }
        let x = f32::from(pos.x - b.origin.x) / scale;
        let y = f32::from(pos.y - b.origin.y) / scale;
        Some((x, y))
    }

    pub(super) fn start_draw(&mut self, media: String, owner: Option<String>, path: &Path, cx: &mut Context<Self>) {
        let Some(size) = image_size(path) else {
            self.set_error("photo.no_size", t!("photo.no_size"));
            cx.notify();
            return;
        };
        let page = self.current_page(&media);
        self.photo_draw = Some(PhotoDraw { media, owner, page, start: None, current: None, size, back: None });
        self.set_status(t!("photo.draw_hint"), false);
        cx.notify();
    }

    /// Opens the image `media` to define the area of `owner` in it (`OBJE` link with `CROP`), then
    /// returns to the record of `owner`.
    pub(super) fn start_crop_for(&mut self, owner: String, media: String, window: &mut Window, cx: &mut Context<Self>) {
        let tab = self.tab;
        self.goto(media.clone(), window, cx);
        self.tab = CenterTab::Form;
        let Some(path) = self.photo_file(&media) else {
            self.set_error("photo.no_image", t!("photo.no_image"));
            cx.notify();
            return;
        };
        self.start_draw(media, Some(owner), &path, cx);
        if let Some(d) = self.photo_draw.as_mut() {
            d.back = Some(tab);
        }
    }

    pub(super) fn cancel_draw(&mut self, cx: &mut Context<Self>) {
        if self.photo_draw.take().is_some() {
            self.status = None;
            cx.notify();
        }
    }

    fn finish_draw(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(d) = &self.photo_draw else { return };
        if d.start.is_none() {
            return;
        }
        let rect = d.rect();
        let tiny = rect.is_none_or(|(_, _, w, h)| {
            let s = self.photo_bounds.scale.get();
            (w as f32) * s < 6. || (h as f32) * s < 6.
        });
        if tiny {
            // Single click: wait for a real drawing.
            if let Some(d) = self.photo_draw.as_mut() {
                d.start = None;
                d.current = None;
            }
            cx.notify();
            return;
        }
        let d = self.photo_draw.take().unwrap();
        let crop = Crop::from_rect(rect.unwrap());
        match d.owner {
            Some(owner) => {
                self.mutate(window, cx, |g| {
                    g.set_link_crop_on(&owner, &d.media, d.page, Some(crop))?;
                    let msg = t!("photo.area_saved", name = g.record_label(&owner)).to_string();
                    Ok((msg, d.back.map(|_| owner.clone())))
                });
                if let Some(tab) = d.back {
                    self.tab = tab;
                }
            }
            None => self.open_picker(PickPurpose::TagPerson { media: d.media, page: d.page, crop }, window, cx),
        }
    }

    fn remove_crop(&mut self, owner: String, media: String, page: usize, window: &mut Window, cx: &mut Context<Self>) {
        self.mutate(window, cx, |g| {
            g.set_link_crop_on(&owner, &media, page, None)?;
            Ok((t!("photo.crop_removed", name = g.record_label(&owner)).to_string(), None))
        });
    }

    fn untag(&mut self, indi: String, media: String, page: usize, window: &mut Window, cx: &mut Context<Self>) {
        self.mutate(window, cx, |g| {
            g.untag_person_on(&indi, &media, page);
            Ok((t!("photo.untagged", name = g.record_label(&indi)).to_string(), None))
        });
    }

    /// Photo of a media with the identified people, and the list of croppable links.
    pub(super) fn render_photo(&self, media: &str, path: PathBuf, cx: &mut Context<Self>) -> AnyElement {
        let g = &self.doc.as_ref().unwrap().ged;
        let Some(size) = image_size(&path) else {
            return picture(path, MAX_W, MAX_H, ObjectFit::Contain).max_w_full().max_h(px(MAX_H)).into_any_element();
        };
        let avail = match self.photo_bounds.avail.get() {
            a if a > 50. => a,
            _ => MAX_W,
        };
        let scale = display_scale(size, avail);
        self.photo_bounds.scale.set(scale);
        let (dw, dh) = (size.0 as f32 * scale, size.1 as f32 * scale);
        let draw = self.photo_draw.as_ref().filter(|d| d.media == media);
        let drawing = draw.is_some();
        let links = g.media_links(media);
        // Multi-page media: only the areas of the displayed page are framed.
        let page = self.current_page(media);
        let multipage = self.page_count(media) > 1;
        let here = |l: &MediaLink| l.crop.is_some() && l.page == page;
        let people = links.iter().filter(|l| l.owner_tag == "INDI" && here(l)).count();
        let bounds = self.photo_bounds.bounds.clone();
        let avail_cell = self.photo_bounds.avail.clone();

        // Identified people, and cropped areas for the other links (record excerpt for a source…).
        let boxes = links.iter().filter(|l| here(l)).filter_map(|l| {
            let person = l.owner_tag == "INDI";
            let (x, y, w, h) = l.crop?.rect(size.0, size.1)?;
            let hovered = self.photo_hover.as_deref() == Some(l.owner.as_str());
            let editing = draw.is_some_and(|d| d.owner.as_deref() == Some(l.owner.as_str()));
            let color = match (hovered, person) {
                (true, _) => theme::image_accent(),
                (false, true) => theme::image_text(),
                (false, false) => theme::image_muted(),
            };
            let target = l.owner.clone();
            let hover_key = l.owner.clone();
            Some(
                div()
                    .id(SharedString::from(format!("face-{}", l.owner)))
                    .absolute()
                    .left(px(x as f32 * scale))
                    .top(px(y as f32 * scale))
                    .w(px(w as f32 * scale))
                    .h(px(h as f32 * scale))
                    .border_2()
                    .when(!person, |d| d.border_dashed())
                    .rounded(theme::RADIUS_SM)
                    .border_color(color.opacity(if editing { 0.3 } else { 0.9 }))
                    .when(!drawing, |d| {
                        d.cursor_pointer()
                            .on_hover(cx.listener(move |this, h: &bool, _, cx| {
                                this.photo_hover = h.then(|| hover_key.clone());
                                cx.notify();
                            }))
                            .on_click(cx.listener(move |this, _, w, cx| this.goto(target.clone(), w, cx)))
                    })
                    .child(
                        div()
                            .absolute()
                            .left_0()
                            .top_full()
                            .mt_0p5()
                            .px_1()
                            .rounded(theme::RADIUS_SM)
                            .bg(theme::image_label_bg())
                            .text_xs()
                            .text_color(color)
                            .whitespace_nowrap()
                            .child(g.record_label(&l.owner)),
                    ),
            )
        });

        let draft = draw.and_then(|d| d.rect()).map(|(x, y, w, h)| {
            div()
                .absolute()
                .left(px(x as f32 * scale))
                .top(px(y as f32 * scale))
                .w(px(w as f32 * scale))
                .h(px(h as f32 * scale))
                .border_2()
                .border_color(theme::image_accent())
                .bg(theme::image_accent().opacity(0.15))
        });

        let canvas_el = canvas(move |b, _, _| bounds.set(Some(b)), |_, _, _, _| {}).absolute().inset_0();
        let picture = div()
            .id("photo")
            .relative()
            .w(px(dw))
            .h(px(dh))
            .flex_none()
            .child(picture(path.clone(), dw, dh, ObjectFit::Fill).size_full())
            .child(canvas_el)
            .children(boxes)
            .children(draft)
            .when(drawing, |d| {
                d.cursor(CursorStyle::Crosshair)
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, ev: &MouseDownEvent, _, cx| {
                            let p = this.photo_point(ev.position);
                            if let Some(d) = this.photo_draw.as_mut() {
                                d.start = p;
                                d.current = p;
                            }
                            cx.notify();
                        }),
                    )
                    .on_mouse_move(cx.listener(|this, ev: &MouseMoveEvent, _, cx| {
                        if !ev.dragging() {
                            return;
                        }
                        let p = this.photo_point(ev.position);
                        if let Some(d) = this.photo_draw.as_mut().filter(|d| d.start.is_some()) {
                            d.current = p;
                            cx.notify();
                        }
                    }))
                    .on_mouse_up(MouseButton::Left, cx.listener(|this, _, w, cx| this.finish_draw(w, cx)))
                    .on_mouse_up_out(MouseButton::Left, cx.listener(|this, _, w, cx| this.finish_draw(w, cx)))
            });

        let toolbar = h_flex()
            .gap_2()
            .items_center()
            .when(people > 0, |d| d.child(label_caps(crate::tn!("photo.people", people))))
            .child(div().flex_1())
            // Identify a person: Media menu (the drawing is then done on the photo).
            .when(drawing, |d| {
                d.child(
                    Button::new("photo-cancel")
                        .small()
                        .ghost()
                        .icon(IconName::Close)
                        .label(t!("photo.cancel_draw"))
                        .on_click(cx.listener(|this, _, _, cx| this.cancel_draw(cx))),
                )
            });

        let hint = draw.map(|d| {
            mono(match &d.owner {
                Some(o) => t!("photo.draw_for", name = g.record_label(o)).to_string(),
                None => t!("photo.draw_face").into(),
            })
        });

        let rows = links.iter().enumerate().map(|(n, l)| {
            let elsewhere = l.crop.is_some() && l.page != page;
            let link_page = l.page;
            let (owner, media_a, media_b, media_c) =
                (l.owner.clone(), media.to_string(), media.to_string(), media.to_string());
            let (o1, o2, o3) = (owner.clone(), owner.clone(), owner.clone());
            let path_c = path.clone();
            let is_person = l.owner_tag == "INDI";
            let kind = RecordKind::from_tag(&l.owner_tag);
            let hovered = self.photo_hover.as_deref() == Some(owner.as_str());
            let hover_key = owner.clone();
            h_flex()
                .id(SharedString::from(format!("link-{owner}-{n}")))
                .gap_2()
                .px_2()
                .py_1()
                .rounded(theme::RADIUS_SM)
                .items_center()
                .when(hovered, |d| d.bg(theme::hover()))
                .on_hover(cx.listener(move |this, h: &bool, _, cx| {
                    let k = h.then(|| hover_key.clone());
                    if this.photo_hover != k {
                        this.photo_hover = k;
                        cx.notify();
                    }
                }))
                .children(kind.map(|k| icon(View::for_kind(k).icon()).text_color(theme::muted())))
                .child(
                    div()
                        .id(SharedString::from(format!("link-name-{owner}-{n}")))
                        .flex_1()
                        .min_w_0()
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_ellipsis()
                        .cursor_pointer()
                        .hover(|d| d.text_color(theme::accent()))
                        .child(g.record_label(&owner))
                        .on_click(cx.listener(move |this, _, w, cx| this.goto(o1.clone(), w, cx))),
                )
                // Area on another page: a click leads there.
                .when(elsewhere, |d| {
                    d.child(
                        Button::new(SharedString::from(format!("goto-page-{owner}-{n}")))
                            .xsmall()
                            .ghost()
                            .label(t!("photo.page", n = link_page))
                            .on_click(cx.listener(move |this, _, _, cx| this.show_page(link_page, cx))),
                    )
                })
                .when(!elsewhere, |d| {
                    d.child(mono(match l.crop {
                        Some(c) => c.describe(),
                        None if multipage => t!("photo.whole_document").into(),
                        None => t!("photo.whole_image_lower").into(),
                    }))
                })
                .when(!elsewhere, |d| {
                    d.child(
                        Button::new(SharedString::from(format!("crop-{owner}-{n}")))
                            .xsmall()
                            .ghost()
                            .icon(IconName::Crop)
                            .label(if l.crop.is_some() { t!("photo.redraw") } else { t!("photo.delimit") })
                            .disabled(drawing)
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.start_draw(media_a.clone(), Some(o2.clone()), &path_c, cx)
                            })),
                    )
                })
                .when(l.crop.is_some() && !elsewhere, |d| {
                    d.child(
                        Button::new(SharedString::from(format!("uncrop-{owner}-{n}")))
                            .xsmall()
                            .ghost()
                            .label(t!("photo.whole_image"))
                            .disabled(drawing)
                            .on_click(cx.listener(move |this, _, w, cx| {
                                this.remove_crop(o3.clone(), media_b.clone(), link_page, w, cx)
                            })),
                    )
                })
                .when(is_person, |d| {
                    let o4 = owner.clone();
                    d.child(
                        Button::new(SharedString::from(format!("untag-{owner}-{n}")))
                            .xsmall()
                            .ghost()
                            .icon(IconName::Close)
                            .disabled(drawing)
                            .on_click(cx.listener(move |this, _, w, cx| {
                                this.untag(o4.clone(), media_c.clone(), link_page, w, cx)
                            })),
                    )
                })
        });

        v_flex()
            .gap_2()
            .child(toolbar)
            .child(
                div()
                    .rounded(theme::RADIUS_SM)
                    .border_1()
                    .border_color(if drawing { theme::accent() } else { theme::border() })
                    .bg(theme::surface())
                    .p_2()
                    // Space below the image for the names of people framed at the bottom.
                    .pb_6()
                    .overflow_hidden()
                    .relative()
                    .child(picture)
                    // Measures the available width: the image adapts to it at the next render.
                    .child(
                        canvas(
                            move |b, window, _| {
                                let w = f32::from(b.size.width) - 18.;
                                if (w - avail_cell.get()).abs() > 1. {
                                    avail_cell.set(w);
                                    window.refresh();
                                }
                            },
                            |_, _, _, _| {},
                        )
                        .absolute()
                        .inset_0(),
                    ),
            )
            .children(hint)
            .when(!links.is_empty(), |d| {
                d.child(label_caps(t!("photo.used_by", n = links.len()))).child(v_flex().children(rows))
            })
            .into_any_element()
    }
}

fn display_scale((w, h): (u32, u32), max_w: f32) -> f32 {
    (max_w.min(MAX_W) / w.max(1) as f32).min(MAX_H / h.max(1) as f32).min(2.)
}
