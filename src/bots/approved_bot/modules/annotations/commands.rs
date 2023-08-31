use regex::Regex;

use crate::bots::approved_bot::modules::utils::CommandParse;


#[derive(Debug, Clone)]
pub enum AnnotationCommand {
    Book { id: u32 },
    Author { id: u32 },
}

impl CommandParse<Self> for AnnotationCommand {
    fn parse(s: &str, bot_name: &str) -> Result<Self, strum::ParseError> {
        let re = Regex::new(r"^/(?P<an_type>a|b)_an_(?P<id>\d+)$")
            .unwrap_or_else(|_| panic!("Can't create AnnotationCommand regexp!"));

        let full_bot_name = format!("@{bot_name}");
        let after_replace = s.replace(&full_bot_name, "");

        let caps = re.captures(&after_replace);
        let caps = match caps {
            Some(v) => v,
            None => return Err(strum::ParseError::VariantNotFound),
        };

        let annotation_type = &caps["an_type"];
        let id: u32 = caps["id"]
            .parse()
            .unwrap_or_else(|_| panic!("Can't get id from AnnotationCommand!"));

        match annotation_type {
            "a" => Ok(AnnotationCommand::Author { id }),
            "b" => Ok(AnnotationCommand::Book { id }),
            _ => Err(strum::ParseError::VariantNotFound),
        }
    }
}
