//! The session process: one request at a time, a child that carries it out in this terminal, the
//! ticket held only once there is one, and the ticket dropped when the screen is gone.
//!
//! The channel is a pair of sockets here rather than the two named pipes of a real session, which
//! is the same two directions without files; everything else is the real thing. The stand-ins are
//! shell scripts in the system's temporary folder: a binary that writes down what it was asked to
//! do, and a `sudo` that writes down its arguments and only succeeds once the test says a
//! password has been typed.

use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use super::*;
use crate::app::tests::Script;

/// Generous: these tests run next to many others on a busy machine.
const PATIENCE: Duration = Duration::from_secs(20);

/// How often the keepalive refreshes here, so a test does not wait a minute for it.
const QUICK: Duration = Duration::from_millis(20);

/// Runs `f` with qtools' own language files and English active, so `t!` resolves outside a harness.
fn in_english<R>(f: impl FnOnce() -> R) -> R {
    let mut i18n = crate::locales::env().i18n().clone();
    i18n.set_active("en");
    qframe::i18n::scope(Arc::new(i18n), f)
}

/// A stand-in for the binary a request is carried out by: it writes down the command line it was
/// given into `log`, and ends the way `marker(log, "fail")` standing there says, so a test can
/// have one run go through and the next one fail without a second script.
fn child(name: &str, log: &Path) -> Script {
    let log = log.display().to_string();
    Script::new(name, &format!("printf '%s\\n' \"$*\" >> '{log}'\n[ -e '{log}.fail' ] && exit 1\nexit 0\n"))
}

/// A stand-in for `sudo`: it writes down its arguments into `log`, and says yes only while
/// `marker(log, "typed")` is there, which is how a test says the person has typed the password
/// and there is a ticket to keep.
fn sudo(name: &str, log: &Path) -> Script {
    let log = log.display().to_string();
    Script::new(name, &format!("printf '%s\\n' \"$*\" >> '{log}'\n[ -e '{log}.typed' ]\n"))
}

/// A file beside the log a stand-in keeps, which is what it looks at to change what it answers.
fn marker(log: &Path, kind: &str) -> PathBuf {
    PathBuf::from(format!("{}.{kind}", log.display()))
}

/// What a stand-in has written down, one argument list per line, with the lines that have not
/// arrived yet left out.
fn lines(log: &Path) -> Vec<String> {
    fs::read_to_string(log).unwrap_or_default().lines().map(str::to_owned).collect()
}

/// A terminal a test can read: the session writes to it from its own thread, as it writes to the
/// pseudo-terminal it was given.
#[derive(Clone, Default)]
struct Terminal(Arc<Mutex<Vec<u8>>>);

impl Write for Terminal {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.0.lock().expect("the terminal is not poisoned").extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl Terminal {
    fn text(&self) -> String {
        String::from_utf8_lossy(&self.0.lock().expect("the terminal is not poisoned")).into_owned()
    }
}

/// The screen's side of a session that is running: where a test writes a request, where the
/// answer comes back, and the terminal the session writes to.
struct Screen {
    /// Where a request is written.
    requests: UnixStream,
    /// Where the answer to a request is read.
    answers: UnixStream,
    /// The terminal the session writes to.
    terminal: Terminal,
    /// The thread the session runs in.
    session: Option<JoinHandle<io::Result<()>>>,
}

impl Screen {
    /// Starts `session` in its own thread and returns the screen's side of the channel.
    fn start(session: Session) -> Self {
        let (requests, incoming) = UnixStream::pair().expect("a socket pair");
        let (outgoing, answers) = UnixStream::pair().expect("a socket pair");
        let terminal = Terminal::default();
        let written = terminal.clone();
        let thread = std::thread::spawn(move || {
            let mut incoming = BufReader::new(incoming);
            let mut outgoing = outgoing;
            let mut terminal = written;
            let control = Control { requests: &mut incoming, answers: &mut outgoing };
            in_english(|| session.serve(control, &mut terminal))
        });
        Self { requests, answers, terminal, session: Some(thread) }
    }

    /// Asks for `line` and waits for the answer, the way the screen does.
    fn ask(&mut self, line: &str) -> String {
        writeln!(self.requests, "{line}").expect("the request goes in");
        self.requests.flush().expect("the request is sent");
        let mut answer = String::new();
        BufReader::new(&self.answers).read_line(&mut answer).expect("the answer comes back");
        answer
    }

    /// What the session has written to the terminal.
    fn shown(&self) -> String {
        self.terminal.text()
    }

