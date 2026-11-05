//! Large images (record scans, photos, proposals to review): zoom and pan, with a minimap.
//!
//! One component for every large image of the app, so that it behaves the same everywhere, like
//! Preview, Photos or an archive viewer:
//! - the image first fits the view;
//! - pinch, or ⌘ / Ctrl + scroll, zooms around the pointer; View › Zoom In, Zoom Out, Zoom to Fit
//!   and Actual Size around the center;
//! - double-click: from the fitted image to its actual size at the pointer (twice the fitted size
//!   for a small image), and back;
//! - once zoomed, dragging or scrolling moves the image, and a minimap in the bottom right corner
//!   (like the tree's) shows the visible part; a click or drag in it moves there.
//!
//! The state is shared ([`Zoom`] is a handle): the view holding the image keeps it, the menu
//! commands act on it, and the image's event handlers update it without going through the view.

use crate::images::{image_size, picture, picture_detail};
use crate::theme;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;

/// Largest scale: 4 screen pixels per image pixel (400 %), or the fitted scale if larger.
const MAX_SCALE: f32 = 4.;
/// One step of View › Zoom In / Zoom Out, and of a mouse wheel notch with ⌘ / Ctrl.
const STEP: f32 = 1.25;
/// Largest side of the minimap.
const MINI_W: f32 = 180.;
const MINI_H: f32 = 140.;
/// Below this, the view is too small for a minimap.
const MINI_MIN: f32 = 80.;
/// Movement beyond which a press becomes a drag (and not a click).
const DRAG_SLOP: f32 = 4.;
/// Size of the view before it is measured.
const DEFAULT_VIEW: (f32, f32) = (800., 600.);

/// Placement of the image in the view, at the last render.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Frame {
    /// Screen pixels per image pixel.
    pub scale: f32,
    /// Scale where the image fits the view.
    pub fit: f32,
    /// Position of the image's top left corner in the view.
    pub x: f32,
    pub y: f32,
    /// Size of the image on screen.
    pub w: f32,
    pub h: f32,
}

impl Frame {
    /// Is the image enlarged beyond the fitted size?
    pub fn zoomed(&self) -> bool {
        self.scale > self.fit * 1.001
    }
}

/// Parameters of the placement, given by the view holding the image.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Fit {
    /// Free space kept under the fitted image (names under the frames of a photo).
    bottom: f32,
    /// Largest scale of the fitted image: a small image is not blown up beyond it.
    max: f32,
}

#[derive(Default)]
struct State {
    path: Option<PathBuf>,
    natural: (f32, f32),
    /// `None`: the image fits the view; otherwise screen pixels per image pixel.
    scale: Option<f32>,
    /// Point of the image at the center of the view (image pixels).
    center: (f32, f32),
    /// The view on screen, measured at the last render.
    view: Option<Bounds<Pixels>>,
    fit: Option<Fit>,
    frame: Option<Frame>,
    /// Drag in progress: last pointer position, and whether it moved beyond the slop.
    drag: Option<(Point<Pixels>, Point<Pixels>)>,
    dragged: bool,
    mini_drag: bool,
    /// Minimap on screen and its scale (minimap pixels per image pixel).
    mini: Option<(Bounds<Pixels>, f32)>,
}

impl State {
    fn view_size(&self) -> (f32, f32) {
        self.view.map(|b| (f32::from(b.size.width), f32::from(b.size.height))).unwrap_or(DEFAULT_VIEW)
    }

    /// Placement for the current state; the center is kept within the image.
    fn place(&mut self) -> Frame {
        let (vw, vh) = self.view_size();
        let fit = self.fit.unwrap_or(Fit { bottom: 0., max: 2. });
        let (iw, ih) = (self.natural.0.max(1.), self.natural.1.max(1.));
        let avail_h = (vh - fit.bottom).max(1.);
        let fitted = (vw / iw).min(avail_h / ih).min(fit.max).max(0.0001);
        let scale = self.scale.unwrap_or(fitted).clamp(fitted, MAX_SCALE.max(fitted));
        if scale <= fitted * 1.001 {
            self.scale = None;
        }
        let (w, h) = (iw * scale, ih * scale);
        let axis = |view: f32, room: f32, size: f32, center: f32| {
            if size <= room { (room - size) / 2. } else { (view / 2. - center * scale).clamp(view - size, 0.) }
        };
        let x = axis(vw, vw, w, self.center.0);
        let y = if h <= avail_h { (avail_h - h) / 2. } else { axis(vh, vh, h, self.center.1) };
        self.center = ((vw / 2. - x) / scale, (vh / 2. - y) / scale);
        let f = Frame { scale, fit: fitted, x, y, w, h };
        self.frame = Some(f);
        f
    }

