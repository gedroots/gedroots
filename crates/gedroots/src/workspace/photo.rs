//! Photos : affichage recadré (`CROP`) et identification des personnes.
//!
//! - les vignettes et portraits n'affichent que la zone `CROP` du lien ;
//! - sur la fiche d'un média, les personnes identifiées sont encadrées sur
//!   l'image ; on en ajoute en traçant un cadre puis en choisissant la
//!   personne, ce qui crée un lien `INDI → OBJE` avec `CROP` (GEDCOM 7 standard).

use super::relatives::PickPurpose;
use super::*;
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::path::Path;

/// Taille maximale de l'image dans la fiche d'un média.
const MAX_W: f32 = 700.;
const MAX_H: f32 = 600.;

thread_local! {
    static SIZES: RefCell<HashMap<PathBuf, Option<(u32, u32)>>> = RefCell::new(HashMap::new());
}

/// Dimensions de l'image telle qu'affichée (orientation EXIF appliquée,
/// comme le fait GPUI), lues dans l'en-tête du fichier et mises en cache.
pub(super) fn image_size(path: &Path) -> Option<(u32, u32)> {
    if let Some(s) = SIZES.with(|c| c.borrow().get(path).copied()) {
        return s;
    }
    let read = || -> Option<(u32, u32)> {
        use image::ImageDecoder;
        let mut dec = image::ImageReader::open(path).ok()?.with_guessed_format().ok()?.into_decoder().ok()?;
        let (w, h) = dec.dimensions();
        let turned = matches!(
            dec.orientation().unwrap_or(image::metadata::Orientation::NoTransforms),
            image::metadata::Orientation::Rotate90
                | image::metadata::Orientation::Rotate270
                | image::metadata::Orientation::Rotate90FlipH
                | image::metadata::Orientation::Rotate270FlipH
        );
        Some(if turned { (h, w) } else { (w, h) })
    };
    let s = read();
    SIZES.with(|c| c.borrow_mut().insert(path.to_path_buf(), s));
    s
}

/// La zone `rect` de l'image, mise à l'échelle dans une boîte `w × h`.
/// `cover` : remplit la boîte (en rognant), sinon la zone y tient entière.
pub(super) fn cropped_img(path: PathBuf, size: (u32, u32), rect: Rect, w: f32, h: f32, cover: bool) -> Div {
    let (x, y, rw, rh) = (rect.0 as f32, rect.1 as f32, rect.2 as f32, rect.3 as f32);
    let s = if cover { (w / rw).max(h / rh) } else { (w / rw).min(h / rh) };
    // Centre la zone dans la boîte.
    let (ox, oy) = ((w - rw * s) / 2., (h - rh * s) / 2.);
    div().relative().overflow_hidden().w(px(w)).h(px(h)).flex_none().child(
        img(path)
            .absolute()
            .left(px(ox - x * s))
            .top(px(oy - y * s))
            .w(px(size.0 as f32 * s))
            .h(px(size.1 as f32 * s))
            .object_fit(ObjectFit::Fill),
    )
}

/// Image d'un lien, recadrée si le lien porte un `CROP` (boîte `w × h`, en remplissage).
pub(super) fn linked_img(path: PathBuf, crop: Option<Crop>, w: f32, h: f32, cover: bool) -> AnyElement {
    match crop.zip(image_size(&path)).and_then(|(c, size)| c.rect(size.0, size.1).map(|r| (r, size))) {
        Some((r, size)) if !Crop::from_rect(r).is_full(size.0, size.1) => {
            cropped_img(path, size, r, w, h, cover).into_any_element()
        }
        _ => img(path)
            .w(px(w))
            .h(px(h))
            .object_fit(if cover { ObjectFit::Cover } else { ObjectFit::Contain })
            .into_any_element(),
    }
}

