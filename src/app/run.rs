//! The confirm, run and revert flow: what a run says it will do, and the run itself while the
//! embedded terminal carries it out.
//!
//! The screen never runs a step itself. It starts its own binary with `--run` or `--revert`
//! (see [`crate::cli`]) inside a pseudo-terminal and shows that terminal, so every command has
//! a terminal of its own: `sudo` asks for the password there, and qtools never reads, carries
//! or stores it. That is what happens when the `password-once` setting is off. With it on, the
//! pseudo-terminal is the session's and stays alive between runs, so the password is asked once
//! for the whole qtools; see [`crate::app::session`].

use std::ffi::OsString;
use std::io;
use std::path::{Path, PathBuf};

use qframe::prelude::*;
use qframe::widgets::TerminalSession;

use crate::step::{Step, Touch};
use crate::tweak::{Tweak, TweakState};

/// A confirmed run or revert: the process carrying it out and how it ended.
#[derive(Debug)]
pub struct Running {
    /// The titles of the tweaks being applied or undone, in the active language.
    pub titles: Vec<String>,
    /// Whether the tweaks are being undone rather than applied.
    pub reverting: bool,
    /// The process, in its pseudo-terminal.
    pub session: TerminalSession,
    /// Tells a watch of an earlier run from this one's.
    pub run: u64,
    /// How the process ended, once it has: its exit code, or `None` when it could not be read.
    pub exit: Option<Option<u32>>,
    /// Whether the terminal belongs to the session that lives as long as qtools does
    /// ([`crate::app::session`]). Then this run has no process of its own to wait for: it ends
    /// when the session answers over the channel, and the terminal keeps its words for the runs
    /// after this one.
    pub held: bool,
}

impl Running {
    /// Starts `program` with `--run` (or `--revert`) and the tweak ids in a pseudo-terminal.
    ///
    /// # Errors
    ///
    /// Fails when no pseudo-terminal can be opened or the program cannot start.
    pub fn start(program: &Path, ids: &[&str], titles: Vec<String>, reverting: bool, run: u64) -> io::Result<Self> {
        let mut args: Vec<OsString> = vec![if reverting { "--revert" } else { "--run" }.into()];
        args.extend(ids.iter().map(OsString::from));
        let folder = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/"));
        let session = TerminalSession::spawn(program.as_os_str(), &args, &folder)?;
        Ok(Self { titles, reverting, session, run, exit: None, held: false })
    }

    /// Whether the process ended without an error.
    #[must_use]
    pub fn succeeded(&self) -> bool {
        self.exit == Some(Some(0))
    }
}

/// The confirmation's message: what every chosen tweak touches, where a file it writes is backed
/// up first (an undo writes no backup, so it says nothing of them), and how many steps ask for a
/// password.
///
/// Each tweak comes with where it stands. A run leaves alone a step that is done already, so an
/// apply counts only the steps its reading does not show as done; an undo, and a reading that
/// does not say, count every step.
pub fn plan_text(tweaks: &[(Tweak, TweakState)], revert: bool, backups: &str) -> String {
    let mut lines = Vec::new();
    let mut password_steps = 0usize;
    let mut writes_files = false;
    for (tweak, state) in tweaks {
        lines.push(t!(&format!("tweak.{}.title", tweak.id)));
        for touch in tweak.touches() {
            writes_files |= matches!(touch, Touch::File(_));
            lines.push(format!("  {}", touch_line(&touch)));
        }
        let waiting = |&(index, _): &(usize, &Step)| revert || state.step_done(index) != Some(true);
        password_steps +=
            tweak.steps.iter().enumerate().filter(waiting).filter(|(_, step)| step_needs_root(step)).count();
    }
    if writes_files && !revert {
        lines.push(String::new());
        lines.push(t!("confirm.backups", folder = backups));
    }
    if password_steps > 0 {
        lines.push(String::new());
        lines.push(t!("confirm.password-steps", n = password_steps));
    }
    lines.join("\n")
}

/// One touched thing, worded the way the detail panel words it.
pub(crate) fn touch_line(touch: &Touch) -> String {
    match touch {
        Touch::Package(name) => format!("{name}  {}", t!("touch.package")),
        Touch::NeededPackage(name) => format!("{name}  {}", t!("touch.needed-package")),
        Touch::File(path) => path.clone(),
        Touch::Service(unit) => format!("{unit}  {}", t!("touch.service")),
        Touch::Unit(unit) => format!("{unit}  {}", t!("touch.unit")),
        Touch::Command(label) => format!("{label}  {}", t!("touch.command")),
    }
}

