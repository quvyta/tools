//! The smallest piece of work qtools can do, and the three questions every piece answers:
//! is it done, how is it done, and how is it undone.

use std::io;
use std::path::{Path, PathBuf};

use crate::problem::{Problem, first_difference};
use crate::system::{Cmd, System};

pub mod conf;
pub mod list;

/// Where a step stands right now.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StepState {
    /// Already done.
    Done,
    /// Not done yet.
    Missing,
    /// Something else changed this; the problem says what was found.
    Conflict(Problem),
}

/// A single thing a step touches, shown to the person before anything runs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Touch {
    /// A file that is written, or a folder a file of it may be written in.
    File(String),
    /// A package that is installed.
    Package(String),
    /// A package that is installed and stays when the item is undone.
    NeededPackage(String),
    /// A service that is enabled and started.
    Service(String),
    /// A unit that is enabled to run on its own occasion, and not started now.
    Unit(String),
    /// A command that is run.
    Command(String),
}

/// What a step is about to change, read before it changes anything, so the backup can be on
/// disk before the machine is touched.
#[derive(Debug, Clone, Default)]
pub struct Before {
    /// The file the step writes, with its content now, or `None` when it does not exist yet.
    pub file: Option<(PathBuf, Option<String>)>,
}

/// The smallest piece of work.
#[derive(Debug, Clone)]
pub enum Step {
    /// Sets `key = value` in a configuration file, inside one of its `[sections]`.
    ConfOption {
        /// The file to change.
        file: &'static str,
        /// The section the option belongs to, or `None` for a file without sections.
        section: Option<&'static str>,
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
    /// Installs a package another step needs, and leaves it when the item is undone: a tool
    /// such as a compiler or git, which the person may have come to use for other things by
    /// the time they undo the item that brought it.
    PackageNeeded {
        /// The package's name.
        package: &'static str,
    },
    /// Enables a systemd unit and starts it.
    ServiceEnabled {
        /// The unit's name, with its suffix.
        unit: &'static str,
    },
    /// Enables a systemd unit without starting it: a unit that runs on an occasion of its
    /// own, such as NVIDIA's suspend hand-off, which runs when the machine goes to sleep.
    /// Starting it now would do its work now; and a one-shot unit that has done its work is
    /// inactive again, so only whether it is enabled says whether the step is done.
    UnitEnabled {
        /// The unit's name, with its suffix.
        unit: &'static str,
    },
    /// Writes a whole file whose content qtools decides, over whatever the machine had there
    /// before — a package's own default, say — which the backup keeps. Unlike
    /// [`Step::FileManaged`], a file with other content is not someone else's change: it is
    /// simply not done yet.
    FileReplaced {
        /// Where the file goes.
        path: &'static str,
        /// Exactly what the file should contain.
        content: String,
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
        /// Whether something this item turns on — a timer — writes the file again later. Its
        /// digest then says nothing about whether someone else changed it, so it is not kept
        /// as a tripwire, and the undo puts back what was there before qtools.
        renewed: bool,
    },
    /// Adds one item to a list in a configuration file, in place, leaving the rest of the file
    /// exactly as it was.
    ///
    /// The file is the one the program that reads the list reads it from: the last of `file`
    /// and the drop-ins under `drop_ins` that sets it. A machine that keeps the list in a
    /// drop-in is edited in the drop-in rather than in a main file nothing reads, which is
    /// why both are named in the preview. mkinitcpio, systemd and pacman.conf all read a
    /// main file and then a directory of drop-ins, where the last assignment wins.
    ListItemAdded {
        /// The main configuration file.
        file: &'static str,
        /// The directory of drop-ins read after it.
        drop_ins: &'static str,
        /// The list's name, as it is assigned.
        key: &'static str,
        /// The item to add.
        item: &'static str,
        /// Where it goes: after the first of these that is in the list, or at its end.
        after: &'static [&'static str],
    },
}

