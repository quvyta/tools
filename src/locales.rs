//! The language files and the keymap, compiled in so an installed binary needs nothing beside it.

use qframe::env::{AssetDirs, Env};

/// The compiled-in language files.
pub const LOCALES: [(&str, &str); 2] =
    [("en.toml", include_str!("../assets/locales/en.toml")), ("tr.toml", include_str!("../assets/locales/tr.toml"))];

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
