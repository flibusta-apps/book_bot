//! BBCode-ish + sanitized-HTML tokenizer/renderer for annotation text.
//!
//! Input text comes from an upstream pipeline that already HTML-sanitizes
//! the raw text via `ammonia` (allowing only real `<a href="...">` tags,
//! everything else HTML-entity-escaped), but does NOT touch BBCode markup
//! like `[b]`, `[i]`, `[url=...]`.
//!
//! This module performs a single left-to-right pass over the raw string and
//! produces a normalized (tag-stack-balanced) stream of [`Token`]s that can
//! be rendered as Telegram HTML. It never decodes-then-rescans text (that
//! would be a double-decode injection bug): entity decoding happens only
//! when consuming characters directly into a `Text` token.

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Tag {
    Bold,
    Italic,
    Underline,
    Strike,
    Link { href: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Token {
    Text(String),
    Open(Tag),
    Close(Tag),
}

/// Escape text content for Telegram HTML body (only `&`, `<`, `>`).
pub fn escape_text(s: &str) -> String {
    s.chars().fold(String::with_capacity(s.len()), |mut acc, c| {
        match c {
            '&' => acc.push_str("&amp;"),
            '<' => acc.push_str("&lt;"),
            '>' => acc.push_str("&gt;"),
            c => acc.push(c),
        }
        acc
    })
}

/// Escape a value for use inside an HTML attribute (`&`, `<`, `>`, `"`).
pub fn escape_attr(s: &str) -> String {
    s.chars().fold(String::with_capacity(s.len()), |mut acc, c| {
        match c {
            '&' => acc.push_str("&amp;"),
            '<' => acc.push_str("&lt;"),
            '>' => acc.push_str("&gt;"),
            '"' => acc.push_str("&quot;"),
            c => acc.push(c),
        }
        acc
    })
}

pub fn render_open(tag: &Tag) -> String {
    match tag {
        Tag::Bold => "<b>".to_string(),
        Tag::Italic => "<i>".to_string(),
        Tag::Underline => "<u>".to_string(),
        Tag::Strike => "<s>".to_string(),
        Tag::Link { href } => format!("<a href=\"{}\">", escape_attr(href)),
    }
}

pub fn render_close(tag: &Tag) -> String {
    match tag {
        Tag::Bold => "</b>".to_string(),
        Tag::Italic => "</i>".to_string(),
        Tag::Underline => "</u>".to_string(),
        Tag::Strike => "</s>".to_string(),
        Tag::Link { .. } => "</a>".to_string(),
    }
}

fn is_valid_url(url: &str) -> bool {
    let lower = url.trim().to_ascii_lowercase();
    lower.starts_with("http://")
        || lower.starts_with("https://")
        || lower.starts_with("mailto:")
        || lower.starts_with("tg://")
}

/// BBCode tags known but not supported by Telegram HTML: drop the wrapper
/// (both open and close occurrences), keep the inner content in place
/// (still scanned normally for nested tags).
const UNSUPPORTED_TAGS: &[&str] = &[
    "color", "size", "center", "left", "right", "font", "list", "*", "quote", "code", "img",
];

fn simple_tag_for(name: &str) -> Option<Tag> {
    match name.to_ascii_lowercase().as_str() {
        "b" | "strong" => Some(Tag::Bold),
        "i" | "em" => Some(Tag::Italic),
        "u" => Some(Tag::Underline),
        "s" | "strike" | "del" => Some(Tag::Strike),
        _ => None,
    }
}

/// Raw pre-normalization tokens emitted by the scanner.
enum RawToken {
    Text(String),
    Open(Tag),
    Close(Tag),
}

/// Decode a single HTML entity in `rest` (the string right after `&`).
/// Returns `(decoded_char, bytes_consumed_including_semicolon)` or `None`
/// if it's not a recognized entity.
fn try_decode_entity(rest: &str) -> Option<(char, usize)> {
    let semi = rest.find(';')?;
    if semi > 12 {
        return None;
    }
    let body = &rest[..semi];
    let consumed = semi + 1;

    let ch = match body {
        "amp" => '&',
        "lt" => '<',
        "gt" => '>',
        "quot" => '"',
        "apos" => '\'',
        "nbsp" => ' ',
        "laquo" => '\u{00AB}',
        "raquo" => '\u{00BB}',
        "mdash" => '\u{2014}',
        "ndash" => '\u{2013}',
        "hellip" => '\u{2026}',
        _ => {
            if let Some(hex) = body.strip_prefix('x').or_else(|| body.strip_prefix('X')) {
                let code = u32::from_str_radix(hex, 16).ok()?;
                char::from_u32(code)?
            } else if let Some(num) = body.strip_prefix('#') {
                if let Some(hex) = num.strip_prefix('x').or_else(|| num.strip_prefix('X')) {
                    let code = u32::from_str_radix(hex, 16).ok()?;
                    char::from_u32(code)?
                } else {
                    let code: u32 = num.parse().ok()?;
                    char::from_u32(code)?
                }
            } else {
                return None;
            }
        }
    };

    Some((ch, consumed))
}

/// Parse a `[...]` bracket tag starting at `s` (`s` must start with `[`).
struct ParsedBracket {
    name: String,
    arg: Option<String>,
    is_closing: bool,
    len: usize,
}

fn parse_bracket(s: &str) -> Option<ParsedBracket> {
    if !s.starts_with('[') {
        return None;
    }
    let close_idx = s.find(']')?;
    let inner = &s[1..close_idx];
    if inner.contains('[') || inner.is_empty() {
        return None;
    }
    let (is_closing, inner) = match inner.strip_prefix('/') {
        Some(rest) => (true, rest),
        None => (false, inner),
    };
    if inner.is_empty() {
        return None;
    }
    let (name, arg) = match inner.split_once('=') {
        Some((n, a)) => (n.to_string(), Some(a.to_string())),
        None => (inner.to_string(), None),
    };
    if name.is_empty() || !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '*') {
        return None;
    }
    Some(ParsedBracket {
        name,
        arg,
        is_closing,
        len: close_idx + 1,
    })
}

/// Try to parse `<a href="...">` or `</a>` starting at `s`.
fn parse_html_a(s: &str) -> Option<(RawToken, usize)> {
    if s.starts_with("</a>") || s.starts_with("</A>") {
        return Some((RawToken::Close(Tag::Link { href: String::new() }), 4));
    }

    let looks_like_a_open = s.len() >= 2
        && s.as_bytes()[0] == b'<'
        && (s.as_bytes()[1] == b'a' || s.as_bytes()[1] == b'A')
        && s.as_bytes()
            .get(2)
            .map(|b| b.is_ascii_whitespace() || *b == b'>')
            .unwrap_or(false);
    if !looks_like_a_open {
        return None;
    }
    let close_idx = s.find('>')?;
    let tag_body = &s[..close_idx];
    let total_len = close_idx + 1;

    let lower_body = tag_body.to_ascii_lowercase();
    let href_pos = lower_body.find("href")?;
    let after = &tag_body[href_pos + 4..];
    let after_trimmed = after.trim_start();
    let after_trimmed = after_trimmed.strip_prefix('=')?;
    let after_trimmed = after_trimmed.trim_start();
    let quote = after_trimmed.chars().next()?;
    if quote != '"' && quote != '\'' {
        return None;
    }
    let value_part = &after_trimmed[1..];
    let end_quote = value_part.find(quote)?;
    let raw_href = &value_part[..end_quote];
    let href = decode_entities_fully(raw_href);

    Some((RawToken::Open(Tag::Link { href }), total_len))
}

/// Decode HTML entities in a whole string. Used only for attribute values
/// (`href="..."`), which are never re-scanned for BBCode/HTML tags
/// afterwards -- they only ever become the `href` attribute, itself
/// escaped again on render, so this is not a double-decode injection risk.
fn decode_entities_fully(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.char_indices().peekable();
    while let Some((idx, c)) = chars.next() {
        if c == '&' {
            let rest = &s[idx + 1..];
            if let Some((decoded, consumed)) = try_decode_entity(rest) {
                out.push(decoded);
                let target = idx + 1 + consumed;
                while let Some(&(next_idx, _)) = chars.peek() {
                    if next_idx < target {
                        chars.next();
                    } else {
                        break;
                    }
                }
                continue;
            }
        }
        out.push(c);
    }
    out
}

/// Scan raw input into a flat (non-normalized) token stream.
fn scan(raw: &str) -> Vec<RawToken> {
    let mut tokens: Vec<RawToken> = Vec::new();
    let mut cur_text = String::new();
    let mut rest = raw;

    macro_rules! flush_text {
        () => {
            if !cur_text.is_empty() {
                tokens.push(RawToken::Text(std::mem::take(&mut cur_text)));
            }
        };
    }

    while !rest.is_empty() {
        let c = rest.chars().next().unwrap();

        if c == '[' {
            if let Some(bracket) = parse_bracket(rest) {
                let lower_name = bracket.name.to_ascii_lowercase();

                if let Some(tag) = simple_tag_for(&lower_name) {
                    flush_text!();
                    if bracket.is_closing {
                        tokens.push(RawToken::Close(tag));
                    } else {
                        tokens.push(RawToken::Open(tag));
                    }
                    rest = &rest[bracket.len..];
                    continue;
                }

                if lower_name == "url" {
                    if bracket.is_closing {
                        flush_text!();
                        tokens.push(RawToken::Close(Tag::Link { href: String::new() }));
                        rest = &rest[bracket.len..];
                        continue;
                    }

                    let after_open = &rest[bracket.len..];
                    let href_opt: Option<String> = match &bracket.arg {
                        Some(arg) => {
                            if is_valid_url(arg) {
                                Some(arg.clone())
                            } else {
                                None
                            }
                        }
                        None => {
                            let lower_after = after_open.to_ascii_lowercase();
                            lower_after.find("[/url]").and_then(|close_pos| {
                                let trimmed = after_open[..close_pos].trim();
                                is_valid_url(trimmed).then(|| trimmed.to_string())
                            })
                        }
                    };

                    if let Some(href) = href_opt {
                        flush_text!();
                        tokens.push(RawToken::Open(Tag::Link { href }));
                    }
                    // Either way, only consume the opening bracket itself;
                    // the inner content is scanned normally by the main
                    // loop and the eventual [/url] is handled above.
                    rest = &rest[bracket.len..];
                    continue;
                }

                if UNSUPPORTED_TAGS.contains(&lower_name.as_str()) {
                    // Drop the wrapper (open or close), keep scanning the
                    // remainder (inner content) normally.
                    rest = &rest[bracket.len..];
                    continue;
                }
            }

            // Not a recognized bracket construct: literal '['.
            cur_text.push('[');
            rest = &rest[1..];
            continue;
        }

        if c == '<' {
            if let Some((tok, len)) = parse_html_a(rest) {
                flush_text!();
                tokens.push(tok);
                rest = &rest[len..];
                continue;
            }
            cur_text.push('<');
            rest = &rest[1..];
            continue;
        }

        if c == '&' {
            let after = &rest[1..];
            if let Some((decoded, consumed)) = try_decode_entity(after) {
                cur_text.push(decoded);
                rest = &rest[1 + consumed..];
                continue;
            }
            cur_text.push('&');
            rest = &rest[1..];
            continue;
        }

        cur_text.push(c);
        rest = &rest[c.len_utf8()..];
    }

    flush_text!();
    tokens
}

struct StackEntry {
    tag: Tag,
    emitted: bool,
}

fn same_kind(a: &Tag, b: &Tag) -> bool {
    std::mem::discriminant(a) == std::mem::discriminant(b)
}

/// Normalize a raw token stream: resolve tag-stack crossing/unmatched
/// closes, drop nested links (both their Open and matching Close), and
/// auto-close everything left open at EOF.
fn normalize(raw_tokens: Vec<RawToken>) -> Vec<Token> {
    let mut out: Vec<Token> = Vec::new();
    let mut stack: Vec<StackEntry> = Vec::new();

    for rt in raw_tokens {
        match rt {
            RawToken::Text(s) => {
                if !s.is_empty() {
                    out.push(Token::Text(s));
                }
            }
            RawToken::Open(tag) => {
                let suppress = matches!(tag, Tag::Link { .. })
                    && stack.iter().any(|e| matches!(e.tag, Tag::Link { .. }));
                if !suppress {
                    out.push(Token::Open(tag.clone()));
                }
                stack.push(StackEntry {
                    tag,
                    emitted: !suppress,
                });
            }
            RawToken::Close(tag) => {
                let pos = stack.iter().rposition(|e| same_kind(&e.tag, &tag));
                match pos {
                    None => {
                        // Unmatched close tag: drop silently.
                    }
                    Some(p) if p == stack.len() - 1 => {
                        let popped = stack.pop().unwrap();
                        if popped.emitted {
                            out.push(Token::Close(popped.tag));
                        }
                    }
                    Some(p) => {
                        // Crossed nesting: close everything above p (LIFO),
                        // close p, then reopen the ones above p.
                        let above: Vec<StackEntry> = stack.split_off(p + 1);
                        for e in above.iter().rev() {
                            if e.emitted {
                                out.push(Token::Close(e.tag.clone()));
                            }
                        }
                        let popped = stack.pop().unwrap();
                        if popped.emitted {
                            out.push(Token::Close(popped.tag));
                        }
                        for e in above {
                            if e.emitted {
                                out.push(Token::Open(e.tag.clone()));
                            }
                            stack.push(e);
                        }
                    }
                }
            }
        }
    }

    while let Some(e) = stack.pop() {
        if e.emitted {
            out.push(Token::Close(e.tag));
        }
    }

    out
}

/// Tokenize raw annotation text into a normalized (tag-stack-balanced)
/// token stream ready for rendering.
pub fn tokenize(raw: &str) -> Vec<Token> {
    normalize(scan(raw))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn render_all(tokens: &[Token]) -> String {
        let mut out = String::new();
        for t in tokens {
            match t {
                Token::Text(s) => out.push_str(&escape_text(s)),
                Token::Open(tag) => out.push_str(&render_open(tag)),
                Token::Close(tag) => out.push_str(&render_close(tag)),
            }
        }
        out
    }

    #[test]
    fn simple_bold() {
        let tokens = tokenize("[b]x[/b]");
        assert_eq!(render_all(&tokens), "<b>x</b>");
    }

    #[test]
    fn unclosed_bold_auto_closes() {
        let tokens = tokenize("[b]x");
        assert_eq!(render_all(&tokens), "<b>x</b>");
    }

    #[test]
    fn crossed_nesting() {
        let tokens = tokenize("[b][i]x[/b]y[/i]");
        assert_eq!(render_all(&tokens), "<b><i>x</i></b><i>y</i>");
    }

    #[test]
    fn empty_bold_produces_nothing_visible() {
        let tokens = tokenize("[b][/b]");
        assert!(!tokens.iter().any(|t| matches!(t, Token::Text(_))));
    }

    #[test]
    fn bbcode_url_bare() {
        let tokens = tokenize("[url]http://x.com[/url]");
        assert_eq!(render_all(&tokens), "<a href=\"http://x.com\">http://x.com</a>");
    }

    #[test]
    fn bbcode_url_with_label_and_amp() {
        let tokens = tokenize("[url=http://x.com?a=1&b=2]label[/url]");
        assert_eq!(
            render_all(&tokens),
            "<a href=\"http://x.com?a=1&amp;b=2\">label</a>"
        );
    }

    #[test]
    fn bbcode_url_invalid_scheme_dropped() {
        let tokens = tokenize("[url=javascript:alert(1)]label[/url]");
        assert_eq!(render_all(&tokens), "label");
    }

    #[test]
    fn html_a_strips_extra_attrs() {
        let tokens = tokenize("<a href=\"http://x.com\" rel=\"noopener noreferrer\">l</a>");
        assert_eq!(render_all(&tokens), "<a href=\"http://x.com\">l</a>");
    }

    #[test]
    fn plain_text_escaped() {
        let tokens = tokenize("a < b & c");
        assert_eq!(render_all(&tokens), "a &lt; b &amp; c");
    }

    #[test]
    fn already_escaped_input_not_double_decoded() {
        let tokens = tokenize("&lt;b&gt;");
        assert_eq!(render_all(&tokens), "&lt;b&gt;");
    }

    #[test]
    fn nbsp_decodes_to_space() {
        let tokens = tokenize("a&nbsp;b");
        assert_eq!(render_all(&tokens), "a b");
        let plain: String = tokens
            .iter()
            .filter_map(|t| match t {
                Token::Text(s) => Some(s.clone()),
                _ => None,
            })
            .collect();
        assert_eq!(plain, "a\u{0020}b");
    }

    #[test]
    fn unsupported_tag_dropped_content_kept() {
        let tokens = tokenize("[color=red]x[/color]");
        assert_eq!(render_all(&tokens), "x");
    }

    #[test]
    fn bare_bracket_kept_literal() {
        let tokens = tokenize("[1]");
        assert_eq!(render_all(&tokens), "[1]");
    }

    #[test]
    fn nested_links_dropped() {
        let tokens = tokenize("[url=http://a.com][url=http://b.com]x[/url]y[/url]");
        assert_eq!(render_all(&tokens), "<a href=\"http://a.com\">xy</a>");
    }
}
