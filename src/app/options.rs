//! The options form: the values a person changes before a tweak is applied.
//!
//! It opens with `a`, or with the button under the options in the detail panel, on a tweak that
//! has options: one field per option, a text input, a number input or a choice, then Save and
//! Cancel, with `esc` for Cancel. Nothing is written until Save is pressed and every field has
//! passed its own check; a field that fails says why under itself and Save writes nothing.
//!
//! The form owns what was typed. The application writes what it hands back to `tools.conf` in the
//! background and takes the values into use once the file holds them, the way the Settings page
//! writes the update notice. [`crate::choices`] owns the file and the limits.

use qframe::prelude::*;
use qframe::widgets::{Button, Field, Form, FormErrors, Modal, NumberInput, Segmented, TextInput};

use super::Msg;
use crate::choices::{self, Problem};
use crate::tweak::{OptionValue, TweakOption};

/// The lowest a count field may hold, so a minus sign cannot be typed at all.
pub const FLOOR: f64 = 0.0;

/// The widest number a person may type into a count field.
///
/// The limits are qtools' own and are named under the field, so the number field is not given the
/// option's range: it would refuse the value silently and Save would write the old one without a
/// word. This ceiling is wide enough for any number a person would want to be told is too large,
/// and narrow enough that every count option fits in it.
pub const TYPED: f64 = 9999.0;

/// Cells the labels take beside the controls while the dialog is wide enough for that.
const LABEL_WIDTH: u16 = 12;

/// The width of the dialog in cells. A narrower terminal shrinks it rather than cutting a field.
const WIDTH: u16 = 52;

/// The options form of one tweak, with the values as they are now.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Options {
    /// The place of the tweak whose options it edits, in the current group.
    tweak: usize,
    /// The options of that tweak, as the catalog built them.
    options: Vec<TweakOption>,
    /// What was typed, one value per option, in the same order.
    values: Vec<OptionValue>,
    /// Whether saving was tried. A field says why from then on rather than while the value is
    /// being typed, so a number that is not finished yet is not shouted at.
    tried: bool,
}

impl Options {
    /// A form over the options of the tweak at `tweak`, starting from the values in force.
    #[must_use]
    pub fn new(tweak: usize, options: Vec<TweakOption>) -> Self {
        let values = options.iter().map(|option| option.value.clone()).collect();
        Self { tweak, options, values, tried: false }
    }

    /// The place of the tweak whose options it edits.
    #[must_use]
    pub fn tweak(&self) -> usize {
        self.tweak
    }

    /// The options of the tweak, as the catalog built them.
    #[must_use]
    pub fn options(&self) -> &[TweakOption] {
        &self.options
    }

    /// What was typed, one value per option of the tweak, in the catalog's order.
    #[must_use]
    pub fn values(&self) -> &[OptionValue] {
        &self.values
    }

    /// A value was typed into the option at `index`. It is kept whatever it is: whether it may
    /// be saved is asked on Save, so a number that is not finished yet is not refused.
    pub fn set(&mut self, index: usize, value: OptionValue) {
        if let Some(slot) = self.values.get_mut(index) {
            *slot = value;
        }
    }

    /// Saving was asked for: everything wrong with what was typed is under the name of the field
    /// it belongs to, which is that field's own name, so the keyboard can be taken to the first
    /// one. An empty answer means every field may be saved.
    pub fn check(&mut self) -> FormErrors {
        self.tried = true;
        self.errors()
    }

    /// The values to write, each with the key it is kept under. Asked only once [`Self::check`]
    /// came back with nothing.
    #[must_use]
    pub fn saved(&self) -> Vec<(&'static str, OptionValue)> {
        self.options.iter().zip(&self.values).map(|(option, value)| (option.key, value.clone())).collect()
    }

    /// Everything wrong with what was typed, whether or not saving was tried.
    #[must_use]
    pub fn errors(&self) -> FormErrors {
        let mut errors = FormErrors::new();
        for (option, value) in self.options.iter().zip(&self.values) {
            if let Some(problem) = choices::problem(option.key, value) {
                errors.set(option.key, message(problem));
            }
        }
        errors
    }

    /// What the fields are told: nothing until saving was tried, since a value still being typed
    /// is not a wrong value yet.
    fn field_errors(&self) -> FormErrors {
        if self.tried { self.errors() } else { FormErrors::new() }
    }
}

/// The line a field shows under itself, in the language in force.
fn message(problem: Problem) -> String {
    match problem {
        Problem::Country => t!("form.country-bad"),
        Problem::Count { min, max } => t!("form.count-bad", min = min, max = max),
        Problem::Helper => t!("form.helper-bad"),
    }
}

/// Draws the form as a dialog over the screen: one field per option, then Cancel and Save.
///
/// `esc` and the close mark send Cancel, and the first field takes the keyboard when the dialog
/// opens; `tab` walks the fields and then the two buttons, so `enter` on Save saves it.
pub fn view(form: &Options, ui: &mut View<'_, Msg>) {
    let errors = form.field_errors();
    let dialog = Modal::new()
        .title(t!("form.title"))
        .width(WIDTH)
        .on_close(Msg::OptionCancel)
        .action(Button::new(t!("form.cancel")).on_press(Msg::OptionCancel))
        .action(Button::new(t!("form.save")).variant("primary").on_press(Msg::OptionSave));
    ui.add_with(dialog, |ui| {
        Form::new().label_width(LABEL_WIDTH).show(ui, |fields| {
            for (index, (option, value)) in form.options.iter().zip(form.values()).enumerate() {
                let error = errors.get(option.key).map(str::to_owned);
                let invalid = error.is_some();
                let field = Field::new(t!(&format!("option.{}", option.key))).error(error);
                fields.field(field, |ui| control(ui, index, option, value, invalid));
            }
        });
    });
}

/// Adds the control one option is edited in: a text input, a number input or a choice, each
/// marked invalid while its own field says why.
fn control(ui: &mut View<'_, Msg>, index: usize, option: &TweakOption, value: &OptionValue, invalid: bool) {
    match value {
        OptionValue::Text(text) => {
            let changed = move |text: String| Msg::OptionValue(index, OptionValue::Text(text));
            ui.add(TextInput::new(text).invalid(invalid).max_length(choices::COUNTRY_LENGTH.1).on_change(changed))
                .id(option.key)
                .fill_width();
        }
        OptionValue::Count(count) => {
            let changed = move |typed: f64| {
                // The field holds whole numbers below the ceiling above, so the conversion
                // cannot fail; a value that somehow were not one is left out of every count's
                // range and is refused on Save like any other.
                let count = if (FLOOR..=TYPED).contains(&typed) { typed as u32 } else { u32::MAX };
                Msg::OptionValue(index, OptionValue::Count(count))
            };
            ui.add(
                NumberInput::new(f64::from(*count)).range(FLOOR, TYPED).step(1.0).invalid(invalid).on_change(changed),
            )
            .id(option.key)
            .fill_width();
        }
        OptionValue::Choice { chosen, options } => {
            let labels = options.iter().map(|key| t!(&format!("option.{key}")));
            let chosen_options = options.clone();
            let changed = move |chosen: usize| {
                Msg::OptionValue(index, OptionValue::Choice { chosen, options: chosen_options.clone() })
            };
            ui.add(Segmented::new(labels).selected(*chosen).on_select(changed)).id(option.key).fill_width();
        }
    }
}

#[cfg(test)]
mod tests;