    /// Zooms by `factor`, keeping still the point of the view at `(ax, ay)`.
    fn zoom_at(&mut self, factor: f32, (ax, ay): (f32, f32)) {
        let f = self.place();
        let scale = (f.scale * factor).clamp(f.fit, MAX_SCALE.max(f.fit));
        self.set_scale_at(scale, (ax, ay));
    }

    fn set_scale_at(&mut self, scale: f32, (ax, ay): (f32, f32)) {
        let f = self.place();
        // Point of the image under the anchor, which must stay there.
        let (px_, py_) = ((ax - f.x) / f.scale, (ay - f.y) / f.scale);
        let (vw, vh) = self.view_size();
        self.scale = Some(scale);
        self.center = (px_ + (vw / 2. - ax) / scale, py_ + (vh / 2. - ay) / scale);
        self.place();
    }

    fn view_center(&self) -> (f32, f32) {
        let (vw, vh) = self.view_size();
        (vw / 2., vh / 2.)
    }

    /// Position of a window point in the view.
    fn local(&self, p: Point<Pixels>) -> (f32, f32) {
        let o = self.view.map(|b| b.origin).unwrap_or_default();
        (f32::from(p.x - o.x), f32::from(p.y - o.y))
    }

    fn pan_by(&mut self, dx: f32, dy: f32) {
        if let Some(f) = self.frame {
            self.center = (self.center.0 - dx / f.scale, self.center.1 - dy / f.scale);
            self.place();
        }
    }

    /// Point of the minimap → center of the view.
    fn mini_jump(&mut self, p: Point<Pixels>) {
        let Some((b, s)) = self.mini else { return };
        self.center = (f32::from(p.x - b.origin.x) / s, f32::from(p.y - b.origin.y) / s);
        self.place();
    }
}

/// Zoom and position of a large image. Cloning gives another handle on the same state.
#[derive(Clone, Default)]
pub struct Zoom(Rc<RefCell<State>>);

impl Zoom {
    /// The image shown, if any (the View menu commands then apply).
    pub fn is_shown(&self) -> bool {
        self.0.borrow().path.is_some()
    }

    /// No image shown any more: the View menu commands no longer apply.
    pub fn clear(&self) {
        let mut s = self.0.borrow_mut();
        s.path = None;
        s.scale = None;
        s.frame = None;
    }

    /// The last press moved the image: the click that follows is not one.
    pub fn dragged(&self) -> bool {
        self.0.borrow().dragged
    }

    pub fn zoom_in(&self) {
        let mut s = self.0.borrow_mut();
        let c = s.view_center();
        s.zoom_at(STEP, c);
    }

    pub fn zoom_out(&self) {
        let mut s = self.0.borrow_mut();
        let c = s.view_center();
        s.zoom_at(1. / STEP, c);
    }

    pub fn zoom_to_fit(&self) {
        self.0.borrow_mut().scale = None;
    }

    /// One image pixel per screen pixel, around the center.
    pub fn actual_size(&self) {
        let mut s = self.0.borrow_mut();
        let c = s.view_center();
        let f = s.place();
        s.set_scale_at(1f32.max(f.fit), c);
    }

    /// Image `path` in this zoom, to place in a parent that gives it its size.
    pub fn view(&self, id: impl Into<ElementId>, path: PathBuf) -> ImageView {
        ImageView {
            zoom: self.clone(),
            id: id.into(),
            path,
            bottom: 0.,
            max_fit: 2.,
            pan: true,
            overlay: Vec::new(),
            background: None,
        }
    }

    #[cfg(test)]
    fn with(natural: (f32, f32), view: (f32, f32)) -> Self {
        let z = Zoom::default();
        {
            let mut s = z.0.borrow_mut();
            s.path = Some(PathBuf::from("test.png"));
            s.natural = natural;
            s.view = Some(Bounds::new(point(px(0.), px(0.)), size(px(view.0), px(view.1))));
            s.center = (natural.0 / 2., natural.1 / 2.);
        }
        z
    }

