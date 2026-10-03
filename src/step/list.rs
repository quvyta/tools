//! A list in a configuration file, the way these files write one: `KEY=(one two three)`,
//! spread over as many lines as it takes, with comments wherever the author likes them.
//!
//! The last assignment in a file is the one in force, and the items around a comment are the
//! items a shell reading the file would see, so an item put back goes exactly where it is
//! asked for and nothing else in the file moves.

/// One item of a list and where it ends, so the next item can be put right after it.
struct Placed {
    /// The item as it is written.
    name: String,
    /// The line it is on.
    line: usize,
    /// Where it ends on that line.
    end: usize,
}

/// A list read out of a file, with the file's own lines kept as they are.
pub struct List {
    /// The file's lines with their own line endings, so writing the file back changes
    /// nothing but the place an item is added.
    lines: Vec<String>,
    /// The items, in the order they are written.
    items: Vec<Placed>,
    /// The line the closing parenthesis is on and where it starts: where an item goes when
    /// none of the anchors is in the list.
    closing: (usize, usize),
}

impl List {
    /// Whether the list holds `item`.
    #[must_use]
    pub fn has(&self, item: &str) -> bool {
        self.items.iter().any(|placed| placed.name == item)
    }

    /// The whole file with `item` added, right after the first of `after` that is in the
    /// list, or just before the closing parenthesis when none of them is.
    #[must_use]
    pub fn with(&self, item: &str, after: &[&str]) -> String {
        let (line, at) = self.anchor(after).unwrap_or(self.closing);
        // A list with nothing in it gets the item without a leading space, so the line comes
        // out the way a person would have written it.
        let added = if self.items.is_empty() { (*item).to_owned() } else { format!(" {item}") };
        let mut lines = self.lines.clone();
        lines[line].insert_str(at, &added);
        lines.concat()
    }

    /// Where the first of `after` ends, which is where a new item follows it.
    fn anchor(&self, after: &[&str]) -> Option<(usize, usize)> {
        after.iter().find_map(|wanted| {
            self.items.iter().find(|placed| placed.name == *wanted).map(|placed| (placed.line, placed.end))
        })
    }
}

/// The list `key` sets in `text`, or the text that could not be understood.
///
/// The assignment that counts is the last one in the file, because that is the one a shell
/// reading the file ends up with: a file may carry a commented-out example above the real
/// line, and one assignment may follow another. The value has to be a parenthesised list;
/// `HOOKS=base udev` and a parenthesis that is never closed are not, and each comes back as
/// the text that stopped us, so the person is shown it rather than having it guessed at.
/// `Err(None)` is a file that does not assign the list at all.
pub fn read(text: &str, key: &str) -> Result<List, Option<String>> {
    let lines: Vec<&str> = text.split_inclusive('\n').collect();
    let Some((first, opened)) = last_assignment(&lines, key) else {
        return Err(refused(&lines, key));
    };
    let mut items = Vec::new();
    for (at, line) in lines.iter().enumerate().skip(first) {
        let body = before_comment(line);
        let from = if at == first { opened + 1 } else { 0 };
        let region = body.get(from..).unwrap_or_default();
        match region.find(')') {
            Some(close) => {
                collect(&region[..close], from, at, &mut items);
                return Ok(List {
                    lines: lines.iter().map(|line| (*line).to_owned()).collect(),
                    items,
                    closing: (at, from + close),
                });
            }
            None => collect(region, from, at, &mut items),
        }
    }
    // A list that never closes runs to the end of the file, and the assignment with it is
    // what was found: it is the person's own text, shown back rather than repaired.
    Err(Some(lines[first..].concat().trim().to_owned()))
}

/// Where the last line that assigns `key` opens its list, and the text of the last one that
/// assigns it in a way that is not a list.
fn last_assignment(lines: &[&str], key: &str) -> Option<(usize, usize)> {
    let mut opened = None;
    for (at, line) in lines.iter().enumerate() {
        match opening(line, key) {
            Opening::Other => {}
            Opening::NotAList(_) => opened = None,
            Opening::At(offset) => opened = Some((at, offset)),
        }
    }
    opened
}

