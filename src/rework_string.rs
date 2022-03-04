// Format string literals.

use itertools::Itertools;
use regex::Regex;
use unicode_categories::UnicodeCategories;
use unicode_linebreak::{linebreaks, BreakOpportunity};
use unicode_segmentation::UnicodeSegmentation;

use crate::config::Config;
use crate::shape::Shape;
use crate::utils::{unicode_str_width, wrap_str};

/// Describes the layout of a piece of text.
pub(crate) struct StringFormat<'a> {
    /// The opening sequence of characters for the piece of text
    pub(crate) opener: &'a str,
    /// The closing sequence of characters for the piece of text
    pub(crate) closer: &'a str,
    /// The opening sequence of characters for a line
    pub(crate) line_start: &'a str,
    /// The closing sequence of characters for a line
    pub(crate) line_end: &'a str,
    /// The allocated box to fit the text into
    pub(crate) shape: Shape,
    /// Trim trailing whitespaces
    pub(crate) trim_end: bool,
    pub(crate) config: &'a Config,
}

impl<'a> StringFormat<'a> {
    pub(crate) fn new(shape: Shape, config: &'a Config) -> StringFormat<'a> {
        StringFormat {
            opener: "\"",
            closer: "\"",
            line_start: " ",
            line_end: "\\",
            shape,
            trim_end: false,
            config,
        }
    }

    /// Returns the maximum number of graphemes that is possible on a line while taking the
    /// indentation into account.
    ///
    /// If we cannot put at least a single character per line, the rewrite won't succeed.
    fn max_width_with_indent(&self) -> Option<usize> {
        Some(
            self.shape
                .width
                .checked_sub(self.opener.len() + self.line_end.len() + 1)?
                + 1,
        )
    }

    /// Like max_width_with_indent but the indentation is not subtracted.
    /// This allows to fit more graphemes from the string on a line when
    /// SnippetState::EndWithLineFeed.
    fn max_width_without_indent(&self) -> Option<usize> {
        self.config.max_width().checked_sub(self.line_end.len())
    }
}

/// check if the input text contains a URL
fn contains_url(text: &str) -> bool {
    text.contains("https://")
        || text.contains("http://")
        || text.contains("ftp://")
        || text.contains("file://")
}

/// Follow the Unicode Line Breaking Algorithm [UAX#14] to find valid byte positions
/// where we can safely break the string on grapheme boundaries.
///
/// [UAX#14]: http://unicode.org/reports/tr14/
fn line_break_opportunities(
    text: &str,
    max_graphemes: usize,
    trim_end: bool,
) -> impl Iterator<Item = (usize, BreakOpportunity)> + '_ {
    linebreaks(text).filter(move |(byte_idx, _)| {
        let snippet = &text[..*byte_idx];
        let ends_with_escape = snippet.ends_with('\\');
        let ends_with_dash = snippet.ends_with(UnicodeCategories::is_punctuation_dash);

        // '\\' characters are odd when broken. If we break in the middle
        // '\\' becomes '\\\n\\', because each '\' needs to be escaped itself.
        //  For that reason it's best to dissalow breaking on '\\' altogether.
        if ends_with_escape || ends_with_dash {
            return false
        }

        let width = unicode_str_width(snippet);

        if width <= max_graphemes {
            true
        } else if width == max_graphemes + 1 {
                // Althrough we're technically one column past the boundary we still want to
                // consider this break if We're on a whitespace character that can be trimmed.
                trim_end && snippet.ends_with(UnicodeCategories::is_separator_space)
        } else {
            // We're past the max width boundary, don't consider any of these breaks
            false
        }
    })
}

/// Check if the grapheme is a member of the Unicode "Punctuation, Other" (Po) category.
fn is_punctuation(grapheme: &str) -> bool {
    grapheme
        .chars()
        .all(|c| UnicodeCategories::is_punctuation_other(c))
}

