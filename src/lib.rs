//! qtools: applies the recommended Arch Linux settings, with a preview, a backup and an undo.

/// The screen: groups, the list of tweaks and the chosen one's detail.
pub mod app;

/// The single door to the machine.
pub mod system;

/// What went wrong, said in the person's language.
pub mod problem;

/// The smallest piece of work qtools can do.
pub mod step;

/// What qtools has applied, and the backups that let it be undone.
pub mod state;

/// What the person sees in the list: a named piece of work made of steps.
pub mod tweak;

/// Every tweak qtools knows, grouped the way the sidebar shows them.
pub mod catalog;

/// The language files, compiled in so an installed binary needs nothing beside it.
pub mod locales;

/// The `--run` and `--revert` command lines, which the embedded terminal runs.
pub mod cli;

/// Whether this machine runs Arch Linux or a distribution built on it.
pub mod distro;

/// The values a person chose for the tweaks' options, and the limits they have.
pub mod choices;

/// The process that holds one terminal for a whole qtools session, so `sudo` asks once.
pub mod session;

use std::io;
use std::path::Path;
use std::process::ExitCode;

use qframe::runtime::Runtime;
use qframe::storage::Ecosystem;

use choices::Choices;

/// Runs one `qtools` invocation on this machine: a `--run` or `--revert` line carries out its
/// tweaks and exits, anything else opens the screen. Both commands, `qtools` and
/// `quvyta-tools`, start here.
///
/// # Errors
///
/// Returns the terminal's error when the screen cannot be opened or drawn.
pub fn run() -> io::Result<ExitCode> {
    let os_release = distro::os_release(&system::RealSystem::new());
    run_on(std::env::args().skip(1), os_release.as_deref())
}

/// Runs one invocation on a machine that describes itself with `os_release` (the text of
/// `/etc/os-release`, or `None` when it could not be read).
///
/// On a distribution qtools does not support nothing else is read or changed: the screen shows
/// only a notice, and a command line says the same and exits with code 3.
///
/// # Errors
///
/// Returns the terminal's error when the screen cannot be opened or drawn.
pub fn run_on(args: impl IntoIterator<Item = String>, os_release: Option<&str>) -> io::Result<ExitCode> {
    let invocation = cli::parse(args);
    // The usage and the version read nothing and change nothing, so every distribution gets them.
    if matches!(invocation, cli::Invocation::Help | cli::Invocation::Version) {
        return cli::carry_out(&invocation);
    }
    let supported = distro::is_supported(os_release);
    if invocation != cli::Invocation::Screen {
        return if supported { cli::carry_out(&invocation) } else { Ok(cli::unsupported()) };
    }
    if !supported {
        locales::LOCALES
            .iter()
            .fold(Runtime::new(app::unsupported::Unsupported), |runtime, (file, text)| {
                runtime.locale_source(*file, *text)
            })
            .keymap_source(locales::KEYMAP.0, locales::KEYMAP.1)
            .run()?;
        return Ok(ExitCode::SUCCESS);
    }

    // The shared Quvyta folder decides whether the wizard opens and where it writes; the look it
    // resolves is in force from the first frame. The same folder holds the Quvyta-wide
    // update notice, and the Quvyta state folder for qtools remembers when it last asked. It
    // holds `tools.conf` too, which is where the values a person chose for the options live.
    let folder = Ecosystem::QUVYTA.config_dir();

    // The screen may not do I/O, so where every tweak stands is read once before it opens, on
    // the catalog of the machine's own processor and with the values that file holds: a step's
    // check compares against the value it would write, so a person who chose eight parallel
    // downloads must not be shown a `ParallelDownloads = 5` they never asked for.
    let mut system = system::RealSystem::reading();
    let cpu = catalog::hardware::cpu(&mut system);
    let reading = app::read(&choices_in(folder.as_deref()), &mut system, cpu, &state::folder());

    let mut opening =
        app::Opening::new(folder.as_deref(), None, reading.states, cpu).with_updates(app::UpdateFolders::here());
    // Runs start this very binary, and the machine is read again after each; when the binary
    // cannot find itself, the installed command is the next best guess. The same binary is the
    // session that keeps one terminal for the whole qtools, and the runtime folder is where that
    // session's control channel goes.
    let program = std::env::current_exe().unwrap_or_else(|_| std::path::PathBuf::from("qtools"));
    opening.tools = opening
        .tools
        .machine(program, app::read_machine)
        .recorded(reading.recorded)
        .one_terminal(app::session::channel_folder());
    let runtime = locales::LOCALES
        .iter()
        .fold(Runtime::new(opening.tools), |runtime, (file, text)| runtime.locale_source(*file, *text))
        .keymap_source(locales::KEYMAP.0, locales::KEYMAP.1)
        .settings(&opening.settings)
        .preferences(&opening.preferences);
    // A member follows the look another Quvyta application changes while qtools is open; the
    // settings and preferences given above are used as they are and not read twice.
    let runtime = match folder {
        Some(folder) => runtime.member_in(Ecosystem::QUVYTA, folder, app::APP),
        None => runtime,
    };
    runtime.run()?;
    Ok(ExitCode::SUCCESS)
}

/// The values qtools uses on a machine whose Quvyta folder is `folder`: what `tools.conf` says,
/// and the recommendation wherever it says nothing or says something its option does not allow.
/// `None` is a machine with no folder of its own, where only the recommendations are available.
///
/// Both halves of qtools read this one file, so a run carries out the values the screen showed.
#[must_use]
pub fn choices_in(folder: Option<&Path>) -> Choices {
    Choices::read(&app::settings_in(folder))
}

#[cfg(test)]
mod tests {
    use super::*;

    const UBUNTU: &str = "NAME=\"Ubuntu\"\nID=ubuntu\nID_LIKE=debian\n";

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|arg| (*arg).to_owned()).collect()
    }

    // Each line here stops before any reading or writing even when supported (no tweak, an
    // unknown id, an unknown flag), so a broken check could never reach the machine; only the
    // exit code tells the refusal apart.
    #[test]
    fn a_command_line_on_another_distribution_is_refused_with_code_3() {
        for line in [&["--run"][..], &["--revert", "no-such-tweak"], &["--frobnicate"]] {
            assert_eq!(run_on(args(line), Some(UBUNTU)).unwrap(), ExitCode::from(3), "{line:?}");
            assert_eq!(run_on(args(line), None).unwrap(), ExitCode::from(3), "{line:?} with no os-release");
        }
    }

    #[test]
    fn help_and_version_answer_on_every_distribution() {
        for line in [&["--help"][..], &["--version"]] {
            assert_eq!(run_on(args(line), Some(UBUNTU)).unwrap(), ExitCode::SUCCESS, "{line:?}");
            assert_eq!(run_on(args(line), Some("ID=arch\n")).unwrap(), ExitCode::SUCCESS, "{line:?}");
        }
    }

    #[test]
    fn a_command_line_on_arch_goes_through() {
        for line in [&["--run"][..], &["--revert", "no-such-tweak"], &["--frobnicate"]] {
            assert_eq!(run_on(args(line), Some("ID=arch\n")).unwrap(), ExitCode::from(2), "{line:?}");
            assert_eq!(run_on(args(line), Some("ID=endeavouros\nID_LIKE=arch\n")).unwrap(), ExitCode::from(2));
        }
    }
}