    #[cfg(test)]
    fn frame(&self) -> Frame {
        self.0.borrow_mut().place()
    }
}

/// An image in a [`Zoom`], being built.
pub struct ImageView {
    zoom: Zoom,
    id: ElementId,
    path: PathBuf,
    bottom: f32,
    max_fit: f32,
    pan: bool,
    overlay: Vec<AnyElement>,
    background: Option<Hsla>,
}

impl ImageView {
    /// Free space kept under the fitted image.
    pub fn bottom_room(mut self, px_: f32) -> Self {
        self.bottom = px_;
        self
    }

    /// Largest scale of the fitted image (2 by default).
    pub fn max_fit(mut self, scale: f32) -> Self {
        self.max_fit = scale;
        self
    }

    /// Dragging moves the image (true by default): false while the view draws on the image.
    pub fn drag_to_pan(mut self, on: bool) -> Self {
        self.pan = on;
        self
    }

    /// Elements laid over the image, in a box of the image's size: placed with `relative()`
    /// positions and sizes, they follow the zoom.
    pub fn overlay(mut self, elements: impl IntoIterator<Item = AnyElement>) -> Self {
        self.overlay.extend(elements);
        self
    }

    /// Background around the image (`theme::viewer()` by default).
    pub fn background(mut self, color: Hsla) -> Self {
        self.background = Some(color);
        self
    }
}

impl IntoElement for ImageView {
    type Element = AnyElement;

    fn into_element(self) -> Self::Element {
        render(self)
    }
}

/// Window refresh after a change of zoom or position.
fn changed(window: &mut Window, cx: &mut App) {
    cx.stop_propagation();
    window.refresh();
}

