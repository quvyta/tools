//! The Settings page: the appearance rows every Quvyta application shows the same way, language,
//! theme and icons each with its "In every Quvyta application" box, then reduced motion and the
//! pillar, the Quvyta-wide update notice, and qtools' own "Ask for the password once while
//! qtools is open".
//!
//! It is the last entry of the sidebar, apart from the groups of tweaks, and the last tab of the
//! narrow strip; choosing it gives the page the place of the list and its detail. The tweak keys
//! do nothing while it is shown, since no tweak is on screen for them to act on.
//!
//! There is no Save button. A change on the appearance rows is in force at once and written by
//! the framework's `Appearance`, the way every Quvyta application writes it: into the shared
//! file while its box is checked and into `tools.conf` while it is not, with the framework's note
//! under the row when the file cannot be written. The update notice is the framework's row too,
//! written in the background; the password-once switch is qtools' own, written the same way. A
//! change on either that cannot be written goes back to what it was and a notice says where and
//! why.

use std::path::Path;

use qframe::prelude::*;
use qframe::storage::{Ecosystem, Preferences};
use qframe::widgets::{Appearance, AppearanceSave, ScrollView, SettingRow, SettingsList, Switch, Toast};

use super::{APP, Msg, Tools};

/// The key the sidebar and the strip know the page by; no group has it.
pub(super) const SETTINGS: &str = "settings";

/// The key in `tools.conf` that says whether the password is asked once while qtools is open. It
/// is qtools' own, so it stays in qtools' own file and the shared Quvyta keys do not touch it. On
/// unless the file says otherwise.
pub(super) const PASSWORD_ONCE: &str = "password-once";

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
/// `folder`, the update notice off the drawing thread. Without a folder there is nowhere to keep a
/// change: it lasts until qtools quits.
pub(super) fn appearance(preferences: Preferences, folder: Option<&Path>) -> Appearance {
    let appearance = Appearance::new(Ecosystem::QUVYTA, APP, preferences).updates_in_background();
    match folder {
        Some(folder) => appearance.in_folder(folder),
        None => appearance.without_saving(),
    }
}

impl Tools {
    /// What became of the update notice the Settings page wrote in the background. One that could
    /// not be written is back on the switch as the file still has it, and a notice says where and
    /// why.
    pub(super) fn appearance_saved(&mut self, save: &AppearanceSave) -> Command<Msg> {
        self.appearance.saved(save);
        let AppearanceSave::Failed(reason) = save else { return Command::none() };
        // The folder is what could not be written; the reason alone does not say where.
        let where_ = self.config.as_deref().map_or_else(String::new, super::shown);
        let body = format!("{where_}\n{reason}");
        Command::toast(Toast::warning(t!("settings.not-saved")).body(body))
    }

    /// The password-once switch was turned. It is in force from the next run, and a session that
    /// is alive is ended the way one ends when the screen goes, so nothing is left refreshing
    /// `sudo`'s ticket for a setting that is now off. The key is written in the background, the
    /// way the update notice's is.
    pub(super) fn password_once(&mut self, on: bool) -> Command<Msg> {
        self.password_once = on;
        if !on {
            self.session = None;
        }
        self.settings.set(PASSWORD_ONCE, on);
        self.settings.save_command(move |result| Msg::PasswordOnceSaved { on, result })
    }

    /// The password-once switch was written into `tools.conf`, or why not. One that could not be
    /// written goes back to what it was, in the file's memory as well as on the switch, and a
    /// notice says where and why.
    pub(super) fn password_once_saved(&mut self, on: bool, result: Result<(), String>) -> Command<Msg> {
        let Err(reason) = result else { return Command::none() };
        self.password_once = !on;
        self.settings.set(PASSWORD_ONCE, !on);
        // The folder is what could not be written; the reason alone does not say where.
        let folder = self.settings.path().and_then(Path::parent).map(Path::to_path_buf);
        let where_ = folder.as_deref().map_or_else(String::new, super::shown);
        let body = format!("{where_}\n{reason}");
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
                    if self.updates.is_some() {
                        self.appearance.updates(list, Msg::Appearance);
                    }
                    // And this one is qtools' own, last on the page: the password is asked once
                    // while qtools is open, in the one terminal every run shares. It is drawn
                    // whatever the Quvyta folders are, because a screen that cannot write the key
                    // still keeps it until it quits, and a run still behaves as it was turned.
                    let on = self.password_once;
                    let row =
                        SettingRow::new(t!("settings.password-once")).description(t!("settings.password-once-text"));
                    list.row(row, |ui| {
                        ui.add(Switch::new(on).on_toggle(Msg::TogglePasswordOnce));
                    });
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
