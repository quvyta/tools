//! The screen's end of a session: while the `password-once` setting is on the runs of one qtools
//! share a terminal, the setting off gives every run a terminal of its own, and the session goes
//! when the screen goes.
//!
//! Every stand-in is a shell script in the system's temporary folder and every channel is in a
//! temporary folder of its own, so nothing a test does reaches the machine's real runtime folder,
//! its real `sudo`, or qtools itself. The stand-in for the session plays the part the real one
//! plays: it writes down each of its starts, reads a request line from the channel the screen
//! made, carries the line out by running the stand-in child in its own terminal, and answers with
//! the code that child chose. The child writes down the terminal it was given, so a test can see
//! that two runs shared one.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use qframe::icons::GlyphMode;
use qframe::prelude::*;

use super::super::tests::{Script, every_step_of, every_tweak_off, focus_tweaks, say_yes};
use super::super::wizard::tests::Root;
use super::super::{Msg, Opening, Reading, Tools};
use crate::catalog;
use crate::catalog::hardware::Cpu;
use crate::tweak::TweakState;

/// Generous: these tests start processes and wait for stand-ins in the temporary folder.
const PATIENCE: Duration = Duration::from_secs(20);

/// One step of waiting: the stand-ins need real time to do their work, and the screen's own waits
/// move with the harness's clock. Both are finite, and the whole wait is bounded above.
const STEP: Duration = Duration::from_millis(20);

/// Long enough for a write in the background to come back and a notice to settle in.
const MOMENT: Duration = Duration::from_millis(300);

/// The Settings page's password-once row, by the text a person clicks.
const ONCE: &str = "Ask for the password once while qtools is open";

/// The row's label as it reads before the page is too narrow for the whole of it, which is what a
/// screen test at a narrow size can look for.
const ONCE_HEAD: &str = "Ask for the password once";

/// The stand-ins of one test: the session that holds the one terminal, and the child it carries
/// out. The channel is in a folder of this test's own, so nothing of a session is ever made in
/// the machine's own folders.
struct Stands {
    /// The stand-in for the session process: the program the screen starts with `--session`.
    session: Script,
    /// The stand-in for the binary a run starts, which the session runs in its terminal.
    child: Script,
    /// Where the session writes down each of its starts and its end.
    started: PathBuf,
    /// Where the child writes down what it was asked to do and the terminal it was given.
    asked: PathBuf,
    /// The folder the channel is made in.
    folder: PathBuf,
}

impl Stands {
    /// The stand-ins of the test called `name`, each in a file of its own.
    fn new(name: &str) -> Self {
        let mine =
            |suffix: &str| std::env::temp_dir().join(format!("qtools-once-{}-{name}.{suffix}", std::process::id()));
        let started = mine("start");
        let asked = mine("ask");
        let folder = mine("channel");
        let _ = fs::remove_dir_all(&folder);
        // The child first, so the session's own words can name it: what it was asked to do, the
        // terminal it was given and the process it was, then the words the person reads while the
        // run goes on. It opens with the session marker sudo prints around its own commands, which
        // the terminal is there to swallow. The marker beside the log is what makes a run fail
        // rather than go through.
        let child = Script::new(
            &format!("{name}-child"),
            &format!(
                "printf '%s on %s by %s\\n' \"$*\" \"$(tty 2>/dev/null || echo none)\" \"$$\" >> {asked}\n\
                 printf '\\033]3008;start=1789000000;type=session\\033\\\\ going through\\n'\n\
                 [ -e {failed} ] && exit 1\nexit 0\n",
                asked = quoted(&asked),
                failed = quoted(&failed(&asked))
            ),
        );
        // The session: one line per start with its own pid, then one request at a time out of the
        // channel, each carried out by the child in this terminal and answered with its code. The
        // channel arrives the way the screen writes it, `--session <control>`, and the end of the
        // channel is the end of the session, which is what the screen's leaving does.
        let session = Script::new(
            &format!("{name}-session"),
            &format!(
                "printf 'start %s\\n' \"$$\" >> {started}\nexec 3< \"$2\"\nexec 4> \"$2.answers\"\n\
                 while IFS= read -r request <&3; do\n\
                 \x20 printf '\\033[H\\033[2J'\n\
                 \x20 set -- $request\n\
                 \x20 kind=$1\n\
                 \x20 shift\n\
                 \x20 {child} \"--${{kind}}\" \"$@\"\n\
                 \x20 code=$?\n\
                 \x20 printf 'done %s\\n' \"$code\" >&4\n\
                 done\n\
                 printf 'end %s\\n' \"$$\" >> {started}\nexit 0\n",
                started = quoted(&started),
                child = quoted(&script_path(&format!("{name}-child")))
            ),
        );
        Self { session, child, started, asked, folder }
    }

