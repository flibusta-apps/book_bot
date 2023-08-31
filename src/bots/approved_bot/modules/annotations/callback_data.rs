use std::str::FromStr;

use regex::Regex;

use crate::bots::approved_bot::modules::utils::GetPaginationCallbackData;


#[derive(Debug, Clone)]
pub enum AnnotationCallbackData {
    Book { id: u32, page: u32 },
    Author { id: u32, page: u32 },
}

impl FromStr for AnnotationCallbackData {
    type Err = strum::ParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let re = Regex::new(r"^(?P<an_type>a|b)_an_(?P<id>\d+)_(?P<page>\d+)$").unwrap();

        let caps = re.captures(s);
        let caps = match caps {
            Some(v) => v,
            None => return Err(strum::ParseError::VariantNotFound),
        };

        let annotation_type = &caps["an_type"];
        let id = caps["id"].parse::<u32>().unwrap();
        let page = caps["page"].parse::<u32>().unwrap();

        match annotation_type {
            "a" => Ok(AnnotationCallbackData::Author { id, page }),
            "b" => Ok(AnnotationCallbackData::Book { id, page }),
            _ => Err(strum::ParseError::VariantNotFound),
        }
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
