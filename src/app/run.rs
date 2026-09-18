//! The confirm, run and revert flow: what a run says it will do, and the run itself while the
//! embedded terminal carries it out.
//!
//! The screen never runs a step itself. It starts its own binary with `--run` or `--revert`
//! (see [`crate::cli`]) inside a pseudo-terminal and shows that terminal, so every command has
//! a terminal of its own: `sudo` asks for the password there, and qtools never reads, carries
//! or stores it.

use std::ffi::OsString;
use std::io;
use std::path::{Path, PathBuf};

use qframe::prelude::*;
use qframe::widgets::TerminalSession;

use crate::step::{Step, Touch};
use crate::tweak::Tweak;

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
        Ok(Self { titles, reverting, session, run, exit: None })
    }

    /// Whether the process ended without an error.
    #[must_use]
    pub fn succeeded(&self) -> bool {
        self.exit == Some(Some(0))
    }
}

/// The confirmation's message: what every chosen tweak touches, and how many steps ask for a
/// password.
pub fn plan_text(tweaks: &[Tweak]) -> String {
    let mut lines = Vec::new();
    let mut password_steps = 0usize;
    for tweak in tweaks {
        lines.push(t!(&format!("tweak.{}.title", tweak.id)));
        for touch in tweak.touches() {
            lines.push(format!("  {}", touch_line(&touch)));
        }
        password_steps += tweak.steps.iter().filter(|step| step_needs_root(step)).count();
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
        Touch::File(path) => path.clone(),
        Touch::Service(unit) => format!("{unit}  {}", t!("touch.service")),
        Touch::Command(label) => format!("{label}  {}", t!("touch.command")),
    }
}

/// Whether a step's own command runs as root. `ConfOption`, `PackageInstalled`, `ServiceEnabled`
/// and `FileManaged` always write through a root helper (see `Step::apply`); the other two carry
/// their own command, which may or may not ask for one.
fn step_needs_root(step: &Step) -> bool {
    match step {
        Step::ConfOption { .. }
        | Step::PackageInstalled { .. }
        | Step::ServiceEnabled { .. }
        | Step::FileManaged { .. } => true,
        Step::CommandRun { run, .. } | Step::FileRewritten { run, .. } => run.needs_root(),
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::catalog::packages;

    /// Runs `f` with the application's own language files, English active, so `t!` resolves
    /// outside a harness.
    fn in_english<R>(f: impl FnOnce() -> R) -> R {
        let mut i18n = crate::locales::env().i18n().clone();
        i18n.set_active("en");
        qframe::i18n::scope(Arc::new(i18n), f)
    }

    #[test]
    fn the_plan_names_every_touch() {
        let text = plan_text(&[packages::mirrors("US", 10)]);
        assert!(text.contains("reflector"), "{text}");
        assert!(text.contains("/etc/pacman.d/mirrorlist"), "{text}");
        assert!(text.contains("reflector.timer"), "{text}");
    }

    #[test]
    fn several_tweaks_all_appear_in_one_plan() {
        let text = plan_text(&[packages::multilib(), packages::cache_cleanup()]);
        assert!(text.contains("/etc/pacman.conf"), "{text}");
        assert!(text.contains("paccache"), "{text}");
    }

    #[test]
    fn the_plan_says_how_many_steps_ask_for_a_password() {
        let text = in_english(|| plan_text(&[packages::pacman_options(5)]));
        assert!(text.contains("password"), "{text}");
        assert!(text.contains("/etc/pacman.conf"), "{text}");
    }
}
