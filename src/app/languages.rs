//! Every language on every screen size: nothing missing, nothing cut, nothing out of line.
//!
//! The languages are read from the compiled-in files, so a new one is held to these checks the
//! moment it is added.

use std::collections::BTreeSet;
use std::time::Duration;

use qframe::i18n::I18n;
use qframe::icons::GlyphMode;
use qframe::text;

use super::unsupported::Unsupported;
use super::*;
use crate::catalog::hardware::Cpu;

/// The sizes the layout is designed for: wide, the narrowest fold, and short.
const SIZES: [(u16, u16); 3] = [(100, 24), (48, 20), (80, 14)];

/// The codes of the languages qtools speaks, in the order the files are compiled in.
pub(super) fn codes() -> Vec<String> {
    crate::locales::env().i18n().list().into_iter().map(|(code, _)| code).collect()
}

fn catalog() -> I18n {
    crate::locales::env().i18n().clone()
}

/// `key` in the language `code`, with `args`.
fn text_in(code: &str, key: &str, args: &[(&str, qframe::i18n::Arg)]) -> String {
    let mut i18n = catalog();
    assert!(i18n.set_active(code), "`{code}` is a known language");
    i18n.translate(key, args)
}

fn harness(code: &str, width: u16, height: u16) -> Harness<Tools> {
    let mut h = Harness::with_env(super::tests::off(Cpu::Intel), crate::locales::env(), width, height);
    h.set_locale(code).set_glyph_mode(GlyphMode::Unicode).set_reduced_motion(true);
    h
}

fn focus_tweaks(h: &mut Harness<Tools>) {
    for _ in 0..8 {
        h.press("tab");
        if h.is_focused("tweaks") {
            return;
        }
    }
    panic!("tab never reached the tweaks list");
}

/// The screen reads cleanly: no key without text, no line wider than the screen, and every
/// double-width character keeps the cell after it to itself, so nothing after it shifts.
fn assert_clean<A: App>(h: &Harness<A>, what: &str) {
    let screen = h.screen();
    assert!(!screen.contains('⟦'), "{what}: a key has no text:\n{screen}");
    let buffer = h.buffer();
    let area = buffer.area;
    for (y, row) in screen.lines().enumerate() {
        assert!(text::width(row) <= area.width, "{what}: row {y} is wider than the screen:\n{screen}");
    }
    for y in 0..area.height {
        for x in 0..area.width {
            let symbol = buffer[(x, y)].symbol();
            if text::width(symbol) == 2 {
                assert!(x + 1 < area.width, "{what}: a wide character is cut at the edge of row {y}:\n{screen}");
                let next = buffer[(x + 1, y)].symbol();
                assert!(
                    next.is_empty() || next == " ",
                    "{what}: `{next}` is drawn over the second half of `{symbol}` at {x},{y}:\n{screen}"
                );
            }
        }
    }
}

/// The same check with the forbidden shapes of the aesthetic rules, for ASCII mode.
fn assert_no_forbidden_shapes<A: App>(h: &Harness<A>, what: &str) {
    let screen = h.screen();
    for forbidden in ["[ ]", "[x]", "(o)", "|", "==="] {
        assert!(!screen.contains(forbidden), "{what}: `{forbidden}` is forbidden in every mode:\n{screen}");
    }
}

#[test]
fn every_group_reads_in_full_in_every_language_and_size() {
    for code in codes() {
        for (width, height) in SIZES {
            let mut h = harness(&code, width, height);
            for group in Group::ALL {
                h.send(Msg::PickGroup(group.key().to_owned()));
                let what = format!("{code} {width}x{height} {}", group.key());
                assert_clean(&h, &what);
                let screen = h.screen();
                let label = text_in(&code, &format!("group.{}", group.key()), &[]);
                assert!(screen.contains(&label), "{what}: the open group `{label}` is cut:\n{screen}");
                if height < 20 {
                    continue;
                }
                for tweak in catalog::in_group(group, Cpu::Intel, &crate::choices::Choices::recommended()) {
                    let title = text_in(&code, &format!("tweak.{}.title", tweak.id), &[]);
                    assert!(screen.contains(&title), "{what}: `{title}` is cut:\n{screen}");
                }
            }
        }
    }
}

