//! Place map: OpenStreetMap tiles cached on disk, a marker per place with coordinates (`PLAC.MAP`)
//! and placing by hand with a click. The same view, in “picker” mode, is used to set the pin of a
//! new place.
//!
//! Offline, the map uses the tiles already cached and shows the markers on a grid: the coordinates
//! come from the GEDCOM file, not from the Internet.

use crate::images::picture;
use crate::theme;
use gedcom::Place;
use gedcom::geo::{TILE_SIZE, project, unproject};
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::menu::{ContextMenuExt, PopupMenu, PopupMenuItem};
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::component::{Sizable, h_flex};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use std::collections::HashSet;
use std::path::PathBuf;
use std::time::Duration;

const MIN_ZOOM: u8 = 2;
const MAX_ZOOM: u8 = 18;
const MAX_INFLIGHT: usize = 6;
const DEFAULT_TILES: &str = "https://tile.openstreetmap.org/{z}/{x}/{y}.png";

/// What the map asks of the workspace.
pub enum MapEvent {
    /// A marker was clicked.
    Select(String),
    /// Coordinates set by hand, to write into the GEDCOM (in picker mode: new position of the pin,
    /// `name` empty). `checkpoint`: create an undo point (false for the rest of the same series).
    SetCoords { name: String, lat: f64, lon: f64, checkpoint: bool },
}

struct Marker {
    name: String,
    label: SharedString,
    /// “World” position at zoom 0.
    world: (f64, f64),
    uses: usize,
}

type TileKey = (u8, u32, u32);

pub struct MapView {
    markers: Vec<Marker>,
    /// Number of places without coordinates.
    selected: Option<String>,
    /// Center of the view, in “world” coordinates at zoom 0.
    center: (f64, f64),
    zoom: u8,
    bounds: Bounds<Pixels>,
    needs_fit: bool,
    drag: Option<Point<Pixels>>,
    scroll_acc: f32,
    online: bool,
    placing: bool,
    /// Position of the last right-click on the map (“Place Here” menu). (Shared without going
    /// through the view: updating it during the click would stop the menu from receiving the
    /// mouse.)
    menu_pos: std::rc::Rc<std::cell::Cell<Option<Point<Pixels>>>>,
    tile_dir: Option<PathBuf>,
    tile_url: String,
    pending: HashSet<TileKey>,
    failed: HashSet<TileKey>,
    agent: ureq::Agent,
    /// Position picker (“New Place” window): a single pin, each click moves it.
    picker: bool,
    /// Picker: position where the button was pressed, to tell a click from a drag.
    press: Option<Point<Pixels>>,
}

impl EventEmitter<MapEvent> for MapView {}

impl MapView {
    pub fn new() -> Self {
        let agent = ureq::AgentBuilder::new()
            .user_agent(crate::gazetteer::USER_AGENT)
            .timeout(Duration::from_secs(15))
            .try_proxy_from_env(true)
            .build();
        Self {
            markers: Vec::new(),
            selected: None,
            center: project(46.6, 2.4),
            zoom: 5,
            bounds: Bounds::default(),
            needs_fit: true,
            drag: None,
            scroll_acc: 0.,
            online: std::env::var_os("GEDROOTS_OFFLINE").is_none(),
            placing: false,
            menu_pos: Default::default(),
            tile_dir: tile_cache_dir(),
            tile_url: std::env::var("GEDROOTS_TILE_URL").unwrap_or_else(|_| DEFAULT_TILES.into()),
            pending: HashSet::new(),
            failed: HashSet::new(),
            agent,
            picker: false,
            press: None,
        }
    }

    /// Small map to choose a place's position: pin at `pin`, view centered on `pin` or, failing
    /// that, on `near` (parent place); otherwise France.
    pub fn picker(pin: Option<(f64, f64)>, near: Option<(f64, f64)>) -> Self {
        let mut m = Self::new();
        m.picker = true;
        m.needs_fit = false;
        m.selected = Some(String::new());
        if let Some(c) = pin.or(near) {
            m.center = project(c.0, c.1);
            m.zoom = 12;
        }
        if let Some((lat, lon)) = pin {
            m.set_pin(lat, lon);
        }
        m
    }

