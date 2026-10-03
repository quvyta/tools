use super::*;
use crate::system::FakeSystem;

/// Does a step the way a tweak does it: what it is about to change is read first, then it runs.
/// Answers what was read, which is what the backup keeps.
fn apply(step: &Step, system: &mut FakeSystem) -> Result<Before, Problem> {
    let before = step.before(system)?;
    step.apply(system, &before)?;
    Ok(before)
}

const PACMAN_CONF: &str = "\
[options]
HoldPkg = pacman glibc
#ParallelDownloads = 5
#Color
";

fn parallel_downloads() -> Step {
    Step::ConfOption { file: "/etc/pacman.conf", section: Some("options"), key: "ParallelDownloads", value: "5".into() }
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

    apply(&parallel_downloads(), &mut system).expect("the file is written");

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

    apply(&parallel_downloads(), &mut system).expect("the file is written");

    assert_eq!(parallel_downloads().state(&mut system).expect("the file reads"), StepState::Done);
}

#[test]
fn another_value_is_a_conflict_and_is_not_overwritten_silently() {
    let mut system = FakeSystem::new();
    system.file("/etc/pacman.conf", "[options]\nParallelDownloads = 12\n");

    let state = parallel_downloads().state(&mut system).expect("the file reads");

    assert_eq!(
        state,
        StepState::Conflict(Problem::OptionDiffers {
            file: PathBuf::from("/etc/pacman.conf"),
            key: "ParallelDownloads".into(),
            found: "12".into()
        })
    );
}

#[test]
fn a_missing_file_is_a_conflict_rather_than_a_crash() {
    let mut system = FakeSystem::new();

    let state = parallel_downloads().state(&mut system).expect("a missing file is not an error");

    assert_eq!(state, StepState::Conflict(Problem::FileMissing { file: PathBuf::from("/etc/pacman.conf") }));
}

#[test]
fn the_undo_carries_the_file_back_to_what_it_was() {
    let mut system = FakeSystem::new();
    system.file("/etc/pacman.conf", PACMAN_CONF);

    let undo = apply(&parallel_downloads(), &mut system).expect("the file is written");
    let (path, before) = undo.file.expect("a file change can be undone");
    assert!(parallel_downloads().undo_command().is_none(), "the backup is the undo");

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

    let before = apply(&ufw(), &mut system).expect("pacman runs");

    assert_eq!(system.calls(), ["sudo pacman -S --needed --noconfirm ufw"]);
    assert!(before.file.is_none(), "installing a package writes no file of qtools'");
    let command = ufw().undo_command().expect("an installed package can be removed again");
    assert_eq!(command.argv(), ["sudo", "pacman", "-Rns", "--noconfirm", "ufw"]);
}

