//! The `--run` and `--revert` command lines: the half of qtools that touches the machine.
//!
//! The screen starts its own binary this way inside an embedded terminal, so every command,
//! `sudo`'s password prompt included, talks to a terminal of its own. The same lines can be
//! typed by hand: `qtools --run mirrors pacman-options`.

use std::io::{self, Write};
use std::process::ExitCode;
use std::sync::Arc;

use qframe::prelude::*;

use crate::state::{self, Journal};
use crate::system::RealSystem;
use crate::{catalog, locales};

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
    /// Something else, which is shown back with the usage.
    Unknown(String),
}

/// Reads the arguments after the program name.
pub fn parse(args: impl IntoIterator<Item = String>) -> Invocation {
    let mut args = args.into_iter();
    match args.next().as_deref() {
        None => Invocation::Screen,
        Some(flag @ ("--run" | "--revert")) => Invocation::Run { ids: args.collect(), revert: flag == "--revert" },
        Some(other) => Invocation::Unknown(other.to_owned()),
    }
}

/// Carries out a run, a revert or an unknown invocation, in the language the environment
/// asks for, and says what happened line by line on standard output.
///
/// The exit code is 0 when every tweak went through, 1 when one failed (the rest are not run)
/// and 2 when the arguments made no sense.
///
/// # Errors
///
/// Fails when the journal cannot be read or written, or standard output is closed.
pub fn carry_out(invocation: &Invocation) -> io::Result<ExitCode> {
    let env = locales::env();
    qframe::i18n::scope(Arc::new(env.i18n().clone()), || match invocation {
        Invocation::Screen => Ok(ExitCode::SUCCESS),
        Invocation::Run { ids, revert } => run(ids, *revert),
        Invocation::Unknown(argument) => {
            eprintln!("{}", t!("cli.unknown-argument", argument = argument.as_str()));
            eprintln!("{}", t!("cli.usage"));
            Ok(ExitCode::from(2))
        }
    })
}

fn run(ids: &[String], revert: bool) -> io::Result<ExitCode> {
    let catalog = catalog::all();
    let mut chosen = Vec::new();
    for id in ids {
        match catalog.iter().find(|tweak| tweak.id == id) {
            Some(tweak) => chosen.push(tweak),
            None => {
                eprintln!("{}", t!("cli.unknown-tweak", id = id.as_str()));
                return Ok(ExitCode::from(2));
            }
        }
    }
    if chosen.is_empty() {
        eprintln!("{}", t!("cli.usage"));
        return Ok(ExitCode::from(2));
    }

    let folder = state::folder();
    let mut system = RealSystem;
    let mut journal = Journal::load(&system, &folder)?;
    let mut out = io::stdout().lock();
    let last = chosen.len() - 1;
    for (index, tweak) in chosen.into_iter().enumerate() {
        let title = t!(&format!("tweak.{}.title", tweak.id));
        let heading = if revert { t!("cli.reverting", tweak = title) } else { t!("cli.applying", tweak = title) };
        writeln!(out, "{heading}")?;
        out.flush()?;
        let result = if revert {
            tweak.revert(&mut system, &mut journal).map_err(|error| error.to_string())
        } else {
            tweak
                .apply(&mut system, &mut journal, &folder)
                .map_err(|failure| t!("cli.step-failed", step = failure.step + 1, reason = failure.reason))
        };
        // Whatever was recorded before a failure stays undoable, so the journal is saved
        // either way, before the outcome is reported.
        journal.save(&mut system, &folder)?;
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
    use super::*;

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|arg| (*arg).to_owned()).collect()
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
        assert_eq!(parse(args(&["--help"])), Invocation::Unknown("--help".into()));
    }
}
