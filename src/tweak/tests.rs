use std::path::PathBuf;

use super::*;
use crate::problem::Problem;
use crate::step::Step;
use crate::system::{Cmd, FakeSystem, Output, System};

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
            Step::ConfOption {
                file: "/etc/pacman.conf",
                section: Some("options"),
                key: "ParallelDownloads",
                value: "5".into(),
            },
            Step::ConfOption { file: "/etc/pacman.conf", section: Some("options"), key: "Color", value: "on".into() },
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

    assert_eq!(two_steps().state(&mut system).expect("the file reads"), TweakState::Half(vec![true, false]));
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
        TweakState::Changed(Problem::OptionDiffers {
            file: PathBuf::from("/etc/pacman.conf"),
            key: "ParallelDownloads".into(),
            found: "12".into()
        })
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
fn a_run_refuses_a_tweak_that_does_not_fit_before_its_first_step() {
    let mut system = FakeSystem::new();
    system.file("/etc/pacman.conf", "[options]\n");
    let tweak = Tweak { applies_to: |_| Availability::No("unavailable.no-bluetooth"), ..two_steps() };
    let mut journal = crate::state::Journal::default();

    let failure = tweak.apply(&mut system, &mut journal, &folder()).expect_err("it does not fit");

    assert_eq!(failure.step, None);
    assert_eq!(failure.reason, Problem::DoesNotFit("unavailable.no-bluetooth"));
    assert_eq!(
        system.read(std::path::Path::new("/etc/pacman.conf")).expect("it reads").as_deref(),
        Some("[options]\n")
    );
    assert!(!journal.has("pacman-options"));
}

#[test]
fn a_state_that_cannot_be_read_is_unknown_with_the_reason_rather_than_off() {
    let mut system = FakeSystem::new();
    system.refuse_reads("/etc/pacman.conf");

    match two_steps().reading(&mut system) {
        TweakState::Unknown(Problem::Unreadable(why)) => {
            assert!(matches!(*why, Problem::System(_)), "the machine's own words: {why:?}");
        }
        other => panic!("an unread tweak must not look off or applied: {other:?}"),
    }
}

#[test]
fn a_check_only_root_can_make_is_not_run_by_the_screen() {
    let mut system = FakeSystem::new();
    system.answer("sudo sh -c true", ok());
    system.reading();
    let tweak = Tweak {
        steps: vec![Step::CommandRun {
            run: Cmd::new("true", Vec::<String>::new()).root(),
            check: Cmd::new("sh", ["-c", "true"]).root(),
            undo: Cmd::new("true", Vec::<String>::new()).root(),
            label: "true",
        }],
        ..two_steps()
    };

    assert_eq!(tweak.reading(&mut system), TweakState::Unknown(Problem::RootToRead));
    assert!(system.calls().is_empty(), "{:?}", system.calls());
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
            Step::ConfOption { file: "/etc/pacman.conf", section: Some("options"), key: "Color", value: "on".into() },
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

    assert_eq!(failure.step, Some(1));
    assert_eq!(failure.reason, Problem::System("mirror unreachable".into()));
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

    two_steps().revert(&mut system, &mut journal, &folder()).expect("both steps come back");

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

    tweak.revert(&mut system, &mut journal, &folder()).expect("both steps come back");

    let undone: Vec<&String> =
        system.calls().iter().filter(|call| call.contains("disable") || call.contains("-Rns")).collect();
    assert_eq!(undone, ["sudo systemctl disable --now reflector.timer", "sudo pacman -Rns --noconfirm reflector"]);
}

