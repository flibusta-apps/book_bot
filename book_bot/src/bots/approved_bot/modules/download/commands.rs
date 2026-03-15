use std::fmt::Display;

use regex::Regex;
use std::sync::LazyLock;
use strum_macros::EnumIter;

use crate::bots::approved_bot::modules::utils::{
    errors::CommandParseError, filter_command::CommandParse,
};

static RE_DOWNLOAD: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^/d_(?P<book_id>\d+)$").unwrap());

static RE_ARCHIVE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^/da_(?P<type>[sat])_(?P<id>\d+)$").unwrap());

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
        let input = s.replace(&format!("@{bot_name}"), "");
        let caps = RE_DOWNLOAD.captures(&input).ok_or(CommandParseError)?;

        let book_id: u32 = caps["book_id"].parse().map_err(|_| CommandParseError)?;

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
        let input = s.replace(&format!("@{bot_name}"), "");
        let caps = RE_ARCHIVE.captures(&input).ok_or(CommandParseError)?;

        let id: u32 = caps["id"].parse().map_err(|_| CommandParseError)?;

        match &caps["type"] {
            "s" => Ok(DownloadArchiveCommand::Sequence { id }),
            "a" => Ok(DownloadArchiveCommand::Author { id }),
            "t" => Ok(DownloadArchiveCommand::Translator { id }),
            _ => Err(CommandParseError),
        }
    }
}
