//! The screen: groups on the left, tweaks in the middle, the chosen tweak's detail on the right.
//!
//! `view` reads the width it is drawn in and folds below `FOLD_BELOW` columns: the groups
//! leave the sidebar for a strip of tabs above the list, the list takes the full width, and the
//! detail is not shown beside it. There `alt+b` (the same toggle that closes the wide panel, or
//! the edge column of the closed panel under the mouse) opens the detail as a column below the
//! list, and `esc` closes it. At the fold and above the three columns sit side by side, and the
//! detail panel can be closed by hand to give the list the full width.
//!
//! While a run is under way the columns give way to the embedded terminal that carries it out,
//! and come back when the person closes it.

use std::path::PathBuf;

use qframe::prelude::*;
use qframe::widgets::{
    Badge, Button, Closed, List, ListItem, Menu, MenuGroup, MenuItem, ScrollView, Side, SidePanel, Tabs, Terminal,
    TerminalEvent, Toast, ToastKind,
};

/// Below this many columns the screen folds. Seventy-two is the narrowest at which the three
/// columns are each still usable: the sixteen-cell sidebar, the detail panel at its twenty-cell
/// minimum plus padding, and a list of some thirty cells that shows every title's head beside its
/// state word. Narrower than that one of them would have to go anyway, and the narrowest screen
/// the layout is meant for (forty-eight columns) lies well below.
const FOLD_BELOW: u16 = 72;

/// The fewest rows the narrow list keeps when the detail opens below it.
const NARROW_LIST_ROWS: u16 = 5;

use crate::catalog;
use crate::tweak::{Group, Tweak, TweakState};

pub mod run;
use run::Running;

pub mod unsupported;

/// Reads where every tweak stands, on this machine.
fn read_states() -> Vec<TweakState> {
    let mut system = crate::system::RealSystem;
    catalog::all().iter().map(|tweak| tweak.state(&mut system).unwrap_or(TweakState::Off)).collect()
}

/// The application's state.
#[derive(Debug)]
pub struct Tools {
    /// Which group the sidebar has selected.
    pub group: Group,
    /// Which tweak of that group is shown.
    pub selected: usize,
    /// Where every tweak of the catalog stands, in catalog order.
    pub states: Vec<TweakState>,
    /// Which tweaks are checked for a run of several at once.
    pub checked: Vec<bool>,
    /// Whether the detail panel is open; closed gives the list the full width. Read on wide
    /// screens only.
    pub panel_open: bool,
    /// Whether the detail is open below the list. Read on narrow screens only, so folding and
    /// unfolding never disturb the wide panel's own state.
    pub detail_open: bool,
    /// The tweaks a confirmation is asking about, and whether it is an apply or a revert.
    /// Read again when the person answers, so [`Msg::Confirmed`] does not have to carry it.
    pending: Option<Pending>,
    /// The run under way, or just ended and still on screen. `None` before a confirmation
    /// and once the person closes the terminal.
    pub running: Option<Running>,
    /// The program a run starts in the embedded terminal: this very binary, with `--run` or
    /// `--revert` (see [`crate::cli`]).
    program: PathBuf,
    /// Reads where every tweak stands, after a run: the real machine, or a double in tests.
    reader: fn() -> Vec<TweakState>,
    /// How many runs were started, so a watch of an earlier run is told from the current one.
    runs: u64,
}

/// What a confirmation is asking about: which tweaks, in catalog order, and which direction.
#[derive(Debug, Clone)]
struct Pending {
    /// The catalog places of the chosen tweaks.
    indices: Vec<usize>,
    /// Whether answering "yes" applies them or reverts them.
    revert: bool,
}

impl Tools {
    /// The screen, opened on the packages group, showing the states it was handed. Runs start
    /// this very binary and read the real machine afterwards.
    pub fn new(states: Vec<TweakState>) -> Self {
        let count = states.len();
        Self {
            group: Group::Packages,
            selected: 0,
            states,
            checked: vec![false; count],
            panel_open: true,
            detail_open: false,
            pending: None,
            running: None,
            // When the binary cannot find itself, the installed command is the next best guess.
            program: std::env::current_exe().unwrap_or_else(|_| PathBuf::from("qtools")),
            reader: read_states,
            runs: 0,
        }
    }

    /// The same screen, with runs started as `program` and states read by `reader` instead of
    /// this binary and the real machine. Tests point both at doubles.
    #[must_use]
    pub fn machine(mut self, program: PathBuf, reader: fn() -> Vec<TweakState>) -> Self {
        self.program = program;
        self.reader = reader;
        self
    }

