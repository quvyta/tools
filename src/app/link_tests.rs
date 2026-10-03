//! The links between qtools and the framework, checked from the drawn screen.
//!
//! Every test here reads a cell the screen really shows and holds it against the framework value
//! that cell is supposed to come from: a theme colour token, a typography role, a badge variant,
//! a glyph from the icon set, a chord from the keymap, a word from the locale. A colour typed
//! into qtools, a glyph drawn by hand or a key written into a footer would each fail the test
//! that owns them, and so would a framework that stopped answering with those values.
//!
//! The built-in themes are what make this possible: `iris`, `nordic` and `amber` all extend
//! `monochrome` and none of them agrees with another on `accent`, `muted`, `dim`, `success`,
//! `warning` or `danger`, so a value that is really read from the theme cannot be told apart from
//! a constant by looking at one theme alone.

use std::collections::BTreeSet;

use qframe::color::ColorDepth;
use qframe::env::{AssetDirs, Env};
use qframe::icons::GlyphMode;
use qframe::prelude::*;
use qframe::widgets::ScrollView;

use super::wizard::tests::Root;
use super::{Msg, Opening, Tools};
use crate::catalog;
use crate::catalog::hardware::Cpu;
use crate::choices::Choices;
use crate::locales::LOCALES;
use crate::tweak::{Group, TweakState};

/// The themes the built-in set ships, all of which extend `monochrome` and differ from one
/// another where it matters here.
const THEMES: [&str; 4] = ["monochrome", "iris", "nordic", "amber"];

/// The processor the screens below are drawn for. Nothing here is decided by it — a theme
/// colour, a glyph, a chord and a word are the same on either — but the screen and the
/// catalog its states are counted against have to be one machine's, so one is named.
const CPU: Cpu = Cpu::Intel;

/// qtools' own environment, with `keymap` in place of the keymap file, so a rebinding can be
/// watched on screen the way a person who edits the file would see it.
fn env_with_keymap(keymap: &str) -> Env {
    let dirs = AssetDirs {
        locale_sources: LOCALES.iter().map(|(file, text)| ((*file).to_owned(), (*text).to_owned())).collect(),
        keymap_source: Some(("keymap.toml".to_owned(), keymap.to_owned())),
        ..AssetDirs::default()
    };
    Env::load(&dirs).expect("the compiled-in locales are readable")
}

/// Every tweak off but the ones named, each standing as it is given.
fn states_with(entries: &[(&str, TweakState)]) -> Vec<TweakState> {
    let mut states = vec![TweakState::Off; catalog::all(CPU).len()];
    for (id, state) in entries {
        let at = catalog::all(CPU).iter().position(|tweak| tweak.id == *id).expect("the tweak is in the catalog");
        states[at] = state.clone();
    }
    states
}

/// A screen in full colour, so a cell carries the theme's own numbers and not a palette index
/// nearest to them; a link is read from the cell, and a reduced colour would only say which
/// palette entry it landed on.
fn true_colour<A: App>(h: &mut Harness<A>) -> &mut Harness<A> {
    h.set_depth(ColorDepth::TrueColor)
}

/// The list screen in `theme`, in English, on the group the first of `entries` belongs to, with
/// that entry chosen, so every title named is on the screen and the detail speaks about it.
///
/// The detail panel is closed unless `panel` says otherwise: open, it repeats the chosen tweak's
/// title beside the list, and a test that means the row in the list would be reading the panel
/// instead.
fn list_in(theme: &str, entries: &[(&str, TweakState)], panel: bool) -> Harness<Tools> {
    let states = states_with(entries);
    let group = entries.first().map_or(Group::Packages, |(id, _)| {
        catalog::all(CPU).iter().find(|tweak| tweak.id == *id).map_or(Group::Packages, |tweak| tweak.group)
    });
    let chosen = entries.first().and_then(|(id, _)| {
        catalog::in_group(group, CPU, &Choices::recommended()).iter().position(|tweak| tweak.id == *id)
    });
    let mut h = Harness::with_env(Tools::new(states, CPU), crate::locales::env(), 100, 24);
    h.set_locale("en").set_theme(theme).set_glyph_mode(GlyphMode::Unicode).set_reduced_motion(true);
    true_colour(&mut h);
    h.send(Msg::PickGroup(group.key().to_owned()));
    if let Some(chosen) = chosen {
        h.send(Msg::Select(chosen));
    }
    h.send(Msg::TogglePanel(panel));
    h
}

