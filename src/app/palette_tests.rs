//! The command palette: opened with `ctrl k`, found by typing a few letters of a command's name,
//! run with `enter` or with a click on its row, and closed with `esc`, the mark beside the filter
//! or a click on the dimmed screen.
//!
//! Every test drives the control, never a message: the palette is opened with its own key, typed
//! into with key presses, and a command is run with a key or the mouse, so what is tested is the
//! palette and not a message that would have the same effect. A run that is confirmed starts the
//! same stand-in script the screen's own tests use, so nothing on this machine is ever touched.

use std::time::Duration;

use qframe::icons::GlyphMode;
use qframe::keymap::{KeyChord, Scope};
use qframe::prelude::*;

use super::tests::{
    Script, every_tweak_off, focus_tweaks, harness, harness_recorded, harness_running, say_yes, wait_for_exit,
};
use super::wizard::tests::Root;
use super::{Opening, Tools, UpdateFolders};
use crate::catalog::hardware::Cpu;
use crate::tweak::{Group, TweakState};

/// Long enough for the palette to finish coming in, so a click lands on the row that is drawn.
const ENTERED: Duration = Duration::from_millis(300);

/// Long enough for what a command moved to finish moving: the side panel takes its own time to
/// slide shut, and the screen is only asked about once it has.
const SETTLED: Duration = Duration::from_millis(400);

/// The framework's own wording of the filter field, which no screen of qtools draws anywhere else:
/// its presence says the palette is open.
const FILTER: &str = "Type a command";

/// What the framework says when nothing matches what has been typed.
const NO_MATCH: &str = "No matching commands";

/// Opens the palette the way a person does, and lets its entrance finish.
fn open(h: &mut Harness<Tools>) -> &mut Harness<Tools> {
    h.press("ctrl+k").advance(ENTERED)
}

/// Runs the command whose name begins with `query`: open, type a few letters, `enter`, and let
/// the screen finish moving.
fn run<'a>(h: &'a mut Harness<Tools>, query: &str) -> &'a mut Harness<Tools> {
    open(h).type_text(query).press("enter").advance(SETTLED)
}

/// Runs the row under the highlight, the way a person who did not type does: `enter`, then the
/// time the screen needs to finish moving.
fn run_enter(h: &mut Harness<Tools>) -> &mut Harness<Tools> {
    h.press("enter").advance(SETTLED)
}

/// Whether the palette is on screen.
fn is_open(h: &Harness<Tools>) -> bool {
    h.screen().contains(FILTER)
}

/// The screen's row that carries `label`, so a test can see what stands beside it.
fn row_of(h: &Harness<Tools>, label: &str) -> String {
    h.screen().lines().find(|line| line.contains(label)).unwrap_or_default().to_owned()
}

/// The screen's first row, the header.
fn header_of(h: &Harness<Tools>) -> String {
    h.screen().lines().next().unwrap_or_default().to_owned()
}

/// The first marked row of the list, which is the chosen item's.
fn marked(h: &Harness<Tools>) -> bool {
    h.screen().contains("☑")
}

/// The screen on a machine where the catalog places `on_record` are on qtools' record and the
/// tweaks stand as `states`: what the person sees, with no run behind it.
fn recorded(states: Vec<TweakState>, on_record: Vec<usize>) -> Harness<Tools> {
    let mut flags = vec![false; states.len()];
    for at in on_record {
        flags[at] = true;
    }
    let mut h = Harness::with_env(Tools::new(states, Cpu::default()).recorded(flags), crate::locales::env(), 100, 24);
    h.set_locale("en").set_glyph_mode(GlyphMode::Unicode);
    h
}

