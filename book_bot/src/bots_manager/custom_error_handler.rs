use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use tracing::log;

pub struct CustomErrorHandler {
    pub text: String,
}

impl CustomErrorHandler {
    pub fn with_custom_text<T>(text: T) -> Arc<Self>
    where
        T: Into<String>,
    {
        Arc::new(Self { text: text.into() })
    }
}

/// Classify an error by severity based on its string representation.
///
/// Transient errors (rate limits, network timeouts, DNS failures) are expected
/// in a distributed system and should be logged at WARN level to avoid
/// flooding Sentry with non-actionable events. Only truly unexpected errors
/// are logged at ERROR level.
fn classify_error(error_string: &str) -> log::Level {
    // Rate limit errors from Telegram API
    if error_string.contains("Retry after")
        || error_string.contains("Too Many Requests")
        || error_string.contains("Flood")
    {
        return log::Level::Warn;
    }

    // Network/transient errors (timeouts, DNS, connection refused, incomplete messages)
    if error_string.contains("operation timed out")
        || error_string.contains("dns error")
        || error_string.contains("tcp connect error")
        || error_string.contains("connection refused")
        || error_string.contains("error sending request")
        || error_string.contains("A network error")
        || error_string.contains("network error")
        || error_string.contains("IncompleteMessage")
        || error_string.contains("ConnectError")
    {
        return log::Level::Warn;
    }

    // Telegram permission/authorization errors — expected in production when
    // bots are added to groups without send rights, users block the bot, etc.
    if error_string.contains("not enough rights")
        || error_string.contains("CHAT_WRITE_FORBIDDEN")
        || error_string.contains("Forbidden: bot was blocked by the user")
        || error_string.contains("Forbidden: user is deactivated")
        || error_string.contains("Forbidden: bot can't initiate conversation")
        || error_string.contains("Bad Request: chat not found")
        || error_string.contains("Forbidden: bot was kicked from the group")
    {
        return log::Level::Warn;
    }

    log::Level::Error
}

impl<E> teloxide::error_handlers::ErrorHandler<E> for CustomErrorHandler
where
    E: std::fmt::Debug + Send + 'static,
{
    fn handle_error(
        self: Arc<Self>,
        error: E,
    ) -> Pin<Box<dyn Future<Output = ()> + Send + 'static>> {
        Box::pin(async move {
            let error_string = format!("{:?}", error);

            if error_string.contains("Bad Request: message to be replied not found") {
                log::debug!("Ignoring Telegram reply error: {:?}", error);
                return;
            }

            let level = classify_error(&error_string);

            match level {
                log::Level::Error => {
                    let backtrace = std::backtrace::Backtrace::force_capture();

                    let backtrace_info = match backtrace.status() {
                        std::backtrace::BacktraceStatus::Captured => {
                            format!("\nBacktrace:\n{}", backtrace)
                        }
                        std::backtrace::BacktraceStatus::Disabled => {
                            "\nBacktrace: disabled (compile with debug info for stack traces)"
                                .to_string()
                        }
                        std::backtrace::BacktraceStatus::Unsupported => {
                            "\nBacktrace: unsupported on this platform".to_string()
                        }
                        _ => String::new(),
                    };

                    log::error!("{}: {:?}{}", self.text, error, backtrace_info);
                }
                log::Level::Warn => {
                    log::warn!("{}: {:?}", self.text, error);
                }
                _ => {
                    log::log!(level, "{}: {:?}", self.text, error);
                }
            }
        })
    }
}

impl Default for CustomErrorHandler {
    fn default() -> Self {
        Self {
            text: "An error from the update listener".to_string(),
        }
    }
}
