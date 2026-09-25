//! The Settings page: the shared Quvyta appearance rows and its update notice, reached from the
//! sidebar and from the narrow strip, each change in force at once and written where its box says.
//!
//! Every test lives in a temporary root: the shared Quvyta folder, qtools' state folder and the fonts
//! the wizard looks at are all inside it, and the harness answers the update question itself, so
//! nothing reaches the network or the person's own files. The screen is built by
//! [`Opening::new`], the way [`crate::run_on`] builds it.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::time::Duration;

use qframe::icons::GlyphMode;
use qframe::prelude::*;
use qframe::storage::Ecosystem;

use super::tests::focus_tweaks;
use super::wizard::tests::Root;
use super::{Opening, Tools, UpdateFolders};
use crate::catalog;
use crate::tweak::{Group, TweakState};

/// Long enough for a write in the background to come back and a notice to settle in.
const MOMENT: Duration = Duration::from_millis(300);

/// The update notice's row, by the text a person clicks.
const NOTICE: &str = "Say when an update is out";

fn folders(root: &Path) -> UpdateFolders {
    UpdateFolders { config: root.join("config"), state: root.join("state") }
}

/// qtools set up before, with a shared Quvyta language of `language`: no wizard, and every shared
/// key of `tools.conf` follows the shared file.
fn set_up_in(root: &Path, language: &str) {
    fs::create_dir_all(root.join("config")).expect("folder");
    let shared_look = format!("language = \"{language}\"\ntheme = \"monochrome\"\nicons = \"unicode\"\n");
    fs::write(root.join("config/quvyta.conf"), shared_look).expect("shared file");
    fs::write(root.join("config/tools.conf"), "language = \"quvyta\"\ntheme = \"quvyta\"\nicons = \"quvyta\"\n")
        .expect("own file");
}

/// qtools over the Quvyta folder of `root`, on a screen of `width` by `height`, started the way the
/// runtime starts it: the language and theme [`Opening::new`] resolved are those of the first
/// frame.
fn start(root: &Path, width: u16, height: u16) -> Harness<Tools> {
    let states = vec![TweakState::Off; catalog::all().len()];
    let opening =
        Opening::new(Some(&root.join("config")), Some(&root.join("fonts")), states).with_updates(Some(folders(root)));
    let language = opening.preferences.language().value.clone();
    let theme = opening.preferences.theme().value.clone();
    let mut h = Harness::with_env(opening.tools, crate::locales::env(), width, height);
    h.set_theme(&theme).set_locale(&language).set_glyph_mode(GlyphMode::Unicode).set_reduced_motion(true);
    h
}

/// qtools in English, set up before, on a wide screen.
fn english(root: &Path) -> Harness<Tools> {
    set_up_in(root, "en");
    start(root, 100, 40)
}

fn read(root: &Path, name: &str) -> String {
    fs::read_to_string(root.join("config").join(name)).unwrap_or_default()
}

/// Whether the settings page has the screen: its update row stands where the list stood.
fn on_settings(h: &Harness<Tools>) -> bool {
    let screen = h.screen();
    screen.contains(NOTICE) && !screen.contains("Mirror list")
}

/// A language qtools speaks that this machine would not choose by itself, so a screen in it can
/// only come from the change just made.
fn not_detected() -> String {
    let detected = crate::locales::i18n().detect(|name| std::env::var(name).ok());
    ["de", "fr"].into_iter().find(|code| detected.as_deref() != Some(*code)).unwrap_or("de").to_owned()
}

/// `key` in the language `code`.
fn said_in(code: &str, key: &str) -> String {
    let mut i18n = crate::locales::env().i18n().clone();
    assert!(i18n.set_active(code), "`{code}` is a known language");
    i18n.translate(key, &[])
}

/// The name a language is listed under in the language row.
fn language_name(code: &str) -> String {
    let list = crate::locales::env().i18n().list();
    list.into_iter().find(|(listed, _)| listed == code).map(|(_, name)| name).expect("a listed language")
}