    /// Reads where every tweak stands, off the drawing thread. Used after a run or a revert,
    /// so the list shows what the machine now says rather than what it said before.
    fn refresh(&self) -> Command<Msg> {
        let reader = self.reader;
        Command::perform(move || Msg::StatesRead(reader()))
    }
}

impl Default for Tools {
    /// Every tweak off: only used where nothing has been read yet, such as a screen test
    /// that sends its own states.
    fn default() -> Self {
        Self::new(vec![TweakState::Off; catalog::all().len()])
    }
}

/// Everything that can happen on this screen.
#[derive(Debug, Clone)]
pub enum Msg {
    /// Shows the group with this key.
    PickGroup(String),
    /// Shows the tweak at this place in the current group.
    Select(usize),
    /// Checks or unchecks the tweak at this place.
    Toggle(usize),
    /// Opens or closes the detail panel of the wide screen.
    TogglePanel(bool),
    /// Opens or closes the detail below the list of the narrow screen.
    ToggleDetail(bool),
    /// Where every tweak of the catalog stands, freshly read off the machine.
    StatesRead(Vec<TweakState>),
    /// Asks to apply the tweak at this place of the current group (or every checked tweak, when
    /// any are checked), after a confirmation.
    Apply(usize),
    /// Asks to revert the tweak at this place, after its own confirmation.
    Revert(usize),
    /// The confirmation was answered "yes".
    Confirmed,
    /// The embedded terminal of run number `.0` has new output, or its process ended.
    Changed(u64, TerminalEvent),
    /// Closes the embedded terminal once its process has ended.
    CloseRun,
}

impl App for Tools {
    type Msg = Msg;

    fn update(&mut self, msg: Msg) -> Command<Msg> {
        match msg {
            Msg::PickGroup(key) => {
                if let Some(group) = Group::ALL.into_iter().find(|group| group.key() == key) {
                    self.group = group;
                    self.selected = 0;
                }
            }
            Msg::Select(index) if index < catalog::in_group(self.group).len() => self.selected = index,
            Msg::Select(_) => {}
            Msg::Toggle(index) => {
                // A tweak that does not fit this machine is never among the checked ones a run applies.
                if matches!(self.state_of(index), TweakState::Unavailable(_)) {
                    return Command::none();
                }
                if let Some(slot) = self.catalog_index(index).and_then(|at| self.checked.get_mut(at)) {
                    *slot = !*slot;
                }
            }
            Msg::TogglePanel(open) => self.panel_open = open,
            Msg::ToggleDetail(open) => self.detail_open = open,
            Msg::StatesRead(states) => self.states = states,
            Msg::Apply(index) => return self.ask(index, false),
            Msg::Revert(index) => return self.ask(index, true),
            Msg::Confirmed => return self.confirmed(),
            Msg::Changed(run, event) => return self.changed(run, event),
            Msg::CloseRun => {
                if self.running.as_ref().is_some_and(|running| running.exit.is_some()) {
                    self.running = None;
                }
            }
        }
        Command::none()
    }

    fn view(&self, ui: &mut View<'_, Msg>) {
        AppShell::new().header(header).body(|ui| self.body(ui)).footer(|ui| self.footer(ui)).show(ui);
    }

    fn action(&self, name: &str) -> Option<Msg> {
        match name {
            "revert" if self.running.is_none() => Some(Msg::Revert(self.selected)),
            "close-run" if self.running.as_ref().is_some_and(|running| running.exit.is_some()) => Some(Msg::CloseRun),
            // `esc` is bound once, as `close-run`, and closes whatever sits on top: the ended
            // run, or the detail the narrow screen opened below the list.
            "close-run" if self.running.is_none() && self.detail_open => Some(Msg::ToggleDetail(false)),
            _ => None,
        }
    }
}

impl Tools {
    /// Where a tweak of the current group sits in the whole catalog.
    fn catalog_index(&self, index: usize) -> Option<usize> {
        let id = catalog::in_group(self.group).get(index)?.id;
        catalog::all().iter().position(|tweak| tweak.id == id)
    }

    /// Where the tweak at `index` of the current group stands, or off when it has never been
    /// read. The fallback is a defensive guard, not a real case: `index` always comes from
    /// `catalog::in_group(self.group)`, which is built from the same catalog as `self.states`.
    fn state_of(&self, index: usize) -> TweakState {
        self.catalog_index(index).and_then(|at| self.states.get(at)).cloned().unwrap_or(TweakState::Off)
    }

