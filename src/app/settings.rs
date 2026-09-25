//! The Settings page: the appearance rows every Quvyta application shows the same way, language,
//! theme and icons each with its "In every Quvyta application" box, then reduced motion and the
//! pillar, and the Quvyta-wide update notice.
//!
//! It is the last entry of the sidebar, apart from the groups of tweaks, and the last tab of the
//! narrow strip; choosing it gives the page the place of the list and its detail. The tweak keys
//! do nothing while it is shown, since no tweak is on screen for them to act on.
//!
//! There is no Save button. A change on the appearance rows is in force at once and written by
//! the framework's `Appearance`, the way every Quvyta application writes it: into the shared
//! file while its box is checked and into `tools.conf` while it is not, with the framework's note
//! under the row when the file cannot be written. The update notice is the one Quvyta-wide switch,
//! written in the background; when it cannot be written it goes back and a notice says where and
//! why.

use std::path::Path;

use qframe::prelude::*;
use qframe::storage::{Ecosystem, Preferences};
use qframe::widgets::{Appearance, ScrollView, SettingRow, SettingsList, Switch, Toast};

use super::{APP, Msg, Tools};

/// The key the sidebar and the strip know the page by; no group has it.
pub(super) const SETTINGS: &str = "settings";

/// The name of the sidebar, or of the strip, while the page is shown. A widget's identity
/// comes from the widgets around it, which the page and the list do not share, so a name the list
/// also used would make the keys land on the sidebar that is no longer drawn.
pub(super) const SIDEBAR: &str = "settings-sidebar";

/// The widest the rows grow: beyond it a label and its control drift too far apart to be read as
/// one line.
const SECTION: u16 = 76;

/// The cells the scrolling page's bar takes; the rows keep clear of it, so nothing on them is cut
/// when the page is longer than the screen.
const SCROLLBAR: u16 = 2;

/// The page's appearance rows over `preferences`, writing each change into the shared Quvyta folder
/// `folder`. Without a folder there is nowhere to keep a change: it lasts until qtools quits.
pub(super) fn appearance(preferences: Preferences, folder: Option<&Path>) -> Appearance {
    let appearance = Appearance::new(Ecosystem::QUVYTA, APP, preferences);
    match folder {
        Some(folder) => appearance.in_folder(folder),
        None => appearance.without_saving(),
    }
}

impl Tools {
    /// The update switch was turned: on the wizard's page it waits for Finish; on the Settings
    /// page it is written at once, off the drawing thread, into the shared Quvyta folder.
    pub(super) fn toggle_update_notice(&mut self, on: bool) -> Command<Msg> {
        self.update_notice = on;
        if self.setting_up() {
            return Command::none();
        }
        let Some(folder) = self.updates.as_ref().map(|folders| folders.config.clone()) else {
            return Command::none();
        };
        Command::perform(move || {
            let result = Ecosystem::QUVYTA.set_update_notice_in(&folder, on).map_err(|error| error.to_string());
            Msg::NoticeSaved { on, folder, result }
        })
    }

    /// The Settings page's switch was written into `folder`, or why not. One that could not be
    /// written goes back to what it was and a notice says where and why.
    pub(super) fn notice_saved(&mut self, on: bool, folder: &Path, result: Result<(), String>) -> Command<Msg> {
        let Err(reason) = result else { return Command::none() };
        self.update_notice = !on;
        // The folder is what could not be written; the reason alone does not say where.
        let body = format!("{}\n{reason}", super::shown(folder, std::env::var("HOME").ok().as_deref()));
        Command::toast(Toast::warning(t!("settings.not-saved")).body(body))
    }

    /// The page itself, in the place of the list and its detail.
    pub(super) fn settings_page(&self, ui: &mut View<'_, Msg>) {
        // The appearance rows are the framework's, the same in every Quvyta application. qtools
        // asks crates.io at start, so the Quvyta-wide update switch follows them; without the
        // folders that keep it nothing is asked, and a switch there would change nothing.
        let page = |ui: &mut View<'_, Msg>| {
            ui.column(|ui| {
                SettingsList::show(ui, |list| {
                    self.appearance.section(list, Msg::Appearance);
                    // The framework's own row writes the switch as it is turned; this one is
                    // drawn from the same parts and words, and written in the background.
                    if self.updates.is_some() {
                        let text = t!("quvyta.appearance.updates-text", family = Ecosystem::QUVYTA.title());
                        let on = self.update_notice;
                        list.row(SettingRow::new(t!("quvyta.appearance.updates")).description(text), |ui| {
                            ui.add(Switch::new(on).on_toggle(Msg::ToggleUpdateNotice));
                        });
                    }
                })
                .width(Length::Cells(SECTION))
                .id("appearance");
            })
            .padding(Padding { top: 1, right: SCROLLBAR, bottom: 1, left: 0 })
            .fill_width();
        };
        ui.add_with(ScrollView::new(), page).fill().id("settings");
    }

    /// The footer while the page is shown: how to move between the rows and change one, and
    /// none of the list's keys, which do nothing here.
    pub(super) fn settings_hints(ui: &mut View<'_, Msg>) {
        ui.add(
            KeyHints::new()
                .hint("↑↓", t!("hints.move"))
                .hint("enter", t!("hints.change"))
                .action(Scope::Global, "focus-next")
                .action_right(Scope::Global, "quit"),
        )
        .fill_width();
    }
}