impl Step {
    /// Where this step stands on the machine right now.
    pub fn state(&self, system: &mut dyn System) -> io::Result<StepState> {
        match self {
            Self::ConfOption { file, section, key, value } => {
                let path = PathBuf::from(file);
                let Some(text) = system.read(&path)? else {
                    return Ok(StepState::Conflict(Problem::FileMissing { file: path }));
                };
                if !conf::has_section(&text, *section) {
                    return Ok(StepState::Conflict(Problem::SectionMissing {
                        file: path,
                        section: section.unwrap_or_default().to_owned(),
                    }));
                }
                Ok(match conf::read_option(&text, *section, key) {
                    Some(found) if &found == value => StepState::Done,
                    Some(found) => {
                        StepState::Conflict(Problem::OptionDiffers { file: path, key: (*key).to_owned(), found })
                    }
                    None => StepState::Missing,
                })
            }
            Self::PackageInstalled { package } | Self::PackageNeeded { package } => {
                let output = system.run(&Cmd::new("pacman", ["-Q", package]))?;
                Ok(if output.ok() { StepState::Done } else { StepState::Missing })
            }
            Self::ServiceEnabled { unit } => {
                let enabled = system.run(&Cmd::new("systemctl", ["is-enabled", unit]))?;
                let active = system.run(&Cmd::new("systemctl", ["is-active", unit]))?;
                Ok(if enabled.ok() && active.ok() { StepState::Done } else { StepState::Missing })
            }
            Self::UnitEnabled { unit } => {
                let enabled = system.run(&Cmd::new("systemctl", ["is-enabled", unit]))?;
                Ok(if enabled.ok() { StepState::Done } else { StepState::Missing })
            }
            Self::FileReplaced { path, content } => {
                let found = system.read(Path::new(path))?;
                Ok(if found.as_ref() == Some(content) { StepState::Done } else { StepState::Missing })
            }
            Self::FileManaged { path, content } => Ok(match system.read(Path::new(path))? {
                Some(found) if &found == content => StepState::Done,
                Some(found) => StepState::Conflict(first_difference(PathBuf::from(path), content, &found)),
                None => StepState::Missing,
            }),
            Self::CommandRun { check, .. } | Self::FileRewritten { check, .. } => {
                let output = system.run(check)?;
                Ok(if output.ok() { StepState::Done } else { StepState::Missing })
            }
            Self::ListItemAdded { file, drop_ins, key, item, .. } => {
                let (path, text) = match list_file(system, file, drop_ins, key)? {
                    Ok(found) => found,
                    Err(problem) => return Ok(StepState::Conflict(problem)),
                };
                Ok(match list::read(&text, key) {
                    Ok(found) if found.has(item) => StepState::Done,
                    Ok(_) => StepState::Missing,
                    Err(found) => StepState::Conflict(Problem::ListNotUnderstood { file: path, found }),
                })
            }
        }
    }

    /// Reads what the step is about to change: the file it writes and that file's content now.
    /// Nothing is changed here, so a backup can be kept before [`Step::apply`] runs.
    ///
    /// # Errors
    ///
    /// Fails when the file cannot be read, or, for a list, when the file that sets it cannot
    /// be found or read.
    pub fn before(&self, system: &mut dyn System) -> Result<Before, Problem> {
        let path = match self {
            Self::ConfOption { file, .. } => PathBuf::from(file),
            Self::FileManaged { path, .. } | Self::FileReplaced { path, .. } | Self::FileRewritten { path, .. } => {
                PathBuf::from(path)
            }
            Self::ListItemAdded { file, drop_ins, key, .. } => {
                let (path, text) = list_file(system, file, drop_ins, key)??;
                return Ok(Before { file: Some((path, Some(text))) });
            }
            Self::PackageInstalled { .. }
            | Self::PackageNeeded { .. }
            | Self::ServiceEnabled { .. }
            | Self::UnitEnabled { .. }
            | Self::CommandRun { .. } => return Ok(Before::default()),
        };
        let content = system.read(&path)?;
        Ok(Before { file: Some((path, content)) })
    }

