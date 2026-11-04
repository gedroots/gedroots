//! Commands of the menus and the palette, connected to the workspace.
//!
//! A command is only declared when it applies (individual selected, file open…): macOS menus and
//! the palette then grey out or hide on their own those that are not available.

use super::relatives::PickPurpose;
use super::tree::TreeMode;
use super::*;
use crate::editor::NamePart;
use gpui_kit::component::command::{Command, CommandItem, CommandState};
use rust_i18n::t;

/// Declares an action handler on the root element.
macro_rules! on {
    ($d:expr, $cx:expr, $action:ty, |$this:ident, $w:ident, $c:ident| $body:expr) => {
        $d.on_action($cx.listener(
            |$this: &mut Workspace, _: &$action, $w: &mut Window, $c: &mut Context<Workspace>| {
                // Last commands, for Help › Report an Issue (action names only).
                crate::report::record_command(<$action as Action>::name_for_type());
                #[allow(unused_variables)]
                let _ = (&$w, &$c);
                $body
            },
        ))
    };
}

/// Palette de commandes ouverte.
pub struct Palette {
    state: Entity<CommandState>,
    items: Vec<CommandItem>,
}

impl Workspace {
    /// Registers the handlers of the commands that apply in the current state.
    pub(super) fn bind_actions(&self, d: Div, _window: &mut Window, cx: &mut Context<Self>) -> Div {
        let doc = self.doc.as_ref();
        let g = doc.map(|d| &d.ged);
        let has_doc = doc.is_some();
        let sel = self.selected.clone();
        let sel_kind = sel.as_ref().and_then(|x| g?.get(x)).and_then(|n| RecordKind::from_tag(&n.tag));
        let indi = sel.clone().filter(|_| sel_kind == Some(RecordKind::Individual));
        let media = sel.clone().filter(|_| sel_kind == Some(RecordKind::Media));
        let parents = indi.as_ref().and_then(|x| Some(g?.parents(x))).unwrap_or((None, None));
        let photo = media.as_ref().and_then(|x| self.photo_file(x)).filter(|_| self.photo_draw.is_none());
        let root = g.and_then(|g| g.root());
        let can_undo = doc.is_some_and(|d| !d.history.is_empty());
        let can_redo = doc.is_some_and(|d| !d.future.is_empty());
        let individuals = has_doc && self.view == View::Individuals && self.act_editor.is_none();
        let attachable = matches!(sel_kind, Some(RecordKind::Individual | RecordKind::Family | RecordKind::Source));
        let web_linkable = sel.as_ref().is_some_and(|x| g.is_some_and(|g| g.can_have_web_links(x)));

        let d = on!(d, cx, OpenFile, |this, w, cx| this.open_file(&OpenFile, w, cx));
        let d = d.on_action(cx.listener(|this, a: &OpenRecent, w, cx| this.open_tree(a.path.clone(), w, cx)));
        let d = d.on_action(cx.listener(|_, _: &CloseWindow, w, _| w.remove_window()));
        let d = d.on_action(cx.listener(|_, _: &MinimizeWindow, w, _| w.minimize_window()));
        let d = d.on_action(cx.listener(|_, _: &ZoomWindow, w, _| w.zoom_window()));
        let d = on!(d, cx, ClearRecent, |_this, w, cx| crate::recent::clear(cx));
        let d = on!(d, cx, ImportFile, |this, w, cx| this.import_file(w, cx));
        let d = on!(d, cx, NewFile, |this, w, cx| this.new_file(&NewFile, w, cx));
        let d = on!(d, cx, CommandPalette, |this, w, cx| this.toggle_palette(w, cx));
        let d = on!(d, cx, ReportIssue, |this, w, cx| this.open_report(w, cx));
        let d = on!(d, cx, OpenSampleTree, |_this, w, cx| cx.defer(crate::open_sample));
        if !has_doc {
            return d;
        }
        let sample = doc.is_some_and(|d| d.sample);
        let d = if sample { d } else { on!(d, cx, SaveFile, |this, w, cx| this.save_file(&SaveFile, w, cx)) };
        let d = on!(d, cx, TakeTour, |this, w, cx| this.open_tour_chooser(cx));
        let d = on!(d, cx, OpenFileSettings, |this, w, cx| this.open_settings(w, cx));
        let d = on!(d, cx, ReviewProposals, |this, w, cx| this.open_review(cx));
        let d = on!(d, cx, AcceptProposal, |this, w, cx| this.accept_proposal(w, cx));
        let d = on!(d, cx, RejectProposal, |this, w, cx| this.reject_proposal(cx));
        let d = on!(d, cx, ConnectAgent, |this, w, cx| this.open_agent_dialog(cx));
        let d = on!(d, cx, MobileDevices, |this, w, cx| this.open_mobile_dialog(w, cx));
        let d = if sample { d } else { on!(d, cx, SaveFileAs, |this, w, cx| this.save_file_as(w, cx)) };
        // On the Books page, File › Export… exports the book shown, as PDF.
        let d = on!(d, cx, ExportFile, |this, w, cx| {
            if this.view == View::Books && this.selected.is_some() {
                this.export_book(w, cx)
            } else {
                this.export(w, cx)
            }
        });
        let d = d.when(self.has_version_history(), |d| {
            let d = on!(d, cx, SaveVersion, |this, w, cx| this.open_save_version(w, cx));
            on!(d, cx, BrowseVersions, |this, w, cx| this.open_version_history(w, cx))
        });
        let d = d.when(self.can_show_record_history(), |d| {
            on!(d, cx, RecordHistory, |this, w, cx| this.open_record_history_of_selection(w, cx))
        });
        let d = d.when(self.changed_since_opened(), |d| {
            on!(d, cx, RevertToOpened, |this, w, cx| this.revert_to_opened(w, cx))
        });
        let d = on!(d, cx, EnterRecord, |this, w, cx| this.enter_act(w, cx));
        let d = on!(d, cx, FocusSearch, |this, w, cx| this.filter.update(cx, |f, cx| f.focus(w, cx)));
        let d = on!(d, cx, ClearSearch, |this, w, cx| this.set_query(String::new(), w, cx));

        // Creations: switch to the right list, then create.
        let d = on!(d, cx, NewIndividual, |this, w, cx| this.create_in(View::Individuals, w, cx));
        // New source: it opens on an empty record (a source without a record is still possible:
        // “‹”).
        let d = on!(d, cx, NewSource, |this, w, cx| {
            this.create_in(View::Sources, w, cx);
            if let Some(s) = this.selected.clone() {
                this.open_act(s, None, w, cx);
            }
        });
        let d = on!(d, cx, NewMedia, |this, w, cx| this.create_in(View::Media, w, cx));
        let d = on!(d, cx, NewRepository, |this, w, cx| this.create_in(View::Repositories, w, cx));
        let d = on!(d, cx, NewNote, |this, w, cx| this.create_in(View::Notes, w, cx));
        let d = on!(d, cx, ToggleSidebar, |this, w, cx| this.toggle_column(Drawer::Sidebar, cx));
        let d = if self.has_inspector() {
            on!(d, cx, ToggleInspector, |this, w, cx| this.toggle_column(Drawer::Inspector, cx))
        } else {
            d
        };

        // Vues.
        let d = on!(d, cx, ShowIndividuals, |this, w, cx| this.show_view(View::Individuals, w, cx));
        let d = on!(d, cx, ShowSources, |this, w, cx| this.show_view(View::Sources, w, cx));
        let d = on!(d, cx, ShowPlaces, |this, w, cx| this.show_view(View::Places, w, cx));
        let d = on!(d, cx, ShowMedia, |this, w, cx| this.show_view(View::Media, w, cx));
        let d = on!(d, cx, ShowRepositories, |this, w, cx| this.show_view(View::Repositories, w, cx));
        let d = on!(d, cx, ShowNotes, |this, w, cx| this.show_view(View::Notes, w, cx));
        let d = on!(d, cx, ShowResearch, |this, w, cx| this.show_view(View::Research, w, cx));
        let d = on!(d, cx, ShowQuality, |this, w, cx| this.show_view(View::Quality, w, cx));
        // The place shown on the Places page.
        let d = d.when(g.is_some(), |d| on!(d, cx, FindDuplicatePlaces, |this, _w, cx| this.find_duplicate_places(cx)));
        let place = self.selected_place();
        let unlocated = place
            .as_ref()
            .and_then(|p| g?.places().into_iter().find(|x| x.name == *p))
            .is_some_and(|p| p.coords().is_none());
        let d = d.when(place.is_some(), |d| {
            let d = on!(d, cx, MergeWithPlace, |this, w, cx| this.ask_merge_place_with(w, cx));
            on!(d, cx, PlaceOnMap, |this, w, cx| {
                if let Some(p) = this.selected_place() {
                    this.place_by_hand(p, cx)
                }
            })
        });
        let d = d.when(unlocated, |d| {
            on!(d, cx, LocatePlace, |this, w, cx| {
                if let Some(p) = this.selected_place() {
                    this.start_locate(p, cx)
                }
            })
        });
        // Research: the commands of the chosen lead.
        let d = match self.selected_lead().map(|l| l.state) {
            Some(state) => {
                let d = on!(d, cx, NoteSearch, |this, w, cx| {
                    if let Some(k) = this.selected.clone() {
                        this.open_search_dialog(k, w, cx)
                    }
                });
                let d = on!(d, cx, CopyAgentRequest, |this, w, cx| {
                    if let Some(k) = this.selected.clone() {
                        this.copy_agent_request(k, cx)
                    }
                });
                let ignored = state == gedcom::research_log::TaskState::Ignored;
                if ignored {
                    on!(d, cx, ResumeLead, |this, w, cx| {
                        if let Some(k) = this.selected.clone() {
                            this.set_lead_ignored(k, false, w, cx)
                        }
                    })
                } else {
                    on!(d, cx, IgnoreLead, |this, w, cx| {
                        if let Some(k) = this.selected.clone() {
                            this.set_lead_ignored(k, true, w, cx)
                        }
                    })
                }
            }
            None => d,
        };
        let d = on!(d, cx, ShowStats, |this, w, cx| this.show_view(View::Stats, w, cx));
        let d = d.when(has_doc, |d| on!(d, cx, NewBook, |this, w, cx| this.new_book(w, cx)));
        let d = d.when(self.view == View::Books && sel.is_some(), |d| {
            on!(d, cx, EditRecord, |this, w, cx| this.edit_book(w, cx))
        });
        let d = on!(d, cx, ToggleIgnoredIssues, |this, _w, cx| this.toggle_ignored_issues(cx));
        let d = on!(d, cx, ToggleRelativeEvents, |this, _w, cx| this.toggle_relative_events(cx));
        // Individuals and places: the record is the right panel, not a tab.
        let d =
            d.when(!self.view.has_panel(), |d| on!(d, cx, ShowForm, |this, w, cx| this.show_tab(CenterTab::Form, cx)));
        let d = on!(d, cx, ShowUnreferencedFiles, |this, w, cx| {
            this.open_quality(gedcom::quality::Category::Files, w, cx)
        });

        d.when(self.history.can_back(), |d| on!(d, cx, GoBack, |this, w, cx| this.go_back(w, cx)))
            .when(self.history.can_forward(), |d| on!(d, cx, GoForward, |this, w, cx| this.go_forward(w, cx)))
            .when(can_undo, |d| on!(d, cx, Undo, |this, w, cx| this.undo(&Undo, w, cx)))
            .when(can_redo, |d| on!(d, cx, Redo, |this, w, cx| this.redo(&Redo, w, cx)))
            .when(
                self.editor.as_ref().is_some_and(|ed| {
                    ed.error.is_none() && (!ed.fields.is_empty() || !ed.pickers.is_empty() || ed.long_text.is_some())
                }),
                |d| on!(d, cx, EditRecord, |this, w, cx| this.start_edit(w, cx)),
            )
            .when(sel.is_some(), |d| {
                on!(d, cx, DeleteRecord, |this, w, cx| {
                    if !this.view.has_panel() {
                        this.tab = CenterTab::Form;
                    }
                    this.delete_selected(w, cx);
                })
            })
            .when(sel_kind.is_some(), |d| {
                on!(d, cx, CopyId, |this, w, cx| {
                    if let Some(x) = this.selected.clone() {
                        this.copy_id(&x, cx)
                    }
                })
            })
            .when(sel.is_some(), |d| {
                on!(d, cx, CopyName, |this, w, cx| {
                    if let Some(x) = this.selected.clone() {
                        this.copy_name(&x, cx)
                    }
                })
            })
            .when(self.view != View::Places, |d| on!(d, cx, ShowRaw, |this, w, cx| this.show_tab(CenterTab::Raw, cx)))
            .when(!self.flag_label().is_empty(), |d| {
                on!(d, cx, ToggleFlagged, |this, w, cx| {
                    this.only_flagged = !this.only_flagged;
                    this.rebuild_rows(cx);
                    cx.notify();
                })
            })
            .when(root.is_some(), |d| on!(d, cx, GoToRoot, |this, w, cx| this.goto_root(w, cx)))
            .when(self.can_save_search(cx), |d| on!(d, cx, SaveSearch, |this, w, cx| this.save_search(w, cx)))
            .when(self.active_saved(cx).is_some(), |d| {
                let d = on!(d, cx, RenameSearch, |this, w, cx| {
                    if let Some(i) = this.active_saved(cx) {
                        this.start_rename_search(i, w, cx)
                    }
                });
                on!(d, cx, DeleteSearch, |this, w, cx| {
                    if let Some(i) = this.active_saved(cx) {
                        this.delete_saved(i, w, cx)
                    }
                })
            })
            // Record entry: what the record will write.
            .when(self.act_editor.is_some(), |d| {
                on!(d, cx, ToggleRecordPreview, |this, _w, cx| {
                    if let Some((editor, _)) = &this.act_editor {
                        editor.update(cx, |e, cx| e.toggle_preview(cx));
                    }
                })
            })
            // Zoom of the large image shown (act, proposal to review, media).
            .when(self.active_zoom(cx).is_some(), |d| {
                let d = on!(d, cx, ZoomIn, |this, w, cx| this.with_zoom(w, cx, |z| z.zoom_in()));
                let d = on!(d, cx, ZoomOut, |this, w, cx| this.with_zoom(w, cx, |z| z.zoom_out()));
                let d = on!(d, cx, ZoomToFit, |this, w, cx| this.with_zoom(w, cx, |z| z.zoom_to_fit()));
                on!(d, cx, ActualSize, |this, w, cx| this.with_zoom(w, cx, |z| z.actual_size()))
            })
            // Chart and filters of the individuals list.
            .when(individuals, |d| {
                let d = on!(d, cx, ShowTree, |this, w, cx| this.show_tab(CenterTab::Tree, cx));
                let d = on!(d, cx, TreeChart, |this, w, cx| this.set_tree_mode(TreeMode::Chart, cx));
                let d = on!(d, cx, TreeFan, |this, w, cx| this.set_tree_mode(TreeMode::Fan, cx));
                // The free chart loads its generations on its own; the fan chart is adjustable.
                let d = d.when(self.tree_mode == TreeMode::Fan, |d| {
                    let d = on!(d, cx, MoreGenerations, |this, w, cx| this.step_generations(1, cx));
                    on!(d, cx, FewerGenerations, |this, w, cx| this.step_generations(-1, cx))
                });
                let d = on!(d, cx, FilterMen, |this, w, cx| this.toggle_filter("sex:m", w, cx));
                let d = on!(d, cx, FilterWomen, |this, w, cx| this.toggle_filter("sex:f", w, cx));
                let d = on!(d, cx, FilterAncestors, |this, w, cx| this.toggle_filter("sosa", w, cx));
                let d = on!(d, cx, FilterUnsourced, |this, w, cx| this.toggle_filter("without:source", w, cx));
                let d = on!(d, cx, FilterNoParents, |this, w, cx| this.toggle_filter("without:parents", w, cx));
                let d = on!(d, cx, FilterNoDate, |this, w, cx| this.toggle_filter("without:date", w, cx));
                let d = on!(d, cx, FilterNoMarriage, |this, w, cx| this.toggle_filter("without:marriage", w, cx));
                on!(d, cx, FilterWithMedia, |this, w, cx| this.toggle_filter("with:media", w, cx))
            })
            // Selected individual.
            .when(indi.is_some(), |d| {
                let d = on!(d, cx, CenterTree, |this, w, cx| {
                    if let Some(x) = this.selected.clone() {
                        this.tree_mode = TreeMode::Chart;
                        this.center_chart_on(x, w, cx)
                    }
                });
                let d = d
                    .when(parents.0.is_none(), |d| {
                        let d = on!(d, cx, AddFather, |this, w, cx| this.relative(Relation::Father, false, w, cx));
                        on!(d, cx, LinkFather, |this, w, cx| this.relative(Relation::Father, true, w, cx))
                    })
                    .when(parents.1.is_none(), |d| {
                        let d = on!(d, cx, AddMother, |this, w, cx| this.relative(Relation::Mother, false, w, cx));
                        on!(d, cx, LinkMother, |this, w, cx| this.relative(Relation::Mother, true, w, cx))
                    });
                let d = on!(d, cx, AddSpouse, |this, w, cx| this.relative(Relation::Spouse, false, w, cx));
                let d = on!(d, cx, AddChild, |this, w, cx| this.relative(Relation::Child, false, w, cx));
                let d = on!(d, cx, AddSibling, |this, w, cx| this.relative(Relation::Sibling, false, w, cx));
                let d = on!(d, cx, LinkSpouse, |this, w, cx| this.relative(Relation::Spouse, true, w, cx));
                let d = on!(d, cx, LinkChild, |this, w, cx| this.relative(Relation::Child, true, w, cx));
                let d = on!(d, cx, LinkSibling, |this, w, cx| this.relative(Relation::Sibling, true, w, cx));
                let d = on!(d, cx, KinshipWith, |this, w, cx| this.ask_kinship(w, cx));
                let d = on!(d, cx, MergeWithPerson, |this, w, cx| this.ask_merge_with(w, cx));
                let d = on!(d, cx, FindDuplicates, |this, _w, cx| this.find_duplicates(cx));
                let d = on!(d, cx, SearchAncestors, |this, w, cx| this.search_kin(Kin::Ancestors, w, cx));
                let d = on!(d, cx, SearchDescendants, |this, w, cx| this.search_kin(Kin::Descendants, w, cx));
                d.on_action(cx.listener(|this, a: &SearchWeb, _, cx| {
                    if let Some(x) = this.selected.clone() {
                        this.search_web(a.index, &x, cx)
                    }
                }))
            })
            .when(indi.is_some(), |d| {
                on!(d, cx, AddEvent, |this, w, cx| {
                    if let Some(x) = this.selected.clone() {
                        this.open_event_dialog(x, None, w, cx)
                    }
                })
            })
            .when(indi.is_some() && self.editor.as_ref().is_some_and(|ed| ed.names.is_some()), |d| {
                let d = on!(d, cx, AddOtherName, |this, w, cx| this.add_to_name(None, w, cx));
                let d = on!(d, cx, AddNickname, |this, w, cx| this.add_to_name(Some(NamePart::Nickname), w, cx));
                let d = on!(d, cx, AddNameTitle, |this, w, cx| this.add_to_name(Some(NamePart::Title), w, cx));
                let d = on!(d, cx, AddNameParticle, |this, w, cx| this.add_to_name(Some(NamePart::Particle), w, cx));
                on!(d, cx, AddNameSuffix, |this, w, cx| this.add_to_name(Some(NamePart::Suffix), w, cx))
            })
            .when(web_linkable, |d| {
                on!(d, cx, AddWebLink, |this, w, cx| {
                    if let Some(x) = this.selected.clone() {
                        this.open_weblink_dialog(x, None, w, cx)
                    }
                })
            })
            .when(attachable, |d| on!(d, cx, MediaGallery, |this, w, cx| this.open_gallery_for_selection(w, cx)))
            .when(photo.is_some(), |d| on!(d, cx, TagPerson, |this, w, cx| this.tag_person_on_photo(cx)))
            .when(media.is_some(), |d| {
                on!(d, cx, AttachMediaTo, |this, w, cx| {
                    if let Some(media) = this.selected.clone() {
                        this.open_picker(PickPurpose::MediaTo { media }, w, cx)
                    }
                })
            })
            .when(media.is_some() && g.is_some_and(|g| !g.media_naming().is_empty()), |d| {
                on!(d, cx, RenameMediaFiles, |this, w, cx| {
                    if let Some(media) = this.selected.clone() {
                        this.open_rename_media(media, cx)
                    }
                })
            })
    }

