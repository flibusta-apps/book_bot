pub fn tuple_first_mut<A, B>(tuple: &mut (A, B)) -> &mut A {
    &mut tuple.0
}

pub fn mask_token(token: &str) -> String {
    format!("{}…", &token[..token.len().min(8)])
}

pub fn mask_uri_path(path: &str) -> String {
    let stripped = path.trim_start_matches('/');
    let end = stripped.find('/').unwrap_or(stripped.len());
    let segment = &stripped[..end];

    if let Some(colon) = segment.find(':') {
        let bot_id = &segment[..colon];
        if !bot_id.is_empty() && bot_id.chars().all(|c| c.is_ascii_digit()) {
            return format!("/[bot:{}]/", bot_id);
        }
    }

    path.to_string()
}

/// Truncates `s` to at most `max_chars` characters for safe logging,
/// appending `…` when truncated. Char-based (not byte-based) so it never
/// panics on multi-byte UTF-8 input, unlike `mask_token`'s byte slicing
/// (which is safe there only because bot tokens are ASCII).
pub fn truncate_for_log(s: &str, max_chars: usize) -> String {
    if s.chars().count() <= max_chars {
        s.to_string()
    } else {
        let truncated: String = s.chars().take(max_chars).collect();
        format!("{truncated}…")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mask_token_long() {
        assert_eq!(mask_token("123456789:ABCDEFGHIJK-long-secret"), "12345678…");
    }

    #[test]
    fn mask_token_exactly_8() {
        assert_eq!(mask_token("12345678"), "12345678…");
    }

    #[test]
    fn mask_token_short() {
        assert_eq!(mask_token("abc"), "abc…");
    }

    #[test]
    fn mask_uri_path_telegram_token() {
        assert_eq!(mask_uri_path("/987654321:XYZ-secret/"), "/[bot:987654321]/");
    }

    #[test]
    fn mask_uri_path_no_token() {
        assert_eq!(mask_uri_path("/metrics"), "/metrics");
        assert_eq!(mask_uri_path("/health"), "/health");
    }

    #[test]
    fn truncate_for_log_leaves_short_strings_untouched() {
        assert_eq!(truncate_for_log("hello", 10), "hello");
    }

    #[test]
    fn truncate_for_log_truncates_and_marks_long_strings() {
        assert_eq!(truncate_for_log("hello world", 5), "hello…");
    }

    #[test]
    fn truncate_for_log_is_char_safe_on_multibyte_input() {
        // "héllo wörld" — truncating by raw byte offset 5 would land mid-character; char-based truncation must not panic.
        assert_eq!(truncate_for_log("héllo wörld", 5), "héllo…");
    }
}
