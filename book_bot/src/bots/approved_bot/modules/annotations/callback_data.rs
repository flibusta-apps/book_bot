use std::{fmt::Display, str::FromStr};

use regex::Regex;
use std::sync::LazyLock;

use crate::bots::approved_bot::modules::utils::{
    errors::CallbackQueryParseError, pagination::GetPaginationCallbackData,
};

static RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^(?P<an_type>[ab])_an_(?P<id>\d+)_(?P<page>\d+)$").unwrap());

#[derive(Debug, Clone)]
pub enum AnnotationCallbackData {
    Book { id: u32, page: u32 },
    Author { id: u32, page: u32 },
}

impl FromStr for AnnotationCallbackData {
    type Err = CallbackQueryParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let caps = RE.captures(s).ok_or(CallbackQueryParseError)?;

        let an_type = &caps["an_type"];
        let id: u32 = caps["id"].parse().map_err(|_| CallbackQueryParseError)?;
        let page: u32 = caps["page"].parse().map_err(|_| CallbackQueryParseError)?;

        match an_type {
            "a" => Ok(AnnotationCallbackData::Author { id, page }),
            "b" => Ok(AnnotationCallbackData::Book { id, page }),
            _ => Err(CallbackQueryParseError),
        }
    }
}

impl Display for AnnotationCallbackData {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AnnotationCallbackData::Book { id, page } => write!(f, "b_an_{id}_{page}"),
            AnnotationCallbackData::Author { id, page } => write!(f, "a_an_{id}_{page}"),
        }
    }
}

impl GetPaginationCallbackData for AnnotationCallbackData {
    fn get_pagination_callback_data(&self, target_page: u32) -> String {
        match self {
            AnnotationCallbackData::Book { id, .. } => AnnotationCallbackData::Book {
                id: *id,
                page: target_page,
            },
            AnnotationCallbackData::Author { id, .. } => AnnotationCallbackData::Author {
                id: *id,
                page: target_page,
            },
        }
        .to_string()
    }
}
