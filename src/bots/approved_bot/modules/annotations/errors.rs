use std::fmt;

use super::commands::AnnotationCommand;

#[derive(Debug)]
pub struct AnnotationFormatError {
    pub command: AnnotationCommand,
    pub text: String,
}

impl fmt::Display for AnnotationFormatError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

impl std::error::Error for AnnotationFormatError {}
