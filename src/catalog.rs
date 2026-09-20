//! Every tweak qtools knows, grouped the way the sidebar shows them.

use crate::tweak::{Group, Tweak};

pub mod appearance;
pub mod hardware;
pub mod maintenance;
pub mod packages;
pub mod security;

/// The whole catalog, with the values we recommend.
pub fn all() -> Vec<Tweak> {
    vec![
        packages::mirrors(&default_country(), 10),
        packages::pacman_options(5),
        packages::multilib(),
        packages::aur_helper("paru"),
        packages::cache_cleanup(),
        security::firewall(),
        security::ssh_hardening(),
        hardware::zram_swap(),
        hardware::bluetooth(),
        hardware::laptop_power(),
        hardware::nvidia_wayland(),
        appearance::qt_gtk_match(),
        appearance::fonts(),
        maintenance::ssd_trim(),
        maintenance::journal_limit(500),
        maintenance::time_sync(),
    ]
}

/// The tweaks of one group, in catalog order.
pub fn in_group(group: Group) -> Vec<Tweak> {
    all().into_iter().filter(|tweak| tweak.group == group).collect()
}

/// The country we suggest for the mirror list, taken from the system's locale.
fn default_country() -> String {
    std::env::var("LANG")
        .ok()
        .and_then(|lang| lang.split('.').next().and_then(|code| code.split('_').nth(1)).map(str::to_owned))
        .unwrap_or_else(|| "US".to_owned())
}

#[cfg(test)]
mod tests;
