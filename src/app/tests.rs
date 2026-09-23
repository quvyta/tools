use qframe::icons::GlyphMode;

use super::*;

fn harness(width: u16, height: u16) -> Harness<Tools> {
    let mut h = Harness::with_env(Tools::default(), crate::locales::env(), width, height);
    h.set_locale("en").set_glyph_mode(GlyphMode::Unicode);
    h
}

/// Moves focus to the tweaks list. The groups sidebar is the first stop in reading order, and
/// the panel's own edge (opened by `on_toggle`) is one more; loop rather than hard-code how many.
pub(super) fn focus_tweaks(h: &mut Harness<Tools>) {
    for _ in 0..5 {
        h.press("tab");
        if h.is_focused("tweaks") {
            return;
        }
    }
    panic!("tab never reached the tweaks list");
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

    for _ in 0..5 {
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
    for text in ["Touches", "reflector", "/etc/pacman.d/mirrorlist", "reflector.timer"] {
        assert!(open.contains(text), "`{text}` is in the detail:\n{open}");
    }
    for text in PACKAGE_TITLES {
        assert!(open.contains(text), "the list keeps its rows while the detail is open:\n{open}");
    }
    assert!(open.contains("close"), "the footer says how to leave the detail:\n{open}");

    h.press("esc");
    let closed = h.screen();
    assert!(!closed.contains("reflector"), "esc closes the detail:\n{closed}");
    assert!(closed.contains("Cache cleanup"), "the list is whole again:\n{closed}");
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
    let mut states = vec![TweakState::Off; crate::catalog::all().len()];
    states[0] = TweakState::Unavailable("state.off");
    let mut h = Harness::with_env(Tools::new(states), crate::locales::env(), 100, 24);
    h.set_locale("en").set_glyph_mode(GlyphMode::Unicode);

    // By key: select the unavailable row (mirrors, index 0) and try to check it.
    h.send(Msg::Select(0));
    h.send(Msg::Toggle(0));
    assert!(!h.app().checked[0], "an unavailable tweak stays unchecked from a message");

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
    let mut states = vec![TweakState::Off; crate::catalog::all().len()];
    states[0] = TweakState::Applied;
    h.send(Msg::StatesRead(states));

    let screen = h.screen();
    assert!(screen.contains("Applied"), "an applied tweak says so:\n{screen}");
}

#[test]
fn a_tweak_that_does_not_fit_this_machine_is_faint_and_cannot_be_checked() {
    let mut h = harness(100, 24);
    let mut states = vec![TweakState::Off; crate::catalog::all().len()];
    states[0] = TweakState::Unavailable("unavailable.no-ssd");
    h.send(Msg::StatesRead(states));
    h.press("tab").press("space");

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

/// A stand-in for the binary a run starts: a shell script that gets the same arguments.
pub(super) struct Script(pub(super) std::path::PathBuf);

impl Script {
    pub(super) fn new(name: &str, body: &str) -> Self {
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
    vec![TweakState::Off; crate::catalog::all().len()]
}

/// A screen whose runs start `script` instead of the real binary, and read no real machine.
fn harness_running(script: &Script, states: Vec<TweakState>) -> Harness<Tools> {
    let tools = Tools::new(states).machine(script.0.clone(), every_tweak_off);
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
    h.send(Msg::Toggle(1)).send(Msg::Toggle(2));
    focus_tweaks(&mut h);
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
    let mut h = harness_running(&script, states);
    focus_tweaks(&mut h);
    h.press("z");
    assert!(h.screen().contains("Undo this?"), "undoing asks first:\n{}", h.screen());
    say_yes(&mut h);
    wait_for_exit(&mut h);
    let screen = h.screen();
    assert!(screen.contains("--revert mirrors"), "the binary undoes the tweak:\n{screen}");
    assert!(screen.contains("Undoing Mirror list"), "{screen}");
}

#[test]
fn nothing_is_asked_while_a_run_is_on_screen() {
    let script = Script::new("busy", "echo \"$@\"");
    let mut h = harness_running(&script, every_tweak_off());
    focus_tweaks(&mut h);
    h.press("enter");
    say_yes(&mut h);
    h.send(Msg::Apply(1));
    assert!(!h.screen().contains("Apply this?"), "no second confirmation over a run:\n{}", h.screen());
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

    // A short terminal, fourteen rows: nothing is cut there either.
    let h = harness(80, 14);
    let screen = h.screen();
    for text in ["Mirror list", "Cache cleanup", "reflector.timer"] {
        assert!(screen.contains(text), "`{text}` fits fourteen rows:\n{screen}");
    }
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