#[test]
fn settings_opens_from_the_sidebar_by_click_and_a_group_comes_back_with_its_selection() {
    let root = Root::new();
    let mut h = english(root.path());
    focus_tweaks(&mut h);
    h.press("down").press("down");
    assert_eq!(h.app().selected, 2);

    h.click_text("Settings");
    assert!(on_settings(&h), "the page replaces the list and its detail:\n{}", h.screen());
    let screen = h.screen();
    for text in ["Appearance", "Language", "Theme", "Icons", "In every Quvyta application", "Reduce motion", "Pillar"] {
        assert!(screen.contains(text), "`{text}` is on the page:\n{screen}");
    }
    assert!(!screen.contains("reflector"), "no tweak's detail beside it:\n{screen}");

    h.click_text("Packages");
    let screen = h.screen();
    assert!(screen.contains("Mirror list") && !screen.contains(NOTICE), "the list is back:\n{screen}");
    assert_eq!(h.app().group, Group::Packages);
    assert_eq!(h.app().selected, 2, "the tweak chosen before is still chosen");
    assert!(screen.matches("Multilib repository").count() >= 2, "and its detail is beside the list:\n{screen}");
}

#[test]
fn settings_opens_from_the_sidebar_by_keyboard() {
    let root = Root::new();
    let mut h = english(root.path());
    for _ in 0..5 {
        if h.is_focused("groups") {
            break;
        }
        h.press("tab");
    }
    assert!(h.is_focused("groups"), "{}", h.screen());
    h.press("end").press("enter");
    assert!(on_settings(&h), "the last entry of the sidebar is Settings:\n{}", h.screen());
    h.press("home").press("enter");
    assert!(h.screen().contains("Mirror list"), "the first group is back:\n{}", h.screen());
}

#[test]
fn settings_is_the_last_tab_of_the_narrow_strip() {
    let root = Root::new();
    set_up_in(root.path(), "en");
    let mut h = start(root.path(), 48, 30);
    for _ in 0..5 {
        if h.is_focused("groups") {
            break;
        }
        h.press("tab");
    }
    assert!(h.is_focused("groups"), "{}", h.screen());
    for _ in Group::ALL {
        h.press("right");
    }
    assert!(on_settings(&h), "right of the last group is Settings:\n{}", h.screen());
    h.press("left");
    assert_eq!(h.app().group, Group::Maintenance);
    assert!(h.screen().contains("Journal size limit"), "the group left of it:\n{}", h.screen());

    // A narrow strip shows a few tabs at a time; its arrow brings the last one into view.
    h.click_text("▶");
    h.click_text("Settings");
    assert!(on_settings(&h), "a click on the tab opens it too:\n{}", h.screen());
    h.click_text("Maintenance");
    assert!(h.screen().contains("Journal size limit"), "{}", h.screen());
}

#[test]
fn a_theme_chosen_for_every_quvyta_app_is_in_force_at_once_and_written_to_the_shared_file() {
    let root = Root::new();
    let mut h = english(root.path());
    h.click_text("Settings");
    h.click_text("Monochrome");
    h.click_text("Nordic");
    h.advance(MOMENT);
    assert_eq!(h.env().theme().id(), "nordic", "qtools draws in it at once");
    let shared = read(root.path(), "quvyta.conf");
    assert!(shared.contains("theme = \"nordic\""), "the box is checked, so the shared file takes it:\n{shared}");
    assert!(shared.contains("language = \"en\""), "the other shared keys stay:\n{shared}");
    let own = read(root.path(), "tools.conf");
    assert!(own.contains("theme = \"quvyta\""), "qtools keeps following the shared file:\n{own}");
    assert!(!own.contains("nordic"), "{own}");
}