/// Find graphems in the [Punctuation, Other] (Po) category.
/// These graphemes can also be considered as valid locations for rustfmt to break strings.
/// [Punctuation, Other]: https://www.compart.com/en/unicode/category/Po
fn alternative_punctuation_breaks<'text>(
    text: &'text str,
    max_graphemes: usize,
    trim_end: bool,
) -> impl Iterator<Item = usize> + 'text {
    UnicodeSegmentation::grapheme_indices(text, true)
        .tuple_windows()
        .filter_map(move |(curr, next)| {
            if !is_punctuation(curr.1)
                || !trim_end && is_whitespace(next.1)
                || is_punctuation(next.1)
            {
                // We don't want to consider this grapheme if:
                // - It is not a Unicode punctuation character
                // - The break would occur on a whitespace character we can't trim
                // - The next character is also a punctuation.
                //   For example, We don't want to break "!!" -> "!\n!"
                return None;
            }

            if unicode_str_width(&text[..next.0]) <= max_graphemes {
                Some(next.0)
            } else {
                None
            }
        })
}

/// Remove all whitespace from an owned string
fn trim_end_whitespace(mut text: String) -> String {
    while text.ends_with(|c: char| c.is_whitespace()) {
        text.pop();
    }
    text
}

/// Remove all whitespace except a single trailing newline when trim_end is true.
/// If trim end is false no whitespace is removed from the end of the string.
///
/// ```rust
/// // don't trim end
/// let text = "some string with trailing whitespace          \n".to_owned();
/// let not_trimmed = trim_end_but_line_feed(text, false);
/// assert_eq!(text, not_trimmed);
///
/// //trim the string
/// let trimmed = trim_end_but_line_feed(text, true);
/// assert_eq!(trimmed, "some string with trailing whitespace\n");
/// ```
fn trim_end_but_line_feed(mut text: String, trim_end: bool) -> String {
    if !trim_end {
        return text;
    }

    let ends_with_newline = text.ends_with('\n');
    text = trim_end_whitespace(text);

    if ends_with_newline {
        text.push('\n')
    }

    text
}

struct StringSplitter<'a> {
    text: &'a str,
    offset: usize,
    max_graphemes: usize,
    max_graphemes_with_indent: usize,
    max_graphemes_without_indent: usize,
    newline_max_graphemes: usize,
    is_bareline_ok: bool,
    trim_end: bool,
    contains_url: bool,
}

impl<'a> StringSplitter<'a> {
    fn from_format(
        text: &'a str,
        fmt: &'a StringFormat<'a>,
        newline_max_graphemes: usize,
    ) -> Option<Self> {
        let max_graphemes_with_indent = fmt.max_width_with_indent()?;
        let max_graphemes_without_indent = fmt.max_width_without_indent()?;
        let is_bareline_ok = fmt.line_start.is_empty() || is_whitespace(fmt.line_start);

        let contains_url = contains_url(text);
        Some(Self {
            text,
            offset: 0,
            max_graphemes: max_graphemes_with_indent,
            max_graphemes_with_indent,
            max_graphemes_without_indent,
            newline_max_graphemes,
            is_bareline_ok,
            trim_end: fmt.trim_end,
            contains_url,
        })
    }

    fn is_bareline_ok(&self) -> bool {
        self.is_bareline_ok
    }

    fn update(&mut self, mut split_index: usize) -> SnippetState<'a> {
        // adjust the index in case there was a URL in the text
        if self.contains_url {
            if let Some(i) = safe_break_after_url(self.text) {
                if i > split_index {
                    split_index = i;
                }
                // we've moved passed the url so we no longer need to check for it
                self.contains_url = contains_url(self.text[i..])
            }
        }

        let (mut text, remainder) = self.text.split_at(split_index);
        self.text = remainder;
        self.offset += split_index;

        let ends_with_newline = text.ends_with("\n");
        let can_trim_end = self.trim_end && !ends_with_newline;

        if can_trim_end {
            text = text.trim_end()
        }

