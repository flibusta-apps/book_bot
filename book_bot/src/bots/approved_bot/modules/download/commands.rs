use std::fmt::Display;

use regex::Regex;
use std::sync::LazyLock;

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
    fn parse(s: &str) -> Result<Self, CommandParseError> {
        let caps = RE_DOWNLOAD.captures(s).ok_or(CommandParseError)?;

        let book_id: u32 = caps["book_id"].parse().map_err(|_| CommandParseError)?;

        Ok(StartDownloadCommand { id: book_id })
    }
}

#[derive(Clone)]
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
    fn parse(s: &str) -> Result<Self, CommandParseError> {
        let caps = RE_ARCHIVE.captures(s).ok_or(CommandParseError)?;

        let id: u32 = caps["id"].parse().map_err(|_| CommandParseError)?;

        match &caps["type"] {
            "s" => Ok(DownloadArchiveCommand::Sequence { id }),
            "a" => Ok(DownloadArchiveCommand::Author { id }),
            "t" => Ok(DownloadArchiveCommand::Translator { id }),
            _ => Err(CommandParseError),
        }
    }
}

impl DownloadArchiveCommand {
    pub fn to_query_data(
        &self,
        file_type: String,
    ) -> crate::bots::approved_bot::modules::download::callback_data::DownloadArchiveQueryData {
        use crate::bots::approved_bot::modules::download::callback_data::DownloadArchiveQueryData;

        match *self {
            DownloadArchiveCommand::Sequence { id } => {
                DownloadArchiveQueryData::Sequence { id, file_type }
            }
            DownloadArchiveCommand::Author { id } => {
                DownloadArchiveQueryData::Author { id, file_type }
            }
            DownloadArchiveCommand::Translator { id } => {
                DownloadArchiveQueryData::Translator { id, file_type }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{DownloadArchiveCommand, StartDownloadCommand};
    use crate::bots::approved_bot::modules::utils::filter_command::{
        strip_bot_mention, CommandParse,
    };

    use super::super::callback_data::DownloadArchiveQueryData;

    #[test]
    fn to_query_data_sequence() {
        let cmd = DownloadArchiveCommand::Sequence { id: 3 };
        match cmd.to_query_data("fb2".to_string()) {
            DownloadArchiveQueryData::Sequence { id, file_type } => {
                assert_eq!(id, 3);
                assert_eq!(file_type, "fb2");
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn to_query_data_author() {
        let cmd = DownloadArchiveCommand::Author { id: 4 };
        match cmd.to_query_data("epub".to_string()) {
            DownloadArchiveQueryData::Author { id, file_type } => {
                assert_eq!(id, 4);
                assert_eq!(file_type, "epub");
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn to_query_data_translator() {
        let cmd = DownloadArchiveCommand::Translator { id: 6 };
        match cmd.to_query_data("zip".to_string()) {
            DownloadArchiveQueryData::Translator { id, file_type } => {
                assert_eq!(id, 6);
                assert_eq!(file_type, "zip");
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn round_trip_start_download() {
        let cmd = StartDownloadCommand { id: 5 };
        let parsed = StartDownloadCommand::parse(&cmd.to_string()).unwrap();
        assert_eq!(parsed.id, 5);
    }

    #[test]
    fn parses_after_case_insensitive_mention_strip() {
        let text = strip_bot_mention("/d_1@MyBot", "mybot");
        let parsed = StartDownloadCommand::parse(&text).unwrap();
        assert_eq!(parsed.id, 1);
    }

    #[test]
    fn rejects_non_numeric_book_id() {
        assert!(StartDownloadCommand::parse("/d_abc").is_err());
    }

    #[test]
    fn round_trip_archive_sequence() {
        let cmd = DownloadArchiveCommand::Sequence { id: 3 };
        match DownloadArchiveCommand::parse(&cmd.to_string()).unwrap() {
            DownloadArchiveCommand::Sequence { id } => assert_eq!(id, 3),
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn round_trip_archive_author() {
        let cmd = DownloadArchiveCommand::Author { id: 4 };
        match DownloadArchiveCommand::parse(&cmd.to_string()).unwrap() {
            DownloadArchiveCommand::Author { id } => assert_eq!(id, 4),
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn round_trip_archive_translator() {
        let cmd = DownloadArchiveCommand::Translator { id: 6 };
        match DownloadArchiveCommand::parse(&cmd.to_string()).unwrap() {
            DownloadArchiveCommand::Translator { id } => assert_eq!(id, 6),
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn rejects_foreign_archive_prefix() {
        assert!(DownloadArchiveCommand::parse("/da_x_5").is_err());
    }
}
