use std::str::FromStr;

use regex::Regex;

use crate::bots::approved_bot::modules::utils::{
    errors::CallbackQueryParseError, pagination::GetPaginationCallbackData,
};

#[derive(Debug, Clone)]
pub enum AnnotationCallbackData {
    Book { id: u32, page: u32 },
    Author { id: u32, page: u32 },
}

impl FromStr for AnnotationCallbackData {
    type Err = CallbackQueryParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Regex::new(r"^(?P<an_type>[ab])_an_(?P<id>\d+)_(?P<page>\d+)$")
            .unwrap_or_else(|_| panic!("Broken AnnotationCallbackData regex pattern!"))
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
                    "a" => AnnotationCallbackData::Author { id, page },
                    "b" => AnnotationCallbackData::Book { id, page },
                    _ => panic!("Unknown AnnotationCallbackData type: {}!", annotation_type),
                },
            )
    }
}

impl ToString for AnnotationCallbackData {
    fn to_string(&self) -> String {
        match self {
            AnnotationCallbackData::Book { id, page } => format!("b_an_{id}_{page}"),
            AnnotationCallbackData::Author { id, page } => format!("a_an_{id}_{page}"),
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
