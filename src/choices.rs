//! The values a person chose for the tweaks' options: where they are kept, what they may be and
//! what the catalog is then built from.
//!
//! They live in qtools' own settings file, `tools.conf`, under the key of their option with an
//! `option.` prefix: `option.mirrors.country = "TR"`, `option.mirrors.count = 20`. Both processes
//! read that one file, so the list, the detail panel, the confirmation and the `--run` child
//! build the catalog the same way; see [`crate::choices_in`].
//!
//! A key that is not there, or that holds something its option does not allow, is not an error
//! and nothing is written back: the value qtools recommends is used instead and the file is left
//! for the person to look at. The limits are qtools' own rather than a widget's, so a value a
//! person typed is refused with a reason instead of being clamped to a range without a word.

use qframe::storage::Settings;

use crate::tweak::OptionValue;

/// The prefix every option key carries in the settings file: `option.mirrors.count`.
pub const PREFIX: &str = "option";

/// The key the value of the option `option` is kept under in the settings file.
#[must_use]
pub fn key_of(option: &str) -> String {
    format!("{PREFIX}.{option}")
}

/// The option the mirror list is built for.
pub const COUNTRY: &str = "mirrors.country";

/// The option that says how many mirrors the list keeps.
pub const MIRRORS: &str = "mirrors.count";

/// The option that says how many packages pacman downloads at once.
pub const PARALLEL: &str = "pacman.parallel";

/// The option that says which AUR helper is built.
pub const HELPER: &str = "aur.helper";

/// The option that says how large the system journal may grow.
pub const JOURNAL: &str = "journal.max-use";

/// How many mirrors are kept: one is no mirror list, and a list nobody reads is no help. The
/// widest value that is not a number with four digits, so a number field can carry it.
pub const MIRROR_RANGE: (u32, u32) = (1, 50);

/// How many packages pacman downloads at once. Past ten the mirrors are the bottleneck, not
/// pacman.
pub const PARALLEL_RANGE: (u32, u32) = (1, 20);

/// How large the system journal may grow, in megabytes. Below the lower bound nothing is
/// logged, which looks like a machine that is not working.
pub const JOURNAL_RANGE: (u32, u32) = (50, 4096);

/// How long a country may be written, in characters: enough for `Turkey,Germany` and for
/// `United States`, and no more.
pub const COUNTRY_LENGTH: (usize, usize) = (2, 60);

/// The AUR helpers the catalog builds, in the order the choice offers them.
///
/// The name is interpolated into a shell line unescaped (see `packages::build_script`), so a
/// helper is always one of these two and never text a person typed.
pub const HELPERS: [&str; 2] = ["paru", "yay"];

/// What an option's value may be.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Limit {
    /// A country code or a country name, or several of them, the way reflector's `--country`
    /// takes them.
    Country,
    /// A whole number, no smaller and no larger than these.
    Count {
        /// The smallest value allowed.
        min: u32,
        /// The largest value allowed.
        max: u32,
    },
    /// One of the [`HELPERS`].
    Helper,
}

/// The limit of the option `option`, or `None` for an option qtools does not know.
#[must_use]
pub fn limit(option: &str) -> Option<Limit> {
    let range = |bounds: (u32, u32)| Limit::Count { min: bounds.0, max: bounds.1 };
    Some(match option {
        COUNTRY => Limit::Country,
        MIRRORS => range(MIRROR_RANGE),
        PARALLEL => range(PARALLEL_RANGE),
        JOURNAL => range(JOURNAL_RANGE),
        HELPER => Limit::Helper,
        _ => return None,
    })
}

/// What is wrong with a value, in the words the field shows under itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Problem {
    /// A country is not text reflector's `--country` would take.
    Country,
    /// A number is out of the range its option allows.
    Count {
        /// The smallest value allowed.
        min: u32,
        /// The largest value allowed.
        max: u32,
    },
    /// A helper is not one of the two the catalog builds.
    Helper,
}

/// What is wrong with `value` for the option `option`, or `None` when it may be used and saved.
#[must_use]
pub fn problem(option: &str, value: &OptionValue) -> Option<Problem> {
    let limit = limit(option)?;
    match (limit, value) {
        (Limit::Country, OptionValue::Text(text)) if country_ok(text) => None,
        (Limit::Country, _) => Some(Problem::Country),
        (Limit::Count { min, max }, OptionValue::Count(count)) if (min..=max).contains(count) => None,
        (Limit::Count { min, max }, _) => Some(Problem::Count { min, max }),
        (Limit::Helper, OptionValue::Choice { chosen, options }) if helper_at(*chosen, options).is_some() => None,
        (Limit::Helper, _) => Some(Problem::Helper),
    }
}

