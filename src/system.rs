//! The single door to the machine: running commands and reading or writing files.
//!
//! Every step talks to the system through this trait, so tests can run against a fake
//! machine and never touch the real one.

use std::io;
use std::path::Path;
use std::process::{Command as Process, Stdio};

use crate::problem::Problem;

/// A command to run, and whether it needs root.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cmd {
    program: String,
    args: Vec<String>,
    root: bool,
}

impl Cmd {
    /// A command run as the current user.
    pub fn new(program: &str, args: impl IntoIterator<Item = impl Into<String>>) -> Self {
        Self { program: program.to_owned(), args: args.into_iter().map(Into::into).collect(), root: false }
    }

    /// The same command, run through `sudo`.
    #[must_use]
    pub fn root(mut self) -> Self {
        self.root = true;
        self
    }

    /// Whether this command asks for root.
    pub fn needs_root(&self) -> bool {
        self.root
    }

    /// The program and its arguments, `sudo` first when root is needed.
    pub fn argv(&self) -> Vec<String> {
        let mut argv = Vec::with_capacity(self.args.len() + 2);
        if self.root {
            argv.push("sudo".to_owned());
        }
        argv.push(self.program.clone());
        argv.extend(self.args.iter().cloned());
        argv
    }
}

/// What a command left behind.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Output {
    /// The exit code; anything but zero is a failure.
    pub code: i32,
    /// Standard output, trimmed of its trailing newline.
    pub stdout: String,
    /// Standard error, trimmed of its trailing newline.
    pub stderr: String,
}

impl Output {
    /// Whether the command succeeded.
    pub fn ok(&self) -> bool {
        self.code == 0
    }
}

/// Running commands and touching files.
pub trait System {
    /// Runs a command and waits for it.
    fn run(&mut self, cmd: &Cmd) -> io::Result<Output>;
    /// Reads a file, or `None` when it does not exist.
    fn read(&self, path: &Path) -> io::Result<Option<String>>;
    /// Writes a file, asking for root when the file needs it.
    fn write(&mut self, path: &Path, content: &str, root: bool) -> io::Result<()>;
    /// Removes a file, asking for root when the file needs it.
    fn remove(&mut self, path: &Path, root: bool) -> io::Result<()>;
}

/// The real machine.
#[derive(Debug)]
pub struct RealSystem {
    /// What a command that needs root is run through, in place of the `sudo` of [`Cmd::argv`].
    elevator: Vec<String>,
    /// Whether a command that needs root is refused rather than run.
    reading: bool,
}

impl Default for RealSystem {
    fn default() -> Self {
        Self { elevator: vec!["sudo".to_owned()], reading: false }
    }
}

impl RealSystem {
    /// The machine qtools runs on, asking `sudo` for root.
    pub fn new() -> Self {
        Self::default()
    }

    /// The same machine, with root asked of `elevator` instead of `sudo`; an empty one runs the
    /// command as it is. The tests give an empty one and a temporary folder, so the paths a
    /// root write takes are exercised for real without anything ever asking for root.
    pub fn elevated_by(elevator: &[&str]) -> Self {
        Self { elevator: elevator.iter().map(|&part| part.to_owned()).collect(), reading: false }
    }

    /// The machine as the screen reads it: a command that needs root is refused, never run.
    /// The screen owns the terminal it draws in, so a `sudo` asking there for a password would
    /// write over the screen and wait for keys the screen takes for itself.
    pub fn reading() -> Self {
        Self { reading: true, ..Self::default() }
    }

    /// The command line as this machine runs it: `sudo` swapped for the elevator.
    fn argv(&self, cmd: &Cmd) -> Vec<String> {
        let mut argv = cmd.argv();
        if cmd.needs_root() {
            argv.splice(0..1, self.elevator.iter().cloned());
        }
        argv
    }

    /// A process for `program` and `args`, run as root through the elevator.
    fn elevated(&self, program: &str, args: &[&str]) -> Process {
        let argv = self.argv(&Cmd::new(program, args.iter().copied()).root());
        let mut process = Process::new(&argv[0]);
        process.args(&argv[1..]);
        process
    }
}

