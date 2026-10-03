//! Reading and setting `key = value` options in a configuration file, the way pacman writes them.
//!
//! A file like `pacman.conf` is made of `[sections]`, and an option means something only in the
//! section it belongs to: `ParallelDownloads` under `[extra]` is not pacman's option but an
//! unknown directive of a repository. So an option is read and written inside its section, and
//! one that is not there yet goes after the section's last directive, never at the end of the
//! file, where the last repository is.

/// The lines of `text` an option of `section` may be on, as a range of line numbers: the lines
/// after the section's header up to the next header, or the whole file when there is no
/// section. `None` when the file has no such section.
fn span(lines: &[&str], section: Option<&str>) -> Option<(usize, usize)> {
    let Some(section) = section else { return Some((0, lines.len())) };
    let start = lines.iter().position(|line| header(line) == Some(section))? + 1;
    let end = lines[start..].iter().position(|line| header(line).is_some()).map_or(lines.len(), |at| start + at);
    Some((start, end))
}

/// The name of the section a line opens, when it is a section header.
fn header(line: &str) -> Option<&str> {
    line.trim().strip_prefix('[')?.strip_suffix(']')
}

/// Whether the file has `section` at all; a file without sections has every option in it.
pub fn has_section(text: &str, section: Option<&str>) -> bool {
    let lines: Vec<&str> = text.lines().collect();
    span(&lines, section).is_some()
}

/// The value of an option that is really set in its section, ignoring commented-out lines.
///
/// A bare directive with no `=` (pacman's `Color`, `VerbosePkgLists`) is a valueless option:
/// when it is set uncommented, this answers `Some(String::new())` rather than `None`.
pub fn read_option(text: &str, section: Option<&str>, key: &str) -> Option<String> {
    let lines: Vec<&str> = text.lines().collect();
    let (start, end) = span(&lines, section)?;
    lines[start..end].iter().find_map(|line| {
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

/// The file with the option set in its section, uncommenting the line it was hiding in when
/// there is one there, or adding it after the section's last directive when there is not.
/// `None` when the file has no such section, which is not guessed at.
///
/// An empty `value` writes a bare directive (just `key`, no `=`), matching how pacman.conf
/// spells `Color` and `VerbosePkgLists`; any other value writes `key = value`.
pub fn set_option(text: &str, section: Option<&str>, key: &str, value: &str) -> Option<String> {
    let wanted = if value.is_empty() { key.to_owned() } else { format!("{key} = {value}") };
    let original: Vec<&str> = text.lines().collect();
    let (start, end) = span(&original, section)?;
    let mut lines: Vec<String> = original.iter().map(|line| (*line).to_owned()).collect();
    let found = (start..end).find(|&at| {
        let bare = original[at].trim().trim_start_matches('#').trim();
        bare.split_once('=').map_or(bare, |(name, _)| name.trim()) == key
    });
    match found {
        Some(at) => lines[at] = wanted,
        None => {
            // After the last line of the section that is a directive of its own, so the option
            // does not end up below the comments that introduce the next section.
            let after = (start..end).rev().find(|&at| {
                let line = original[at].trim();
                !line.is_empty() && !line.starts_with('#')
            });
            let at = after.map_or(start, |at| at + 1);
            lines.insert(at, wanted);
        }
    }
    // `lines()` already strips whatever line endings the text came in with, so joining with a
    // plain `\n` and adding one trailing newline is a deliberate normalization, not data loss.
    let mut out = lines.join("\n");
    out.push('\n');
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The opening of Arch's own `/etc/pacman.conf`, trimmed: `[options]` with its directives
    /// and comments, the comment block that introduces the repositories, and the repositories
    /// after it, a user's own one last.
    const PACMAN_CONF: &str = "\
[options]
#RootDir     = /
HoldPkg     = pacman glibc
Architecture = auto

# Misc options
#UseSyslog
#Color
#NoProgressBar
CheckSpace
#VerbosePkgLists
ParallelDownloads = 5

SigLevel    = Required DatabaseOptional
LocalFileSigLevel = Optional

# REPOSITORIES
#   - can be defined here or included from another file

#[core-testing]
#Include = /etc/pacman.d/mirrorlist

[core]
Include = /etc/pacman.d/mirrorlist

[extra]
Include = /etc/pacman.d/mirrorlist

[mine]
Server = file:///home/me/repo
";

    #[test]
    fn a_bare_directive_is_added_to_its_section_when_the_file_never_had_it() {
        assert_eq!(set_option("[options]\n", Some("options"), "Color", ""), Some("[options]\nColor\n".into()));
    }

    #[test]
    fn a_bare_directive_is_uncommented_without_gaining_an_equals_sign() {
        assert_eq!(set_option("#Color\n", None, "Color", ""), Some("Color\n".into()));
    }

    #[test]
    fn a_bare_directive_reads_back_as_an_empty_value() {
        assert_eq!(read_option("Color\n", None, "Color"), Some(String::new()));
    }

    #[test]
    fn a_commented_bare_directive_reads_as_not_set() {
        assert_eq!(read_option("#Color\n", None, "Color"), None);
    }

    #[test]
    fn a_key_value_line_still_reads_its_value() {
        assert_eq!(read_option("ParallelDownloads = 5\n", None, "ParallelDownloads"), Some("5".to_owned()));
    }

    #[test]
    fn a_key_present_twice_is_rewritten_only_the_first_time() {
        let text = "ParallelDownloads = 5\nParallelDownloads = 12\n";
        assert_eq!(
            set_option(text, None, "ParallelDownloads", "8"),
            Some("ParallelDownloads = 8\nParallelDownloads = 12\n".into())
        );
    }

    // pacman reads a directive under the section it is in: one that lands under the last
    // repository is that repository's unknown directive, and pacman warns or stops on it.
    #[test]
    fn a_missing_option_goes_after_the_last_directive_of_its_section_not_to_the_end_of_the_file() {
        let text = PACMAN_CONF.replace("ParallelDownloads = 5\n", "");

        let written = set_option(&text, Some("options"), "ParallelDownloads", "5").expect("the section is there");

        let expected = PACMAN_CONF
            .replace("ParallelDownloads = 5\n", "")
            .replace("LocalFileSigLevel = Optional\n", "LocalFileSigLevel = Optional\nParallelDownloads = 5\n");
        assert_eq!(written, expected);
        assert_eq!(read_option(&written, Some("options"), "ParallelDownloads"), Some("5".into()));
        assert_eq!(read_option(&written, Some("mine"), "ParallelDownloads"), None);
    }

    #[test]
    fn a_commented_option_is_uncommented_where_it_stands_in_its_section() {
        let written = set_option(PACMAN_CONF, Some("options"), "Color", "").expect("the section is there");

        assert_eq!(written, PACMAN_CONF.replace("#Color\n", "Color\n"));
    }

    #[test]
    fn a_commented_line_of_another_section_is_left_alone() {
        let text = "[options]\nCheckSpace\n\n[mine]\n#Color\n";

        let written = set_option(text, Some("options"), "Color", "").expect("the section is there");

        assert_eq!(written, "[options]\nCheckSpace\nColor\n\n[mine]\n#Color\n");
    }

    #[test]
    fn an_option_set_in_another_section_does_not_count_as_set() {
        assert_eq!(read_option("[options]\nCheckSpace\n[mine]\nColor\n", Some("options"), "Color"), None);
    }

    #[test]
    fn a_file_without_the_section_is_not_written_to() {
        assert_eq!(set_option("[core]\nInclude = x\n", Some("options"), "Color", ""), None);
        assert!(!has_section("[core]\nInclude = x\n", Some("options")));
    }
}
