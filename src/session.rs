//! One terminal for a whole qtools session: the requests the screen sends over its control
//! channel, the child that carries each one out in that terminal, and the keepalive that holds
//! `sudo`'s ticket while qtools is open.
//!
//! `qtools --session <control>` is this process (see [`crate::cli`]). The screen starts it inside
//! a pseudo-terminal and it lives as long as the screen does: every run and every revert happens
//! as a child `qtools --run`/`--revert` process that inherits this terminal, so `sudo` sees the
//! same terminal every time and asks for the password once. Nothing here reads, carries or stores
//! the password: `sudo` asks in this terminal, the person types into it, and every command the
//! keepalive runs says `-n`, so it never asks at all.
//!
//! The channel carries tweak ids and nothing else, never a command line: a request is
//! `run <id>...` or `revert <id>...`, and an id no tweak answers to is refused on the terminal.
//! The exit code of the child goes back over the sibling channel as `done <code>`, so the screen
//! shows the outcome the way it always has.
//!
//! A keepalive is only started once `sudo` has a ticket to keep, and a refresh that fails ends
//! it: a ticket that is gone only comes back when the person types the password again, and
//! trying every minute in between would fill the authentication log with failures. When the
//! screen is gone the refreshing stops, the ticket is dropped with `sudo -n -k`, and the process
//! goes. Nothing asks at start, and nothing runs after the screen has gone.

use std::fs::{File, OpenOptions};
use std::io::{self, BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Stdio};
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::thread::JoinHandle;
use std::time::Duration;

use qframe::prelude::*;

use crate::catalog;
use crate::catalog::hardware::{Cpu, cpu};

/// How often `sudo`'s ticket is refreshed while qtools is open. Long enough that the
/// authentication log says nothing about it, short enough that the ticket never reaches the
/// timeout while the person keeps working.
pub const KEEPALIVE: Duration = Duration::from_secs(60);

/// The answer a refused request gets: the same code a command line gives for an argument it
/// cannot make sense of, which is a failure on the screen as well.
const REFUSED: u32 = 2;

/// The word each answer begins with, so a line that is not one is not read as a code.
const DONE: &str = "done";

/// The two ends of a session's control channel: the requests that arrive, and where the exit code
/// of the child that carried one out goes back.
///
/// The screen opens the requests pipe at both ends and this end read-only, so its read returns at
/// once and a request written before this process has arrived waits in the pipe rather than on it;
/// the answers pipe the other way round, the screen reads and this writes, so both ends meet
/// without either waiting for the other. A test opens a pair of sockets instead, which is the same
/// two directions without the files.
pub struct Control<'a> {
    /// The requests, as the screen writes them: one line each.
    pub requests: &'a mut dyn BufRead,
    /// Where the exit code of each request's child is written: one line each, `done <code>`.
    pub answers: &'a mut dyn Write,
}

/// What one request on the control channel asks for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Request {
    /// `run <id>...`: apply these tweaks, in the order they are given.
    Run(Vec<String>),
    /// `revert <id>...`: undo them.
    Revert(Vec<String>),
}

impl Request {
    /// The request in `line`, or `None` when the line is neither `run` nor `revert`, or names no
    /// tweak at all. The channel carries ids and nothing else, so a line that says anything else
    /// is not a request and is refused rather than guessed at.
    #[must_use]
    pub fn parse(line: &str) -> Option<Self> {
        let (word, rest) = line.trim().split_once(char::is_whitespace)?;
        let ids: Vec<String> = rest.split_whitespace().map(str::to_owned).collect();
        if ids.is_empty() {
            return None;
        }
        match word {
            "run" => Some(Self::Run(ids)),
            "revert" => Some(Self::Revert(ids)),
            _ => None,
        }
    }

    /// The line the screen writes for this request, the words [`Request::parse`] reads back. The
    /// command line's own words without their dashes, since the channel carries ids and not a
    /// command: nothing that reaches here becomes a command of the session's own choosing.
    #[must_use]
    pub fn line(&self) -> String {
        std::iter::once(self.word()).chain(self.ids().iter().map(String::as_str)).collect::<Vec<_>>().join(" ")
    }

    /// The ids it names, whichever way it goes.
    #[must_use]
    pub fn ids(&self) -> &[String] {
        match self {
            Self::Run(ids) | Self::Revert(ids) => ids,
        }
    }

    /// Whether this request undoes the tweaks rather than applying them.
    #[must_use]
    pub fn is_revert(&self) -> bool {
        matches!(self, Self::Revert(_))
    }

    /// The first id no tweak of the catalog for `cpu` answers to, when there is one. The catalog
    /// is the machine's own, exactly as `--run` builds it, so the two ways of asking for the
    /// same tweak accept and refuse the same ids.
    #[must_use]
    pub fn unknown_id(&self, cpu: Cpu) -> Option<&str> {
        let catalog = catalog::all(cpu);
        self.ids().iter().find(|id| !catalog.iter().any(|tweak| tweak.id == id.as_str())).map(String::as_str)
    }

