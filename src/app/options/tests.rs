//! The form as a piece of state: what it keeps, what it refuses and what it hands back. What the
//! screen looks like while it is open is checked in `super::options_tests`, which drives the real
//! screen with key presses.

use std::sync::Arc;

use super::*;

/// Runs `f` with the application's own language files, English active, so `t!` resolves outside
/// a harness.
fn in_english<R>(f: impl FnOnce() -> R) -> R {
    let mut i18n = crate::locales::env().i18n().clone();
    i18n.set_active("en");
    qframe::i18n::scope(Arc::new(i18n), f)
}

/// The options of the tweak with this id, as the catalog built them with the values in force.
fn options_of(id: &str) -> Vec<TweakOption> {
    crate::catalog::with(&crate::choices::Choices::recommended(), crate::catalog::hardware::Cpu::Intel)
        .into_iter()
        .find(|tweak| tweak.id == id)
        .map(|tweak| tweak.options)
        .unwrap_or_else(|| panic!("the catalog has {id}"))
}

fn mirrors() -> Options {
    Options::new(0, options_of("mirrors"))
}

#[test]
fn a_form_starts_from_the_values_in_force() {
    let form = Options::new(3, options_of("pacman-options"));
    assert_eq!(form.tweak(), 3);
    assert_eq!(form.values(), [OptionValue::Count(5)], "what the catalog recommends");
    assert_eq!(form.saved(), vec![("pacman.parallel", OptionValue::Count(5))]);
}

#[test]
fn a_typed_value_is_kept_even_while_it_is_wrong() {
    // Refusing it here would leave the field showing the old number, so a person would not see
    // what they typed; it is refused on Save instead, with the reason under the field.
    let mut form = mirrors();
    form.set(0, OptionValue::Text("T;rm".to_owned()));
    form.set(1, OptionValue::Count(900));
    assert_eq!(form.values(), [OptionValue::Text("T;rm".to_owned()), OptionValue::Count(900)]);
    assert_eq!(form.saved().len(), 2, "both values would be written if they were allowed");
}

#[test]
fn a_field_that_fails_names_itself_and_its_range() {
    let mut form = mirrors();
    form.set(0, OptionValue::Text("T;rm".to_owned()));
    form.set(1, OptionValue::Count(900));
    let errors = in_english(|| form.check());
    assert!(errors.has(choices::COUNTRY), "the country field says why: {errors:?}");
    assert!(errors.has(choices::MIRRORS), "and the count field names its range: {errors:?}");
    assert_eq!(errors.first(), Some(choices::COUNTRY), "the first problem takes the keyboard");
    let said = in_english(|| errors.get(choices::MIRRORS).unwrap_or_default().to_owned());
    assert!(said.contains("1") && said.contains("50"), "`{said}` names the range");
    assert_eq!(form.values()[1], OptionValue::Count(900), "the value is still what was typed");
}

#[test]
fn values_that_are_allowed_are_handed_back_with_the_key_each_is_saved_under() {
    let mut form = mirrors();
    form.set(0, OptionValue::Text("Germany".to_owned()));
    form.set(1, OptionValue::Count(25));
    assert!(in_english(|| form.check()).is_empty(), "both values pass their own check");
    assert_eq!(
        form.saved(),
        vec![(choices::COUNTRY, OptionValue::Text("Germany".to_owned())), (choices::MIRRORS, OptionValue::Count(25))]
    );
}

#[test]
fn a_journal_size_below_its_floor_is_refused_and_its_helper_choice_is_always_one_of_two() {
    let mut form = Options::new(0, options_of("journal-limit"));
    form.set(0, OptionValue::Count(49));
    let errors = in_english(|| form.check());
    assert!(errors.has(choices::JOURNAL), "{errors:?}");
    form.set(0, OptionValue::Count(50));
    assert!(in_english(|| form.check()).is_empty(), "the floor itself is allowed");

    let mut form = Options::new(0, options_of("aur-helper"));
    assert_eq!(form.values(), [helper_of(0)], "paru is the recommendation");
    assert!(in_english(|| form.check()).is_empty(), "one of the two is always allowed");
    form.set(0, helper_of(1));
    assert_eq!(form.saved(), vec![(choices::HELPER, helper_of(1))]);
}

fn helper_of(chosen: usize) -> OptionValue {
    OptionValue::Choice { chosen, options: vec!["aur.paru", "aur.yay"] }
}
