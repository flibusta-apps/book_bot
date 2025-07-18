use std::fmt::Display;

use regex::Regex;
use strum_macros::EnumIter;

use crate::bots::approved_bot::modules::utils::{
    errors::CommandParseError, filter_command::CommandParse,
};

#[derive(Clone)]
pub struct StartDownloadCommand {
    pub id: u32,
}

impl Display for StartDownloadCommand {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "/d_{}", self.id)
    }
}

impl CommandParse<Self> for StartDownloadCommand {
    fn parse(s: &str, bot_name: &str) -> Result<Self, CommandParseError> {
        let re = Regex::new(r"^/d_(?P<book_id>\d+)$").unwrap();

        let full_bot_name = format!("@{bot_name}");
        let after_replace = s.replace(&full_bot_name, "");

        let caps = re.captures(&after_replace);
        let caps = match caps {
            Some(v) => v,
            None => return Err(CommandParseError),
        };

        let book_id: u32 = caps["book_id"].parse().unwrap();

        Ok(StartDownloadCommand { id: book_id })
    }
}

#[derive(Clone, EnumIter)]
pub enum DownloadArchiveCommand {
    Sequence { id: u32 },
    Author { id: u32 },
    Translator { id: u32 },
}

impl Display for DownloadArchiveCommand {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DownloadArchiveCommand::Sequence { id } => write!(f, "/da_s_{id}"),
            DownloadArchiveCommand::Author { id } => write!(f, "/da_a_{id}"),
            DownloadArchiveCommand::Translator { id } => write!(f, "/da_t_{id}"),
        }
    }
}

impl CommandParse<Self> for DownloadArchiveCommand {
    fn parse(s: &str, bot_name: &str) -> Result<Self, CommandParseError> {
        let re = Regex::new(r"^/da_(?P<type>[sat])_(?P<id>\d+)$").unwrap();

        let full_bot_name = format!("@{bot_name}");
        let after_replace = s.replace(&full_bot_name, "");

        let caps = re.captures(&after_replace);
        let caps = match caps {
            Some(v) => v,
            None => return Err(CommandParseError),
        };

        let obj_id: u32 = caps["id"].parse().unwrap();

        match &caps["type"] {
            "s" => Ok(DownloadArchiveCommand::Sequence { id: obj_id }),
            "a" => Ok(DownloadArchiveCommand::Author { id: obj_id }),
            "t" => Ok(DownloadArchiveCommand::Translator { id: obj_id }),
            _ => Err(CommandParseError),
        }
    }
}
