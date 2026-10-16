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
//!
//! Over the screen the command palette opens: everything the keys and the sidebar do from where the
//! screen stands, each with its key beside it, found by typing a few letters of its name. What
//! cannot be done right now is not in it: undo on an item qtools has not done itself, apply on an
//! item this machine cannot use, and the items' own commands while the Settings page is shown. A
//! run's terminal and the wizard have the screen, so `ctrl k` does nothing while either is up.

use std::path::{Path, PathBuf};

use qframe::env::Env;
use qframe::icons::nerd_font::Install;
use qframe::prelude::*;
use qframe::runtime::{Update, UpdateCheck};
use qframe::storage::{Ecosystem, Preferences, Settings};
use qframe::text;
use qframe::widgets::{
    Appearance, AppearanceChange, AppearanceSave, Badge, Button, Closed, CommandPalette, List, ListItem, Menu,
    MenuGroup, MenuItem, PaletteCommand, ScrollView, Setup, SetupMsg, Side, SidePanel, Tabs, Terminal, TerminalEvent,
    TerminalSession, Toast, ToastKind,
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
use crate::catalog::hardware::Cpu;
use crate::choices::{self, Choices};
use crate::session::Request;
use crate::tweak::{Group, OptionValue, Tweak, TweakState};

pub mod run;
use run::Running;

pub mod options;
use options::Options;
pub mod session;
use session::Live;

pub mod unsupported;

pub mod wizard;

mod settings;
use settings::{PASSWORD_ONCE, SETTINGS};

/// qtools' name among the Quvyta apps: its settings file is `tools.conf` in the shared Quvyta folder.
pub const APP: &str = "tools";

/// qtools' own settings file: `tools.conf` in the shared Quvyta folder `folder`, or the member's own
/// file for a machine with no folder of its own. Both halves of qtools open it through here, so the
/// values a run uses are the ones the screen showed.
#[must_use]
pub fn settings_in(folder: Option<&Path>) -> Settings {
    match folder {
        Some(folder) => Settings::open(folder.join(format!("{APP}.conf"))).member_of(&Ecosystem::QUVYTA),
        None => Settings::load_member(&Ecosystem::QUVYTA, APP),
    }
}

/// What a look at the machine found: where every tweak of the catalog stands, and which of
/// them qtools has on record, both in catalog order.
#[derive(Debug, Clone, Default)]
pub struct Reading {
    /// Where every tweak stands.
    pub states: Vec<TweakState>,
    /// Which steps of each tweak qtools' journal holds, in step order. A tweak with none is not
    /// one that can be undone: a timer the person turned on by hand shows as applied, but qtools
    /// did nothing there to undo. The steps are what an undo would put back, so the confirmation
    /// counts its passwords from them.
    pub recorded: Vec<Vec<usize>>,
}

/// Reads where every tweak of the catalog for `cpu` stands on `system`, and which of them the
/// journal in `folder` has on record.
///
/// The catalog is built with `choices`, the values a person chose, so every step's check compares
/// against the value that step would write: the list, the confirmation and the run that follows
/// them then agree with each other. A tweak whose state cannot be read says so, with what stopped
/// the reading, rather than stopping the screen from opening, and a journal that cannot be read
/// offers nothing to undo; the command line says why when an undo is asked for.
pub fn read(choices: &Choices, system: &mut dyn crate::system::System, cpu: Cpu, folder: &Path) -> Reading {
    let catalog = catalog::with(choices, cpu);
    let journal = crate::state::Journal::load(system, folder).unwrap_or_default();
    Reading {
        states: catalog.iter().map(|tweak| tweak.reading(system)).collect(),
        recorded: catalog.iter().map(|tweak| journal.steps(tweak.id)).collect(),
    }
}

/// Reads this machine, for the screen to show after a run or after a saved form, with the values
/// in force.
#[must_use]
pub fn read_machine(choices: &Choices) -> Reading {
    let mut system = crate::system::RealSystem::reading();
    let cpu = catalog::hardware::cpu(&mut system);
    read(choices, &mut system, cpu, &crate::state::folder())
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
    /// Which steps of every tweak qtools has on record, in catalog order: only a tweak with some
    /// is offered an undo.
    recorded: Vec<Vec<usize>>,
    /// The machine's processor, which decides what the catalog offers. It is read once, next
    /// to the states, so the list is drawn from what was read and not from the machine.
    cpu: Cpu,
    /// Which tweaks are checked for a run of several at once.
    pub checked: Vec<bool>,
    /// Whether the detail panel is open; closed gives the list the full width. Read on wide
    /// screens only.
    pub panel_open: bool,
    /// Whether the detail is open below the list. Read on narrow screens only, so folding and
    /// unfolding never disturb the wide panel's own state.
    pub detail_open: bool,
    /// Whether the command palette is open over the screen.
    palette: bool,
    /// The tweaks a confirmation is asking about, and whether it is an apply or a revert.
    /// Read again when the person answers, so [`Msg::Confirmed`] does not have to carry it.
    pending: Option<Pending>,
    /// The run under way, or just ended and still on screen. `None` before a confirmation
    /// and once the person closes the terminal.
    pub running: Option<Running>,
    /// The program a run starts in the embedded terminal: this very binary, with `--run` or
    /// `--revert` (see [`crate::cli`]). `None` starts nothing, which is what a screen that was
    /// not given a machine does.
    program: Option<PathBuf>,
    /// Reads where every tweak stands, after a run and after a saved form: the real machine with
    /// the values in force, a double in tests, or nothing at all for a screen that was not given
    /// a machine.
    reader: Option<fn(&Choices) -> Reading>,
    /// Where a session's control channel is kept while the `password-once` setting is on. `None`
    /// leaves every run a terminal of its own, as it was before there was a session.
    channel: Option<PathBuf>,
    /// The session that holds the one terminal every run shares, started with the first run and
    /// living until qtools goes, the setting is turned off or the session ends by itself.
    session: Option<Live>,
    /// The "Ask for the password once while qtools is open" switch, as it was last left: on
    /// unless `tools.conf` says otherwise.
    password_once: bool,
    /// How many runs were started, so a watch of an earlier run is told from the current one.
    runs: u64,
    /// The first-run wizard, while qtools has no `tools.conf`; `None` once it is finished or
    /// when it was never needed.
    setup: Option<Setup<Msg>>,
    /// `tools.conf` as held in memory: the shared Quvyta keys, which the wizard writes as it
    /// finishes, the appearance rows qtools keeps for itself, and the values a person chose for
    /// the tweaks' options.
    settings: Settings,
    /// The values the catalog is built with, read from `tools.conf`. They change only through the
    /// options form, which writes the file first and adopts them once the file holds them.
    choices: Choices,
    /// The options form of a tweak that has options, while it is open.
    form: Option<Options>,
    /// Whether the values of a closed form are still being written. The form is not opened again
    /// until that write has come back, so two writes never race each other for the file.
    saving: bool,
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
    /// The wizard's "Say when an update is out" box, as it was last left; written only when the
    /// wizard finishes. The Settings page's switch is the framework's and keeps its own value.
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

/// What became of a request that was to go through the session: `Started`, with the work the
/// screen owes a run that is now under way in the session's own terminal, or `Without`, with what
/// the person is told when there is no session to carry it out.
enum Held {
    /// The run is under way in the session's terminal.
    Started(Command<Msg>),
    /// It is not, and this is what the person is told about it; the run then happens in a
    /// terminal of its own.
    Without(Command<Msg>),
}

impl Tools {
    /// The screen, opened on the packages group, showing the states it was handed and the
    /// catalog of the machine whose processor is `cpu`. It knows no machine: nothing is on
    /// record, a confirmed run starts nothing and nothing is read after one, until
    /// [`Tools::machine`] and [`Tools::recorded`] say otherwise.
    ///
    /// It knows no Quvyta folder: the Settings page starts from the look this machine detects and
    /// keeps a change until qtools quits. [`Opening::new`] gives it the shared Quvyta one.
    pub fn new(states: Vec<TweakState>, cpu: Cpu) -> Self {
        let count = states.len();
        // A folder that is never created, so resolving reads no one's files and writes nothing.
        let nowhere = std::env::temp_dir().join("quvyta-tools-no-config");
        let detected = Ecosystem::QUVYTA.preferences_without_saving_in(&nowhere, APP, &crate::locales::i18n());
        Self {
            group: Group::Packages,
            settings_open: false,
            selected: 0,
            states,
            recorded: vec![Vec::new(); count],
            cpu,
            checked: vec![false; count],
            panel_open: true,
            detail_open: false,
            palette: false,
            pending: None,
            running: None,
            program: None,
            reader: None,
            channel: None,
            session: None,
            password_once: true,
            runs: 0,
            setup: None,
            settings: Settings::in_memory(),
            choices: Choices::recommended(),
            form: None,
            saving: false,
            appearance: settings::appearance(detected, None),
            config: None,
            backups: shown(&crate::state::folder()),
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
        self.update_notice = folders.as_ref().is_none_or(|folders| Ecosystem::QUVYTA.update_notice_in(&folders.config));
        self.updates = folders;
        self
    }

    /// The same screen, with runs started as `program` and the machine read by `reader` after
    /// each one: this very binary and [`read_machine`] for the real screen, doubles in tests. The
    /// values in force go with the reading, so a screen that has just saved a form is drawn from
    /// the values the file now holds.
    #[must_use]
    pub fn machine(mut self, program: PathBuf, reader: fn(&Choices) -> Reading) -> Self {
        self.program = Some(program);
        self.reader = Some(reader);
        self
    }

    /// The same screen, with the values the catalog is built from, as a test or a machine without
    /// a Quvyta folder chooses rather than as `tools.conf` says.
    #[must_use]
    pub fn choices(mut self, choices: Choices) -> Self {
        self.choices = choices;
        self
    }

    /// The same screen, whose runs share one terminal for as long as qtools is open while the
    /// `password-once` setting is on, so `sudo` asks for the password once. `folder` is where
    /// that session keeps its control channel (see [`session::channel_folder`]) and holds the
    /// real screen's runtime folder; `None` leaves every run a terminal of its own, which is what
    /// a screen that was given no machine or no place for a channel does.
    #[must_use]
    pub fn one_terminal(mut self, folder: Option<PathBuf>) -> Self {
        self.channel = folder;
        self
    }

    /// The same screen, knowing which steps of every tweak qtools has on record, in catalog order.
    #[must_use]
    pub fn recorded(mut self, recorded: Vec<Vec<usize>>) -> Self {
        self.recorded = recorded;
        self
    }

    /// The question for a newer version of qtools, when the Quvyta-wide update notice is on.
    ///
    /// The switch is read here, not only where the question is sent: a machine where it is off
    /// asks nothing at all, whoever runs the question.
    fn ask_for_update(&self) -> Command<Msg> {
        let Some(folders) = &self.updates else { return Command::none() };
        if !Ecosystem::QUVYTA.update_notice_in(&folders.config) {
            return Command::none();
        }
        let check = UpdateCheck::new(
            Ecosystem::QUVYTA,
            APP,
            env!("CARGO_PKG_NAME"),
            env!("CARGO_PKG_VERSION"),
            Msg::NewVersion,
        )
        .in_folders(folders.config.clone(), folders.state.clone());
        Command::check_for_update(check)
    }

    /// The wizard's box is in the shared file now, so the Settings page, which carries on from the
    /// wizard's choices, reads it from there and shows the switch as the box left it.
    fn notice_stored(&mut self) {
        if let Some(folder) = &self.config {
            let preferences = Ecosystem::QUVYTA.preferences_without_saving_in(folder, APP, &crate::locales::i18n());
            self.appearance.refresh(preferences);
        }
    }

    /// Writes the wizard's box to the Quvyta-wide switch, off the drawing thread, when it differs
    /// from what the shared file says; otherwise asks at once. Only called as the wizard
    /// finishes, so nothing is written before Finish.
    fn store_update_notice(&self) -> Command<Msg> {
        let Some(folders) = &self.updates else { return Command::none() };
        if Ecosystem::QUVYTA.update_notice_in(&folders.config) == self.update_notice {
            return self.ask_for_update();
        }
        let (folder, on) = (folders.config.clone(), self.update_notice);
        Command::perform(move || {
            Msg::NoticeStored(Ecosystem::QUVYTA.set_update_notice_in(&folder, on).map_err(|error| error.to_string()))
        })
    }

    /// Reads where every tweak stands, off the drawing thread. Used after a run, a revert or a
    /// saved options form, so the list shows what the machine now says rather than what it said
    /// before, read with the values in force: a step's check compares against the value its step
    /// would write, and a value that changed is not applied yet.
    fn refresh(&self) -> Command<Msg> {
        let Some(reader) = self.reader else { return Command::none() };
        let choices = self.choices.clone();
        Command::perform(move || Msg::StatesRead(reader(&choices)))
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
    /// test's) with the tweaks standing as `states` and the processor `cpu`. `fonts`, when given,
    /// is the only folder the wizard looks in for a Nerd Font and the one it would install into,
    /// without registering it; tests give one so no real font is looked at. Without a Quvyta
    /// folder there is nowhere to write what the wizard asks, so it does not open.
    ///
    /// Nothing is written here while the wizard is wanted: its preferences are resolved without
    /// saving, where resolving them the usual way would make `quvyta.conf` before anything was
    /// chosen.
    pub fn new(folder: Option<&Path>, fonts: Option<&Path>, states: Vec<TweakState>, cpu: Cpu) -> Self {
        let ecosystem = Ecosystem::QUVYTA;
        let i18n = crate::locales::i18n();
        let setup = folder
            .map(|folder| {
                let setup = Setup::new_in(folder, ecosystem, APP, &i18n, Msg::Setup).on_finish(Msg::SetUp);
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
            (None, Some(folder)) => ecosystem.preferences_in(folder, APP, &i18n),
            (None, None) => ecosystem.preferences(APP, &i18n),
        };
        let settings = settings_in(folder);
        let mut tools = Tools::new(states, cpu);
        tools.setup = setup;
        tools.settings = settings.clone();
        // The values the catalog is built from come out of the same file, so the list, the
        // detail panel and the run that follows it all work from what the person chose.
        tools.choices = Choices::read(&settings);
        tools.appearance = settings::appearance(preferences.clone(), folder);
        tools.config = folder.map(Path::to_path_buf);
        // The password is asked once while qtools is open unless the file says otherwise; the
        // switch on the Settings page writes the key from there on.
        tools.password_once = settings.get_or(PASSWORD_ONCE, true);
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
        let ecosystem = Ecosystem::QUVYTA;
        ecosystem.config_dir().zip(ecosystem.state_dir(APP)).map(|(config, state)| Self { config, state })
    }
}

/// The first key bound to `action` in `scope`, in the form the hint bar writes it (`"ctrl q"`,
/// `"z"`), or `None` when the action is unbound: a command with no key shows no key beside it.
fn first_key(env: &Env, scope: Scope, action: &str) -> Option<String> {
    env.keymap().label_for(scope, action)
}

/// `folder` as a person reads it: under their home folder it starts with `~`.
fn shown(folder: &Path) -> String {
    qframe::storage::display_home(folder)
}

impl Default for Tools {
    /// Every tweak off, and a processor qtools has no microcode package for: only used where
    /// nothing has been read yet, such as a screen test that sends its own states.
    fn default() -> Self {
        Self::new(vec![TweakState::Off; catalog::all(Cpu::default()).len()], Cpu::default())
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
    /// The command palette opened over the screen.
    OpenPalette,
    /// The command palette closed: `esc`, the mark beside the filter, a click on the dimmed
    /// screen, or a command that ran and is on its way.
    ClosePalette,
    /// Leave qtools, the way the `quit` key does.
    Quit,
    /// The options form of the chosen tweak was asked for, by the `a` key or by the button in
    /// the detail panel.
    Options,
    /// The value of the option at this place of the options form changed.
    OptionValue(usize, OptionValue),
    /// Save was asked for on the options form.
    OptionSave,
    /// The options form was closed without saving: Cancel, `esc` or the close mark.
    OptionCancel,
    /// The values of the options form were written to `tools.conf`, or why they could not be.
    OptionsStored {
        /// The settings as they were written, taken into use once they are on disk.
        settings: Settings,
        /// The Quvyta folder the file is in, which a failure names.
        folder: Option<PathBuf>,
        /// Nothing, or the reason the file could not be written.
        result: Result<(), String>,
    },
    /// Where every tweak of the catalog stands, freshly read off the machine.
    StatesRead(Reading),
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
    /// The session carrying out run number `.0` is done with it: `Some` is the code the child of
    /// the session chose, `None` that the session ended before it could answer.
    Answered(u64, Option<u32>),
    /// Closes the embedded terminal once its process has ended.
    CloseRun,
    /// The wizard's "Say when an update is out" box was turned. It waits for Finish; the Settings
    /// page's switch is the framework's row and comes as an [`Appearance`](Msg::Appearance).
    ToggleUpdateNotice(bool),
    /// The Quvyta-wide update notice was written as the wizard asked, or it could not be.
    NoticeStored(Result<(), String>),
    /// A newer version of qtools is out.
    NewVersion(Update),
    /// A change on the Settings page's rows.
    Appearance(AppearanceChange),
    /// Another Quvyta application changed the shared language, theme, icons or reduced motion;
    /// the runtime has already switched the screen, the Settings page still has to show it and
    /// write the next pick where its boxes now say.
    Preferences(Preferences),
    /// The Settings page's update switch was written in the background, or why not.
    AppearanceSaved(AppearanceSave),
    /// The "Ask for the password once while qtools is open" switch of the Settings page was
    /// turned, or the file could not be written.
    TogglePasswordOnce(bool),
    /// The password-once switch was written into `tools.conf`, or it could not be.
    PasswordOnceSaved {
        /// What it was turned to.
        on: bool,
        /// Nothing, or the reason the file could not be written.
        result: Result<(), String>,
    },
}

impl App for Tools {
    type Msg = Msg;

    fn preferences(&self, preferences: &Preferences) -> Option<Msg> {
        Some(Msg::Preferences(preferences.clone()))
    }

    fn update(&mut self, msg: Msg) -> Command<Msg> {
        match msg {
            Msg::PickGroup(key) => return self.pick(&key),
            Msg::Select(index) if index < self.group().len() => self.selected = index,
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
            Msg::OpenPalette => self.palette = true,
            Msg::ClosePalette => self.palette = false,
            // qtools asks nothing before it quits, so the palette's quit is the quit key's: the
            // runtime's own decision, never asked about.
            Msg::Quit => return Command::quit(),
            Msg::Options => return self.open_options(),
            Msg::OptionValue(index, value) => return self.change_option(index, value),
            Msg::OptionSave => return self.save_options(),
            Msg::OptionCancel => return self.cancel_options(),
            // The values are only taken into use once the file holds them, so a failure leaves the
            // list and the detail showing what was really saved.
            Msg::OptionsStored { settings, result: Ok(()), .. } => {
                self.settings = settings;
                self.choices = Choices::read(&self.settings);
                self.saving = false;
                return self.refresh();
            }
            Msg::OptionsStored { folder, result: Err(reason), .. } => return self.options_not_stored(&folder, &reason),
            Msg::StatesRead(reading) => {
                self.states = reading.states;
                self.recorded = reading.recorded;
            }
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
            Msg::Answered(run, code) => return self.answered(run, code),
            Msg::CloseRun => {
                if self.running.as_ref().is_some_and(|running| running.exit.is_some()) {
                    self.running = None;
                    if let Some(update) = self.waiting_update.take() {
                        return Command::toast(update.toast());
                    }
                }
            }
            Msg::ToggleUpdateNotice(on) => self.update_notice = on,
            Msg::TogglePasswordOnce(on) => return self.password_once(on),
            Msg::PasswordOnceSaved { on, result } => return self.password_once_saved(on, result),
            // The switch was written as the person chose it; the question follows it, which asks
            // nothing when they turned it off.
            Msg::NoticeStored(Ok(())) => {
                self.notice_stored();
                return self.ask_for_update();
            }
            Msg::NoticeStored(Err(reason)) => {
                return Command::toast(Toast::new(ToastKind::Danger, t!("quvyta.setup.not-saved", reason = reason)));
            }
            // A toast floats over the terminal's bottom corner, where its note gives the exit
            // code, so while a run is on screen the news waits for the list to come back.
            Msg::NewVersion(update) if self.running.is_some() => self.waiting_update = Some(update),
            Msg::NewVersion(update) => return Command::toast(update.toast()),
            // The framework's rows write their own files, key by key, and keep the settings qtools
            // holds in step, so nothing more is saved here.
            Msg::Appearance(change) => {
                return self.appearance.update_saving(change, &mut self.settings, Msg::AppearanceSaved);
            }
            Msg::Preferences(preferences) => self.appearance.refresh(preferences),
            Msg::AppearanceSaved(save) => return self.appearance_saved(&save),
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
        // The palette is a layer over the whole screen, so it is drawn after the frame rather than
        // inside it: it dims the columns, the header and the footer alike, and only the screen it
        // is asked for knows whether the detail is the wide panel or the one below a narrow list.
        if self.palette {
            let size = ui.size();
            ui.add(self.palette(ui.env(), size));
        }
    }

    fn action(&self, name: &str) -> Option<Msg> {
        // Behind the wizard the list is not on screen, so none of its keys may act on it.
        if self.setting_up() {
            return None;
        }
        match name {
            // The run's terminal has the screen while it is up, and its own keys with it.
            "palette" if self.running.is_none() => Some(Msg::OpenPalette),
            // The Settings page has no tweak on screen for the key to undo.
            "revert" if self.running.is_none() && !self.settings_open => Some(Msg::Revert(self.selected)),
            // A run has the screen to itself, the Settings page has no tweak on it, and a tweak
            // without options has nothing to ask about: none of them shows the key or opens the
            // form.
            "options" if self.can_open_options() => Some(Msg::Options),
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
    /// The whole catalog, built with the values in force and for the machine's own processor.
    /// Everything the screen shows about a tweak comes from here, so one place decides what its
    /// steps write and what they check.
    fn catalog(&self) -> Vec<Tweak> {
        catalog::with(&self.choices, self.cpu)
    }

    /// The tweaks of the current group, in catalog order.
    fn group(&self) -> Vec<Tweak> {
        catalog::in_group(self.group, self.cpu, &self.choices)
    }

    /// The chosen tweak of the current group, or `None` before anything is on screen.
    fn chosen(&self) -> Option<Tweak> {
        self.group().get(self.selected).cloned()
    }

    /// Whether the chosen tweak has options to change, which is also when the `a` key and its
    /// hint belong on the screen: a run has it to itself, the Settings page has no tweak on it, a
    /// tweak without options has nothing to ask about, and a value whose write is still on its
    /// way is not asked for again before it has landed.
    fn can_open_options(&self) -> bool {
        self.form.is_none()
            && !self.saving
            && self.running.is_none()
            && !self.settings_open
            && self.chosen().is_some_and(|tweak| !tweak.options.is_empty())
    }

    /// Where a tweak of the current group sits in the whole catalog.
    fn catalog_index(&self, index: usize) -> Option<usize> {
        self.positions().get(index).copied()
    }

    /// Where each tweak of the current group sits in the whole catalog, in group order, from one
    /// build of the catalog: the list asks this for every row on every frame.
    fn positions(&self) -> Vec<usize> {
        let group = self.group;
        self.catalog().iter().enumerate().filter(|(_, tweak)| tweak.group == group).map(|(at, _)| at).collect()
    }

    /// Where the tweak at `index` of the current group stands, or off when it has never been
    /// read. The fallback is a defensive guard, not a real case: `index` always comes from the
    /// current group, which is built from the same catalog as `self.states`.
    fn state_of(&self, index: usize) -> TweakState {
        self.state_at(self.catalog_index(index))
    }

    /// Where the tweak at catalog place `at` stands; see [`Self::state_of`].
    fn state_at(&self, at: Option<usize>) -> TweakState {
        at.and_then(|at| self.states.get(at)).cloned().unwrap_or(TweakState::Off)
    }

    /// Whether qtools has the tweak at `index` of the current group on record, so it can undo it.
    fn is_recorded(&self, index: usize) -> bool {
        self.catalog_index(index).and_then(|at| self.recorded.get(at)).is_some_and(|steps| !steps.is_empty())
    }

    /// Whether the tweak at `index` of the current group is checked to run with the others. The
    /// fallback is the same defensive guard as [`Self::state_of`].
    fn is_checked(&self, index: usize) -> bool {
        self.checked_at(self.catalog_index(index))
    }

    /// Whether the tweak at catalog place `at` is checked; see [`Self::is_checked`].
    fn checked_at(&self, at: Option<usize>) -> bool {
        at.and_then(|at| self.checked.get(at)).copied().unwrap_or(false)
    }

    /// The catalog places a run of `index` (of the current group) would cover: every checked
    /// tweak when any are checked, otherwise just the one activated.
    fn chosen_indices(&self, index: usize) -> Vec<usize> {
        if self.checked.iter().any(|checked| *checked) {
            return self.checked.iter().enumerate().filter_map(|(at, checked)| checked.then_some(at)).collect();
        }
        self.catalog_index(index).into_iter().collect()
    }

    /// Opens the options form of the chosen tweak, if it has options to change. The dialog takes
    /// the keyboard for itself, so the list and the detail behind it take no key until it closes.
    fn open_options(&mut self) -> Command<Msg> {
        if !self.can_open_options() {
            return Command::none();
        }
        let Some(tweak) = self.chosen() else { return Command::none() };
        if tweak.options.is_empty() {
            return Command::none();
        }
        self.form = Some(Options::new(self.selected, tweak.options));
        Command::none()
    }

    /// A value on the options form was typed. It is kept whatever it is: whether it may be saved
    /// is asked on Save, so a number that is not finished yet is not refused while it is typed.
    fn change_option(&mut self, index: usize, value: OptionValue) -> Command<Msg> {
        if let Some(form) = self.form.as_mut() {
            form.set(index, value);
        }
        Command::none()
    }

    /// Save on the options form: every field is checked at once, the first one that fails takes
    /// the keyboard with its reason under it, and nothing is written. When they all pass, the
    /// values go to `tools.conf` in the background and the form closes; they are taken into use
    /// only once the file holds them, which is [`Msg::OptionsStored`].
    fn save_options(&mut self) -> Command<Msg> {
        let Some(mut form) = self.form.take() else { return Command::none() };
        let errors = form.check();
        if !errors.is_empty() {
            self.form = Some(form);
            return errors.focus_first();
        }
        let values = form.saved();
        let mut settings = self.settings.clone();
        for (option, value) in &values {
            choices::store(&mut settings, option, value);
        }
        let folder = settings.path().map(Path::to_path_buf);
        self.saving = true;
        Command::perform(move || {
            let result = settings.save().map_err(|error| error.to_string());
            Msg::OptionsStored { settings, folder, result }
        })
    }

    /// Cancel, `esc` or the close mark on the options form: it closes and nothing is written.
    fn cancel_options(&mut self) -> Command<Msg> {
        self.form = None;
        Command::none()
    }

    /// The values of a form could not be written, so they are not used: the list and the detail
    /// keep showing what the file really holds, and a notice says which folder and why.
    fn options_not_stored(&mut self, folder: &Option<PathBuf>, reason: &str) -> Command<Msg> {
        self.saving = false;
        let body = folder.as_ref().map_or_else(String::new, |folder| {
            // The folder is what could not be written; the reason alone does not say where.
            format!("{}\n{reason}", shown(folder))
        });
        Command::toast(Toast::new(ToastKind::Danger, t!("form.not-saved")).body(body))
    }

    /// Asks whether to apply (or, with `revert`, undo) the tweak at `index` of the current
    /// group, or every checked tweak when any are checked. A tweak that does not fit this
    /// machine is never asked about, the same guard [`Msg::Toggle`] uses; nothing is asked
    /// while a run is on screen either.
    fn ask(&mut self, index: usize, revert: bool) -> Command<Msg> {
        if self.running.is_some() || self.settings_open {
            return Command::none();
        }
        // An undo is offered for what qtools has on record, whatever the machine fits now; an
        // apply only for a tweak that fits.
        let refused =
            if revert { !self.is_recorded(index) } else { matches!(self.state_of(index), TweakState::Unavailable(_)) };
        if refused {
            return Command::none();
        }
        let mut indices = self.chosen_indices(index);
        if revert {
            indices.retain(|&at| self.recorded.get(at).is_some_and(|steps| !steps.is_empty()));
        }
        if indices.is_empty() {
            return Command::none();
        }
        let catalog = self.catalog();
        let tweaks: Vec<run::Chosen> = indices
            .iter()
            .filter_map(|&at| {
                catalog.get(at).map(|tweak| run::Chosen {
                    tweak: tweak.clone(),
                    state: self.state_at(Some(at)),
                    on_record: self.recorded.get(at).cloned().unwrap_or_default(),
                })
            })
            .collect();
        self.pending = Some(Pending { indices, revert });
        let title = if revert { t!("confirm.revert.title") } else { t!("confirm.apply.title") };
        let backups = format!("{}/backups", self.backups);
        let mut confirm = Confirm::new(title, Msg::Confirmed).message(run::plan_text(&tweaks, revert, &backups));
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
    ///
    /// While the `password-once` setting is on the run is not a process of its own: it is a
    /// request to the session, which carries it out in the terminal that session already holds
    /// (see [`session`]). The first confirmed run starts that session; the ones after it are
    /// lines to it, and `sudo` asks for the password in that one terminal once.
    fn confirmed(&mut self) -> Command<Msg> {
        let Some(pending) = self.pending.take() else { return Command::none() };
        // A screen that was given no machine has nothing to run the tweaks with.
        let Some(program) = self.program.clone() else {
            return Command::toast(Toast::new(ToastKind::Danger, t!("run.failed")));
        };
        let catalog = self.catalog();
        let chosen: Vec<&Tweak> = pending.indices.iter().filter_map(|&at| catalog.get(at)).collect();
        if chosen.is_empty() {
            return Command::none();
        }
        let ids: Vec<&str> = chosen.iter().map(|tweak| tweak.id).collect();
        let titles: Vec<String> = chosen.iter().map(|tweak| t!(&format!("tweak.{}.title", tweak.id))).collect();
        self.runs += 1;
        let run = self.runs;
        if self.password_once {
            // The request takes the ids with them, so the words that go down the channel are the
            // screen's own and the session never has to know what a tweak is.
            let owned: Vec<String> = ids.iter().map(|id| (*id).to_owned()).collect();
            let request = if pending.revert { Request::Revert(owned) } else { Request::Run(owned) };
            return match self.held(&program, &request, titles.clone(), run) {
                // The run is under way in the session's own terminal.
                Held::Started(command) => command,
                // There is no session to carry it out, and the person is told why: the run goes
                // ahead in a terminal of its own, exactly as it did before there was a session.
                Held::Without(said) => {
                    Command::batch([said, self.started(&program, &ids, titles, pending.revert, run)])
                }
            };
        }
        self.started(&program, &ids, titles, pending.revert, run)
    }

    /// Starts the run as a process of its own in a terminal of its own: what happens when the
    /// `password-once` setting is off, and what happens when there is no session to share one.
    fn started(
        &mut self,
        program: &Path,
        ids: &[&str],
        titles: Vec<String>,
        reverting: bool,
        run: u64,
    ) -> Command<Msg> {
        match Running::start(program, ids, titles, reverting, run) {
            Ok(running) => {
                let watch = Self::watch(&running);
                self.running = Some(running);
                self.checked.iter_mut().for_each(|checked| *checked = false);
                Command::batch([watch, Command::focus("run")])
            }
            Err(error) => Command::toast(Toast::new(ToastKind::Danger, t!("run.failed")).body(error.to_string())),
        }
    }

    /// Starts `request` in the session's own terminal, so the password asked there is asked once
    /// for the whole qtools, and watches it to its end. The session is started with the first
    /// request and lives until qtools goes, the setting is turned off, or it ends by itself.
    fn held(&mut self, program: &Path, request: &Request, titles: Vec<String>, run: u64) -> Held {
        let following = match self.asked(program, run, request) {
            Ok(following) => following,
            // Without a session to carry it out the person is told why, and the run goes ahead in
            // a terminal of its own, exactly as it did before there was a session.
            Err(reason) => return Held::Without(Command::toast(Toast::new(ToastKind::Warning, reason))),
        };
        let (terminal, watch) = following;
        self.running =
            Some(Running { titles, reverting: request.is_revert(), session: terminal, run, exit: None, held: true });
        self.checked.iter_mut().for_each(|checked| *checked = false);
        Held::Started(Command::batch([watch, Command::focus("run")]))
    }

    /// Asks the session to carry `request` out in the terminal it holds, starting that session
    /// with the first request, and hands back what the screen follows it with: the terminal to
    /// draw and the command that redraws it as the session writes and ends the run when the
    /// answer comes. `Err` is what the person is told when there is no session to carry it out.
    fn asked(
        &mut self,
        program: &Path,
        run: u64,
        request: &Request,
    ) -> Result<(TerminalSession, Command<Msg>), String> {
        if self.session.is_none() {
            let Some(folder) = self.channel.clone() else { return Err(t!("session.no-folder")) };
            // A session that cannot be started is not the end of the run, so the reason is said
            // rather than swallowed: it is what tells the person why the password is asked again
            // in a terminal of its own.
            let live = Live::start(program, &folder)
                .map_err(|error| t!("session.channel-not-made", reason = error.to_string()))?;
            self.session = Some(live);
        }
        // The session that is there is the one just started or the one an earlier run left up. A
        // channel that cannot be written to is a session that is gone or going: the next run starts
        // a new one rather than write into a terminal nobody is listening on.
        let asked = self.session.as_mut().ok_or_else(|| t!("session.gone"))?.ask(run, request);
        match asked {
            Ok(following) => Ok(following),
            Err(error) => {
                let reason = error.to_string();
                self.session = None;
                Err(t!("session.not-asked", reason = reason))
            }
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
        let Some(running) = self.running.as_ref().filter(|running| running.run == run) else {
            return Command::none();
        };
        // A held terminal is the session's, which stays alive between runs: the run's end comes
        // over the channel, and the task of that run draws the terminal as the session writes to
        // it. Only the session's own end is heard here, and it means the run is not coming back.
        let held = running.held;
        if held {
            return match event {
                TerminalEvent::Output => Command::none(),
                TerminalEvent::Exited(_) => self.session_gone(),
            };
        }
        match event {
            TerminalEvent::Output => Self::watch(running),
            TerminalEvent::Exited(code) => {
                // The outcome is drawn beside the run's title rather than as a toast: a toast
                // floats over the terminal's bottom corner, where its note gives the exit code.
                if let Some(running) = self.running.as_mut() {
                    running.exit = Some(code);
                }
                self.refresh()
            }
        }
    }

    /// The command palette over the screen: every action of qtools that the screen's own state
    /// allows right now, each with the key that does the same thing beside it.
    ///
    /// A command that cannot do anything now is left out rather than shown and refused, by the
    /// rules the screen's own Undo button and the `z` key keep: undo is offered only for a tweak
    /// qtools has on record, since a service the person turned on by hand shows as applied and
    /// there is nothing of qtools' there to undo; apply and check only for a tweak that fits this
    /// machine; and no tweak command at all while the Settings page is showing, since no tweak is
    /// on screen there. The groups and Settings are always offered, because they are how one
    /// leaves whatever is shown; the detail is offered where the screen has a detail to show.
    ///
    /// Running a command sends the message its key sends, so the confirmation, the run and the
    /// undo are the same work either way round.
    ///
    /// `env` is the frame's own environment: the keys are read from its keymap, so a rebound key
    /// shows beside its own command.
    fn palette(&self, env: &Env, size: Size) -> CommandPalette<Msg> {
        let mut commands = Vec::new();
        // A command with no key beside it says nothing about keys, so the key is only given where
        // the keymap has one.
        let mut add = |id: String, label: String, key: Option<String>, message: Msg| {
            let command = PaletteCommand::new(id, label, message);
            commands.push(match key {
                Some(key) => command.chord(key),
                None => command,
            });
        };
        let key = |scope: Scope, action: &str| first_key(env, scope, action);
        let narrow = size.width < FOLD_BELOW;
        // The detail is a panel beside the list on a wide screen and a column under it on a narrow
        // one, and the Settings page has neither.
        if !self.settings_open {
            let open = if narrow { self.detail_open } else { self.panel_open };
            let label = if open { "palette.hide-detail" } else { "palette.show-detail" };
            let message =
                if narrow { Msg::ToggleDetail(!self.detail_open) } else { Msg::TogglePanel(!self.panel_open) };
            add("detail".to_owned(), t!(label), key(Scope::Global, "toggle-panel"), message);
        }
        // The item's own commands, in the order the footer offers them. The page shows no item, so
        // none of them is offered there.
        if !self.settings_open {
            let state = self.state_of(self.selected);
            if let Some(title) = self.selected_title() {
                // An item that does not fit this machine cannot be applied or checked, the same
                // guard `Msg::Apply` and `Msg::Toggle` keep.
                if !matches!(state, TweakState::Unavailable(_)) {
                    add(
                        "apply".to_owned(),
                        t!("palette.apply", tweak = title.clone()),
                        Some("enter".to_owned()),
                        Msg::Apply(self.selected),
                    );
                    let checked = self.is_checked(self.selected);
                    let label = if checked { "palette.unmark" } else { "palette.mark" };
                    let id = if checked { "unmark" } else { "mark" };
                    add(
                        id.to_owned(),
                        t!(label, tweak = title.clone()),
                        Some("space".to_owned()),
                        Msg::Toggle(self.selected),
                    );
                }
                // The item's own values, where it has any: the same form the `a` key opens.
                if self.can_open_options() {
                    add(
                        "options".to_owned(),
                        t!("palette.options", tweak = title.clone()),
                        key(Scope::App, "options"),
                        Msg::Options,
                    );
                }
                // Undoing is offered only for what qtools itself did and has on record, the rule
                // the detail's own Undo button and the `z` key follow: a service the person turned
                // on by hand stands as applied, and there is nothing of qtools' there to undo.
                if self.is_recorded(self.selected) {
                    add(
                        "undo".to_owned(),
                        t!("palette.undo", tweak = title),
                        key(Scope::App, "revert"),
                        Msg::Revert(self.selected),
                    );
                }
            }
        }
        // The groups and the page are how one leaves whatever is shown, so they are always there.
        for group in Group::ALL {
            add(
                format!("group.{}", group.key()),
                t!(&format!("group.{}", group.key())),
                None,
                Msg::PickGroup(group.key().to_owned()),
            );
        }
        add("settings".to_owned(), t!("settings.title"), None, Msg::PickGroup(SETTINGS.to_owned()));
        add("quit".to_owned(), t!("palette.quit"), key(Scope::Global, "quit"), Msg::Quit);
        CommandPalette::new(commands, Msg::ClosePalette)
    }

    /// The chosen tweak's title in the active language, or nothing when the list has no row for
    /// the selection. The same text the list and the detail head themselves with.
    fn selected_title(&self) -> Option<String> {
        let id = catalog::in_group(self.group, self.cpu, &self.choices).get(self.selected)?.id;
        Some(t!(&format!("tweak.{id}.title")))
    }

    /// The session of a run is over before it could answer it: the run did not go through, the
    /// badge says so, and the next run starts a session of its own rather than write into a
    /// terminal nobody is listening on.
    fn session_gone(&mut self) -> Command<Msg> {
        self.session = None;
        if let Some(running) = self.running.as_mut() {
            // What the tweaks did is not known any more, and the code the session itself ended
            // with says nothing about them: a run that did not go through is what this shows.
            running.exit.get_or_insert(Some(session::UNANSWERED));
        }
        Command::batch([
            Command::toast(Toast::new(ToastKind::Danger, t!("run.failed")).body(t!("session.gone"))),
            self.refresh(),
        ])
    }

    /// The session carrying out a run is done with it: the code it chose is what the badge says,
    /// and `None` is a session that ended before it could answer (see [`Self::session_gone`]).
    fn answered(&mut self, run: u64, code: Option<u32>) -> Command<Msg> {
        if self.running.as_ref().is_none_or(|running| running.run != run || running.exit.is_some()) {
            return Command::none();
        }
        match code {
            Some(code) => {
                if let Some(running) = self.running.as_mut() {
                    running.exit = Some(Some(code));
                }
                self.refresh()
            }
            None => self.session_gone(),
        }
    }

    fn body(&self, ui: &mut View<'_, Msg>) {
        if let Some(running) = &self.running {
            Self::run_view(running, ui);
            return;
        }
        let tweaks = self.group();
        let size = ui.size();
        if self.settings_open {
            self.settings_body(size, ui);
        } else if size.width < FOLD_BELOW {
            self.narrow(&tweaks, size, ui);
        } else {
            self.wide(&tweaks, ui);
        }
        // The form is a dialog over whatever is on screen, and it is added last so it stands
        // above the confirmation, should a person open one over the other.
        if let Some(form) = &self.form {
            options::view(form, ui);
        }
    }

    /// The wide screen: groups in a sidebar, the list beside it, the detail in a side panel.
    ///
    /// The panel's own content scrolls, as the detail under the narrow list does: a detail with
    /// options on it is longer than a short terminal, and what is below the fold has to be
    /// reachable rather than lost.
    fn wide(&self, tweaks: &[Tweak], ui: &mut View<'_, Msg>) {
        SidePanel::new(30)
            .side(Side::Right)
            .open(self.panel_open)
            .closed(Closed::Hide)
            .limits(20, 44)
            .on_toggle(Msg::TogglePanel)
            .panel(|ui| {
                ui.add_with(ScrollView::new(), |ui| self.detail(tweaks, ui)).fill();
            })
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
        // Each row's place in the catalog is found once for the whole list, not once per row.
        let positions = self.positions();
        List::new(tweaks.iter().enumerate().map(|(index, tweak)| {
            let state = self.state_at(positions.get(index).copied());
            ListItem::new(t!(&format!("tweak.{}.title", tweak.id)))
                .icon("dot", state_variant(&state))
                .detail(state_word(&state))
                .faint(matches!(state, TweakState::Unavailable(_)))
        }))
        .selected(Some(self.selected))
        .checked((0..tweaks.len()).map(|index| self.checked_at(positions.get(index).copied())).collect())
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
            // `esc` belongs to the application and not to the program, so the run view can be
            // closed while the terminal behind it is still there: a session lives on, and a key
            // that types a character still goes to the program as it always has.
            ui.add(Terminal::new(&running.session).pass_through(Scope::App, "close-run"))
                .width(Length::Fill(1))
                .height(Length::Fill(1))
                .id("run");
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
        // A changed or unread item says what it found, so the badge's one word is not all the
        // person learns before deciding what to do.
        if let TweakState::Changed(found) | TweakState::Unknown(found) = &state {
            let found = Text::new(found.text());
            ui.add(match state_variant(&state) {
                Some(variant) => found.color(variant),
                None => found,
            })
            .fill_width()
            .id("found");
        }
        ui.add(Text::new(t!(&format!("tweak.{}.summary", tweak.id)))).fill_width();
        self.options(tweak, ui);
        ui.add(Text::new(t!("detail.touches")).role("faint"));
        for touch in tweak.touches() {
            ui.add(Text::new(run::touch_line(&touch))).fill_width();
        }
        // Undoing is offered only for what qtools itself did: a service the person turned on by
        // hand shows as applied, and there is nothing of qtools' there to undo.
        if self.is_recorded(self.selected) {
            ui.add(Button::new(t!("revert.button")).on_press(Msg::Revert(self.selected))).id("revert");
        }
    }

    /// The tweak's options with the values in force, under the summary and above what the tweak
    /// touches: the label on the left in faint words and the value at the right end of the row,
    /// so the values line up whatever their length. The way in is the button under them, which
    /// the `a` key and the mouse both reach. A tweak without options draws nothing here at all.
    fn options(&self, tweak: &Tweak, ui: &mut View<'_, Msg>) {
        for option in &tweak.options {
            let value = option_word(&option.value);
            // The value is as wide as it needs and no wider, so it stands at the right end of the
            // row however narrow the panel is, and the label wraps beside it rather than cutting
            // it off.
            let cells = qframe::text::width(&value).saturating_add(1);
            ui.row(|ui| {
                ui.add(Text::new(t!(&format!("option.{}", option.key))).role("faint")).fill_width();
                ui.add(Text::new(value).no_wrap()).width(Length::Cells(cells));
            })
            .fill_width();
        }
        if !tweak.options.is_empty() {
            ui.add(Button::new(t!("form.button")).on_press(Msg::Options)).id("options");
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
        // A narrow bar drops hints from its end first. The hint to reopen a closed panel matters
        // most exactly when the terminal is narrow enough to have closed it, so it comes first and
        // is the last to go.
        let mut hints = KeyHints::new().action_first(Scope::Global, "toggle-panel");
        // The detail below the narrow list is closed by the same key as an ended run; the way out
        // of it is as important as the way in, so it comes right after that hint.
        if self.detail_open
            && ui.size().width < FOLD_BELOW
            && let Some(chord) = env.keymap().chords_for(Scope::App, "close-run").first()
        {
            let label = env.i18n().translate(&Scope::App.label_key("close-run"), &[]);
            hints = hints.hint(chord.label(), label);
        }
        // The chosen tweak's own options are what `a` is for, so the hint is there only while
        // there is a tweak on screen that has them.
        if self.can_open_options() {
            hints = hints.action(Scope::App, "options");
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
        TweakState::Half(_) => Some("warning"),
        TweakState::Changed(_) => Some("danger"),
        TweakState::Unavailable(_) => None,
        TweakState::Unknown(_) => Some("info"),
    }
}

/// The word next to a state's mark, in the active language.
fn state_word(state: &TweakState) -> String {
    match state {
        TweakState::Applied => t!("state.applied"),
        TweakState::Off => t!("state.off"),
        TweakState::Half(_) => t!("state.half"),
        TweakState::Changed(_) => t!("state.changed"),
        TweakState::Unavailable(reason) => t!(reason),
        TweakState::Unknown(_) => t!("state.unknown"),
    }
}

/// A value as the detail panel words it: a number as it is written, and a choice as its own
/// label rather than as a place in a row of them, so the panel says "yay".
fn option_word(value: &OptionValue) -> String {
    match value {
        OptionValue::Text(text) => text.clone(),
        OptionValue::Count(count) => count.to_string(),
        OptionValue::Choice { chosen, options } => match options.get(*chosen) {
            Some(key) => t!(&format!("option.{key}")),
            None => String::new(),
        },
    }
}

/// The header: qtools and its tagline on the left, and the way to the command palette on the right
/// as the design draws it, `ctrl k  commands`.
///
/// The hint is drawn the way the footer draws its own, from the keymap and the language files, so a
/// rebound key and another language both say themselves. It is left out where the name and the
/// tagline would leave it no room rather than cut in half, which is what a narrow screen does; the
/// keys that matter most at that width are in the footer.
fn header<M: Clone + 'static>(ui: &mut View<'_, M>) {
    let commands = commands_hint(ui);
    ui.row(|ui| {
        ui.add(
            Text::rich([
                Span::new("qtools").color("accent").bold(),
                Span::new(format!("  {}", t!("header.tagline"))).role("faint"),
            ])
            .no_wrap(),
        );
        if let Some(hints) = commands {
            ui.add(hints);
        }
    })
    .padding(Padding::symmetric(0, 2))
    .fill_width();
}

/// The palette's hint for the header's corner, or nothing where it does not fit.
///
/// The cells are the ones the hint bar draws with: the key on its own pad, a cell between, the
/// label, and the two cells the bar keeps at each end. A person who unbound the key gets no hint,
/// for there is no key then to point at.
fn commands_hint<M: Clone + 'static>(ui: &View<'_, M>) -> Option<KeyHints> {
    let env = ui.env();
    let chord = env.keymap().label_for(Scope::Global, "palette")?;
    let label = env.i18n().translate(&Scope::Global.label_key("palette"), &[]);
    let name = text::width("qtools") + 2 + text::width(&t!("header.tagline"));
    let hint = text::width(chord.as_str()) + 3 + text::width(&label) + 4;
    // Four more for the two cells the header keeps at its own ends.
    (name + hint + 4 <= ui.size().width).then(|| KeyHints::new().action_right(Scope::Global, "palette"))
}

#[cfg(test)]
pub(crate) mod tests;

#[cfg(test)]
mod palette_tests;

#[cfg(test)]
mod update_tests;

#[cfg(test)]
mod settings_tests;

#[cfg(test)]
mod options_tests;

#[cfg(test)]
mod languages;

#[cfg(test)]
mod scenes;

#[cfg(test)]
mod link_tests;