#[test]
fn the_state_words_line_up_in_every_language() {
    for code in codes() {
        let h = harness(&code, 100, 24);
        let off = text_in(&code, "state.off", &[]);
        let screen = h.screen();
        let ends: BTreeSet<usize> = screen
            .lines()
            .filter(|row| row.trim_end().ends_with(off.as_str()))
            .map(|row| text::width(row.trim_end()).into())
            .collect();
        assert_eq!(ends.len(), 1, "{code}: the state words end in different columns: {ends:?}\n{screen}");
    }
}

#[test]
fn the_detail_reads_in_every_language_on_a_narrow_screen() {
    for code in codes() {
        let mut h = harness(&code, 48, 20);
        focus_tweaks(&mut h);
        h.press("alt+b");
        let what = format!("{code} 48x20 detail");
        assert_clean(&h, &what);
        let screen = h.screen();
        let title = text_in(&code, "tweak.mirrors.title", &[]);
        assert!(screen.matches(title.as_str()).count() >= 2, "{what}: the title heads the detail:\n{screen}");
        let touches = text_in(&code, "detail.touches", &[]);
        assert!(screen.contains(&touches), "{what}: `{touches}` is cut:\n{screen}");
        assert!(screen.contains("reflector"), "{what}: the detail lists what it touches:\n{screen}");

        h.set_glyph_mode(GlyphMode::Ascii);
        assert_no_forbidden_shapes(&h, &what);
    }
}

#[test]
fn the_confirmation_reads_in_every_language_at_every_size() {
    for code in codes() {
        for (width, height) in SIZES {
            let mut h = harness(&code, width, height);
            focus_tweaks(&mut h);
            h.press("enter");
            let what = format!("{code} {width}x{height} confirmation");
            assert_clean(&h, &what);
            let screen = h.screen();
            for key in ["confirm.apply.title", "confirm.apply", "confirm.cancel"] {
                let label = text_in(&code, key, &[]);
                assert!(screen.contains(&label), "{what}: `{label}` is cut:\n{screen}");
            }
            assert!(h.app().running.is_none(), "{what}: asking is not doing");
        }
    }
}

#[test]
fn the_narrow_screen_keeps_the_rules_in_ascii_in_every_language() {
    for code in codes() {
        let mut h = harness(&code, 48, 20);
        h.set_glyph_mode(GlyphMode::Ascii);
        let what = format!("{code} 48x20 ascii");
        assert_clean(&h, &what);
        assert_no_forbidden_shapes(&h, &what);
    }
}

