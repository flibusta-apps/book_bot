use regex::Regex;

use crate::bots::approved_bot::modules::utils::{
    errors::CommandParseError, filter_command::CommandParse,
};

#[derive(Clone)]
pub enum BookCommand {
    Author { id: u32 },
    Translator { id: u32 },
    Sequence { id: u32 },
}

impl CommandParse<Self> for BookCommand {
    fn parse(s: &str, bot_name: &str) -> Result<Self, CommandParseError> {
        Regex::new(r"^/(?P<an_type>[ats])_(?P<id>\d+)$")
            .unwrap_or_else(|_| panic!("Broken BookCommand regexp!"))
            .captures(&s.replace(&format!("@{bot_name}"), ""))
            .ok_or(CommandParseError)
            .map(|caps| (caps["an_type"].to_string(), caps["id"].parse().unwrap()))
            .map(|(annotation_type, id)| match annotation_type.as_str() {
                "a" => Ok(BookCommand::Author { id }),
                "t" => Ok(BookCommand::Translator { id }),
                "s" => Ok(BookCommand::Sequence { id }),
                _ => panic!("Unknown BookCommand type: {annotation_type}!"),
            })?
    }
}