// A tweak that changes a file and then builds something out of it — the microcode item and
// its initramfs are the case this order exists for: `mkinitcpio -P` has to read
// `mkinitcpio.conf` as it was, not as qtools left it. The command here fails, which is what
// makes the order visible at all: the file is already back when the failure is reported, and
// the journal is kept so the revert can be tried again.
#[test]
fn a_revert_puts_every_file_back_before_it_runs_any_undo_command() {
    let mut system = FakeSystem::new();
    system.file("/etc/mkinitcpio.conf", "HOOKS=(base)\n");
    let tweak = Tweak {
        steps: vec![
            Step::ConfOption { file: "/etc/mkinitcpio.conf", section: None, key: "Color", value: "on".into() },
            Step::CommandRun {
                run: Cmd::new("mkinitcpio", ["-P"]).root(),
                check: Cmd::new("sh", ["-c", "test -e /boot/initramfs-linux.img"]),
                undo: Cmd::new("mkinitcpio", ["-P"]).root(),
                label: "mkinitcpio -P",
            },
        ],
        ..two_steps()
    };
    system.answer("sh -c test -e /boot/initramfs-linux.img", crate::system::Output { code: 1, ..ok() });
    system.answer("sudo mkinitcpio -P", ok());
    let mut journal = crate::state::Journal::default();
    tweak.apply(&mut system, &mut journal, &folder()).expect("both steps run");

    // The rebuild goes wrong the second time round, which is what makes the order visible:
    system.answer(
        "sudo mkinitcpio -P",
        Output { code: 1, stdout: String::new(), stderr: "no preset for the running kernel".into() },
    );
    let error = tweak.revert(&mut system, &mut journal, &folder()).expect_err("the rebuild fails");

    assert_eq!(error, Problem::System("no preset for the running kernel".into()));
    let text = system.read(std::path::Path::new("/etc/mkinitcpio.conf")).unwrap().unwrap();
    assert_eq!(text, "HOOKS=(base)\n", "the file is back before the command that reads it is run");
    let left: Vec<usize> = journal.entries("pacman-options").iter().map(|entry| entry.step).collect();
    assert_eq!(left, [1], "the file is off the record, the command that failed is still on it");
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
    tweak.revert(&mut system, &mut journal, &folder()).expect("the recorded step comes back");

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
        steps: vec![Step::ConfOption {
            file: "/etc/pacman.conf",
            section: Some("options"),
            key: "ParallelDownloads",
            value: "5".into(),
        }],
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
    assert!(entry.hash.is_some());

    tweak.revert(&mut system, &mut journal, &folder()).expect("the file comes back");

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

    tweak.revert(&mut system, &mut journal, &folder()).expect("the file is removed again");

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
    tweak.revert(&mut system, &mut journal, &folder()).expect("the step comes back");

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
        steps: vec![Step::ConfOption {
            file: "/etc/pacman.conf",
            section: Some("options"),
            key: "Color",
            value: String::new(),
        }],
        ..two_steps()
    };
    let mut journal = crate::state::Journal::default();
    tweak.apply(&mut system, &mut journal, &folder()).expect("the step runs");

    // Something else changes the file after qtools wrote it.
    system.file("/etc/pacman.conf", "[options]\nColor\nSomethingElse = true\n");

    let error = tweak.revert(&mut system, &mut journal, &folder()).expect_err("a changed file stops the revert");

    assert_eq!(error, Problem::ChangedAfterWrite { file: PathBuf::from("/etc/pacman.conf") });
    let text = system.read(std::path::Path::new("/etc/pacman.conf")).unwrap().unwrap();
    assert_eq!(text, "[options]\nColor\nSomethingElse = true\n", "the file is left untouched");
    assert_eq!(journal.entries("pacman-options").len(), 1, "the journal still holds the entry");
}

// A corrupted `applied.toml` must never panic, and a record that does not match the catalog's
// step is not acted on: the tweak keeps telling the truth about still being applied.
#[test]
fn a_record_that_does_not_match_its_step_is_refused_and_names_the_step() {
    let mut system = FakeSystem::new();
    system.file(
        folder().join("applied.toml"),
        "[[pacman-options]]\nstep = 1\nundo = [\"sudo\", \"rm\", \"-rf\", \"/\"]\n",
    );
    let mut journal = crate::state::Journal::load(&system, &folder()).expect("the journal reads");

    let error = two_steps().revert(&mut system, &mut journal, &folder()).expect_err("the record is refused");

    assert_eq!(error, Problem::RecordDoesNotMatch { step: 2 });
    assert!(system.calls().is_empty(), "no command from the journal is ever run: {:?}", system.calls());
    assert_eq!(journal.entries("pacman-options").len(), 1, "the tweak still says it is applied");
}

// A journal that names a file the catalog's step never writes is not trusted with a root write.
#[test]
fn a_record_that_names_another_file_is_not_written_to() {
    let mut system = FakeSystem::new();
    system.file("/home/test/evil", "root::0:0::/root:/bin/sh\n");
    system.file(
        folder().join("applied.toml"),
        "[[pacman-options]]\nstep = 0\ntarget = \"/etc/shadow\"\nbackup = \"/home/test/evil\"\n",
    );
    let mut journal = crate::state::Journal::load(&system, &folder()).expect("the journal reads");

    let error = two_steps().revert(&mut system, &mut journal, &folder()).expect_err("the record is refused");

    assert_eq!(error, Problem::RecordDoesNotMatch { step: 1 });
    assert_eq!(system.read(std::path::Path::new("/etc/shadow")).unwrap(), None);
}

// An undo that stops halfway leaves on record exactly what is still on the machine, so asking
// again does the rest. It used to keep every step on record, and the file step, already put
// back, then no longer matched its digest and refused the undo for good.
#[test]
fn an_undo_that_stops_halfway_can_be_asked_again_and_does_the_rest() {
    let mut system = FakeSystem::new();
    system.file("/etc/pacman.conf", "[options]\n");
    let tweak = Tweak {
        steps: vec![
            Step::PackageInstalled { package: "git" },
            Step::ConfOption { file: "/etc/pacman.conf", section: Some("options"), key: "Color", value: String::new() },
        ],
        ..two_steps()
    };
    system.answer("sudo pacman -S --needed --noconfirm git", ok());
    let mut journal = crate::state::Journal::default();
    tweak.apply(&mut system, &mut journal, &folder()).expect("both steps run");

    system.answer(
        "sudo pacman -Rns --noconfirm git",
        Output { code: 1, stdout: String::new(), stderr: "removing git breaks dependency".into() },
    );
    tweak.revert(&mut system, &mut journal, &folder()).expect_err("the removal fails");
    assert_eq!(system.read(std::path::Path::new("/etc/pacman.conf")).unwrap().as_deref(), Some("[options]\n"));

    // What is on disk is what a second run of `qtools --revert` reads.
    let mut journal = crate::state::Journal::load(&system, &folder()).expect("the journal reads back");
    system.answer("sudo pacman -Rns --noconfirm git", ok());
    tweak.revert(&mut system, &mut journal, &folder()).expect("the second undo does what was left");

    assert!(!journal.has("pacman-options"), "nothing is left on record");
    assert_eq!(system.read(std::path::Path::new("/etc/pacman.conf")).unwrap().as_deref(), Some("[options]\n"));
}

