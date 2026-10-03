//! The options form on the real screen: opened by `a` and by the button in the detail panel,
//! typed into with key presses, saved with the Save button — and the value that comes out of it
//! showing up in the file, in the detail panel and in the steps a run would carry out.
//!
//! Every test lives in a temporary root: the shared Quvyta folder, qtools' state folder and the
//! fonts the wizard looks at are all inside it, so nothing reaches the network or the person's
//! own files, and runs start a stand-in script. The screen is built by [`Opening::new`], the way
//! [`crate::run_on`] builds it, so the wiring the real start goes through is the wiring under
//! test.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::time::Duration;

use qframe::icons::GlyphMode;
use qframe::prelude::*;
use qframe::text;

use super::languages::reads_whole;
use super::tests::{Script, every_tweak_off, focus_tweaks, reading_off, say_yes, wait_for_exit};
use super::wizard::tests::Root;
use super::{Opening, Tools};
use crate::catalog;
use crate::catalog::hardware::Cpu;
use crate::choices::Choices;
use crate::step::Step;
use crate::tweak::{Tweak, TweakState};

/// Long enough for a write in the background to come back and the states to be read again.
const MOMENT: Duration = Duration::from_millis(300);

/// The save button of the form, as a person finds it.
const SAVE: &str = "Save";

/// The button under the options, and the title of the form it opens.
const OPTIONS: &str = "Options";

/// The file the mirror list's country and count reach: the one `reflector.timer` reads every week,
/// so a country chosen in the form has to be in the file it writes, not only in the first run's
/// command line.
const REFLECTOR_CONF: &str = "/etc/xdg/reflector/reflector.conf";

/// qtools set up before, in English, with the values a person has already chosen: they are in the
/// file, so every test starts from a known value rather than from the machine's own locale.
fn set_up_in(root: &Path) {
    fs::create_dir_all(root.join("config")).expect("folder");
    let shared_look = "language = \"en\"\ntheme = \"monochrome\"\nicons = \"unicode\"\n";
    fs::write(root.join("config/quvyta.conf"), shared_look).expect("shared file");
    let own = "language = \"quvyta\"\ntheme = \"quvyta\"\nicons = \"quvyta\"\n\
               \n[option.mirrors]\ncountry = \"TR\"\ncount = 10\n\
               \n[option.aur]\nhelper = \"paru\"\n";
    fs::write(root.join("config/tools.conf"), own).expect("own file");
}

/// qtools over the Quvyta folder of `root`, on a screen of `width` by `height`, started the way
/// the runtime starts it: the language and theme [`Opening::new`] resolved are those of the first
/// frame.
fn start(root: &Path, width: u16, height: u16) -> Harness<Tools> {
    let states = vec![TweakState::Off; catalog::all(Cpu::Intel).len()];
    let opening = Opening::new(Some(&root.join("config")), Some(&root.join("fonts")), states, Cpu::Intel)
        .with_updates(Some(folders(root)));
    let language = opening.preferences.language().value.clone();
    let theme = opening.preferences.theme().value.clone();
    let mut h = Harness::with_env(opening.tools, crate::locales::env(), width, height);
    h.set_theme(&theme).set_locale(&language).set_glyph_mode(GlyphMode::Unicode).set_reduced_motion(true);
    h
}

fn folders(root: &Path) -> super::UpdateFolders {
    super::UpdateFolders { config: root.join("config"), state: root.join("state") }
}

/// qtools in English, set up before, on a wide screen with the tweaks list focused.
fn english(root: &Path) -> Harness<Tools> {
    set_up_in(root);
    let mut h = start(root, 100, 40);
    focus_tweaks(&mut h);
    h
}

/// `tools.conf` as it is on disk.
fn read(root: &Path) -> String {
    fs::read_to_string(root.join("config/tools.conf")).unwrap_or_default()
}

/// The tweak as a run would build it out of the file the form wrote: the same file, read the same
/// way, so this is what `--run` carries out.
fn built_from_the_file(root: &Path, id: &str) -> Tweak {
    let choices = crate::choices_in(Some(&root.join("config")));
    catalog::with(&choices, Cpu::Intel).into_iter().find(|tweak| tweak.id == id).unwrap_or_else(|| panic!("{id}"))
}

