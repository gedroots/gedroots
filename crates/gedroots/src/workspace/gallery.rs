//! “Media” window (Media › Record Media…, ⌘⇧M, the images’ “+”, right-click on an event): a single entry
//! point for the media of an individual, a family, an event or a source.
//!
//! - **Attached**: the linked media, grouped by origin (the event, the cited record, the
//!   portraits…), with the details of the selected one;
//! - **Library**: link a media of the file, or a file of the folder not attached yet (it then takes
//!   the template’s name);
//! - **Import files…** (or drop files on the window): a preview shows the name and folder each file
//!   will get according to File Settings › Media, before moving (or copying) it.
//!
//! The link (where the media is attached) and the groups come from [`gedcom::media_links`]; file
//! organization from [`super::media_files`].

use super::media_files::{free_name, planned_paths, store_act_media, store_imported};
use super::settings::SettingsPage;
use super::*;
use gpui_kit::component::checkbox::Checkbox;
use rust_i18n::t;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Tab {
    Attached,
    Library,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum LibScope {
    Suggested,
    Media,
    Files,
}

/// An item chosen in the library.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Pick {
    Media(String),
    /// File of the `.ged` folder, not attached yet (relative path).
    File(String),
}

/// Files being imported, with the preview of where they will go.
pub(super) struct ImportDraft {
    files: Vec<PathBuf>,
    /// Where to attach the media; `None`: enter a new record with these images.
    host: Option<MediaHost>,
    move_files: bool,
    /// Several files: a single media with several pages.
    one_media: bool,
    /// (original file, path after organization)
    rows: Vec<(String, String)>,
}

pub(super) struct Gallery {
    pub(super) target: GalleryTarget,
    tab: Tab,
    selected: Option<(String, MediaHost)>,
    title: Option<(Entity<InputState>, Subscription)>,
    /// The library’s “Attach to” (`None`: the default choice).
    host: Option<MediaHost>,
    scope: LibScope,
    query: Entity<InputState>,
    picked: Vec<Pick>,
    import: Option<ImportDraft>,
    _sub: Subscription,
}

/// The “Move” rather than “Copy” choice is kept for the session.
static MOVE_FILES: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(true);

/// Segment of a segmented control (tabs, Copy/Move): the current choice stands out on a light
/// background, the others stay discreet.
fn segment(id: &'static str, selected: bool, label: impl Into<SharedString>) -> Stateful<Div> {
    div()
        .id(id)
        .px_2()
        .py_0p5()
        .rounded(theme::RADIUS_SM)
        .text_xs()
        .cursor_pointer()
        .when(selected, |d| d.bg(theme::bg()).text_color(theme::text()).font_weight(FontWeight::MEDIUM).shadow_xs())
        .when(!selected, |d| d.text_color(theme::muted()).hover(|d| d.text_color(theme::text())))
        .child(label.into())
}

/// Readable path of an original file: `~/Downloads/scan.jpg`.
fn display_path(p: &str) -> String {
    let home = dirs_home();
    match Path::new(p).strip_prefix(&home) {
        Ok(rest) if Path::new(p).is_absolute() => format!("~/{}", rest.to_string_lossy()),
        _ => p.to_string(),
    }
}

/// Creates the media of the imported files: a single media with several pages, or one per file. A
/// record or source image takes the source’s title.
fn create_import_media(g: &mut Gedcom, files: &[String], one: bool, host: Option<&MediaHost>) -> Vec<String> {
    let source_title = match host {
        Some(MediaHost::Act { source, key }) => Some(
            [g.record_label(source), key.page.trim().to_string()]
                .into_iter()
                .filter(|s| !s.is_empty())
                .collect::<Vec<_>>()
                .join(", "),
        ),
        Some(MediaHost::Record(x)) if g.get(x).is_some_and(|n| n.tag == "SOUR") => Some(g.record_label(x)),
        _ => None,
    };
    let groups: Vec<Vec<&String>> =
        if one && files.len() > 1 { vec![files.iter().collect()] } else { files.iter().map(|f| vec![f]).collect() };
    let mut out = Vec::new();
    for group in groups {
        let x = g.create(RecordKind::Media);
        let node = g.get_mut(&x).unwrap();
        node.children.retain(|c| c.tag != "FILE");
        for (i, path) in group.iter().enumerate() {
            let mut file = Node::with_value("FILE", path.as_str()).push(Node::with_value("FORM", mime_for(path)));
            if i == 0 {
                let title = source_title.clone().unwrap_or_else(|| title_from_file(path));
                file.set_child_val("TITL", &title);
            }
            match node.children.iter().rposition(|c| c.tag == "FILE") {
                Some(at) => node.children.insert(at + 1, file),
                None => node.insert_child(file),
            }
        }
        if source_title.is_some() {
            set_medium(node, "MANUSCRIPT");
        }
        out.push(x);
    }
    out
}

/// Label of a link: “Birth record of Paul MARCHAND”, “Birth · Paul MARCHAND”…
fn host_label(g: &Gedcom, h: &MediaHost) -> String {
    match h {
        MediaHost::Record(x) => g.record_label(x),
        MediaHost::Event { owner, ix } => {
            format!("{} · {}", g.event_title(owner, *ix).unwrap_or_default(), g.record_label(owner))
        }
        MediaHost::Act { source, key } if key.page.is_empty() => g.record_label(source),
        MediaHost::Act { source, key } => format!("{} · {}", g.record_label(source), key.page),
    }
}

/// Record opened by a click on the link.
fn host_record(h: &MediaHost) -> &str {
    match h {
        MediaHost::Record(x) | MediaHost::Event { owner: x, .. } | MediaHost::Act { source: x, .. } => x,
    }
}

impl Workspace {
    // ------------------------------------------------------------------
    // Ouverture
    // ------------------------------------------------------------------

    pub(super) fn open_gallery(&mut self, target: GalleryTarget, window: &mut Window, cx: &mut Context<Self>) {
        let query = cx.new(|cx| InputState::new(window, cx).placeholder(t!("gallery.search")));
        let sub = cx.subscribe_in(&query, window, |_, _, ev: &InputEvent, _, cx| {
            if matches!(ev, InputEvent::Change) {
                cx.notify();
            }
        });
        self.rescan_media();
        self.gallery = Some(Gallery {
            target,
            tab: Tab::Attached,
            selected: None,
            title: None,
            host: None,
            scope: LibScope::Suggested,
            query,
            picked: Vec::new(),
            import: None,
            _sub: sub,
        });
        cx.notify();
    }

