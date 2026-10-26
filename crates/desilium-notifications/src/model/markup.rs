//! Sanitiser for the freedesktop body markup subset.
//!
//! Only `<b>`, `<i>`, `<u>`, `<a href>` and `<br>` survive. Every other tag,
//! including `<img>`, is dropped while its text is kept. The parser never
//! uses regular expressions, never emits raw markup, and stops once the
//! output reaches the requested character cap.

use super::limits::SUMMARY_MAX_CHARS;

/// Text emphasis carried by a span.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Style {
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
}

/// A run of text with one style and an optional hyperlink.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Span {
    pub text: String,
    pub style: Style,
    pub link: Option<String>,
}

const MAX_HREF_CHARS: usize = 2048;
/// Longest entity we accept, `&#x10FFFF;` fits comfortably.
const MAX_ENTITY_BYTES: usize = 12;
const ALLOWED_SCHEMES: [&str; 3] = ["http://", "https://", "mailto:"];

/// Parses `input` into styled spans, producing at most `max_chars` characters of text.
pub fn parse(input: &str, max_chars: usize) -> Vec<Span> {
    let mut builder = Builder::new(max_chars);
    let mut rest = input;
    while !rest.is_empty() && !builder.is_full() {
        rest = match rest.find('<') {
            None => {
                builder.text(rest);
                ""
            }
            Some(0) => builder.tag(rest),
            Some(at) => {
                builder.text(rest.get(..at).unwrap_or_default());
                rest.get(at..).unwrap_or_default()
            }
        };
    }
    builder.finish()
}

/// Flattens spans into plain text, dropping all styling.
pub fn plain(spans: &[Span]) -> String {
    spans.iter().map(|span| span.text.as_str()).collect()
}

/// Nesting depth per emphasis tag, so `<b><b>x</b>y</b>` keeps `y` bold.
#[derive(Default)]
struct Depth {
    bold: u32,
    italic: u32,
    underline: u32,
}

struct Builder {
    spans: Vec<Span>,
    pending: String,
    depth: Depth,
    link: Option<String>,
    chars: usize,
    max_chars: usize,
}

impl Builder {
    fn new(max_chars: usize) -> Self {
        Self {
            spans: Vec::new(),
            pending: String::new(),
            depth: Depth::default(),
            link: None,
            chars: 0,
            max_chars,
        }
    }

    fn is_full(&self) -> bool {
        self.chars >= self.max_chars
    }

    fn style(&self) -> Style {
        Style {
            bold: self.depth.bold > 0,
            italic: self.depth.italic > 0,
            underline: self.depth.underline > 0,
        }
    }

    /// Ends the pending run so that a style change applies only to what follows.
    fn flush(&mut self) {
        if self.pending.is_empty() {
            return;
        }
        let text = std::mem::take(&mut self.pending);
        let span = Span {
            text,
            style: self.style(),
            link: self.link.clone(),
        };
        self.spans.push(span);
    }

    fn push_char(&mut self, c: char) {
        if self.is_full() {
            return;
        }
        let normalised = if c == '\t' { ' ' } else { c };
        if normalised.is_control() && normalised != '\n' {
            return;
        }
        self.pending.push(normalised);
        self.chars += 1;
    }

    fn text(&mut self, raw: &str) {
        for c in decode_entities(raw) {
            if self.is_full() {
                return;
            }
            self.push_char(c);
        }
    }

    /// Consumes one tag (or a stray `<`) at the start of `rest` and returns the remainder.
    fn tag<'a>(&mut self, rest: &'a str) -> &'a str {
        let Some(end) = rest.find('>') else {
            self.push_char('<');
            return rest.get(1..).unwrap_or_default();
        };
        let inner = rest.get(1..end).unwrap_or_default();
        let (closing, body) = match inner.strip_prefix('/') {
            Some(body) => (true, body),
            None => (false, inner),
        };
        let name_len = body.bytes().take_while(u8::is_ascii_alphanumeric).count();
        let starts_with_letter = body.bytes().next().is_some_and(|b| b.is_ascii_alphabetic());
        if !starts_with_letter {
            self.push_char('<');
            return rest.get(1..).unwrap_or_default();
        }
        let name = body
            .get(..name_len)
            .unwrap_or_default()
            .to_ascii_lowercase();
        self.apply(&name, closing, body.get(name_len..).unwrap_or_default());
        rest.get(end + 1..).unwrap_or_default()
    }

    fn apply(&mut self, name: &str, closing: bool, attributes: &str) {
        match name {
            "b" | "i" | "u" => {
                self.flush();
                self.adjust_depth(name, closing);
            }
            "a" => {
                self.flush();
                self.link = if closing { None } else { href(attributes) };
            }
            "br" if !closing => self.push_char('\n'),
            _ => {}
        }
    }

    fn adjust_depth(&mut self, name: &str, closing: bool) {
        let counter = match name {
            "b" => &mut self.depth.bold,
            "i" => &mut self.depth.italic,
            _ => &mut self.depth.underline,
        };
        let next = if closing {
            counter.saturating_sub(1)
        } else {
            counter.saturating_add(1)
        };
        *counter = next;
    }

    fn finish(mut self) -> Vec<Span> {
        self.flush();
        self.spans
    }
}