#[test]
fn ctrl_k_opens_the_palette_with_what_the_screen_allows_and_the_keys_beside_them() {
    let mut states = every_tweak_off();
    states[0] = TweakState::Applied;
    // The mirrors are on qtools' record, so all four of the item's own commands are there.
    let mut h = recorded(states, vec![0]);
    open(&mut h);

    let screen = h.screen();
    assert!(screen.contains(FILTER), "the filter is on top of the screen:\n{screen}");
    // The item's own commands and the key each of them shares with the screen, and the key the
    // screen binds for the detail.
    for (label, key) in [
        ("Apply: Mirror list", "enter"),
        ("Check: Mirror list", "space"),
        ("Undo: Mirror list", "z"),
        ("Hide the detail panel", "alt b"),
    ] {
        let row = row_of(&h, label);
        assert!(row.contains(label), "`{label}` is listed:\n{screen}");
        assert!(row.contains(key), "`{key}` stands beside `{label}`:\n{screen}");
    }
    // Every group and the Settings page, whose keys are the sidebar's rather than the keymap's.
    for label in ["Packages", "Security", "Hardware", "Appearance", "Maintenance", "Settings"] {
        let row = row_of(&h, label);
        assert!(row.contains(label), "`{label}` is listed:\n{screen}");
        assert!(!row.contains("ctrl"), "a group has no key beside it: {row}");
    }
    // Leaving is the last of the twelve, below the fold of a list that shows ten at a time: it is
    // reached by typing, which is how a person finds it, and the key beside it is checked in the
    // test below. What this screen says is that the list knows of all twelve, the item's options
    // among them.
    assert!(screen.contains("12 / 12"), "every command is counted:\n{screen}");
}

#[test]
fn the_key_beside_a_command_is_the_one_the_keymap_binds() {
    let mut env = crate::locales::env();
    env.keymap_mut().bind(Scope::App, "revert", &["u".parse::<KeyChord>().expect("a key")]);
    let mut states = every_tweak_off();
    states[0] = TweakState::Applied;
    let tools = Tools::new(states, Cpu::default()).recorded({
        let mut flags = vec![false; every_tweak_off().len()];
        flags[0] = true;
        flags
    });
    let mut h = Harness::with_env(tools, env, 100, 24);
    h.set_locale("en").set_glyph_mode(GlyphMode::Unicode);
    open(&mut h);

    let row = row_of(&h, "Undo: Mirror list");
    assert!(row.contains("u"), "the rebound key stands beside the command:\n{}", h.screen());
    assert!(!row.contains('z'), "and the old one does not: {}", h.screen());
}

/// The list's own keys are the framework's, not the keymap's, so the palette's column and the
/// footer's hints are two written-out statements about them. Where the two drift apart, one of the
/// screen's two own statements is a lie, and nothing else would say so.
#[test]
fn the_keys_beside_the_commands_are_the_ones_the_footer_offers() {
    let mut h = harness(100, 24);
    focus_tweaks(&mut h);
    let footer = h.screen().lines().last().unwrap_or_default().to_owned();
    open(&mut h);
    for (label, key) in [("Apply: Mirror list", "enter"), ("Check: Mirror list", "space")] {
        assert!(row_of(&h, label).contains(key), "`{key}` stands beside `{label}`:\n{}", h.screen());
        assert!(footer.contains(key), "and the footer offers that same `{key}`: {footer}");
    }
}

#[test]
fn typing_a_few_letters_finds_a_command_and_enter_runs_it() {
    let mut h = harness(100, 24);
    open(&mut h).type_text("secur");
    let screen = h.screen();
    assert!(screen.contains("Security"), "the group is the one match:\n{screen}");
    assert!(!screen.contains("Quit"), "and the only one: {screen}");

    h.press("enter").advance(SETTLED);
    assert_eq!(h.app().group, Group::Security, "enter ran it");
    assert!(!is_open(&h), "the palette closed with the command:\n{}", h.screen());
    let screen = h.screen();
    assert!(screen.contains("Firewall") && screen.contains("ufw"), "and the group's own items followed: {screen}");
}

