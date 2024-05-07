use std::{fmt::Display, str::FromStr};

use regex::Regex;

use crate::bots::approved_bot::modules::utils::{
    errors::CallbackQueryParseError, pagination::GetPaginationCallbackData,
};

#[derive(Clone)]
pub enum BookCallbackData {
    Author { id: u32, page: u32 },
    Translator { id: u32, page: u32 },
    Sequence { id: u32, page: u32 },
}

impl FromStr for BookCallbackData {
    type Err = CallbackQueryParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Regex::new(r"^b(?P<an_type>[ats])_(?P<id>\d+)_(?P<page>\d+)$")
            .unwrap_or_else(|_| panic!("Broken BookCallbackData regex pattern!"))
            .captures(s)
            .ok_or(CallbackQueryParseError)
            .map(|caps| {
                (
                    caps["an_type"].to_string(),
                    caps["id"].parse::<u32>().unwrap(),
                    caps["page"].parse::<u32>().unwrap(),
                )
            })
            .map(
                |(annotation_type, id, page)| match annotation_type.as_str() {
                    "a" => Ok(BookCallbackData::Author { id, page }),
                    "t" => Ok(BookCallbackData::Translator { id, page }),
                    "s" => Ok(BookCallbackData::Sequence { id, page }),
                    _ => panic!("Unknown BookCallbackData type: {}!", annotation_type),
                },
            )?
    }
}

impl Display for BookCallbackData {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BookCallbackData::Author { id, page } => write!(f, "ba_{}_{}", id, page),
            BookCallbackData::Translator { id, page } => write!(f, "bt_{}_{}", id, page),
            BookCallbackData::Sequence { id, page } => write!(f, "bs_{}_{}", id, page),
        }
    }
}

impl GetPaginationCallbackData for BookCallbackData {
    fn get_pagination_callback_data(&self, target_page: u32) -> String {
        match self {
            BookCallbackData::Author { id, .. } => BookCallbackData::Author {
                id: *id,
                page: target_page,
            },
            BookCallbackData::Translator { id, .. } => BookCallbackData::Translator {
                id: *id,
                page: target_page,
            },
            BookCallbackData::Sequence { id, .. } => BookCallbackData::Sequence {
                id: *id,
                page: target_page,
            },
        }
        .to_string()
    }
}
