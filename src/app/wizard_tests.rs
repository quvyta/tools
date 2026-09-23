//! The first start: the wizard opens while qtools has no `tools.conf`, writes nothing until it
//! finishes, and the look chosen in it is the look of the list afterwards.
//!
//! Every test lives in a temporary root: the shared Quvyta folder and the fonts the appearance step
//! looks at are both inside it, so neither the person's settings nor a real font is ever touched,
//! and the font install button is never pressed. The screen is built by [`Opening::new`], the
//! same function [`crate::run_on`] builds it with, so these tests cover the real wiring.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use qframe::icons::GlyphMode;
use qframe::prelude::*;

use crate::app::{Opening, Tools};
use crate::catalog;
use crate::tweak::TweakState;

/// A folder of its own under the system's temporary folder, removed with everything in it when
/// the test ends. Nothing a test writes lands anywhere else.
pub(crate) struct Root(PathBuf);

impl Root {
    pub(crate) fn new() -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let n = NEXT.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!("qtools-wizard-{}-{n}", std::process::id()));
        // A folder left by an earlier run with the same process id would not be a first start.
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).expect("a temporary folder");
        Self(path)
    }

    pub(crate) fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for Root {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// qtools on a machine whose Quvyta folder is `root/config`, on a screen of `width` by
/// `height`, started the way the runtime starts it: the preferences [`Opening::new`] resolved
/// are the language and theme of the first frame.
fn start(root: &Path, width: u16, height: u16) -> Harness<Tools> {
    let states = vec![TweakState::Off; catalog::all().len()];
    let opening = Opening::new(Some(&root.join("config")), Some(&root.join("fonts")), states);
    let language = opening.preferences.language().value.clone();
    let theme = opening.preferences.theme().value.clone();
    let mut h = Harness::with_env(opening.tools, crate::locales::env(), width, height);
    h.set_theme(&theme).set_locale(&language).set_glyph_mode(GlyphMode::Unicode).set_reduced_motion(true);
    h
}

/// A shared Quvyta look another app has already set up: English, the default theme, Unicode icons. The
/// language is written down so what is on screen does not depend on the machine's own.
fn shared_look_in_english(root: &Path) {
    fs::create_dir_all(root.join("config")).expect("folder");
    fs::write(root.join("config/quvyta.conf"), "language = \"en\"\ntheme = \"monochrome\"\nicons = \"unicode\"\n")
        .expect("shared file");
}

/// What the shared Quvyta folder holds, by name, in order; empty when there is no folder at all.
fn names(root: &Path) -> Vec<String> {
    let Ok(entries) = fs::read_dir(root.join("config")) else { return Vec::new() };
    let mut names: Vec<String> =
        entries.map(|entry| entry.expect("entry").file_name().to_string_lossy().into()).collect();
    names.sort();
    names
}

/// Every file of the shared Quvyta folder with what it holds, to tell "exactly as it was".
fn contents(root: &Path) -> Vec<(String, String)> {
    names(root)
        .into_iter()
        .map(|name| {
            let text = fs::read_to_string(root.join("config").join(&name)).expect("readable");
            (name, text)
        })
        .collect()
}

/// `key` in the language the screen is drawn in.
fn said(h: &Harness<Tools>, key: &str) -> String {
    h.env().i18n().translate(key, &[])
}

#[test]
fn it_opens_while_there_is_no_tools_conf_and_creates_nothing() {
    let root = Root::new();
    let h = start(root.path(), 80, 30);
    assert!(h.app().setting_up(), "the first start asks:\n{}", h.screen());
    let screen = h.screen();
    // Whatever language this machine detects, the wizard speaks it.
    for key in ["quvyta.appearance.heading", "quvyta.setup.defaults", "wizard.step-before"] {
        let text = said(&h, key);
        assert!(screen.contains(&text), "`{text}` is missing:\n{screen}");
    }
    assert!(screen.contains("qtools"), "{screen}");
    assert!(!screen.contains(&said(&h, "tweak.mirrors.title")), "the list waits its turn:\n{screen}");
    assert!(!root.path().join("config").exists(), "nothing is written before Finish");
}

#[test]
fn the_before_you_start_step_says_what_qtools_promises_and_back_returns() {
    let root = Root::new();
    shared_look_in_english(root.path());
    let mut h = start(root.path(), 80, 30);
    h.click_text("Next");
    let screen = h.screen();
    for text in [
        "Before you start",
        "shows what it will touch before anything runs",
        "Nothing runs until you confirm",
        "type your own password",
        "undone with z",
        "quvyta-tools",
    ] {
        assert!(screen.contains(text), "`{text}` is missing:\n{screen}");
    }
    h.click_text("Back");
    assert!(h.screen().contains("Start with the defaults"), "back on the appearance step:\n{}", h.screen());
    assert!(h.app().setting_up());
}

#[test]
fn stopping_half_way_leaves_the_folder_exactly_as_it_was() {
    let root = Root::new();
    shared_look_in_english(root.path());
    let before = contents(root.path());
    let mut h = start(root.path(), 80, 30);
    h.click_text("Next");
    h.click_text("Back");
    h.click_text("Next");
    // The keys of the list do nothing behind the wizard: `z` asks no revert of an unseen tweak.
    h.press("z");
    assert!(h.screen().contains("Before you start"), "{}", h.screen());
    assert!(!h.screen().contains("Undo this?"), "no revert is asked behind the wizard:\n{}", h.screen());
    drop(h);
    assert_eq!(contents(root.path()), before, "half-way through, the folder is as it was");
    let again = start(root.path(), 80, 30);
    assert!(again.app().setting_up(), "the wizard comes again:\n{}", again.screen());
}

#[test]
fn finish_writes_both_files_and_opens_the_list() {
    let root = Root::new();
    let mut h = start(root.path(), 80, 30);
    h.click_text(&said(&h, "quvyta.wizard.next"));
    h.click_text(&said(&h, "quvyta.wizard.finish"));
    h.render();
    assert!(!h.app().setting_up(), "{}", h.screen());
    assert_eq!(names(root.path()), ["quvyta.conf", "tools.conf"]);
    let own = fs::read_to_string(root.path().join("config/tools.conf")).expect("written");
    assert!(own.contains("language = \"quvyta\""), "the language follows the shared file:\n{own}");
    let screen = h.screen();
    assert!(screen.contains(&said(&h, "tweak.mirrors.title")), "the list has the screen:\n{screen}");
    assert!(h.is_focused("tweaks"), "and the keys:\n{screen}");
    let again = start(root.path(), 80, 30);
    assert!(!again.app().setting_up(), "it never asks again:\n{}", again.screen());
}

#[test]
fn start_with_the_defaults_ends_it_from_the_first_step() {
    let root = Root::new();
    shared_look_in_english(root.path());
    let mut h = start(root.path(), 80, 30);
    h.click_text("Start with the defaults");
    h.render();
    assert!(!h.app().setting_up(), "{}", h.screen());
    assert_eq!(names(root.path()), ["quvyta.conf", "tools.conf"]);
    assert!(h.screen().contains("Mirror list"), "{}", h.screen());
    assert!(h.is_focused("tweaks"));
}

#[test]
fn turkish_chosen_in_the_wizard_is_written_and_is_what_the_list_speaks() {
    let root = Root::new();
    shared_look_in_english(root.path());
    let mut h = start(root.path(), 80, 30);
    // The language select, opened and answered the way a person does it.
    h.click_text("English");
    h.click_text("Türkçe");
    assert!(h.screen().contains("Başlamadan önce"), "the wizard turns Turkish at once:\n{}", h.screen());
    h.click_text("İleri");
    assert!(h.screen().contains("parolanı"), "{}", h.screen());
    h.click_text("Bitir");
    h.render();
    assert!(!h.app().setting_up(), "{}", h.screen());
    let screen = h.screen();
    for text in ["Paketler", "Yansı listesi", "Kapalı"] {
        assert!(screen.contains(text), "`{text}`: the list stays Turkish:\n{screen}");
    }
    let shared = fs::read_to_string(root.path().join("config/quvyta.conf")).expect("written");
    assert!(shared.contains("language = \"tr\""), "the shared file keeps it:\n{shared}");
    // And the next start opens in it too.
    let again = start(root.path(), 80, 30);
    assert!(again.screen().contains("Paketler"), "{}", again.screen());
}

/// A language qtools speaks that this machine would not choose by itself, so a screen in it can
/// only come from the shared file.
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

#[test]
fn with_tools_conf_there_is_no_wizard_and_the_shared_quvyta_language_is_in_force() {
    let root = Root::new();
    let code = not_detected();
    fs::create_dir_all(root.path().join("config")).expect("folder");
    let shared_look = format!("language = \"{code}\"\ntheme = \"monochrome\"\nicons = \"unicode\"\n");
    fs::write(root.path().join("config/quvyta.conf"), shared_look).expect("shared file");
    fs::write(root.path().join("config/tools.conf"), "language = \"quvyta\"\ntheme = \"quvyta\"\nicons = \"quvyta\"\n")
        .expect("own file");
    let before = contents(root.path());
    let h = start(root.path(), 80, 30);
    assert!(!h.app().setting_up(), "{}", h.screen());
    let screen = h.screen();
    for key in ["group.packages", "tweak.mirrors.title", "state.off"] {
        let text = said_in(&code, key);
        assert!(screen.contains(&text), "`{text}`: the shared Quvyta language is in force:\n{screen}");
    }
    assert_eq!(contents(root.path()), before, "starting writes nothing");
}

#[test]
fn a_shared_file_missing_beside_tools_conf_is_written_for_the_next_app() {
    // Without the wizard qtools resolves the look as every Quvyta app does, and the first to start
    // leaves the shared file for the others.
    let root = Root::new();
    fs::create_dir_all(root.path().join("config")).expect("folder");
    fs::write(root.path().join("config/tools.conf"), "language = \"quvyta\"\n").expect("own file");
    let h = start(root.path(), 80, 30);
    assert!(!h.app().setting_up(), "{}", h.screen());
    assert_eq!(names(root.path()), ["quvyta.conf", "tools.conf"]);
}

#[test]
fn a_theme_chosen_for_every_quvyta_app_is_the_theme_at_start() {
    let root = Root::new();
    fs::create_dir_all(root.path().join("config")).expect("folder");
    fs::write(root.path().join("config/quvyta.conf"), "language = \"en\"\ntheme = \"nordic\"\nicons = \"unicode\"\n")
        .expect("shared file");
    fs::write(root.path().join("config/tools.conf"), "language = \"quvyta\"\ntheme = \"quvyta\"\nicons = \"quvyta\"\n")
        .expect("own file");
    let h = start(root.path(), 80, 30);
    assert_eq!(h.env().theme().id(), "nordic");
}