    fn set_pin(&mut self, lat: f64, lon: f64) {
        self.markers = vec![Marker { name: String::new(), label: "".into(), world: project(lat, lon), uses: 1 }];
    }

    /// “Place on the Map”: the next click on the map places this place.
    pub fn start_placing(&mut self, name: String, cx: &mut Context<Self>) {
        self.selected = Some(name);
        self.placing = true;
        cx.notify();
    }

    /// Updates the list of places (after each change to the document).
    pub fn set_places(&mut self, places: &[Place], cx: &mut Context<Self>) {
        let had_markers = !self.markers.is_empty();
        self.markers.clear();
        for p in places {
            if let Some((lat, lon)) = p.coords() {
                self.markers.push(Marker {
                    name: p.name.clone(),
                    label: format!("{} · {}", p.short(), crate::tn!("places.uses", p.uses)).into(),
                    world: project(lat, lon),
                    uses: p.uses,
                });
            }
        }
        // The most used drawn last, so on top.
        self.markers.sort_by_key(|m| m.uses);
        if !had_markers && !self.markers.is_empty() {
            self.needs_fit = true;
        }
        cx.notify();
    }

    /// Place selected in the list: show it if it is out of view.
    pub fn set_selected(&mut self, name: Option<String>, cx: &mut Context<Self>) {
        if self.selected == name {
            return;
        }
        self.selected = name;
        self.placing = false;
        if let Some(m) = self.selected.as_ref().and_then(|n| self.markers.iter().find(|m| &m.name == n)) {
            let (x, y) = self.to_screen(m.world);
            let (w, h) = (f32::from(self.bounds.size.width), f32::from(self.bounds.size.height));
            if !self.needs_fit && (x < 40. || y < 40. || x > w - 40. || y > h - 40.) {
                self.center = m.world;
                self.zoom = self.zoom.max(9);
            }
        }
        cx.notify();
    }

    fn scale(&self) -> f64 {
        f64::from(1u32 << self.zoom)
    }

    /// Position in the view (pixels from the top left corner) of a “world” point.
    fn to_screen(&self, world: (f64, f64)) -> (f32, f32) {
        let s = self.scale();
        let w = f64::from(f32::from(self.bounds.size.width));
        let h = f64::from(f32::from(self.bounds.size.height));
        (((world.0 - self.center.0) * s + w / 2.) as f32, ((world.1 - self.center.1) * s + h / 2.) as f32)
    }

    /// “World” point under a window position.
    fn to_world(&self, pos: Point<Pixels>) -> (f64, f64) {
        let s = self.scale();
        let x = f64::from(f32::from(pos.x - self.bounds.origin.x) - f32::from(self.bounds.size.width) / 2.);
        let y = f64::from(f32::from(pos.y - self.bounds.origin.y) - f32::from(self.bounds.size.height) / 2.);
        (self.center.0 + x / s, self.center.1 + y / s)
    }

    fn clamp_center(&mut self) {
        self.center.0 = self.center.0.rem_euclid(TILE_SIZE);
        self.center.1 = self.center.1.clamp(0., TILE_SIZE);
    }

    pub fn zoom_by(&mut self, delta: i32, around: Option<Point<Pixels>>, cx: &mut Context<Self>) {
        let z = (i32::from(self.zoom) + delta).clamp(MIN_ZOOM.into(), MAX_ZOOM.into()) as u8;
        if z == self.zoom {
            return;
        }
        let anchor = around.map(|p| (p, self.to_world(p)));
        self.zoom = z;
        if let Some((p, world)) = anchor {
            // The point under the cursor stays under the cursor.
            let after = self.to_world(p);
            self.center.0 += world.0 - after.0;
            self.center.1 += world.1 - after.1;
        }
        self.clamp_center();
        cx.notify();
    }

    /// Fits the view to every marker.
    pub fn fit(&mut self) {
        let (w, h) = (f64::from(f32::from(self.bounds.size.width)), f64::from(f32::from(self.bounds.size.height)));
        if self.markers.is_empty() || w < 1. || h < 1. {
            return;
        }
        let (mut x0, mut y0, mut x1, mut y1) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
        for m in &self.markers {
            x0 = x0.min(m.world.0);
            y0 = y0.min(m.world.1);
            x1 = x1.max(m.world.0);
            y1 = y1.max(m.world.1);
        }
        self.center = ((x0 + x1) / 2., (y0 + y1) / 2.);
        let mut z = MIN_ZOOM;
        while z < 11 {
            let s = f64::from(1u32 << (z + 1));
            if (x1 - x0) * s > w * 0.85 || (y1 - y0) * s > h * 0.85 {
                break;
            }
            z += 1;
        }
        self.zoom = z;
        self.needs_fit = false;
    }

