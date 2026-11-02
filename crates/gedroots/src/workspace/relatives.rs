//! Adding and linking relatives, person picker, attaching media.

use super::*;
use gpui_kit::component::IconName;
use rust_i18n::t;

/// What the choice in the picker will do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PickPurpose {
    /// `other` becomes the `rel` of `indi`.
    Relative { indi: String, rel: Relation },
    /// The chosen person becomes a child of the family.
    FamilyChild { fam: String },
    /// The media is attached to the chosen record.
    MediaTo { media: String },
    /// The chosen person is identified on the photo (page `page`), in the `crop` area.
    TagPerson { media: String, page: usize, crop: Crop },
    /// The chosen media is added at the end of `media`, as new pages.
    MergeInto { media: String },
    /// Source in which to enter a record (or new source, empty choice).
    ActSource,
    /// Relationship between `indi` and the chosen person (nothing is changed).
    Kinship { indi: String },
    /// The chosen person is the person at `idx` in the record being entered; the likely
    /// individuals (individual, why) come first.
    ActPerson { idx: usize, suggestions: Vec<(String, String)> },
}

impl PickPurpose {
    fn title(&self, g: &Gedcom) -> String {
        match self {
            PickPurpose::Relative { indi, rel } => rel_title(*rel, &g.record_label(indi)),
            PickPurpose::FamilyChild { fam } => t!("picker.choose_child", name = g.record_label(fam)).into_owned(),
            PickPurpose::MediaTo { media } => t!("picker.attach_to", name = g.record_label(media)).into_owned(),
            PickPurpose::TagPerson { .. } => t!("picker.who").into(),
            PickPurpose::Kinship { indi } => t!("picker.kinship", name = g.record_label(indi)).into_owned(),
            PickPurpose::MergeInto { media } => t!("picker.merge_into", name = g.record_label(media)).into_owned(),
            PickPurpose::ActSource => t!("picker.act_source").into(),
            PickPurpose::ActPerson { .. } => t!("picker.act_person").into(),
        }
    }
}

fn rel_title(rel: Relation, name: &str) -> String {
    match rel {
        Relation::Father => t!("picker.choose_father", name = name),
        Relation::Mother => t!("picker.choose_mother", name = name),
        Relation::Spouse => t!("picker.choose_spouse", name = name),
        Relation::Child => t!("picker.choose_child", name = name),
        Relation::Sibling => t!("picker.choose_sibling", name = name),
    }
    .into_owned()
}

pub struct Picker {
    pub input: Entity<InputState>,
    pub purpose: PickPurpose,
    _sub: Subscription,
}

/// Image/PDF files of the `.ged` folder (relative paths, separated by `/`).
pub fn scan_media_files(ged_path: Option<&std::path::Path>) -> Vec<String> {
    let Some(root) = ged_path.and_then(|p| p.parent()) else { return vec![] };
    let mut out = Vec::new();
    fn walk(dir: &std::path::Path, rel: &str, depth: u32, out: &mut Vec<String>) {
        let Ok(rd) = std::fs::read_dir(dir) else { return };
        let mut entries: Vec<_> = rd.flatten().collect();
        entries.sort_by_key(|e| e.file_name());
        for e in entries {
            if out.len() >= 20_000 {
                return;
            }
            let name = e.file_name().to_string_lossy().to_string();
            if name.starts_with('.') {
                continue;
            }
            let path = if rel.is_empty() { name.clone() } else { format!("{rel}/{name}") };
            let Ok(ft) = e.file_type() else { continue };
            if ft.is_dir() && depth < 4 {
                walk(&e.path(), &path, depth + 1, out);
            } else if ft.is_file() && is_media_file(&name) {
                out.push(path);
            }
        }
    }
    walk(root, "", 0, &mut out);
    out
}

/// Files not attached yet whose name contains the keywords (surname, given name…), from most to
/// least relevant.
pub(super) fn suggest_files(
    files: &[String],
    referenced: &std::collections::HashSet<String>,
    keys: &[String],
) -> Vec<String> {
    let Some(first) = keys.first() else { return vec![] };
    let mut scored: Vec<(usize, &String)> = files
        .iter()
        .filter(|f| !referenced.contains(*f))
        .filter_map(|f| {
            let k = sort_key(f);
            // The first keyword (surname, title…) is required.
            if !k.contains(first.as_str()) {
                return None;
            }
            Some((keys.iter().filter(|w| k.contains(w.as_str())).count(), f))
        })
        .collect();
    scored.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(b.1)));
    scored.into_iter().take(6).map(|(_, f)| f.clone()).collect()
}