/// What the mirror list's step would write into reflector's own configuration, the file its timer
/// reads: the country and the count a person chose are in there.
fn reflector_conf_of(tweak: &Tweak) -> String {
    let Step::FileReplaced { path: _, content } = tweak
        .steps
        .iter()
        .find(|step| matches!(step, Step::FileReplaced { path, .. } if *path == REFLECTOR_CONF))
        .unwrap_or_else(|| panic!("{} writes reflector's configuration", tweak.id))
    else {
        unreachable!("found by that very match")
    };
    content.clone()
}

/// The command of the step of `tweak` that runs one, as the machine would be asked to run it.
fn command_of(tweak: &Tweak) -> String {
    tweak
        .steps
        .iter()
        .find_map(|step| match step {
            Step::CommandRun { run, .. } | Step::FileRewritten { run, .. } => Some(run.argv().join(" ")),
            _ => None,
        })
        .unwrap_or_else(|| panic!("{} runs a command", tweak.id))
}

/// A value a person typed over the one that was there: the field is selected when it takes the
/// keyboard, so typing replaces it.
fn retype(h: &mut Harness<Tools>, text: &str) {
    h.press("ctrl+a").type_text(text);
}

#[test]
fn a_opens_the_options_form_with_the_current_values_in_it() {
    let root = Root::new();
    let mut h = english(root.path());
    assert!(h.screen().contains("TR"), "the detail lists the value in force:\n{}", h.screen());
    h.press("a");
    let screen = h.screen();
    assert!(screen.contains("Country") && screen.contains("TR"), "the country is in the form:\n{screen}");
    assert!(screen.contains("Mirrors") && screen.contains("10"), "and the count of mirrors:\n{screen}");
    assert!(screen.contains(SAVE), "Save is offered:\n{screen}");
    assert!(h.is_focused(crate::choices::COUNTRY), "the first field takes the keyboard:\n{screen}");
}

#[test]
fn the_keyboard_reaches_save_on_its_own_and_saves() {
    // The dialog holds the keyboard while it is open: `tab` walks its own widgets, the fields
    // first and then the two buttons, and `enter` presses the one it is on. So the form is
    // filled in and saved without the mouse ever being used.
    let root = Root::new();
    let mut h = english(root.path());
    h.press("a");
    h.press("tab");
    assert!(h.is_focused(crate::choices::MIRRORS), "the count is the second field:\n{}", h.screen());
    retype(&mut h, "25");
    h.press("tab").press("tab");
    h.press("enter");
    h.advance(MOMENT);

    let screen = h.screen();
    assert!(!screen.contains(SAVE), "`enter` on the last widget of the dialog saved it:\n{screen}");
    assert!(read(root.path()).contains("count = 25"), "and the value is in the file: {}", read(root.path()));
    assert!(screen.contains("25"), "the detail panel shows it at once:\n{screen}");
}

#[test]
fn the_button_under_the_options_opens_the_same_form() {
    let root = Root::new();
    let mut h = english(root.path());
    h.click_text(OPTIONS);
    let screen = h.screen();
    assert!(screen.contains("Country") && screen.contains("TR"), "the mouse opens the form too:\n{screen}");
    assert!(h.is_focused(crate::choices::COUNTRY), "{screen}");
}

#[test]
fn a_country_chosen_in_the_form_reaches_the_file_the_mirror_timer_reads() {
    let root = Root::new();
    let mut h = english(root.path());
    h.press("a");
    retype(&mut h, "Germany");
    h.click_text(SAVE);
    h.advance(MOMENT);
    let own = read(root.path());
    assert!(own.contains("country = \"Germany\""), "the value is in the file:\n{own}");
    let screen = h.screen();
    assert!(screen.contains("Germany"), "the detail panel shows it at once:\n{screen}");
    assert!(!screen.contains("TR"), "and not the value it replaced:\n{screen}");

    let built = built_from_the_file(root.path(), "mirrors");
    let conf = reflector_conf_of(&built);
    assert!(conf.lines().any(|line| line == "--country Germany"), "the step writes it into that file:\n{conf}");
    assert_eq!(
        command_of(&built),
        "sudo reflector @/etc/xdg/reflector/reflector.conf",
        "and the list is made from that same file, so the country is in force a week later too"
    );
}

#[test]
fn a_mirror_count_chosen_in_the_form_reaches_the_file_the_mirror_timer_reads() {
    let root = Root::new();
    let mut h = english(root.path());
    h.press("a");
    h.press("tab");
    assert!(h.is_focused(crate::choices::MIRRORS), "the count is the second field:\n{}", h.screen());
    retype(&mut h, "25");
    h.click_text(SAVE);
    h.advance(MOMENT);
    assert!(read(root.path()).contains("count = 25"), "{}", read(root.path()));
    assert!(h.screen().contains("25"), "the detail panel shows it:\n{}", h.screen());

    let conf = reflector_conf_of(&built_from_the_file(root.path(), "mirrors"));
    assert!(conf.lines().any(|line| line == "--latest 25"), "the step writes it into that file:\n{conf}");
    assert!(conf.lines().any(|line| line == "--country TR"), "and the country it did not change is kept:\n{conf}");
}

