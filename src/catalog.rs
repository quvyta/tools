//! Every tweak qtools knows, grouped the way the sidebar shows them.

use crate::choices::Choices;
use crate::tweak::{Group, Tweak};

pub mod appearance;
pub mod hardware;
pub mod maintenance;
pub mod packages;
pub mod security;

use hardware::Cpu;

/// The whole catalog, with the values we recommend, for the machine whose processor is
/// `cpu`: an item that installs a package for a processor asks the machine whether it is
/// that processor, so a catalog built for one machine refuses the item rather than
/// installing the package meant for another.
pub fn all(cpu: Cpu) -> Vec<Tweak> {
    with(&Choices::recommended(), cpu)
}

/// The whole catalog, built with the values a person chose in `choices` and for the machine
/// whose processor is `cpu`.
///
/// Everything that depends on an option is built here and nowhere else, so the list, the detail
/// panel, the confirmation and the `--run` child all work from the same values: what a person
/// chose in `tools.conf` is what a step writes and what that step's check compares against. See
/// [`crate::choices`] for where the values come from.
pub fn with(choices: &Choices, cpu: Cpu) -> Vec<Tweak> {
    vec![
        packages::mirrors(&choices.country, choices.mirrors),
        packages::pacman_options(choices.parallel),
        packages::multilib(),
        packages::aur_helper(choices.helper),
        packages::cache_cleanup(),
        security::firewall(),
        security::firewall_ssh(),
        security::ssh_hardening(),
        hardware::microcode(cpu),
        hardware::zram_swap(),
        hardware::bluetooth(),
        hardware::laptop_power(),
        hardware::nvidia_wayland(),
        appearance::qt_gtk_match(),
        appearance::fonts(),
        maintenance::ssd_trim(),
        maintenance::journal_limit(choices.journal),
        maintenance::time_sync(),
    ]
}

/// The tweaks of one group, in catalog order, built with the values a person chose.
pub fn in_group(group: Group, cpu: Cpu, choices: &Choices) -> Vec<Tweak> {
    with(choices, cpu).into_iter().filter(|tweak| tweak.group == group).collect()
}

/// The country a locale such as `tr_TR.UTF-8` is in, which is the one qtools suggests for the
/// mirror list when the person has chosen none. Only letters are taken, since the country is
/// written into reflector's configuration.
pub fn country_of(lang: Option<&str>) -> String {
    lang.and_then(|lang| lang.split(['.', '@']).next())
        .and_then(|code| code.split('_').nth(1))
        .filter(|country| country.len() == 2 && country.chars().all(|c| c.is_ascii_alphabetic()))
        .map_or_else(|| "US".to_owned(), str::to_ascii_uppercase)
}

#[cfg(test)]
mod tests;