        if remainder.is_empty() {
            SnippetState::EndOfInput(text)
        } else if !self.trim_end && ends_with_newline {
            if self.is_bareline_ok {
                // the next line can benefit from the full width
                self.max_graphemes = self.max_graphemes_without_indent;
            } else {
                self.max_graphemes = self.max_graphemes_with_indent;
            }
            SnippetState::EndWithLineFeed(text, self.offset)
        } else if self.trim_end && ends_with_newline {
            if self.is_bareline_ok {
                // the next line can benefit from the full width
                self.max_graphemes = self.max_graphemes_without_indent;
            } else {
                self.max_graphemes = self.max_graphemes_with_indent;
            }
            SnippetState::RemovedSignificantLineFeed(text.trim_end(), self.offset)
        } else {
            self.max_graphemes = self.newline_max_graphemes;
            SnippetState::LineEnd(text, self.offset)
        }
    }
}

/// Iterate over the string at valid break points.
/// Mandatory and Allowed Break opportunities are determined by using the [UAX#14] algorithm
/// implemented by [unicode_linebreak].
///
/// Additional line breaks are found by checking for punctuation characters in the string.
/// punctuation characters are determined using [UnicodeCategories::is_punctuation_other].
/// Here is a helpful reference on [Other Punctuation] unicode characters.
///
/// [UAX#14]: http://unicode.org/reports/tr14/
/// [unicode_linebreak]: https://crates.io/crates/unicode-linebreak
/// [Other Punctuation]: https://www.compart.com/en/unicode/category/Po
impl<'a> Iterator for StringSplitter<'a> {
    type Item = SnippetState<'a>;
    fn next(&mut self) -> Option<Self::Item> {
        if self.text.is_empty() {
            return None;
        }

        let break_opportunities =
            line_break_opportunities(self.text, self.max_graphemes, self.trim_end);
        let mut allowed_break_idx: usize = 0;

        for (byte_idx, break_opportunity) in break_opportunities {
            if break_opportunity == BreakOpportunity::Mandatory {
                return Some(self.update(byte_idx));
            }
            allowed_break_idx = byte_idx;
        }

        let punctuation_breaks =
            alternative_punctuation_breaks(self.text, self.max_graphemes, self.trim_end);

        if allowed_break_idx != 0 {
            Some(self.update(allowed_break_idx))
        } else if let Some(punctuation_break) = punctuation_breaks.last() {
            Some(self.update(punctuation_break))
        } else {
            // couldn't find any place to break the string
            Some(self.update(self.text.len()))
        }
    }
}

pub(crate) fn rewrite_string<'a>(
    orig: &str,
    fmt: &StringFormat<'a>,
    newline_max_chars: usize,
) -> Option<String> {
    let indent_with_newline = fmt.shape.indent.to_string_with_newline(fmt.config);
    let indent_without_newline = fmt.shape.indent.to_string(fmt.config);
    // Strip line breaks.
    // With this regex applied, all remaining whitespaces are significant
    let strip_line_breaks_re = Regex::new(r"([^\\](\\\\)*)\\[\n\r][[:space:]]*").unwrap();
    let stripped_str = strip_line_breaks_re.replace_all(orig, "$1");
    let mut result = String::with_capacity(
        stripped_str
            .len()
            .checked_next_power_of_two()
            .unwrap_or(usize::max_value()),
    );
    result.push_str(fmt.opener);
    let str_splitter = StringSplitter::from_format(&stripped_str, fmt, newline_max_chars)?;
    let is_bareline_ok = str_splitter.is_bareline_ok();

    for snippet_state in str_splitter {
        debug!("SNIPPET STATE: {:?}", &snippet_state);
        match snippet_state {
            SnippetState::LineEnd(line, _) => {
                debug!("unicode_str_width: {}", unicode_str_width(&line));
                result.push_str(&line);
                result.push_str(fmt.line_end);
                result.push_str(&indent_with_newline);
                result.push_str(fmt.line_start);
            }
            SnippetState::EndWithLineFeed(line, _) => {
                if line == "\n" && fmt.trim_end {
                    result = trim_end_but_line_feed(result, true);
                }
                result.push_str(&line);
                if !is_bareline_ok {
                    result.push_str(&indent_without_newline);
                    result.push_str(fmt.line_start);
                }
            }
            SnippetState::RemovedSignificantLineFeed(line, _) => {
                if line == "" {
                    result = trim_end_but_line_feed(result, true);
                    result.push('\n')
                } else {
                    result.push_str(line);
                    result.push('\n')
                }
                if !is_bareline_ok {
                    result.push_str(&indent_without_newline);
                    result.push_str(fmt.line_start);
                }
            }
            SnippetState::EndOfInput(line) => {
                if line == "\n" {
                    result = trim_end_but_line_feed(result, true);
                }
                result.push_str(&line);
            }
        }
    }

    result.push_str(fmt.closer);
    wrap_str(result, fmt.config.max_width(), fmt.shape)
}