/// The list is walked without typing, with the arrow keys, and the row under the highlight is the
/// one `enter` runs: the detail first, then the item's own commands in the footer's order.
#[test]
fn the_arrow_keys_walk_the_list_and_enter_runs_the_row_under_the_highlight() {
    let mut h = harness(100, 24);
    focus_tweaks(&mut h);
    open(&mut h);
    // The first row is the detail, and `enter` on it would close the panel; the row below is the
    // item's own apply, which asks first.
    h.press("down");
    run_enter(&mut h);
    let screen = h.screen();
    assert!(screen.contains("Apply this?"), "the row under the highlight ran:\n{screen}");
    assert!(screen.contains("reflector"), "and it was the chosen item's own command: {screen}");
    h.press("esc");
}

#[test]
fn a_click_on_a_row_runs_that_command_too() {
    let mut h = harness(100, 24);
    open(&mut h);
    h.click_text("Hide the detail panel").advance(SETTLED);
    assert!(!h.app().panel_open, "the panel the command named is closed");
    assert!(!h.screen().contains("reflector"), "and its detail is gone: {}", h.screen());

    open(&mut h);
    h.click_text("Check: Mirror list");
    assert!(h.app().checked[0], "the row is checked");
    assert!(marked(&h), "and the mark is on screen: {}", h.screen());
}

#[test]
fn esc_closes_the_palette_without_running_anything() {
    let mut h = harness(100, 24);
    open(&mut h).type_text("quit");
    h.press("esc");
    assert!(!is_open(&h), "esc closed it:\n{}", h.screen());
    assert!(!h.quit_requested(), "and nothing ran");
    assert!(h.app().group == Group::Packages && !h.app().checked[0], "the screen is as it was");
}

#[test]
fn a_click_on_the_dimmed_screen_closes_it_too() {
    let mut h = harness(100, 24);
    open(&mut h);
    h.click(1, 22);
    assert!(!is_open(&h), "{}", h.screen());
    assert!(h.app().running.is_none() && !h.app().checked[0], "and nothing ran");
}

#[test]
fn the_apply_command_asks_first_for_the_tweak_chosen_on_the_screen() {
    let mut h = harness(100, 24);
    focus_tweaks(&mut h);
    // The detail closes, so what only the confirmation says can only have come from it.
    h.press("alt+b").press("down").press("down").press("down").press("down");
    run(&mut h, "apply");

    let screen = h.screen();
    assert!(screen.contains("Apply this?"), "applying asks first:\n{screen}");
    assert!(screen.contains("paccache.timer"), "for the chosen tweak:\n{screen}");
    assert!(!screen.contains("reflector"), "and for no other one:\n{screen}");
    assert!(h.app().running.is_none(), "asking is not doing");
}

#[test]
fn a_run_confirmed_from_the_palette_starts_the_stand_in_and_nothing_else() {
    let script = Script::new("palette-run", "echo \"$@\"");
    let mut h = harness_running(&script, every_tweak_off());
    focus_tweaks(&mut h);
    run(&mut h, "apply");
    assert!(h.screen().contains("Apply this?"), "{}", h.screen());
    say_yes(&mut h);
    assert!(h.is_focused("run"), "the terminal takes the keys, as it does from the list");

    wait_for_exit(&mut h);
    let screen = h.screen();
    assert!(screen.contains("--run mirrors"), "the stand-in got the chosen tweak:\n{screen}");
    assert!(screen.contains("Done"), "and how it went is said: {screen}");
}

#[test]
fn the_mark_command_checks_the_chosen_tweak_and_the_next_one_unchecks_it() {
    let mut h = harness(100, 24);
    focus_tweaks(&mut h);
    assert!(!marked(&h), "nothing is marked to begin with: {}", h.screen());

    run(&mut h, "check");
    assert!(h.app().checked[0], "the item is checked for a run with others");
    assert!(marked(&h), "and the mark is on screen: {}", h.screen());

    run(&mut h, "uncheck");
    assert!(!h.app().checked[0], "the command says what it will do, so this one unchecks");
    assert!(!marked(&h), "and the mark is gone: {}", h.screen());
}

