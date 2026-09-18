use std::path::PathBuf;

use super::*;
use crate::step::Step;
use crate::system::{Cmd, FakeSystem, Output};

fn folder() -> PathBuf {
    PathBuf::from("/home/test/.local/state/quvyta-tools")
}

fn ok() -> Output {
    Output { code: 0, stdout: String::new(), stderr: String::new() }
}

fn two_steps() -> Tweak {
    Tweak {
        id: "pacman-options",
        group: Group::Packages,
        options: Vec::new(),
        steps: vec![
            Step::ConfOption { file: "/etc/pacman.conf", key: "ParallelDownloads", value: "5".into() },
            Step::ConfOption { file: "/etc/pacman.conf", key: "Color", value: "on".into() },
        ],
        applies_to: |_| Availability::Yes,
    }
}

#[test]
fn a_tweak_with_nothing_done_is_off() {
    let mut system = FakeSystem::new();
    system.file("/etc/pacman.conf", "[options]\n");

    assert_eq!(two_steps().state(&mut system).expect("the file reads"), TweakState::Off);
}

#[test]
fn a_tweak_with_one_of_two_steps_done_is_half() {
    let mut system = FakeSystem::new();
    system.file("/etc/pacman.conf", "[options]\nParallelDownloads = 5\n");

    assert_eq!(two_steps().state(&mut system).expect("the file reads"), TweakState::Half);
}

#[test]
fn a_tweak_whose_steps_are_all_done_is_applied() {
    let mut system = FakeSystem::new();
    system.file("/etc/pacman.conf", "[options]\nParallelDownloads = 5\nColor = on\n");

    assert_eq!(two_steps().state(&mut system).expect("the file reads"), TweakState::Applied);
}

#[test]
fn a_step_someone_else_changed_makes_the_whole_tweak_changed() {
    let mut system = FakeSystem::new();
    system.file("/etc/pacman.conf", "[options]\nParallelDownloads = 12\nColor = on\n");

    assert_eq!(
        two_steps().state(&mut system).expect("the file reads"),
        TweakState::Changed("ParallelDownloads = 12".into())
    );
}

#[test]
fn a_tweak_that_does_not_fit_this_machine_says_so_instead_of_running() {
    let mut system = FakeSystem::new();
    let tweak = Tweak { applies_to: |_| Availability::No("no.nvidia"), ..two_steps() };

    assert_eq!(tweak.state(&mut system).expect("nothing runs"), TweakState::Unavailable("no.nvidia"));
    assert!(system.calls().is_empty(), "an unavailable tweak touches nothing");
}

#[test]
fn applying_writes_every_step_and_records_them_all() {
    let mut system = FakeSystem::new();
    system.file("/etc/pacman.conf", "[options]\n");
    let mut journal = crate::state::Journal::default();

    two_steps().apply(&mut system, &mut journal, &folder()).expect("both steps run");

    assert_eq!(two_steps().state(&mut system).expect("the file reads"), TweakState::Applied);
    assert_eq!(journal.entries("pacman-options").len(), 2);
}

#[test]
fn a_failing_step_stops_the_rest_and_says_which_step_failed() {
    let mut system = FakeSystem::new();
    system.file("/etc/pacman.conf", "[options]\n");
    let tweak = Tweak {
        steps: vec![
            Step::ConfOption { file: "/etc/pacman.conf", key: "Color", value: "on".into() },
            Step::PackageInstalled { package: "reflector" },
            Step::ServiceEnabled { unit: "reflector.timer" },
        ],
        ..two_steps()
    };
    system.answer(
        "sudo pacman -S --needed --noconfirm reflector",
        Output { code: 1, stdout: String::new(), stderr: "mirror unreachable".into() },
    );
    let mut journal = crate::state::Journal::default();

    let failure = tweak.apply(&mut system, &mut journal, &folder()).expect_err("the install fails");

    assert_eq!(failure.step, 1);
    assert!(failure.reason.contains("mirror unreachable"), "{}", failure.reason);
    assert!(
        !system.calls().iter().any(|call| call.contains("systemctl enable")),
        "the steps after the failure never run: {:?}",
        system.calls()
    );
    let entries = journal.entries("pacman-options");
    assert_eq!(entries.len(), 1, "the step that succeeded before the failure stays in the journal");
    assert_eq!(entries[0].step, 0, "so a half-applied tweak can still be undone");
}

