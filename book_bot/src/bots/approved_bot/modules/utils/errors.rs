use thiserror::Error;

#[derive(Debug, Error)]
#[error("failed to parse callback query data")]
pub struct CallbackQueryParseError;

#[derive(Debug, Error)]
#[error("failed to parse command")]
pub struct CommandParseError;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn callback_query_parse_error_has_readable_message() {
        assert_eq!(
            CallbackQueryParseError.to_string(),
            "failed to parse callback query data"
        );
    }

    #[test]
    fn command_parse_error_has_readable_message() {
        assert_eq!(CommandParseError.to_string(), "failed to parse command");
    }
}
