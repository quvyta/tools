use super::*;
use crate::system::FakeSystem;

const PACMAN_CONF: &str = "\
[options]
HoldPkg = pacman glibc
#ParallelDownloads = 5
#Color
";

fn parallel_downloads() -> Step {
    Step::ConfOption { file: "/etc/pacman.conf", key: "ParallelDownloads", value: "5".into() }
}

#[test]
fn a_commented_out_option_counts_as_missing() {
    let mut system = FakeSystem::new();
    system.file("/etc/pacman.conf", PACMAN_CONF);

    assert_eq!(parallel_downloads().state(&mut system).expect("the file reads"), StepState::Missing);
}

#[test]
fn applying_uncomments_the_option_and_keeps_the_rest_of_the_file() {
    let mut system = FakeSystem::new();
    system.file("/etc/pacman.conf", PACMAN_CONF);

    parallel_downloads().apply(&mut system).expect("the file is written");

    let written = system.read(std::path::Path::new("/etc/pacman.conf")).unwrap().unwrap();
    assert!(written.contains("ParallelDownloads = 5"), "{written}");
    assert!(!written.contains("#ParallelDownloads"), "the commented line is gone:\n{written}");
    assert!(written.contains("HoldPkg = pacman glibc"), "the rest of the file stays:\n{written}");
    assert!(written.contains("#Color"), "an option we were not asked about stays as it was:\n{written}");
}

#[test]
fn a_second_run_says_it_is_already_done() {
    let mut system = FakeSystem::new();
    system.file("/etc/pacman.conf", PACMAN_CONF);

    parallel_downloads().apply(&mut system).expect("the file is written");

    assert_eq!(parallel_downloads().state(&mut system).expect("the file reads"), StepState::Done);
}

#[test]
fn another_value_is_a_conflict_and_is_not_overwritten_silently() {
    let mut system = FakeSystem::new();
    system.file("/etc/pacman.conf", "[options]\nParallelDownloads = 12\n");

    let state = parallel_downloads().state(&mut system).expect("the file reads");

    assert_eq!(state, StepState::Conflict("ParallelDownloads = 12".into()));
}

#[test]
fn a_missing_file_is_a_conflict_rather_than_a_crash() {
    let mut system = FakeSystem::new();

    let state = parallel_downloads().state(&mut system).expect("a missing file is not an error");

    assert!(matches!(state, StepState::Conflict(_)), "{state:?}");
}

#[test]
fn the_undo_carries_the_file_back_to_what_it_was() {
    let mut system = FakeSystem::new();
    system.file("/etc/pacman.conf", PACMAN_CONF);

    let undo = parallel_downloads().apply(&mut system).expect("the file is written");
    let (path, before) = undo.file.expect("a file change can be undone");

    assert_eq!(path, std::path::Path::new("/etc/pacman.conf"));
    assert_eq!(before.as_deref(), Some(PACMAN_CONF));
}

#[test]
fn the_description_names_the_file_it_touches() {
    assert_eq!(parallel_downloads().describe(), vec![Touch::File("/etc/pacman.conf".into())]);
}

fn ufw() -> Step {
    Step::PackageInstalled { package: "ufw" }
}

#[test]
fn an_installed_package_is_done() {
    let mut system = FakeSystem::new();
    system
        .answer("pacman -Q ufw", crate::system::Output { code: 0, stdout: "ufw 0.36-9".into(), stderr: String::new() });

    assert_eq!(ufw().state(&mut system).expect("pacman answers"), StepState::Done);
}

#[test]
fn a_package_that_is_not_there_is_missing() {
    let mut system = FakeSystem::new();
    system.answer(
        "pacman -Q ufw",
        crate::system::Output { code: 1, stdout: String::new(), stderr: "error: package 'ufw' was not found".into() },
    );

    assert_eq!(ufw().state(&mut system).expect("pacman answers"), StepState::Missing);
}

#[test]
fn applying_installs_the_package_and_the_undo_removes_it() {
    let mut system = FakeSystem::new();
    system.answer(
        "sudo pacman -S --needed --noconfirm ufw",
        crate::system::Output { code: 0, stdout: String::new(), stderr: String::new() },
    );

    let undo = ufw().apply(&mut system).expect("pacman runs");

    assert_eq!(system.calls(), ["sudo pacman -S --needed --noconfirm ufw"]);
    let command = undo.command.expect("an installed package can be removed again");
    assert_eq!(command.argv(), ["sudo", "pacman", "-Rns", "--noconfirm", "ufw"]);
}