    /// The screen these stand-ins serve, with the setting as it is unless the file says otherwise:
    /// the runs go to the session, which holds the one terminal.
    fn harness(&self) -> Harness<Tools> {
        self.screen_with(self.session.0.clone())
    }

    /// The same screen with the setting off: a run is a process of its own, started with
    /// `--run`, exactly as it was before there was a session. The stand-in for it is the child the
    /// session would have carried out, so both ways of running write into the same log.
    fn apart(&self) -> Harness<Tools> {
        self.screen_with(self.child.0.clone())
    }

    /// The screen whose runs are started by `program`, given a channel folder of this test's own.
    fn screen_with(&self, program: PathBuf) -> Harness<Tools> {
        let tools = Tools::new(every_tweak_off(), Cpu::Intel)
            .machine(program, reading_off)
            .recorded(vec![Vec::new(); catalog::all(Cpu::Intel).len()])
            .one_terminal(Some(self.folder.clone()));
        let mut h = Harness::with_env(tools, crate::locales::env(), 100, 30);
        h.set_locale("en").set_glyph_mode(GlyphMode::Unicode);
        h
    }

    /// How many times the session was started, and with which pids.
    fn starts(&self) -> Vec<String> {
        lines(&self.started).into_iter().filter(|line| line.starts_with("start ")).collect()
    }

    /// What the child was asked to do, each line naming the terminal it was given.
    fn asked(&self) -> Vec<String> {
        lines(&self.asked)
    }

    /// The marker that makes the next child fail instead of going through.
    fn fail_next(&self) {
        fs::write(failed(&self.asked), "").expect("the marker");
    }
}

/// The marker beside a child's log that makes it fail, the way a machine that says no does.
fn failed(asked: &Path) -> PathBuf {
    PathBuf::from(format!("{}.fail", asked.display()))
}

fn lines(path: &Path) -> Vec<String> {
    fs::read_to_string(path).unwrap_or_default().lines().map(str::to_owned).collect()
}

/// The terminal a line of a child's log names: the word before the one that says which process it
/// was. A line is `--run mirrors on /dev/pts/3 by 4711`.
fn terminal_of(line: &str) -> &str {
    line.rsplit_once(" by ")
        .and_then(|(rest, _)| rest.rsplit_once(' '))
        .map(|(_, terminal)| terminal)
        .expect("the child names its terminal")
}

/// The process a line of a child's log came from, which is what says two runs were two processes
/// and not one session carrying out two requests.
fn process_of(line: &str) -> &str {
    line.rsplit_once(" by ").map(|(_, process)| process).expect("the child names its process")
}

/// A path as a shell script can put in its own words, so a folder with a space in its name is
/// still one word.
fn quoted(path: &Path) -> String {
    format!("'{}'", path.display().to_string().replace('\'', r"'\''"))
}

/// Where [`Script::new`] puts the stand-in of that name, the way a session's own words name the
/// child it carries out before the child is written.
fn script_path(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!("qtools-{}-{name}.sh", std::process::id()))
}

/// What the screen reads after a run: every tweak off, nothing on qtools' record.
fn reading_off(_: &crate::choices::Choices) -> Reading {
    let states = every_tweak_off();
    let recorded = vec![Vec::new(); states.len()];
    Reading { states, recorded }
}

/// Waits for `what` to be so. The stand-ins need real time to do their work and the screen's own
/// waits move with the harness's clock, so both are given, in finite steps inside a bound.
fn wait_for(h: &mut Harness<Tools>, what: &str, mut ready: impl FnMut(&Tools) -> bool) {
    let started = Instant::now();
    while !ready(h.app()) {
        assert!(started.elapsed() < PATIENCE, "{what} never happened:\n{}", h.screen());
        std::thread::sleep(STEP);
        h.advance(STEP);
    }
}

/// Whether the run on screen has been answered and its outcome is on the badge.
fn answered(app: &Tools) -> bool {
    app.running.as_ref().is_some_and(|running| running.exit.is_some())
}