/// Extracts a whitelisted `href` value from a tag's attribute text.
fn href(attributes: &str) -> Option<String> {
    let lower = attributes.to_ascii_lowercase();
    let at = lower.find("href")? + "href".len();
    let value = attributes
        .get(at..)?
        .trim_start()
        .strip_prefix('=')?
        .trim_start();
    let quote = value.chars().next()?;
    let raw = if quote == '"' || quote == '\'' {
        value.get(1..)?.split(quote).next()?
    } else {
        value.split_whitespace().next()?
    };
    let decoded: String = decode_entities(raw).into_iter().collect();
    safe_href(&decoded)
}

/// Accepts only web and mail links, so a notification cannot launch local handlers.
fn safe_href(url: &str) -> Option<String> {
    let trimmed = url.trim();
    if trimmed.chars().count() > MAX_HREF_CHARS || trimmed.chars().any(char::is_control) {
        return None;
    }
    let lower = trimmed.to_ascii_lowercase();
    ALLOWED_SCHEMES
        .iter()
        .any(|scheme| lower.starts_with(scheme))
        .then(|| trimmed.to_string())
}

/// Decodes character references in a text run; unknown references stay literal.
fn decode_entities(raw: &str) -> Vec<char> {
    let mut out = Vec::with_capacity(raw.len());
    let mut rest = raw;
    while let Some(c) = rest.chars().next() {
        if c == '&'
            && let Some((decoded, used)) = decode_entity(rest)
        {
            out.push(decoded);
            rest = rest.get(used..).unwrap_or_default();
            continue;
        }
        out.push(c);
        rest = rest.get(c.len_utf8()..).unwrap_or_default();
    }
    out
}

fn decode_entity(rest: &str) -> Option<(char, usize)> {
    let window = rest.as_bytes().get(..MAX_ENTITY_BYTES.min(rest.len()))?;
    let end = window.iter().position(|&b| b == b';')?;
    let name = std::str::from_utf8(window.get(1..end)?).ok()?;
    let decoded = named_entity(name).or_else(|| numeric_entity(name))?;
    Some((decoded, end + 1))
}

fn named_entity(name: &str) -> Option<char> {
    match name {
        "amp" => Some('&'),
        "lt" => Some('<'),
        "gt" => Some('>'),
        "quot" => Some('"'),
        "apos" => Some('\''),
        "nbsp" => Some(' '),
        _ => None,
    }
}

fn numeric_entity(name: &str) -> Option<char> {
    let digits = name.strip_prefix('#')?;
    let code = match digits.strip_prefix(['x', 'X']) {
        Some(hex) => u32::from_str_radix(hex, 16).ok()?,
        None => digits.parse::<u32>().ok()?,
    };
    char::from_u32(code)
}

