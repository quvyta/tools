use qframe::event::MouseKind;
use qframe::icons::GlyphMode;
use qframe::prelude::*;

use super::*;
use crate::catalog::hardware::Cpu;
use crate::choices::Choices;

/// The screen's own harness, the one every screen test opens with, at `width` by `height`.
pub(super) fn harness(width: u16, height: u16) -> Harness<Tools> {
    let mut h = Harness::with_env(Tools::default(), crate::locales::env(), width, height);
    h.set_locale("en").set_glyph_mode(GlyphMode::Unicode);
    h
}

/// A screen of nothing applied yet, on the machine whose processor is `cpu`: the tests that
/// care which package an item names give the processor they mean rather than the default one,
/// which has no microcode package and so shows none.
pub(super) fn off(cpu: Cpu) -> Tools {
    Tools::new(vec![TweakState::Off; crate::catalog::all(cpu).len()], cpu)
}

/// Moves focus to the tweaks list. The detail panel's own buttons and the panel's edge come
/// before the sidebar and the list in reading order, and which of them is on screen depends on the
/// chosen tweak, so loop rather than hard-code how many there are.
pub(super) fn focus_tweaks(h: &mut Harness<Tools>) {
    for _ in 0..8 {
        h.press("tab");
        if h.is_focused("tweaks") {
            return;
        }
    }
    panic!("tab never reached the tweaks list:\n{}", h.screen());
}

#[test]
fn it_opens_on_the_packages_group_with_its_tweaks() {
    let h = harness(100, 24);
    let screen = h.screen();
    for text in ["qtools", "Packages", "Mirror list", "Pacman options"] {
        assert!(screen.contains(text), "`{text}` is missing:\n{screen}");
    }
}

#[test]
fn a_click_on_a_group_shows_that_group() {
    let mut h = harness(100, 24);
    h.click_text("Maintenance");
    assert!(h.screen().contains("Maintenance"), "{}", h.screen());
}

#[test]
fn the_detail_panel_lists_everything_the_chosen_tweak_touches() {
    let h = harness(100, 24);
    let screen = h.screen();
    for text in ["reflector", "/etc/pacman.d/mirrorlist", "reflector.timer"] {
        assert!(screen.contains(text), "`{text}` is missing from the preview:\n{screen}");
    }
}

#[test]
fn the_microcode_item_reads_in_the_hardware_group_at_every_size_with_the_package_of_its_processor() {
    for (cpu, package) in [(Cpu::Intel, "intel-ucode"), (Cpu::Amd, "amd-ucode")] {
        for (width, height) in [(100, 24), (48, 20), (80, 14)] {
            for glyphs in [GlyphMode::Unicode, GlyphMode::Ascii] {
                let mut h = Harness::with_env(off(cpu), crate::locales::env(), width, height);
                h.set_locale("en").set_glyph_mode(glyphs);
                // The narrow strip scrolls, so the group is picked the way the keyboard picks
                // it rather than by a click on a tab that may be off the strip.
                h.send(Msg::PickGroup("hardware".to_owned()));
                // The list is all a narrow screen gives, and the detail below it the package
                // the item installs; a wide screen shows both at once.
                if width < 72 {
                    focus_tweaks(&mut h);
                    h.press("alt+b");
                }
                let what = format!("{cpu:?} {width}x{height} {glyphs:?}");
                let screen = h.screen();
                assert!(!screen.contains('⟦'), "{what}: a key has no text:\n{screen}");
                for text in ["Processor microcode", package] {
                    assert!(screen.contains(text), "{what}: `{text}` is missing:\n{screen}");
                }
                if glyphs == GlyphMode::Ascii {
                    for forbidden in ["[ ]", "[x]", "(o)", "|", "===", "{", "}"] {
                        assert!(!screen.contains(forbidden), "{what}: `{forbidden}` is forbidden:\n{screen}");
                    }
                }
            }
        }
    }
}

#[test]
fn the_microcode_item_shows_what_it_touches_before_anything_runs() {
    let mut h = Harness::with_env(off(Cpu::Intel), crate::locales::env(), 100, 40);
    h.set_locale("en").set_glyph_mode(GlyphMode::Unicode);
    h.click_text("Hardware");

    let screen = h.screen();
    for text in ["Touches", "intel-ucode", "/etc/mkinitcpio.conf", "/etc/mkinitcpio.conf.d", "mkinitcpio -P"] {
        assert!(screen.contains(text), "`{text}` is missing from the preview:\n{screen}");
    }
    for forbidden in ["/boot", "loader.conf", "grub"] {
        assert!(!screen.contains(forbidden), "no bootloader is named: {forbidden}\n{screen}");
    }
}

#[test]
fn space_checks_a_tweak_and_space_again_unchecks_it() {
    let mut h = harness(100, 24);
    focus_tweaks(&mut h);
    h.press("space");
    assert_eq!(h.app().checked.iter().filter(|checked| **checked).count(), 1);
    h.press("space");
    assert_eq!(h.app().checked.iter().filter(|checked| **checked).count(), 0);
}