    /// (Existing) image of the displayed page of a media.
    /// Zoom of the large image shown, if any: the act's in the act editor, the proposal's on the
    /// review page, otherwise the displayed media's.
    fn active_zoom(&self, cx: &App) -> Option<crate::viewer::Zoom> {
        let z = if let Some((editor, _)) = &self.act_editor {
            editor.read(cx).zoom().clone()
        } else if self.review.open {
            self.review.zoom.clone()
        } else {
            let media = self.selected.as_ref().filter(|x| self.photo_file(x).is_some());
            if media.is_none() || self.tab != CenterTab::Form {
                return None;
            }
            self.photo_zoom.clone()
        };
        z.is_shown().then_some(z)
    }

    fn with_zoom(&self, window: &mut Window, cx: &App, f: impl FnOnce(&crate::viewer::Zoom)) {
        if let Some(z) = self.active_zoom(cx) {
            f(&z);
            window.refresh();
        }
    }

    pub(super) fn photo_file(&self, media: &str) -> Option<PathBuf> {
        let doc = self.doc.as_ref()?;
        let m = Media::new(doc.ged.get(media)?);
        let page = self.current_page(media);
        let f = m.files().get(page - 1).copied()?;
        let image = f.form.is_some_and(|t| t.starts_with("image/")) || mime_for(f.path).starts_with("image/");
        super::pages::page_path(doc, &m, page).filter(|p| image && p.exists())
    }

