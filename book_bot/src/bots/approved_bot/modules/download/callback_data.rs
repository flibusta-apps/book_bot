use std::{fmt::Display, str::FromStr};

use regex::Regex;
use std::sync::LazyLock;
use strum_macros::EnumIter;

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

#[derive(Clone, EnumIter)]
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

#[derive(Clone, EnumIter)]
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