#[test]
fn a_narrow_screen_keeps_the_rules_and_stays_readable() {
    let mut h = harness(48, 20);
    h.set_glyph_mode(GlyphMode::Ascii);
    let screen = h.screen();
    for forbidden in ["[ ]", "[x]", "(o)", "|", "==="] {
        assert!(!screen.contains(forbidden), "`{forbidden}` is forbidden in every mode:\n{screen}");
    }
    assert!(screen.contains("Mirror list"), "the list stays readable:\n{screen}");
}

/// The five package titles, in full: the narrow list must never clip one.
const PACKAGE_TITLES: [&str; 5] =
    ["Mirror list", "Pacman options", "Multilib repository", "AUR helper", "Cache cleanup"];

/// Screen line `y`, as text.
fn line(h: &Harness<Tools>, y: i32) -> String {
    h.screen().lines().nth(usize::try_from(y).expect("a row on screen")).unwrap_or_default().to_owned()
}

#[test]
fn a_narrow_screen_has_the_list_at_full_width_without_closing_anything() {
    let mut h = harness(48, 20);
    h.set_glyph_mode(GlyphMode::Ascii);
    h.set_reduced_motion(true);

    let screen = h.screen();
    for forbidden in ["[ ]", "[x]", "(o)", "|", "==="] {
        assert!(!screen.contains(forbidden), "`{forbidden}` is forbidden in every mode:\n{screen}");
    }
    for text in PACKAGE_TITLES {
        assert!(screen.contains(text), "`{text}` reads in full on a narrow screen:\n{screen}");
    }
    assert!(!screen.contains("reflector"), "the detail is not beside the list on a narrow screen:\n{screen}");
}

#[test]
fn a_narrow_screen_moves_the_groups_to_a_strip_above_the_list() {
    let h = harness(48, 20);
    let screen = h.screen();
    let (Some(packages), Some(security), Some(mirrors)) =
        (h.find("Packages"), h.find("Security"), h.find("Mirror list"))
    else {
        panic!("the strip and the list are on screen:\n{screen}");
    };
    assert_eq!(packages.1, security.1, "the groups sit on one line:\n{screen}");
    assert!(packages.0 < security.0, "in catalog order:\n{screen}");
    assert!(packages.1 < mirrors.1, "the strip sits above the list:\n{screen}");
    // The open tab carries the framework's pillar, two cells before its label; the other does not.
    assert!(line(&h, packages.1).contains("▌ Packages"), "the open tab is marked:\n{screen}");
    assert!(!line(&h, packages.1).contains("▌ Security"), "only the open tab is marked:\n{screen}");
    for text in PACKAGE_TITLES {
        assert!(screen.contains(text), "`{text}` reads in full:\n{screen}");
    }
}

#[test]
fn a_group_in_the_strip_is_picked_by_click_and_by_key() {
    let mut h = harness(48, 20);
    h.click_text("Security");
    assert_eq!(h.app().group, Group::Security);
    assert!(h.screen().contains("Firewall"), "the list follows the tab clicked:\n{}", h.screen());

    for _ in 0..8 {
        if h.is_focused("groups") {
            break;
        }
        h.press("tab");
    }
    assert!(h.is_focused("groups"), "tab reaches the strip");
    h.press("left");
    assert_eq!(h.app().group, Group::Packages, "left opens the tab before");
    assert!(h.screen().contains("Mirror list"), "the list follows the key:\n{}", h.screen());
}

#[test]
fn folding_and_unfolding_keeps_the_selection() {
    let mut h = harness(100, 24);
    focus_tweaks(&mut h);
    h.press("down").press("down");
    assert_eq!(h.app().selected, 2);

    h.resize(48, 20);
    assert_eq!(h.app().selected, 2, "folding keeps the selection");
    let narrow = h.screen();
    let (Some(packages), Some(multilib)) = (h.find("Packages"), h.find("Multilib repository")) else {
        panic!("{narrow}");
    };
    assert!(packages.1 < multilib.1, "folded: the strip is above the list:\n{narrow}");
    assert!(line(&h, multilib.1).starts_with('▌'), "the selected row keeps its pillar:\n{narrow}");

    h.resize(100, 24);
    assert_eq!(h.app().selected, 2, "unfolding keeps the selection");
    let wide = h.screen();
    let (Some(packages), Some(multilib)) = (h.find("Packages"), h.find("Multilib repository")) else {
        panic!("{wide}");
    };
    assert_eq!(packages.1, multilib.1, "unfolded: the sidebar is beside the list again:\n{wide}");
    assert!(wide.contains("pacman.conf"), "the detail is beside the list again:\n{wide}");
}

// The other half of that promise: the row a person chose belongs to the group they chose it in.
// The groups do not even have the same number of rows, so a row number carried into another one
// points past its end, where the detail has no item to speak of and no row is marked at all.
#[test]
fn another_group_starts_at_its_own_first_row() {
    let mut h = harness(100, 24);
    focus_tweaks(&mut h);
    h.press("down").press("down");
    assert_eq!(h.app().selected, 2, "the third of the five in Packages");

    h.click_text("Security");
    assert_eq!(h.app().group, Group::Security);
    assert_eq!(h.app().selected, 0, "a group other than the one open starts at its first row");
    let screen = h.screen();
    assert!(screen.contains("ufw.service"), "the detail is that first item's own:\n{screen}");
    assert!(!screen.contains("Multilib repository"), "nothing of the group left behind is on screen:\n{screen}");
}

