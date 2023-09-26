use std::str::FromStr;

use regex::Regex;
use strum_macros::EnumIter;

use crate::bots::approved_bot::modules::utils::errors::{
    CallbackQueryParseError, CommandParseError,
};

#[derive(Clone, EnumIter)]
pub enum DownloadQueryData {
    DownloadData { book_id: u32, file_type: String },
}

impl ToString for DownloadQueryData {
    fn to_string(&self) -> String {
        match self {
            DownloadQueryData::DownloadData { book_id, file_type } => {
                format!("d_{book_id}_{file_type}")
            }
        }
    }
}

impl FromStr for DownloadQueryData {
    type Err = CommandParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Regex::new(r"^d_(?P<book_id>\d+)_(?P<file_type>\w+)$")
            .unwrap_or_else(|_| panic!("Broken DownloadQueryData regexp!"))
            .captures(s)
            .ok_or(CommandParseError)
            .map(|caps| {
                (
                    caps["book_id"].parse().unwrap(),
                    caps["file_type"].to_string(),
                )
            })
            .map(|(book_id, file_type)| DownloadQueryData::DownloadData { book_id, file_type })
    }
}

#[derive(Clone, EnumIter)]
pub enum DownloadArchiveQueryData {
    Sequence { id: u32, file_type: String },
    Author { id: u32, file_type: String },
    Translator { id: u32, file_type: String },
}

impl ToString for DownloadArchiveQueryData {
    fn to_string(&self) -> String {
        match self {
            DownloadArchiveQueryData::Sequence { id, file_type } => {
                format!("da_s_{id}_{file_type}")
            }
            DownloadArchiveQueryData::Author { id, file_type } => format!("da_a_{id}_{file_type}"),
            DownloadArchiveQueryData::Translator { id, file_type } => {
                format!("da_t_{id}_{file_type}")
            }
        }
    }
}

impl FromStr for DownloadArchiveQueryData {
    type Err = CallbackQueryParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Regex::new(r"^da_(?P<obj_type>[sat])_(?P<id>\d+)_(?P<file_type>\w+)$")
            .unwrap_or_else(|_| panic!("Broken BookCallbackData regex pattern!"))
            .captures(s)
            .ok_or(CallbackQueryParseError)
            .map(|caps| {
                (
                    caps["id"].parse().unwrap(),
                    caps["file_type"].to_string(),
                    caps["obj_type"].to_string(),
                )
            })
            .map(|(id, file_type, obj_type)| match obj_type.as_str() {
                "s" => DownloadArchiveQueryData::Sequence { id, file_type },
                "a" => DownloadArchiveQueryData::Author { id, file_type },
                "t" => DownloadArchiveQueryData::Translator { id, file_type },
                _ => panic!("Unknown DownloadArchiveQueryData type: {}!", obj_type),
            })
    }
}

#[derive(Clone)]
pub struct CheckArchiveStatus {
    pub task_id: String,
}

impl ToString for CheckArchiveStatus {
    fn to_string(&self) -> String {
        format!("check_da_{}", self.task_id)
    }
}

impl FromStr for CheckArchiveStatus {
    type Err = CallbackQueryParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Regex::new(r"^check_da_(?P<task_id>\w+)$")
            .unwrap_or_else(|_| panic!("Broken CheckArchiveStatus regex pattern!"))
            .captures(s)
            .ok_or(CallbackQueryParseError)
            .map(|caps| caps["task_id"].parse().unwrap())
            .map(|task_id| CheckArchiveStatus { task_id })
    }
}
