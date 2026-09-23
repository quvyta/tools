//! The first start: the framework's setup wizard, with one step of qtools' own that says what it
//! promises before it changes anything.
//!
//! It opens while qtools has no `tools.conf`, and only then. The framework owns the appearance
//! step, the buttons and the two files; nothing at all is written until Finish, so a qtools
//! closed half-way leaves the settings folder exactly as it was and the wizard comes again next
//! start. qtools has no settings of its own, so its step asks nothing: it only reads.

use qframe::prelude::*;
use qframe::widgets::{ScrollView, Setup, SetupWizard};

use super::{Msg, Tools};

/// Rows the wizard takes around its page: the padding, the name, the blank line under it, the
/// steps, the blank lines around the page and the row of buttons.
const AROUND_PAGE: u16 = 8;

/// The fewest rows the page keeps, however short the terminal is.
const LEAST_PAGE_ROWS: u16 = 8;

/// The widget id the keyboard starts on: the appearance rows of the framework's step.
pub(super) const FIRST: &str = "setup-appearance";

impl Tools {
    /// Whether the first-run wizard has the screen.
    pub fn setting_up(&self) -> bool {
        self.setup.as_ref().is_some_and(Setup::needed)
    }

    /// The wizard wrote the shared keys and made `tools.conf`. qtools has no keys of its own to
    /// add, so the list simply takes the screen and the keys; the look chosen is in force
    /// already, since the wizard applied each choice as it was made.
    pub(super) fn finish_setup(&mut self) -> Command<Msg> {
        self.setup = None;
        Command::focus("tweaks")
    }

    /// The wizard, while it is wanted: the framework's appearance step, then qtools' own.
    pub(super) fn setup_wizard(&self, ui: &mut View<'_, Msg>) {
        let Some(setup) = &self.setup else { return };
        let size = ui.size();
        let rows = size.height.saturating_sub(AROUND_PAGE).max(LEAST_PAGE_ROWS);
        ui.column(|ui| {
            wizard_name(size.width, ui);
            ui.spacer().height(Length::Cells(1));
            // Every step is given the rows that are left, so the buttons stand at the bottom
            // wherever the person is.
            SetupWizard::new(setup)
                .step(t!("wizard.step-before"), |ui| self.before_step(ui))
                .page_height(rows)
                .show(ui)
                .fill_width();
        })
        .padding(Padding::symmetric(1, 2))
        .fill();
    }

    /// qtools' own step: what it does before, during and after a change to the system. Each line
    /// is a promise the list keeps: the detail and the confirmation both list what an item
    /// touches, a run starts only from the confirmation and in the embedded terminal, where
    /// `sudo` asks for the password, and the revert key undoes from the journal's backups.
    fn before_step(&self, ui: &mut View<'_, Msg>) {
        // The key is read from the keymap, so the line stays true if the person rebinds it.
        let key = ui
            .env()
            .keymap()
            .chords_for(Scope::App, "revert")
            .first()
            .map_or_else(|| "z".to_owned(), qframe::keymap::KeyChord::label);
        let promises =
            [t!("wizard.touches"), t!("wizard.confirm"), t!("wizard.undo", key = key, folder = self.backups.as_str())];
        ui.add(Text::new(t!("wizard.intro")).role("secondary")).fill_width();
        ui.spacer().height(Length::Cells(1));
        // On a narrow, short screen the lines outgrow the page; it scrolls rather than cutting
        // the last promise off.
        ui.add_with(ScrollView::new(), |ui| {
            ui.column(|ui| {
                for (index, promise) in promises.into_iter().enumerate() {
                    if index > 0 {
                        ui.spacer().height(Length::Cells(1));
                    }
                    ui.add(Text::new(promise)).fill_width();
                }
            })
            .fill_width();
        })
        .fill()
        .id("wizard-before");
    }
}

/// The name over the wizard, with the tagline when there is room for it whole: cut short it
/// would read worse than not being there.
fn wizard_name(width: u16, ui: &mut View<'_, Msg>) {
    let tagline = format!("  {}", t!("header.tagline"));
    let mut spans = vec![Span::new("qtools").color("accent").bold()];
    if qframe::text::width("qtools") + qframe::text::width(&tagline) + 4 <= width {
        spans.push(Span::new(tagline).role("faint"));
    }
    ui.add(Text::rich(spans).no_wrap()).fill_width();
}

#[cfg(test)]
#[path = "wizard_tests.rs"]
pub(super) mod tests;