/// The text of the last line that assigns `key` in a way that is not a list.
fn refused(lines: &[&str], key: &str) -> Option<String> {
    lines.iter().rev().find_map(|line| match opening(line, key) {
        Opening::NotAList(found) => Some(found),
        Opening::At(_) | Opening::Other => None,
    })
}

/// What a line says about `key`.
enum Opening {
    /// The line does not assign this list.
    Other,
    /// The line assigns it, but not as a parenthesised list: the text to show.
    NotAList(String),
    /// The list opens at this offset into the line.
    At(usize),
}

/// Where the list `key` opens on `line`.
fn opening(line: &str, key: &str) -> Opening {
    let body = before_comment(line);
    let trimmed = body.trim_start();
    let lead = body.len() - trimmed.len();
    let Some(rest) = trimmed.strip_prefix(key) else { return Opening::Other };
    // `HOOKS` alone is not this key: `HOOKS_MICROCODE` is a different one.
    if !(rest.is_empty() || rest.starts_with('=') || rest.starts_with(char::is_whitespace)) {
        return Opening::Other;
    }
    let Some(equals) = rest.find('=') else { return Opening::Other };
    // The value starts after the `=`, with whatever is written between them.
    let after_equals = &rest[equals + 1..];
    let value = after_equals.trim_start();
    let at = lead + key.len() + equals + 1 + (after_equals.len() - value.len());
    if value.starts_with('(') {
        Opening::At(at)
    } else {
        Opening::NotAList(body[lead.min(body.len())..].trim_end().to_owned())
    }
}

/// The part of a line before any comment: from a `#` on, the line belongs to the comment,
/// which is how the shell that reads these files sees it too.
fn before_comment(line: &str) -> &str {
    line.split_once('#').map_or(line, |(before, _)| before)
}

