use std::{fmt::Display, str::FromStr};

use regex::Regex;
use std::sync::LazyLock;

use crate::bots::approved_bot::modules::utils::errors::{
    CallbackQueryParseError, CommandParseError,
};

static RE_DOWNLOAD: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^d_(?P<book_id>\d+)_(?P<file_type>\w+)$").unwrap());

static RE_ARCHIVE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^da_(?P<obj_type>[sat])_(?P<id>\d+)_(?P<file_type>\w+)$").unwrap()
});

static RE_CHECK: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^check_da_(?P<task_id>\w+)$").unwrap());

#[derive(Clone)]
pub enum DownloadQueryData {
    DownloadData { book_id: u32, file_type: String },
}

impl Display for DownloadQueryData {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DownloadQueryData::DownloadData { book_id, file_type } => {
                write!(f, "d_{book_id}_{file_type}")
            }
        }
    }
}

impl FromStr for DownloadQueryData {
    type Err = CommandParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let caps = RE_DOWNLOAD.captures(s).ok_or(CommandParseError)?;

        let book_id: u32 = caps["book_id"].parse().map_err(|_| CommandParseError)?;
        let file_type = caps["file_type"].to_string();

        Ok(DownloadQueryData::DownloadData { book_id, file_type })
    }
}

#[derive(Clone)]
pub enum DownloadArchiveQueryData {
    Sequence { id: u32, file_type: String },
    Author { id: u32, file_type: String },
    Translator { id: u32, file_type: String },
}

impl Display for DownloadArchiveQueryData {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DownloadArchiveQueryData::Sequence { id, file_type } => {
                write!(f, "da_s_{id}_{file_type}")
            }
            DownloadArchiveQueryData::Author { id, file_type } => {
                write!(f, "da_a_{id}_{file_type}")
            }
            DownloadArchiveQueryData::Translator { id, file_type } => {
                write!(f, "da_t_{id}_{file_type}")
            }
        }
    }
}

impl FromStr for DownloadArchiveQueryData {
    type Err = CallbackQueryParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let caps = RE_ARCHIVE.captures(s).ok_or(CallbackQueryParseError)?;

        let id: u32 = caps["id"].parse().map_err(|_| CallbackQueryParseError)?;
        let file_type = caps["file_type"].to_string();
        let obj_type = &caps["obj_type"];

        match obj_type {
            "s" => Ok(DownloadArchiveQueryData::Sequence { id, file_type }),
            "a" => Ok(DownloadArchiveQueryData::Author { id, file_type }),
            "t" => Ok(DownloadArchiveQueryData::Translator { id, file_type }),
            _ => Err(CallbackQueryParseError),
        }
    }
}

#[derive(Clone)]
pub struct CheckArchiveStatus {
    pub task_id: String,
}

impl Display for CheckArchiveStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "check_da_{}", self.task_id)
    }
}

impl FromStr for CheckArchiveStatus {
    type Err = CallbackQueryParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let caps = RE_CHECK.captures(s).ok_or(CallbackQueryParseError)?;
        let task_id = caps["task_id"].to_string();
        Ok(CheckArchiveStatus { task_id })
    }
}

#[cfg(test)]
mod tests {
    use super::{CheckArchiveStatus, DownloadArchiveQueryData, DownloadQueryData};
    use std::str::FromStr;

    #[test]
    fn round_trip_download_data() {
        let cd = DownloadQueryData::DownloadData {
            book_id: 5,
            file_type: "fb2".to_string(),
        };
        match DownloadQueryData::from_str(&cd.to_string()).unwrap() {
            DownloadQueryData::DownloadData { book_id, file_type } => {
                assert_eq!(book_id, 5);
                assert_eq!(file_type, "fb2");
            }
        }
    }

    #[test]
    fn rejects_non_numeric_book_id() {
        assert!(DownloadQueryData::from_str("d_x_fb2").is_err());
    }

    #[test]
    fn round_trip_archive_sequence() {
        let cd = DownloadArchiveQueryData::Sequence {
            id: 3,
            file_type: "zip".to_string(),
        };
        match DownloadArchiveQueryData::from_str(&cd.to_string()).unwrap() {
            DownloadArchiveQueryData::Sequence { id, file_type } => {
                assert_eq!(id, 3);
                assert_eq!(file_type, "zip");
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn round_trip_archive_author() {
        let cd = DownloadArchiveQueryData::Author {
            id: 4,
            file_type: "fb2".to_string(),
        };
        match DownloadArchiveQueryData::from_str(&cd.to_string()).unwrap() {
            DownloadArchiveQueryData::Author { id, file_type } => {
                assert_eq!(id, 4);
                assert_eq!(file_type, "fb2");
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn round_trip_archive_translator() {
        let cd = DownloadArchiveQueryData::Translator {
            id: 6,
            file_type: "epub".to_string(),
        };
        match DownloadArchiveQueryData::from_str(&cd.to_string()).unwrap() {
            DownloadArchiveQueryData::Translator { id, file_type } => {
                assert_eq!(id, 6);
                assert_eq!(file_type, "epub");
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn rejects_foreign_archive_prefix() {
        assert!(DownloadArchiveQueryData::from_str("da_x_5_fb2").is_err());
    }

    #[test]
    fn round_trip_check_archive_status() {
        let cd = CheckArchiveStatus {
            task_id: "abc123".to_string(),
        };
        let parsed = CheckArchiveStatus::from_str(&cd.to_string()).unwrap();
        assert_eq!(parsed.task_id, "abc123");
    }

    #[test]
    fn rejects_check_archive_without_prefix() {
        assert!(CheckArchiveStatus::from_str("da_abc123").is_err());
    }
}