/// Whether a step's own command runs as root. `ConfOption`, `PackageInstalled`, `ServiceEnabled`,
/// `UnitEnabled`, `FileManaged` and `ListItemAdded` always write through a root helper (see `Step::apply`); the
/// other two carry their own command, which may or may not ask for one.
fn step_needs_root(step: &Step) -> bool {
    match step {
        Step::ConfOption { .. }
        | Step::PackageInstalled { .. }
        | Step::PackageNeeded { .. }
        | Step::ServiceEnabled { .. }
        | Step::UnitEnabled { .. }
        | Step::FileManaged { .. }
        | Step::FileReplaced { .. }
        | Step::ListItemAdded { .. } => true,
        Step::CommandRun { run, .. } | Step::FileRewritten { run, .. } => run.needs_root(),
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::catalog::packages;
    use crate::problem::Problem;

    /// A backup folder no real run uses, so finding it in the text means the plan wrote it.
    const BACKUPS: &str = "/somewhere/else/backups";

    /// Runs `f` with the application's own language files, English active, so `t!` resolves
    /// outside a harness.
    fn in_english<R>(f: impl FnOnce() -> R) -> R {
        let mut i18n = crate::locales::env().i18n().clone();
        i18n.set_active("en");
        qframe::i18n::scope(Arc::new(i18n), f)
    }

    #[test]
    fn the_plan_names_every_touch() {
        let text = plan_text(&[(packages::mirrors("US", 10), TweakState::Off)], false, BACKUPS);
        assert!(text.contains("reflector"), "{text}");
        assert!(text.contains("/etc/pacman.d/mirrorlist"), "{text}");
        assert!(text.contains("reflector.timer"), "{text}");
    }

    #[test]
    fn several_tweaks_all_appear_in_one_plan() {
        let text = plan_text(
            &[(packages::multilib(), TweakState::Off), (packages::cache_cleanup(), TweakState::Off)],
            false,
            BACKUPS,
        );
        assert!(text.contains("/etc/pacman.conf"), "{text}");
        assert!(text.contains("paccache"), "{text}");
    }

    #[test]
    fn the_plan_says_how_many_steps_ask_for_a_password() {
        let text = in_english(|| plan_text(&[(packages::pacman_options(5), TweakState::Off)], false, BACKUPS));
        assert!(text.contains("password"), "{text}");
        assert!(text.contains("/etc/pacman.conf"), "{text}");
    }

    #[test]
    fn the_plan_says_where_the_files_it_writes_are_backed_up() {
        let text = in_english(|| plan_text(&[(packages::pacman_options(5), TweakState::Off)], false, BACKUPS));
        assert!(text.contains(BACKUPS), "{text}");
        // An undo writes no backup, and a tweak that writes no file has none to keep.
        let undo = in_english(|| plan_text(&[(packages::pacman_options(5), TweakState::Applied)], true, BACKUPS));
        assert!(!undo.contains(BACKUPS), "{undo}");
        let no_file = in_english(|| plan_text(&[(packages::cache_cleanup(), TweakState::Off)], false, BACKUPS));
        assert!(!no_file.contains(BACKUPS), "{no_file}");
    }

    #[test]
    fn an_apply_counts_only_the_password_steps_still_to_do_and_an_undo_counts_them_all() {
        // Every step of the mirrors item runs as root; the reading has the package and the timer
        // in place, so a run does the other two and asks no password for the done ones.
        let half = TweakState::Half(vec![true, false, false, true]);
        let mirrors = || packages::mirrors("US", 10);
        let text = in_english(|| plan_text(&[(mirrors(), half.clone())], false, BACKUPS));
        assert!(text.contains("2 steps will ask for your password."), "{text}");
        let off = in_english(|| plan_text(&[(mirrors(), TweakState::Off)], false, BACKUPS));
        assert!(off.contains("4 steps will ask for your password."), "{off}");
        let applied = in_english(|| plan_text(&[(mirrors(), TweakState::Applied)], false, BACKUPS));
        assert!(!applied.contains("password"), "nothing is left to do:\n{applied}");
        // A reading that could not be made does not say what is done, so every step counts.
        let unknown =
            in_english(|| plan_text(&[(mirrors(), TweakState::Unknown(Problem::RootToRead))], false, BACKUPS));
        assert!(unknown.contains("4 steps will ask for your password."), "{unknown}");
        // An undo is not asked of the reading: it puts back what qtools has on record.
        let undo = in_english(|| plan_text(&[(mirrors(), half)], true, BACKUPS));
        assert!(undo.contains("4 steps will ask for your password."), "{undo}");
    }
}
