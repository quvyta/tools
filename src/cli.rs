//! The `--run`, `--revert` and `--session` command lines: the half of qtools that touches the
//! machine.
//!
//! The screen starts its own binary with `--run` or `--revert` inside an embedded terminal, so
//! every command, `sudo`'s password prompt included, talks to a terminal of its own. The same
//! lines can be typed by hand: `qtools --run mirrors pacman-options`.
//!
//! `--session <control>` is the other half: the screen starts it once, in a terminal that lives
//! as long as qtools does, and each run and revert is a `--run`/`--revert` child inheriting that
//! terminal, so `sudo` asks for the password once rather than once per run. The control channel
//! is two named pipes the screen makes; the screen writes the ids to apply there and reads each
//! child's exit code back through the sibling. See [`crate::session`] for what a request is and
//! how the ticket `sudo` holds is kept and dropped. It is a program line like the others, and it
//! is not what a person types.

use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::Arc;

use qframe::prelude::*;
use qframe::storage::Ecosystem;

use crate::catalog::hardware::Cpu;
use crate::choices::Choices;
use crate::state::{self, Journal};
use crate::system::{RealSystem, System};
use crate::tweak::Tweak;
use crate::{catalog, locales, session};

/// What the command line asks for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Invocation {
    /// No arguments: the screen opens.
    Screen,
    /// `--run <id>...` applies these tweaks in order; `--revert <id>...` undoes them.
    Run {
        /// The tweak ids, in the order given.
        ids: Vec<String>,
        /// Whether the tweaks are undone rather than applied.
        revert: bool,
    },
    /// `--help` or `-h`: the usage, on standard output.
    Help,
    /// `--version` or `-V`: the name and version.
    Version,
    /// `--session <control>`: the process that holds one terminal for the whole qtools session
    /// and carries out the requests that arrive on the control channel at `control`. The exit
    /// codes of those requests go back through the sibling channel, `<control>.answers`.
    Session {
        /// The named pipe the requests arrive in.
        control: PathBuf,
    },
    /// Something else, which is shown back with the usage.
    Unknown(String),
}

/// Reads the arguments after the program name.
pub fn parse(args: impl IntoIterator<Item = String>) -> Invocation {
    let mut args = args.into_iter();
    match args.next().as_deref() {
        None => Invocation::Screen,
        Some(flag @ ("--run" | "--revert")) => Invocation::Run { ids: args.collect(), revert: flag == "--revert" },
        Some("--help" | "-h") => Invocation::Help,
        Some("--version" | "-V") => Invocation::Version,
        // One path, and nothing after it: a channel is one path, and the sibling it answers
        // through is named after it.
        Some("--session") => match (args.next(), args.next()) {
            (Some(control), None) => Invocation::Session { control: PathBuf::from(control) },
            _ => Invocation::Unknown("--session".to_owned()),
        },
        Some(other) => Invocation::Unknown(other.to_owned()),
    }
}

/// Carries out a run, a revert, a session or an unknown invocation, in the language the
/// environment asks for, and says what happened line by line on standard output.
///
/// The values the catalog is built with come out of the same `tools.conf` the screen read, so a
/// run carries out exactly what the list and the confirmation showed.
///
/// The exit code is 0 when every tweak went through, 1 when one failed (the rest are not run),
/// 2 when the arguments made no sense, and 1 for a session whose channel could not be opened:
/// nothing could be asked of it then. The distribution is checked before this is called; see
/// [`unsupported`] for the code it gives.
///
/// # Errors
///
/// Fails when the journal cannot be read or written, or standard output is closed.
pub fn carry_out(invocation: &Invocation) -> io::Result<ExitCode> {
    let choices = crate::choices_in(Ecosystem::QUVYTA.config_dir().as_deref());
    carry_out_on(invocation, &choices, &mut RealSystem::new(), &state::folder())
}

