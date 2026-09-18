//! What the person sees in the list: a named piece of work made of steps.

use std::io;
use std::path::Path;

use crate::state::{Journal, digest};
use crate::step::{Step, StepState, Touch};
use crate::system::{Cmd, System};

/// The five groups in the sidebar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Group {
    /// Mirrors, pacman and the AUR.
    Packages,
    /// The firewall and what listens on the network.
    Security,
    /// Microcode, graphics, power and swap.
    Hardware,
    /// Themes, fonts and cursors.
    Appearance,
    /// Timers and limits that keep the system tidy.
    Maintenance,
}

impl Group {
    /// Every group, in the order the sidebar shows them.
    pub const ALL: [Self; 5] = [Self::Packages, Self::Security, Self::Hardware, Self::Appearance, Self::Maintenance];

    /// The key this group's text lives under in the language files.
    pub fn key(self) -> &'static str {
        match self {
            Self::Packages => "packages",
            Self::Security => "security",
            Self::Hardware => "hardware",
            Self::Appearance => "appearance",
            Self::Maintenance => "maintenance",
        }
    }
}

/// Whether a tweak fits this machine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Availability {
    /// It fits.
    Yes,
    /// It does not, and this language key says why.
    No(&'static str),
}

/// A value the person may change before applying.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OptionValue {
    /// Free text, such as a country code.
    Text(String),
    /// A number, such as how many mirrors to keep.
    Count(u32),
    /// One of a few named choices, such as paru or yay.
    Choice {
        /// Which one is chosen.
        chosen: usize,
        /// The language keys of the choices.
        options: Vec<&'static str>,
    },
}

/// A setting on a tweak, with the value we recommend.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TweakOption {
    /// The key this option's label lives under.
    pub key: &'static str,
    /// The recommended value.
    pub value: OptionValue,
}

/// Where a tweak stands on this machine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TweakState {
    /// Every step is done.
    Applied,
    /// No step is done.
    Off,
    /// Some steps are done and some are not.
    Half,
    /// A step was changed by something else; the text says what was found.
    Changed(String),
    /// This tweak does not fit this machine; the language key says why.
    Unavailable(&'static str),
}

/// A step that failed, and why.
#[derive(Debug, Clone)]
pub struct ApplyFailure {
    /// Which step failed, counted from zero.
    pub step: usize,
    /// What went wrong, in the system's own words.
    pub reason: String,
}

/// One entry in the catalog.
#[derive(Debug, Clone)]
pub struct Tweak {
    /// The identity used in the journal and the language files.
    pub id: &'static str,
    /// Which group it belongs to.
    pub group: Group,
    /// The settings the person may change.
    pub options: Vec<TweakOption>,
    /// The work, in order.
    pub steps: Vec<Step>,
    /// Whether this tweak fits the machine in front of us.
    pub applies_to: fn(&mut dyn System) -> Availability,
}

impl Tweak {
    /// Where this tweak stands right now.
    pub fn state(&self, system: &mut dyn System) -> io::Result<TweakState> {
        if let Availability::No(reason) = (self.applies_to)(system) {
            return Ok(TweakState::Unavailable(reason));
        }
        let mut done = 0;
        for step in &self.steps {
            match step.state(system)? {
                StepState::Done => done += 1,
                StepState::Missing => {}
                StepState::Conflict(found) => return Ok(TweakState::Changed(found)),
            }
        }
        Ok(match done {
            0 => TweakState::Off,
            n if n == self.steps.len() => TweakState::Applied,
            _ => TweakState::Half,
        })
    }

    /// Everything this tweak touches, for the preview and the confirmation.
    pub fn touches(&self) -> Vec<Touch> {
        self.steps.iter().flat_map(Step::describe).collect()
    }

    /// Runs every step in order, remembering each one. Stops at the first failure.
    pub fn apply(&self, system: &mut dyn System, journal: &mut Journal, folder: &Path) -> Result<(), ApplyFailure> {
        for (index, step) in self.steps.iter().enumerate() {
            match step.state(system) {
                Ok(StepState::Done) => continue,
                Ok(StepState::Missing) => {}
                Ok(StepState::Conflict(found)) => return Err(ApplyFailure { step: index, reason: found }),
                Err(error) => return Err(ApplyFailure { step: index, reason: error.to_string() }),
            }
            let undo = step.apply(system).map_err(|error| ApplyFailure { step: index, reason: error.to_string() })?;
            journal
                .record(self.id, index, &undo, system, folder)
                .map_err(|error| ApplyFailure { step: index, reason: error.to_string() })?;
        }
        Ok(())
    }

    /// Puts every applied step back, newest first, and forgets them.
    pub fn revert(&self, system: &mut dyn System, journal: &mut Journal) -> io::Result<()> {
        let entries: Vec<_> = journal.entries(self.id).into_iter().cloned().collect();
        for entry in entries.iter().rev() {
            if let (Some(target), backup) = (&entry.target, &entry.backup) {
                if let Some(hash) = &entry.hash {
                    // The file may have been changed by the person, or by another tweak
                    // that shares it, since qtools wrote it; the digest is the tripwire.
                    // Nothing is touched and nothing is undone when it no longer matches.
                    let current = system.read(target)?;
                    if current.as_deref().map(digest).as_ref() != Some(hash) {
                        return Err(io::Error::other(format!(
                            "{} changed after qtools wrote it; revert stopped before touching it",
                            target.display()
                        )));
                    }
                }
                match backup {
                    Some(backup) => {
                        let saved = system.read(backup)?.unwrap_or_default();
                        system.write(target, &saved, true)?;
                    }
                    // The file was not there before qtools wrote it.
                    None => system.remove(target, true)?,
                }
            }
            if let Some(argv) = &entry.undo {
                // The argv comes from `applied.toml`, which is on disk and can be
                // truncated or hand-edited; a broken entry is a diagnostic, not a panic.
                let broken = || {
                    io::Error::other(format!(
                        "the journal entry for `{}` step {} has no undo command",
                        self.id, entry.step
                    ))
                };
                let command = match argv.split_first() {
                    Some((program, rest)) if program == "sudo" => match rest.split_first() {
                        Some((program, args)) => Cmd::new(program, args.to_vec()).root(),
                        None => return Err(broken()),
                    },
                    Some((program, args)) => Cmd::new(program, args.to_vec()),
                    None => return Err(broken()),
                };
                let output = system.run(&command)?;
                if !output.ok() {
                    return Err(io::Error::other(output.stderr));
                }
            }
        }
        journal.forget(self.id);
        Ok(())
    }
}

#[cfg(test)]
mod tests;