/// The helper the choice at `chosen` names, or `None` when it is not one of the [`HELPERS`].
///
/// `options` holds the language keys of the choice (`aur.paru`, `aur.yay`), so the answer is the
/// catalog's own literal rather than a key a person could have written into the file.
#[must_use]
pub fn helper_at(chosen: usize, options: &[&str]) -> Option<&'static str> {
    let name = options.get(chosen)?.strip_prefix("aur.")?;
    HELPERS.iter().copied().find(|helper| *helper == name)
}

/// Whether `text` is a country reflector's `--country` takes: ASCII letters with spaces, commas
/// and hyphens between them, so a code, a name and a list of them are all one value.
///
/// Nothing else is allowed, because reflector reads the file the country is written into as its
/// own arguments, one per line: a `;` or a `/` there would be another directive or another file,
/// not part of a country.
#[must_use]
pub fn country_ok(text: &str) -> bool {
    let letters = text.chars().all(|c| c.is_ascii_alphabetic() || c == ' ' || c == ',' || c == '-');
    letters && (COUNTRY_LENGTH.0..=COUNTRY_LENGTH.1).contains(&text.chars().count())
}

/// The values the catalog is built with: what a person chose, or what qtools recommends
/// wherever they chose nothing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Choices {
    /// The country the mirror list is built for.
    pub country: String,
    /// How many mirrors the list keeps.
    pub mirrors: u32,
    /// How many packages pacman downloads at once.
    pub parallel: u32,
    /// The AUR helper that is built.
    pub helper: &'static str,
    /// How large the system journal may grow, in megabytes.
    pub journal: u32,
}

impl Choices {
    /// The values qtools recommends, for a person who has chosen nothing.
    ///
    /// The country comes from the system's locale, the way `catalog::country_of` reads it: a
    /// locale that names no country qtools could use, such as `C`, falls back to `US` rather
    /// than to a word reflector would refuse.
    #[must_use]
    pub fn recommended() -> Self {
        Self {
            country: crate::catalog::country_of(std::env::var("LANG").ok().as_deref()),
            mirrors: 10,
            parallel: 5,
            helper: HELPERS[0],
            journal: 500,
        }
    }

    /// The values in `settings`, each one the recommendation where the file says nothing or says
    /// something its option does not allow. Nothing is written back and nothing fails.
    #[must_use]
    pub fn read(settings: &Settings) -> Self {
        let recommended = Self::recommended();
        Self {
            country: text(settings, COUNTRY, recommended.country),
            mirrors: count(settings, MIRRORS, recommended.mirrors),
            parallel: count(settings, PARALLEL, recommended.parallel),
            helper: helper(settings, recommended.helper),
            journal: count(settings, JOURNAL, recommended.journal),
        }
    }
}

/// The text in `settings` for the option `option`, or `fallback` where the file holds text its
/// option would not take.
fn text(settings: &Settings, option: &str, fallback: String) -> String {
    let Some(value) = settings.get::<String>(&key_of(option)) else { return fallback };
    if problem(option, &OptionValue::Text(value.clone())).is_some() { fallback } else { value }
}

/// The number in `settings` for the option `option`, or `fallback` where the file holds something
/// that is not a number in the range the option allows. A number too large for the file's own
/// type is not a number here either, so it falls back with everything else.
fn count(settings: &Settings, option: &str, fallback: u32) -> u32 {
    match settings.get::<u32>(&key_of(option)) {
        Some(value) if problem(option, &OptionValue::Count(value)).is_none() => value,
        _ => fallback,
    }
}

/// The helper in `settings`, or `fallback` where the file names a program the catalog does not
/// build: the name is interpolated into a shell line, so it must stay one of the [`HELPERS`].
fn helper(settings: &Settings, fallback: &'static str) -> &'static str {
    let Some(name) = settings.get::<String>(&key_of(HELPER)) else { return fallback };
    HELPERS.into_iter().find(|known| *known == name).unwrap_or(fallback)
}

/// Stores `value` for the option `option` in `settings`, under the key it is kept at.
///
/// Nothing is stored for a value the option does not allow, so a broken one can never reach the
/// file; the caller is expected to have shown the person why first.
pub fn store(settings: &mut Settings, option: &str, value: &OptionValue) {
    if problem(option, value).is_some() {
        return;
    }
    let key = key_of(option);
    match value {
        OptionValue::Text(text) => {
            settings.set(&key, text.clone());
        }
        OptionValue::Count(count) => {
            settings.set(&key, *count);
        }
        OptionValue::Choice { chosen, options } => {
            if let Some(helper) = helper_at(*chosen, options) {
                settings.set(&key, helper.to_owned());
            }
        }
    }
}

#[cfg(test)]
mod tests;
