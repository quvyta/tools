//! The screen shown instead of the list on a distribution qtools does not support yet.
//!
//! It only says so and waits to be closed: nothing on the machine is read beyond os-release,
//! and nothing is changed.

use qframe::prelude::*;
use qframe::widgets::EmptyState;

use super::header;

/// The notice's state: there is none, it only says one thing.
#[derive(Debug, Default)]
pub struct Unsupported;

/// The one thing the notice does: close.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Msg {
    /// Leave qtools.
    Close,
}

impl App for Unsupported {
    type Msg = Msg;

    fn update(&mut self, msg: Msg) -> Command<Msg> {
        match msg {
            Msg::Close => Command::quit(),
        }
    }

    fn view(&self, ui: &mut View<'_, Msg>) {
        AppShell::new()
            .header(header)
            .body(|ui| {
                ui.add(EmptyState::new(t!("unsupported.title")).icon("info").message(t!("unsupported.message"))).fill();
            })
            .footer(|ui| {
                ui.add(KeyHints::new().action(Scope::App, "close-run").action_right(Scope::Global, "quit"))
                    .fill_width();
            })
            .show(ui);
    }

    fn action(&self, name: &str) -> Option<Msg> {
        // `esc` closes whatever sits on top in the list screen; here the notice is all there is.
        (name == "close-run").then_some(Msg::Close)
    }
}

#[cfg(test)]
mod tests {
    use qframe::icons::GlyphMode;

    use super::*;

    fn harness(locale: &str, width: u16, height: u16) -> Harness<Unsupported> {
        let mut h = Harness::with_env(Unsupported, crate::locales::env(), width, height);
        h.set_locale(locale).set_glyph_mode(GlyphMode::Unicode);
        h
    }

    #[test]
    fn it_says_only_arch_for_now_in_english() {
        let screen = harness("en", 100, 24).screen();
        for text in ["qtools", "Only Arch Linux for now", "works only on Arch Linux", "will come later"] {
            assert!(screen.contains(text), "`{text}` is missing:\n{screen}");
        }
        assert!(!screen.contains("Mirror list"), "the list must not open:\n{screen}");
    }

    #[test]
    fn it_says_only_arch_for_now_in_turkish() {
        let screen = harness("tr", 100, 24).screen();
        for text in ["Şimdilik yalnızca Arch Linux", "türevlerinde", "sonra gelecek"] {
            assert!(screen.contains(text), "`{text}` is missing:\n{screen}");
        }
    }

    #[test]
    fn it_reads_on_a_narrow_screen() {
        let screen = harness("en", 48, 14).screen();
        assert!(screen.contains("Only Arch Linux for now"), "{screen}");
    }

    #[test]
    fn esc_closes_it() {
        let mut h = harness("en", 100, 24);
        h.press("esc");
        assert!(h.quit_requested(), "esc should leave qtools");
    }

    #[test]
    fn nothing_bracketed_shows_in_ascii() {
        let mut h = harness("en", 100, 24);
        h.set_glyph_mode(GlyphMode::Ascii);
        let screen = h.screen();
        for forbidden in ['[', ']', '{', '}', '|'] {
            assert!(!screen.contains(forbidden), "`{forbidden}` shows in ASCII mode:\n{screen}");
        }
    }
}
