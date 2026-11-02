//! Lieux : menu du champ Lieu (lieux du fichier, lieu-dit d'un lieu connu, lieux réels
//! trouvés dans OpenStreetMap, création), fenêtre « Nouveau lieu » découpée selon le
//! format des lieux du fichier, et « Localiser » sur la fiche d'un lieu sans coordonnées.
//!
//! Les recherches en ligne se font une à la fois, après une pause de saisie ; rien n'est
//! écrit sans un clic.

use super::*;
use crate::gazetteer;
use gedcom::gazetteer::{Candidate, Level, join_parts, sub_place};
use std::collections::HashMap;

/// Lieux réels proposés pendant la saisie d'un lieu.
#[derive(Default)]
pub(super) struct PlaceSearch {
    /// Saisie dont les résultats sont affichés.
    query: String,
    results: Vec<Candidate>,
    cache: HashMap<String, Vec<Candidate>>,
    generation: u64,
}

/// Coordonnées d'un lieu créé, écrites dès que le lieu apparaît dans le fichier
/// (à l'enregistrement de l'événement ou de la fiche).
pub(super) struct PendingPlace {
    name: String,
    lat: f64,
    lon: f64,
    exid: Option<(String, String)>,
}

/// Fenêtre « Nouveau lieu ».
pub(super) struct NewPlace {
    /// Champ de lieu qui recevra le nom.
    target: Entity<InputState>,
    fields: Vec<(SharedString, Entity<InputState>)>,
    pin: Option<(f64, f64)>,
    exid: Option<(String, String)>,
    map: Entity<MapView>,
    error: Option<SharedString>,
    _sub: Subscription,
}

/// « Localiser » sur la fiche d'un lieu.
pub(super) struct Locate {
    name: String,
    /// `None` : recherche en cours.
    results: Option<Result<Vec<Candidate>, String>>,
}

/// Exemple affiché dans le champ vide d'une juridiction.
fn example(level: Level) -> &'static str {
    match level {
        Level::Locality => "ex. Le Moulin",
        Level::Municipality => "ex. Troyes",
        Level::PostCode => "ex. 10000",
        Level::County => "ex. Aube",
        Level::State => "ex. Grand Est",
        Level::Country => "ex. France",
        Level::Other => "",
    }
}

/// Une ligne cliquable du menu de lieux : nom, et dessous les juridictions plus larges.
fn place_row(id: SharedString, ic: IconName, name: SharedString, context: Option<SharedString>) -> Stateful<Div> {
    h_flex()
        .id(id)
        .gap_2()
        .px_2()
        .py_1()
        .items_start()
        .rounded(theme::RADIUS_SM)
        .cursor_pointer()
        .hover(|d| d.bg(theme::hover()))
        .child(icon(ic).text_color(theme::muted()).mt(px(3.)))
        .child(
            v_flex()
                .min_w_0()
                .child(div().overflow_hidden().whitespace_nowrap().text_ellipsis().child(name))
                .when_some(context, |d, c| {
                    d.child(
                        div()
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .text_xs()
                            .text_color(theme::muted())
                            .child(c),
                    )
                }),
        )
}

impl Workspace {
    /// Saisie dans un champ de lieu : affiche le menu, et cherche les lieux réels après une pause.
    pub(super) fn on_place_input(&mut self, input: &Entity<InputState>, ev: &InputEvent, cx: &mut Context<Self>) {
        if !matches!(ev, InputEvent::Focus | InputEvent::Change) {
            return;
        }
        self.place_focus = Some(input.entity_id());
        if matches!(ev, InputEvent::Change) {
            let value = input.read(cx).value().to_string();
            self.search_places(value, cx);
        }
        cx.notify();
    }