fn render(v: ImageView) -> AnyElement {
    let ImageView { zoom, id, path, bottom, max_fit, pan, overlay, background } = v;
    let natural = image_size(&path).map(|(w, h)| (w as f32, h as f32)).unwrap_or((1000., 1400.));
    let frame = {
        let mut s = zoom.0.borrow_mut();
        if s.path.as_deref() != Some(path.as_path()) {
            // Another image: it fits the view again.
            s.path = Some(path.clone());
            s.scale = None;
            s.center = (natural.0 / 2., natural.1 / 2.);
        }
        s.natural = natural;
        s.fit = Some(Fit { bottom, max: max_fit });
        s.place()
    };
    let zoomed = frame.zoomed();
    let dragging = zoom.0.borrow().drag.is_some() && zoom.dragged();

    // Measures the view: the image is placed again when its size changes.
    let measured = zoom.clone();
    let measure = canvas(
        move |b, window, _| {
            let mut s = measured.0.borrow_mut();
            let resized = s.view.map(|v| v.size) != Some(b.size);
            s.view = Some(b);
            if resized {
                // A refresh asked while painting would be lost: the next frame places it again.
                window.request_animation_frame();
            }
        },
        |_, _, _, _| {},
    )
    .absolute()
    .inset_0();

    // The image decoded for its fitted size, then, once zoomed, in detail above it: while the
    // detail loads, the fitted image stays visible, enlarged.
    let base =
        picture(path.clone(), frame.w.min(natural.0 * frame.fit), frame.h.min(natural.1 * frame.fit), ObjectFit::Fill);
    let base = picture_base(base);
    let detail = zoomed.then(|| picture_detail(path.clone()).absolute().size_full());
    let image = div()
        .absolute()
        .left(px(frame.x))
        .top(px(frame.y))
        .w(px(frame.w))
        .h(px(frame.h))
        .child(base)
        .children(detail)
        .children(overlay);

    let mini = zoomed.then(|| minimap(&zoom, &path, natural, frame)).flatten();

    let (z_down, z_move, z_up, z_up_out, z_scroll, z_pinch) =
        (zoom.clone(), zoom.clone(), zoom.clone(), zoom.clone(), zoom.clone(), zoom.clone());
    div()
        .id(id)
        .relative()
        .size_full()
        .overflow_hidden()
        .bg(background.unwrap_or_else(theme::viewer))
        .when(zoomed && pan, |d| d.cursor(if dragging { CursorStyle::ClosedHand } else { CursorStyle::OpenHand }))
        .child(measure)
        .child(image)
        .children(mini)
        .on_mouse_down(MouseButton::Left, move |ev: &MouseDownEvent, window, cx| {
            let mut s = z_down.0.borrow_mut();
            s.dragged = false;
            let at = s.local(ev.position);
            if ev.click_count >= 2 {
                // Double-click: actual size at the pointer, or back to the fitted image.
                let f = s.place();
                if f.zoomed() {
                    s.scale = None;
                } else {
                    let target = if f.fit < 1. { 1. } else { (f.fit * 2.).min(MAX_SCALE.max(f.fit)) };
                    s.set_scale_at(target, at);
                }
                s.drag = None;
                drop(s);
                changed(window, cx);
                return;
            }
            if pan && s.frame.is_some_and(|f| f.zoomed()) {
                s.drag = Some((ev.position, ev.position));
            }
        })
        .on_mouse_move(move |ev: &MouseMoveEvent, window, cx| {
            let mut s = z_move.0.borrow_mut();
            if ev.pressed_button != Some(MouseButton::Left) {
                s.drag = None;
                s.mini_drag = false;
                return;
            }
            if s.mini_drag {
                s.mini_jump(ev.position);
                drop(s);
                changed(window, cx);
            } else if let Some((start, last)) = s.drag {
                let moved = ev.position - start;
                if f32::from(moved.x).abs() + f32::from(moved.y).abs() > DRAG_SLOP {
                    s.dragged = true;
                }
                if s.dragged {
                    let d = ev.position - last;
                    s.pan_by(f32::from(d.x), f32::from(d.y));
                    s.drag = Some((start, ev.position));
                    drop(s);
                    changed(window, cx);
                }
            }
        })
        .on_mouse_up(MouseButton::Left, move |_, window, _| {
            let mut s = z_up.0.borrow_mut();
            let was = s.drag.is_some() || s.mini_drag;
            s.drag = None;
            s.mini_drag = false;
            if was {
                window.refresh();
            }
        })
        .on_mouse_up_out(MouseButton::Left, move |_, _, _| {
            let mut s = z_up_out.0.borrow_mut();
            s.drag = None;
            s.mini_drag = false;
        })
        .on_scroll(move |ev: &ScrollEvent, window, cx| {
            let mut s = z_scroll.0.borrow_mut();
            let d = ev.delta.pixel_delta(px(40.));
            let (mut dx, mut dy) = (f32::from(d.x), f32::from(d.y));
            // ⌘ (macOS) or Ctrl + scroll: zoom, like Preview, Photos and browsers.
            if ev.modifiers.platform || ev.modifiers.control {
                let factor = if ev.delta.precise() { (dy * 0.01).exp() } else { STEP.powf(dy.signum()) };
                let at = s.local(ev.position);
                s.zoom_at(factor, at);
                drop(s);
                changed(window, cx);
                return;
            }
            // Fitted image: the scroll goes on to the page around it.
            if !s.frame.is_some_and(|f| f.zoomed()) {
                return;
            }
            // Classic wheel with Shift: horizontal scrolling.
            if !ev.delta.precise() && ev.modifiers.shift && dx == 0. {
                (dx, dy) = (dy, 0.);
            }
            s.pan_by(dx, dy);
            drop(s);
            changed(window, cx);
        })
        .on_pinch(move |ev: &PinchEvent, window, cx| {
            let mut s = z_pinch.0.borrow_mut();
            let at = s.local(ev.position);
            s.zoom_at(1. + ev.delta, at);
            drop(s);
            changed(window, cx);
        })
        .into_any_element()
}

/// The fitted image fills the frame of the image, whatever its zoom.
fn picture_base(i: Img) -> Img {
    i.absolute().size_full()
}