impl System for RealSystem {
    fn run(&mut self, cmd: &Cmd) -> io::Result<Output> {
        if self.reading && cmd.needs_root() {
            return Err(Problem::RootToRead.into_io());
        }
        let argv = self.argv(cmd);
        let output = Process::new(&argv[0]).args(&argv[1..]).output()?;
        Ok(Output {
            code: output.status.code().unwrap_or(-1),
            stdout: String::from_utf8_lossy(&output.stdout).trim_end().to_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).trim_end().to_owned(),
        })
    }

    fn read(&self, path: &Path) -> io::Result<Option<String>> {
        match std::fs::read_to_string(path) {
            Ok(text) => Ok(Some(text)),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(error),
        }
    }

    fn write(&mut self, path: &Path, content: &str, root: bool) -> io::Result<()> {
        if self.reading && root {
            return Err(Problem::RootToRead.into_io());
        }
        if !root {
            if let Some(folder) = path.parent() {
                std::fs::create_dir_all(folder)?;
            }
            return std::fs::write(path, content);
        }
        // The process is never root itself, so a root-owned file is written by a small
        // elevated helper that reads the content from its own standard input. `tee` opens the
        // file in place, which keeps an existing file's owner and mode, but it makes no folder:
        // one the file is the first of is made first, by the same elevator.
        if let Some(folder) = path.parent().filter(|folder| !folder.as_os_str().is_empty() && !folder.is_dir()) {
            let made = self.elevated("mkdir", &["-p", &folder.to_string_lossy()]).stdin(Stdio::null()).output()?;
            if !made.status.success() {
                return Err(refused(path, &made.stderr));
            }
        }
        let mut child = self
            .elevated("tee", &[&path.to_string_lossy()])
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()?;
        if let Some(mut stdin) = child.stdin.take() {
            use std::io::Write;
            stdin.write_all(content.as_bytes())?;
            // Dropped here, so the helper sees the end of its input and finishes.
        }
        let written = child.wait_with_output()?;
        if written.status.success() { Ok(()) } else { Err(refused(path, &written.stderr)) }
    }

    fn remove(&mut self, path: &Path, root: bool) -> io::Result<()> {
        if !root {
            return match std::fs::remove_file(path) {
                Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
                other => other,
            };
        }
        let output = self.run(&Cmd::new("rm", ["-f", &path.to_string_lossy()]).root())?;
        if output.ok() { Ok(()) } else { Err(io::Error::other(output.stderr)) }
    }
}

/// A root write that did not go through, with what the helper said about it.
fn refused(path: &Path, said: &[u8]) -> io::Error {
    Problem::WriteRefused { file: path.to_path_buf(), said: String::from_utf8_lossy(said).trim_end().to_owned() }
        .into_io()
}

/// A machine that answers only what a test told it to answer.
#[derive(Debug, Default)]
pub struct FakeSystem {
    answers: std::collections::HashMap<String, Output>,
    files: std::collections::HashMap<std::path::PathBuf, String>,
    calls: Vec<String>,
    refused: Vec<std::path::PathBuf>,
    unreadable: Vec<std::path::PathBuf>,
    reading: bool,
}

impl FakeSystem {
    /// An empty machine: no files, no answers.
    pub fn new() -> Self {
        Self::default()
    }

    /// Teaches the machine what a command answers. The key is the command line with single spaces.
    pub fn answer(&mut self, command: &str, output: Output) -> &mut Self {
        self.answers.insert(command.to_owned(), output);
        self
    }

    /// Puts a file on the machine before the test starts.
    pub fn file(&mut self, path: impl Into<std::path::PathBuf>, content: &str) -> &mut Self {
        self.files.insert(path.into(), content.to_owned());
        self
    }

    /// Makes every write to `path`, or to anything under it, fail, the way a full disk or a
    /// folder that cannot be written does.
    pub fn refuse_writes(&mut self, path: impl Into<std::path::PathBuf>) -> &mut Self {
        self.refused.push(path.into());
        self
    }

    /// Makes every read of `path`, or of anything under it, fail, the way a file only root may
    /// read does.
    pub fn refuse_reads(&mut self, path: impl Into<std::path::PathBuf>) -> &mut Self {
        self.unreadable.push(path.into());
        self
    }

    /// Makes this the machine as the screen reads it: a command that needs root is refused, the
    /// way [`RealSystem::reading`] refuses it.
    pub fn reading(&mut self) -> &mut Self {
        self.reading = true;
        self
    }

    /// Every command that was run, in order.
    pub fn calls(&self) -> &[String] {
        &self.calls
    }
}

impl System for FakeSystem {
    fn run(&mut self, cmd: &Cmd) -> io::Result<Output> {
        let line = cmd.argv().join(" ");
        if self.reading && cmd.needs_root() {
            return Err(Problem::RootToRead.into_io());
        }
        self.calls.push(line.clone());
        // An unknown command fails loudly rather than looking like a success.
        Ok(self.answers.get(&line).cloned().unwrap_or(Output {
            code: 127,
            stdout: String::new(),
            stderr: format!("the test did not answer `{line}`"),
        }))
    }

    fn read(&self, path: &Path) -> io::Result<Option<String>> {
        if self.unreadable.iter().any(|unreadable| path.starts_with(unreadable)) {
            return Err(io::Error::from(io::ErrorKind::PermissionDenied));
        }
        Ok(self.files.get(path).cloned())
    }

    fn write(&mut self, path: &Path, content: &str, _root: bool) -> io::Result<()> {
        if self.refused.iter().any(|refused| path.starts_with(refused)) {
            return Err(io::Error::from(io::ErrorKind::StorageFull));
        }
        self.files.insert(path.to_path_buf(), content.to_owned());
        Ok(())
    }

    fn remove(&mut self, path: &Path, _root: bool) -> io::Result<()> {
        self.files.remove(path);
        Ok(())
    }
}

#[cfg(test)]
mod tests;
