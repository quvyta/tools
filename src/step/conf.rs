//! Reading and setting `key = value` options in a configuration file, the way pacman writes them.

/// The value of an option that is really set, ignoring commented-out lines.
///
/// A bare directive with no `=` (pacman's `Color`, `VerbosePkgLists`) is a valueless option:
/// when it is set uncommented, this answers `Some(String::new())` rather than `None`.
pub fn read_option(text: &str, key: &str) -> Option<String> {
    text.lines().find_map(|line| {
        let line = line.trim();
        if line.starts_with('#') {
            return None;
        }
        match line.split_once('=') {
            Some((name, value)) => (name.trim() == key).then(|| value.trim().to_owned()),
            None => (line == key).then(String::new),
        }
    })
}

/// The file with the option set, uncommenting the line it was hiding in when there is one.
///
/// An empty `value` writes a bare directive (just `key`, no `=`), matching how pacman.conf
/// spells `Color` and `VerbosePkgLists`; any other value writes `key = value` as before.
pub fn set_option(text: &str, key: &str, value: &str) -> String {
    let wanted = if value.is_empty() { key.to_owned() } else { format!("{key} = {value}") };
    let mut written = false;
    let mut lines: Vec<String> = text
        .lines()
        .map(|line| {
            let bare = line.trim().trim_start_matches('#').trim();
            let name = bare.split_once('=').map_or(bare, |(name, _)| name.trim());
            if name == key && !written {
                written = true;
                wanted.clone()
            } else {
                line.to_owned()
            }
        })
        .collect();
    if !written {
        lines.push(wanted);
    }
    // `lines()` already strips whatever line endings the text came in with, so joining with a
    // plain `\n` and adding one trailing newline is a deliberate normalization, not data loss.
    let mut out = lines.join("\n");
    out.push('\n');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_bare_directive_is_appended_when_the_file_never_had_it() {
        assert_eq!(set_option("[options]\n", "Color", ""), "[options]\nColor\n");
    }

    #[test]
    fn a_bare_directive_is_uncommented_without_gaining_an_equals_sign() {
        assert_eq!(set_option("#Color\n", "Color", ""), "Color\n");
    }

    #[test]
    fn a_bare_directive_reads_back_as_an_empty_value() {
        assert_eq!(read_option("Color\n", "Color"), Some(String::new()));
    }

    #[test]
    fn a_commented_bare_directive_reads_as_not_set() {
        assert_eq!(read_option("#Color\n", "Color"), None);
    }

    #[test]
    fn a_key_value_line_still_reads_its_value() {
        assert_eq!(read_option("ParallelDownloads = 5\n", "ParallelDownloads"), Some("5".to_owned()));
    }

    #[test]
    fn a_key_missing_entirely_is_appended_as_key_value() {
        assert_eq!(set_option("[options]\n", "ParallelDownloads", "5"), "[options]\nParallelDownloads = 5\n");
    }

    #[test]
    fn a_key_present_twice_is_rewritten_only_the_first_time() {
        let text = "ParallelDownloads = 5\nParallelDownloads = 12\n";
        assert_eq!(set_option(text, "ParallelDownloads", "8"), "ParallelDownloads = 8\nParallelDownloads = 12\n");
    }
}
