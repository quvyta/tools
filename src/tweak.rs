//! What the person sees in the list: a named piece of work made of steps.

use std::collections::HashMap;
use std::io;
use std::path::{Path, PathBuf};

use crate::problem::Problem;
use crate::state::{Applied, Journal, digest};
use crate::step::{Before, Step, StepState, Touch};
use crate::system::System;

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
    /// Some steps are done and some are not; one flag per step, in order, says which, so the
    /// confirmation counts only the steps a run would still carry out.
    Half(Vec<bool>),
    /// A step was changed by something else; the problem says what was found.
    Changed(Problem),
    /// This tweak does not fit this machine; the language key says why.
    Unavailable(&'static str),
    /// Where this tweak stands could not be read; the problem says what stopped the reading.
    /// It may still be applied: the run reads every step again, and leaves alone what is done.
    Unknown(Problem),
}

impl TweakState {
    /// Whether the step at `index` is done, as far as this reading tells: `None` when it does not
    /// say, which is the case for a tweak that was changed, does not fit or could not be read.
    pub fn step_done(&self, index: usize) -> Option<bool> {
        match self {
            Self::Applied => Some(true),
            Self::Off => Some(false),
            Self::Half(done) => done.get(index).copied(),
            Self::Changed(_) | Self::Unavailable(_) | Self::Unknown(_) => None,
        }
    }
}