#[test]
fn the_undo_command_asks_first_for_an_applied_tweak_and_runs_the_revert() {
    let script = Script::new("palette-undo", "echo \"$@\"");
    let mut states = every_tweak_off();
    states[0] = TweakState::Applied;
    // The mirrors are on qtools' record, so the palette offers their undo: a run's own reading
    // afterwards would find the machine off, which is what the stand-in script stands for.
    let mut h = harness_recorded(&script, states, vec![0]);
    focus_tweaks(&mut h);
    // The detail closes, so the package the confirmation names can only have come from the
    // confirmation itself and not from the panel behind it.
    h.press("alt+b");
    run(&mut h, "undo");

    let screen = h.screen();
    assert!(screen.contains("Undo this?"), "undoing asks first:\n{screen}");
    assert!(screen.contains("reflector"), "for the chosen tweak:\n{screen}");
    say_yes(&mut h);
    wait_for_exit(&mut h);
    let screen = h.screen();
    assert!(screen.contains("--revert mirrors"), "the stand-in undid it:\n{screen}");
    assert!(screen.contains("Undoing Mirror list"), "{screen}");
}

#[test]
fn every_group_is_reached_from_the_palette_and_shows_its_own_items() {
    for (label, key, item) in [
        ("Packages", "packages", "Mirror list"),
        ("Security", "security", "Firewall"),
        ("Hardware", "hardware", "Bluetooth"),
        ("Appearance", "appearance", "Fonts"),
        ("Maintenance", "maintenance", "SSD trim"),
    ] {
        let mut h = harness(100, 24);
        run(&mut h, label);
        let group = Group::ALL.into_iter().find(|group| group.key() == key).expect("a group of the catalog");
        assert_eq!(h.app().group, group, "`{label}` from the palette opened that group");
        assert!(h.screen().contains(item), "and the group's own items followed:\n{}", h.screen());
    }
}

#[test]
fn the_settings_command_opens_the_page_where_the_sidebar_entry_does() {
    let mut h = harness(100, 24);
    run(&mut h, "settings");
    let screen = h.screen();
    assert!(!screen.contains("Mirror list"), "the list made way for the page:\n{screen}");
    assert!(screen.contains("Language"), "which is the page's own first row: {screen}");
    assert!(h.app().settings_open, "and the screen knows the page is the one showing");
}

#[test]
fn the_detail_command_hides_the_wide_panel_and_shows_it_again() {
    let mut h = harness(100, 24);
    focus_tweaks(&mut h);
    run(&mut h, "hide");
    assert!(!h.app().panel_open, "the panel closed, as `alt b` closes it");
    assert!(!h.screen().contains("reflector"), "and the detail went with it: {}", h.screen());

    run(&mut h, "show");
    assert!(h.app().panel_open, "and the command now says so, and opened it again");
    assert!(h.screen().contains("reflector"), "{}", h.screen());
}

#[test]
fn on_a_narrow_screen_the_detail_command_opens_and_closes_the_detail_below_the_list() {
    let mut h = harness(48, 20);
    focus_tweaks(&mut h);
    run(&mut h, "show");
    assert!(h.app().detail_open, "the detail is open under the list, not beside it");
    // The last lines of a long detail are below the fold of a twenty-row screen; the list file is
    // one the detail names within it.
    assert!(h.screen().contains("/etc/pacman.d/mirrorlist"), "{}", h.screen());
    // The wide panel's own state is not a narrow screen's to touch: it is read again unchanged when
    // the screen unfolds, which is what keeps folding from disturbing the wide layout.
    assert!(h.app().panel_open, "the wide panel keeps its own state: {}", h.screen());

    run(&mut h, "hide");
    assert!(!h.app().detail_open, "and the detail below the list closed again");
    assert!(!h.screen().contains("/etc/pacman.d/mirrorlist"), "{}", h.screen());
}

