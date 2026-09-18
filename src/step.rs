//! The smallest piece of work qtools can do, and the three questions every piece answers:
//! is it done, how is it done, and how is it undone.

use std::io;
use std::path::{Path, PathBuf};

use crate::system::{Cmd, System};

pub mod conf;

/// Where a step stands right now.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StepState {
    /// Already done.
    Done,
    /// Not done yet.
    Missing,
    /// Something else changed this; the text says what was found.
    Conflict(String),
}

/// A single thing a step touches, shown to the person before anything runs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Touch {
    /// A file that is written.
    File(String),
    /// A package that is installed.
    Package(String),
    /// A service that is enabled.
    Service(String),
    /// A command that is run.
    Command(String),
}

/// What it takes to put a step back the way it was.
#[derive(Debug, Clone, Default)]
pub struct Undo {
    /// The file to restore, with its content from before, or `None` when it did not exist.
    pub file: Option<(PathBuf, Option<String>)>,
    /// The command that undoes the step.
    pub command: Option<Cmd>,
}

/// The smallest piece of work.
#[derive(Debug, Clone)]
pub enum Step {
    /// Sets `key = value` in a configuration file.
    ConfOption {
        /// The file to change.
        file: &'static str,
        /// The option's name.
        key: &'static str,
        /// The value it should have.
        value: String,
    },
    /// Installs a package with pacman.
    PackageInstalled {
        /// The package's name.
        package: &'static str,
    },
    /// Enables a systemd unit and starts it.
    ServiceEnabled {
        /// The unit's name, with its suffix.
        unit: &'static str,
    },
    /// Writes a whole file that qtools owns.
    FileManaged {
        /// Where the file goes.
        path: &'static str,
        /// Exactly what the file should contain.
        content: String,
    },
    /// Anything the other steps cannot express. A command may only enter the catalog
    /// together with the check that tells whether it ran and the command that undoes it.
    CommandRun {
        /// The command that does the work.
        run: Cmd,
        /// The command whose success means the work is already done.
        check: Cmd,
        /// The command that undoes the work.
        undo: Cmd,
        /// The name shown in the preview.
        label: &'static str,
    },
    /// A command that rewrites one known file. The file's prior content is captured before the
    /// command runs, so the undo comes from the backup rather than from a second command.
    FileRewritten {
        /// The file the command rewrites.
        path: &'static str,
        /// The command that rewrites it.
        run: Cmd,
        /// The command whose success means the work is already done.
        check: Cmd,
        /// The name shown in the preview.
        label: &'static str,
    },
}

impl Step {
    /// Where this step stands on the machine right now.
    pub fn state(&self, system: &mut dyn System) -> io::Result<StepState> {
        match self {
            Self::ConfOption { file, key, value } => {
                let Some(text) = system.read(Path::new(file))? else {
                    return Ok(StepState::Conflict(format!("{file} does not exist")));
                };
                Ok(match conf::read_option(&text, key) {
                    Some(found) if &found == value => StepState::Done,
                    Some(found) => StepState::Conflict(format!("{key} = {found}")),
                    None => StepState::Missing,
                })
            }
            Self::PackageInstalled { package } => {
                let output = system.run(&Cmd::new("pacman", ["-Q", package]))?;
                Ok(if output.ok() { StepState::Done } else { StepState::Missing })
            }
            Self::ServiceEnabled { unit } => {
                let enabled = system.run(&Cmd::new("systemctl", ["is-enabled", unit]))?;
                let active = system.run(&Cmd::new("systemctl", ["is-active", unit]))?;
                Ok(if enabled.ok() && active.ok() { StepState::Done } else { StepState::Missing })
            }
            Self::FileManaged { path, content } => Ok(match system.read(Path::new(path))? {
                Some(found) if &found == content => StepState::Done,
                Some(found) => StepState::Conflict(found),
                None => StepState::Missing,
            }),
            Self::CommandRun { check, .. } => {
                let output = system.run(check)?;
                Ok(if output.ok() { StepState::Done } else { StepState::Missing })
            }
            Self::FileRewritten { check, .. } => {
                let output = system.run(check)?;
                Ok(if output.ok() { StepState::Done } else { StepState::Missing })
            }
        }
    }

    /// Does the step and returns what it takes to undo it.
    pub fn apply(&self, system: &mut dyn System) -> io::Result<Undo> {
        match self {
            Self::ConfOption { file, key, value } => {
                let path = PathBuf::from(file);
                let before = system.read(&path)?;
                let text = before.clone().unwrap_or_default();
                system.write(&path, &conf::set_option(&text, key, value), true)?;
                Ok(Undo { file: Some((path, before)), command: None })
            }
            Self::PackageInstalled { package } => {
                let output = system.run(&Cmd::new("pacman", ["-S", "--needed", "--noconfirm", package]).root())?;
                if !output.ok() {
                    return Err(io::Error::other(output.stderr));
                }
                Ok(Undo { file: None, command: Some(Cmd::new("pacman", ["-Rns", "--noconfirm", package]).root()) })
            }
            Self::ServiceEnabled { unit } => {
                let output = system.run(&Cmd::new("systemctl", ["enable", "--now", unit]).root())?;
                if !output.ok() {
                    return Err(io::Error::other(output.stderr));
                }
                Ok(Undo { file: None, command: Some(Cmd::new("systemctl", ["disable", "--now", unit]).root()) })
            }
            Self::FileManaged { path, content } => {
                let path = PathBuf::from(path);
                let before = system.read(&path)?;
                system.write(&path, content, true)?;
                Ok(Undo { file: Some((path, before)), command: None })
            }
            Self::CommandRun { run, undo, .. } => {
                let output = system.run(run)?;
                if !output.ok() {
                    return Err(io::Error::other(output.stderr));
                }
                Ok(Undo { file: None, command: Some(undo.clone()) })
            }
            Self::FileRewritten { path, run, .. } => {
                // Read the prior content first, so a command that fails halfway never
                // loses what needs to come back; nothing is recorded when it fails.
                let path = PathBuf::from(path);
                let before = system.read(&path)?;
                let output = system.run(run)?;
                if !output.ok() {
                    return Err(io::Error::other(output.stderr));
                }
                Ok(Undo { file: Some((path, before)), command: None })
            }
        }
    }

    /// Everything this step touches, for the preview and the confirmation screen.
    pub fn describe(&self) -> Vec<Touch> {
        match self {
            Self::ConfOption { file, .. } => vec![Touch::File((*file).to_owned())],
            Self::PackageInstalled { package } => vec![Touch::Package((*package).to_owned())],
            Self::ServiceEnabled { unit } => vec![Touch::Service((*unit).to_owned())],
            Self::FileManaged { path, .. } => vec![Touch::File((*path).to_owned())],
            Self::CommandRun { label, .. } => vec![Touch::Command((*label).to_owned())],
            Self::FileRewritten { path, label, .. } => {
                vec![Touch::File((*path).to_owned()), Touch::Command((*label).to_owned())]
            }
        }
    }
}

#[cfg(test)]
mod tests;