#[test]
fn reverting_puts_the_file_back_and_forgets_the_tweak() {
    let mut system = FakeSystem::new();
    system.file("/etc/pacman.conf", "[options]\n");
    let mut journal = crate::state::Journal::default();
    two_steps().apply(&mut system, &mut journal, &folder()).expect("both steps run");

    two_steps().revert(&mut system, &mut journal).expect("both steps come back");

    let text = system.read(std::path::Path::new("/etc/pacman.conf")).unwrap().unwrap();
    assert_eq!(text, "[options]\n");
    assert!(journal.entries("pacman-options").is_empty());
}

#[test]
fn reverting_runs_the_undo_commands_newest_first() {
    let mut system = FakeSystem::new();
    let tweak = Tweak {
        steps: vec![Step::PackageInstalled { package: "reflector" }, Step::ServiceEnabled { unit: "reflector.timer" }],
        ..two_steps()
    };
    system.answer("sudo pacman -S --needed --noconfirm reflector", ok());
    system.answer("sudo systemctl enable --now reflector.timer", ok());
    system.answer("sudo systemctl disable --now reflector.timer", ok());
    system.answer("sudo pacman -Rns --noconfirm reflector", ok());
    let mut journal = crate::state::Journal::default();
    tweak.apply(&mut system, &mut journal, &folder()).expect("both steps run");

    tweak.revert(&mut system, &mut journal).expect("both steps come back");

    let undone: Vec<&String> =
        system.calls().iter().filter(|call| call.contains("disable") || call.contains("-Rns")).collect();
    assert_eq!(undone, ["sudo systemctl disable --now reflector.timer", "sudo pacman -Rns --noconfirm reflector"]);
}

// Carried item 1: the `Done`-skip is load-bearing. A step that is already done when
// `apply` runs must be skipped, and skipping it must leave no journal entry, so a
// later revert never touches something the person already had installed.
#[test]
fn a_step_that_is_already_done_is_skipped_and_leaves_no_journal_entry() {
    let mut system = FakeSystem::new();
    system.file("/etc/pacman.conf", "[options]\n");
    system.answer("pacman -Q reflector", ok());
    system.answer("sudo systemctl enable --now reflector.timer", ok());
    let tweak = Tweak {
        steps: vec![Step::PackageInstalled { package: "reflector" }, Step::ServiceEnabled { unit: "reflector.timer" }],
        ..two_steps()
    };
    let mut journal = crate::state::Journal::default();

    tweak.apply(&mut system, &mut journal, &folder()).expect("the missing step runs");

    assert_eq!(journal.entries("pacman-options").len(), 1, "only the step that actually ran is recorded");

    system.answer("sudo systemctl disable --now reflector.timer", ok());
    tweak.revert(&mut system, &mut journal).expect("the recorded step comes back");

    assert!(
        !system.calls().iter().any(|call| call.contains("-Rns")),
        "a package already installed before qtools ran is never removed: {:?}",
        system.calls()
    );
}

// Carried item 2: both undo branches for a file, exercised through a journal that has
// been through a save/load round trip, so the on-disk representation is proven too.
#[test]
fn reverting_a_file_that_existed_restores_it_byte_for_byte_through_a_saved_journal() {
    let mut system = FakeSystem::new();
    system.file("/etc/pacman.conf", "[options]\n#ParallelDownloads = 5\n");
    let tweak = Tweak {
        steps: vec![Step::ConfOption { file: "/etc/pacman.conf", key: "ParallelDownloads", value: "5".into() }],
        ..two_steps()
    };
    let mut journal = crate::state::Journal::default();
    tweak.apply(&mut system, &mut journal, &folder()).expect("the step runs");
    journal.save(&mut system, &folder()).expect("the journal is written");

    let mut journal = crate::state::Journal::load(&system, &folder()).expect("the journal reads back");
    let entry = journal.entries("pacman-options")[0].clone();
    assert_eq!(entry.step, 0);
    assert_eq!(entry.target, Some(PathBuf::from("/etc/pacman.conf")));
    assert!(entry.backup.is_some(), "a file that existed before is backed up");
    assert!(entry.undo.is_none(), "a file change has no command to undo it");
    assert!(entry.hash.is_some());

    tweak.revert(&mut system, &mut journal).expect("the file comes back");

    let text = system.read(std::path::Path::new("/etc/pacman.conf")).unwrap().unwrap();
    assert_eq!(text, "[options]\n#ParallelDownloads = 5\n");
}