    pub(super) fn tag_person_on_photo(&mut self, cx: &mut Context<Self>) {
        let Some(media) = self.selected.clone() else { return };
        let Some(path) = self.photo_file(&media) else { return };
        self.tab = CenterTab::Form;
        self.start_draw(media, None, &path, cx);
    }

    fn save_file_as(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(doc) = &self.doc else { return };
        let (dir, name) = match doc.path.as_deref().filter(|_| !doc.imported) {
            Some(p) => (
                p.parent().map(|p| p.to_path_buf()).unwrap_or_else(dirs_home),
                p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default(),
            ),
            None => save_as_default(doc.path.as_deref()),
        };
        let rx = cx.prompt_for_new_path(&dir, Some(&name));
        cx.spawn_in(window, async move |this, cx| {
            if let Ok(Ok(Some(p))) = rx.await {
                let _ = this.update_in(cx, |this, _, cx| this.write_to(p, Versioning::Now, cx));
            }
        })
        .detach();
    }

    fn create_in(&mut self, view: View, window: &mut Window, cx: &mut Context<Self>) {
        self.show_view(view, window, cx);
        self.create_record(window, cx);
        cx.notify();
    }

    /// Switches lists, leaving record entry if needed.
    pub(super) fn show_view(&mut self, view: View, window: &mut Window, cx: &mut Context<Self>) {
        self.act_editor = None;
        self.review.open = false;
        self.set_view(view, window, cx);
        cx.notify();
    }