/// Minimap in the bottom right corner: the whole image, the visible part outlined, the rest
/// veiled, and the zoom level.
fn minimap(zoom: &Zoom, path: &Path, natural: (f32, f32), f: Frame) -> Option<AnyElement> {
    let (vw, vh) = zoom.0.borrow().view_size();
    // Smaller in a narrow panel (act editor), never more than a fraction of the view.
    let (max_w, max_h) = (MINI_W.min(vw * 0.4), MINI_H.min(vh * 0.3));
    if max_w < MINI_MIN || max_h < MINI_MIN {
        return None;
    }
    let s = (max_w / natural.0).min(max_h / natural.1);
    let (mw, mh) = (natural.0 * s, natural.1 * s);
    // Visible part, in minimap pixels.
    let x0 = ((-f.x) / f.scale * s).max(0.);
    let y0 = ((-f.y) / f.scale * s).max(0.);
    let x1 = ((vw - f.x) / f.scale * s).min(mw);
    let y1 = ((vh - f.y) / f.scale * s).min(mh);
    let cell = zoom.clone();
    let marks = canvas(
        move |b, _, _| cell.0.borrow_mut().mini = Some((b, s)),
        move |b, _, window, _| {
            let o = b.origin;
            let rect = |x: f32, y: f32, w: f32, h: f32| Bounds::new(o + point(px(x), px(y)), size(px(w), px(h)));
            let veil = theme::scrim();
            for r in [
                rect(0., 0., mw, y0),
                rect(0., y1, mw, mh - y1),
                rect(0., y0, x0, y1 - y0),
                rect(x1, y0, mw - x1, y1 - y0),
            ] {
                if f32::from(r.size.width) > 0. && f32::from(r.size.height) > 0. {
                    window.paint_quad(fill(r, veil));
                }
            }
            window.paint_quad(outline(rect(x0, y0, x1 - x0, y1 - y0), theme::accent(), BorderStyle::Solid));
        },
    )
    .absolute()
    .inset_0();
    let jump = zoom.clone();
    Some(
        div()
            .id("image-minimap")
            .absolute()
            .right(px(12.))
            .bottom(px(12.))
            .w(px(mw + 2.))
            .h(px(mh + 2.))
            .rounded(theme::RADIUS_SM)
            .border_1()
            .border_color(theme::border())
            .bg(theme::panel())
            .overflow_hidden()
            .cursor_pointer()
            .child(
                div()
                    .relative()
                    .w(px(mw))
                    .h(px(mh))
                    .child(picture(path.to_path_buf(), mw, mh, ObjectFit::Fill).size_full())
                    .child(marks),
            )
            .child(
                div()
                    .absolute()
                    .left(px(4.))
                    .top(px(4.))
                    .px_1()
                    .rounded(theme::RADIUS_SM)
                    .bg(theme::image_label_bg())
                    .text_xs()
                    .text_color(theme::image_text())
                    .child(crate::t!("stats.percent", value = format!("{:.0}", f.scale * 100.)).into_owned()),
            )
            .on_mouse_down(MouseButton::Left, move |ev: &MouseDownEvent, window, cx| {
                let mut s = jump.0.borrow_mut();
                s.mini_drag = true;
                s.drag = None;
                s.mini_jump(ev.position);
                drop(s);
                changed(window, cx);
            })
            .into_any_element(),
    )
}

#[cfg(test)]
mod tests {
    use super::Zoom;

    #[test]
    fn fits_then_zooms_around_a_point() {
        // A 4000 × 3000 scan in a 400 × 300 view: fitted at 10 %.
        let z = Zoom::with((4000., 3000.), (400., 300.));
        let f = z.frame();
        assert_eq!((f.scale, f.x, f.y), (0.1, 0., 0.));
        assert!(!f.zoomed());
        // Zoom around the top left corner: it stays in place.
        z.0.borrow_mut().zoom_at(2., (0., 0.));
        let f = z.frame();
        assert!((f.scale - 0.2).abs() < 1e-6 && f.x == 0. && f.y == 0.);
        assert!(f.zoomed());
        // Actual size around the center: the center of the image stays at the center.
        z.zoom_to_fit();
        z.actual_size();
        let f = z.frame();
        assert_eq!(f.scale, 1.);
        assert_eq!((f.x, f.y), (200. - 2000., 150. - 1500.));
    }

    #[test]
    fn stays_within_the_image_and_limits() {
        let z = Zoom::with((4000., 3000.), (400., 300.));
        z.actual_size();
        // Panning far away stops at the edge of the image.
        z.0.borrow_mut().pan_by(100_000., 100_000.);
        let f = z.frame();
        assert_eq!((f.x, f.y), (0., 0.));
        // Zooming out never goes below the fitted image, nor in beyond 400 %.
        for _ in 0..40 {
            z.zoom_out();
        }
        assert!(!z.frame().zoomed());
        for _ in 0..40 {
            z.zoom_in();
        }
        assert_eq!(z.frame().scale, 4.);
    }

    #[test]
    fn small_image_is_centered() {
        // 100 × 100 photo in a 400 × 300 view: enlarged twice at most, centered.
        let z = Zoom::with((100., 100.), (400., 300.));
        let f = z.frame();
        assert_eq!((f.scale, f.x, f.y), (2., 100., 50.));
    }
}