#[test]
fn the_quit_command_quits_the_way_the_quit_key_does() {
    let mut h = harness(100, 24);
    run(&mut h, "quit");
    assert!(h.quit_requested(), "the command quits, and asks for nothing first: {}", h.screen());
    assert!(!is_open(&h), "the palette is gone with it");

    let mut key = harness(100, 24);
    key.press("ctrl+q");
    assert!(key.quit_requested(), "the key quits the same way, at once");
}

#[test]
fn a_command_that_cannot_do_anything_now_is_not_listed_at_all() {
    // Apply and check on a tweak that does not fit this machine.
    let mut states = every_tweak_off();
    states[0] = TweakState::Unavailable("unavailable.no-ssd");
    for query in ["apply", "check", "undo"] {
        let mut h = recorded(states.clone(), Vec::new());
        open(&mut h).type_text(query);
        assert!(h.screen().contains(NO_MATCH), "`{query}` fits no machine that has no ssd:\n{}", h.screen());
    }
}

/// The undo is offered for what qtools itself did, not for what merely stands applied: a service
/// the person turned on by hand is on the machine, and there is nothing of qtools' there to undo.
/// The detail panel's own Undo button and the `z` key keep the same rule, and the palette keeps it
/// with them.
#[test]
fn the_undo_is_offered_only_for_a_tweak_qtools_has_on_record() {
    // Applied by hand, nothing on record: the list says applied, and there is no undo to offer.
    let mut states = every_tweak_off();
    states[0] = TweakState::Applied;
    let mut hand = recorded(states.clone(), Vec::new());
    open(&mut hand).type_text("undo");
    assert!(hand.screen().contains(NO_MATCH), "qtools did not do this one, so it offers no undo:\n{}", hand.screen());
    assert!(!hand.screen().contains("Undo"), "and the word is nowhere in the list: {}", hand.screen());

    // The same tweak, this time on qtools' record: the undo is there, with the key that does it.
    let mut mine = recorded(states, vec![0]);
    open(&mut mine).type_text("undo");
    let screen = mine.screen();
    assert!(screen.contains("Undo: Mirror list"), "qtools' own work can be undone:\n{screen}");
    assert!(row_of(&mine, "Undo: Mirror list").contains('z'), "and the `z` key stands beside it: {screen}");
}

/// What the palette and the screen's own Undo button say about one tweak must be the same answer,
/// or one of them offers something the other will not do.
#[test]
fn the_palette_and_the_detail_button_offer_the_undo_for_the_same_tweaks() {
    for (state, on_record) in [
        (TweakState::Off, false),
        (TweakState::Applied, false),
        (TweakState::Applied, true),
        (TweakState::Half(vec![true, false, false, true]), true),
    ] {
        let mut states = every_tweak_off();
        states[0] = state.clone();
        let mut h = recorded(states, if on_record { vec![0] } else { Vec::new() });
        // The detail's own button, read with the palette closed.
        let button = h.screen().contains("Undo");
        open(&mut h).type_text("undo");
        let listed = h.screen().contains("Undo: Mirror list");
        assert_eq!(
            button, listed,
            "`{state:?}` on record `{on_record}`: the button says {button}, the palette {listed}"
        );
    }
}

#[test]
fn the_item_commands_are_not_listed_while_the_settings_page_is_showing() {
    for query in ["apply", "check", "undo", "detail"] {
        let mut h = harness(100, 24);
        h.click_text("Settings");
        assert!(!h.screen().contains("Mirror list"), "the page has the screen: {}", h.screen());
        open(&mut h).type_text(query);
        assert!(h.screen().contains(NO_MATCH), "`{query}` has nothing to act on here:\n{}", h.screen());
    }
}

#[test]
fn the_way_out_of_the_settings_page_and_the_way_to_leave_are_still_listed() {
    let mut h = harness(100, 24);
    h.click_text("Settings");
    open(&mut h).type_text("quit");
    let screen = h.screen();
    assert!(!screen.contains(NO_MATCH), "quitting is there from anywhere:\n{screen}");
    assert!(row_of(&h, "Quit").contains("ctrl q"), "with its key: {screen}");

    let mut h = harness(100, 24);
    h.click_text("Settings");
    run(&mut h, "mainten");
    assert_eq!(h.app().group, Group::Maintenance, "a group is reached from the page too");
    assert!(!h.app().settings_open, "and it takes the page's place: {}", h.screen());
}