/// Opens the Settings page the way a person does at this size: the sidebar's last entry on a wide
/// screen, the strip's last tab on a narrow one, and then the keyboard's own walk onto the last
/// row of the page. A settings page scrolls to the row the keyboard is on, so the row it ends on
/// is on screen whatever the size is.
fn open_settings(h: &mut Harness<Tools>, width: u16) {
    for _ in 0..5 {
        if h.is_focused("groups") {
            break;
        }
        h.press("tab");
    }
    assert!(h.is_focused("groups"), "the groups are reached:\n{}", h.screen());
    if width < 72 {
        for _ in crate::tweak::Group::ALL {
            h.press("right");
        }
    } else {
        h.press("end");
    }
    h.press("enter");
    for _ in 0..4 {
        if h.is_focused("appearance") {
            break;
        }
        h.press("tab");
    }
    assert!(h.is_focused("appearance"), "the rows of the page are reached:\n{}", h.screen());
    h.press("end");
    let screen = h.screen();
    assert!(screen.contains(ONCE_HEAD), "the password-once row is the last of the page:\n{screen}");
}

/// Turns the row the keyboard is on with its own switch, and lets the write in the background
/// come back.
fn turn_row(h: &mut Harness<Tools>) {
    h.press("space");
    h.advance(MOMENT);
}

/// Confirms the chosen tweak, waits for the run to be answered, and leaves the answer on screen:
/// what a person does with enter, the button, and then a look.
fn confirm(h: &mut Harness<Tools>) {
    focus_tweaks(h);
    h.press("enter");
    say_yes(h);
    wait_for(h, "the run to be answered", answered);
}

#[test]
fn two_confirmed_runs_share_one_terminal_and_both_outcomes_reach_the_badge() {
    let stands = Stands::new("shared");
    let mut h = stands.harness();

    confirm(&mut h);
    let screen = h.screen();
    assert!(screen.contains("Applying Mirror list"), "the run says what it is doing:\n{screen}");
    assert!(screen.contains("Done"), "and how it went:\n{screen}");
    assert!(screen.contains("going through"), "the run's own words are on the terminal:\n{screen}");
    assert!(!screen.contains("3008"), "and the session marker sudo prints is not text on it:\n{screen}");
    h.press("esc");
    assert!(h.app().running.is_none(), "esc closes the ended run while the session lives on");
    assert!(h.screen().contains("Mirror list"), "the list is back:\n{}", h.screen());

    // The next run fails, and the badge says so. The terminal is the same one, cleared for it.
    stands.fail_next();
    confirm(&mut h);
    let screen = h.screen();
    assert!(screen.contains("It failed"), "the failure is announced:\n{screen}");
    h.press("esc");
    assert!(h.app().running.is_none(), "and esc closes it again");
    assert!(h.screen().contains("Mirror list"), "and the list comes back again:\n{}", h.screen());

    let asked = stands.asked();
    assert_eq!(asked.len(), 2, "both runs were carried out: {asked:?}");
    assert!(asked.iter().all(|line| line.starts_with("--run mirrors ")), "with the ids of the chosen tweak: {asked:?}");
    let terminal = terminal_of(&asked[0]);
    assert!(
        asked.iter().all(|line| terminal_of(line) == terminal),
        "one terminal for both runs, {terminal}: {asked:?}"
    );
    assert_ne!(process_of(&asked[0]), process_of(&asked[1]), "and each its own process: {asked:?}");
    assert_eq!(stands.starts().len(), 1, "and one session behind it: {:?}", stands.starts());
}

#[test]
fn a_revert_goes_through_the_session_too() {
    let stands = Stands::new("reverted");
    let mut h = stands.harness();
    let mut states = every_tweak_off();
    states[0] = TweakState::Applied;
    let recorded = every_step_of(&(0..states.len()).collect::<Vec<_>>(), states.len(), Cpu::Intel);
    h.send(Msg::StatesRead(Reading { states, recorded }));

    focus_tweaks(&mut h);
    h.press("z");
    assert!(h.screen().contains("Undo this?"), "undoing asks first:\n{}", h.screen());
    say_yes(&mut h);
    wait_for(&mut h, "the undo to be answered", answered);
    let screen = h.screen();
    assert!(screen.contains("Undoing Mirror list"), "{screen}");
    assert!(screen.contains("Done"), "{screen}");
    assert_eq!(stands.asked().len(), 1, "the session carried the undo out: {:?}", stands.asked());
    assert!(stands.asked()[0].starts_with("--revert mirrors "), "{:?}", stands.asked());
}