    /// Does the step, starting from what [`Step::before`] read.
    ///
    /// # Errors
    ///
    /// Fails with the machine's own words when a command or a write fails, and with the
    /// problem it found when the file is not one the step can change.
    pub fn apply(&self, system: &mut dyn System, before: &Before) -> Result<(), Problem> {
        let text = || before.file.as_ref().and_then(|(_, text)| text.clone());
        match self {
            Self::ConfOption { file, section, key, value } => {
                let path = PathBuf::from(file);
                let Some(current) = text() else { return Err(Problem::FileMissing { file: path }) };
                let Some(written) = conf::set_option(&current, *section, key, value) else {
                    return Err(Problem::SectionMissing {
                        file: path,
                        section: section.unwrap_or_default().to_owned(),
                    });
                };
                system.write(&path, &written, true)?;
            }
            Self::PackageInstalled { package } | Self::PackageNeeded { package } => {
                succeed(system, &Cmd::new("pacman", ["-S", "--needed", "--noconfirm", package]).root())?;
            }
            Self::ServiceEnabled { unit } => succeed(system, &Cmd::new("systemctl", ["enable", "--now", unit]).root())?,
            Self::UnitEnabled { unit } => succeed(system, &Cmd::new("systemctl", ["enable", unit]).root())?,
            Self::FileManaged { path, content } | Self::FileReplaced { path, content } => {
                system.write(Path::new(path), content, true)?;
            }
            Self::CommandRun { run, .. } | Self::FileRewritten { run, .. } => succeed(system, run)?,
            Self::ListItemAdded { file, key, item, after, .. } => {
                let Some((path, Some(current))) = &before.file else {
                    return Err(Problem::ListNotUnderstood { file: PathBuf::from(file), found: None });
                };
                let list = list::read(current, key)
                    .map_err(|found| Problem::ListNotUnderstood { file: path.clone(), found })?;
                system.write(path, &list.with(item, after), true)?;
            }
        }
        Ok(())
    }

    /// The command that undoes this step, for a step that is undone by a command rather than
    /// by putting a file back. It comes from the catalog, never from the journal on disk: the
    /// journal says which steps ran, and the steps themselves say how they are undone.
    pub fn undo_command(&self) -> Option<Cmd> {
        match self {
            Self::PackageInstalled { package } => Some(Cmd::new("pacman", ["-Rns", "--noconfirm", package]).root()),
            Self::ServiceEnabled { unit } => Some(Cmd::new("systemctl", ["disable", "--now", unit]).root()),
            Self::UnitEnabled { unit } => Some(Cmd::new("systemctl", ["disable", unit]).root()),
            Self::CommandRun { undo, .. } => Some(undo.clone()),
            Self::ConfOption { .. }
            | Self::PackageNeeded { .. }
            | Self::FileManaged { .. }
            | Self::FileReplaced { .. }
            | Self::FileRewritten { .. }
            | Self::ListItemAdded { .. } => None,
        }
    }

    /// Whether `target` is a file this step may have written, so a record in the journal that
    /// names any other file is refused rather than written to as root.
    pub fn may_write(&self, target: &Path) -> bool {
        match self {
            Self::ConfOption { file, .. } => target == Path::new(file),
            Self::FileManaged { path, .. } | Self::FileReplaced { path, .. } | Self::FileRewritten { path, .. } => {
                target == Path::new(path)
            }
            Self::ListItemAdded { file, drop_ins, .. } => {
                target == Path::new(file)
                    || (target.parent() == Some(Path::new(drop_ins))
                        && target.extension().is_some_and(|extension| extension == "conf"))
            }
            Self::PackageInstalled { .. }
            | Self::PackageNeeded { .. }
            | Self::ServiceEnabled { .. }
            | Self::UnitEnabled { .. }
            | Self::CommandRun { .. } => false,
        }
    }

    /// Whether undoing the item leaves this step as it is.
    pub fn stays_on_undo(&self) -> bool {
        matches!(self, Self::PackageNeeded { .. })
    }