    /// Closes the channel, as the screen does when it goes, and waits for the session to be over.
    fn close(mut self) {
        drop(std::mem::replace(&mut self.requests, closed()));
        if let Some(session) = self.session.take() {
            let outcome = session.join().expect("the session thread ended");
            outcome.expect("the session ended without an error");
        }
    }
}

/// A closed socket, to take the screen's end of the channel away.
fn closed() -> UnixStream {
    UnixStream::pair().expect("a socket pair").0
}

/// Waits until `what` says so, without giving up before `PATIENCE`.
fn wait_for(what: &str, mut ready: impl FnMut() -> bool) {
    let started = Instant::now();
    while !ready() {
        assert!(started.elapsed() < PATIENCE, "{what} never happened");
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn a_request_is_carried_out_by_the_child_and_its_code_comes_back() {
    let (log, sudo_log) = (log_path("carried-out"), sudo_log_path("carried-out"));
    let (child, sudo) = (child("carried-out-child", &log), sudo("carried-out-sudo", &sudo_log));
    let session = Session::new(child.0.clone(), sudo.0.clone(), QUICK, Cpu::default());
    let mut screen = Screen::start(session);

    assert_eq!(screen.ask("run mirrors multilib"), "done 0\n");
    assert_eq!(lines(&log), vec!["--run mirrors multilib"], "the child got the words a person could type");
    assert!(screen.shown().starts_with("\x1b[H\x1b[2J"), "the screen is cleared first: {:?}", screen.shown());
    screen.close();
}

#[test]
fn a_revert_is_carried_out_the_way_a_run_is() {
    let (log, sudo_log) = (log_path("reverted"), sudo_log_path("reverted"));
    let (child, sudo) = (child("reverted-child", &log), sudo("reverted-sudo", &sudo_log));
    let session = Session::new(child.0.clone(), sudo.0.clone(), QUICK, Cpu::default());
    let mut screen = Screen::start(session);

    assert_eq!(screen.ask("revert mirrors"), "done 0\n");
    assert_eq!(lines(&log), vec!["--revert mirrors"]);
    screen.close();
}

#[test]
fn a_child_that_fails_comes_back_as_a_failure() {
    let (log, sudo_log) = (log_path("failed"), sudo_log_path("failed"));
    fs::write(marker(&log, "fail"), "").expect("the marker");
    let (child, sudo) = (child("failed-child", &log), sudo("failed-sudo", &sudo_log));
    let session = Session::new(child.0.clone(), sudo.0.clone(), QUICK, Cpu::default());
    let mut screen = Screen::start(session);

    assert_eq!(screen.ask("run mirrors"), "done 1\n", "the code the child chose is the one the screen shows");
    screen.close();
    let _ = fs::remove_file(&log);
}

#[test]
fn a_child_a_signal_killed_is_a_failure_too() {
    let sudo_log = sudo_log_path("killed");
    let (child, sudo) = (Script::new("killed-child", "kill -9 $$\n"), sudo("killed-sudo", &sudo_log));
    let session = Session::new(child.0.clone(), sudo.0.clone(), QUICK, Cpu::default());
    let mut screen = Screen::start(session);

    // A signalled child has no exit code of its own; it did not go through, and one is what the
    // screen shows for a run that did not.
    assert_eq!(screen.ask("run mirrors"), "done 1\n");
    screen.close();
}

#[test]
fn an_id_no_tweak_answers_to_is_refused_on_the_terminal_and_nothing_runs() {
    let (log, sudo_log) = (log_path("unknown-id"), sudo_log_path("unknown-id"));
    let (child, sudo) = (child("unknown-id-child", &log), sudo("unknown-id-sudo", &sudo_log));
    let session = Session::new(child.0.clone(), sudo.0.clone(), QUICK, Cpu::default());
    let mut screen = Screen::start(session);

    assert_eq!(
        screen.ask("run no-such-tweak"),
        "done 2\n",
        "the code a command line gives for an argument it cannot read"
    );
    let shown = screen.shown();
    assert!(shown.contains("no-such-tweak"), "the terminal says which id: {shown:?}");
    assert!(!shown.contains("Applying"), "nothing was carried out: {shown:?}");
    screen.close();
    assert!(lines(&log).is_empty(), "no child ran: {:?}", lines(&log));
}

#[test]
fn a_request_that_is_neither_run_nor_revert_is_refused() {
    let (log, sudo_log) = (log_path("not-a-request"), sudo_log_path("not-a-request"));
    let (child, sudo) = (child("not-a-request-child", &log), sudo("not-a-request-sudo", &sudo_log));
    let session = Session::new(child.0.clone(), sudo.0.clone(), QUICK, Cpu::default());
    let mut screen = Screen::start(session);

    for line in ["frobnicate mirrors", "run", "revert", "", "  ", "runs mirrors"] {
        assert_eq!(screen.ask(line), "done 2\n", "`{line}` is not a request");
    }
    let shown = screen.shown();
    assert_eq!(shown.matches("did nothing").count(), 6, "each one says so on the terminal: {shown:?}");
    screen.close();
    assert!(lines(&log).is_empty(), "no child ran: {:?}", lines(&log));
}

#[test]
fn the_ticket_is_kept_only_after_a_root_command_and_dropped_when_the_screen_is_gone() {
    let (log, sudo_log) = (log_path("ticket"), sudo_log_path("ticket"));
    let (child, sudo) = (child("ticket-child", &log), sudo("ticket-sudo", &sudo_log));
    let session = Session::new(child.0.clone(), sudo.0.clone(), QUICK, Cpu::default());
    let mut screen = Screen::start(session);

    // No password has been typed, so there is no ticket: `sudo -n true` says no and the
    // authentication log is not filled with a refresh every minute before anything was asked.
    assert_eq!(screen.ask("run mirrors"), "done 0\n");
    assert_eq!(lines(&log), vec!["--run mirrors"], "the child carried the request out");
    std::thread::sleep(QUICK * 8);
    assert_eq!(lines(&sudo_log), vec!["-n true"], "nothing was refreshed: {:?}", lines(&sudo_log));

    // The person typed the password during that run: the ticket is there, and it is kept.
    fs::write(marker(&sudo_log, "typed"), "").expect("the marker");
    assert_eq!(screen.ask("run mirrors"), "done 0\n");
    let refreshing = || lines(&sudo_log).iter().filter(|line| line.as_str() == "-n -v").count();
    wait_for("the ticket is refreshed", || refreshing() >= 3);

    screen.close();
    let after = lines(&sudo_log);
    assert_eq!(after.last().map(String::as_str), Some("-n -k"), "the ticket is dropped last: {after:?}");
    assert_eq!(after.iter().filter(|line| line.as_str() == "-n -k").count(), 1, "and dropped once: {after:?}");
    assert!(
        after.iter().all(|line| line.split(' ').all(|word| word != "-v" || line.starts_with("-n -v"))),
        "no refresh without -n, which would ask for the password: {after:?}"
    );
    let dropped = after.iter().position(|line| line == "-n -k").expect("the ticket is dropped");
    let last_refresh = after.iter().rposition(|line| line == "-n -v").expect("the ticket was refreshed");
    assert!(dropped > last_refresh, "the last refresh is over before it: {after:?}");

    // Nothing is left running once the screen has gone.
    std::thread::sleep(QUICK * 8);
    assert_eq!(lines(&sudo_log), after, "no refresh follows the drop");
    let _ = fs::remove_file(&sudo_log);
}

#[test]
fn a_refresh_that_fails_ends_the_refreshing() {
    let (log, sudo_log) = (log_path("vanished"), sudo_log_path("vanished"));
    fs::write(marker(&sudo_log, "typed"), "").expect("the marker");
    let (child, sudo) = (child("vanished-child", &log), sudo("vanished-sudo", &sudo_log));
    let session = Session::new(child.0.clone(), sudo.0.clone(), QUICK, Cpu::default());
    let mut screen = Screen::start(session);

    assert_eq!(screen.ask("run mirrors"), "done 0\n");
    let refreshing = || lines(&sudo_log).iter().filter(|line| line.as_str() == "-n -v").count();
    wait_for("the ticket is refreshed", || refreshing() > 0);

    // The ticket is gone again (a `sudo -k` of the person's own, a much shorter timeout): the
    // refreshing stops rather than asking for a password nobody is there to type.
    fs::remove_file(marker(&sudo_log, "typed")).expect("the marker goes");
    let last = refreshing();
    wait_for("the refreshing ends", || refreshing() > last);
    std::thread::sleep(QUICK * 8);
    assert_eq!(refreshing(), last + 1, "the one refresh that failed was the last");
    screen.close();
    let _ = fs::remove_file(&sudo_log);
}

#[test]
fn a_request_is_parsed_from_the_words_the_screen_writes() {
    let parsed = Request::parse("run mirrors multilib");
    assert_eq!(parsed, Some(Request::Run(vec!["mirrors".to_owned(), "multilib".to_owned()])));
    assert_eq!(Request::parse("  revert   mirrors  "), Some(Request::Revert(vec!["mirrors".to_owned()])));
    assert_eq!(parsed.as_ref().map(Request::args), Some(vec!["--run", "mirrors", "multilib"]));
    let revert = Request::parse("revert mirrors");
    assert_eq!(revert.as_ref().map(Request::args), Some(vec!["--revert", "mirrors"]));
}

#[test]
fn anything_that_is_not_a_request_is_not_parsed_as_one() {
    for line in ["", "   ", "run", "revert", "runmirrors", "frobnicate mirrors", "--run mirrors", "run  "] {
        assert_eq!(Request::parse(line), None, "`{line}` is not a request");
    }
}

#[test]
fn a_request_names_an_id_no_tweak_answers_to() {
    let known = Request::parse("run mirrors").expect("a request");
    assert_eq!(known.unknown_id(Cpu::default()), None);
    let unknown = Request::parse("run mirrors no-such-tweak multilib").expect("a request");
    assert_eq!(unknown.unknown_id(Cpu::default()), Some("no-such-tweak"));
}

#[test]
fn the_answers_come_back_through_the_sibling_of_the_control_channel() {
    assert_eq!(
        answers_of(Path::new("/run/user/1000/quvyta-tools/session-42")),
        Path::new("/run/user/1000/quvyta-tools/session-42.answers")
    );
}

/// A path of this test's own in the system's temporary folder, so two tests never share a log: one
/// for what the child was asked to do, another for what `sudo` was.
fn log_path(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!("qtools-session-{}-{name}.log", std::process::id()))
}

/// The same for the stand-in `sudo`; see [`log_path`].
fn sudo_log_path(name: &str) -> PathBuf {
    log_path(name).with_file_name(format!("qtools-session-{}-{name}.sudo", std::process::id()))
}
