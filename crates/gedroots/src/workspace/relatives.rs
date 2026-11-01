//! Ajout et liaison de proches, sélecteur de personne, rattachement de médias.

use super::*;
use gpui_kit::component::IconName;

/// Ce que fera le choix dans le sélecteur.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PickPurpose {
    /// `other` devient le `rel` de `indi`.
    Relative { indi: String, rel: Relation },
    /// La personne choisie devient un parent de la famille.
    FamilySpouse { fam: String },
    /// La personne choisie devient un enfant de la famille.
    FamilyChild { fam: String },
    /// Le média est rattaché à l'enregistrement choisi.
    MediaTo { media: String },
    /// La personne choisie est identifiée sur la photo, dans la zone `crop`.
    TagPerson { media: String, crop: Crop },
}

impl PickPurpose {
    fn title(&self, g: &Gedcom) -> String {
        match self {
            PickPurpose::Relative { indi, rel } => {
                format!("Choisir {} de {}", rel_article(*rel), g.record_label(indi))
            }
            PickPurpose::FamilySpouse { fam } => format!("Choisir un parent pour {}", g.record_label(fam)),
            PickPurpose::FamilyChild { fam } => format!("Choisir un enfant pour {}", g.record_label(fam)),
            PickPurpose::MediaTo { media } => format!("Rattacher « {} » à…", g.record_label(media)),
            PickPurpose::TagPerson { .. } => "Qui est sur cette zone de la photo ?".into(),
        }
    }
}

fn rel_article(rel: Relation) -> &'static str {
    match rel {
        Relation::Father => "le père",
        Relation::Mother => "la mère",
        Relation::Spouse => "un conjoint",
        Relation::Child => "un enfant",
        Relation::Sibling => "un frère ou une sœur",
    }
}

pub struct Picker {
    pub input: Entity<InputState>,
    pub purpose: PickPurpose,
    _sub: Subscription,
}

/// Fichiers image/PDF du dossier du `.ged` (chemins relatifs, séparés par `/`).
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

/// Fichiers non encore rattachés dont le nom contient les mots-clés (nom, prénom…),
/// du plus pertinent au moins pertinent.
fn suggest_files(files: &[String], referenced: &std::collections::HashSet<String>, keys: &[String]) -> Vec<String> {
    let Some(first) = keys.first() else { return vec![] };
    let mut scored: Vec<(usize, &String)> = files
        .iter()
        .filter(|f| !referenced.contains(*f))
        .filter_map(|f| {
            let k = sort_key(f);
            // Le premier mot-clé (nom de famille, titre…) est obligatoire.
            if !k.contains(first.as_str()) {
                return None;
            }
            Some((keys.iter().filter(|w| k.contains(w.as_str())).count(), f))
        })
        .collect();
    scored.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(b.1)));
    scored.into_iter().take(6).map(|(_, f)| f.clone()).collect()
}

fn keywords(s: &str) -> Vec<String> {
    sort_key(s).split(|c: char| !c.is_alphanumeric()).filter(|w| w.chars().count() >= 3).map(str::to_string).collect()
}

