//! Every language on every screen size: nothing missing, nothing cut, nothing out of line.
//!
//! The languages are read from the compiled-in files, so a new one is held to these checks the
//! moment it is added.

use std::collections::BTreeSet;

use qframe::i18n::I18n;
use qframe::icons::GlyphMode;
use qframe::text;

use super::unsupported::Unsupported;
use super::*;

/// The sizes the layout is designed for: wide, the narrowest fold, and short.
const SIZES: [(u16, u16); 3] = [(100, 24), (48, 20), (80, 14)];

fn codes() -> Vec<String> {
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
    let mut h = Harness::with_env(Tools::default(), crate::locales::env(), width, height);
    h.set_locale(code).set_glyph_mode(GlyphMode::Unicode).set_reduced_motion(true);
    h
}

fn focus_tweaks(h: &mut Harness<Tools>) {
    for _ in 0..5 {
        h.press("tab");
        if h.is_focused("tweaks") {
            return;
        }
    }
    panic!("tab never reached the tweaks list");
}

/// The screen as text, one line per row, without the cell each double-width character hides
/// behind it. `Harness::screen` keeps that cell as a space, which would split `防火墙` into
/// `防 火 墙` and make every check for a Chinese or Japanese text fail.
fn screen<A: App>(h: &Harness<A>) -> String {
    let buffer = h.buffer();
    let area = buffer.area;
    let mut out = String::new();
    for y in 0..area.height {
        let mut row = String::new();
        let mut hidden = 0;
        for x in 0..area.width {
            if hidden > 0 {
                hidden -= 1;
                continue;
            }
            let symbol = buffer[(x, y)].symbol();
            hidden = text::width(symbol).saturating_sub(1);
            row.push_str(symbol);
        }
        out.push_str(row.trim_end());
        out.push('\n');
    }
    out
}

/// The screen reads cleanly: no key without text, no line wider than the screen, and every
/// double-width character keeps the cell after it to itself, so nothing after it shifts.
fn assert_clean<A: App>(h: &Harness<A>, what: &str) {
    let screen = screen(h);
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
    let screen = screen(h);
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
                let screen = screen(&h);
                let label = text_in(&code, &format!("group.{}", group.key()), &[]);
                assert!(screen.contains(&label), "{what}: the open group `{label}` is cut:\n{screen}");
                if height < 20 {
                    continue;
                }
                for tweak in catalog::in_group(group) {
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
        let screen = screen(&h);
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
        let screen = screen(&h);
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
            let screen = screen(&h);
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

#[test]
fn the_unsupported_notice_reads_in_every_language() {
    for code in codes() {
        for (width, height) in [(100, 24), (48, 14)] {
            let mut h = Harness::with_env(Unsupported, crate::locales::env(), width, height);
            h.set_locale(&code).set_glyph_mode(GlyphMode::Unicode);
            let what = format!("{code} {width}x{height} unsupported");
            assert_clean(&h, &what);
            let screen = screen(&h);
            let title = text_in(&code, "unsupported.title", &[]);
            assert!(screen.contains(&title), "{what}: `{title}` is cut:\n{screen}");
        }
    }
}

#[test]
fn visual_review_of_every_language() {
    let mut pages = Vec::new();
    for code in codes() {
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
    }
    let target = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("target");
    std::fs::create_dir_all(&target).expect("the target directory exists");
    std::fs::write(target.join("languages-review.html"), qframe::runtime::html_page(&pages))
        .expect("the review page is written");
}