    /// Whether the tweak at `index` of the current group is checked to run with the others. The
    /// fallback is the same defensive guard as [`Self::state_of`].
    fn is_checked(&self, index: usize) -> bool {
        self.catalog_index(index).and_then(|at| self.checked.get(at)).copied().unwrap_or(false)
    }

    /// The catalog places a run of `index` (of the current group) would cover: every checked
    /// tweak when any are checked, otherwise just the one activated.
    fn chosen_indices(&self, index: usize) -> Vec<usize> {
        if self.checked.iter().any(|checked| *checked) {
            return self.checked.iter().enumerate().filter_map(|(at, checked)| checked.then_some(at)).collect();
        }
        self.catalog_index(index).into_iter().collect()
    }

    /// Asks whether to apply (or, with `revert`, undo) the tweak at `index` of the current
    /// group, or every checked tweak when any are checked. A tweak that does not fit this
    /// machine is never asked about, the same guard [`Msg::Toggle`] uses; nothing is asked
    /// while a run is on screen either.
    fn ask(&mut self, index: usize, revert: bool) -> Command<Msg> {
        if self.running.is_some() || matches!(self.state_of(index), TweakState::Unavailable(_)) {
            return Command::none();
        }
        let indices = self.chosen_indices(index);
        if indices.is_empty() {
            return Command::none();
        }
        let catalog = catalog::all();
        let tweaks: Vec<Tweak> = indices.iter().filter_map(|&at| catalog.get(at).cloned()).collect();
        self.pending = Some(Pending { indices, revert });
        let title = if revert { t!("confirm.revert.title") } else { t!("confirm.apply.title") };
        let mut confirm = Confirm::new(title, Msg::Confirmed).message(run::plan_text(&tweaks));
        confirm = confirm.cancel_label(t!("confirm.cancel"));
        confirm = if revert {
            confirm.confirm_label(t!("confirm.revert")).danger()
        } else {
            confirm.confirm_label(t!("confirm.apply"))
        };
        Command::confirm(confirm)
    }

    /// The confirmation was answered "yes": the run starts in the embedded terminal, which
    /// takes focus so a password prompt can be answered at once, and a watch starts waiting
    /// for its output.
    fn confirmed(&mut self) -> Command<Msg> {
        let Some(pending) = self.pending.take() else { return Command::none() };
        let catalog = catalog::all();
        let chosen: Vec<&Tweak> = pending.indices.iter().filter_map(|&at| catalog.get(at)).collect();
        if chosen.is_empty() {
            return Command::none();
        }
        let ids: Vec<&str> = chosen.iter().map(|tweak| tweak.id).collect();
        let titles = chosen.iter().map(|tweak| t!(&format!("tweak.{}.title", tweak.id))).collect();
        self.runs += 1;
        match Running::start(&self.program, &ids, titles, pending.revert, self.runs) {
            Ok(running) => {
                let watch = Self::watch(&running);
                self.running = Some(running);
                self.checked.iter_mut().for_each(|checked| *checked = false);
                Command::batch([watch, Command::focus("run")])
            }
            Err(error) => Command::toast(Toast::new(ToastKind::Danger, t!("run.failed")).body(error.to_string())),
        }
    }

    /// Waits, off the drawing thread, for the run's next output or its end.
    fn watch(running: &Running) -> Command<Msg> {
        let watch = running.session.watch();
        let run = running.run;
        Command::perform(move || Msg::Changed(run, watch.next()))
    }

    /// The run's terminal changed: keep watching on output; on its end, read the machine again
    /// and say how it went. The terminal stays on screen either way, so the output can be read.
    fn changed(&mut self, run: u64, event: TerminalEvent) -> Command<Msg> {
        let Some(running) = self.running.as_mut().filter(|running| running.run == run) else {
            return Command::none();
        };
        match event {
            TerminalEvent::Output => Self::watch(running),
            TerminalEvent::Exited(code) => {
                // The outcome is drawn beside the run's title rather than as a toast: a toast
                // floats over the terminal's bottom corner, where its note gives the exit code.
                running.exit = Some(code);
                self.refresh()
            }
        }
    }

    fn body(&self, ui: &mut View<'_, Msg>) {
        if let Some(running) = &self.running {
            Self::run_view(running, ui);
            return;
        }
        let tweaks = catalog::in_group(self.group);
        let size = ui.size();
        if size.width < FOLD_BELOW {
            self.narrow(&tweaks, size, ui);
        } else {
            self.wide(&tweaks, ui);
        }
    }