/// The command palette in `code`, on a screen of `width` by `height`, open over the list: the
/// framework's own words come from its language files, the commands from the screen's.
#[test]
fn the_palette_reads_in_every_language_at_every_size() {
    for code in codes() {
        for (width, height) in SIZES {
            let mut h = harness(&code, width, height);
            h.press("ctrl+k").advance(Duration::from_millis(300));
            let what = format!("{code} {width}x{height} palette");
            let filter = text_in(&code, "quvyta.palette.placeholder", &[]);
            assert!(h.screen().contains(&filter), "{what}: the palette did not open:\n{}", h.screen());
            assert_clean(&h, &what);
            let screen = h.screen();
            // The detail is a panel beside the list on a wide screen and a column under it on a
            // narrow one, and each starts open in its own way, so the command names the one this
            // screen has.
            let detail_key = if width < super::FOLD_BELOW { "palette.show-detail" } else { "palette.hide-detail" };
            // The detail, the chosen item's own two commands and the first groups are the rows a
            // short screen still shows; the rest of the list scrolls, as it does for a person.
            let title = text_in(&code, "tweak.mirrors.title", &[]);
            for key in [detail_key, "palette.apply", "palette.mark"] {
                let label = text_in(&code, key, &[("tweak", title.clone().into())]);
                assert!(reads_whole(&screen, &label), "{what}: `{label}` is cut:\n{screen}");
            }
            for key in ["group.packages", "group.security"] {
                let label = text_in(&code, key, &[]);
                assert!(screen.contains(&label), "{what}: `{label}` is cut:\n{screen}");
            }
            // The key beside a command is the one the keymap binds, and it is written the same way
            // in every language: the chord is not translated, the word beside it is.
            let detail = row_with(&screen, &text_in(&code, detail_key, &[]));
            assert!(detail.contains("alt b"), "{what}: the detail's row has no key:\n{screen}");

            h.set_glyph_mode(GlyphMode::Ascii);
            assert_clean(&h, &format!("{what} ascii"));
            assert_no_forbidden_shapes(&h, &format!("{what} ascii"));
        }
    }
}

/// The row of `screen` that carries `label`.
fn row_with(screen: &str, label: &str) -> String {
    screen.lines().find(|row| row.contains(label)).unwrap_or_default().to_owned()
}

#[test]
fn the_unsupported_notice_reads_in_every_language() {
    for code in codes() {
        for (width, height) in [(100, 24), (48, 14)] {
            let mut h = Harness::with_env(Unsupported, crate::locales::env(), width, height);
            h.set_locale(&code).set_glyph_mode(GlyphMode::Unicode);
            let what = format!("{code} {width}x{height} unsupported");
            assert_clean(&h, &what);
            let screen = h.screen();
            let title = text_in(&code, "unsupported.title", &[]);
            assert!(screen.contains(&title), "{what}: `{title}` is cut:\n{screen}");
        }
    }
}

/// The wizard of a first start in `code`, on a screen of `width` by `height`: built by
/// [`Opening::new`] over an empty temporary Quvyta folder, as a first start is.
fn wizard(root: &std::path::Path, code: &str, width: u16, height: u16) -> Harness<Tools> {
    let states = super::tests::every_tweak_off();
    let folders = UpdateFolders { config: root.join("config"), state: root.join("state") };
    let opening = Opening::new(Some(&root.join("config")), Some(&root.join("fonts")), states, Cpu::Intel)
        .with_updates(Some(folders));
    let mut h = Harness::with_env(opening.tools, crate::locales::env(), width, height);
    h.set_locale(code).set_glyph_mode(GlyphMode::Unicode).set_reduced_motion(true);
    h
}

/// Whether `text` stands on screen whole, however it was wrapped: the screen and the text are
/// compared without their spaces and without the pillar, which a dialog draws down its left edge
/// on every row and which would sit between the two halves of a message that wrapped.
pub(super) fn reads_whole(screen: &str, text: &str) -> bool {
    let squeeze = |text: &str| text.chars().filter(|c| !c.is_whitespace() && *c != '\u{258c}').collect::<String>();
    squeeze(screen).contains(&squeeze(text))
}