    /// The command line the child is started with: the words a person could type themselves.
    #[must_use]
    pub fn args(&self) -> Vec<&str> {
        let flag = match self {
            Self::Run(_) => "--run",
            Self::Revert(_) => "--revert",
        };
        std::iter::once(flag).chain(self.ids().iter().map(String::as_str)).collect()
    }

    /// The one word that says which way it goes, on the channel and on the command line.
    fn word(&self) -> &'static str {
        match self {
            Self::Run(_) => "run",
            Self::Revert(_) => "revert",
        }
    }
}

/// The answer a request's child gets, on the sibling of the control channel: `done <code>`, one
/// line each, so the screen learns how the run went from the same channel the request took.
#[must_use]
pub fn done(code: u32) -> String {
    format!("{DONE} {code}")
}

/// The code in an answer line, or `None` when the line is not an answer at all.
#[must_use]
pub fn answered(line: &str) -> Option<u32> {
    line.trim().strip_prefix(DONE)?.trim().parse().ok()
}

/// Carries out the requests on the control channel `control` until the channel is closed, as
/// `qtools --session <control>` does: this very binary carries them out in the terminal this
/// process was given, and `sudo` keeps its ticket for as long as the screen is open.
///
/// # Errors
///
/// Returns the error when either end of the channel cannot be opened, or the terminal cannot be
/// written to. The screen is told nothing either way, so qtools has to be started again.
pub fn serve(control: &Path, terminal: &mut dyn Write) -> io::Result<()> {
    let requests = File::open(control)?;
    let mut answers = OpenOptions::new().write(true).open(answers_of(control))?;
    let mut requests = BufReader::new(requests);
    let session = Session::here()?;
    session.serve(Control { requests: &mut requests, answers: &mut answers }, terminal)
}

/// The processor of the machine this session runs on, read the way `--run` reads it: an item
/// that installs a package for a processor is in the catalog only on that processor, and the
/// channel refuses an id the catalog does not have.
#[must_use]
pub fn here_cpu() -> Cpu {
    cpu(&mut crate::system::RealSystem::new())
}

/// The channel the exit codes go back through: the sibling of the control channel, named after it,
/// so the two always belong together and the screen can name both from the one path it keeps.
#[must_use]
pub fn answers_of(control: &Path) -> PathBuf {
    let mut answers = control.as_os_str().to_os_string();
    answers.push(".answers");
    PathBuf::from(answers)
}

/// The qtools session: the terminal every run shares, the channel that asks for one, and the
/// keepalive that holds `sudo`'s ticket between them.
#[derive(Debug, Clone)]
pub struct Session {
    /// The qtools binary a request is carried out by.
    program: PathBuf,
    /// The program that owns `sudo`'s ticket, and the only one the keepalive ever runs.
    sudo: PathBuf,
    /// How often the ticket is refreshed once there is one.
    interval: Duration,
    /// The processor this session's machine has, which decides what the catalog holds and so
    /// which ids the channel accepts.
    cpu: Cpu,
}

impl Session {
    /// A session that carries every request out with `program` and keeps the ticket with `sudo`
    /// every `interval`, refusing the ids the catalog for `cpu` does not have. A test points
    /// `program` and `sudo` at stand-ins; `qtools --session` passes this very binary, `sudo` from
    /// the path and the machine's own processor.
    #[must_use]
    pub fn new(program: PathBuf, sudo: PathBuf, interval: Duration, cpu: Cpu) -> Self {
        Self { program, sudo, interval, cpu }
    }

    /// The session `qtools --session` starts: this very binary carries the requests out, in the
    /// terminal this process was given, `sudo` keeps its ticket and the machine's own processor
    /// says which ids there are.
    ///
    /// # Errors
    ///
    /// Returns the error when this binary's own path cannot be read. A request could not be
    /// carried out then, and the screen would wait for an answer that never came.
    pub fn here() -> io::Result<Self> {
        Ok(Self::new(std::env::current_exe()?, PathBuf::from("sudo"), KEEPALIVE, here_cpu()))
    }

    /// Carries out the requests that arrive on `control` until it is closed, and says what
    /// happened on `terminal`, the terminal the screen shows: the child's own words, a refusal,
    /// and a cleared screen before each request, so one run's output never stands behind the
    /// next one's.
    ///
    /// However the loop ends — the screen closing the channel, the channel failing, the screen
    /// gone while a request was still being carried out — the refreshing ends and the ticket is
    /// dropped, because a session that could not finish its last request must not leave a ticket
    /// behind either.
    ///
    /// # Errors
    ///
    /// Returns the error when a request cannot be read, the child cannot be started, or the
    /// terminal or the channel cannot be written to. The screen is told nothing either way, so
    /// qtools has to be started again.
    pub fn serve(&self, mut control: Control<'_>, terminal: &mut dyn Write) -> io::Result<()> {
        let mut keepalive = None;
        let outcome = self.carry_requests(&mut control, terminal, &mut keepalive);
        // The refreshing ends first, so the last `-v` is over before the ticket is dropped and
        // nothing runs after it.
        drop(keepalive);
        self.forget();
        outcome
    }

