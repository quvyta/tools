//! Themes, fonts and cursors.

use crate::step::Step;
use crate::system::{Cmd, System};
use crate::tweak::{Availability, Group, Tweak};

/// Always fits: fontconfig belongs to every Arch machine, graphical or not.
fn always(_: &mut dyn System) -> Availability {
    Availability::Yes
}

/// Makes Qt applications follow GTK's colours and fonts, through the smallest path there is.
///
/// `qt6-base` already ships the gtk3 platform theme plugin, so no extra package is needed;
/// only `QT_QPA_PLATFORMTHEME` has to point at it. `qt6ct` and `kvantum` would ask the person
/// to keep a second theme in sync by hand, which is exactly what this tweak avoids.
///
/// The variable is set through `/etc/environment.d`, not a shell profile: it is read by
/// `pam_systemd` when the session starts, before any Qt process exists, and it works the same
/// under a display manager and a bare `startx`. That also means it takes effect on the next
/// session, not the running one, and the summary says so.
pub fn qt_gtk_match() -> Tweak {
    Tweak {
        id: "qt-gtk-match",
        group: Group::Appearance,
        options: Vec::new(),
        steps: vec![Step::FileManaged { path: QT_ENVIRONMENT_D, content: "QT_QPA_PLATFORMTHEME=gtk3\n".to_owned() }],
        applies_to: has_a_graphical_session,
    }
}

const QT_ENVIRONMENT_D: &str = "/etc/environment.d/90-quvyta-qt.conf";

/// Whether the machine has an X or Xwayland binary at all.
///
/// A server has neither, and setting a Qt platform theme there would sit unread; the
/// binaries are checked with `test` rather than read as text, since either one is an ELF
/// executable and reading it as a string would fail for the wrong reason.
fn has_a_graphical_session(system: &mut dyn System) -> Availability {
    let graphical = system
        .run(&Cmd::new("sh", ["-c", "test -e /usr/bin/Xwayland -o -e /usr/bin/X"]))
        .is_ok_and(|output| output.ok());
    if graphical { Availability::Yes } else { Availability::No("unavailable.no-graphical-session") }
}

/// Noto as the system-wide default for sans, serif and monospace, with its emoji face as a
/// fallback rather than a replacement.
///
/// The three families go into a `local.conf` of qtools' own so Arch's own fontconfig
/// defaults (hinting, subpixel order) stay untouched; only which family answers to
/// `sans-serif`, `serif` and `monospace` changes. `noto-fonts-cjk` is pulled in alongside the
/// Latin and emoji sets so Han, Hangul and Kana text renders instead of falling back to
/// whatever the next installed font happens to cover.
pub fn fonts() -> Tweak {
    Tweak {
        id: "fonts",
        group: Group::Appearance,
        options: Vec::new(),
        steps: vec![
            Step::PackageInstalled { package: "noto-fonts" },
            Step::PackageInstalled { package: "noto-fonts-emoji" },
            Step::PackageInstalled { package: "noto-fonts-cjk" },
            Step::FileManaged { path: FONTCONFIG_LOCAL, content: fontconfig_local() },
        ],
        applies_to: always,
    }
}

const FONTCONFIG_LOCAL: &str = "/etc/fonts/local.conf";

/// The `local.conf` content: one `<alias>` per generic family, Noto first and the emoji face
/// as a second choice so colour emoji still render wherever a glyph is missing.
pub(crate) fn fontconfig_local() -> String {
    let alias = |family: &str, prefer: &str| {
        format!(
            "  <alias>\n    <family>{family}</family>\n    <prefer>\n      <family>{prefer}</family>\n      \
             <family>Noto Color Emoji</family>\n    </prefer>\n  </alias>\n"
        )
    };
    format!(
        "<?xml version='1.0'?>\n<!DOCTYPE fontconfig SYSTEM 'fonts.dtd'>\n<fontconfig>\n{}{}{}</fontconfig>\n",
        alias("sans-serif", "Noto Sans"),
        alias("serif", "Noto Serif"),
        alias("monospace", "Noto Sans Mono"),
    )
}
