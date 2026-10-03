//! What went wrong, said in the person's language.
//!
//! Every sentence qtools says about a failure lives in the language files. What the machine
//! itself said — a command's error output, the operating system's own message — is passed
//! through inside that sentence, because it is the machine's words and not qtools'.

use std::fmt;
use std::io;
use std::path::PathBuf;

use qframe::prelude::t;

/// One thing that stopped a step, a revert or a reading of the machine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Problem {
    /// A file a step works on is not there.
    FileMissing {
        /// The file.
        file: PathBuf,
    },
    /// An option is set to a value other than the one qtools would write.
    OptionDiffers {
        /// The file the option is in.
        file: PathBuf,
        /// The option's name.
        key: String,
        /// The value it has.
        found: String,
    },
    /// A configuration file has no section of this name, and qtools does not add one.
    SectionMissing {
        /// The file.
        file: PathBuf,
        /// The section's name, without its brackets.
        section: String,
    },
    /// A file qtools writes whole holds something else, first differing on this line.
    FileDiffers {
        /// The file.
        file: PathBuf,
        /// The first line that differs, counted from one.
        line: usize,
        /// That line as the file has it; `None` when the file ends before it.
        found: Option<String>,
    },
    /// A list is set neither in the main file nor in any of its drop-ins.
    ListNotSet {
        /// The list's name.
        key: String,
        /// The main configuration file.
        file: PathBuf,
        /// The folder of drop-ins.
        drop_ins: PathBuf,
    },
    /// The file named a moment ago as the one setting a list cannot be read now.
    ListUnreadable {
        /// The list's name.
        key: String,
        /// The file.
        file: PathBuf,
    },
    /// A list is assigned in a way qtools does not understand; the text is shown as found.
    ListNotUnderstood {
        /// The file.
        file: PathBuf,
        /// What the file says, or `None` when the list is not assigned in it at all.
        found: Option<String>,
    },
    /// A file was changed by something else after qtools wrote it.
    ChangedAfterWrite {
        /// The file.
        file: PathBuf,
    },
    /// The backup a revert would put back is gone.
    BackupMissing {
        /// The file that would have been put back.
        file: PathBuf,
        /// Where its backup was kept.
        backup: PathBuf,
    },
    /// qtools has no record of doing anything for this item, so there is nothing to undo.
    NothingRecorded,
    /// The item does not fit this machine; the language key says why. A run is refused before
    /// its first step, the same rule the screen keeps by not offering the item at all.
    DoesNotFit(&'static str),
    /// Whether this is done can only be read as root, and the screen never asks for root: it
    /// reads the machine while it draws, with no terminal for `sudo` to ask in.
    RootToRead,
    /// Where an item stands could not be read; what stopped the reading.
    Unreadable(Box<Problem>),
    /// A record in the journal does not match the step of the catalog it names.
    RecordDoesNotMatch {
        /// The step's place in its item, counted from one.
        step: usize,
    },
    /// A change was made but could not be recorded.
    NotRecorded {
        /// Why the record could not be written.
        why: Box<Problem>,
        /// Whether the change was put back again.
        put_back: bool,
    },
    /// Writing a file as root did not go through; what the helper said.
    WriteRefused {
        /// The file.
        file: PathBuf,
        /// The helper's own words, which may be empty when it was stopped.
        said: String,
    },
    /// The machine's own words: a command's error output, or the operating system's message.
    System(String),
}

impl Problem {
    /// The problem as a sentence in the language in force.
    pub fn text(&self) -> String {
        match self {
            Self::FileMissing { file } => t!("problem.file-missing", file = shown(file)),
            Self::OptionDiffers { file, key, found } => {
                t!("problem.option-differs", file = shown(file), key = key.as_str(), found = found.as_str())
            }
            Self::SectionMissing { file, section } => {
                t!("problem.section-missing", file = shown(file), section = section.as_str())
            }
            Self::FileDiffers { file, line, found: Some(found) } => {
                t!("problem.file-differs", file = shown(file), line = *line, found = found.as_str())
            }
            Self::FileDiffers { file, line, found: None } => {
                t!("problem.file-ends", file = shown(file), line = *line)
            }
            Self::ListNotSet { key, file, drop_ins } => {
                t!("problem.list-not-set", key = key.as_str(), file = shown(file), folder = shown(drop_ins))
            }
            Self::ListUnreadable { key, file } => t!("problem.list-unreadable", key = key.as_str(), file = shown(file)),
            Self::ListNotUnderstood { file, found: Some(found) } => {
                t!("problem.list-not-understood", file = shown(file), found = found.as_str())
            }
            Self::ListNotUnderstood { file, found: None } => t!("problem.list-not-in-file", file = shown(file)),
            Self::ChangedAfterWrite { file } => t!("problem.changed-after-write", file = shown(file)),
            Self::BackupMissing { file, backup } => {
                t!("problem.backup-missing", file = shown(file), backup = shown(backup))
            }
            Self::NothingRecorded => t!("problem.nothing-recorded"),
            Self::DoesNotFit(reason) => t!(reason),
            Self::RootToRead => t!("problem.root-to-read"),
            Self::Unreadable(why) => t!("problem.unreadable", why = why.text()),
            Self::RecordDoesNotMatch { step } => t!("problem.record-does-not-match", step = *step),
            Self::NotRecorded { why, put_back: true } => t!("problem.not-recorded", why = why.text()),
            Self::NotRecorded { why, put_back: false } => t!("problem.not-recorded-left", why = why.text()),
            Self::WriteRefused { file, said } if said.is_empty() => t!("problem.write-refused", file = shown(file)),
            Self::WriteRefused { file, said } => {
                t!("problem.write-refused-said", file = shown(file), said = said.as_str())
            }
            Self::System(said) => said.clone(),
        }
    }

    /// The problem carried inside an `io::Error`, for the places that speak `io::Result`
    /// (the [`crate::system::System`] trait); [`Problem::from_io`] takes it out again.
    pub fn into_io(self) -> io::Error {
        io::Error::other(self)
    }

    /// The problem an `io::Error` carries, or the operating system's own words when it
    /// carries none.
    pub fn from_io(error: &io::Error) -> Self {
        match error.get_ref().and_then(|inner| inner.downcast_ref::<Self>()) {
            Some(problem) => problem.clone(),
            None => Self::System(error.to_string()),
        }
    }
}

impl From<io::Error> for Problem {
    fn from(error: io::Error) -> Self {
        Self::from_io(&error)
    }
}

impl fmt::Display for Problem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.text())
    }
}

impl std::error::Error for Problem {}

/// A path as the sentence shows it.
fn shown(path: &std::path::Path) -> String {
    path.display().to_string()
}

/// Where two texts first differ, as a [`Problem::FileDiffers`]: the line number, counted from
/// one, and that line as `found` has it. Only the line is shown, never the whole file: a file
/// can be long, and its first different line is what tells the person what happened.
pub fn first_difference(file: PathBuf, wanted: &str, found: &str) -> Problem {
    let mut wanted_lines = wanted.lines();
    let mut found_lines = found.lines();
    let mut line = 1;
    loop {
        match (wanted_lines.next(), found_lines.next()) {
            (Some(a), Some(b)) if a == b => line += 1,
            (_, Some(b)) => return Problem::FileDiffers { file, line, found: Some(b.to_owned()) },
            (_, None) => return Problem::FileDiffers { file, line, found: None },
        }
    }
}

#[cfg(test)]
mod tests;