/// The first place `text` is drawn: rows from the top, and within a row from the left, so a word
/// that is on screen more than once is read where it is drawn first.
fn where_is<A: App>(h: &Harness<A>, text: &str) -> (u16, u16) {
    for (y, line) in h.screen().lines().enumerate() {
        if let Some(at) = line.find(text) {
            let x = qframe::text::width(&line[..at]);
            return (x, u16::try_from(y).expect("a row on screen"));
        }
    }
    panic!("`{text}` is on screen:\n{}", h.screen());
}

/// The mark on the row of `title`: the icon set's own `dot` glyph, found on the row and to the
/// left of the label, so the test does not have to know which column the list puts it in.
fn mark_of<A: App>(h: &Harness<A>, title: &str) -> (u16, u16) {
    let glyph = h.env().icons().glyph("dot").into_owned();
    let (x, y) = where_is(h, title);
    (0..x)
        .find(|&at| h.buffer()[(at, y)].symbol() == glyph)
        .map(|at| (at, y))
        .unwrap_or_else(|| panic!("the mark of `{title}` is on its row:\n{}", h.screen()))
}

#[test]
fn a_states_mark_is_the_icon_sets_own_glyph_in_the_tone_its_theme_gives_that_state() {
    for (id, title, state, tone) in [
        ("mirrors", "Mirror list", TweakState::Applied, "success"),
        ("multilib", "Multilib repository", TweakState::Half(vec![true, false]), "warning"),
        (
            "firewall",
            "Firewall",
            TweakState::Changed(crate::problem::Problem::System("ufw is inactive".to_owned())),
            "danger",
        ),
    ] {
        let mut seen = BTreeSet::new();
        for theme in THEMES {
            let h = list_in(theme, &[(id, state.clone())], false);
            let (x, y) = mark_of(&h, title);
            let expected = h.env().theme().color(tone);
            assert_eq!(h.fg(x, y), expected, "{theme}: the mark of `{title}` is the theme's `{tone}`");
            seen.insert(format!("{theme} {tone} {expected:?}"));
        }
        assert_eq!(seen.len(), THEMES.len(), "each theme has its own `{tone}`, so it cannot be a constant");
    }
}

#[test]
fn a_state_with_no_tone_leaves_its_mark_in_the_rows_own_text_colour() {
    // `Off` and `Unavailable` are neutral: the framework falls back to the row's own text colour,
    // which is what a mark with no tone must wear rather than a colour of its own.
    for theme in THEMES {
        let h = list_in(theme, &[("mirrors", TweakState::Off)], false);
        let (x, y) = mark_of(&h, "Mirror list");
        let (title_x, title_y) = where_is(&h, "Mirror list");
        let label = h.fg(title_x, title_y).expect("the label has the row's colour");
        assert_eq!(h.fg(x, y), Some(label), "{theme}: the mark wears the label's own colour");
        for tone in ["success", "warning", "danger"] {
            assert_ne!(h.fg(x, y), h.env().theme().color(tone), "{theme}: and it is not the `{tone}` tone");
        }
    }
}

#[test]
fn the_applications_name_is_drawn_in_its_themes_accent() {
    let mut seen = BTreeSet::new();
    for theme in THEMES {
        let h = list_in(theme, &[], false);
        let (x, y) = where_is(&h, "qtools");
        let accent = h.env().theme().color("accent");
        assert_eq!(h.fg(x, y), accent, "{theme}: the name is the theme's `accent`");
        assert!(h.is_bold(x, y), "{theme}: and the name is bold, as the role says");
        seen.insert(format!("{theme} {accent:?}"));
    }
    assert_eq!(seen.len(), THEMES.len(), "each theme has its own `accent`");
}

