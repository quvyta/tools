use std::path::Path;

use super::*;

#[test]
fn a_command_that_needs_root_is_run_through_sudo() {
    let plain = Cmd::new("pacman", ["-Q", "ufw"]);
    assert_eq!(plain.argv(), ["pacman", "-Q", "ufw"]);

    let elevated = Cmd::new("pacman", ["-S", "--needed", "ufw"]).root();
    assert_eq!(elevated.argv(), ["sudo", "pacman", "-S", "--needed", "ufw"]);
}

#[test]
fn the_fake_system_answers_what_it_was_told_and_records_what_it_was_asked() {
    let mut system = FakeSystem::new();
    system.answer("pacman -Q ufw", Output { code: 0, stdout: "ufw 0.36".into(), stderr: String::new() });

    let output = system.run(&Cmd::new("pacman", ["-Q", "ufw"])).expect("the fake system answers");

    assert!(output.ok());
    assert_eq!(output.stdout, "ufw 0.36");
    assert_eq!(system.calls(), ["pacman -Q ufw"]);
}

#[test]
fn an_unanswered_command_fails_instead_of_pretending_to_succeed() {
    let mut system = FakeSystem::new();
    let output = system.run(&Cmd::new("pacman", ["-Q", "ufw"])).expect("an answer is still returned");
    assert!(!output.ok(), "an unanswered command must not look like success");
}

#[test]
fn files_written_to_the_fake_system_can_be_read_back() {
    let mut system = FakeSystem::new();
    let path = Path::new("/etc/pacman.conf");

    assert_eq!(system.read(path).expect("reading is allowed"), None);
    system.write(path, "ParallelDownloads = 5\n", true).expect("writing is allowed");
    assert_eq!(system.read(path).expect("reading is allowed").as_deref(), Some("ParallelDownloads = 5\n"));

    system.remove(path, true).expect("removing is allowed");
    assert_eq!(system.read(path).expect("reading is allowed"), None);
}
