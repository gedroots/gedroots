//! “Research” page: the records still to find (`gedcom::leads`), with where to look and the research
//! log (`gedcom::research_log`).
//!
//! Leads are computed, never typed: the list groups them by state (in progress, to find, searched
//! without a result, ignored), the centre shows the chosen lead (the agent working on it, its
//! clues, where it was handed over, estimated period and why, what is known, where to look with the
//! registers the file already cites, the log). What agents do is shown as they do it (`Live`, from
//! their tool calls, never kept); what they note is in the file, for the next one. The field above the list is
//! the focus, a search of individuals (`sosa:1-63` by default), kept while the file is open. What
//! the user or an agent notes goes in the file with an undo point.

use super::add::{AddDialog, plus_button};
use super::relatives::PickPurpose;
use super::*;
use crate::commands::label;
use crate::t;
use gedcom::leads::{DEFAULT_FOCUS, Goal, Lead, LogEntry, LogKind, RecordType, WhereToLook, Years};
use gedcom::research::current_year;
use gedcom::research_log::{NewEntry, SearchResult, TaskState};
use gpui_kit::component::Selectable;
use gpui_kit::component::button::ButtonGroup;
use gpui_kit::component::checkbox::Checkbox;
use gpui_kit::component::menu::PopupMenu;
use std::cell::RefCell;

/// State of the page.
pub(super) struct ResearchState {
    /// Leads of the focus, in the engine's order (Sosa number, name).
    pub leads: Rc<Vec<Lead>>,
    /// Focus: the people whose records are searched (search syntax).
    pub focus: String,
    /// Groups unfolded: “No result”, “Ignored” (folded by default, like a logbook).
    pub show_not_found: bool,
    pub show_ignored: bool,
    /// Row targeted by the last right-click (lead key).
    pub menu_lead: Rc<RefCell<Option<String>>>,
    /// What each connected agent is doing, by its name.
    pub live: std::collections::HashMap<String, Live>,
    /// The handoff is unfolded (folded by default: the clues and the log say the rest).
    pub handoff_open: bool,
}

/// An agent's last tool call: shown while it is recent.
pub(super) struct Live {
    /// The lead it last noted something on (key).
    pub lead: Option<String>,
    /// What it did, in the interface's language.
    pub text: String,
    pub at: std::time::Instant,
}

/// How long an agent counts as working after its last tool call.
const LIVE: std::time::Duration = std::time::Duration::from_secs(120);

impl Default for ResearchState {
    fn default() -> Self {
        ResearchState {
            leads: Rc::default(),
            focus: DEFAULT_FOCUS.to_string(),
            show_not_found: false,
            show_ignored: false,
            handoff_open: false,
            menu_lead: Rc::default(),
            live: Default::default(),
        }
    }
}

/// The window of Note a Search….
pub(super) struct SearchDialog {
    key: String,
    searched: Entity<InputState>,
    result: SearchResult,
    from: Entity<InputState>,
    to: Entity<InputState>,
    url: Entity<InputState>,
    note: Entity<InputState>,
    /// Nothing left to search online: the lead goes to “No result”.
    finished: bool,
}

/// Group of the list.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Group {
    InProgress,
    ToFind,
    NotFound,
    Ignored,
}

impl Group {
    /// `live`: an agent is working on it now.
    fn of(l: &Lead, live: bool) -> Group {
        match l.state {
            _ if live || l.in_progress() => Group::InProgress,
            TaskState::NotFound => Group::NotFound,
            TaskState::Ignored => Group::Ignored,
            TaskState::Suggested | TaskState::Planned | TaskState::Found => Group::ToFind,
        }
    }

    fn label(self) -> std::borrow::Cow<'static, str> {
        match self {
            Group::InProgress => t!("research.group.in_progress"),
            Group::ToFind => t!("research.group.to_find"),
            Group::NotFound => t!("research.group.not_found"),
            Group::Ignored => t!("research.group.ignored"),
        }
    }
}

/// Key of a lead in the list and the history: its owner (the person, or the family for a marriage)
/// and its goal.
pub(super) fn lead_key(l: &Lead) -> String {
    format!("{}|{}", l.owner(), l.goal.tag())
}

/// Shape and colour of a state (never colour alone).
fn state_icon(state: TaskState) -> (IconName, Hsla) {
    match state {
        TaskState::Suggested => (IconName::CircleDashed, theme::muted()),
        TaskState::Planned => (IconName::Circle, theme::accent()),
        TaskState::Found => (IconName::CircleCheck, theme::ok()),
        TaskState::NotFound => (IconName::CircleSlash, theme::muted()),
        TaskState::Ignored => (IconName::CircleMinus, theme::muted()),
    }
}

fn result_icon(r: SearchResult) -> (IconName, Hsla) {
    match r {
        SearchResult::Found => (IconName::CircleCheck, theme::ok()),
        SearchResult::Nothing => (IconName::CircleSlash, theme::muted()),
        SearchResult::Partial => (IconName::CircleDot, theme::warn()),
    }
}

/// The agent's name, as it gave it (“AI agent” when it did not).
fn agent_label(agent: &str) -> String {
    match agent {
        gedcom::research_log::UNKNOWN_AGENT => t!("research.by_agent").into_owned(),
        name => name.to_string(),
    }
}