#[test]
fn ctrl_k_does_nothing_while_a_runs_terminal_has_the_screen() {
    let script = Script::new("palette-busy", "echo \"$@\"");
    let mut h = harness_running(&script, every_tweak_off());
    focus_tweaks(&mut h);
    h.press("enter");
    say_yes(&mut h);
    assert!(h.app().running.is_some(), "the run is on screen");

    h.press("ctrl+k").advance(ENTERED);
    let screen = h.screen();
    assert!(!is_open(&h), "the palette does not open over a run's terminal:\n{screen}");
    assert!(screen.contains("--run mirrors"), "and the run is untouched: {screen}");
    assert!(!screen.contains("Apply this?"), "nothing is asked of the list behind it either: {screen}");

    wait_for_exit(&mut h);
    h.press("esc");
    assert!(h.app().running.is_none(), "the ended run closed, and the list is back");
    open(&mut h);
    assert!(is_open(&h), "and now the palette opens: {}", h.screen());
}

#[test]
fn ctrl_k_does_nothing_on_the_first_starts_wizard() {
    let root = Root::new();
    let config = root.path().join("config");
    let folders = UpdateFolders { config: config.clone(), state: root.path().join("state") };
    let opening = Opening::new(Some(&config), Some(&root.path().join("fonts")), every_tweak_off(), Cpu::Intel)
        .with_updates(Some(folders));
    let mut h = Harness::with_env(opening.tools, crate::locales::env(), 100, 30);
    h.set_locale("en").set_glyph_mode(GlyphMode::Unicode);
    assert!(h.app().setting_up(), "the wizard has the screen: {}", h.screen());

    h.press("ctrl+k").advance(ENTERED);
    let screen = h.screen();
    assert!(!is_open(&h), "the palette does not open over the wizard:\n{screen}");
    assert!(screen.contains("Before you start"), "and the wizard is untouched: {screen}");
}

#[test]
fn the_header_points_at_the_commands_where_it_has_room_for_them() {
    let wide = harness(100, 24);
    let header = header_of(&wide);
    assert!(header.contains("ctrl k") && header.contains("commands"), "the design's hint is in the header: {header}");

    let narrow = harness(48, 20);
    let header = header_of(&narrow);
    assert!(!header.contains("ctrl k"), "a header that narrow leaves it out rather than cutting it: {header}");
    assert!(header.contains("qtools"), "and keeps the name: {header}");
}

/// The palette with the screen behind it, in every language and at every size, written out to one
/// page for looking at them side by side.
#[test]
fn visual_review_of_the_palette() {
    let mut pages = Vec::new();
    for code in super::languages::codes() {
        for (width, height) in [(100, 24), (48, 20), (80, 14)] {
            let mut h = Harness::with_env(Tools::default(), crate::locales::env(), width, height);
            h.set_locale(&code).set_glyph_mode(GlyphMode::Unicode).set_reduced_motion(true);
            open(&mut h);
            pages.push(h.html(&format!("{code} {width}x{height}, palette")));
        }
    }
    let target = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("target");
    std::fs::create_dir_all(&target).expect("the target directory exists");
    std::fs::write(target.join("palette-review.html"), qframe::runtime::html_page(&pages))
        .expect("the review page is written");
}

// The palette offers the item's own values where the item has any, and the command opens the same
// form the `a` key opens.
#[test]
fn the_palette_opens_the_options_form_of_an_item_that_has_options() {
    let mut h = recorded(super::tests::every_tweak_off(), Vec::new());
    open(&mut h).type_text("options").press("enter").advance(SETTLED);

    let screen = h.screen();
    assert!(screen.contains("Save"), "the options form is open:\n{screen}");
}