    // ------------------------------------------------------------------
    // Tuiles
    // ------------------------------------------------------------------

    fn tile_path(&self, (z, x, y): TileKey) -> Option<PathBuf> {
        Some(self.tile_dir.as_ref()?.join(format!("{z}/{x}/{y}.png")))
    }

    fn request_tile(&mut self, key: TileKey, cx: &mut Context<Self>) {
        if !self.online
            || self.pending.len() >= MAX_INFLIGHT
            || self.pending.contains(&key)
            || self.failed.contains(&key)
        {
            return;
        }
        let Some(path) = self.tile_path(key) else { return };
        let (z, x, y) = key;
        let url =
            self.tile_url.replace("{z}", &z.to_string()).replace("{x}", &x.to_string()).replace("{y}", &y.to_string());
        let agent = self.agent.clone();
        self.pending.insert(key);
        let task = cx.background_spawn(async move { download(&agent, &url, &path) });
        cx.spawn(async move |this, cx| {
            let ok = task.await.is_ok();
            let _ = this.update(cx, |this, cx| {
                this.pending.remove(&key);
                if !ok {
                    this.failed.insert(key);
                }
                cx.notify();
            });
        })
        .detach();
    }

    // ------------------------------------------------------------------
    // Rendu
    // ------------------------------------------------------------------

    fn render_tiles(&mut self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let (w, h) = (f64::from(f32::from(self.bounds.size.width)), f64::from(f32::from(self.bounds.size.height)));
        let s = self.scale();
        let n = 1u32 << self.zoom;
        let left = self.center.0 * s - w / 2.;
        let top = self.center.1 * s - h / 2.;
        let (tx0, tx1) = ((left / TILE_SIZE).floor() as i64, ((left + w) / TILE_SIZE).floor() as i64);
        let (ty0, ty1) =
            (((top / TILE_SIZE).floor() as i64).max(0), ((top + h) / TILE_SIZE).floor().min(f64::from(n - 1)) as i64);
        let mut out = Vec::new();
        // From the center to the edges: the tiles in the middle load first.
        let mut keys: Vec<(i64, i64)> = (ty0..=ty1).flat_map(|ty| (tx0..=tx1).map(move |tx| (tx, ty))).collect();
        let (cx0, cy0) = ((tx0 + tx1) as f64 / 2., (ty0 + ty1) as f64 / 2.);
        keys.sort_by(|a, b| {
            let d = |(x, y): &(i64, i64)| (*x as f64 - cx0).powi(2) + (*y as f64 - cy0).powi(2);
            d(a).total_cmp(&d(b))
        });
        for (tx, ty) in keys {
            let key = (self.zoom, tx.rem_euclid(n.into()) as u32, ty as u32);
            let x = (tx as f64 * TILE_SIZE - left).round() as f32;
            let y = (ty as f64 * TILE_SIZE - top).round() as f32;
            let cell = div().absolute().left(px(x)).top(px(y)).size(px(TILE_SIZE as f32));
            match self.tile_path(key).filter(|p| p.exists()) {
                Some(p) => out.push(
                    cell.child(picture(p, TILE_SIZE as f32, TILE_SIZE as f32, ObjectFit::Fill).size_full())
                        .into_any_element(),
                ),
                None => {
                    self.request_tile(key, cx);
                    out.push(cell.border_r_1().border_b_1().border_color(theme::border()).into_any_element());
                }
            }
        }
        out
    }

