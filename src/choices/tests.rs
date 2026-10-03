//! The values a person chose: what the settings file keeps, what it may hold, and what the
//! catalog is then built from.

use std::fs;
use std::sync::atomic::{AtomicUsize, Ordering};

use qframe::storage::{Ecosystem, Settings};

use super::*;

/// A folder of its own under the system's temporary folder, removed with everything in it when
/// the test ends. Nothing a test writes lands anywhere else.
struct Root(std::path::PathBuf);

impl Root {
    fn new() -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let n = NEXT.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!("qtools-choices-{}-{n}", std::process::id()));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).expect("a temporary folder");
        Self(path)
    }

    /// The Quvyta folder of this root, created so the file has somewhere to go.
    fn folder(&self) -> std::path::PathBuf {
        let folder = self.0.join("config");
        fs::create_dir_all(&folder).expect("the Quvyta folder");
        folder
    }

    /// qtools' settings file in this root, opened the way the screen opens it.
    fn settings(&self) -> Settings {
        Settings::open(self.folder().join("tools.conf")).member_of(&Ecosystem::QUVYTA)
    }
}

impl Drop for Root {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// A settings file holding `text`, as if a person had written it.
fn file_with(text: &str) -> Settings {
    Settings::parse_str("tools.conf", text)
}

fn text_of() -> OptionValue {
    OptionValue::Text("Germany".to_owned())
}

fn count_of(value: u32) -> OptionValue {
    OptionValue::Count(value)
}

fn helper_of(chosen: usize) -> OptionValue {
    OptionValue::Choice { chosen, options: vec!["aur.paru", "aur.yay"] }
}

// The whole way this feature stands or falls: the framework must keep a key it does not know.
#[test]
fn the_framework_keeps_a_key_qtools_added_itself() {
    let root = Root::new();
    let mut settings = root.settings();
    store(&mut settings, MIRRORS, &count_of(20));
    settings.save().expect("the file is written");

    let written = fs::read_to_string(root.folder().join("tools.conf")).expect("the file is there");
    assert!(written.contains("[option.mirrors]"), "the key is written as it is named:\n{written}");
    assert!(written.contains("count = 20"), "{written}");

    // The next start reads it out of the file again.
    let reopened = root.settings();
    assert_eq!(reopened.get::<u32>(&key_of(MIRRORS)), Some(20));
    assert_eq!(reopened.diagnostics(), &[], "a key of ours is not a problem: {:?}", reopened.diagnostics());
}

#[test]
fn a_value_a_person_typed_is_written_under_the_key_of_its_option() {
    let root = Root::new();
    let mut settings = root.settings();
    store(&mut settings, COUNTRY, &text_of());
    store(&mut settings, MIRRORS, &count_of(25));
    store(&mut settings, PARALLEL, &count_of(8));
    store(&mut settings, HELPER, &helper_of(1));
    store(&mut settings, JOURNAL, &count_of(1000));
    settings.save().expect("the file is written");

    let written = fs::read_to_string(root.folder().join("tools.conf")).expect("the file is there");
    for line in ["country = \"Germany\"", "count = 25", "parallel = 8", "helper = \"yay\"", "max-use = 1000"] {
        assert!(written.contains(line), "`{line}` is missing:\n{written}");
    }

    let choices = Choices::read(&root.settings());
    assert_eq!(
        choices,
        Choices { country: "Germany".to_owned(), mirrors: 25, parallel: 8, helper: "yay", journal: 1000 }
    );
}

#[test]
fn nothing_chosen_is_what_qtools_recommends() {
    let choices = Choices::read(&file_with(""));
    let recommended = Choices::recommended();
    assert_eq!(choices, recommended);
    assert_eq!(recommended.mirrors, 10, "ten mirrors is the recommendation");
    assert_eq!(recommended.parallel, 5, "five parallel downloads is the recommendation");
    assert_eq!(recommended.helper, "paru");
    assert_eq!(recommended.journal, 500);
}

#[test]
fn a_chosen_value_is_used_and_the_rest_are_left_at_the_recommendation() {
    let choices = Choices::read(&file_with("[option.pacman]\nparallel = 8\n"));
    assert_eq!(choices.parallel, 8);
    assert_eq!(choices.mirrors, Choices::recommended().mirrors);
    assert_eq!(choices.country, Choices::recommended().country);
    assert_eq!(choices.helper, "paru");
    assert_eq!(choices.journal, 500);
}

#[test]
fn a_value_the_file_cannot_mean_is_ignored_and_the_recommendation_is_used() {
    let recommended = Choices::recommended();
    for (text, what) in [
        ("[option]\nmirrors = 999\n", "a table where a number belongs"),
        ("[option.mirrors]\ncount = \"twenty\"\n", "text where a number belongs"),
        ("[option.mirrors]\ncount = 0\n", "no mirrors at all"),
        ("[option.mirrors]\ncount = 51\n", "more mirrors than the limit"),
        ("[option.mirrors]\ncount = -4\n", "a number below zero"),
        ("[option.pacman]\nparallel = 21\n", "more parallel downloads than the limit"),
        ("[option.journal]\nmax-use = 49\n", "a journal too small to log into"),
        ("[option.journal]\nmax-use = 4097\n", "a journal larger than the limit"),
        ("[option.mirrors]\ncountry = \"T;rm\"\n", "a semicolon in a country"),
        ("[option.mirrors]\ncountry = \"T/R\"\n", "a slash in a country"),
        ("[option.mirrors]\ncountry = \"T\"\n", "a country too short to be one"),
        ("[option.mirrors]\ncountry = \"\"\n", "no country at all"),
        ("[option.aur]\nhelper = \"anything\"\n", "a helper the catalog does not build"),
        ("[option.aur]\nhelper = 7\n", "a number where a helper belongs"),
    ] {
        let choices = Choices::read(&file_with(text));
        assert_eq!(choices, recommended, "{what}: {text:?} is ignored");
    }
}

#[test]
fn a_broken_value_is_left_in_the_file_for_the_person_to_see() {
    let root = Root::new();
    fs::write(root.folder().join("tools.conf"), "[option.mirrors]\ncount = 999\n").expect("a hand-edited file");
    let choices = Choices::read(&root.settings());
    assert_eq!(choices.mirrors, 10, "the recommendation is used");
    let written = fs::read_to_string(root.folder().join("tools.conf")).expect("the file is still there");
    assert!(written.contains("count = 999"), "qtools does not rewrite what a person wrote:\n{written}");
}

#[test]
fn a_country_is_a_code_a_name_or_a_list_of_them() {
    for good in ["TR", "Turkey", "United States", "Germany,Netherlands", "tr-TR", "TR-TR"] {
        assert!(country_ok(good), "`{good}` is a country");
        assert!(problem(COUNTRY, &OptionValue::Text(good.to_owned())).is_none(), "`{good}` is allowed");
    }
    for bad in ["T", "", "T;rm", "T/R", "TR $(id)", "TR\nid", "Türkiye", "TR; ", "a".repeat(61).as_str()] {
        assert!(!country_ok(bad), "`{bad}` is not a country");
        assert_eq!(problem(COUNTRY, &OptionValue::Text(bad.to_owned())), Some(Problem::Country), "`{bad}` is refused");
    }
    assert!(country_ok(&"a".repeat(60)), "sixty characters are still a country");
}

#[test]
fn a_number_outside_its_range_is_refused_with_the_range_it_allows() {
    assert_eq!(problem(MIRRORS, &count_of(1)), None, "one mirror is the smallest that is allowed");
    assert_eq!(problem(MIRRORS, &count_of(50)), None);
    assert_eq!(problem(MIRRORS, &count_of(0)), Some(Problem::Count { min: 1, max: 50 }));
    assert_eq!(problem(MIRRORS, &count_of(51)), Some(Problem::Count { min: 1, max: 50 }));
    assert_eq!(problem(PARALLEL, &count_of(20)), None);
    assert_eq!(problem(PARALLEL, &count_of(21)), Some(Problem::Count { min: 1, max: 20 }));
    assert_eq!(problem(JOURNAL, &count_of(50)), None);
    assert_eq!(problem(JOURNAL, &count_of(49)), Some(Problem::Count { min: 50, max: 4096 }));
    assert_eq!(problem(JOURNAL, &count_of(4096)), None);
    assert_eq!(problem(JOURNAL, &count_of(4097)), Some(Problem::Count { min: 50, max: 4096 }));
    // A value of the wrong kind is refused too, so it can never be written.
    assert_eq!(problem(MIRRORS, &text_of()), Some(Problem::Count { min: 1, max: 50 }));
    assert!(
        problem("no.such.option", &count_of(1)).is_none(),
        "an option qtools does not know is not this one's business"
    );
}

#[test]
fn a_helper_is_one_of_the_two_the_catalog_builds() {
    assert_eq!(helper_at(0, &["aur.paru", "aur.yay"]), Some("paru"));
    assert_eq!(helper_at(1, &["aur.paru", "aur.yay"]), Some("yay"));
    assert_eq!(helper_at(2, &["aur.paru", "aur.yay"]), None, "a third choice names no helper");
    assert_eq!(helper_at(0, &["aur.pacman", "aur.yay"]), None, "a key of another tweak names no helper");
    assert_eq!(helper_at(0, &[]), None, "a choice with no options names no helper");
    assert_eq!(problem(HELPER, &helper_of(0)), None);
    assert_eq!(problem(HELPER, &helper_of(1)), None);
    assert_eq!(
        problem(HELPER, &OptionValue::Choice { chosen: 5, options: vec!["aur.paru", "aur.yay"] }),
        Some(Problem::Helper)
    );
    assert_eq!(problem(HELPER, &count_of(1)), Some(Problem::Helper), "a number names no helper");
}

#[test]
fn a_value_the_option_refuses_is_never_stored() {
    let mut settings = file_with("");
    store(&mut settings, MIRRORS, &count_of(999));
    store(&mut settings, COUNTRY, &OptionValue::Text("T;rm".to_owned()));
    store(&mut settings, HELPER, &OptionValue::Choice { chosen: 4, options: vec!["aur.paru", "aur.yay"] });
    assert_eq!(settings.to_toml(), "", "nothing was written:\n{}", settings.to_toml());
}

#[test]
fn every_option_the_catalog_gives_has_a_limit_here() {
    // The two places that must not drift apart: the keys the catalog writes its options under,
    // and the limits and recommended values read back from the file.
    let mut seen = Vec::new();
    for tweak in crate::catalog::all(crate::catalog::hardware::Cpu::Intel) {
        for option in &tweak.options {
            assert!(limit(option.key).is_some(), "the option `{}` has no limit", option.key);
            seen.push(option.key);
        }
    }
    let mut known: Vec<&str> = [COUNTRY, MIRRORS, PARALLEL, HELPER, JOURNAL].into();
    known.sort_unstable();
    seen.sort_unstable();
    assert_eq!(seen, known, "the catalog's options and the known keys are the same set");
}

#[test]
fn the_recommended_country_is_one_a_person_may_use() {
    let country = Choices::recommended().country;
    assert!(country_ok(&country), "`{country}` is a country reflector takes");
    assert!(problem(COUNTRY, &OptionValue::Text(country)).is_none());
}