pub(super) fn keywords(s: &str) -> Vec<String> {
    sort_key(s).split(|c: char| !c.is_alphanumeric()).filter(|w| w.chars().count() >= 3).map(str::to_string).collect()
}

impl Workspace {
    /// Applies a change to the document with an undo point. `f` returns the status message and,
    /// optionally, the record to open.
    pub(super) fn mutate(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
        f: impl FnOnce(&mut Gedcom) -> Result<(String, Option<String>), String>,
    ) {
        if self.doc.is_none() {
            return;
        }
        self.checkpoint();
        let doc = self.doc.as_mut().unwrap();
        match f(&mut doc.ged) {
            Ok((msg, open)) => {
                self.apply_pending_places();
                self.rebuild_rows(cx);
                match open {
                    Some(x) => {
                        self.filter.update(cx, |f, cx| f.set_value("", window, cx));
                        self.goto(x, window, cx);
                        self.rebuild_rows(cx);
                        self.tab = CenterTab::Form;
                    }
                    None => self.refresh_editor(window, cx),
                }
                self.set_status(msg, false);
            }
            Err(e) => {
                self.drop_checkpoint();
                self.set_error("relatives.failed", e);
            }
        }
        cx.notify();
    }

    pub(super) fn add_relative(&mut self, indi: String, rel: Relation, window: &mut Window, cx: &mut Context<Self>) {
        self.mutate(window, cx, |g| {
            let name = g.record_label(&indi);
            let x = g.add_relative(&indi, rel, None)?;
            Ok((t!("picker.relative_added", rel = crate::i18n::relation(rel), name = name).into_owned(), Some(x)))
        });
    }

    pub(super) fn add_family_child(&mut self, fam: String, window: &mut Window, cx: &mut Context<Self>) {
        self.mutate(window, cx, |g| {
            let f = g.family(&fam).ok_or_else(|| t!("picker.family_not_found").to_string())?;
            let surname = f.husband().and_then(|h| g.individual(h)).map(|h| h.surname()).unwrap_or_default();
            let x = g.create_individual("", &surname, "U");
            g.add_child_to_family(&fam, &x)?;
            Ok((t!("picker.child_added").into(), Some(x)))
        });
    }

    pub(super) fn unlink(&mut self, indi: String, fam: String, window: &mut Window, cx: &mut Context<Self>) {
        self.mutate(window, cx, |g| {
            let (who, from) = (g.record_label(&indi), g.record_label(&fam));
            g.unlink_from_family(&indi, &fam);
            Ok((t!("picker.detached", who = who, from = from).into_owned(), None))
        });
    }

    pub(super) fn open_picker(&mut self, purpose: PickPurpose, window: &mut Window, cx: &mut Context<Self>) {
        let hint = match purpose {
            PickPurpose::MergeInto { .. } => t!("picker.media_hint"),
            PickPurpose::ActSource => t!("picker.source_hint"),
            _ => t!("picker.person_hint"),
        };
        let input = cx.new(|cx| InputState::new(window, cx).placeholder(hint));
        let sub = cx.subscribe_in(&input, window, |this, _, ev: &InputEvent, window, cx| match ev {
            InputEvent::Change => cx.notify(),
            InputEvent::PressEnter { .. } => {
                if let Some(first) = this.picker_results(cx).into_iter().next() {
                    this.pick(first, window, cx);
                }
            }
            _ => {}
        });
        input.update(cx, |s, cx| s.focus(window, cx));
        self.picker = Some(Picker { input, purpose, _sub: sub });
        cx.notify();
    }

    fn close_picker(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.picker = None;
        window.focus(&self.focus, cx);
        cx.notify();
    }