#[test]
fn the_session_goes_when_the_screen_goes_and_leaves_nothing_in_the_folder() {
    let stands = Stands::new("ending");
    let mut h = stands.harness();

    confirm(&mut h);
    assert_eq!(stands.starts().len(), 1, "the run started a session");

    // The screen is gone: it let go of the channel, and the session came out of its own read.
    drop(h);
    assert!(
        lines(&stands.started).iter().any(|line| line.starts_with("end ")),
        "the session ended by itself once the channel was closed: {:?}",
        lines(&stands.started)
    );
    let left: Vec<PathBuf> = fs::read_dir(&stands.folder)
        .map(|entries| entries.flatten().map(|entry| entry.path()).collect())
        .unwrap_or_default();
    assert!(left.is_empty(), "nothing of a session that is over is left in the folder: {left:?}");
    let _ = fs::remove_dir_all(&stands.folder);
}

/// The switch turned off while a session is alive ends that session the way the screen going
/// does: nothing is left refreshing `sudo`'s ticket for a setting that is now off.
#[test]
fn turning_the_setting_off_ends_the_session_that_is_alive() {
    let stands = Stands::new("turned-off");
    let mut h = stands.harness();
    h.set_reduced_motion(true);

    confirm(&mut h);
    assert_eq!(stands.starts().len(), 1, "the run started a session");
    h.press("esc");
    assert!(h.app().session.is_some(), "and closing the run leaves it there: nothing ends it but the screen");

    open_settings(&mut h, 100);
    turn_row(&mut h);
    assert!(!h.app().password_once, "the switch is off now");
    assert!(h.app().session.is_none(), "and the session it held is gone");
    assert!(
        lines(&stands.started).iter().any(|line| line.starts_with("end ")),
        "the session ended by itself once the channel was closed: {:?}",
        lines(&stands.started)
    );
    let left: Vec<PathBuf> = fs::read_dir(&stands.folder)
        .map(|entries| entries.flatten().map(|entry| entry.path()).collect())
        .unwrap_or_default();
    assert!(left.is_empty(), "and nothing of it is left in the folder: {left:?}");
    let _ = fs::remove_dir_all(&stands.folder);
}

/// The setting off, turned the way a person turns it: every run gets a terminal of its own,
/// exactly as it did before there was a session, and no session is ever started.
#[test]
fn the_setting_turned_off_on_the_page_gives_every_run_a_terminal_of_its_own() {
    let stands = Stands::new("apart");
    let mut h = stands.apart();
    h.set_reduced_motion(true);

    open_settings(&mut h, 100);
    h.click_text(ONCE);
    turn_row(&mut h);
    assert!(!h.app().password_once, "the switch is off now");
    h.click_text("Packages");

    for _ in 0..2 {
        focus_tweaks(&mut h);
        h.press("enter");
        say_yes(&mut h);
        // Each run is waited for: nothing is asked while one is on screen, so the second would
        // never be asked for.
        wait_for(&mut h, "the run to be answered", answered);
        // The badge carries the run's own exit code, which says this was a run of its own and not
        // a request nobody answered.
        assert!(h.screen().contains("Done"), "the run's own code is on the badge:\n{}", h.screen());
        h.press("esc");
    }
    let asked = stands.asked();
    assert_eq!(asked.len(), 2, "two runs, two processes: {asked:?}");
    assert_ne!(process_of(&asked[0]), process_of(&asked[1]), "each a process of its own: {asked:?}");
    assert!(stands.starts().is_empty(), "no session was ever started, so no keepalive either: {:?}", stands.starts());
    let _ = fs::remove_dir_all(&stands.folder);
}

/// The switch is qtools' own key in `tools.conf`, it is written in the background, and the next
/// start opens with what it says.
#[test]
fn the_switch_is_written_into_the_settings_file_and_read_at_the_next_start() {
    let root = Root::new();
    let folder = root.path().join("config");
    set_up_in(&folder);
    let opening = Opening::new(Some(&folder), Some(&root.path().join("fonts")), every_tweak_off(), Cpu::Intel);
    assert!(opening.tools.password_once, "on unless the file says otherwise");
    let mut h = Harness::with_env(opening.tools, crate::locales::env(), 100, 40);
    h.set_locale("en").set_glyph_mode(GlyphMode::Unicode).set_reduced_motion(true);

    open_settings(&mut h, 100);
    turn_row(&mut h);
    let own = fs::read_to_string(folder.join("tools.conf")).unwrap_or_default();
    assert!(own.contains("password-once = false"), "qtools' own file takes the key: {own}");

    let again = Opening::new(Some(&folder), Some(&root.path().join("fonts")), every_tweak_off(), Cpu::Intel);
    assert!(!again.tools.password_once, "the next start opens with what the file says");
}