// The wide screen's own panel: `alt+b` is the one key that closes it and opens it again, and
// the list takes the whole width while it is shut. The narrow screen has its own key, in
// `on_a_narrow_screen_the_detail_opens_below_the_list_and_esc_closes_it`; this is the wide half
// of the promise, which nothing else reaches.
#[test]
fn alt_b_closes_the_detail_beside_the_list_and_opens_it_again() {
    let mut h = harness(100, 24);
    assert!(h.app().panel_open, "the detail is open beside the list to begin with");
    let open = h.screen();
    assert!(open.contains("reflector") && open.contains("/etc/pacman.d/mirrorlist"), "{open}");
    focus_tweaks(&mut h);

    h.press("alt+b");
    assert!(!h.app().panel_open, "alt+b shut it");
    // The panel slides shut, as a person watches it do.
    h.advance(std::time::Duration::from_millis(800));
    let closed = h.screen();
    for text in PACKAGE_TITLES {
        assert!(closed.contains(text), "`{text}` has the whole width while the panel is shut:\n{closed}");
    }
    assert!(!closed.contains("reflector"), "nothing of the detail is left on screen:\n{closed}");
    assert!(!closed.contains("Touches"), "{closed}");

    h.press("alt+b");
    assert!(h.app().panel_open, "alt+b opened it again");
    h.advance(std::time::Duration::from_millis(800));
    let again = h.screen();
    assert!(again.contains("reflector") && again.contains("/etc/pacman.d/mirrorlist"), "{again}");
}

#[test]
fn on_a_narrow_screen_the_detail_opens_below_the_list_and_esc_closes_it() {
    let mut h = harness(48, 20);
    focus_tweaks(&mut h);
    h.press("alt+b");
    let open = h.screen();
    // The title reads twice: once in the list row, once at the head of the detail below it.
    let mut titled = open.lines().filter(|row| row.contains("Mirror list"));
    let (Some(_), Some(heading)) = (titled.next(), titled.next()) else {
        panic!("the detail repeats the chosen tweak's title below the list:\n{open}");
    };
    assert!(heading.contains("Off"), "the badge is beside the title:\n{open}");
    for text in ["Touches", "reflector", "/etc/pacman.d/mirrorlist"] {
        assert!(open.contains(text), "`{text}` is in the detail:\n{open}");
    }
    for text in PACKAGE_TITLES {
        assert!(open.contains(text), "the list keeps its rows while the detail is open:\n{open}");
    }
    assert!(open.contains("close"), "the footer says how to leave the detail:\n{open}");

    // The detail scrolls: what is below the fold is reachable rather than lost.
    scroll_detail_down(&mut h);
    let scrolled = h.screen();
    assert!(scrolled.contains("reflector.timer"), "the last touch is one scroll away:\n{scrolled}");

    h.press("esc");
    let closed = h.screen();
    assert!(!closed.contains("reflector"), "esc closes the detail:\n{closed}");
    assert!(closed.contains("Cache cleanup"), "the list is whole again:\n{closed}");
}

/// Wheels the pointer over the detail, wherever it is on screen, so what is below its fold comes
/// into view.
fn scroll_detail_down(h: &mut Harness<Tools>) {
    let area = h.buffer().area;
    h.mouse(MouseKind::ScrollDown, i32::from(area.width) - 4, i32::from(area.height) / 2);
    h.advance(std::time::Duration::from_millis(120));
}

#[test]
fn the_narrow_detail_keeps_the_rules_in_ascii() {
    let mut h = harness(48, 20);
    h.set_glyph_mode(GlyphMode::Ascii);
    focus_tweaks(&mut h);
    h.press("alt+b");
    let screen = h.screen();
    assert!(screen.contains("reflector"), "the detail is open:\n{screen}");
    for forbidden in ["[ ]", "[x]", "(o)", "|", "==="] {
        assert!(!screen.contains(forbidden), "`{forbidden}` is forbidden in every mode:\n{screen}");
    }
}

#[test]
fn turkish_uses_its_own_words() {
    let mut h = harness(100, 24);
    h.set_locale("tr");
    let screen = h.screen();
    assert!(screen.contains("Yansı listesi"), "{screen}");
    assert!(!screen.contains("Mirror list"), "no English leaks into Turkish:\n{screen}");
}

