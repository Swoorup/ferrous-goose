//! The regex crate for Goose: `import rx from "github.com/goose-rs/regex@v1";`.
//!
//! Matches are views of the text Goose passed (offsets come back and the
//! Goose wrapper re-slices its argument), so finding copies nothing.

use goose::GooseBuf;

/// A compiled regular expression. Release it with `rx::release` when done.
#[goose::handle]
pub struct Regex(regex::Regex);

/// How a pattern is compiled.
#[derive(goose::Flat, Clone, Copy, Debug, Default)]
#[repr(C)]
pub struct Options {
    pub case_insensitive: bool,
    pub multi_line: bool,
    pub dot_matches_new_line: bool,
    pub ignore_whitespace: bool,
    pub unicode: bool,
}

/// Compiles a pattern.
#[goose::export]
pub fn compile(pattern: &str) -> Result<Regex, regex::Error> {
    regex::Regex::new(pattern).map(Regex)
}

/// Compiles a pattern with options.
#[goose::export]
pub fn compile_with(pattern: &str, options: &Options) -> Result<Regex, regex::Error> {
    regex::RegexBuilder::new(pattern)
        .case_insensitive(options.case_insensitive)
        .multi_line(options.multi_line)
        .dot_matches_new_line(options.dot_matches_new_line)
        .ignore_whitespace(options.ignore_whitespace)
        .unicode(options.unicode)
        .build()
        .map(Regex)
}

/// Whether the pattern matches anywhere in the text.
#[goose::export]
pub fn is_match(re: &Regex, text: &str) -> bool {
    re.0.is_match(text)
}

/// The first match: a view of the text.
#[goose::export]
pub fn find<'t>(re: &Regex, text: &'t str) -> Option<&'t str> {
    re.0.find(text).map(|m| m.as_str())
}

/// The first match starting at byte `start` or later.
#[goose::export]
pub fn find_at<'t>(re: &Regex, text: &'t str, start: i64) -> Option<&'t str> {
    let start = (start.max(0) as usize).min(text.len());
    if !text.is_char_boundary(start) {
        return None;
    }
    re.0.find_at(text, start).map(|m| m.as_str())
}

/// Every match, each a view of the text.
#[goose::export]
pub fn find_all<'t>(re: &Regex, text: &'t str) -> impl Iterator<Item = &'t str> + 't {
    re.0.find_iter(text).map(|m| m.as_str()).collect::<Vec<_>>().into_iter()
}

/// How many matches there are.
#[goose::export]
pub fn count(re: &Regex, text: &str) -> i64 {
    re.0.find_iter(text).count() as i64
}

/// Capture group `group` of the first match (0 is the whole match), if the
/// pattern matched and the group took part.
#[goose::export]
pub fn group<'t>(re: &Regex, text: &'t str, group: i64) -> Option<&'t str> {
    let caps = re.0.captures(text)?;
    caps.get(group.max(0) as usize).map(|m| m.as_str())
}

/// The named capture group of the first match.
#[goose::export]
pub fn named<'t>(re: &Regex, text: &'t str, name: &str) -> Option<&'t str> {
    re.0.captures(text)?.name(name).map(|m| m.as_str())
}

/// How many capture groups the pattern has, the whole match included.
#[goose::export]
pub fn groups(re: &Regex) -> i64 {
    re.0.captures_len() as i64
}

/// The text with the first match replaced (`$1`, `$name` refer to groups).
#[goose::export]
pub fn replace(re: &Regex, text: &str, with: &str, out: &mut GooseBuf) {
    out.push_str(&re.0.replace(text, with));
}

/// The text with every match replaced.
#[goose::export]
pub fn replace_all(re: &Regex, text: &str, with: &str, out: &mut GooseBuf) {
    out.push_str(&re.0.replace_all(text, with));
}

/// The parts of the text between matches.
#[goose::export]
pub fn split<'t>(re: &Regex, text: &'t str) -> impl Iterator<Item = &'t str> + 't {
    re.0.split(text).collect::<Vec<_>>().into_iter()
}

/// The pattern the regex was compiled from.
#[goose::export]
pub fn pattern(re: &Regex) -> &str {
    re.0.as_str()
}

/// The text with every character special to a pattern escaped.
#[goose::export]
pub fn escape(text: &str) -> String {
    regex::escape(text)
}

/// The version of the regex crate inside.
#[goose::export]
pub fn version(out: &mut GooseBuf) {
    out.push_str("regex 1, through Ferrous Goose ");
    out.push_str(env!("CARGO_PKG_VERSION"));
}