/// A file that cannot be written leaves the switch where it was and says why, the way the update
/// notice's own switch does.
#[test]
fn a_password_once_switch_that_cannot_be_written_goes_back_on() {
    let root = Root::new();
    let folder = root.path().join("config");
    set_up_in(&folder);
    let opening = Opening::new(Some(&folder), Some(&root.path().join("fonts")), every_tweak_off(), Cpu::Intel);
    let mut h = Harness::with_env(opening.tools, crate::locales::env(), 100, 40);
    h.set_locale("en").set_glyph_mode(GlyphMode::Unicode).set_reduced_motion(true);

    open_settings(&mut h, 100);
    h.click_text(ONCE);
    fs::set_permissions(&folder, fs::Permissions::from_mode(0o555)).expect("read-only");
    turn_row(&mut h);
    let screen = h.screen();
    fs::set_permissions(&folder, fs::Permissions::from_mode(0o755)).expect("writable again");
    assert!(screen.contains("could not be saved"), "{screen}");
    assert!(h.app().password_once, "the switch went back on");
    let own = fs::read_to_string(folder.join("tools.conf")).unwrap_or_default();
    assert!(!own.contains("password-once = false"), "and the file says nothing that was not written: {own}");
}

/// The row is a row of the Settings page at every size the screen has, its own words are read at
/// every size, and it keeps the rules in ASCII glyphs.
#[test]
fn the_password_once_row_reads_at_every_size_and_keeps_the_ascii_rules() {
    for (width, height) in [(100, 40), (48, 30), (80, 14)] {
        for glyphs in [GlyphMode::Unicode, GlyphMode::Ascii] {
            let root = Root::new();
            let folder = root.path().join("config");
            set_up_in(&folder);
            let opening = Opening::new(Some(&folder), Some(&root.path().join("fonts")), every_tweak_off(), Cpu::Intel);
            let mut h = Harness::with_env(opening.tools, crate::locales::env(), width, height);
            h.set_locale("en").set_glyph_mode(glyphs).set_reduced_motion(true);
            open_settings(&mut h, width);
            let what = format!("{width}x{height} {glyphs:?}");
            let screen = h.screen();
            assert!(!screen.contains('⟦'), "{what}: a key has no text:\n{screen}");
            // The description wraps over as many lines as the row needs, so what it says is read
            // as one run of words rather than as the lines it was drawn in.
            let words = screen.split_whitespace().collect::<Vec<_>>().join(" ");
            assert!(words.contains("The password is never written to a file"), "{what}: its own words:\n{screen}");
            if glyphs == GlyphMode::Ascii {
                for forbidden in ["[ ]", "[x]", "(o)", "|", "===", "{", "}"] {
                    assert!(!screen.contains(forbidden), "{what}: `{forbidden}` is forbidden:\n{screen}");
                }
            }
        }
    }
}

/// qtools set up before, in the shared Quvyta folder `folder`: a shared look in English and
/// qtools' own file, so the first-run wizard is not what opens.
fn set_up_in(folder: &Path) {
    fs::create_dir_all(folder).expect("folder");
    fs::write(folder.join("quvyta.conf"), "language = \"en\"\ntheme = \"monochrome\"\nicons = \"unicode\"\n")
        .expect("the shared file");
    fs::write(folder.join("tools.conf"), "language = \"quvyta\"\ntheme = \"quvyta\"\nicons = \"quvyta\"\n")
        .expect("qtools' own file");
}

/// The channel goes in the runtime folder, which is this person's own time on the machine, and
/// in the state folder where the backups already are when there is no runtime folder.
#[test]
fn the_channel_goes_in_the_runtime_folder_or_in_the_state_folder() {
    let runtime = || Some("/run/user/1000".to_owned());
    let state = || Some("/state".to_owned());
    let home = || Some("/home/someone".to_owned());
    assert_eq!(
        super::channel_folder_from(runtime(), state(), home()),
        Some(PathBuf::from("/run/user/1000/quvyta-tools"))
    );
    assert_eq!(super::channel_folder_from(None, state(), home()), Some(PathBuf::from("/state/quvyta-tools")));
    assert_eq!(
        super::channel_folder_from(None, None, home()),
        Some(PathBuf::from("/home/someone/.local/state/quvyta-tools"))
    );
    assert_eq!(super::channel_folder_from(None, None, None), None, "a machine with neither has nowhere");
    assert_eq!(super::channel_folder_from(Some(String::new()), None, None), None, "an empty one is no folder");
}