#[test]
fn parallel_downloads_chosen_in_the_form_reach_what_pacman_is_told() {
    let root = Root::new();
    let mut h = english(root.path());
    h.press("down");
    h.press("a");
    retype(&mut h, "8");
    h.click_text(SAVE);
    h.advance(MOMENT);
    assert!(read(root.path()).contains("parallel = 8"), "{}", read(root.path()));
    let Step::ConfOption { key, value, .. } = &built_from_the_file(root.path(), "pacman-options").steps[0] else {
        panic!("the first step sets an option");
    };
    assert_eq!((*key, value.as_str()), ("ParallelDownloads", "8"));
}

#[test]
fn the_helper_chosen_in_the_form_is_the_one_that_is_built() {
    let root = Root::new();
    let mut h = english(root.path());
    h.press("down").press("down").press("down");
    h.press("a");
    let screen = h.screen();
    assert!(screen.contains("paru") && screen.contains("yay"), "both helpers are offered:\n{screen}");
    h.press("right");
    h.click_text(SAVE);
    h.advance(MOMENT);
    assert!(read(root.path()).contains("helper = \"yay\""), "{}", read(root.path()));
    assert!(h.screen().contains("yay"), "the detail panel says which one:\n{}", h.screen());
    let built = command_of(&built_from_the_file(root.path(), "aur-helper"));
    assert!(built.contains("aur.archlinux.org/yay.git"), "the chosen helper is the one built:\n{built}");
    assert!(!built.contains("paru"), "and the other one nowhere:\n{built}");
}

#[test]
fn the_journal_size_chosen_in_the_form_reaches_the_drop_in_that_is_written() {
    let root = Root::new();
    let mut h = english(root.path());
    h.click_text("Maintenance");
    focus_tweaks(&mut h);
    h.press("down");
    h.press("a");
    retype(&mut h, "1000");
    h.click_text(SAVE);
    h.advance(MOMENT);
    assert!(read(root.path()).contains("max-use = 1000"), "{}", read(root.path()));
    let Step::FileManaged { content, .. } = &built_from_the_file(root.path(), "journal-limit").steps[0] else {
        panic!("the first step writes the drop-in");
    };
    assert_eq!(content, "[Journal]\nSystemMaxUse=1000M\n");
}

#[test]
fn a_count_outside_its_range_is_refused_with_the_range_and_nothing_is_written() {
    let root = Root::new();
    let mut h = english(root.path());
    h.press("a");
    h.press("tab");
    retype(&mut h, "900");
    h.click_text(SAVE);
    h.advance(MOMENT);
    let before = read(root.path());
    let screen = h.screen();
    assert!(reads_whole(&screen, "Enter a number between 1 and 50."), "the field says what it allows:\n{screen}");
    assert_eq!(read(root.path()), before, "nothing was written");
    assert!(h.is_focused(crate::choices::MIRRORS), "the keyboard goes to the field that failed:\n{screen}");
}

#[test]
fn a_country_that_is_not_one_reflector_takes_is_refused_and_nothing_is_written() {
    let root = Root::new();
    let mut h = english(root.path());
    h.press("a");
    retype(&mut h, "T;rm");
    let before = read(root.path());
    h.click_text(SAVE);
    h.advance(MOMENT);
    let screen = h.screen();
    assert!(reads_whole(&screen, "Use 2 to 60 letters"), "the field says what a country may hold:\n{screen}");
    assert_eq!(read(root.path()), before, "nothing was written");
    assert!(h.is_focused(crate::choices::COUNTRY), "the keyboard goes to the field that failed:\n{screen}");

    // A slash is refused the same way, and the field is editable again.
    h.press("backspace").press("backspace").press("backspace");
    retype(&mut h, "T/R");
    h.click_text(SAVE);
    h.advance(MOMENT);
    assert_eq!(read(root.path()), before, "still nothing was written");
}

