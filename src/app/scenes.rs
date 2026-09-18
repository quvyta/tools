//! The screens the README shows. Each scene builds one screen on the test harness from invented
//! states, so nothing on it comes from the machine that draws it, and its test checks that the
//! screen says what the picture is meant to show.

use qframe::icons::GlyphMode;

use super::*;

/// The glyphs every scene draws with. Unicode reads the same in any font an image is set in.
const GLYPHS: GlyphMode = GlyphMode::Unicode;

/// A screen of `width` by `height` in English, showing `states` and drawn in `theme` (`None`
/// keeps the default). Runs point at a program that does not exist and states are read from
/// the scene itself, so no scene can touch or read the machine even by accident.
fn scene(states: Vec<TweakState>, theme: Option<&str>, width: u16, height: u16) -> Harness<Tools> {
    let tools = Tools::new(states).machine(PathBuf::from("/nonexistent/qtools"), invented_states);
    let mut h = Harness::with_env(tools, crate::locales::env(), width, height);
    h.set_locale("en").set_glyph_mode(GLYPHS).set_reduced_motion(true);
    if let Some(theme) = theme {
        h.set_theme(theme);
    }
    h
}

/// A machine halfway through: some tweaks done, one half done, one changed by hand since, the
/// rest not yet applied. Catalog order, as [`catalog::all`] lists it.
fn invented_states() -> Vec<TweakState> {
    let mut states = vec![TweakState::Off; catalog::all().len()];
    for (id, state) in [
        ("mirrors", TweakState::Applied),
        ("pacman-options", TweakState::Applied),
        ("multilib", TweakState::Half),
        ("firewall", TweakState::Changed("ufw is inactive".to_owned())),
        ("ssh-hardening", TweakState::Applied),
        ("journal-limit", TweakState::Applied),
    ] {
        let at = catalog::all().iter().position(|tweak| tweak.id == id).expect("the tweak is in the catalog");
        states[at] = state;
    }
    states
}

/// Moves focus to the tweaks list, so the selected row carries its focused pillar.
fn focus_tweaks(h: &mut Harness<Tools>) {
    for _ in 0..5 {
        if h.is_focused("tweaks") {
            return;
        }
        h.press("tab");
    }
    panic!("tab never reached the tweaks list:\n{}", h.screen());
}

/// The packages list: two tweaks checked for one run together, the half-done one selected and
/// its detail beside the list.
fn list(theme: Option<&str>, width: u16, height: u16) -> Harness<Tools> {
    let mut h = scene(invented_states(), theme, width, height);
    focus_tweaks(&mut h);
    h.press("down").press("down").press("down").press("space").press("down").press("space").press("up").press("up");
    h
}

/// The security group with the firewall selected: something turned it off since it was applied,
/// and its detail says what it touches. SSH hardening would be the other choice, but its drop-in's
/// path is wider than the panel and breaks in the middle.
fn security(theme: Option<&str>, width: u16, height: u16) -> Harness<Tools> {
    let mut h = scene(invented_states(), theme, width, height);
    h.click_text("Security");
    focus_tweaks(&mut h);
    h
}

/// The confirmation before the two checked tweaks run: what they install, write and enable.
/// Nothing runs; the scene stops at the question.
fn confirm(theme: Option<&str>, width: u16, height: u16) -> Harness<Tools> {
    let mut h = list(theme, width, height);
    h.press("enter");
    h
}

/// The narrow screen: the groups as a strip of tabs, the list at full width, and the detail of
/// the selected tweak opened below it.
fn narrow(theme: Option<&str>, width: u16, height: u16) -> Harness<Tools> {
    let mut h = scene(invented_states(), theme, width, height);
    focus_tweaks(&mut h);
    h.press("down").press("down").press("alt+b");
    h
}

/// Every scene, with the name its picture is saved under and a caption.
fn all() -> Vec<(&'static str, &'static str, Harness<Tools>)> {
    vec![
        ("list", "the list, default theme 110x24", list(None, 110, 24)),
        ("security", "security detail, iris 110x24", security(Some("iris"), 110, 24)),
        ("confirm", "confirmation, default theme 110x24", confirm(None, 110, 24)),
        ("narrow", "narrow, detail below, nordic 60x24", narrow(Some("nordic"), 60, 24)),
        ("list-amber", "the list, amber 100x20", list(Some("amber"), 100, 20)),
    ]
}

