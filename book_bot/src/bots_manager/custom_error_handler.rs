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

            let backtrace = std::backtrace::Backtrace::force_capture();

            let backtrace_info = match backtrace.status() {
                std::backtrace::BacktraceStatus::Captured => {
                    format!("\nBacktrace:\n{}", backtrace)
                }
                std::backtrace::BacktraceStatus::Disabled => {
                    "\nBacktrace: disabled (compile with debug info for stack traces)".to_string()
                }
                std::backtrace::BacktraceStatus::Unsupported => {
                    "\nBacktrace: unsupported on this platform".to_string()
                }
                _ => String::new(),
            };

            log::error!("{}: {:?}{}", self.text, error, backtrace_info);
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