/// “Death of Michel OHL”, in the interface's language.
pub(super) fn lead_title(l: &Lead) -> String {
    let name = &l.person.name;
    match (l.goal, &l.spouse) {
        (Goal::Birth, _) => t!("research.title.birth", name = name),
        (Goal::Marriage, Some(s)) => t!("research.title.marriage_of", name = name, spouse = s.name),
        (Goal::Marriage, None) => t!("research.title.marriage", name = name),
        (Goal::Death, _) => t!("research.title.death", name = name),
        (Goal::Parents, _) => t!("research.title.parents", name = name),
    }
    .into_owned()
}

/// “Death registers”: kind of records and what to read in them, in the interface's language, with
/// its local name when it differs (“Registration indexes (tables décennales)”).
fn where_label(w: &WhereToLook, goal: Goal) -> String {
    let kind = match w.record {
        RecordType::CivilRegistration => t!("research.record.civil_registration"),
        RecordType::RegistrationIndex => t!("research.record.registration_index"),
        RecordType::ChurchRegisters => t!("research.record.church_registers"),
        RecordType::Census => t!("research.record.census"),
        RecordType::Probate => t!("research.record.probate"),
        RecordType::MarriageContracts => t!("research.record.marriage_contracts"),
        RecordType::MilitaryRegisters => t!("research.record.military_registers"),
        RecordType::DeathIndex => t!("research.record.death_index"),
        RecordType::PopulationRegisters => t!("research.record.population_registers"),
    };
    let what = match (w.record, goal) {
        (RecordType::CivilRegistration | RecordType::RegistrationIndex, Goal::Birth) => t!("research.what.births"),
        (RecordType::CivilRegistration | RecordType::RegistrationIndex, Goal::Marriage) => {
            t!("research.what.marriages")
        }
        (RecordType::CivilRegistration | RecordType::RegistrationIndex, Goal::Death) => t!("research.what.deaths"),
        (RecordType::CivilRegistration | RecordType::RegistrationIndex, Goal::Parents) => t!("research.what.all"),
        (RecordType::ChurchRegisters, Goal::Birth) => t!("research.what.baptisms"),
        (RecordType::ChurchRegisters, Goal::Marriage) => t!("research.what.marriages"),
        (RecordType::ChurchRegisters, Goal::Death) => t!("research.what.burials"),
        (RecordType::ChurchRegisters, Goal::Parents) => t!("research.what.baptisms_marriages"),
        _ => "".into(),
    };
    let local = w.local_name.trim();
    let kind = match local.is_empty() || sort_key(local) == sort_key(&kind) {
        true => kind.into_owned(),
        false => t!("research.record.local", kind = kind, local = local).into_owned(),
    };
    match what.is_empty() {
        true => kind,
        false => t!("research.record.what", kind = kind, what = what).into_owned(),
    }
}

/// A year field's number, if it is one.
fn year_of(input: &Entity<InputState>, cx: &App) -> Option<i32> {
    input.read(cx).value().trim().parse().ok()
}

impl Workspace {
    /// Recomputes the leads of the focus (the file or the focus changed).
    pub(super) fn refresh_research(&mut self) {
        let leads = match &self.doc {
            // Sosa numbers need a root person.
            Some(doc) if doc.ged.root().is_some() || !self.research.focus.contains("sosa") => {
                doc.ged.research_leads(&self.research.focus, current_year())
            }
            _ => Vec::new(),
        };
        // The leads being worked on show whatever the focus.
        let mut leads = leads;
        if let Some(doc) = &self.doc {
            for l in doc.ged.leads_in_progress(current_year()) {
                if !leads.iter().any(|m| lead_key(m) == lead_key(&l)) {
                    leads.push(l);
                }
            }
        }
        self.research.leads = Rc::new(leads);
    }

    /// Leads still to find or in progress, for the sidebar.
    pub(super) fn research_count(&self) -> Option<usize> {
        self.doc.as_ref()?;
        let open = |l: &&Lead| matches!(self.group(l), Group::ToFind | Group::InProgress);
        Some(self.research.leads.iter().filter(open).count())
    }

    fn group(&self, l: &Lead) -> Group {
        Group::of(l, self.live_on(&lead_key(l)).is_some())
    }

    /// The agent working on a lead now, and what it last did.
    fn live_on(&self, key: &str) -> Option<(&str, &Live)> {
        self.research
            .live
            .iter()
            .filter(|(_, l)| l.lead.as_deref() == Some(key) && l.at.elapsed() < LIVE)
            .max_by_key(|(_, l)| l.at)
            .map(|(a, l)| (a.as_str(), l))
    }

    /// An agent is working on this tree now.
    pub(super) fn agent_working(&self) -> bool {
        self.research.live.values().any(|l| l.at.elapsed() < LIVE)
    }