// Without the backup there is nothing to put back. Writing the file empty instead would leave
// pacman without its configuration; the undo stops before it touches anything.
#[test]
fn an_undo_whose_backup_is_gone_touches_nothing() {
    let mut system = FakeSystem::new();
    system.file("/etc/pacman.conf", "[options]\n#Color\n");
    let mut journal = crate::state::Journal::default();
    two_steps().apply(&mut system, &mut journal, &folder()).expect("both steps run");
    let written = system.read(std::path::Path::new("/etc/pacman.conf")).unwrap();
    let backup = journal.entries("pacman-options")[0].backup.clone().expect("the file was backed up");
    system.remove(&backup, false).expect("the backup goes");

    let error = two_steps().revert(&mut system, &mut journal, &folder()).expect_err("the undo stops");

    assert_eq!(error, Problem::BackupMissing { file: PathBuf::from("/etc/pacman.conf"), backup });
    assert_eq!(system.read(std::path::Path::new("/etc/pacman.conf")).unwrap(), written, "the file is untouched");
    assert_eq!(journal.entries("pacman-options").len(), 2, "both steps are still on record");
}

#[test]
fn undoing_a_tweak_qtools_did_not_do_says_so_and_runs_nothing() {
    let mut system = FakeSystem::new();
    let mut journal = crate::state::Journal::default();
    let tweak = Tweak { steps: vec![Step::ServiceEnabled { unit: "fstrim.timer" }], ..two_steps() };

    let error = tweak.revert(&mut system, &mut journal, &folder()).expect_err("nothing is on record");

    assert_eq!(error, Problem::NothingRecorded);
    assert!(system.calls().is_empty(), "{:?}", system.calls());
}

// Each step is on disk in the journal before the next one starts, so a run cut short — a
// closed terminal, `ctrl c` at the next password — leaves every finished step undoable.
#[test]
fn the_journal_is_on_disk_after_every_step() {
    let mut system = FakeSystem::new();
    system.file("/etc/pacman.conf", "[options]\n");
    let tweak = Tweak {
        steps: vec![
            Step::ConfOption { file: "/etc/pacman.conf", section: Some("options"), key: "Color", value: String::new() },
            Step::PackageInstalled { package: "reflector" },
        ],
        ..two_steps()
    };
    // The second step never comes back: the fake machine was not told how to answer it.
    let mut journal = crate::state::Journal::default();
    tweak.apply(&mut system, &mut journal, &folder()).expect_err("the install is not answered");

    let on_disk = crate::state::Journal::load(&system, &folder()).expect("the journal was written");
    assert_eq!(on_disk.entries("pacman-options").len(), 1, "the finished step is on disk");
}

// A change that cannot be recorded is not left on the machine without a record, where no undo
// could ever find it: it is put back, and the failure says so.
#[test]
fn a_change_that_cannot_be_recorded_is_put_back() {
    let mut system = FakeSystem::new();
    system.file("/etc/pacman.conf", "[options]\n");
    system.refuse_writes(folder().join("applied.toml"));
    let mut journal = crate::state::Journal::default();

    let failure = two_steps().apply(&mut system, &mut journal, &folder()).expect_err("the journal is not written");

    assert_eq!(failure.step, Some(0));
    assert!(matches!(failure.reason, Problem::NotRecorded { put_back: true, .. }), "{:?}", failure.reason);
    assert_eq!(system.read(std::path::Path::new("/etc/pacman.conf")).unwrap().as_deref(), Some("[options]\n"));
    assert!(!journal.has("pacman-options"));
}

// The backup is on disk before the file changes: when it cannot be kept, the step stops with the
// file as it was. The other way round, the old content would live only in memory for a moment,
// and a failure then would lose it.
#[test]
fn a_backup_that_cannot_be_kept_stops_the_step_before_the_file_changes() {
    let mut system = FakeSystem::new();
    system.file("/etc/pacman.conf", "[options]\n#ParallelDownloads = 5\n");
    system.refuse_writes(folder().join("backups"));
    let mut journal = crate::state::Journal::default();

    let failure = two_steps().apply(&mut system, &mut journal, &folder()).expect_err("no backup, no change");

    assert_eq!(failure.step, Some(0));
    assert_eq!(
        system.read(std::path::Path::new("/etc/pacman.conf")).unwrap().as_deref(),
        Some("[options]\n#ParallelDownloads = 5\n")
    );
    assert!(!journal.has("pacman-options"));
}
