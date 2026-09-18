//! The single door to the machine: running commands and reading or writing files.
//!
//! Every step talks to the system through this trait, so tests can run against a fake
//! machine and never touch the real one.

use std::io;
use std::path::Path;
use std::process::Command as Process;

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
#[derive(Debug, Default)]
pub struct RealSystem;

impl System for RealSystem {
    fn run(&mut self, cmd: &Cmd) -> io::Result<Output> {
        let argv = cmd.argv();
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
        if !root {
            if let Some(folder) = path.parent() {
                std::fs::create_dir_all(folder)?;
            }
            return std::fs::write(path, content);
        }
        // The process is never root itself, so a root-owned file is written by a small
        // elevated helper that reads the content from its own standard input.
        let mut child = Process::new("sudo")
            .args(["tee", &path.to_string_lossy()])
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::null())
            .spawn()?;
        {
            use std::io::Write;
            let stdin = child.stdin.as_mut().ok_or_else(|| io::Error::other("the helper has no input"))?;
            stdin.write_all(content.as_bytes())?;
        }
        let status = child.wait()?;
        if status.success() { Ok(()) } else { Err(io::Error::other("the file could not be written")) }
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

/// A machine that answers only what a test told it to answer.
#[derive(Debug, Default)]
pub struct FakeSystem {
    answers: std::collections::HashMap<String, Output>,
    files: std::collections::HashMap<std::path::PathBuf, String>,
    calls: Vec<String>,
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

    /// Every command that was run, in order.
    pub fn calls(&self) -> &[String] {
        &self.calls
    }
}

impl System for FakeSystem {
    fn run(&mut self, cmd: &Cmd) -> io::Result<Output> {
        let line = cmd.argv().join(" ");
        self.calls.push(line.clone());
        // An unknown command fails loudly rather than looking like a success.
        Ok(self.answers.get(&line).cloned().unwrap_or(Output {
            code: 127,
            stdout: String::new(),
            stderr: format!("the test did not answer `{line}`"),
        }))
    }

    fn read(&self, path: &Path) -> io::Result<Option<String>> {
        Ok(self.files.get(path).cloned())
    }

    fn write(&mut self, path: &Path, content: &str, _root: bool) -> io::Result<()> {
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