    /// One request at a time until the channel ends, keeping `keepalive` in step with the
    /// requests: started once there is a ticket, and it ends with the channel.
    fn carry_requests(
        &self,
        control: &mut Control<'_>,
        terminal: &mut dyn Write,
        keepalive: &mut Option<Keepalive>,
    ) -> io::Result<()> {
        loop {
            let mut line = String::new();
            if control.requests.read_line(&mut line)? == 0 {
                // The screen closed the channel: it is gone, and this session is over.
                return Ok(());
            }
            let code = match Request::parse(&line) {
                // The words a command line uses for an id no tweak answers to, so both ways of
                // asking for the same thing refuse in the same words.
                Some(request) => match request.unknown_id(self.cpu) {
                    None => {
                        clear(terminal)?;
                        self.carry_out(&request)?
                    }
                    Some(id) => {
                        refuse(terminal, &t!("cli.unknown-tweak", id = id))?;
                        REFUSED
                    }
                },
                None => {
                    refuse(terminal, &t!("session.unknown-request"))?;
                    REFUSED
                }
            };
            writeln!(control.answers, "{}", done(code))?;
            control.answers.flush()?;
            // `sudo -n true` asks for nothing and says yes only once a password has been typed in
            // this terminal, which is the moment there is a ticket worth keeping.
            if keepalive.is_none() && self.holds_ticket() {
                *keepalive = Some(Keepalive::start(self.sudo.clone(), self.interval));
            }
        }
    }

    /// Carries out one request: this binary, with the words a person could type, in the terminal
    /// this process already has, so `sudo` sees the same terminal as the request before it. The
    /// child's exit code is what the screen shows.
    fn carry_out(&self, request: &Request) -> io::Result<u32> {
        // Inherited rather than piped: the terminal is the whole point, both for what the person
        // reads and for the `/dev/tty` `sudo` asks the password on.
        let status = Command::new(&self.program)
            .args(request.args())
            .stdin(Stdio::inherit())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .status()?;
        // A child a signal killed has no exit code of its own. It did not go through, and one is
        // what the screen shows for a run that did not.
        Ok(status.code().and_then(|code| u32::try_from(code).ok()).unwrap_or(1))
    }

    /// Whether `sudo` still holds a ticket for this terminal: `sudo -n true` asks for nothing
    /// and says yes only when a password has been given here.
    fn holds_ticket(&self) -> bool {
        run_sudo(&self.sudo, &["-n", "true"]).is_ok_and(|status| status.success())
    }

    /// Drops the ticket, so the next command in this terminal asks again. `sudo -k` for this
    /// terminal only, and `-n` so that dropping it asks for nothing either.
    fn forget(&self) {
        let _ = run_sudo(&self.sudo, &["-n", "-k"]);
    }
}

/// The thread that holds `sudo`'s ticket while the screen is open: `sudo -n -v` every interval,
/// which never asks for a password. It ends when the session drops it, and when a refresh fails:
/// a ticket that is gone only comes back when the person types the password again, and asking for
/// one every minute in between would fill the authentication log with failures.
struct Keepalive {
    /// Dropping this ends the wait at once instead of after another interval.
    stop: Option<Sender<()>>,
    /// The thread itself, so the last refresh is over before the ticket is dropped.
    thread: Option<JoinHandle<()>>,
}

impl Keepalive {
    /// Starts refreshing with `sudo` every `interval`.
    fn start(sudo: PathBuf, interval: Duration) -> Self {
        let (stop, wait) = mpsc::channel();
        let thread = std::thread::spawn(move || {
            // Nothing runs before the first interval is over: the session starts this right after
            // a request, and the ticket it has been good for a while by then. Only a timeout that
            // means the interval is over leads to a refresh; a sender that is gone ends the thread.
            while wait.recv_timeout(interval) == Err(RecvTimeoutError::Timeout) {
                match run_sudo(&sudo, &["-n", "-v"]) {
                    Ok(status) if status.success() => {}
                    _ => return,
                }
            }
        });
        Self { stop: Some(stop), thread: Some(thread) }
    }
}

impl Drop for Keepalive {
    fn drop(&mut self) {
        // The sender first: the thread is waiting for it, so it stops now rather than after
        // another interval, and the join below is the last refresh coming to an end.
        drop(self.stop.take());
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

/// Runs `sudo` with `args` and waits for it, saying nothing on the terminal: what the ticket is
/// doing is not the person's business, and a refresh that fails has nothing to say to them
/// either.
fn run_sudo(sudo: &Path, args: &[&str]) -> io::Result<ExitStatus> {
    Command::new(sudo).args(args).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).status()
}

/// Clears the terminal before a request: the cursor home and the whole screen wiped, which every
/// terminal reads the same way. The lines above it stay where they are, so the wheel still walks
/// back into the run before this one.
fn clear(terminal: &mut dyn Write) -> io::Result<()> {
    terminal.write_all(b"\x1b[H\x1b[2J")?;
    terminal.flush()
}

/// Says on the terminal that a request was refused and that nothing was run, in the place a run's
/// own words would start.
fn refuse(terminal: &mut dyn Write, why: &str) -> io::Result<()> {
    writeln!(terminal, "  {why}")?;
    terminal.flush()
}

#[cfg(test)]
mod tests;
