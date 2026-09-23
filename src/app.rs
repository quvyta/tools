//! The screen: groups on the left, tweaks in the middle, the chosen tweak's detail on the right.
//!
//! `view` reads the width it is drawn in and folds below `FOLD_BELOW` columns: the groups
//! leave the sidebar for a strip of tabs above the list, the list takes the full width, and the
//! detail is not shown beside it. There `alt+b` (the same toggle that closes the wide panel, or
//! the edge column of the closed panel under the mouse) opens the detail as a column below the
//! list, and `esc` closes it. At the fold and above the three columns sit side by side, and the
//! detail panel can be closed by hand to give the list the full width.
//!
//! The last entry of the sidebar, and the last tab of the strip, is the Settings page, which takes
//! the place of the list and its detail while it is shown.
//!
//! While a run is under way the columns give way to the embedded terminal that carries it out,
//! and come back when the person closes it. On the first start the first-run wizard
//! ([`crate::app::wizard`]) has the whole screen instead, until it is finished.

use std::path::{Path, PathBuf};

use qframe::icons::nerd_font::Install;
use qframe::prelude::*;
use qframe::runtime::{Update, UpdateCheck};
use qframe::storage::{Family, Preferences, Settings};
use qframe::widgets::{
    Appearance, AppearanceChange, Badge, Button, Closed, List, ListItem, Menu, MenuGroup, MenuItem, ScrollView, Setup,
    SetupMsg, Side, SidePanel, Tabs, Terminal, TerminalEvent, Toast, ToastKind,
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

pub mod wizard;

mod settings;
use settings::SETTINGS;

/// qtools' name among the Quvyta apps: its settings file is `tools.conf` in the shared Quvyta folder.
pub const APP: &str = "tools";

/// Reads where every tweak stands, on this machine.
fn read_states() -> Vec<TweakState> {
    let mut system = crate::system::RealSystem;
    catalog::all().iter().map(|tweak| tweak.state(&mut system).unwrap_or(TweakState::Off)).collect()
}

/// The application's state.
#[derive(Debug)]
pub struct Tools {
    /// Which group the sidebar has selected. It stays while the Settings page is shown, so the
    /// list comes back as it was.
    pub group: Group,
    /// Whether the Settings page has the place of the list and its detail.
    pub settings_open: bool,
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
    /// The first-run wizard, while qtools has no `tools.conf`; `None` once it is finished or
    /// when it was never needed.
    setup: Option<Setup<Msg>>,
    /// `tools.conf` as held in memory: the shared Quvyta keys, which the wizard writes as it
    /// finishes, and the appearance rows qtools keeps for itself.
    settings: Settings,
    /// The Settings page's appearance rows, which apply and write each change themselves.
    appearance: Appearance,
    /// The shared Quvyta folder the Settings page writes into; `None` keeps a change until qtools
    /// quits, for a machine without one.
    config: Option<PathBuf>,
    /// Where the backups a revert restores from are kept, as the wizard names it.
    backups: String,
    /// Where the Quvyta-wide update notice is kept and where the last question is remembered;
    /// `None` asks nothing and leaves the wizard's box out, since it would change nothing.
    updates: Option<UpdateFolders>,
    /// The "Say when an update is out" switch, as it was last left. The wizard's box writes it
    /// only when the wizard finishes; the Settings page writes it as it is turned.
    update_notice: bool,
    /// A newer version that was found while a run was on screen, said once the run is closed.
    waiting_update: Option<Update>,
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
    ///
    /// It knows no Quvyta folder: the Settings page starts from the look this machine detects and
    /// keeps a change until qtools quits. [`Opening::new`] gives it the shared Quvyta one.
    pub fn new(states: Vec<TweakState>) -> Self {
        let count = states.len();
        // A folder that is never created, so resolving reads no one's files and writes nothing.
        let nowhere = std::env::temp_dir().join("quvyta-tools-no-config");
        let detected = Family::QUVYTA.preferences_without_saving_in(&nowhere, APP, &crate::locales::i18n());
        Self {
            group: Group::Packages,
            settings_open: false,
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
            setup: None,
            settings: Settings::in_memory(),
            appearance: settings::appearance(detected, None),
            config: None,
            backups: shown(&crate::state::folder(), std::env::var("HOME").ok().as_deref()),
            updates: None,
            update_notice: true,
            waiting_update: None,
        }
    }

    /// The same screen, asking at start whether a newer qtools is out, with the Quvyta-wide switch
    /// in `folders.config` and the time of the last question in `folders.state`. Tests give
    /// folders of their own, so nothing they do reads or turns off the person's own switch.
    #[must_use]
    pub fn updates(mut self, folders: Option<UpdateFolders>) -> Self {
        self.update_notice = folders.as_ref().is_none_or(|folders| Family::QUVYTA.update_notice_in(&folders.config));
        self.updates = folders;
        self
    }

    /// The same screen, with runs started as `program` and states read by `reader` instead of
    /// this binary and the real machine. Tests point both at doubles.
    #[must_use]
    pub fn machine(mut self, program: PathBuf, reader: fn() -> Vec<TweakState>) -> Self {
        self.program = program;
        self.reader = reader;
        self
    }

    /// The question for a newer version of qtools, when the Quvyta-wide update notice is on.
    ///
    /// The switch is read here, not only where the question is sent: a machine where it is off
    /// asks nothing at all, whoever runs the question.
    fn ask_for_update(&self) -> Command<Msg> {
        let Some(folders) = &self.updates else { return Command::none() };
        if !Family::QUVYTA.update_notice_in(&folders.config) {
            return Command::none();
        }
        let check =
            UpdateCheck::new(Family::QUVYTA, APP, env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"), Msg::NewVersion)
                .in_folders(folders.config.clone(), folders.state.clone());
        Command::check_for_update(check)
    }

    /// Writes the wizard's box to the Quvyta-wide switch, off the drawing thread, when it differs
    /// from what the shared file says; otherwise asks at once. Only called as the wizard
    /// finishes, so nothing is written before Finish.
    fn store_update_notice(&self) -> Command<Msg> {
        let Some(folders) = &self.updates else { return Command::none() };
        if Family::QUVYTA.update_notice_in(&folders.config) == self.update_notice {
            return self.ask_for_update();
        }
        let (folder, on) = (folders.config.clone(), self.update_notice);
        Command::perform(move || {
            Msg::NoticeStored(Family::QUVYTA.set_update_notice_in(&folder, on).map_err(|error| error.to_string()))
        })
    }

    /// Reads where every tweak stands, off the drawing thread. Used after a run or a revert,
    /// so the list shows what the machine now says rather than what it said before.
    fn refresh(&self) -> Command<Msg> {
        let reader = self.reader;
        Command::perform(move || Msg::StatesRead(reader()))
    }
}

/// What the screen starts with: qtools itself, with the wizard when it is wanted, its settings
/// file and the shared Quvyta look the runtime opens in.
#[derive(Debug)]
pub struct Opening {
    /// The screen, holding the wizard on a first start.
    pub tools: Tools,
    /// `tools.conf`, for the runtime's saved look.
    pub settings: Settings,
    /// The shared Quvyta language, theme and icons, in force from the first frame.
    pub preferences: Preferences,
}

impl Opening {
    /// Builds the screen for a machine whose Quvyta folder is `folder` (the platform's own, or a
    /// test's) with the tweaks standing as `states`. `fonts`, when given, is the only folder the
    /// wizard looks in for a Nerd Font and the one it would install into, without registering
    /// it; tests give one so no real font is looked at. Without a Quvyta folder there is nowhere
    /// to write what the wizard asks, so it does not open.
    ///
    /// Nothing is written here while the wizard is wanted: its preferences are resolved without
    /// saving, where resolving them the usual way would make `quvyta.conf` before anything was
    /// chosen.
    pub fn new(folder: Option<&Path>, fonts: Option<&Path>, states: Vec<TweakState>) -> Self {
        let family = Family::QUVYTA;
        let i18n = crate::locales::i18n();
        let setup = folder
            .map(|folder| {
                let setup = Setup::new_in(folder, family, APP, &i18n, Msg::Setup).on_finish(Msg::SetUp);
                match fonts {
                    Some(fonts) => setup
                        .install(Install::new().target(fonts.join("QuvytaNerdFont")).register(false))
                        .font_dirs(vec![fonts.to_path_buf()]),
                    None => setup,
                }
            })
            .filter(Setup::needed);
        let preferences = match (&setup, folder) {
            (Some(setup), _) => setup.preferences().clone(),
            (None, Some(folder)) => family.preferences_in(folder, APP, &i18n),
            (None, None) => family.preferences(APP, &i18n),
        };
        let settings = match folder {
            Some(folder) => Settings::open(folder.join(format!("{APP}.conf"))).member_of(&family),
            None => Settings::load_member(&family, APP),
        };
        let mut tools = Tools::new(states);
        tools.setup = setup;
        tools.settings = settings.clone();
        tools.appearance = settings::appearance(preferences.clone(), folder);
        tools.config = folder.map(Path::to_path_buf);
        Self { tools, settings, preferences }
    }
}

impl Opening {
    /// The same opening, asking whether a newer qtools is out over `folders`; see
    /// [`Tools::updates`].
    #[must_use]
    pub fn with_updates(mut self, folders: Option<UpdateFolders>) -> Self {
        self.tools = self.tools.updates(folders);
        self
    }
}

/// Where the Quvyta-wide update notice is kept and where qtools remembers when it last asked for a
/// newer version of itself.
///
/// The switch is Quvyta-wide, one for every Quvyta application, so it is read from the shared
/// Quvyta folder. The last question is remembered in the Quvyta state folder for qtools, which is not
/// the folder qtools keeps its backups in: those stay where they have always been.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpdateFolders {
    /// The shared Quvyta configuration folder, whose shared file holds the switch.
    pub config: PathBuf,
    /// qtools' state folder under the Quvyta one, which remembers when the question was last asked.
    pub state: PathBuf,
}

impl UpdateFolders {
    /// This machine's folders, or `None` without a home folder, where nothing could remember the
    /// switch or the last question and so nothing is asked.
    #[must_use]
    pub fn here() -> Option<Self> {
        let family = Family::QUVYTA;
        family.config_dir().zip(family.state_dir(APP)).map(|(config, state)| Self { config, state })
    }
}

/// `folder` as a person reads it: under their home folder it starts with `~`.
fn shown(folder: &Path, home: Option<&str>) -> String {
    match home.filter(|home| !home.is_empty()).and_then(|home| folder.strip_prefix(home).ok()) {
        Some(rest) => format!("~/{}", rest.display()),
        None => folder.display().to_string(),
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
    /// Something on the framework's step of the first-run wizard, or on its buttons.
    Setup(SetupMsg),
    /// The wizard wrote the shared keys and made `tools.conf`.
    SetUp,
    /// The embedded terminal of run number `.0` has new output, or its process ended.
    Changed(u64, TerminalEvent),
    /// Closes the embedded terminal once its process has ended.
    CloseRun,
    /// The "Say when an update is out" box of the wizard, or switch of the Settings page, was
    /// turned.
    ToggleUpdateNotice(bool),
    /// The Quvyta-wide update notice was written as the wizard asked, or it could not be.
    NoticeStored(Result<(), String>),
    /// A newer version of qtools is out.
    NewVersion(Update),
    /// A change on the Settings page's rows.
    Appearance(AppearanceChange),
    /// The Settings page's update switch was written into `folder`, or why not.
    NoticeSaved {
        /// What it was turned to.
        on: bool,
        /// The shared Quvyta folder it was written into.
        folder: PathBuf,
        /// Nothing, or the reason the file could not be written.
        result: Result<(), String>,
    },
}

impl App for Tools {
    type Msg = Msg;

    fn update(&mut self, msg: Msg) -> Command<Msg> {
        match msg {
            Msg::PickGroup(key) => return self.pick(&key),
            Msg::Select(index) if index < catalog::in_group(self.group).len() => self.selected = index,
            Msg::Select(_) => {}
            Msg::Toggle(index) => {
                // A tweak that does not fit this machine is never among the checked ones a run
                // applies, and none is on screen to check while the Settings page is.
                if self.settings_open || matches!(self.state_of(index), TweakState::Unavailable(_)) {
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
            // The framework owns its step: it applies each change at once, writes the two files
            // when the wizard finishes, and answers with `Msg::SetUp`.
            Msg::Setup(msg) => {
                if let Some(setup) = self.setup.as_mut() {
                    return setup.update(msg, &mut self.settings);
                }
            }
            Msg::SetUp => return self.finish_setup(),
            Msg::Changed(run, event) => return self.changed(run, event),
            Msg::CloseRun => {
                if self.running.as_ref().is_some_and(|running| running.exit.is_some()) {
                    self.running = None;
                    if let Some(update) = self.waiting_update.take() {
                        return Command::toast(update.toast());
                    }
                }
            }
            Msg::ToggleUpdateNotice(on) => return self.toggle_update_notice(on),
            // The switch was written as the person chose it; the question follows it, which asks
            // nothing when they turned it off.
            Msg::NoticeStored(Ok(())) => return self.ask_for_update(),
            Msg::NoticeStored(Err(reason)) => {
                return Command::toast(Toast::new(ToastKind::Danger, t!("quvyta.setup.not-saved", reason = reason)));
            }
            // A toast floats over the terminal's bottom corner, where its note gives the exit
            // code, so while a run is on screen the news waits for the list to come back.
            Msg::NewVersion(update) if self.running.is_some() => self.waiting_update = Some(update),
            Msg::NewVersion(update) => return Command::toast(update.toast()),
            // The framework's rows write their own files, key by key, and keep the settings qtools
            // holds in step, so nothing more is saved here.
            Msg::Appearance(change) => return self.appearance.update(change, &mut self.settings),
            Msg::NoticeSaved { on, folder, result } => return self.notice_saved(on, &folder, result),
        }
        Command::none()
    }

    fn init(&mut self) -> Command<Msg> {
        // On the first start the wizard has the screen, so its appearance rows take the keys, and
        // the question waits until it is over: the person may be about to turn it off.
        if self.setting_up() { Command::focus(wizard::FIRST) } else { self.ask_for_update() }
    }

    fn view(&self, ui: &mut View<'_, Msg>) {
        if self.setting_up() {
            self.setup_wizard(ui);
            return;
        }
        AppShell::new().header(header).body(|ui| self.body(ui)).footer(|ui| self.footer(ui)).show(ui);
    }

    fn action(&self, name: &str) -> Option<Msg> {
        // Behind the wizard the list is not on screen, so none of its keys may act on it.
        if self.setting_up() {
            return None;
        }
        match name {
            // The Settings page has no tweak on screen for the key to undo.
            "revert" if self.running.is_none() && !self.settings_open => Some(Msg::Revert(self.selected)),
            "close-run" if self.running.as_ref().is_some_and(|running| running.exit.is_some()) => Some(Msg::CloseRun),
            // `esc` is bound once, as `close-run`, and closes whatever sits on top: the ended
            // run, or the detail the narrow screen opened below the list.
            "close-run" if self.running.is_none() && self.detail_open && !self.settings_open => {
                Some(Msg::ToggleDetail(false))
            }
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
        if self.running.is_some() || self.settings_open || matches!(self.state_of(index), TweakState::Unavailable(_)) {
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
        if self.settings_open {
            self.settings_body(size, ui);
        } else if size.width < FOLD_BELOW {
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
                    ui.add(self.menu()).id("groups");
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
                    ui.add(self.strip()).fill_width().id("groups");
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

    /// Shows the group with `key`, or the Settings page.
    fn pick(&mut self, key: &str) -> Command<Msg> {
        let was_open = self.settings_open;
        if key == SETTINGS {
            self.settings_open = true;
        } else if let Some(group) = Group::ALL.into_iter().find(|group| group.key() == key) {
            // Coming back to the group that was open keeps the tweak chosen in it.
            if group != self.group {
                self.selected = 0;
            }
            self.group = group;
            self.settings_open = false;
        }
        // The page and the list are laid out differently, so the sidebar or the strip the key
        // was pressed on is drawn anew, under a name of its own on each; the keys follow it there.
        match (was_open, self.settings_open) {
            (false, true) => Command::focus(settings::SIDEBAR),
            (true, false) => Command::focus("groups"),
            _ => Command::none(),
        }
    }

    /// The sidebar: the groups of tweaks, then, apart from them, the Settings page.
    fn menu(&self) -> Menu<Msg> {
        let groups = Group::ALL.map(|group| MenuItem::new(group.key(), t!(&format!("group.{}", group.key()))));
        let selected = if self.settings_open { SETTINGS } else { self.group.key() };
        Menu::new([
            MenuGroup::new("groups", groups),
            MenuGroup::new("app", [MenuItem::new(SETTINGS, t!("settings.title"))]),
        ])
        .selected(Some(selected))
        .on_select(|key| Msg::PickGroup(key.to_owned()))
    }

    /// The narrow screen's strip: a tab for each group, and the Settings page last.
    fn strip(&self) -> Tabs<Msg> {
        let labels = Group::ALL
            .iter()
            .map(|group| t!(&format!("group.{}", group.key())))
            .chain(std::iter::once(t!("settings.title")));
        let active = if self.settings_open {
            Group::ALL.len()
        } else {
            Group::ALL.iter().position(|group| *group == self.group).unwrap_or_default()
        };
        Tabs::new(labels)
            .active(active)
            .on_select(|index| Msg::PickGroup(Group::ALL.get(index).map_or(SETTINGS, |group| group.key()).to_owned()))
    }

    /// The Settings page beside the sidebar, or under the strip on a narrow screen.
    fn settings_body(&self, size: Size, ui: &mut View<'_, Msg>) {
        if size.width < FOLD_BELOW {
            ui.column(|ui| {
                ui.add(self.strip()).fill_width().id(settings::SIDEBAR);
                self.settings_page(ui);
            })
            .fill();
        } else {
            ui.row(|ui| {
                ui.add(self.menu()).id(settings::SIDEBAR);
                self.settings_page(ui);
            })
            .fill();
        }
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
        if self.settings_open {
            Self::settings_hints(ui);
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
mod update_tests;

#[cfg(test)]
mod settings_tests;

#[cfg(test)]
mod languages;

#[cfg(test)]
mod scenes;