#[test]
fn a_failed_install_is_an_error_not_a_silent_success() {
    let mut system = FakeSystem::new();
    system.answer(
        "sudo pacman -S --needed --noconfirm ufw",
        crate::system::Output { code: 1, stdout: String::new(), stderr: "no space left".into() },
    );

    let error = ufw().apply(&mut system).expect_err("a failed install must be an error");

    assert!(error.to_string().contains("no space left"), "the reason travels with the error: {error}");
}

#[test]
fn the_description_names_the_package() {
    assert_eq!(ufw().describe(), vec![Touch::Package("ufw".into())]);
}

fn reflector_timer() -> Step {
    Step::ServiceEnabled { unit: "reflector.timer" }
}

fn enabled(stdout: &str) -> crate::system::Output {
    crate::system::Output { code: 0, stdout: stdout.into(), stderr: String::new() }
}

#[test]
fn a_unit_that_is_enabled_and_running_is_done() {
    let mut system = FakeSystem::new();
    system.answer("systemctl is-enabled reflector.timer", enabled("enabled"));
    system.answer("systemctl is-active reflector.timer", enabled("active"));

    assert_eq!(reflector_timer().state(&mut system).expect("systemctl answers"), StepState::Done);
}

#[test]
fn a_unit_that_is_enabled_but_not_running_is_missing() {
    let mut system = FakeSystem::new();
    system.answer("systemctl is-enabled reflector.timer", enabled("enabled"));
    system.answer(
        "systemctl is-active reflector.timer",
        crate::system::Output { code: 3, stdout: "inactive".into(), stderr: String::new() },
    );

    assert_eq!(reflector_timer().state(&mut system).expect("systemctl answers"), StepState::Missing);
}

#[test]
fn applying_enables_and_starts_in_one_call_and_the_undo_reverses_both() {
    let mut system = FakeSystem::new();
    system.answer("sudo systemctl enable --now reflector.timer", enabled(""));

    let undo = reflector_timer().apply(&mut system).expect("systemctl runs");

    assert_eq!(system.calls(), ["sudo systemctl enable --now reflector.timer"]);
    assert_eq!(
        undo.command.expect("a service can be turned off again").argv(),
        ["sudo", "systemctl", "disable", "--now", "reflector.timer"]
    );
}

#[test]
fn the_description_names_the_service() {
    assert_eq!(reflector_timer().describe(), vec![Touch::Service("reflector.timer".into())]);
}

fn modeset() -> Step {
    Step::FileManaged { path: "/etc/modprobe.d/qtools-nvidia.conf", content: "options nvidia_drm modeset=1\n".into() }
}

#[test]
fn a_managed_file_with_the_same_content_is_done_and_a_different_one_is_a_conflict() {
    let mut system = FakeSystem::new();
    assert_eq!(modeset().state(&mut system).expect("a missing file reads"), StepState::Missing);

    modeset().apply(&mut system).expect("the file is written");
    assert_eq!(modeset().state(&mut system).expect("the file reads"), StepState::Done);

    system.file("/etc/modprobe.d/qtools-nvidia.conf", "options nvidia_drm modeset=0\n");
    assert!(matches!(modeset().state(&mut system).expect("the file reads"), StepState::Conflict(_)));
}

#[test]
fn undoing_a_file_that_did_not_exist_removes_it_again() {
    let mut system = FakeSystem::new();

    let undo = modeset().apply(&mut system).expect("the file is written");
    let (path, before) = undo.file.expect("a written file can be undone");

    assert_eq!(path, std::path::Path::new("/etc/modprobe.d/qtools-nvidia.conf"));
    assert_eq!(before, None, "the file was not there before, so undoing removes it");
}