#[test]
fn a_value_written_by_hand_and_a_value_the_file_cannot_mean_are_ignored() {
    let root = Root::new();
    set_up_in(root.path());
    fs::write(
        root.path().join("config/tools.conf"),
        "language = \"quvyta\"\ntheme = \"quvyta\"\nicons = \"quvyta\"\n\
         \n[option.mirrors]\ncountry = \"T;rm\"\ncount = 900\n",
    )
    .expect("a hand-edited file");
    let h = start(root.path(), 100, 40);
    let screen = h.screen();
    assert!(screen.contains("Country") && !screen.contains("T;rm"), "the broken country is not used:\n{screen}");
    assert!(!screen.contains("900"), "nor the count that is out of range:\n{screen}");
    assert!(read(root.path()).contains("count = 900"), "and the file is left as the person wrote it");
}

#[test]
fn cancel_and_esc_close_the_form_and_write_nothing() {
    let root = Root::new();
    let before = {
        let mut h = english(root.path());
        h.press("a");
        h.press("tab");
        retype(&mut h, "25");
        h.click_text("Cancel");
        let after = read(root.path());
        assert!(!h.screen().contains(SAVE), "the form is closed:\n{}", h.screen());
        assert!(h.screen().contains("10"), "and the detail still shows what was saved:\n{}", h.screen());
        after
    };
    assert_eq!(before, read(root.path()), "Cancel wrote nothing");

    let mut h = start(root.path(), 100, 40);
    focus_tweaks(&mut h);
    h.press("a");
    retype(&mut h, "Germany");
    h.press("esc");
    h.advance(MOMENT);
    assert!(!h.screen().contains(SAVE), "esc closes the form:\n{}", h.screen());
    assert!(h.screen().contains("TR"), "the value in the file is still the one in force:\n{}", h.screen());
    assert!(!read(root.path()).contains("Germany"), "esc wrote nothing");
}

#[test]
fn a_tweak_without_options_shows_neither_the_key_nor_the_button() {
    let root = Root::new();
    let mut h = english(root.path());
    h.press("down").press("down");
    let screen = h.screen();
    assert!(screen.contains("Multilib repository"), "{screen}");
    assert!(!screen.contains(OPTIONS), "no button to open a form that has no options:\n{screen}");
    let hints = screen.lines().last().unwrap_or_default();
    assert!(!hints.contains("options"), "and no hint for the key either:\n{hints}");
    h.press("a");
    assert!(!h.screen().contains(SAVE), "`a` does nothing here:\n{}", h.screen());
}

#[test]
fn the_key_does_nothing_while_a_run_is_on_screen_or_on_the_settings_page() {
    let root = Root::new();
    let script = Script::new("options-busy", "echo \"$@\"");
    let tools = Tools::new(every_tweak_off(), Cpu::Intel).machine(script.0.clone(), reading_off);
    let mut h = Harness::with_env(tools, crate::locales::env(), 100, 24);
    h.set_locale("en").set_glyph_mode(GlyphMode::Unicode).set_reduced_motion(true);
    focus_tweaks(&mut h);
    h.press("enter");
    say_yes(&mut h);
    let screen = h.screen();
    assert!(screen.contains("Applying"), "a run is on screen:\n{screen}");
    assert!(!screen.lines().last().unwrap_or_default().contains("options"), "no hint for it:\n{screen}");
    h.press("a");
    assert!(!h.screen().contains(SAVE), "`a` does nothing while a run is showing:\n{}", h.screen());
    wait_for_exit(&mut h);

    // Nor on the Settings page, which has no tweak on it at all.
    set_up_in(root.path());
    let mut h = start(root.path(), 100, 40);
    h.click_text("Settings");
    let screen = h.screen();
    assert!(!screen.lines().last().unwrap_or_default().contains("options"), "no hint on the page:\n{screen}");
    h.press("a");
    assert!(!screen.contains(SAVE), "`a` does nothing there either:\n{screen}");
}

#[test]
fn values_that_cannot_be_written_are_not_used_and_a_notice_says_where_and_why() {
    let root = Root::new();
    let mut h = english(root.path());
    let config = root.path().join("config");
    // The folder is closed before Save is pressed, so the write in the background is the one
    // that cannot be made.
    fs::set_permissions(&config, fs::Permissions::from_mode(0o555)).expect("read-only");
    h.press("a");
    h.press("tab");
    retype(&mut h, "25");
    h.click_text(SAVE);
    h.advance(MOMENT);
    let screen = h.screen();
    fs::set_permissions(&config, fs::Permissions::from_mode(0o755)).expect("writable again");
    assert!(screen.contains("could not be saved"), "the notice says so:\n{screen}");
    assert!(screen.contains("Permission denied"), "and why:\n{screen}");
    assert!(screen.contains("10"), "the values that were really saved are still the ones in force:\n{screen}");
    assert!(!read(root.path()).contains("count = 25"), "and the file does not have the new one");
}