/// The list row that holds `text` with the selection's pillar right before its check mark. The
/// sidebar's open group carries a pillar too, so a pillar anywhere on the line would not do.
fn pillar_row(h: &Harness<Tools>, text: &str) -> Option<String> {
    h.screen().lines().find(|row| row.contains(text) && (row.contains("▌ ☐") || row.contains("▌ ☑"))).map(str::to_owned)
}

/// What every scene must hold: no missing language key, and no string from the machine drawing it.
fn assert_clean(h: &Harness<Tools>) {
    let screen = h.screen();
    assert!(!screen.contains('⟦'), "a language key is missing:\n{screen}");
    assert!(h.app().running.is_none(), "a scene never starts a run:\n{screen}");
    if let Ok(home) = std::env::var("HOME") {
        assert!(home.len() < 2 || !screen.contains(&home), "the machine's home folder leaks into a scene:\n{screen}");
    }
}

#[test]
fn the_list_scene_shows_the_states_the_checks_and_the_detail() {
    for (theme, width, height) in [(None, 110, 24), (Some("amber"), 100, 20)] {
        let h = list(theme, width, height);
        assert_clean(&h);
        let screen = h.screen();
        for text in ["Packages", "Security", "Mirror list", "Applied", "Half", "Off", "Touches", "/etc/pacman.conf"] {
            assert!(screen.contains(text), "`{text}` is missing:\n{screen}");
        }
        assert_eq!(h.app().selected, 2, "the half-done tweak is selected");
        assert_eq!(h.app().checked.iter().filter(|checked| **checked).count(), 2, "two tweaks are checked");
        assert!(pillar_row(&h, "Multilib repository").is_some(), "the selected row carries the pillar:\n{screen}");
    }
}

#[test]
fn the_security_scene_shows_what_the_changed_firewall_touches() {
    let h = security(Some("iris"), 110, 24);
    assert_clean(&h);
    let screen = h.screen();
    assert_eq!(h.app().group, Group::Security);
    for text in ["Firewall", "SSH hardening", "Changed", "Touches", "ufw"] {
        assert!(screen.contains(text), "`{text}` is missing:\n{screen}");
    }
    assert!(pillar_row(&h, "Firewall").is_some(), "the selected row carries the pillar:\n{screen}");
}

#[test]
fn the_confirm_scene_asks_and_lists_both_checked_tweaks() {
    let h = confirm(None, 110, 24);
    assert_clean(&h);
    let screen = h.screen();
    for text in ["Apply this?", "AUR helper", "Cache cleanup", "Cancel"] {
        assert!(screen.contains(text), "`{text}` is missing:\n{screen}");
    }
}

#[test]
fn the_narrow_scene_has_the_strip_the_list_and_the_detail_below() {
    let h = narrow(Some("nordic"), 60, 24);
    assert_clean(&h);
    let screen = h.screen();
    for text in ["Packages", "Cache cleanup", "Touches", "/etc/pacman.conf", "close"] {
        assert!(screen.contains(text), "`{text}` is missing:\n{screen}");
    }
    assert!(pillar_row(&h, "Multilib repository").is_some(), "the selected row carries the pillar:\n{screen}");
}

#[test]
fn every_scene_draws_in_the_theme_it_names() {
    for theme in ["iris", "nordic", "amber"] {
        let h = scene(invented_states(), Some(theme), 60, 24);
        assert_eq!(h.env().theme().id(), theme, "the theme is found");
    }
}

/// Writes every scene to one page for looking at them side by side.
#[test]
fn scenes_review() {
    let pages: Vec<String> = all().iter().map(|(_, caption, h)| h.html(caption)).collect();
    let target = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("target");
    std::fs::create_dir_all(&target).expect("the target directory exists");
    std::fs::write(target.join("scenes-review.html"), qframe::runtime::html_page(&pages))
        .expect("the review page is written");
}