    /// Media of the selected item (individual, family, source).
    pub(super) fn open_gallery_for_selection(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(doc) = &self.doc else { return };
        let Some(x) = self.selected.clone() else { return };
        if doc.ged.get(&x).is_some_and(|n| matches!(n.tag.as_str(), "INDI" | "FAM" | "SOUR")) {
            self.open_gallery(GalleryTarget::Record(x), window, cx);
        }
    }

    /// Opens the window on the import preview of the dropped files.
    pub(super) fn open_gallery_with_files(
        &mut self,
        target: GalleryTarget,
        files: Vec<PathBuf>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_gallery(target, window, cx);
        self.gallery_import(files, None, window, cx);
    }

    pub(super) fn close_gallery(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.gallery = None;
        window.focus(&self.focus, cx);
        cx.notify();
    }

    /// Esc: closes the import preview, otherwise the window.
    pub(super) fn gallery_escape(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let Some(gal) = &mut self.gallery else { return false };
        if gal.import.take().is_none() {
            self.close_gallery(window, cx);
        }
        cx.notify();
        true
    }

    /// “Attach to” choices: the cited record, the event, the person… For a register, “New record”
    /// (`None`) comes first.
    fn host_choices(&self) -> Vec<(String, Option<MediaHost>)> {
        let (Some(doc), Some(gal)) = (&self.doc, &self.gallery) else { return Vec::new() };
        let g = &doc.ged;
        let mut out: Vec<(String, Option<MediaHost>)> = Vec::new();
        if let GalleryTarget::Record(x) = &gal.target
            && g.is_register(x)
        {
            out.push((t!("gallery.new_act").into(), None));
        }
        out.extend(g.media_hosts(&gal.target).into_iter().map(|(l, h)| (l, Some(h))));
        out
    }

    // ------------------------------------------------------------------
    // Import
    // ------------------------------------------------------------------

    /// Prepares the import preview; `host`: target group (otherwise the default choice).
    fn gallery_import(
        &mut self,
        files: Vec<PathBuf>,
        host: Option<Option<MediaHost>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let files: Vec<PathBuf> = files.into_iter().filter(|p| p.is_file()).collect();
        if files.is_empty() {
            return;
        }
        // Drop on the folder of an already open media: nothing to do here.
        let host = match host {
            Some(h) => h,
            None => self.host_choices().into_iter().next().and_then(|(_, h)| h),
        };
        let one = files.len() > 1
            && matches!(&host, Some(MediaHost::Act { .. }) | None)
            && !matches!(&host, Some(MediaHost::Record(_)));
        let mut d = ImportDraft {
            files,
            host,
            move_files: MOVE_FILES.load(std::sync::atomic::Ordering::Relaxed),
            one_media: one,
            rows: Vec::new(),
        };
        d.rows = self.plan_import(&d);
        if let Some(gal) = &mut self.gallery {
            gal.import = Some(d);
        }
        window.focus(&self.focus, cx);
        cx.notify();
    }