#[test]
fn a_faint_word_is_the_faint_colour_its_theme_defines() {
    // The tagline in the header and the "Touches" heading in the detail both ask for the `faint`
    // role, which the built-in typography defines as the theme's `muted`.
    for theme in THEMES {
        let h = list_in(theme, &[], true);
        let muted = h.env().theme().color("muted");
        let dim = h.env().theme().color("dim");
        for text in ["Recommended Arch Linux settings", "Touches"] {
            let (x, y) = where_is(&h, text);
            assert_eq!(h.fg(x, y), muted, "{theme}: `{text}` is the theme's `muted`");
            assert_ne!(muted, dim, "{theme}: and `muted` is not the `secondary` role's colour");
        }
    }
}

/// A written cell of the badge in the detail panel: the first one wearing the badge's own surface,
/// which nothing else on the screen wears, so the badge is picked out rather than guessed at.
fn badge_cell<A: App>(h: &Harness<A>, tone: &str) -> (u16, u16) {
    let surface = h
        .env()
        .theme()
        .style("badge", Some(tone), &[])
        .paint("bg")
        .map(|paint| paint.at(0.0))
        .expect("a badge surface");
    let area = h.buffer().area;
    (0..area.width)
        .flat_map(|x| (0..area.height).map(move |y| (x, y)))
        .find(|&(x, y)| h.bg(x, y) == Some(surface) && !h.buffer()[(x, y)].symbol().trim().is_empty())
        .unwrap_or_else(|| panic!("a `{tone}` badge is on screen:\n{}", h.screen()))
}

#[test]
fn the_badge_beside_the_title_wears_the_variant_of_the_state_it_names() {
    for (id, state, tone) in [
        ("mirrors", TweakState::Applied, "success"),
        ("multilib", TweakState::Half(vec![true, false]), "warning"),
        ("firewall", TweakState::Changed(crate::problem::Problem::System("ufw is inactive".to_owned())), "danger"),
    ] {
        for theme in THEMES {
            let h = list_in(theme, &[(id, state.clone())], true);
            let badge = |variant| {
                h.env()
                    .theme()
                    .style("badge", variant, &[])
                    .paint("fg")
                    .map(|paint| paint.at(0.0))
                    .expect("a badge colour")
            };
            let (x, y) = badge_cell(&h, tone);
            assert_eq!(h.fg(x, y), Some(badge(Some(tone))), "{theme}: the badge's word is the theme's `badge.{tone}`");
            assert_ne!(badge(Some(tone)), badge(None), "{theme}: `{tone}` is not the neutral badge");
        }
    }
}

#[test]
fn the_marks_glyph_is_the_icon_sets_own_in_every_glyph_mode() {
    for mode in [GlyphMode::Nerd, GlyphMode::Unicode, GlyphMode::Ascii] {
        let mut h = list_in("monochrome", &[("mirrors", TweakState::Applied)], false);
        h.set_glyph_mode(mode);
        let glyph = h.env().icons().glyph("dot").into_owned();
        let (x, y) = mark_of(&h, "Mirror list");
        assert_eq!(h.buffer()[(x, y)].symbol(), glyph, "{mode:?}: the mark is the icon set's `dot`");
    }
}

#[test]
fn the_footers_panel_hint_is_the_chord_and_the_word_the_keymap_and_the_locale_hold() {
    // The footer says how to reopen a closed panel. It is not written into qtools: the chord
    // comes from the keymap and the word from the locale, both resolved as they are drawn.
    let mut h = Harness::with_env(Tools::default(), crate::locales::env(), 100, 24);
    h.set_locale("en").set_glyph_mode(GlyphMode::Unicode);
    let footer = h.screen().lines().last().unwrap_or_default().to_owned();
    assert!(footer.contains("alt b"), "the built-in chord is named:\n{footer}");
    assert!(footer.contains("panel"), "and the locale's word for it:\n{footer}");

    // A person who binds the panel key to something else sees that key instead.
    let rebound = env_with_keymap("[global]\ntoggle-panel = \"f9\"\n");
    let mut h = Harness::with_env(Tools::default(), rebound, 100, 24);
    h.set_locale("en").set_glyph_mode(GlyphMode::Unicode);
    let footer = h.screen().lines().last().unwrap_or_default().to_owned();
    assert!(footer.contains("f9"), "the rebound chord is named:\n{footer}");
    assert!(!footer.contains("alt b"), "and the old one is not:\n{footer}");

    // The same hint in another language says what that language calls it.
    let mut h = Harness::with_env(Tools::default(), crate::locales::env(), 100, 24);
    h.set_locale("tr").set_glyph_mode(GlyphMode::Unicode);
    let footer = h.screen().lines().last().unwrap_or_default().to_owned();
    assert!(footer.contains("panel"), "Turkish calls it `panel` too:\n{footer}");
    assert!(!footer.contains(" undo"), "while qtools' own word for undo is Turkish:\n{footer}");
}