    fn render_markers(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let (w, h) = (f32::from(self.bounds.size.width), f32::from(self.bounds.size.height));
        let mut out = Vec::new();
        let mut selected = None;
        for (i, m) in self.markers.iter().enumerate() {
            let (x, y) = self.to_screen(m.world);
            if x < -20. || y < -20. || x > w + 20. || y > h + 20. {
                continue;
            }
            if self.selected.as_deref() == Some(m.name.as_str()) {
                selected = Some((m, x, y));
                continue;
            }
            let r = (6. + (m.uses as f32).ln() * 1.6).min(12.);
            let name = m.name.clone();
            let label = m.label.clone();
            out.push(
                div()
                    .id(("marker", i))
                    .absolute()
                    .left(px(x - r))
                    .top(px(y - r))
                    .size(px(2. * r))
                    .rounded_full()
                    .bg(theme::accent().opacity(0.85))
                    .border_2()
                    .border_color(theme::text().opacity(0.9))
                    .cursor_pointer()
                    .hover(|d| d.bg(theme::accent()))
                    .tooltip(move |w, cx| Tooltip::new(label.clone()).build(w, cx))
                    .on_mouse_down(MouseButton::Left, {
                        let name = name.clone();
                        cx.listener(move |_, _, _, cx| {
                            cx.stop_propagation();
                            cx.emit(MapEvent::Select(name.clone()));
                        })
                    })
                    .context_menu(marker_menu(cx.entity().downgrade(), name, point(px(x), px(y))))
                    .into_any_element(),
            );
        }
        // The selected place, above the others and with its name.
        if let Some((m, x, y)) = selected {
            out.push(
                div()
                    .absolute()
                    .left(px(x - 9.))
                    .top(px(y - 9.))
                    .size(px(18.))
                    .rounded_full()
                    .bg(if self.picker { theme::accent() } else { theme::warn() })
                    .border_2()
                    .border_color(theme::text())
                    .into_any_element(),
            );
            if m.label.is_empty() {
                return out;
            }
            out.push(
                div()
                    .absolute()
                    .left(px(x + 14.))
                    .top(px(y - 12.))
                    .px_2()
                    .py_0p5()
                    .rounded(theme::RADIUS_SM)
                    .bg(theme::panel())
                    .border_1()
                    .border_color(theme::border_strong())
                    .text_xs()
                    .whitespace_nowrap()
                    .child(m.label.clone())
                    .into_any_element(),
            );
        }
        out
    }

    /// Zoom buttons of the place picker. The main map has none: View › Zoom, the scroll wheel and
    /// the context menu do it.
    fn render_toolbar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        h_flex()
            .absolute()
            .top_2()
            .left_2()
            .gap_1()
            .items_center()
            // Clicks on the bar do not move the map.
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(
                pill()
                    .child(
                        Button::new("zoom-in")
                            .ghost()
                            .xsmall()
                            .label("+")
                            .tooltip(t!("cmd.zoom_in"))
                            .on_click(cx.listener(|this, _, _, cx| this.zoom_by(1, None, cx))),
                    )
                    .child(
                        Button::new("zoom-out")
                            .ghost()
                            .xsmall()
                            .label("−")
                            .tooltip(t!("cmd.zoom_out"))
                            .on_click(cx.listener(|this, _, _, cx| this.zoom_by(-1, None, cx))),
                    ),
            )
    }
}

fn pill() -> Div {
    h_flex()
        .gap_0p5()
        .p_0p5()
        .items_center()
        .rounded(theme::RADIUS_SM)
        .bg(theme::panel().opacity(0.92))
        .border_1()
        .border_color(theme::border_strong())
}