#[test]
fn a_language_kept_to_qtools_is_in_force_at_once_and_written_to_its_own_file() {
    let root = Root::new();
    let mut h = english(root.path());
    h.click_text("Settings");
    // The box under the language row, reached the way a person reaches it: its label, then down.
    h.click_text("Language");
    h.press("down").press("space");
    h.advance(MOMENT);
    let code = not_detected();
    h.click_text("English");
    h.click_text(&language_name(&code));
    h.advance(MOMENT);
    assert_eq!(h.env().i18n().active(), code, "qtools speaks it at once");
    let group = said_in(&code, "group.packages");
    assert!(h.screen().contains(&group), "`{group}`: the sidebar is in it already:\n{}", h.screen());
    let own = read(root.path(), "tools.conf");
    assert!(own.contains(&format!("language = \"{code}\"")), "only qtools takes it:\n{own}");
    let shared = read(root.path(), "quvyta.conf");
    assert!(shared.contains("language = \"en\""), "the shared file keeps its language:\n{shared}");

    // The next start speaks it too.
    let again = start(root.path(), 100, 40);
    assert!(again.screen().contains(&group), "{}", again.screen());
}

#[test]
fn the_update_notice_row_turns_the_quvyta_wide_switch_off_and_on_again() {
    let root = Root::new();
    let mut h = english(root.path());
    assert_eq!(h.update_checks().len(), 1, "on, it asks at start");
    let config = root.path().join("config");
    h.click_text("Settings");
    h.click_text(NOTICE);
    h.press("space");
    h.advance(MOMENT);
    assert!(!Ecosystem::QUVYTA.update_notice_in(&config), "the shared file says off");
    assert!(read(root.path(), "quvyta.conf").contains("update-notice = false"));
    assert!(!read(root.path(), "tools.conf").contains("update-notice"), "the switch is Quvyta-wide");
    let next = start(root.path(), 100, 40);
    assert!(next.update_checks().is_empty(), "the next start asks nothing");

    h.press("space");
    h.advance(MOMENT);
    assert!(Ecosystem::QUVYTA.update_notice_in(&config), "on again");
    let next = start(root.path(), 100, 40);
    assert_eq!(next.update_checks().len(), 1, "and the next start asks");
}

#[test]
fn a_folder_that_cannot_be_written_keeps_the_change_and_the_row_says_why() {
    // The framework's behaviour, the same in every Quvyta application: the change stays in force
    // and the row it was made on says it was not saved, and why.
    let root = Root::new();
    let mut h = english(root.path());
    let config = root.path().join("config");
    let before = (read(root.path(), "quvyta.conf"), read(root.path(), "tools.conf"));
    h.click_text("Settings");
    fs::set_permissions(&config, fs::Permissions::from_mode(0o555)).expect("read-only");
    h.click_text("Monochrome");
    h.click_text("Nordic");
    h.advance(MOMENT);
    let screen = h.screen();
    let theme = h.env().theme().id().to_owned();
    let after = (read(root.path(), "quvyta.conf"), read(root.path(), "tools.conf"));
    fs::set_permissions(&config, fs::Permissions::from_mode(0o755)).expect("writable again");
    assert_eq!(theme, "nordic", "applied all the same:\n{screen}");
    assert!(screen.contains("Applied, but not saved"), "the row says so:\n{screen}");
    assert!(screen.contains("Permission denied"), "and why:\n{screen}");
    assert_eq!(after, before, "nothing was written");
}

#[test]
fn a_notice_switch_that_cannot_be_written_goes_back_on() {
    let root = Root::new();
    let mut h = english(root.path());
    let config = root.path().join("config");
    h.click_text("Settings");
    h.click_text(NOTICE);
    fs::set_permissions(&config, fs::Permissions::from_mode(0o555)).expect("read-only");
    h.press("space");
    h.advance(MOMENT);
    let screen = h.screen();
    fs::set_permissions(&config, fs::Permissions::from_mode(0o755)).expect("writable again");
    assert!(screen.contains("could not be saved"), "{screen}");
    assert!(Ecosystem::QUVYTA.update_notice_in(&config), "the shared file still says on");
    // Put back on, the next press turns it off rather than on again.
    h.press("space");
    h.advance(MOMENT);
    assert!(!Ecosystem::QUVYTA.update_notice_in(&config), "the switch was back on, so this turned it off");
}

