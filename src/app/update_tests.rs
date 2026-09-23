//! The family's update notice: qtools asks once at start whether a newer version is out, says so
//! with the family's notice, asks nothing while the switch is off or the first-run wizard is open,
//! and the wizard's box is where the switch is turned off.
//!
//! Every test lives in a temporary root: the family's folder, qtools' state folder and the fonts
//! the wizard looks at are all inside it, and the harness answers the question itself, so nothing
//! reaches the network or the person's own files.

use std::fs;
use std::path::Path;
use std::time::Duration;

use qframe::icons::GlyphMode;
use qframe::prelude::*;
use qframe::storage::Family;

use super::tests::{Script, every_tweak_off, focus_tweaks, say_yes, wait_for_exit};
use super::wizard::tests::Root;
use super::{Opening, Tools, UpdateFolders};
use crate::catalog;
use crate::tweak::TweakState;

/// Long enough for an answered question to come back and its toast to settle in.
const MOMENT: Duration = Duration::from_millis(300);

/// The box on the wizard's own step, by the text a person clicks.
const BOX: &str = "Say when an update is out";

fn folders(root: &Path) -> UpdateFolders {
    UpdateFolders { config: root.join("config"), state: root.join("state") }
}

/// A family whose look is already English, so the screen can be read in English whatever the
/// machine's own language is.
fn family_in_english(root: &Path) {
    fs::create_dir_all(root.join("config")).expect("folder");
    fs::write(root.join("config/quvyta.conf"), "language = \"en\"\ntheme = \"monochrome\"\nicons = \"unicode\"\n")
        .expect("family file");
}

/// qtools has been set up before: no wizard.
fn set_up_before(root: &Path) {
    family_in_english(root);
    fs::write(root.join("config/tools.conf"), "language = \"quvyta\"\ntheme = \"quvyta\"\nicons = \"quvyta\"\n")
        .expect("own file");
}

/// qtools over the family of `root`, built the way [`crate::run_on`] builds it.
fn start(root: &Path) -> Harness<Tools> {
    let states = vec![TweakState::Off; catalog::all().len()];
    let opening =
        Opening::new(Some(&root.join("config")), Some(&root.join("fonts")), states).with_updates(Some(folders(root)));
    let mut h = Harness::with_env(opening.tools, crate::locales::env(), 100, 30);
    h.set_locale("en").set_glyph_mode(GlyphMode::Unicode).set_reduced_motion(true);
    h
}

fn said_out(h: &Harness<Tools>) -> bool {
    h.screen().contains("is out")
}

#[test]
fn a_newer_version_is_said_in_the_familys_notice_and_the_same_or_an_older_one_is_not() {
    let root = Root::new();
    set_up_before(root.path());
    let mut h = start(root.path());
    let asked = h.update_checks().to_vec();
    assert_eq!(asked.len(), 1, "asked once at start:\n{}", h.screen());
    assert_eq!((asked[0].package(), asked[0].current()), ("quvyta-tools", env!("CARGO_PKG_VERSION")));
    assert!(!said_out(&h), "nothing is said before the answer");

    h.set_latest_version(Some("9.9.9")).advance(MOMENT);
    let screen = h.screen();
    assert!(screen.contains("quvyta-tools 9.9.9 is out"), "the notice names the new version:\n{screen}");
    assert!(screen.contains(env!("CARGO_PKG_VERSION")), "and the one running:\n{screen}");

    for latest in [env!("CARGO_PKG_VERSION"), "0.0.1"] {
        let mut same = start(root.path());
        same.set_latest_version(Some(latest)).advance(MOMENT);
        assert!(!said_out(&same), "{latest} is not newer:\n{}", same.screen());
    }
}

#[test]
fn with_the_familys_switch_off_nothing_is_asked() {
    let root = Root::new();
    set_up_before(root.path());
    Family::QUVYTA.set_update_notice_in(&root.path().join("config"), false).expect("saved");
    let mut h = start(root.path());
    assert!(h.update_checks().is_empty(), "off asks nothing at all");
    h.set_latest_version(Some("9.9.9")).advance(MOMENT);
    assert!(!said_out(&h), "{}", h.screen());
}

#[test]
fn without_the_folders_nothing_is_asked() {
    let root = Root::new();
    set_up_before(root.path());
    let states = vec![TweakState::Off; catalog::all().len()];
    let opening = Opening::new(Some(&root.path().join("config")), None, states).with_updates(None);
    let h = Harness::with_env(opening.tools, crate::locales::env(), 100, 30);
    assert!(h.update_checks().is_empty());
}

#[test]
fn the_unsupported_notice_asks_nothing() {
    let h = Harness::with_env(super::unsupported::Unsupported, crate::locales::env(), 100, 30);
    assert!(h.update_checks().is_empty());
}