#[test]
fn reverting_a_file_that_did_not_exist_removes_it_again_through_a_saved_journal() {
    let mut system = FakeSystem::new();
    let tweak = Tweak {
        steps: vec![Step::FileManaged {
            path: "/etc/quvyta-tools/managed.conf",
            content: "managed by qtools\n".into(),
        }],
        ..two_steps()
    };
    let mut journal = crate::state::Journal::default();
    tweak.apply(&mut system, &mut journal, &folder()).expect("the step runs");
    journal.save(&mut system, &folder()).expect("the journal is written");

    let mut journal = crate::state::Journal::load(&system, &folder()).expect("the journal reads back");
    let entry = journal.entries("pacman-options")[0].clone();
    assert_eq!(entry.target, Some(PathBuf::from("/etc/quvyta-tools/managed.conf")));
    assert_eq!(entry.backup, None, "a file that did not exist before has nothing to back up");

    tweak.revert(&mut system, &mut journal).expect("the file is removed again");

    assert_eq!(
        system.read(std::path::Path::new("/etc/quvyta-tools/managed.conf")).unwrap(),
        None,
        "the file is gone, not left behind empty"
    );
}

// Finding 2: the plain (non-`sudo`) branch of rebuilding an undo command was never
// exercised — every other revert test carries a root command.
#[test]
fn reverting_a_non_root_undo_command_runs_it_without_a_sudo_prefix() {
    let mut system = FakeSystem::new();
    let tweak = Tweak {
        steps: vec![Step::CommandRun {
            run: Cmd::new("flatpak", ["remote-add", "flathub", "https://flathub.org/repo/flathub.flatpakrepo"]),
            check: Cmd::new("flatpak", ["remote-list", "flathub"]),
            undo: Cmd::new("flatpak", ["remote-delete", "flathub"]),
            label: "flathub-remote",
        }],
        ..two_steps()
    };
    system.answer("flatpak remote-list flathub", Output { code: 1, stdout: String::new(), stderr: String::new() });
    system.answer("flatpak remote-add flathub https://flathub.org/repo/flathub.flatpakrepo", ok());
    system.answer("flatpak remote-delete flathub", ok());
    let mut journal = crate::state::Journal::default();
    tweak.apply(&mut system, &mut journal, &folder()).expect("the step runs");
    journal.save(&mut system, &folder()).expect("the journal is written");

    let mut journal = crate::state::Journal::load(&system, &folder()).expect("the journal reads back");
    tweak.revert(&mut system, &mut journal).expect("the step comes back");

    assert!(system.calls().iter().any(|call| call == "flatpak remote-delete flathub"), "{:?}", system.calls());
    assert!(
        !system.calls().iter().any(|call| call.starts_with("sudo")),
        "a non-root undo command runs without sudo: {:?}",
        system.calls()
    );
}

// Ruling D: revert must not clobber a change made after qtools wrote the file — by the
// person, or by another tweak sharing the same file. The journal's recorded digest is the
// tripwire.
#[test]
fn reverting_refuses_when_the_file_changed_after_qtools_wrote_it() {
    let mut system = FakeSystem::new();
    system.file("/etc/pacman.conf", "[options]\n");
    let tweak = Tweak {
        steps: vec![Step::ConfOption { file: "/etc/pacman.conf", key: "Color", value: String::new() }],
        ..two_steps()
    };
    let mut journal = crate::state::Journal::default();
    tweak.apply(&mut system, &mut journal, &folder()).expect("the step runs");

    // Something else changes the file after qtools wrote it.
    system.file("/etc/pacman.conf", "[options]\nColor\nSomethingElse = true\n");

    let error = tweak.revert(&mut system, &mut journal).expect_err("a changed file stops the revert");

    assert!(error.to_string().contains("/etc/pacman.conf"), "{error}");
    let text = system.read(std::path::Path::new("/etc/pacman.conf")).unwrap().unwrap();
    assert_eq!(text, "[options]\nColor\nSomethingElse = true\n", "the file is left untouched");
    assert_eq!(journal.entries("pacman-options").len(), 1, "the journal still holds the entry");
}

// Finding 3: a corrupted `applied.toml` must never panic. An entry whose stored undo
// argv is empty is a diagnostic naming the tweak and step, and the tweak keeps
// telling the truth about still being applied — `forget` is not reached.
#[test]
fn a_broken_undo_command_in_the_journal_does_not_panic_and_names_the_step() {
    let mut system = FakeSystem::new();
    system.file(folder().join("applied.toml"), "[[pacman-options]]\nstep = 2\nundo = []\n");
    let mut journal = crate::state::Journal::load(&system, &folder()).expect("the journal reads");

    let error =
        two_steps().revert(&mut system, &mut journal).expect_err("a broken undo command is a diagnostic, not a panic");

    let message = error.to_string();
    assert!(message.contains("pacman-options"), "{message}");
    assert!(message.contains('2'), "{message}");
    assert_eq!(journal.entries("pacman-options").len(), 1, "the tweak still says it is applied");
}
