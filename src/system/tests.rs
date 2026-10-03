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

/// A folder of the test's own under the system's temporary folder, taken away at the end.
struct Scratch(std::path::PathBuf);

impl Scratch {
    fn new(name: &str) -> Self {
        let path = std::env::temp_dir().join(format!("qtools-system-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).expect("the scratch folder is made");
        Self(path)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

// `/etc/systemd/journald.conf.d` and `/etc/environment.d` belong to no package, so a clean Arch
// has neither, and the file a tweak writes there is the first thing in its folder. The write
// that needs root makes the folder itself, the way the write that does not always did.
#[test]
fn a_root_write_into_a_folder_that_is_not_there_yet_makes_the_folder() {
    let scratch = Scratch::new("root-folder");
    let path = scratch.0.join("journald.conf.d/50-quvyta-tools.conf");
    let mut system = RealSystem::elevated_by(&[]);

    system.write(&path, "[Journal]\nSystemMaxUse=500M\n", true).expect("the folder is made and the file written");

    assert_eq!(std::fs::read_to_string(&path).expect("the file is there"), "[Journal]\nSystemMaxUse=500M\n");
}

#[test]
fn a_root_write_over_a_file_keeps_its_folder_and_replaces_the_content() {
    let scratch = Scratch::new("root-over");
    let path = scratch.0.join("pacman.conf");
    std::fs::write(&path, "[options]\nColor\nVerbosePkgLists\n").expect("the file is set up");
    let mut system = RealSystem::elevated_by(&[]);

    system.write(&path, "[options]\n", true).expect("the file is written");

    assert_eq!(std::fs::read_to_string(&path).expect("the file is there"), "[options]\n");
}