/// A step that failed, and why.
#[derive(Debug, Clone)]
pub struct ApplyFailure {
    /// Which step failed, counted from zero; `None` when the tweak was refused before its first
    /// step.
    pub step: Option<usize>,
    /// What went wrong.
    pub reason: Problem,
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
        let mut done = Vec::with_capacity(self.steps.len());
        for step in &self.steps {
            match step.state(system)? {
                StepState::Done => done.push(true),
                StepState::Missing => done.push(false),
                StepState::Conflict(found) => return Ok(TweakState::Changed(found)),
            }
        }
        Ok(if !done.contains(&true) {
            TweakState::Off
        } else if !done.contains(&false) {
            TweakState::Applied
        } else {
            TweakState::Half(done)
        })
    }

    /// Where this tweak stands, for the screen: a reading that fails is a state of its own rather
    /// than a tweak that looks off, which would offer to apply something already done.
    pub fn reading(&self, system: &mut dyn System) -> TweakState {
        self.state(system).unwrap_or_else(|error| {
            TweakState::Unknown(match Problem::from_io(&error) {
                Problem::RootToRead => Problem::RootToRead,
                other => Problem::Unreadable(Box::new(other)),
            })
        })
    }

    /// Everything this tweak touches, for the preview and the confirmation.
    pub fn touches(&self) -> Vec<Touch> {
        self.steps.iter().flat_map(Step::describe).collect()
    }

    /// Runs every step in order, remembering each one. Stops at the first failure.
    ///
    /// Each step's backup is on disk before the step touches the machine, and the journal is
    /// on disk before the next step starts: a run that is cut short — a failure, a closed
    /// terminal, `ctrl c` at the password — leaves on record every step it finished, so all of
    /// them can be undone.
    ///
    /// # Errors
    ///
    /// Returns the step that failed and why, or no step when the tweak does not fit this machine.
    pub fn apply(&self, system: &mut dyn System, journal: &mut Journal, folder: &Path) -> Result<(), ApplyFailure> {
        // The screen never offers a tweak that does not fit; the command line is held to the same
        // rule here, at the one door both of them go through.
        if let Availability::No(reason) = (self.applies_to)(system) {
            return Err(ApplyFailure { step: None, reason: Problem::DoesNotFit(reason) });
        }
        for (index, step) in self.steps.iter().enumerate() {
            let fail = |reason: Problem| ApplyFailure { step: Some(index), reason };
            match step.state(system) {
                Ok(StepState::Done) => continue,
                Ok(StepState::Missing) => {}
                Ok(StepState::Conflict(found)) => return Err(fail(found)),
                Err(error) => return Err(fail(error.into())),
            }
            let before = step.before(system).map_err(fail)?;
            let backup = Journal::keep_backup(self.id, index, &before, system, folder).map_err(fail)?;
            step.apply(system, &before).map_err(fail)?;
            if let Err(why) = journal.record(self.id, (index, step), &before, backup, system) {
                return Err(fail(put_back(step, &before, system, why)));
            }
            if let Err(why) = journal.save(system, folder) {
                journal.remove(self.id, index);
                return Err(fail(put_back(step, &before, system, why.into())));
            }
        }
        Ok(())
    }

    /// Puts every applied step back, newest first, dropping each from the journal as soon as it
    /// is undone.
    ///
    /// Everything that can be checked is checked before anything is touched: that each record
    /// matches the catalog's step, that no file was changed after qtools wrote it, and that
    /// every backup is still there. Then every file goes back first and the undo commands run
    /// after them, so a command that reads a file this tweak wrote — a rebuild from the
    /// configuration, say — sees the file as it was rather than as qtools left it. The journal
    /// is written after each step, so an undo that stops halfway can simply be asked again and
    /// does what is left.
    ///
    /// # Errors
    ///
    /// Returns what stopped the undo; the steps not yet undone stay on record.
    pub fn revert(&self, system: &mut dyn System, journal: &mut Journal, folder: &Path) -> Result<(), Problem> {
        let entries: Vec<Applied> = journal.entries(self.id).into_iter().cloned().collect();
        if entries.is_empty() {
            return Err(Problem::NothingRecorded);
        }
        // What each file will hold once the newer records of it are undone, as a digest: the
        // steps of one tweak may write the same file one after another, and each step's digest
        // is of the file as that step left it, which is what the newer step's backup holds.
        let mut after_newer: HashMap<PathBuf, Option<String>> = HashMap::new();
        for entry in entries.iter().rev() {
            self.check(entry, system, &mut after_newer)?;
        }
        for entry in entries.iter().rev() {
            let Some(target) = &entry.target else { continue };
            match &entry.backup {
                Some(backup) => {
                    let saved = system
                        .read(backup)?
                        .ok_or_else(|| Problem::BackupMissing { file: target.clone(), backup: backup.clone() })?;
                    system.write(target, &saved, true)?;
                }
                // The file was not there before qtools wrote it.
                None => system.remove(target, true)?,
            }
            journal.remove(self.id, entry.step);
            journal.save(system, folder)?;
        }
        for entry in entries.iter().rev() {
            if entry.target.is_some() {
                continue;
            }
            // A step that stays needs nothing undone; it only leaves the record.
            if self.steps.get(entry.step).is_some_and(Step::stays_on_undo) {
                journal.remove(self.id, entry.step);
                journal.save(system, folder)?;
                continue;
            }
            let command = self.steps.get(entry.step).and_then(Step::undo_command);
            let command = command.ok_or(Problem::RecordDoesNotMatch { step: entry.step + 1 })?;
            let output = system.run(&command)?;
            if !output.ok() {
                return Err(Problem::System(output.stderr));
            }
            journal.remove(self.id, entry.step);
            journal.save(system, folder)?;
        }
        Ok(())
    }

    /// Whether one record can be undone, asked before anything is undone and newest first:
    /// `after_newer` holds what each file will read once the newer records are undone, and is
    /// moved on to what it will read once this one is.
    fn check(
        &self,
        entry: &Applied,
        system: &mut dyn System,
        after_newer: &mut HashMap<PathBuf, Option<String>>,
    ) -> Result<(), Problem> {
        let mismatch = || Problem::RecordDoesNotMatch { step: entry.step + 1 };
        let step = self.steps.get(entry.step).ok_or_else(mismatch)?;
        let Some(target) = &entry.target else {
            if step.stays_on_undo() {
                return Ok(());
            }
            return step.undo_command().map(|_| ()).ok_or_else(mismatch);
        };
        // The target comes from `applied.toml`, which is on disk and can be edited; a file the
        // catalog's step never writes is not written to as root on its word.
        if !step.may_write(target) {
            return Err(mismatch());
        }
        let current = match after_newer.get(target) {
            Some(digest) => digest.clone(),
            None => system.read(target)?.as_deref().map(digest),
        };
        // The file may have been changed by the person, or by another tweak that shares it,
        // since qtools wrote it; the digest is the tripwire.
        if entry.hash.as_ref().is_some_and(|hash| current.as_ref() != Some(hash)) {
            return Err(Problem::ChangedAfterWrite { file: target.clone() });
        }
        let restored = match &entry.backup {
            // Without the backup there is nothing to put back; writing the file empty instead
            // would break what reads it, `/etc/pacman.conf` first of all.
            Some(backup) => Some(digest(
                &system
                    .read(backup)?
                    .ok_or_else(|| Problem::BackupMissing { file: target.clone(), backup: backup.clone() })?,
            )),
            None => None,
        };
        after_newer.insert(target.clone(), restored);
        Ok(())
    }
}

/// Undoes a step that ran but could not be recorded, so nothing is left on the machine that
/// qtools would not know to undo, and says whether that worked.
fn put_back(step: &Step, before: &Before, system: &mut dyn System, why: Problem) -> Problem {
    let undone: io::Result<bool> = match (&before.file, step.undo_command()) {
        (Some((path, Some(content))), _) => system.write(path, content, true).map(|()| true),
        (Some((path, None)), _) => system.remove(path, true).map(|()| true),
        (None, Some(command)) => system.run(&command).map(|output| output.ok()),
        (None, None) => Ok(false),
    };
    Problem::NotRecorded { why: Box::new(why), put_back: matches!(undone, Ok(true)) }
}

#[cfg(test)]
mod tests;
