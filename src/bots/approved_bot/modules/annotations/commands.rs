use regex::Regex;

use crate::bots::approved_bot::modules::utils::{
    errors::CommandParseError, filter_command::CommandParse,
};

#[derive(Debug, Clone)]
pub enum AnnotationCommand {
    Book { id: u32 },
    Author { id: u32 },
}

impl CommandParse<Self> for AnnotationCommand {
    fn parse(s: &str, bot_name: &str) -> Result<Self, CommandParseError> {
        Regex::new(r"^/(?P<an_type>[ab])_an_(?P<id>\d+)$")
            .unwrap_or_else(|_| panic!("Broken AnnotationCommand regexp!"))
            .captures(&s.replace(&format!("@{bot_name}"), ""))
            .ok_or(CommandParseError)
            .map(|caps| {
                (
                    caps["an_type"].to_string(),
                    caps["id"]
                        .parse::<u32>()
                        .unwrap_or_else(|_| panic!("Can't get id from AnnotationCommand!")),
                )
            })
            .map(|(annotation_type, id)| match annotation_type.as_str() {
                "a" => Ok(AnnotationCommand::Author { id }),
                "b" => Ok(AnnotationCommand::Book { id }),
                _ => panic!("Unknown AnnotationCommand type: {annotation_type}!"),
            })?
    }
}