    /// The wide screen: groups in a sidebar, the list beside it, the detail in a side panel.
    fn wide(&self, tweaks: &[Tweak], ui: &mut View<'_, Msg>) {
        SidePanel::new(30)
            .side(Side::Right)
            .open(self.panel_open)
            .closed(Closed::Hide)
            .limits(20, 44)
            .on_toggle(Msg::TogglePanel)
            .panel(|ui| self.detail(tweaks, ui))
            .body(|ui| {
                ui.row(|ui| {
                    ui.add(
                        Menu::new([MenuGroup::new(
                            "groups",
                            Group::ALL.map(|group| MenuItem::new(group.key(), t!(&format!("group.{}", group.key())))),
                        )])
                        .selected(Some(self.group.key()))
                        .on_select(|key| Msg::PickGroup(key.to_owned())),
                    )
                    .id("groups");
                    ui.add(self.list(tweaks)).fill().id("tweaks");
                })
                .fill();
            })
            .show(ui);
    }

    /// The narrow screen: the groups as a strip of tabs, the list at full width below it, and
    /// the detail as a column under the list while it is open.
    ///
    /// The side panel stays, closed and hidden, because it is what hears the global `toggle-panel`
    /// key: the framework routes that key to a side panel above the focused widget rather than to
    /// the application, so without it `alt+b` would do nothing here. Its edge column is also the
    /// one place the mouse can open the detail from, the same way it opens the wide panel.
    fn narrow(&self, tweaks: &[Tweak], size: Size, ui: &mut View<'_, Msg>) {
        let open = self.detail_open;
        // The header, the strip and the footer take a row each. The list shows every row it has
        // up to half the rest, but never fewer than its minimum; the detail scrolls in what is left.
        let rows = u16::try_from(tweaks.len()).unwrap_or(u16::MAX);
        let list_rows = rows.min(size.height.saturating_sub(3) / 2).max(NARROW_LIST_ROWS);
        SidePanel::new(20)
            .side(Side::Right)
            .open(false)
            .closed(Closed::Hide)
            .on_toggle(move |_| Msg::ToggleDetail(!open))
            .body(|ui| {
                ui.column(|ui| {
                    let active = Group::ALL.iter().position(|group| *group == self.group).unwrap_or_default();
                    ui.add(
                        Tabs::new(Group::ALL.map(|group| t!(&format!("group.{}", group.key()))))
                            .active(active)
                            .on_select(|index| {
                                Msg::PickGroup(
                                    Group::ALL.get(index).map_or_else(String::new, |group| group.key().to_owned()),
                                )
                            }),
                    )
                    .fill_width()
                    .id("groups");
                    let list = ui.add(self.list(tweaks)).id("tweaks");
                    if open {
                        list.height(Length::Cells(list_rows));
                        ui.add_with(ScrollView::new(), |ui| {
                            ui.column(|ui| self.detail(tweaks, ui)).fill_width().padding(Padding::symmetric(1, 3));
                        })
                        .fill()
                        .id("detail");
                    } else {
                        list.fill();
                    }
                })
                .fill();
            })
            .show(ui)
            // Its own name: the wide panel at this place is open, and a fold must not read as
            // that panel sliding shut.
            .id("fold");
    }

    /// The tweaks of the current group, each with its state's mark and word.
    fn list(&self, tweaks: &[Tweak]) -> List<Msg> {
        List::new(tweaks.iter().enumerate().map(|(index, tweak)| {
            let state = self.state_of(index);
            ListItem::new(t!(&format!("tweak.{}.title", tweak.id)))
                .icon("dot", state_variant(&state))
                .detail(state_word(&state))
                .faint(matches!(state, TweakState::Unavailable(_)))
        }))
        .selected(Some(self.selected))
        .checked(tweaks.iter().enumerate().map(|(index, _)| self.is_checked(index)).collect())
        .on_select(Msg::Select)
        .on_toggle(Msg::Toggle)
        .on_activate(Msg::Apply)
    }

    /// The run: what it is doing, its terminal, and a way to close it once it has ended.
    fn run_view(running: &Running, ui: &mut View<'_, Msg>) {
        let tweaks = running.titles.join(", ");
        let title =
            if running.reverting { t!("run.reverting", tweak = tweaks) } else { t!("run.applying", tweak = tweaks) };
        ui.column(|ui| {
            ui.row(|ui| {
                ui.add(Text::new(title).role("title").no_wrap()).fill_width();
                if running.exit.is_some() {
                    let outcome = if running.succeeded() {
                        Badge::new(t!("run.done")).variant("success")
                    } else {
                        Badge::new(t!("run.failed")).variant("danger")
                    };
                    ui.add(outcome);
                    ui.add(Button::new(t!("run.close")).on_press(Msg::CloseRun)).id("close-run");
                }
            })
            .gap(2)
            .padding(Padding::symmetric(0, 2))
            .fill_width();
            if running.exit.is_some() && !running.succeeded() {
                ui.add(Text::new(t!("run.failed-body")).role("faint")).fill_width().padding(Padding::symmetric(0, 2));
            }
            ui.add(Terminal::new(&running.session)).width(Length::Fill(1)).height(Length::Fill(1)).id("run");
        })
        .fill();
    }

