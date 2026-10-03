//! The screen's own end of a qtools session: the one pseudo-terminal that lives as long as
//! qtools does, the channel its requests go through, and the answers that come back over it.
//!
//! While the `password-once` setting is on, a confirmed run is not a process of its own any more:
//! it is a line the screen writes to a session that is already there (see [`crate::session`]),
//! and that session carries the line out in its terminal as a `--run`/`--revert` child which
//! inherits it. `sudo` keeps its ticket per terminal, so the password is asked in that one
//! terminal and asked once, however many runs follow it.
//!
//! The channel is two named pipes in a folder only this user can open: the screen writes
//! `run <id>...` or `revert <id>...` into one and reads `done <code>` out of the other, so the
//! channel carries tweak ids and never a command line. A pipe is what says "I am gone" by
//! itself: the session's read ends the moment the screen lets go of its end of the first pipe,
//! which is how the session learns to stop refreshing `sudo`'s ticket and to drop it. A socket
//! would say the same, but nothing that can stand in for the session can write to one, and what
//! stands in for it here is a shell script.//!
//! Nothing is asked at start: the session begins with the first confirmed run, and it goes when
//! the screen goes.

use std::ffi::OsString;
use std::fs::{self, File, OpenOptions, Permissions};
use std::io::{self, BufRead, BufReader, Write};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use qframe::prelude::*;
use qframe::runtime::Task;
use qframe::widgets::{TerminalChange, TerminalEvent, TerminalSession, TerminalWatch};

use crate::session::{Request, answered, answers_of};

use super::Msg;

/// The folder qtools keeps its own files in, inside the runtime folder.
const FOLDER: &str = "quvyta-tools";

/// How long the screen waits for a session to go once it has let go of the channel, before the
/// terminal is closed under it. Generous: a session has to come out of a child that may still be
/// running, drop `sudo`'s ticket and go, and qtools is not worth hurrying it for.
const GRACE: Duration = Duration::from_secs(2);

/// How long the closed terminal is watched for its end while the grace runs.
const PATIENCE: Duration = Duration::from_millis(20);

/// How long a run's own task waits for the terminal to change and for the answer to come. Short
/// enough that the terminal is redrawn as the words arrive and a session that has ended is
/// noticed at once; the waits themselves are what a test harness's clock moves.
const POLL: Duration = Duration::from_millis(20);

/// The name the run's task is given in a thread list. It is never shown: the task reports no
/// events, and the outcome of a run is the badge beside its title, not a row in a list.
const NAME: &str = "qtools-run";

/// What a run's task says when the screen asked it to stop, which only happens as qtools quits.
const STOPPED: &str = "stopped";

/// The outcome a run gets whose session ended before it could answer it: it did not go through,
/// and one is what the screen shows for a run that did not.
pub const UNANSWERED: u32 = 1;

/// Where a session's control channel is kept: the runtime folder, which belongs to this person's
/// own time on the machine and is cleaned up when they log out, or the state folder where the
/// backups already are when there is no runtime folder. `None` on a machine with neither, and
/// then there is nowhere to keep a channel and every run gets a terminal of its own.
#[must_use]
pub fn channel_folder() -> Option<PathBuf> {
    channel_folder_from(
        std::env::var("XDG_RUNTIME_DIR").ok(),
        std::env::var("XDG_STATE_HOME").ok(),
        std::env::var("HOME").ok(),
    )
}

/// The pure part of [`channel_folder`], taking the three variables as arguments, so it can be
/// read without the machine's own: a machine with no runtime folder, no state folder and no home
/// folder has nowhere to keep a channel.
#[must_use]
fn channel_folder_from(runtime: Option<String>, state: Option<String>, home: Option<String>) -> Option<PathBuf> {
    if let Some(folder) = runtime.filter(|value| !value.is_empty()) {
        return Some(PathBuf::from(folder).join(FOLDER));
    }
    // The state folder is where the backups already are, and it is a folder of this person's own
    // only because one of the two variables names it; with neither there is no such folder.
    let state = state.filter(|value| !value.is_empty());
    let home = home.filter(|value| !value.is_empty());
    if state.is_none() && home.is_none() {
        return None;
    }
    Some(crate::state::folder_from(state, home))
}

/// The qtools session the screen holds open for as long as qtools is open: the process in one
/// pseudo-terminal, the ends of its channel, and the codes its requests are answered with.
///
/// Dropping it closes the channel, which is the only way a session is asked to go, and then waits
/// for the session to go: nothing of a session may be left running once the screen is gone.
pub struct Live {
    /// The session process, in the terminal every run shares.
    terminal: TerminalSession,
    /// Where the requests are written. Let go on its own first, so the session's read ends while
    /// the reader of the answers is still there to hear it go.
    requests: Option<File>,
    /// The answers, one line per request, as the thread that reads them leaves them.
    answers: Arc<Mutex<Receiver<u32>>>,
    /// The two pipes of this session, in a folder of this person's own, gone once it is over.
    channel: PathBuf,
}