impl Workspace {
    /// Applique une modification au document avec point d'annulation.
    /// `f` renvoie le message d'état et, éventuellement, l'enregistrement à ouvrir.
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
                self.set_status(e, true);
            }
        }
        cx.notify();
    }

    pub(super) fn add_relative(&mut self, indi: String, rel: Relation, window: &mut Window, cx: &mut Context<Self>) {
        self.mutate(window, cx, |g| {
            let name = g.record_label(&indi);
            let x = g.add_relative(&indi, rel, None)?;
            Ok((format!("{} de {name} ajouté(e) : complétez sa fiche", rel.label_fr()), Some(x)))
        });
    }

    pub(super) fn add_family_child(&mut self, fam: String, window: &mut Window, cx: &mut Context<Self>) {
        self.mutate(window, cx, |g| {
            let f = g.family(&fam).ok_or("famille introuvable")?;
            let surname = f.husband().and_then(|h| g.individual(h)).map(|h| h.surname()).unwrap_or_default();
            let x = g.create_individual("", &surname, "U");
            g.add_child_to_family(&fam, &x)?;
            Ok(("Enfant ajouté : complétez sa fiche".into(), Some(x)))
        });
    }

    fn unlink(&mut self, indi: String, fam: String, window: &mut Window, cx: &mut Context<Self>) {
        self.mutate(window, cx, |g| {
            let (who, from) = (g.record_label(&indi), g.record_label(&fam));
            g.unlink_from_family(&indi, &fam);
            Ok((format!("{who} détaché(e) de {from}"), None))
        });
    }

    pub(super) fn open_picker(&mut self, purpose: PickPurpose, window: &mut Window, cx: &mut Context<Self>) {
        let input =
            cx.new(|cx| InputState::new(window, cx).placeholder("Nom, prénom… (critères : né:1850 lieu:troyes)"));
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

    /// Candidats du sélecteur (au plus 80).
    fn picker_results(&self, cx: &App) -> Vec<String> {
        let (Some(p), Some(doc)) = (&self.picker, &self.doc) else { return vec![] };
        let g = &doc.ged;
        let text = p.input.read(cx).value().to_string();
        let q = IndividualQuery::parse(&text);
        let exclude: Option<&str> = match &p.purpose {
            PickPurpose::Relative { indi, .. } => Some(indi),
            _ => None,
        };
        // Pour un lien de parenté : sexe compatible, et les dates plausibles en premier.
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
        // Proches déjà liés (parents, frères et sœurs, conjoints, enfants) : inutile de les proposer.
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
                Some(i.xref()) != exclude && !close.contains(i.xref()) && Some(i.sex()) != wrong_sex && q.matches(g, i)
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
        if let PickPurpose::MediaTo { .. } = p.purpose {
            // Sources et familles aussi, d'après le texte libre.
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
            // D'abord les personnes déjà liées à la photo sans zone, en dernier celles déjà situées.
            let links = g.media_links(media);
            let rank = |x: &str| match links.iter().find(|l| l.owner == x) {
                Some(l) if l.crop.is_none() => 0,
                None => 1,
                Some(_) => 2,
            };
            people.sort_by_key(|i| rank(i.xref()));
        }
        out.extend(people.into_iter().map(|i| i.xref().to_string()));
        out.truncate(80);
        out
    }

    fn pick(&mut self, chosen: String, window: &mut Window, cx: &mut Context<Self>) {
        let Some(p) = self.picker.take() else { return };
        window.focus(&self.focus, cx);
        self.mutate(window, cx, |g| {
            let who = g.record_label(&chosen);
            match p.purpose {
                PickPurpose::Relative { indi, rel } => {
                    g.link_relative(&indi, rel, &chosen)?;
                    Ok((
                        format!("{who} lié(e) comme {} de {}", rel.label_fr().to_lowercase(), g.record_label(&indi)),
                        None,
                    ))
                }
                PickPurpose::FamilySpouse { fam } => {
                    g.set_spouse(&fam, &chosen)?;
                    Ok((format!("{who} ajouté(e) comme parent"), None))
                }
                PickPurpose::FamilyChild { fam } => {
                    g.add_child_to_family(&fam, &chosen)?;
                    Ok((format!("{who} ajouté(e) comme enfant"), None))
                }
                PickPurpose::MediaTo { media } => {
                    g.attach_media(&chosen, &media)?;
                    Ok((format!("Média rattaché à {who}"), None))
                }
                PickPurpose::TagPerson { media, crop } => {
                    g.tag_person(&chosen, &media, crop)?;
                    Ok((format!("{who} identifié(e) sur la photo"), None))
                }
            }
        });
    }

    // ------------------------------------------------------------------
    // Médias
    // ------------------------------------------------------------------

    /// Chemin à écrire dans `FILE` : relatif au dossier du `.ged` si possible.
    pub(super) fn file_value(&self, path: &std::path::Path) -> String {
        let base = self.doc.as_ref().and_then(|d| d.path.as_ref()).and_then(|p| p.parent());
        match base.and_then(|b| path.strip_prefix(b).ok()) {
            Some(rel) => rel.components().map(|c| c.as_os_str().to_string_lossy()).collect::<Vec<_>>().join("/"),
            None => path.to_string_lossy().to_string(),
        }
    }

    /// Crée (ou réutilise) les médias des fichiers et les rattache à `record` (si donné).
    pub(super) fn attach_files(
        &mut self,
        record: Option<String>,
        files: Vec<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if files.is_empty() {
            return;
        }
        let outside = files.iter().filter(|f| std::path::Path::new(f).is_absolute()).count();
        self.mutate(window, cx, |g| {
            for f in &files {
                let m = g.media_for_file(f);
                if let Some(r) = &record {
                    g.attach_media(r, &m)?;
                }
            }
            let n = files.len();
            let mut msg = match &record {
                Some(r) => format!("{n} fichier{} rattaché{} à {}", plural(n), plural(n), g.record_label(r)),
                None => format!("{n} média{} créé{}", plural(n), plural(n)),
            };
            if outside > 0 {
                msg.push_str(" — attention : fichier hors du dossier du .ged, chemin absolu enregistré");
            }
            Ok((msg, None))
        });
    }

    pub(super) fn choose_files(&mut self, record: String, window: &mut Window, cx: &mut Context<Self>) {
        let rx = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: true,
            prompt: Some("Rattacher".into()),
        });
        cx.spawn_in(window, async move |this, cx| {
            if let Ok(Ok(Some(paths))) = rx.await {
                let _ = this.update_in(cx, |this, window, cx| {
                    let files = paths.iter().map(|p| this.file_value(p)).collect();
                    this.attach_files(Some(record), files, window, cx);
                });
            }
        })
        .detach();
    }

    pub(super) fn rescan_media(&mut self) {
        let path = self.doc.as_ref().and_then(|d| d.path.clone());
        self.media_files = Rc::new(scan_media_files(path.as_deref()));
    }

    // ------------------------------------------------------------------
    // Rendu
    // ------------------------------------------------------------------

    /// Section « Ajouter / lier » du panneau de détail, selon la vue.
    pub(super) fn render_actions(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let doc = self.doc.as_ref()?;
        let key = self.selected.clone()?;
        let g = &doc.ged;
        let section = || v_flex().p_4().gap_2().border_b_1().border_color(theme::border());
        let el = match self.view {
            View::Individuals => {
                let i = g.individual(&key)?;
                // Détacher : famille d'origine et unions.
                let mut detach: Vec<(String, String)> = i
                    .famc()
                    .into_iter()
                    .map(|fam| (fam.to_string(), format!("des parents ({})", g.record_label(fam))))
                    .collect();
                for (fam, spouse) in i.unions(g) {
                    let label = match spouse {
                        Some(s) => format!("de l'union avec {}", g.record_label(&s)),
                        None => match g.family(&fam).and_then(|f| f.children().first().map(|c| g.record_label(c))) {
                            Some(kid) => format!("de la famille de {kid}"),
                            None => "d'une famille vide".into(),
                        },
                    };
                    detach.push((fam, label));
                }
                let mut media_keys = keywords(&i.surname());
                media_keys.extend(keywords(&i.given()).into_iter().take(1));
                // Ajouter ou lier un proche : menu Individu.
                if detach.is_empty() && !self.has_suggestions(&media_keys) {
                    return None;
                }
                let open = self.show_detach;
                section()
                    .when(!detach.is_empty() && !open, |d| {
                        d.child(h_flex().child(
                            Button::new("show-detach").xsmall().ghost().label("Détacher de…").on_click(cx.listener(
                                |this, _, _, cx| {
                                    this.show_detach = true;
                                    cx.notify();
                                },
                            )),
                        ))
                    })
                    .when(!detach.is_empty() && open, |d| {
                        d.child(label_caps("Détacher")).children(detach.into_iter().map(|(fam, label)| {
                            let x = key.clone();
                            let id = SharedString::from(format!("unlink-{fam}"));
                            h_flex().child(
                                Button::new(id).xsmall().ghost().icon(IconName::Close).label(label).on_click(
                                    cx.listener(move |this, _, w, cx| this.unlink(x.clone(), fam.clone(), w, cx)),
                                ),
                            )
                        }))
                    })
                    .child(self.render_media_drop(&key, media_keys, cx))
            }
            View::Families => {
                let fam = g.family(&key)?;
                let members: Vec<(&'static str, String)> = [("Parent", fam.husband()), ("Parent", fam.wife())]
                    .into_iter()
                    .filter_map(|(r, x)| x.map(|x| (r, x.to_string())))
                    .chain(fam.children().into_iter().map(|c| ("Enfant", c.to_string())))
                    .collect();
                // Ajouter un enfant, lier un parent : menu Famille.
                section()
                    .child(section_header("Membres", {
                        let f = key.clone();
                        super::add::plus_button("add-member")
                            .tooltip("Ajouter un enfant ou un parent")
                            .on_click(cx.listener(move |this, _, _, cx| this.open_relatives_dialog(f.clone(), cx)))
                    }))
                    .children(members.into_iter().map(|(role, x)| {
                        let (who, fam) = (x.clone(), key.clone());
                        h_flex()
                            .gap_2()
                            .items_center()
                            .child(div().w(px(52.)).text_xs().text_color(theme::muted()).child(role))
                            .child(
                                div()
                                    .flex_1()
                                    .overflow_hidden()
                                    .whitespace_nowrap()
                                    .text_ellipsis()
                                    .child(g.record_label(&x)),
                            )
                            .child(
                                Button::new(SharedString::from(format!("fam-unlink-{x}")))
                                    .xsmall()
                                    .ghost()
                                    .icon(IconName::Close)
                                    .tooltip("Retirer de la famille")
                                    .on_click(
                                        cx.listener(move |this, _, w, cx| this.unlink(who.clone(), fam.clone(), w, cx)),
                                    ),
                            )
                    }))
                    .child(self.render_media_drop(&key, vec![], cx))
            }
            View::Sources => {
                let keys = keywords(&g.record_label(&key));
                if !self.has_suggestions(&keys) {
                    return None;
                }
                section().child(self.render_media_drop(&key, keys, cx))
            }
            _ => return None,
        };
        Some(el.into_any_element())
    }

    fn has_suggestions(&self, keys: &[String]) -> bool {
        let Some(doc) = &self.doc else { return false };
        !keys.is_empty() && !suggest_files(&self.media_files, &doc.ged.referenced_files(), keys).is_empty()
    }

    /// Fichiers du dossier qui semblent correspondre à l'élément. On peut aussi
    /// déposer des fichiers sur le panneau, ou passer par le menu (Rattacher…).
    fn render_media_drop(&self, record: &str, keys: Vec<String>, cx: &mut Context<Self>) -> impl IntoElement {
        let g = &self.doc.as_ref().unwrap().ged;
        let referenced = g.referenced_files();
        let suggestions = suggest_files(&self.media_files, &referenced, &keys);
        v_flex().gap_1().when(!suggestions.is_empty(), |d| {
            d.child(label_caps("Fichiers suggérés")).children(suggestions.into_iter().map(|f| {
                let (rec, file) = (record.to_string(), f.clone());
                h_flex()
                    .gap_2()
                    .items_center()
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .text_xs()
                            .child(f.clone()),
                    )
                    .child(
                        Button::new(SharedString::from(format!("suggest-{f}")))
                            .xsmall()
                            .ghost()
                            .icon(IconName::Plus)
                            .label("Rattacher")
                            .on_click(cx.listener(move |this, _, w, cx| {
                                this.attach_files(Some(rec.clone()), vec![file.clone()], w, cx)
                            })),
                    )
            }))
        })
    }

    /// Fenêtre « Fichiers du dossier sans média » (menu Média) : images et PDF
    /// du dossier du `.ged` qu'aucun média ne référence encore.
    pub(super) fn render_unreferenced_files(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !self.orphans_open {
            return None;
        }
        let g = &self.doc.as_ref()?.ged;
        let referenced = g.referenced_files();
        let missing: Vec<String> = self.media_files.iter().filter(|f| !referenced.contains(*f)).cloned().collect();
        let all = missing.clone();
        let close = |this: &mut Self, cx: &mut Context<Self>| {
            this.orphans_open = false;
            cx.notify();
        };
        Some(
            div()
                .absolute()
                .inset_0()
                .flex()
                .justify_center()
                .items_start()
                .pt(px(90.))
                .bg(gpui_kit::hsla(0., 0., 0., 0.45))
                .child(
                    v_flex()
                        .id("orphans")
                        .occlude()
                        .w(px(560.))
                        .max_h(px(600.))
                        .rounded_lg()
                        .border_1()
                        .border_color(theme::border_strong())
                        .bg(theme::panel())
                        .shadow_lg()
                        .on_mouse_down_out(cx.listener(move |this, _, _, cx| close(this, cx)))
                        .on_key_down(cx.listener(move |this, ev: &KeyDownEvent, _, cx| {
                            if ev.keystroke.key == "escape" {
                                close(this, cx);
                            }
                        }))
                        .child(
                            h_flex()
                                .p_3()
                                .gap_2()
                                .items_center()
                                .border_b_1()
                                .border_color(theme::border())
                                .child(
                                    div()
                                        .flex_1()
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .child(format!("Fichiers sans média · {}", missing.len())),
                                )
                                .when(!missing.is_empty(), |d| {
                                    d.child(
                                        Button::new("import-all").xsmall().primary().label("Tout importer").on_click(
                                            cx.listener(move |this, _, w, cx| {
                                                this.attach_files(None, all.clone(), w, cx)
                                            }),
                                        ),
                                    )
                                })
                                .child(
                                    Button::new("orphans-close")
                                        .xsmall()
                                        .ghost()
                                        .icon(IconName::Close)
                                        .on_click(cx.listener(move |this, _, _, cx| close(this, cx))),
                                ),
                        )
                        .child(
                            v_flex()
                                .id("orphans-list")
                                .max_h(px(520.))
                                .overflow_y_scroll()
                                .p_3()
                                .gap_1()
                                .when(missing.is_empty(), |d| {
                                    d.child(
                                        div()
                                            .text_color(theme::muted())
                                            .child("Tous les fichiers du dossier ont déjà un média."),
                                    )
                                })
                                .children(missing.into_iter().take(500).map(|f| {
                                    let file = f.clone();
                                    h_flex()
                                        .gap_2()
                                        .items_center()
                                        .child(
                                            div()
                                                .flex_1()
                                                .min_w_0()
                                                .overflow_hidden()
                                                .whitespace_nowrap()
                                                .text_ellipsis()
                                                .text_xs()
                                                .child(f.clone()),
                                        )
                                        .child(
                                            Button::new(SharedString::from(format!("import-{f}")))
                                                .xsmall()
                                                .ghost()
                                                .icon(IconName::Plus)
                                                .tooltip("Créer le média")
                                                .on_click(cx.listener(move |this, _, w, cx| {
                                                    this.attach_files(None, vec![file.clone()], w, cx)
                                                })),
                                        )
                                })),
                        ),
                )
                .into_any_element(),
        )
    }

    /// Fenêtre de choix d'une personne (ou d'un enregistrement), au-dessus de l'interface.
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
                .bg(gpui_kit::hsla(0., 0., 0., 0.45))
                .child(
                    v_flex()
                        .id("picker")
                        .occlude()
                        .w(px(520.))
                        .max_h(px(560.))
                        .rounded_lg()
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
                                    d.child(div().px_3().py_2().text_color(theme::muted()).child("Aucun résultat"))
                                })
                                .children(results.into_iter().enumerate().map(|(n, x)| {
                                    let kind = g.get(&x).and_then(|r| RecordKind::from_tag(&r.tag));
                                    let meta = match g.individual(&x) {
                                        Some(i) => {
                                            let place =
                                                i.birth().and_then(|e| e.place().map(short_place)).unwrap_or_default();
                                            [i.years(), place]
                                                .into_iter()
                                                .filter(|s| !s.is_empty())
                                                .collect::<Vec<_>>()
                                                .join(" · ")
                                        }
                                        None => kind.map(|k| k.label_fr().to_string()).unwrap_or_default(),
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
                                            div()
                                                .flex_1()
                                                .overflow_hidden()
                                                .whitespace_nowrap()
                                                .text_ellipsis()
                                                .child(g.record_label(&x)),
                                        )
                                        .child(mono(meta))
                                        .on_click(cx.listener(move |this, _, w, cx| this.pick(target.clone(), w, cx)))
                                })),
                        )
                        .child(
                            div()
                                .px_3()
                                .py_2()
                                .border_t_1()
                                .border_color(theme::border())
                                .child(mono("Entrée : premier résultat · Échap : fermer")),
                        ),
                )
                .into_any_element(),
        )
    }
}

fn plural(n: usize) -> &'static str {
    if n > 1 { "s" } else { "" }
}