/// The items in `region`, a run of a line without its comment, with where each one ends.
fn collect(region: &str, from: usize, line: usize, items: &mut Vec<Placed>) {
    let mut at = 0;
    while at < region.len() {
        let rest = &region[at..];
        let trimmed = rest.trim_start();
        at += rest.len() - trimmed.len();
        if at >= region.len() {
            return;
        }
        let word = trimmed.find(char::is_whitespace).unwrap_or(trimmed.len());
        items.push(Placed { name: trimmed[..word].to_owned(), line, end: from + at + word });
        at += word;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn list(text: &str) -> List {
        read(text, "HOOKS").expect("the list is understood")
    }

    /// Why a list could not be read.
    fn refused(text: &str) -> String {
        read(text, "HOOKS").err().expect("the list is not understood").unwrap_or_else(|| "not set".to_owned())
    }

    /// The whole file with `microcode` added after `autodetect`, the way the catalog adds it.
    fn added(text: &str) -> String {
        list(text).with("microcode", &["autodetect", "udev", "systemd"])
    }

    #[test]
    fn an_item_is_found_wherever_it_is_written() {
        let hooks = list("HOOKS=(base systemd autodetect microcode modconf)\n");
        assert!(hooks.has("microcode"));
        assert!(!hooks.has("lvm2"));
    }

    #[test]
    fn a_list_spread_over_lines_is_one_list() {
        let hooks = list("HOOKS=(base systemd\n       autodetect\n       modconf)\n");
        assert!(hooks.has("autodetect"));
        assert_eq!(
            hooks.with("microcode", &["autodetect", "udev", "systemd"]),
            "HOOKS=(base systemd\n       autodetect microcode\n       modconf)\n"
        );
    }

    #[test]
    fn an_item_goes_after_the_anchor_even_when_the_anchor_is_the_last_one_on_its_line() {
        let hooks = list("HOOKS=(base autodetect\n       modconf)\n");
        assert_eq!(hooks.with("microcode", &["autodetect"]), "HOOKS=(base autodetect microcode\n       modconf)\n");
    }

    #[test]
    fn without_an_anchor_the_item_goes_before_the_closing_parenthesis() {
        let hooks = list("HOOKS=(base modconf\n       kms)\n");
        assert_eq!(hooks.with("microcode", &["autodetect"]), "HOOKS=(base modconf\n       kms microcode)\n");
    }

    #[test]
    fn an_empty_list_takes_the_item() {
        assert_eq!(list("HOOKS=()\n").with("microcode", &["autodetect"]), "HOOKS=(microcode)\n");
    }

    #[test]
    fn a_commented_out_line_is_not_the_list() {
        assert_eq!(
            added("#HOOKS=(base modconf)\nHOOKS=(base autodetect modconf)\n"),
            "#HOOKS=(base modconf)\nHOOKS=(base autodetect microcode modconf)\n"
        );
    }

    #[test]
    fn a_comment_inside_the_list_is_neither_an_item_nor_a_place_to_put_one() {
        let hooks = list("HOOKS=(base autodetect\n       # the disk hooks come after microcode\n       modconf)\n");
        assert!(!hooks.has("the"));
        assert_eq!(
            hooks.with("microcode", &["autodetect"]),
            "HOOKS=(base autodetect microcode\n       # the disk hooks come after microcode\n       modconf)\n"
        );
    }

    #[test]
    fn a_closing_parenthesis_in_a_comment_does_not_end_the_list() {
        let hooks = list("HOOKS=(base autodetect\n       # ) closes nothing here\n       modconf)\n");
        assert!(hooks.has("modconf"));
    }
    #[test]
    fn a_comment_after_the_closing_parenthesis_is_left_where_it_is() {
        assert_eq!(
            added("HOOKS=(base autodetect modconf) # keep modconf last\n"),
            "HOOKS=(base autodetect microcode modconf) # keep modconf last\n"
        );
    }

    #[test]
    fn the_last_assignment_in_a_file_is_the_one_that_counts() {
        let hooks = list("HOOKS=(base autodetect)\nHOOKS=(base autodetect modconf)\n");
        assert!(hooks.has("modconf"));
        assert_eq!(
            hooks.with("microcode", &["autodetect"]),
            "HOOKS=(base autodetect)\nHOOKS=(base autodetect microcode modconf)\n"
        );
    }

    #[test]
    fn the_first_assignment_is_left_alone_when_a_later_one_is_the_list() {
        assert_eq!(
            added("HOOKS=base udev\nHOOKS=(base autodetect)\n"),
            "HOOKS=base udev\nHOOKS=(base autodetect microcode)\n"
        );
    }

    #[test]
    fn a_value_that_is_not_a_list_comes_back_as_the_text_that_stopped_us() {
        assert_eq!(refused("HOOKS=base udev\n"), "HOOKS=base udev");
        assert_eq!(refused("HOOKS = (base\n"), "HOOKS = (base");
    }

    #[test]
    fn a_list_that_is_never_closed_comes_back_as_the_rest_of_the_file() {
        assert_eq!(refused("MODULES=()\nHOOKS=(base udev\n\n"), "HOOKS=(base udev");
    }

    #[test]
    fn a_key_that_is_never_set_says_so() {
        assert_eq!(refused("MODULES=()\n"), "not set");
    }

    #[test]
    fn another_key_whose_name_starts_the_same_is_not_this_list() {
        assert_eq!(refused("HOOKS_MICROCODE=(base)\n"), "not set");
    }

    #[test]
    fn a_file_without_a_closing_line_keeps_its_last_line_as_it_is() {
        let hooks = list("HOOKS=(base autodetect modconf)");
        assert_eq!(hooks.with("microcode", &["autodetect"]), "HOOKS=(base autodetect microcode modconf)");
    }

    #[test]
    fn windows_line_endings_survive_an_added_item() {
        let hooks = list("HOOKS=(base\r\n       autodetect\r\n       modconf)\r\n");
        assert_eq!(
            hooks.with("microcode", &["autodetect"]),
            "HOOKS=(base\r\n       autodetect microcode\r\n       modconf)\r\n"
        );
    }
}