    /// Candidates of the picker (at most 80).
    fn picker_results(&self, cx: &App) -> Vec<String> {
        let (Some(p), Some(doc)) = (&self.picker, &self.doc) else { return vec![] };
        let g = &doc.ged;
        let text = p.input.read(cx).value().to_string();
        let q = IndividualQuery::parse(&text);
        let exclude: Option<&str> = match &p.purpose {
            PickPurpose::Relative { indi, .. } => Some(indi),
            _ => None,
        };
        let me: Option<&str> = match &p.purpose {
            PickPurpose::Kinship { indi } => Some(indi),
            _ => exclude,
        };
        // For a relationship: compatible sex, and plausible dates first.
        let (wrong_sex, expected) = match &p.purpose {
            PickPurpose::Relative { indi, rel } => {
                let year = g.individual(indi).and_then(|i| i.birth_year());
                let expected = year.map(|y| match rel {
                    Relation::Father | Relation::Mother => y - 28,
                    Relation::Spouse | Relation::Sibling => y,
                    Relation::Child => y + 28,
                });
                let wrong = match rel {
                    Relation::Father => Some("F"),
                    Relation::Mother => Some("M"),
                    _ => None,
                };
                (wrong, expected)
            }
            _ => (None, None),
        };
        // Relatives already linked (parents, siblings, spouses, children): no need to suggest them.
        let mut close = std::collections::HashSet::new();
        if let Some(i) = exclude.and_then(|x| g.individual(x)) {
            for f in i.famc().into_iter().chain(i.fams()).filter_map(|f| g.family(f)) {
                close.extend([f.husband(), f.wife()].into_iter().flatten());
                close.extend(f.children());
            }
        }
        let mut people: Vec<Individual> = g
            .records_of(RecordKind::Individual)
            .map(Individual::new)
            .filter(|i| {
                Some(i.xref()) != me && !close.contains(i.xref()) && Some(i.sex()) != wrong_sex && q.matches(g, i)
            })
            .collect();
        people.sort_by_cached_key(|i| {
            let distance = match (expected, i.birth_year()) {
                (Some(e), Some(y)) => (e - y).abs() / 10,
                (Some(_), None) => i32::MAX,
                _ => 0,
            };
            (distance, sort_key(&i.surname()), sort_key(&i.given()))
        });
        let mut out: Vec<String> = Vec::new();
        if let PickPurpose::MergeInto { media } = &p.purpose {
            // Media, by their title or their files.
            let words: Vec<String> = text.split_whitespace().map(sort_key).collect();
            for r in g.records_of(RecordKind::Media) {
                let Some(x) = r.xref.as_deref().filter(|x| *x != media) else { continue };
                let m = Media::new(r);
                let k = sort_key(&format!(
                    "{} {}",
                    m.title(),
                    m.files().iter().map(|f| f.path).collect::<Vec<_>>().join(" ")
                ));
                if words.iter().all(|w| k.contains(w.as_str())) {
                    out.push(x.to_string());
                }
            }
            out.truncate(80);
            return out;
        }
        if let PickPurpose::ActSource = p.purpose {
            // Sources by their title; last, “New Source” (empty choice).
            let words: Vec<String> = text.split_whitespace().map(sort_key).collect();
            let mut found: Vec<(String, String)> = g
                .records_of(RecordKind::Source)
                .filter_map(|r| {
                    let x = r.xref.clone()?;
                    let k = sort_key(&g.record_label(&x));
                    words.iter().all(|w| k.contains(w.as_str())).then_some((k, x))
                })
                .collect();
            found.sort();
            out.extend(found.into_iter().map(|(_, x)| x).take(79));
            out.push(String::new());
            return out;
        }
        if let PickPurpose::MediaTo { .. } = p.purpose {
            // Sources and families too, by the free text.
            let words: Vec<String> = q.words.clone();
            let hit = |label: &str| {
                let k = sort_key(label);
                !words.is_empty() && words.iter().all(|w| k.contains(w.as_str()))
            };
            for kind in [RecordKind::Source, RecordKind::Family] {
                for r in g.records_of(kind) {
                    if let Some(x) = &r.xref
                        && hit(&g.record_label(x))
                    {
                        out.push(x.clone());
                    }
                }
            }
        }
        if let PickPurpose::TagPerson { media, .. } = &p.purpose {
            // First the people already linked to the photo without an area, last those already
            // located.
            let links = g.media_links(media);
            let rank = |x: &str| match links.iter().find(|l| l.owner == x) {
                Some(l) if l.crop.is_none() => 0,
                None => 1,
                Some(_) => 2,
            };
            people.sort_by_key(|i| rank(i.xref()));
        }
        if let PickPurpose::ActPerson { suggestions, .. } = &p.purpose {
            // The likely individuals first, in their order.
            let rank = |x: &str| suggestions.iter().position(|(s, _)| s == x).unwrap_or(usize::MAX);
            people.sort_by_key(|i| rank(i.xref()));
        }
        out.extend(people.into_iter().map(|i| i.xref().to_string()));
        out.truncate(80);
        out
    }

