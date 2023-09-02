use std::str::FromStr;

use regex::Regex;
use strum_macros::EnumIter;


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
    type Err = strum::ParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let re = Regex::new(r"^d_(?P<book_id>\d+)_(?P<file_type>\w+)$").unwrap();

        let caps = re.captures(s);
        let caps = match caps {
            Some(v) => v,
            None => return Err(strum::ParseError::VariantNotFound),
        };

        let book_id: u32 = caps["book_id"].parse().unwrap();
        let file_type: String = caps["file_type"].to_string();

        Ok(DownloadQueryData::DownloadData { book_id, file_type })
    }
}

#[derive(Clone, EnumIter)]
pub enum DownloadArchiveQueryData {
    Sequence { id: u32, file_type: String },
    Author { id: u32, file_type: String },
    Translator { id: u32, file_type: String }
}

impl ToString for DownloadArchiveQueryData {
    fn to_string(&self) -> String {
        match self {
            DownloadArchiveQueryData::Sequence { id, file_type } => format!("da_s_{id}_{file_type}"),
            DownloadArchiveQueryData::Author { id, file_type } => format!("da_a_{id}_{file_type}"),
            DownloadArchiveQueryData::Translator { id, file_type } => format!("da_t_{id}_{file_type}"),
        }
    }
}

impl FromStr for DownloadArchiveQueryData {
    type Err = strum::ParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let re = Regex::new(r"^da_(?P<obj_type>[s|a|t])_(?P<id>\d+)_(?P<file_type>\w+)$").unwrap();

        let caps = re.captures(s);
        let caps = match caps {
            Some(v) => v,
            None => return Err(strum::ParseError::VariantNotFound),
        };

        let id: u32 = caps["id"].parse().unwrap();
        let file_type: String = caps["file_type"].to_string();

        Ok(
            match caps["obj_type"].to_string().as_str() {
                "s" => DownloadArchiveQueryData::Sequence { id, file_type },
                "a" => DownloadArchiveQueryData::Author { id, file_type },
                "t" => DownloadArchiveQueryData::Translator { id, file_type },
                _ => return Err(strum::ParseError::VariantNotFound)
            }
        )
    }
}

#[derive(Clone)]
pub struct CheckArchiveStatus {
    pub task_id: String
}

impl ToString for CheckArchiveStatus {
    fn to_string(&self) -> String {
    format!("check_da_{}", self.task_id)
    }
}

impl FromStr for CheckArchiveStatus {
    type Err = strum::ParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let re = Regex::new(r"^check_da_(?P<task_id>\w+)$").unwrap();

        let caps = re.captures(s);
        let caps = match caps {
            Some(v) => v,
            None => return Err(strum::ParseError::VariantNotFound),
        };

        let task_id: String = caps["task_id"].parse().unwrap();

        Ok(CheckArchiveStatus { task_id })
    }
}