    fn show_tab(&mut self, tab: CenterTab, cx: &mut Context<Self>) {
        if tab == CenterTab::Raw {
            self.raw_open = true;
        }
        self.tab = tab;
        cx.notify();
    }

    /// Closes the raw GEDCOM tab and goes back to the list's first view.
    pub(super) fn close_raw(&mut self, cx: &mut Context<Self>) {
        self.raw_open = false;
        if self.tab == CenterTab::Raw {
            self.tab = CenterTab::Form;
        }
        cx.notify();
    }

    fn set_tree_mode(&mut self, mode: TreeMode, cx: &mut Context<Self>) {
        self.tab = CenterTab::Tree;
        self.tree_mode = mode;
        if mode == TreeMode::Fan && self.tree_gens < 5 {
            self.tree_gens = 5;
        }
        cx.notify();
    }

    fn step_generations(&mut self, delta: i32, cx: &mut Context<Self>) {
        let range = self.tree_mode.generations();
        let gens = self.tree_gens.clamp(*range.start(), *range.end()) as i32 + delta;
        self.tree_gens = (gens.max(0) as u32).clamp(*range.start(), *range.end());
        self.tab = CenterTab::Tree;
        cx.notify();
    }

    pub(super) fn set_query(&mut self, query: String, window: &mut Window, cx: &mut Context<Self>) {
        self.filter.update(cx, |f, cx| {
            let end = query.len();
            f.set_value(query, window, cx);
            f.set_selected_range(end..end, cx);
        });
        self.tokenize_search(window, cx);
        self.rebuild_rows(cx);
        cx.notify();
    }

