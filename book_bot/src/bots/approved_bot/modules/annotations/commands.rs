use regex::Regex;
use std::sync::LazyLock;

use crate::bots::approved_bot::modules::utils::{
    errors::CommandParseError, filter_command::CommandParse,
};

static RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^/(?P<an_type>[ab])_an_(?P<id>\d+)$").unwrap());

#[derive(Debug, Clone)]
pub enum AnnotationCommand {
    Book { id: u32 },
    Author { id: u32 },
}

impl CommandParse<Self> for AnnotationCommand {
    fn parse(s: &str) -> Result<Self, CommandParseError> {
        let caps = RE.captures(s).ok_or(CommandParseError)?;

        let an_type = &caps["an_type"];
        let id: u32 = caps["id"].parse().map_err(|_| CommandParseError)?;

        match an_type {
            "a" => Ok(AnnotationCommand::Author { id }),
            "b" => Ok(AnnotationCommand::Book { id }),
            _ => Err(CommandParseError),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::AnnotationCommand;
    use crate::bots::approved_bot::modules::utils::filter_command::CommandParse;

    #[test]
    fn parses_book() {
        match AnnotationCommand::parse("/b_an_5").unwrap() {
            AnnotationCommand::Book { id } => assert_eq!(id, 5),
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn parses_author() {
        match AnnotationCommand::parse("/a_an_7").unwrap() {
            AnnotationCommand::Author { id } => assert_eq!(id, 7),
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn rejects_foreign_prefix() {
        assert!(AnnotationCommand::parse("/x_an_5").is_err());
    }

    #[test]
    fn rejects_non_numeric_id() {
        assert!(AnnotationCommand::parse("/b_an_abc").is_err());
    }
}