#[test]
fn the_wizard_reads_in_full_in_every_language_and_width() {
    let root = super::wizard::tests::Root::new();
    for code in codes() {
        for (width, height) in [(48, 40), (80, 30), (120, 30)] {
            let mut h = wizard(root.path(), &code, width, height);
            let what = format!("{code} {width}x{height} wizard");
            assert!(h.app().setting_up(), "{what}");
            assert_clean(&h, &what);
            let screen = h.screen();
            for key in ["quvyta.appearance.heading", "wizard.step-before", "quvyta.setup.defaults"] {
                let label = text_in(&code, key, &[]);
                assert!(screen.contains(&label), "{what}: `{label}` is cut:\n{screen}");
            }

            h.click_text(&text_in(&code, "quvyta.wizard.next", &[]));
            let what = format!("{what}, own step");
            assert_clean(&h, &what);
            let screen = h.screen();
            let undo =
                text_in(&code, "wizard.undo", &[("key", "z".into()), ("folder", h.app().backups.as_str().into())]);
            for text in [
                text_in(&code, "wizard.intro", &[]),
                text_in(&code, "wizard.touches", &[]),
                text_in(&code, "wizard.confirm", &[]),
                undo,
                text_in(&code, "quvyta.appearance.updates-text", &[("family", "Quvyta".into())]),
                text_in(&code, "quvyta.appearance.updates", &[]),
                text_in(&code, "quvyta.wizard.back", &[]),
                text_in(&code, "quvyta.wizard.finish", &[]),
            ] {
                assert!(reads_whole(&screen, &text), "{what}: `{text}` is cut:\n{screen}");
            }
            h.set_glyph_mode(GlyphMode::Ascii);
            assert_no_forbidden_shapes(&h, &what);
        }
        // The shortest screen the wizard is laid out for, its page at the fewest rows it keeps:
        // the page scrolls and the buttons stay on screen.
        let mut h = wizard(root.path(), &code, 48, 16);
        h.click_text(&text_in(&code, "quvyta.wizard.next", &[]));
        let what = format!("{code} 48x16 wizard, own step");
        assert_clean(&h, &what);
        let finish = text_in(&code, "quvyta.wizard.finish", &[]);
        assert!(h.screen().contains(&finish), "{what}: `{finish}` is off the screen:\n{}", h.screen());
        let updates = text_in(&code, "quvyta.appearance.updates", &[]);
        assert!(h.screen().contains(&updates), "{what}: the update box `{updates}` is off the screen:\n{}", h.screen());

        // The shortest page on which the box's description stands under it rather than at the
        // end of the promises: it must still read whole there, in the longest languages too.
        let mut h = wizard(root.path(), &code, 48, 22);
        h.click_text(&text_in(&code, "quvyta.wizard.next", &[]));
        let what = format!("{code} 48x22 wizard, own step");
        assert_clean(&h, &what);
        let screen = h.screen();
        for text in [
            text_in(&code, "quvyta.appearance.updates", &[]),
            text_in(&code, "quvyta.appearance.updates-text", &[("family", "Quvyta".into())]),
            finish,
        ] {
            assert!(reads_whole(&screen, &text), "{what}: `{text}` is cut:\n{screen}");
        }
    }
    assert!(!root.path().join("config").exists(), "looking writes nothing");
}

/// qtools set up before in `code`, on a screen of `width` by `height`, with the Settings page
/// open: built by [`Opening::new`] over a temporary Quvyta folder whose files say `code`.
fn settings_page(root: &std::path::Path, code: &str, width: u16, height: u16) -> Harness<Tools> {
    let config = root.join("config");
    std::fs::create_dir_all(&config).expect("folder");
    std::fs::write(config.join("quvyta.conf"), format!("language = \"{code}\"\ntheme = \"monochrome\"\n"))
        .expect("shared file");
    std::fs::write(config.join("tools.conf"), "language = \"quvyta\"\ntheme = \"quvyta\"\nicons = \"quvyta\"\n")
        .expect("own file");
    let states = super::tests::every_tweak_off();
    let folders = UpdateFolders { config: config.clone(), state: root.join("state") };
    let opening =
        Opening::new(Some(&config), Some(&root.join("fonts")), states, Cpu::Intel).with_updates(Some(folders));
    let mut h = Harness::with_env(opening.tools, crate::locales::env(), width, height);
    h.set_locale(code).set_glyph_mode(GlyphMode::Unicode).set_reduced_motion(true);
    h.send(Msg::PickGroup("settings".to_owned()));
    h
}