    /// Searches the ancestors or descendants of the displayed person.
    fn search_kin(&mut self, kin: Kin, window: &mut Window, cx: &mut Context<Self>) {
        let Some(x) = self.selected.clone() else { return };
        if self.view != View::Individuals {
            self.show_view(View::Individuals, window, cx);
        }
        self.set_query(format!("{}:@{x}@ ", kin.key()), window, cx);
        self.filter.update(cx, |f, cx| f.focus(window, cx));
    }

    /// Opens in the browser the search for person `xref` on site `index` of the file settings.
    pub(super) fn search_web(&mut self, index: usize, xref: &str, cx: &mut Context<Self>) {
        let Some(g) = self.doc.as_ref().map(|d| &d.ged) else { return };
        if let Some(site) = g.search_sites().get(index) {
            cx.open_url(&g.search_url(&site.url, xref));
        }
    }

    fn toggle_filter(&mut self, token: &str, window: &mut Window, cx: &mut Context<Self>) {
        let current = self.filter.read(cx).value().to_string();
        self.set_query(toggle_token(&current, token), window, cx);
    }

    /// Adds a new relative, or opens the picker to link an existing person.
    fn relative(&mut self, rel: Relation, existing: bool, window: &mut Window, cx: &mut Context<Self>) {
        let Some(indi) = self.selected.clone() else { return };
        if existing {
            self.open_picker(PickPurpose::Relative { indi, rel }, window, cx);
        } else {
            self.add_relative(indi, rel, window, cx);
        }
    }