/// The same, with the values the catalog is built from, the machine it runs on and the folder the
/// backups are kept in all given, so that a test can stand in for each of them: nothing here
/// reaches the machine a person is sitting at.
///
/// # Errors
///
/// Fails when the journal cannot be read or written, or standard output is closed.
pub fn carry_out_on(
    invocation: &Invocation,
    choices: &Choices,
    system: &mut dyn System,
    folder: &Path,
) -> io::Result<ExitCode> {
    let env = locales::env();
    qframe::i18n::scope(Arc::new(env.i18n().clone()), || match invocation {
        Invocation::Screen => Ok(ExitCode::SUCCESS),
        Invocation::Run { ids, revert } => run(ids, *revert, choices, system, folder),
        Invocation::Help => {
            println!("{}", t!("cli.usage"));
            Ok(ExitCode::SUCCESS)
        }
        Invocation::Version => {
            println!("qtools {}", env!("CARGO_PKG_VERSION"));
            Ok(ExitCode::SUCCESS)
        }
        Invocation::Session { control } => hold(control),
        Invocation::Unknown(argument) => {
            eprintln!("{}", t!("cli.unknown-argument", argument = argument.as_str()));
            eprintln!("{}", t!("cli.usage"));
            Ok(ExitCode::from(2))
        }
    })
}

/// What a `--run` or `--revert` line carries out, built with the values a person chose.
#[derive(Debug, Clone)]
pub struct Plan {
    /// The tweaks, in the order the line named them.
    pub tweaks: Vec<Tweak>,
    /// Whether they are undone rather than applied.
    pub revert: bool,
}

/// Why a line names nothing to carry out.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlanError {
    /// The line named no tweak at all.
    Empty,
    /// The line named a tweak qtools does not know.
    Unknown(String),
}

/// The tweaks a line names, in the order it names them, built with `choices` for the machine
/// whose processor is `cpu`.
///
/// This is the whole of a command line that touches the machine, kept apart from carrying it
/// out: it builds the catalog and answers, and runs nothing. A test can therefore read off the
/// commands and the files a line would write, with the values a person chose, without a machine
/// anywhere.
pub fn plan(ids: &[String], revert: bool, choices: &Choices, cpu: Cpu) -> Result<Plan, PlanError> {
    if ids.is_empty() {
        return Err(PlanError::Empty);
    }
    let catalog = catalog::with(choices, cpu);
    let mut tweaks = Vec::new();
    for id in ids {
        match catalog.iter().find(|tweak| tweak.id == id) {
            Some(tweak) => tweaks.push(tweak.clone()),
            None => return Err(PlanError::Unknown(id.clone())),
        }
    }
    Ok(Plan { tweaks, revert })
}

/// Says, in the language the environment asks for, that this distribution is not supported
/// yet, and gives the exit code for it: 3, apart from the codes a run itself uses.
pub fn unsupported() -> ExitCode {
    let env = locales::env();
    qframe::i18n::scope(Arc::new(env.i18n().clone()), || eprintln!("{}", t!("unsupported.message")));
    ExitCode::from(3)
}

/// Holds the terminal of a whole qtools session open for the screen that started this process,
/// and carries out the requests that arrive on `control` until it is closed. A channel that
/// cannot be opened is a failure the screen cannot see, so it is said here and given the code a
/// failed run has.
fn hold(control: &Path) -> io::Result<ExitCode> {
    let mut out = io::stdout();
    match session::serve(control, &mut out) {
        Ok(()) => Ok(ExitCode::SUCCESS),
        Err(error) => {
            let channel = control.display().to_string();
            let _ = writeln!(out, "{}", t!("session.channel-failed", channel = channel, reason = error.to_string()));
            let _ = out.flush();
            Ok(ExitCode::from(1))
        }
    }
}

