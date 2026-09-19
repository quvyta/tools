//! The language files and the keymap, compiled in so an installed binary needs nothing beside it.

use qframe::env::{AssetDirs, Env};

/// The compiled-in language files. English comes first: it is the fallback, and every other file
/// is held to its keys.
pub const LOCALES: &[(&str, &str)] = &[
    ("en.toml", include_str!("../assets/locales/en.toml")),
    ("tr.toml", include_str!("../assets/locales/tr.toml")),
    ("de.toml", include_str!("../assets/locales/de.toml")),
    ("es.toml", include_str!("../assets/locales/es.toml")),
    ("fr.toml", include_str!("../assets/locales/fr.toml")),
    ("pt-BR.toml", include_str!("../assets/locales/pt-BR.toml")),
    ("ru.toml", include_str!("../assets/locales/ru.toml")),
    ("zh-Hans.toml", include_str!("../assets/locales/zh-Hans.toml")),
    ("ja.toml", include_str!("../assets/locales/ja.toml")),
];

/// The compiled-in keymap: the application's own keys.
pub const KEYMAP: (&str, &str) = ("keymap.toml", include_str!("../assets/keymap.toml"));

/// The environment as the runtime loads it, used by the application and by the tests.
pub fn env() -> Env {
    let dirs = AssetDirs {
        locale_sources: LOCALES.iter().map(|(file, text)| ((*file).to_owned(), (*text).to_owned())).collect(),
        keymap_source: Some((KEYMAP.0.to_owned(), KEYMAP.1.to_owned())),
        ..AssetDirs::default()
    };
    Env::load(&dirs).expect("the compiled-in locales are readable")
}

#[cfg(test)]
mod tests;