    // ------------------------------------------------------------------
    // Palette de commandes
    // ------------------------------------------------------------------

    fn toggle_palette(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.palette.take().is_some() {
            window.focus(&self.focus, cx);
            cx.notify();
            return;
        }
        // Only the commands that apply now (checked from the workspace), plus those of the
        // application (About, Release Notes, Quit…), which apply everywhere.
        let items = crate::commands::palette_entries()
            .into_iter()
            .filter(|e| window.is_action_available(e.action.as_ref(), cx) || crate::is_global_action(e.action.as_ref()))
            .map(|e| {
                // Keywords without accents: “pere” finds “Père”.
                let words: Vec<String> = e.path.split(" › ").map(fold_accents).collect();
                CommandItem::new().label(e.path).keywords(words).action(e.action)
            })
            .collect();
        let state = cx.new(|cx| CommandState::new(window, cx));
        state.update(cx, |s, cx| s.focus(window, cx));
        self.tour_did(super::tour::Advance::Palette, cx);
        self.palette = Some(Palette { state, items });
        cx.notify();
    }

    fn close_palette(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.palette.take().is_some() {
            window.focus(&self.focus, cx);
            cx.notify();
        }
    }

    pub(super) fn render_palette(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let p = self.palette.as_ref()?;
        let (e1, e2) = (cx.entity().downgrade(), cx.entity().downgrade());
        let command = Command::new(&p.state)
            .items(p.items.iter().cloned())
            .placeholder(t!("palette.placeholder"))
            .empty(|_, _, _| div().p_4().text_color(theme::muted()).child(t!("palette.empty")))
            .max_h(px(420.))
            .on_confirm(move |_, window, cx| {
                let _ = e1.update(cx, |this, cx| this.close_palette(window, cx));
            })
            .on_cancel(move |window, cx| {
                let _ = e2.update(cx, |this, cx| this.close_palette(window, cx));
            });
        Some(
            div()
                .id("palette-backdrop")
                .absolute()
                .inset_0()
                .flex()
                .justify_center()
                .items_start()
                .pt(px(80.))
                .bg(theme::scrim())
                .on_mouse_down(MouseButton::Left, cx.listener(|this, _, w, cx| this.close_palette(w, cx)))
                .child(
                    div()
                        .id("palette")
                        .w(px(560.))
                        .rounded(theme::RADIUS_LG)
                        .bg(theme::panel())
                        .border_1()
                        .border_color(theme::border_strong())
                        .shadow_lg()
                        .overflow_hidden()
                        // A click in the palette does not close it.
                        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                        .child(command),
                )
                .into_any_element(),
        )
    }
}

/// Removes accents (é → e, œ → oe…) for the search in the palette.
fn fold_accents(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            'à' | 'â' | 'ä' | 'À' | 'Â' | 'Ä' => out.push('a'),
            'é' | 'è' | 'ê' | 'ë' | 'É' | 'È' | 'Ê' | 'Ë' => out.push('e'),
            'î' | 'ï' | 'Î' | 'Ï' => out.push('i'),
            'ô' | 'ö' | 'Ô' | 'Ö' => out.push('o'),
            'ù' | 'û' | 'ü' | 'Ù' | 'Û' | 'Ü' => out.push('u'),
            'ç' | 'Ç' => out.push('c'),
            'œ' | 'Œ' => out.push_str("oe"),
            c => out.extend(c.to_lowercase()),
        }
    }
    out
}