#[test]
fn the_wizards_undo_sentence_names_the_chord_the_keymap_binds() {
    // The promise under the wizard's own step says which key undoes an applied item, so the
    // sentence stays true when the key is rebound.
    for (keymap, expected, unexpected) in [
        ("[app]\nrevert = \"z\"\n", "undone with z", "undone with f9"),
        ("[app]\nrevert = \"f9\"\n", "undone with f9", "undone with z"),
        // A key the person unbound is not named: the promise says where the undo is instead.
        ("[app]\nrevert = []\n", "undone from its detail", "undone with z"),
    ] {
        let root = Root::new();
        std::fs::create_dir_all(root.path().join("config")).expect("folder");
        std::fs::write(root.path().join("config/quvyta.conf"), "language = \"en\"\n").expect("shared file");
        let states = states_with(&[]);
        let opening = Opening::new(Some(&root.path().join("config")), None, states, CPU);
        let mut h = Harness::with_env(opening.tools, env_with_keymap(keymap), 80, 30);
        h.set_locale("en").set_glyph_mode(GlyphMode::Unicode).set_reduced_motion(true);
        h.click_text("Next");
        let screen = h.screen();
        assert!(screen.contains(expected), "`{expected}` is in the promise:\n{screen}");
        assert!(!screen.contains(unexpected), "`{unexpected}` is not:\n{screen}");
    }
}

/// A view whose content is both wider and taller than it, so the bar appears and the gutter in
/// front of it is there to be measured.
struct Overflowing;

impl App for Overflowing {
    type Msg = ();

    fn update(&mut self, _: ()) -> Command<()> {
        Command::none()
    }

    fn view(&self, ui: &mut View<'_, ()>) {
        ui.add_with(ScrollView::new(), |ui| {
            ui.add(Text::new("x".repeat(400))).fill_width();
            for _ in 0..40 {
                ui.add(Text::new("row")).fill_width();
            }
        })
        .fill();
    }
}

#[test]
fn a_scroll_view_keeps_a_gutter_of_its_own_in_front_of_its_bar() {
    // Written down because of a finding rather than for one: the Settings page keeps a gutter of
    // its own for the bar, and the view already keeps this one. The cell the content stops at is
    // measured here so the report can point at something.
    let h = Harness::new(Overflowing, 20, 6);
    let width = h.buffer().area.width;
    let bar = width - 1;
    assert_eq!(h.buffer()[(bar, 0)].symbol(), " ", "the last column is the bar's own");
    assert_eq!(h.buffer()[(bar - 1, 0)].symbol(), " ", "and the column before it is the air in front of it");
    assert_eq!(h.buffer()[(bar - 2, 0)].symbol(), "x", "the content stops two columns short of the edge");
}

/// That the mark the tests above measure is the one a person sees beside the title: on the title's
/// own row, to the left of it.
#[test]
fn the_mark_a_colour_is_read_from_is_the_one_beside_the_title() {
    let h = list_in("iris", &[("mirrors", TweakState::Applied)], false);
    let (x, y) = where_is(&h, "Mirror list");
    let (mark_x, mark_y) = mark_of(&h, "Mirror list");
    assert_eq!(y, mark_y, "the mark is on the title's row");
    assert!(mark_x < x, "and to the left of it");
    assert_eq!(h.fg(mark_x, mark_y), h.env().theme().color("success"));
}