impl Live {
    /// Starts `program --session <control>` in a pseudo-terminal of its own, with the two pipes of
    /// the channel made in `folder` and the screen's end of the requests open before the session
    /// starts. The pipes are named after this process, and one screen keeps one session, so nothing
    /// else of qtools is in that folder.
    ///
    /// Nothing here waits for the session, because the screen may not be held up and a program
    /// that takes a moment to arrive must not be able to freeze it. The requests' pipe is opened
    /// at both ends, which a named pipe answers without waiting for anyone: the session's read
    /// returns at once, and a request written before the session has arrived waits in the pipe
    /// rather than on it. The answers are read on a thread of their own, because the session is
    /// the one that opens that end and a read waits for the next request for as long as qtools is
    /// open.
    ///
    /// # Errors
    ///
    /// Fails when `folder` cannot be made a folder only its owner can open, when a pipe of the
    /// channel cannot be made, or when no pseudo-terminal can be opened for the session. What
    /// was made before the failure is taken away again.
    pub fn start(program: &Path, folder: &Path) -> io::Result<Self> {
        let channel = folder.join(format!("session-{}", std::process::id()));
        make_folder(folder)?;
        make_pipe(&channel)?;
        if let Err(error) = make_pipe(&answers_of(&channel)) {
            let _ = fs::remove_file(&channel);
            return Err(error);
        }
        // The screen's end of the requests, open at both ends of the pipe: a request goes into it,
        // and the session's read ends the moment this end is let go of.
        let requests = OpenOptions::new().read(true).write(true).open(&channel)?;
        let answers = answers_of(&channel);
        let terminal = match start_session(program, &channel) {
            Ok(terminal) => terminal,
            Err(error) => return Err(removed(&channel, &answers, error)),
        };
        let (sender, receiver) = mpsc::channel();
        // The thread is left to end by itself: it ends when the session's end of the answers is
        // gone, and a session that never opened it leaves a thread that ends with qtools.
        std::thread::spawn(move || read_answers(&answers, &sender));
        Ok(Self { terminal, requests: Some(requests), answers: Arc::new(Mutex::new(receiver)), channel })
    }

    /// The terminal every run of this session happens in. A clone of the session's own, so the run
    /// on screen and the session behind it are one terminal and not two.
    fn terminal(&self) -> TerminalSession {
        self.terminal.clone()
    }

    /// Asks the session to carry `request` out in this terminal and hands back what the screen
    /// needs to follow it: the terminal to draw and the command that redraws it as the session
    /// writes and ends the run when the answer comes. Nothing is carried out on the screen's own:
    /// the words go down the channel and the answer comes back over its sibling, and the process
    /// behind the pipe is the one that runs.
    ///
    /// # Errors
    ///
    /// Fails when the channel is already closed, which only happens while the session is being
    /// ended, or when the pipe cannot be written to.
    pub fn ask(&mut self, run: u64, request: &Request) -> io::Result<(TerminalSession, Command<Msg>)> {
        let requests = self.requests.as_mut().ok_or_else(|| io::Error::from(io::ErrorKind::BrokenPipe))?;
        writeln!(requests, "{}", request.line())?;
        requests.flush()?;
        Ok((self.terminal(), self.watching(run, self.terminal().watch())))
    }

    /// The watch of run `run`: the terminal drawn again as the session writes to it, and the code
    /// the request is answered with once it comes. A task rather than a perform, because the
    /// terminal of a session does not end with the run: there is no process of this run to wait
    /// for, and a wait that would never come back would hold the screen for as long as qtools is
    /// open.
    #[must_use]
    fn watching(&self, run: u64, watch: TerminalWatch) -> Command<Msg> {
        let answers = Arc::clone(&self.answers);
        Command::task(Task::new(NAME, move |cx| {
            loop {
                match lock(&answers).try_recv() {
                    // The answer of the request that was asked: the run is over.
                    Ok(code) => return Ok(Msg::Answered(run, Some(code))),
                    // The session's end of the answers is gone, so it will answer nothing: the
                    // screen is told that rather than waiting for an answer that is not coming.
                    Err(TryRecvError::Disconnected) => return Ok(Msg::Answered(run, None)),
                    Err(TryRecvError::Empty) => {}
                }
                match watch.next_change_within(POLL) {
                    Some(TerminalChange::Exited(code)) => {
                        cx.send(Msg::Changed(run, TerminalEvent::Exited(code)));
                        return Ok(Msg::Answered(run, None));
                    }
                    // What a session writes is what the person reads while the run goes on. What
                    // else the program says about itself, as a shell does about its title or its
                    // folder, is not the screen's business, exactly as the watch of a run that
                    // has a process of its own reports only the output and the end.
                    Some(TerminalChange::Output) => cx.send(Msg::Changed(run, TerminalEvent::Output)),
                    Some(_) => {}
                    // Nothing in this time: the wait goes on until the answer or the end.
                    None => {}
                }
                // The wait the task's own clock keeps, so the screen is not held by a thread that
                // is only looking.
                if !cx.sleep(POLL) {
                    return Err(STOPPED.to_owned());
                }
            }
        }))
    }
}

