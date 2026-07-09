use std::{fmt::Display, str::FromStr};

use regex::Regex;
use std::sync::LazyLock;
use strum_macros::EnumIter;

use crate::bots::approved_bot::{
    modules::utils::pagination::GetPaginationCallbackData,
    services::user_settings::DefaultSearchType,
};

static RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^(?P<search_type>s[abst])_(?P<page>\d+)$").unwrap());

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
        let caps = RE.captures(s).ok_or(strum::ParseError::VariantNotFound)?;

        let search_type = &caps["search_type"];
        let page: u32 = caps["page"]
            .parse()
            .map_err(|_| strum::ParseError::VariantNotFound)?;

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

#[cfg(test)]
mod tests {
    use super::SearchCallbackData;
    use std::str::FromStr;

    #[test]
    fn round_trip_book() {
        let cd = SearchCallbackData::Book { page: 3 };
        match SearchCallbackData::from_str(&cd.to_string()).unwrap() {
            SearchCallbackData::Book { page } => assert_eq!(page, 3),
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn round_trip_authors() {
        let cd = SearchCallbackData::Authors { page: 4 };
        match SearchCallbackData::from_str(&cd.to_string()).unwrap() {
            SearchCallbackData::Authors { page } => assert_eq!(page, 4),
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn round_trip_sequences() {
        let cd = SearchCallbackData::Sequences { page: 5 };
        match SearchCallbackData::from_str(&cd.to_string()).unwrap() {
            SearchCallbackData::Sequences { page } => assert_eq!(page, 5),
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn round_trip_translators() {
        let cd = SearchCallbackData::Translators { page: 6 };
        match SearchCallbackData::from_str(&cd.to_string()).unwrap() {
            SearchCallbackData::Translators { page } => assert_eq!(page, 6),
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn page_zero_normalized_to_one() {
        match SearchCallbackData::from_str("sb_0").unwrap() {
            SearchCallbackData::Book { page } => assert_eq!(page, 1),
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn rejects_foreign_prefix() {
        assert!(SearchCallbackData::from_str("sx_1").is_err());
    }

    #[test]
    fn rejects_non_numeric_page() {
        assert!(SearchCallbackData::from_str("sb_abc").is_err());
    }
}