/// Zone recadrée seule, à sa propre proportion, dans au plus `max_w × max_h`.
pub(super) fn fitted_crop(path: PathBuf, crop: Option<Crop>, max_w: f32, max_h: f32) -> AnyElement {
    let Some((size, r)) = image_size(&path).and_then(|s| crop?.rect(s.0, s.1).map(|r| (s, r))) else {
        return img(path).max_w_full().max_h(px(max_h)).object_fit(ObjectFit::Contain).into_any_element();
    };
    let (x, y, rw, rh) = (r.0 as f32, r.1 as f32, r.2 as f32, r.3 as f32);
    let s = (max_w / rw).min(max_h / rh).min(2.);
    // Dimensions relatives : la zone suit la largeur disponible en gardant ses proportions.
    div()
        .relative()
        .overflow_hidden()
        .w_full()
        .max_w(px(rw * s))
        .aspect_ratio(rw / rh)
        .child(
            img(path)
                .absolute()
                .left(relative(-x / rw))
                .top(relative(-y / rh))
                .w(relative(size.0 as f32 / rw))
                .h(relative(size.1 as f32 / rh))
                .object_fit(ObjectFit::Fill),
        )
        .into_any_element()
}

/// Tracé en cours d'un cadre sur une photo.
pub struct PhotoDraw {
    pub media: String,
    /// `Some(x)` : on (re)délimite le lien de `x` ; `None` : on identifie une nouvelle personne.
    pub owner: Option<String>,
    /// Coin de départ et position courante, en pixels de l'image.
    pub start: Option<(f32, f32)>,
    pub current: Option<(f32, f32)>,
    /// Taille de l'image.
    pub size: (u32, u32),
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

/// État partagé avec le rendu : position de l'image à l'écran.
#[derive(Default, Clone)]
pub struct PhotoBounds {
    /// Position de l'image à l'écran (au dernier rendu).
    bounds: Rc<Cell<Option<Bounds<Pixels>>>>,
    /// Largeur disponible pour l'image, mesurée au rendu précédent.
    avail: Rc<Cell<f32>>,
    /// Échelle d'affichage : pixels écran par pixel d'image.
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
            self.set_status("Impossible de lire les dimensions de l'image", true);
            cx.notify();
            return;
        };
        self.photo_draw = Some(PhotoDraw { media, owner, start: None, current: None, size });
        self.set_status("Tracez un cadre sur la photo (Échap pour annuler)", false);
        cx.notify();
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
            // Simple clic : on attend un vrai tracé.
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
            Some(owner) => self.mutate(window, cx, |g| {
                g.set_link_crop(&owner, &d.media, Some(crop))?;
                Ok((format!("Zone de {} enregistrée", g.record_label(&owner)), None))
            }),
            None => self.open_picker(PickPurpose::TagPerson { media: d.media, crop }, window, cx),
        }
    }

    fn remove_crop(&mut self, owner: String, media: String, window: &mut Window, cx: &mut Context<Self>) {
        self.mutate(window, cx, |g| {
            g.set_link_crop(&owner, &media, None)?;
            Ok((format!("Recadrage retiré pour {}", g.record_label(&owner)), None))
        });
    }

    fn untag(&mut self, indi: String, media: String, window: &mut Window, cx: &mut Context<Self>) {
        self.mutate(window, cx, |g| {
            g.untag_person(&indi, &media);
            Ok((format!("{} retiré(e) de la photo", g.record_label(&indi)), None))
        });
    }

    /// Photo d'un média avec les personnes identifiées, et la liste des liens recadrables.
    pub(super) fn render_photo(&self, media: &str, path: PathBuf, cx: &mut Context<Self>) -> AnyElement {
        let g = &self.doc.as_ref().unwrap().ged;
        let Some(size) = image_size(&path) else {
            return img(path).max_w_full().max_h(px(MAX_H)).object_fit(ObjectFit::Contain).into_any_element();
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
        let people = links.iter().filter(|l| l.owner_tag == "INDI" && l.crop.is_some()).count();
        let bounds = self.photo_bounds.bounds.clone();
        let avail_cell = self.photo_bounds.avail.clone();

        // Personnes identifiées, et zones recadrées pour les autres liens (extrait d'acte pour une source…).
        let boxes = links.iter().filter_map(|l| {
            let person = l.owner_tag == "INDI";
            let (x, y, w, h) = l.crop?.rect(size.0, size.1)?;
            let hovered = self.photo_hover.as_deref() == Some(l.owner.as_str());
            let editing = draw.is_some_and(|d| d.owner.as_deref() == Some(l.owner.as_str()));
            let color = match (hovered, person) {
                (true, _) => theme::accent(),
                (false, true) => theme::text(),
                (false, false) => theme::muted(),
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
                    .rounded_sm()
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
                            .rounded_sm()
                            .bg(gpui_kit::hsla(0., 0., 0., 0.7))
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
                .border_color(theme::accent())
                .bg(theme::accent().opacity(0.15))
        });

        let canvas_el = canvas(move |b, _, _| bounds.set(Some(b)), |_, _, _, _| {}).absolute().inset_0();
        let picture = div()
            .id("photo")
            .relative()
            .w(px(dw))
            .h(px(dh))
            .flex_none()
            .child(img(path.clone()).size_full().object_fit(ObjectFit::Fill))
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
            .when(people > 0, |d| {
                d.child(label_caps(format!("{people} personne{} identifiée{}", plural(people), plural(people))))
            })
            .child(div().flex_1())
            // Identifier une personne : menu Média (le tracé se fait ensuite sur la photo).
            .when(drawing, |d| {
                d.child(
                    Button::new("photo-cancel")
                        .small()
                        .ghost()
                        .icon(IconName::Close)
                        .label("Annuler le tracé")
                        .on_click(cx.listener(|this, _, _, cx| this.cancel_draw(cx))),
                )
            });

        let hint = draw.map(|d| {
            mono(match &d.owner {
                Some(o) => format!("Tracez la zone pour {} — Échap pour annuler", g.record_label(o)),
                None => "Tracez un cadre autour du visage, puis choisissez la personne — Échap pour annuler".into(),
            })
        });

        let rows = links.iter().map(|l| {
            let (owner, media_a, media_b, media_c) =
                (l.owner.clone(), media.to_string(), media.to_string(), media.to_string());
            let (o1, o2, o3) = (owner.clone(), owner.clone(), owner.clone());
            let path_c = path.clone();
            let is_person = l.owner_tag == "INDI";
            let kind = RecordKind::from_tag(&l.owner_tag);
            let hovered = self.photo_hover.as_deref() == Some(owner.as_str());
            let hover_key = owner.clone();
            h_flex()
                .id(SharedString::from(format!("link-{owner}")))
                .gap_2()
                .px_2()
                .py_1()
                .rounded_sm()
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
                        .id(SharedString::from(format!("link-name-{owner}")))
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
                .child(mono(match l.crop {
                    Some(c) => c.describe(),
                    None => "image entière".into(),
                }))
                .child(
                    Button::new(SharedString::from(format!("crop-{owner}")))
                        .xsmall()
                        .ghost()
                        .icon(IconName::Crop)
                        .label(if l.crop.is_some() { "Redessiner" } else { "Délimiter" })
                        .disabled(drawing)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.start_draw(media_a.clone(), Some(o2.clone()), &path_c, cx)
                        })),
                )
                .when(l.crop.is_some(), |d| {
                    d.child(
                        Button::new(SharedString::from(format!("uncrop-{owner}")))
                            .xsmall()
                            .ghost()
                            .label("Image entière")
                            .disabled(drawing)
                            .on_click(
                                cx.listener(move |this, _, w, cx| this.remove_crop(o3.clone(), media_b.clone(), w, cx)),
                            ),
                    )
                })
                .when(is_person, |d| {
                    let o4 = owner.clone();
                    d.child(
                        Button::new(SharedString::from(format!("untag-{owner}")))
                            .xsmall()
                            .ghost()
                            .icon(IconName::Close)
                            .disabled(drawing)
                            .on_click(
                                cx.listener(move |this, _, w, cx| this.untag(o4.clone(), media_c.clone(), w, cx)),
                            ),
                    )
                })
        });

        v_flex()
            .gap_2()
            .child(toolbar)
            .child(
                div()
                    .rounded_md()
                    .border_1()
                    .border_color(if drawing { theme::accent() } else { theme::border() })
                    .bg(theme::surface())
                    .p_2()
                    // Place sous l'image pour les noms des personnes encadrées en bas.
                    .pb_6()
                    .overflow_hidden()
                    .relative()
                    .child(picture)
                    // Mesure la largeur disponible : l'image s'y adapte au rendu suivant.
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
                d.child(label_caps(format!("Utilisée par · {}", links.len()))).child(v_flex().children(rows))
            })
            .into_any_element()
    }
}

fn display_scale((w, h): (u32, u32), max_w: f32) -> f32 {
    (max_w.min(MAX_W) / w.max(1) as f32).min(MAX_H / h.max(1) as f32).min(2.)
}

fn plural(n: usize) -> &'static str {
    if n > 1 { "s" } else { "" }
}
