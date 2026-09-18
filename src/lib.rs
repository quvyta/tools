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

use std::io;
use std::process::ExitCode;

use qframe::runtime::Runtime;

/// Runs one `qtools` invocation: a `--run` or `--revert` line carries out its tweaks and exits,
/// anything else opens the screen. Both commands, `qtools` and `quvyta-tools`, start here.
///
/// # Errors
///
/// Returns the terminal's error when the screen cannot be opened or drawn.
pub fn run() -> io::Result<ExitCode> {
    let invocation = cli::parse(std::env::args().skip(1));
    if invocation != cli::Invocation::Screen {
        return cli::carry_out(&invocation);
    }

    // The screen may not do I/O, so where every tweak stands is read once before it opens.
    let mut system = system::RealSystem;
    let states =
        catalog::all().iter().map(|tweak| tweak.state(&mut system).unwrap_or(tweak::TweakState::Off)).collect();

    locales::LOCALES
        .iter()
        .fold(Runtime::new(app::Tools::new(states)), |runtime, (file, text)| runtime.locale_source(*file, *text))
        .keymap_source(locales::KEYMAP.0, locales::KEYMAP.1)
        .run()?;
    Ok(ExitCode::SUCCESS)
}