    fn detail(&self, tweaks: &[Tweak], ui: &mut View<'_, Msg>) {
        let Some(tweak) = tweaks.get(self.selected) else { return };
        let state = self.state_of(self.selected);
        ui.row(|ui| {
            ui.add(Text::new(t!(&format!("tweak.{}.title", tweak.id))).role("title").no_wrap());
            let badge = Badge::new(state_word(&state));
            ui.add(match state_variant(&state) {
                Some(variant) => badge.variant(variant),
                None => badge,
            });
        })
        .gap(2);
        ui.add(Text::new(t!(&format!("tweak.{}.summary", tweak.id)))).fill_width();
        ui.add(Text::new(t!("detail.touches")).role("faint"));
        for touch in tweak.touches() {
            ui.add(Text::new(run::touch_line(&touch))).fill_width();
        }
        // Reverting only makes sense once something is on the machine to undo; `Off` and
        // `Unavailable` have nothing a revert could touch.
        if matches!(state, TweakState::Applied | TweakState::Half | TweakState::Changed(_)) {
            ui.add(Button::new(t!("revert.button")).on_press(Msg::Revert(self.selected))).id("revert");
        }
    }

    fn footer(&self, ui: &mut View<'_, Msg>) {
        let env = ui.env();
        if let Some(running) = &self.running {
            let mut hints = KeyHints::new();
            if running.exit.is_some() {
                hints = hints.action(Scope::App, "close-run");
            }
            ui.add(hints.action_right(Scope::Global, "quit")).fill_width();
            return;
        }
        // `KeyHints` keeps every plain `.hint()` ahead of every `.action()`, dropping from the end
        // of that combined list first when the bar is too narrow. The hint to reopen a closed panel
        // matters most exactly when the terminal is narrow enough to have closed it, so it is given
        // as a hint (highest priority) rather than an action, ahead of the others.
        let mut hints = KeyHints::new();
        if let Some(chord) = env.keymap().chords_for(Scope::Global, "toggle-panel").first() {
            let label = env.i18n().translate(&Scope::Global.label_key("toggle-panel"), &[]);
            hints = hints.hint(chord.label(), label);
        }
        // The detail below the narrow list is closed by the same key as an ended run; the way out
        // of it is as important as the way in, so it comes right after that hint.
        if self.detail_open
            && ui.size().width < FOLD_BELOW
            && let Some(chord) = env.keymap().chords_for(Scope::App, "close-run").first()
        {
            let label = env.i18n().translate(&Scope::App.label_key("close-run"), &[]);
            hints = hints.hint(chord.label(), label);
        }
        ui.add(
            hints
                .hint("↑↓", t!("hints.move"))
                .hint("space", t!("hints.toggle"))
                .hint("enter", t!("hints.apply"))
                .action(Scope::App, "revert")
                .action(Scope::Global, "focus-next")
                .action_right(Scope::Global, "quit"),
        )
        .fill_width();
    }
}

/// The theme variant a state's mark and badge draw in, or `None` for a neutral tone.
fn state_variant(state: &TweakState) -> Option<&'static str> {
    match state {
        TweakState::Applied => Some("success"),
        TweakState::Off => None,
        TweakState::Half => Some("warning"),
        TweakState::Changed(_) => Some("danger"),
        TweakState::Unavailable(_) => None,
    }
}

/// The word next to a state's mark, in the active language.
fn state_word(state: &TweakState) -> String {
    match state {
        TweakState::Applied => t!("state.applied"),
        TweakState::Off => t!("state.off"),
        TweakState::Half => t!("state.half"),
        TweakState::Changed(_) => t!("state.changed"),
        TweakState::Unavailable(reason) => t!(reason),
    }
}

fn header<M: Clone + 'static>(ui: &mut View<'_, M>) {
    ui.add(
        Text::rich([
            Span::new("qtools").color("accent").bold(),
            Span::new(format!("  {}", t!("header.tagline"))).role("faint"),
        ])
        .no_wrap(),
    )
    .padding(Padding::symmetric(0, 2))
    .fill_width();
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod languages;

#[cfg(test)]
mod scenes;