impl Render for MapView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.needs_fit {
            self.fit();
        }
        let tiles = self.render_tiles(cx);
        let markers = self.render_markers(cx);
        let entity = cx.entity().downgrade();
        div()
            .id("map")
            .flex_1()
            .size_full()
            .relative()
            .overflow_hidden()
            .bg(theme::surface())
            .when(self.placing, |d| d.cursor_crosshair())
            .when(!self.placing && self.drag.is_some(), |d| d.cursor_grabbing())
            .child(
                canvas(
                    move |bounds, _, cx| {
                        let _ = entity.update(cx, |this, cx| {
                            if this.bounds != bounds {
                                this.bounds = bounds;
                                cx.notify();
                            }
                        });
                    },
                    |_, _, _, _| {},
                )
                .absolute()
                .size_full(),
            )
            .children(tiles)
            .children(markers)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, ev: &MouseDownEvent, _, cx| {
                    if this.picker {
                        // Picker: dragging moves the map, a simple click sets the pin.
                        this.press = Some(ev.position);
                        this.drag = Some(ev.position);
                    } else if this.placing {
                        let (x, y) = this.to_world(ev.position);
                        let (lat, lon) = unproject(x.rem_euclid(TILE_SIZE), y.clamp(0., TILE_SIZE));
                        this.placing = false;
                        if let Some(name) = this.selected.clone() {
                            cx.emit(MapEvent::SetCoords { name, lat, lon, checkpoint: true });
                        }
                    } else {
                        this.drag = Some(ev.position);
                    }
                    cx.notify();
                }),
            )
            .on_mouse_move(cx.listener(|this, ev: &MouseMoveEvent, _, cx| {
                let Some(last) = this.drag else { return };
                if ev.pressed_button != Some(MouseButton::Left) {
                    this.drag = None;
                    cx.notify();
                    return;
                }
                let s = this.scale();
                this.center.0 -= f64::from(f32::from(ev.position.x - last.x)) / s;
                this.center.1 -= f64::from(f32::from(ev.position.y - last.y)) / s;
                this.clamp_center();
                this.drag = Some(ev.position);
                cx.notify();
            }))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, ev: &MouseUpEvent, _, cx| {
                    this.drag = None;
                    if let Some(at) = this.press.take().filter(|_| this.picker) {
                        let moved = f32::from((ev.position.x - at.x).abs()) + f32::from((ev.position.y - at.y).abs());
                        if moved < 4. {
                            let (x, y) = this.to_world(ev.position);
                            let (lat, lon) = unproject(x.rem_euclid(TILE_SIZE), y.clamp(0., TILE_SIZE));
                            this.set_pin(lat, lon);
                            cx.emit(MapEvent::SetCoords { name: String::new(), lat, lon, checkpoint: true });
                        }
                    }
                    cx.notify();
                }),
            )
            .on_mouse_up_out(
                MouseButton::Left,
                cx.listener(|this, _, _, cx| {
                    this.drag = None;
                    cx.notify();
                }),
            )
            .on_mouse_down(MouseButton::Right, {
                let pos = self.menu_pos.clone();
                move |ev: &MouseDownEvent, _, _| pos.set(Some(ev.position))
            })
            .on_scroll(cx.listener(|this, ev: &ScrollEvent, _, cx| {
                let dy = f32::from(ev.delta.pixel_delta(px(20.)).y);
                this.scroll_acc += dy;
                if this.scroll_acc.abs() >= 40. {
                    let step = if this.scroll_acc > 0. { 1 } else { -1 };
                    this.scroll_acc = 0.;
                    this.zoom_by(step, Some(ev.position), cx);
                }
            }))
            .when(self.picker, |d| d.child(self.render_toolbar(cx)))
            // Corner of the map: connection state (click to switch) and the required attribution.
            .child(
                h_flex()
                    .absolute()
                    .bottom_0()
                    .right_0()
                    .px_2()
                    .py_0p5()
                    .gap_2()
                    .text_xs()
                    .rounded_tl(theme::RADIUS_SM)
                    .bg(theme::panel().opacity(0.85))
                    .text_color(theme::muted())
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .when(!self.picker, |d| {
                        d.child(
                            div()
                                .id("online")
                                .cursor_pointer()
                                .hover(|d| d.text_color(theme::text()))
                                .child(if self.online { t!("map.online") } else { t!("map.offline") })
                                .tooltip({
                                    let tip: SharedString =
                                        if self.online { t!("map.online_tip") } else { t!("map.offline_tip") }.into();
                                    move |w, cx| Tooltip::new(tip.clone()).build(w, cx)
                                })
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.online = !this.online;
                                    this.failed.clear();
                                    cx.notify();
                                })),
                        )
                        .child("·")
                    })
                    .child(t!("map.attribution")),
            )
            .context_menu(background_menu(cx.entity().downgrade()))
    }
}

/// An entry of the map's context menu.
fn map_item(
    map: &WeakEntity<MapView>,
    label: impl Into<SharedString>,
    f: impl Fn(&mut MapView, &mut Context<MapView>) + 'static,
) -> PopupMenuItem {
    let map = map.clone();
    PopupMenuItem::new(label).on_click(move |_, _, cx| {
        let _ = map.update(cx, |this, cx| {
            f(this, cx);
            cx.notify();
        });
    })
}