    /// Whether the digest of what this step wrote tells a later change by someone else, which
    /// is true of every file step but one whose file a timer of the same item writes again.
    pub fn keeps_tripwire(&self) -> bool {
        !matches!(self, Self::FileRewritten { renewed: true, .. })
    }

    /// Everything this step touches, for the preview and the confirmation screen.
    pub fn describe(&self) -> Vec<Touch> {
        match self {
            Self::ConfOption { file, .. } => vec![Touch::File((*file).to_owned())],
            Self::PackageInstalled { package } => vec![Touch::Package((*package).to_owned())],
            Self::PackageNeeded { package } => vec![Touch::NeededPackage((*package).to_owned())],
            Self::ServiceEnabled { unit } => vec![Touch::Service((*unit).to_owned())],
            Self::UnitEnabled { unit } => vec![Touch::Unit((*unit).to_owned())],
            Self::FileManaged { path, .. } | Self::FileReplaced { path, .. } => vec![Touch::File((*path).to_owned())],
            Self::CommandRun { label, .. } => vec![Touch::Command((*label).to_owned())],
            Self::FileRewritten { path, label, .. } => {
                vec![Touch::File((*path).to_owned()), Touch::Command((*label).to_owned())]
            }
            // Both places the list may be written are named: which one it really is depends
            // on the machine, and the promise is made before the machine is read.
            Self::ListItemAdded { file, drop_ins, .. } => {
                vec![Touch::File((*file).to_owned()), Touch::File((*drop_ins).to_owned())]
            }
        }
    }
}

/// Runs a command that has to succeed, and turns its failure into its own error output.
fn succeed(system: &mut dyn System, cmd: &Cmd) -> Result<(), Problem> {
    let output = system.run(cmd)?;
    if output.ok() { Ok(()) } else { Err(Problem::System(output.stderr)) }
}

/// The file the list `key` is really read from, with its text, or the problem that stops a
/// step from reading it.
///
/// This is the machine that answers, not a guess: the program that reads the list is asked
/// which of its files has the last word, and that file is the one qtools reads and writes.
fn list_file(
    system: &mut dyn System,
    file: &str,
    drop_ins: &str,
    key: &str,
) -> io::Result<Result<(PathBuf, String), Problem>> {
    let output = system.run(&find_command(file, drop_ins, key))?;
    let found = output.stdout.trim();
    if found.is_empty() {
        return Ok(Err(Problem::ListNotSet {
            key: key.to_owned(),
            file: PathBuf::from(file),
            drop_ins: PathBuf::from(drop_ins),
        }));
    }
    let path = PathBuf::from(found);
    Ok(match system.read(&path)? {
        Some(text) => Ok((path, text)),
        // The file was named a moment ago and cannot be read now. That is not an empty file
        // qtools may write an item into.
        None => Err(Problem::ListUnreadable { key: key.to_owned(), file: path }),
    })
}

/// The command that answers with the file a list is really read from.
///
/// The main file first, then the drop-ins in the order the program reads them, and the last
/// one that sets the list wins. mkinitcpio 42 works exactly this way: it concatenates
/// `/etc/mkinitcpio.conf` and then `/etc/mkinitcpio.conf.d/*.conf`, taken in
/// `LC_ALL=C.UTF-8 sort -zVu` order, into one file and sources the lot, so a later
/// assignment is the one in force. `sort -V` is that version order, and a file that does
/// not set the list is not answered for, so the last path printed is the one in force.
fn find_command(file: &str, drop_ins: &str, key: &str) -> Cmd {
    Cmd::new(
        "sh",
        [
            "-c",
            &format!(
                "for f in {file} $(ls -1 {drop_ins}/*.conf 2>/dev/null | LC_ALL=C sort -V); do \
             [ -r \"$f\" ] && grep -q '^[[:space:]]*{key}=' \"$f\" && echo \"$f\"; done | tail -n 1"
            ),
        ],
    )
}

#[cfg(test)]
mod tests;