// find the next break opportunity after a URL if it exists
fn safe_break_after_url(s: &str) -> Option<usize> {
    if !contains_url(s) {
        return None;
    }

    let byte_index = s.find("://")?;

    // there shouldn't be any whitespace in a URL. we want to break at the first
    // whitespace char we find or at the end of the string
    match s[byte_index..].find(char::is_whitespace) {
        Some(pos) => linebreaks(s)
            .filter(|(i, _)| *i >= byte_index + pos)
            .next()
            .and_then(|(i, _)| Some(i)),
        None => Some(s.len()),
    }
}

/// Result of breaking a string so it fits in a line and the state it ended in.
/// The state informs about what to do with the snippet and how to continue the breaking process.
#[derive(Debug, PartialEq)]
enum SnippetState<'a> {
    /// The input could not be broken and so rewriting the string is finished.
    /// if the bool is true it means a significant newline must be added back to the ouput.
    EndOfInput(&'a str),
    /// The input could be broken and the returned snippet should be ended with a
    /// `[StringFormat::line_end]`. The next snippet needs to be indented.
    ///
    /// The returned string is the line to print out and the number is the length that got read in
    /// the text being rewritten. That length may be greater than the returned string if trailing
    /// whitespaces got trimmed.
    LineEnd(&'a str, usize),
    /// The input could be broken but a newline is present that cannot be trimmed. The next snippet
    /// to be rewritten *could* use more width than what is specified by the given shape. For
    /// example with a multiline string, the next snippet does not need to be indented, allowing
    /// more characters to be fit within a line.
    ///
    /// The returned string is the line to print out and the number is the length that got read in
    /// the text being rewritten.
    EndWithLineFeed(&'a str, usize),
    RemovedSignificantLineFeed(&'a str, usize),
}

fn is_whitespace(grapheme: &str) -> bool {
    grapheme.chars().all(char::is_whitespace)
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::config::Config;
    use crate::shape::{Indent, Shape};

    #[test]
    fn issue343() {
        let config = Default::default();
        let fmt = StringFormat::new(Shape::legacy(2, Indent::empty()), &config);
        rewrite_string("eq_", &fmt, 2);
    }

    // #[test]
    // fn should_break_on_whitespace() {
    //     let string = "Placerat felis. Mauris porta ante sagittis purus.";
    //     let graphemes = UnicodeSegmentation::graphemes(&*string, false).collect::<Vec<&str>>();
    //     assert_eq!(
    //         break_string(20, false, "", &graphemes[..]),
    //         SnippetState::LineEnd("Placerat felis. ".to_string(), 16)
    //     );
    //     assert_eq!(
    //         break_string(20, true, "", &graphemes[..]),
    //         SnippetState::LineEnd("Placerat felis.".to_string(), 16)
    //     );
    // }

    // #[test]
    // fn should_break_on_punctuation() {
    //     let string = "Placerat_felis._Mauris_porta_ante_sagittis_purus.";
    //     let graphemes = UnicodeSegmentation::graphemes(&*string, false).collect::<Vec<&str>>();
    //     assert_eq!(
    //         break_string(20, false, "", &graphemes[..]),
    //         SnippetState::LineEnd("Placerat_felis.".to_string(), 15)
    //     );
    // }

    // #[test]
    // fn should_break_forward() {
    //     let string = "Venenatis_tellus_vel_tellus. Aliquam aliquam dolor at justo.";
    //     let graphemes = UnicodeSegmentation::graphemes(&*string, false).collect::<Vec<&str>>();
    //     assert_eq!(
    //         break_string(20, false, "", &graphemes[..]),
    //         SnippetState::LineEnd("Venenatis_tellus_vel_tellus. ".to_string(), 29)
    //     );
    //     assert_eq!(
    //         break_string(20, true, "", &graphemes[..]),
    //         SnippetState::LineEnd("Venenatis_tellus_vel_tellus.".to_string(), 29)
    //     );
    // }

    // #[test]
    // fn nothing_to_break() {
    //     let string = "Venenatis_tellus_vel_tellus";
    //     let graphemes = UnicodeSegmentation::graphemes(&*string, false).collect::<Vec<&str>>();
    //     assert_eq!(
    //         break_string(20, false, "", &graphemes[..]),
    //         SnippetState::EndOfInput("Venenatis_tellus_vel_tellus".to_string())
    //     );
    // }

    // #[test]
    // fn significant_whitespaces() {
    //     let string = "Neque in sem.      \n      Pellentesque tellus augue.";
    //     let graphemes = UnicodeSegmentation::graphemes(&*string, false).collect::<Vec<&str>>();
    //     assert_eq!(
    //         break_string(15, false, "", &graphemes[..]),
    //         SnippetState::EndWithLineFeed("Neque in sem.      \n".to_string(), 20)
    //     );
    //     assert_eq!(
    //         break_string(25, false, "", &graphemes[..]),
    //         SnippetState::EndWithLineFeed("Neque in sem.      \n".to_string(), 20)
    //     );

    //     assert_eq!(
    //         break_string(15, true, "", &graphemes[..]),
    //         SnippetState::LineEnd("Neque in sem.".to_string(), 19)
    //     );
    //     assert_eq!(
    //         break_string(25, true, "", &graphemes[..]),
    //         SnippetState::EndWithLineFeed("Neque in sem.\n".to_string(), 20)
    //     );
    // }

    // #[test]
    // fn big_whitespace() {
    //     let string = "Neque in sem.            Pellentesque tellus augue.";
    //     let graphemes = UnicodeSegmentation::graphemes(&*string, false).collect::<Vec<&str>>();
    //     assert_eq!(
    //         break_string(20, false, "", &graphemes[..]),
    //         SnippetState::LineEnd("Neque in sem.            ".to_string(), 25)
    //     );
    //     assert_eq!(
    //         break_string(20, true, "", &graphemes[..]),
    //         SnippetState::LineEnd("Neque in sem.".to_string(), 25)
    //     );
    // }

    #[test]
    fn big_whitespace_with_snippet_state_iterator() {
        let config: Config = Default::default();
        let fmt = StringFormat::new(Shape::legacy(27, Indent::empty()), &config);

        let text = "Neque in sem.            Pellentesque tellus augue.";
        let mut snippter_state = StringSplitter::from_format(text, &fmt, 27).unwrap();
        assert_eq!(
            Some(SnippetState::LineEnd("Neque in sem.            ", 25)),
            snippter_state.next()
        );
        assert_eq!(
            Some(SnippetState::EndOfInput("Pellentesque tellus augue.")),
            snippter_state.next()
        );
        assert_eq!(None, snippter_state.next());
    }

    #[test]
    fn newline_in_candidate_line() {
        let mut config: Config = Default::default();
        config.set().max_width(27);
        let fmt = StringFormat::new(Shape::legacy(25, Indent::empty()), &config);
        let string = "Nulla\nconsequat erat at massa. Vivamus id mi.";
        let rewritten_string = rewrite_string(string, &fmt, 27);
        assert_eq!(
            rewritten_string,
            Some("\"Nulla\nconsequat erat at massa. \\\n Vivamus id mi.\"".to_string())
        );
    }

    #[test]
    fn newline_in_candidate_line_with_snippet_state_iterator() {
        let mut config: Config = Default::default();
        config.set().max_width(27);
        let fmt = StringFormat::new(Shape::legacy(25, Indent::empty()), &config);

        let text = "Nulla\nconsequat erat at massa. Vivamus id mi.";
        let mut snippter_state = StringSplitter::from_format(text, &fmt, 27).unwrap();
        assert_eq!(
            Some(SnippetState::EndWithLineFeed("Nulla\n", 6)),
            snippter_state.next()
        );
        assert_eq!(
            Some(SnippetState::LineEnd("consequat erat at massa. ", 31)),
            snippter_state.next()
        );
        assert_eq!(
            Some(SnippetState::EndOfInput("Vivamus id mi.")),
            snippter_state.next()
        );
        assert_eq!(None, snippter_state.next());
    }

    #[test]
    fn last_line_fit_with_trailing_whitespaces() {
        let string = "Vivamus id mi.  ";
        let config: Config = Default::default();
        let mut fmt = StringFormat::new(Shape::legacy(25, Indent::empty()), &config);

        fmt.trim_end = true;
        let rewritten_string = rewrite_string(string, &fmt, 25);
        assert_eq!(rewritten_string, Some("\"Vivamus id mi.\"".to_string()));

        fmt.trim_end = false; // default value of trim_end
        let rewritten_string = rewrite_string(string, &fmt, 25);
        assert_eq!(rewritten_string, Some("\"Vivamus id mi.  \"".to_string()));
    }

    #[test]
    fn last_line_fit_with_newline() {
        let string = "Vivamus id mi.\nVivamus id mi.";
        let config: Config = Default::default();
        let fmt = StringFormat {
            opener: "",
            closer: "",
            line_start: "// ",
            line_end: "",
            shape: Shape::legacy(100, Indent::from_width(&config, 4)),
            trim_end: true,
            config: &config,
        };

        let rewritten_string = rewrite_string(string, &fmt, 100);
        assert_eq!(
            rewritten_string,
            Some("Vivamus id mi.\n    // Vivamus id mi.".to_string())
        );
    }

    #[test]
    fn overflow_in_non_string_content() {
        let comment = "Aenean metus.\nVestibulum ac lacus. Vivamus porttitor";
        let config: Config = Default::default();
        let fmt = StringFormat {
            opener: "",
            closer: "",
            line_start: "// ",
            line_end: "",
            shape: Shape::legacy(30, Indent::from_width(&config, 8)),
            trim_end: true,
            config: &config,
        };

        assert_eq!(
            rewrite_string(comment, &fmt, 30),
            Some(
                "Aenean metus.\n        // Vestibulum ac lacus. Vivamus\n        // porttitor"
                    .to_string()
            )
        );
    }

    #[test]
    fn overflow_in_non_string_content_with_line_end() {
        let comment = "Aenean metus.\nVestibulum ac lacus. Vivamus porttitor";
        let config: Config = Default::default();
        let fmt = StringFormat {
            opener: "",
            closer: "",
            line_start: "// ",
            line_end: "@",
            shape: Shape::legacy(30, Indent::from_width(&config, 8)),
            trim_end: true,
            config: &config,
        };

        assert_eq!(
            rewrite_string(comment, &fmt, 30),
            Some(
                "Aenean metus.\n        // Vestibulum ac lacus. Vivamus@\n        // porttitor"
                    .to_string()
            )
        );
    }

    #[test]
    fn blank_line_with_non_empty_line_start() {
        let config: Config = Default::default();
        let mut fmt = StringFormat {
            opener: "",
            closer: "",
            line_start: "// ",
            line_end: "",
            shape: Shape::legacy(30, Indent::from_width(&config, 4)),
            trim_end: true,
            config: &config,
        };

        let comment = "Aenean metus. Vestibulum\n\nac lacus. Vivamus porttitor";
        assert_eq!(
            rewrite_string(comment, &fmt, 30),
            Some(
                "Aenean metus. Vestibulum\n    //\n    // ac lacus. Vivamus porttitor".to_string()
            )
        );

        fmt.shape = Shape::legacy(18, Indent::from_width(&config, 4));
        let comment = "Aenean\n\nmetus. Vestibulum ac lacus. Vivamus porttitor";
        assert_eq!(
            rewrite_string(comment, &fmt, 18),
            Some(
                r#"Aenean
    //
    // metus. Vestibulum
    // ac lacus. Vivamus
    // porttitor"#
                    .to_string()
            )
        );
    }

    #[test]
    fn retain_blank_lines() {
        // env_logger::Builder::from_env("RUSTFMT_LOG").init();
        let config: Config = Default::default();
        let fmt = StringFormat {
            opener: "",
            closer: "",
            line_start: "// ",
            line_end: "",
            shape: Shape::legacy(20, Indent::from_width(&config, 4)),
            trim_end: true,
            config: &config,
        };

        let comment = "Aenean\n\nmetus. Vestibulum ac lacus.\n\n";
        assert_eq!(
            rewrite_string(comment, &fmt, 20),
            Some(
                "Aenean\n    //\n    // metus. Vestibulum ac\n    // lacus.\n    //\n".to_string()
            )
        );

        let comment = "Aenean\n\nmetus. Vestibulum ac lacus.\n";
        assert_eq!(
            rewrite_string(comment, &fmt, 20),
            Some("Aenean\n    //\n    // metus. Vestibulum ac\n    // lacus.\n".to_string())
        );

        let comment = "Aenean\n        \nmetus. Vestibulum ac lacus.";
        assert_eq!(
            rewrite_string(comment, &fmt, 20),
            Some("Aenean\n    //\n    // metus. Vestibulum ac\n    // lacus.".to_string())
        );
    }

    #[test]
    fn boundary_on_edge() {
        let config: Config = Default::default();
        let mut fmt = StringFormat {
            opener: "",
            closer: "",
            line_start: "// ",
            line_end: "",
            shape: Shape::legacy(13, Indent::from_width(&config, 4)),
            trim_end: true,
            config: &config,
        };

        let comment = "Aenean metus. Vestibulum ac lacus.";
        assert_eq!(
            rewrite_string(comment, &fmt, 13),
            Some("Aenean metus.\n    // Vestibulum ac\n    // lacus.".to_string())
        );

        fmt.trim_end = false;
        let comment = "Vestibulum ac lacus.";
        // with a width of 13 and a line end of "" we have a max of 13 columns to fill
        // because we can't trim the end, we should treat all whitespace as significant
        assert_eq!(
            rewrite_string(comment, &fmt, 13),
            Some("Vestibulum \n    // ac lacus.".to_string())
        );

        fmt.trim_end = true;
        fmt.line_end = "\\";
        let comment = "Vestibulum ac lacus.";
        // with a width of 13 and a line end of "\\" we have a max of 11 columns to fill
        // here's we're allowed to trim whitespace
        assert_eq!(
            rewrite_string(comment, &fmt, 13),
            Some("Vestibulum\\\n    // ac lacus.".to_string())
        );
    }

    #[test]
    fn detect_urls() {
        let string = "aaa http://example.org something";
        assert_eq!(safe_break_after_url(string), Some(23));
        assert!(string[..23].ends_with("http://example.org "));

        let string = "https://example.org something";
        assert_eq!(safe_break_after_url(string), Some(20));
        assert!(string[..20].ends_with("https://example.org "));

        let string = "aaa ftp://example.org something";
        assert_eq!(safe_break_after_url(string), Some(22));
        assert!(string[..22].ends_with("ftp://example.org "));

        let string = "aaa file://example.org something";
        assert_eq!(safe_break_after_url(string), Some(23));
        assert!(string[..23].ends_with("file://example.org "));

        let string = "aaa http not an url";
        assert_eq!(safe_break_after_url(string), None);

        let string = "aaa file://example.org";
        assert_eq!(safe_break_after_url(string), Some(22));
        assert!(string[..22].ends_with("file://example.org"));

        let string =
            "우리 모두가 만들어가는 자유 백과사전 http://ko.wikipedia.org/wiki/위키백과:대문";
        assert_eq!(safe_break_after_url(string), Some(101));
        assert!(string[..101].ends_with("http://ko.wikipedia.org/wiki/위키백과:대문"));
    }
}
