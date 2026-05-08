//! Telegram error classification — single source of truth for deciding
//! whether an error is an expected/normal Telegram API response or a
//! genuine unexpected error that needs investigation.

#![allow(dead_code)] // Used by custom_error_handler and main.rs Sentry filter

/// Category of a Telegram API error.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorCategory {
    /// Expected in production — rate limits, network blips, permission
    /// issues, message state conflicts. Logged at WARN, never sent to Sentry.
    Expected,
    /// Unexpected — genuine bugs or infrastructure problems. Logged at
    /// ERROR, sent to Sentry.
    Unexpected,
}

/// Classify a Telegram error string into Expected vs Unexpected.
pub fn classify_telegram_error(error_string: &str) -> ErrorCategory {
    if is_rate_limit_error(error_string)
        || is_network_error(error_string)
        || is_permission_error(error_string)
        || is_message_state_error(error_string)
    {
        ErrorCategory::Expected
    } else {
        ErrorCategory::Unexpected
    }
}

/// Returns `true` if the error is an expected/normal Telegram API response
/// that should not be sent to Sentry.
pub fn is_expected_telegram_error(error_string: &str) -> bool {
    matches!(
        classify_telegram_error(error_string),
        ErrorCategory::Expected
    )
}

// ---------------------------------------------------------------------------
// Private predicates — each covers one family of expected errors
// ---------------------------------------------------------------------------

fn is_rate_limit_error(s: &str) -> bool {
    s.contains("Retry after") || s.contains("Too Many Requests") || s.contains("Flood")
}

fn is_network_error(s: &str) -> bool {
    s.contains("operation timed out")
        || s.contains("dns error")
        || s.contains("tcp connect error")
        || s.contains("connection refused")
        || s.contains("error sending request")
        || s.contains("A network error")
        || s.contains("network error")
        || s.contains("IncompleteMessage")
        || s.contains("ConnectError")
}

fn is_permission_error(s: &str) -> bool {
    s.contains("not enough rights")
        || s.contains("CHAT_WRITE_FORBIDDEN")
        || s.contains("Forbidden: bot was blocked by the user")
        || s.contains("Forbidden: user is deactivated")
        || s.contains("Forbidden: bot can't initiate conversation")
        || s.contains("Bad Request: chat not found")
        || s.contains("Forbidden: bot was kicked from the group")
}

fn is_message_state_error(s: &str) -> bool {
    s.contains("message to edit not found")
        || s.contains("message is not modified")
        || s.contains("MESSAGE_ID_INVALID")
        || s.contains("text must be non-empty")
        || s.contains("Bad Request: message to be replied not found")
}
