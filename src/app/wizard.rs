//! The first start: the framework's setup wizard, with one step of qtools' own that says what it
//! promises before it changes anything.
//!
//! It opens while qtools has no `tools.conf`, and only then. The framework owns the appearance
//! step, the buttons and the two files; nothing at all is written until Finish, so a qtools
//! closed half-way leaves the settings folder exactly as it was and the wizard comes again next
//! start. qtools has no settings of its own beyond the shared Quvyta look, so its step asks one thing
//! only: whether to say when an update is out, a switch qtools shares with every Quvyta
//! application and which the Settings page offers again afterwards.

use qframe::prelude::*;
use qframe::storage::Family;
use qframe::widgets::{Checkbox, ScrollView, Setup, SetupWizard};

use super::{Msg, Tools};

/// Rows the wizard takes around its page: the padding, the name, the blank line under it, the
/// steps, the blank lines around the page and the row of buttons.
const AROUND_PAGE: u16 = 8;

/// The fewest rows the page keeps, however short the terminal is.
const LEAST_PAGE_ROWS: u16 = 8;

/// The fewest page rows at which the update box's description stands under it: the intro, the
/// box and the four lines the description takes at the narrowest width, with the blank rows
/// between them, and still a few rows for the promises.
const DESCRIPTION_UNDER_BOX: u16 = 14;

/// The cells a checkbox's label starts after: its two-cell box and the gap beside it.
const BOX_AND_GAP: u16 = 4;

/// The widget id the keyboard starts on: the appearance rows of the framework's step.
pub(super) const FIRST: &str = "setup-appearance";

impl Tools {
    /// Whether the first-run wizard has the screen.
    pub fn setting_up(&self) -> bool {
        self.setup.as_ref().is_some_and(Setup::needed)
    }

    /// The wizard wrote the shared keys and made `tools.conf`. qtools has no keys of its own to
    /// add, so the list takes the screen and the keys, and the Settings page starts from the
    /// wizard's choices; the look chosen is in force already, since
    /// the wizard applied each choice as it was made. The update box is written now, to the
    /// Quvyta-wide switch, and the question held back while the wizard was open follows it.
    pub(super) fn finish_setup(&mut self) -> Command<Msg> {
        // The Settings page carries on from what the wizard chose: its appearance held those
        // choices without writing them, and the update box is qtools' own already.
        if let Some(setup) = self.setup.take() {
            self.appearance = super::settings::appearance(setup.preferences().clone(), self.config.as_deref());
        }
        Command::batch([Command::focus("tweaks"), self.store_update_notice()])
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
                .step(t!("wizard.step-before"), |ui| self.before_step(rows, ui))
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
    ///
    /// What the update box asks is said right under it when the page has `rows` enough; on a
    /// short page it ends the scrolling promises instead, so the box keeps its row.
    fn before_step(&self, rows: u16, ui: &mut View<'_, Msg>) {
        let beside_box = rows >= DESCRIPTION_UNDER_BOX;
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
                if self.updates.is_some() && !beside_box {
                    ui.spacer().height(Length::Cells(1));
                    ui.add(Text::new(update_text()).role("faint")).fill_width();
                }
            })
            .fill_width();
        })
        .fill()
        .id("wizard-before");
        // The box stays under the page rather than at the end of it, so a short screen that
        // scrolls the promises never hides the one choice this step offers.
        if self.updates.is_some() {
            ui.spacer().height(Length::Cells(1));
            ui.add(
                Checkbox::new(self.update_notice)
                    .label(t!("quvyta.appearance.updates"))
                    .on_toggle(Msg::ToggleUpdateNotice),
            )
            .fill_width()
            .id("wizard-updates");
            if beside_box {
                // Under the label, where a setting's description stands.
                let under_label = Padding { top: 0, right: 0, bottom: 0, left: BOX_AND_GAP };
                ui.add(Text::new(update_text()).role("faint")).fill_width().padding(under_label);
            }
        }
    }
}

/// What the update box asks and what it never sends, in the Quvyta ecosystem's own words.
fn update_text() -> String {
    t!("quvyta.appearance.updates-text", family = Family::QUVYTA.title())
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
