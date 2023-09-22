use regex::Regex;

use crate::bots::approved_bot::modules::utils::{filter_command::CommandParse, errors::CommandParseError};


#[derive(Clone)]
pub enum BookCommand {
    Author { id: u32 },
    Translator { id: u32 },
    Sequence { id: u32 },
}

impl CommandParse<Self> for BookCommand {
    fn parse(s: &str, bot_name: &str) -> Result<Self, CommandParseError> {
        let re = Regex::new(r"^/(?P<an_type>[ats])_(?P<id>\d+)$").unwrap();

        let full_bot_name = format!("@{bot_name}");
        let after_replace = s.replace(&full_bot_name, "");

        let caps = re.captures(&after_replace);
        let caps = match caps {
            Some(v) => v,
            None => return Err(CommandParseError),
        };

        let annotation_type = &caps["an_type"];
        let id: u32 = caps["id"].parse().unwrap();

        match annotation_type {
            "a" => Ok(BookCommand::Author { id }),
            "t" => Ok(BookCommand::Translator { id }),
            "s" => Ok(BookCommand::Sequence { id }),
            _ => Err(CommandParseError),
        }
    }
}