    /// An agent called a tool: the Research page shows it while it works.
    pub(crate) fn agent_activity(&mut self, a: crate::agent::Activity, cx: &mut Context<Self>) {
        use crate::agent::{Doing, NoteKind};
        let name = |x: &str| self.doc.as_ref().map(|d| d.ged.person_ref(x).name).unwrap_or_default();
        let (text, lead) = match a.doing {
            Doing::Resume => (t!("research.activity.resume"), None),
            Doing::ReadsPerson(x) => (t!("research.activity.person", name = name(&x)), None),
            Doing::SearchesSources(q) => (t!("research.activity.sources", query = q), None),
            Doing::MatchesPeople => (t!("research.activity.match"), None),
            Doing::LocatesPlace(p) => (t!("research.activity.place", place = p), None),
            Doing::Notes { lead, kind, text } => (
                match kind {
                    NoteKind::Plan => t!("research.activity.plan", text = text),
                    NoteKind::Search => t!("research.activity.search", text = text),
                    NoteKind::Clue => t!("research.activity.clue", text = text),
                    NoteKind::Handoff => t!("research.activity.handoff", text = text),
                },
                Some(lead),
            ),
            Doing::ProposesAct => (t!("research.activity.act"), None),
            Doing::ProposesChange => (t!("research.activity.change"), None),
            Doing::ProposesChapter => (t!("research.activity.chapter"), None),
            Doing::ReadsQuality => (t!("research.activity.quality"), None),
        };
        let at = std::time::Instant::now();
        let entry = self.research.live.entry(a.agent).or_insert(Live { lead: None, text: String::new(), at });
        entry.text = text.into_owned();
        entry.at = at;
        // It stays on its lead until it notes something on another one.
        if lead.is_some() {
            entry.lead = lead;
        }
        cx.notify();
        // Shown as stopped once it has been quiet for a while.
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(LIVE + std::time::Duration::from_secs(1)).await;
            let _ = this.update(cx, |_, cx| cx.notify());
        })
        .detach();
    }

    /// Rows of the list (used for the selection and the history; the list draws the leads).
    pub(super) fn research_rows(&self) -> Vec<Row> {
        self.research
            .leads
            .iter()
            .map(|l| Row {
                key: lead_key(l),
                title: lead_title(l),
                strong: String::new(),
                meta: l.years.map(|y| y.to_string()).unwrap_or_default(),
                dot: None,
                flagged: false,
            })
            .collect()
    }

    pub(super) fn lead_title(&self, key: &str) -> Option<String> {
        self.research.leads.iter().find(|l| lead_key(l) == key).map(lead_title)
    }

    fn lead(&self, key: &str) -> Option<&Lead> {
        self.research.leads.iter().find(|l| lead_key(l) == key)
    }

    /// The lead the commands act on: the selected one, on the Research page.
    pub(super) fn selected_lead(&self) -> Option<&Lead> {
        (self.view == View::Research).then_some(())?;
        self.lead(self.selected.as_deref()?)
    }

    /// Opens Note a Search… on a lead.
    pub(super) fn open_search_dialog(&mut self, key: String, window: &mut Window, cx: &mut Context<Self>) {
        let Some(l) = self.lead(&key) else { return };
        let (from, to) = l.years.map(|y| (y.from.to_string(), y.to.to_string())).unwrap_or_default();
        let input = |ph: std::borrow::Cow<'static, str>, v: String, window: &mut Window, cx: &mut Context<Self>| {
            cx.new(|cx| InputState::new(window, cx).placeholder(ph).default_value(v))
        };
        let searched = input(t!("research.searched_placeholder"), String::new(), window, cx);
        let from = input(t!("research.from_placeholder"), from, window, cx);
        let to = input(t!("research.to_placeholder"), to, window, cx);
        let url = input("https://…".into(), String::new(), window, cx);
        let note = input(t!("research.note_placeholder"), String::new(), window, cx);
        searched.update(cx, |s, cx| s.focus(window, cx));
        self.add_dialog = Some(AddDialog::Search(Box::new(SearchDialog {
            key,
            searched,
            result: SearchResult::Nothing,
            from,
            to,
            url,
            note,
            finished: false,
        })));
        cx.notify();
    }

    fn save_noted_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(AddDialog::Search(d)) = &self.add_dialog else { return };
        let searched = d.searched.read(cx).value().trim().to_string();
        if searched.is_empty() {
            d.searched.update(cx, |s, cx| s.focus(window, cx));
            return;
        }
        let Some(lead) = self.lead(&d.key).cloned() else { return };
        let years = match (year_of(&d.from, cx), year_of(&d.to, cx)) {
            (Some(a), Some(b)) => Some(Years::new(a, b)),
            (Some(a), None) | (None, Some(a)) => Some(Years::new(a, a)),
            (None, None) => None,
        };
        let search = NewEntry {
            text: searched,
            result: Some(d.result),
            years,
            place: lead.places.first().cloned().unwrap_or_default(),
            url: d.url.read(cx).value().trim().to_string(),
            note: d.note.read(cx).value().trim().to_string(),
            ..Default::default()
        };
        let (finished, found) = (d.finished, d.result == SearchResult::Found);
        self.add_dialog = None;
        let (owner, goal) = (lead.owner().to_string(), lead.goal);
        let msg = t!("research.noted", lead = lead_title(&lead)).into_owned();
        self.edit_ged(move |g| g.log_research(&owner, goal, &search, finished), msg, window, cx);
        window.focus(&self.focus, cx);
        // Found: the record is entered next, from its source.
        if found {
            self.enter_lead_record(lead, window, cx);
        }
    }

    /// Ignore Lead / Resume Lead.
    pub(super) fn set_lead_ignored(&mut self, key: String, ignore: bool, window: &mut Window, cx: &mut Context<Self>) {
        let Some(l) = self.lead(&key).cloned() else { return };
        let (owner, goal) = (l.owner().to_string(), l.goal);
        // Resumed: back to planned if something was searched, otherwise only suggested.
        let state = match (ignore, l.log.is_empty()) {
            (true, _) => TaskState::Ignored,
            (false, true) => TaskState::Suggested,
            (false, false) => TaskState::Planned,
        };
        let msg = match ignore {
            true => t!("research.ignored", lead = lead_title(&l)),
            false => t!("research.resumed", lead = lead_title(&l)),
        }
        .into_owned();
        self.edit_ged(move |g| g.set_research_state(&owner, goal, state, ""), msg, window, cx);
    }

    /// Copy Request for the Agent: what to paste in the agent's conversation.
    pub(super) fn copy_agent_request(&mut self, key: String, cx: &mut Context<Self>) {
        let Some(l) = self.lead(&key) else { return };
        let goal = match l.goal {
            Goal::Birth => "birth",
            Goal::Marriage => "marriage",
            Goal::Death => "death",
            Goal::Parents => "parents",
        };
        let text = t!("research.agent_request", lead = lead_title(l), person = l.person.id, goal = goal).into_owned();
        cx.write_to_clipboard(ClipboardItem::new_string(text));
        self.set_status(t!("research.request_copied"), false);
        cx.notify();
    }

    /// Enter Record… on a lead: choose the source, then the record opens with its event and its
    /// people.
    pub(super) fn enter_lead_record(&mut self, lead: Lead, window: &mut Window, cx: &mut Context<Self>) {
        let event = match lead.goal {
            Goal::Birth | Goal::Parents => "BIRT",
            Goal::Marriage => "MARR",
            Goal::Death => "DEAT",
        };
        let people = std::iter::once(lead.person.id.clone()).chain(lead.spouse.map(|s| s.id)).collect();
        self.open_picker(PickPurpose::ActSource { prefill: Some((event.to_string(), people)) }, window, cx);
    }

    /// The menu of a lead: right-click on a row.
    fn lead_menu(&self, cx: &mut Context<Self>) -> menus::MenuBuilder {
        let ws = cx.entity().downgrade();
        let target = self.research.menu_lead.clone();
        Rc::new(move |menu: PopupMenu, _, cx| {
            let Some(key) = target.borrow_mut().take() else { return menu };
            let Some((lead, ignored)) = ws
                .read_with(cx, |this, _| this.lead(&key).map(|l| (l.clone(), l.state == TaskState::Ignored)))
                .ok()
                .flatten()
            else {
                return menu;
            };
            let (k1, k2, k3) = (key.clone(), key.clone(), key.clone());
            let person = lead.person.id.clone();
            menu.item(menus::act(&ws, label::note_search(), move |this, w, cx| {
                this.open_search_dialog(k1.clone(), w, cx)
            }))
            .item(menus::act(&ws, label::enter_act(), move |this, w, cx| this.enter_lead_record(lead.clone(), w, cx)))
            .separator()
            .item(menus::act(
                &ws,
                if ignored { label::resume_lead() } else { label::ignore_lead() },
                move |this, w, cx| this.set_lead_ignored(k2.clone(), !ignored, w, cx),
            ))
            .item(menus::act(&ws, label::copy_agent_request(), move |this, _, cx| {
                this.copy_agent_request(k3.clone(), cx)
            }))
            .separator()
            .item(menus::act(&ws, t!("research.open_person"), move |this, w, cx| this.goto(person.clone(), w, cx)))
        })
    }

    fn toggle_group(&mut self, g: Group, cx: &mut Context<Self>) {
        match g {
            Group::NotFound => self.research.show_not_found = !self.research.show_not_found,
            Group::Ignored => self.research.show_ignored = !self.research.show_ignored,
            Group::ToFind | Group::InProgress => {}
        }
        cx.notify();
    }

    /// The list: the focus field, then the leads by group.
    pub(super) fn render_research_list(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let leads = self.research.leads.clone();
        let selected = self.selected.clone();
        let mut body = v_flex().w_full().pb_2();
        for g in [Group::InProgress, Group::ToFind, Group::NotFound, Group::Ignored] {
            let mut of: Vec<&Lead> = leads.iter().filter(|l| self.group(l) == g).collect();
            if of.is_empty() {
                continue;
            }
            // Last worked on first.
            if g == Group::InProgress {
                of.sort_by_key(|l| std::cmp::Reverse(l.log.last().map(|e| (e.date_key(), e.time.clone()))));
            }
            let open = match g {
                Group::ToFind | Group::InProgress => true,
                Group::NotFound => self.research.show_not_found,
                Group::Ignored => self.research.show_ignored,
            };
            let foldable = !matches!(g, Group::ToFind | Group::InProgress);
            body = body.child(
                h_flex()
                    .id(SharedString::from(format!("lead-group-{}", g as u8)))
                    .px_3()
                    .pt_3()
                    .pb_1()
                    .gap_1()
                    .items_center()
                    .text_xs()
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(theme::muted())
                    .when(foldable, |d| {
                        d.cursor_pointer().on_click(cx.listener(move |this, _, _, cx| this.toggle_group(g, cx)))
                    })
                    .when(foldable, |d| {
                        d.child(icon(if open { IconName::ChevronDown } else { IconName::ChevronRight }))
                    })
                    .child(g.label())
                    .child(mono(of.len().to_string()).font_weight(FontWeight::NORMAL)),
            );
            if !open {
                continue;
            }
            for l in of {
                let key = lead_key(l);
                let on = selected.as_deref() == Some(key.as_str());
                let live = self.live_on(&key);
                let (shape, color) = match (g, live) {
                    (_, Some(_)) => (IconName::LoaderCircle, theme::ok()),
                    (Group::InProgress, None) => (IconName::CircleDot, theme::accent()),
                    _ => state_icon(l.state),
                };
                let mut meta: Vec<String> = Vec::new();
                if let Some(n) = l.sosa {
                    meta.push(t!("research.sosa", n = n).into_owned());
                }
                if let Some(p) = l.places.first() {
                    meta.push(short_place(p));
                }
                // Who works on it: the agent now, or the last one that noted something.
                let worker = live.map(|(a, _)| a.to_string()).or_else(|| {
                    (g == Group::InProgress)
                        .then(|| l.log.iter().rev().find(|e| e.by_agent()).map(|e| agent_label(&e.agent)))
                        .flatten()
                });
                match worker {
                    Some(w) => meta.push(w),
                    None if l.state == TaskState::Planned => meta.push(t!("research.planned").into_owned()),
                    None => {}
                }
                let clues = l.log.iter().filter(|e| e.kind == LogKind::Clue).count();
                if clues > 0 && g != Group::InProgress {
                    meta.push(crate::tn!("research.clue_count", clues).into_owned());
                }
                let (k1, k2) = (key.clone(), key.clone());
                let menu_lead = self.research.menu_lead.clone();
                body =
                    body.child(
                        h_flex()
                            .id(SharedString::from(format!("lead-{key}")))
                            .w_full()
                            .px_3()
                            .py_1()
                            .gap_2()
                            .items_start()
                            .cursor_pointer()
                            .when(on, |d| d.bg(theme::hover()))
                            .hover(|d| d.bg(theme::surface()))
                            .child(div().pt_0p5().child(Icon::new(shape).size(px(14.)).text_color(color)))
                            .child(
                                v_flex().flex_1().min_w_0().child(div().truncate().child(lead_title(l))).child(
                                    div().truncate().text_xs().text_color(theme::muted()).child(meta.join(" · ")),
                                ),
                            )
                            .children(l.years.map(|y| mono(y.to_string()).flex_none()))
                            .on_click(cx.listener(move |this, _, w, cx| this.select(k1.clone(), w, cx)))
                            .on_mouse_down(MouseButton::Right, move |_, _, _| {
                                *menu_lead.borrow_mut() = Some(k2.clone());
                            }),
                    );
            }
        }
        let empty = leads.is_empty();
        v_flex()
            .w(px(300.))
            .flex_none()
            .relative()
            .bg(theme::bg())
            .border_r_1()
            .border_color(theme::border())
            .child(self.render_search_field(cx))
            .child(self.render_search_notes(cx))
            .child(
                div()
                    .id("leads")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .text_sm()
                    .when(!empty, |d| d.child(body))
                    .with_menu(self.lead_menu(cx)),
            )
            .children(self.render_suggestions(cx))
            .children(self.render_chip_menu(cx))
    }

    /// The centre: the chosen lead, in one readable column.
    pub(super) fn render_research(&self, cx: &mut Context<Self>) -> AnyElement {
        let lead = self.selected.as_deref().and_then(|k| self.lead(k));
        let (Some(l), Some(doc)) = (lead, &self.doc) else {
            let none = self.research.leads.is_empty();
            let no_root = self.doc.as_ref().is_some_and(|d| d.ged.root().is_none());
            return v_flex()
                .flex_1()
                .items_center()
                .justify_center()
                .gap_2()
                .text_color(theme::muted())
                .child(Icon::new(IconName::Compass).size(px(24.)))
                .child(match (none, no_root) {
                    (true, true) => t!("research.no_root"),
                    (true, false) => t!("research.no_lead"),
                    (false, _) => t!("research.choose_lead"),
                })
                .into_any_element();
        };
        let g = &doc.ged;
        let key = lead_key(l);
        let person = l.person.id.clone();
        // Under the title: the person (link), their Sosa number, their years, the state.
        let mut sub: Vec<AnyElement> = vec![
            div()
                .id("lead-person")
                .text_color(theme::accent())
                .cursor_pointer()
                .child(l.person.name.clone())
                .on_click(cx.listener(move |this, _, w, cx| this.goto(person.clone(), w, cx)))
                .into_any_element(),
        ];
        let mut extra: Vec<String> = Vec::new();
        if let Some(n) = l.sosa {
            extra.push(t!("research.sosa", n = n).into_owned());
        }
        if !l.person.years.is_empty() {
            extra.push(l.person.years.clone());
        }
        match l.state {
            TaskState::Planned => extra.push(t!("research.planned").into_owned()),
            TaskState::Found => extra.push(t!("research.state.found").into_owned()),
            TaskState::NotFound => extra.push(t!("research.state.not_found").into_owned()),
            TaskState::Ignored => extra.push(t!("research.state.ignored").into_owned()),
            TaskState::Suggested => {}
        }
        for e in extra {
            sub.push(div().text_color(theme::muted()).child("·").into_any_element());
            sub.push(div().text_color(theme::muted()).child(e).into_any_element());
        }
        let header = v_flex()
            .gap_1()
            .child(div().text_xl().font_weight(FontWeight::SEMIBOLD).child(lead_title(l)))
            .child(h_flex().flex_wrap().gap_1().text_sm().children(sub))
            .when(!l.note.is_empty(), |d| d.child(div().text_sm().text_color(theme::muted()).child(l.note.clone())));
        let section = |title: std::borrow::Cow<'static, str>| v_flex().gap_2().child(label_caps(title));
        // The agent working on it now, and what it last did.
        let live = self.live_on(&key).map(|(agent, now)| {
            h_flex()
                .gap_2()
                .px_3()
                .py_2()
                .items_center()
                .rounded(theme::RADIUS_SM)
                .border_1()
                .border_color(theme::border())
                .bg(theme::surface())
                .text_sm()
                .child(dot(theme::ok()))
                .child(div().flex_none().font_weight(FontWeight::SEMIBOLD).child(agent_label(agent)))
                .child(div().min_w_0().truncate().text_color(theme::muted()).child(now.text.clone()))
        });
        // Records that may be the one, with what speaks for and against: newest first.
        let clue_cards: Vec<Div> = l
            .log
            .iter()
            .rev()
            .filter(|e| e.kind == LogKind::Clue)
            .enumerate()
            .map(|(i, e)| {
                // “For: …”, “Against: …”: one line each, the label muted.
                let side = |title: std::borrow::Cow<'static, str>, text: &str| {
                    (!text.is_empty()).then(|| {
                        h_flex()
                            .gap_1()
                            .items_start()
                            .child(div().flex_none().text_color(theme::muted()).child(title))
                            .child(div().flex_1().min_w_0().child(text.to_string()))
                    })
                };
                let url = e.url.clone();
                v_flex()
                    .gap_1()
                    .px_3()
                    .py_2()
                    .rounded(theme::RADIUS_SM)
                    .border_1()
                    .border_color(theme::border())
                    .text_sm()
                    .child(
                        h_flex()
                            .gap_2()
                            .items_start()
                            .child(
                                div()
                                    .pt_0p5()
                                    .child(Icon::new(IconName::Diamond).size(px(14.)).text_color(theme::warn())),
                            )
                            .child(div().flex_1().min_w_0().font_weight(FontWeight::MEDIUM).child(e.text.clone()))
                            .when(!url.is_empty(), |d| {
                                let href = url.clone();
                                d.child(
                                    div()
                                        .id(SharedString::from(format!("clue-link-{i}")))
                                        .flex_none()
                                        .text_color(theme::accent())
                                        .cursor_pointer()
                                        .child(host(&url).to_string())
                                        .on_click(move |_, _, cx| cx.open_url(&href)),
                                )
                            }),
                    )
                    .child(
                        v_flex()
                            .pl(px(22.))
                            .gap_0p5()
                            .when(!e.note.is_empty(), |d| {
                                d.child(div().text_color(theme::muted()).child(e.note.clone()))
                            })
                            .children(side(t!("research.for"), &e.evidence_for))
                            .children(side(t!("research.against"), &e.against)),
                    )
            })
            .collect();
        let clues = (!clue_cards.is_empty()).then(|| section(t!("research.clues")).children(clue_cards));
        // Where the last one stopped, and the search to make next.
        let handoff = l.log.iter().rev().find(|e| e.kind == LogKind::Handoff).map(|e| {
            let open = self.research.handoff_open;
            let mut by = vec![Date::parse(&e.date).display()];
            if e.by_agent() {
                by.insert(0, agent_label(&e.agent));
            }
            v_flex()
                .gap_2()
                .child(
                    h_flex()
                        .id("research-handoff")
                        .gap_1()
                        .items_center()
                        .cursor_pointer()
                        .text_color(theme::muted())
                        .child(
                            Icon::new(if open { IconName::ChevronDown } else { IconName::ChevronRight }).size(px(14.)),
                        )
                        .child(label_caps(t!("research.handoff")))
                        .child(div().text_xs().child(format!("· {}", by.join(" · "))))
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.research.handoff_open = !this.research.handoff_open;
                            cx.notify();
                        })),
                )
                .when(open, |d| {
                    d.child(v_flex().gap_1().text_sm().child(div().child(e.text.clone())).when(
                        !e.next.is_empty(),
                        |d| {
                            d.child(
                                h_flex()
                                    .gap_1()
                                    .items_start()
                                    .child(
                                        div().flex_none().text_color(theme::muted()).child(t!("research.next_search")),
                                    )
                                    .child(div().flex_1().min_w_0().child(e.next.clone())),
                            )
                        },
                    ))
                })
        });
        // What is known: the person's events and those of their unions, then the record to find.
        let entries: Vec<_> =
            timeline::timeline(g, &l.person.id).into_iter().filter(|e| e.kind != EntryKind::Relative).collect();
        let row = |date: String, what: AnyElement, end: AnyElement| {
            h_flex()
                .gap_3()
                .py_1()
                .items_start()
                .border_b_1()
                .border_color(theme::border())
                .child(mono(date).w(px(110.)).flex_none())
                .child(div().flex_1().min_w_0().child(what))
                .child(div().flex_none().child(end))
        };
        let known = section(t!("research.known"))
            .children(entries.iter().map(|e| {
                let place = e.place.as_deref().map(short_place).unwrap_or_default();
                let what = h_flex()
                    .gap_1()
                    .flex_wrap()
                    .text_sm()
                    .child(div().font_weight(FontWeight::MEDIUM).child(e.label.clone()))
                    .when(!place.is_empty(), |d| d.child(div().text_color(theme::muted()).child(format!("· {place}"))));
                let end = h_flex()
                    .gap_1()
                    .items_center()
                    .text_xs()
                    .text_color(theme::muted())
                    .child(dot(if e.sourced { theme::ok() } else { theme::warn() }))
                    .child(if e.sourced { t!("research.sourced") } else { t!("research.unsourced") });
                row(
                    e.date.as_ref().map(|d| d.display()).unwrap_or_default(),
                    what.into_any_element(),
                    end.into_any_element(),
                )
                .into_any_element()
            }))
            .child(
                row(
                    l.years.map(|y| y.to_string()).unwrap_or_default(),
                    v_flex()
                        .gap_0p5()
                        .text_sm()
                        .child(
                            h_flex()
                                .gap_1()
                                .text_color(theme::accent())
                                .child(div().font_weight(FontWeight::SEMIBOLD).child(goal_label(l.goal)))
                                .when_some(l.places.first(), |d, p| d.child(format!("· {} ?", short_place(p)))),
                        )
                        // Why these years: computed from what dates the person.
                        .when(!l.years_reasons.is_empty(), |d| {
                            d.child(
                                div()
                                    .text_color(theme::muted())
                                    .child(l.years_reasons.iter().map(|r| r.label()).collect::<Vec<_>>().join(" · ")),
                            )
                        })
                        .into_any_element(),
                    div()
                        .px_1()
                        .rounded(theme::RADIUS_SM)
                        .border_1()
                        .border_dashed()
                        .border_color(theme::accent())
                        .text_xs()
                        .text_color(theme::accent())
                        .child(t!("research.to_find"))
                        .into_any_element(),
                )
                .text_color(theme::accent()),
            );
        // Where to look: numbered, with the registers the file already cites.
        let where_rows = l.where_to_look.iter().enumerate().map(|(i, w)| {
            let mut title = vec![where_label(w, l.goal)];
            if !w.place.is_empty() {
                title.push(short_place(&w.place));
            }
            // The years stay on one line.
            let years = w.years.map(|y| format!("· {y}"));
            h_flex()
                .gap_3()
                .px_3()
                .py_2()
                .items_start()
                .when(i > 0, |d| d.border_t_1().border_color(theme::border()))
                .child(mono((i + 1).to_string()).w(px(14.)).flex_none())
                .child(
                    v_flex()
                        .flex_1()
                        .min_w_0()
                        .gap_0p5()
                        .child(
                            h_flex()
                                .flex_wrap()
                                .gap_1()
                                .text_sm()
                                .font_weight(FontWeight::MEDIUM)
                                .child(title.join(" · "))
                                .children(years.map(|y| div().flex_none().child(y))),
                        )
                        .children(w.used_registers.iter().map(|r| {
                            let rest: Vec<&str> = [r.repository.as_str(), r.call_number.as_str()]
                                .into_iter()
                                .filter(|s| !s.is_empty())
                                .collect();
                            h_flex()
                                .gap_1()
                                .min_w_0()
                                .text_xs()
                                .text_color(theme::muted())
                                .child(div().min_w_0().truncate().text_color(theme::text()).child(r.register.clone()))
                                .when(!rest.is_empty(), |d| {
                                    d.child(div().flex_none().child(format!("· {}", rest.join(" · "))))
                                })
                        }))
                        .when(!w.census_years.is_empty(), |d| {
                            d.child(h_flex().gap_1().flex_wrap().children(w.census_years.iter().map(|y| {
                                mono(y.to_string())
                                    .px_1()
                                    .rounded(theme::RADIUS_SM)
                                    .border_1()
                                    .border_color(theme::border())
                            })))
                        })
                        .when_some(w.public_until, |d, y| {
                            d.child(
                                div().text_xs().text_color(theme::muted()).child(t!("research.public_until", year = y)),
                            )
                        }),
                )
        });
        let where_ = (!l.where_to_look.is_empty()).then(|| {
            section(t!("research.where"))
                .child(v_flex().rounded(theme::RADIUS_SM).border_1().border_color(theme::border()).children(where_rows))
        });
        // The log, with “+” to note a search.
        let k = key.clone();
        let log = v_flex()
            .gap_2()
            .child(section_header(
                t!("research.log"),
                plus_button("lead-note-search")
                    .tooltip(label::note_search().trim_end_matches('…').to_string())
                    .on_click(cx.listener(move |this, _, w, cx| this.open_search_dialog(k.clone(), w, cx))),
            ))
            .children(l.log.iter().rev().map(|s| self.render_log_entry(s)));
        let column = v_flex()
            .w_full()
            .max_w(px(680.))
            .px_4()
            .py_6()
            .gap_6()
            .child(header)
            .children(live)
            .children(clues)
            .children(handoff)
            .child(known)
            .children(where_)
            .child(log);
        div()
            .id("research-detail")
            .size_full()
            .overflow_y_scroll()
            .child(h_flex().w_full().justify_center().child(column))
            .into_any_element()
    }

    fn render_log_entry(&self, s: &LogEntry) -> Div {
        let (shape, color) = match s.kind {
            LogKind::Search => result_icon(s.result),
            LogKind::Clue => (IconName::Diamond, theme::warn()),
            LogKind::Handoff => (IconName::ArrowRight, theme::muted()),
        };
        let date = if s.date.is_empty() { String::new() } else { Date::parse(&s.date).display() };
        let mut meta: Vec<String> = Vec::new();
        if let Some(y) = s.years {
            meta.push(y.to_string());
        }
        if s.kind == LogKind::Search {
            meta.push(
                match s.result {
                    SearchResult::Found => t!("research.result.found"),
                    SearchResult::Nothing => t!("research.result.nothing"),
                    SearchResult::Partial => t!("research.result.partial"),
                }
                .into_owned(),
            );
        }
        for v in [&s.note, &s.evidence_for, &s.against, &s.next] {
            if !v.is_empty() {
                meta.push(v.clone());
            }
        }
        let url = s.url.clone();
        h_flex()
            .gap_3()
            .py_1()
            .items_start()
            .border_b_1()
            .border_color(theme::border())
            .child(div().pt_0p5().child(Icon::new(shape).size(px(14.)).text_color(color)))
            .child(mono(date).w(px(84.)).flex_none())
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .text_sm()
                    .child(s.text.clone())
                    .child(div().text_xs().text_color(theme::muted()).child(meta.join(" · ")))
                    .when(!url.is_empty(), |d| {
                        let href = url.clone();
                        d.child(
                            div()
                                .id(SharedString::from(format!("search-link-{}", s.date)))
                                .text_xs()
                                .truncate()
                                .text_color(theme::accent())
                                .cursor_pointer()
                                .child(host(&url).to_string())
                                .on_click(move |_, _, cx| cx.open_url(&href)),
                        )
                    }),
            )
            .when(s.by_agent(), |d| {
                d.child(div().flex_none().text_xs().text_color(theme::muted()).child(agent_label(&s.agent)))
            })
    }

    /// Body of Note a Search….
    pub(super) fn search_dialog_title(&self, d: &SearchDialog) -> String {
        let what = label::note_search().trim_end_matches('…').to_string();
        match self.lead(&d.key) {
            Some(l) => format!("{what} · {}", lead_title(l)),
            None => what,
        }
    }

    pub(super) fn search_dialog_body(&self, d: &SearchDialog, cx: &mut Context<Self>) -> AnyElement {
        let field = |label: std::borrow::Cow<'static, str>, el: AnyElement| {
            v_flex().gap_1().min_w_0().child(label_caps(label)).child(el)
        };
        let results = [
            (SearchResult::Found, t!("research.result.found")),
            (SearchResult::Nothing, t!("research.result.nothing")),
            (SearchResult::Partial, t!("research.result.partial")),
        ];
        let current = d.result;
        let result =
            ButtonGroup::new("search-result")
                .outline()
                .small()
                .children(results.iter().enumerate().map(|(i, (r, label))| {
                    Button::new(("search-result", i)).label(label.clone()).selected(*r == current)
                }))
                .on_click(cx.listener(move |this, clicked: &Vec<usize>, _, cx| {
                    if let Some(AddDialog::Search(d)) = &mut this.add_dialog
                        && let Some(i) = clicked.first()
                    {
                        d.result = [SearchResult::Found, SearchResult::Nothing, SearchResult::Partial][*i];
                        cx.notify();
                    }
                }));
        let finished = d.finished;
        v_flex()
            .gap_3()
            .child(field(t!("research.searched"), Input::new(&d.searched).small().into_any_element()))
            .child(field(t!("research.result"), result.into_any_element()))
            .child(field(
                t!("research.years"),
                h_flex()
                    .gap_2()
                    .items_center()
                    .child(div().w(px(90.)).child(Input::new(&d.from).small()))
                    .child(div().text_color(theme::muted()).child("–"))
                    .child(div().w(px(90.)).child(Input::new(&d.to).small()))
                    .into_any_element(),
            ))
            .child(field(t!("research.link"), Input::new(&d.url).small().into_any_element()))
            .child(field(t!("research.note"), Input::new(&d.note).small().into_any_element()))
            .when(current != SearchResult::Found, |v| {
                v.child(
                    Checkbox::new("search-finished")
                        .checked(finished)
                        .label(t!("research.finished").into_owned())
                        .on_click(cx.listener(|this, checked: &bool, _, cx| {
                            if let Some(AddDialog::Search(d)) = &mut this.add_dialog {
                                d.finished = *checked;
                                cx.notify();
                            }
                        })),
                )
            })
            .child(
                h_flex().gap_2().justify_end().child(
                    Button::new("search-save")
                        .primary()
                        .small()
                        .icon(IconName::Check)
                        .label(t!("research.note_button"))
                        .on_click(cx.listener(|this, _, w, cx| this.save_noted_search(w, cx))),
                ),
            )
            .on_key_down(cx.listener(|this, ev: &KeyDownEvent, w, cx| {
                if ev.keystroke.key == "enter" {
                    this.save_noted_search(w, cx);
                }
            }))
            .into_any_element()
    }
}

/// “Death”, the record to find, in the line of what is known.
fn goal_label(goal: Goal) -> std::borrow::Cow<'static, str> {
    match goal {
        Goal::Birth => t!("research.goal.birth"),
        Goal::Marriage => t!("research.goal.marriage"),
        Goal::Death => t!("research.goal.death"),
        Goal::Parents => t!("research.goal.parents"),
    }
}