/// What a changed value does to a tweak that is already applied: the machine says five parallel
/// downloads, the person asks for eight, and the list and `enter` say what follows from that.
///
/// This is the behaviour found by writing the test and reading what it does, not a redesign of it:
/// a step whose check finds a different value than the one the catalog would write is a conflict,
/// and a conflict reads as `Changed` until it is applied again.
fn machine_with_five_parallel_downloads(choices: &Choices) -> super::Reading {
    let mut system = crate::system::FakeSystem::new();
    system.file("/etc/pacman.conf", "[options]\nParallelDownloads = 5\nColor\nVerbosePkgLists\n");
    super::read(choices, &mut system, Cpu::Intel, &std::env::temp_dir().join("qtools-options-none"))
}

#[test]
fn a_value_changed_on_an_applied_tweak_reads_as_changed_until_it_is_applied_again() {
    let root = Root::new();
    set_up_in(root.path());
    let reader = machine_with_five_parallel_downloads;
    let reading = reader(&Choices::recommended());
    assert_eq!(
        reading.states[1],
        TweakState::Applied,
        "the machine has five parallel downloads, which is what the file says"
    );
    assert!(!reading.recorded[1], "and qtools has not done it, so there is nothing to undo");

    // A run starts a stand-in that prints what `qtools --run` prints; the machine behind the
    // states is the fake one above, so nothing here can reach this computer.
    let script = Script::new("options-changed", "echo \"$@\"");
    let opening =
        Opening::new(Some(&root.path().join("config")), Some(&root.path().join("fonts")), reading.states, Cpu::Intel)
            .with_updates(Some(folders(root.path())));
    let tools = opening.tools.machine(script.0.clone(), reader).recorded(reading.recorded);
    let mut h = Harness::with_env(tools, crate::locales::env(), 100, 24);
    h.set_locale("en").set_glyph_mode(GlyphMode::Unicode).set_reduced_motion(true);
    focus_tweaks(&mut h);
    h.press("down");
    h.press("a");
    retype(&mut h, "8");
    h.click_text(SAVE);
    h.advance(MOMENT);

    // The machine still says five, the person now wants eight: the list and the detail say so.
    // What the state carries about the value it found is not on screen today, and this pins
    // that: the word is `Changed`, and the value in force is the one the person chose.
    let screen = h.screen();
    assert!(screen.contains("Changed"), "the item reads as changed:\n{screen}");
    assert!(screen.matches("Changed").count() >= 2, "in the list and in the detail:\n{screen}");
    assert!(screen.contains("8"), "and the value in force is the one that was chosen:\n{screen}");

    // `enter` asks before it touches anything, and the run it starts is the one asked about.
    if !h.is_focused("tweaks") {
        focus_tweaks(&mut h);
    }
    h.press("enter");
    let screen = h.screen();
    assert!(screen.contains("Apply this?"), "it asks first:\n{screen}");
    say_yes(&mut h);
    wait_for_exit(&mut h);
    assert!(h.screen().contains("--run pacman-options"), "the run is the one that was asked about:\n{}", h.screen());
}

#[test]
fn the_form_reads_whole_at_every_size_and_in_ascii() {
    let root = Root::new();
    for (width, height) in [(100, 24), (60, 20), (80, 14)] {
        let mut h = english(root.path());
        h.press("a");
        let screen = h.screen();
        let what = format!("{width}x{height} form");
        assert!(!screen.contains('⟦'), "{what}: a key has no text:\n{screen}");
        for text in ["Country", "Mirrors", SAVE, "Cancel", "TR", "10"] {
            assert!(screen.contains(text), "{what}: `{text}` is missing:\n{screen}");
        }
        let buffer = h.buffer();
        for (y, row) in screen.lines().enumerate() {
            assert!(text::width(row) <= buffer.area.width, "{what}: row {y} is wider than the screen:\n{screen}");
        }
        h.set_glyph_mode(GlyphMode::Ascii);
        let ascii = h.screen();
        for forbidden in ["[ ]", "[x]", "(o)", "|", "==="] {
            assert!(!ascii.contains(forbidden), "{what}: `{forbidden}` is forbidden in ASCII mode:\n{ascii}");
        }
    }
}
