use thiserror::Error;

use super::commands::AnnotationCommand;

#[derive(Debug, Error)]
#[error("annotation text for {command:?} is not normal text: {text:?}")]
pub struct AnnotationFormatError {
    pub command: AnnotationCommand,
    pub text: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bots::approved_bot::modules::annotations::commands::AnnotationCommand;

    #[test]
    fn message_includes_command_and_text() {
        let err = AnnotationFormatError {
            command: AnnotationCommand::Book { id: 42 },
            text: "   \n  ".to_string(),
        };
        let msg = err.to_string();
        assert!(msg.contains("Book"));
        assert!(msg.contains("42"));
    }
}
