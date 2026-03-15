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
    fn parse(s: &str, bot_name: &str) -> Result<Self, CommandParseError> {
        let input = s.replace(&format!("@{bot_name}"), "");
        let caps = RE.captures(&input).ok_or(CommandParseError)?;

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