#[test]
fn the_settings_page_reads_in_full_in_every_language_and_width() {
    let root = super::wizard::tests::Root::new();
    for code in codes() {
        for (width, height) in [(48, 44), (80, 40), (120, 40)] {
            let mut h = settings_page(root.path(), &code, width, height);
            let what = format!("{code} {width}x{height} settings");
            assert_clean(&h, &what);
            let screen = h.screen();
            for text in [
                text_in(&code, "settings.title", &[]),
                text_in(&code, "quvyta.appearance.heading", &[]),
                text_in(&code, "quvyta.appearance.language", &[]),
                text_in(&code, "quvyta.appearance.theme", &[]),
                text_in(&code, "quvyta.appearance.icons", &[]),
                text_in(&code, "quvyta.appearance.everywhere", &[("family", "Quvyta".into())]),
                text_in(&code, "quvyta.appearance.reduce-motion", &[]),
                text_in(&code, "quvyta.appearance.pillar", &[]),
                text_in(&code, "quvyta.appearance.updates", &[]),
                text_in(&code, "quvyta.appearance.updates-text", &[("family", "Quvyta".into())]),
            ] {
                assert!(reads_whole(&screen, &text), "{what}: `{text}` is cut:\n{screen}");
            }
            let footer = screen.lines().last().unwrap_or_default();
            assert!(!footer.contains(&text_in(&code, "hints.apply", &[])), "{what}: no apply hint here:\n{screen}");
            h.set_glyph_mode(GlyphMode::Ascii);
            assert_clean(&h, &format!("{what} ascii"));
            assert_no_forbidden_shapes(&h, &format!("{what} ascii"));
        }
        // A short screen: the page scrolls, its first rows and the way back stay on screen.
        let mut h = settings_page(root.path(), &code, 80, 14);
        let what = format!("{code} 80x14 settings");
        assert_clean(&h, &what);
        let screen = h.screen();
        for key in ["settings.title", "quvyta.appearance.heading", "group.packages"] {
            let label = text_in(&code, key, &[]);
            assert!(screen.contains(&label), "{what}: `{label}` is cut:\n{screen}");
        }
        h.set_glyph_mode(GlyphMode::Ascii);
        assert_no_forbidden_shapes(&h, &format!("{what} ascii"));
    }
}

#[test]
fn visual_review_of_every_language() {
    let mut pages = Vec::new();
    let root = super::wizard::tests::Root::new();
    for code in codes() {
        let mut h = wizard(root.path(), &code, 80, 30);
        pages.push(h.html(&format!("{code} 80x30, wizard")));
        h.click_text(&text_in(&code, "quvyta.wizard.next", &[]));
        pages.push(h.html(&format!("{code} 80x30, wizard's own step")));
        pages.push(harness(&code, 100, 24).html(&format!("{code} 100x24")));
        pages.push(harness(&code, 48, 20).html(&format!("{code} 48x20")));
        let mut h = harness(&code, 48, 20);
        focus_tweaks(&mut h);
        h.press("alt+b");
        pages.push(h.html(&format!("{code} 48x20, detail open")));
        let mut h = harness(&code, 100, 24);
        focus_tweaks(&mut h);
        h.press("enter");
        pages.push(h.html(&format!("{code} 100x24, confirmation")));
        let mut h = harness(&code, 100, 24);
        h.press("ctrl+k").advance(Duration::from_millis(300));
        pages.push(h.html(&format!("{code} 100x24, palette")));
        // A folder of its own: the wizard of the next language needs one without `tools.conf`.
        let set_up = super::wizard::tests::Root::new();
        pages.push(settings_page(set_up.path(), &code, 100, 30).html(&format!("{code} 100x30, settings")));
        pages.push(settings_page(set_up.path(), &code, 48, 30).html(&format!("{code} 48x30, settings")));
    }
    let target = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("target");
    std::fs::create_dir_all(&target).expect("the target directory exists");
    std::fs::write(target.join("languages-review.html"), qframe::runtime::html_page(&pages))
        .expect("the review page is written");
}
