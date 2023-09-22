use std::fmt;


#[derive(Debug)]
pub struct CallbackQueryParseError;

impl fmt::Display for CallbackQueryParseError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

impl std::error::Error for CallbackQueryParseError {}


#[derive(Debug)]
pub struct CommandParseError;

impl fmt::Display for CommandParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

impl std::error::Error for CommandParseError {}
