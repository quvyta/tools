use std::sync::Arc;

use super::*;

/// Runs `f` with the application's own language files, `code` active.
fn in_language<R>(code: &str, f: impl FnOnce() -> R) -> R {
    let mut i18n = crate::locales::env().i18n().clone();
    i18n.set_active(code);
    qframe::i18n::scope(Arc::new(i18n), f)
}

// The sentences used to be English text in the source, so a Turkish terminal showed an English
// failure; they now come from the language file of the language in force.
#[test]
fn a_problem_is_said_in_the_language_in_force() {
    let problem = Problem::BackupMissing {
        file: PathBuf::from("/etc/pacman.conf"),
        backup: PathBuf::from("/home/me/.local/state/quvyta-tools/backups/x"),
    };

    let english = in_language("en", || problem.text());
    let turkish = in_language("tr", || problem.text());

    assert!(english.contains("/etc/pacman.conf") && english.contains("backup"), "{english}");
    assert!(turkish.contains("/etc/pacman.conf") && turkish.contains("yedeği"), "{turkish}");
}

#[test]
fn every_problem_has_a_sentence_of_its_own_in_every_language() {
    let file = PathBuf::from("/etc/pacman.conf");
    let problems = [
        Problem::FileMissing { file: file.clone() },
        Problem::OptionDiffers { file: file.clone(), key: "Color".into(), found: "no".into() },
        Problem::SectionMissing { file: file.clone(), section: "options".into() },
        Problem::FileDiffers { file: file.clone(), line: 2, found: Some("x".into()) },
        Problem::FileDiffers { file: file.clone(), line: 2, found: None },
        Problem::ListNotSet { key: "HOOKS".into(), file: file.clone(), drop_ins: file.clone() },
        Problem::ListUnreadable { key: "HOOKS".into(), file: file.clone() },
        Problem::ListNotUnderstood { file: file.clone(), found: Some("HOOKS=base".into()) },
        Problem::ListNotUnderstood { file: file.clone(), found: None },
        Problem::ChangedAfterWrite { file: file.clone() },
        Problem::BackupMissing { file: file.clone(), backup: file.clone() },
        Problem::NothingRecorded,
        Problem::RecordDoesNotMatch { step: 2 },
        Problem::NotRecorded { why: Box::new(Problem::System("disk full".into())), put_back: true },
        Problem::NotRecorded { why: Box::new(Problem::System("disk full".into())), put_back: false },
        Problem::WriteRefused { file: file.clone(), said: String::new() },
        Problem::WriteRefused { file, said: "tee: permission denied".into() },
    ];
    for code in ["en", "tr", "de", "es", "fr", "pt-BR", "ru", "zh-Hans", "ja"] {
        for problem in &problems {
            let text = in_language(code, || problem.text());
            assert!(!text.starts_with("problem."), "`{code}` has no sentence for {problem:?}: {text}");
        }
    }
}

#[test]
fn a_problem_travels_through_an_io_error_and_comes_out_the_same() {
    let problem = Problem::ChangedAfterWrite { file: PathBuf::from("/etc/pacman.conf") };

    assert_eq!(Problem::from_io(&problem.clone().into_io()), problem);
    assert_eq!(
        Problem::from_io(&io::Error::other("disk full")),
        Problem::System("disk full".into()),
        "an error that carries no problem is the machine's own words"
    );
}

#[test]
fn the_first_difference_names_the_line_and_what_is_on_it() {
    let file = PathBuf::from("/etc/x.conf");
    assert_eq!(
        first_difference(file.clone(), "a\nb\nc\n", "a\nB\nc\n"),
        Problem::FileDiffers { file: file.clone(), line: 2, found: Some("B".into()) }
    );
    assert_eq!(first_difference(file.clone(), "a\nb\n", "a\n"), Problem::FileDiffers { file, line: 2, found: None });
}