fn multilib_mirrors() -> Step {
    Step::CommandRun {
        run: crate::system::Cmd::new("reflector", ["--save", "/etc/pacman.d/mirrorlist"]).root(),
        check: crate::system::Cmd::new("test", ["-s", "/etc/pacman.d/mirrorlist"]),
        undo: crate::system::Cmd::new("cp", ["/etc/pacman.d/mirrorlist.qtools", "/etc/pacman.d/mirrorlist"]).root(),
        label: "reflector",
    }
}

#[test]
fn a_command_step_is_done_when_its_check_succeeds() {
    let mut system = FakeSystem::new();
    system.answer("test -s /etc/pacman.d/mirrorlist", enabled(""));

    assert_eq!(multilib_mirrors().state(&mut system).expect("the check runs"), StepState::Done);
}

#[test]
fn a_command_step_hands_back_its_undo_command() {
    let mut system = FakeSystem::new();
    system.answer("sudo reflector --save /etc/pacman.d/mirrorlist", enabled(""));

    let undo = multilib_mirrors().apply(&mut system).expect("the command runs");

    assert_eq!(
        undo.command.expect("a command step carries its undo").argv(),
        ["sudo", "cp", "/etc/pacman.d/mirrorlist.qtools", "/etc/pacman.d/mirrorlist"]
    );
}

fn mirrorlist_rewrite() -> Step {
    Step::FileRewritten {
        path: "/etc/pacman.d/mirrorlist",
        run: crate::system::Cmd::new("reflector", ["--save", "/etc/pacman.d/mirrorlist"]).root(),
        check: crate::system::Cmd::new("sh", ["-c", "grep -q 'generated by Reflector' /etc/pacman.d/mirrorlist"]),
        label: "reflector",
    }
}

#[test]
fn a_rewritten_file_is_done_when_its_check_succeeds() {
    let mut system = FakeSystem::new();
    system.answer("sh -c grep -q 'generated by Reflector' /etc/pacman.d/mirrorlist", enabled(""));

    assert_eq!(mirrorlist_rewrite().state(&mut system).expect("the check runs"), StepState::Done);
}

#[test]
fn a_rewritten_file_is_missing_when_its_check_fails() {
    let mut system = FakeSystem::new();
    system.answer(
        "sh -c grep -q 'generated by Reflector' /etc/pacman.d/mirrorlist",
        crate::system::Output { code: 1, stdout: String::new(), stderr: String::new() },
    );

    assert_eq!(mirrorlist_rewrite().state(&mut system).expect("the check runs"), StepState::Missing);
}

#[test]
fn applying_captures_the_prior_content_before_running_the_command() {
    let mut system = FakeSystem::new();
    system.file("/etc/pacman.d/mirrorlist", "# old mirrors\n");
    system.answer("sudo reflector --save /etc/pacman.d/mirrorlist", enabled(""));

    let undo = mirrorlist_rewrite().apply(&mut system).expect("the command runs");

    let (path, before) = undo.file.expect("the rewritten file can be undone");
    assert_eq!(path, std::path::Path::new("/etc/pacman.d/mirrorlist"));
    assert_eq!(before.as_deref(), Some("# old mirrors\n"));
}

#[test]
fn applying_captures_no_prior_content_when_the_file_did_not_exist() {
    let mut system = FakeSystem::new();
    system.answer("sudo reflector --save /etc/pacman.d/mirrorlist", enabled(""));

    let undo = mirrorlist_rewrite().apply(&mut system).expect("the command runs");

    let (_, before) = undo.file.expect("the rewritten file can be undone");
    assert_eq!(before, None, "the file was not there before the command ran");
}

#[test]
fn a_failing_rewrite_is_an_error_and_nothing_is_recorded() {
    let mut system = FakeSystem::new();
    system.answer(
        "sudo reflector --save /etc/pacman.d/mirrorlist",
        crate::system::Output { code: 1, stdout: String::new(), stderr: "no mirrors responded".into() },
    );

    let error = mirrorlist_rewrite().apply(&mut system).expect_err("a failing command must be an error");

    assert!(error.to_string().contains("no mirrors responded"), "the reason travels with the error: {error}");
}

#[test]
fn the_description_names_both_the_file_and_the_command() {
    assert_eq!(
        mirrorlist_rewrite().describe(),
        vec![Touch::File("/etc/pacman.d/mirrorlist".into()), Touch::Command("reflector".into())]
    );
}