#[test]
fn a_failed_install_is_an_error_not_a_silent_success() {
    let mut system = FakeSystem::new();
    system.answer(
        "sudo pacman -S --needed --noconfirm ufw",
        crate::system::Output { code: 1, stdout: String::new(), stderr: "no space left".into() },
    );

    let error = apply(&ufw(), &mut system).expect_err("a failed install must be an error");

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

    apply(&reflector_timer(), &mut system).expect("systemctl runs");

    assert_eq!(system.calls(), ["sudo systemctl enable --now reflector.timer"]);
    assert_eq!(
        reflector_timer().undo_command().expect("a service can be turned off again").argv(),
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

    apply(&modeset(), &mut system).expect("the file is written");
    assert_eq!(modeset().state(&mut system).expect("the file reads"), StepState::Done);

    system.file("/etc/modprobe.d/qtools-nvidia.conf", "options nvidia_drm modeset=0\n");
    assert!(matches!(modeset().state(&mut system).expect("the file reads"), StepState::Conflict(_)));
}

#[test]
fn undoing_a_file_that_did_not_exist_removes_it_again() {
    let mut system = FakeSystem::new();

    let undo = apply(&modeset(), &mut system).expect("the file is written");
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

    apply(&multilib_mirrors(), &mut system).expect("the command runs");

    assert_eq!(
        multilib_mirrors().undo_command().expect("a command step carries its undo").argv(),
        ["sudo", "cp", "/etc/pacman.d/mirrorlist.qtools", "/etc/pacman.d/mirrorlist"]
    );
}

fn mirrorlist_rewrite() -> Step {
    Step::FileRewritten {
        path: "/etc/pacman.d/mirrorlist",
        run: crate::system::Cmd::new("reflector", ["--save", "/etc/pacman.d/mirrorlist"]).root(),
        check: crate::system::Cmd::new("sh", ["-c", "grep -q 'generated by Reflector' /etc/pacman.d/mirrorlist"]),
        label: "reflector",
        renewed: true,
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

    let undo = apply(&mirrorlist_rewrite(), &mut system).expect("the command runs");

    let (path, before) = undo.file.expect("the rewritten file can be undone");
    assert_eq!(path, std::path::Path::new("/etc/pacman.d/mirrorlist"));
    assert_eq!(before.as_deref(), Some("# old mirrors\n"));
}

#[test]
fn applying_captures_no_prior_content_when_the_file_did_not_exist() {
    let mut system = FakeSystem::new();
    system.answer("sudo reflector --save /etc/pacman.d/mirrorlist", enabled(""));

    let undo = apply(&mirrorlist_rewrite(), &mut system).expect("the command runs");

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

    let error = apply(&mirrorlist_rewrite(), &mut system).expect_err("a failing command must be an error");

    assert!(error.to_string().contains("no mirrors responded"), "the reason travels with the error: {error}");
}

#[test]
fn the_description_names_both_the_file_and_the_command() {
    assert_eq!(
        mirrorlist_rewrite().describe(),
        vec![Touch::File("/etc/pacman.d/mirrorlist".into()), Touch::Command("reflector".into())]
    );
}

/// The `sh` line the step asks which file a list is really read from, spelled out as it is
/// run, so the tests below are about that command and not about a copy of it.
const FIND_HOOKS: &str = "sh -c for f in /etc/mkinitcpio.conf $(ls -1 /etc/mkinitcpio.conf.d/*.conf 2>/dev/null | LC_ALL=C sort -V); do [ -r \"$f\" ] && grep -q '^[[:space:]]*HOOKS=' \"$f\" && echo \"$f\"; done | tail -n 1";

const HOOKS_CONF: &str = "/etc/mkinitcpio.conf";

/// A step that puts `microcode` in mkinitcpio's `HOOKS` the way the catalog does.
fn hooks() -> Step {
    Step::ListItemAdded {
        file: HOOKS_CONF,
        drop_ins: "/etc/mkinitcpio.conf.d",
        key: "HOOKS",
        item: "microcode",
        after: &["autodetect", "udev", "systemd"],
    }
}

/// A machine whose `HOOKS` is in `text`, and the file that holds it.
fn with_hooks_at(path: &str, text: &str) -> FakeSystem {
    let mut system = FakeSystem::new();
    system.answer(FIND_HOOKS, crate::system::Output { code: 0, stdout: path.into(), stderr: String::new() });
    system.file(path, text);
    system
}

#[test]
fn a_list_without_the_item_is_missing_and_with_it_is_done() {
    let mut system = with_hooks_at(HOOKS_CONF, "HOOKS=(base udev autodetect modconf)\n");
    assert_eq!(hooks().state(&mut system).expect("the machine answers"), StepState::Missing);

    system = with_hooks_at(HOOKS_CONF, "HOOKS=(base udev autodetect microcode modconf)\n");
    assert_eq!(hooks().state(&mut system).expect("the machine answers"), StepState::Done);
}

#[test]
fn the_file_the_machine_points_at_is_the_one_that_is_read() {
    let mut system = with_hooks_at("/etc/mkinitcpio.conf.d/20-hooks.conf", "HOOKS=(base autodetect)\n");
    system.file(HOOKS_CONF, "HOOKS=(base udev autodetect)\n");

    apply(&hooks(), &mut system).expect("the item is added");

    let drop_in = system.read(std::path::Path::new("/etc/mkinitcpio.conf.d/20-hooks.conf")).unwrap().unwrap();
    assert_eq!(drop_in, "HOOKS=(base autodetect microcode)\n", "the drop-in is the file mkinitcpio reads");
    let main = system.read(std::path::Path::new(HOOKS_CONF)).unwrap().unwrap();
    assert_eq!(main, "HOOKS=(base udev autodetect)\n", "the main file is left exactly as it was");
}

#[test]
fn a_list_qtools_cannot_understand_is_a_conflict_with_the_text_it_found() {
    let mut system = with_hooks_at(HOOKS_CONF, "HOOKS=base udev autodetect\n");

    let state = hooks().state(&mut system).expect("the machine answers");

    assert_eq!(
        state,
        StepState::Conflict(Problem::ListNotUnderstood {
            file: PathBuf::from(HOOKS_CONF),
            found: Some("HOOKS=base udev autodetect".into())
        })
    );
}

#[test]
fn a_file_nobody_sets_the_list_in_says_so_rather_than_writing_into_an_empty_one() {
    let mut system = FakeSystem::new();
    system.answer(FIND_HOOKS, crate::system::Output { code: 1, stdout: String::new(), stderr: String::new() });

    let state = hooks().state(&mut system).expect("the machine answers");

    assert_eq!(
        state,
        StepState::Conflict(Problem::ListNotSet {
            key: "HOOKS".into(),
            file: PathBuf::from("/etc/mkinitcpio.conf"),
            drop_ins: PathBuf::from("/etc/mkinitcpio.conf.d")
        })
    );
    assert!(apply(&hooks(), &mut system).is_err(), "and applying refuses");
}

#[test]
fn a_file_the_machine_named_but_cannot_read_is_a_conflict() {
    let mut system = FakeSystem::new();
    system.answer(
        FIND_HOOKS,
        crate::system::Output { code: 0, stdout: "/etc/mkinitcpio.conf.d/9-gone.conf\n".into(), stderr: String::new() },
    );

    let state = hooks().state(&mut system).expect("the machine answers");

    assert_eq!(
        state,
        StepState::Conflict(Problem::ListUnreadable {
            key: "HOOKS".into(),
            file: PathBuf::from("/etc/mkinitcpio.conf.d/9-gone.conf")
        })
    );
}

#[test]
fn the_undo_of_the_step_carries_the_whole_file_back() {
    let mut system = with_hooks_at(HOOKS_CONF, "HOOKS=(base autodetect modconf)\n");

    let undo = apply(&hooks(), &mut system).expect("the item is added");

    let (path, before) = undo.file.expect("the list can be undone");
    assert_eq!(path, std::path::Path::new(HOOKS_CONF));
    assert_eq!(before.as_deref(), Some("HOOKS=(base autodetect modconf)\n"));
    assert!(hooks().undo_command().is_none(), "a file change needs no command to undo it: the backup is the undo");
}

#[test]
fn the_description_names_both_places_the_list_may_be_written() {
    assert_eq!(
        hooks().describe(),
        vec![Touch::File("/etc/mkinitcpio.conf".into()), Touch::File("/etc/mkinitcpio.conf.d".into())]
    );
}

fn nvidia_suspend() -> Step {
    Step::UnitEnabled { unit: "nvidia-suspend.service" }
}

// `nvidia-suspend.service` is a one-shot unit whose work is `nvidia-sleep.sh suspend`: started
// now, it would suspend the driver under the running session. Enabling is all it needs, and
// once it has run it is inactive again, which is how a real machine answers.
#[test]
fn a_unit_for_an_occasion_is_enabled_without_being_started() {
    let mut system = FakeSystem::new();
    system.answer("sudo systemctl enable nvidia-suspend.service", enabled(""));

    apply(&nvidia_suspend(), &mut system).expect("systemctl runs");

    assert_eq!(system.calls(), ["sudo systemctl enable nvidia-suspend.service"]);
    assert_eq!(
        nvidia_suspend().undo_command().expect("a unit can be disabled again").argv(),
        ["sudo", "systemctl", "disable", "nvidia-suspend.service"]
    );
}

#[test]
fn an_enabled_one_shot_unit_that_is_inactive_is_done() {
    let mut system = FakeSystem::new();
    system.answer("systemctl is-enabled nvidia-suspend.service", enabled("enabled"));
    system.answer(
        "systemctl is-active nvidia-suspend.service",
        crate::system::Output { code: 3, stdout: "inactive".into(), stderr: String::new() },
    );

    assert_eq!(nvidia_suspend().state(&mut system).expect("systemctl answers"), StepState::Done);
}

#[test]
fn a_disabled_unit_for_an_occasion_is_missing() {
    let mut system = FakeSystem::new();
    system.answer(
        "systemctl is-enabled nvidia-suspend.service",
        crate::system::Output { code: 1, stdout: "disabled".into(), stderr: String::new() },
    );

    assert_eq!(nvidia_suspend().state(&mut system).expect("systemctl answers"), StepState::Missing);
    assert_eq!(nvidia_suspend().describe(), vec![Touch::Unit("nvidia-suspend.service".into())]);
}

fn reflector_conf() -> Step {
    Step::FileReplaced { path: "/etc/xdg/reflector/reflector.conf", content: "--country TR\n".into() }
}

#[test]
fn a_replaced_file_with_the_package_default_is_not_done_yet_rather_than_changed() {
    let mut system = FakeSystem::new();
    system.file("/etc/xdg/reflector/reflector.conf", "--latest 5\n--sort age\n");

    assert_eq!(reflector_conf().state(&mut system).expect("the file reads"), StepState::Missing);

    let before = apply(&reflector_conf(), &mut system).expect("the file is written");

    assert_eq!(reflector_conf().state(&mut system).expect("the file reads"), StepState::Done);
    let (_, kept) = before.file.expect("the default is read before it is replaced");
    assert_eq!(kept.as_deref(), Some("--latest 5\n--sort age\n"), "the package's default is what comes back");
}

#[test]
fn a_managed_file_that_differs_is_shown_by_its_first_different_line_not_its_whole_content() {
    let mut system = FakeSystem::new();
    system.file("/etc/modprobe.d/qtools-nvidia.conf", "# mine\noptions nvidia_drm modeset=0\n");

    let state = modeset().state(&mut system).expect("the file reads");

    assert_eq!(
        state,
        StepState::Conflict(Problem::FileDiffers {
            file: PathBuf::from("/etc/modprobe.d/qtools-nvidia.conf"),
            line: 1,
            found: Some("# mine".into())
        })
    );
}

#[test]
fn an_option_whose_section_is_not_in_the_file_is_a_conflict_and_is_not_written() {
    let mut system = FakeSystem::new();
    system.file("/etc/pacman.conf", "[core]\nInclude = /etc/pacman.d/mirrorlist\n");

    let state = parallel_downloads().state(&mut system).expect("the file reads");

    let missing = Problem::SectionMissing { file: PathBuf::from("/etc/pacman.conf"), section: "options".into() };
    assert_eq!(state, StepState::Conflict(missing.clone()));
    assert_eq!(apply(&parallel_downloads(), &mut system).expect_err("nothing is guessed"), missing);
    assert_eq!(
        system.read(std::path::Path::new("/etc/pacman.conf")).unwrap().as_deref(),
        Some("[core]\nInclude = /etc/pacman.d/mirrorlist\n")
    );
}

#[test]
fn a_step_may_write_only_the_files_it_names() {
    assert!(parallel_downloads().may_write(std::path::Path::new("/etc/pacman.conf")));
    assert!(!parallel_downloads().may_write(std::path::Path::new("/etc/shadow")));
    assert!(hooks().may_write(std::path::Path::new("/etc/mkinitcpio.conf.d/50-site.conf")));
    assert!(!hooks().may_write(std::path::Path::new("/etc/mkinitcpio.conf.d/../shadow")));
    assert!(!ufw().may_write(std::path::Path::new("/etc/pacman.conf")));
}
