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
}
