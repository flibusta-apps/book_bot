use regex::Regex;
use std::sync::LazyLock;

use crate::bots::approved_bot::modules::utils::{
    errors::CommandParseError, filter_command::CommandParse,
};

static RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^/(?P<an_type>[ats])_(?P<id>\d+)$").unwrap());

#[derive(Clone)]
pub enum BookCommand {
    Author { id: u32 },
    Translator { id: u32 },
    Sequence { id: u32 },
}

impl CommandParse<Self> for BookCommand {
    fn parse(s: &str) -> Result<Self, CommandParseError> {
        let caps = RE.captures(s).ok_or(CommandParseError)?;

        let an_type = &caps["an_type"];
        let id: u32 = caps["id"].parse().map_err(|_| CommandParseError)?;

        match an_type {
            "a" => Ok(BookCommand::Author { id }),
            "t" => Ok(BookCommand::Translator { id }),
            "s" => Ok(BookCommand::Sequence { id }),
            _ => Err(CommandParseError),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::BookCommand;
    use crate::bots::approved_bot::modules::utils::filter_command::CommandParse;

    #[test]
    fn parses_author() {
        match BookCommand::parse("/a_5").unwrap() {
            BookCommand::Author { id } => assert_eq!(id, 5),
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn parses_translator() {
        match BookCommand::parse("/t_7").unwrap() {
            BookCommand::Translator { id } => assert_eq!(id, 7),
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn parses_sequence() {
        match BookCommand::parse("/s_9").unwrap() {
            BookCommand::Sequence { id } => assert_eq!(id, 9),
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn rejects_foreign_prefix() {
        assert!(BookCommand::parse("/x_5").is_err());
    }

    #[test]
    fn rejects_non_numeric_id() {
        assert!(BookCommand::parse("/a_abc").is_err());
    }
}