#[test]
fn an_unavailable_tweak_cannot_be_checked_by_key_or_by_click() {
    let mut states = every_tweak_off();
    states[0] = TweakState::Unavailable("state.off");
    let mut h = Harness::with_env(Tools::new(states, Cpu::Intel), crate::locales::env(), 100, 24);
    h.set_locale("en").set_glyph_mode(GlyphMode::Unicode);

    // By key: the unavailable row (mirrors) is the first one, selected as the list opens.
    focus_tweaks(&mut h);
    h.press("space");
    assert!(!h.app().checked[0], "an unavailable tweak stays unchecked by its key");

    // By click: the check mark column never moves, so the unavailable row's own mark sits at
    // the same x as every other row's; only the y differs, one line per row.
    let (mark_x, mirrors_y) = h.find("☐").expect("an unchecked mark is drawn");
    h.click(mark_x, mirrors_y);
    assert!(!h.app().checked[0], "a click on its mark does nothing either");

    // An ordinary tweak in the same list, one row down, still checks normally.
    let (_, pacman_y) = h.find("Pacman options").expect("the ordinary tweak's row");
    h.click(mark_x, pacman_y);
    assert!(h.app().checked[1], "an ordinary tweak still checks");
}

#[test]
fn the_list_shows_what_the_machine_says_rather_than_guessing() {
    let mut h = harness(100, 24);
    let mut states = every_tweak_off();
    states[0] = TweakState::Applied;
    h.send(Msg::StatesRead(reading(states)));

    let screen = h.screen();
    assert!(screen.contains("Applied"), "an applied tweak says so:\n{screen}");
}

#[test]
fn a_tweak_that_does_not_fit_this_machine_is_faint_and_cannot_be_checked() {
    let mut h = harness(100, 24);
    let mut states = every_tweak_off();
    states[0] = TweakState::Unavailable("unavailable.no-ssd");
    h.send(Msg::StatesRead(reading(states)));
    // The space reaches the list itself, on the faint row, rather than the sidebar.
    focus_tweaks(&mut h);
    assert_eq!(h.app().selected, 0);
    h.press("space");

    assert!(h.app().checked.iter().all(|checked| !checked), "an unavailable tweak cannot be checked");
}

#[test]
fn enter_asks_before_it_touches_anything() {
    let mut h = harness(100, 24);
    focus_tweaks(&mut h);
    h.press("enter");
    let screen = h.screen();
    assert!(screen.contains("reflector"), "the confirmation names what will run:\n{screen}");
    // `t!` only resolves inside a running harness, so the English words are spelled out here
    // rather than looked up, the same way the other screen tests check literal English text.
    assert!(screen.contains("Apply"), "the confirmation asks:\n{screen}");
}

#[test]
fn cancelling_the_confirmation_changes_nothing() {
    let mut h = harness(100, 24);
    focus_tweaks(&mut h);
    h.press("enter").press("esc");
    assert!(h.app().running.is_none(), "nothing is running after a cancel");
}

/// A stand-in for the binary a run starts: a shell script that gets the same arguments. The
/// session's own tests start stand-ins through the same helper, so every test program in the
/// suite is a script in the system's temporary folder.
pub(crate) struct Script(pub(crate) std::path::PathBuf);

impl Script {
    pub(crate) fn new(name: &str, body: &str) -> Self {
        use std::os::unix::fs::PermissionsExt;
        let path = std::env::temp_dir().join(format!("qtools-{}-{name}.sh", std::process::id()));
        std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).expect("the script is written");
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).expect("the script is executable");
        Self(path)
    }
}

