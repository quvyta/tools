//! qtools: applies the recommended Arch Linux settings, with a preview, a backup and an undo.

/// The screen: groups, the list of tweaks and the chosen one's detail.
pub mod app;

/// The single door to the machine.
pub mod system;

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

use std::io;
use std::process::ExitCode;

use qframe::runtime::Runtime;
use qframe::storage::Family;

/// Runs one `qtools` invocation on this machine: a `--run` or `--revert` line carries out its
/// tweaks and exits, anything else opens the screen. Both commands, `qtools` and
/// `quvyta-tools`, start here.
///
/// # Errors
///
/// Returns the terminal's error when the screen cannot be opened or drawn.
pub fn run() -> io::Result<ExitCode> {
    let os_release = distro::os_release(&system::RealSystem);
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

    // The screen may not do I/O, so where every tweak stands is read once before it opens.
    let mut system = system::RealSystem;
    let states =
        catalog::all().iter().map(|tweak| tweak.state(&mut system).unwrap_or(tweak::TweakState::Off)).collect();

    // The shared Quvyta folder decides whether the wizard opens and where it writes; the look it
    // resolves is in force from the first frame. The same folder holds the Quvyta-wide
    // update notice, and the Quvyta state folder for qtools remembers when it last asked.
    let folder = Family::QUVYTA.config_dir();
    let opening = app::Opening::new(folder.as_deref(), None, states).with_updates(app::UpdateFolders::here());
    locales::LOCALES
        .iter()
        .fold(Runtime::new(opening.tools), |runtime, (file, text)| runtime.locale_source(*file, *text))
        .keymap_source(locales::KEYMAP.0, locales::KEYMAP.1)
        .settings(&opening.settings)
        .preferences(&opening.preferences)
        .run()?;
    Ok(ExitCode::SUCCESS)
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
        for line in [&["--run"][..], &["--revert", "no-such-tweak"], &["--help"]] {
            assert_eq!(run_on(args(line), Some(UBUNTU)).unwrap(), ExitCode::from(3), "{line:?}");
            assert_eq!(run_on(args(line), None).unwrap(), ExitCode::from(3), "{line:?} with no os-release");
        }
    }

    #[test]
    fn a_command_line_on_arch_goes_through() {
        for line in [&["--run"][..], &["--revert", "no-such-tweak"], &["--help"]] {
            assert_eq!(run_on(args(line), Some("ID=arch\n")).unwrap(), ExitCode::from(2), "{line:?}");
            assert_eq!(run_on(args(line), Some("ID=endeavouros\nID_LIKE=arch\n")).unwrap(), ExitCode::from(2));
        }
    }
}
