use std::str::FromStr;

use regex::Regex;

use crate::bots::approved_bot::modules::utils::GetPaginationCallbackData;


#[derive(Clone)]
pub enum BookCallbackData {
    Author { id: u32, page: u32 },
    Translator { id: u32, page: u32 },
    Sequence { id: u32, page: u32 },
}

impl FromStr for BookCallbackData {
    type Err = strum::ParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let re = Regex::new(r"^b(?P<an_type>a|t|s)_(?P<id>\d+)_(?P<page>\d+)$").unwrap();

        let caps = re.captures(s);
        let caps = match caps {
            Some(v) => v,
            None => return Err(strum::ParseError::VariantNotFound),
        };

        let annotation_type = &caps["an_type"];
        let id = caps["id"].parse::<u32>().unwrap();
        let page = caps["page"].parse::<u32>().unwrap();

        match annotation_type {
            "a" => Ok(BookCallbackData::Author { id, page }),
            "t" => Ok(BookCallbackData::Translator { id, page }),
            "s" => Ok(BookCallbackData::Sequence { id, page }),
            _ => Err(strum::ParseError::VariantNotFound),
        }
    }
}

impl ToString for BookCallbackData {
    fn to_string(&self) -> String {
        match self {
            BookCallbackData::Author { id, page } => format!("ba_{id}_{page}"),
            BookCallbackData::Translator { id, page } => format!("bt_{id}_{page}"),
            BookCallbackData::Sequence { id, page } => format!("bs_{id}_{page}"),
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