/// Summary text: the markup subset flattened to one line.
pub fn summary_text(raw: &str) -> String {
    let flat = plain(&parse(raw, SUMMARY_MAX_CHARS));
    flat.chars()
        .map(|c| if c == '\n' { ' ' } else { c })
        .collect::<String>()
        .trim()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn styled(text: &str, bold: bool, italic: bool, underline: bool) -> Span {
        Span {
            text: text.to_string(),
            style: Style {
                bold,
                italic,
                underline,
            },
            link: None,
        }
    }

    #[test]
    fn plain_text_is_one_unstyled_span() {
        assert_eq!(
            parse("hello", 100),
            vec![styled("hello", false, false, false)]
        );
    }

    #[test]
    fn bold_italic_underline_are_applied() {
        let spans = parse("a<b>b</b><i>c</i><u>d</u>", 100);
        assert_eq!(
            spans,
            vec![
                styled("a", false, false, false),
                styled("b", true, false, false),
                styled("c", false, true, false),
                styled("d", false, false, true),
            ]
        );
    }

    #[test]
    fn nested_emphasis_keeps_outer_style_after_inner_close() {
        let spans = parse("<b>x<b>y</b>z</b>", 100);
        assert_eq!(plain(&spans), "xyz");
        assert!(spans.iter().all(|s| s.style.bold));
    }

    #[test]
    fn unknown_tags_and_img_are_dropped_but_text_kept() {
        let spans = parse("a<span class=x>b</span><img src=\"evil\">c", 100);
        assert_eq!(plain(&spans), "abc");
    }

    #[test]
    fn safe_link_is_kept_with_href() {
        let spans = parse("<a href=\"https://example.org/x?a=1&amp;b=2\">go</a>", 100);
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].text, "go");
        assert_eq!(
            spans[0].link.as_deref(),
            Some("https://example.org/x?a=1&b=2")
        );
    }

    #[test]
    fn unsafe_link_scheme_is_dropped_but_text_kept() {
        let spans = parse("<a href='file:///etc/passwd'>x</a>", 100);
        assert_eq!(plain(&spans), "x");
        assert!(spans.iter().all(|s| s.link.is_none()));
    }

    #[test]
    fn javascript_and_relative_links_are_rejected() {
        assert_eq!(safe_href("javascript:alert(1)"), None);
        assert_eq!(safe_href("/relative"), None);
        assert_eq!(
            safe_href("HTTPS://Example.org"),
            Some("HTTPS://Example.org".into())
        );
    }

    #[test]
    fn overlong_or_control_href_is_rejected() {
        let long = format!("https://a.org/{}", "x".repeat(MAX_HREF_CHARS));
        assert_eq!(safe_href(&long), None);
        assert_eq!(safe_href("https://a.org/\u{7}"), None);
    }

    #[test]
    fn named_and_numeric_entities_are_decoded() {
        assert_eq!(
            plain(&parse("a &lt;b&gt; &amp; &#65;&#x42;", 100)),
            "a <b> & AB"
        );
    }

    #[test]
    fn unknown_or_broken_entities_stay_literal() {
        assert_eq!(plain(&parse("&bogus; & x;", 100)), "&bogus; & x;");
    }

    #[test]
    fn control_characters_are_stripped_but_newlines_kept() {
        assert_eq!(plain(&parse("a\u{1b}[31mb\nc\td\r", 100)), "a[31mb\nc d");
    }

    #[test]
    fn decoded_control_entities_are_stripped() {
        assert_eq!(plain(&parse("a&#0;&#27;b", 100)), "ab");
    }

    #[test]
    fn br_becomes_newline() {
        assert_eq!(plain(&parse("a<br/>b", 100)), "a\nb");
    }

    #[test]
    fn stray_less_than_is_literal() {
        assert_eq!(plain(&parse("1 < 2 and 3 > 2", 100)), "1 < 2 and 3 > 2");
        assert_eq!(plain(&parse("unclosed <b", 100)), "unclosed <b");
    }

    #[test]
    fn unclosed_tags_do_not_leak_styles_into_later_text() {
        let spans = parse("<b>bold", 100);
        assert_eq!(spans, vec![styled("bold", true, false, false)]);
    }

    #[test]
    fn output_is_capped_in_characters() {
        let spans = parse(&"ä".repeat(50), 10);
        assert_eq!(plain(&spans).chars().count(), 10);
    }

    #[test]
    fn cap_counts_text_not_markup() {
        let spans = parse("<b><b><b><b>xy</b></b></b></b>", 2);
        assert_eq!(plain(&spans), "xy");
    }

    #[test]
    fn huge_markup_input_terminates_within_cap() {
        let input = "<i>x</i>".repeat(100_000);
        assert_eq!(plain(&parse(&input, 16)).chars().count(), 16);
    }

    #[test]
    fn summary_is_single_line_and_trimmed() {
        assert_eq!(summary_text("  <b>Hi</b>\nthere  "), "Hi there");
    }
}