impl Drop for Live {
    fn drop(&mut self) {
        // The channel first: the session's read of the requests ends, so it stops refreshing
        // `sudo`'s ticket, drops it and goes. Nothing waits for that read, and the thread that
        // reads the answers is left to hear the session go rather than waited for: a session that
        // has not come would otherwise hold qtools open for nothing.
        drop(self.requests.take());
        // And then it is given the time to do that: qtools is not worth leaving a refreshing
        // ticket behind for.
        let started = Instant::now();
        while self.terminal.exit().is_none() && started.elapsed() < GRACE {
            std::thread::sleep(PATIENCE);
        }
        // Nothing of a session that is over is left in the folder. One that is still there after
        // the grace ends with the terminal dropped below it, and its ticket is of no use to
        // anyone: it belongs to the terminal that is going away.
        let _ = fs::remove_file(&self.channel);
        let _ = fs::remove_file(answers_of(&self.channel));
    }
}

impl std::fmt::Debug for Live {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Live").field("channel", &self.channel).finish_non_exhaustive()
    }
}

/// Starts this very binary as a session in a pseudo-terminal, in `folder`.
fn start_session(program: &Path, channel: &Path) -> io::Result<TerminalSession> {
    let folder = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/"));
    let args: Vec<OsString> = vec!["--session".into(), channel.as_os_str().to_owned()];
    TerminalSession::spawn(program.as_os_str(), &args, &folder)
}

/// Reads the codes the session answers with, one line each, until its end of the pipe is gone.
/// Runs on a thread of its own, because opening that end waits for the session to arrive and
/// every read waits for the next request.
fn read_answers(answers: &Path, sender: &mpsc::Sender<u32>) {
    // An end that cannot be opened is a session that never answered anything, and the end of the
    // thread says so: the channel closes and the run waiting on it is told the session is gone.
    let Ok(pipe) = OpenOptions::new().read(true).open(answers) else { return };
    for line in BufReader::new(pipe).lines().map_while(Result::ok) {
        // A line that is not an answer is not this protocol's, and a run cannot be reported from
        // it: the channel closes and the screen says the session ended rather than guess.
        let Some(code) = answered(&line) else { return };
        if sender.send(code).is_err() {
            // The screen has gone, so there is nobody left to answer.
            return;
        }
    }
}

/// Makes `folder`, a folder only its owner may open: the control channel inside it is a way to
/// run things as the person who started qtools, so nobody else has any business in it.
fn make_folder(folder: &Path) -> io::Result<()> {
    fs::create_dir_all(folder)?;
    fs::set_permissions(folder, Permissions::from_mode(0o700))
}

/// Makes the named pipe `path` with `mkfifo` of coreutils, which is in `base` on Arch and in
/// every distribution built on it. A machine without it is told so rather than given a channel
/// that would not work; the run then happens in a terminal of its own, as it always did.
fn make_pipe(path: &Path) -> io::Result<()> {
    // A pipe of a session that was killed rather than closed would still be there under the same
    // name, and it belongs to this person alone: it goes before another is made.
    let _ = fs::remove_file(path);
    // Nothing of `mkfifo` is for qtools' own screen: it says nothing when it works, and what it
    // says when it cannot is carried back as the error rather than printed into the display.
    let silent = std::process::Command::new("mkfifo")
        .arg("-m")
        .arg("0600")
        .arg(path)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
    match silent {
        Ok(status) if status.success() => Ok(()),
        // The machine's own words: what `mkfifo` said, or that there is no such program.
        Ok(status) => Err(io::Error::other(status.to_string())),
        Err(error) => Err(error),
    }
}

/// The channel taken away again after a failure, so a folder is never left with half of one.
fn removed(channel: &Path, answers: &Path, error: io::Error) -> io::Error {
    let _ = fs::remove_file(channel);
    let _ = fs::remove_file(answers);
    error
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

#[cfg(test)]
mod tests;