/// Right-click on a marker. `at`: position of the marker in the map.
fn marker_menu(
    map: WeakEntity<MapView>,
    name: String,
    at: Point<Pixels>,
) -> impl Fn(PopupMenu, &mut Window, &mut Context<PopupMenu>) -> PopupMenu + 'static {
    move |menu, _, _| {
        let (n1, n2) = (name.clone(), name.clone());
        menu.item(map_item(&map, t!("map.show_place"), move |_, cx| cx.emit(MapEvent::Select(n1.clone()))))
            .item(map_item(&map, t!("map.move_by_hand"), move |this, cx| {
                cx.emit(MapEvent::Select(n2.clone()));
                this.placing = true;
            }))
            .separator()
            .item(map_item(&map, t!("map.zoom_in"), move |this, cx| {
                let origin = this.bounds.origin;
                this.zoom_by(2, Some(origin + at), cx)
            }))
    }
}

/// Right-click on the map background.
fn background_menu(
    map: WeakEntity<MapView>,
) -> impl Fn(PopupMenu, &mut Window, &mut Context<PopupMenu>) -> PopupMenu + 'static {
    move |menu, _, cx| {
        let Some(this) = map.upgrade() else { return menu };
        let (selected, pos) = {
            let this = this.read(cx);
            (this.selected.clone().filter(|s| !s.is_empty()), this.menu_pos.get())
        };
        menu.when_some(selected.zip(pos), |m, (name, pos)| {
            let short = gedcom::short_place(&name).to_string();
            m.item(map_item(&map, t!("map.place_here", name = short), move |this, cx| {
                let (x, y) = this.to_world(pos);
                let (lat, lon) = unproject(x.rem_euclid(TILE_SIZE), y.clamp(0., TILE_SIZE));
                this.placing = false;
                cx.emit(MapEvent::SetCoords { name: name.clone(), lat, lon, checkpoint: true });
            }))
            .separator()
        })
        .when_some(pos, |m, pos| {
            m.item(map_item(&map, t!("map.zoom_in"), move |this, cx| this.zoom_by(2, Some(pos), cx))).item(map_item(
                &map,
                t!("map.zoom_out"),
                move |this, cx| this.zoom_by(-2, Some(pos), cx),
            ))
        })
        .item(map_item(&map, t!("map.fit"), |this, _| this.fit()))
    }
}

/// Tile cache folder (`~/Library/Caches/GEDRoots/tiles`, `$XDG_CACHE_HOME/gedroots/tiles`…).
fn tile_cache_dir() -> Option<PathBuf> {
    if let Some(d) = std::env::var_os("GEDROOTS_TILE_CACHE") {
        return Some(PathBuf::from(d));
    }
    let home = std::env::var_os("HOME").map(PathBuf::from);
    if cfg!(target_os = "macos") {
        return Some(home?.join("Library/Caches/GEDRoots/tiles"));
    }
    if cfg!(target_os = "windows") {
        return Some(PathBuf::from(std::env::var_os("LOCALAPPDATA")?).join("GEDRoots/tiles"));
    }
    let base = std::env::var_os("XDG_CACHE_HOME").map(PathBuf::from).or_else(|| Some(home?.join(".cache")))?;
    Some(base.join("gedroots/tiles"))
}

fn download(agent: &ureq::Agent, url: &str, path: &std::path::Path) -> Result<(), String> {
    let resp = agent.get(url).call().map_err(|e| e.to_string())?;
    let mut bytes = Vec::new();
    std::io::Read::read_to_end(&mut resp.into_reader().take(4 << 20), &mut bytes).map_err(|e| e.to_string())?;
    if bytes.len() < 8 || !(bytes.starts_with(b"\x89PNG") || bytes.starts_with(&[0xFF, 0xD8])) {
        return Err(t!("map.unexpected").into());
    }
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let tmp = path.with_extension("part");
    std::fs::write(&tmp, &bytes).map_err(|e| e.to_string())?;
    std::fs::rename(tmp, path).map_err(|e| e.to_string())
}

use crate::t;
use std::io::Read as _;