    fn pick(&mut self, chosen: String, window: &mut Window, cx: &mut Context<Self>) {
        let Some(p) = self.picker.take() else { return };
        window.focus(&self.focus, cx);
        if let PickPurpose::ActSource = p.purpose {
            let source = if chosen.is_empty() {
                // New source, titled after the search (otherwise automatic title at the first
                // record).
                let title = p.input.read(cx).value().trim().to_string();
                self.checkpoint();
                let Some(doc) = &mut self.doc else { return };
                let x = doc.ged.create(RecordKind::Source);
                if !title.is_empty() {
                    doc.ged.get_mut(&x).unwrap().set_child_text("TITL", &title);
                }
                self.rebuild_rows(cx);
                x
            } else {
                chosen
            };
            self.open_act(source, None, window, cx);
            return;
        }
        if let PickPurpose::ActPerson { idx, .. } = p.purpose {
            if let Some((editor, _)) = &self.act_editor {
                editor.update(cx, |e, cx| e.link_person(idx, chosen, window, cx));
            }
            return;
        }
        if let PickPurpose::Kinship { indi } = p.purpose {
            self.kinship = Some((indi, chosen));
            cx.notify();
            return;
        }
        self.mutate(window, cx, |g| {
            let who = g.record_label(&chosen);
            match p.purpose {
                PickPurpose::Relative { indi, rel } => {
                    g.link_relative(&indi, rel, &chosen)?;
                    Ok((
                        t!(
                            "picker.linked_as",
                            who = who,
                            rel = crate::i18n::relation(rel).to_lowercase(),
                            name = g.record_label(&indi)
                        )
                        .into_owned(),
                        None,
                    ))
                }
                PickPurpose::FamilyChild { fam } => {
                    g.add_child_to_family(&fam, &chosen)?;
                    Ok((t!("picker.added_child", who = who).into_owned(), None))
                }
                PickPurpose::MediaTo { media } => {
                    g.attach_media(&chosen, &media)?;
                    Ok((t!("picker.media_attached", who = who).into_owned(), None))
                }
                PickPurpose::MergeInto { media } => {
                    let n = Media::new(g.get(&chosen).ok_or_else(|| t!("picker.media_not_found").to_string())?)
                        .page_count();
                    g.merge_media(&[media.clone(), chosen])?;
                    Ok((crate::tn!("picker.added_to_media", n, who = who).into_owned(), Some(media)))
                }
                // Handled before (writes nothing by itself).
                PickPurpose::ActSource => Ok((String::new(), None)),
                PickPurpose::TagPerson { media, page, crop } => {
                    g.tag_person_on(&chosen, &media, page, crop)?;
                    Ok((t!("picker.tagged", who = who).into_owned(), None))
                }
                // Handled before: nothing to change.
                PickPurpose::Kinship { .. } | PickPurpose::ActPerson { .. } => Err(t!("stats.kinship_title").into()),
            }
        });
    }

    // ------------------------------------------------------------------
    // Media
    // ------------------------------------------------------------------

    /// Path to write in `FILE`: relative to the `.ged` folder if possible.
    pub(super) fn file_value(&self, path: &std::path::Path) -> String {
        let base = self.doc.as_ref().and_then(|d| d.path.as_ref()).and_then(|p| p.parent());
        match base.and_then(|b| path.strip_prefix(b).ok()) {
            Some(rel) => rel.components().map(|c| c.as_os_str().to_string_lossy()).collect::<Vec<_>>().join("/"),
            None => path.to_string_lossy().to_string(),
        }
    }

    pub(super) fn rescan_media(&mut self) {
        let path = self.doc.as_ref().and_then(|d| d.path.clone());
        self.media_files = Rc::new(scan_media_files(path.as_deref()));
    }

    // ------------------------------------------------------------------
    // Rendu
    // ------------------------------------------------------------------