fn run(
    ids: &[String],
    revert: bool,
    choices: &Choices,
    system: &mut dyn System,
    folder: &Path,
) -> io::Result<ExitCode> {
    // The catalog is built for the machine it is about to change: an item that installs a
    // package for a processor asks the machine whether it is that processor.
    let plan = match plan(ids, revert, choices, catalog::hardware::cpu(system)) {
        Ok(plan) => plan,
        Err(PlanError::Unknown(id)) => {
            eprintln!("{}", t!("cli.unknown-tweak", id = id.as_str()));
            return Ok(ExitCode::from(2));
        }
        Err(PlanError::Empty) => {
            eprintln!("{}", t!("cli.usage"));
            return Ok(ExitCode::from(2));
        }
    };
    let revert = plan.revert;
    let chosen = plan.tweaks;
    let mut journal = Journal::load(system, folder)?;
    let mut out = io::stdout().lock();
    let last = chosen.len() - 1;
    for (index, tweak) in chosen.into_iter().enumerate() {
        let title = t!(&format!("tweak.{}.title", tweak.id));
        let heading = if revert { t!("cli.reverting", tweak = title) } else { t!("cli.applying", tweak = title) };
        writeln!(out, "{heading}")?;
        out.flush()?;
        let result = if revert {
            tweak.revert(system, &mut journal, folder).map_err(|problem| problem.text())
        } else {
            tweak.apply(system, &mut journal, folder).map_err(|failure| match failure.step {
                Some(step) => t!("cli.step-failed", step = step + 1, reason = failure.reason.text()),
                None => failure.reason.text(),
            })
        };
        match result {
            Ok(()) => writeln!(out, "  {}", t!("cli.done"))?,
            Err(reason) => {
                writeln!(out, "  {}", t!("cli.failed", reason = reason))?;
                if index < last {
                    writeln!(out, "{}", t!("cli.stopped"))?;
                }
                return Ok(ExitCode::from(1));
            }
        }
    }
    Ok(ExitCode::SUCCESS)
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::process::ExitCode;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use crate::step::Step;
    use crate::system::{FakeSystem, Output, System};

    use super::*;

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|arg| (*arg).to_owned()).collect()
    }

    /// A folder of its own under the system's temporary folder, removed with everything in it when
    /// the test ends. Nothing a test writes lands anywhere else, and the machine a run reaches is
    /// a [`FakeSystem`] that answers only what it was told to answer.
    struct Root(PathBuf);

    impl Root {
        fn new() -> Self {
            static NEXT: AtomicUsize = AtomicUsize::new(0);
            let n = NEXT.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!("qtools-cli-{}-{n}", std::process::id()));
            let _ = fs::remove_dir_all(&path);
            fs::create_dir_all(path.join("config")).expect("a temporary Quvyta folder");
            Self(path)
        }

        /// qtools' settings file, holding `text` as if a person had written it.
        fn settings_of(&self, text: &str) {
            fs::write(self.0.join("config/tools.conf"), text).expect("the settings file");
        }

        /// What qtools would use on this machine, read the way [`crate::run_on`] reads it.
        fn choices(&self) -> Choices {
            crate::choices_in(Some(&self.0.join("config")))
        }
    }

    impl Drop for Root {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    /// What a command answers when it worked, and what it answers when it did not.
    fn ok() -> Output {
        Output { code: 0, stdout: String::new(), stderr: String::new() }
    }

    /// A command that failed, so the step that runs it is one thing still to do.
    fn no() -> Output {
        Output { code: 1, stdout: String::new(), stderr: String::new() }
    }

    /// The opening of Arch's own `/etc/pacman.conf`, trimmed: `[options]` is the section the
    /// parallel downloads go in, and a file without it would be refused rather than edited.
    const STOCK_PACMAN_CONF: &str = "\
[options]
HoldPkg     = pacman glibc
Architecture = auto
CheckSpace
SigLevel    = Required DatabaseOptional

[core]
Include = /etc/pacman.d/mirrorlist
";

    /// A machine where the mirror list, the packages and the timer are all still to do, and the
    /// only file on it is pacman's own configuration.
    fn clean_machine() -> FakeSystem {
        let mut system = FakeSystem::new();
        system.file("/etc/pacman.conf", STOCK_PACMAN_CONF);
        for package in ["reflector", "base-devel", "git"] {
            system.answer(&format!("pacman -Q {package}"), ok());
            system.answer(&format!("sudo pacman -S --needed --noconfirm {package}"), ok());
        }
        system.answer("sh -c grep -q 'generated by Reflector' /etc/pacman.d/mirrorlist", no());
        system.answer("sudo reflector @/etc/xdg/reflector/reflector.conf", ok());
        for unit in ["reflector.timer", "paccache.timer", "fstrim.timer"] {
            system.answer(&format!("systemctl is-enabled {unit}"), no());
            system.answer(&format!("systemctl is-active {unit}"), no());
            system.answer(&format!("sudo systemctl enable --now {unit}"), ok());
        }
        system
    }

    /// The AUR helper item's own build line, as the machine sees it.
    fn build_line(helper: &str) -> String {
        let tweak = catalog::packages::aur_helper(if helper == "yay" { "yay" } else { "paru" });
        let Some(Step::CommandRun { run, .. }) = tweak.steps.last() else { panic!("the build is a command") };
        run.argv().join(" ")
    }

    /// What the steps of a plan would write, read off the steps themselves: the files with what
    /// would go into them.
    fn written(plan: &Plan) -> Vec<(String, String)> {
        plan.tweaks
            .iter()
            .flat_map(|tweak| &tweak.steps)
            .filter_map(|step| match step {
                Step::FileReplaced { path, content } | Step::FileManaged { path, content } => {
                    Some(((*path).to_owned(), content.replace('\n', " ")))
                }
                _ => None,
            })
            .collect()
    }

    #[test]
    fn no_arguments_opens_the_screen() {
        assert_eq!(parse(args(&[])), Invocation::Screen);
    }

    #[test]
    fn run_and_revert_take_the_ids_in_order() {
        assert_eq!(
            parse(args(&["--run", "mirrors", "multilib"])),
            Invocation::Run { ids: args(&["mirrors", "multilib"]), revert: false }
        );
        assert_eq!(parse(args(&["--revert", "mirrors"])), Invocation::Run { ids: args(&["mirrors"]), revert: true });
    }

    #[test]
    fn anything_else_is_shown_back() {
        assert_eq!(parse(args(&["--frobnicate"])), Invocation::Unknown("--frobnicate".into()));
    }

    // The first thing a person tries on a new command is `--help`; it used to be an unknown
    // argument with exit code 2.
    #[test]
    fn help_and_version_are_answered_with_success() {
        assert_eq!(parse(args(&["--help"])), Invocation::Help);
        assert_eq!(parse(args(&["-h"])), Invocation::Help);
        assert_eq!(parse(args(&["--version"])), Invocation::Version);
        assert_eq!(parse(args(&["-V"])), Invocation::Version);
        assert_eq!(carry_out(&Invocation::Help).expect("stdout is open"), ExitCode::SUCCESS);
        assert_eq!(carry_out(&Invocation::Version).expect("stdout is open"), ExitCode::SUCCESS);
    }

    #[test]
    fn a_line_that_names_nothing_to_carry_out_is_refused() {
        let choices = Choices::recommended();
        assert_eq!(plan(&[], false, &choices, Cpu::Intel).unwrap_err(), PlanError::Empty);
        assert_eq!(
            plan(&args(&["no-such-tweak"]), false, &choices, Cpu::Intel).unwrap_err(),
            PlanError::Unknown("no-such-tweak".into())
        );
    }

    // A run is the half of qtools that touches the machine, and it must carry out the values the
    // person chose on the screen. The country and the count reach reflector's own configuration
    // file, the one its timer reads every week, so the file a run writes is where they show.
    #[test]
    fn a_run_carries_out_the_country_and_the_mirror_count_that_were_chosen() {
        let root = Root::new();
        root.settings_of("[option.mirrors]\ncountry = \"Germany\"\ncount = 25\n");
        let choices = root.choices();
        assert_eq!((choices.country.as_str(), choices.mirrors), ("Germany", 25));

        let mut system = clean_machine();
        let code = carry_out_on(&parse(args(&["--run", "mirrors"])), &choices, &mut system, &root.0)
            .expect("it is carried out");

        assert_eq!(code, ExitCode::SUCCESS);
        let conf = system
            .read(Path::new("/etc/xdg/reflector/reflector.conf"))
            .expect("the configuration is read")
            .expect("the chosen values are written into it");
        assert!(conf.lines().any(|line| line == "--country Germany"), "{conf}");
        assert!(conf.lines().any(|line| line == "--latest 25"), "{conf}");
        assert!(
            system.calls().iter().any(|call| call == "sudo reflector @/etc/xdg/reflector/reflector.conf"),
            "and the list is made from that same file: {:?}",
            system.calls()
        );
    }

    // The screen never offers an item that does not fit the machine; a line typed by hand must
    // not reach around that rule. Without a Bluetooth controller, `--run bluetooth` would
    // otherwise install bluez and turn its service on for nothing.
    #[test]
    fn a_run_of_an_item_that_does_not_fit_this_machine_changes_nothing_and_fails() {
        let root = Root::new();
        let mut system = clean_machine();
        system.answer("sh -c [ -n \"$(ls -A /sys/class/bluetooth 2>/dev/null)\" ]", no());
        for package in ["bluez", "bluez-utils"] {
            system.answer(&format!("pacman -Q {package}"), no());
            system.answer(&format!("sudo pacman -S --needed --noconfirm {package}"), ok());
        }

        let code = carry_out_on(&parse(args(&["--run", "bluetooth"])), &root.choices(), &mut system, &root.0)
            .expect("it is answered");

        assert_eq!(code, ExitCode::from(1));
        assert!(!system.calls().iter().any(|call| call.starts_with("sudo ")), "{:?}", system.calls());
        assert!(!Journal::load(&system, &root.0).expect("the journal reads").has("bluetooth"));
    }

    #[test]
    fn a_session_takes_the_one_path_of_its_control_channel() {
        assert_eq!(
            parse(args(&["--session", "/run/user/1000/quvyta-tools/session-42"])),
            Invocation::Session { control: PathBuf::from("/run/user/1000/quvyta-tools/session-42") }
        );
        // The answers go back through the sibling, named after it, so one path is enough.
        assert_eq!(
            session::answers_of(Path::new("/run/user/1000/quvyta-tools/session-42")),
            Path::new("/run/user/1000/quvyta-tools/session-42.answers")
        );
    }

    #[test]
    fn a_run_carries_out_the_parallel_downloads_that_were_chosen() {
        let root = Root::new();
        root.settings_of("[option.pacman]\nparallel = 8\n");
        let choices = root.choices();
        assert_eq!(choices.parallel, 8);

        let mut system = clean_machine();
        let code = carry_out_on(&parse(args(&["--run", "pacman-options"])), &choices, &mut system, &root.0)
            .expect("it is carried out");

        assert_eq!(code, ExitCode::SUCCESS);
        let written =
            system.read(Path::new("/etc/pacman.conf")).expect("the fake reads").expect("pacman's file is written");
        let options = written.split("[core]").next().expect("the options come first");
        assert!(options.lines().any(|line| line == "ParallelDownloads = 8"), "under [options]:\n{written}");
    }

    #[test]
    fn a_run_carries_out_the_helper_that_were_chosen() {
        let root = Root::new();
        root.settings_of("[option.aur]\nhelper = \"yay\"\n");
        let choices = root.choices();
        assert_eq!(choices.helper, "yay");

        let mut system = clean_machine();
        system.answer("sh -c command -v paru", no());
        system.answer("sh -c command -v yay", no());
        system.answer(&build_line("yay"), ok());
        system.answer("sudo pacman -Rns --noconfirm paru", ok());
        system.answer("sudo pacman -Rns --noconfirm yay", ok());
        let code = carry_out_on(&parse(args(&["--run", "aur-helper"])), &choices, &mut system, &root.0)
            .expect("it is carried out");

        assert_eq!(code, ExitCode::SUCCESS);
        assert!(
            system.calls().contains(&build_line("yay")),
            "the chosen helper is the one built: {:?}",
            system.calls()
        );
        assert!(!system.calls().iter().any(|call| call.contains("paru")), "the other is nowhere: {:?}", system.calls());
    }

    #[test]
    fn a_run_carries_out_the_journal_size_that_were_chosen() {
        let root = Root::new();
        root.settings_of("[option.journal]\nmax-use = 1000\n");
        let choices = root.choices();
        assert_eq!(choices.journal, 1000);

        let mut system = FakeSystem::new();
        let code = carry_out_on(&parse(args(&["--run", "journal-limit"])), &choices, &mut system, &root.0)
            .expect("it is carried out");

        assert_eq!(code, ExitCode::SUCCESS);
        let written = system
            .read(Path::new("/etc/systemd/journald.conf.d/50-quvyta-tools.conf"))
            .expect("the drop-in is read")
            .expect("the chosen size is written into it");
        assert_eq!(written, "[Journal]\nSystemMaxUse=1000M\n");
    }

    // A value a person could not have typed, or that qtools would not have offered, is ignored
    // where it sits in the file: the run goes on with what qtools recommends and nothing reaches
    // a command that reflector or pacman would refuse.
    #[test]
    fn a_value_the_file_cannot_mean_leaves_the_run_at_the_recommendation() {
        let recommended = Choices::recommended();
        let root = Root::new();
        root.settings_of("[option.mirrors]\ncount = 900\ncountry = \"T;rm\"\n");
        let choices = root.choices();
        assert_eq!(choices.country, recommended.country, "the broken country is not used");
        assert_eq!(choices.mirrors, 10, "nor the count that is out of range");

        let files = written(&plan(&args(&["mirrors"]), false, &choices, Cpu::Intel).expect("a known tweak"));
        let conf =
            files.iter().find(|(path, _)| path == "/etc/xdg/reflector/reflector.conf").expect("it writes that file");
        assert!(conf.1.contains("--latest 10"), "the recommended count: {:?}", conf.1);
        assert!(!conf.1.contains("T;rm"), "nothing typed into reflector's own arguments: {:?}", conf.1);
    }

    // The line is typed by hand, so it can name something qtools does not carry. The command says
    // so and leaves the code that says the arguments made no sense, which a script can tell from
    // the code a failed run gives: nothing is read and nothing is written for an id that names
    // nothing, so a name mistyped costs nothing.
    #[test]
    fn an_id_no_item_carries_is_refused_with_code_two() {
        let root = Root::new();
        let mut system = clean_machine();

        let code = carry_out_on(&parse(args(&["--run", "no-such-tweak"])), &root.choices(), &mut system, &root.0)
            .expect("it is answered");

        assert_eq!(code, ExitCode::from(2));
        assert!(system.calls().is_empty(), "nothing is run: {:?}", system.calls());
        assert!(
            system
                .read(Path::new("/etc/systemd/journald.conf.d/50-quvyta-tools.conf"))
                .expect("the fake reads")
                .is_none(),
            "and nothing is written"
        );
    }

    // A line that names two items in order promises both: when the first one's own step fails, the
    // second one is not begun at all. Half of the first item with the whole of the second would be
    // a state qtools never promised and could not undo, and the code says a run failed rather than
    // that the line made no sense.
    #[test]
    fn a_failing_item_stops_the_rest_and_leaves_code_one() {
        let root = Root::new();
        let mut system = clean_machine();
        // Nothing installed and nothing installable: the first item's own step fails.
        system.answer("pacman -Q pacman-contrib", no());
        system.answer("sudo pacman -S --needed --noconfirm pacman-contrib", no());

        let code = carry_out_on(
            &parse(args(&["--run", "cache-cleanup", "journal-limit"])),
            &root.choices(),
            &mut system,
            &root.0,
        )
        .expect("it is answered");

        assert_eq!(code, ExitCode::from(1));
        assert!(
            !system.calls().iter().any(|call| call.starts_with("sudo systemctl enable --now paccache.timer")),
            "the failed item stops at its own failing step: {:?}",
            system.calls()
        );
        assert!(
            system
                .read(Path::new("/etc/systemd/journald.conf.d/50-quvyta-tools.conf"))
                .expect("the fake reads")
                .is_none(),
            "and the item after the failure never starts: {:?}",
            system.calls()
        );
    }

    #[test]
    fn a_session_without_its_one_path_is_an_argument_nobody_made_sense_of() {
        for line in [&["--session"][..], &["--session", "one", "two"][..]] {
            assert_eq!(parse(args(line)), Invocation::Unknown("--session".into()), "{line:?}");
        }
    }
}