#[test]
fn nothing_is_asked_while_the_wizard_is_open_and_once_after_finish() {
    let root = Root::new();
    family_in_english(root.path());
    let mut h = start(root.path());
    assert!(h.app().setting_up(), "{}", h.screen());
    assert!(h.update_checks().is_empty(), "the wizard is not interrupted by the question");
    h.click_text("Next");
    assert!(h.screen().contains(BOX), "the box is on the wizard's own step:\n{}", h.screen());
    assert!(h.update_checks().is_empty());
    h.click_text("Finish");
    h.render();
    assert!(!h.app().setting_up(), "{}", h.screen());
    assert_eq!(h.update_checks().len(), 1, "asked once the wizard is over");
    h.set_latest_version(Some("9.9.9")).advance(MOMENT);
    assert!(h.screen().contains("quvyta-tools 9.9.9 is out"), "{}", h.screen());
}

#[test]
fn start_with_the_defaults_asks_once_it_is_over() {
    let root = Root::new();
    family_in_english(root.path());
    let mut h = start(root.path());
    h.click_text("Start with the defaults");
    h.render();
    assert!(!h.app().setting_up(), "{}", h.screen());
    assert_eq!(h.update_checks().len(), 1);
}

#[test]
fn unchecking_the_box_turns_the_switch_off_for_the_family_on_finish_and_asks_nothing() {
    let root = Root::new();
    family_in_english(root.path());
    let before = fs::read_to_string(root.path().join("config/quvyta.conf")).expect("family file");
    let mut h = start(root.path());
    h.click_text("Next");
    h.click_text(BOX);
    assert_eq!(
        fs::read_to_string(root.path().join("config/quvyta.conf")).expect("family file"),
        before,
        "nothing is written before Finish"
    );
    assert!(!root.path().join("config/tools.conf").exists());
    h.click_text("Finish");
    h.render();
    h.advance(MOMENT);
    assert!(!h.app().setting_up(), "{}", h.screen());
    let config = root.path().join("config");
    assert!(!Family::QUVYTA.update_notice_in(&config), "the family's file says off");
    let shared = fs::read_to_string(config.join("quvyta.conf")).expect("family file");
    assert!(shared.contains("update-notice = false"), "{shared}");
    assert!(h.update_checks().is_empty(), "the person said no: nothing is asked");
    h.set_latest_version(Some("9.9.9")).advance(MOMENT);
    assert!(!said_out(&h), "{}", h.screen());

    let again = start(root.path());
    assert!(!again.app().setting_up());
    assert!(again.update_checks().is_empty(), "and the next start asks nothing either");
}

#[test]
fn leaving_the_box_on_keeps_the_switch_on() {
    let root = Root::new();
    family_in_english(root.path());
    let mut h = start(root.path());
    h.click_text("Next");
    h.click_text("Finish");
    h.render();
    h.advance(MOMENT);
    let config = root.path().join("config");
    assert!(Family::QUVYTA.update_notice_in(&config));
    let shared = fs::read_to_string(config.join("quvyta.conf")).expect("family file");
    assert!(!shared.contains("update-notice = false"), "{shared}");
    assert_eq!(h.update_checks().len(), 1);
}

#[test]
fn the_box_starts_as_the_family_left_the_switch() {
    // Another member turned it off; the wizard shows it off, and finishing untouched keeps it so.
    let root = Root::new();
    family_in_english(root.path());
    let config = root.path().join("config");
    Family::QUVYTA.set_update_notice_in(&config, false).expect("saved");
    let mut h = start(root.path());
    h.click_text("Next");
    h.click_text("Finish");
    h.render();
    h.advance(MOMENT);
    assert!(!Family::QUVYTA.update_notice_in(&config));
    assert!(h.update_checks().is_empty());

    // Checked by hand, it turns the switch back on for the family, and the question follows.
    let root = Root::new();
    family_in_english(root.path());
    let config = root.path().join("config");
    Family::QUVYTA.set_update_notice_in(&config, false).expect("saved");
    let mut h = start(root.path());
    h.click_text("Next");
    h.click_text(BOX);
    h.click_text("Finish");
    h.render();
    h.advance(MOMENT);
    assert!(Family::QUVYTA.update_notice_in(&config), "turned back on");
    assert_eq!(h.update_checks().len(), 1);
}

#[test]
fn a_notice_that_comes_during_a_run_waits_until_the_run_is_closed() {
    // The toast would float over the terminal's bottom corner, where its note gives the exit code.
    let root = Root::new();
    set_up_before(root.path());
    let script = Script::new("update-notice", "echo \"$@\"");
    let tools =
        Tools::new(every_tweak_off()).machine(script.0.clone(), every_tweak_off).updates(Some(folders(root.path())));
    let mut h = Harness::with_env(tools, crate::locales::env(), 100, 24);
    h.set_locale("en").set_glyph_mode(GlyphMode::Unicode).set_reduced_motion(true);
    assert_eq!(h.update_checks().len(), 1);
    focus_tweaks(&mut h);
    h.press("enter");
    say_yes(&mut h);
    assert!(h.app().running.is_some());
    h.set_latest_version(Some("9.9.9")).advance(MOMENT);
    assert!(!said_out(&h), "not over a running terminal:\n{}", h.screen());
    wait_for_exit(&mut h);
    assert!(!said_out(&h), "nor over one that ended and still shows its note:\n{}", h.screen());
    h.press("esc");
    h.advance(MOMENT);
    assert!(h.app().running.is_none());
    assert!(h.screen().contains("quvyta-tools 9.9.9 is out"), "said once the list is back:\n{}", h.screen());
}
