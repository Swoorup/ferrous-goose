//! The regex crate, marked for Goose. No wrapper code beyond the attributes.

use goose::GooseBuf;

/// A compiled regular expression. Release it when done.
#[goose::handle]
pub struct Regex(regex::Regex);

/// Compiles a pattern.
#[goose::export]
pub fn compile(pattern: &str) -> Result<Regex, regex::Error> {
    regex::Regex::new(pattern).map(Regex)
}

/// Whether the pattern matches anywhere in the text.
#[goose::export]
pub fn is_match(re: &Regex, text: &str) -> bool {
    re.0.is_match(text)
}

/// The first match: a view of the text, not a copy.
#[goose::export]
pub fn find<'t>(re: &Regex, text: &'t str) -> Option<&'t str> {
    re.0.find(text).map(|m| m.as_str())
}

/// Every match, each a view of the text.
#[goose::export]
pub fn find_all<'t>(re: &Regex, text: &'t str) -> impl Iterator<Item = &'t str> + 't {
    let re = re.0.clone();
    re.find_iter(text).map(|m| m.as_str()).collect::<Vec<_>>().into_iter()
}

/// The parts of the text between matches.
#[goose::export]
pub fn split<'t>(re: &Regex, text: &'t str) -> impl Iterator<Item = &'t str> + 't {
    re.0.split(text).collect::<Vec<_>>().into_iter()
}

/// The text with every match replaced (`$1`, `$name` refer to groups).
#[goose::export]
pub fn replace_all(re: &Regex, text: &str, with: &str, out: &mut GooseBuf) {
    out.push_str(&re.0.replace_all(text, with));
}

/// How many matches there are.
#[goose::export]
pub fn count(re: &Regex, text: &str) -> i64 {
    re.0.find_iter(text).count() as i64
}

/// The pattern the regex was compiled from (copied out: it is the Rust
/// object's, not the caller's).
#[goose::export]
pub fn pattern(re: &Regex) -> &str {
    re.0.as_str()
}

/// Escapes the text so it matches literally.
#[goose::export]
pub fn escape(text: &str) -> String {
    regex::escape(text)
}
