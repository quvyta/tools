//! Whether this machine runs Arch Linux or a distribution built on it.
//!
//! Every tweak assumes `pacman`, the Arch file layout and `systemd`. On any other distribution
//! qtools says so and leaves, before it reads or touches anything else.

use std::path::Path;

use crate::system::System;

/// Where the distribution describes itself: the first is the standard place, the second the
/// fallback the os-release specification names for systems without it.
const OS_RELEASE: [&str; 2] = ["/etc/os-release", "/usr/lib/os-release"];

/// Reads the machine's os-release through `system`, or `None` when neither file can be read.
pub fn os_release(system: &impl System) -> Option<String> {
    OS_RELEASE.iter().find_map(|path| system.read(Path::new(path)).ok().flatten())
}

/// Whether an os-release text describes Arch Linux or a distribution built on it: `ID=arch`,
/// or `arch` among the space-separated ids of `ID_LIKE` (EndeavourOS, Manjaro, Garuda...).
/// A missing text means the distribution is unknown, which is not supported either.
pub fn is_supported(os_release: Option<&str>) -> bool {
    let Some(text) = os_release else { return false };
    text.lines().filter_map(|line| line.trim().split_once('=')).any(|(key, value)| {
        let value = value.trim().trim_matches(|quote| quote == '"' || quote == '\'');
        match key.trim() {
            "ID" => value == "arch",
            "ID_LIKE" => value.split_whitespace().any(|id| id == "arch"),
            _ => false,
        }
    })
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::io;
    use std::path::PathBuf;

    use super::*;
    use crate::system::{Cmd, Output};

    #[test]
    fn arch_itself_is_supported() {
        assert!(is_supported(Some("NAME=\"Arch Linux\"\nID=arch\nBUILD_ID=rolling\n")));
    }

    #[test]
    fn distributions_built_on_arch_are_supported() {
        assert!(is_supported(Some("NAME=\"EndeavourOS\"\nID=\"endeavouros\"\nID_LIKE=\"arch\"\n")));
        assert!(is_supported(Some("ID=manjaro\nID_LIKE=arch\n")));
        assert!(is_supported(Some("ID=example\nID_LIKE='archlinux arch'\n")));
    }

    #[test]
    fn other_distributions_are_not() {
        assert!(!is_supported(Some("NAME=\"Ubuntu\"\nID=ubuntu\nID_LIKE=debian\n")));
        assert!(!is_supported(Some("ID=fedora\n")));
        // A name that only starts with arch is another distribution.
        assert!(!is_supported(Some("ID=archcraft-like\nID_LIKE=\"archlinux\"\n")));
        // Arch mentioned outside ID and ID_LIKE does not count.
        assert!(!is_supported(Some("ID=debian\nNAME=\"not arch\"\nVARIANT_ID=arch\n")));
    }

    #[test]
    fn an_unknown_distribution_is_not() {
        assert!(!is_supported(None));
        assert!(!is_supported(Some("")));
    }

    /// A machine that has only the files it is given, and runs nothing.
    struct Files(HashMap<PathBuf, String>);

    impl System for Files {
        fn run(&mut self, cmd: &Cmd) -> io::Result<Output> {
            Err(io::Error::other(format!("nothing runs here: {cmd:?}")))
        }
        fn read(&self, path: &Path) -> io::Result<Option<String>> {
            Ok(self.0.get(path).cloned())
        }
        fn write(&mut self, path: &Path, _: &str, _: bool) -> io::Result<()> {
            Err(io::Error::other(format!("nothing is written here: {}", path.display())))
        }
        fn remove(&mut self, path: &Path, _: bool) -> io::Result<()> {
            Err(io::Error::other(format!("nothing is removed here: {}", path.display())))
        }
    }

    fn machine(files: &[(&str, &str)]) -> Files {
        Files(files.iter().map(|(path, text)| (PathBuf::from(path), (*text).to_owned())).collect())
    }

    #[test]
    fn etc_comes_first_and_usr_lib_is_the_fallback() {
        let both = machine(&[("/etc/os-release", "ID=arch"), ("/usr/lib/os-release", "ID=debian")]);
        assert_eq!(os_release(&both).as_deref(), Some("ID=arch"));
        let fallback = machine(&[("/usr/lib/os-release", "ID=debian")]);
        assert_eq!(os_release(&fallback).as_deref(), Some("ID=debian"));
        assert_eq!(os_release(&machine(&[])), None);
    }
}