#[test]
fn the_tweak_keys_do_nothing_on_the_settings_page() {
    let root = Root::new();
    let mut h = english(root.path());
    focus_tweaks(&mut h);
    h.click_text("Settings");
    for key in ["space", "enter", "z"] {
        h.press(key);
        let screen = h.screen();
        assert!(!screen.contains("Apply this?") && !screen.contains("Undo this?"), "`{key}` asks nothing:\n{screen}");
    }
    // On the rows themselves too.
    h.press("tab").press("z");
    assert!(!h.screen().contains("Undo this?"), "`z` on the rows asks nothing:\n{}", h.screen());
    assert!(h.app().checked.iter().all(|checked| !checked), "nothing was checked");
    assert!(h.app().running.is_none());
    let hints = h.screen().lines().last().unwrap_or_default().to_owned();
    for hint in ["apply", "check", "undo"] {
        assert!(!hints.contains(hint), "the footer offers no `{hint}` here: {hints}");
    }
}

#[test]
fn after_the_wizard_the_page_carries_on_from_what_it_chose() {
    let root = Root::new();
    fs::create_dir_all(root.path().join("config")).expect("folder");
    fs::write(
        root.path().join("config/quvyta.conf"),
        "language = \"en\"\ntheme = \"monochrome\"\nicons = \"unicode\"\n",
    )
    .expect("shared file");
    let mut h = start(root.path(), 100, 40);
    assert!(h.app().setting_up());
    // The shared look is answered in full, so the wizard opens on qtools' own step.
    h.click_text(NOTICE);
    h.click_text("Finish");
    h.advance(MOMENT);
    let config = root.path().join("config");
    assert!(!Ecosystem::QUVYTA.update_notice_in(&config), "the wizard turned it off");
    h.click_text("Settings");
    h.click_text(NOTICE);
    h.press("space");
    h.advance(MOMENT);
    assert!(Ecosystem::QUVYTA.update_notice_in(&config), "the page knew it was off, so space turned it on");
}

/// The Settings entry of the sidebar in a member harness. `Harness::member_in` starts from the
/// framework's own language files, so qtools' strings show as their keys; the click still lands
/// where a person's would.
const SETTINGS_ENTRY: &str = "⟦settings.title⟧";

#[test]
fn a_theme_another_quvyta_application_gives_qtools_while_it_is_open_is_where_the_next_pick_goes() {
    use qframe::storage::{Scope, Shared};
    let root = Root::new();
    set_up_in(root.path(), "en");
    let folder = root.path().join("config");
    let states = vec![TweakState::Off; catalog::all().len()];
    let opening = Opening::new(Some(&folder), Some(&root.path().join("fonts")), states);
    let mut h = Harness::member_in(opening.tools, Ecosystem::QUVYTA, &folder, super::APP, 100, 40);
    h.set_glyph_mode(GlyphMode::Unicode).set_reduced_motion(true);
    h.click_text(SETTINGS_ENTRY);
    assert!(h.screen().contains("Monochrome"), "{}", h.screen());
    // Another member, the launcher say, gives qtools a theme of its own in qtools' file.
    Ecosystem::QUVYTA.set_in(&folder, super::APP, Shared::Theme, "iris", Scope::App).expect("saved");
    h.poll_preferences();
    h.advance(MOMENT);
    assert_eq!(h.env().theme().id(), "iris", "the screen follows");
    assert!(h.screen().contains("Iris"), "and the page says so:\n{}", h.screen());
    // qtools keeps its own theme now, so the next one picked here stays with qtools.
    h.click_text("Iris");
    h.click_text("Nordic");
    h.advance(MOMENT);
    assert_eq!(h.env().theme().id(), "nordic", "{}", h.screen());
    let own = read(root.path(), "tools.conf");
    assert!(own.contains("theme = \"nordic\""), "{own}");
    let shared = read(root.path(), "quvyta.conf");
    assert!(shared.contains("theme = \"monochrome\""), "the other applications keep theirs:\n{shared}");
}
