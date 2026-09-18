use std::path::{Path, PathBuf};

use super::*;
use crate::step::Step;
use crate::system::FakeSystem;

fn folder() -> PathBuf {
    PathBuf::from("/home/test/.local/state/quvyta-tools")
}

#[test]
fn recording_a_file_change_copies_the_old_content_into_the_backup_folder() {
    let mut system = FakeSystem::new();
    system.file("/etc/pacman.conf", "[options]\n#ParallelDownloads = 5\n");
    let step = Step::ConfOption { file: "/etc/pacman.conf", key: "ParallelDownloads", value: "5".into() };
    let undo = step.apply(&mut system).expect("the file is written");

    let mut journal = Journal::default();
    journal.record("pacman-options", 0, &undo, &mut system, &folder()).expect("the backup is written");

    let entry = journal.entries("pacman-options")[0];
    let backup = entry.backup.clone().expect("a file change is backed up");
    let saved = system.read(&backup).expect("the backup reads").expect("the backup exists");
    assert_eq!(saved, "[options]\n#ParallelDownloads = 5\n");
}

#[test]
fn the_journal_survives_a_round_trip_through_the_file() {
    let mut system = FakeSystem::new();
    system.file("/etc/pacman.conf", "[options]\n");
    let step = Step::ConfOption { file: "/etc/pacman.conf", key: "Color", value: String::new() };
    let undo = step.apply(&mut system).expect("the file is written");

    let mut journal = Journal::default();
    journal.record("pacman-options", 1, &undo, &mut system, &folder()).expect("the backup is written");
    journal.save(&mut system, &folder()).expect("the journal is written");

    let read_back = Journal::load(&system, &folder()).expect("the journal reads");
    let entry = read_back.entries("pacman-options")[0];
    assert_eq!(entry.step, 1);
    assert_eq!(entry.tweak, "pacman-options");
}

#[test]
fn a_broken_journal_file_does_not_panic_and_reports_where_it_broke() {
    let mut system = FakeSystem::new();
    system.file(folder().join("applied.toml"), "this is not toml at all [[[");

    let error = Journal::load(&system, &folder()).expect_err("a broken file is a diagnostic, not a panic");

    assert!(error.to_string().contains("applied.toml"), "the message names the file: {error}");
}

#[test]
fn a_journal_that_is_not_there_yet_is_simply_empty() {
    let system = FakeSystem::new();
    let journal = Journal::load(&system, &folder()).expect("a missing journal is not an error");
    assert!(journal.entries("pacman-options").is_empty());
}

#[test]
fn the_digest_changes_when_the_content_changes() {
    assert_eq!(digest("same"), digest("same"));
    assert_ne!(digest("same"), digest("different"));
}

#[test]
fn folder_prefers_xdg_state_home_when_it_is_set() {
    let path = folder_from(Some("/custom/state".to_owned()), Some("/home/someone".to_owned()));
    assert_eq!(path, Path::new("/custom/state/quvyta-tools"));
}

#[test]
fn folder_falls_back_to_home_local_state_when_xdg_state_home_is_unset() {
    let path = folder_from(None, Some("/home/someone".to_owned()));
    assert_eq!(path, Path::new("/home/someone/.local/state/quvyta-tools"));
}

#[test]
fn folder_falls_back_to_the_current_directory_when_neither_variable_is_set() {
    let path = folder_from(None, None);
    assert!(path.ends_with("quvyta-tools"));
    assert_eq!(path, Path::new("quvyta-tools"));
}