    fn search_places(&mut self, value: String, cx: &mut Context<Self>) {
        let q = value.trim().to_string();
        let s = &mut self.place_search;
        s.generation += 1;
        // Lieu-dit d'un lieu du fichier (« La Borde, Troyes ») : pas de recherche en ligne.
        if q.chars().count() < 3 || q.contains(',') || gazetteer::offline() {
            return;
        }
        if let Some(hit) = s.cache.get(&q) {
            s.results = hit.clone();
            s.query = q;
            return;
        }
        let generation = s.generation;
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(gazetteer::TYPING_PAUSE).await;
            let current = this.update(cx, |this, _| this.place_search.generation == generation).unwrap_or(false);
            if !current {
                return;
            }
            let query = q.clone();
            let res = cx.background_spawn(async move { gazetteer::search(&query, 5) }).await;
            let _ = this.update(cx, |this, cx| {
                let found = res.unwrap_or_default();
                let s = &mut this.place_search;
                s.cache.insert(q.clone(), found.clone());
                if s.generation == generation {
                    s.query = q;
                    s.results = found;
                    cx.notify();
                }
            });
        })
        .detach();
    }

    /// Menu sous le champ de lieu en cours de saisie.
    pub(super) fn place_suggestions(&self, input: &Entity<InputState>, cx: &mut Context<Self>) -> Option<AnyElement> {
        if self.place_focus != Some(input.entity_id()) {
            return None;
        }
        let value = input.read(cx).value().to_string();
        let typed = value.trim().to_string();
        let q = sort_key(&typed);
        if q.chars().count() < 2 {
            return None;
        }
        let doc = self.doc.as_ref()?;
        let places = doc.ged.places();
        if places.iter().any(|p| p.name == value) {
            return None;
        }
        let form = doc.ged.place_form();
        let key = sort_key(typed.split(',').next().unwrap_or("").trim());
        let mut hits: Vec<(String, String)> = places
            .iter()
            .filter(|p| key.chars().count() >= 2 && sort_key(&p.name).contains(&key))
            .map(|p| (p.short(), p.name.clone()))
            .collect();
        // Les lieux qui commencent par la saisie d'abord.
        hits.sort_by_key(|(short, _)| (!sort_key(short).starts_with(&key), short.clone()));
        hits.truncate(4);
        let sub = sub_place(&typed, &places, &form);
        let real: Vec<Candidate> = if self.place_search.query == typed && sub.is_none() {
            self.place_search
                .results
                .iter()
                .filter(|c| !places.iter().any(|p| p.name == join_parts(&c.parts(&form))))
                .take(4)
                .cloned()
                .collect()
        } else {
            Vec::new()
        };
        let id = input.entity_id();
        let mut list = v_flex()
            .mt_1()
            .p_1()
            .gap_0p5()
            .rounded(theme::RADIUS_MD)
            .border_1()
            .border_color(theme::border_strong())
            .bg(theme::bg())
            .text_sm();
        let mut sections = 0;
        if !hits.is_empty() {
            sections += 1;
            list = list.children(hits.into_iter().enumerate().map(|(n, (short, full))| {
                let entity = input.clone();
                place_row(format!("pl-{id}-{n}").into(), IconName::MapPin, short.into(), None).on_click(cx.listener(
                    move |this, _, window, cx| {
                        entity.update(cx, |s, cx| s.set_value(full.clone(), window, cx));
                        this.place_focus = None;
                        cx.notify();
                    },
                ))
            }));
        }
        if let Some((parts, parent)) = sub {
            let name = typed.split(',').next().unwrap_or("").trim().to_string();
            let near = places.iter().find(|p| p.name == parent).and_then(|p| p.coords());
            let entity = input.clone();
            list = list.when(sections > 0, |l| l.child(separator())).child(
                place_row(
                    format!("pl-sub-{id}").into(),
                    IconName::CornerDownRight,
                    name.into(),
                    Some(format!("lieu-dit à {}", short_place(&parent)).into()),
                )
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.open_new_place(entity.clone(), parts.clone(), None, near, None, window, cx)
                })),
            );
            sections += 1;
        }
        if !real.is_empty() {
            list = list
                .when(sections > 0, |l| l.child(separator()))
                .child(div().px_2().pt_1().child(label_caps("Lieux réels")))
                .children(real.into_iter().enumerate().map(|(n, c)| {
                    let entity = input.clone();
                    let parts = c.parts(&form);
                    place_row(
                        format!("pl-osm-{id}-{n}").into(),
                        IconName::Globe,
                        c.title().into(),
                        Some(c.context().into()),
                    )
                    .on_click(cx.listener(move |this, _, window, cx| {
                        let pin = Some((c.lat, c.lon));
                        this.open_new_place(entity.clone(), parts.clone(), pin, None, c.exid.clone(), window, cx)
                    }))
                }));
            sections += 1;
        }
        let entity = input.clone();
        let blank = {
            let mut v = vec![String::new(); form.len().max(1)];
            let slot = if form.len() >= 2 {
                form.iter().position(|f| Level::of(f) == Level::Municipality).unwrap_or(0)
            } else {
                0
            };
            v[slot] = typed.split(',').next().unwrap_or("").trim().to_string();
            if form.len() < 2 { vec![typed.clone()] } else { v }
        };
        list = list.when(sections > 0, |l| l.child(separator())).child(
            h_flex()
                .id(SharedString::from(format!("pl-new-{id}")))
                .gap_2()
                .px_2()
                .py_1()
                .items_center()
                .rounded(theme::RADIUS_SM)
                .cursor_pointer()
                .hover(|d| d.bg(theme::hover()))
                .text_color(theme::accent())
                .child(icon(IconName::Plus))
                .child(format!("Créer « {typed} »"))
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.open_new_place(entity.clone(), blank.clone(), None, None, None, window, cx)
                })),
        );
        Some(list.into_any_element())
    }

    /// Ouvre la fenêtre « Nouveau lieu », juridictions pré-remplies.
    #[allow(clippy::too_many_arguments)]
    fn open_new_place(
        &mut self,
        target: Entity<InputState>,
        parts: Vec<String>,
        pin: Option<(f64, f64)>,
        near: Option<(f64, f64)>,
        exid: Option<(String, String)>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(doc) = &self.doc else { return };
        let form = doc.ged.place_form();
        let (labels, values): (Vec<(String, Level)>, Vec<String>) = if form.len() >= 2 && parts.len() == form.len() {
            (form.iter().map(|f| (f.replace('_', " "), Level::of(f))).collect(), parts)
        } else {
            (vec![("Lieu".into(), Level::Other)], vec![join_parts(&parts).trim_start_matches(", ").to_string()])
        };
        let fields: Vec<(SharedString, Entity<InputState>)> = labels
            .into_iter()
            .zip(values)
            .map(|((label, level), value)| {
                let ph = if label == "Lieu" { "Commune, Département, Pays" } else { example(level) };
                let state = cx.new(|cx| InputState::new(window, cx).placeholder(ph).default_value(value));
                (SharedString::from(label), state)
            })
            .collect();
        let map = cx.new(|_| MapView::picker(pin, near));
        let sub = cx.subscribe(&map, |this, _, ev: &MapEvent, cx| {
            if let (MapEvent::SetCoords { lat, lon, .. }, Some(d)) = (ev, this.new_place.as_mut()) {
                d.pin = Some((*lat, *lon));
                cx.notify();
            }
        });
        // Le curseur dans la première juridiction à compléter (Échap ferme la fenêtre).
        let first = fields.iter().find(|(_, f)| f.read(cx).value().is_empty()).or(fields.first());
        if let Some((_, f)) = first {
            f.update(cx, |s, cx| s.focus(window, cx));
        }
        self.place_focus = None;
        self.new_place = Some(NewPlace { target, fields, pin, exid, map, error: None, _sub: sub });
        cx.notify();
    }

    fn close_new_place(&mut self, cx: &mut Context<Self>) {
        self.new_place = None;
        cx.notify();
    }

    /// « Créer le lieu » : le nom va dans le champ, les coordonnées attendent l'enregistrement.
    fn create_new_place(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(d) = &mut self.new_place else { return };
        let values: Vec<String> = d.fields.iter().map(|(_, s)| s.read(cx).value().trim().to_string()).collect();
        let name = join_parts(&values);
        if name.is_empty() {
            d.error = Some("Le lieu est vide".into());
            cx.notify();
            return;
        }
        let target = d.target.clone();
        if let Some((lat, lon)) = d.pin {
            let exid = d.exid.clone();
            self.pending_places.retain(|p| p.name != name);
            self.pending_places.push(PendingPlace { name: name.clone(), lat, lon, exid });
        }
        target.update(cx, |s, cx| s.set_value(name, window, cx));
        self.new_place = None;
        self.place_focus = None;
        cx.notify();
    }

    /// Écrit les coordonnées des lieux créés qui sont maintenant dans le fichier.
    pub(super) fn apply_pending_places(&mut self) {
        if self.pending_places.is_empty() {
            return;
        }
        let Some(doc) = &mut self.doc else { return };
        let names: Vec<String> = doc.ged.places().into_iter().map(|p| p.name).collect();
        let pending = std::mem::take(&mut self.pending_places);
        for p in pending {
            if names.contains(&p.name) {
                let (la, lo) = (geo::format_latitude(p.lat), geo::format_longitude(p.lon));
                doc.ged.locate_place(&p.name, &la, &lo, p.exid.as_ref());
            } else {
                self.pending_places.push(p);
            }
        }
    }

    pub(super) fn render_new_place(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let d = self.new_place.as_ref()?;
        let fields = d.fields.iter().map(|(label, state)| {
            v_flex().w(px(196.)).gap_1().child(label_caps(label.clone())).child(Input::new(state).small())
        });
        let coords = match d.pin {
            Some((lat, lon)) => mono(format!("{} · {}", geo::format_latitude(lat), geo::format_longitude(lon))),
            None => div().text_xs().text_color(theme::muted()).child("Sans coordonnées"),
        };
        Some(
            div()
                .absolute()
                .inset_0()
                .flex()
                .justify_center()
                .items_start()
                .pt(px(70.))
                .bg(theme::scrim())
                .child(
                    v_flex()
                        .id("new-place")
                        .occlude()
                        .w(px(800.))
                        .rounded(theme::RADIUS_LG)
                        .border_1()
                        .border_color(theme::border_strong())
                        .bg(theme::panel())
                        .shadow_lg()
                        .on_key_down(cx.listener(|this, ev: &KeyDownEvent, _, cx| {
                            if ev.keystroke.key == "escape" {
                                this.close_new_place(cx);
                            }
                        }))
                        .child(
                            h_flex()
                                .p_3()
                                .gap_2()
                                .items_center()
                                .border_b_1()
                                .border_color(theme::border())
                                .child(div().flex_1().font_weight(FontWeight::SEMIBOLD).child("Nouveau lieu"))
                                .child(
                                    Button::new("new-place-close")
                                        .xsmall()
                                        .ghost()
                                        .icon(IconName::Close)
                                        .on_click(cx.listener(|this, _, _, cx| this.close_new_place(cx))),
                                ),
                        )
                        .child(
                            h_flex()
                                .p_4()
                                .gap_4()
                                .items_start()
                                .child(h_flex().w(px(404.)).flex_none().flex_wrap().gap_3().children(fields))
                                .child(
                                    v_flex()
                                        .flex_1()
                                        .min_w_0()
                                        .gap_2()
                                        .child(
                                            div()
                                                .h(px(240.))
                                                .rounded(theme::RADIUS_SM)
                                                .border_1()
                                                .border_color(theme::border())
                                                .overflow_hidden()
                                                .child(d.map.clone()),
                                        )
                                        .child(coords),
                                ),
                        )
                        .child(
                            h_flex()
                                .p_3()
                                .gap_2()
                                .items_center()
                                .border_t_1()
                                .border_color(theme::border())
                                .when_some(d.error.clone(), |r, e| {
                                    r.child(div().text_sm().text_color(theme::danger()).child(e))
                                })
                                .child(div().flex_1())
                                .child(
                                    Button::new("new-place-cancel")
                                        .ghost()
                                        .small()
                                        .label("Annuler")
                                        .on_click(cx.listener(|this, _, _, cx| this.close_new_place(cx))),
                                )
                                .child(
                                    Button::new("new-place-create")
                                        .primary()
                                        .small()
                                        .icon(IconName::Check)
                                        .label("Créer le lieu")
                                        .on_click(cx.listener(|this, _, w, cx| this.create_new_place(w, cx))),
                                ),
                        ),
                )
                .into_any_element(),
        )
    }

    // ------------------------------------------------------------------
    // Localiser un lieu existant
    // ------------------------------------------------------------------

    /// Cherche un lieu du fichier dans OpenStreetMap, avec toutes ses juridictions.
    pub(super) fn start_locate(&mut self, name: String, cx: &mut Context<Self>) {
        if gazetteer::offline() {
            self.set_status("Recherche en ligne désactivée", true);
            cx.notify();
            return;
        }
        let Some(place) = self.doc.as_ref().and_then(|d| d.ged.places().into_iter().find(|p| p.name == name)) else {
            return;
        };
        let queries = place.geocode_queries();
        self.locate = Some(Locate { name: name.clone(), results: None });
        cx.notify();
        cx.spawn(async move |this, cx| {
            let mut out = Ok(Vec::new());
            for (i, q) in queries.into_iter().enumerate() {
                if i > 0 {
                    cx.background_executor().timer(gazetteer::PAUSE).await;
                }
                out = cx.background_spawn(async move { gazetteer::search(&q, 5) }).await;
                if !matches!(&out, Ok(v) if v.is_empty()) {
                    break;
                }
            }
            let _ = this.update(cx, |this, cx| {
                if let Some(l) = this.locate.as_mut().filter(|l| l.name == name) {
                    l.results = Some(out);
                    cx.notify();
                }
            });
        })
        .detach();
    }

    fn choose_location(&mut self, name: String, c: Candidate, window: &mut Window, cx: &mut Context<Self>) {
        self.locate = None;
        self.mutate(window, cx, move |g| {
            let (la, lo) = (geo::format_latitude(c.lat), geo::format_longitude(c.lon));
            let n = g.locate_place(&name, &la, &lo, c.exid.as_ref());
            Ok((format!("{} : {la} {lo} ({n} occurrence(s))", short_place(&name)), None))
        });
    }

    fn place_by_hand(&mut self, name: String, cx: &mut Context<Self>) {
        self.locate = None;
        self.tab = CenterTab::Map;
        self.map.update(cx, |m, cx| m.start_placing(name, cx));
        cx.notify();
    }

    /// Section « Position » de la fiche d'un lieu sans coordonnées.
    pub(super) fn render_locate(&self, place: &Place, cx: &mut Context<Self>) -> Option<AnyElement> {
        if place.coords().is_some() {
            return None;
        }
        let name = place.name.clone();
        let by_hand = {
            let name = name.clone();
            place_row("locate-hand".into(), IconName::MapPin, "Placer sur la carte".into(), None)
                .on_click(cx.listener(move |this, _, _, cx| this.place_by_hand(name.clone(), cx)))
        };
        let body = match self.locate.as_ref().filter(|l| l.name == name).map(|l| &l.results) {
            None => h_flex()
                .gap_2()
                .child(Button::new("locate").primary().small().icon(IconName::Locate).label("Localiser").on_click(
                    cx.listener({
                        let name = name.clone();
                        move |this, _, _, cx| this.start_locate(name.clone(), cx)
                    }),
                ))
                .child(Button::new("locate-hand-btn").ghost().small().label("Placer sur la carte").on_click(
                    cx.listener({
                        let name = name.clone();
                        move |this, _, _, cx| this.place_by_hand(name.clone(), cx)
                    }),
                ))
                .into_any_element(),
            Some(None) => div().text_color(theme::muted()).child("Recherche…").into_any_element(),
            Some(Some(Err(e))) => v_flex()
                .gap_1()
                .child(div().text_color(theme::danger()).child(format!("Recherche impossible : {e}")))
                .child(by_hand)
                .into_any_element(),
            Some(Some(Ok(found))) => v_flex()
                .gap_0p5()
                .when(found.is_empty(), |d| d.child(div().text_color(theme::muted()).child("Aucun lieu trouvé")))
                .children(found.iter().enumerate().map(|(n, c)| {
                    let (name, c2) = (name.clone(), c.clone());
                    place_row(format!("locate-{n}").into(), IconName::Globe, c.title().into(), Some(c.context().into()))
                        .on_click(
                            cx.listener(move |this, _, w, cx| this.choose_location(name.clone(), c2.clone(), w, cx)),
                        )
                }))
                .child(by_hand)
                .into_any_element(),
        };
        Some(
            v_flex()
                .p_4()
                .gap_2()
                .border_b_1()
                .border_color(theme::border())
                .child(label_caps("Position"))
                .child(body)
                .into_any_element(),
        )
    }
}

fn separator() -> Div {
    div().h(px(1.)).mx_1().my_0p5().bg(theme::border())
}
