use std::{fmt::Display, str::FromStr};

use regex::Regex;
use strum_macros::EnumIter;

use crate::bots::approved_bot::{
    modules::utils::pagination::GetPaginationCallbackData,
    services::user_settings::DefaultSearchType,
};

#[derive(Clone, EnumIter)]
pub enum SearchCallbackData {
    Book { page: u32 },
    Authors { page: u32 },
    Sequences { page: u32 },
    Translators { page: u32 },
}

impl Display for SearchCallbackData {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SearchCallbackData::Book { page } => write!(f, "sb_{page}"),
            SearchCallbackData::Authors { page } => write!(f, "sa_{page}"),
            SearchCallbackData::Sequences { page } => write!(f, "ss_{page}"),
            SearchCallbackData::Translators { page } => write!(f, "st_{page}"),
        }
    }
}

impl FromStr for SearchCallbackData {
    type Err = strum::ParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let re = Regex::new(r"^(?P<search_type>s[abst])_(?P<page>\d+)$").unwrap();

        let caps = re.captures(s);
        let caps = match caps {
            Some(v) => v,
            None => return Err(strum::ParseError::VariantNotFound),
        };

        let search_type = &caps["search_type"];
        let page: u32 = caps["page"].parse::<u32>().unwrap();

        // Fix for migrate from old bot implementation
        let page: u32 = std::cmp::max(1, page);

        match search_type {
            "sb" => Ok(SearchCallbackData::Book { page }),
            "sa" => Ok(SearchCallbackData::Authors { page }),
            "ss" => Ok(SearchCallbackData::Sequences { page }),
            "st" => Ok(SearchCallbackData::Translators { page }),
            _ => Err(strum::ParseError::VariantNotFound),
        }
    }
}

/// Converts default search type to SearchCallbackData with page 1.
pub fn default_search_to_callback_data(t: DefaultSearchType) -> SearchCallbackData {
    match t {
        DefaultSearchType::Book => SearchCallbackData::Book { page: 1 },
        DefaultSearchType::Author => SearchCallbackData::Authors { page: 1 },
        DefaultSearchType::Series => SearchCallbackData::Sequences { page: 1 },
        DefaultSearchType::Translator => SearchCallbackData::Translators { page: 1 },
    }
}

impl GetPaginationCallbackData for SearchCallbackData {
    fn get_pagination_callback_data(&self, target_page: u32) -> String {
        match self {
            SearchCallbackData::Book { .. } => SearchCallbackData::Book { page: target_page },
            SearchCallbackData::Authors { .. } => SearchCallbackData::Authors { page: target_page },
            SearchCallbackData::Sequences { .. } => {
                SearchCallbackData::Sequences { page: target_page }
            }
            SearchCallbackData::Translators { .. } => {
                SearchCallbackData::Translators { page: target_page }
            }
        }
        .to_string()
    }
}