    fn choose_import(&mut self, host: Option<Option<MediaHost>>, window: &mut Window, cx: &mut Context<Self>) {
        let rx = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: true,
            prompt: Some(t!("gallery.import").into()),
        });
        cx.spawn_in(window, async move |this, cx| {
            if let Ok(Ok(Some(paths))) = rx.await {
                let _ = this.update_in(cx, |this, window, cx| this.gallery_import(paths, host, window, cx));
            }
        })
        .detach();
    }

    /// Name each file will get, computed on a copy of the GEDCOM file.
    fn plan_import(&self, d: &ImportDraft) -> Vec<(String, String)> {
        let Some(doc) = &self.doc else { return Vec::new() };
        let files: Vec<String> = d.files.iter().map(|p| self.file_value(p)).collect();
        let mut g = doc.ged.clone();
        let media = create_import_media(&mut g, &files, d.one_media, d.host.as_ref());
        if let Some(h) = &d.host {
            for m in &media {
                let _ = g.link_media(h, m);
            }
        }
        let base = self.ged_dir();
        let mut taken = std::collections::HashSet::new();
        let mut rows = Vec::new();
        for m in &media {
            let planned = planned_paths(&g, m);
            let n = g.get(m).map(|n| n.children_with("FILE").count()).unwrap_or(0);
            for i in 0..n {
                let old = g.get(m).and_then(|n| n.children_with("FILE").nth(i)).map(|f| f.val().to_string());
                let Some(old) = old else { continue };
                let new = match (planned.get(i), &base) {
                    (Some((_, b)), Some(base)) => free_name(base, b, &mut taken),
                    (Some((_, b)), None) => b.clone(),
                    _ => old.clone(),
                };
                rows.push((display_path(&old), new));
            }
        }
        rows
    }

    /// Recomputes the planned names of the current import (settings changed).
    pub(super) fn refresh_import_plan(&mut self, cx: &mut Context<Self>) {
        self.update_draft(|_| {}, cx);
    }

    fn update_draft(&mut self, f: impl FnOnce(&mut ImportDraft), cx: &mut Context<Self>) {
        let Some(mut d) = self.gallery.as_mut().and_then(|g| g.import.take()) else { return };
        f(&mut d);
        d.rows = self.plan_import(&d);
        if let Some(gal) = &mut self.gallery {
            gal.import = Some(d);
        }
        cx.notify();
    }

    fn run_import(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(d) = self.gallery.as_mut().and_then(|g| g.import.take()) else { return };
        MOVE_FILES.store(d.move_files, std::sync::atomic::Ordering::Relaxed);
        let Some(host) = d.host.clone() else {
            // Register: the record is entered with the images at hand (named when saved).
            let Some(GalleryTarget::Record(source)) = self.gallery.as_ref().map(|g| g.target.clone()) else { return };
            self.close_gallery(window, cx);
            self.open_act(source, None, window, cx);
            if let Some((editor, _)) = &self.act_editor {
                editor.update(cx, |e, cx| e.add_files(&d.files, false, cx));
            }
            return;
        };
        let files: Vec<String> = d.files.iter().map(|p| self.file_value(p)).collect();
        let base = self.ged_dir();
        let (one, move_files) = (d.one_media, d.move_files);
        self.mutate(window, cx, |g| {
            let media = create_import_media(g, &files, one, Some(&host));
            for m in &media {
                g.link_media(&host, m)?;
            }
            let n = files.len();
            let mut msg = crate::tn!("gallery.imported", n, to = host_label(g, &host)).into_owned();
            msg.push_str(&store_imported(g, base.as_deref(), &media, move_files));
            Ok((msg, None))
        });
        self.rescan_media();
        if let Some(gal) = &mut self.gallery {
            gal.tab = Tab::Attached;
        }
        cx.notify();
    }

    // ------------------------------------------------------------------
    // Attached
    // ------------------------------------------------------------------

    fn gallery_select(&mut self, media: String, host: MediaHost, window: &mut Window, cx: &mut Context<Self>) {
        let Some(doc) = &self.doc else { return };
        let title = doc.ged.get(&media).map(|n| Media::new(n).title()).unwrap_or_default();
        let input = cx.new(|cx| InputState::new(window, cx).default_value(title));
        let m = media.clone();
        let sub = cx.subscribe_in(&input, window, move |this, input, ev: &InputEvent, window, cx| {
            if matches!(ev, InputEvent::PressEnter { .. } | InputEvent::Blur) {
                let v = input.read(cx).value().trim().to_string();
                this.set_media_title(m.clone(), v, window, cx);
            }
        });
        if let Some(gal) = &mut self.gallery {
            gal.selected = Some((media, host));
            gal.title = Some((input, sub));
        }
        cx.notify();
    }

    fn set_media_title(&mut self, media: String, title: String, window: &mut Window, cx: &mut Context<Self>) {
        let Some(doc) = &self.doc else { return };
        let Some(node) = doc.ged.get(&media) else { return };
        if title.is_empty() || Media::new(node).title() == title {
            return;
        }
        self.mutate(window, cx, |g| {
            let n = g.get_mut(&media).ok_or(t!("gallery.not_found"))?;
            // A media’s title is that of its first file (GEDCOM 7).
            match n.children.iter_mut().find(|c| c.tag == "FILE") {
                Some(f) => f.set_child_val("TITL", &title),
                None => n.set_child_val("TITL", &title),
            }
            n.touch();
            Ok((t!("gallery.title_changed").into(), None))
        });
    }

    fn gallery_detach(&mut self, media: String, host: MediaHost, window: &mut Window, cx: &mut Context<Self>) {
        self.mutate(window, cx, |g| {
            let what = g.record_label(&media);
            let from = host_label(g, &host);
            g.unlink_media(&host, &media)?;
            Ok((t!("gallery.detached", what = what, from = from).into(), None))
        });
        if let Some(gal) = &mut self.gallery {
            gal.selected = None;
            gal.title = None;
        }
        cx.notify();
    }

    fn gallery_delete(&mut self, media: String, window: &mut Window, cx: &mut Context<Self>) {
        self.mutate(window, cx, |g| {
            let what = g.record_label(&media);
            g.delete_record(&media).ok_or(t!("gallery.not_found"))?;
            Ok((t!("gallery.deleted", what = what).into(), None))
        });
        if let Some(gal) = &mut self.gallery {
            gal.selected = None;
            gal.title = None;
        }
        cx.notify();
    }

    // ------------------------------------------------------------------
    // Library
    // ------------------------------------------------------------------

    /// Keywords of the item: surname and first given name, source title…
    fn gallery_keywords(&self, g: &Gedcom, target: &GalleryTarget) -> Vec<String> {
        let x = match target {
            GalleryTarget::Record(x) | GalleryTarget::Event { owner: x, .. } => x,
        };
        match g.get(x).map(|n| n.tag.as_str()) {
            Some("INDI") => {
                let i = g.individual(x).unwrap();
                let mut k = relatives::keywords(&i.surname());
                k.extend(relatives::keywords(&i.given()).into_iter().take(1));
                k
            }
            _ => relatives::keywords(&g.record_label(x)),
        }
    }

    fn library_items(&self, cx: &App) -> Vec<Pick> {
        let (Some(doc), Some(gal)) = (&self.doc, &self.gallery) else { return Vec::new() };
        let g = &doc.ged;
        let attached: std::collections::HashSet<String> =
            g.gallery(&gal.target).into_iter().flat_map(|gr| gr.items.into_iter().map(|(m, _)| m)).collect();
        let query = sort_key(&gal.query.read(cx).value());
        let words: Vec<&str> = query.split_whitespace().collect();
        let media_text = |n: &Node| {
            let m = Media::new(n);
            sort_key(&format!("{} {}", m.title(), m.files().iter().map(|f| f.path).collect::<Vec<_>>().join(" ")))
        };
        let medias = || {
            g.records_of(RecordKind::Media)
                .filter(|n| !Media::new(n).is_web())
                .filter(|n| n.xref.as_ref().is_some_and(|x| !attached.contains(x)))
        };
        let referenced = g.referenced_files();
        let files = || self.media_files.iter().filter(|f| !referenced.contains(*f));
        let matches = |text: &str| words.iter().all(|w| text.contains(w));
        let scope = if words.is_empty() { gal.scope } else { gal.scope.max_query() };
        let mut out = Vec::new();
        match scope {
            LibScope::Suggested => {
                let keys = self.gallery_keywords(g, &gal.target);
                let Some(first) = keys.first() else { return out };
                let mut scored: Vec<(usize, String)> = medias()
                    .filter_map(|n| {
                        let t = media_text(n);
                        t.contains(first.as_str())
                            .then(|| (keys.iter().filter(|k| t.contains(k.as_str())).count(), n.xref.clone().unwrap()))
                    })
                    .collect();
                scored.sort_by_key(|b| std::cmp::Reverse(b.0));
                out.extend(scored.into_iter().take(12).map(|(_, x)| Pick::Media(x)));
                out.extend(relatives::suggest_files(&self.media_files, &referenced, &keys).into_iter().map(Pick::File));
            }
            LibScope::Media => out.extend(
                medias().filter(|n| matches(&media_text(n))).take(120).filter_map(|n| n.xref.clone()).map(Pick::Media),
            ),
            LibScope::Files => out.extend(files().filter(|f| matches(&sort_key(f))).take(120).cloned().map(Pick::File)),
        }
        out
    }

    fn toggle_pick(&mut self, p: Pick, cx: &mut Context<Self>) {
        if let Some(gal) = &mut self.gallery {
            match gal.picked.iter().position(|x| *x == p) {
                Some(i) => {
                    gal.picked.remove(i);
                }
                None => gal.picked.push(p),
            }
        }
        cx.notify();
    }

    /// Host kept for the library: the user’s choice, otherwise the first possible link (never “New
    /// record”).
    fn library_host(&self) -> Option<MediaHost> {
        let gal = self.gallery.as_ref()?;
        gal.host.clone().or_else(|| self.host_choices().into_iter().find_map(|(_, h)| h))
    }

    fn link_picked(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(host) = self.library_host() else { return };
        let picked = self.gallery.as_ref().map(|g| g.picked.clone()).unwrap_or_default();
        if picked.is_empty() {
            return;
        }
        let base = self.ged_dir();
        self.mutate(window, cx, |g| {
            let mut note = String::new();
            for p in &picked {
                match p {
                    Pick::Media(m) => g.link_media(&host, m)?,
                    Pick::File(f) => {
                        let m = g.media_for_file(f);
                        g.link_media(&host, &m)?;
                        note = store_act_media(g, base.as_deref(), &m);
                    }
                }
            }
            let n = picked.len();
            Ok((crate::tn!("gallery.linked", n, to = host_label(g, &host), note = note).into(), None))
        });
        self.rescan_media();
        if let Some(gal) = &mut self.gallery {
            gal.picked.clear();
            gal.tab = Tab::Attached;
        }
        cx.notify();
    }

    // ------------------------------------------------------------------
    // Rendu
    // ------------------------------------------------------------------

    /// Thumbnail of a media (first page) or of a file.
    fn gallery_thumb(&self, path: Option<PathBuf>, is_image: bool, w: f32, h: f32, fit: ObjectFit) -> AnyElement {
        let frame = div()
            .w_full()
            .h(px(h))
            .flex()
            .items_center()
            .justify_center()
            .overflow_hidden()
            .rounded(theme::RADIUS_SM)
            .bg(theme::viewer());
        match path.filter(|p| p.exists()) {
            Some(p) if is_image => frame.child(picture(p, w, h, fit).size_full()).into_any_element(),
            Some(p) => {
                let ext = p.extension().map(|e| e.to_string_lossy().to_uppercase()).unwrap_or_default();
                frame
                    .child(
                        v_flex()
                            .items_center()
                            .gap_1()
                            .text_xs()
                            .text_color(theme::image_text())
                            .child(icon(IconName::FileText))
                            .child(ext),
                    )
                    .into_any_element()
            }
            None => frame
                .child(
                    v_flex()
                        .items_center()
                        .gap_1()
                        .text_xs()
                        .text_color(theme::warn())
                        .child(icon(IconName::Image))
                        .child(t!("gallery.missing")),
                )
                .into_any_element(),
        }
    }

    fn media_thumb(&self, m: &Media, w: f32, h: f32, fit: ObjectFit) -> AnyElement {
        let doc = self.doc.as_ref().unwrap();
        self.gallery_thumb(pages::page_path(doc, m, 1), m.is_image(), w, h, fit)
    }

    pub(super) fn render_media_gallery(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let gal = self.gallery.as_ref()?;
        let g = &self.doc.as_ref()?.ged;
        let (title, subtitle) = match &gal.target {
            GalleryTarget::Record(x) => (t!("gallery.title").into_owned(), g.record_label(x)),
            GalleryTarget::Event { owner, ix } => (
                t!("gallery.title").into_owned(),
                format!("{} · {}", g.event_heading(owner, *ix), g.record_label(owner)),
            ),
        };
        let groups = g.gallery(&gal.target);
        let count: usize = groups.iter().map(|gr| gr.items.len()).sum();
        let importing = gal.import.as_ref();
        let header = h_flex()
            .p_3()
            .gap_3()
            .items_center()
            .border_b_1()
            .border_color(theme::border())
            .when(importing.is_some(), |d| {
                d.child(
                    Button::new("gal-back")
                        .xsmall()
                        .ghost()
                        .icon(IconName::ChevronLeft)
                        .tooltip(t!("gallery.back"))
                        .on_click(cx.listener(|this, _, _, cx| {
                            if let Some(gal) = &mut this.gallery {
                                gal.import = None;
                            }
                            cx.notify();
                        })),
                )
            })
            .child(
                v_flex()
                    .min_w_0()
                    .child(div().font_weight(FontWeight::SEMIBOLD).child(match importing {
                        Some(d) => crate::tn!("gallery.import_files", d.files.len()).into_owned(),
                        None => title,
                    }))
                    .child(div().text_xs().text_color(theme::muted()).truncate().child(subtitle)),
            )
            .child(div().flex_1())
            .when(importing.is_none(), |d| {
                d.child(
                    h_flex()
                        .gap_1()
                        .p_0p5()
                        .rounded(theme::RADIUS_SM)
                        .bg(theme::surface())
                        .child(
                            segment("gal-tab-attached", gal.tab == Tab::Attached, t!("gallery.attached", n = count))
                                .on_click(cx.listener(|this, _, _, cx| {
                                    if let Some(g) = &mut this.gallery {
                                        g.tab = Tab::Attached;
                                    }
                                    cx.notify();
                                })),
                        )
                        .child(segment("gal-tab-library", gal.tab == Tab::Library, t!("gallery.library")).on_click(
                            cx.listener(|this, _, w, cx| {
                                if let Some(g) = &mut this.gallery {
                                    g.tab = Tab::Library;
                                    g.query.update(cx, |s, cx| s.focus(w, cx));
                                }
                                cx.notify();
                            }),
                        )),
                )
                .child(div().flex_1())
            })
            .child(
                Button::new("gal-close")
                    .xsmall()
                    .ghost()
                    .icon(IconName::Close)
                    .tooltip(t!("gallery.close"))
                    .on_click(cx.listener(|this, _, w, cx| this.close_gallery(w, cx))),
            );
        let (body, footer) = match (importing, gal.tab) {
            (Some(d), _) => (self.render_import(d, cx), self.render_import_footer(d, cx)),
            (None, Tab::Attached) => (self.render_attached(gal, groups, cx), self.render_gallery_footer(gal, cx)),
            (None, Tab::Library) => (self.render_library(gal, cx), self.render_gallery_footer(gal, cx)),
        };
        let entity = cx.entity().downgrade();
        Some(
            div()
                .absolute()
                .inset_0()
                .flex()
                .justify_center()
                .items_center()
                .bg(theme::scrim())
                .child(
                    v_flex()
                        .id("media-gallery")
                        .occlude()
                        .w(px(940.))
                        .max_w(relative(0.94))
                        .h(px(600.))
                        .max_h(relative(0.9))
                        .rounded(theme::RADIUS_LG)
                        .border_1()
                        .border_color(theme::border_strong())
                        .bg(theme::panel())
                        .shadow_lg()
                        .overflow_hidden()
                        // Files dropped anywhere: import preview.
                        .drag_over::<ExternalPaths>(|d, _, _, _| d.border_color(theme::accent()))
                        .on_drop(move |paths: &ExternalPaths, window, cx| {
                            let files = paths.paths().to_vec();
                            let _ = entity.update(cx, |this, cx| this.gallery_import(files, None, window, cx));
                        })
                        .child(header)
                        .child(div().flex_1().min_h_0().flex().child(body))
                        .child(footer),
                )
                .into_any_element(),
        )
    }

    fn render_attached(&self, gal: &Gallery, groups: Vec<GalleryGroup>, cx: &mut Context<Self>) -> AnyElement {
        let doc = self.doc.as_ref().unwrap();
        let g = &doc.ged;
        // “Import…” slot: on each group of an event or a source; only on the first one for an
        // individual or a family.
        let slots_everywhere = match &gal.target {
            GalleryTarget::Event { .. } => true,
            GalleryTarget::Record(x) => g.get(x).is_some_and(|n| n.tag == "SOUR"),
        };
        let register = matches!(&gal.target, GalleryTarget::Record(x) if g.is_register(x));
        let cols = if gal.selected.is_some() { 4 } else { 5 };
        let selected = gal.selected.clone();
        let mut list = v_flex().gap_4();
        for (gi, gr) in groups.into_iter().enumerate() {
            let slot = gr.host.clone().filter(|_| slots_everywhere || gi == 0);
            if gr.items.is_empty() && slot.is_none() {
                continue;
            }
            let tiles = gr.items.iter().enumerate().map(|(ti, (m, host))| {
                let Some(node) = g.get(m) else { return div().into_any_element() };
                let media = Media::new(node);
                let sel = selected.as_ref().is_some_and(|(sm, sh)| sm == m && sh == host);
                let off_template = planned_paths(g, m).iter().any(|(a, b)| a != b);
                let pages = media.page_count();
                let (m1, h1) = (m.clone(), host.clone());
                v_flex()
                    .id(SharedString::from(format!("gal-{gi}-{ti}")))
                    .relative()
                    .p_1()
                    .gap_1()
                    .rounded(theme::RADIUS_SM)
                    .border_1()
                    .border_color(if sel { theme::accent() } else { theme::border() })
                    .when(sel, |d| d.bg(theme::accent_soft()))
                    .cursor_pointer()
                    .hover(|d| d.border_color(theme::accent()))
                    .child(self.media_thumb(&media, 168., 96., ObjectFit::Cover))
                    .when(pages > 1, |d| {
                        d.child(
                            div()
                                .absolute()
                                .top(px(8.))
                                .right(px(8.))
                                .px_1()
                                .rounded(theme::RADIUS_SM)
                                .bg(theme::image_label_bg())
                                .text_xs()
                                .text_color(theme::image_text())
                                .child(t!("gallery.pages", n = pages)),
                        )
                    })
                    .child(
                        v_flex()
                            .px_1()
                            .text_xs()
                            .child(div().font_weight(FontWeight::MEDIUM).truncate().child(media.title()))
                            .child(if off_template {
                                div().text_color(theme::warn()).child(t!("gallery.off_template"))
                            } else {
                                div().text_color(theme::muted()).truncate().child(
                                    media
                                        .file()
                                        .map(|f| f.rsplit('/').next().unwrap_or(f).to_string())
                                        .unwrap_or_default(),
                                )
                            }),
                    )
                    .on_click(cx.listener(move |this, _, w, cx| this.gallery_select(m1.clone(), h1.clone(), w, cx)))
                    .into_any_element()
            });
            let slot_el = slot.map(|host| {
                let (h1, h2) = (host.clone(), host);
                let entity = cx.entity().downgrade();
                v_flex()
                    .id(SharedString::from(format!("gal-slot-{gi}")))
                    .h(px(140.))
                    .items_center()
                    .justify_center()
                    .gap_1()
                    .rounded(theme::RADIUS_SM)
                    .border_1()
                    .border_dashed()
                    .border_color(theme::border_strong())
                    .text_xs()
                    .text_color(theme::muted())
                    .cursor_pointer()
                    .hover(|d| d.text_color(theme::text()).border_color(theme::accent()))
                    .drag_over::<ExternalPaths>(|d, _, _, _| d.bg(theme::accent_soft()).border_color(theme::accent()))
                    .on_drop(move |paths: &ExternalPaths, window, cx| {
                        cx.stop_propagation();
                        let (files, h) = (paths.paths().to_vec(), h1.clone());
                        let _ = entity.update(cx, |this, cx| this.gallery_import(files, Some(Some(h)), window, cx));
                    })
                    .child(icon(IconName::Plus))
                    .child(t!("gallery.import_more"))
                    .on_click(cx.listener(move |this, _, w, cx| this.choose_import(Some(Some(h2.clone())), w, cx)))
                    .into_any_element()
            });
            list = list.child(
                v_flex()
                    .gap_2()
                    .when(!gr.title.is_empty(), |d| {
                        d.child(
                            h_flex()
                                .gap_2()
                                .items_baseline()
                                .child(label_caps(gr.title.clone()))
                                .when(!gr.subtitle.is_empty(), |d| {
                                    d.child(div().text_xs().text_color(theme::muted()).child(gr.subtitle.clone()))
                                }),
                        )
                    })
                    .child(div().grid().grid_cols(cols).gap_3().children(tiles).children(slot_el)),
            );
        }
        if register {
            let entity = cx.entity().downgrade();
            list = list.child(
                v_flex().gap_2().child(label_caps(t!("gallery.new_act_caps"))).child(
                    h_flex()
                        .id("gal-new-act")
                        .h(px(56.))
                        .justify_center()
                        .gap_2()
                        .rounded(theme::RADIUS_SM)
                        .border_1()
                        .border_dashed()
                        .border_color(theme::border_strong())
                        .text_xs()
                        .text_color(theme::muted())
                        .cursor_pointer()
                        .hover(|d| d.text_color(theme::text()).border_color(theme::accent()))
                        .drag_over::<ExternalPaths>(|d, _, _, _| {
                            d.bg(theme::accent_soft()).border_color(theme::accent())
                        })
                        .on_drop(move |paths: &ExternalPaths, window, cx| {
                            cx.stop_propagation();
                            let files = paths.paths().to_vec();
                            let _ = entity.update(cx, |this, cx| this.gallery_import(files, Some(None), window, cx));
                        })
                        .child(icon(IconName::Plus))
                        .child(t!("gallery.enter_act"))
                        .on_click(cx.listener(|this, _, w, cx| this.choose_import(Some(None), w, cx))),
                ),
            );
        }
        h_flex()
            .size_full()
            .items_stretch()
            .child(div().id("gal-attached").flex_1().min_w_0().p_4().bg(theme::bg()).overflow_y_scroll().child(list))
            .children(selected.and_then(|(m, h)| self.render_media_inspector(gal, m, h, cx)))
            .into_any_element()
    }

    fn render_media_inspector(
        &self,
        gal: &Gallery,
        media: String,
        host: MediaHost,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let g = &self.doc.as_ref()?.ged;
        let node = g.get(&media)?;
        let m = Media::new(node);
        let planned = planned_paths(g, &media);
        let off_template = planned.iter().any(|(a, b)| a != b);
        // Crop: a photo attached directly to the person.
        let crop_owner = match (&gal.target, &host) {
            (GalleryTarget::Record(x), MediaHost::Record(h))
                if x == h && g.get(x).is_some_and(|n| n.tag == "INDI") && m.is_image() =>
            {
                Some(x.clone())
            }
            _ => None,
        };
        let open_rec = host_record(&host).to_string();
        let (m1, m2, m3, m4, m5, m6) =
            (media.clone(), media.clone(), media.clone(), media.clone(), media.clone(), media.clone());
        let h1 = host.clone();
        let me = cx.entity().downgrade();
        Some(
            v_flex()
                .id("gal-inspector")
                .w(px(280.))
                .flex_none()
                .p_4()
                .gap_3()
                .border_l_1()
                .border_color(theme::border())
                .overflow_y_scroll()
                .child(self.media_thumb(&m, 248., 184., ObjectFit::Contain))
                .children(gal.title.as_ref().map(|(input, _)| {
                    v_flex()
                        .gap_1()
                        .child(div().text_xs().text_color(theme::muted()).child(t!("editor.title")))
                        .child(Input::new(input).small())
                }))
                .child(
                    v_flex()
                        .gap_1()
                        .child(div().text_xs().text_color(theme::muted()).child(t!("gallery.attached_to")))
                        .child(
                            div()
                                .id("gal-host")
                                .text_color(theme::accent())
                                .cursor_pointer()
                                .child(host_label(g, &host))
                                .on_click(cx.listener(move |this, _, w, cx| {
                                    this.close_gallery(w, cx);
                                    this.goto(open_rec.clone(), w, cx)
                                })),
                        ),
                )
                .child(
                    v_flex()
                        .gap_1()
                        .child(div().text_xs().text_color(theme::muted()).child(if m.page_count() > 1 {
                            t!("gallery.files_count", n = m.page_count()).into_owned()
                        } else {
                            t!("editor.file").into_owned()
                        }))
                        .children(m.files().into_iter().map(|f| mono(f.path.to_string()).text_xs()))
                        .when(off_template, |d| {
                            d.child(
                                h_flex()
                                    .gap_2()
                                    .items_center()
                                    .text_xs()
                                    .text_color(theme::warn())
                                    .child(t!("gallery.off_template"))
                                    .child(
                                        Button::new("gal-rename")
                                            .xsmall()
                                            .ghost()
                                            .label(t!("gallery.rename"))
                                            .on_click(cx.listener(move |this, _, _, cx| {
                                                this.open_rename_media(m1.clone(), cx)
                                            })),
                                    ),
                            )
                        }),
                )
                .child(div().flex_1())
                .child(
                    h_flex()
                        .gap_2()
                        .flex_wrap()
                        .items_center()
                        .child(Button::new("gal-detach").xsmall().label(t!("gallery.detach")).on_click(
                            cx.listener(move |this, _, w, cx| this.gallery_detach(m3.clone(), h1.clone(), w, cx)),
                        ))
                        .when_some(crop_owner, |d, owner| {
                            d.child(Button::new("gal-crop").xsmall().label(t!("gallery.crop")).on_click(cx.listener(
                                move |this, _, w, cx| {
                                    this.close_gallery(w, cx);
                                    this.start_crop_for(owner.clone(), m4.clone(), w, cx)
                                },
                            )))
                        })
                        .child(
                            Button::new("gal-more")
                                .xsmall()
                                .ghost()
                                .icon(IconName::Ellipsis)
                                .tooltip(t!("gallery.more"))
                                .dropdown_menu(move |menu, _, _| {
                                    let (me0, me1, me2, me3) = (me.clone(), me.clone(), me.clone(), me.clone());
                                    let (o, a, b, c) = (m2.clone(), m5.clone(), m6.clone(), m5.clone());
                                    menu.item(PopupMenuItem::new(t!("context.open_details")).on_click(
                                        move |_, w, cx| {
                                            let o = o.clone();
                                            let _ = me0.update(cx, |this, cx| {
                                                this.close_gallery(w, cx);
                                                this.open_in(o, CenterTab::Form, w, cx)
                                            });
                                        },
                                    ))
                                    .separator()
                                    .item(PopupMenuItem::new(label::add_pages()).on_click(move |_, w, cx| {
                                        let a = a.clone();
                                        let _ = me1.update(cx, |this, cx| this.choose_pages(a, w, cx));
                                    }))
                                    .item(PopupMenuItem::new(t!("cmd.rename_media_files")).on_click(move |_, _, cx| {
                                        let b = b.clone();
                                        let _ = me2.update(cx, |this, cx| this.open_rename_media(b, cx));
                                    }))
                                    .separator()
                                    .item(
                                        PopupMenuItem::new(t!("gallery.delete")).on_click(move |_, w, cx| {
                                            let c = c.clone();
                                            let _ = me3.update(cx, |this, cx| this.gallery_delete(c, w, cx));
                                        }),
                                    )
                                }),
                        ),
                )
                .into_any_element(),
        )
    }

    fn render_library(&self, gal: &Gallery, cx: &mut Context<Self>) -> AnyElement {
        let doc = self.doc.as_ref().unwrap();
        let g = &doc.ged;
        let items = self.library_items(cx);
        let referenced_count = {
            let r = g.referenced_files();
            self.media_files.iter().filter(|f| !r.contains(*f)).count()
        };
        let media_count = g.records_of(RecordKind::Media).filter(|n| !Media::new(n).is_web()).count();
        let scope_button = |id: &'static str, scope: LibScope, label: String| {
            segment(id, gal.scope == scope, label).on_click(cx.listener(move |this, _, _, cx| {
                if let Some(g) = &mut this.gallery {
                    g.scope = scope;
                }
                cx.notify();
            }))
        };
        let tiles = items.into_iter().enumerate().map(|(i, p)| {
            let picked = gal.picked.contains(&p);
            let (thumb, title, sub) = match &p {
                Pick::Media(x) => {
                    let m = Media::new(g.get(x).unwrap());
                    let sub = match m.file().and_then(|f| f.rsplit_once('/')) {
                        Some((d, _)) => t!("gallery.media_in", dir = d).into_owned(),
                        None => t!("menu.media").into_owned(),
                    };
                    (self.media_thumb(&m, 168., 96., ObjectFit::Cover), m.title(), sub)
                }
                Pick::File(f) => {
                    let path = pages::file_path(doc, f);
                    let name = f.rsplit('/').next().unwrap_or(f).to_string();
                    let dir = f
                        .rsplit_once('/')
                        .map(|(d, _)| t!("gallery.file_in", dir = d).into_owned())
                        .unwrap_or(t!("editor.file").into_owned());
                    (self.gallery_thumb(path, is_image_file(f), 168., 96., ObjectFit::Cover), name, dir)
                }
            };
            let p1 = p.clone();
            v_flex()
                .id(SharedString::from(format!("lib-{i}")))
                .relative()
                .p_1()
                .gap_1()
                .rounded(theme::RADIUS_SM)
                .border_1()
                .border_color(if picked { theme::accent() } else { theme::border() })
                .when(picked, |d| d.bg(theme::accent_soft()))
                .cursor_pointer()
                .hover(|d| d.border_color(theme::accent()))
                .child(thumb)
                .child(div().absolute().top(px(8.)).left(px(8.)).child(
                    Checkbox::new(SharedString::from(format!("lib-check-{i}"))).checked(picked).on_click(cx.listener(
                        {
                            let p = p.clone();
                            move |this, _, _, cx| this.toggle_pick(p.clone(), cx)
                        },
                    )),
                ))
                .child(
                    v_flex()
                        .px_1()
                        .text_xs()
                        .child(div().font_weight(FontWeight::MEDIUM).truncate().child(title))
                        .child(div().text_color(theme::muted()).truncate().child(sub)),
                )
                .on_click(cx.listener(move |this, _, _, cx| this.toggle_pick(p1.clone(), cx)))
                .into_any_element()
        });
        let tiles: Vec<AnyElement> = tiles.collect();
        let empty = tiles.is_empty();
        v_flex()
            .size_full()
            .p_4()
            .gap_3()
            .bg(theme::bg())
            .child(Input::new(&gal.query).small().cleanable(true))
            .child(
                h_flex()
                    .self_start()
                    .gap_1()
                    .p_0p5()
                    .rounded(theme::RADIUS_SM)
                    .bg(theme::surface())
                    .child(scope_button("lib-suggested", LibScope::Suggested, t!("gallery.suggested").into_owned()))
                    .child(scope_button(
                        "lib-media",
                        LibScope::Media,
                        t!("gallery.all_media", n = media_count).into_owned(),
                    ))
                    .child(scope_button(
                        "lib-files",
                        LibScope::Files,
                        t!("gallery.unattached", n = referenced_count).into_owned(),
                    )),
            )
            .child(
                div()
                    .id("gal-library")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .when(empty, |d| {
                        d.flex()
                            .items_center()
                            .justify_center()
                            .text_color(theme::muted())
                            .child(t!("picker.no_result"))
                    })
                    .when(!empty, |d| d.child(div().grid().grid_cols(5).gap_3().children(tiles))),
            )
            .into_any_element()
    }

    /// “Attach to” button: list of possible links, check mark on the current choice.
    fn host_button(
        &self,
        id: &'static str,
        current: Option<&MediaHost>,
        on_pick: impl Fn(&mut Workspace, Option<MediaHost>, &mut Context<Workspace>) + 'static,
        with_new_act: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let g = &self.doc.as_ref().unwrap().ged;
        let choices: Vec<(String, Option<MediaHost>)> =
            self.host_choices().into_iter().filter(|(_, h)| with_new_act || h.is_some()).collect();
        let label = match current {
            Some(h) => choices
                .iter()
                .find(|(_, c)| c.as_ref() == Some(h))
                .map(|(l, _)| l.clone())
                .unwrap_or_else(|| host_label(g, h)),
            None => t!("gallery.new_act").into(),
        };
        if choices.len() <= 1 {
            return div().text_sm().child(label).into_any_element();
        }
        let current = current.cloned();
        let me = cx.entity().downgrade();
        let on_pick = Rc::new(on_pick);
        Button::new(id)
            .small()
            .outline()
            .label(label)
            .dropdown_caret(true)
            .dropdown_menu(move |menu, _, _| {
                choices.iter().fold(menu, |menu, (l, h)| {
                    let (me, h, on_pick) = (me.clone(), h.clone(), on_pick.clone());
                    menu.item(PopupMenuItem::new(l.clone()).checked(h == current).on_click(move |_, _, cx| {
                        let h = h.clone();
                        let _ = me.update(cx, |this, cx| on_pick(this, h, cx));
                    }))
                })
            })
            .into_any_element()
    }

    fn render_gallery_footer(&self, gal: &Gallery, cx: &mut Context<Self>) -> AnyElement {
        let library = gal.tab == Tab::Library;
        let n = gal.picked.len();
        let host = self.library_host();
        h_flex()
            .p_3()
            .gap_2()
            .items_center()
            .border_t_1()
            .border_color(theme::border())
            .child(
                Button::new("gal-import")
                    .small()
                    .icon(IconName::Upload)
                    .label(t!("gallery.import_files_button"))
                    .on_click(cx.listener(|this, _, w, cx| this.choose_import(None, w, cx))),
            )
            .child(div().flex_1())
            .when(library, |d| {
                d.child(div().text_sm().text_color(theme::muted()).child(t!("gallery.attach_to")))
                    .child(self.host_button(
                        "gal-lib-host",
                        host.as_ref(),
                        |this, h, cx| {
                            if let Some(g) = &mut this.gallery {
                                g.host = h;
                            }
                            cx.notify();
                        },
                        false,
                        cx,
                    ))
                    .child(
                        Button::new("gal-link")
                            .small()
                            .primary()
                            .disabled(n == 0)
                            .label(if n == 0 {
                                t!("gallery.link").into_owned()
                            } else {
                                crate::tn!("gallery.link_n", n).into_owned()
                            })
                            .on_click(cx.listener(|this, _, w, cx| this.link_picked(w, cx))),
                    )
            })
            .into_any_element()
    }

    fn render_import(&self, d: &ImportDraft, cx: &mut Context<Self>) -> AnyElement {
        let g = &self.doc.as_ref().unwrap().ged;
        let unnamed = g.media_naming().is_empty();
        let row = |label: std::borrow::Cow<'static, str>, content: AnyElement| {
            h_flex()
                .gap_4()
                .items_center()
                .child(div().w(px(120.)).text_right().text_color(theme::muted()).child(label))
                .child(content)
        };
        let move_files = d.move_files;
        let mode = h_flex()
            .gap_1()
            .p_0p5()
            .rounded(theme::RADIUS_SM)
            .bg(theme::surface())
            .child(
                segment("imp-copy", !move_files, t!("gallery.copy"))
                    .on_click(cx.listener(|this, _, _, cx| this.update_draft(|d| d.move_files = false, cx))),
            )
            .child(
                segment("imp-move", move_files, t!("gallery.move"))
                    .on_click(cx.listener(|this, _, _, cx| this.update_draft(|d| d.move_files = true, cx))),
            );
        let n = d.files.len();
        let pages = Checkbox::new("imp-one")
            .checked(d.one_media)
            .label(t!("gallery.one_media", n = n).into_owned())
            .on_click(cx.listener(|this, checked: &bool, _, cx| {
                let c = *checked;
                this.update_draft(|d| d.one_media = c, cx)
            }));
        let new_act = d.host.is_none();
        let doc = self.doc.as_ref().unwrap();
        v_flex()
            .id("gal-import-body")
            .size_full()
            .p_4()
            .gap_4()
            .bg(theme::bg())
            .overflow_y_scroll()
            .child(row(
                t!("gallery.attach_to"),
                self.host_button(
                    "imp-host",
                    d.host.as_ref(),
                    |this, h, cx| this.update_draft(|d| d.host = h, cx),
                    true,
                    cx,
                ),
            ))
            .when(!new_act, |el| {
                el.child(row(
                    t!("gallery.files"),
                    h_flex().gap_4().items_center().child(mode).when(n > 1, |e| e.child(pages)).into_any_element(),
                ))
            })
            .child(
                v_flex().rounded(theme::RADIUS_SM).border_1().border_color(theme::border()).children(
                    d.files
                        .iter()
                        .zip(d.rows.iter().chain(std::iter::repeat(&(String::new(), String::new()))))
                        .enumerate()
                        .map(|(i, (path, (old, new)))| {
                            let thumb = self.gallery_thumb(
                                Some(path.clone()),
                                is_image_file(&path.to_string_lossy()),
                                48.,
                                48.,
                                ObjectFit::Cover,
                            );
                            let (dir, name) = match new.rsplit_once('/') {
                                Some((d, n)) => (format!("{d}/"), n.to_string()),
                                None => (String::new(), new.clone()),
                            };
                            h_flex()
                                .gap_3()
                                .px_3()
                                .py_2()
                                .items_center()
                                .when(i > 0, |e| e.border_t_1().border_color(theme::border()))
                                .child(div().w(px(48.)).flex_none().child(thumb))
                                .child(
                                    div()
                                        .flex_1()
                                        .min_w_0()
                                        .text_xs()
                                        .font_family("monospace")
                                        .text_color(theme::muted())
                                        .child(old.clone()),
                                )
                                .when(!new_act, |e| {
                                    e.child(icon(IconName::ArrowRight).text_color(theme::muted())).child(
                                        h_flex()
                                            .flex_1()
                                            .min_w_0()
                                            .text_xs()
                                            .font_family("monospace")
                                            .child(div().text_color(theme::muted()).child(dir))
                                            .child(div().font_weight(FontWeight::SEMIBOLD).child(name)),
                                    )
                                })
                        }),
                ),
            )
            .when(unnamed && !new_act && doc.path.is_some(), |el| {
                el.child(div().text_xs().text_color(theme::warn()).child(t!("gallery.no_naming")))
            })
            .into_any_element()
    }

    fn render_import_footer(&self, d: &ImportDraft, cx: &mut Context<Self>) -> AnyElement {
        let new_act = d.host.is_none();
        let cancel =
            Button::new("imp-cancel").small().label(t!("common.cancel")).on_click(cx.listener(|this, _, _, cx| {
                if let Some(gal) = &mut this.gallery {
                    gal.import = None;
                }
                cx.notify();
            }));
        let go = Button::new("imp-go")
            .small()
            .primary()
            .label(if new_act { t!("gallery.enter_act_go") } else { t!("gallery.import") })
            .on_click(cx.listener(|this, _, w, cx| this.run_import(w, cx)));
        // Button order: action on the left on Windows, on the right elsewhere.
        let buttons = if cfg!(target_os = "windows") {
            h_flex().gap_2().child(go).child(cancel)
        } else {
            h_flex().gap_2().child(cancel).child(go)
        };
        h_flex()
            .p_3()
            .gap_2()
            .items_center()
            .border_t_1()
            .border_color(theme::border())
            .child(Button::new("imp-settings").small().ghost().label(t!("gallery.naming_settings")).on_click(
                cx.listener(|this, _, w, cx| {
                    this.close_gallery(w, cx);
                    this.open_settings_page(SettingsPage::Media, w, cx)
                }),
            ))
            .child(div().flex_1())
            .child(buttons)
            .into_any_element()
    }
}

impl LibScope {
    /// A search typed in “Suggested” searches all media.
    fn max_query(self) -> LibScope {
        match self {
            LibScope::Suggested => LibScope::Media,
            s => s,
        }
    }
}

fn is_image_file(f: &str) -> bool {
    let l = f.to_ascii_lowercase();
    [".jpg", ".jpeg", ".png", ".gif", ".webp", ".bmp", ".tif", ".tiff"].iter().any(|e| l.ends_with(e))
}