    /// Window for choosing a person (or a record), above the interface.
    pub(super) fn render_picker(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let p = self.picker.as_ref()?;
        let g = &self.doc.as_ref()?.ged;
        let results = self.picker_results(cx);
        let title = p.purpose.title(g);
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
                        .id("picker")
                        .occlude()
                        .w(px(520.))
                        .max_h(px(560.))
                        .rounded(theme::RADIUS_LG)
                        .border_1()
                        .border_color(theme::border_strong())
                        .bg(theme::panel())
                        .shadow_lg()
                        .on_mouse_down_out(cx.listener(|this, _, w, cx| this.close_picker(w, cx)))
                        .on_key_down(cx.listener(|this, ev: &KeyDownEvent, w, cx| {
                            if ev.keystroke.key == "escape" {
                                this.close_picker(w, cx);
                            }
                        }))
                        .child(
                            h_flex()
                                .p_3()
                                .gap_2()
                                .items_center()
                                .border_b_1()
                                .border_color(theme::border())
                                .child(div().flex_1().font_weight(FontWeight::SEMIBOLD).child(title))
                                .child(
                                    Button::new("picker-close")
                                        .xsmall()
                                        .ghost()
                                        .icon(IconName::Close)
                                        .on_click(cx.listener(|this, _, w, cx| this.close_picker(w, cx))),
                                ),
                        )
                        .child(div().p_3().child(Input::new(&p.input).small().prefix(IconName::Search)))
                        .child(
                            v_flex()
                                .id("picker-results")
                                .flex_1()
                                .min_h_0()
                                .overflow_y_scroll()
                                .pb_2()
                                .when(results.is_empty(), |d| {
                                    d.child(
                                        div().px_3().py_2().text_color(theme::muted()).child(t!("picker.no_result")),
                                    )
                                })
                                .children(results.into_iter().enumerate().map(|(n, x)| {
                                    let kind = g.get(&x).and_then(|r| RecordKind::from_tag(&r.tag));
                                    let why = match &p.purpose {
                                        PickPurpose::ActPerson { suggestions, .. } => {
                                            suggestions.iter().find(|(s, _)| *s == x).map(|(_, why)| why.clone())
                                        }
                                        _ => None,
                                    };
                                    let meta = match g.individual(&x) {
                                        // A likely individual: why it is suggested.
                                        Some(i) if why.as_deref().is_some_and(|w| !w.is_empty()) => {
                                            [i.years(), why.clone().unwrap_or_default()].join(" · ")
                                        }
                                        Some(i) => {
                                            let place =
                                                i.birth().and_then(|e| e.place().map(short_place)).unwrap_or_default();
                                            [i.years(), place]
                                                .into_iter()
                                                .filter(|s| !s.is_empty())
                                                .collect::<Vec<_>>()
                                                .join(" · ")
                                        }
                                        None if kind == Some(RecordKind::Media) => {
                                            let n = g.get(&x).map(|r| Media::new(r).page_count()).unwrap_or(0);
                                            if n > 1 {
                                                t!("picker.pages", n = n).into_owned()
                                            } else {
                                                gedcom::RecordKind::Media.label().into_owned()
                                            }
                                        }
                                        None => {
                                            kind.map(|k| crate::i18n::record_kind(k).into_owned()).unwrap_or_default()
                                        }
                                    };
                                    let target = x.clone();
                                    h_flex()
                                        .id(SharedString::from(format!("pick-{x}")))
                                        .px_3()
                                        .py_1()
                                        .gap_2()
                                        .items_center()
                                        .cursor_pointer()
                                        .when(n == 0, |d| d.bg(theme::hover()))
                                        .hover(|d| d.bg(theme::surface()))
                                        .child(
                                            div().flex_1().overflow_hidden().whitespace_nowrap().text_ellipsis().child(
                                                if x.is_empty() {
                                                    t!("picker.new_source").into_owned()
                                                } else {
                                                    g.record_label(&x)
                                                },
                                            ),
                                        )
                                        // A long reason gives way to the name.
                                        .child(
                                            mono(meta)
                                                .max_w(px(280.))
                                                .overflow_hidden()
                                                .whitespace_nowrap()
                                                .text_ellipsis(),
                                        )
                                        .on_click(cx.listener(move |this, _, w, cx| this.pick(target.clone(), w, cx)))
                                })),
                        ),
                )
                .into_any_element(),
        )
    }
}
