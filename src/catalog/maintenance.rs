//! Timers and limits that keep the system tidy.

use crate::step::Step;
use crate::system::{Cmd, System};
use crate::tweak::{Availability, Group, OptionValue, Tweak, TweakOption};

/// Always fits: every systemd machine has a journal.
fn always(_: &mut dyn System) -> Availability {
    Availability::Yes
}

/// Periodic trim of every mounted SSD, through the timer util-linux ships.
///
/// Trim only means something on solid-state storage, so the tweak fits when at least one
/// real disk reports itself as non-rotational.
pub fn ssd_trim() -> Tweak {
    Tweak {
        id: "ssd-trim",
        group: Group::Maintenance,
        options: Vec::new(),
        steps: vec![Step::ServiceEnabled { unit: "fstrim.timer" }],
        applies_to: has_a_solid_state_disk,
    }
}

/// Whether any physical disk is non-rotational.
///
/// `lsblk` lists every block device, and zram swap, loop mounts and ram disks all answer
/// `ROTA=0` although none of them is a drive that could be trimmed; only names outside
/// those families count. A machine where `lsblk` cannot run is treated as having no SSD,
/// so the tweak stays out of the way rather than offering a timer it cannot judge.
fn has_a_solid_state_disk(system: &mut dyn System) -> Availability {
    let solid = system
        .run(&Cmd::new("lsblk", ["-dno", "NAME,ROTA"]))
        .is_ok_and(|output| output.ok() && output.stdout.lines().any(is_solid_state_disk));
    if solid { Availability::Yes } else { Availability::No("unavailable.no-ssd") }
}

/// One `NAME ROTA` line of `lsblk`: a real disk that reports itself as non-rotational.
fn is_solid_state_disk(line: &str) -> bool {
    let mut fields = line.split_whitespace();
    let (Some(name), Some(rota)) = (fields.next(), fields.next()) else {
        return false;
    };
    let virtual_device = ["zram", "loop", "ram", "sr"].iter().any(|family| name.starts_with(family));
    rota == "0" && !virtual_device
}

/// Caps how much disk the system journal may use.
///
/// The cap goes in a drop-in of qtools' own rather than into `journald.conf`, so the stock
/// file stays untouched and the undo is simply removing the drop-in. journald reads its
/// configuration at start, so the limit takes effect on the next boot or restart of the
/// service; qtools does not restart it, because that would interrupt logging mid-session.
pub fn journal_limit(megabytes: u32) -> Tweak {
    Tweak {
        id: "journal-limit",
        group: Group::Maintenance,
        options: vec![TweakOption { key: "journal.max-use", value: OptionValue::Count(megabytes) }],
        steps: vec![Step::FileManaged {
            path: JOURNALD_DROP_IN,
            content: format!("[Journal]\nSystemMaxUse={megabytes}M\n"),
        }],
        applies_to: always,
    }
}

const JOURNALD_DROP_IN: &str = "/etc/systemd/journald.conf.d/50-quvyta-tools.conf";

/// Keeps the clock right with the time service systemd ships.
///
/// A machine that already runs chrony or an ntp daemon has its clock looked after; two
/// services fighting over it would be worse than either alone, so the tweak steps aside.
pub fn time_sync() -> Tweak {
    Tweak {
        id: "time-sync",
        group: Group::Maintenance,
        options: Vec::new(),
        steps: vec![Step::ServiceEnabled { unit: "systemd-timesyncd.service" }],
        applies_to: no_other_time_service,
    }
}

/// The packages whose presence means another daemon owns the clock.
const OTHER_TIME_SERVICES: [&str; 3] = ["chrony", "ntp", "openntpd"];

/// Whether none of the other time daemons is installed.
fn no_other_time_service(system: &mut dyn System) -> Availability {
    let other = OTHER_TIME_SERVICES
        .iter()
        .any(|package| system.run(&Cmd::new("pacman", ["-Q", package])).is_ok_and(|output| output.ok()));
    if other { Availability::No("unavailable.other-time-service") } else { Availability::Yes }
}