impl Drop for Script {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

pub(super) fn every_tweak_off() -> Vec<TweakState> {
    off(Cpu::default()).states
}

/// A machine where no tweak is applied and nothing is on record, read after every run. The
/// values in force are the recommended ones whatever the screen was built with, so a reading is
/// the same machine whoever asks.
pub(super) fn reading_off(_: &Choices) -> Reading {
    let states = every_tweak_off();
    let recorded = vec![Vec::new(); states.len()];
    Reading { states, recorded }
}

/// What the screen reads after a run: `states`, with nothing on record.
fn reading(states: Vec<TweakState>) -> Reading {
    let recorded = vec![Vec::new(); states.len()];
    Reading { states, recorded }
}

/// A screen whose runs start `script` instead of the real binary, and read no real machine.
pub(super) fn harness_running(script: &Script, states: Vec<TweakState>) -> Harness<Tools> {
    harness_recorded(script, states, &[])
}

/// Every step of the tweaks at the catalog places `on_record` on qtools' record, nothing of the
/// others, for a catalog of `count` tweaks on `cpu`.
pub(super) fn every_step_of(on_record: &[usize], count: usize, cpu: Cpu) -> Vec<Vec<usize>> {
    let catalog = catalog::all(cpu);
    (0..count)
        .map(|at| if on_record.contains(&at) { (0..catalog[at].steps.len()).collect() } else { Vec::new() })
        .collect()
}

/// The same screen, with every step of the tweaks at these catalog places on qtools' record.
pub(super) fn harness_recorded(script: &Script, states: Vec<TweakState>, on_record: &[usize]) -> Harness<Tools> {
    let recorded = every_step_of(on_record, states.len(), Cpu::Intel);
    let mut tools = Tools::new(states, Cpu::Intel).machine(script.0.clone(), reading_off).recorded(recorded);
    // Every run in this file is a process of its own with a terminal of its own, which is what
    // "ask for the password once while qtools is open" turned off means. The runs that share one
    // terminal are in `session::tests`.
    tools.password_once = false;
    let mut h = Harness::with_env(tools, crate::locales::env(), 100, 24);
    h.set_locale("en").set_glyph_mode(GlyphMode::Unicode);
    h
}

/// The same screen, with `reader` as its way of asking where every tweak stands once a run has
/// ended. A test that cares what the list says after a run gives a reader that answers
/// differently the second time.
fn harness_read_by(script: &Script, states: Vec<TweakState>, reader: fn(&Choices) -> Reading) -> Harness<Tools> {
    let recorded = vec![Vec::new(); states.len()];
    let mut tools = Tools::new(states, Cpu::Intel).machine(script.0.clone(), reader).recorded(recorded);
    tools.password_once = false;
    let mut h = Harness::with_env(tools, crate::locales::env(), 100, 24);
    h.set_locale("en").set_glyph_mode(GlyphMode::Unicode);
    h
}

/// Answers the open confirmation with its confirm button (Cancel has focus first).
pub(super) fn say_yes(h: &mut Harness<Tools>) {
    h.press("tab").press("enter");
}

/// Lets the run's watch deliver output until the process has ended, then lets the toast in.
pub(super) fn wait_for_exit(h: &mut Harness<Tools>) {
    use std::time::Duration;
    for _ in 0..100 {
        if h.app().running.as_ref().is_some_and(|running| running.exit.is_some()) {
            h.advance(Duration::from_millis(300));
            return;
        }
        h.advance(Duration::from_millis(10));
    }
    panic!("the run never ended:\n{}", h.screen());
}

#[test]
fn a_confirmed_run_happens_in_the_embedded_terminal_with_the_chosen_tweaks() {
    let script = Script::new("run", "echo \"$@\"");
    let mut h = harness_running(&script, every_tweak_off());
    focus_tweaks(&mut h);
    h.press("enter");
    say_yes(&mut h);
    assert!(h.app().running.is_some(), "the run starts once confirmed");
    assert!(h.is_focused("run"), "the terminal has focus so a password prompt can be answered");

    wait_for_exit(&mut h);
    let screen = h.screen();
    assert!(screen.contains("--run mirrors"), "the binary gets the chosen tweak:\n{screen}");
    assert!(screen.contains("Applying Mirror list"), "the run says what it is doing:\n{screen}");
    assert!(screen.contains("Done"), "the outcome is announced:\n{screen}");
    assert!(!screen.contains("password:"), "qtools never fakes a password prompt:\n{screen}");

    h.press("esc");
    assert!(h.app().running.is_none(), "esc closes the ended run");
    assert!(h.screen().contains("Mirror list"), "the list is back:\n{}", h.screen());
}

#[test]
fn checked_tweaks_run_together_and_are_unchecked_afterwards() {
    let script = Script::new("together", "echo \"$@\"");
    let mut h = harness_running(&script, every_tweak_off());
    focus_tweaks(&mut h);
    h.press("down").press("space").press("down").press("space");
    h.press("enter");
    say_yes(&mut h);
    wait_for_exit(&mut h);
    let screen = h.screen();
    assert!(screen.contains("--run pacman-options multilib"), "both checked tweaks run, in order:\n{screen}");
    assert!(h.app().checked.iter().all(|checked| !checked), "the checks are cleared once they ran");
}

/// The terminal's own note in its bottom corner says how the process ended; nothing of the
/// screen may sit over it, or the exit code is lost behind the outcome.
#[test]
fn the_outcome_leaves_the_terminal_exit_note_readable() {
    for (body, code, outcome) in [("echo \"$@\"", 0, "Done"), ("exit 1", 1, "It failed")] {
        let script = Script::new(&format!("note-{code}"), body);
        let mut h = harness_running(&script, every_tweak_off());
        focus_tweaks(&mut h);
        h.press("enter");
        say_yes(&mut h);
        wait_for_exit(&mut h);
        let screen = h.screen();
        assert!(screen.contains(&format!("exited with {code}")), "the exit note is whole:\n{screen}");
        let title = screen.lines().find(|line| line.contains("Applying Mirror list")).unwrap_or_default();
        assert!(title.contains(outcome), "the outcome stays beside the title:\n{screen}");
    }
}

#[test]
fn a_failed_run_says_so_and_keeps_its_output_on_screen() {
    let script = Script::new("fail", "echo 'mirror unreachable' >&2; exit 1");
    let mut h = harness_running(&script, every_tweak_off());
    focus_tweaks(&mut h);
    h.press("enter");
    say_yes(&mut h);
    wait_for_exit(&mut h);
    let screen = h.screen();
    assert!(screen.contains("mirror unreachable"), "the reason stays readable:\n{screen}");
    assert!(screen.contains("It failed"), "the failure is announced:\n{screen}");
    assert!(h.app().running.is_some(), "the output is not taken away");
}

#[test]
fn z_asks_before_undoing_an_applied_tweak_and_runs_the_revert() {
    let script = Script::new("revert", "echo \"$@\"");
    let mut states = every_tweak_off();
    states[0] = TweakState::Applied;
    let mut h = harness_recorded(&script, states, &[0]);
    focus_tweaks(&mut h);
    h.press("z");
    assert!(h.screen().contains("Undo this?"), "undoing asks first:\n{}", h.screen());
    say_yes(&mut h);
    wait_for_exit(&mut h);
    let screen = h.screen();
    assert!(screen.contains("--revert mirrors"), "the binary undoes the tweak:\n{screen}");
    assert!(screen.contains("Undoing Mirror list"), "{screen}");
}

// The detail's own Undo button: the mouse's way of doing what `z` does, for the tweak whose
// detail is open. The click lands on the button, whose word the footer's hint spells in lower
// case, so the two cannot be confused.
#[test]
fn the_details_undo_button_asks_before_undoing_the_way_the_z_key_does() {
    let script = Script::new("undo-button", "echo \"$@\"");
    let mut states = every_tweak_off();
    states[0] = TweakState::Applied;
    let mut h = harness_recorded(&script, states, &[0]);
    let (x, y) = h.find("Undo").expect("an applied tweak offers an Undo button in its detail");
    h.click(x, y);
    assert!(h.screen().contains("Undo this?"), "it asks first, like the key:\n{}", h.screen());
    say_yes(&mut h);
    wait_for_exit(&mut h);
    assert!(h.screen().contains("--revert mirrors"), "the binary undoes the tweak:\n{}", h.screen());
}

// The states a run leaves behind are read again from the machine, not remembered from before:
// the list has to say what the machine says, or a second run starts from a lie.
#[test]
fn the_list_reads_the_machine_again_once_a_run_has_ended() {
    let script = Script::new("reread", "echo \"$@\"");
    let mut h = harness_read_by(&script, every_tweak_off(), mirrors_applied);
    assert!(!h.screen().contains("Applied"), "nothing is applied to begin with:\n{}", h.screen());
    focus_tweaks(&mut h);
    h.press("enter");
    say_yes(&mut h);
    wait_for_exit(&mut h);
    h.press("esc");
    let screen = h.screen();
    assert!(screen.contains("Applied"), "the list shows what the machine now says:\n{screen}");
}

/// Where a machine that has just had the mirror list applied says every tweak stands.
fn mirrors_applied(_: &Choices) -> Reading {
    let mut states = every_tweak_off();
    states[0] = TweakState::Applied;
    let recorded = vec![Vec::new(); states.len()];
    Reading { states, recorded }
}

#[test]
fn nothing_is_asked_while_a_run_is_on_screen() {
    let script = Script::new("busy", "echo \"$@\"");
    let mut h = harness_running(&script, every_tweak_off());
    focus_tweaks(&mut h);
    h.press("enter");
    say_yes(&mut h);
    // The undo key is the one key of the list that is bound for the whole screen.
    h.press("z");
    assert!(!h.screen().contains("Undo this?"), "no second confirmation over a run:\n{}", h.screen());
    wait_for_exit(&mut h);
}

#[test]
fn the_panel_toggle_hint_survives_at_forty_eight_columns() {
    let h = harness(48, 20);
    let screen = h.screen();
    assert!(screen.contains("panel"), "the hint to reopen a closed panel must stay visible:\n{screen}");
}

#[test]
fn a_short_terminal_scrolls_instead_of_cutting_the_list() {
    // Six rows leave the list fewer lines than the packages group has tweaks.
    let mut h = harness(80, 6);
    assert!(!h.screen().contains("Cache cleanup"), "the last row starts out of view:\n{}", h.screen());
    focus_tweaks(&mut h);
    h.press("down").press("down").press("down").press("down");
    assert_eq!(h.app().selected, 4, "down moves the selection one row at a time");
    assert!(h.screen().contains("Cache cleanup"), "the focused row is brought into view:\n{}", h.screen());

    // A short terminal, fourteen rows: the list is whole and the detail scrolls rather than
    // losing what does not fit.
    let mut h = harness(80, 14);
    let screen = h.screen();
    for text in ["Mirror list", "Cache cleanup", "reflector", "/etc/pacman.d/mirrorlist"] {
        assert!(screen.contains(text), "`{text}` fits fourteen rows:\n{screen}");
    }
    scroll_detail_down(&mut h);
    assert!(h.screen().contains("reflector.timer"), "and the rest of it is one scroll away:\n{}", h.screen());
}

#[test]
fn reduced_motion_is_respected() {
    let mut h = harness(100, 24);
    h.set_reduced_motion(true);
    focus_tweaks(&mut h);
    // A selection move is what would slide; with motion reduced it lands at once.
    h.press("down");
    let before = h.screen();
    h.advance(std::time::Duration::from_millis(400));
    assert_eq!(before, h.screen(), "nothing moves on its own when motion is reduced");
    assert!(before.contains("Pacman options"), "{before}");
}

#[test]
fn the_mouse_can_do_what_the_keyboard_can() {
    let mut h = harness(100, 24);
    h.click_text("Multilib");
    assert_eq!(h.app().selected, 2, "a click selects the row the keyboard would reach with down, down");
    // A click on a row is also its enter: it asks, and nothing happens until the answer.
    assert!(h.screen().contains("Apply this?"), "a click asks before it touches anything:\n{}", h.screen());
    assert!(h.app().running.is_none(), "asking is not doing");
    h.press("esc");

    h.click_text("Security");
    assert_eq!(h.app().group, Group::Security, "a click on a group shows it, like the sidebar keys");
    assert!(!h.screen().contains("Mirror list"), "the packages list makes way for the group clicked:\n{}", h.screen());
}

#[test]
fn the_confirmation_says_where_the_backups_go_and_an_undo_does_not() {
    // Multilib writes /etc/pacman.conf, so the confirmation says where its old copy is kept.
    let mut h = harness(140, 40);
    h.click_text("Multilib");
    let screen = h.screen();
    assert!(screen.contains("Apply this?"), "{screen}");
    assert!(screen.contains("quvyta-tools/backups"), "where the backup goes:\n{screen}");
    h.press("esc");

    // An undo writes the backups back rather than keeping new ones.
    let script = Script::new("revert-backups", "echo \"$@\"");
    let mut states = every_tweak_off();
    states[0] = TweakState::Applied;
    let mut h = harness_recorded(&script, states, &[0]);
    h.resize(140, 40);
    focus_tweaks(&mut h);
    h.press("z");
    let screen = h.screen();
    assert!(screen.contains("Undo this?"), "{screen}");
    assert!(!screen.contains("quvyta-tools/backups"), "no backup is kept by an undo:\n{screen}");
}

#[test]
fn the_confirmation_counts_only_the_password_steps_a_run_still_has_to_do() {
    // The mirrors item is half done: the package and the timer are in place, and a run leaves
    // those alone, so only the other two of its four root steps will ask for the password.
    let mut states = every_tweak_off();
    states[0] = TweakState::Half(vec![true, false, false, true]);
    let mut h = Harness::with_env(Tools::new(states, Cpu::default()), crate::locales::env(), 140, 40);
    h.set_locale("en").set_glyph_mode(GlyphMode::Unicode);
    focus_tweaks(&mut h);
    h.press("enter");
    let screen = h.screen();
    assert!(screen.contains("Apply this?"), "{screen}");
    assert!(screen.contains("2 steps will ask for your password."), "{screen}");
}

#[test]
fn the_undo_confirmation_counts_only_the_password_steps_on_record() {
    // qtools wrote the mirror list and turned the timer on; the package was there before it, so
    // an undo puts back those two steps alone, each as root.
    let mut states = every_tweak_off();
    states[0] = TweakState::Half(vec![true, true, false, true]);
    let mut recorded = vec![Vec::new(); states.len()];
    recorded[0] = vec![1, 3];
    let tools = Tools::new(states, Cpu::default()).recorded(recorded);
    let mut h = Harness::with_env(tools, crate::locales::env(), 140, 40);
    h.set_locale("en").set_glyph_mode(GlyphMode::Unicode);
    focus_tweaks(&mut h);
    h.press("z");
    let screen = h.screen();
    assert!(screen.contains("Undo this?"), "{screen}");
    assert!(screen.contains("2 steps will ask for your password."), "{screen}");
}

// The undo count above is only as true as the reading behind it: the steps on record come from
// qtools' journal, each once and in step order, however the journal lists them.
#[test]
fn the_reading_takes_the_steps_on_record_from_the_journal() {
    let folder = std::path::Path::new("/state/quvyta-tools");
    let mut system = crate::system::FakeSystem::new();
    system.file(
        "/state/quvyta-tools/applied.toml",
        "[[mirrors]]\nstep = 3\n\n[[mirrors]]\nstep = 1\n\n[[mirrors]]\nstep = 3\n",
    );
    let reading = read(&Choices::recommended(), &mut system, Cpu::Intel, folder);
    let mirrors = catalog::all(Cpu::Intel).iter().position(|tweak| tweak.id == "mirrors").expect("in the catalog");
    assert_eq!(reading.recorded[mirrors], vec![1, 3]);
    assert!(
        reading.recorded.iter().enumerate().all(|(at, steps)| at == mirrors || steps.is_empty()),
        "nothing else is on record: {:?}",
        reading.recorded
    );
}

#[test]
fn an_ascii_run_screen_keeps_the_rules() {
    let script = Script::new("ascii", "echo \"$@\"");
    let mut h = harness_running(&script, every_tweak_off());
    focus_tweaks(&mut h);
    h.press("enter");
    say_yes(&mut h);
    wait_for_exit(&mut h);
    h.set_glyph_mode(GlyphMode::Ascii);
    let screen = h.screen();
    for forbidden in ["[ ]", "[x]", "(o)", "|", "==="] {
        assert!(!screen.contains(forbidden), "`{forbidden}` is forbidden in every mode:\n{screen}");
    }
    assert!(screen.contains("--run mirrors"), "the run's output is still there:\n{screen}");
}

#[test]
fn visual_review() {
    let mut pages = Vec::new();
    for (width, height, caption) in [(100, 24, "wide 100x24"), (48, 20, "narrow 48x20"), (80, 14, "short 80x14")] {
        pages.push(harness(width, height).html(caption));
    }
    let mut h = harness(48, 20);
    focus_tweaks(&mut h);
    h.press("alt+b");
    pages.push(h.html("narrow 48x20, detail open"));
    let mut h = harness(100, 24);
    focus_tweaks(&mut h);
    h.press("enter");
    pages.push(h.html("confirmation open 100x24"));
    let mut h = harness(100, 24);
    h.set_locale("tr");
    pages.push(h.html("Turkish 100x24"));

    let target = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("target");
    std::fs::create_dir_all(&target).expect("the target directory exists");
    std::fs::write(target.join("visual-review.html"), qframe::runtime::html_page(&pages))
        .expect("the review page is written");
}

// A timer the person turned on by hand shows as applied, but qtools did nothing there: an undo
// used to be offered anyway, asked for its confirmation, ran, said "Done" and changed nothing.
#[test]
fn an_applied_tweak_qtools_did_not_do_offers_no_undo() {
    let script = Script::new("not-ours", "echo \"$@\"");
    let mut states = every_tweak_off();
    states[0] = TweakState::Applied;
    let mut h = harness_running(&script, states);
    assert!(h.screen().contains("Applied"), "{}", h.screen());
    assert!(h.find("Undo").is_none(), "no undo button for what qtools did not do:\n{}", h.screen());

    focus_tweaks(&mut h);
    h.press("z");

    assert!(!h.screen().contains("Undo this?"), "the key asks nothing either:\n{}", h.screen());
}

#[test]
fn a_tweak_qtools_did_offers_an_undo_whose_button_asks_first() {
    let script = Script::new("ours", "echo \"$@\"");
    let mut states = every_tweak_off();
    states[0] = TweakState::Applied;
    let mut h = harness_recorded(&script, states, &[0]);

    h.click_text("Undo");

    assert!(h.screen().contains("Undo this?"), "the button asks before anything runs:\n{}", h.screen());
}

// Enter and a click on a row that does not fit the machine open no confirmation: the guard in
// front of every run, not only the check mark's.
#[test]
fn an_unavailable_tweak_is_not_asked_about_by_enter() {
    let script = Script::new("unavailable", "echo \"$@\"");
    let mut states = every_tweak_off();
    states[0] = TweakState::Unavailable("unavailable.no-ssd");
    let mut h = harness_running(&script, states);
    focus_tweaks(&mut h);

    h.press("enter");

    assert!(!h.screen().contains("Apply this?"), "nothing is asked:\n{}", h.screen());
}

/// The screen opened on what [`read`] made of `system`, as `run_on` opens it on the real machine.
fn read_off(system: &mut crate::system::FakeSystem) -> Harness<Tools> {
    let reading = read(&Choices::recommended(), system, Cpu::Intel, Path::new("/home/test/.local/state/quvyta-tools"));
    let mut h = Harness::with_env(Tools::new(reading.states, Cpu::Intel), crate::locales::env(), 140, 40);
    h.set_locale("en").set_glyph_mode(GlyphMode::Unicode);
    h
}

#[test]
fn an_item_whose_state_cannot_be_read_says_unknown_and_why_rather_than_off() {
    let mut system = crate::system::FakeSystem::new();
    system.refuse_reads("/etc/pacman.conf");
    let mut h = read_off(&mut system);
    focus_tweaks(&mut h);
    h.press("down");

    let screen = h.screen();
    assert!(screen.contains("Pacman options"), "{screen}");
    assert!(screen.contains("Unknown"), "the list and the badge say it could not be read:\n{screen}");
    assert!(screen.contains("qtools could not read"), "and the panel says why:\n{screen}");
}

#[test]
fn the_screen_says_whether_ssh_gets_through_the_firewall_without_asking_for_root() {
    let ok = |stdout: &str| crate::system::Output { code: 0, stdout: stdout.into(), stderr: String::new() };
    let mut system = crate::system::FakeSystem::new();
    system.answer("pacman -Q openssh", ok("openssh 10.0p1-1"));
    system.answer("pacman -Q ufw", ok("ufw 0.36.2-4"));
    system.answer("test -r /etc/ufw/user.rules", ok(""));
    system.answer("grep -qx ### tuple ### allow tcp 22 0.0.0.0/0 any 0.0.0.0/0 in /etc/ufw/user.rules", ok(""));
    system.reading();
    let mut h = read_off(&mut system);
    h.click_text("Security");
    focus_tweaks(&mut h);
    h.press("down");

    let screen = h.screen();
    assert!(screen.contains("SSH through the firewall"), "{screen}");
    assert!(screen.contains("Applied"), "the rule is there, read without root:\n{screen}");
    assert!(
        !system.calls().iter().any(|call| call.starts_with("sudo ")),
        "the screen asked for root: {:?}",
        system.calls()
    );
}
